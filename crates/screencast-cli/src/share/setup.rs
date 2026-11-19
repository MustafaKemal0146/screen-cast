//! Codec validation, audio monitor/loopback initialization, and device discovery helpers.

use screencast_audio::{
    create_loopback_null_sink, resolve_default_monitor, AudioEvent, LoopbackGuard, SourceWatcher,
};
use screencast_capture::{WaylandSource, X11Source};
use screencast_core::{Backend, DeviceInfo, Discovery, Result, StreamPlan};
use screencast_sinks::{pick_device, CombinedDiscovery};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{watch, RwLock};
use tracing::{info, warn};

pub(crate) fn validate_codec_for_backend(backend: Backend, codec: Option<&str>) -> Result<()> {
    match backend {
        Backend::X11 => {
            X11Source::detect_codec_result_with_bin("ffmpeg", Some(Path::new("/dev/dri")), codec)
                .map(|_| ())
        }
        Backend::Test => {
            if let Some(c) =
                codec.filter(|s| !s.trim().is_empty() && !s.eq_ignore_ascii_case("auto"))
            {
                X11Source::detect_codec_result_with_bin(
                    "ffmpeg",
                    Some(Path::new("/dev/dri")),
                    Some(c),
                )
                .map(|_| ())
            } else {
                Ok(())
            }
        }
        Backend::Wayland => {
            WaylandSource::detect_gst_encoder_with_bins("gst-inspect-1.0", "gst-launch-1.0", codec)
                .map(|_| ())
        }
    }
}

pub(crate) fn init_audio_device(
    audio_enabled: bool,
    audio_loopback: bool,
) -> (Option<String>, Option<LoopbackGuard>) {
    if !audio_enabled {
        return (None, None);
    }
    if audio_loopback {
        match create_loopback_null_sink() {
            Ok((module_id, dev)) => {
                let guard = LoopbackGuard {
                    module_id,
                    device: dev.clone(),
                };
                return (Some(dev), Some(guard));
            }
            Err(e) => {
                warn!("failed to create audio-loopback: {e}");
                return (resolve_default_monitor().ok().map(|s| s.device), None);
            }
        }
    }
    match resolve_default_monitor() {
        Ok(src) => (Some(src.device), None),
        Err(e) => {
            warn!("audio monitor source not found: {e}");
            (None, None)
        }
    }
}

pub(crate) async fn discover_target_devices(
    target: Option<&str>,
    use_active_p2p: bool,
) -> Vec<DeviceInfo> {
    if target == Some("browser") {
        return Vec::new();
    }
    let mut devices = CombinedDiscovery::new()
        .with_active_p2p(use_active_p2p)
        .scan(Duration::from_secs(3))
        .await
        .unwrap_or_default();

    if let Some(target_name) = target.filter(|t| !t.is_empty() && *t != "auto") {
        for _ in 0..2 {
            if pick_device(&devices, Some(target_name)).is_ok() {
                break;
            }
            devices = CombinedDiscovery::new()
                .with_active_p2p(use_active_p2p)
                .scan(Duration::from_secs(3))
                .await
                .unwrap_or_default();
        }
    }
    devices
}

pub(crate) fn spawn_audio_watcher(
    shared_plan: Arc<RwLock<StreamPlan>>,
    mut stop_rx: watch::Receiver<bool>,
) {
    let watcher = SourceWatcher::new();
    let (mut audio_rx, audio_stop) = watcher.spawn();
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = stop_rx.changed() => {
                    if *stop_rx.borrow() { break; }
                }
                evt = audio_rx.recv() => {
                    match evt {
                        Some(AudioEvent::SourceChanged(src)) => {
                            info!("audio source changed -> {}", src.device);
                            shared_plan.write().await.audio_device = Some(src.device);
                        }
                        Some(AudioEvent::Lost(msg)) => {
                            warn!("audio watcher warning: {msg}");
                        }
                        None => break,
                    }
                }
            }
        }
        let _ = audio_stop.send(true);
    });
}
