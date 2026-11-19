//! Adaptive bitrate ladder (T-M2-05) and idle-screen detection (T-M2-04).

use screencast_core::Quality;
use std::time::{Duration, Instant};

/// Computes a 64-bit FNV-1a signature over a downsampled 64x36 luma/RGB frame.
pub fn frame_signature(bytes: &[u8]) -> u64 {
    const FNV_OFFSET: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;
    let mut hash = FNV_OFFSET;
    for &b in bytes {
        hash ^= b as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

/// Detects static/idle screens (64x36 signature unchanged for `idle_seconds`, default 3s).
#[derive(Debug, Clone)]
pub struct IdleDetector {
    idle_after: Duration,
    last_sig: Option<u64>,
    unchanged_since: Option<Instant>,
    is_idle: bool,
}

impl IdleDetector {
    pub fn new(idle_seconds: u64) -> Self {
        Self {
            idle_after: Duration::from_secs(idle_seconds.max(1)),
            last_sig: None,
            unchanged_since: None,
            is_idle: false,
        }
    }

    /// Observe a new downsampled frame at `now`. Returns `Some(new_idle_state)` on transition.
    pub fn observe(&mut self, frame: &[u8], now: Instant) -> Option<bool> {
        let sig = frame_signature(frame);
        match self.last_sig {
            Some(prev) if prev == sig => {
                let since = *self.unchanged_since.get_or_insert(now);
                if !self.is_idle && now.duration_since(since) >= self.idle_after {
                    self.is_idle = true;
                    return Some(true);
                }
                None
            }
            _ => {
                self.last_sig = Some(sig);
                self.unchanged_since = Some(now);
                if self.is_idle {
                    self.is_idle = false;
                    return Some(false);
                }
                None
            }
        }
    }

    pub fn is_idle(&self) -> bool {
        self.is_idle
    }
}

/// Client throughput / latency sample used by [`AdaptiveController`].
#[derive(Debug, Clone, Copy)]
pub struct NetworkSample {
    pub measured_kbps: u32,
    pub latency_ms: u32,
}

/// Hysteresis-guarded adaptive quality ladder controller (`+20%` up / `-15%` down, T-M2-05).
#[derive(Debug, Clone)]
pub struct AdaptiveController {
    current_idx: usize,
    up_threshold: f64,
    down_threshold: f64,
    cooldown: Duration,
    last_change: Option<Instant>,
}

impl AdaptiveController {
    pub fn new(initial: Quality, up_threshold: f64, down_threshold: f64) -> Self {
        let idx = Quality::LADDER
            .iter()
            .position(|&q| q == initial)
            .unwrap_or(3);
        Self {
            current_idx: idx,
            up_threshold,
            down_threshold,
            cooldown: Duration::from_secs(4),
            last_change: None,
        }
    }

    pub fn current_quality(&self) -> Quality {
        Quality::LADDER[self.current_idx]
    }

    /// Feed a client measurement at `now`. Returns `Some(new_quality)` when quality changes.
    pub fn update(&mut self, sample: NetworkSample, now: Instant) -> Option<Quality> {
        if let Some(prev) = self.last_change {
            if now.duration_since(prev) < self.cooldown {
                return None;
            }
        }

        let current = Quality::LADDER[self.current_idx];
        let current_kbps = current.video_bitrate_kbps() as f64;
        let measured = sample.measured_kbps as f64;

        let down_limit = current_kbps * (1.0 - self.down_threshold);
        if (measured < down_limit || sample.latency_ms > 650) && self.current_idx > 0 {
            self.current_idx -= 1;
            self.last_change = Some(now);
            return Some(Quality::LADDER[self.current_idx]);
        }

        if self.current_idx + 1 < Quality::LADDER.len() && sample.latency_ms < 350 {
            let next_kbps = Quality::LADDER[self.current_idx + 1].video_bitrate_kbps() as f64;
            let up_limit = next_kbps * (1.0 + self.up_threshold);
            if measured >= up_limit {
                self.current_idx += 1;
                self.last_change = Some(now);
                return Some(Quality::LADDER[self.current_idx]);
            }
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hysteresis_no_flap() {
        let t0 = Instant::now();
        let mut ctrl = AdaptiveController::new(Quality::P720, 0.20, 0.15);
        assert_eq!(ctrl.current_quality(), Quality::P720);

        for kbps in [3500, 4500, 7500, 9000] {
            let changed = ctrl.update(
                NetworkSample {
                    measured_kbps: kbps,
                    latency_ms: 180,
                },
                t0 + Duration::from_secs(10),
            );
            assert!(changed.is_none(), "unexpected flap at {kbps} kbps");
        }

        let down = ctrl.update(
            NetworkSample {
                measured_kbps: 3000,
                latency_ms: 250,
            },
            t0 + Duration::from_secs(11),
        );
        assert_eq!(down, Some(Quality::P540));

        let suppressed = ctrl.update(
            NetworkSample {
                measured_kbps: 15000,
                latency_ms: 100,
            },
            t0 + Duration::from_secs(12),
        );
        assert!(suppressed.is_none());

        let up = ctrl.update(
            NetworkSample {
                measured_kbps: 6000,
                latency_ms: 100,
            },
            t0 + Duration::from_secs(16),
        );
        assert_eq!(up, Some(Quality::P720));
    }

    #[test]
    fn idle_detector_transitions() {
        let t0 = Instant::now();
        let mut det = IdleDetector::new(3);
        let frame_a = vec![42u8; 64 * 36];
        let frame_b = vec![99u8; 64 * 36];

        assert_eq!(det.observe(&frame_a, t0), None);
        assert_eq!(det.observe(&frame_a, t0 + Duration::from_secs(2)), None);
        assert_eq!(
            det.observe(&frame_a, t0 + Duration::from_secs(3)),
            Some(true)
        );
        assert!(det.is_idle());
        assert_eq!(
            det.observe(&frame_b, t0 + Duration::from_secs(4)),
            Some(false)
        );
        assert!(!det.is_idle());
    }
}
