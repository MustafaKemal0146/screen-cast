# ekran — Arayüz Tasarım Sistemi (v1.0)

> Bu dosya **normatiftir**: UI'daki her renk, ölçü, hareket ve bileşen burada tanımlıdır.
> Uygulayıcı (veya AI) yeni bir arayüz öğesi eklerken **önce bu dosyaya** ekler.
> TV tarafı web (HTML/CSS/JS), Linux tarafı terminal (TUI) — ikisi de aynı "dilbilgisi"ni paylaşır.

---

## 1. Tasarım ilkeleri

1. **Tek ekran, tek fikir.** Oynatıcıda video her şeydir; kontroller katman, panel değil.
2. **Sessiz zemin, güçlü odak.** Siyah zemin, tek renk vurgusu (mavi), gri metin. Renk sadece
   "şu anda ne oluyor" bilgisini taşır.
3. **Hareket amaçlı.** 120–220 ms, `ease-out`; dekoratif animasyon yok. `prefers-reduced-motion`
   desteklenir (TV'de dikkat dağıtıcı olmamalı).
4. **10 feet kuralı.** TV'de 2.5–4 m mesafeden okunacak: gövde metni ≥ 22 px (1080p ölçeğinde
   ≥ 32 px), dokunma/odak hedefi ≥ 48 px, kontrast ≥ 4.5:1.
5. **Kumanda önceliği.** Her etkileşim TV kumandasıyla yapılabilmeli (ok/Enter/Geri). Fare
   bir bonus, asıl yol değil.
6. **Sessiz mod.** Yayın sırasında klavye/fare girdisi konsola değil TV'ye gider; TV'de yazı
   yazmak isteniyorsa kumanda modu açılır.

## 2. Renk paleti (dark-first)

| Token | Değer | Kullanım |
|---|---|---|
| `--canvas` | `#000000` | Tam ekran video arka planı |
| `--surface-1` | `rgba(28,28,30,0.72)` | Kontrol paneli (buzlu cam) |
| `--surface-2` | `rgba(44,44,46,0.92)` | İkincil panel / menü |
| `--hairline` | `rgba(255,255,255,0.12)` | Ayırıcı çizgi |
| `--text-primary` | `#F5F5F7` | Başlık, aktif öğe |
| `--text-secondary` | `rgba(245,245,247,0.62)` |ikincil metin, rozet |
| `--text-tertiary` | `rgba(245,245,247,0.38)` | Zaman damgası, pasif |
| `--accent` | `#0A84FF` | Odak halkası, canlı göstergesi, birincil eylem |
| `--accent-dim` | `rgba(10,132,255,0.18)` | Odak dolgusu |
| `--live` | `#FF3B30` | "CANLI" noktası (nur kayıp cihazda kırmızı değil) |
| `--warn` | `#FF9F0A` | Buffer uyarısı, düşük bant uyarısı |
| `--ok` | `#30D158` | Bağlandı, ölçüm tamam |

**Buzlu cam (frosted glass) reçetesi:** `backdrop-filter: blur(40px) saturate(180%)`,
arka plan `--surface-1`, 1 px `--hairline` kenarlık, üstte `inset 0 1px 0 rgba(255,255,255,0.06)`
highlight, gölge `0 12px 40px rgba(0,0,0,0.55)`.

**Renk körlüğü:** Canlı/uyarı durumları asla yalnız renkle anlatılmaz — her zaman ikon + metin eşlikler.

## 3. Tipografi

| Rol | Boyut (1080p referans) | Ağırlık | Satır aralığı | Not |
|---|---|---|---|---|
| Ekran başlığı | 34 px | 600 | 1.2 | `SF Pro Display` yığını |
| Gövde | 22 px | 400 | 1.45 | |
| Rozet / durum | 19 px | 500 | 1.2 | `font-variant-numeric: tabular-nums` |
| İkincil | 17 px | 400 | 1.4 | açıklama metni |

**Font yığını:** `-apple-system, "SF Pro Display", "SF Pro Text", "Segoe UI Variable", Inter,
system-ui, "Helvetica Neue", Arial, sans-serif`. TV'de font yoksa `system-ui` kullanılır; ölçüler
`clamp()` ile 720p→4K arası ölçeklenir (720p'de gövde 16 px'e iner).

## 4. Ölçü token'ları

| Token | Değer | Kullanım |
|---|---|---|
| `--s-1` | 4 px | İkon iç boşluk |
| `--s-2` | 8 px | Rozet iç boşluk |
| `--s-3` | 12 px | Buton iç boşluk (yatay) |
| `--s-4` | 16 px | Panel iç boşluk |
| `--s-5` | 24 px | Kart iç boşluk |
| `--s-6` | 32 px | Bölüm arası |
| `--radius-s` | 10 px | Rozet, küçük buton |
| `--radius-m` | 16 px | Panel, kart |
| `--radius-l` | 22 px | Buton (pill) |

## 5. Hareket

| Olay | Süre | Easing |
|---|---|---|
| Panel açılış/kapanış | 200 ms | `cubic-bezier(0.32, 0.72, 0, 1)` |
| Kontrol çubuğu gizlenmesi | 250 ms | aynı |
| Odak halkası | 120 ms | `ease-out` |
| Durum rozeti değişimi | 160 ms | `ease-in-out` |
| İmleç takibi (kumanda modu) | 0 ms (anında) | — |

`@media (prefers-reduced-motion: reduce)` → tüm geçişler 0 ms, imleç takibi yine anında.

## 6. Oynatıcı ekran anatomisi

Katmanlar (alttan üste):

```
┌──────────────────────────────────────────────────────────────┐
│ ① Video sahnesi (letterbox, radius 0, siyah)                │
│                                                              │
│      ┌──────────────────────────────┐                        │
│      │        <video>               │                        │
│      └──────────────────────────────┘                        │
│                                                              │
│ ② Kumanda imleci overlay (yarı saydam beyaz halka + halo)    │
│                                                              │
│ ③ Üst bar (auto-hide): ekran adı · kalite · gecikme rozeti  │
│                                                              │
│ ④ Alt kontrol paneli (auto-hide, buzlu cam)                  │
│    ▶/⏸   🔊   ⛶   🖱kumanda   ⋯(menü: kalite, ses, kayıt)  │
│                                                              │
│ ⑤ Toast / durum bildirimi (sağ üst, 3 sn)                  │
└──────────────────────────────────────────────────────────────┘
```

### 6.1 Kontrol paneli davranışı
- Fare hareketi, tıklama veya kumanda tuşu → panel **görünür**; 2.5 sn hareketsizlik → **gizlenir**.
- Video üzerinde çift tıkla → tam ekran ↔ pencere.
- `Esc` / kumanda "Geri" → tam ekrandan çıkar (panel açık kalır).
- Panel gizliyken video **tam boy** (kenar boşluğu yok) — Apple TV hissi.

### 6.2 Durumlar ve görsel karşılıkları

| Durum | Ekranda görünen | Süre |
|---|---|---|
| Bağlanıyor | Siyah ekran + ortada ince spinner (2 px çizgi, sonsuz döngü) + "Bağlanılıyor…" | — |
| Canlı | Sağ üstte `● CANLI` (kırmızı nokta, 1.2 sn nabız) + sol üstte cihaz adı | sürekli |
| Buffering | Video üstünde hafif karartma + "Ağ yavaş" uyarısı (yalnız > 1 sn) | olay bazlı |
| Duraklatıldı | Ortada büyük ▶ rozeti, kumanda ipucu | kalıcı |
| İzin gerekli | "Ekran paylaşım izni verilmedi" + "Tekrar dene" butonu | kalıcı |
| Hata (sink öldü) | "Yayın kesildi" + "Yeniden bağlan" | kalıcı |
| Çok düşük bant | Sağ üstte `-- 480p` uyarısı (otomatik kalite düştüyse) | 4 sn |

### 6.3 Kumanda (uzaktan kontrol) modu
- TV'den gelen işaretçi ekranda **yumuşak beyaz halka** (24 px) + 8 px halo olarak görünür.
- Sol tık → halka koyulaşır (basıldı), sağ tık → kırmızımsı ton.
- TV kumandası ok tuşları: halka 12 px adım atar, Enter = tıkla, Geri = sağ tık (uzun basış).
- `C` ile kumanda modu açılır/kapanır; kapalıyken kumanda TV'nin kendi menüsünde çalışır.

### 6.4 Rozetler (sağ üst)
- **Gecikme**: `240 ms` — 400 ms üstüyse sarı, 700 ms üstüyse kırmızı.
- **Kalite**: `1080p` — `auto` moddaysa `1080p (auto)` ve bağlantı zayıfladığında `↓ 720p`.
- **İstemci**: ev sahibi tarafında gösterilir, TV'de değil.

## 7. TUI (Linux tarafı, ratatui)

Düzen (80×24 varsayılan):

```
┌ ekran 0.1.0 ─────────────────────────────────────────────┐
│  cihazlar (5)                    yayın: 1080p · 6.2 Mb/s │
│  ▸ LG OLED (Chromecast)  10.9.46.60:8009    ● uygun      │
│    Samsung Crystal (DLNA) 10.9.46.61:8200    ● uygun      │
│    Fire TV (Miracast)     10.9.46.62:7231    ○ destek yok │
│    Tarayıcı modu (QR)      http://10.9.46.60:7700         │
├─────────────────────────────────────────────────────────┤
│  istatistik: 29 fps · 2 istemci · gecikme 240 ms · cpu %6 │
├─────────────────────────────────────────────────────────┤
│ ↑↓ seç  ⏎ paylaş  p duraklat  m ses  q çık               │
└─────────────────────────────────────────────────────────┘
```

Renkler: siyah zemin, mavi seçim satırı, yeşil "uygun", sarı "beklemede", gri soluk satırlar.
80×24'ten küçükse cihaz listesi 3 satıra kısalır, istatistik satırı korunur.
Kumanda modu açıkken TUI girdi almayı bırakır (girdi TV'ye akar).

## 8. Erişilebilirlik

- Tüm metinler ≥ 4.5:1 kontrast; `--text-tertiary` yalnızca dekoratif.
- Odak halkası her odaklanabilir öğede görünür (2 px `--accent` + 2 px dış halo).
- Sadece renk kullanan durum yok (her rozet metin + ikon).
- `prefers-reduced-motion` desteklenir.
- TV'de 10-feet arayüz: min. hedef 48 px, min. gövde 16 px (720p).

## 9. Varlık dosyaları (repo içinde)

```
assets/player/
├── index.html      # tek dosya iskelet: <video> + katmanlar
├── app.js          # MSE oynatıcı, WS istemcisi, kumanda modu, rozetler
├── style.css       # token'lar + bileşenler (bölüm 2-6)
└── hls.min.js      # HLS yedeği (yalnız HLS sink'i için; MSE varsa gerekmez)
```

Gereksinim: `style.css` içindeki token'lar bölüm 2-4 ile **birebir** eşleşmeli; yeni renk
eklenirse önce bu doküman güncellenir.
