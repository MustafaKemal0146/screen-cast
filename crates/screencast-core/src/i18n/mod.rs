//! Zero-dependency English-first i18n subsystem (`en` default + `tr` catalog).

mod en;
pub mod keys;
pub mod tr;

pub use keys::TextKey;

use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU8, Ordering};

/// Supported UI / CLI / diagnostic languages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    #[default]
    En,
    Tr,
}

impl Language {
    /// Two-letter ISO 639-1 code (`"en"` or `"tr"`).
    pub fn code(self) -> &'static str {
        match self {
            Language::En => "en",
            Language::Tr => "tr",
        }
    }

    /// Strict parser for `--lang` CLI flag and `config.toml`.
    pub fn parse(s: &str) -> Result<Self> {
        let norm = s.trim().to_ascii_lowercase();
        match norm.as_str() {
            "en" | "english" => Ok(Language::En),
            "tr" | "turkish" => Ok(Language::Tr),
            other => Err(Error::ConfigInvalid {
                field: "language".into(),
                hint: fmt(
                    current_language(),
                    TextKey::LangHintInvalid,
                    &[("other", other)],
                ),
            }),
        }
    }

    /// Lenient locale string parser for `SCREENCAST_LANG` and `LANG` (e.g. `tr_TR.UTF-8`).
    pub fn from_env_str(s: &str) -> Option<Self> {
        let norm = s.trim().to_ascii_lowercase();
        if norm.is_empty() {
            return None;
        }
        let base = norm.split(['_', '-', '.']).next().unwrap_or(&norm);
        match base {
            "en" | "english" | "c" | "posix" => Some(Language::En),
            "tr" | "turkish" => Some(Language::Tr),
            _ => None,
        }
    }
}

static ACTIVE_LANG: AtomicU8 = AtomicU8::new(0);

/// Return the active process language (defaults to [`Language::En`]).
pub fn current_language() -> Language {
    match ACTIVE_LANG.load(Ordering::Relaxed) {
        1 => Language::Tr,
        _ => Language::En,
    }
}

/// Set the active process language (called once at CLI startup).
pub fn set_current_language(lang: Language) {
    let val = match lang {
        Language::En => 0,
        Language::Tr => 1,
    };
    ACTIVE_LANG.store(val, Ordering::Relaxed);
}

/// Resolve language from: `--lang` > `SCREENCAST_LANG` > explicit `config.toml` > `LANG` > `En`.
pub fn resolve_language(cli_lang: Option<Language>, config_lang: Option<Language>) -> Language {
    let env_sc = std::env::var("SCREENCAST_LANG").ok();
    let env_lang = std::env::var("LANG").ok();
    resolve_language_inputs(
        cli_lang,
        env_sc.as_deref(),
        config_lang,
        env_lang.as_deref(),
    )
}

/// Pure, testable language resolution helper.
pub fn resolve_language_inputs(
    cli_lang: Option<Language>,
    screencast_lang_env: Option<&str>,
    config_lang: Option<Language>,
    lang_env: Option<&str>,
) -> Language {
    if let Some(l) = cli_lang {
        return l;
    }
    if let Some(l) = screencast_lang_env.and_then(Language::from_env_str) {
        return l;
    }
    if let Some(l) = config_lang {
        return l;
    }
    if let Some(l) = lang_env.and_then(Language::from_env_str) {
        return l;
    }
    Language::En
}

/// Look up a static template in the specified language.
pub fn text(lang: Language, key: TextKey) -> &'static str {
    match lang {
        Language::En => en::lookup(key),
        Language::Tr => tr::lookup(key),
    }
}

/// Look up a static template in [`current_language()`].
pub fn t(key: TextKey) -> &'static str {
    text(current_language(), key)
}

/// Format a template in the specified language replacing `{name}` placeholders.
pub fn fmt(lang: Language, key: TextKey, args: &[(&str, &str)]) -> String {
    let mut out = text(lang, key).to_string();
    for &(name, val) in args {
        let pat = format!("{{{name}}}");
        out = out.replace(&pat, val);
    }
    out
}

/// Format a template in [`current_language()`].
pub fn tf(key: TextKey, args: &[(&str, &str)]) -> String {
    fmt(current_language(), key, args)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_to_english_and_respects_precedence() {
        assert_eq!(
            resolve_language_inputs(None, None, None, None),
            Language::En
        );
        assert_eq!(
            resolve_language_inputs(None, None, None, Some("tr_TR.UTF-8")),
            Language::Tr
        );
        assert_eq!(
            resolve_language_inputs(None, None, Some(Language::En), Some("tr_TR.UTF-8")),
            Language::En
        );
        assert_eq!(
            resolve_language_inputs(None, Some("tr"), Some(Language::En), Some("en_US.UTF-8")),
            Language::Tr
        );
        assert_eq!(
            resolve_language_inputs(
                Some(Language::En),
                Some("tr"),
                Some(Language::Tr),
                Some("tr_TR.UTF-8")
            ),
            Language::En
        );
    }

    #[test]
    fn language_parse_and_formatting_work() {
        assert_eq!(Language::parse("en").unwrap(), Language::En);
        assert_eq!(Language::parse("TR").unwrap(), Language::Tr);
        assert!(Language::parse("fr").is_err());

        let en_msg = fmt(
            Language::En,
            TextKey::PortalCancelled,
            &[("stage", "Start")],
        );
        assert!(en_msg.contains("denied"));
        let tr_msg = fmt(
            Language::Tr,
            TextKey::PortalCancelled,
            &[("stage", "Start")],
        );
        assert!(tr_msg.contains("reddetti"));
    }
}
