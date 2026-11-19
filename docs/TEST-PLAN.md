# ekran — Test Planı (v2.0)

İki bölüm: **A. otomatik testler** (CI'da çalışır) ve **B. gerçek cihaz doğrulaması**
(elle, `ekran doctor --live` ile).

---

## A. Otomatik testler

### A1. Birim testleri (crate bazında)

| Test adı | Kapsam | AC |
|---|---|---|
| `ekran_core::tests::quality_roundtrip` | 480/540/720/1080/1440 → doğru ölçü | PLAN §11 |
| `ekran_core::tests::gop_shrinks_with_quality` | GOP kaliteyle küçülür | AC-1 |
| `ekran_core::tests::quality_parse_rejects_unknown` | "4k" hata verir, Türkçe mesaj | §9 |
| `ekran_core::config::tests::schema_version_mismatch` | eski şema sürümü anlaşılır hata | T-M0-05 |
| `ekran_encode::plan::tests::snapshot_1080p` | ffmpeg argümanları PLAN §11 ile birebir | T-M1-04 |
| `ekran_encode::plan::tests::snapshot_480p` | aynı, 480p | T-M1-04 |
| `ekran_encode::codec::tests::fallback_chain` | nvenc→vaapi→libx264 | T-M1-03 |
| `ekran_encode::adaptive::tests::hysteresis_no_flap` | %20↑ / %15↓ eşiği zıplama yapmaz | T-M2-05 |
| `ekran_transport::mp4::tests::init_segment_detected` | ftyp+moov ayrımı | T-M1-06 |
| `ekran_transport::mp4::tests::fragment_detected` | moof+mdat ayrımı | T-M1-06 |
| `ekran_transport::hub::tests::slow_client_does_not_block` | yavaş istemci diğerlerini bloklamaz | T-M1-07 |
| `ekran_transport::ws::tests::handshake_and_binary_frame` | RFC 6455 el sıkışma + binary | T-M1-05 |
| `ekran_transport::ws::tests::token_required` | token'sız bağlantı 401 | T-M1-08 |
| `ekran_audio::watcher::tests::sink_change_triggers_rebind` | mock `pactl` ile değişim → yeniden bağlama | AC-3 |
| `ekran_audio::source::tests::resolves_default_monitor` | varsayılan sink'in monitor'ı | T-M2-01 |
| `ekran_sinks::mdns::tests::parses_googlecast_txt` | TXT alanları doğru | T-M3-01 |
| `ekran_sinks::ssdp::tests::parses_location_and_usn` | SSDP yanıtı ayrıştırma | T-M3-02 |
| `ekran_sinks::cast::tests::protobuf_roundtrip` | CastMessage kodlama/çözme | T-M4-01 |
| `ekran_sinks::cast::tests::mock_server_handshake` | mock Cast sunucusuyla CONNECT/PING | T-M4-02 |
| `ekran_sinks::dlna::tests::content_xml_is_valid` | DIDL-Lite XML geçerli | T-M5-01 |
| `ekran_sinks::select::tests::prefers_cast_over_dlna` | otomatik seçim sırası | PROTOCOLS §7 |
| `ekran_sinks::select::tests::falls_back_to_browser` | cihaz yoksa tarayıcı modu | T-M3-05 |

### A2. Entegrasyon testleri

| Test adı | Akış | AC |
|---|---|---|
| `it_test_source_to_ws` | lavfi → fMP4 → WS → istemci ilk kareyi alır | T-M1-01 |
| `it_twenty_clients_same_stream` | 20 istemci, tek üretici | T-M1-07 |
| `it_hls_playlist_updates` | HLS playlist'e yeni segment eklenir | T-M4-03 |
| `it_token_blocks_unauthorised` | token'sız istek reddedilir | T-M1-08 |
| `it_http_api_contract` | `/api/info`, `/api/stats` şema doğru | T-M1-08 |

### A3. Fixtures (test verileri)

| Fixture | Açıklama | Nerede |
|---|---|---|
| `sample_frag.mp4` | 3 saniyelik geçerli fMP4 (moov+2 fragment) | `crates/ekran-transport/tests/fixtures/` |
| `fake_pactl.sh` | `pactl` davranışını taklit eden betik (sink değişimi senaryosu) | `crates/ekran-audio/tests/fixtures/` |
| `fake_cast_server.rs` | yerel TLS + protobuf Cast sunucusu | `crates/ekran-sinks/tests/fixtures/` |
| `fake_ssdp_server.rs` | SSDP M-SEARCH yanıtlayıcı | `crates/ekran-sinks/tests/fixtures/` |
| `quality_ladder.json` | PLAN §11 tablosunun makine-okunur hâli | `crates/ekran-core/tests/fixtures/` |

> `quality_ladder.json` ile `ekran-core` testi, PLAN.md §11 tablosunu **otomatik karşılaştırır**:
> doküman ile kod birbirinden ayrışırsa CI kırılır.

### A4. CI

```
fmt (cargo fmt --check) → clippy (-D warnings) → test (workspace) →
snapshot testleri → musl çapraz derleme → .deb + flatpak paketleme
```
Ortam: `stable` + `1.23` Rust, Ubuntu 24.04 runner. Ağ gerektiren testler `--ignored` olarak
işaretlenir, CI'da çalıştırılmaz.

---

## B. Gerçek cihaz doğrulaması (Zorin OS 18.1 + gerçek TV)

| # | Kontrol | Beklenen | Sonuç |
|---|---|---|---|
| A1 | `ekran doctor` | Wayland + portal + NVENC tespit edilir | ➖ |
| A2 | `ekran doctor --discover` | TV listede (protokol türüyle) | ➖ |
| A3 | `ekran selftest` | "SELFTEST OK" | ➖ |
| B1 | Wayland portal ekranı seç → yayın | Akıcı görüntü, siyah ekran YOK | ➖ |
| B2 | İkinci monitör seçimi | Doğru ekran geliyor | ➖ |
| B3 | Xorg oturumunda (`zorin-xorg.desktop`) | Çalışıyor | ➖ |
| B4 | Ekran kilitliyken | Son kare donuyor (çökme yok) | ➖ |
| C1 | Spotify/Web çalarken ses TV'de | Ses duyuluyor | ➖ |
| C2 | Hoparlör → Bluetooth kulaklık | ≤ 3 sn'de ses geri geliyor (AC-3) | ➖ |
| C3 | Sessizlik → sonra müzik | Takılma yok, ses başlıyor | ➖ |
| C4 | `ekran doctor --audio-switch` | Otomatik test geçiyor | ➖ |
| D1 | `--quality 480p` | 854×480, akıcı | ➖ |
| D2 | `--quality 540p` | 960×540 | ➖ |
| D3 | `--quality 720p` | 1280×720 | ➖ |
| D4 | `--quality 1080p` | 1920×1080, metinler net | ➖ |
| D5 | `--quality 1440p` | 2560×1440 | ➖ |
| D6 | `--quality auto` | Zayıf Wi-Fi'de merdiven düşüyor, takılma yok | ➖ |
| E1 | **Cast**: TV'nin yerleşik oynatıcısında görüntü | Görüntü + ses, TV'de uygulama yok (AC-6/AC-7) | ➖ |
| E2 | **DLNA**: TV'nin medya oynatıcısında görüntü | Görüntü (canlı desteklenmiyorsa ➖ + uyarı) | ➖ |
| E3 | Browser: TV tarayıcısında `http://<ip>:7700` | MSE ile akış | ➖ |
| E4 | Hiçbir sink yokken | Anlaşılır hata + URL/QR önerisi | ➖ |
| F1 | TV'den fare hareketi | İmleç PC'de hareket ediyor | ➖ |
| F2 | TV'den tıklama | Tıklanıyor | ➖ |
| F3 | TV'den klavye | Türkçe karakterler doğru | ➖ |
| F4 | Wayland'da kumanda (portal) | Root'suz çalışıyor | ➖ |
| F5 | TV kumandasıyla ok/Enter/Geri | Menüler gezinilebiliyor | ➖ |
| G1 | `ekran doctor --bench` (1080p, Wi-Fi) | p50 ≤ 400 ms, p95 ≤ 700 ms (AC-1) | ➖ |
| G2 | `ekran doctor --idle` | CPU < %8, bant < 300 kb/s (AC-2) | ➖ |
| G3 | 30 dk sustained yayın | Bellek sızıntısı yok, fan sakin | ➖ |
| G4 | 3 TV + 2 tarayıcı aynı anda | Hepsi akıcı | ➖ |
| H1 | Arayüz DESIGN.md ile uyum | Token'lar, durumlar, ölçüler aynı (AC-10) | ➖ |
| H2 | 720p TV'de okunabilirlik | Gövde ≥ 16 px, hedef ≥ 48 px | ➖ |
| H3 | `prefers-reduced-motion` | Geçişler kapanır | ➖ |
