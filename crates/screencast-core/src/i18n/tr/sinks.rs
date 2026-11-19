//! Turkish sink protocol and wireless diagnostics messages.

use crate::i18n::TextKey;

pub fn lookup(key: TextKey) -> Option<&'static str> {
    Some(match key {
        TextKey::BrowserDeviceName => "Tarayıcı modu ({url})",
        TextKey::BrowserDeviceNote => {
            "TV veya telefon tarayıcısından açın; kumanda etkin"
        }
        TextKey::BrowserPreparedNote => {
            "Tarayıcıda açın (düşük gecikmeli MSE + uzaktan kumanda)"
        }
        TextKey::CastInvalidFrameSize => "Geçersiz Cast çerçeve boyutu: {len}",
        TextKey::CastProtobufDecode => "Cast protobuf çözülemedi: {err}",
        TextKey::CastSessionClosed => "Cast oturumu kapandı",
        TextKey::CastConnectTimeout => "Cast bağlantısı zaman aşımı: {addr}",
        TextKey::CastTlsHandshakeFailed => "TLS el sıkışma hatası: {err}",
        TextKey::CastPongTimeout => "Cast PONG yanıtı zaman aşımı",
        TextKey::CastUnexpectedHeartbeat => "Beklenmeyen heartbeat yanıtı: {txt}",
        TextKey::CastSessionNotReady => "Cast oturumu hazır değil",
        TextKey::CastLoadRejected => "Cast medya yüklemesi reddedildi: {txt}",
        TextKey::CastReceiverClosedAfterLoad => "Cast alıcı LOAD sonrası bağlantıyı kapattı",
        TextKey::CastPreparedNote => "Google Cast HLS oynatımı (kumanda TV'de kalır)",

        TextKey::DlnaLiveTitle => "screencast canlı",
        TextKey::DlnaTvUnreachable => {
            "TV'ye ulaşılamıyor ({addr}). TV'nin açık ve aynı ağda olduğundan emin olun (bekleme modundaki TV ağı kapatabilir)"
        }
        TextKey::DlnaSoapRejected => "DLNA {action} ({path}) reddedildi ({detail})",
        TextKey::DlnaPreparedNote => "DLNA HLS -> progressive MP4 yedekli",
        TextKey::DlnaUnsupportedLive => {
            "bu TV canlı DLNA desteklemiyor, tarayıcı/Cast önerilir ({err})"
        }
        TextKey::MdnsMiceNote => {
            "MS-MICE (yerel ağ/Ethernet üzerinden Miracast — P2P kapalı)"
        }
        TextKey::MdnsAirplayNote => "bu TV için tarayıcı modu gerekli (Dalga 5)",

        TextKey::MiceFrameTooShort => "MS-MICE çerçevesi 4 bayttan kısa",
        TextKey::MiceInvalidLength => "Geçersiz MS-MICE uzunluğu: {declared} (tampon: {buf_len})",
        TextKey::MiceTruncatedTlvHeader => "Eksik MS-MICE TLV başlığı",
        TextKey::MiceTlvOutOfBounds => "MS-MICE TLV uzunluğu sınırı aşıyor",
        TextKey::MiceInvalidFrameSize => "Geçersiz MS-MICE çerçeve boyutu: {total}",
        TextKey::MiceConnectTimeout => "MS-MICE bağlantısı zaman aşımı: {addr}",
        TextKey::MicePortUnreachable => "MS-MICE portuna ({addr}) ulaşılamıyor: {err}",
        TextKey::MicePinUnsupported => {
            "TV MS-MICE için PIN doğrulaması istedi; henüz desteklenmiyor (tarayıcı/Cast deneyin)"
        }
        TextKey::MiceDtlsUnsupported => {
            "TV MS-MICE için DTLS şifreli el sıkışma zorunlu tutuyor; henüz desteklenmiyor (tarayıcı/Cast deneyin)"
        }
        TextKey::MiceRejectedStop => "TV MS-MICE oturumunu reddetti (STOP_PROJECTION)",

        TextKey::MiracastMiceNote => {
            "MS-MICE (yerel ağ/Ethernet üzerinden Miracast — P2P radyosu kapalı)"
        }
        TextKey::MiracastP2pTaskFailed => "P2P görevi başarısız: {err}",
        TextKey::MiracastNmP2pNote => {
            "Wi-Fi Direct P2P (NetworkManager WifiP2P, arayüz {iface})"
        }
        TextKey::MiracastWpaP2pNote => {
            "Wi-Fi Direct P2P (go_intent={intent}, Kanal {ch}, sanal {iface})"
        }
        TextKey::MiracastDirectNote => "Wi-Fi Display (WFD RTSP)",
        TextKey::MiracastSessionNotReady => "Miracast oturumu hazır değil",
        TextKey::AirplayWave5Refused => {
            "{name} ({kind}) için doğrudan protokol Dalga 5 kapsamındadır; bu TV için tarayıcı modu (`--to browser`) gerekli"
        }

        TextKey::WfdRtspClosed => "RTSP bağlantısı kapandı",
        TextKey::WfdRtspConnectTimeout => {
            "TV WFD RTSP portuna ({port}) zamanında bağlanmadı"
        }
        TextKey::WfdM1Timeout => "WFD M1 zaman aşımı",
        TextKey::WfdM1ExpectedOptions => "WFD M1 OPTIONS bekleniyordu, gelen: {line}",
        TextKey::WfdM2Timeout => "WFD M2 zaman aşımı",
        TextKey::WfdM2Failed => "WFD M2 başarısız: {line}",
        TextKey::WfdM3Timeout => "WFD M3 zaman aşımı",
        TextKey::WfdM4Timeout => "WFD M4 zaman aşımı",
        TextKey::WfdM4Rejected => "WFD M4 reddedildi: {line}",
        TextKey::WfdM5Timeout => "WFD M5 zaman aşımı",
        TextKey::WfdM5Rejected => "WFD M5 reddedildi: {line}",
        TextKey::WfdM6Timeout => "WFD M6 SETUP zaman aşımı",
        TextKey::WfdM6ExpectedSetup => "WFD M6 SETUP bekleniyordu, gelen: {line}",
        TextKey::WfdM7Timeout => "WFD M7 PLAY zaman aşımı",
        TextKey::WfdM7ExpectedPlay => "WFD M7 PLAY bekleniyordu, gelen: {line}",
        TextKey::WfdInvalidLocalUrl => "Geçersiz yerel akış adresi: {url}",
        TextKey::WfdLocalFmp4Connect => "Yerel fMP4 akışına bağlanılamadı",
        TextKey::WfdLocalFmp4Closed => "Yerel fMP4 akışı beklenmeden kapandı",
        TextKey::WfdLocalFmp4Error => "Yerel fMP4 akışı hata döndürdü: {status}",
        TextKey::WfdFfmpegStart => "WFD ffmpeg başlatılamadı: {err}",
        TextKey::WfdFfmpegStdin => "WFD ffmpeg stdin açılamadı",
        TextKey::WfdFfmpegStdout => "WFD ffmpeg stdout açılamadı",

        TextKey::WirelessMccLabel => {
            "MCC destekli (zaman paylaşımlı çok kanallı · {channels} kanal, {ifaces} arayüz)"
        }
        TextKey::WirelessSccLabel => {
            "Yalnız SCC (tek kanal eşzamanlı · {ifaces} arayüz)"
        }
        TextKey::WirelessNoConcLabel => "Eşzamanlı STA+P2P yok (tek arayüz kısıtı)",
        TextKey::WirelessUnknownLabel => "Bilinmiyor (Wi-Fi adaptörü yok veya erişilemedi)",
        TextKey::WirelessNoP2pModeWarn => {
            "Wi-Fi sürücüsü P2P-client / P2P-GO arayüz modlarını bildirmiyor; MS-MICE önerilir."
        }
        TextKey::WirelessSccDfsWarn => {
            "Kart yalnız SCC (#channels=1) destekliyor ve aktif Wi-Fi DFS Kanal {ch} ({freq} MHz) üzerinde. DFS-dışı Kanal {op_ch} üzerinde P2P Client oturumu STA bağlantısını kesintiye uğratabilir; mümkünse aynı yerel ağda MS-MICE kullanın."
        }
        TextKey::WirelessNoConcDfsWarn => {
            "Wi-Fi kartı STA+P2P eşzamanlılığını desteklemiyor; P2P aktif Wi-Fi bağlantısını düşürebilir."
        }
        TextKey::WirelessDfsReason => {
            "Aktif Wi-Fi DFS/radar kanalında (Kanal {ch}, {freq} MHz). P2P pazarlığında go_intent=0 ile P2P Client rolü seçildi; TV DFS-dışı Kanal {op_ch} ({op_freq} MHz) üzerinde P2P-GO olmaya zorlanıyor — STA internet bağlantısı korunur."
        }
        TextKey::WirelessMccReason => {
            "WDI/MCC zaman paylaşımlı radyo etkin (STA Kanal {ch}, P2P Kanal {op_ch}). P2P Client (go_intent=1) ile sanal arayüz üzerinden bağlanılır."
        }
        TextKey::WirelessSccReason => {
            "SCC eşzamanlılığı için P2P kanalı aktif STA kanalıyla ({op_ch} / {op_freq} MHz) eşleştirildi."
        }
        TextKey::WirelessIdleReason => {
            "Aktif Wi-Fi bağlantısı yok; DFS-dışı Kanal {op_ch} ({op_freq} MHz) ve go_intent=1 seçildi."
        }
        TextKey::WirelessP2pDeviceNote => "Wi-Fi Direct P2P (go_intent={intent}, Kanal {ch})",
        TextKey::WirelessVirtExisting => "Mevcut sanal P2P grup arayüzü kullanılıyor: {iface}",
        TextKey::WirelessVirtDynamic => {
            "wpa_supplicant p2p_no_group_iface=0 ile pazarlık sonrası sanal P2P grup arayüzü ayrılacak"
        }
        TextKey::WirelessNmDfsFallbackWarn => {
            "wpa_supplicant kontrol soketi kapalı; NetworkManager WifiP2P kullanılıyor. Aktif Wi-Fi DFS/radar kanalında (Kanal {ch}, {freq} MHz) ve NetworkManager D-Bus arayüzü go_intent=0 / disallow_freq zorlamasını desteklemiyor; P2P başarısız olursa veya STA interneti düşerse wpa_supplicant ctrl_interface açın ya da aynı ağda MS-MICE/tarayıcı modunu kullanın."
        }
        TextKey::WirelessNoWifiAdapter => {
            "Wi-Fi adaptörü bulunamadı; P2P yerine aynı ağda MS-MICE veya tarayıcı kullanın."
        }
        TextKey::WirelessWpaCliFailed => {
            "wpa_cli çalıştırılamadı ({err}). wpasupplicant kurulu olmalı."
        }
        TextKey::WirelessP2pRejected => "Wi-Fi Direct P2P bağlantısı reddedildi ({mac}): {detail}",
        TextKey::WirelessP2pGroupTimeout => {
            "P2P grubu zaman aşımına uğradı ({mac}). TV ekranında eşleşme onayını kontrol edin."
        }
        TextKey::WirelessTokioError => "Tokio çalışma zamanı hatası: {err}",
        TextKey::WirelessNmThreadCrashed => "NetworkManager P2P iş parçacığı çöktü",
        TextKey::WirelessNoP2pBackend => {
            "wpa_supplicant kontrol soketi (/run/wpa_supplicant) erişilemiyor ve NetworkManager WifiP2P etkin değil; aynı ağda MS-MICE veya tarayıcı modunu kullanın."
        }
        TextKey::WirelessDbusSystemFailed => "Sistem D-Bus bağlantısı kurulamadı: {err}",
        TextKey::WirelessNmDbusFailed => "NetworkManager D-Bus arayüzü açılamadı: {err}",
        TextKey::WirelessNmGetDevicesFailed => "NetworkManager GetAllDevices hatası: {err}",
        TextKey::WirelessNmNoWifiP2pDevice => {
            "NetworkManager üzerinde kullanılabilir WifiP2P aygıtı bulunamadı."
        }
        TextKey::WirelessNmDeviceP2pFailed => "NetworkManager Device.WifiP2P hatası: {err}",
        TextKey::WirelessNmAddActivateFailed => {
            "NetworkManager AddAndActivateConnection reddedildi ({mac}): {err}"
        }
        TextKey::WirelessNmActiveWatchFailed => "ActiveConnection izlenemedi: {err}",
        TextKey::WirelessNmP2pRejected => {
            "NetworkManager WifiP2P bağlantısı reddedildi veya kesildi ({mac})"
        }
        TextKey::WirelessNmP2pClosed => {
            "NetworkManager WifiP2P etkin bağlantı nesnesi kapandı ({mac})"
        }
        TextKey::WirelessNmP2pTimeout => {
            "NetworkManager WifiP2P grubu zaman aşımına uğradı ({mac}). TV ekranında eşleşme onayını kontrol edin."
        }
        _ => return None,
    })
}
