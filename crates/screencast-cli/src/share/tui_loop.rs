//! Interactive ratatui event loop during `screencast share`.

use super::frame_worker::capture_latest_snapshot_jpeg;
use super::start_sink_with_retry;
use screencast_core::i18n::{t, tf, TextKey};
use screencast_core::{DeviceInfo, Discovery, Sink, StreamPlan};
use screencast_sinks::{build_sink, CombinedDiscovery};
use screencast_transport::{ServerState, StreamHub};
use screencast_ui::{TuiAction, TuiSession, TuiState};
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{watch, RwLock};

#[allow(clippy::too_many_arguments)]
pub(crate) async fn run_tui_loop(
    devices: Vec<DeviceInfo>,
    browser_url: String,
    status_line: String,
    active_device_id: Option<String>,
    hub: Arc<StreamHub>,
    server_state: Arc<ServerState>,
    active_sink: Arc<RwLock<Option<Box<dyn Sink>>>>,
    shared_plan: Arc<RwLock<StreamPlan>>,
    recent_fragments: Arc<RwLock<VecDeque<Arc<Vec<u8>>>>>,
    host_base_url: String,
    active_p2p: bool,
    fail_rx: watch::Receiver<Option<String>>,
) -> anyhow::Result<()> {
    let mut session = TuiSession::enter()?;
    let mut state = TuiState {
        version: env!("CARGO_PKG_VERSION").into(),
        devices,
        selected: 0,
        browser_url,
        stats: hub.stats().await,
        status_line,
        active_device_id,
        audio_muted: false,
        remote_mode: false,
    };

    loop {
        if let Some(err_msg) = fail_rx.borrow().clone() {
            drop(session);
            anyhow::bail!("{err_msg}");
        }
        state.stats = hub.stats().await;
        session.draw(&state)?;

        match session.poll_action(&mut state, Duration::from_millis(200))? {
            Some(TuiAction::Quit) => break,
            Some(TuiAction::TogglePause) => {
                let paused = !state.stats.paused;
                hub.set_paused(paused).await;
                if let Some(s) = active_sink.read().await.as_ref() {
                    let _ = if paused {
                        s.pause().await
                    } else {
                        s.resume().await
                    };
                }
                state.status_line = if paused {
                    t(TextKey::ShareTuiPaused).into()
                } else {
                    t(TextKey::ShareTuiLive).into()
                };
            }
            Some(TuiAction::ToggleMute) => {
                state.audio_muted = !state.audio_muted;
                let mut p = shared_plan.write().await;
                p.audio = !state.audio_muted;
                hub.set_audio_enabled(!state.audio_muted).await;
            }
            Some(TuiAction::Rescan) => {
                state.status_line = t(TextKey::ShareTuiScanning).into();
                session.draw(&state)?;
                let scanned = CombinedDiscovery::new()
                    .with_active_p2p(active_p2p)
                    .scan(Duration::from_secs(3))
                    .await
                    .unwrap_or_default();
                *server_state.devices.write().await = scanned.clone();
                state.devices = scanned;
                let cnt_s = state.devices.len().to_string();
                state.status_line = tf(TextKey::ShareTuiFoundCount, &[("count", &cnt_s)]);
            }
            Some(TuiAction::Snapshot) => {
                if let Some(jpg) = capture_latest_snapshot_jpeg(&hub, &recent_fragments).await {
                    *server_state.snapshot_jpeg.write().await = Some(jpg.clone());
                    let ts = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_secs())
                        .unwrap_or(0);
                    let fname = format!("screencast-snap-{ts}.jpg");
                    if std::fs::write(&fname, &jpg).is_ok() {
                        state.status_line = tf(TextKey::ShareTuiSnapSaved, &[("file", &fname)]);
                    } else {
                        state.status_line = t(TextKey::ShareTuiSnapUpdated).into();
                    }
                } else {
                    state.status_line = t(TextKey::ShareTuiSnapUnavailable).into();
                }
            }
            Some(TuiAction::ShareSelected(idx)) => {
                if let Some(old) = active_sink.write().await.take() {
                    let _ = old.stop().await;
                }
                state.active_device_id = None;
                let target_id = state
                    .devices
                    .get(idx)
                    .map(|d| d.id.as_str())
                    .or(Some("browser"));
                let plan = shared_plan.read().await.clone();
                match build_sink(&state.devices, target_id, &plan, &host_base_url) {
                    Ok(new_sink) => match start_sink_with_retry(&*new_sink, &plan).await {
                        Ok(prep) => {
                            let note = prep.note.unwrap_or_else(|| prep.stream_url.clone());
                            state.active_device_id = Some(prep.device.id.clone());
                            state.status_line = tf(
                                TextKey::ShareStatusLive,
                                &[("name", &prep.device.name), ("note", &note)],
                            );
                            *active_sink.write().await = Some(new_sink);
                        }
                        Err(e) => {
                            let err_s = e.to_string();
                            state.status_line =
                                tf(TextKey::ShareTuiErrorPrefix, &[("err", &err_s)]);
                        }
                    },
                    Err(e) => {
                        let err_s = e.to_string();
                        state.status_line = tf(TextKey::ShareTuiErrorPrefix, &[("err", &err_s)]);
                    }
                }
            }
            None => {}
        }
    }
    Ok(())
}
