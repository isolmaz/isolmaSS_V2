# isolmaSS

Native Windows screenshot editor in Rust. Capture a region or window, annotate it, then copy or save it locally. There is no screenshot upload or telemetry. Version **0.4.0**.

## Use

1. Start `isolmass.exe`; it remains in the notification area. Launching it again opens the running instance's Settings rather than an installer.
2. Press **PrintScreen**, click the tray icon, or choose **Capture now** from the native tray menu. The capture shortcut and delay are configurable.
3. Drag a region of at least 8 × 8 pixels, or click a visible window with window snapping enabled. The selection shows its pixel dimensions without covering the image with a magnifier.
4. Annotate with **Seç/taşı**, **Çerçeve**, arrow, pen, translucent highlighter, text, numbered steps, blur/pixelation or opaque **Karart**. Secondary tools are grouped in the toolbar's F10/Apps command menu. Select an annotation to move, recolor, adjust its width, resize where applicable, or delete it. Undo and redo include edits.
5. Choose **Copy**, **Save** or **Save as**. Editor chrome, handles, selection frame, caret and unfinished previews do not appear in the exported image.

**Karart** replaces covered pixels opaquely and is rendered after other annotations. Blur and highlighter are visual effects, **not** secure redaction. Inspect the exported image before sharing sensitive material.

### Keyboard

| Shortcut | Action |
|---|---|
| Configured capture shortcut (default PrintScreen) | Start capture using the configured delay |
| V / R / A / P / T / H / N / B / M | Select, rectangle, arrow, pen, text, highlighter, numbered step, blur, opaque redaction |
| Ctrl+C or Enter | Copy the selection |
| Ctrl+S / Ctrl+Shift+S | Save / Save as |
| Ctrl+Z / Ctrl+Y | Undo / redo |
| Ctrl+, | Settings |
| F10 or Apps | Native toolbar command menu |
| Delete or Backspace | Delete selected annotation |
| Arrow keys / Shift+arrows | Move selection or object by 1 / 10 pixels |
| Ctrl+arrows | Resize selection when no object is selected |
| Shift while drawing | Square rectangle/redaction; 45-degree arrow angles |
| Esc or right-click | Cancel the current edit, deselect, clear the selection, or close the editor |

While editing text, Enter commits; Ctrl+C and Ctrl+S commit before exporting. Text selection, clipboard paste and local undo/redo are supported. Text is single-line and limited to 16 KiB; pasted newlines become spaces. The toolbar width slider and its numeric button accept **1–64 px**; click the number, type a value and press Enter, or press Esc to discard it. The editor also offers the native Windows color picker.

## Settings and updates

Settings has **Genel / Düzenleyici / Güncellemeler** tabs, a fixed header and footer, and scrollable content. Appearance can follow Windows or be explicitly light/dark. The general tab contains shortcut recording, capture delay, save location/format, startup and notification preferences. The editor tab contains annotation color, 1–64 px width, window snapping and close-after-action. The updates tab controls checks at startup and offers a manual check.

Record a shortcut using Ctrl/Alt/Shift/Win with a letter, digit, F1–F24 or PrintScreen; bare PrintScreen also works. Unsupported keys are ignored. An unavailable combination is reported rather than silently replacing the current shortcut. On a startup conflict, the app asks whether to use Ctrl+Shift+S for that session. Settings remain open after a save error.

Update checks use public releases from [`isolmaz/isolmaSS-updates`](https://github.com/isolmaz/isolmaSS-updates). A newer release offers **Yükle / Daha sonra / Bu sürümü atla**; installation always requires an explicit choice. The skipped version stays skipped for automatic checks; a manual check can offer it again. Installation waits for the active editor/settings session and any copy/save action to finish, verifies the signed installer again, then restarts. The installer keeps a rollback executable until the installed app passes a startup check. See [SECURITY.md](SECURITY.md) and [DISTRIBUTION.md](DISTRIBUTION.md).

The signed-update mechanism begins with version 0.4.0. An installation that cannot verify this release's pinned signature needs **one manual installation** of 0.4.0 before subsequent in-app updates. The free application-level signature does not remove Windows SmartScreen warnings; do not bypass a warning automatically. Portable ZIPs are for manual use, not in-place updates of an installed copy.

The native tray menu provides Capture now, Settings, Open screenshot folder, Check for updates, recent captures and Quit. Quit waits until an active edit/settings session ends.

## Files and privacy

- Default output: Windows Pictures known folder → `Screenshots`.
- Configuration: `%APPDATA%\isolmaSS\settings.json` (64 KiB limit, validated, atomically saved under a cross-process lock).
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
