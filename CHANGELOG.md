# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased] — 0.3.0

### Added

- Modern light Settings, editor toolbar and tray command menu, with rounded cards, Segoe UI, indigo accents, larger targets and labeled actions. Settings has General / Editor and system navigation, resizing, scrolling and DPI updates; the toolbar has a compact grid fallback.
- Opaque Redact, selection magnifier/dimensions, keyboard positioning, richer single-line text selection/paste/undo, Save as and opening the last save folder.
- MIT license, third-party notices, signing/distribution and security documentation, pinned Rust 1.98.0 and Windows CI.
- Cargo-derived Windows file versions and a capture-to-visible benchmark command with explicit measurement boundaries.

### Fixed

- Stale toolbar hover panics, small-region resize bounds and Settings work-area clamp failures.
- Child-window state lifetime and nested quit handling, modal input isolation, text export shortcuts, V/M routing, AltGr command handling and Ctrl+, while editing text.
- Clipboard ownership on failure, opaque PNG/DIB output, capture GDI synchronization, text glyph measurement/surrogate handling, annotation history order and selection translations.
- Blur layer ordering; final opaque masks also cover later annotations.
- Hook initialization/cleanup errors, real fallback shortcut display, stale queued captures, live daemon preferences and editor-only configuration merges. Startup rollback preserves the exact previous registry value.
- Update publisher identity, transport/worker bounds, cancellation, version checks, unique downloads and verification immediately before execution.
- NSIS UPDATE detection, old-process waiting, executable staging/rollback and uninstall refusal when the application cannot be removed.
- Fresh smoke installer compilation with the source version. Packaging now validates final artifact sizes/versions and refuses failed checksum generation instead of printing false success.

### Performance

- Coalesced scene drawing, bounded unchanged-scene and GDI caches, deferred editor preference writes, timer-based countdowns and bounded capture queues.
- Large capture-buffer release, pen point coalescing/caps and cached geometry, background recent-file refresh, and skipped closing-frame composition.
- Selected release optimization 3 after 50 repeated and 50 process-cold captures per profile. Repeated p95 was 51.016 ms versus 71.575 ms for z in that comparison; the 30 ms target remains open. See ROADMAP.md for the subsequent confirmation, memory figures and unmeasured scopes.

### Verification — 2026-09-05

- Formatting, locked/offline check, warning-free Clippy, 21 unit tests including tray lifecycle, release build, full existing Windows smoke command and NSIS packaging passed.
- Smoke decoded PNG pixels/alpha, checked failed replacement preservation and repeated busy-clipboard failures; the local harness restored clipboard contents.
- Wrong-publisher rejection was exercised with Edge's otherwise valid embedded signature. The artifacts are unsigned; a successful signed isolmaSS update and real install/uninstall fault scenarios remain unverified.
- Final UI verification was stopped by the user with Escape. Alternate DPI and complete interactive UX validation remain open. Remote CI was not run.
- All 27 locked registry package/version entries were compared with the 1,219 crate advisories in RustSec snapshot `5a0ebedfe8bdd2e295b171f4162f8c977bcad9a5`; no affected locked package was found. The only matching advisory is patched in windows 0.58.0. This used direct database metadata/range checks; cargo-audit/cargo-deny were not run.
- Local unsigned executable: **888,832 bytes**; SHA-256: `0c05a0a46725deb59322a41a3d5807c09eb48b3dde72d2489c9fe1ba9149a66e`.
- Local unsigned installer: **417,203 bytes**; SHA-256: `01e67dd9598fee6b33b4916d8452e22177769cc66c2ac72cfb88d9bd54c77490`. Both embed version 0.3.0 and pass their 2.5 / 3 MiB size limits.
- Detailed audit disposition and remaining quality gates are in ROADMAP.md.

## 0.2.0 working-tree history — 2026-09-04

The entries below describe the earlier checkout and its verification at that time. They are retained as history, not evidence for the 0.3.0 interface or a claim that either version was published.


### Added
- **Explicit Select and annotation transforms**:
  - Added a pointer **Select** tool at the rail's bottom-nearest position with the `V` shortcut. Drawing Rectangle, Arrow, Pen, Text, or Blur commits without auto-selecting and leaves that drawing tool active.
  - Restricted existing annotation selection, interior dragging, resizing, recoloring, rethickening, deletion, and text double-click editing to Select mode. Screenshot-region border move/resize is Select-only as well.
  - Added visible type-appropriate resize handles for rectangle/blur bounds, arrow endpoints, scaled pen geometry, and proportional text size/position, with safe extents and practical screenshot-bound clamping. Each completed transform records one undoable Modify command on mouse-up.
  - `Ctrl+C` now commits active text editing before copying a flattened screenshot without selection handles.
- **Production Testing & Verification Hardening (Advisory 1)**:
  - Added testable production methods to `TextEditState`: `insert_char`, `backspace`, `delete`, `move_left`, and `move_right`.
  - Added hierarchical Escape action enum and method `OverlayState::handle_escape_action(&mut self) -> EscapeAction`, unified across keyboard and mouse inputs.
  - Added `verify_pe_subsystem_windows_gui` asserting PE Optional Header Subsystem equals 2 (`IMAGE_SUBSYSTEM_WINDOWS_GUI`) directly from binary PE headers.
  - Added isolated settings verification testing `Settings::default()` defaults, explicit round-trips for both false and true, and temporary file save/load.
  - Added NSIS installer script (`installer.nsi`) and self-contained distribution packaging script (`package.bat`), verifying installer size is ≤3 MiB.
- **Settings Persistence & Error Propagation (Advisory 2)**:
  - Added `Settings::save_to_path` and `Settings::load_from_path` for explicit file persistence checks.
  - `Settings::save` now returns `NotFound` instead of reporting success when `%APPDATA%` is unavailable.
  - Parent-directory creation and file-write failures propagate from `save_to_path`.
  - **Save & Apply** applies the startup preference before persistence; if persistence fails it restores the prior startup state, keeps the dialog open, and reports any rollback failure.
  - Settings loading remains intentionally tolerant: missing, unreadable, or malformed configuration falls back to defaults.
  - Settings dialog callers consume the saved result without performing redundant persistence writes.
- **Pure GUI Subsystem without Console (Slice C4)**:
  - Added `#![windows_subsystem = "windows"]` to suppress the console window when launching the executable or daemon.
  - Attached to parent console (`AttachConsole`) on startup when CLI arguments are supplied so terminal output (`--smoke-test`, `--settings`, etc.) displays properly.
- **System Tray Icon & Right-Click Menu (Slice C5)**:
  - Added persistent system tray icon in the Windows taskbar notification area via `Shell_NotifyIconW`.
  - Left-click or double-click on tray icon triggers capture immediately.
  - Right-click on the tray icon opens a native context menu with Capture Now, Settings, Check for Updates, up to five Recent Captures, and Exit.
  - Cleanly unregisters tray icon with `NIM_DELETE` on shutdown so no ghost icons remain.
- **Simple/Advanced Settings Panel & Behavior Toggles (Slice C6)**:
  - Split a compact 700×620 light native Settings surface into Simple everyday capture/save cards and Advanced annotation, Windows-integration, and update cards. View switches retain unsaved state, PNG natively disables JPEG quality, and the standard default Save & Apply then Cancel footer remains visible.
  - Added two persistent boolean toggles to `Settings`: `enable_window_snap` (default: true) and `close_after_action` (default: true).
  - Designed native checkbox controls in the standalone settings dialog.
  - Disabling window snap suppresses the window snap hover highlight and avoids accidental snaps on click.
  - Disabling auto-close keeps the capture overlay open after copying or saving so users can continue annotating.
  - Accessible from tray menu, toolbar button, `Ctrl+,` shortcut, and `--settings` CLI flag.
  - The dialog includes a native folder picker, PNG/JPEG controls, JPEG quality, capture delay, startup, notification, and update preferences.
  - Saved hotkey changes restart the running daemon listener in place; registration failure restores both the previous listener and persisted preference.

### Changed
- Reworked the capture controls into one coherent 90° L whose panels meet at exactly one corner. A usual selection places the padded L outside; alternate orientations flip coherently, and constrained/full-work-area selections use a padded inside placement bounded to the selected monitor. Every tool, named color, named thickness, and all four core action icons remain visible with monitor-bounded hover labels; irrelevant style controls are hidden for the active tool or selected object.
- Reorganized Settings into compact light Simple and Advanced card views with coherent Segoe UI hierarchy and a conventional footer. Both overlay entry paths pass an explicit modal owner so Settings stays above the active overlay while disabling it; standalone/tray Settings remains non-topmost.
- Windows icon and manifest resources are now compiled from their source files during each build, so visual-style and manifest changes cannot silently remain stale.
- Consolidated the historical implementation plans into `ROADMAP.md`, retaining durable status, verification gaps, deferred features, and future priorities.
- Removed an incomplete, risky capture-latency optimization; capture remains on the prior single coherent GDI `BitBlt` path.

### Verification — 2026-09-04
- `cargo fmt --check`, `cargo check`, `cargo clippy --all-targets -- -D warnings`, and `cargo build --release` passed.
- `cargo test` passed: 17 passed, 0 failed.
- `cmd /c package.bat` passed, including the strict NSIS installer check. No signing certificate was configured, so the executable and installer are unsigned.
- `--smoke-test` was intentionally not rerun. Safe direct-HWND automation exercised normal and full-primary-work-area selections without synthesizing PrintScreen; it did not repeat every object-transform path.
- `target/release/isolmass.exe` measured 546,304 bytes, below the 2.5 MiB target.
- The unsigned `target/release/isolmass-setup.exe` measured 288,209 bytes, below the 3 MiB target; SHA-256: `a29178bbce0b720becd83efbc8ed18a96aaf66b225e589f7d09e8b653bfd3622`.
- At 96 DPI, the compact Settings window showed Simple and Advanced without clipping, preserved unsaved state across switches, natively disabled JPEG-quality controls for PNG, retained the default Save & Apply then Cancel footer, and canceled with `settings.json` byte-identical. Both overlay paths were wired to explicit ownership; alternate-DPI runtime was not performed.
- The actual daemon was sampled for 60.039 seconds: 11,157,504 bytes at start, 11,157,504 bytes maximum, and 11,116,544 bytes at end. The 10.640625 MiB maximum was below the 15 MiB target.
- Actual PrintScreen-to-visible-overlay latency was 48.936 ms, so the ≤30 ms target was **not met** and C9 remains open.
- In the targeted runtime scenario, Esc closed the overlay in 15.801 ms, the daemon remained alive, clean `WM_CLOSE` shutdown exited 0, and no console `HWND` was observed.
- A representative native pass with MSI Afterburner and RTSS active and the latest full-window pass visually showed no rendering trails. Direct-HWND automation exercised a normal selection with the padded L outside and a full 1920×1032 primary-work-area selection with the full bottom-right L inside at a 10px inset; neither panel crossed the x=1920 seam. Direct image/source inspection confirmed all four core action icons were visible. The saved 1920×1032 output contained no toolbar, border, handles, caret, tooltip, or in-progress preview. This was targeted evidence, not exhaustive hardware or transform coverage.

### Fixed
- **Notification-area activation**: the negotiated version-4 tray callback now handles mouse selection, keyboard selection, and context-menu events while retaining legacy mouse callback decoding. Context menus use the Shell-provided anchor when available and post `WM_NULL` after dismissal so repeated openings remain reliable.
- **Overlay drag trails**: every interaction now rebuilds and invalidates the complete deterministic DIB frame, with explicit GDI synchronization before CPU writes and presentation. Representative MSI Afterburner/RTSS and full-window passes showed no stale selection, annotation, or toolbar pixels; exhaustive third-party hardware compatibility is not claimed.
- **Clean capture export**: Copy and Save now compose committed source plus committed annotations in place under an explicit export policy, then restore the editor frame even on failure. Selection borders/handles, toolbar/tooltip, caret, and in-progress previews are excluded.
- **Overlay-owned Settings**: both overlay Settings paths explicitly own and center the modal dialog, disable the active overlay for the modal lifetime, deliberately activate Settings, and restore the owner on exit; standalone/tray Settings remains non-topmost.
- **PrintScreen Registry Mutation**:
  - Starting the hotkey listener and changing Settings no longer modify the registry. Only the explicit `--fix-printscreen` command writes `HKCU\Control Panel\Keyboard\PrintScreenKeyForSnippingEnabled`.

- **Keyboard Input on Overlay (Slice C1)**:
  - Routed all overlay keyboard input through the low-level keyboard hook (`WH_KEYBOARD_LL`), ensuring reliable input delivery even with `WS_EX_NOACTIVATE`.
  - Implemented hierarchical Escape key flow: dismisses active text edit, deselects shape, cancels committed selection, or closes the overlay on first press from idle state.
  - Implemented full text tool editing support: character insertion at caret, Backspace, Delete, Left/Right arrow caret movement, and Enter to commit without auto-selection while Text remains active.
  - Added a blinking caret (`|`) indicator during text editing.
  - Added double-click on existing committed text annotations to re-open them in editing mode with caret at the end.
  - Supported overlay shortcuts when not text editing: `Ctrl+C` (copy), `Ctrl+S` (save), `Ctrl+Z` (undo), `Ctrl+Y` (redo), `Ctrl+,` (settings), Select-mode `Delete`/`Backspace`, and `V`/`R`/`A`/`P`/`T`/`B` tool switches.
  - Ensured foreground and input focus are explicitly requested on overlay click and show.

- **Select-Only Selection and Object Transforms (Slice C2)**:
  - Added a Select-only 8px hit band and four corner handles for moving/resizing the screenshot region and translating its annotations.
  - Made every selected annotation movable from its interior and resizable from visible, type-appropriate handles.
  - Transform previews update continuously, but history records exactly one Modify command on mouse-up.
  - Bound and clamped selection and object geometry with safe minimum extents.

- **Tool Interaction UX Pass (Slice C3)**:
  - Added distinct active and hovered visual states for all toolbar buttons, tool switches, color swatches, and thickness presets.
  - Implemented contextual cursors: `IDC_HAND`/`IDC_ARROW` over toolbar, `IDC_SIZENWSE`/`IDC_SIZENESW` over corner resize handles, `IDC_SIZEALL` over border move bands, `IDC_IBEAM` over text objects and active text edits, and `IDC_CROSS` while drawing tools are active.
  - Added dual-tone high-contrast outlines for the selection rectangle and corner handles (dark outer border + bright accent inner border) for crisp visibility on both 100% white and 100% dark backgrounds.
  - Added a >= 3px drag movement threshold before committing permanent drawing shapes, preventing stray 1-pixel marks from accidental clicks.
