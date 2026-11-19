//! Fixed JSON schemas for HTTP API endpoints (docs/ARCHITECTURE.md §10).

use ekran_core::{DeviceInfo, Error, Monitor, Stats, StreamPlan};
use serde::{Deserialize, Serialize};

/// `GET /api/info` payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiInfo {
    pub version: String,
    pub plan: StreamPlan,
    pub monitors: Vec<Monitor>,
    pub devices: Vec<DeviceInfo>,
    pub backend: String,
    pub started_at: String,
}

/// `GET /api/discover` payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiDiscover {
    pub devices: Vec<DeviceInfo>,
}

/// Standard JSON error payload (`{"error": ..., "message": ..., "hint": ...}`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiError {
    pub error: String,
    pub message: String,
    pub hint: String,
}

impl ApiError {
    pub fn from_core(err: &Error) -> Self {
        Self {
            error: err.code().into(),
            message: err.to_string(),
            hint: err.hint(),
        }
    }

    pub fn unauthorized() -> Self {
        Self {
            error: "Unauthorized".into(),
            message: "Geçersiz veya eksik erişim anahtarı (token)".into(),
            hint: "URL'ye `?token=<anahtar>` ekleyin veya sunucunun gösterdiği QR/bağlantıyı kullanın."
                .into(),
        }
    }
}

/// `GET /api/stats` payload alias matching [`Stats`].
pub type ApiStats = Stats;
