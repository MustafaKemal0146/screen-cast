//! screencast-audio: PulseAudio/PipeWire monitor source resolution and sink switch watcher.

pub mod source;
pub mod watcher;

pub use source::{
    create_loopback_null_sink, list_monitor_sources, resolve_default_monitor,
    resolve_default_monitor_with_bin, resolve_monitor_from_outputs, unload_loopback_module,
    AudioKind, AudioSource, LoopbackGuard,
};
pub use watcher::{is_relevant_pactl_event, AudioEvent, SourceWatcher};
