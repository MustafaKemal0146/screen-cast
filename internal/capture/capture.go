// Package capture starts and supervises the process that turns a screen into
// a fragmented MP4 byte stream. Two producers are supported out of the box:
//
//	x11     - ffmpeg x11grab (X11 sessions)
//	test    - ffmpeg lavfi test pattern (no display required, used by selftest)
//
// The Wayland producer (XDG ScreenCast portal + PipeWire + GStreamer) lives in
// internal/portal and plugs into the same interface.
package capture

import (
	"fmt"
	"io"
	"os/exec"
	"strconv"
	"strings"
)

// Config describes what should be captured and how it should be encoded.
type Config struct {
	Backend     string // x11 | test | wayland
	Display     string // ":0.0"
	MonitorName string
	Width       int
	Height      int
	FPS         int
	VideoCodec  string // h264_nvenc | libx264
	Bitrate     string // "6M"
	Audio       bool
	AudioDevice string
	ExtraFilter string
	Framerate   string
}

// Producer is a running capture process.
type Producer struct {
	Cmd    *exec.Cmd
	Stdout io.ReadCloser
	Stderr *strings.Builder
}

// Args builds the ffmpeg command line for the x11 and test producers.
func Args(c Config) []string {
	if c.Width <= 0 {
		c.Width = 1920
	}
	if c.Height <= 0 {
		c.Height = 1080
	}
	if c.FPS <= 0 {
		c.FPS = 30
	}
	if c.VideoCodec == "" {
		c.VideoCodec = "libx264"
	}
	if c.Bitrate == "" {
		c.Bitrate = "6M"
	}

	args := []string{"-hide_banner", "-loglevel", "warning", "-fflags", "+genpts", "-use_wallclock_as_timestamps", "1"}

	switch c.Backend {
	case "test":
		args = append(args, "-f", "lavfi", "-i",
			fmt.Sprintf("testsrc2=size=%dx%d:rate=%d", c.Width, c.Height, c.FPS))
	default: // x11
		disp := c.Display
		if disp == "" {
			disp = ":0"
		}
		args = append(args, "-f", "x11grab", "-draw_mouse", "1",
			"-video_size", fmt.Sprintf("%dx%d", c.Width, c.Height),
			"-framerate", strconv.Itoa(c.FPS), "-i", disp)
	}

	filters := []string{}
	if c.ExtraFilter != "" {
		filters = append(filters, c.ExtraFilter)
	}
	filters = append(filters, fmt.Sprintf("scale=%d:%d:force_original_aspect_ratio=decrease", c.Width, c.Height))
	filters = append(filters, "format=yuv420p")
	if len(filters) > 0 {
		args = append(args, "-vf", strings.Join(filters, ","))
	}

	if c.VideoCodec == "h264_nvenc" {
		args = append(args, "-c:v", "h264_nvenc", "-preset", "p4", "-tune", "ll",
			"-b:v", c.Bitrate, "-maxrate", c.Bitrate, "-bufsize", "12M")
	} else {
		args = append(args, "-c:v", "libx264", "-preset", "veryfast", "-tune", "zerolatency",
			"-b:v", c.Bitrate, "-maxrate", c.Bitrate, "-bufsize", "12M")
	}
	args = append(args, "-g", strconv.Itoa(c.FPS), "-bf", "0",
		"-force_key_frames", fmt.Sprintf("expr:gte(t,n_forced*%0.2f)", 1.0/float64(c.FPS)))

	if c.Audio && c.AudioDevice != "" {
		args = append(args, "-f", "pulse", "-i", c.AudioDevice, "-c:a", "aac", "-b:a", "160k", "-ar", "48000", "-ac", "2")
	}

	// Fragmented MP4 on stdout: non seekable output, so we must ask for
	// empty_moov + frag_keyframe, otherwise ffmpeg refuses to write to a pipe.
	args = append(args, "-movflags", "+frag_keyframe+empty_moov+default_base_moof+omit_tfhd_offset",
		"-frag_duration", "500000", "-flush_packets", "1", "-f", "mp4", "pipe:1")
	return args
}

// Start launches ffmpeg with the given configuration.
func Start(c Config) (*Producer, error) {
	args := Args(c)
	cmd := exec.Command("ffmpeg", args...)
	stdout, err := cmd.StdoutPipe()
	if err != nil {
		return nil, err
	}
	errBuf := &strings.Builder{}
	cmd.Stderr = errBuf
	if err := cmd.Start(); err != nil {
		return nil, fmt.Errorf("ffmpeg başlatılamadı: %w", err)
	}
	return &Producer{Cmd: cmd, Stdout: stdout, Stderr: errBuf}, nil
}

// Stop terminates the producer.
func (p *Producer) Stop() {
	if p.Cmd != nil && p.Cmd.Process != nil {
		p.Cmd.Process.Kill()
		p.Cmd.Wait()
	}
}
