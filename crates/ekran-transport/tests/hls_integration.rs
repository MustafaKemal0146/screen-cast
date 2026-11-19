use ekran_core::StreamPlan;
use ekran_transport::{BoxReader, FragmentAssembler, HlsPipeline, MediaChunk, StreamHub};
use std::io::Cursor;
use std::sync::Arc;
use std::time::Duration;

#[tokio::test]
async fn it_hls_playlist_updates() {
    let hub = Arc::new(StreamHub::new(&StreamPlan::default()));
    let hls = HlsPipeline::start(hub.clone())
        .await
        .expect("hls pipeline starts");

    // Feed sample_frag.mp4 multiple times so ffmpeg HLS produces at least 1 segment + m3u8.
    let data = include_bytes!("fixtures/sample_frag.mp4");
    for round in 0..3 {
        let mut br = BoxReader::new(Cursor::new(&data[..]));
        let mut asm = FragmentAssembler::new();
        while let Ok(Some(bx)) = br.next_box().await {
            if let Some(chunk) = asm.push(bx) {
                match chunk {
                    MediaChunk::Init(b) => {
                        if round == 0 {
                            hub.set_init(b).await;
                        }
                    }
                    MediaChunk::Fragment(b) => hub.publish_fragment(b).await,
                }
            }
        }
    }

    let mut found = None;
    for _ in 0..25 {
        if let Some(m3u8) = hls.read_playlist("tok123") {
            if m3u8.contains(".ts?token=tok123") {
                found = Some(m3u8);
                break;
            }
        }
        tokio::time::sleep(Duration::from_millis(120)).await;
    }

    hls.stop();
    let playlist = found.expect("HLS playlist stream.m3u8 should be generated with .ts segments");
    assert!(playlist.contains("#EXTM3U"));
}
