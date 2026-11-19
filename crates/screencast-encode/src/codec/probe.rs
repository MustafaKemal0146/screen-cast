//! Live 1-frame ffmpeg encoder probing for NVENC, VAAPI render nodes, and libx264.

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use tracing::debug;

/// Find the first working VAAPI render device (`/dev/dri/renderD128` preferred) for `ffmpeg_bin`.
pub fn probe_vaapi_device(ffmpeg_bin: &str) -> Option<String> {
    let is_real_ffmpeg =
        Path::new(ffmpeg_bin).file_name().and_then(|n| n.to_str()) == Some("ffmpeg");
    let dri_dir = is_real_ffmpeg.then(|| Path::new("/dev/dri"));
    probe_vaapi_device_in(ffmpeg_bin, dri_dir)
}

/// Find the first working VAAPI render device inside `dri_dir` (or a synthetic fallback when `None` in tests).
pub fn probe_vaapi_device_in(ffmpeg_bin: &str, dri_dir: Option<&Path>) -> Option<String> {
    let mut devices = Vec::new();
    if let Some(dir) = dri_dir {
        let preferred = dir.join("renderD128");
        if preferred.exists() {
            devices.push(preferred.to_string_lossy().to_string());
        }
        if let Ok(entries) = std::fs::read_dir(dir) {
            let mut extra: Vec<String> = entries
                .flatten()
                .filter_map(|e| {
                    let name = e.file_name().to_string_lossy().to_string();
                    if name.starts_with("renderD") && name != "renderD128" {
                        Some(e.path().to_string_lossy().to_string())
                    } else {
                        None
                    }
                })
                .collect();
            extra.sort();
            devices.extend(extra);
        }
        if devices.is_empty() {
            return None;
        }
    } else {
        devices.push("/dev/dri/renderD128".to_string());
    }

    for dev in devices {
        let args = [
            "-hide_banner",
            "-nostdin",
            "-loglevel",
            "error",
            "-vaapi_device",
            dev.as_str(),
            "-f",
            "lavfi",
            "-i",
            "color=black:s=256x256:d=0.1",
            "-vf",
            "format=nv12,hwupload",
            "-frames:v",
            "1",
            "-c:v",
            "h264_vaapi",
            "-rc_mode",
            "VBR",
            "-b:v",
            "2M",
            "-maxrate",
            "2M",
            "-bufsize",
            "4M",
            "-bf",
            "0",
            "-f",
            "null",
            "-",
        ];
        if run_probe_cmd(ffmpeg_bin, &args, "h264_vaapi") {
            return Some(dev);
        }
    }
    None
}

pub(crate) fn probe_nvenc(ffmpeg_bin: &str) -> bool {
    let args = [
        "-hide_banner",
        "-nostdin",
        "-loglevel",
        "error",
        "-f",
        "lavfi",
        "-i",
        "color=black:s=256x256:d=0.1",
        "-vf",
        "format=yuv420p",
        "-frames:v",
        "1",
        "-c:v",
        "h264_nvenc",
        "-preset",
        "p1",
        "-tune",
        "ll",
        "-rc",
        "vbr",
        "-zerolatency",
        "1",
        "-b:v",
        "2M",
        "-f",
        "null",
        "-",
    ];
    run_probe_cmd(ffmpeg_bin, &args, "h264_nvenc")
}

pub(crate) fn probe_libx264(ffmpeg_bin: &str) -> bool {
    let args = [
        "-hide_banner",
        "-nostdin",
        "-loglevel",
        "error",
        "-f",
        "lavfi",
        "-i",
        "color=black:s=256x256:d=0.1",
        "-vf",
        "format=yuv420p",
        "-frames:v",
        "1",
        "-c:v",
        "libx264",
        "-preset",
        "veryfast",
        "-tune",
        "zerolatency",
        "-b:v",
        "2M",
        "-f",
        "null",
        "-",
    ];
    run_probe_cmd(ffmpeg_bin, &args, "libx264")
}

fn run_probe_cmd(ffmpeg_bin: &str, args: &[&str], label: &str) -> bool {
    let Ok(mut child) = Command::new(ffmpeg_bin)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
    else {
        return false;
    };

    let deadline = Instant::now() + Duration::from_secs(4);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                if status.success() {
                    return true;
                }
                if let Some(mut err) = child.stderr.take() {
                    use std::io::Read;
                    let mut buf = String::new();
                    let _ = err.read_to_string(&mut buf);
                    let msg = buf.lines().next().unwrap_or("").trim();
                    if !msg.is_empty() {
                        debug!("encoder probe failed ({label}): {msg}");
                    }
                }
                return false;
            }
            Ok(None) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(15));
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                debug!("encoder probe timed out ({label})");
                return false;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::{select_codec, Codec, EncoderProbe};

    #[test]
    fn probe_rejects_broken_nvenc_false_positive_and_falls_back_to_vaapi() {
        use std::os::unix::fs::PermissionsExt;

        let dir =
            std::env::temp_dir().join(format!("screencast_probe_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let script = dir.join("mock_ffmpeg.sh");
        let body = r#"#!/usr/bin/env bash
for arg in "$@"; do
  if [ "$arg" = "-encoders" ]; then
    echo " V....D libx264              libx264"
    echo " V....D h264_nvenc           NVIDIA NVENC"
    echo " V....D h264_vaapi           VAAPI"
    exit 0
  fi
  if [ "$arg" = "h264_nvenc" ]; then
    echo "[h264_nvenc] Cannot load libcuda.so.1" >&2
    exit 1
  fi
  if [ "$arg" = "h264_vaapi" ]; then
    exit 0
  fi
  if [ "$arg" = "libx264" ]; then
    exit 0
  fi
done
exit 1
"#;
        std::fs::write(&script, body).unwrap();
        let mut perms = std::fs::metadata(&script).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&script, perms).unwrap();

        let probe = EncoderProbe::probe_with_bin(script.to_str().unwrap()).unwrap();
        assert!(!probe.nvenc, "broken nvenc must not be marked available");
        assert!(probe.vaapi, "working vaapi should be available");
        assert!(probe.libx264);
        assert_eq!(select_codec(None, probe).unwrap(), Codec::Vaapi);
        assert!(select_codec(Some("h264_nvenc"), probe).is_err());

        let body_cpu = r#"#!/usr/bin/env bash
for arg in "$@"; do
  if [ "$arg" = "-encoders" ]; then
    echo " V....D libx264"
    echo " V....D h264_nvenc"
    echo " V....D h264_vaapi"
    exit 0
  fi
  if [ "$arg" = "libx264" ]; then
    exit 0
  fi
done
exit 1
"#;
        std::fs::write(&script, body_cpu).unwrap();
        let probe_cpu = EncoderProbe::probe_with_bin(script.to_str().unwrap()).unwrap();
        assert_eq!(
            probe_cpu,
            EncoderProbe {
                nvenc: false,
                vaapi: false,
                libx264: true
            }
        );
        assert_eq!(select_codec(None, probe_cpu).unwrap(), Codec::Libx264);

        let dri = dir.join("dri");
        std::fs::create_dir_all(&dri).unwrap();
        std::fs::write(dri.join("renderD128"), b"").unwrap();
        let d129 = dri.join("renderD129");
        std::fs::write(&d129, b"").unwrap();
        let body_d129 = format!(
            r#"#!/usr/bin/env bash
for arg in "$@"; do
  if [ "$arg" = "-encoders" ]; then
    echo " V....D libx264"
    echo " V....D h264_vaapi"
    exit 0
  fi
  if [ "$arg" = "{d129}" ]; then
    exit 0
  fi
  if [ "$arg" = "libx264" ]; then
    exit 0
  fi
done
exit 1
"#,
            d129 = d129.display()
        );
        std::fs::write(&script, body_d129).unwrap();
        let chosen_dev = probe_vaapi_device_in(script.to_str().unwrap(), Some(&dri)).unwrap();
        assert_eq!(chosen_dev, d129.to_string_lossy());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
