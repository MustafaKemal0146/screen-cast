"""Turkish translation catalog for the screencast GTK3 desktop GUI."""

TR_CATALOG = {
    "window_title": "screencast",
    "ready_title": "Hazır",
    "ready_sub": "Bir TV seçip “Yansıt”a basın.",
    "checking_env": "Ortam denetleniyor…",
    "refresh": "Yenile",
    "copy_url": "Tarayıcı adresini kopyala",
    "stop_stream": "Yayını durdur",
    "section_tvs": "TV'LER",
    "browser_link": "TV listede yok mu? Tarayıcı modunu kullan  →",
    "browser_mode_name": "Tarayıcı modu",
    "other_devices": "Diğer cihazlar",
    "unsupported_count": "Desteklenmeyen cihazlar ({count})",
    "section_settings": "AYARLAR",
    "quality_auto": "Otomatik",
    "p2p_tooltip": "Wi-Fi Direct (Miracast P2P) aktif radyo taramasını dahil et (--p2p)",
    "off": "Kapalı",
    "lbl_quality": "Kalite",
    "lbl_fps": "Kare hızı",
    "lbl_audio": "Sistem sesi",
    "lbl_p2p": "P2P tarama",
    "lbl_record": "Kayıt",
    "details_log": "Ayrıntılar (günlük)",
    "env_unavailable": "Ortam bilgisi alınamadı",
    "env_unavailable_err": "Ortam bilgisi alınamadı ({err})",
    "scanning_title": "TV'ler aranıyor…",
    "scanning_p2p": "Ağdaki cihazlar ve Wi-Fi Direct P2P taranıyor.",
    "scanning_lan": "Ağdaki cihazlar taranıyor.",
    "scan_error": "[tarama hatası] {err}",
    "found_tvs": "{count} TV bulundu. “Yansıt”a basın.",
    "no_tv_title": "TV bulunamadı",
    "no_tv_sub": "TV'yi açıp “Yenile”ye basın.",
    "cast_btn": "Yansıt",
    "unsupported_row": "{kinds} · {address} — henüz desteklenmiyor",
    "empty_card_title": "Ağda uygun TV bulunamadı",
    "empty_card_desc": (
        "TV'nin açık ve bilgisayarla aynı Wi-Fi ağında olduğundan emin olun, "
        "sonra “Yenile”ye basın. Samsung TV'ler bekleme modunda ağı kapatabilir."
    ),
    "record_dialog_title": "Kayıt dosyası seç",
    "cancel": "_İptal",
    "save": "_Kaydet",
    "record_default_name": "screencast-recording.mp4",
    "start_failed": "Başlatılamadı",
    "connecting_title": "Bağlanıyor…",
    "connecting_portal_sub": "{target} · Açılan pencerede paylaşılacak ekranı seçip onaylayın.",
    "preparing_title": "Yayın hazırlanıyor…",
    "screen_captured": "{target} · Ekran yakalandı.",
    "live_title": "Canlı yayında",
    "playing_on": "{target} ekranında oynatılıyor.",
    "browser_fallback_ready": "\nTarayıcıdan izlemek için adres hazır: {url}",
    "tv_unreachable_title": "TV'ye bağlanılamadı",
    "stopped_title": "Durduruldu",
    "stopped_sub": "Yeni bir yayın başlatabilirsiniz.",
    "unexpected_exit": "Yayın beklenmedik şekilde kapandı (kod {code}).",
    "stream_start_failed": "Yayın başlatılamadı",
    "copied_clipboard": "Panoya kopyalandı: {url}",
    "unknown_device": "Bilinmeyen cihaz",
    "err_unreachable": (
        "TV'ye ulaşılamıyor. TV'nin açık ve bilgisayarla aynı Wi-Fi ağında "
        "olduğundan emin olun (bekleme modunda ağ kapanabilir)."
    ),
    "err_portal": (
        "Ekran paylaşımı onaylanmadı. Tekrar “Yansıt”a basın ve açılan "
        "pencerede paylaşılacak ekranı seçip onaylayın."
    ),
    "err_no_sink": "TV ağda bulunamadı. TV'yi açıp “Yenile”ye basın, sonra tekrar deneyin.",
    "err_encoder": (
        "Uygun video kodlayıcı bulunamadı. `ffmpeg` ve GPU sürücülerini "
        "(`screencast doctor`) kontrol edin."
    ),
    "err_p2p": (
        "Wi-Fi Direct (P2P) bağlantısı kurulamadı. TV aynı ağdaysa P2P yerine "
        "MS-MICE / Cast / DLNA veya tarayıcı modunu deneyin."
    ),
}

TR_PARSE_TOKENS = {
    "type_keys": ("tip",),
    "selected_keys": ("seçilen:",),
    "network_sections": ("== Ağ",),
    "browser_prefixes": ("tarayıcı  :",),
    "error_prefixes": ("Hata",),
    "status_live_prefixes": ("durum     : Yayın:",),
    "status_fallback_split": ("Tarayıcı yedeği:",),
    "pipewire_ready": ("PipeWire düğümü hazır",),
    "unreachable_words": ("ulaşılamıyor", "bağlantıyı reddetti", "zaman aşımı"),
    "portal_words": (
        "izni verilmedi",
        "izin verilmedi",
        "ekran paylaşımını reddetti",
        "portal izin penceresi",
    ),
    "no_sink_words": ("hedef cihaz bulunamadı",),
    "encoder_words": ("kullanılabilir kodlayıcı yok", "uygun video kodlayıcı bulunamadı"),
    "p2p_words": ("p2p bağlantısı", "p2p grubu", "p2p görevi", "wi-fi adaptörü bulunamadı"),
}
