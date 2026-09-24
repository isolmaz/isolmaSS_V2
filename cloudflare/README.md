# isolmaSS Cloudflare paylaşımı

Bu klasör, her kullanıcının **kendi Cloudflare hesabına** kurduğu, uygulamanın yerel kayıt/kopyalama akışından bağımsız resim paylaşım servisidir. `ss.isolmaz.com` resim depolamaz. Kod bağımlılık gerektirmeyen bir Worker, özel R2 deposu ve D1 veritabanı kullanır.

## Kurulum

1. Cloudflare hesabında R2'yi etkinleştirin; Cloudflare hesap/abonelik adımını uygulama sizin yerinize geçemez.
2. Uygulamada **Ayarlar → Cloudflare** bölümünde iki farklı güvenli anahtar üretin. Bu sırlar Windows'ta kullanıcıya özel DPAPI korumasıyla tutulur; `settings.json` içine yazılmaz.
3. Kaynak depo yayımlandıktan sonra [Cloudflare'a kur](https://deploy.workers.cloudflare.com/?url=https://github.com/isolmaz/isolmaSS_V2/tree/v0.5.0/cloudflare) bağlantısını açın. Cloudflare hesabınızı, istediğiniz Worker adını ve otomatik oluşturulacak özel R2/D1 kaynaklarını onaylayın. Cloudflare sır formuna `UPLOAD_TOKEN` ve `ADMIN_TOKEN` değerlerini girin. Bu değerleri URL'ye veya kaynak depoya koymayın.
4. Oluşan `https://...workers.dev` adresini uygulamadaki Worker adresine yazıp **Eşleştir**'e basın. Bu işlem admin anahtarını kullanarak şemayı ilk kez kurar ve servisle eşleşir. Alan adınız Cloudflare'daysa Worker'a ayrıca kendi alan adınızı bağlayabilir, ardından uygulamayı yeni adresle yeniden eşleştirebilirsiniz.
5. Seçim düzenleyicisinde **Yükle** ya da `Ctrl+U` kullanın. Link yalnızca sunucu başarı yanıtı verdikten sonra panoya yazılır. Üretilen bağlantıyı bilen herkes, şifre ayarlanmadıysa görüntüyü açabilir.

Kurulumu henüz kamuya açık olmayan bir repodan başlatamazsınız; Deploy to Cloudflare yalnızca public GitHub/GitLab kaynaklarıyla çalışır. Cloudflare hesabında canlı kurulum/testler sahibi tarafından yapılacaktır; yayınlanan kod bunları otomatik olarak başka bir hesaba yapmaz.

## API

`POST /api/setup`, `GET/PUT /api/settings`, `GET /api/stats`, `GET /api/images?limit=50&offset=0`, `DELETE /api/images/:id`: `Authorization: Bearer <ADMIN_TOKEN>`.

`POST /api/upload`: `Authorization: Bearer <UPLOAD_TOKEN>`, `Content-Type: image/png` veya `image/jpeg`, gövde yalnızca görüntü baytları. İsteğe bağlı `X-Image-Password`: UTF-8 şifrenin base64url (padding'siz) karşılığı; en az 12 karakter. Başarı `201 {"id":"<192-bit-id>","url":"https://<worker>/i/<id>"}`. Şifreler kaynağa URL üzerinden gönderilmez.

`GET /i/:id`: şifresiz görüntüyü döndürür, şifreli görüntüde şifre formunu gösterir. `POST /i/:id/unlock` doğrulanan şifreyle görüntüyü döndürür; saatte 20 yanlış girişten sonra geçici engel. Linkler 32 karakterlik kriptografik rastgele kimliklerdir; R2 bucket herkese açık olmaz. Geçerli linki bilen biri şifresiz görüntüye erişebilir. Servis SVG/HTML kabul etmez, görüntüleri `nosniff`, CSP ve `no-store` ile sunar. Başka birinin kaydettiği resimler uzaktan geri alınamaz.

## Ayarlar ve sayaçlar

`GET /api/settings` değerleri; `PUT /api/settings` verilen alanları günceller:

| Alan | Başlangıç | Aralık |
| --- | ---: | ---: |
| `max_active` | 50 | 1–100000 |
| `max_image_bytes` | 10485760 | 1024–104857600 |
| `max_storage_bytes` | 1000000000 | 1048576–1000000000000 |
| `daily_upload_limit` | 20 | 1–100000 |
| `daily_view_limit` | 1000 | 1–100000000 |
| `retention_days` | 30 | 1–3650 |
| `warning_percent` | 90 | 1–100 |
| `limit_action` | `warn` | `warn`, `block_upload`, `block_all` |

Sınırlar **bu kurulumun kendi trafiği** üzerinden, UTC günlük sayaçlarla yaklaşık ölçülür. `warn` yalnızca uyarı; `block_upload` yüklemeleri, `block_all` yükleme ve görüntülemeleri seçilen eşikten sonra durdurur. Mevcut linklerin de durması `block_all` için bilinçli davranıştır. Başka Cloudflare projeleri, Cloudflare'ın kendi sayaç gecikmesi ve engellenen isteklerin Worker maliyeti bu sayaca dahil değildir; **sıfır fatura garantisi yoktur**. İstatistikler başarılı resim yanıtları ve yüklemeler üzerinden, `daily`, `monthly`, `totals`, `images`, `warning`, `estimates` alanlarıyla döner; IP veya kullanıcı profili depolanmaz. Maliyet tahminleri fatura değildir.

Yeni yükleme son aktif resim sınırını aşarsa en eski erişilebilir link atomik olarak devre dışı kalır; R2 dosyasının fiziksel silinmesi 15 dakikada bir çalışan zamanlanmış işte tamamlanır. Silinen/sona ermiş görüntü adresi `404` verir. Başarısız yükleme ve yarım kalmış silme kayıtları aynı işlemle temizlenir. Önbellekleme kapalı tutulur; silme/kota kararları CDN'de baypas edilmez. Tercih değişikliğinde mevcut aktif resimlerin son kullanma tarihi güncellenir; silinmiş resimler geri gelmez.

Kaynaklar ücretsiz plan limitleriyle başlatılabilir, ancak R2 etkinleştirmesi ve geçerli hesaptaki gerçek kullanımlar ücret doğurabilir. Güncel sınırlar: [Workers](https://developers.cloudflare.com/workers/platform/pricing/), [R2](https://developers.cloudflare.com/r2/pricing/), [D1](https://developers.cloudflare.com/d1/platform/pricing/).
