//! Turkish core, capture, encode, audio, and transport messages.

use crate::i18n::TextKey;

pub fn lookup(key: TextKey) -> Option<&'static str> {
    Some(match key {
        TextKey::ErrNoDisplay => "Grafik oturumu bulunamadı (DISPLAY/WAYLAND_DISPLAY boş)",
        TextKey::ErrNoDisplayHint => {
            "Bir X11 veya Wayland masaüstü oturumunda çalıştırın ya da `--backend test` kullanın."
        }
        TextKey::ErrPortalDenied => "Ekran paylaşım izni verilmedi ({detail})",
        TextKey::ErrPortalDeniedHint => {
            "Portal penceresinde ekran paylaşımına izin verin veya Xorg oturumuna geçin."
        }
        TextKey::ErrEncoderUnavailable => "Kullanılabilir kodlayıcı yok (denenenler: {tried})",
        TextKey::ErrEncoderUnavailableHint => {
            "Denenenler: {tried}. `ffmpeg` kurulumunu ve GPU sürücülerini (`screencast doctor`) kontrol edin."
        }
        TextKey::ErrNoSinkFound => {
            "Hedef cihaz bulunamadı. `screencast discover` ile ağı tara."
        }
        TextKey::ErrNoSinkFoundHint => {
            "TV ve bilgisayarın aynı Wi-Fi/LAN ağında olduğunu doğrulayın; veya tarayıcı modunu kullanın."
        }
        TextKey::ErrSinkRefused => "Hedef cihaz bağlantıyı reddetti: {detail}",
        TextKey::ErrSinkRefusedHint => {
            "TV'nin Cast/DLNA/Miracast desteğini ve güvenlik duvarı kurallarını kontrol edin."
        }
        TextKey::ErrAudioDeviceLost => "Ses kaynağı kayboldu: {detail}",
        TextKey::ErrAudioDeviceLostHint => {
            "Ses kaynağı otomatik olarak yeniden bağlanıyor; gerekirse `--no-audio` ile başlatın."
        }
        TextKey::ErrConfigInvalid => "Geçersiz ayar: {field} - {hint}",
        TextKey::ErrProtocol => "Protokol hatası: {detail}",
        TextKey::ErrProtocolHint => {
            "Hedef cihaz beklenmeyen yanıt döndürdü; tarayıcı moduna geçmeyi deneyin."
        }
        TextKey::ErrIo => "Ağ/GÇ hatası: {detail}",
        TextKey::ErrIoHint => {
            "Ağ bağlantısını, port çakışmasını (`--port`) ve dosya izinlerini kontrol edin."
        }
        TextKey::QualityHintInvalid => {
            "geçerli değerler: 480p, 540p, 720p, 1080p, 1440p, auto (verilen: {other})"
        }
        TextKey::LangHintInvalid => "geçerli diller: en, tr (verilen: {other})",
        TextKey::ConfigSchemaMismatchHint => {
            "desteklenen şema sürümü {supported}, dosyadaki {found}. `screencast config init --force` ile güncelleyin"
        }
        TextKey::ConfigFpsInvalidHint => "1 ile 120 arasında bir FPS değeri girin",
        TextKey::ConfigPortInvalidHint => "1-65535 arasında geçerli bir TCP portu girin",
        TextKey::ConfigCaptureBackendHint => {
            "geçerli değerler: auto, x11, wayland, test (verilen: {other})"
        }
        TextKey::ConfigControlBackendHint => {
            "geçerli değerler: auto, xdotool, portal, none (verilen: {other})"
        }
        TextKey::ConfigTomlParseHint => "TOML sözdizimi hatası: {err}",
        TextKey::ConfigTomlSerializeHint => "TOML serileştirme hatası: {err}",
        TextKey::CaptureStdoutFailed => "Yakalama sürecinin standart çıktısı açılamadı",

        TextKey::SinkBrowser => "Tarayıcı (MSE)",
        TextKey::SinkCast => "Google Cast",
        TextKey::SinkDlna => "DLNA / UPnP",
        TextKey::SinkMiracast => "Miracast",
        TextKey::SinkAirplay => "AirPlay",
        TextKey::BackendX11 => "X11",
        TextKey::BackendWayland => "Wayland (XDG portal)",
        TextKey::BackendTest => "Test (lavfi)",

        TextKey::CodecHintInvalid => {
            "geçerli kodlayıcılar: h264_nvenc, h264_vaapi, libx264 (verilen: {other})"
        }
        TextKey::FfmpegNotFound => "ffmpeg bulunamadı",
        TextKey::FfmpegEncodersFailed => "ffmpeg -encoders hata verdi",
        TextKey::PactlNotFound => "pactl komutu bulunamadı",
        TextKey::PactlListFailed => "pactl ses kaynaklarını listeleyemedi",
        TextKey::NoMonitorSource => "Kullanılabilir PulseAudio/PipeWire monitor kaynağı yok",
        TextKey::PactlLoopbackFailed => "pactl loopback yüklenemedi: {err}",
        TextKey::WatcherPactlMissing => "pactl bulunamadı; ses değişimi izlenmeyecek",
        TextKey::WatcherStdoutFailed => "pactl subscribe çıktısı açılamadı",

        TextKey::WaylandX11Unsupported => {
            "Wayland oturumunda x11grab ekranı göstermez (yalnız XWayland/siyah). `--backend wayland` kullanın."
        }
        TextKey::WaylandNotDetected => {
            "Wayland oturumu algılanmadı; X11 için `--backend x11` kullanın."
        }
        TextKey::WaylandCodecInvalid => {
            "geçerli kodlayıcılar: h264_nvenc, h264_vaapi, libx264, vah264enc, x264enc (verilen: {other})"
        }
        TextKey::PipewireSrcMissing => {
            "GStreamer `pipewiresrc` eklentisi bulunamadı (gstreamer1.0-pipewire kurun)."
        }
        TextKey::PortalDbusConnect => "Oturum D-Bus bağlantısı kurulamadı: {err}",
        TextKey::PortalScreenCastMissing => {
            "ScreenCast portalı bulunamadı: {err}. `xdg-desktop-portal` kurulu olmalı."
        }
        TextKey::PortalMissingSessionHandle => "Portal session_handle döndürmedi",
        TextKey::PortalInvalidSessionHandle => "Geçersiz session_handle: {err}",
        TextKey::PortalMissingStreams => "Portal yayın akışı döndürmedi",
        TextKey::PortalStreamsDecode => "Portal streams alanı çözülemedi: {err}",
        TextKey::PortalEmptyStreams => "Seçilen ekran akışı boş",
        TextKey::PortalPipewireFd => "PipeWire fd alınamadı: {err}",
        TextKey::DbusUniqueName => "D-Bus benzersiz adı alınamadı",
        TextKey::DbusSignalListen => "D-Bus sinyal dinleme hatası: {err}",
        TextKey::PortalCallFailed => {
            "xdg-desktop-portal çağrısı başarısız: {err}. Portal servisi çalışıyor mu?"
        }
        TextKey::PortalTimeout => "Portal izin penceresi zaman aşımına uğradı",
        TextKey::PortalStreamClosed => "Portal yanıt akışı kapandı",
        TextKey::PortalReplyDecode => "Portal yanıtı çözülemedi: {err}",
        TextKey::PortalCancelled => "Kullanıcı {stage} aşamasında ekran paylaşımını reddetti",
        TextKey::PortalStageFailed => {
            "Portal {stage} başarısız oldu (kod {code}). GNOME/xdg-desktop-portal sürümünü kontrol edin"
        }
        TextKey::TestSourceNote => "Sanal test ekranı (ekran gerektirmez)",
        TextKey::WaylandSourceNote => "Wayland: XDG ScreenCast portalı + PipeWire + GStreamer",
        TextKey::XdotoolSpawnFailed => "xdotool çalıştırılamadı: {err}",
        TextKey::XdotoolFailed => "xdotool girdi komutunu uygulayamadı",
        TextKey::RemoteDesktopMissing => "RemoteDesktop portalı bulunamadı: {err}",
        TextKey::RemoteDesktopSessionHandle => "RemoteDesktop session_handle döndürmedi",
        TextKey::RemoteDesktopPointer => "RemoteDesktop fare hareketi hatası: {err}",
        TextKey::RemoteDesktopClick => "RemoteDesktop tık hatası: {err}",
        TextKey::RemoteDesktopScroll => "RemoteDesktop kaydırma hatası: {err}",
        TextKey::RemoteDesktopKey => "RemoteDesktop tuş hatası: {err}",

        TextKey::UnauthorizedMsg => "Geçersiz veya eksik erişim anahtarı (token)",
        TextKey::UnauthorizedHint => {
            "URL'ye `?token=<anahtar>` ekleyin veya sunucunun gösterdiği QR/bağlantıyı kullanın."
        }
        TextKey::HlsStdinFailed => "HLS stdin açılamadı",
        TextKey::HlsFfmpegStartFailed => "HLS ffmpeg başlatılamadı: {err}",
        TextKey::HlsNotReady => "HLS henüz hazır değil",
        TextKey::DmrModelName => "screencast Linux yansıtıcı",
        TextKey::Mp4IncompleteBoxHeader => "Eksik fMP4 kutu başlığı",
        TextKey::Mp4InvalidBoxSize64 => "Geçersiz 64-bit fMP4 kutu boyutu: {size}",
        TextKey::Mp4InvalidBoxSize => "Geçersiz fMP4 kutu boyutu {size} ({box_type})",
        TextKey::WsFrameTooLarge => "WebSocket çerçevesi çok büyük",
        TextKey::WsFrameLimitExceeded => "WebSocket çerçevesi sınırını aştı",
        TextKey::WsUnknownOpcode => "Bilinmeyen WebSocket opcode: {opcode}",
        TextKey::WsInvalidUtf8 => "Geçersiz UTF-8 WebSocket metni",
        _ => return None,
    })
}
