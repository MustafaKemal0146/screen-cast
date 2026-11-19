//! Error definitions, localized user-facing messages, actionable hints, and retry policies.
//! Specification: docs/ARCHITECTURE.md §9 & §12.

use crate::i18n::{current_language, fmt, text, Language, TextKey};
use std::fmt as stdfmt;

/// Retry strategy associated with an [`Error`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryPolicy {
    /// Do not automatically retry.
    None,
    /// Retry when the user triggers the action again.
    OnUserRetry,
    /// Retry after a fixed delay (e.g. device discovery).
    DelaySeconds(u64),
    /// Exponential backoff (e.g. 1s, 2s, 4s) up to N attempts.
    Backoff { max_attempts: u8 },
    /// Continuously watched / healed in background (e.g. audio monitor).
    Continuous,
}

/// Shared error type across all `screencast` crates.
/// User-facing messages follow the active [`Language`] (`en` by default, `tr` supported).
#[derive(Debug)]
pub enum Error {
    NoDisplay,
    PortalDenied(String),
    EncoderUnavailable { tried: Vec<String> },
    NoSinkFound,
    SinkRefused(String),
    AudioDeviceLost(String),
    ConfigInvalid { field: String, hint: String },
    Protocol(String),
    Io(std::io::Error),
}

impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Self {
        Error::Io(err)
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl stdfmt::Display for Error {
    fn fmt(&self, f: &mut stdfmt::Formatter<'_>) -> stdfmt::Result {
        f.write_str(&self.message(current_language()))
    }
}

impl Error {
    /// Machine-readable error variant code for JSON API responses.
    pub fn code(&self) -> &'static str {
        match self {
            Error::NoDisplay => "NoDisplay",
            Error::PortalDenied(_) => "PortalDenied",
            Error::EncoderUnavailable { .. } => "EncoderUnavailable",
            Error::NoSinkFound => "NoSinkFound",
            Error::SinkRefused(_) => "SinkRefused",
            Error::AudioDeviceLost(_) => "AudioDeviceLost",
            Error::ConfigInvalid { .. } => "ConfigInvalid",
            Error::Protocol(_) => "Protocol",
            Error::Io(_) => "Io",
        }
    }

    /// Render the localized user-facing error message for `lang`.
    pub fn message(&self, lang: Language) -> String {
        match self {
            Error::NoDisplay => text(lang, TextKey::ErrNoDisplay).to_string(),
            Error::PortalDenied(detail) => {
                fmt(lang, TextKey::ErrPortalDenied, &[("detail", detail)])
            }
            Error::EncoderUnavailable { tried } => {
                let tried_str = format!("{tried:?}");
                fmt(
                    lang,
                    TextKey::ErrEncoderUnavailable,
                    &[("tried", &tried_str)],
                )
            }
            Error::NoSinkFound => text(lang, TextKey::ErrNoSinkFound).to_string(),
            Error::SinkRefused(detail) => fmt(lang, TextKey::ErrSinkRefused, &[("detail", detail)]),
            Error::AudioDeviceLost(detail) => {
                fmt(lang, TextKey::ErrAudioDeviceLost, &[("detail", detail)])
            }
            Error::ConfigInvalid { field, hint } => fmt(
                lang,
                TextKey::ErrConfigInvalid,
                &[("field", field), ("hint", hint)],
            ),
            Error::Protocol(detail) => fmt(lang, TextKey::ErrProtocol, &[("detail", detail)]),
            Error::Io(err) => {
                let detail = err.to_string();
                fmt(lang, TextKey::ErrIo, &[("detail", &detail)])
            }
        }
    }

    /// Actionable hint shown alongside the error message in the active language.
    pub fn hint(&self) -> String {
        self.hint_for(current_language())
    }

    /// Actionable hint shown alongside the error message in `lang`.
    pub fn hint_for(&self, lang: Language) -> String {
        match self {
            Error::NoDisplay => text(lang, TextKey::ErrNoDisplayHint).into(),
            Error::PortalDenied(_) => text(lang, TextKey::ErrPortalDeniedHint).into(),
            Error::EncoderUnavailable { tried } => fmt(
                lang,
                TextKey::ErrEncoderUnavailableHint,
                &[("tried", &tried.join(" -> "))],
            ),
            Error::NoSinkFound => text(lang, TextKey::ErrNoSinkFoundHint).into(),
            Error::SinkRefused(_) => text(lang, TextKey::ErrSinkRefusedHint).into(),
            Error::AudioDeviceLost(_) => text(lang, TextKey::ErrAudioDeviceLostHint).into(),
            Error::ConfigInvalid { hint, .. } => hint.clone(),
            Error::Protocol(_) => text(lang, TextKey::ErrProtocolHint).into(),
            Error::Io(_) => text(lang, TextKey::ErrIoHint).into(),
        }
    }

    /// Retry policy recommended by docs/ARCHITECTURE.md §12.
    pub fn retry_policy(&self) -> RetryPolicy {
        match self {
            Error::NoDisplay | Error::EncoderUnavailable { .. } | Error::ConfigInvalid { .. } => {
                RetryPolicy::None
            }
            Error::PortalDenied(_) => RetryPolicy::OnUserRetry,
            Error::NoSinkFound => RetryPolicy::DelaySeconds(5),
            Error::SinkRefused(_) => RetryPolicy::Backoff { max_attempts: 3 },
            Error::AudioDeviceLost(_) => RetryPolicy::Continuous,
            Error::Protocol(_) | Error::Io(_) => RetryPolicy::None,
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_messages_and_hints_are_english_first_with_turkish_support() {
        let e = Error::NoSinkFound;
        assert_eq!(e.code(), "NoSinkFound");
        assert!(e.to_string().contains("No target device found"));
        assert!(e.hint().contains("Wi-Fi/LAN"));
        assert!(e.message(Language::Tr).contains("Hedef cihaz"));
        assert!(e.hint_for(Language::Tr).contains("Wi-Fi/LAN"));
        assert_eq!(e.retry_policy(), RetryPolicy::DelaySeconds(5));

        let e2 = Error::SinkRefused("8009".into());
        assert_eq!(e2.retry_policy(), RetryPolicy::Backoff { max_attempts: 3 });
        assert!(e2.hint().contains("Cast/DLNA"));
    }
}
