// Package ws implements the small slice of RFC 6455 that ekran needs:
// a server side WebSocket that can push binary frames (media fragments)
// and receive short text messages (remote control events).
package ws

import (
	"bufio"
	"crypto/sha1"
	"encoding/base64"
	"encoding/binary"
	"errors"
	"fmt"
	"io"
	"net"
	"net/http"
	"strings"
	"sync"
	"time"
)

const acceptMagic = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11"

// Conn is an accepted WebSocket connection.
type Conn struct {
	c    net.Conn
	r    *bufio.Reader
	wmu  sync.Mutex
	mu   sync.Mutex
	open bool
}

// Upgrade turns an HTTP request into a WebSocket connection.
func Upgrade(w http.ResponseWriter, r *http.Request) (*Conn, error) {
	if !strings.EqualFold(r.Header.Get("Upgrade"), "websocket") {
		return nil, errors.New("ws: not a websocket upgrade")
	}
	key := r.Header.Get("Sec-WebSocket-Key")
	if key == "" {
		return nil, errors.New("ws: missing key")
	}
	hj, ok := w.(http.Hijacker)
	if !ok {
		return nil, errors.New("ws: hijack unsupported")
	}
	conn, brw, err := hj.Hijack()
	if err != nil {
		return nil, err
	}
	sum := sha1.Sum([]byte(key + acceptMagic))
	accept := base64.StdEncoding.EncodeToString(sum[:])
	resp := "HTTP/1.1 101 Switching Protocols\r\n" +
		"Upgrade: websocket\r\n" +
		"Connection: Upgrade\r\n" +
		"Sec-WebSocket-Accept: " + accept + "\r\n\r\n"
	if _, err := conn.Write([]byte(resp)); err != nil {
		conn.Close()
		return nil, err
	}
	return &Conn{c: conn, r: brw.Reader, open: true}, nil
}

// WriteBinary sends a binary frame.
func (c *Conn) WriteBinary(p []byte) error {
	return c.write(0x2, p)
}

// WriteText sends a text frame.
func (c *Conn) WriteText(p []byte) error {
	return c.write(0x1, p)
}

func (c *Conn) write(op byte, payload []byte) error {
	c.mu.Lock()
	defer c.mu.Unlock()
	if !c.open {
		return errors.New("ws: closed")
	}
	header := make([]byte, 0, 10)
	header = append(header, 0x80|op)
	n := len(payload)
	switch {
	case n < 126:
		header = append(header, byte(n))
	case n < 65536:
		header = append(header, 126)
		var b [2]byte
		binary.BigEndian.PutUint16(b[:], uint16(n))
		header = append(header, b[:]...)
	default:
		header = append(header, 127)
		var b [8]byte
		binary.BigEndian.PutUint64(b[:], uint64(n))
		header = append(header, b[:]...)
	}
	c.wmu.Lock()
	defer c.wmu.Unlock()
	if _, err := c.c.Write(header); err != nil {
		return err
	}
	_, err := c.c.Write(payload)
	return err
}

// ReadMessage reads the next text or binary message from the peer.
func (c *Conn) ReadMessage() (opcode byte, payload []byte, err error) {
	for {
		var h [2]byte
		if _, err = io.ReadFull(c.r, h[:]); err != nil {
			return 0, nil, err
		}
		opcode = h[0] & 0x0f
		masked := h[1]&0x80 != 0
		length := int(h[1] & 0x7f)
		switch length {
		case 126:
			var b [2]byte
			if _, err = io.ReadFull(c.r, b[:]); err != nil {
				return 0, nil, err
			}
			length = int(binary.BigEndian.Uint16(b[:]))
		case 127:
			var b [8]byte
			if _, err = io.ReadFull(c.r, b[:]); err != nil {
				return 0, nil, err
			}
			length = int(binary.BigEndian.Uint64(b[:]))
		}
		var mask [4]byte
		if masked {
			if _, err = io.ReadFull(c.r, mask[:]); err != nil {
				return 0, nil, err
			}
		}
		data := make([]byte, length)
		if _, err = io.ReadFull(c.r, data); err != nil {
			return 0, nil, err
		}
		if masked {
			for i := range data {
				data[i] ^= mask[i%4]
			}
		}
		switch opcode {
		case 0x8: // close
			c.Close()
			return opcode, data, io.EOF
		case 0x9: // ping -> pong
			c.wmu.Lock()
			c.c.Write([]byte{0x8a, 0x00})
			c.wmu.Unlock()
			continue
		case 0xa: // pong
			continue
		default:
			return opcode, data, nil
		}
	}
}

// Close shuts the connection down.
func (c *Conn) Close() {
	c.mu.Lock()
	defer c.mu.Unlock()
	if !c.open {
		return
	}
	c.open = false
	c.wmu.Lock()
	c.c.Write([]byte{0x88, 0x00})
	c.wmu.Unlock()
	c.c.Close()
}

// SetReadDeadline applies a read timeout so dead peers do not leak.
func (c *Conn) SetReadDeadline(t time.Time) { c.c.SetReadDeadline(t) }

// ErrNotWebsocket is returned when the request was not an upgrade.
var ErrNotWebsocket = fmt.Errorf("ws: not a websocket request")
