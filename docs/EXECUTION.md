# ekran — Çoklu Ajan Çalıştırma Protokolü (v1.0)

> Bu proje **tek başına da**, **birden çok ajanla paralel de** yürütülebilir.
> Bu dosya, "şunu yap" dendiğinde hangi ajanın hangi sırayla neyi yapacağını tanımlar.
> Her ajan **yalnızca kendi dosyalarına** dokunur; çakışma olursa sorun çözülmez, kural uygulanır.

---

## 1. Altın kurallar (önce bunlar)

1. **Sözleşme önce.** `ekran-core` (tipler + trait'ler) Wave 0'da bitirilir ve **dondurulur**.
   Başka ajanlar kendi trait'lerini `ekran-core`'a eklemez; ihtiyaç bildirir.
   *(Dondurma: `ekran-core` için PR onayı olmadan değişiklik kabul edilmez.)*
2. **Tek dosya sahibi.** Her ajan yalnızca kendi crate'ine ait dosyaları düzenler.
   Ortak dosya (`Cargo.toml`, `docs/`) değişiklikleri **entegrasyon ajanının** işidir.
3. **Her görev tek commit.** Commit mesajında görev ID'si: `feat(sinks): cast handshake [T-M4-02]`.
4. **Test yeşil olmadan PR yok.** `cargo test --workspace` + `cargo clippy --workspace -- -D warnings`.
5. **Testler ağa çıkmaz.** Canlı cihaz testleri `#[ignore]` + `docs/TEST-PLAN.md` checklist'ine yazılır.
6. **Bir ajan bir dal.** Dal adı: `agent/<rol>/<görev-id>` (örn. `agent/sinks/T-M4-02`).

## 2. Kaç ajan?

| Ajan sayısı | Nasıl | Ne zaman |
|---|---|---|
| 1 | Sıralı: Wave 0 → 1 → 2 → … | Zaman az, bağlam bol |
| 2–3 | Wave 0 sonrası 2–3 dal aynı anda | Önerilen başlangıç |
| 4–6 | Rol bazlı tam paralel | En verimli |
| 7–8 | Tüm roller eşzamanlı | Sadece Wave 1 sonrası; entegrasyon riski yüksek |

**Tavsiye: 4 ajan** (A: core+transport, B: capture+encode, C: sinks, D: ui). 4'te çakışma yönetimi
kolay, iş yükü dengeli, her ajanın bağlamı rahat kalır.

> Token/bağlam notu: 8 ajan = 8 ayrı bağlam = 8× token. Her ajan **yalnızca kendi görev
> dosyalarını** okumalı; `docs/PLAN.md` içindeki görev satırı + ilgili bölüm yeterlidir.

## 3. Dalga planı (bağımlılık grafiği)

```
Wave 0 (tek ajan, zorunlu ön adım — ~1 gün)
  T-M0-01 workspace          T-M0-02 core tipler      T-M0-06 trait sözleşmeleri
  T-M0-03 doctor             T-M0-04 config            T-M0-05 selftest iskelet
        │  (ekran-core DONDURULUR)
        ▼
Wave 1 (paralel, 4 ajan)
  A: transport  T-M1-04 T-M1-05 T-M1-06        (B ile eşzamanlı)
  B: capture/encode T-M1-01 T-M1-02 T-M1-03
  C: audio      T-M2-01 T-M2-02
  D: web oynatıcı T-M1-07 T-M1-08
        │
        ▼
Wave 2 (paralel)
  A: M2 uyarlamalar (T-M2-03 T-M2-04 T-M2-05)   E: TUI T-M3-03
  B: Wayland portal  T-M6-01..T-M6-04
  C: keşif  T-M3-01 T-M3-02 T-M3-05
  D: Cast sink  T-M4-01..T-M4-04
        │
        ▼
Wave 3 (paralel)
  A: kumanda X11+Wayland  T-M7-01..T-M7-05
  B: DLNA sink T-M5-01..T-M5-03
  C: TUI cihaz listesi + `share --to` T-M3-03 T-M3-04
  D: UI son tasarım T-M8-01 T-M8-02
        │
        ▼
Wave 4 (entegrasyon ajanı, tek kişi)
  T-M8-03 hata mesajları   T-M8-04 bench   T-M9-01..T-M9-05 paketleme/yayın
```

**Kritik yol:** Wave 0 → Wave 1(A,B,D) → Wave 3 → Wave 4. Wave 2'deki Cast/DLNA (C,D)
MVP'yi bloklamaz; sırayla bitirilir.

## 4. Roller ve dosya sahipliği

| Rol | Crate'ler | Sahibi olduğu dosyalar |
|---|---|---|
| **A** | `ekran-transport` | `crates/ekran-transport/**` |
| **B** | `ekran-capture`, `ekran-encode` | `crates/ekran-{capture,encode}/**` |
| **C** | `ekran-audio` | `crates/ekran-audio/**` |
| **D** | `ekran-sinks` | `crates/ekran-sinks/**` |
| **E** | `ekran-ui` (TUI) | `crates/ekran-ui/**` |
| **F** | web oynatıcı + varlıklar | `assets/player/**`, `crates/ekran-ui/assets/**` |
| **G** | `ekran-core` (sözleşme sahibi) | `crates/ekran-core/**` |
| **H** | CLI + entegrasyon + doküman | `crates/ekran-cli/**`, `docs/**`, `Cargo.toml`, `README.md` |

**Ortak dosya kuralı:** `Cargo.toml` (workspace), `README.md`, `docs/` → **yalnız H**.
Başka ajanlar yeni bağımlılık gerektiğinde `docs/PLAN.md` görev satırına not düşer ve
H ekler; ya da ajan kendi crate'inin `Cargo.toml`'unu değiştirebilir (crate'e özel dosya).

## 5. Ajan başına kopyala-yapıştır görev talimatı

> Bu blokları doğrudan alt ajana (subagent) ver. Her biri tek başına yetkilidir.

### Ajan G — "Sözleşme sahibi" (Wave 0, TEK ajan, diğerleri başlamadan)

```
ekran deposunda Wave 0'ı uygula. Dayanma noktası: docs/PLAN.md (T-M0-01..T-M0-06).
Kurallar:
- Önce docs/PLAN.md ve docs/ARCHITECTURE.md §1'i oku.
- Yalnızca crates/ekran-core/** ve docs/ARCHITECTURE.md §1'i düzenle; Cargo.toml'a ekran-core dışında
  bağımlılık ekleme.
- Sink, CaptureSource, Quality, StreamPlan, Stats, Error tiplerini dokümana birebir uygun yaz.
- her biri için 3-5 satır doc yorumu yaz; clippy temiz olmalı.
- Test yaz: Quality parse, gop, hata mesajları.
Bittiğinde: cargo test --workspace ve cargo clippy --workspace -- -D warnings yeşil olmalı.
Çıktı: değişen dosyaların listesi + test sonucu. Doküman değiştirdiysen PLAN.md'deki durumu güncelle.
```

### Ajan A — "Taşıma katmanı" (Wave 1)

```
ekran deposunda ekran-transport crate'ini uygula. Dayanma noktası: docs/PLAN.md T-M1-04..T-M1-06,
docs/ARCHITECTURE.md §5, legacy/prototype-go/internal/{ws,mp4,stream} (referans).
Kurallar:
- Yalnızca crates/ekran-transport/** dosyalarına dokun.
- WebSocket: RFC 6455 sunucu tarafı (binary medya + metin kontrol), token kontrolü, yavaş istemci koruması.
- fMP4 ayrıştırıcı: init segment (ftyp/moov) ve medya parçası (moof/mdat) ayrımı.
- Hub: 1 üretici -> N istemci; 2 sn'yi aşan kuyrukta parça at.
- Testler: eşzamanlı 20 istemci, ayrıştırıcı, hub. Ağa çıkma yok (localhost loopback serbest).
Çıktı: dosya listesi + test sonucu.
```

### Ajan B — "Yakalama ve kodlama" (Wave 1)

```
ekran reposunda ekran-capture ve ekran-encode crate'lerini uygula.
Dayanma noktası: docs/PLAN.md T-M1-01..T-M1-03, docs/ARCHITECTURE.md §2-§3, docs/PLAN.md §6 (kalite tablosu).
Kurallar:
- Yalnızca crates/ekran-capture/** ve crates/ekran-encode/** dosyalarına dokun.
- CaptureSource trait + TestSource(lavfi) + X11Source(x11grab). Wayland'ı M6'da yap, şimdi sadece
  "desteklenmiyor" hatası ver (Error::NoDisplay değil, PortalDenied ile bilgilendir).
- Kodlayıcı seçimi: h264_nvenc -> h264_vaapi -> libx264, otomatik.
- ffmpeg argüman üretimi: docs/PLAN.md §6'daki GOP/bitrate değerleri BİREBİR kullanılacak; snapshot testi yaz.
- Testler: argüman snapshot'ları, kalite tablosu sabitleri (PLAN.md §6 ile eşleştiğini doğrulayan test).
Çıktı: dosya listesi + test sonucu.
```

### Ajan C — "Ses" (Wave 1)

```
ekran reposunda ekran-audio crate'ini uygula. Dayanma noktası: docs/PLAN.md T-M2-01, T-M2-02 ve §7 (ses sorunu tablosu).
Kurallar:
- Yalnızca crates/ekran-audio/** dosyalarına dokun.
- Varsayılan sink'in monitor kaynağını çöz (pactl list short sinks/sources).
- SourceWatcher: `pactl subscribe` ile sink/source değişimini izle, AudioSourceChanged olayı üret.
- Değişim anında: eski kaynağı bırak, yenisine bağlan, olay yayınla.
- pactl yoksa graceful: "ses kaynağı bulunamadı" uyarısı, çökmeyle.
- Testler: mock `pactl` betiği yazıp (fake binary) değişim senaryosunu test et (ağ yok).
Çıktı: dosya listesi + test sonucu.
```

### Ajan F — "Web oynatıcı" (Wave 1)

```
ekran reposunda TV tarafı web oynatıcıyı uygula. Dayanma noktası: docs/DESIGN.md (tamamı),
docs/PLAN.md T-M1-07, T-M1-08, docs/ARCHITECTURE.md §7.
Kurallar:
- Yalnızca assets/player/** dosyalarına dokun.
- MSE: init segment ilk mesaj, sonra medya parçaları; live-edge modu (buffered < 0.3 sn).
- Gecikme ölçümü: PTS ↔ performance.now, rozet; 400 ms üstü sarı, 700 ms üstü kırmızı.
- Kumanda modu: TV'den gelen işaretçi için sanal imleç overlay, ok tuşlarıyla 12 px adım.
- Tasarım: docs/DESIGN.md §2-§6 token'larını ve durumlarını BİREBİR uygula.
- Ağ çağrısı yok; tüm veri WS üzerinden gelir. Kontrol mesajları JSON: {"t":"move","x":0.5,"y":0.4}.
Çıktı: dosya listesi + kısa kullanım notu.
```

### Ajan D — "Cast & DLNA sink" (Wave 2)

```
ekran reposunda ekran-sinks crate'ini uygula. Dayanma noktası: docs/PROTOCOLS.md §2 ve §3,
docs/PLAN.md T-M3-01..T-M3-05, T-M4-01..T-M4-05, T-M5-01..T-M5-03.
Kurallar:
- Yalnızca crates/ekran-sinks/** dosyalarına dokun.
- Sink trait (ekran-core'da) uygulanır; markaya özel kod YAZMA, sadece SinkKind'e göre.
- Keşif: mDNS (_googlecast._tcp, _airplay._tcp) + SSDP (MediaRenderer, WifiDisplay). 5 sn tarama.
- Cast: prost ile protobuf, TLS 8009, CONNECT/PING/PONG, Media Receiver'a HLS URL'i LOAD.
- DLNA: SSDP NOTIFY + device.xml + content.xml (DIDL-Lite), canlı URL.
- HLS üretimi transport katmanında; sink'lar HLS URL'ini kullanır.
- Testler: mock Cast sunucusu (yerel TLS), mock SSDP yanıtlayıcı, keşif listeleme testi.
Çıktı: dosya listesi + test sonucu + "gerçek TV'de doğrulanacak" maddeleri.
```

### Ajan H — "Entegrasyon ve paketleme" (Wave 4)

```
ekran reposunda entegrasyon, CLI ve paketlemeyi tamamla. Dayanma noktası: docs/PLAN.md M3-M4,
docs/ARCHITECTURE.md §5 (HTTP API), docs/AGENTS.md kuralları.
Kurallar:
- crates/ekran-cli/**, docs/**, Cargo.toml ve README.md senin.
- `ekran share --to <id>` cihaz keşfi + otomatik sink seçimi (docs/PROTOCOLS.md §7).
- `ekran doctor --bench|--idle|--discover|--audio-switch` bayrakları gerçek ölçüm yapsın.
- Hata mesajları Türkçe ve eylem önerili olsun (docs/ARCHITECTURE.md §9).
- CI: fmt, clippy, test, musl çapraz derleme, .deb + flatpak.
- Tüm dokümanlarda PLAN.md durumlarını güncelle.
Çıktı: dosya listesi + test sonucu + kalan risk listesi.
```

## 6. Doğrulama kapıları (dalga kapıları)

| Dalga | Geçiş koşulu (hepsi sağlanmalı) |
|---|---|
| 0 → 1 | `cargo build --workspace` temiz; `ekran-core` trait'leri hazır ve dondurulmuş |
| 1 → 2 | `cargo test --workspace` yeşil; `ekran selftest` "SELFTEST OK"; MSE akışı tarayıcıda görünür |
| 2 → 3 | Cast/DLNA en az biri gerçek TV'de doğrulandı (TEST-PLAN E1/E2) veya gerekçeli olarak ertelendi |
| 3 → 4 | Kumanda çalışıyor (TEST-PLAN F1–F4); gecikme ölçümü AC-1'i doğruluyor |
| 4 → yayın | `cargo install --path crates/ekran-cli` temiz makinede çalışıyor; `.deb` kuruluyor; README güncel |

## 7. Çakışma ve kurtarma

- **Aynı dosyaya iki ajan dokunduysa:** dosya sahibi kuralı uygulanır, sahibi olan ajanın
  değişikliği korunur, diğeri geri alınır (`git checkout <dosya>`).
- **Wave 0 sonrası `ekran-core` değişikliği gerekirse:** durumu `docs/PLAN.md`'ye "DONDURMA
  KALDIRMA İSTEĞİ" notuyla yaz; G rolü değerlendirir ve gerekçeyle günceller.
- **Dalga testi kırılırsa:** dalga durur, ilgili ajan düzeltir; diğer ajanlar kendi işlerine
  devam eder (bağımsız dallar).
- **Plan eksik çıkarsa:** yeni görev `T-Mx-yy` kimliğiyle `docs/PLAN.md`'ye eklenir, sonra kod.
  Kimliksiz iş yapılmaz.

## 8. Token ve bağlam disiplini

- Her ajan önce **kendi görev satırlarını** okur (PLAN.md), sonra ilgili mimari bölümünü.
  Tüm `docs/` dosyasını birden okumak gerekmez.
- `docs/PLAN.md` ~200 satırda kalacak şekilde tutulur; ayrıntılar alt dokümanlara taşınır.
- Ajan çıktısı: **dosya listesi + test sonucu + 3 cümle özet**. Uzun log kopyalanmaz.
- Tekrar eden bilgi (crate yapısı, kalite tablosu) tek kaynaktan (bu dokümanlar) okunur;
  ajanlar kendi kopyalarını tutmaz.

## 9. Tek ajanla çalıştırma (sıralı plan)

Wave sırasıyla tek tek: **0 → 1(A,B,C,F) → 2 → 3 → 4**.
Tek ajan, Wave 1'de A rolünü, sonra B, C, F sırasıyla üstlenir.
Tavsiye edilen ajan sayısı 4'tür; 1 ajan ~2 kat yavaş, 8 ajan ~2,5 kat risklidir.
