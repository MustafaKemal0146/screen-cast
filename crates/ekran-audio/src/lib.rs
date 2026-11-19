//! ekran-audio: PulseAudio/PipeWire monitor kaynağı çözümleme ve değişim izleyici.

pub mod source;
pub mod watcher;

pub use source::{
    create_loopback_null_sink, list_monitor_sources, resolve_default_monitor,
    resolve_default_monitor_with_bin, resolve_monitor_from_outputs, AudioKind, AudioSource,
};
pub use watcher::{is_relevant_pactl_event, AudioEvent, SourceWatcher};
