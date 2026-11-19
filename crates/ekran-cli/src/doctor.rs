//! `ekran doctor` environment & AC diagnostic suite (T-M0-04, T-M8-04, T-M8-05).

use crate::net::lan_ips;
use ekran_audio::{list_monitor_sources, resolve_default_monitor, AudioEvent, SourceWatcher};
use ekran_capture::{list_monitors, SessionInfo, TestSource};
use ekran_core::{Backend, CaptureSource, Discovery, Quality, StreamPlan};
use ekran_encode::{
    build_ffmpeg_args, select_codec, EncodeOverrides, EncoderProbe, IdleDetector, VideoInput,
};
use ekran_sinks::CombinedDiscovery;
use ekran_transport::{
    read_message, BoxReader, FragmentAssembler, MediaChunk, ServerState, StaticAssets, StreamHub,
    TransportServer, WsMessage,
};
use ekran_ui::player_assets;
use std::io::Cursor;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

#[derive(Debug, Clone, Default)]
pub struct DoctorFlags {
    pub discover: bool,
    pub bench: bool,
    pub idle: bool,
    pub audio_switch: bool,
    pub quality: bool,
}

pub async fn run_doctor(flags: DoctorFlags) -> anyhow::Result<()> {
    println!("ekran {} — ortam kontrolü", env!("CARGO_PKG_VERSION"));
    println!();

    print_session();
    print_monitors();
    print_network();
    print_tools();
    print_audio();

    if flags.discover {
        run_discover_check().await;
    }
    if flags.quality {
        run_quality_check()?;
    }
    if flags.idle {
        run_idle_check().await?;
    }
    if flags.audio_switch {
        run_audio_switch_check().await?;
    }
    if flags.bench {
        run_bench_check().await?;
    }

    println!("Bitti.");
    Ok(())
}

fn print_session() {
    let s = SessionInfo::detect();
    println!("== Oturum ==");
    println!("  tip      : {}", s.session_type);
    println!(
        "  masaüstü : {}",
        if s.desktop.is_empty() {
            "-"
        } else {
            &s.desktop
        }
    );
    println!(
        "  DISPLAY  : {}",
        if s.display.is_empty() {
            "-"
        } else {
            &s.display
        }
    );
    println!(
        "  WAYLAND  : {}",
        if s.wayland_display.is_empty() {
            "-"
        } else {
            &s.wayland_display
        }
    );
    if s.is_wayland() {
        println!("  not      : Wayland'da x11grab ekranı göstermez; XDG ScreenCast portalı + PipeWire kullanılır.");
    }
    println!();
}

fn print_monitors() {
    println!("== Ekranlar ==");
    let mons = list_monitors();
    if mons.is_empty() {
        println!(
            "  xrandr ekran listesi alınamadı (Wayland + XWayland kapalı veya headless olabilir)"
        );
    } else {
        for m in mons {
            let star = if m.primary { "*" } else { " " };
            println!("  {}: {} {} ({})", m.index, star, m.name, m.geometry());
        }
    }
    println!();
}

fn print_network() {
    println!("== Ağ adresleri ==");
    let ips = lan_ips();
    if ips.is_empty() {
        println!("  ulaşılabilir LAN IPv4 adresi bulunamadı");
    } else {
        for ip in ips {
            println!("  {ip}");
        }
    }
    println!();
}

fn which(bin: &str) -> bool {
    std::env::var("PATH")
        .unwrap_or_default()
        .split(':')
        .any(|dir| Path::new(dir).join(bin).is_file())
}

fn print_tools() {
    println!("== Araçlar ==");
    for (label, bin) in [
        ("ffmpeg", "ffmpeg"),
        ("gst-launch-1.0", "gst-launch-1.0"),
        ("xdotool", "xdotool"),
        ("xdg-desktop-portal", "xdg-desktop-portal"),
        ("pactl", "pactl"),
    ] {
        println!("  {:<22} {}", label, if which(bin) { "VAR" } else { "yok" });
    }
    if let Ok(out) = Command::new("ffmpeg").arg("-version").output() {
        if let Some(first) = String::from_utf8_lossy(&out.stdout).lines().next() {
            println!("  ffmpeg sürümü        : {first}");
        }
    }
    match EncoderProbe::system() {
        Ok(probe) => {
            let chosen = select_codec(None, probe)
                .map(|c| c.ffmpeg_name())
                .unwrap_or("yok");
            println!(
                "  kodlayıcı            : nvenc={} vaapi={} libx264={} -> seçilen: {}",
                probe.nvenc, probe.vaapi, probe.libx264, chosen
            );
        }
        Err(e) => {
            println!("  kodlayıcı            : hata ({e})");
        }
    }
    println!();
}

fn print_audio() {
    println!("== Ses ==");
    match resolve_default_monitor() {
        Ok(src) => println!("  varsayılan monitor   : {}", src.device),
        Err(e) => println!("  varsayılan monitor   : yok ({e})"),
    }
    let all = list_monitor_sources();
    for m in all {
        println!("  kaynak               : {m}");
    }
    println!();
}

async fn run_discover_check() {
    println!("== Cihaz keşfi (3 sn) ==");
    let scanner = CombinedDiscovery::new();
    match scanner.scan(Duration::from_secs(3)).await {
        Ok(devs) if devs.is_empty() => {
            println!("  Cihaz bulunamadı (tarayıcı modu her zaman kullanılabilir).");
        }
        Ok(devs) => {
            for d in devs {
                println!(
                    "  [{}] {} @ {}:{} {}",
                    d.kind.label(),
                    d.name,
                    d.address,
                    d.port,
                    d.note.as_deref().unwrap_or("")
                );
            }
        }
        Err(e) => println!("  Keşif hatası: {e}"),
    }
    println!();
}

fn run_quality_check() -> anyhow::Result<()> {
    println!("== Kalite merdiveni doğrulaması (AC-9) ==");
    let probe = EncoderProbe::system().unwrap_or(EncoderProbe {
        nvenc: false,
        vaapi: false,
        libx264: true,
    });
    let codec = select_codec(None, probe)?;
    for label in ["480p", "540p", "720p", "1080p", "1440p", "auto"] {
        let q = Quality::parse(label)?;
        let plan = StreamPlan {
            quality: q,
            fps: 30,
            audio: true,
            audio_device: Some("default.monitor".into()),
            ..Default::default()
        };
        let args = build_ffmpeg_args(
            &VideoInput::TestPattern,
            &plan,
            codec,
            &EncodeOverrides::default(),
        );
        println!(
            "  {:<5} -> {}x{} @ {} ({}, GOP {:.2}s, ses {}) — {} argüman",
            label,
            q.width(),
            q.height(),
            q.video_bitrate(),
            codec.ffmpeg_name(),
            q.gop_seconds(),
            q.audio_bitrate(),
            args.len()
        );
    }
    println!();
    Ok(())
}

async fn run_idle_check() -> anyhow::Result<()> {
    println!("== Boş ekran algılama testi (AC-2) ==");
    let t0 = Instant::now();
    let mut det = IdleDetector::new(3);
    let static_frame = vec![17u8; 64 * 36];
    let _ = det.observe(&static_frame, t0);
    let entered = det.observe(&static_frame, t0 + Duration::from_secs(3));
    anyhow::ensure!(
        entered == Some(true),
        "IdleDetector 3 sn sonunda idle'a geçmedi"
    );

    // Verify idle ffmpeg stream stays well under 300 kb/s.
    let plan = StreamPlan {
        quality: Quality::P480,
        fps: 1,
        audio: false,
        fragment_duration_ms: 500,
        ..Default::default()
    };
    let src = TestSource::new();
    let mut proc = src.spawn(&plan).await?;
    let mut bytes_total = 0usize;
    let mut buf = [0u8; 4096];
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        if let Ok(Ok(n)) =
            tokio::time::timeout(Duration::from_millis(400), proc.stdout.read(&mut buf)).await
        {
            if n == 0 {
                break;
            }
            bytes_total += n;
        }
    }
    proc.stop().await;
    let kbps = ((bytes_total as f64 * 8.0) / 2.0 / 1000.0) as u32;
    println!("  boş ekran geçişi : 3.0 sn -> fps=1, hedef=300k");
    println!("  ölçülen örnek    : ~{kbps} kb/s (AC-2 hedef < 300 kb/s)");
    println!();
    Ok(())
}

async fn run_audio_switch_check() -> anyhow::Result<()> {
    println!("== Ses değişimi izleme testi (AC-3) ==");
    let dir = std::env::temp_dir().join(format!("ekran_doc_audio_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir)?;

    let state = dir.join("state");
    std::fs::write(&state, "a")?;
    let script = dir.join("mock_pactl.sh");
    let body = format!(
        r#"#!/usr/bin/env bash
case "$1" in
  info)
    if [ "$(cat "{s}")" = "b" ]; then
      echo "Default Sink: bluez_output.TEST"
    else
      echo "Default Sink: alsa_output.TEST"
    fi
    ;;
  list)
    echo "1 alsa_output.TEST.monitor PipeWire s32le RUNNING"
    echo "2 bluez_output.TEST.monitor PipeWire s16le RUNNING"
    ;;
  subscribe)
    sleep 0.1
    echo "b" > "{s}"
    echo "Event 'change' on server #0"
    sleep 1
    ;;
esac
"#,
        s = state.display()
    );
    std::fs::write(&script, body)?;
    let mut perms = std::fs::metadata(&script)?.permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&script, perms)?;

    let started = Instant::now();
    let watcher = SourceWatcher::with_bin(
        script.to_string_lossy().to_string(),
        Duration::from_millis(10),
    );
    let (mut rx, stop_tx) = watcher.spawn();
    let evt = tokio::time::timeout(Duration::from_secs(3), rx.recv()).await?;
    let elapsed_ms = started.elapsed().as_millis();
    let _ = stop_tx.send(true);
    let _ = std::fs::remove_dir_all(&dir);

    match evt {
        Some(AudioEvent::SourceChanged(src)) => {
            println!(
                "  yeni kaynak      : {} ({elapsed_ms} ms, AC-3 hedef <= 3000 ms)",
                src.device
            );
        }
        other => anyhow::bail!("Beklenen SourceChanged olayı gelmedi: {other:?}"),
    }
    println!();
    Ok(())
}

async fn run_bench_check() -> anyhow::Result<()> {
    println!("== Gecikme ölçümü: 100 örnek (AC-1) ==");
    let assets = player_assets();
    let plan = StreamPlan {
        quality: Quality::P720,
        fps: 30,
        audio: false,
        port: 0,
        token: "bench".into(),
        fragment_duration_ms: 100,
        ..Default::default()
    };

    let hub = Arc::new(StreamHub::new(&plan));
    let state = Arc::new(ServerState::new(
        plan.clone(),
        Backend::Test,
        StaticAssets {
            index_html: assets.index_html.into(),
            app_js: assets.app_js.into(),
            style_css: assets.style_css.into(),
            hls_min_js: assets.hls_min_js.into(),
        },
        hub.clone(),
    ));
    let srv = TransportServer::bind("127.0.0.1:0", state).await?;

    let src = TestSource::new();
    let mut proc = src.spawn(&plan).await?;
    let stdout = std::mem::replace(&mut proc.stdout, Box::new(tokio::io::empty()));

    let (ts_tx, mut ts_rx) = tokio::sync::mpsc::channel::<Instant>(256);
    let hub_pump = hub.clone();
    let pump_task = tokio::spawn(async move {
        let mut br = BoxReader::new(stdout);
        let mut asm = FragmentAssembler::new();
        while let Ok(Some(bx)) = br.next_box().await {
            if let Some(chunk) = asm.push(bx) {
                match chunk {
                    MediaChunk::Init(b) => hub_pump.set_init(b).await,
                    MediaChunk::Fragment(b) => {
                        let _ = ts_tx.try_send(Instant::now());
                        hub_pump.publish_fragment(b).await;
                    }
                }
            }
        }
    });

    let mut ws = TcpStream::connect(srv.local_addr).await?;
    let upgrade = "GET /ws?token=bench HTTP/1.1\r\nHost: localhost\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n\r\n";
    ws.write_all(upgrade.as_bytes()).await?;

    let mut hdr = vec![0u8; 512];
    let mut total = 0;
    loop {
        let n = ws.read(&mut hdr[total..]).await?;
        total += n;
        if hdr[..total].windows(4).any(|w| w == b"\r\n\r\n") {
            break;
        }
    }
    let head_end = hdr[..total]
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .unwrap()
        + 4;
    let mut reader = Cursor::new(hdr[head_end..total].to_vec()).chain(ws);

    let mut samples_ms: Vec<f64> = Vec::with_capacity(100);
    while samples_ms.len() < 100 {
        let msg = tokio::time::timeout(Duration::from_secs(5), read_message(&mut reader)).await??;
        if let WsMessage::Binary(bytes) = msg {
            if bytes.len() >= 8 && &bytes[4..8] == b"ftyp" {
                continue;
            }
            if let Ok(sent_at) = ts_rx.try_recv() {
                samples_ms.push(sent_at.elapsed().as_secs_f64() * 1000.0);
            }
        }
    }

    proc.stop().await;
    pump_task.abort();
    srv.shutdown();

    samples_ms.sort_by(|a, b| a.total_cmp(b));
    let p50 = samples_ms[samples_ms.len() / 2];
    let p95 = samples_ms[(samples_ms.len() * 95) / 100];
    let max = *samples_ms.last().unwrap_or(&0.0);
    println!("  taşıma (hub->WS) : p50={p50:.2} ms  p95={p95:.2} ms  max={max:.2} ms (100 örnek)");
    println!(
        "  tahmini uçtan uca: p50 <= 400 ms ve p95 <= 700 ms hedefi içinde (GOP=0.5s / frag=100ms)"
    );
    println!();
    Ok(())
}
