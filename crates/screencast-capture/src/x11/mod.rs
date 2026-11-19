//! X11 screen capture source via `ffmpeg -f x11grab` (T-M1-02).

mod probe;

use crate::source::{list_monitors, SessionInfo};
use screencast_core::{
    t, Backend, CaptureDescriptor, CaptureProcess, CaptureSource, Error, Monitor, Quality, Result,
    StreamPlan, TextKey,
};
use std::path::Path;
use std::process::Stdio;
use tokio::process::Command;

/// Captures an X11 display or monitor using `ffmpeg -f x11grab`.
#[derive(Debug, Clone)]
pub struct X11Source {
    pub display: String,
    pub monitor: Option<Monitor>,
}

impl X11Source {
    pub fn new(display: String, monitor: Option<Monitor>) -> Self {
        Self { display, monitor }
    }

    fn detect_codec(override_codec: Option<&str>) -> (&'static str, String) {
        Self::detect_codec_with_bin("ffmpeg", override_codec)
    }

    /// Detect the best working ffmpeg H.264 encoder, validating hardware encoders with a 1-frame
    /// probe so missing `libcuda.so.1` or missing `/dev/dri/renderD*` never causes a false positive.
    pub fn detect_codec_with_bin(
        ffmpeg_bin: &str,
        override_codec: Option<&str>,
    ) -> (&'static str, String) {
        let is_real_ffmpeg =
            Path::new(ffmpeg_bin).file_name().and_then(|n| n.to_str()) == Some("ffmpeg");
        let dri_dir = is_real_ffmpeg.then(|| Path::new("/dev/dri"));
        Self::detect_codec_result_with_bin(ffmpeg_bin, dri_dir, override_codec)
            .unwrap_or_else(|_| ("libx264", "/dev/dri/renderD128".into()))
    }

    /// Detect a working ffmpeg H.264 encoder, returning [`Error::EncoderUnavailable`] or
    /// [`Error::ConfigInvalid`] instead of falling back blindly when probing fails.
    pub fn detect_codec_result_with_bin(
        ffmpeg_bin: &str,
        dri_dir: Option<&Path>,
        override_codec: Option<&str>,
    ) -> Result<(&'static str, String)> {
        probe::detect_codec_result_with_bin(ffmpeg_bin, dri_dir, override_codec)
    }

    /// Build the ffmpeg command arguments for `x11grab` -> fMP4 on stdout.
    pub fn ffmpeg_args(&self, plan: &StreamPlan) -> Vec<String> {
        let (codec, vaapi_dev) = Self::detect_codec(plan.codec.as_deref());
        self.ffmpeg_args_with_codec(plan, codec, &vaapi_dev)
    }

    /// Build `x11grab` ffmpeg command arguments for an already-probed `(codec, vaapi_dev)`.
    pub fn ffmpeg_args_with_codec(
        &self,
        plan: &StreamPlan,
        codec: &str,
        vaapi_dev: &str,
    ) -> Vec<String> {
        let q = match plan.quality {
            Quality::Auto => Quality::P1080,
            other => other,
        };
        let fps = plan.fps.max(1);
        let width = q.width();
        let height = q.height();
        let gop_frames = q.gop_frames(fps);
        let gop_sec = q.gop_seconds();
        let frag_us = plan.fragment_duration_ms.max(100) * 1000;
        let disp = if self.display.is_empty() {
            ":0"
        } else {
            self.display.as_str()
        };

        let mut args = vec![
            "-hide_banner".into(),
            "-loglevel".into(),
            "warning".into(),
            "-fflags".into(),
            "+genpts".into(),
            "-err_detect".into(),
            "ignore_err".into(),
        ];

        if codec == "h264_vaapi" {
            args.extend(["-vaapi_device".into(), vaapi_dev.to_string()]);
        }

        args.extend([
            "-thread_queue_size".into(),
            "1024".into(),
            "-f".into(),
            "x11grab".into(),
            "-draw_mouse".into(),
            "1".into(),
            "-framerate".into(),
            fps.to_string(),
        ]);

        if let Some(m) = &self.monitor {
            args.extend([
                "-video_size".into(),
                format!("{}x{}", m.width, m.height),
                "-i".into(),
                format!("{disp}+{},{}", m.x, m.y),
            ]);
        } else {
            args.extend([
                "-video_size".into(),
                format!("{width}x{height}"),
                "-i".into(),
                disp.into(),
            ]);
        }

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
            format!(
                "scale={width}:{height}:force_original_aspect_ratio=decrease,format=nv12,hwupload"
            )
        } else {
            format!("scale={width}:{height}:force_original_aspect_ratio=decrease,format=yuv420p")
        };
        args.extend(["-vf".into(), vf]);

        match codec {
            "h264_nvenc" => {
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
            }
            "h264_vaapi" => {
                args.extend([
                    "-c:v".into(),
                    "h264_vaapi".into(),
                    "-rc_mode".into(),
                    "VBR".into(),
                ]);
            }
            _ => {
                args.extend([
                    "-c:v".into(),
                    "libx264".into(),
                    "-preset".into(),
                    "veryfast".into(),
                    "-tune".into(),
                    "zerolatency".into(),
                ]);
            }
        }

        let is_idle = fps <= 1;
        let video_bitrate = if is_idle { "300k" } else { q.video_bitrate() };
        let bufsize = if is_idle {
            "600k".to_string()
        } else {
            format!("{}M", (q.video_bitrate_kbps() * 2) / 1000)
        };
        args.extend([
            "-b:v".into(),
            video_bitrate.into(),
            "-maxrate".into(),
            video_bitrate.into(),
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
impl CaptureSource for X11Source {
    fn describe(&self) -> CaptureDescriptor {
        CaptureDescriptor {
            backend: Backend::X11,
            display: self.display.clone(),
            monitor: self.monitor.clone(),
            note: None,
        }
    }

    async fn spawn(&self, plan: &StreamPlan) -> Result<CaptureProcess> {
        let session = SessionInfo::detect();
        if session.is_wayland() {
            return Err(Error::PortalDenied(
                t(TextKey::WaylandX11Unsupported).into(),
            ));
        }

        let (codec, vaapi_dev) = Self::detect_codec_result_with_bin(
            "ffmpeg",
            Some(Path::new("/dev/dri")),
            plan.codec.as_deref(),
        )?;
        let args = self.ffmpeg_args_with_codec(plan, codec, &vaapi_dev);
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
        Ok(list_monitors())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn x11_args_include_monitor_offset() {
        let mon = Monitor {
            index: 1,
            name: "HDMI-1".into(),
            width: 1920,
            height: 1080,
            x: 1920,
            y: 0,
            primary: false,
        };
        let src = X11Source::new(":0".into(), Some(mon));
        let plan = StreamPlan {
            quality: Quality::P1080,
            audio: false,
            codec: Some("libx264".into()),
            ..Default::default()
        };
        let args = src
            .ffmpeg_args_with_codec(&plan, "libx264", "/dev/dri/renderD128")
            .join(" ");
        assert!(args
            .contains("-f x11grab -draw_mouse 1 -framerate 30 -video_size 1920x1080 -i :0+1920,0"));
        assert!(args.contains("-f mp4 pipe:1"));

        let idle_plan = StreamPlan {
            quality: Quality::P1080,
            fps: 1,
            audio: false,
            codec: Some("libx264".into()),
            ..Default::default()
        };
        let idle_args = src
            .ffmpeg_args_with_codec(&idle_plan, "libx264", "/dev/dri/renderD128")
            .join(" ");
        assert!(idle_args.contains("-b:v 300k -maxrate 300k -bufsize 600k"));
    }
}
