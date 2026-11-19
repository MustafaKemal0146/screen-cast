// Package mp4 reads the top level box structure of a fragmented MP4 stream.
// ekran does not decode video itself: it only needs to know which bytes
// form the initialisation segment (ftyp/moov) and which bytes form a
// media fragment (moof/mdat) so it can hand them to the browser's Media
// Source Extensions without re-muxing anything.
package mp4

import (
	"encoding/binary"
	"fmt"
	"io"
)

// Box is one top level MP4 box.
type Box struct {
	Type string
	Size int
	Data []byte // full box including header
}

// InitTypes are boxes that belong to the initialisation segment.
var InitTypes = map[string]bool{
	"ftyp": true,
	"moov": true,
	"free": true,
	"skip": true,
}

// FragmentTypes are boxes that make up a media fragment.
var FragmentTypes = map[string]bool{
	"moof": true,
	"mdat": true,
	"sidx": true,
	"styp": true,
	"sbgp": true,
	"sgpd": true,
	"subs": true,
}

// Reader parses a stream of top level boxes.
type Reader struct {
	r   io.Reader
	hdr [8]byte
}

// NewReader wraps r.
func NewReader(r io.Reader) *Reader { return &Reader{r: r} }

// Next returns the next box, or io.EOF at the end of the stream.
func (b *Reader) Next() (Box, error) {
	if _, err := io.ReadFull(b.r, b.hdr[:]); err != nil {
		return Box{}, err
	}
	size := int(binary.BigEndian.Uint32(b.hdr[0:4]))
	typ := string(b.hdr[4:8])
	headerLen := 8
	if size == 1 {
		var ext [8]byte
		if _, err := io.ReadFull(b.r, ext[:]); err != nil {
			return Box{}, err
		}
		size = int(binary.BigEndian.Uint64(ext[:]))
		headerLen = 16
	} else if size == 0 {
		// box extends to the end of the stream; read the rest
		rest, err := io.ReadAll(b.r)
		if err != nil {
			return Box{}, err
		}
		return Box{Type: typ, Size: headerLen + len(rest), Data: append(b.hdr[:], rest...)}, nil
	}
	if size < headerLen {
		return Box{}, fmt.Errorf("mp4: invalid box size %d for %q", size, typ)
	}
	data := make([]byte, size)
	copy(data, b.hdr[:])
	if headerLen == 16 {
		// re-read extended header bytes that we already consumed
		if _, err := io.ReadFull(b.r, data[8:16]); err != nil {
			return Box{}, err
		}
	}
	if _, err := io.ReadFull(b.r, data[headerLen:]); err != nil {
		return Box{}, err
	}
	return Box{Type: typ, Size: size, Data: data}, nil
}
