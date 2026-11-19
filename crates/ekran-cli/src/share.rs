//! `ekran share` command: orchestrates capture, audio watcher, transport, sinks, recording, and TUI.

use crate::net::{generate_token, primary_lan_ip};
use ekran_audio::{create_loopback_null_sink, resolve_default_monitor, AudioEvent, SourceWatcher};
use ekran_capture::{list_monitors, select_capture_source, select_control_backend};
use ekran_core::{
    config::default_config_path, Config, DeviceInfo, Discovery, Error, Quality, RetryPolicy, Sink,
    StreamPlan,
};
use ekran_encode::{AdaptiveController, NetworkSample};
use ekran_sinks::{build_sink, CombinedDiscovery};
use ekran_transport::{
    BoxReader, FragmentAssembler, HlsPipeline, MediaChunk, ServerState, StaticAssets, StreamHub,
    TransportServer,
};
use ekran_ui::{player_assets, TuiAction, TuiSession, TuiState};
use std::io::IsTerminal;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::AsyncWriteExt;
use tokio::sync::{watch, RwLock};
use tracing::{info, warn};

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
    pub record: Option<PathBuf>,
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

    let audio_device = if audio_enabled {
        if args.audio_loopback {
            match create_loopback_null_sink() {
                Ok((_, dev)) => Some(dev),
                Err(e) => {
                    warn!("audio-loopback oluşturulamadı: {e}");
                    resolve_default_monitor().ok().map(|s| s.device)
                }
            }
        } else {
            match resolve_default_monitor() {
                Ok(src) => Some(src.device),
                Err(e) => {
                    warn!("ses monitor kaynağı bulunamadı: {e}");
                    None
                }
            }
        }
    } else {
        None
    };

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

    let assets = player_assets();
    let static_assets = StaticAssets {
        index_html: assets.index_html.into(),
        app_js: assets.app_js.into(),
        style_css: assets.style_css.into(),
        hls_min_js: assets.hls_min_js.into(),
    };

    let hub = Arc::new(StreamHub::new(&plan));
    if cfg.control.enabled {
        let ctrl: Arc<dyn ekran_core::Control> =
            Arc::from(select_control_backend(&cfg.control.backend, &monitor_pref));
        hub.set_control(ctrl).await;
    }

    let server_state = Arc::new(ServerState::new(
        plan.clone(),
        desc.backend,
        static_assets,
        hub.clone(),
    ));
    *server_state.monitors.write().await = list_monitors();

    let bind_addr = format!("{}:{}", plan.bind, plan.port);
    let srv = TransportServer::bind(&bind_addr, server_state.clone()).await?;
    let actual_port = srv.local_addr.port();
    let actual_base_url = format!("http://{lan_ip}:{actual_port}");
    let browser_url = format!("{actual_base_url}/?token={token}");

    // Start HLS packager for Cast/DLNA/legacy players.
    if let Ok(hls) = HlsPipeline::start(hub.clone()).await {
        server_state.set_hls(Arc::new(hls)).await;
    }

    // Optional --record <path> (T-M10-05)
    if let Some(rec_path) = args.record.clone() {
        spawn_recorder(hub.clone(), rec_path);
    }

    // Start capture supervisor first so stream init/HLS is live before TV connects.
    let (stop_tx, stop_rx) = watch::channel(false);
    let (fail_tx, mut fail_rx) = watch::channel::<Option<String>>(None);
    let shared_plan = Arc::new(RwLock::new(plan.clone()));
    spawn_capture_supervisor(
        Arc::from(capture),
        shared_plan.clone(),
        hub.clone(),
        stop_rx.clone(),
        fail_tx,
    );

    // Discover devices unless explicitly targeting browser. A named target is rescanned a
    // couple of times because a single multicast sweep can miss a sleepy TV.
    let wants_browser = args.to.as_deref() == Some("browser");
    let mut devices: Vec<DeviceInfo> = if wants_browser {
        Vec::new()
    } else {
        CombinedDiscovery::new()
            .scan(Duration::from_secs(3))
            .await
            .unwrap_or_default()
    };
    if !wants_browser {
        if let Some(target) = args.to.as_deref().filter(|t| !t.is_empty() && *t != "auto") {
            for _ in 0..2 {
                if ekran_sinks::pick_device(&devices, Some(target)).is_ok() {
                    break;
                }
                devices = CombinedDiscovery::new()
                    .scan(Duration::from_secs(3))
                    .await
                    .unwrap_or_default();
            }
        }
    }
    *server_state.devices.write().await = devices.clone();

    let active_sink: Arc<RwLock<Option<Box<dyn Sink>>>> = Arc::new(RwLock::new(None));
    let selected_sink = build_sink(&devices, args.to.as_deref(), &plan, &actual_base_url)?;
    if selected_sink.kind() != ekran_core::SinkKind::Browser {
        // Wait up to 60s for the capture init segment (covers the Wayland portal dialog).
        let mut got_init = false;
        for _ in 0..400 {
            if hub.subscribe_raw().await.0.is_some() {
                got_init = true;
                break;
            }
            if let Some(msg) = fail_rx.borrow().clone() {
                let _ = stop_tx.send(true);
                srv.shutdown();
                anyhow::bail!("{msg}");
            }
            tokio::time::sleep(Duration::from_millis(150)).await;
        }
        if !got_init {
            warn!("yakalama 60 sn içinde ilk görüntü parçasını üretemedi");
        }
    }

    let mut active_device_id = None;
    let status_line = match start_sink_with_retry(&*selected_sink, &plan).await {
        Ok(prep) => {
            active_device_id = Some(prep.device.id.clone());
            let note = prep.note.unwrap_or_else(|| prep.stream_url.clone());
            *active_sink.write().await = Some(selected_sink);
            format!("Yayın: {} ({note})", prep.device.name)
        }
        Err(err) => {
            warn!("hedef sink başlatılamadı: {} — {}", err, err.hint());
            format!("Tarayıcı yedeği: {browser_url} ({err})")
        }
    };

    if args.audio.unwrap_or(cfg.share.audio) {
        let watcher = SourceWatcher::new();
        let (mut audio_rx, audio_stop) = watcher.spawn();
        let sp = shared_plan.clone();
        let mut s_rx = stop_rx.clone();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = s_rx.changed() => {
                        if *s_rx.borrow() { break; }
                    }
                    evt = audio_rx.recv() => {
                        match evt {
                            Some(AudioEvent::SourceChanged(src)) => {
                                info!("ses kaynağı değişti -> {}", src.device);
                                sp.write().await.audio_device = Some(src.device);
                            }
                            Some(AudioEvent::Lost(msg)) => {
                                warn!("ses izleyici uyarısı: {msg}");
                            }
                            None => break,
                        }
                    }
                }
            }
            let _ = audio_stop.send(true);
        });
    }

    // Adaptive quality controller when quality == Auto (T-M2-05).
    if plan.quality == Quality::Auto {
        spawn_adaptive_loop(
            hub.clone(),
            cfg.adaptive.up_threshold,
            cfg.adaptive.down_threshold,
        );
    }

    let use_tui = !args.no_tui && std::io::stdout().is_terminal();
    if use_tui {
        run_tui_loop(
            devices,
            browser_url,
            status_line,
            active_device_id,
            hub.clone(),
            server_state.clone(),
            active_sink.clone(),
            shared_plan.clone(),
            actual_base_url,
        )
        .await?;
    } else {
        println!("ekran yayında!");
        println!("  durum     : {status_line}");
        println!("  tarayıcı  : {browser_url}");
        println!("  arka uç   : {}", desc.backend.label());
        println!("Durdurmak için Ctrl+C'ye basın.");
        let capture_failure = tokio::select! {
            r = tokio::signal::ctrl_c() => { r?; None }
            _ = fail_rx.changed() => fail_rx.borrow().clone(),
        };
        if let Some(msg) = capture_failure {
            let _ = stop_tx.send(true);
            if let Some(sink) = active_sink.write().await.take() {
                let _ = sink.stop().await;
            }
            srv.shutdown();
            anyhow::bail!("{msg}");
        }
    }

    let _ = stop_tx.send(true);
    if let Some(sink) = active_sink.write().await.take() {
        let _ = sink.stop().await;
    }
    srv.shutdown();
    Ok(())
}

async fn start_sink_with_retry(
    sink: &dyn Sink,
    plan: &StreamPlan,
) -> ekran_core::Result<ekran_core::PreparedSink> {
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
                    warn!("sink başlatma denemesi {attempt}/{max_attempts} başarısız ({e}), {wait}s bekleniyor");
                    tokio::time::sleep(Duration::from_secs(wait)).await;
                }
                Err(e) => return Err(e),
            },
            Err(e) if attempt < max_attempts => {
                let wait = 1u64 << (attempt - 1);
                warn!("sink hazırlık denemesi {attempt}/{max_attempts} başarısız ({e}), {wait}s bekleniyor");
                tokio::time::sleep(Duration::from_secs(wait)).await;
            }
            Err(e) => return Err(e),
        }
    }
}

fn spawn_capture_supervisor(
    capture: Arc<dyn ekran_core::CaptureSource>,
    shared_plan: Arc<RwLock<StreamPlan>>,
    hub: Arc<StreamHub>,
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

            let mut proc = match capture.spawn(&plan).await {
                Ok(p) => p,
                Err(e) => {
                    warn!("yakalama başlatılamadı: {} — {}", e, e.hint());
                    let msg = serde_json::json!({
                        "t": "error",
                        "code": e.code(),
                        "message": e.to_string()
                    })
                    .to_string();
                    hub.broadcast_text(msg).await;
                    if matches!(e, Error::PortalDenied(_)) {
                        // Retrying would re-open the portal dialog over and over; stop and report.
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

            let mut restart_for_audio = false;
            let mut check_tick = tokio::time::interval(Duration::from_millis(400));

            loop {
                tokio::select! {
                    _ = stop_rx.changed() => {
                        if *stop_rx.borrow() {
                            break;
                        }
                    }
                    _ = check_tick.tick() => {
                        let latest_audio = shared_plan.read().await.audio_device.clone();
                        if latest_audio != current_audio {
                            info!("ses kaynağı güncellendi; yakalama süreci yeniden bağlanıyor");
                            restart_for_audio = true;
                            break;
                        }
                    }
                    box_res = br.next_box() => {
                        match box_res {
                            Ok(Some(bx)) => {
                                if let Some(chunk) = asm.push(bx) {
                                    match chunk {
                                        MediaChunk::Init(b) => hub.set_init(b).await,
                                        MediaChunk::Fragment(b) => {
                                            if !hub.stats().await.paused {
                                                hub.publish_fragment(b).await;
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
            if !restart_for_audio {
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        }
    });
}

fn spawn_recorder(hub: Arc<StreamHub>, path: PathBuf) {
    tokio::spawn(async move {
        let Ok(mut file) = tokio::fs::File::create(&path).await else {
            warn!("kayıt dosyası açılamadı: {}", path.display());
            return;
        };
        let (init_opt, mut rx) = hub.subscribe_raw().await;
        if let Some(init) = init_opt {
            let _ = file.write_all(&init).await;
        }
        while let Ok(chunk) = rx.recv().await {
            if file.write_all(&chunk).await.is_err() {
                break;
            }
        }
        let _ = file.flush().await;
    });
}

fn spawn_adaptive_loop(hub: Arc<StreamHub>, up_threshold: f64, down_threshold: f64) {
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

#[allow(clippy::too_many_arguments)]
async fn run_tui_loop(
    devices: Vec<DeviceInfo>,
    browser_url: String,
    status_line: String,
    active_device_id: Option<String>,
    hub: Arc<StreamHub>,
    server_state: Arc<ServerState>,
    active_sink: Arc<RwLock<Option<Box<dyn Sink>>>>,
    shared_plan: Arc<RwLock<StreamPlan>>,
    host_base_url: String,
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
                    "Duraklatıldı".into()
                } else {
                    "Yayında".into()
                };
            }
            Some(TuiAction::ToggleMute) => {
                state.audio_muted = !state.audio_muted;
                let mut p = shared_plan.write().await;
                p.audio = !state.audio_muted;
            }
            Some(TuiAction::Rescan) => {
                state.status_line = "Cihazlar taranıyor…".into();
                session.draw(&state)?;
                let scanned = CombinedDiscovery::new()
                    .scan(Duration::from_secs(3))
                    .await
                    .unwrap_or_default();
                *server_state.devices.write().await = scanned.clone();
                state.devices = scanned;
                state.status_line = format!("{} cihaz bulundu", state.devices.len());
            }
            Some(TuiAction::ShareSelected(idx)) => {
                if let Some(old) = active_sink.write().await.take() {
                    let _ = old.stop().await;
                }
                let target_id = state
                    .devices
                    .get(idx)
                    .map(|d| d.id.as_str())
                    .or(Some("browser"));
                let plan = shared_plan.read().await.clone();
                match build_sink(&state.devices, target_id, &plan, &host_base_url) {
                    Ok(new_sink) => match start_sink_with_retry(&*new_sink, &plan).await {
                        Ok(prep) => {
                            state.active_device_id = Some(prep.device.id.clone());
                            state.status_line = format!("Yayın: {}", prep.device.name);
                            *active_sink.write().await = Some(new_sink);
                        }
                        Err(e) => {
                            state.status_line = format!("Hata: {e}");
                        }
                    },
                    Err(e) => {
                        state.status_line = format!("{e}");
                    }
                }
            }
            None => {}
        }
    }
    Ok(())
}
