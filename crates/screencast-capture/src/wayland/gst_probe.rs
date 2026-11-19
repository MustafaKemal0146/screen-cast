//! GStreamer video and audio encoder probing for Wayland PipeWire pipelines.

use screencast_core::{tf, Error, Result, TextKey};
use std::process::{Command as StdCommand, Stdio};
use std::time::{Duration, Instant};
use tracing::debug;

pub(crate) fn detect_gst_encoder_with_bins(
    inspect_bin: &str,
    launch_bin: &str,
    override_codec: Option<&str>,
) -> Result<&'static str> {
    if let Some(c) =
        override_codec.filter(|s| !s.trim().is_empty() && !s.eq_ignore_ascii_case("auto"))
    {
        let lower = c.trim().to_ascii_lowercase();
        match lower.as_str() {
            "nvenc" | "h264_nvenc" | "nvh264enc" => {
                if has_gst_element_with(inspect_bin, "nvh264enc")
                    && test_gst_encoder_with(launch_bin, "nvh264enc")
                {
                    return Ok("nvh264enc");
                }
                return Err(Error::EncoderUnavailable {
                    tried: vec!["nvh264enc".into()],
                });
            }
            "vaapi" | "h264_vaapi" => {
                for cand in ["vah264enc", "vaapih264enc"] {
                    if has_gst_element_with(inspect_bin, cand)
                        && test_gst_encoder_with(launch_bin, cand)
                    {
                        return Ok(cand);
                    }
                }
                return Err(Error::EncoderUnavailable {
                    tried: vec!["vah264enc".into(), "vaapih264enc".into()],
                });
            }
            "vah264enc" => {
                if has_gst_element_with(inspect_bin, "vah264enc")
                    && test_gst_encoder_with(launch_bin, "vah264enc")
                {
                    return Ok("vah264enc");
                }
                return Err(Error::EncoderUnavailable {
                    tried: vec!["vah264enc".into()],
                });
            }
            "vaapih264enc" => {
                if has_gst_element_with(inspect_bin, "vaapih264enc")
                    && test_gst_encoder_with(launch_bin, "vaapih264enc")
                {
                    return Ok("vaapih264enc");
                }
                return Err(Error::EncoderUnavailable {
                    tried: vec!["vaapih264enc".into()],
                });
            }
            "x264" | "libx264" | "cpu" | "x264enc" => {
                for cand in ["x264enc", "openh264enc"] {
                    if has_gst_element_with(inspect_bin, cand) {
                        return Ok(cand);
                    }
                }
                return Err(Error::EncoderUnavailable {
                    tried: vec!["x264enc".into(), "openh264enc".into()],
                });
            }
            "openh264" | "openh264enc" => {
                if has_gst_element_with(inspect_bin, "openh264enc") {
                    return Ok("openh264enc");
                }
                return Err(Error::EncoderUnavailable {
                    tried: vec!["openh264enc".into()],
                });
            }
            other => {
                return Err(Error::ConfigInvalid {
                    field: "codec".into(),
                    hint: tf(TextKey::WaylandCodecInvalid, &[("other", other)]),
                });
            }
        }
    }

    let mut tried = Vec::new();
    for hw in ["nvh264enc", "vah264enc", "vaapih264enc"] {
        if has_gst_element_with(inspect_bin, hw) && test_gst_encoder_with(launch_bin, hw) {
            return Ok(hw);
        }
        tried.push(hw.to_string());
    }
    for sw in ["x264enc", "openh264enc"] {
        if has_gst_element_with(inspect_bin, sw) {
            return Ok(sw);
        }
        tried.push(sw.to_string());
    }
    Err(Error::EncoderUnavailable { tried })
}

pub(crate) fn detect_gst_audio_encoder_with_bin(inspect_bin: &str) -> Option<&'static str> {
    ["avenc_aac", "fdkaacenc", "faac", "voaacenc"]
        .into_iter()
        .find(|&cand| has_gst_element_with(inspect_bin, cand))
}

pub(crate) fn has_gst_element(name: &str) -> bool {
    has_gst_element_with("gst-inspect-1.0", name)
}

fn has_gst_element_with(inspect_bin: &str, name: &str) -> bool {
    StdCommand::new(inspect_bin)
        .arg(name)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

fn test_gst_encoder_with(launch_bin: &str, elem: &str) -> bool {
    let fmt = if elem == "openh264enc" {
        "I420"
    } else {
        "NV12"
    };
    let caps = format!("video/x-raw,format={fmt},width=256,height=256,framerate=30/1");
    let mut args = vec![
        "-q".to_string(),
        "videotestsrc".into(),
        "num-buffers=1".into(),
        "!".into(),
        "videoconvert".into(),
        "!".into(),
        caps,
        "!".into(),
    ];
    match elem {
        "nvh264enc" => args.extend([
            "nvh264enc".into(),
            "preset=low-latency-hp".into(),
            "zerolatency=true".into(),
            "bitrate=2000".into(),
            "gop-size=15".into(),
        ]),
        "vah264enc" => args.extend([
            "vah264enc".into(),
            "rate-control=vbr".into(),
            "bitrate=2000".into(),
            "key-int-max=15".into(),
        ]),
        "vaapih264enc" => args.extend([
            "vaapih264enc".into(),
            "rate-control=vbr".into(),
            "bitrate=2000".into(),
            "keyframe-period=15".into(),
        ]),
        other => args.push(other.into()),
    }
    args.extend([
        "!".into(),
        "h264parse".into(),
        "!".into(),
        "fakesink".into(),
    ]);

    let Ok(mut child) = StdCommand::new(launch_bin)
        .args(&args)
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
                        debug!("GStreamer encoder probe failed ({elem}): {msg}");
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
                debug!("GStreamer encoder probe timed out ({elem})");
                return false;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::wayland::WaylandSource;
    use screencast_core::Error;

    #[test]
    fn detects_vah264enc_and_aac_fallback_with_mock_gst_bins() {
        use std::os::unix::fs::PermissionsExt;

        let dir = std::env::temp_dir().join(format!("screencast_gst_probe_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let inspect = dir.join("mock_gst_inspect.sh");
        let launch = dir.join("mock_gst_launch.sh");

        std::fs::write(
            &inspect,
            r#"#!/usr/bin/env bash
case "$1" in
  nvh264enc|vah264enc|x264enc|fdkaacenc|faac) exit 0 ;;
  *) exit 1 ;;
esac
"#,
        )
        .unwrap();
        std::fs::write(
            &launch,
            r#"#!/usr/bin/env bash
for arg in "$@"; do
  if [ "$arg" = "nvh264enc" ]; then
    echo "CUDA error" >&2
    exit 1
  fi
  if [ "$arg" = "vah264enc" ]; then
    exit 0
  fi
done
exit 1
"#,
        )
        .unwrap();

        for p in [&inspect, &launch] {
            let mut perms = std::fs::metadata(p).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(p, perms).unwrap();
        }

        let video_enc = WaylandSource::detect_gst_encoder_with_bins(
            inspect.to_str().unwrap(),
            launch.to_str().unwrap(),
            None,
        )
        .unwrap();
        assert_eq!(video_enc, "vah264enc");

        let explicit_nv_err = WaylandSource::detect_gst_encoder_with_bins(
            inspect.to_str().unwrap(),
            launch.to_str().unwrap(),
            Some("h264_nvenc"),
        );
        assert!(matches!(
            explicit_nv_err,
            Err(Error::EncoderUnavailable { .. })
        ));

        let invalid_codec_err = WaylandSource::detect_gst_encoder_with_bins(
            inspect.to_str().unwrap(),
            launch.to_str().unwrap(),
            Some("bogus_codec"),
        );
        assert!(matches!(
            invalid_codec_err,
            Err(Error::ConfigInvalid { .. })
        ));

        let audio_enc = WaylandSource::detect_gst_audio_encoder_with_bin(inspect.to_str().unwrap());
        assert_eq!(audio_enc, Some("fdkaacenc"));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
