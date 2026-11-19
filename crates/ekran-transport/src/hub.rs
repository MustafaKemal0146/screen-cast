//! Live media fan-out hub with slow-client drop protection and telemetry (T-M1-07).

use crate::mp4::{BoxReader, FragmentAssembler, MediaChunk};
use ekran_core::{Control, ControlEvent, Result, Stats, StreamPlan};
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::AsyncRead;
use tokio::sync::{broadcast, mpsc, Mutex, RwLock};
use tracing::{debug, warn};

/// Maximum queued media chunks per client (~2s at 500ms/fragment or 4-8 fragments).
pub const MAX_CLIENT_QUEUE: usize = 6;

/// Maximum age before queued media fragments are considered stale (>2s per PLAN T-M1-07).
pub const MAX_QUEUE_AGE: Duration = Duration::from_secs(2);

/// Outbound item queued for an individual WebSocket client.
#[derive(Debug, Clone)]
pub enum OutboundFrame {
    Binary {
        data: Arc<Vec<u8>>,
        enqueued_at: Instant,
    },
    Text(String),
    Pong(Vec<u8>),
}

/// Telemetry reported back by a web player client (`{"t":"stats",...}`).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientTelemetry {
    pub latency_ms: u32,
    pub fps: f64,
    #[serde(default)]
    pub measured_kbps: u32,
    #[serde(default)]
    pub quality: String,
}

struct ClientSlot {
    tx: mpsc::Sender<OutboundFrame>,
    dropped_frames: Arc<AtomicU64>,
}

/// Fans out init segments and media fragments from one producer to N WebSocket/HLS/MP4 consumers.
pub struct StreamHub {
    clients: RwLock<HashMap<u64, ClientSlot>>,
    next_id: AtomicU64,
    init_segment: RwLock<Option<Arc<Vec<u8>>>>,
    stats: RwLock<Stats>,
    raw_tx: broadcast::Sender<Arc<Vec<u8>>>,
    telemetry_tx: broadcast::Sender<ClientTelemetry>,
    control: RwLock<Option<Arc<dyn Control>>>,
    rate_window: Mutex<RateWindow>,
}

struct RateWindow {
    last_tick: Instant,
    bytes_in_window: u64,
    frags_in_window: u64,
}

impl Default for StreamHub {
    fn default() -> Self {
        Self::new(&StreamPlan::default())
    }
}

impl StreamHub {
    pub fn new(plan: &StreamPlan) -> Self {
        let (raw_tx, _) = broadcast::channel(64);
        let (telemetry_tx, _) = broadcast::channel(32);
        let q = match plan.quality {
            ekran_core::Quality::Auto => ekran_core::Quality::P1080,
            other => other,
        };
        Self {
            clients: RwLock::new(HashMap::new()),
            next_id: AtomicU64::new(1),
            init_segment: RwLock::new(None),
            stats: RwLock::new(Stats {
                width: q.width(),
                height: q.height(),
                quality: plan.quality.label().into(),
                audio: plan.audio,
                ..Default::default()
            }),
            raw_tx,
            telemetry_tx,
            control: RwLock::new(None),
            rate_window: Mutex::new(RateWindow {
                last_tick: Instant::now(),
                bytes_in_window: 0,
                frags_in_window: 0,
            }),
        }
    }

    pub async fn set_control(&self, ctrl: Arc<dyn Control>) {
        *self.control.write().await = Some(ctrl);
    }

    pub async fn set_paused(&self, paused: bool) {
        self.stats.write().await.paused = paused;
    }

    pub async fn set_idle(&self, idle: bool) {
        self.stats.write().await.idle = idle;
    }

    pub async fn set_quality_label(&self, label: String, width: u16, height: u16) {
        let mut s = self.stats.write().await;
        s.quality = label;
        s.width = width;
        s.height = height;
    }

    /// Subscribe to the raw fMP4 stream (used by HLS packager, `/stream.mp4`, and `--record`).
    pub async fn subscribe_raw(&self) -> (Option<Arc<Vec<u8>>>, broadcast::Receiver<Arc<Vec<u8>>>) {
        let init = self.init_segment.read().await.clone();
        (init, self.raw_tx.subscribe())
    }

    /// Subscribe to client latency/throughput telemetry samples.
    pub fn subscribe_telemetry(&self) -> broadcast::Receiver<ClientTelemetry> {
        self.telemetry_tx.subscribe()
    }

    /// Register a new WebSocket client and queue the current `init_segment` if known.
    pub async fn register_client(&self) -> (u64, mpsc::Receiver<OutboundFrame>, Arc<AtomicU64>) {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = mpsc::channel(MAX_CLIENT_QUEUE);
        let dropped = Arc::new(AtomicU64::new(0));

        if let Some(init) = self.init_segment.read().await.clone() {
            let _ = tx.try_send(OutboundFrame::Binary {
                data: init,
                enqueued_at: Instant::now(),
            });
        }

        {
            let mut clients = self.clients.write().await;
            clients.insert(
                id,
                ClientSlot {
                    tx,
                    dropped_frames: dropped.clone(),
                },
            );
            self.stats.write().await.clients = clients.len();
        }
        (id, rx, dropped)
    }

    /// Remove a disconnected WebSocket client.
    pub async fn unregister_client(&self, id: u64) {
        let mut clients = self.clients.write().await;
        clients.remove(&id);
        self.stats.write().await.clients = clients.len();
    }

    /// Store the fMP4 initialisation segment (`ftyp+moov`) and push to connected clients.
    pub async fn set_init(&self, bytes: Vec<u8>) {
        let arc = Arc::new(bytes);
        *self.init_segment.write().await = Some(arc.clone());
        let _ = self.raw_tx.send(arc.clone());
        self.broadcast_binary(arc).await;
    }

    /// Publish a media fragment (`moof+mdat`) without blocking on slow clients.
    pub async fn publish_fragment(&self, bytes: Vec<u8>) {
        let len = bytes.len() as u64;
        let arc = Arc::new(bytes);
        let _ = self.raw_tx.send(arc.clone());

        {
            let mut rw = self.rate_window.lock().await;
            rw.bytes_in_window += len;
            rw.frags_in_window += 1;
            let elapsed = rw.last_tick.elapsed();
            let mut s = self.stats.write().await;
            s.fragments += 1;
            s.bytes += len;
            if elapsed >= Duration::from_secs(1) {
                let secs = elapsed.as_secs_f64();
                s.bitrate_kbps = ((rw.bytes_in_window as f64 * 8.0) / secs / 1000.0) as u32;
                // Each fragment typically covers ~0.5s or multiple frames; report fragment cadence if no client fps yet.
                if s.fps <= 0.1 {
                    s.fps = (rw.frags_in_window as f64 / secs) * 15.0;
                }
                rw.bytes_in_window = 0;
                rw.frags_in_window = 0;
                rw.last_tick = Instant::now();
            }
        }

        self.broadcast_binary(arc).await;
    }

    /// Broadcast a text control/notification JSON frame to all connected clients.
    pub async fn broadcast_text(&self, text: String) {
        let clients = self.clients.read().await;
        for slot in clients.values() {
            let _ = slot.tx.try_send(OutboundFrame::Text(text.clone()));
        }
    }

    async fn broadcast_binary(&self, data: Arc<Vec<u8>>) {
        let now = Instant::now();
        let clients = self.clients.read().await;
        for (&id, slot) in clients.iter() {
            match slot.tx.try_send(OutboundFrame::Binary {
                data: data.clone(),
                enqueued_at: now,
            }) {
                Ok(()) => {}
                Err(mpsc::error::TrySendError::Full(_)) => {
                    slot.dropped_frames.fetch_add(1, Ordering::Relaxed);
                    debug!("yavaş istemci #{id}: parça atıldı (kuyruk dolu)");
                }
                Err(mpsc::error::TrySendError::Closed(_)) => {}
            }
        }
    }

    /// Handle an incoming JSON text frame from a WebSocket client (control event or telemetry).
    pub async fn handle_client_text(&self, text: &str) {
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(text) {
            if val.get("t").and_then(|v| v.as_str()) == Some("stats") {
                if let Ok(telem) = serde_json::from_value::<ClientTelemetry>(val) {
                    {
                        let mut s = self.stats.write().await;
                        s.latency_ms = telem.latency_ms;
                        if telem.fps > 0.0 {
                            s.fps = telem.fps;
                        }
                    }
                    let _ = self.telemetry_tx.send(telem);
                }
                return;
            }
        }

        if let Ok(event) = serde_json::from_str::<ControlEvent>(text) {
            let ctrl = self.control.read().await.clone();
            if let Some(ctrl) = ctrl {
                if let Err(e) = ctrl.apply(event).await {
                    warn!(
                        "kumanda olayı uygulanamadı ({}): {}",
                        ctrl.backend_name(),
                        e
                    );
                }
            }
        }
    }

    /// Current stream statistics snapshot.
    pub async fn stats(&self) -> Stats {
        self.stats.read().await.clone()
    }

    /// Pump a capture source's fMP4 stdout stream into this hub until EOF.
    pub async fn pump_fmp4<R: AsyncRead + Unpin>(&self, reader: R) -> Result<()> {
        let mut box_reader = BoxReader::new(reader);
        let mut assembler = FragmentAssembler::new();
        while let Some(bx) = box_reader.next_box().await? {
            if let Some(chunk) = assembler.push(bx) {
                match chunk {
                    MediaChunk::Init(bytes) => self.set_init(bytes).await,
                    MediaChunk::Fragment(bytes) => {
                        if !self.stats.read().await.paused {
                            self.publish_fragment(bytes).await;
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn slow_client_does_not_block() {
        let hub = StreamHub::new(&StreamPlan::default());
        hub.set_init(vec![0, 0, 0, 8, b'f', b't', b'y', b'p']).await;

        let (_slow_id, _slow_rx, slow_dropped) = hub.register_client().await;
        let (_fast_id, mut fast_rx, fast_dropped) = hub.register_client().await;

        // Drain init segment from fast client.
        assert!(fast_rx.recv().await.is_some());

        // Publish 25 fragments without ever draining slow_rx.
        let start = Instant::now();
        for i in 0..25u8 {
            hub.publish_fragment(vec![i; 64]).await;
            while fast_rx.try_recv().is_ok() {}
        }
        assert!(start.elapsed() < Duration::from_millis(500));
        assert!(slow_dropped.load(Ordering::Relaxed) >= 15);
        assert_eq!(fast_dropped.load(Ordering::Relaxed), 0);
    }
}
