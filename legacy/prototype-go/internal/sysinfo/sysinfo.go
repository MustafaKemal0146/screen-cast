// Package sysinfo collects everything ekran needs to know about the host
// machine: which session type it is running in, which monitors exist,
// which LAN addresses are reachable and which helper tools are installed.
package sysinfo

import (
	"os"
	"os/exec"
	"regexp"
	"strconv"
	"strings"
)

// Session describes the graphical session ekran was started in.
type Session struct {
	Type          string // "x11", "wayland" or "unknown"
	Desktop       string // e.g. "zorin:GNOME"
	Display       string // X11 display, e.g. ":0"
	WaylandDisp   string // e.g. "wayland-0"
	RuntimeDir    string // XDG_RUNTIME_DIR
	Hostname      string
	LoggedInUser  string
	XWaylandBased bool
}

// IsWayland reports whether the session is a native Wayland session.
func (s Session) IsWayland() bool { return s.Type == "wayland" }

// IsX11 reports whether the session is a native X11 session.
func (s Session) IsX11() bool { return s.Type == "x11" }

// Monitor is one physical or virtual output.
type Monitor struct {
	Index    int
	Name     string // e.g. "eDP-1"
	Primary  bool
	Width    int
	Height   int
	X, Y     int
	MMWidth  int
	MMHeight int
}

// Geometry returns the "WxH+X+Y" form used by ffmpeg's crop filter.
func (m Monitor) Geometry() string {
	return strconv.Itoa(m.Width) + "x" + strconv.Itoa(m.Height) + "+" +
		strconv.Itoa(m.X) + "+" + strconv.Itoa(m.Y)
}

// DetectSession inspects the environment to figure out what kind of
// graphical session we are running in.
func DetectSession() Session {
	s := Session{
		Type:        os.Getenv("XDG_SESSION_TYPE"),
		Desktop:     os.Getenv("XDG_CURRENT_DESKTOP"),
		Display:     os.Getenv("DISPLAY"),
		WaylandDisp: os.Getenv("WAYLAND_DISPLAY"),
		RuntimeDir:  os.Getenv("XDG_RUNTIME_DIR"),
	}
	s.Hostname, _ = os.Hostname()
	s.LoggedInUser = os.Getenv("USER")
	if s.Type == "" {
		switch {
		case s.WaylandDisp != "":
			s.Type = "wayland"
		case s.Display != "":
			s.Type = "x11"
		default:
			s.Type = "unknown"
		}
	}
	// A Wayland session usually still exports DISPLAY for XWayland. When the
	// session is Wayland we deliberately ignore DISPLAY for capture, because
	// x11grab would only see XWayland windows (usually a black frame).
	s.XWaylandBased = s.Type == "wayland" && s.Display != ""
	return s
}

var monitorLine = regexp.MustCompile(`^\s*(\d+):\s+(\S+)\s+(\d+)x(\d+)\+(-?\d+)\+(-?\d+)\s+(\S+)`)

// ListMonitors returns the attached outputs. It prefers xrandr (works on X11
// and on XWayland mirrors the compositor layout) and falls back to wlr-randr.
func ListMonitors() []Monitor {
	var out []string
	if b, err := exec.Command("xrandr", "--listmonitors").Output(); err == nil {
		out = strings.Split(string(b), "\n")
	}
	if len(out) < 2 {
		if b, err := exec.Command("wlr-randr").Output(); err == nil {
			out = strings.Split(string(b), "\n")
		}
	}
	var mons []Monitor
	for _, line := range out {
		if strings.HasPrefix(line, "Monitors:") {
			continue
		}
		m := monitorLine.FindStringSubmatch(line)
		if m == nil {
			continue
		}
		mon := Monitor{Name: m[7]}
		mon.Index, _ = strconv.Atoi(m[1])
		mon.Primary = strings.Contains(m[2], "*")
		mon.Width, _ = strconv.Atoi(m[3])
		mon.Height, _ = strconv.Atoi(m[4])
		mon.X, _ = strconv.Atoi(m[5])
		mon.Y, _ = strconv.Atoi(m[6])
		mons = append(mons, mon)
	}
	return mons
}

// PrimaryMonitor returns the output flagged as primary, otherwise the first.
func PrimaryMonitor(mons []Monitor) (Monitor, bool) {
	if len(mons) == 0 {
		return Monitor{}, false
	}
	for _, m := range mons {
		if m.Primary {
			return m, true
		}
	}
	return mons[0], true
}

// LANIPs lists the global-scope IPv4 addresses of this host, skipping
// loopback, docker bridges and other obviously unreachable ranges.
func LANIPs() []string {
	b, err := exec.Command("ip", "-4", "-o", "addr", "show", "scope", "global").Output()
	if err != nil {
		return nil
	}
	var ips []string
	for _, line := range strings.Split(string(b), "\n") {
		f := strings.Fields(line)
		if len(f) < 4 {
			continue
		}
		ifname, cidr := f[1], f[3]
		if strings.HasPrefix(ifname, "docker") || strings.HasPrefix(ifname, "br-") ||
			strings.HasPrefix(ifname, "veth") || strings.HasPrefix(ifname, "zt") {
			continue
		}
		ip := strings.SplitN(cidr, "/", 2)[0]
		if strings.HasPrefix(ip, "172.1") || strings.HasPrefix(ip, "172.2") ||
			strings.HasPrefix(ip, "172.3") {
			continue
		}
		ips = append(ips, ip)
	}
	return ips
}

// Tools lists the optional helper programs ekran can use.
type Tools struct {
	FFmpeg       bool
	FFmpegVer    string
	HasX264      bool
	HasNVENC     bool
	HasVAAPI     bool
	GstLaunch    bool
	GstPipewire  bool
	GstNVENC     bool
	GstVAAPI     bool
	GstSplitMux  bool
	Xdotool      bool
	Portal       bool
	PortalGnome  bool
	Python3      bool
}

// Probe inspects the host for the tools ekran can make use of.
func Probe() Tools {
	var t Tools
	t.FFmpeg = has("ffmpeg")
	t.GstLaunch = has("gst-launch-1.0")
	t.Xdotool = has("xdotool")
	t.Python3 = has("python3")
	t.Portal = has("xdg-desktop-portal")
	t.PortalGnome = has("xdg-desktop-portal-gnome")
	if t.FFmpeg {
		if b, err := exec.Command("ffmpeg", "-version").Output(); err == nil {
			line := strings.SplitN(string(b), "\n", 2)[0]
			t.FFmpegVer = strings.TrimPrefix(line, "ffmpeg version ")
		}
		if b, err := exec.Command("ffmpeg", "-hide_banner", "-encoders").Output(); err == nil {
			s := string(b)
			t.HasX264 = strings.Contains(s, "libx264")
			t.HasNVENC = strings.Contains(s, "h264_nvenc")
			t.HasVAAPI = strings.Contains(s, "h264_vaapi")
		}
	}
	if t.GstLaunch {
		for _, e := range []struct {
			name string
			dst  *bool
		}{
			{"pipewiresrc", &t.GstPipewire},
			{"nvh264enc", &t.GstNVENC},
			{"vaapih264enc", &t.GstVAAPI},
			{"splitmuxsink", &t.GstSplitMux},
		} {
			if exec.Command("gst-inspect-1.0", e.name).Run() == nil {
				*e.dst = true
			}
		}
	}
	return t
}

func has(bin string) bool {
	_, err := exec.LookPath(bin)
	return err == nil
}

// PulseMonitorSource returns the first PulseAudio/PipeWire monitor source,
// which is what we need to capture the system sound.
func PulseMonitorSource() string {
	b, err := exec.Command("pactl", "list", "short", "sources").Output()
	if err != nil {
		return ""
	}
	for _, line := range strings.Split(string(b), "\n") {
		f := strings.Fields(line)
		if len(f) >= 2 && strings.HasSuffix(f[1], ".monitor") {
			return f[1]
		}
	}
	return ""
}
