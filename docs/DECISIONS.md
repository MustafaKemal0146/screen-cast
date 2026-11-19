# ekran — Karar Günlüğü (DECISIONS)

> Kilitli kararlar `docs/PLAN.md` §1'de durur. Burada **değiştirilmiş** kararlar ve gerekçeleri
> kronolojik olarak tutulur. Yeni bir karar değişikliği: önce buraya "Öneri" olarak eklenir,
> gerekçe + etki + geri dönüş planı yazılır, sonra PLAN.md güncellenir.

## D-001 — Kalite merdiveninde `540p` değişkeni neden var?
**Tarih:** 2026-11-13 · **Durum:** kabul edildi
Kullanıcı 540p'yi ayrı bir seçenek olarak istedi. 960×540, 4:3 içerik ve eski TV'lerde
1080p'den verimli; 480p ile 720p arasında doğal bir ara basamak. Merdiven korunur.

## D-002 — Gecikme ölçümü istemcide mi sunucuda mı?
**Tarih:** 2026-11-13 · **Durum:** kabul edildi
**İstemcide.** PTS ile `performance.now()` eşleşmesi gerçek uçtan uca gecikmeyi verir
(kodlama, taşıma, decode dahil). Sunucu tarafı ölçümü yalnızca kodlama/taşıma kısmını görür.
Rozet oynatıcıda gösterilir; `ekran doctor --bench` ise 100 örnek toplar (AC-1).

## D-003 — Cast/DLNA'da kumanda neden yok?
**Tarih:** 2026-11-13 · **Durum:** kabul edildi
Cast ve DLNA protokolleri fare/klavye taşımaz. Bu bir eksiklik değil, protokol sınırıdır.
Kumanda modu yalnız tarayıcı (MSE) yolunda aktiftir; Cast/DLNA'da TV'nin kendi kumandası
zaten kumandadır. Bu bilgi cihaz listesinde "kumanda: yok" olarak gösterilir.

## D-004 — Go prototipi neden Rust'a taşınıyor?
**Tarih:** 2026-11-13 · **Durum:** kabul edildi
Kullanıcı dili Rust olarak seçti. Go prototipi `legacy/prototype-go/` altında **referans** olarak
korunur: WebSocket framing, fMP4 kutu ayrıştırma ve ffmpeg argüman üretimi mantığı buradan
alınır. Bakımı yoktur, CI'ya girmez.

## D-005 — TV tarayıcısı neden "yedek" yol?
**Tarih:** 2026-11-13 · **Durum:** kabul edildi
Kullanıcının kuralı: "TV'ye uygulama yüklemeyeceğim". TV'nin yerleşik tarayıcısı da bir
uygulamadır ama TV'nin kendi özelliğidir; yine de hiçbir TV'de olduğu garanti değildir.
Birincil yol daima yerleşik protokol (Cast/DLNA), tarayıcı en son çare.

## D-006 — Ses düzeltmesi neden "sink değişimi" olarak adlandırıldı?
**Tarih:** 2026-11-13 · **Durum:** kabul edildi
Kullanıcının belirtisi "sekme değiştirince ses gidiyor" idi; ölçüm, kök nedenin **çıkış cihazı
(sink) değişince PulseAudio monitor kaynağının değişmesi** olduğunu gösterdi. Çözüm
`pactl subscribe` tabanlı `SourceWatcher`'dır (PLAN T-M2-02). `module-null-sink` + remap
alternatifi yalnız `--audio-loopback` bayrağıyla, varsayılan kapalı (PLAN §10 satır 6).

## D-007 — Ajan sayısı neden 4 olarak önerildi?
**Tarih:** 2026-11-13 · **Durum:** kabul edildi
4 ajan (core/transport, capture/encode, sinks, ui) dengeyi korur: her birinin bağlamı rahat,
dosya sahipliği çakışmaz, entegrasyon tek noktada (CLI). 8 ajan bağlam/token maliyetini ikiye
katlar ve dalga 1'de benzer dosyalara yazma riskini artırır. `docs/EXECUTION.md` §2'de
1/2-3/4-6/7-8 ajan senaryoları tanımlıdır.

## D-008 — `CastMessage` protobuf alanları 1 ve 5 neden `required` (proto2)?
**Tarih:** 2026-11-13 · **Durum:** kabul edildi
`docs/PROTOCOLS.md` §2.3 örnek şeması `proto3` sözdizimiyle yazılmıştı; ancak gerçek Chromecast
cihazları (port 8009) resmi `cast_channel.proto` (proto2) tel biçimini bekler: `protocol_version = 0`
(`CASTV2_1_0`) ve `payload_type = 0` (`STRING`) değerleri `0` olsa bile telde açıkça kodlanmalıdır.
`ekran-sinks/src/cast.rs` içinde `prost` `required` etiketleri kullanıldı ve testlerle doğrulandı.

## D-009 — `ekran-transport` ve `ekran-sinks` çapraz bağımlılıklarının kaldırılması
**Tarih:** 2026-11-13 · **Durum:** kabul edildi
Başlangıç iskeletindeki `Cargo.toml` dosyalarında `ekran-sinks → ekran-transport` ve
`ekran-transport → ekran-encode` bağımlılıkları bulunuyordu. `AGENTS.md` §1 tek yönlü bağımlılık
kuralı gereği bu çapraz bağımlılıklar kaldırıldı; tüm kütüphane crate'leri yalnızca `ekran-core`'a
bağlıdır, birleştirme `ekran-cli` içindedir.

