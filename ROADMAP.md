# isolmaSS implementation and verification status

Updated **2026-09-22** for the **0.3.0 working tree**. This is an unreleased development build. [README.md](README.md) describes current behavior; [CHANGELOG.md](CHANGELOG.md) preserves the earlier history. A completed implementation row does not imply that all its interactive or release acceptance checks have passed.

## Hardening pass — 2026-09-22

A source-verified hardening round was applied before commit. `cargo fmt --check`, `cargo check --all-targets --locked`, `cargo clippy --all-targets -- -D warnings` and `cargo build --release --locked` passed on the final tree. Unit tests and the full smoke command were explicitly skipped this round (the user directed test work off), packaging was not re-run, and remote CI remains unobserved. Three corrected behaviors were re-exercised on the release binary: cancelled tiny selections dismiss instead of committing, Escape during a selection drag cancels the drag while keeping the editor open, and an exclusively locked settings file now fails loudly (exit 1, os error 32) without modifying the file instead of silently loading defaults.

Verified fixes, each confirmed in the working-tree source:

- **Settings:** `Settings::load_with_warning()` fails closed — transient read errors propagate, only a missing file yields defaults, invalid JSON is copied to `settings.corrupt.json` before any reset and a failed backup aborts recovery; `save_directory` must be absolute and NUL-free; an unavailable network folder is skipped on save instead of failing it.
- **Clipboard:** text reads error on a failed size query, are bounded to 32 KiB, and record truncation only when text is actually cut.
- **Locks and logs:** `SettingsLock` distinguishes `WAIT_FAILED` (with the captured Win32 error) from a lock timeout; diagnostic rotation falls back to appending when the rename fails so no record is dropped.
- **Build:** `build.rs` discovers `rc.exe` by numeric SDK version under the `WindowsSdkDir` and ProgramFiles roots, honors the `RC` override and `rc.exe` on `PATH`, and reports a descriptive launch error listing what was searched.
- **Packaging:** `package.bat` resolves makensis as `MAKENSIS` environment variable, then `PATH`, then the highest versioned `%LOCALAPPDATA%\Programs\nsis-*` directory; both artifacts are gated on the three numeric components of `ProductVersion` and `FileVersion`; `installer.nsi` writes the three-part `${PRODUCT_VERSION}` as the string `FileVersion` while `VIProductVersion` keeps its required four-part padding.
- **CI:** push limited to `main` (plus pull requests), tests run with `--test-threads=1 --skip test_tray_manager_lifecycle`, NSIS is provisioned, and a non-interactive `package.bat` step enforces size and SHA-256 gates. None of this has been observed remotely.
- **Advisories:** the comparison was refreshed against RustSec snapshot `57ad4063bb49c1deb04b6fcee30cfbac6b508474` (fetched 2026-09-21, 1,238 crate records) using parsed-TOML metadata and range comparison without cargo-audit; only RUSTSEC-2022-0008 (windows, patched in 0.58.0) matched and no affected locked package was found. Scope limits remain as stated in SECURITY.md.
- **Overlay:** Escape during a selection drag cancels the drag instead of closing the editor; a cancelled tiny or click selection is no longer returned as a committed result; arrow nudges that move nothing record no undo entry; toolbar tooltip text is measured with the same 12-pixel font that draws it; the Settings heading control's font is restored on DPI change; modal dialogs route Escape to `WM_CLOSE`; `ui::confirm` falls back to `MessageBoxW` when the task dialog is unavailable.
- **Tray and hotkey:** the control-command queue deduplicates with a hard capacity bound (a drop is recorded in diagnostics) and delivers commands issued during nested dialogs after those dialogs return; a hotkey change that fails — including a failed rollback of the saved preference — leaves the daemon running without a shortcut and notifies instead of exiting; listener registration, fallback, hook and wait failures are logged through diagnostics; hook state is published before the keyboard hook is installed and cleared if installation fails.
- **Updater:** cancellation is set under the lock that publishes worker results, so a concurrent cancel discards or clears pending results instead of letting them surface later; failed automatic download attempts report through diagnostics and a tray notification rather than a modal error.

Rejected claims from this round:

| Claim | Disposition |
|---|---|
| Moving a selection moves all its annotations — reported as a bug | Rejected: intentional. A committed selection contains its annotations, so arrow-nudge and drag moves translate them together, recorded as one history entry. |
| NSIS `Delete` of an absent file can abort uninstall by setting the error flag | Rejected: per the official NSIS reference, `Delete` does not set the error flag when the file does not exist. |
| `find_window_at_point` is dead code and should be removed | Rejected: it is kept for the smoke command's window-snap hit check. |
| Settings Escape was already handled through `IsDialogMessage` | Rejected: the dialog loop converts Escape to `WM_CLOSE` before dispatch; `IsDialogMessageW` handles navigation and focus only. |

The remaining release gates below (signed install/rollback, alternate-DPI visual pass, remote CI observation and latency re-measure) are unchanged and still open.

## Interface and product changes

The older interface has been replaced across the main application surfaces:

- **Settings:** a resizable light window with a sidebar, General / Editor and system views, rounded cards, Windows 11 Fluent colors from `src/theme.rs` (system accent, light/dark token sets), a 14/12/20 px Segoe UI type scale, and a Save changes / Cancel footer available in both views. Work-area clamping, scrolling, focus scrolling and DPI font/layout updates keep small windows usable.
- **Editor:** larger tool targets, named Save / Copy actions, consistent selected/hover states and tooltips. The normal L-shaped arrangement switches to a compact grid when the selected monitor cannot accommodate it.
- **Tray:** a custom native command menu with keyboard navigation, recent captures, the actual active capture shortcut, Settings, folder access, update and quit actions.
- **Dialogs and setup:** native Windows task/file/folder dialogs and a matching Segoe UI/light installer treatment. No browser runtime or new external package was introduced.

Opaque Redact, a selection magnifier and pixel dimensions, arrow-key positioning, richer single-line text editing, Save as and opening the last save folder are implemented. The design uses native controls where their keyboard/accessibility semantics are useful. A complete screen-reader/high-contrast review has not been performed.

**Visual verification remains incomplete.** An intermediate Settings view was inspected, but the user stopped Computer Use with physical Escape before the final interface pass. No subsequent computer-control workaround was used. The final Settings/tray/editor surfaces, alternate DPI and all interaction paths must still be inspected together. Compilation and geometry tests are not a substitute for this review.

## Native W11 UI refresh — 2026-09-22

Phased plan approved by the user: a native Windows 11 (Fluent) look, smaller/denser surfaces, the system accent color, Mica + dark mode, and a commit after every phase with docs kept in sync.

- **Faz 0 — Fluent token layer:** new `src/theme.rs` is the single source for colors and typography (upcoming phases add metrics): light/dark Fluent palettes, system accent read from `AccentColorMenu` with documented Fluent fallbacks, theme detection via `AppsUseLightTheme`, readable on-accent text, and the 14/12/20 px Segoe UI type scale. The settings window no longer hardcodes its palette; every color resolves through the token module, and its fonts moved from point math to the shared pixel scale (body 15 px -> 14 px, title 27 px -> 20 px). Window metrics, toolbar metrics/icons, Mica/dark wiring and the tray/dialog pass arrive in Faz 1-4 as each consumer lands.
- **Faz 1 — compact Settings geometry:** window 960x820 -> 800x600 (96-DPI base, still work-area clamped), sidebar 180 -> 144 px (the longest nav label, "Editor and system", was measured at 110.3 px so it cannot clip), and all layout constants now come from the new `src/theme.rs` metrics block (`PAGE_MARGIN` 24, `CARD_PADDING` 16, `CONTROL_HEIGHT` 32, `GRID` 4, `RADIUS_CARD` 8): card/button radii 16/14 -> 8, footer buttons 42 -> 32, row height 36 -> 32, card inset 20 -> 16. Both views keep every control (JPEG quality got its own stacked row at the narrower width); the vertical scroll extent is 712 px so the existing scrollbar and focus-scroll cover the footer, with no horizontal scrollbar. Label/chip widths were measured against the new columns before committing the layout.

- **Faz 2 — toolbar metrics, Fluent Icons, HUD retoken:** `Toolbar::TOOL_BUTTON` 40 -> 36, `ACTION_HEIGHT` 52 -> 48, action buttons 40 -> 36; panel radius 14 -> `RADIUS_CARD` (8), button/swatch radius 10/8 -> `RADIUS_CONTROL` (4), tooltip radius 8 -> 8-card token. Unicode glyphs (`↖ □ ↗ ✎ ⚙ ✓` and text Save/Copy) replaced with Segoe Fluent Icons codepoints drawn by a new `crate::drawing::icon` helper (face `"Segoe Fluent Icons"`, keyable in the GDI font cache): Select E7C4, Rectangle E799, Arrow E72A, Pen E70F, Text E90A, Undo E7A7, Redo E7A6, Save E74E, Copy E8C8, Settings E713, Cancel E711, check E73E — every codepoint verified present in both Microsoft glyph tables (Fluent + MDL2) so Windows 10 fallback renders; Blur stays a Segoe UI glyph because no shared square-grid icon exists. Overlay HUD/tooltip/magnifier chrome colors now resolve through `theme::tokens()`. The F10/Apps command menu (hardening pass) is now documented in README's shortcut table.

- **Faz 3 — Mica, dark title bar, live theme:** `theme::apply_window_theme` sets `DWMWA_SYSTEMBACKDROP_TYPE` (38 = Mica only when `backdrop_supported()`, i.e. build >= 22621 read from HKLM `CurrentBuildNumber`; otherwise value 1 = none, a no-op) and `DWMWA_USE_IMMERSIVE_DARK_MODE` (20, legacy fallback 19) driven by `AppsUseLightTheme`. Under Mica the Settings window skips only its page-background fill (`paint_settings_surface` + `WM_ERASEBKGND` guard on `state.mica_active`); cards and controls still paint. The two cached HBRUSHes are now recreated by `SettingsWindowState::refresh_brushes` at init and on `WM_SETTINGCHANGE` (AppsUseLightTheme -> theme swap; any other string -> accent cache invalidation) and `WM_DWMCOLORIZATIONCOLORCHANGED`, followed by full invalidation — so the window flips light/dark live without restarting. Fonts are theme-independent and never rebuilt. Pre-Win11 systems behave exactly as before (solid page, no backdrop).

## Audit findings: implementation and evidence

The numbers correspond to the 20 findings in the 2026-09-05 source audit of 0.2.0. All have implementation changes in this branch; the evidence column states the actual verification boundary.

| # | Finding and implemented correction | Evidence / remaining verification |
|---|---|---|
| 1 | Toolbar hover survives tool/context changes safely; missing targets are cleared and tooltip lookup cannot panic. | Existing toolbar tests exercise the missing-hover rendering case. Interactive hover/tool/delete transitions remain in the final UX pass. |
| 2 | Settings dimensions fit the monitor; valid clamp ranges, scrolling and DPI relayout replace fixed-size assumptions. | Code/build checks. Actual Settings at 125%, 150%, 200% and mixed DPI remains open. |
| 3 | Region creation and resizing share an 8-pixel minimum; edge and tiny bounds are guarded. | Existing capture/annotation geometry tests pass, including tiny-edge resizing. |
| 4 | Windows have explicit ownership; userdata is cleared at destruction, child loops preserve application quit, and installation/exit wait for editing. Background services are joined on normal and error exits. | Tray lifecycle tests and repeated real capture-window creation/closure pass. Nested quit during Settings/text and fault paths still need runtime verification. |
| 5 | NSIS recognizes a valueless UPDATE flag, waits on the old process, stages replacement and attempts executable rollback. | A fresh NSIS compile and matching embedded version pass. Actual update, lock, timeout, rollback and restart scenarios have not been run. |
| 6 | Shared command routing sends Ctrl+C / Ctrl+S through text commit and export. Ctrl+, also works during a text edit. | Hook routing and text behavior checks pass. Interactive text-to-export equivalence remains open. |
| 7 | V and M join all other tools in one command mapping; native and hook modifier handling agree and AltGr is excluded from editor commands. | Existing hook tests and smoke routing checks pass; keyboard-layout interaction still needs verification. |
| 8 | Modal input suspension covers Settings, file dialogs, error/confirmation dialogs and the tray menu. | Hook suspension checks pass. Full nested-dialog interaction remains open. |
| 9 | Clipboard allocations have RAII ownership, transferred only after success; standalone copy has an owner window. | Smoke: 32 failed 8 MiB copy attempts while another thread held the clipboard, followed by successful copy. Private-memory growth stayed below the 24 MiB leak-detection threshold. Original clipboard data was restored by the local verification harness. |
| 10 | Exported PNG/JPEG and clipboard pixels are explicitly opaque. Replacement is atomic after successful encoding. | Smoke decoded all 60,000 pixels of a PNG created from zero-alpha input and checked RGB plus alpha 255. A failed replacement preserved the previous file bytes. Complete annotated-scene export parity remains open. |
| 11 | Capture flushes GDI before CPU reads; mixed CPU/GDI scene effects synchronize before accessing the same pixels. | Real capture/dimming smoke and the capture benchmark passed. Final visual RTSS/no-trails verification remains open. |
| 12 | Delete restores original layer index; selection movement participates in history; a new selection resets history. Re-editing text preserves its original object and empty completion records a deletion. | Existing history order/translation tests pass; text editing smoke passes. The full interactive undo matrix is not complete. |
| 13 | Preview/output share measured Segoe UI text rendering, preserve literal ampersands, and assemble UTF-16 surrogate pairs. Text supports selection, paste and local undo/redo. | Glyph-width, surrogate, Unicode editing and length-limit checks pass. Cursor movement is by scalar value; grapheme-aware editing and full IME behavior are not implemented/verified. |
| 14 | Blur follows annotation order; opaque Redact is applied over the final annotation pixels. | Pixel-level redaction test passes. Full copy/save layering and interactive mask transforms remain to be checked. |
| 15 | Hook initialization reports failure; partial startup resources are cleaned up, listener shutdown signals and joins its thread, and fallback shortcut identity is shown. | Real listener startup/shutdown and routing checks pass. Forced hook failure and fallback-conflict recovery were not exercised interactively. |
| 16 | Settings load/save validates fields and a 64 KiB limit; recovery is visible. Startup rollback preserves the exact previous registry type/value. | Existing configuration validation, serialization, isolated persistence and error tests pass. Registry rollback under a real persistence failure remains open. |
| 17 | Settings routes to the single daemon; saved preferences apply there. Editor changes merge only tool/color/stroke fields under a cross-process lock. Stale capture requests are discarded when a session ends. | Build and configuration checks pass. Multi-instance simultaneous save/hotkey interaction remains open. |
| 18 | Windows trust is followed by a publisher public-key check, strict version/resource matching and a defined rotation policy; network hosts/redirects are bounded. | The unsigned local build rejected Edge's otherwise valid embedded Authenticode signature as an unexpected publisher. A successful signed isolmaSS update and signed key rotation remain unverified. |
| 19 | Smoke always rebuilds setup with the Cargo version and verifies artifact versions. Packaging validates both size limits and computes a checked SHA-256; checksum failures stop packaging. | Smoke's fresh NSIS rebuild, package.bat and an independent Python SHA-256 comparison passed. Signing is a separate gate. |
| 20 | One tracked update worker, cancellation, unique temporary files, bounded transport and UI-side installation replace detached jobs. Uninstall waits and preserves registration if executable deletion fails. | Code/build checks and publisher rejection passed. End-to-end network cancellation and install/uninstall failure injection remain open. |

## Performance changes

| Audit recommendation | Implementation | Measurement boundary |
|---|---|---|
| Reduce repeated scene composition | WM_PAINT coalescing and a lazy unchanged-annotation frame cache capped at 32 MiB; full-frame presentation is retained. | Per-frame cache parity and long editing FPS have not been measured. |
| Coalesce preference writes | Tool/color/stroke preferences are merged and flushed once at session end, with visible failures. | Code path changed; storage latency and interactive persistence still need measurement. |
| Bound capture requests and avoid blocking countdown | Capacity-one hotkey channel, bounded daemon queue, duplicate/stale suppression and a cancellable Windows timer. | Routing tests pass; rapid-input/countdown UI stress remains open. |
| Release large buffers | The dimmed-buffer pool retains at most 4 MiB; larger capture and editor buffers are released with their session. | Immediate post-capture private/working-set measurements are below. A new 60-second daemon idle run and long editing session remain open. |
| Reduce pen work | Near-identical points are coalesced, a stroke is capped at 32,768 points, and committed Win32 geometry is cached. | Long-stroke visual fidelity/frame-time comparison remains open. |
| Bound GDI resources | Shared thread-local cache holds at most 48 font/pen/brush objects; overflow objects are temporary and text metrics are session-cleared. | Repeated capture resource lifetimes were exercised; a GDI-count stress measurement across every UI surface remains open. |
| Avoid a redundant closing frame | Successful copy/save skips editor recomposition when the session will close; failures restore the editor frame. | Smoke covers output and failed replacement; interactive failure restoration remains open. |
| Make recent captures asynchronous | Bounded recent-file cache, immediate save updates and a tracked background refresh replace menu-time directory scans. | Refresh has a two-second budget checked between filesystem calls; a blocked network filesystem call can exceed it. Large/network-folder menu timing is unmeasured. |

### Capture profile comparison

Measured on **2026-09-05**, Windows x64, Rust **1.98.0**, two 1920×1080 displays forming a **3840×1080** virtual desktop with origin (0, 0). MSI Afterburner and RTSS were running. Per-monitor DPI and refresh rate were not recorded, so this is not a fully specified hardware benchmark or a cross-device result.

The same capture/window/presentation path was built with release optimization z, 2 and 3. Each profile received **50 samples in one process** and **50 fresh-process samples**, 300 captures total. Fresh-process samples were interleaved by profile; the OS/file cache was not flushed. These are process-cold samples, not a cold-boot benchmark. The p50 is the lower middle ordered observation and p95 uses the nearest-rank definition.

| Optimization | EXE bytes at comparison | Repeated p50 | Repeated p95 | Process-cold p50 | Process-cold p95 |
|---|---:|---:|---:|---:|---:|
| z | 627,712 | 61.899 ms | 71.575 ms | 63.229 ms | 65.007 ms |
| 2 | 836,608 | 48.030 ms | 55.226 ms | 53.037 ms | 59.711 ms |
| **3 — selected** | 883,200 | 48.210 ms | 51.016 ms | 54.809 ms | 58.011 ms |

Optimization 3 reduced repeated p95 by approximately 28.7% relative to z in this comparison while staying comfortably within the executable-size budget. Sampling variability is visible: a subsequent 50-sample build confirmation measured **p50 47.940 ms / p95 54.093 ms / maximum 55.247 ms**. The comparison binaries preceded small subsequent non-capture corrections; their byte counts are not the final package byte counts.

The timer starts synthetically just before capture. Completion requires an actual visible HWND, UpdateWindow and DwmFlush; each measurement closes its own window. It does **not** measure a physical PrintScreen event, daemon queue latency, every input-ready stage, or editing FPS. The production event timestamp is now carried through the capture path for later physical-input measurements. Pixels stay in memory and no screenshot files are written by this diagnostic.

**The ≤30 ms latency target is still not met.** The old 0.2.0 single-event result of 48.936 ms used a different trigger and is not a valid before/after comparison with these distributions.

The selected-profile comparison had immediate post-capture private usage below 3 MiB and working sets around 30 MiB. The subsequent confirmation observed maximum private usage **2,936,832 bytes** and working set **31,277,056 bytes** after closing the overlay. Working set was not forcibly trimmed. This does **not** establish the 15 MiB idle-daemon target; the historical 2026-09-04 60-second result applies only to 0.2.0.

Raw JSON and temporary comparison binaries are local ignored artifacts under `target/performance`; they are not committed or included in the installer. `--benchmark 50` reproduces the repeated diagnostic on the current build.

## Verification performed for 0.3.0

- Formatting, locked/offline check, Clippy with warnings denied, and release compilation passed using the installed stable alias of the exact pinned Rust 1.98.0 compiler.
- The full existing unit suite passed: **21 passed, 0 failed**, including the real Windows tray lifecycle test with desktop access.
- The existing full smoke command passed with desktop access. It covers its listed behavior checks, capture/dimming, clipboard failure/success, PNG decoded pixels, failed output replacement, text/history/configuration, GUI PE subsystem, tray resources and a fresh NSIS installer build/version check. It does not establish complete GUI usability or the open runtime budgets.
- `package.bat` passed after correcting a checksum error-propagation defect exposed by this environment. The checksum was independently compared with Python hashlib. Local executable and installer remain **unsigned**.
- The wrong-publisher negative check described above passed; no signing certificate was available for the successful publisher path.
- All 27 locked registry package/version entries were compared against all 1,219 crate records in the official RustSec database snapshot `5a0ebedfe8bdd2e295b171f4162f8c977bcad9a5` on 2026-09-05. The only matching advisory, RUSTSEC-2022-0008 for windows, is patched in the locked version 0.58.0. No affected locked package was found. The check used parsed TOML metadata and a direct range comparison, not cargo-audit/cargo-deny; this comparison was refreshed on 2026-09-21 — see the hardening section above. Scope and source links are in SECURITY.md.
- The Windows CI workflow was added, but remote CI was not run. No installation, uninstall, push or release publication was performed.

The final local artifact sizes/checksums are recorded in the 0.3.0 changelog verification entry. They describe the local unsigned artifacts; signing changes the file hashes and may change sizes.

## Remaining release gates and future candidates

Before calling 0.3.0 release-verified:

1. Complete the final visual and keyboard/accessibility pass for Settings, the tray menu and all editor tools, including mixed DPI, text/IME, normal/full-monitor regions, clipboard/save errors and RTSS/no-trails behavior.
2. Exercise graceful/nested shutdown, rapid capture/countdown requests, multi-instance preference updates, startup rollback and both update cancellation preferences on a disposable Windows desktop.
3. Exercise installation/update/uninstall with the real publisher certificate, valueless UPDATE, a slow/locked old process, failed replacement, executable rollback and successful restart on a disposable Windows VM.
4. Re-measure physical-event-to-visible latency with complete display metadata, decide on the next capture-path change for the still-open 30 ms target, and repeat current idle/long-session memory and rendering/GDI/menu budgets.
5. Observe remote CI before publishing; the dependency advisory comparison was refreshed on 2026-09-21 and must be repeated against the then-latest database at release time.

OCR, pin-to-screen, cloud upload/custom domains/QR/delete tokens, HDR/DXGI capture, magnetic guides, counters, shutter sound, auto-save-on-copy and high-DPI output downscaling remain future candidates. They were not represented as completed features in this change. The current work keeps the native Win32/GDI architecture and does not add new dependencies to implement those larger features.
