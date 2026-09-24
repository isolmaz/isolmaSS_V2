# ss.isolmaz.com

Bağımsız, statik indirme/özellik/döküman/gizlilik sitesi. Ekran görüntüsü yüklemelerini **barındırmaz**; her kullanıcı ayrı Cloudflare kurulumunu yönetir.

`site/wrangler.jsonc` Workers Static Assets için hazırdır. Site kökünde `index.html`, `docs.html`, `changelog.html`, `privacy.html`, `contact.html`, `terms.html`, `styles.css`, `site.js` ve `favicon.svg` bulunur. Sayfa düzeni, TR/EN geçişi, küçük ekran menüsü ve yayın akışı aynı sahibin `SSDownload-site` referansına göre uyarlanmıştır; görseller temsili arayüz çizimleridir, gerçek ekran görüntüsü olarak sunulmaz. `.assetsignore`, ürün belgesini ve yapılandırma dosyasını herkese açık asset olarak dağıtmaz.

Site sahibi `site/` dizininden kendi Cloudflare hesabında Worker'ı dağıtır ve `ss.isolmaz.com` Custom Domain bağlantısını kurar. Bu repo Cloudflare hesabına otomatik erişmez; canlı dağıtım ve alan adı DNS işlemleri hesap sahibinin kontrolündedir. İndirme düğmeleri public [isolmaSS güncelleme deposundaki en güncel imzalı sürüme](https://github.com/isolmaz/isolmaSS-updates/releases/latest) yönlendirir. Bu alan adı yalnızca tanıtım sitesine aittir: paylaşım kullanıcıları için kendi alan adı gerekmez, Cloudflare'ın `workers.dev` adresi yeterlidir. `v0.5.1` kurulum bağlantısı için kaynak etiketinin yayımlanması gerekir.

Aşağıdaki komutlar Wrangler kurulu hesap sahibi içindir; bu projeye yeni paket eklenmez:

```powershell
cd site
wrangler deploy
```

Tarayıcı/yerel statik sunucu ile doğrulama Cloudflare kaynaklarına dokunmadan yapılabilir. Ücret örnekleri açıklayıcıdır; Cloudflare hesabında R2 aboneliği, trafik ve fatura bağımsız izlenmelidir.
