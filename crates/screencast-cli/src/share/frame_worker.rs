//! Stream worker loops: MP4 recorder, adaptive quality ladder, idle detector, and JPEG snapshots.

use screencast_core::{Backend, Quality, StreamPlan};
use screencast_encode::{AdaptiveController, IdleDetector, NetworkSample};
use screencast_transport::{ServerState, StreamHub};
use std::collections::VecDeque;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;
use tokio::sync::{watch, RwLock};
use tracing::{info, warn};

pub(crate) fn spawn_recorder(
    hub: Arc<StreamHub>,
    path: PathBuf,
    mut stop_rx: watch::Receiver<bool>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let Ok(mut file) = tokio::fs::File::create(&path).await else {
            warn!("failed to open recording file: {}", path.display());
            return;
        };
        let (init_opt, mut rx) = hub.subscribe_raw().await;
        let mut wrote_init = false;
        if let Some(init) = init_opt {
            if file.write_all(&init).await.is_ok() {
                wrote_init = true;
            }
        }
        loop {
            tokio::select! {
                _ = stop_rx.changed() => {
                    if *stop_rx.borrow() { break; }
                }
                chunk = rx.recv() => {
                    match chunk {
                        Ok(bytes) => {
                            let is_init = bytes.len() >= 8 && &bytes[4..8] == b"ftyp";
                            if is_init && wrote_init {
                                continue;
                            }
                            if is_init {
                                wrote_init = true;
                            }
                            if file.write_all(&bytes).await.is_err() {
                                break;
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                        Err(_) => break,
                    }
                }
            }
        }
        let _ = file.flush().await;
    })
}

pub(crate) fn spawn_adaptive_loop(
    hub: Arc<StreamHub>,
    shared_plan: Arc<RwLock<StreamPlan>>,
    up_threshold: f64,
    down_threshold: f64,
) {
    tokio::spawn(async move {
        let mut rx = hub.subscribe_telemetry();
        let mut ctrl = AdaptiveController::new(Quality::P1080, up_threshold, down_threshold);
        while let Ok(sample) = rx.recv().await {
            if sample.measured_kbps == 0 {
                continue;
            }
            if let Some(new_q) = ctrl.update(
                NetworkSample {
                    measured_kbps: sample.measured_kbps,
                    latency_ms: sample.latency_ms,
                },
                Instant::now(),
            ) {
                shared_plan.write().await.quality = new_q;
                hub.set_quality_label(new_q.label().into(), new_q.width(), new_q.height())
                    .await;
                let msg = serde_json::json!({
                    "t": "quality",
                    "quality": new_q.label(),
                    "auto": true
                })
                .to_string();
                hub.broadcast_text(msg).await;
            }
        }
    });
}

/// Runtime `IdleDetector` supervisor: extracts 64x36 grayscale keyframes from the live
/// fMP4 stream and reduces fps when the desktop is static.
pub(crate) fn spawn_idle_watcher(
    hub: Arc<StreamHub>,
    shared_plan: Arc<RwLock<StreamPlan>>,
    backend: Backend,
    normal_fps: u32,
    idle_seconds: u64,
    idle_fps: u32,
    mut stop_rx: watch::Receiver<bool>,
) {
    tokio::spawn(async move {
        let mut detector = IdleDetector::new(idle_seconds);
        loop {
            if *stop_rx.borrow() {
                break;
            }
            let (init_opt, mut rx) = hub.subscribe_raw().await;
            let Some(init) = init_opt else {
                tokio::select! {
                    _ = stop_rx.changed() => { if *stop_rx.borrow() { break; } }
                    _ = tokio::time::sleep(Duration::from_millis(400)) => {}
                }
                continue;
            };

            let Ok(mut child) = Command::new("ffmpeg")
                .args([
                    "-hide_banner",
                    "-loglevel",
                    "error",
                    "-f",
                    "mp4",
                    "-i",
                    "pipe:0",
                    "-skip_frame",
                    "nokey",
                    "-vf",
                    "scale=64:36,format=gray",
                    "-f",
                    "rawvideo",
                    "pipe:1",
                ])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .kill_on_drop(true)
                .spawn()
            else {
                return;
            };

            let Some(mut stdin) = child.stdin.take() else {
                let _ = child.kill().await;
                return;
            };
            let Some(mut stdout) = child.stdout.take() else {
                let _ = child.kill().await;
                return;
            };

            if stdin.write_all(&init).await.is_err() {
                let _ = child.kill().await;
                continue;
            }

            let mut stop_feed = stop_rx.clone();
            let feeder = tokio::spawn(async move {
                loop {
                    tokio::select! {
                        _ = stop_feed.changed() => {
                            if *stop_feed.borrow() { break; }
                        }
                        chunk = rx.recv() => {
                            match chunk {
                                Ok(bytes) => {
                                    if bytes.len() >= 8 && &bytes[4..8] == b"ftyp" {
                                        break;
                                    }
                                    if stdin.write_all(&bytes).await.is_err() {
                                        break;
                                    }
                                }
                                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                                Err(_) => break,
                            }
                        }
                    }
                }
            });

            let mut frame = vec![0u8; 64 * 36];
            loop {
                tokio::select! {
                    _ = stop_rx.changed() => {
                        if *stop_rx.borrow() { break; }
                    }
                    r = stdout.read_exact(&mut frame) => {
                        match r {
                            Ok(_) => {
                                if let Some(is_idle) = detector.observe(&frame, Instant::now()) {
                                    hub.set_idle(is_idle).await;
                                    if backend != Backend::Wayland {
                                        shared_plan.write().await.fps = if is_idle {
                                            idle_fps.max(1)
                                        } else {
                                            normal_fps.max(1)
                                        };
                                    }
                                    info!("idle detection -> idle={is_idle}");
                                }
                            }
                            Err(_) => break,
                        }
                    }
                }
            }

            feeder.abort();
            let _ = child.kill().await;
            let _ = child.wait().await;
        }
    });
}

pub(crate) fn spawn_snapshot_updater(
    hub: Arc<StreamHub>,
    state: Arc<ServerState>,
    recent_fragments: Arc<RwLock<VecDeque<Arc<Vec<u8>>>>>,
    mut stop_rx: watch::Receiver<bool>,
) {
    tokio::spawn(async move {
        let mut has_initial = false;
        loop {
            let wait = if has_initial {
                Duration::from_secs(3)
            } else {
                Duration::from_millis(400)
            };
            tokio::select! {
                _ = stop_rx.changed() => {
                    if *stop_rx.borrow() { break; }
                }
                _ = tokio::time::sleep(wait) => {
                    if let Some(jpg) = capture_latest_snapshot_jpeg(&hub, &recent_fragments).await {
                        *state.snapshot_jpeg.write().await = Some(jpg);
                        has_initial = true;
                    }
                }
            }
        }
    });
}

pub(crate) async fn capture_latest_snapshot_jpeg(
    hub: &StreamHub,
    recent_fragments: &RwLock<VecDeque<Arc<Vec<u8>>>>,
) -> Option<Vec<u8>> {
    let (init_opt, _) = hub.subscribe_raw().await;
    let init = init_opt?;
    let frags: Vec<Arc<Vec<u8>>> = recent_fragments.read().await.iter().cloned().collect();
    if frags.is_empty() {
        return None;
    }

    let mut child = Command::new("ffmpeg")
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "mp4",
            "-i",
            "pipe:0",
            "-frames:v",
            "1",
            "-f",
            "image2",
            "-vcodec",
            "mjpeg",
            "pipe:1",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .ok()?;

    let mut stdin = child.stdin.take()?;
    let mut stdout = child.stdout.take()?;
    let feed = tokio::spawn(async move {
        let _ = stdin.write_all(&init).await;
        for frag in frags {
            if stdin.write_all(&frag).await.is_err() {
                break;
            }
        }
        let _ = stdin.flush().await;
        drop(stdin);
    });

    let mut jpg = Vec::new();
    let _ = tokio::time::timeout(Duration::from_secs(3), stdout.read_to_end(&mut jpg)).await;
    let _ = feed.await;
    let _ = child.kill().await;
    let _ = child.wait().await;
    (!jpg.is_empty()).then_some(jpg)
}
