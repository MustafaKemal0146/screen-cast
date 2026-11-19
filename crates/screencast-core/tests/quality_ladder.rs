use screencast_core::Quality;
use serde::Deserialize;

#[derive(Deserialize)]
struct LadderRow {
    label: String,
    width: u16,
    height: u16,
    fps: u32,
    video_bitrate: String,
    gop_seconds: f64,
    audio_bitrate: String,
}

#[test]
fn quality_matches_plan_table() {
    let raw = include_str!("fixtures/quality_ladder.json");
    let rows: Vec<LadderRow> = serde_json::from_str(raw).expect("valid fixture json");
    assert_eq!(rows.len(), 5);
    for row in rows {
        let q = Quality::parse(&row.label).expect("known quality");
        assert_eq!(q.width(), row.width, "width mismatch for {}", row.label);
        assert_eq!(q.height(), row.height, "height mismatch for {}", row.label);
        assert_eq!(q.video_bitrate(), row.video_bitrate);
        assert!((q.gop_seconds() - row.gop_seconds).abs() < 1e-6);
        assert_eq!(q.audio_bitrate(), row.audio_bitrate);
        assert!(q.gop_frames(row.fps) >= 15);
    }
}
