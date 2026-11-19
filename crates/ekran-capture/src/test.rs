//! Synthetic headless capture source using ffmpeg `lavfi` `testsrc2` (T-M1-01).

use crate::source::list_monitors;
use ekran_core::{
    Backend, CaptureDescriptor, CaptureProcess, CaptureSource, Monitor, Result, StreamPlan,
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
            ekran_core::Quality::Auto => ekran_core::Quality::P720,
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

        let mut args = vec![
            "-hide_banner".into(),
            "-loglevel".into(),
            "warning".into(),
            "-re".into(),
            "-f".into(),
            "lavfi".into(),
            "-i".into(),
            format!("testsrc2=size={width}x{height}:rate={fps}"),
        ];

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

        args.extend(["-vf".into(), "format=yuv420p".into()]);
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

        args.extend([
            "-b:v".into(),
            q.video_bitrate().into(),
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
            note: Some("Sanal test ekranı (ekran gerektirmez)".into()),
        }
    }

    async fn spawn(&self, plan: &StreamPlan) -> Result<CaptureProcess> {
        let args = self.ffmpeg_args(plan);
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
            quality: ekran_core::Quality::P480,
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
}
