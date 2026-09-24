# isolmaSS

Native Windows screenshot editor in Rust. Capture a region or window, draw on it, then copy/save locally or explicitly upload to a Worker in **your own Cloudflare account**. Upload is disabled until configured; isolmaSS operates no shared screenshot service and collects no in-app analytics. Version **0.5.0**.

## Use

1. Start `isolmass.exe`; it remains in the notification area. Launching it again opens the running instance's Settings rather than an installer.
2. Press **PrintScreen**, click the tray icon, or choose **Capture now** from the native tray menu. The capture shortcut and delay are configurable.
3. Drag a region of at least 8 × 8 pixels, or click a visible window with window snapping enabled. Resize from the four corners or four edge midpoints; drag the dimensions label above the top-left corner to move the entire selection.
4. Use the four main tools (**Seç/taşı**, **Çerçeve**, arrow, pen) or click the chevron for highlighter, text, numbered steps, blur/pixelation and opaque **Karart**. F10/Apps also exposes every tool. Select a drawing to move, recolor, adjust its width, resize where applicable, or delete it. Undo and redo include edits.
5. Choose **Copy**, **Save** or **Save as**. Editor chrome, handles, selection frame, caret and unfinished previews do not appear in the exported image. **Upload** is available after pairing your own Cloudflare Worker: it sends only the flattened selection, keeps the editor open during transfer and copies the link after the Worker confirms success. A failed transfer leaves the clipboard unchanged.

**Karart** replaces covered pixels opaquely and is rendered after other annotations. Blur and highlighter are visual effects, **not** secure redaction. Inspect the exported image before sharing sensitive material.

### Keyboard

| Shortcut | Action |
|---|---|
| Configured capture shortcut (default PrintScreen) | Start capture using the configured delay |
| V / R / A / P / T / H / N / B / M | Select, rectangle, arrow, pen, text, highlighter, numbered step, blur, opaque redaction |
| Ctrl+C or Enter | Copy the selection |
| Ctrl+S / Ctrl+Shift+S | Save / Save as |
| Ctrl+U | Upload to your paired Cloudflare Worker; copy the successful share URL |
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

Settings has **Genel / Düzenleyici / Güncellemeler** tabs and a dedicated **Cloudflare** settings window, a compact two-column general page, and a window that fits each original tab's content until you resize it yourself. Smaller displays still scroll the content without moving the tabs or Save/Cancel. Appearance follows Windows or can be explicitly light/dark. General contains shortcut recording, capture delay, save location/format, startup and notification preferences. Editor contains drawing color, 1–64 px width, window snapping and close-after-action. Updates shows the check result beside its button and a progress bar below while checking.

Record a shortcut using Ctrl/Alt/Shift/Win with a letter, digit, F1–F24 or PrintScreen; bare PrintScreen also works. Unsupported keys are ignored. An unavailable combination is reported rather than silently replacing the current shortcut. On a startup conflict, the app asks whether to use Ctrl+Shift+S for that session. Settings remain open after a save error.

Update checks use public releases from [`isolmaz/isolmaSS-updates`](https://github.com/isolmaz/isolmaSS-updates). A manual check in Settings displays a live progress indicator and its result inline; a tray-initiated check immediately announces progress and reports the result in a native dialog. A newer release offers **Yükle / Daha sonra / Bu sürümü atla** and warns before installation that isolmaSS will close and reopen; installation always requires an explicit choice. The skipped version stays skipped for automatic checks; a manual check can offer it again. Installation waits for the active editor/settings session and any copy/save action to finish, verifies the signed installer again, then restarts. The installer keeps a rollback executable until the installed app passes a startup check. See [SECURITY.md](SECURITY.md) and [DISTRIBUTION.md](DISTRIBUTION.md).

The signed-update mechanism begins with version 0.4.0. An installation that predates the pinned verifier needs **one manual installation** of 0.4.0 or later before subsequent in-app updates. The free application-level signature does not remove Windows SmartScreen warnings; do not bypass a warning automatically. Portable ZIPs are for manual use, not in-place updates of an installed copy.

The native tray menu provides Capture now, Settings, Open screenshot folder, Check for updates, recent captures and Quit. Quit waits until an active edit/settings session ends.

## Optional Cloudflare upload

There is **no central isolmaSS image host or sign-in**. Each user who wants Upload provisions their own Worker/R2/D1 installation. Open **Settings → Cloudflare**, generate separate upload and administrator tokens, open the [Deploy to Cloudflare template](https://deploy.workers.cloudflare.com/?url=https://github.com/isolmaz/isolmaSS_V2/tree/v0.5.0/cloudflare), approve Cloudflare's own account/resource/R2 activation prompts, paste the tokens into Cloudflare's secret fields, then paste the new HTTPS Worker origin back into the app and choose **Eşleştir**. The template uses the public source release tag. Cloudflare prompts and any R2 subscription cannot be bypassed by the app. For custom domains, attach the domain to your Worker and pair the new origin. The native app retains no Cloudflare account-level API key.

**Cloudflare** offers separate controls for the last N active images (50 initially), per-image bytes, total stored bytes, daily uploads/views, retention days and the warning percentage (90% initially). At that threshold choose *warn*, *block new uploads* or *block new uploads and viewing*. The panel lists recent images for deletion and displays daily/monthly usage, occupied space, daily quota percentages and an illustrative paid-plan estimate. Counts cover this Worker only, not other Cloudflare projects; even rejected Worker requests and R2 billing can incur costs. **No option guarantees a zero invoice.** Daily counters use UTC. Enabling a default image password sends the password over HTTPS with future screenshots; only a salted verifier is stored in D1. Recipients need the password separately. A valid unprotected link is visible to anyone who has it. Background cleanup marks expired or evicted images unavailable immediately and retries R2 deletion; previously downloaded copies cannot be revoked. See [Cloudflare deployment and API](cloudflare/README.md) and [the website](site/README.md).

## Files and privacy

- Default output: Windows Pictures known folder → `Screenshots`.
- Configuration: `%APPDATA%\isolmaSS\settings.json` (64 KiB limit, validated, atomically saved under a cross-process lock). It contains the Worker origin, but **no access token**.
- Cloudflare upload/admin tokens and optional default image password: `%LOCALAPPDATA%\isolmaSS\cloud-credentials.bin`, protected for the current Windows user with DPAPI. Disconnecting locally does not delete remote images.
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

Current verification and deferred features are tracked in [ROADMAP.md](ROADMAP.md). MIT license: [LICENSE](LICENSE); dependencies: [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
