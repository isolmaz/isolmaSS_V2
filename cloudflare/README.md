# isolmaSS Cloudflare paylaşımı

**Sürüm:** 0.5.2. OAuth kurulumunun gerçek Cloudflare hesabındaki uçtan uca testi kullanıcıya aittir; yerel Worker denemesi canlı hesap başarısı anlamına gelmez.

Bu servis, her kullanıcının **kendi Cloudflare hesabındaki** özel Worker ve SQLite tabanlı Durable Object içinde çalışır. Yayıncının `ss.isolmaz.com` sitesi görüntü barındırmaz, Cloudflare hesabınıza giriş yapmaz veya yükleme anahtarı tutmaz. R2, D1, Git deposu ve kişisel alan adı gerekmez.

## Kullanım ve kurulum

Bir görüntü seçip **Yükle** veya `Ctrl+U`'ya basın. Kurulum yoksa görüntü gönderilmez; **Ayarlar → Cloudflare → Cloudflare ile devam et** yolundan tarayıcıda Cloudflare hesabınıza giriş yapıp gerekli izinleri verin. Birden çok hesabınız varsa kurulacak hesabı uygulamada seçersiniz. Kurulum Worker ve depolamayı seçilen hesapta oluşturur, `https://<worker>.<hesap>.workers.dev` adresini otomatik bağlar ve işlemi tamamlar. URL, GitHub hesabı, sır veya anahtar yapıştırma yoktur. İlk Yükle'de bekleyen görüntü başarıdan sonra yüklenir; vazgeçerseniz yerelde kalır.

Uygulama OAuth Authorization Code + PKCE kullanır. Hesap yönetimi için yalnız üyelikleri okuma ve Worker kurmak için Workers Scripts Write izni gerekir. OAuth erişim anahtarı yalnız kurulum sırasında bellekte kullanılır; yüklemeler için ayrı Worker sırları Windows DPAPI ile korunur. Bağlantıyı kaldırmak yalnız bu bilgisayardaki sırları siler; Cloudflare'daki mevcut Worker veya görüntülere dokunmaz. Hesap izni Cloudflare profilinizde ayrıca iptal edilebilir.

Yeni Worker'ın adı rastgele üretilir; başka Worker'ların üzerine yazılmaz. Kurulum API ile yalnız kendi Worker'ını dağıtır; Cloudflare hesabınızdaki mevcut siteleri, alan adlarını veya diğer Worker'ları değiştirmez. İlk `workers.dev` alt alanı olmayan hesaplarda uygulama boşta olan bir alt alan oluşturabilir; mevcut bir alt alan adını yeniden adlandırmaz.

Cloudflare OAuth istemcisi yayıncı hesabında kayıtlıdır; kullanıcı onayıyla Public yapılmış ve yayıncı alan adı doğrulanmıştır. Public görünürlük geri alınamaz. Free hesapların Durable Objects/Workers ürün erişimi veya hesap yönetici kısıtlamaları değişebilir; Cloudflare API bir işlemi engellerse uygulama bunu başarı gibi göstermez. **Gerçek yeni hesapta uçtan uca dağıtım ayrıca kullanıcı tarafından sınanmalıdır.**

## HTTP sözleşmesi

- `POST /api/setup`, `GET/PUT /api/settings`, `GET /api/stats`, `GET /api/images?limit=50&offset=0`, `DELETE /api/images/:id`: ayrı `ADMIN_TOKEN` bearer sırrı.
- `POST /api/upload`: ayrı `UPLOAD_TOKEN` bearer sırrı, `Content-Type: image/png` veya `image/jpeg`, gövde görüntü baytları. İsteğe bağlı `X-Image-Password`, UTF-8 şifrenin padding'siz base64url karşılığıdır (en az 12 karakter, en fazla 128 UTF-8 bayt). Başarı: `201 {"id":"<192-bit-id>","url":"https://<worker>/i/<id>"}`.
- `GET /i/:id`: bağlantıyı bilen herkes şifresiz görüntüye erişebilir. Şifreli bağlantı form gösterir; `POST /i/:id/unlock` şifreyi doğrular, saatte 20 yanlış denemeyi sınırlar. Şifre URL'ye konmaz. Silinen/sona eren bağlantı `404` verir. Erişim ve çıktı `no-store`, `nosniff`, CSP ve `no-referrer` ile kısıtlıdır.

Görüntü kimlikleri tahmin edilebilir sıralı numaralar değil 192-bit rastgele değerlerdir. Linkin kendisi şifresiz dosyaya erişim yetkisidir; gizli görüntülerinizi paylaşırken ayrıca şifre kullanın. Bir alıcı görüntüyü indirdikten sonra uzaktan geri alınamaz.

## Saklama ve ücretsiz plan sınırları

Varsayılan: son 50 görüntü, görüntü başına en çok 10 MiB, 30 gün saklama, 800 MB uygulama depolama üst sınırı, günlük 20 yükleme ve 1000 görüntüleme için uyarı. İsteğe bağlı `warn`, `block_upload` veya `block_all` eşiği ve yüzdesi ayarlardan değiştirilebilir. Worker tüm görüntüleri <=1 MB SQLite BLOB parçalarında saklar; limiti aşan en eski bağlantıyı işlem içinde siler. Sona erenler istekte ve Durable Object alarmında temizlenir.

Free planda Durable Object hesabı için toplam 5 GB, bir nesne için 1 GB ve tek SQLite BLOB için 2 MB sınırı vardır. Uygulamanın 800 MB tavanı bunların altında kalmayı hedefler; başka projelerin depolaması ve hesabın genel Workers kotaları uygulamanın sayaçlarında görünmez. **Sınırsız ücretsiz kullanım veya bütün hesap için sıfır fatura garantisi yoktur.** Kota aşılırsa işlem durur ve açık hata verir; R2 aboneliği istenmez. Güncel koşullar: [Workers limitleri](https://developers.cloudflare.com/workers/platform/limits/), [Durable Objects limitleri](https://developers.cloudflare.com/durable-objects/platform/limits/) ve [Free fiyatlandırma](https://developers.cloudflare.com/durable-objects/platform/pricing/).

Kaynak dosya `worker.mjs` tek modüldür; `wrangler.jsonc` aynı `STORE` binding'i ve `ShareStore` SQLite sınıfını içerir. Geliştirme için `.dev.vars.example` şablondur; gerçek sırları depoya koymayın. Site Worker'ı `isolmass-site` bu paylaşım Worker'ından ayrıdır.
