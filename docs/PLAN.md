# ekran — Proje Planı (v2.0)

> **Normatif dosya.** Kod yazan her ajan/person önce burayı okur.
> Görevler `T-Mx-yy` kimliğiyle tanımlıdır; kimliksiz iş yapılmaz.
> Durum işaretleri: `[ ]` açık · `[~]` sürüyor · `[x]` bitti · `[!]` engellendi (gerekçe `docs/DECISIONS.md`'de)
> İlgili dokümanlar: `docs/ARCHITECTURE.md` (teknik), `docs/PROTOCOLS.md` (protokol),
> `docs/DESIGN.md` (arayüz), `docs/EXECUTION.md` (ajan çalıştırma), `docs/TEST-PLAN.md` (cihaz doğrulama).

---

## 0. Tek cümlelik tanım

**ekran**, bir Linux masaüstünü ev ağındaki TV'ye **canlı, ölçülebilir düşük gecikmeyle** yansıtır;
TV'ye **hiçbir uygulama kurulmadan**, TV'nin desteklediği **yerleşik protokolü (Google Cast, DLNA,
Miracast, AirPlay) veya tarayıcısını** otomatik seçerek kullanır; TV'yi ayrıca bir **kumanda
cihazına** dönüştürür.

## 1. Kilitli kararlar

| # | Karar | Gerekçe |
|---|---|---|
| D1 | Dil: **Rust** (2021), cargo workspace, 8 crate | Profil uyumu, düşük gecikme, async |
| D2 | Video: H.264 4:2:0; kodlayıcı zinciri **NVENC → VAAPI → libx264** | Her GPU'da çalışır, CPU yanmaz |
| D3 | Taşıma: **fMP4 (moof+mdat) → WebSocket → MSE** (tarayıcı) ve **HLS** (Cast/DLNA) | Kumanda + en düşük gecikme MSE'de; TV oynatıcıları HLS'i anlar |
| D4 | **Sink modeli marka bağımsız**: `Sink` trait + `SinkKind` adaptörleri | "Her protokol desteklensin" |
| D5 | TV tarayıcısı **opsiyonel**; birincil yol TV'nin yerleşik protokolü | Kullanıcı kuralı |
| D6 | Ses: PulseAudio/PipeWire **monitor** + **sink değişim takipçisi** | Ses kopması sorununun kök nedeni |
| D7 | Kalite: **480p / 540p / 720p / 1080p / 1440p** + `auto` | Kullanıcı isteği |
| D8 | Arayüz: **Apple tarzı web oynatıcı** (TV) + **terminal TUI** (Linux) | Tasarım talebi + terminal-first profil |
| D9 | Bulut/hesap yok, tek binary, tamamen yerel | "Veri sende kalır" |
| D10 | Arayüz metinleri Türkçe, kod tanımlayıcıları İngilizce | Tutarlılık |

## 2. Kapsam matrisi — "hepsi için" ne demek

| Katman | Kapsananlar |
|---|---|
| **Masaüstü oturumu** | X11 (x11grab), Wayland/portal (GNOME), Wayland/Xorg hibrit, Xorg oturumu |
| **TV protokolü** | Google Cast, DLNA/UPnP, Miracast (M10), AirPlay (M10), TV tarayıcısı (MSE + HLS) |
| **Kalite** | 480p, 540p, 720p, 1080p, 1440p, auto (adaptif) |
| **Ses** | Sistem sesi (monitor), mikrofon (isteğe bağlı), sink değişiminde otomatik toparlanma |
| **Kumanda** | Fare, tıklama, kaydırma, klavye (Türkçe karakter), TV kumandası navigasyonu |
| **İstemci** | TV, telefon, tablet, dizüstü (aynı anda çoklu) |
| **İşletim sistemi** | Linux (hedef), macOS/Windows için derleme yolu açık (capture backend değişir) |

**"Desteklenmiyor" kuralı:** Hiçbir yol yoksa kullanıcıya Türkçe, eylem öneren hata gösterilir
(`docs/ARCHITECTURE.md` §9). Sessizce başarısız olmak yasak.

## 3. Kabul kriterleri (proje "bitti" demek için)

| Kimlik | Hedef | Ölçüm |
|---|---|---|
| AC-1 | Gecikme (Wi-Fi, 1080p) **≤ 400 ms p50**, ≤ 700 ms p95 | `ekran doctor --bench` (100 örnek) |
| AC-2 | Boş ekranda CPU < %8, bant < 300 kb/s | `ekran doctor --idle` |
| AC-3 | Ses, çıkış cihazı değişiminden **≤ 3 sn** sonra geri gelir | `ekran doctor --audio-switch` |
| AC-4 | 1080p30 **≥ 30 fps** (NVENC) | oynatıcı FPS sayacı |
| AC-5 | Cihaz listesi TV'yi **≤ 5 sn** içinde gösterir | TUI zaman damgası |
| AC-6 | Cast/DLNA'da **sesli** görüntü | TEST-PLAN E1/E2 |
| AC-7 | TV tarayıcısı olmadan yayın mümkün | TEST-PLAN E1/E2 |
| AC-8 | Tek binary; çalışma anında sadece `ffmpeg` gerekir | `ldd` + temiz makine testi |
| AC-9 | Tüm kalite seviyeleri tek bayrakla seçilebilir | `ekran doctor --quality` |
| AC-10 | Arayüz DESIGN.md ile birebir uyumlu | Tasarım incelemesi |

## 4. Dalga 0 — Temel (tek ajan, 1 gün) — Rol G

| ID | Görev | Dosyalar | AC | Test | Bağımlılık |
|---|---|---|---|---|---|
| [x] T-M0-01 | Cargo workspace: 8 crate + ortak bağımlılıklar | `Cargo.toml`, `crates/*/Cargo.toml` | `cargo build` hatasız | `cargo build --workspace` | — |
| [x] T-M0-02 | `Quality` (480/540/720/1080/1440/auto) + `StreamPlan` + `SinkKind` + `DeviceInfo` + `Capabilities` + `Monitor` + `Backend` + `Error` | `crates/ekran-core/src/lib.rs` | PLAN §6 ile birebir değerler | `cargo test -p ekran-core` | T-M0-01 |
| [x] T-M0-03 | `Sink` trait + `PreparedSink` + `CaptureSource` trait + `Stats` | `crates/ekran-core/src/traits.rs` | ARCHITECTURE §1/§2 ile aynı imzalar | `cargo test -p ekran-core` | T-M0-02 |
| [x] T-M0-04 | `ekran doctor`: oturum, ekranlar, ağ, araçlar, ses kaynakları, kodlayıcı seçimi | `crates/ekran-cli/src/doctor.rs` | Zorin'de Türkçe 40+ satırlık rapor | `./ekran doctor` | T-M0-01 |
| [x] T-M0-05 | Config: `~/.config/ekran/config.toml` (şema sürümü 1) + `ekran config init/show` | `crates/ekran-core/src/config.rs`, `crates/ekran-cli/` | sürümlü şema, anlaşılır hata | `cargo test -p ekran-core` | T-M0-02 |
| [x] T-M0-06 | `ekran selftest` (sanal ekran + döngüsel HTTP/WS sunucu) | `crates/ekran-cli/src/selftest.rs` | "SELFTEST OK" | `./ekran selftest` | T-M0-04 |
| [x] T-M0-07 | **ekran-core dondurma**: sözleşmeler onaylanır, sonraki dalgalar değiştiremez | `docs/PLAN.md` (durum) | tüm ajanlar aynı imzayı kullanır | gözden geçirme | T-M0-03 |

## 5. Dalga 1 — Temel akış (paralel, 4 rol, 3 gün)

| ID | Görev | Rol | AC | Test |
|---|---|---|---|---|
| [x] T-M1-01 | `CaptureSource` + `TestSource` (lavfi testsrc2) | B | sanal görüntü akar | `cargo test -p ekran-capture` |
| [x] T-M1-02 | `X11Source` (ffmpeg x11grab) + Wayland'da anlaşılır hata | B | X11'de akış | `cargo test -p ekran-capture` |
| [x] T-M1-03 | Kodlayıcı seçimi (nvenc→vaapi→libx264) | B | doğru seçim, `--codec` ezmesi | `ekran doctor` |
| [x] T-M1-04 | ffmpeg argüman üretimi (PLAN §11 değerleri, GOP, `-force_key_frames`) | B | snapshot testi | `cargo test -p ekran-encode` |
| [x] T-M1-05 | WebSocket sunucu (binary medya + metin kontrol) + token | A | 20 eşzamanlı istemci | `cargo test -p ekran-transport` |
| [x] T-M1-06 | fMP4 ayrıştırıcı: init segment / medya parçası ayrımı | A | doğru ayrım | `cargo test -p ekran-transport` |
| [x] T-M1-07 | Broadcast hub + yavaş istemci koruması (2 sn kuyruk → at) | A | yavaş istemci diğerlerini bloklamaz | entegrasyon testi |
| [x] T-M1-08 | HTTP API: `/`, `/ws`, `/api/info`, `/api/stats`, `/healthz` | A | rotalar yanıt verir | `cargo test -p ekran-transport` |
| [x] T-M1-09 | Web oynatıcı: MSE, live-edge, durum katmanları (DESIGN §6) | F | tarayıcıda akış görünür | elle |
| [x] T-M1-10 | Gecikme ölçümü + rozet (DESIGN §6.4) | F | ±20 ms doğru | `ekran doctor --bench` |
| [x] T-M2-01 | Ses kaynağı çözümleme (varsayılan sink → monitor) | C | doğru kaynak | `ekran doctor` |
| [x] T-M2-02 | **SourceWatcher**: `pactl subscribe` ile değişim izleme, yeniden bağlama | C | AC-3 | `cargo test -p ekran-audio` |
| [x] T-M2-03 | ffmpeg ses girdisi sağlamlaştırma (`aresample=async=1`, `thread_queue_size`, `-map`) | B | takılma yok | `ekran doctor --audio-switch` |

## 6. Dalga 2 — Akıllı kalite, keşif, Cast (paralel, 4 rol, 4 gün)

| ID | Görev | Rol | AC | Test |
|---|---|---|---|---|
| [x] T-M2-04 | Boş ekran algılama (64×36 imza, 3 sn sabit → fps 1, 300 kb/s) | A | AC-2 | `ekran doctor --idle` |
| [x] T-M2-05 | `auto` kalite: istemci hız ölçümü + histerezis (%20↑ / %15↓) | A | zıplama yok | `cargo test -p ekran-encode` |
| [x] T-M2-06 | Kalite seçimi `--quality 480p\|540p\|720p\|1080p\|1440p\|auto` | H | AC-9 | `ekran doctor --quality` |
| [x] T-M3-01 | mDNS tarayıcı (`_googlecast._tcp`, `_airplay._tcp`, `_raop._tcp`) | D | cihazlar listelenir | `cargo test -p ekran-sinks` |
| [x] T-M3-02 | SSDP tarayıcı (MediaRenderer, WifiDisplay) | D | cihazlar listelenir | `cargo test -p ekran-sinks` |
| [x] T-M3-03 | `ekran discover --json` (sabit JSON şeması) | D | şema sabit | `cargo test -p ekran-sinks` |
| [x] T-M4-01 | Cast protobuf şeması (`prost`) | D | mesajlar çözülüyor | `cargo test -p ekran-sinks` |
| [x] T-M4-02 | TLS 8009 + CONNECT/PING/PONG el sıkışması | D | handshake | mock sunucu testi |
| [x] T-M4-03 | Media Receiver'a HLS `LOAD` | D | TV oynatıyor | TEST-PLAN E1 |
| [x] T-M4-04 | Cast kumanda (oynat/duraklat/ses) | D | TV tepki veriyor | TEST-PLAN E1 |
| [x] T-M6-01 | Portal `ScreenCast` (zbus): CreateSession/SelectSources/Start + unix fd | B | fd alınıyor | `cargo test` (portal yoksa atlanır) |
| [x] T-M6-02 | PipeWire fd → GStreamer `pipewiresrc` → NVENC → fMP4 | B | ekran akar | TEST-PLAN B1 |
| [x] T-M6-03 | Portal monitör listesi + seçim | B | 2. ekran seçilebilir | TEST-PLAN B2 |
| [x] T-M6-04 | Hata yolları (izin reddi, portal yok, eski GNOME) | B | anlaşılır Türkçe mesaj | elle |

## 7. Dalga 3 — Kumanda, DLNA, TUI (paralel, 4 rol, 4 gün)

| ID | Görev | Rol | AC | Test |
|---|---|---|---|---|
| [x] T-M5-01 | DLNA DMR tanıtımı (SSDP NOTIFY + device.xml + content.xml) | D | TV listede görünür | TEST-PLAN E2 |
| [x] T-M5-02 | Canlı URL'i DMR olarak sun (HLS, gerekirse progressive MP4) | D | TV oynatır | TEST-PLAN E2 |
| [x] T-M5-03 | Desteklenmeyen TV'de anlaşılır yönlendirme | D | mesaj doğru | elle |
| [x] T-M7-01 | `Control` trait: move/click/key/scroll/text | A | arayüz sabit | `cargo test` |
| [x] T-M7-02 | X11 girdi uygulaması (xdotool) | A | fare/klavye çalışır | TEST-PLAN F1–F3 |
| [x] T-M7-03 | Wayland girdi uygulaması (RemoteDesktop portalı, root'suz) | A | fare/klavye çalışır | TEST-PLAN F4 |
| [x] T-M7-04 | Tarayıcıdan girdi + sanal imleç overlay (DESIGN §6.3) | F | imleç TV'de görünür | elle |
| [x] T-M7-05 | TV kumandası navigasyonu (ok/Enter/Geri) | F | kumanda ile gezinme | elle |
| [x] T-M3-04 | TUI cihaz listesi (ratatui, DESIGN §7) | E | seçim + durum renkleri | `cargo test -p ekran-ui` |
| [x] T-M3-05 | `ekran share --to <id>`: keşif → seçim → yayın | H | ≤3 sn'de yayın | elle |
| [x] T-M8-01 | Oynatıcı son tasarım (frosted panel, gizli çubuk, geçişler) | F | AC-10 | ekran görüntüsü |
| [x] T-M8-02 | TV modu (büyük yazı, uzaktan kumanda menüleri, PiP) | F | 720p'de okunur | elle |

## 8. Dalga 4 — Entegrasyon, ölçüm, paketleme (tek ajan H, 4 gün)

| ID | Görev | AC | Test |
|---|---|---|---|
| [x] T-M8-03 | Hata mesajları: izin yok / TV reddetti / codec yok / cihaz yok | her durumda Türkçe + eylem önerisi | elle |
| [x] T-M8-04 | `ekran doctor --bench`: 100 örnek gecikme ölçümü + rapor | AC-1 | `./ekran doctor --bench` |
| [x] T-M8-05 | `ekran doctor --idle` / `--audio-switch` / `--quality` raporları | AC-2, AC-3, AC-9 | `./ekran doctor` |
| [x] T-M9-01 | `cargo install --path crates/ekran-cli` | kurulum çalışır | temiz makine |
| [x] T-M9-02 | Tek statik binary (musl CI hedefi) | `ldd` statik | CI |
| [x] T-M9-03 | Debian paketi (`packaging/deb/build-deb.sh`) + Flatpak manifest | kuruluyor | CI / `dpkg-deb` |
| [x] T-M9-04 | CI: fmt/clippy/test/çapraz derleme (`.github/workflows/ci.yml`) | yeşil | GitHub Actions |
| [~] T-M9-05 | README güncel (`ekran` CLI kullanımı); demo videosu / `awesome-*` listesi yayım sonrası | — | elle |
| [x] T-M9-06 | `docs/` son güncelleme + PLAN durumlarının kapatılması | — | gözden geçirme |

## 9. Dalga 5 — Derin yol (opsiyonel, "B" yolu)

| ID | Görev | Not |
|---|---|---|
| [ ] T-M10-01 | Kendi X11 capture motoru (x11rb + SHM) | x11grab bağımlılığını bitirir, ~2× daha düşük gecikme |
| [ ] T-M10-02 | Dirty-rectangle delta codec (yalnız değişen bölge) | bant genişliği 3–5× düşer |
| [ ] T-M10-03 | Miracast sender (WFD + P2P) | TV'ye göre değişken; zor |
| [ ] T-M10-04 | AirPlay sender (RTP/H.264 + ALVED) | Apple TV ve AirPlay alıcılı TV'ler |
| [x] T-M10-05 | Kayıt: aynı akıştan MKV/WebM/fMP4 dosyası (`--record <yol>`) | "paylaş ve kaydet" |

## 10. Ses sorunu — kök neden ve çözüm haritası (AC-3)

Belirti: *"sekme değiştirince ses gidiyor; Spotify açıkken çok saçma."*

| # | Kök neden | Çözüm | Görev |
|---|---|---|---|
| 1 | Çıkış cihazı değişince (kablo→Bluetooth) **monitor kaynağı değişiyor**; ffmpeg eski kaynağı dinliyor | `pactl subscribe` ile izle, yeni monitor'u bul, ses girdisini yeniden bağla | T-M2-02 |
| 2 | Kaynak **SUSPENDED** (ses yokken) → pulse girdisi takılıyor / XRUN | `aresample=async=1:first_pts=0`, `thread_queue_size 4096`, `-err_detect ignore_err` | T-M2-03 |
| 3 | Görüntü+ses akışında **zamanlama kayması** | `-map` ile açık eşleme, ayrı thread kuyrukları | T-M2-03 |
| 4 | `suspended→running` geçişinde **ilk 200 ms sessiz** | Geçişte ses akışını yeniden başlat, boşluğu tolere et | T-M2-02 |
| 5 | Bluetooth codec gecikmesi (AAC ~200 ms, SBC ~300 ms) | PulseAudio'da `aac`/`opus` önceliği; yoksa kabul et | T-M2-02 |
| 6 | (İleri seviye, **varsayılan kapalı**) `module-null-sink` + `module-remap-source` ile daima açık hat | `--audio-loopback` bayrağı | T-M2-02 |

## 11. Kalite merdiveni (kesin değerler — tek kaynak)

| Preset | Çözünürlük | fps | Video bitrate | GOP | Ses | Toplam bant |
|---|---|---|---|---|---|---|
| `480p` | 854×480 | 30 | 1.5 Mb/s | 1.0 sn | 96 kb/s AAC | ~2 Mb/s |
| `540p` | 960×540 | 30 | 2.5 Mb/s | 1.0 sn | 96 kb/s | ~3 Mb/s |
| `720p` | 1280×720 | 30 | 4 Mb/s | 0.75 sn | 128 kb/s | ~5 Mb/s |
| `1080p` | 1920×1080 | 30 | 8 Mb/s | 0.5 sn | 160 kb/s | ~10 Mb/s |
| `1440p` | 2560×1440 | 30 | 14 Mb/s | 0.5 sn | 192 kb/s | ~17 Mb/s |
| `auto` | ölçüme göre | 30 | — | — | — | — |

**Gecikme = GOP/2 + kodlama + taşıma + render.** Yüksek kalitede düşük gecikme için GOP küçülür.
NVENC: `-preset p1 -tune ll -rc vbr -zerolatency 1 -bf 0`.

## 12. Riskler

| Risk | Etki | Azaltma |
|---|---|---|
| Cast protokolü ters-dönmüş dokümana dayanır | M4 takvimi kayar | protobuf alanları elle doğrulanır; mock sunucu testi yazılır |
| TV'lerin HLS desteği değişken | Cast/DLNA'da ses/uyumsuzluk | progressive MP4 fallback + anlaşılır uyarı |
| PipeWire/portal GNOME sürümlerinde değişiyor | M6 riskli | X11 yolu her zaman çalışır; portal sürümü sabitlenir |
| NVENC sürücüsü eski/kapalı | hız düşer | VAAPI → libx264 otomatik fallback |
| "Her protokol" kapsamı şişer | bitirme engeli | MVP: Browser+Cast+DLNA; Miracast/AirPlay Dalga 5'e |
| Çoklu ajan çakışması | bozuk kod | dosya sahipliği + dondurulmuş çekirdek sözleşme (EXECUTION.md §1) |
