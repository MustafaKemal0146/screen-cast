//! Synthetic headless capture source using ffmpeg `lavfi` `testsrc2` (T-M1-01).

use crate::source::list_monitors;
use screencast_core::{
    t, Backend, CaptureDescriptor, CaptureProcess, CaptureSource, Monitor, Result, StreamPlan,
    TextKey,
};
use std::process::Stdio;
use tokio::process::Command;

/// Synthetic test pattern source that runs without an X11/Wayland display.
#[derive(Debug, Clone, Default)]
pub struct TestSource {
    pub custom_codec: Option<String>,
}

impl TestSource {
    pub fn new() -> Self {
        Self::default()
    }

    /// Build the ffmpeg command arguments for `lavfi` `testsrc2` -> fMP4 on stdout.
    pub fn ffmpeg_args(&self, plan: &StreamPlan) -> Vec<String> {
        let q = match plan.quality {
            screencast_core::Quality::Auto => screencast_core::Quality::P720,
            other => other,
        };
        let fps = plan.fps.max(1);
        let width = q.width();
        let height = q.height();
        let gop_frames = q.gop_frames(fps);
        let gop_sec = q.gop_seconds();
        let frag_us = plan.fragment_duration_ms.max(100) * 1000;
        let codec = self
            .custom_codec
            .as_deref()
            .or(plan.codec.as_deref())
            .unwrap_or("libx264");

        let mut args = vec!["-hide_banner".into(), "-loglevel".into(), "warning".into()];
        if codec == "h264_vaapi" {
            args.extend(["-vaapi_device".into(), "/dev/dri/renderD128".into()]);
        }
        args.extend([
            "-re".into(),
            "-f".into(),
            "lavfi".into(),
            "-i".into(),
            format!("testsrc2=size={width}x{height}:rate={fps}"),
        ]);

        let has_audio = plan.audio && plan.audio_device.as_deref().is_some_and(|d| !d.is_empty());
        if has_audio {
            args.extend([
                "-thread_queue_size".into(),
                "4096".into(),
                "-f".into(),
                "pulse".into(),
                "-i".into(),
                plan.audio_device.clone().unwrap_or_default(),
            ]);
        }

        args.extend(["-map".into(), "0:v:0".into()]);
        if has_audio {
            args.extend(["-map".into(), "1:a:0".into()]);
        }

        let vf = if codec == "h264_vaapi" {
            "format=nv12,hwupload".to_string()
        } else {
            "format=yuv420p".to_string()
        };
        args.extend(["-vf".into(), vf]);
        if codec == "h264_nvenc" {
            args.extend([
                "-c:v".into(),
                "h264_nvenc".into(),
                "-preset".into(),
                "p1".into(),
                "-tune".into(),
                "ll".into(),
                "-rc".into(),
                "vbr".into(),
                "-zerolatency".into(),
                "1".into(),
            ]);
        } else if codec == "h264_vaapi" {
            args.extend([
                "-c:v".into(),
                "h264_vaapi".into(),
                "-rc_mode".into(),
                "VBR".into(),
            ]);
        } else {
            args.extend([
                "-c:v".into(),
                "libx264".into(),
                "-preset".into(),
                "ultrafast".into(),
                "-tune".into(),
                "zerolatency".into(),
            ]);
        }

        let is_idle = fps <= 1;
        let video_bitrate = if is_idle { "220k" } else { q.video_bitrate() };
        let maxrate = if is_idle { "250k" } else { q.video_bitrate() };
        let bufsize = if is_idle {
            "300k".to_string()
        } else {
            format!("{}M", (q.video_bitrate_kbps() * 2) / 1000)
        };
        args.extend([
            "-b:v".into(),
            video_bitrate.into(),
            "-maxrate".into(),
            maxrate.into(),
            "-bufsize".into(),
            bufsize,
            "-g".into(),
            gop_frames.to_string(),
            "-bf".into(),
            "0".into(),
            "-force_key_frames".into(),
            format!("expr:gte(t,n_forced*{gop_sec:.2})"),
        ]);

        if has_audio {
            args.extend([
                "-af".into(),
                "aresample=async=1:first_pts=0".into(),
                "-c:a".into(),
                "aac".into(),
                "-b:a".into(),
                q.audio_bitrate().into(),
                "-ar".into(),
                "48000".into(),
                "-ac".into(),
                "2".into(),
            ]);
        }

        args.extend([
            "-movflags".into(),
            "+frag_keyframe+empty_moov+default_base_moof+omit_tfhd_offset".into(),
            "-frag_duration".into(),
            frag_us.to_string(),
            "-flush_packets".into(),
            "1".into(),
            "-f".into(),
            "mp4".into(),
            "pipe:1".into(),
        ]);

        args
    }
}

#[async_trait::async_trait]
impl CaptureSource for TestSource {
    fn describe(&self) -> CaptureDescriptor {
        CaptureDescriptor {
            backend: Backend::Test,
            display: "lavfi:testsrc2".into(),
            monitor: None,
            note: Some(t(TextKey::TestSourceNote).into()),
        }
    }

    async fn spawn(&self, plan: &StreamPlan) -> Result<CaptureProcess> {
        let mut effective_plan = plan.clone();
        if let Some(req) = self.custom_codec.as_deref().or(plan.codec.as_deref()) {
            if !req.trim().is_empty() && !req.eq_ignore_ascii_case("auto") {
                let (codec, _) = crate::x11::X11Source::detect_codec_result_with_bin(
                    "ffmpeg",
                    Some(std::path::Path::new("/dev/dri")),
                    Some(req),
                )?;
                effective_plan.codec = Some(codec.to_string());
            }
        }
        let args = self.ffmpeg_args(&effective_plan);
        let child = Command::new("ffmpeg")
            .args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()?;
        CaptureProcess::from_child(child)
    }

    fn monitors(&self) -> Result<Vec<Monitor>> {
        let mons = list_monitors();
        if mons.is_empty() {
            Ok(vec![Monitor {
                index: 0,
                name: "test-0".into(),
                width: 1280,
                height: 720,
                x: 0,
                y: 0,
                primary: true,
            }])
        } else {
            Ok(mons)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncReadExt;

    #[tokio::test]
    async fn test_source_emits_ftyp_moov() {
        let src = TestSource::new();
        let plan = StreamPlan {
            quality: screencast_core::Quality::P480,
            fps: 15,
            audio: false,
            fragment_duration_ms: 250,
            ..Default::default()
        };
        let mut proc = src.spawn(&plan).await.expect("ffmpeg testsrc2 spawns");
        let mut buf = vec![0u8; 4096];
        let n = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            proc.stdout.read(&mut buf),
        )
        .await
        .expect("read timeout")
        .expect("read ok");
        assert!(n >= 8, "expected fMP4 header bytes");
        assert_eq!(&buf[4..8], b"ftyp");
        proc.stop().await;
    }

    #[test]
    fn test_source_idle_and_vaapi_args() {
        let src = TestSource::new();
        let idle_plan = StreamPlan {
            quality: screencast_core::Quality::P480,
            fps: 1,
            audio: false,
            codec: Some("h264_vaapi".into()),
            ..Default::default()
        };
        let args = src.ffmpeg_args(&idle_plan).join(" ");
        assert!(args.contains("-vaapi_device /dev/dri/renderD128"));
        assert!(args.contains("h264_vaapi"));
        assert!(args.contains("-b:v 220k -maxrate 250k -bufsize 300k"));
    }
}
