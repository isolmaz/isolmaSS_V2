# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
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
- **Standalone Settings Panel & Behavior Toggles (Slice C6)**:
  - Added two new persistent boolean toggles to `Settings`: `enable_window_snap` (default: true) and `close_after_action` (default: true).
  - Designed native checkbox controls in the standalone settings dialog.
  - Disabling window snap suppresses the window snap hover highlight and avoids accidental snaps on click.
  - Disabling auto-close keeps the capture overlay open after copying or saving so users can continue annotating.
  - Accessible from tray menu, toolbar button, `Ctrl+,` shortcut, and `--settings` CLI flag.
  - The dialog includes a native folder picker, PNG/JPEG controls, JPEG quality, capture delay, startup, notification, and update preferences.
  - Saved hotkey changes restart the running daemon listener in place; registration failure restores both the previous listener and persisted preference.

### Changed
- Reworked the capture controls into a compact Lightshot-style side tool rail and bottom action strip with native GDI icons, responsive edge flipping, clearer active/hover states, and concise shortcut tooltips.
- Reorganized Settings into a cleaner native grid for capture, saving, behavior, and update controls; the window no longer forces itself topmost.
- Windows icon and manifest resources are now compiled from their source files during each build, so visual-style and manifest changes cannot silently remain stale.
- Consolidated the historical implementation plans into `ROADMAP.md`, retaining durable status, verification gaps, deferred features, and future priorities.
- Removed an incomplete, risky capture-latency optimization; capture remains on the prior single coherent GDI `BitBlt` path.

### Verification — 2026-09-04
- `cargo check`, `cargo clippy --all-targets -- -D warnings`, and `cargo build --release` passed.
- `cargo test` passed: 7 passed, 0 failed.
- `cmd /c package.bat` passed, including the strict NSIS installer check.
- One post-change `--smoke-test` run exited 0 with `AUTOMATED SMOKE CHECKS PASSED (A1-C8 + C9 packaging checks)` and reported that external runtime budget status is documented separately and is not verified by that command.
- `target/release/isolmass.exe` measured 531,968 bytes, below the 2.5 MiB target.
- `target/release/isolmass-setup.exe` measured 281,904 bytes, below the 3 MiB target.
- The actual daemon was sampled for 60.039 seconds: 11,157,504 bytes at start, 11,157,504 bytes maximum, and 11,116,544 bytes at end. The 10.640625 MiB maximum was below the 15 MiB target.
- Actual PrintScreen-to-visible-overlay latency was 48.936 ms, so the ≤30 ms target was **not met**.
- In the targeted runtime scenario, Esc closed the overlay in 15.801 ms, the daemon remained alive, clean `WM_CLOSE` shutdown exited 0, and no console `HWND` was observed.
- A representative native pass ran with MSI Afterburner and RTSS active: selection creation, rectangle drawing/movement, and selection move/resize visually left only current overlay pixels; the visible tray menu showed update/recent-capture items; Settings opened from the tray, accepted a temporary JPEG selection, and canceled without saving. This was targeted evidence, not exhaustive UX coverage.

### Fixed
- **Overlay drag trails**: every interaction now rebuilds and invalidates the complete deterministic DIB frame, with explicit GDI synchronization before CPU writes and presentation. This removes stale selection, annotation, and toolbar pixels and remains correct when third-party graphics overlays hook GDI.
- **Tray right-click menu**: decode the packed low-word callback event and high-word icon identifier required after negotiating `NOTIFYICON_VERSION_4`.
- **PrintScreen Registry Mutation**:
  - Starting the hotkey listener and changing Settings no longer modify the registry. Only the explicit `--fix-printscreen` command writes `HKCU\Control Panel\Keyboard\PrintScreenKeyForSnippingEnabled`.

- **Keyboard Input on Overlay (Slice C1)**:
  - Routed all overlay keyboard input through the low-level keyboard hook (`WH_KEYBOARD_LL`), ensuring reliable input delivery even with `WS_EX_NOACTIVATE`.
  - Implemented hierarchical Escape key flow: dismisses active text edit, deselects shape, cancels committed selection, or closes the overlay on first press from idle state.
  - Implemented full text tool editing support: character insertion at caret, Backspace, Delete, Left/Right arrow caret movement, and Enter to commit and auto-select.
  - Added a blinking caret (`|`) indicator during text editing.
  - Added double-click on existing committed text annotations to re-open them in editing mode with caret at the end.
  - Supported overlay shortcuts when not text editing: `Ctrl+C` (copy), `Ctrl+S` (save), `Ctrl+Z` (undo), `Ctrl+Y` (redo), `Ctrl+,` (settings), `Delete`/`Backspace` (delete shape), `R`/`A`/`P`/`T`/`B` (tool switches).
  - Ensured foreground and input focus are explicitly requested on overlay click and show.

- **Selection Border Drag-to-Move (Slice C2)**:
  - Added an explicit 8px hit band along the selection rectangle borders.
  - Added 4 corner handles (8x8 px) for diagonal resizing of the selection rectangle.
  - Dragging anywhere on the visible border line or border edge band now smoothly moves the selection and translates all contained annotation objects.
  - Bound and clamped selection movement and resizing within screen dimensions.

- **Tool Interaction UX Pass (Slice C3)**:
  - Added distinct active and hovered visual states for all toolbar buttons, tool switches, color swatches, and thickness presets.
  - Implemented contextual cursors: `IDC_HAND`/`IDC_ARROW` over toolbar, `IDC_SIZENWSE`/`IDC_SIZENESW` over corner resize handles, `IDC_SIZEALL` over border move bands, `IDC_IBEAM` over text objects and active text edits, and `IDC_CROSS` while drawing tools are active.
  - Added dual-tone high-contrast outlines for the selection rectangle and corner handles (dark outer border + bright accent inner border) for crisp visibility on both 100% white and 100% dark backgrounds.
  - Added a >= 3px drag movement threshold before committing permanent drawing shapes, preventing stray 1-pixel marks from accidental clicks.
