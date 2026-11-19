# ekran

Linux masaüstünü ev ağındaki TV'ye **canlı ve ölçülebilir düşük gecikmeyle** yansıt.
TV'ye **hiçbir uygulama kurulmaz**: TV'nin desteklediği yerleşik protokol (Google Cast, DLNA,
Miracast, AirPlay) veya TV'nin tarayıcısı otomatik seçilir. TV ayrıca kumanda cihazına dönüşür.

Türkçe · Rust · tek binary · bulut yok · hesap yok · NVENC donanım kodlama

---

## Dokümanlar (okuma sırasıyla)

| # | Dosya | İçerik |
|---|---|---|
| 1 | [`docs/PLAN.md`](docs/PLAN.md) | **Normatif plan**: 5 dalga, ~60 görev (`T-Mx-yy`), kabul kriterleri (AC-1..AC-10), kalite tablosu, riskler |
| 2 | [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) | 8 crate, Rust trait imzaları, HTTP API sözleşmesi, dosya düzeni, hata modeli |
| 3 | [`docs/PROTOCOLS.md`](docs/PROTOCOLS.md) | Cast (protobuf şemasıyla), DLNA (XML şablonlarıyla), Miracast, AirPlay, MSE — tel düzeyinde |
| 4 | [`docs/DESIGN.md`](docs/DESIGN.md) | Apple tarzı arayüz: renk/ölçü/hareket token'ları, oynatıcı anatomisi, durumlar, TUI düzeni |
| 5 | [`docs/EXECUTION.md`](docs/EXECUTION.md) | **Paralel ajan çalıştırma**: roller, dalga planı, kopyala-yapıştır ajan talimatları, doğrulama kapıları |
| 6 | [`docs/TEST-PLAN.md`](docs/TEST-PLAN.md) | Otomatik test listesi + gerçek TV doğrulama checklist'i (A–H) |
| — | [`docs/DECISIONS.md`](docs/DECISIONS.md) | Alınmış kararlar ve gerekçeleri |
| — | [`AGENTS.md`](AGENTS.md) | **Bu depoda çalışan her AI/person için kurallar** (okuma sırası, tuzaklar, DoD) |

> "ekranı yap" diyebilirsin: `AGENTS.md` + `docs/` yeterli. 4 ajanla paralel yürütme planı
> `docs/EXECUTION.md` §3'te hazır.

## Durum

| Dalga | İçerik | Durum |
|---|---|---|
| 0 | Rust workspace (8 crate), `ekran-core` sözleşmeleri, `ekran doctor` | ✅ tamamlandı |
| 1 | Yakala → NVENC → fMP4 → MSE oynatıcı + ses watcher | ✅ tamamlandı |
| 2 | Akıllı kalite, keşif, Cast sink, Wayland portal | ✅ tamamlandı |
| 3 | Kumanda, DLNA, TUI cihaz listesi | ✅ tamamlandı |
| 4 | Entegrasyon, ölçüm, paketleme (`.deb` / Flatpak / CI) | ✅ tamamlandı (gerçek TV testi beklemede) |
| 5 | Derin yol: `--record` (`T-M10-05` ✅); x11rb/Miracast/AirPlay | 🟡 opsiyonel |

## Hızlı başlangıç (kurulum sonrası)

```bash
ekran doctor                      # ortam kontrolü (oturum, ekran, ağ, araçlar, ses, codec)
ekran discover                    # ağdaki TV'leri listele (Cast/DLNA/Miracast)
ekran share --to <cihaz-id>       # seç ve yayınla (TV'de uygulama gerekmez)
ekran share --quality 1080p       # 480p|540p|720p|1080p|1440p|auto
ekran share --audio               # sistem sesiyle birlikte
ekran share --record yayin.mp4    # yayınlarken aynı anda kaydet
ekran config init                 # ~/.config/ekran/config.toml oluştur
ekran selftest                    # uçtan uca kendi kendine test
```

## Mimari özet

```
X11 (x11grab) ─┐
Wayland (portal)─┤   ┌── ekran (tek süreç, tek binary) ──┐
Test (lavfi) ───┘   │ capture → encode(NVENC) → stream     │
                     │   ├─ WebSocket + MSE  → tarayıcı (kumanda ✅, ~250-400 ms)
                     │   └─ HLS              → Cast/DLNA oynatıcısı (TV'de uygulama yok)
                     │ control: xdotool (X11) / RemoteDesktop portalı (Wayland)
                     │ audio: PulseAudio monitor + sink değişim takipçisi
                     │ ui: Apple tarzı web oynatıcı (TV) + ratatui TUI (Linux)
                     └──────────────────────────────────────────┘
```

## Lisans

MIT (bkz. `LICENSE`).
