//! `screencast share` command: orchestrates capture, audio watcher, idle/adaptive
//! controllers, transport, sinks, snapshots, recording, and TUI.

mod capture_worker;
mod frame_worker;
mod setup;
mod tui_loop;

use crate::net::{embedded_static_assets, generate_token, primary_lan_ip};
use capture_worker::spawn_capture_supervisor;
use frame_worker::{
    spawn_adaptive_loop, spawn_idle_watcher, spawn_recorder, spawn_snapshot_updater,
};
use screencast_capture::{list_monitors, select_capture_source, select_control_backend};
use screencast_core::config::default_config_path;
use screencast_core::i18n::{t, tf, TextKey};
use screencast_core::{
    Config, Control, Error, PreparedSink, Quality, Result, RetryPolicy, Sink, SinkKind, StreamPlan,
};
use screencast_sinks::{build_sink, description_xml, DlnaAdvertiser};
use screencast_transport::{HlsPipeline, ServerState, StreamHub, TransportServer};
use setup::{
    discover_target_devices, init_audio_device, spawn_audio_watcher, validate_codec_for_backend,
};
use std::collections::VecDeque;
use std::io::IsTerminal;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{watch, RwLock};
use tracing::warn;
use tui_loop::run_tui_loop;

#[derive(Debug, Clone)]
pub struct ShareArgs {
    pub to: Option<String>,
    pub quality: Option<String>,
    pub fps: Option<u32>,
    pub audio: Option<bool>,
    pub audio_loopback: bool,
    pub codec: Option<String>,
    pub backend: Option<String>,
    pub monitor: Option<String>,
    pub port: Option<u16>,
    pub token: Option<String>,
    pub no_tui: bool,
    pub p2p: bool,
    pub record: Option<PathBuf>,
}

fn target_requests_p2p(to: Option<&str>) -> bool {
    to.is_some_and(|t| {
        let s = t.trim();
        s.starts_with("wfd-p2p-") || (s.len() == 17 && s.chars().filter(|&c| c == ':').count() == 5)
    })
}

pub async fn run_share(args: ShareArgs) -> anyhow::Result<()> {
    let cfg = Config::load_or_default(&default_config_path())?;
    let quality = match &args.quality {
        Some(q) => Quality::parse(q)?,
        None => cfg.share.quality,
    };
    let fps = args.fps.unwrap_or(cfg.share.fps);
    let audio_enabled = args.audio.unwrap_or(cfg.share.audio);
    let port = args.port.unwrap_or(cfg.share.port);
    let backend_pref = args
        .backend
        .clone()
        .unwrap_or_else(|| cfg.capture.backend.clone());
    let monitor_pref = args
        .monitor
        .clone()
        .unwrap_or_else(|| cfg.capture.monitor.clone());
    let token = args
        .token
        .clone()
        .filter(|t| !t.is_empty())
        .or_else(|| (!cfg.share.token.is_empty()).then(|| cfg.share.token.clone()))
        .unwrap_or_else(generate_token);

    let (audio_device, _loopback_guard) = init_audio_device(audio_enabled, args.audio_loopback);
    let lan_ip = primary_lan_ip();
    let host_base_url = format!("http://{lan_ip}:{port}");
    let plan = StreamPlan {
        quality,
        fps,
        audio: audio_enabled && audio_device.is_some(),
        port,
        token: token.clone(),
        bind: cfg.share.bind.clone(),
        fragment_duration_ms: 500,
        codec: args.codec.clone(),
        audio_device,
        base_url: Some(host_base_url.clone()),
    };

    let capture = select_capture_source(&backend_pref, &monitor_pref)?;
    let desc = capture.describe();
    validate_codec_for_backend(desc.backend, plan.codec.as_deref())?;

    let hub = Arc::new(StreamHub::new(&plan));
    if cfg.control.enabled {
        let ctrl: Arc<dyn Control> =
            Arc::from(select_control_backend(&cfg.control.backend, &monitor_pref));
        hub.set_control(ctrl).await;
    }

    let server_state = Arc::new(ServerState::new(
        plan.clone(),
        desc.backend,
        embedded_static_assets(),
        hub.clone(),
    ));
    *server_state.monitors.write().await = list_monitors();
    *server_state.dmr_description_xml.write().await = Some(description_xml("screencast", &token));

    let bind_addr = format!("{}:{}", plan.bind, plan.port);
    let srv = TransportServer::bind(&bind_addr, server_state.clone()).await?;
    let actual_port = srv.local_addr.port();
    let actual_base_url = format!("http://{lan_ip}:{actual_port}");
    let browser_url = format!("{actual_base_url}/?token={token}");

    let _dlna_advertiser =
        DlnaAdvertiser::start(format!("{actual_base_url}/description.xml"), token.clone());
    if let Ok(hls) = HlsPipeline::start(hub.clone()).await {
        server_state.set_hls(Arc::new(hls)).await;
    }

    let (stop_tx, stop_rx) = watch::channel(false);
    let (fail_tx, mut fail_rx) = watch::channel::<Option<String>>(None);
    let shared_plan = Arc::new(RwLock::new(plan.clone()));
    let recent_fragments: Arc<RwLock<VecDeque<Arc<Vec<u8>>>>> =
        Arc::new(RwLock::new(VecDeque::with_capacity(4)));

    let recorder_handle = args
        .record
        .clone()
        .map(|rec_path| spawn_recorder(hub.clone(), rec_path, stop_rx.clone()));

    spawn_capture_supervisor(
        Arc::from(capture),
        desc.backend,
        shared_plan.clone(),
        hub.clone(),
        recent_fragments.clone(),
        stop_rx.clone(),
        fail_tx,
    );
    spawn_idle_watcher(
        hub.clone(),
        shared_plan.clone(),
        desc.backend,
        fps,
        cfg.adaptive.idle_seconds,
        cfg.adaptive.idle_fps,
        stop_rx.clone(),
    );
    spawn_snapshot_updater(
        hub.clone(),
        server_state.clone(),
        recent_fragments.clone(),
        stop_rx.clone(),
    );

    let use_active_p2p = args.p2p || target_requests_p2p(args.to.as_deref());
    let devices = discover_target_devices(args.to.as_deref(), use_active_p2p).await;
    *server_state.devices.write().await = devices.clone();

    let active_sink: Arc<RwLock<Option<Box<dyn Sink>>>> = Arc::new(RwLock::new(None));
    let selected_sink = build_sink(&devices, args.to.as_deref(), &plan, &actual_base_url)?;
    if selected_sink.kind() != SinkKind::Browser {
        wait_for_sink_readiness(&hub, &server_state, &*selected_sink, &plan, &fail_rx).await;
    }

    if let Some(msg) = fail_rx.borrow().clone() {
        let _ = stop_tx.send(true);
        if let Some(h) = recorder_handle {
            let _ = tokio::time::timeout(Duration::from_secs(2), h).await;
        }
        srv.shutdown();
        anyhow::bail!("{msg}");
    }

    let mut active_device_id = None;
    let status_line = match start_sink_with_retry(&*selected_sink, &plan).await {
        Ok(prep) => {
            active_device_id = Some(prep.device.id.clone());
            let note = prep.note.unwrap_or_else(|| prep.stream_url.clone());
            *active_sink.write().await = Some(selected_sink);
            tf(
                TextKey::ShareStatusLive,
                &[("name", &prep.device.name), ("note", &note)],
            )
        }
        Err(err) => {
            warn!("failed to start target sink: {} — {}", err, err.hint());
            let err_s = err.to_string();
            tf(
                TextKey::ShareStatusBrowserFallback,
                &[("url", &browser_url), ("err", &err_s)],
            )
        }
    };

    if audio_enabled && _loopback_guard.is_none() {
        spawn_audio_watcher(shared_plan.clone(), stop_rx.clone());
    }
    if plan.quality == Quality::Auto {
        spawn_adaptive_loop(
            hub.clone(),
            shared_plan.clone(),
            cfg.adaptive.up_threshold,
            cfg.adaptive.down_threshold,
        );
    }

    let use_tui = !args.no_tui && std::io::stdout().is_terminal();
    let tui_res: anyhow::Result<()> = if use_tui {
        run_tui_loop(
            devices,
            browser_url,
            status_line,
            active_device_id,
            hub.clone(),
            server_state.clone(),
            active_sink.clone(),
            shared_plan.clone(),
            recent_fragments.clone(),
            actual_base_url,
            use_active_p2p,
            fail_rx.clone(),
        )
        .await
    } else {
        println!("{}", t(TextKey::ShareBannerLive));
        println!(
            "{}",
            tf(TextKey::ShareBannerStatus, &[("status", &status_line)])
        );
        println!(
            "{}",
            tf(TextKey::ShareBannerBrowser, &[("url", &browser_url)])
        );
        println!(
            "{}",
            tf(
                TextKey::ShareBannerBackend,
                &[("backend", desc.backend.label())]
            )
        );
        println!("{}", t(TextKey::ShareBannerCtrlC));
        let capture_failure = tokio::select! {
            r = tokio::signal::ctrl_c() => { r?; None }
            _ = fail_rx.changed() => fail_rx.borrow().clone(),
        };
        match capture_failure {
            Some(msg) => Err(anyhow::anyhow!("{msg}")),
            None => Ok(()),
        }
    };

    let _ = stop_tx.send(true);
    if let Some(h) = recorder_handle {
        let _ = tokio::time::timeout(Duration::from_secs(2), h).await;
    }
    if let Some(sink) = active_sink.write().await.take() {
        let _ = sink.stop().await;
    }
    srv.shutdown();
    tui_res
}

async fn wait_for_sink_readiness(
    hub: &StreamHub,
    server_state: &ServerState,
    selected_sink: &dyn Sink,
    plan: &StreamPlan,
    fail_rx: &watch::Receiver<Option<String>>,
) {
    let mut got_init = false;
    for _ in 0..400 {
        if hub.subscribe_raw().await.0.is_some() {
            got_init = true;
            break;
        }
        if fail_rx.borrow().is_some() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(150)).await;
    }
    if !got_init {
        warn!("capture did not produce initial video fragment within 60s");
    } else if matches!(selected_sink.kind(), SinkKind::Cast | SinkKind::Dlna) {
        for _ in 0..40 {
            let ready = server_state
                .hls
                .read()
                .await
                .as_ref()
                .and_then(|h| h.read_playlist(&plan.token))
                .is_some_and(|m| m.contains(".ts"));
            if ready {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
}

pub(crate) async fn start_sink_with_retry(
    sink: &dyn Sink,
    plan: &StreamPlan,
) -> Result<PreparedSink> {
    let mut attempt = 0u8;
    let max_attempts = match Error::SinkRefused(String::new()).retry_policy() {
        RetryPolicy::Backoff { max_attempts } => max_attempts,
        _ => 1,
    };
    loop {
        attempt += 1;
        match sink.prepare(plan).await {
            Ok(prep) => match sink.start(prep.clone()).await {
                Ok(()) => return Ok(prep),
                Err(e) if attempt < max_attempts => {
                    let wait = 1u64 << (attempt - 1);
                    warn!("sink start attempt {attempt}/{max_attempts} failed ({e}), retrying in {wait}s");
                    tokio::time::sleep(Duration::from_secs(wait)).await;
                }
                Err(e) => return Err(e),
            },
            Err(e) if attempt < max_attempts => {
                let wait = 1u64 << (attempt - 1);
                warn!("sink prepare attempt {attempt}/{max_attempts} failed ({e}), retrying in {wait}s");
                tokio::time::sleep(Duration::from_secs(wait)).await;
            }
            Err(e) => return Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_p2p_target_identifiers() {
        assert!(target_requests_p2p(Some("wfd-p2p-aabbccddeeff")));
        assert!(target_requests_p2p(Some("aa:bb:cc:dd:ee:ff")));
        assert!(!target_requests_p2p(Some("browser")));
        assert!(!target_requests_p2p(Some("mice-192.168.1.10-7250")));
        assert!(!target_requests_p2p(None));
    }
}
