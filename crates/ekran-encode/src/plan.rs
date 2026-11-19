//! FFmpeg argument builder matching docs/PLAN.md §11 & docs/ARCHITECTURE.md §3 (T-M1-04, T-M2-03).

use crate::codec::Codec;
use ekran_core::{Monitor, Quality, StreamPlan};

/// Input source for ffmpeg command generation.
#[derive(Debug, Clone)]
pub enum VideoInput {
    /// Headless synthetic pattern (`lavfi testsrc2`) for CI and `selftest`.
    TestPattern,
    /// X11 display grab (`x11grab`), optionally cropped to a monitor.
    X11 {
        display: String,
        monitor: Option<Monitor>,
    },
}

/// Optional encoding overrides (e.g. idle mode 1 fps / 300k).
#[derive(Debug, Clone, Default)]
pub struct EncodeOverrides {
    pub idle: bool,
    pub idle_fps: u32,
    pub idle_bitrate: Option<String>,
    pub quality_override: Option<Quality>,
}

/// Build the full `ffmpeg` argument list outputting fragmented MP4 to `pipe:1`.
pub fn build_ffmpeg_args(
    input: &VideoInput,
    plan: &StreamPlan,
    codec: Codec,
    overrides: &EncodeOverrides,
) -> Vec<String> {
    let quality = overrides.quality_override.unwrap_or(plan.quality);
    let effective_quality = match quality {
        Quality::Auto => Quality::P1080,
        q => q,
    };

    let fps = if overrides.idle {
        overrides.idle_fps.max(1)
    } else {
        plan.fps.max(1)
    };
    let bitrate = if overrides.idle {
        overrides
            .idle_bitrate
            .as_deref()
            .unwrap_or("300k")
            .to_string()
    } else {
        effective_quality.video_bitrate().to_string()
    };
    let bufsize = format!("{}M", (effective_quality.video_bitrate_kbps() * 2) / 1000);
    let bufsize = if overrides.idle {
        "600k".to_string()
    } else if bufsize == "0M" {
        "3M".to_string()
    } else {
        bufsize
    };

    let gop_sec = effective_quality.gop_seconds();
    let gop_frames = effective_quality.gop_frames(fps);
    let width = effective_quality.width();
    let height = effective_quality.height();

    let mut args = vec![
        "-hide_banner".into(),
        "-loglevel".into(),
        "warning".into(),
        "-fflags".into(),
        "+genpts".into(),
        "-err_detect".into(),
        "ignore_err".into(),
    ];

    if codec == Codec::Vaapi {
        args.extend(["-vaapi_device".into(), "/dev/dri/renderD128".into()]);
    }

    // Video input with its own thread queue (T-M2-03).
    args.extend(["-thread_queue_size".into(), "1024".into()]);
    match input {
        VideoInput::TestPattern => {
            args.extend([
                "-f".into(),
                "lavfi".into(),
                "-i".into(),
                format!("testsrc2=size={width}x{height}:rate={fps}"),
            ]);
        }
        VideoInput::X11 { display, monitor } => {
            let disp = if display.is_empty() { ":0" } else { display };
            args.extend([
                "-f".into(),
                "x11grab".into(),
                "-draw_mouse".into(),
                "1".into(),
                "-framerate".into(),
                fps.to_string(),
            ]);
            if let Some(m) = monitor {
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
                    disp.to_string(),
                ]);
            }
        }
    }

    // Audio input (PulseAudio/PipeWire monitor) with large queue & async drift compensation (T-M2-03).
    let has_audio = plan.audio && plan.audio_device.as_deref().is_some_and(|d| !d.is_empty());
    if has_audio {
        let dev = plan.audio_device.as_deref().unwrap_or("default");
        args.extend([
            "-thread_queue_size".into(),
            "4096".into(),
            "-f".into(),
            "pulse".into(),
            "-i".into(),
            dev.into(),
        ]);
    }

    // Explicit stream mapping so video+audio never drift or swap indices (T-M2-03).
    args.extend(["-map".into(), "0:v:0".into()]);
    if has_audio {
        args.extend(["-map".into(), "1:a:0".into()]);
    }

    // Video filter chain.
    let vf = match codec {
        Codec::Vaapi => format!(
            "scale={width}:{height}:force_original_aspect_ratio=decrease,format=nv12,hwupload"
        ),
        Codec::Nvenc | Codec::Libx264 => {
            format!("scale={width}:{height}:force_original_aspect_ratio=decrease,format=yuv420p")
        }
    };
    args.extend(["-vf".into(), vf]);

    // Encoder flags matching PLAN §11 & ARCHITECTURE §3.
    match codec {
        Codec::Nvenc => {
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
        Codec::Vaapi => {
            args.extend([
                "-c:v".into(),
                "h264_vaapi".into(),
                "-rc_mode".into(),
                "VBR".into(),
            ]);
        }
        Codec::Libx264 => {
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

    args.extend([
        "-b:v".into(),
        bitrate.clone(),
        "-maxrate".into(),
        bitrate,
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
            effective_quality.audio_bitrate().into(),
            "-ar".into(),
            "48000".into(),
            "-ac".into(),
            "2".into(),
        ]);
    }

    let frag_us = plan.fragment_duration_ms.max(100) * 1000;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_1080p() {
        let plan = StreamPlan {
            quality: Quality::P1080,
            fps: 30,
            audio: true,
            audio_device: Some("alsa_output.monitor".into()),
            ..Default::default()
        };
        let args = build_ffmpeg_args(
            &VideoInput::TestPattern,
            &plan,
            Codec::Nvenc,
            &EncodeOverrides::default(),
        );
        let joined = args.join(" ");
        assert!(joined.contains("testsrc2=size=1920x1080:rate=30"));
        assert!(joined.contains("-c:v h264_nvenc -preset p1 -tune ll -rc vbr -zerolatency 1"));
        assert!(joined.contains("-b:v 8M -maxrate 8M -bufsize 16M -g 15 -bf 0"));
        assert!(joined.contains("-force_key_frames expr:gte(t,n_forced*0.50)"));
        assert!(joined.contains("-thread_queue_size 4096 -f pulse -i alsa_output.monitor"));
        assert!(joined.contains("-af aresample=async=1:first_pts=0 -c:a aac -b:a 160k"));
        assert!(joined
            .contains("-movflags +frag_keyframe+empty_moov+default_base_moof+omit_tfhd_offset"));
        assert!(joined.ends_with("-f mp4 pipe:1"));
    }

    #[test]
    fn snapshot_480p() {
        let plan = StreamPlan {
            quality: Quality::P480,
            fps: 30,
            audio: false,
            ..Default::default()
        };
        let args = build_ffmpeg_args(
            &VideoInput::TestPattern,
            &plan,
            Codec::Libx264,
            &EncodeOverrides::default(),
        );
        let joined = args.join(" ");
        assert!(joined.contains("testsrc2=size=854x480:rate=30"));
        assert!(joined.contains("-c:v libx264 -preset veryfast -tune zerolatency"));
        assert!(joined.contains("-b:v 1.5M -maxrate 1.5M -bufsize 3M -g 30 -bf 0"));
        assert!(joined.contains("-force_key_frames expr:gte(t,n_forced*1.00)"));
        assert!(!joined.contains("-f pulse"));
    }
}
