# isolmaSS

A small, native Windows screenshot editor written in Rust. Capture a region or a window, annotate it, then copy or save it. Screenshots stay on your computer; the optional update check contacts GitHub.

Version **0.3.0** introduces a modern light interface across Settings, the editor toolbar, the tray command menu, and application dialogs. A single Windows 11 Fluent token module (`src/theme.rs`) drives the palette (the current user's system accent color, light/dark sets) and the Segoe UI type scale (14 px body, 12 px section labels, 20 px titles); labeled Save/Copy actions remain. No web runtime or new external package was added.

## Capture and edit

1. Start `isolmass.exe`. Its icon appears in the notification area.
2. Press **PrintScreen**, click the tray icon, or choose **Capture now** from its menu.
3. Drag a region, or click a visible window when window snapping is enabled. A magnifier and pixel dimensions help precise selection. Regions must be at least 8 × 8 pixels.
4. Add rectangles, arrows, pen strokes, text, blur, or opaque redaction. Use **Select** to edit existing annotations.
5. Choose **Copy** or **Save**. The exported image excludes the toolbar, selection frame, handles, caret, magnifier, and unfinished drawing previews.

The editor toolbar uses the selected monitor's work area and DPI. Buttons are 36 px tall with Fluent 4/8 px corner radii and Segoe Fluent Icons glyphs (shared MDL2 codepoints, so older Windows still renders them). On short or narrow work areas it switches to a grid so all commands remain reachable. Full-frame presentation is retained for compatibility with applications that hook GDI, including RTSS.

**Redact** covers pixels with an opaque fill, including annotations added underneath it later. Blur is a visual mosaic effect and should not be used to remove confidential information.

## Keyboard shortcuts

| Shortcut | Action |
|---|---|
| PrintScreen | Start a capture using the configured delay |
| V / R / A / P / T / B / M | Select / Rectangle / Arrow / Pen / Text / Blur / Redact |
| Ctrl+C or Enter | Copy the selected screenshot |
| Ctrl+S | Save in the configured format and folder |
| Ctrl+Shift+S | Save as, with a native file picker and overwrite confirmation |
| Ctrl+Z / Ctrl+Y | Undo / Redo |
| Ctrl+, | Open Settings |
| F10 or Apps key | Open the toolbar as a native command menu (keyboard/screen-reader access) |
| Delete / Backspace | Delete the selected annotation |
| Arrow keys | Move the selected annotation; otherwise move the screenshot region |
| Shift+arrows | Move by 10 pixels |
| Ctrl+arrows | Resize the screenshot region when no annotation is selected |
| Shift while drawing | Square rectangles/redaction regions; 45-degree arrow angles |
| Esc or right-click | Cancel text editing, deselect an object, clear the region, then close the editor |
| Esc during a selection drag | Cancel the selection drag; the editor stays open |
| Esc during countdown | Cancel the pending capture |

While editing text, **Enter commits text**; **Ctrl+C and Ctrl+S commit it before exporting**. Ctrl+A, Shift+Left/Right/Home/End, Backspace/Delete, Ctrl+V, and local text Undo/Redo are supported. Text uses the normal Windows character-input path, preserves UTF-16 surrogate pairs and literal ampersands, and uses the same measured font for preview and output. Text is single-line, capped at 16 KiB; pasted line breaks become spaces. Cursor movement is by Unicode scalar value, not grapheme cluster. Complex IME and keyboard-layout scenarios still need interactive verification.

A non-PrintScreen shortcut that is unavailable falls back to Ctrl+Shift+S with a notification. PrintScreen uses the keyboard hook even when the Windows registration is occupied. Repeated capture requests are coalesced while a capture or countdown is active.

## Settings and tray menu

Settings has **General** and **Editor and system** views. It opens at a compact 800 x 600 (96-DPI base) using the shared Fluent metrics, unsaved choices survive navigation, and the window resizes, fits the monitor, scrolls when needed, and brings keyboard focus into view. Native buttons, radio choices and checkboxes retain keyboard/accessibility semantics beneath the custom drawing.

Settings controls include the capture shortcut and delay, PNG/JPEG and JPEG quality, destination folder, annotation defaults, window snapping, whether the editor closes after an action, startup registration, notifications, and update preferences. PNG disables JPEG quality controls. Save failures keep the window open and show an error. Startup registration is restored to its exact previous value if saving settings fails.

`--settings` routes to the running daemon. If none exists, it starts the daemon and opens Settings. A request received during editing waits for the current session to finish. Saved hotkey and update changes are applied to the daemon; editor tool/color/stroke changes are merged into the latest configuration once at the end of a session.

The tray command menu offers Capture now, Settings, Open screenshot folder, Check for updates, five recent captures, and Quit. Recent files are cached; directory refresh runs in the background at most every 30 seconds. A refresh scans for at most two seconds between filesystem calls, so the list is best-effort in large or slow folders. Successful saves update it immediately. Quit and update installation wait for an active editing/settings session to finish. Control commands are deduplicated in a queue bounded at 16 entries, so a command issued during a nested dialog is delivered right after that dialog returns; if the queue fills, the dropped command is recorded in diagnostics.

## Files, configuration and privacy

- Default output: the actual Windows **Pictures** known folder, then `Screenshots` (including redirected Pictures folders).
- Settings: `%APPDATA%\isolmaSS\settings.json`.
- Diagnostics: `%LOCALAPPDATA%\isolmaSS\logs\diagnostic.log`, rotated at about 1 MiB into a single previous file; if rotation fails, new records are appended instead of dropped. Logs contain stages and errors, not screenshots, typed annotation text, or keystroke logs. Error messages can contain local file paths.
- Update downloads: `%LOCALAPPDATA%\isolmaSS\updates`.

Settings JSON is limited to 64 KiB and validated on load/save. Supported limits include stroke width 1–64 pixels, delay 0–5000 ms, and JPEG quality 1–100, and the save folder must be an absolute path without NUL characters. Loading fails closed: a missing file yields defaults, invalid JSON is first copied to `settings.corrupt.json` and only then reset — if that backup fails, recovery aborts and the error is reported — and transient read errors such as sharing violations propagate instead of silently substituting defaults. Saving skips an unreachable save folder, such as an unavailable network share, rather than failing the write. Writes use same-directory temporary files and atomic replacement. Configuration merge/write operations are serialized across app processes.

PNG/JPEG output is explicitly opaque. Normal Save reserves a unique timestamped filename; Save as replaces an existing file only after successful encoding and flushing. Copy transfers a bottom-up 32-bit `CF_DIB` to Windows; ownership transfers only after success. Reading pasted text is bounded to 32 KiB: a failed size query is an error, and truncation is recorded only when text was actually cut.

## Build and verify

Windows x64, Visual Studio C++ Build Tools / Windows SDK, and Rust **1.98.0** are required. `rust-toolchain.toml` pins the compiler; `Cargo.lock` pins dependencies.

```powershell
cargo fmt --check
cargo check --locked
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked -- --test-threads=1
cargo build --release --locked
.\target\release\isolmass.exe --smoke-test
```

The smoke command requires an interactive Windows desktop, a release build, and NSIS 3.10 on PATH or in `%LOCALAPPDATA%\Programs\nsis-3.10`. It exercises clipboard ownership/failure, capture geometry, PNG decoding and opaque pixels, history, text, configuration and fresh installer/version checks. **It changes the clipboard** and briefly registers hooks/tray resources; use a disposable desktop or preserve your clipboard before running it. It does not install or uninstall the application. Tests do not silently stand in for complete manual UX validation.

The Windows CI workflow runs on pull requests and on pushes to `main`. It runs formatting, locked check, lint, tests with `--test-threads=1` while skipping the interactive `test_tray_manager_lifecycle` test, the release build with its executable-size gate, NSIS provisioning, and a non-interactive `package.bat` run that re-checks both size budgets and the recorded installer SHA-256. Its remote execution has not been observed.

```powershell
# Actual capture/window/presentation path, 50 samples, JSON lines; no images saved.
.\target\release\isolmass.exe --benchmark 50

# Packaging; production releases additionally require a code-signing certificate.
cmd /c package.bat
cmd /c release.bat
```

Other commands: `--capture-once`, `--settings`, `--check-update`, `--verify-update PATH`, `--help`, and the legacy `--test-capture` alias for smoke. `--fix-printscreen` explicitly changes Windows' PrintScreen/Snipping Tool registration preference; it is never run automatically.

See [ROADMAP.md](ROADMAP.md) for measured results and remaining verification, [DISTRIBUTION.md](DISTRIBUTION.md) for signing/packaging, and [SECURITY.md](SECURITY.md) for the update trust model.

## License

MIT; see [LICENSE](LICENSE). Upstream notices are included in [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) and the installer. The current local artifacts are unsigned development builds, not a verified signed production release.
