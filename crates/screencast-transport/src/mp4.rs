//! Fragmented MP4 (fMP4) top-level box parser and init/fragment assembler (T-M1-06).
//! Ported from `legacy/prototype-go/internal/mp4/mp4.go`.

use screencast_core::{t, tf, Error, Result, TextKey};
use tokio::io::{AsyncRead, AsyncReadExt};

/// Top-level ISO BMFF / fMP4 box.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mp4Box {
    pub box_type: [u8; 4],
    pub data: Vec<u8>,
}

impl Mp4Box {
    pub fn type_str(&self) -> &str {
        std::str::from_utf8(&self.box_type).unwrap_or("????")
    }

    pub fn kind(&self) -> BoxKind {
        match &self.box_type {
            b"ftyp" | b"moov" | b"free" | b"skip" => BoxKind::Init,
            b"moof" | b"mdat" | b"sidx" | b"styp" | b"sbgp" | b"sgpd" | b"subs" => {
                BoxKind::Fragment
            }
            _ => BoxKind::Other,
        }
    }
}

/// Classification of top-level MP4 boxes for MSE streaming.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoxKind {
    Init,
    Fragment,
    Other,
}

/// Async reader over a fragmented MP4 byte stream (`pipe:1` from ffmpeg or GStreamer).
pub struct BoxReader<R> {
    reader: R,
}

impl<R: AsyncRead + Unpin> BoxReader<R> {
    pub fn new(reader: R) -> Self {
        Self { reader }
    }

    /// Read the next top-level MP4 box, returning `Ok(None)` at clean EOF.
    pub async fn next_box(&mut self) -> Result<Option<Mp4Box>> {
        let mut hdr = [0u8; 8];
        let n = match read_exact_or_eof(&mut self.reader, &mut hdr).await? {
            0 => return Ok(None),
            n => n,
        };
        if n < 8 {
            return Err(Error::Protocol(t(TextKey::Mp4IncompleteBoxHeader).into()));
        }

        let mut size = u32::from_be_bytes([hdr[0], hdr[1], hdr[2], hdr[3]]) as usize;
        let box_type = [hdr[4], hdr[5], hdr[6], hdr[7]];
        let mut header_len = 8usize;
        let mut ext = [0u8; 8];

        if size == 1 {
            self.reader.read_exact(&mut ext).await?;
            let ext_size = u64::from_be_bytes(ext);
            if !(16..=128 * 1024 * 1024).contains(&ext_size) {
                return Err(Error::Protocol(tf(
                    TextKey::Mp4InvalidBoxSize64,
                    &[("size", &ext_size.to_string())],
                )));
            }
            size = ext_size as usize;
            header_len = 16;
        } else if size == 0 {
            let mut rest = Vec::new();
            self.reader.read_to_end(&mut rest).await?;
            let mut data = Vec::with_capacity(8 + rest.len());
            data.extend_from_slice(&hdr);
            data.extend_from_slice(&rest);
            return Ok(Some(Mp4Box { box_type, data }));
        }

        if size < header_len || size > 128 * 1024 * 1024 {
            return Err(Error::Protocol(tf(
                TextKey::Mp4InvalidBoxSize,
                &[
                    ("size", &size.to_string()),
                    ("box_type", std::str::from_utf8(&box_type).unwrap_or("????")),
                ],
            )));
        }

        let mut data = vec![0u8; size];
        data[..8].copy_from_slice(&hdr);
        if header_len == 16 {
            data[8..16].copy_from_slice(&ext);
        }
        self.reader.read_exact(&mut data[header_len..]).await?;
        Ok(Some(Mp4Box { box_type, data }))
    }
}

async fn read_exact_or_eof<R: AsyncRead + Unpin>(reader: &mut R, buf: &mut [u8]) -> Result<usize> {
    let mut read_total = 0;
    while read_total < buf.len() {
        let n = reader.read(&mut buf[read_total..]).await?;
        if n == 0 {
            return Ok(read_total);
        }
        read_total += n;
    }
    Ok(read_total)
}

/// Output emitted by [`FragmentAssembler`] ready to forward to MSE clients.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MediaChunk {
    /// Complete initialisation segment (`ftyp` + `moov`).
    Init(Vec<u8>),
    /// Complete media fragment (`moof` + `mdat` + any preceding `styp`/`sidx`).
    Fragment(Vec<u8>),
}

/// Groups individual MP4 boxes (`ftyp`+`moov`, `moof`+`mdat`) into MSE-ready chunks.
#[derive(Debug, Default)]
pub struct FragmentAssembler {
    init_buf: Vec<u8>,
    saw_ftyp: bool,
    init_emitted: bool,
    frag_buf: Vec<u8>,
    saw_moof: bool,
}

impl FragmentAssembler {
    pub fn new() -> Self {
        Self::default()
    }

    /// Push the next top-level box and optionally yield a complete [`MediaChunk`].
    pub fn push(&mut self, bx: Mp4Box) -> Option<MediaChunk> {
        match &bx.box_type {
            b"ftyp" => {
                self.init_buf.clear();
                self.init_buf.extend_from_slice(&bx.data);
                self.saw_ftyp = true;
                self.init_emitted = false;
                None
            }
            b"moov" => {
                self.init_buf.extend_from_slice(&bx.data);
                self.init_emitted = true;
                Some(MediaChunk::Init(self.init_buf.clone()))
            }
            b"free" | b"skip" if !self.init_emitted && self.saw_ftyp => {
                self.init_buf.extend_from_slice(&bx.data);
                None
            }
            b"styp" | b"sidx" => {
                self.frag_buf.extend_from_slice(&bx.data);
                None
            }
            b"moof" => {
                self.frag_buf.clear();
                self.frag_buf.extend_from_slice(&bx.data);
                self.saw_moof = true;
                None
            }
            b"mdat" => {
                if self.saw_moof {
                    self.frag_buf.extend_from_slice(&bx.data);
                    self.saw_moof = false;
                    let out = std::mem::take(&mut self.frag_buf);
                    Some(MediaChunk::Fragment(out))
                } else {
                    Some(MediaChunk::Fragment(bx.data))
                }
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn make_box(typ: &[u8; 4], body: &[u8]) -> Vec<u8> {
        let size = (8 + body.len()) as u32;
        let mut out = Vec::with_capacity(size as usize);
        out.extend_from_slice(&size.to_be_bytes());
        out.extend_from_slice(typ);
        out.extend_from_slice(body);
        out
    }

    #[tokio::test]
    async fn init_segment_detected() {
        let mut stream = Vec::new();
        stream.extend_from_slice(&make_box(b"ftyp", b"isom\0\0\x02\0"));
        stream.extend_from_slice(&make_box(b"moov", b"mvhd_payload"));

        let mut reader = BoxReader::new(Cursor::new(stream));
        let mut asm = FragmentAssembler::new();

        let b1 = reader.next_box().await.unwrap().unwrap();
        assert_eq!(b1.kind(), BoxKind::Init);
        assert!(asm.push(b1).is_none());

        let b2 = reader.next_box().await.unwrap().unwrap();
        assert_eq!(b2.kind(), BoxKind::Init);
        let chunk = asm.push(b2).unwrap();
        match chunk {
            MediaChunk::Init(bytes) => {
                assert_eq!(&bytes[4..8], b"ftyp");
                assert!(bytes.windows(4).any(|w| w == b"moov"));
            }
            other => panic!("expected Init chunk, got {other:?}"),
        }
        assert!(reader.next_box().await.unwrap().is_none());
    }

    #[tokio::test]
    async fn fragment_detected() {
        let mut stream = Vec::new();
        stream.extend_from_slice(&make_box(b"moof", b"mfhd_traf"));
        stream.extend_from_slice(&make_box(b"mdat", b"h264_nal_bytes"));

        let mut reader = BoxReader::new(Cursor::new(stream));
        let mut asm = FragmentAssembler::new();

        let b1 = reader.next_box().await.unwrap().unwrap();
        assert_eq!(b1.kind(), BoxKind::Fragment);
        assert!(asm.push(b1).is_none());

        let b2 = reader.next_box().await.unwrap().unwrap();
        assert_eq!(b2.kind(), BoxKind::Fragment);
        match asm.push(b2).unwrap() {
            MediaChunk::Fragment(bytes) => {
                assert_eq!(&bytes[4..8], b"moof");
                assert!(bytes.windows(4).any(|w| w == b"mdat"));
            }
            other => panic!("expected Fragment chunk, got {other:?}"),
        }
    }
}
