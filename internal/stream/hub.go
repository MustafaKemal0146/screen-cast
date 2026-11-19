// Package stream keeps the live media fan-out: one producer (ffmpeg or
// GStreamer) writes fragmented MP4 into the hub, every connected viewer
// receives the initialisation segment once and then every media fragment
// as it arrives.
package stream

import (
	"sync"
	"sync/atomic"

	"github.com/MustafaKemal0146/ekran/internal/mp4"
	"github.com/MustafaKemal0146/ekran/internal/ws"
)

// Stats describes the live state of the stream for the UI and the terminal.
type Stats struct {
	Clients    int     `json:"clients"`
	Fragments  uint64  `json:"fragments"`
	Bytes      uint64  `json:"bytes"`
	FPS        float64 `json:"fps"`
	BitrateKbps int    `json:"bitrateKbps"`
	Width      int     `json:"width"`
	Height     int     `json:"height"`
	Mode       string  `json:"mode"`
	Backend    string  `json:"backend"`
	Audio      bool    `json:"audio"`
	Started    string  `json:"started"`
}

// Hub fans media fragments out to all connected viewers.
type Hub struct {
	mu       sync.RWMutex
	clients  map[*ws.Conn]struct{}
	init     []byte
	stats    Stats
	fragSeen uint64
	byteSeen uint64
	lastFrag time.Time
	fragRate float64
}

// NewHub creates an empty hub.
func NewHub() *Hub {
	return &Hub{clients: map[*ws.Conn]struct{}{}}
}

// SetInit stores the initialisation segment and pushes it to current clients.
func (h *Hub) SetInit(b []byte) {
	h.mu.Lock()
	h.init = append([]byte(nil), b...)
	h.mu.Unlock()
	for _, c := range h.Clients() {
		c.WriteBinary(b)
	}
}

// Publish sends a media fragment to every client.
func (h *Hub) Publish(b []byte) {
	h.mu.Lock()
	h.fragSeen++
	h.byteSeen += uint64(len(b))
	h.lastFrag = time.Now()
	h.mu.Unlock()
	for _, c := range h.Clients() {
		if err := c.WriteBinary(b); err != nil {
			h.Remove(c)
		}
	}
}

// Add registers a client and sends it the init segment when known.
func (h *Hub) Add(c *ws.Conn) {
	h.mu.Lock()
	h.clients[c] = struct{}{}
	init := h.init
	h.mu.Unlock()
	if len(init) > 0 {
		c.WriteBinary(init)
	}
}

// Remove drops a client.
func (h *Hub) Remove(c *ws.Conn) {
	h.mu.Lock()
	if _, ok := h.clients[c]; ok {
		delete(h.clients, c)
	}
	h.mu.Unlock()
	c.Close()
}

// Clients returns the currently connected viewers.
func (h *Hub) Clients() []*ws.Conn {
	h.mu.RLock()
	defer h.mu.RUnlock()
	out := make([]*ws.Conn, 0, len(h.clients))
	for c := range h.clients {
		out = append(out, c)
	}
	return out
}

// ClientCount returns how many viewers are connected.
func (h *Hub) ClientCount() int {
	h.mu.RLock()
	defer h.mu.RUnlock()
	return len(h.clients)
}

// Stats returns a snapshot of the stream statistics.
func (h *Hub) Stats() Stats {
	h.mu.Lock()
	defer h.mu.Unlock()
	s := h.stats
	s.Clients = len(h.clients)
	s.Fragments = h.fragSeen
	s.Bytes = h.byteSeen
	if !h.lastFrag.IsZero() {
		s.FPS = h.fragRate
	}
	return s
}

// UpdateStats updates the descriptive fields (resolution, codec, ...).
func (h *Hub) UpdateStats(f func(*Stats)) {
	h.mu.Lock()
	f(&h.stats)
	h.mu.Unlock()
}

// ObserveFragment computes the fragment rate over a one second window.
func (h *Hub) ObserveFragment() {
	h.mu.Lock()
	h.fragSeen++
	h.lastFrag = time.Now()
	h.mu.Unlock()
}

// RateCounter is a tiny helper that turns a counter into a per second rate.
type RateCounter struct {
	last  uint64
	lastT time.Time
	rate  float64
}

// Tick returns the current rate and updates it.
func (r *RateCounter) Tick(current uint64) float64 {
	now := time.Now()
	if r.lastT.IsZero() {
		r.lastT = now
		r.last = current
		return 0
	}
	if d := now.Sub(r.lastT); d >= time.Second {
		r.rate = float64(current-r.last) / d.Seconds()
		r.last = current
		r.lastT = now
	}
	return r.rate
}

// Unused keeps the atomic import meaningful for future counters.
var _ = atomic.Uint64{}
