# AGENTS.md — ekran (bu depoda çalışan herkes/AI için)

Bu depoda çalışan **herhangi bir yapay zekâ veya geliştirici** için zorunlu kurallar.
"ekranı yap" dendiğinde bu dosya + `docs/` altındaki 6 doküman yeterlidir.

## 0. Okuma sırası (kod yazmadan önce)

| Sıra | Dosya | Ne için |
|---|---|---|
| 1 | `docs/PLAN.md` | **Normatif**: görevler (`T-Mx-yy`), kabul kriterleri (AC), kapsam, kararlar |
| 2 | `docs/ARCHITECTURE.md` | crate yapısı, trait imzaları, HTTP API, dosya düzeni, hata modeli |
| 3 | `docs/PROTOCOLS.md` | sink protokolleri (Cast/DLNA/Miracast/AirPlay/MSE) tel düzeyinde |
| 4 | `docs/DESIGN.md` | arayüz tasarım sistemi (token'lar, durumlar, hareket, TUI) |
| 5 | `docs/EXECUTION.md` | **paralel ajan çalıştırma** (roller, dalgalar, kopyala-yapıştır talimatlar) |
| 6 | `docs/TEST-PLAN.md` | otomatik testler + gerçek cihaz doğrulama checklist'i |
| — | `docs/DECISIONS.md` | alınmış kararlar ve gerekçeler (değiştirmek isteyenler buraya yazar) |

**Ne kadar okumalı?** Tümünü okumak zorunlu değil: görev satırını + ilgili bölümü oku.
Örneğin "T-M1-04 ffmpeg argüman üretimi" için PLAN §11 ve ARCHITECTURE §3 yeterlidir.

## 1. Çalışma kuralları

- **Görev ID'si olmadan kod yazma.** PLAN.md'deki `T-Mx-yy` görevlerinden birini seç; yeni iş
  gerekiyorsa önce plana yeni bir görev ID'si ekle.
- **AC kırılamaz.** Kabul kriterleri değişecekse: gerekçe + etki + geri dönüş planı ile
  `docs/DECISIONS.md`'ye yaz, sonra PLAN.md'yi güncelle.
- **Bağımlılık yönü tek yönlüdür:**
  `ekran-cli → {ekran-ui, ekran-sinks, ekran-transport, ekran-audio, ekran-encode, ekran-capture} → ekran-core`.
  `ekran-core` hiçbir şeye bağımlı olamaz.
- **`ekran-core` dondurulur** (PLAN T-M0-07). Başka ajanlar trait sözleşmesini değiştiremez;
  ihtiyaç varsa `docs/DECISIONS.md`'ye "DONDURMA KALDIRMA İSTEĞİ" notu düşür.
- **Dosya sahipliği:** her crate'in dosyaları tek ajanın sorumluluğundadır (EXECUTION.md §4).
  Ortak dosyalar (`Cargo.toml`, `README.md`, `docs/`) yalnızca **Entegrasyon ajanının** (H).
- **Kırıcı değişiklikler:** `config.toml` şema sürümü (`schema_version`) artırılır; config alan
  adları ve CLI bayrak adları geriye dönük uyumlu kalır.
- **Dil:** arayüz metinleri/hata mesajları **Türkçe**; kod tanımlayıcıları, doc yorumları ve
  commit mesajları **İngilizce**.
- **Commit:** Conventional Commits + görev ID'si:
  `feat(sinks): cast handshake [T-M4-02]`, `fix(audio): rebind monitor on sink change [T-M2-02]`.
- **Testler ağa çıkmaz** (localhost loopback serbest). Canlı cihaz testleri `#[ignore]` edilir ve
  `docs/TEST-PLAN.md` checklist'ine yazılır.
- **Runtime bağımlılığı:** çalışma anında yalnızca `ffmpeg` gerekir; GStreamer yalnız Wayland
  portal yolunda. Yeni runtime bağımlılığı eklenirse PLAN §12 risk tablosuna yazılır.

## 2. Arayüz kuralları

- Yeni bir renk/ölçü/hareket eklenmeden önce `docs/DESIGN.md` güncellenir.
- Oynatıcıdaki her durum (bağlanıyor, canlı, buffering, duraklatıldı, izin gerekli, hata)
  DESIGN §6.2'deki görsel karşılığa uymak zorundadır.
- Token'lar (`--canvas`, `--accent`, …) `style.css` içinde tanımlıdır; güncellenmiş olmalıdır.
- TUI'de 80×24 terminalde her şey görünür olmalıdır.

## 3. Tuzaklar (önceden bilinen hatalar)

1. **Wayland'da `DISPLAY` var olsa bile `x11grab` ekranı göstermez** (sadece XWayland pencereleri,
   genelde siyah). Wayland tespit edilirse portal yolu kullanılır.
2. **PulseAudio monitor kaynağı** çıkış cihazı değişince ölür → `SourceWatcher` zorunlu.
3. **ffmpeg'e `pipe:1` ile MP4 yazarken** `+empty_moov+frag_keyframe` şart (çıktı seek edilebilir değil).
4. **MSE'de** ilk mesaj init segment'tir; istemci yeniden bağlanırsa sunucu init segment'i tekrar
   gönderir. Parça sırası bozulursa oynatıcı donabilir.
5. **HLS ayrı üretilir** (Cast/DLNA yolu); MSE/WS yolu HLS'i kullanmaz.
6. **NVENC yoksa** VAAPI, o yoksa libx264 kullanılır; `ekran doctor` hangisinin seçildiğini yazar.
7. **Cast protobuf alan numaraları** elle doğrulanmalı; mock sunucu testi yazılmadan gerçek TV'ye
   gidilmez.
8. **DLNA canlı HLS'i** çoğu TV reddeder → progressive MP4 fallback'i ve uyarı mesajı şart.

## 4. Görev tamamlama tanımı (Definition of Done)

Bir görev "bitti" sayılır ancak:
- [ ] kod yazıldı ve `cargo fmt` temiz
- [ ] `cargo clippy --workspace -- -D warnings` temiz
- [ ] ilgili testler var ve geçiyor (`cargo test --workspace`)
- [ ] PLAN.md içindeki görev `[x]` işaretli
- [ ] kullanıcıya görünen davranış değiştiyse README/DESIGN güncellendi
- [ ] commit mesajında görev ID'si var
- [ ] gerçek cihaz gerektiren doğrulama yapıldıysa TEST-PLAN.md checklist'i güncellendi

## 5. İletişim

Kullanıcı Türkçe konuşuyor; ilerleme raporları Türkçe, kısa ve somut olmalı
("ne çalışıyor, ne çalışmıyor, sırada ne var"). Belirsizlikte: **mantığı yazılı hale getir,
sonra soruyu sor.** Belirsizliği çözebiliyorsan çöz ve kararı belgele.
