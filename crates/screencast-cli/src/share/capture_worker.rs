//! Capture supervisor spawning/restarting capture pipelines on audio or quality changes.

use screencast_core::{Backend, CaptureSource, Error, StreamPlan};
use screencast_transport::{BoxReader, FragmentAssembler, MediaChunk, StreamHub};
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{watch, RwLock};
use tracing::{info, warn};

pub(crate) fn spawn_capture_supervisor(
    capture: Arc<dyn CaptureSource>,
    backend: Backend,
    shared_plan: Arc<RwLock<StreamPlan>>,
    hub: Arc<StreamHub>,
    recent_fragments: Arc<RwLock<VecDeque<Arc<Vec<u8>>>>>,
    mut stop_rx: watch::Receiver<bool>,
    fail_tx: watch::Sender<Option<String>>,
) {
    tokio::spawn(async move {
        loop {
            if *stop_rx.borrow() {
                break;
            }
            let plan = shared_plan.read().await.clone();
            let current_audio = plan.audio_device.clone();
            let current_audio_enabled = plan.audio;
            let current_quality = plan.quality;
            let current_fps = plan.fps;

            let mut proc = match capture.spawn(&plan).await {
                Ok(p) => p,
                Err(e) => {
                    warn!("capture failed to start: {} — {}", e, e.hint());
                    let msg = serde_json::json!({
                        "t": "error",
                        "code": e.code(),
                        "message": e.to_string()
                    })
                    .to_string();
                    hub.broadcast_text(msg).await;
                    if matches!(
                        e,
                        Error::PortalDenied(_)
                            | Error::EncoderUnavailable { .. }
                            | Error::ConfigInvalid { .. }
                    ) {
                        let _ = fail_tx.send(Some(format!("{e} — {}", e.hint())));
                        break;
                    }
                    tokio::time::sleep(Duration::from_secs(2)).await;
                    continue;
                }
            };

            let stdout = std::mem::replace(&mut proc.stdout, Box::new(tokio::io::empty()));
            let mut br = BoxReader::new(stdout);
            let mut asm = FragmentAssembler::new();

            let mut restart_for_change = false;
            let mut check_tick = tokio::time::interval(Duration::from_millis(400));

            loop {
                tokio::select! {
                    _ = stop_rx.changed() => {
                        if *stop_rx.borrow() {
                            break;
                        }
                    }
                    _ = check_tick.tick() => {
                        let latest = shared_plan.read().await.clone();
                        let audio_changed = latest.audio_device != current_audio
                            || latest.audio != current_audio_enabled;
                        let encode_changed = backend != Backend::Wayland
                            && (latest.quality != current_quality || latest.fps != current_fps);
                        if audio_changed || encode_changed {
                            info!(
                                "capture plan updated (audio={}, quality/fps={}); restarting",
                                audio_changed, encode_changed
                            );
                            restart_for_change = true;
                            break;
                        }
                    }
                    box_res = br.next_box() => {
                        match box_res {
                            Ok(Some(bx)) => {
                                if let Some(chunk) = asm.push(bx) {
                                    match chunk {
                                        MediaChunk::Init(b) => {
                                            recent_fragments.write().await.clear();
                                            hub.set_init(b).await;
                                        }
                                        MediaChunk::Fragment(b) => {
                                            if !hub.stats().await.paused {
                                                let arc = Arc::new(b);
                                                {
                                                    let mut rf = recent_fragments.write().await;
                                                    if rf.len() >= 3 {
                                                        rf.pop_front();
                                                    }
                                                    rf.push_back(arc.clone());
                                                }
                                                hub.publish_fragment((*arc).clone()).await;
                                            }
                                        }
                                    }
                                }
                            }
                            Ok(None) | Err(_) => break,
                        }
                    }
                }
            }

            proc.stop().await;
            if *stop_rx.borrow() {
                break;
            }
            if !restart_for_change {
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        }
    });
}
