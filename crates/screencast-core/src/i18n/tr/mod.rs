//! Dedicated Turkish (`tr`) translation catalog.

mod cli;
mod core;
mod sinks;

use crate::i18n::TextKey;

/// Localized `pactl info` header prefix emitted on Turkish OS locales.
pub const PACTL_DEFAULT_SINK_PREFIX: &str = "Varsayılan Alıcı:";

/// Look up the Turkish string for a [`TextKey`].
pub fn lookup(key: TextKey) -> &'static str {
    if let Some(s) = core::lookup(key) {
        return s;
    }
    if let Some(s) = sinks::lookup(key) {
        return s;
    }
    cli::lookup(key)
}
