//! Live 1-frame ffmpeg encoder probing for X11 capture.

use screencast_core::{t, tf, Error, Result, TextKey};
use std::path::Path;
use std::process::{Command as StdCommand, Stdio};
use std::time::{Duration, Instant};
use tracing::debug;

pub(crate) fn detect_codec_result_with_bin(
    ffmpeg_bin: &str,
    dri_dir: Option<&Path>,
    override_codec: Option<&str>,
) -> Result<(&'static str, String)> {
    let out = StdCommand::new(ffmpeg_bin)
        .args(["-hide_banner", "-encoders"])
        .stdin(Stdio::null())
        .output()
        .map_err(|_| Error::EncoderUnavailable {
            tried: vec![t(TextKey::FfmpegNotFound).into()],
        })?;
    if !out.status.success() {
        return Err(Error::EncoderUnavailable {
            tried: vec![t(TextKey::FfmpegEncodersFailed).into()],
        });
    }
    let txt = String::from_utf8_lossy(&out.stdout);

    if let Some(raw) =
        override_codec.filter(|s| !s.trim().is_empty() && !s.eq_ignore_ascii_case("auto"))
    {
        return match raw.trim().to_ascii_lowercase().as_str() {
            "nvenc" | "h264_nvenc" => {
                if txt.contains("h264_nvenc") && probe_nvenc(ffmpeg_bin) {
                    Ok(("h264_nvenc", "/dev/dri/renderD128".into()))
                } else {
                    Err(Error::EncoderUnavailable {
                        tried: vec!["h264_nvenc".into()],
                    })
                }
            }
            "vaapi" | "h264_vaapi" => {
                if txt.contains("h264_vaapi") {
                    if let Some(dev) = probe_vaapi_device_in(ffmpeg_bin, dri_dir) {
                        return Ok(("h264_vaapi", dev));
                    }
                }
                Err(Error::EncoderUnavailable {
                    tried: vec!["h264_vaapi".into()],
                })
            }
            "x264" | "libx264" | "cpu" => {
                if txt.contains("libx264") && probe_libx264(ffmpeg_bin) {
                    Ok(("libx264", "/dev/dri/renderD128".into()))
                } else {
                    Err(Error::EncoderUnavailable {
                        tried: vec!["libx264".into()],
                    })
                }
            }
            other => Err(Error::ConfigInvalid {
                field: "codec".into(),
                hint: tf(TextKey::CodecHintInvalid, &[("other", other)]),
            }),
        };
    }

    if txt.contains("h264_nvenc") && probe_nvenc(ffmpeg_bin) {
        return Ok(("h264_nvenc", "/dev/dri/renderD128".into()));
    }
    if txt.contains("h264_vaapi") {
        if let Some(dev) = probe_vaapi_device_in(ffmpeg_bin, dri_dir) {
            return Ok(("h264_vaapi", dev));
        }
    }
    if txt.contains("libx264") && probe_libx264(ffmpeg_bin) {
        return Ok(("libx264", "/dev/dri/renderD128".into()));
    }

    Err(Error::EncoderUnavailable {
        tried: vec!["h264_nvenc".into(), "h264_vaapi".into(), "libx264".into()],
    })
}

fn probe_nvenc(ffmpeg_bin: &str) -> bool {
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

fn probe_libx264(ffmpeg_bin: &str) -> bool {
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

fn probe_vaapi_device_in(ffmpeg_bin: &str, dri_dir: Option<&Path>) -> Option<String> {
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

fn run_probe_cmd(ffmpeg_bin: &str, args: &[&str], label: &str) -> bool {
    let Ok(mut child) = StdCommand::new(ffmpeg_bin)
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
                        debug!("X11 encoder probe failed ({label}): {msg}");
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
                debug!("X11 encoder probe timed out ({label})");
                return false;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::x11::X11Source;

    #[test]
    fn x11_detect_codec_skips_broken_nvenc_and_selects_vaapi_or_x264() {
        use std::os::unix::fs::PermissionsExt;

        let dir = std::env::temp_dir().join(format!("screencast_x11_codec_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let script = dir.join("mock_ffmpeg.sh");
        let body = r#"#!/usr/bin/env bash
for arg in "$@"; do
  if [ "$arg" = "-encoders" ]; then
    echo " V....D libx264"
    echo " V....D h264_nvenc"
    echo " V....D h264_vaapi"
    exit 0
  fi
  if [ "$arg" = "h264_nvenc" ]; then
    echo "Cannot load libcuda.so.1" >&2
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

        let (codec, dev) = X11Source::detect_codec_with_bin(script.to_str().unwrap(), None);
        assert_eq!(codec, "h264_vaapi");
        assert_eq!(dev, "/dev/dri/renderD128");
        assert!(X11Source::detect_codec_result_with_bin(
            script.to_str().unwrap(),
            None,
            Some("h264_nvenc")
        )
        .is_err());
        assert!(matches!(
            X11Source::detect_codec_result_with_bin(script.to_str().unwrap(), None, Some("bogus")),
            Err(Error::ConfigInvalid { .. })
        ));

        let body_x264 = r#"#!/usr/bin/env bash
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
        std::fs::write(&script, body_x264).unwrap();
        let (codec2, _) =
            X11Source::detect_codec_result_with_bin(script.to_str().unwrap(), None, Some("auto"))
                .unwrap();
        assert_eq!(codec2, "libx264");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
