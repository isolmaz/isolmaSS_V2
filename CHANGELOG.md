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
  - Added NSIS installer script (`installer.nsi`) and self-contained distribution packaging script (`package.bat`), verifying package size is <= 3 MB.
- **Settings Persistence & Error Propagation (Advisory 2)**:
  - Added `Settings::save_to_path` and `Settings::load_from_path` for arbitrary file persistence and error propagation testing.
  - Added `last_error: Option<String>` to `SettingsWindowState`, displaying save errors in vibrant red in the settings dialog UI and preventing dialog dismissal on write failure.
  - Cleaned up settings callers across `main.rs`, `overlay.rs`, and `tray.rs` to eliminate swallowed errors and redundant `save()` invocations.
- **Pure GUI Subsystem without Console (Slice C4)**:
  - Added `#![windows_subsystem = "windows"]` to suppress the console window when launching the executable or daemon.
  - Attached to parent console (`AttachConsole`) on startup when CLI arguments are supplied so terminal output (`--smoke-test`, `--settings`, etc.) displays properly.
- **System Tray Icon & Right-Click Menu (Slice C5)**:
  - Added persistent system tray icon in the Windows taskbar notification area via `Shell_NotifyIconW`.
  - Left-click or double-click on tray icon triggers capture immediately.
  - Right-click on tray icon opens native context popup menu with "Capture Now" (default), "Settings...", and "Exit".
  - Cleanly unregisters tray icon with `NIM_DELETE` on shutdown so no ghost icons remain.
- **Standalone Settings Panel & Behavior Toggles (Slice C6)**:
  - Added two new persistent boolean toggles to `Settings`: `enable_window_snap` (default: true) and `close_after_action` (default: true).
  - Designed native checkbox controls in the standalone settings dialog.
  - Disabling window snap suppresses the window snap hover highlight and avoids accidental snaps on click.
  - Disabling auto-close keeps the capture overlay open after copying or saving so users can continue annotating.
  - Accessible from tray menu, toolbar button, `Ctrl+,` shortcut, and `--settings` CLI flag.

### Fixed
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
