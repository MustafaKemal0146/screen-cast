//! Active diagnostic sub-checks for `screencast doctor`: discover, quality, idle, audio, bench.

use crate::net::embedded_static_assets;
use screencast_audio::{AudioEvent, SourceWatcher};
use screencast_capture::TestSource;
use screencast_core::i18n::{t, tf, TextKey};
use screencast_core::{Backend, CaptureSource, Discovery, Quality, StreamPlan};
use screencast_encode::{
    build_ffmpeg_args_with_device, probe_vaapi_device, select_codec, Codec, EncodeOverrides,
    EncoderProbe, IdleDetector, VideoInput,
};
use screencast_sinks::CombinedDiscovery;
use screencast_transport::{
    read_message, BoxReader, FragmentAssembler, MediaChunk, ServerState, StreamHub,
    TransportServer, WsMessage,
};
use std::io::Cursor;
use std::os::unix::fs::PermissionsExt;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

pub(crate) async fn run_discover_check() {
    println!("{}", t(TextKey::DoctorSecDiscover));
    let scanner = CombinedDiscovery::new();
    match scanner.scan(Duration::from_secs(3)).await {
        Ok(devs) if devs.is_empty() => println!("{}", t(TextKey::DoctorDiscoverEmpty)),
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
        Err(e) => {
            let err_s = e.to_string();
            println!("{}", tf(TextKey::DoctorDiscoverErr, &[("err", &err_s)]));
        }
    }
    println!();
}

pub(crate) fn run_quality_check() -> anyhow::Result<()> {
    println!("{}", t(TextKey::DoctorSecQuality));
    let probe = EncoderProbe::system().unwrap_or(EncoderProbe {
        nvenc: false,
        vaapi: false,
        libx264: true,
    });
    let codec = select_codec(None, probe)?;
    let vaapi_dev = if codec == Codec::Vaapi {
        probe_vaapi_device("ffmpeg")
    } else {
        None
    };
    for label in ["480p", "540p", "720p", "1080p", "1440p", "auto"] {
        let q = Quality::parse(label)?;
        let plan = StreamPlan {
            quality: q,
            fps: 30,
            audio: true,
            audio_device: Some("default.monitor".into()),
            ..Default::default()
        };
        let args = build_ffmpeg_args_with_device(
            &VideoInput::TestPattern,
            &plan,
            codec,
            &EncodeOverrides::default(),
            vaapi_dev.as_deref(),
        );
        let w_s = q.width().to_string();
        let h_s = q.height().to_string();
        let gop_s = format!("{:.2}", q.gop_seconds());
        let n_s = args.len().to_string();
        println!(
            "{}",
            tf(
                TextKey::DoctorQualityLine,
                &[
                    ("label", &format!("{label:<5}")),
                    ("w", &w_s),
                    ("h", &h_s),
                    ("vbit", q.video_bitrate()),
                    ("codec", codec.ffmpeg_name()),
                    ("gop", &gop_s),
                    ("abit", q.audio_bitrate()),
                    ("n", &n_s)
                ]
            )
        );
    }
    println!();
    Ok(())
}

pub(crate) async fn run_idle_check() -> anyhow::Result<()> {
    println!("{}", t(TextKey::DoctorSecIdle));
    let t0 = Instant::now();
    let mut det = IdleDetector::new(3);
    let static_frame = vec![17u8; 64 * 36];
    let _ = det.observe(&static_frame, t0);
    let entered = det.observe(&static_frame, t0 + Duration::from_secs(3));
    anyhow::ensure!(entered == Some(true), "{}", t(TextKey::DoctorIdleFail));

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
    let kbps_s = kbps.to_string();
    println!("{}", t(TextKey::DoctorIdleTransition));
    println!("{}", tf(TextKey::DoctorIdleSample, &[("kbps", &kbps_s)]));
    println!();
    Ok(())
}

pub(crate) async fn run_audio_switch_check() -> anyhow::Result<()> {
    println!("{}", t(TextKey::DoctorSecAudioSwitch));
    let dir = std::env::temp_dir().join(format!("screencast_doc_audio_{}", std::process::id()));
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
            let ms_s = elapsed_ms.to_string();
            println!(
                "{}",
                tf(
                    TextKey::DoctorNewSource,
                    &[("dev", &src.device), ("ms", &ms_s)]
                )
            );
        }
        other => {
            let got_s = format!("{other:?}");
            anyhow::bail!(
                "{}",
                tf(TextKey::DoctorAudioSwitchMissing, &[("got", &got_s)])
            );
        }
    }
    println!();
    Ok(())
}

pub(crate) async fn run_bench_check() -> anyhow::Result<()> {
    println!("{}", t(TextKey::DoctorSecBench));
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
        embedded_static_assets(),
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
    let p50 = format!("{:.2}", samples_ms[samples_ms.len() / 2]);
    let p95 = format!("{:.2}", samples_ms[(samples_ms.len() * 95) / 100]);
    let max = format!("{:.2}", *samples_ms.last().unwrap_or(&0.0));
    println!(
        "{}",
        tf(
            TextKey::DoctorBenchTransport,
            &[("p50", &p50), ("p95", &p95), ("max", &max)]
        )
    );
    println!("{}", t(TextKey::DoctorBenchE2e));
    println!();
    Ok(())
}
