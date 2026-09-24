# ss.isolmaz.com

Bağımsız, statik indirme/özellik/döküman/gizlilik sitesi. Ekran görüntüsü yüklemelerini **barındırmaz**; her kullanıcı ayrı Cloudflare kurulumunu yönetir.

`https://ss.isolmaz.com` Cloudflare Workers Static Assets üzerinde `isolmass-site` Worker'ı olarak yayındadır. `site/wrangler.jsonc` yalnızca bu alt alan adını özel alan olarak bağlar. Site kökünde `index.html`, `docs.html`, `changelog.html`, `privacy.html`, `contact.html`, `terms.html`, `styles.css`, `site.js` ve `favicon.svg` bulunur. Sayfa düzeni, TR/EN geçişi, küçük ekran menüsü ve yayın akışı aynı sahibin `SSDownload-site` referansına göre uyarlanmıştır; görseller temsili arayüz çizimleridir, gerçek ekran görüntüsü olarak sunulmaz. `.assetsignore`, ürün belgelerini, yapılandırmayı ve geçici `.wrangler/` çıktılarını herkese açık asset olarak dağıtmaz.

Sonraki sürümler için site sahibi `site/` dizininde `wrangler deploy` çalıştırır; mevcut yapılandırma yalnızca `isolmass-site` Worker'ını `ss.isolmaz.com` özel alanıyla yayımlar. Bu repo Cloudflare hesabına otomatik erişmez; canlı dağıtım ve alan adı DNS işlemleri hesap sahibinin kontrolündedir. İndirme düğmeleri public [isolmaSS güncelleme deposundaki en güncel imzalı sürüme](https://github.com/isolmaz/isolmaSS-updates/releases/latest) yönlendirir. Bu alan adı yalnızca tanıtım sitesine aittir: paylaşım kullanıcıları için kendi alan adı gerekmez, Cloudflare'ın `workers.dev` adresi yeterlidir.

Aşağıdaki komutlar Wrangler kurulu hesap sahibi içindir; bu projeye yeni paket eklenmez:

```powershell
cd site
wrangler deploy
```

Tarayıcı/yerel statik sunucu ile doğrulama Cloudflare kaynaklarına dokunmadan yapılabilir. 0.5.2 sürümünde bulut paylaşımı için uygulamanın açtığı OAuth ekranında izin verilir; Worker ve Durable Object seçilen hesapta kurulur. Cloudflare Free limitleri hesabın tamamına uygulanır; fatura ve genel kullanım hesap sahibi tarafından izlenmelidir.
