# isolmaSS

Native Windows screenshot editor in Rust. Capture a region or window, draw on it, then copy or save locally, or upload to a Worker in **your own Cloudflare account** and get a share link in about a second. isolmaSS operates no shared screenshot service and collects no analytics. Version **0.5.5**.

## Use

1. Start `isolmass.exe`; it stays in the notification area. Launching it again opens Settings of the running instance.
2. Press **PrintScreen**, click the tray icon, or choose **Ekran görüntüsü al** from the tray menu. The shortcut and an optional delay are configurable.
3. Drag a region of at least 8 × 8 pixels, or click a visible window when window snapping is on. Resize from the corners or edge midpoints; drag the size label above the top-left corner to move the selection.
4. Draw with **Seç/taşı**, **Çerçeve**, arrow and pen, or open the chevron for highlighter, text, numbered steps, blur and opaque **Karart**. F10/Apps lists every command. Select a drawing to move, recolor, resize, change its width or delete it. Undo and redo cover every edit.
5. Choose **Kopyala**, **Kaydet**, **Farklı kaydet** or **Yükle**. Editor chrome, handles and unfinished previews never appear in the output.

**Yükle** closes the editor at once and shows a small card in the bottom-right corner: *Yükleniyor…*, then *Yüklendi · bağlantı kopyalandı* with the link, **Kopyala** and **Aç**. The link is already on the clipboard. If the transfer fails the card shows the reason and **Tekrar dene**; the clipboard is left unchanged. The first **Yükle** opens Cloudflare setup instead (see below).

**Karart** replaces covered pixels opaquely and is drawn after other annotations. Blur and highlighter are visual effects, **not** secure redaction. Inspect the image before sharing sensitive material.

### Keyboard

| Shortcut | Action |
|---|---|
| Capture shortcut (default PrintScreen) | Start a capture after the configured delay |
| V / R / A / P / T / H / N / B / M | Select, rectangle, arrow, pen, text, highlighter, numbered step, blur, opaque redaction |
| Ctrl+C or Enter | Copy the selection |
| Ctrl+S / Ctrl+Shift+S | Save / Save as |
| Ctrl+U | Upload and copy the link (first use: Cloudflare setup) |
| Ctrl+Z / Ctrl+Y | Undo / redo |
| Ctrl+, | Settings |
| F10 or Apps | Toolbar command menu |
| Delete or Backspace | Delete the selected annotation |
| Arrow keys / Shift+arrows | Move the selection or object by 1 / 10 pixels |
| Ctrl+arrows | Resize the selection when no object is selected |
| Shift while drawing | Square rectangle/redaction; 45° arrow angles |
| Esc or right-click | Cancel the current edit, deselect, clear the selection, or close the editor |

While editing text, Enter commits; Ctrl+C, Ctrl+S and Ctrl+U commit before exporting. Text is single-line, up to 16 KiB; pasted newlines become spaces. The toolbar width slider and number accept **1–64 px** (drag, scroll, or click the number and type). Three quick colors and the last custom color sit next to the Windows color picker; the active and custom colors persist.

## Settings

Settings is a compact Windows 11 dialog with tabs on top:

- **Genel** — capture shortcut, delay, theme (system, light, dark), start with Windows, save notification.
- **Kaydetme** — folder, PNG or JPEG, JPEG quality.
- **Düzenleyici** — a 20-color palette plus a custom color, line width 1–64 px, window snapping, close after an action.
- **Paylaşım** — Cloudflare connection status and the Cloudflare window.
- **Güncelleme** — automatic checks and a manual check with inline progress.

Record a shortcut with Ctrl/Alt/Shift/Win plus a letter, digit, F1–F24 or PrintScreen; bare PrintScreen also works. A combination already taken by Windows or another app is reported and the current shortcut is kept. If the shortcut is unavailable at startup, the app offers Ctrl+Shift+S for that session. Controls, scroll bars and menus follow the light or dark theme.

The tray menu offers **Ekran görüntüsü al**, **Ayarlar**, **Ekran görüntüsü klasörü**, **Güncellemeleri denetle**, recent captures and **isolmaSS uygulamasından çık**.

## Updates

Update checks read public releases of [`isolmaz/isolmaSS-updates`](https://github.com/isolmaz/isolmaSS-updates). A newer release offers **Yükle / Daha sonra / Bu sürümü atla**; nothing installs without that choice, and a skipped version is offered again only by a manual check. Installation waits for any open editor or Settings window, verifies the installer's publisher signature again, closes the app, installs and restarts it. The installer keeps a rollback copy until the new version passes a startup check. The signature is application-level: Windows SmartScreen may still warn, and the app never bypasses that warning. Portable ZIPs are for manual use and do not update an installed copy. See [SECURITY.md](SECURITY.md) and [DISTRIBUTION.md](DISTRIBUTION.md).

## Cloudflare sharing

There is **no central isolmaSS image host or sign-in**. Sharing uses a Worker that the app installs in your own Cloudflare account; Copy and Save never upload.

**Setup (first Yükle).** The editor steps aside, keeping the capture in memory, and the connection window opens with its own taskbar button. Choose **Cloudflare ile devam et**, sign in in the browser and approve the two requested permissions, `workers-scripts.write` and `memberships.read`, for the account you want. Sign-in may take up to 10 minutes. The window comes back to the front; accounts you did not authorize for Workers are hidden, and with a single account installation starts immediately. The app creates a randomly named Worker with a private SQLite Durable Object, enables its `workers.dev` address, protects the generated Worker keys with Windows DPAPI and uploads the pending screenshot. A brand-new `workers.dev` address can take a few minutes to resolve; the pairing is kept meanwhile and uploads work once it answers. No GitHub account, key pasting, R2, D1 or custom domain is needed. The OAuth access token is used only during installation and never stored.

**Cloudflare window.** Once paired it has three tabs and loads current data when opened:

- **Bağlantı** — Worker address, today's and this month's uploads and views, stored images and size, an optional password for new uploads, **Yenile** and **Bağlantıyı kaldır** (removes the connection from this computer only; the Worker and images stay).
- **Sınırlar** — images kept (50), daily uploads (20) and views (1,000), size per image (10 MB) and in total (800 MB), retention (30 days) and the warning threshold (90%). At the threshold choose *warn*, *stop new uploads* or *stop uploads and viewing*. Counters cover this Worker only, use UTC days, and **no setting guarantees a zero invoice**.
- **Resimler** — recent images with local time, size, views and password status; **Aç** (or double-click), **Bağlantıyı kopyala** and **Sil**. Deleted and expired links stop working immediately; copies already downloaded cannot be revoked.

A valid link without a password can be viewed by anyone who has it. With a password, only a salted per-image verifier is stored; share the password separately.

**If something goes wrong,** dialogs and the upload card show Cloudflare's own error code and message.

- *Workers alt alanı denetlenemedi (403)*: the chosen account does not grant you Workers access, or the publisher's OAuth client is private (usable only by members of its own account). Pick another account.
- *Adres çözülemedi*: Windows could not resolve the Worker's host name. The app clears that DNS cache entry and retries automatically; if it persists, check the adapter's DNS servers, especially IPv6 servers on a network without IPv6.
- *401*: the Worker's keys do not match this computer; connect again. The existing Worker is left untouched.
- *429 / 507*: a daily limit or the storage limit is reached; wait for the UTC reset, raise the limit or delete images.

### Worker API

`cloudflare/worker.mjs` is the per-user Worker; `cloudflare/wrangler.jsonc` binds `STORE` to its SQLite `ShareStore` Durable Object. `cloudflare/.dev.vars.example` is a local template; never commit real secrets.

- `POST /api/upload` requires the `UPLOAD_TOKEN` bearer secret and PNG/JPEG bytes; optional `X-Image-Password` carries a padding-free base64url UTF-8 password (12–128 printable characters). Success: `201 {"id":"<192-bit id>","url":"https://<worker>/i/<id>"}`.
- `POST /api/setup`, `GET/PUT /api/settings`, `GET /api/stats`, `GET /api/images?limit=50&offset=0` and `DELETE /api/images/:id` require the separate `ADMIN_TOKEN` bearer secret.
- `GET /i/:id` serves the image or a password form; `POST /i/:id/unlock` checks the password, with at most 20 wrong attempts per hour. Unknown, expired or deleted links return `404`; responses use `no-store`, `nosniff`, CSP and `no-referrer`.

Images are stored in chunks of at most 1 MB. Consult Cloudflare's [Workers limits](https://developers.cloudflare.com/workers/platform/limits/) and [Durable Objects limits](https://developers.cloudflare.com/durable-objects/platform/limits/); free allowances are shared by the whole account.

### Website

`site/` is the static information and download site for `ss.isolmaz.com`, deployed by the account owner with `wrangler deploy` from `site/`. It hosts no screenshots, installs no Workers and links to the [latest signed release](https://github.com/isolmaz/isolmaSS-updates/releases/latest).

## Files and privacy

- Screenshots: the Windows Pictures folder → `Screenshots` by default.
- Settings: `%APPDATA%\isolmaSS\settings.json` (at most 64 KiB, validated, saved atomically under a cross-process lock). It holds the Worker address but **no key or token**.
- Worker keys and the optional image password: `%LOCALAPPDATA%\isolmaSS\cloud-credentials.bin`, encrypted for the current Windows user with DPAPI.
- Upload staging: an encoded copy of the selection in the user's temp folder, deleted when the upload card closes.
- Diagnostics: `%LOCALAPPDATA%\isolmaSS\logs\diagnostic.log` (rotated near 1 MiB). No pixels, annotation text or keystrokes; errors may include local paths.
- Update downloads: `%LOCALAPPDATA%\isolmaSS\updates`.

Invalid settings JSON is copied to `settings.corrupt.json` before defaults are used. Exported PNG/JPEG and clipboard images are opaque. Save as replaces a file only after encoding and flushing succeed.

## Build

Windows x64, the Windows SDK / Visual Studio C++ Build Tools, Rust **1.98.0** and NSIS are required. `Cargo.lock` pins dependencies.

```powershell
cargo fmt --check
cargo check --locked
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked -- --test-threads=1 --skip test_tray_manager_lifecycle
cmd /c package.bat
```

`package.bat` verifies versions, size budgets and SHA-256. Set `ISOLMASS_BUILD_DIR=target\release-candidate` while `target\release\isolmass.exe` is running. `release.bat` needs the publisher's non-exportable signing key; CI has no key and publishes nothing. Release steps: [DISTRIBUTION.md](DISTRIBUTION.md).

Command line: `--capture-once` (editor without the tray; waits for an upload card to close), `--settings`, `--check-update`, `--verify-update PATH`, `--benchmark N`, `--fix-printscreen` and `--help`. `--smoke-test` needs an interactive desktop and **changes the clipboard**.

Manual acceptance covers both themes, DPI scaling, every tool and export path, tray and Settings flows, updater rollback in an isolated Windows profile, and Cloudflare setup and upload with a real account. MIT license: [LICENSE](LICENSE); dependencies: [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
