# Release notes

## 0.5.2 — Cloudflare OAuth installation

- Replaced manual GitHub/Deploy to Cloudflare, R2 and D1 setup with browser-based Cloudflare OAuth + PKCE and account selection. Installs a new Worker with a SQLite Durable Object in the selected account, enables its workers.dev address and resumes the first Upload after a successful connection.
- Separate Worker upload/admin secrets are generated automatically and protected by per-user Windows DPAPI. The Cloudflare OAuth access token is not persisted. Existing local screenshot editing stays unchanged; 0.5.1 installations can update with the signed 0.5.2 installer.
- Updated per-Worker quotas, retention, password-protected links and deletion for the Durable Object backend. Cloudflare Free limits are account-wide; no zero-cost promise or live third-party-account verification is implied.

## 0.5.1 — previous release

- First `Upload`/`Ctrl+U` opens a short guided Cloudflare installation instead of failing when unpaired. After approved deployment and pairing, the selected screenshot uploads automatically; canceling leaves it local.
- Generated setup keys persist encrypted with per-user Windows DPAPI so users can return to an unfinished installation. No custom domain is needed or connected automatically; Cloudflare account/R2 approval still belongs to the user.
- Updated Turkish/English site and setup instructions to distinguish setup from an actual upload.

## 0.5.0

- Added opt-in `Ctrl+U` screenshot upload and copy-link workflow for each user's own Cloudflare Worker, private R2 bucket and D1 database. Existing local copy/save paths remain unchanged.
- Added a Cloudflare settings window with per-installation DPAPI-protected credentials, editable retention/usage limits, password-protected links, live per-Worker usage statistics and approximate billing information.
- Added deployable self-host Worker source and a bilingual static `ss.isolmaz.com` informational/download/docs/privacy site styled after the owner's SSDownload site. No shared screenshot host or application sign-in.
- Publisher-side verification is local only; real Cloudflare account installation and interactive desktop acceptance are user-run.

## 0.4.2

- Reorganized Settings into a compact two-column General page and content-sized Editor/Updates pages; manually resized windows retain their dimensions. Light and dark controls now use the same visual language.
- Manual update checks show progress and results inside Settings. Tray checks announce activity, expose a checking state and present a visible result dialog.
- Update consent explains that the app closes and starts again after verified installation; download verification and deferred installation have explicit status feedback.

## 0.4.1

- Compact four-tool rail with a chevron for additional drawing tools; refined vector icons and larger, DPI-scaled settings switches.
- Eight selection resize grips and draggable dimensions label; three quick colors plus the persisted last-picked custom swatch.
- Stroke width accepts wheel steps and typed numeric values; arrowheads no longer inherit the thick shaft outline.
- Plain-language drawing labels replace the confusing annotation heading.

## 0.4.0

- Native Windows capture/editor and tray workflow with selection dimensions, local PNG/JPEG and clipboard export.
- Settings tabs **Genel / Düzenleyici / Güncellemeler**, System/Light/Dark appearance, keyboard shortcut recording with conflict feedback, native color picker and 1–64 px line width.
- Editor tools: select/move, rectangle, arrow, pen, text, translucent highlighter, numbered steps, blur and opaque redaction; tool grouping, keyboard commands and undo/redo.
- Public release checks with **Yükle / Daha sonra / Bu sürümü atla** consent, SHA-256 and pinned RSA-3072 update signatures, activity-aware installation and executable startup rollback.
- Signed installer and portable Windows x64 package distributed from the public updates-only repository. The initial installation of this update trust model is manual; SmartScreen may warn.

Interactive desktop acceptance remains user-run. Build, packaging and signature verification instructions are in [DISTRIBUTION.md](DISTRIBUTION.md).
