//! Turkish UI (TUI) and CLI messages.

use crate::i18n::TextKey;

pub fn lookup(key: TextKey) -> &'static str {
    match key {
        TextKey::TuiReady => "Hazır",
        TextKey::TuiHeaderStream => {
            " screencast {ver}  |  yayın: {quality} · {mbps} Mb/s  |  {status} "
        }
        TextKey::TuiUnsupported => "destek yok",
        TextKey::TuiBrowserTag => "tarayıcı",
        TextKey::TuiLiveTag => "yayında",
        TextKey::TuiReadyTag => "uygun",
        TextKey::TuiBrowserModeRow => "Tarayıcı modu (MSE)",
        TextKey::TuiDevicesTitle => " cihazlar ({count}) ",
        TextKey::TuiPausedTag => " · DURAKLATILDI",
        TextKey::TuiIdleTag => " · BOŞTA",
        TextKey::TuiMuted => "sessiz",
        TextKey::TuiAudioOn => "ses açık",
        TextKey::TuiStatsLine => {
            " istatistik: {fps} fps · {clients} istemci · gecikme {latency} ms · {mute}{pause}{idle} "
        }
        TextKey::TuiFooterRemote => " Kumanda modu etkin — TUI tuşları duraklatıldı ",
        TextKey::TuiFooterHelp => {
            " ↑↓ seç  ⏎ paylaş  p duraklat  m ses  r tara  s kare  q çık "
        }

        TextKey::CliErrPrefix => "Hata [{code}]: {err}",
        TextKey::CliErrPlain => "Hata: {err}",
        TextKey::CliHintPrefix => "Öneri: {hint}",
        TextKey::CliGuiStartFailed => "Masaüstü arayüzü (python3 + GTK3) başlatılamadı: {err}",
        TextKey::CliGuiExitFailed => "Masaüstü arayüzü hata ile kapandı ({status})",
        TextKey::CliConfigFileComment => "# Dosya: {path}",
        TextKey::CliConfigExists => {
            "Yapılandırma zaten mevcut: {path} (üzerine yazmak için `--force` kullanın)"
        }
        TextKey::CliConfigCreated => "Yapılandırma oluşturuldu: {path}",

        TextKey::DiscoverNoneFound => "Ağda Cast, DLNA veya Miracast cihazı bulunamadı.",
        TextKey::DiscoverHint => {
            "İpucu: TV'nin aynı Wi-Fi/LAN ağında olduğundan emin olun, P2P için `--p2p` ekleyin veya `screencast share --to browser` kullanın."
        }
        TextKey::DiscoverColId => "KİMLİK",
        TextKey::DiscoverColProto => "PROTOKOL",
        TextKey::DiscoverColAddr => "ADRES",
        TextKey::DiscoverColNameNote => "AD / NOT",
        TextKey::DiscoverDefaultNote => "uygun",

        TextKey::DoctorTitle => "screencast {ver} — ortam kontrolü",
        TextKey::DoctorDone => "Bitti.",
        TextKey::DoctorSecSession => "== Oturum ==",
        TextKey::DoctorKeyType => "tip",
        TextKey::DoctorKeyDesktop => "masaüstü",
        TextKey::DoctorWaylandNote => {
            "  not      : Wayland'da x11grab ekranı göstermez; XDG ScreenCast portalı + PipeWire kullanılır."
        }
        TextKey::DoctorSecDisplays => "== Ekranlar ==",
        TextKey::DoctorNoXrandr => {
            "  xrandr ekran listesi alınamadı (Wayland + XWayland kapalı veya headless olabilir)"
        }
        TextKey::DoctorSecNetwork => "== Ağ adresleri ==",
        TextKey::DoctorNoLanIp => "  ulaşılabilir LAN IPv4 adresi bulunamadı",
        TextKey::DoctorSecWireless => "== Kablosuz Yansıtma (WDI/MCC · DFS · MS-MICE) ==",
        TextKey::DoctorWifiPhyMissing => {
            "  Wi-Fi phy : bulunamadı (yalnız kablolu Ethernet veya sanal ortam)"
        }
        TextKey::DoctorAdapter => "  adaptör   : {phy} (arayüz: {iface}, sürücü: {driver})",
        TextKey::DoctorWdiMcc => "  WDI / MCC : {label}",
        TextKey::DoctorVirtualP2p => {
            "  sanal P2P : P2P-client={c}, P2P-GO={g}, P2P-device={d}, mevcut={existing}"
        }
        TextKey::DoctorActiveWifi => {
            "  aktif WiFi: {iface} -> {ssid} (Kanal {ch}, {freq} MHz{dfs})"
        }
        TextKey::DoctorDfsTag => " [DFS/RADAR]",
        TextKey::DoctorNonDfsTag => " [DFS-dışı]",
        TextKey::DoctorP2pPlan => {
            "  P2P planı : rol={role}, go_intent={intent}, işletim kanalı={ch} ({freq} MHz, reg_class={rc})"
        }
        TextKey::DoctorReason => "  gerekçe   : {reason}",
        TextKey::DoctorWarning => "  uyarı     : {warning}",
        TextKey::DoctorMsMice => "  MS-MICE   : {status} (Ethernet: {eth})",
        TextKey::DoctorMiceReady => "HAZIR (_display._tcp mDNS, P2P gerektirmez)",
        TextKey::DoctorMiceNoLan => "LAN adresi yok",
        TextKey::DoctorP2pCtrl => "  P2P denet : wpa_cli={wpa}, nm_wifi_p2p={nm}",
        TextKey::DoctorNoSocket => "soket yok",
        TextKey::DoctorReadyWord => "hazır",
        TextKey::DoctorMissingWord => "yok",
        TextKey::DoctorP2pPeer => "  P2P eş    : {name} ({mac}, wfd={wfd})",
        TextKey::DoctorSecTools => "== Araçlar ==",
        TextKey::DoctorFfmpegVer => "  ffmpeg sürümü        : {ver}",
        TextKey::DoctorEncoder => "  kodlayıcı            : nvenc={nv} vaapi={va} libx264={x264} -> seçilen: {chosen}",
        TextKey::DoctorSelected => "seçilen",
        TextKey::DoctorErrorWord => "hata",
        TextKey::DoctorGstEncoder => "  gst-kodlayıcı        : video={v} ses={a}",
        TextKey::DoctorMutedWord => "yok (sessiz)",
        TextKey::DoctorSecAudio => "== Ses ==",
        TextKey::DoctorDefaultMonitor => "  varsayılan monitor   : {val}",
        TextKey::DoctorAudioSource => "  kaynak               : {src}",
        TextKey::DoctorSecDiscover => "== Cihaz keşfi (3 sn) ==",
        TextKey::DoctorDiscoverEmpty => {
            "  Cihaz bulunamadı (tarayıcı modu her zaman kullanılabilir)."
        }
        TextKey::DoctorDiscoverErr => "  Keşif hatası: {err}",
        TextKey::DoctorSecQuality => "== Kalite merdiveni doğrulaması (AC-9) ==",
        TextKey::DoctorQualityLine => {
            "  {label} -> {w}x{h} @ {vbit} ({codec}, GOP {gop}s, ses {abit}) — {n} argüman"
        }
        TextKey::DoctorSecIdle => "== Boş ekran algılama testi (AC-2) ==",
        TextKey::DoctorIdleFail => "IdleDetector 3 sn sonunda idle'a geçmedi",
        TextKey::DoctorIdleTransition => "  boş ekran geçişi : 3.0 sn -> fps=1, hedef=300k",
        TextKey::DoctorIdleSample => "  ölçülen örnek    : ~{kbps} kb/s (AC-2 hedef < 300 kb/s)",
        TextKey::DoctorSecAudioSwitch => "== Ses değişimi izleme testi (AC-3) ==",
        TextKey::DoctorNewSource => "  yeni kaynak      : {dev} ({ms} ms, AC-3 hedef <= 3000 ms)",
        TextKey::DoctorAudioSwitchMissing => "Beklenen SourceChanged olayı gelmedi: {got}",
        TextKey::DoctorSecBench => "== Gecikme ölçümü: 100 örnek (AC-1) ==",
        TextKey::DoctorBenchTransport => {
            "  taşıma (hub->WS) : p50={p50} ms  p95={p95} ms  max={max} ms (100 örnek)"
        }
        TextKey::DoctorBenchE2e => {
            "  tahmini uçtan uca: p50 <= 400 ms ve p95 <= 700 ms hedefi içinde (GOP=0.5s / frag=100ms)"
        }

        TextKey::SelftestWs101 => "WS 101 bekleniyordu",
        TextKey::SelftestMissingFtyp => "init segment (ftyp) alınamadı",
        TextKey::SelftestMissingMoof => "medya parçası (moof) alınamadı",
        TextKey::SelftestHealthFail => "/healthz beklenen yanıtı vermedi",
        TextKey::SelftestTokenFail => "token koruması 401 dönmedi",
        TextKey::SelftestInfoFail => "/api/info 200 dönmedi",
        TextKey::SelftestInfoVerFail => "/api/info version eksik",

        TextKey::ShareStatusLive => "Yayın: {name} ({note})",
        TextKey::ShareStatusBrowserFallback => "Tarayıcı yedeği: {url} ({err})",
        TextKey::ShareBannerLive => "screencast yayında!",
        TextKey::ShareBannerStatus => "  durum     : {status}",
        TextKey::ShareBannerBrowser => "  tarayıcı  : {url}",
        TextKey::ShareBannerBackend => "  arka uç   : {backend}",
        TextKey::ShareBannerCtrlC => "Durdurmak için Ctrl+C'ye basın.",
        TextKey::ShareTuiPaused => "Duraklatıldı",
        TextKey::ShareTuiLive => "Yayında",
        TextKey::ShareTuiScanning => "Cihazlar taranıyor…",
        TextKey::ShareTuiFoundCount => "{count} cihaz bulundu",
        TextKey::ShareTuiSnapSaved => "Kare kaydedildi: {file}",
        TextKey::ShareTuiSnapUpdated => "/snap.jpg güncellendi",
        TextKey::ShareTuiSnapUnavailable => "Kare alınamadı (akış henüz hazır değil)",
        TextKey::ShareTuiErrorPrefix => "Hata: {err}",
        _ => "",
    }
}
