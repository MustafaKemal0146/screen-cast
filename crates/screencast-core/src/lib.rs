//! screencast-core: shared types, i18n, configuration, error model, and trait contracts.
//! Details: docs/ARCHITECTURE.md §1 · Quality values: docs/PLAN.md §11

pub mod config;
pub mod error;
pub mod i18n;
pub mod quality;
pub mod traits;
pub mod types;

pub use config::{default_config_path, explicit_language_in_toml, Config, SCHEMA_VERSION};
pub use error::{Error, Result, RetryPolicy};
pub use i18n::{
    current_language, fmt, resolve_language, set_current_language, t, text, tf, Language, TextKey,
};
pub use quality::Quality;
pub use traits::{
    CaptureDescriptor, CaptureProcess, CaptureSource, Control, ControlEvent, Discovery,
    MouseButton, PreparedSink, Sink,
};
pub use types::{Backend, Capabilities, DeviceInfo, Monitor, SinkKind, Stats, StreamPlan};
