# ekran — Mimari (v1.0)

Rust workspace. Her crate bağımsız test edilebilir; asıl uygulama `ekran-cli` binary'sidir.

```
ekran/
├── Cargo.toml                 # workspace
├── crates/
│   ├── ekran-core/            # paylaşılan tipler, trait'ler, hata tipleri
│   ├── ekran-capture/         # CaptureSource: x11 | wayland(portal) | test
│   ├── ekran-encode/          # kalite merdiveni, ffmpeg/NVENC argüman üretimi, adaptif kontrol
│   ├── ekran-audio/           # PulseAudio/PipeWire kaynak çözümleme + değişim takipçisi
│   ├── ekran-transport/       # HTTP + WebSocket sunucu, fMP4 ayrıştırma, hub, latency ölçümü
│   ├── ekran-sinks/           # Sink trait + Cast | DLNA | Browser | (Miracast | AirPlay) + keşif
│   ├── ekran-ui/              # ratatui TUI + gömülü web varlıkları (Apple tarzı oynatıcı)
│   └── ekran-cli/             # clap tabanlı binary: share, doctor, discover, selftest, config
├── assets/
│   ├── player/                # index.html, app.js, style.css (Web oynatıcı)
│   └── receiver/              # (opsiyonel) Cast custom receiver sayfası
├── docs/
│   ├── PLAN.md                # görevler + AC (normatif)
│   ├── ARCHITECTURE.md        # bu dosya
│   ├── PROTOCOLS.md           # sink protokol spesifikasyonları
│   └── TEST-PLAN.md           # gerçek cihaz doğrulama checklist'i
└── tests/                     # entegrasyon testleri, sahte (fake) sink
```

## 1. ekran-core — paylaşılan sözleşmeler

```rust
/// Yayın planı: kalite, çözünürlük, ses ve kısıtlamalar.
pub struct StreamPlan {
    pub quality: Quality,
    pub audio: bool,
    pub max_fps: u32,
    pub target_bitrate_kbps: u32,
    pub fragment_duration_ms: u64,
}

pub enum Quality { P480, P540, P720, P1080, P1440, Auto }
impl Quality {
    pub fn width(&self) -> u16;  // 854, 960, 1280, 1920, 2560
    pub fn height(&self) -> u16; // 480, 540, 720, 1080, 1440
    pub fn video_bitrate(&self) -> &str;   // "1.5M" ... "14M"
    pub fn gop_frames(&self) -> u32;      // GOP = gop_frames / fps
    pub fn audio_bitrate(&self) -> &str;  // "96k" ... "192k"
    pub fn parse(s: &str) -> Result<Quality, Error>;
}

/// Sink türleri — marka bağımsız protokol soyutlaması.
pub enum SinkKind { Browser, Cast, Dlna, Miracast, AirPlay }

pub struct DeviceInfo {
    pub id: String,
    pub name: String,
    pub kind: SinkKind,
    pub address: String,
    pub port: u16,
    pub model: String,
    pub capabilities: Capabilities,
}

pub struct Capabilities {
    pub live_video: bool,
    pub audio: bool,
    pub remote_input: bool,   // kumanda kanalı var mı
    pub native_player: bool,  // TV'nin kendi oynatıcısı mı
}

/// Sink sözleşmesi — her protokol buraya uyar.
#[async_trait::async_trait]
pub trait Sink: Send + Sync {
    fn kind(&self) -> SinkKind;
    /// Verilen planla hazırlık (ör. Cast: CONNECT, DLNA: tanıtım).
    async fn prepare(&self, plan: &StreamPlan) -> Result<PreparedSink>;
    /// Yayını başlat.
    async fn start(&self, sink: PreparedSink) -> Result<()>;
    /// Yayını durdur ve kaynakları bırak.
    async fn stop(&self) -> Result<()>;
    /// Sink canlı mı?
    async fn is_alive(&self) -> bool { true }
}
```

**Kural:** Hiçbir crate `ekran-core` dışındaki crate'e bağımlı olmaz (bağımlılık yönü tek yönlü: cli → {ui, sinks, transport, capture, encode, audio} → core).

## 2. ekran-capture — ekranı kareye çevirme

```rust
#[async_trait::async_trait]
pub trait CaptureSource: Send {
    /// Ekranı ffmpeg/gstreamer girdisi olarak tanımlar.
    fn describe(&self) -> CaptureDescriptor;
    /// Üretici sürecini başlatır; stdout fMP4 taşır.
    async fn spawn(&self) -> Result<CaptureProcess>;
    /// Mevcut monitörleri listeler.
    fn monitors(&self) -> Result<Vec<Monitor>>;
}

pub struct CaptureDescriptor {
    pub backend: &'static str,     // "x11" | "wayland" | "test"
    pub display: String,
    pub monitor: Option<Monitor>,
    pub note: Option<String>,      // "Wayland: portal gerekli"
}
```

Uygulamalar:
- `TestSource` — lavfi `testsrc2`; CI ve selftest için.
- `X11Source` — ffmpeg `x11grab` (hızlı yol) **veya** M10'da `x11rb` tabanlı kendi motorumuz.
- `WaylandSource` — `zbus` ile `org.freedesktop.portal.ScreenCast`: CreateSession → SelectSources → Start; dönen unix fd'yi `gst-launch-1.0 pipewiresrc fd=N ! ... ! mp4mux` sürecine veririz (derleme bağımlılığı yok, runtime'da gst gerekir).

## 3. ekran-encode — kodlama ve akıllı kalite

Sorumluluklar:
1. **Kodlayıcı seçimi**: `h264_nvenc` → `h264_vaapi` → `libx264` (otomatik, `--codec` ile ezilebilir).
2. **Argüman üretimi**: kalite merdiveni + GOP + `-force_key_frames`.
3. **Adaptif kontrol**: istemciden gelen hız ölçümü → merdiven seçimi (histerezis: yükseltme eşiği %20, düşürme eşiği %15).
4. **Boş ekran algılama**: ikinci hafif yakalama (64×36) veya encoder'dan `-vf` ile küçültülmüş kopya; kare imzası (FNV-1a) 3 sn değişmezse `idle` modu: fps 30→1, bitrate → 300 kb/s.

fMP4 çıktı biçimi (ffmpeg, pipe'a yazılır):

```
-f h264_nvenc -preset p1 -tune ll -rc vbr -zerolatency 1
-b:v 8M -maxrate 8M -bufsize 16M -g 15 -bf 0
-force_key_frames expr:gte(t,n_forced*0.5)
-movflags +frag_keyframe+empty_moov+default_base_moof+omit_tfhd_offset
-frag_duration 500000 -f mp4 pipe:1
```

## 4. ekran-audio — sistem sesi

```rust
pub struct AudioSource { pub device: String, pub kind: AudioKind }
pub enum AudioKind { Monitor, Mic, Custom }

/// Varsayılan çıkışın monitör kaynağını çözer.
pub fn resolve_default_monitor() -> Result<AudioSource>;
/// Sink/cihaz değişimini izler; kaynak değişince bildirir.
pub struct SourceWatcher { /* pactl subscribe tabanlı */ }
```

Akış: `ekran-audio` bir **olay** üretir (`AudioSourceChanged`), `ekran-cli` bunu alır ve ses girdisini yeniden bağlar (ffmpeg sürecini yeniden başlatmadan mümkünse; ffmpeg girdileri değişmezse süreç yeniden başlatılır — 1–2 sn'lik bir kesinti kabul edilir).

## 5. ekran-transport — sunucu

Rotalar:

| Yol | Açıklama |
|---|---|
| `GET /` | Oynatıcı sayfası (Apple tarzı) |
| `GET /player.js`, `/style.css` | Statik varlıklar |
| `GET /ws?token=…` | WebSocket: **ikili** = fMP4 parçası, **metin** = kumanda olayı |
| `GET /api/info` | Plan, kalite, cihazlar, sürüm (JSON) |
| `GET /api/stats` | Anlık istatistik (JSON) |
| `GET /api/discover` | Keşfedilen cihazlar (JSON) |
| `GET /hls/stream.m3u8` | HLS (Cast/DLNA sink'ları ve eski tarayıcılar için) |
| `GET /healthz` | Sağlık (herkese açık) |

fMP4 akışı: `moov`/`ftyp` → **init segment** (her istemciye bir kez), sonra `moof`+`mdat` → **medya parçası** (anında yayınlanır).

Yavaş istemci koruması: istemci gönderim kuyruğu 2 sn'yi aşarsa parça atılır (istemci zaten geç kalıyor).

## 6. ekran-sinks — protokoller ve keşif

```rust
pub struct Discovery { pub devices: Vec<DeviceInfo> }

/// mDNS: _googlecast._tcp, _airplay._tcp, _raop._tcp
pub struct MdnsScanner { ... }
/// SSDP: M-SEARCH → MediaRenderer, WifiDisplay
pub struct SsdpScanner { ... }

pub enum SinkFactory { fn build(device: &DeviceInfo, plan: &StreamPlan) -> Box<dyn Sink> }
```

Cast/DLNA sink'ları HLS URL'ini kullanır (TV'nin kendi oynatıcısı), Browser sink ise WebSocket/MSE'yi kullanır.

## 7. ekran-ui — iki arayüz

**Terminal (Linux tarafı, ratatui):**
- Cihaz listesi (isim, protokol türü, IP, durum), ok tuşlarıyla seçim
- Canlı istatistik: fps, bitrate, istemci sayısı, gecikme, kalan CPU
- Kısayollar: `q` çık, `p` duraklat, `m` ses, `f` tam ekran bilgisi, `s` ekran görüntüsü

**Web oynatıcı (TV tarafı):**
- Siyah zemin, buzlu cam kontrol paneli, gizlenen üst bar, yumuşak geçişler
- MSE tabanlı oynatma, "canlı kenar" (live edge) modu → tampon < 0.3 sn
- Gecikme rozeti, kalite göstergesi, kumanda modu toggle, sanal imleç
- Kumanda tuşları: `F` tam ekran, `C` kumanda, `M` ses, `Space` oynat/duraklat

## 8. Konfigürasyon (`~/.config/ekran/config.toml`)

```toml
schema_version = 1

[share]
quality = "auto"          # 480p | 540p | 720p | 1080p | 1440p | auto
fps = 30
audio = true
port = 7700
token = ""                # boşsa otomatik üretilir
bind = "0.0.0.0"

[capture]
backend = "auto"          # auto | x11 | wayland | test
monitor = "primary"

[control]
enabled = true
backend = "auto"          # auto | xdotool | portal

[adaptive]
idle_seconds = 3
idle_fps = 1
idle_bitrate = "300k"
up_threshold = 0.20
down_threshold = 0.15
```

## 9. Hata modeli

Tüm hatalar `ekran_core::Error` üzerinden; her varyant **Türkçe kullanıcı mesajı** taşır ve `ekran-cli` bunları tek satırda, aksiyon önerisiyle gösterir:

```rust
pub enum Error {
    NoDisplay,
    PortalDenied,
    EncoderUnavailable { tried: Vec<String> },
    NoSinkFound,
    SinkRefused(String),
    AudioDeviceLost(String),
    ConfigInvalid { field: String, hint: String },
    Io(std::io::Error),
}
```

---

## 10. HTTP API sözleşmesi (sabit)

| Yöntem | Yol | Gövde / Yanıt | Not |
|---|---|---|---|
| GET | `/` | HTML oynatıcı | `assets/player/index.html` |
| GET | `/player.js`, `/style.css`, `/hls.min.js` | statik | içerik tipi doğru |
| GET | `/ws?token=…` | WebSocket | token yanlışsa 401 |
| GET | `/api/info` | JSON `{version, plan, monitors, devices, backend, startedAt}` | şema sabit |
| GET | `/api/stats` | JSON `{clients, fragments, bytes, fps, bitrateKbps, latencyMs, idle}` | 1 sn aralıkla |
| GET | `/api/discover` | JSON `{devices:[DeviceInfo]}` | `--json` çıktısıyla aynı şema |
| GET | `/hls/stream.m3u8` | HLS playlist | Cast/DLNA sink'ları bunu okur |
| GET | `/hls/seg*.ts` | segment | HLS |
| GET | `/snap.jpg` | JPEG anlık görüntü | `ekran` TUI kısayolu |
| GET | `/healthz` | `ok` | yetki gerekmez |

**Token:** `StreamPlan.token` boşsa sunucu başlarken 16 karakterlik rastgele token üretir ve
terminalde gösterir. `/` dışındaki tüm rotalar token ister (`/healthz` hariç).

**Hata biçimi (JSON):**
```json
{"error":"SinkRefused","message":"Hedef cihaz bağlantıyı reddetti: ...","hint":"TV'nin Cast desteğini kontrol edin"}
```
`hint` alanı her hatta **eylem önerisi** içerir (ARCHITECTURE §9).

## 11. Crate içi dosya düzeni (birebir)

```
crates/ekran-core/src/
  lib.rs        # Quality, StreamPlan, SinkKind, DeviceInfo, Capabilities, Monitor, Backend, Stats
  traits.rs     # Sink, PreparedSink, CaptureSource, Control, Discovery
  error.rs      # Error + Turkish messages + retry policy
  config.rs     # config.toml şeması (schema_version = 1)

crates/ekran-capture/src/
  lib.rs        # mod tanımları
  source.rs     # CaptureSource trait uygulamaları
  x11.rs        # X11Source (ffmpeg x11grab)
  wayland.rs    # WaylandSource (portal; Dalga 2)
  test.rs       # TestSource (lavfi)

crates/ekran-encode/src/
  lib.rs
  plan.rs       # kalite → ffmpeg argümanları (PLAN §6)
  codec.rs      # NVENC/VAAPI/libx264 seçimi
  adaptive.rs   # auto kalite + boş ekran algılama

crates/ekran-audio/src/
  lib.rs
  source.rs     # monitor kaynağı çözümleme
  watcher.rs    # pactl subscribe tabanlı değişim izleyici

crates/ekran-transport/src/
  lib.rs
  http.rs       # HTTP rotaları + statik dosyalar
  ws.rs         # WebSocket (RFC 6455)
  mp4.rs        # fMP4 kutu ayrıştırıcı
  hub.rs        # yayın hub'ı + istemci koruması
  hls.rs        # HLS üretimi (ffmpeg -f hls)
  api.rs        # JSON tipleri

crates/ekran-sinks/src/
  lib.rs
  mdns.rs       # mDNS tarayıcı
  ssdp.rs       # SSDP tarayıcı + NOTIFY
  cast.rs       # Google Cast sink
  dlna.rs       # DLNA sink
  browser.rs    # tarayıcı sink (yalnız URL + kontrol)
  select.rs     # otomatik sink seçimi (PROTOCOLS §7)

crates/ekran-ui/src/
  lib.rs
  tui.rs        # ratatui arayüzü (DESIGN §7)
  assets.rs     # assets/player gömme

crates/ekran-cli/src/
  main.rs       # clap komutları
  doctor.rs     # doctor + --bench/--idle/--audio-switch/--quality
  share.rs      # share --to/--quality/--audio
  discover.rs   # discover --json
  selftest.rs   # selftest
```

## 12. Hata modeli ve yeniden deneme

| Hata | Yeniden deneme | Kullanıcı davranışı |
|---|---|---|
| `NoDisplay` | hayır | "X11/Wayland oturumu bulunamadı" + "Xorg oturumuna geç" önerisi |
| `PortalDenied` | kullanıcı tekrar denerse | "İzin verilmedi" + "portal penceresinde tekrar izin ver" |
| `EncoderUnavailable` | hayır | "h264_nvenc yok → VAAPI → libx264 denendi" zinciri raporlanır |
| `NoSinkFound` | 5 sn sonra tekrar tara | keşif ipucu verilir |
| `SinkRefused` | 3 kez, 1/2/4 sn arayla | TV'nin Cast/DLNA desteği kontrol edilir |
| `AudioDeviceLost` | sürekli (watcher) | ses kaynağı yeniden bağlanır, kullanıcı bilgilendirilmez |
| `ConfigInvalid` | hayır | alan + geçerli değerler listelenir |

Tüm hatalar `tracing` ile `warn!` seviyesinde loglanır (kullanıcıya gösterilmez, `ekran doctor`
ile görülebilir).
