//! `pactl subscribe` based watcher that detects default sink / monitor changes
//! and triggers audio stream rebinding within <= 3s (AC-3, PLAN T-M2-02).

use crate::source::{resolve_default_monitor_with_bin, AudioSource};
use screencast_core::{t, TextKey};
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;
use tracing::{debug, warn};

/// Events emitted by [`SourceWatcher`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AudioEvent {
    /// Default output sink changed and a new monitor source was resolved.
    SourceChanged(AudioSource),
    /// Audio subsystem or `pactl` is unavailable / lost.
    Lost(String),
}

/// Watches PulseAudio/PipeWire sink changes via `pactl subscribe`.
pub struct SourceWatcher {
    pactl_bin: String,
    debounce: Duration,
}

impl Default for SourceWatcher {
    fn default() -> Self {
        Self::new()
    }
}

impl SourceWatcher {
    pub fn new() -> Self {
        Self {
            pactl_bin: "pactl".into(),
            debounce: Duration::from_millis(150),
        }
    }

    /// Create a watcher using a custom `pactl` executable (for unit/fixture testing).
    pub fn with_bin(pactl_bin: impl Into<String>, debounce: Duration) -> Self {
        Self {
            pactl_bin: pactl_bin.into(),
            debounce,
        }
    }

    /// Spawn the background watcher task returning an event receiver and a shutdown handle.
    pub fn spawn(&self) -> (mpsc::Receiver<AudioEvent>, tokio::sync::watch::Sender<bool>) {
        let (tx, rx) = mpsc::channel(16);
        let (stop_tx, mut stop_rx) = tokio::sync::watch::channel(false);
        let pactl_bin = self.pactl_bin.clone();
        let debounce = self.debounce;

        tokio::spawn(async move {
            let mut current_device = resolve_default_monitor_with_bin(&pactl_bin)
                .ok()
                .map(|s| s.device);

            loop {
                if *stop_rx.borrow() {
                    break;
                }

                let mut cmd = Command::new(&pactl_bin);
                cmd.env("LC_ALL", "C")
                    .arg("subscribe")
                    .stdout(Stdio::piped())
                    .stderr(Stdio::null());

                let mut child = match cmd.spawn() {
                    Ok(c) => c,
                    Err(e) => {
                        warn!("failed to start audio watcher ({}): {}", pactl_bin, e);
                        let _ = tx
                            .send(AudioEvent::Lost(t(TextKey::WatcherPactlMissing).into()))
                            .await;
                        return;
                    }
                };

                let Some(stdout) = child.stdout.take() else {
                    let _ = child.kill().await;
                    let _ = tx
                        .send(AudioEvent::Lost(t(TextKey::WatcherStdoutFailed).into()))
                        .await;
                    return;
                };

                let mut lines = BufReader::new(stdout).lines();
                let mut should_stop = false;
                loop {
                    tokio::select! {
                        _ = stop_rx.changed() => {
                            if *stop_rx.borrow() {
                                should_stop = true;
                                break;
                            }
                        }
                        line_res = lines.next_line() => {
                            match line_res {
                                Ok(Some(line)) => {
                                    if !is_relevant_pactl_event(&line) {
                                        continue;
                                    }
                                    debug!("pactl event: {}", line);
                                    if !debounce.is_zero() {
                                        tokio::time::sleep(debounce).await;
                                    }
                                    match resolve_default_monitor_with_bin(&pactl_bin) {
                                        Ok(next) => {
                                            if current_device.as_deref() != Some(next.device.as_str()) {
                                                current_device = Some(next.device.clone());
                                                if tx.send(AudioEvent::SourceChanged(next)).await.is_err() {
                                                    should_stop = true;
                                                    break;
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            current_device = None;
                                            let _ = tx.send(AudioEvent::Lost(e.to_string())).await;
                                        }
                                    }
                                }
                                Ok(None) => {
                                    debug!("pactl subscribe stream ended; reconnecting");
                                    break;
                                }
                                Err(e) => {
                                    warn!("pactl subscribe read error: {}", e);
                                    break;
                                }
                            }
                        }
                    }
                }

                let _ = child.kill().await;
                let _ = child.wait().await;
                if should_stop || *stop_rx.borrow() {
                    break;
                }
                tokio::select! {
                    _ = stop_rx.changed() => {
                        if *stop_rx.borrow() { break; }
                    }
                    _ = tokio::time::sleep(Duration::from_millis(500)) => {}
                }
            }
        });

        (rx, stop_tx)
    }
}

/// Filter `pactl subscribe` lines for sink / server / source events that affect monitor routing.
pub fn is_relevant_pactl_event(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    (lower.contains("on sink") || lower.contains("on server") || lower.contains("on source"))
        && (lower.contains("'change'") || lower.contains("'new'") || lower.contains("'remove'"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn filters_relevant_pactl_lines() {
        assert!(is_relevant_pactl_event("Event 'change' on server #0"));
        assert!(is_relevant_pactl_event("Event 'new' on sink #52"));
        assert!(is_relevant_pactl_event("Event 'remove' on sink #10"));
        assert!(!is_relevant_pactl_event("Event 'change' on client #88"));
    }

    #[tokio::test]
    async fn sink_change_triggers_rebind() {
        let dir =
            std::env::temp_dir().join(format!("screencast_audio_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let state_file = dir.join("state");
        std::fs::write(&state_file, "initial").unwrap();

        let script = dir.join("fake_pactl.sh");
        let body = format!(
            r#"#!/usr/bin/env bash
STATE_FILE="{state}"
case "$1" in
  info)
    if [ "$(cat "$STATE_FILE")" = "switched" ]; then
      echo "Default Sink: bluez_output.BT_HEADSET"
    else
      echo "Default Sink: alsa_output.pci_analog"
    fi
    ;;
  list)
    echo "10 alsa_output.pci_analog.monitor PipeWire s32le RUNNING"
    echo "11 bluez_output.BT_HEADSET.monitor PipeWire s16le RUNNING"
    ;;
  subscribe)
    sleep 0.05
    echo "switched" > "$STATE_FILE"
    echo "Event 'change' on server #0"
    sleep 1
    ;;
esac
"#,
            state = state_file.display()
        );
        std::fs::write(&script, body).unwrap();
        let mut perms = std::fs::metadata(&script).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&script, perms).unwrap();

        let watcher = SourceWatcher::with_bin(
            script.to_string_lossy().to_string(),
            Duration::from_millis(10),
        );
        let (mut rx, stop_tx) = watcher.spawn();

        let evt = tokio::time::timeout(Duration::from_secs(2), rx.recv())
            .await
            .expect("watcher timeout")
            .expect("event expected");

        match evt {
            AudioEvent::SourceChanged(src) => {
                assert_eq!(src.device, "bluez_output.BT_HEADSET.monitor");
            }
            other => panic!("expected SourceChanged, got {other:?}"),
        }

        let _ = stop_tx.send(true);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn missing_pactl_does_not_panic() {
        let watcher = SourceWatcher::with_bin(
            "/nonexistent/screencast-fake-pactl-binary",
            Duration::from_millis(1),
        );
        let (mut rx, _stop) = watcher.spawn();
        let evt = tokio::time::timeout(Duration::from_secs(2), rx.recv())
            .await
            .expect("timeout")
            .expect("lost event");
        assert!(matches!(evt, AudioEvent::Lost(_)));
    }
}
