//! Error definitions, Turkish user-facing messages, actionable hints, and retry policies.
//! Specification: docs/ARCHITECTURE.md §9 & §12.

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

/// Shared error type across all ekran crates. User-facing messages are in Turkish.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Grafik oturumu bulunamadı (DISPLAY/WAYLAND_DISPLAY boş)")]
    NoDisplay,

    #[error("Ekran paylaşım izni verilmedi ({0})")]
    PortalDenied(String),

    #[error("Kullanılabilir kodlayıcı yok (denenenler: {tried:?})")]
    EncoderUnavailable { tried: Vec<String> },

    #[error("Hedef cihaz bulunamadı. `ekran discover` ile ağı tara.")]
    NoSinkFound,

    #[error("Hedef cihaz bağlantıyı reddetti: {0}")]
    SinkRefused(String),

    #[error("Ses kaynağı kayboldu: {0}")]
    AudioDeviceLost(String),

    #[error("Geçersiz ayar: {field} - {hint}")]
    ConfigInvalid { field: String, hint: String },

    #[error("Protokol hatası: {0}")]
    Protocol(String),

    #[error("Ağ/GÇ hatası: {0}")]
    Io(#[from] std::io::Error),
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

    /// Actionable Turkish hint shown alongside the error message.
    pub fn hint(&self) -> String {
        match self {
            Error::NoDisplay => {
                "Bir X11 veya Wayland masaüstü oturumunda çalıştırın ya da `--backend test` kullanın."
                    .into()
            }
            Error::PortalDenied(_) => {
                "Portal penceresinde ekran paylaşımına izin verin veya Xorg oturumuna geçin.".into()
            }
            Error::EncoderUnavailable { tried } => format!(
                "Denenenler: {}. `ffmpeg` kurulumunu ve GPU sürücülerini (`ekran doctor`) kontrol edin.",
                tried.join(" -> ")
            ),
            Error::NoSinkFound => {
                "TV ve bilgisayarın aynı Wi-Fi/LAN ağında olduğunu doğrulayın; veya tarayıcı modunu kullanın."
                    .into()
            }
            Error::SinkRefused(_) => {
                "TV'nin Cast/DLNA desteğini ve güvenlik duvarı kurallarını kontrol edin.".into()
            }
            Error::AudioDeviceLost(_) => {
                "Ses kaynağı otomatik olarak yeniden bağlanıyor; gerekirse `--no-audio` ile başlatın."
                    .into()
            }
            Error::ConfigInvalid { hint, .. } => hint.clone(),
            Error::Protocol(_) => {
                "Hedef cihaz beklenmeyen yanıt döndürdü; tarayıcı moduna geçmeyi deneyin.".into()
            }
            Error::Io(_) => {
                "Ağ bağlantısını, port çakışmasını (`--port`) ve dosya izinlerini kontrol edin."
                    .into()
            }
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
    fn error_messages_and_hints_are_turkish() {
        let e = Error::NoSinkFound;
        assert_eq!(e.code(), "NoSinkFound");
        assert!(e.to_string().contains("bulunamadı"));
        assert!(e.hint().contains("Wi-Fi"));
        assert_eq!(e.retry_policy(), RetryPolicy::DelaySeconds(5));

        let e2 = Error::SinkRefused("8009".into());
        assert_eq!(e2.retry_policy(), RetryPolicy::Backoff { max_attempts: 3 });
        assert!(e2.hint().contains("Cast/DLNA"));
    }
}
