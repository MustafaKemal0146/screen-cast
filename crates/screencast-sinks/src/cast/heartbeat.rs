//! Background Cast TLS reader, proactive heartbeat responder, and media status tracker.

use super::{
    read_cast_message, write_cast_message, CastMessage, NS_CONNECTION, NS_HEARTBEAT, NS_MEDIA,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::WriteHalf;
use tokio::net::TcpStream;
use tokio::sync::{watch, Mutex};
use tokio::task::JoinHandle;
use tokio::time::Instant;
use tracing::{debug, info, warn};

pub(crate) type CastTlsStream = tokio_rustls::client::TlsStream<TcpStream>;

pub(crate) struct CastSharedState {
    pub(crate) media_session_id: Option<i64>,
    pub(crate) receiver_session_id: Option<String>,
    pub(crate) load_error: Option<String>,
    pub(crate) request_seq: i64,
}

impl CastSharedState {
    pub(crate) fn next_req_id(&mut self) -> i64 {
        self.request_seq += 1;
        self.request_seq
    }
}

pub(crate) struct CastSession {
    pub(crate) writer: Arc<Mutex<WriteHalf<CastTlsStream>>>,
    pub(crate) transport_id: String,
    pub(crate) shared: Arc<Mutex<CastSharedState>>,
    pub(crate) alive: Arc<AtomicBool>,
    pub(crate) pong_rx: watch::Receiver<u64>,
    pub(crate) media_status_rx: watch::Receiver<u64>,
    pub(crate) stop_tx: watch::Sender<bool>,
    pub(crate) reader_task: JoinHandle<()>,
}

impl Drop for CastSession {
    fn drop(&mut self) {
        self.alive.store(false, Ordering::Release);
        let _ = self.stop_tx.send(true);
        self.reader_task.abort();
    }
}

pub(crate) struct CastReaderChannels {
    pub(crate) writer: Arc<Mutex<WriteHalf<CastTlsStream>>>,
    pub(crate) shared: Arc<Mutex<CastSharedState>>,
    pub(crate) alive: Arc<AtomicBool>,
    pub(crate) pong_tx: watch::Sender<u64>,
    pub(crate) media_status_tx: watch::Sender<u64>,
    pub(crate) stop_rx: watch::Receiver<bool>,
}

pub(crate) fn spawn_cast_reader(
    device_name: String,
    mut reader: tokio::io::ReadHalf<CastTlsStream>,
    ch: CastReaderChannels,
) -> JoinHandle<()> {
    let CastReaderChannels {
        writer,
        shared,
        alive,
        pong_tx,
        media_status_tx,
        mut stop_rx,
    } = ch;
    tokio::spawn(async move {
        let mut last_rx = Instant::now();
        let mut heartbeat_tick = tokio::time::interval_at(
            Instant::now() + Duration::from_secs(5),
            Duration::from_secs(5),
        );
        heartbeat_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

        loop {
            tokio::select! {
                _ = stop_rx.changed() => {
                    if *stop_rx.borrow() {
                        break;
                    }
                }
                _ = heartbeat_tick.tick() => {
                    if last_rx.elapsed() > Duration::from_secs(16) {
                        warn!("Cast heartbeat timed out ({device_name})");
                        break;
                    }
                    let ping = CastMessage::json(
                        "sender-0",
                        "receiver-0",
                        NS_HEARTBEAT,
                        r#"{"type":"PING"}"#.into(),
                    );
                    let mut wr = writer.lock().await;
                    if write_cast_message(&mut *wr, &ping).await.is_err() {
                        break;
                    }
                }
                msg_res = read_cast_message(&mut reader) => {
                    match msg_res {
                        Ok(msg) => {
                            last_rx = Instant::now();
                            let txt = msg.payload_utf8.as_deref().unwrap_or("");
                            if msg.namespace == NS_HEARTBEAT {
                                if txt.contains("\"PING\"") || txt.contains("PING") {
                                    let dest = if msg.source_id.is_empty() {
                                        "receiver-0"
                                    } else {
                                        msg.source_id.as_str()
                                    };
                                    let pong = CastMessage::json(
                                        "sender-0",
                                        dest,
                                        NS_HEARTBEAT,
                                        r#"{"type":"PONG"}"#.into(),
                                    );
                                    let mut wr = writer.lock().await;
                                    if write_cast_message(&mut *wr, &pong).await.is_err() {
                                        break;
                                    }
                                } else if txt.contains("PONG") {
                                    pong_tx.send_modify(|v| *v += 1);
                                }
                            } else if msg.namespace == NS_MEDIA {
                                debug!("Cast MEDIA message ({device_name}): {txt}");
                                if let Ok(v) = serde_json::from_str::<serde_json::Value>(txt) {
                                    if let Some(msid) = v["status"][0]["mediaSessionId"].as_i64() {
                                        shared.lock().await.media_session_id = Some(msid);
                                    }
                                    match v["type"].as_str() {
                                        Some("LOAD_FAILED") | Some("INVALID_REQUEST") => {
                                            warn!("Cast media error ({device_name}): {txt}");
                                            shared.lock().await.load_error = Some(screencast_core::tf(
                                                screencast_core::TextKey::CastLoadRejected,
                                                &[("txt", txt)],
                                            ));
                                            media_status_tx.send_modify(|v| *v += 1);
                                        }
                                        Some("MEDIA_STATUS") => {
                                            media_status_tx.send_modify(|v| *v += 1);
                                        }
                                        _ => {}
                                    }
                                }
                            } else if msg.namespace == NS_CONNECTION && txt.contains("\"CLOSE\"") {
                                info!("Cast receiver closed connection ({device_name})");
                                break;
                            }
                        }
                        Err(e) => {
                            debug!("Cast reader loop ended ({device_name}): {e}");
                            break;
                        }
                    }
                }
            }
        }

        alive.store(false, Ordering::Release);
        pong_tx.send_modify(|v| *v += 1);
        media_status_tx.send_modify(|v| *v += 1);
    })
}
