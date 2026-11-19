//! `screencast doctor` environment & AC diagnostic suite.

mod checks;
mod wireless;

use crate::net::lan_ips;
use checks::{
    run_audio_switch_check, run_bench_check, run_discover_check, run_idle_check, run_quality_check,
};
use screencast_audio::{list_monitor_sources, resolve_default_monitor};
use screencast_capture::{list_monitors, SessionInfo, WaylandSource};
use screencast_core::i18n::{t, tf, TextKey};
use screencast_encode::{select_codec, EncoderProbe};
use std::path::Path;
use std::process::Command;
use wireless::print_wireless;

#[derive(Debug, Clone, Default)]
pub struct DoctorFlags {
    pub discover: bool,
    pub bench: bool,
    pub idle: bool,
    pub audio_switch: bool,
    pub quality: bool,
    pub wireless: bool,
}

pub async fn run_doctor(flags: DoctorFlags) -> anyhow::Result<()> {
    println!(
        "{}",
        tf(TextKey::DoctorTitle, &[("ver", env!("CARGO_PKG_VERSION"))])
    );
    println!();

    print_session();
    print_monitors();
    print_network();
    print_wireless(flags.wireless);
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

    println!("{}", t(TextKey::DoctorDone));
    Ok(())
}

fn print_session() {
    let s = SessionInfo::detect();
    println!("{}", t(TextKey::DoctorSecSession));
    println!("  {:<8} : {}", t(TextKey::DoctorKeyType), s.session_type);
    println!(
        "  {:<8} : {}",
        t(TextKey::DoctorKeyDesktop),
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
        println!("{}", t(TextKey::DoctorWaylandNote));
    }
    println!();
}

fn print_monitors() {
    println!("{}", t(TextKey::DoctorSecDisplays));
    let mons = list_monitors();
    if mons.is_empty() {
        println!("{}", t(TextKey::DoctorNoXrandr));
    } else {
        for m in mons {
            let star = if m.primary { "*" } else { " " };
            println!("  {}: {} {} ({})", m.index, star, m.name, m.geometry());
        }
    }
    println!();
}

fn print_network() {
    println!("{}", t(TextKey::DoctorSecNetwork));
    let ips = lan_ips();
    if ips.is_empty() {
        println!("{}", t(TextKey::DoctorNoLanIp));
    } else {
        for ip in ips {
            println!("  {ip}");
        }
    }
    println!();
}

fn which(bin: &str) -> bool {
    let in_path = std::env::var("PATH")
        .unwrap_or_default()
        .split(':')
        .any(|dir| Path::new(dir).join(bin).is_file());
    if in_path {
        return true;
    }
    if bin == "xdg-desktop-portal" {
        return ["/usr/lib", "/usr/libexec"]
            .iter()
            .any(|dir| Path::new(dir).join(bin).is_file());
    }
    false
}

fn print_tools() {
    println!("{}", t(TextKey::DoctorSecTools));
    for (label, bin) in [
        ("ffmpeg", "ffmpeg"),
        ("gst-launch-1.0", "gst-launch-1.0"),
        ("xdotool", "xdotool"),
        ("xdg-desktop-portal", "xdg-desktop-portal"),
        ("pactl", "pactl"),
        ("iw", "iw"),
        ("wpa_cli", "wpa_cli"),
        ("nmcli", "nmcli"),
        ("avahi-browse", "avahi-browse"),
    ] {
        let status = if which(bin) {
            t(TextKey::DoctorReadyWord)
        } else {
            t(TextKey::DoctorMissingWord)
        };
        println!("  {:<22} {}", label, status);
    }
    if let Ok(out) = Command::new("ffmpeg").arg("-version").output() {
        if let Some(first) = String::from_utf8_lossy(&out.stdout).lines().next() {
            println!("{}", tf(TextKey::DoctorFfmpegVer, &[("ver", first)]));
        }
    }
    match EncoderProbe::system() {
        Ok(probe) => {
            let chosen = select_codec(None, probe)
                .map(|c| c.ffmpeg_name())
                .unwrap_or_else(|_| t(TextKey::DoctorMissingWord));
            let nv = probe.nvenc.to_string();
            let va = probe.vaapi.to_string();
            let x264 = probe.libx264.to_string();
            println!(
                "{}",
                tf(
                    TextKey::DoctorEncoder,
                    &[
                        ("nv", &nv),
                        ("va", &va),
                        ("x264", &x264),
                        ("chosen", chosen)
                    ]
                )
            );
        }
        Err(e) => {
            println!(
                "  encoder              : {} ({e})",
                t(TextKey::DoctorErrorWord)
            );
        }
    }
    if which("gst-launch-1.0") && which("gst-inspect-1.0") {
        let gst_v =
            WaylandSource::detect_gst_encoder_with_bins("gst-inspect-1.0", "gst-launch-1.0", None)
                .unwrap_or_else(|_| t(TextKey::DoctorMissingWord));
        let gst_a = WaylandSource::detect_gst_audio_encoder_with_bin("gst-inspect-1.0")
            .unwrap_or_else(|| t(TextKey::DoctorMutedWord));
        println!(
            "{}",
            tf(TextKey::DoctorGstEncoder, &[("v", gst_v), ("a", gst_a)])
        );
    }
    println!();
}

fn print_audio() {
    println!("{}", t(TextKey::DoctorSecAudio));
    match resolve_default_monitor() {
        Ok(src) => println!(
            "{}",
            tf(TextKey::DoctorDefaultMonitor, &[("val", &src.device)])
        ),
        Err(e) => {
            let val = format!("{} ({e})", t(TextKey::DoctorMissingWord));
            println!("{}", tf(TextKey::DoctorDefaultMonitor, &[("val", &val)]));
        }
    }
    for m in list_monitor_sources() {
        println!("{}", tf(TextKey::DoctorAudioSource, &[("src", &m)]));
    }
    println!();
}
