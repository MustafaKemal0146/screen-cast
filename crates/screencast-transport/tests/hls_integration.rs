use screencast_core::StreamPlan;
use screencast_transport::{BoxReader, FragmentAssembler, HlsPipeline, MediaChunk, StreamHub};
use std::io::Cursor;
use std::sync::Arc;
use std::time::Duration;

#[tokio::test]
async fn it_hls_playlist_updates() {
    let hub = Arc::new(StreamHub::new(&StreamPlan::default()));
    let hls = HlsPipeline::start(hub.clone())
        .await
        .expect("hls pipeline starts");

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

#[tokio::test]
async fn it_hls_survives_capture_restart() {
    let hub = Arc::new(StreamHub::new(&StreamPlan::default()));
    let hls = HlsPipeline::start(hub.clone())
        .await
        .expect("hls pipeline starts");

    let data = include_bytes!("fixtures/sample_frag.mp4");
    for _restart in 0..2 {
        for round in 0..3 {
            let mut br = BoxReader::new(Cursor::new(&data[..]));
            let mut asm = FragmentAssembler::new();
            while let Ok(Some(bx)) = br.next_box().await {
                if let Some(chunk) = asm.push(bx) {
                    match chunk {
                        MediaChunk::Init(b) if round == 0 => hub.set_init(b).await,
                        MediaChunk::Init(_) => {}
                        MediaChunk::Fragment(b) => hub.publish_fragment(b).await,
                    }
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }

    let mut found = None;
    for _ in 0..30 {
        if let Some(m3u8) = hls.read_playlist("tok123") {
            if m3u8.contains("seg01_") && m3u8.contains(".ts?token=tok123") {
                found = Some(m3u8);
                break;
            }
        }
        tokio::time::sleep(Duration::from_millis(120)).await;
    }

    let files: Vec<String> = std::fs::read_dir(hls.dir())
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    let cur_m3u8 = hls.read_playlist("tok123");
    hls.stop();
    let playlist = found.unwrap_or_else(|| {
        panic!("HLS playlist missing seg01_. files={files:?} m3u8={cur_m3u8:?}")
    });
    assert!(playlist.contains("#EXTM3U"));
}
