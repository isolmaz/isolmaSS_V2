# isolmaSS

Native Windows screenshot editor in Rust. Capture a region or window, draw on it, then copy/save locally or explicitly upload to a Worker in **your own Cloudflare account**. The first Upload opens guided setup if sharing is not configured; isolmaSS operates no shared screenshot service and collects no in-app analytics. Version **0.5.4**.

## Use

1. Start `isolmass.exe`; it remains in the notification area. Launching it again opens the running instance's Settings rather than an installer.
2. Press **PrintScreen**, click the tray icon, or choose **Capture now** from the native tray menu. The capture shortcut and delay are configurable.
3. Drag a region of at least 8 × 8 pixels, or click a visible window with window snapping enabled. Resize from the four corners or four edge midpoints; drag the dimensions label above the top-left corner to move the entire selection.
4. Use the four main tools (**Seç/taşı**, **Çerçeve**, arrow, pen) or click the chevron for highlighter, text, numbered steps, blur/pixelation and opaque **Karart**. F10/Apps also exposes every tool. Select a drawing to move, recolor, adjust its width, resize where applicable, or delete it. Undo and redo include edits.
5. Choose **Copy**, **Save** or **Save as**. Editor chrome, handles, selection frame, caret and unfinished previews do not appear in the exported image. The first **Upload** opens a guided Cloudflare setup without sending the screenshot. After pairing your Worker, the same selection uploads automatically; later uploads send the flattened selection directly and copy the link only after the Worker confirms success. Canceling setup leaves the selection available. A failed transfer leaves the clipboard unchanged.

**Karart** replaces covered pixels opaquely and is rendered after other annotations. Blur and highlighter are visual effects, **not** secure redaction. Inspect the exported image before sharing sensitive material.

### Keyboard

| Shortcut | Action |
|---|---|
| Configured capture shortcut (default PrintScreen) | Start capture using the configured delay |
| V / R / A / P / T / H / N / B / M | Select, rectangle, arrow, pen, text, highlighter, numbered step, blur, opaque redaction |
| Ctrl+C or Enter | Copy the selection |
| Ctrl+S / Ctrl+Shift+S | Save / Save as |
| Ctrl+U | First use: guided setup; after pairing: upload and copy the successful link |
| Ctrl+Z / Ctrl+Y | Undo / redo |
| Ctrl+, | Settings |
| F10 or Apps | Native toolbar command menu |
| Delete or Backspace | Delete selected annotation |
| Arrow keys / Shift+arrows | Move selection or object by 1 / 10 pixels |
| Ctrl+arrows | Resize selection when no object is selected |
| Shift while drawing | Square rectangle/redaction; 45-degree arrow angles |
| Esc or right-click | Cancel the current edit, deselect, clear the selection, or close the editor |

While editing text, Enter commits; Ctrl+C, Ctrl+S and Ctrl+U commit before exporting. Text selection, clipboard paste and local undo/redo are supported. Text is single-line and limited to 16 KiB; pasted newlines become spaces. The toolbar width slider and its numeric button accept **1–64 px**: drag, scroll over either control in 1 px steps, or click the number, type a value and press Enter (Esc discards it). Three quick colors (red, green, blue) and a fourth last-picked custom color are shown next to the native Windows color picker. The custom color and active color persist across editor sessions.

## Settings and updates

Settings is a compact fixed-size Windows 11 dialog with a selector bar of tabs: **Genel** (shortcut, delay, theme, start with Windows, save notification), **Kaydetme** (folder, PNG/JPEG, JPEG quality), **Düzenleyici** (a 20-colour palette plus a custom colour, 1–64 px width, window snapping, close after action), **Paylaşım** (Cloudflare connection status and the Cloudflare window) and **Güncelleme** (automatic checks, manual check with inline progress). Options use native combo boxes and smooth toggle switches; Save/İptal sit in the footer band. Appearance follows Windows or can be explicitly light/dark; scroll bars, combo boxes, edits and menus use the native dark style in dark mode. The Cloudflare window uses the same layout.

Record a shortcut using Ctrl/Alt/Shift/Win with a letter, digit, F1–F24 or PrintScreen; bare PrintScreen also works. Unsupported keys are ignored. An unavailable combination is reported rather than silently replacing the current shortcut. On a startup conflict, the app asks whether to use Ctrl+Shift+S for that session. Settings remain open after a save error.

Update checks use public releases from [`isolmaz/isolmaSS-updates`](https://github.com/isolmaz/isolmaSS-updates). A manual check in Settings displays a live progress indicator and its result inline; a tray-initiated check immediately announces progress and reports the result in a native dialog. A newer release offers **Yükle / Daha sonra / Bu sürümü atla** and warns before installation that isolmaSS will close and reopen; installation always requires an explicit choice. The skipped version stays skipped for automatic checks; a manual check can offer it again. Installation waits for the active editor/settings session and any copy/save action to finish, verifies the signed installer again, then restarts. The installer keeps a rollback executable until the installed app passes a startup check. See [SECURITY.md](SECURITY.md) and [DISTRIBUTION.md](DISTRIBUTION.md).

An installation without the pinned verifier needs **one manual installation** of a signed-update-capable release before in-app updates can work. The free application-level signature does not remove Windows SmartScreen warnings; do not bypass a warning automatically. Portable ZIPs are for manual use, not in-place updates of an installed copy.

The native tray menu provides **Ekran görüntüsü al**, **Ayarlar**, **Ekran görüntüsü klasörü**, **Güncellemeleri denetle**, recent captures and **isolmaSS uygulamasından çık**. Quit waits until an active edit/settings session ends.

## Optional Cloudflare upload

There is **no central isolmaSS image host or sign-in**. Select a screenshot and click **Upload** (`Ctrl+U`): first use opens the Cloudflare connection window, without sending the screenshot. The editor steps aside meanwhile (the capture and annotations stay in memory) so the browser consent page is not hidden behind it; the connection window has its own taskbar button and comes back to the front after consent. Choose **Cloudflare ile devam et**. The app explicitly requests `workers-scripts.write` and `memberships.read` from its registered public OAuth client; review these two required permissions in the browser, then select an account if you have more than one. If the consent page still shows zero permissions, verify the publisher OAuth client's configured scopes before approving anything. The app generates distinct Worker keys, installs a new Worker backed by a SQLite Durable Object in the chosen account, enables its `workers.dev` URL and stores the connection locally. A brand-new `workers.dev` address can take a few minutes to resolve: setup waits up to about two minutes and, if the Worker still does not answer, keeps the pairing and says so instead of abandoning the Worker. After setup the editor returns and the selected screenshot uploads; only a successful link reaches the clipboard. Login may take up to 10 minutes (sign-up, e-mail verification, 2FA). Transient network failures are retried where a retry cannot duplicate an upload. No GitHub account, key pasting, R2 activation or custom domain is needed. Closing setup leaves the selection local. Cloudflare authorization remains explicit, and the OAuth access token is not persisted. Disconnecting locally does not delete the remote Worker or images.

If Cloudflare refuses a step, the dialog shows Cloudflare's own error code and message. `HTTP 403` while reading the account's Workers subdomain usually means the selected account lacks Workers permission for you, or that the publisher OAuth client is still **private** (only members of the client's own account can authorize a private client); choose another account or ask the publisher to make the client public.

Once paired, the **Cloudflare** window opens on **Bağlantı / Sınırlar / Resimler** tabs and loads statistics immediately. **Sınırlar** controls the active image count (50 initially), per-image and total bytes, daily uploads/views, retention and warning percentage (90% initially). At that threshold choose *warn*, *block new uploads* or *block new uploads and viewing*. **Resimler** lists and deletes recent images; daily/monthly counters and occupied storage describe this Worker only, not other Cloudflare projects. **No option guarantees a zero invoice.** Daily counters use UTC. An optional password (**Bağlantı** tab) applies to future screenshots over HTTPS; only a per-image salted verifier is stored in the Durable Object. Share passwords separately. A valid unprotected link is viewable by anyone who has it. Expired or deleted images become unavailable; previously downloaded copies cannot be revoked.

### Sharing API and deployment

`cloudflare/worker.mjs` is the per-user Worker; `cloudflare/wrangler.jsonc` binds `STORE` to its SQLite `ShareStore` Durable Object. The app provisions the Worker through OAuth Authorization Code + PKCE, uses the Cloudflare token only during installation, and stores separate Worker tokens with Windows DPAPI. `cloudflare/.dev.vars.example` is a local template; never commit real secrets. Account owners must separately verify deployment and upload with a real account: local Worker checks cannot prove Cloudflare account permissions or live installation.

- `POST /api/upload` requires the `UPLOAD_TOKEN` bearer secret and PNG/JPEG bytes; optional `X-Image-Password` carries a padding-free base64url-encoded UTF-8 password (at least 12 printable characters, at most 128 UTF-8 bytes). Success: `201 {"id":"<192-bit-id>","url":"https://<worker>/i/<id>"}`.
- `POST /api/setup`, `GET/PUT /api/settings`, `GET /api/stats`, `GET /api/images?limit=50&offset=0`, and `DELETE /api/images/:id` require the separate `ADMIN_TOKEN` bearer secret.
- `GET /i/:id` serves the image or a password form; `POST /i/:id/unlock` checks its password with a limit of 20 incorrect attempts per hour. Expired/deleted links return `404`; responses use `no-store`, `nosniff`, CSP, and `no-referrer` protections.

Images are split into at most 1 MB SQLite chunks. The defaults are 50 active images, 10 MiB per image, 800 MB app storage, 30-day retention, 20 uploads and 1,000 views per UTC day. `warn`, `block_upload`, and `block_all` policies are configurable; counters cover this Worker, **not the whole Cloudflare account**. Consult current [Workers limits](https://developers.cloudflare.com/workers/platform/limits/) and [Durable Objects limits](https://developers.cloudflare.com/durable-objects/platform/limits/); no setting guarantees a zero invoice.

The independent `site/` directory contains the static download/docs/privacy site and `site/wrangler.jsonc` for `ss.isolmaz.com`; it neither receives captures nor installs user Workers. Account-owner deployment: run `wrangler deploy` from `site/`. Its links point to the public [latest signed release](https://github.com/isolmaz/isolmaSS-updates/releases/latest); no Node/Bun project or app Cloudflare account access is needed.

## Files and privacy

- Default output: Windows Pictures known folder → `Screenshots`.
- Configuration: `%APPDATA%\isolmaSS\settings.json` (64 KiB limit, validated, atomically saved under a cross-process lock). It contains the Worker origin, but **no access token**.
- Worker upload/admin secrets and optional default image password: `%LOCALAPPDATA%\isolmaSS\cloud-credentials.bin`, protected for the current Windows user with DPAPI. OAuth authorization codes and access tokens remain transient; they are not saved in settings or the credentials file. Disconnecting locally does not delete the remote Worker or images.
- Diagnostics: `%LOCALAPPDATA%\isolmaSS\logs\diagnostic.log` (rotated near 1 MiB). Logs exclude screenshot pixels, annotation text and recorded keystrokes; errors may include local paths.
- Staged update downloads: `%LOCALAPPDATA%\isolmaSS\updates`.

Invalid settings JSON is copied to `settings.corrupt.json` before reset; a failed backup or other read error remains visible. Output PNG/JPEG and clipboard DIB are opaque. Save as replaces a chosen file only after encoding and flushing succeeds.

## Build

Windows x64, the Windows SDK/Visual Studio C++ Build Tools, Rust **1.98.0** and NSIS are required. `Cargo.lock` pins dependencies.

```powershell
cargo fmt --check
cargo check --locked
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked -- --test-threads=1 --skip test_tray_manager_lifecycle
cmd /c package.bat
```

`package.bat` verifies versions, size budgets and SHA-256. Set `ISOLMASS_BUILD_DIR=target\release-candidate` to avoid replacing a running `target\release\isolmass.exe`. `release.bat` requires access to the publisher's non-exportable Windows signing key; CI has no private key and does not publish releases. Release steps are in [DISTRIBUTION.md](DISTRIBUTION.md).

`--capture-once` opens the editor without starting the tray daemon. `--settings`, `--check-update`, `--verify-update PATH`, `--benchmark N` and `--help` are also available. `--smoke-test` requires an interactive desktop, briefly uses hooks/tray resources and **changes the clipboard**; it builds a temporary installer without modifying the signed release artifact. Use a disposable desktop or preserve your clipboard first. `--fix-printscreen` changes the Windows Snipping Tool registration preference only when explicitly invoked.

Interactive acceptance includes both themes, DPI scaling, every editor tool/export path, tray and settings flows, and rollback in an isolated Windows profile or VM. A real Cloudflare account must be used to accept OAuth, Worker installation, and upload; local builds are not live-account proof. MIT license: [LICENSE](LICENSE); dependencies: [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
