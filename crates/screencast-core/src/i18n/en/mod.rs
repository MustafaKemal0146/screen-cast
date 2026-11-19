//! English translation catalog (`en` — default language).

mod cli;
mod core;
mod sinks;

use crate::i18n::TextKey;

/// Look up the English string for a [`TextKey`].
pub fn lookup(key: TextKey) -> &'static str {
    if let Some(s) = core::lookup(key) {
        return s;
    }
    if let Some(s) = sinks::lookup(key) {
        return s;
    }
    cli::lookup(key)
}
