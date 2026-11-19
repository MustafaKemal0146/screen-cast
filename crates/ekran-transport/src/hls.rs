//! HLS live playlist packager (`ffmpeg -f mp4 -i pipe:0 -c copy -f hls`, T-M4-03 / T-M5-02).

use crate::hub::StreamHub;
use ekran_core::{Error, Result};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tokio::sync::watch;
use tracing::{debug, warn};

/// Manages an ephemeral directory containing `stream.m3u8` and `seg*.ts` segments
/// generated from the live fMP4 stream without re-encoding (`-c copy`).
pub struct HlsPipeline {
    dir: PathBuf,
    stop_tx: watch::Sender<bool>,
}

impl HlsPipeline {
    /// Start background HLS packaging fed by `hub`.
    pub async fn start(hub: Arc<StreamHub>) -> Result<Self> {
        let dir = std::env::temp_dir().join(format!(
            "ekran_hls_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir)?;

        let playlist_path = dir.join("stream.m3u8");
        let seg_pattern = dir.join("seg%05d.ts");

        let mut child = Command::new("ffmpeg")
            .args([
                "-hide_banner",
                "-loglevel",
                "warning",
                "-f",
                "mp4",
                "-i",
                "pipe:0",
                "-c",
                "copy",
                "-f",
                "hls",
                "-hls_time",
                "1",
                "-hls_list_size",
                "4",
                "-hls_flags",
                "delete_segments+append_list+omit_endlist",
                "-hls_segment_filename",
            ])
            .arg(&seg_pattern)
            .arg(&playlist_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| Error::Protocol(format!("HLS ffmpeg başlatılamadı: {e}")))?;

        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| Error::Protocol("HLS stdin açılamadı".into()))?;

        let (stop_tx, mut stop_rx) = watch::channel(false);
        let (init_opt, mut raw_rx) = hub.subscribe_raw().await;

        tokio::spawn(async move {
            let mut init_sent = false;
            if let Some(init) = init_opt {
                if stdin.write_all(&init).await.is_ok() {
                    init_sent = true;
                }
            }

            loop {
                tokio::select! {
                    _ = stop_rx.changed() => {
                        if *stop_rx.borrow() {
                            break;
                        }
                    }
                    msg = raw_rx.recv() => {
                        match msg {
                            Ok(chunk) => {
                                if !init_sent && chunk.len() >= 8 && &chunk[4..8] == b"ftyp" {
                                    init_sent = true;
                                }
                                if stdin.write_all(&chunk).await.is_err() {
                                    debug!("HLS ffmpeg stdin kapandı");
                                    break;
                                }
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                                warn!("HLS besleyici {n} parça atladı");
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                        }
                    }
                }
            }

            drop(stdin);
            let _ = child.kill().await;
            let _ = child.wait().await;
        });

        Ok(Self { dir, stop_tx })
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Read `stream.m3u8` and optionally rewrite segment lines to include `?token=<tok>`.
    pub fn read_playlist(&self, token: &str) -> Option<String> {
        let raw = std::fs::read_to_string(self.dir.join("stream.m3u8")).ok()?;
        if token.is_empty() {
            return Some(raw);
        }
        let mut out = String::with_capacity(raw.len() + 64);
        for line in raw.lines() {
            let trimmed = line.trim();
            if !trimmed.is_empty() && !trimmed.starts_with('#') && !trimmed.contains('?') {
                out.push_str(trimmed);
                out.push_str("?token=");
                out.push_str(token);
            } else {
                out.push_str(line);
            }
            out.push('\n');
        }
        Some(out)
    }

    /// Safely read a `.ts` segment file from the HLS directory (guards against traversal).
    pub fn read_segment(&self, file_name: &str) -> Option<Vec<u8>> {
        if file_name.contains('/') || file_name.contains('\\') || file_name.contains("..") {
            return None;
        }
        if !file_name.ends_with(".ts") {
            return None;
        }
        std::fs::read(self.dir.join(file_name)).ok()
    }

    pub fn stop(&self) {
        let _ = self.stop_tx.send(true);
    }
}

impl Drop for HlsPipeline {
    fn drop(&mut self) {
        let _ = self.stop_tx.send(true);
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}
