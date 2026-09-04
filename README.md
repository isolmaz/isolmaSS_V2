# isolmaSS

Lightweight, native Windows screenshot and annotation utility built in Rust with direct Win32/GDI access. It uses a pure GUI subsystem (zero console window on normal launch) and persistent system tray integration. Release targets are ≤2.5 MiB for the executable, ≤3 MiB for the installer, ≤15 MiB for a separately measured idle daemon working set, and ≤30 ms from PrintScreen to a visible overlay.

## Implementation Status: Phases A, B, and C Implemented

The automated smoke command exercises core model, persistence, Win32 integration, and packaging paths. It is not a substitute for interactive validation of the complete tool UX.

### Verification Snapshot — 2026-09-04

- `cargo check`: passed.
- `cargo clippy --all-targets -- -D warnings`: passed.
- `cargo test`: passed (6 passed, 0 failed).
- `cargo build --release`: passed.
- `cmd /c package.bat`: passed, including the strict NSIS installer check.
- One post-change `--smoke-test` run exited 0 with `AUTOMATED SMOKE CHECKS PASSED (A1-C8 + C9 packaging checks)` and explicitly reported that external runtime budget status is documented separately and is not verified by that command.
- Release executable: 438,784 bytes (0.418457 MiB), under the 2.5 MiB target.
- NSIS installer: 262,534 bytes (0.250372 MiB), under the 3 MiB target.
- Actual daemon sample over 60.039 seconds: 11,157,504 bytes at start, 11,157,504 bytes maximum, and 11,116,544 bytes at end. The 10.640625 MiB maximum is under the 15 MiB target.
- Actual PrintScreen-to-visible-overlay latency: 48.936 ms. This **does not meet** the ≤30 ms target.
- In that targeted runtime scenario, Esc closed the overlay in 15.801 ms, the daemon remained alive, clean `WM_CLOSE` shutdown exited with code 0, and no console `HWND` was observed.

This snapshot does not claim that the complete interactive annotation, Settings, or tray-menu UX was manually validated. A risky incomplete latency optimization was removed; capture remains on the prior single coherent GDI `BitBlt` path.

### Phase A — Core Capture & Annotation Loop
- [x] **A0: Project Scaffold** — Pure Rust binary crate with size-optimized release profile (`opt-level = "z"`, `lto = true`, `panic = "abort"`, `strip = true`).
- [x] **A1: Global Hotkey Registration** — Dedicated background message thread using `WH_KEYBOARD_LL` for `PrintScreen`; configured non-PrintScreen hotkeys fall back to `Ctrl+Shift+S` only when registration fails.
- [x] **A2: Full-Screen Virtual Screen Capture** — Captures the virtual screen via GDI `BitBlt` into an in-memory BGRA buffer with a pre-rendered dimmed backdrop.
- [x] **A3: Fullscreen Layered Overlay** — `WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOACTIVATE` window covering all monitors seamlessly.
- [x] **A4: Drag-to-Select Rectangle** — Real-time mouse tracking and scanline copies reveal undimmed pixels inside the selection.
- [x] **A5: Single-Click Window Snap** — Topmost window detection via `EnumWindows` + `DwmGetWindowAttribute(DWMWA_EXTENDED_FRAME_BOUNDS)` for pixel-perfect window snapping on click.
- [x] **A6: Selection Commit & Toolbar Shell** — Two-row floating toolbar anchored beneath selection with tool buttons, actions, color palette, and thickness presets.
- [x] **A7: Rectangle Tool** — Draw vector rectangles over selection with crisp borders.
- [x] **A8: Arrow Tool** — Draw directional arrows with calculated triangular arrowheads.
- [x] **A9: Pen (Freehand) Tool** — Freehand continuous lines following cursor path.
- [x] **A10: Text Tool** — Inline text entry at click point with caret indicator, Enter to commit, Esc to cancel.
- [x] **A11: Blur / Mosaic Tool** — Pixel-block averaging box blur directly on composited buffer; underlying text becomes unreadable.
- [x] **A12: Universal Auto-Select on Draw** — Objects auto-selected upon creation; click existing object to select, drag to reposition, Delete/Backspace to remove.
- [x] **A13: Undo / Redo Stack** — Rolling 50-command history with `Ctrl+Z` (Undo) and `Ctrl+Y` (Redo).
- [x] **A14: Copy to Clipboard** — Flattens selection + annotations into a 32-bit bottom-up DIB (`CF_DIB`) for universal pasting into Paint, Slack, Discord, Office.
- [x] **A15: Hierarchical Escape / Cancel Flow** — `Esc` cleanly cancels text editing first, then deselects object, then cancels selection, then dismisses overlay.
- [x] **A16: Automated Smoke Checks** — `--smoke-test` exercises core production methods and selected Win32 integration paths; it is not a substitute for interactive end-to-end verification.

### Phase B — Immediate Quality-of-Life
- [x] **B1: Save to File (PNG via GDI+)** — Save button + `Ctrl+S` exports composited selection to PNG using native Windows GDI+ with zero external dependencies. Automatically saved to `Pictures\Screenshots` with timestamped filename `Screenshot_YYYY-MM-DD_HH-MM-SS.png`.
- [x] **B2: Color Palette & Thickness Sub-Bar** — 8 preset colors (Red, Orange, Yellow, Green, Blue, Purple, White, Black) and 3 thickness levels (2px, 4px, 8px). Clicking swatches live-recolors any selected annotation (with Undo/Redo support via `EditCommand::Modify`) and sets default styling for new shapes.
- [x] **B3: Shift-Key Angle & Square Snapping** — Holding `Shift` during drawing constrains rectangles to a 1:1 square aspect ratio and snaps arrows to 45° angle increments (0°, 45°, 90°, 135°, etc.).
- [x] **B4: Minimal Native Settings Window** — Native Win32 settings dialog accessible via `--settings`, toolbar button, or `Ctrl+,`. Settings are stored in `%APPDATA%\isolmaSS\settings.json`; tolerant loading falls back to defaults for missing, unreadable, or malformed data. Saving returns `NotFound` when `%APPDATA%` is unavailable, and parent-directory creation and file-write errors propagate to the dialog.
- [x] **B5: Build & Budget Checks** — The current release executable and installer are under their size targets, and the measured 60.039-second daemon maximum is under 15 MiB. The separate ≤30 ms PrintScreen-to-overlay target is not met.

### Phase C — Production Hardening & Critical Fixes
- [x] **C0: Git & Docs Sync Protocol** — Conventional commit and documentation-sync conventions are defined for Phase C work.
- [x] **C1: Overlay Keyboard Routing & Text Tool Polish** — Routes overlay keyboard input through `WH_KEYBOARD_LL`. Strict hierarchical Esc dismiss (`TextEditing -> cancel text edit`, `ObjectSelected -> deselect`, `SelectionActive -> cancel selection`, `Idle -> close overlay`). Full text editing support (character insertion at caret, Backspace, Delete, Left/Right navigation, Enter commit/auto-select, blinking caret `|`, and double-click to re-edit existing text objects).
- [x] **C2: Selection Border Drag-to-Move & Resize** — Explicit 8px border hit band allows dragging anywhere on the border line to smoothly move the selection and translate all child annotations. 4 corner handles (8x8 px) provide diagonal resizing.
- [x] **C3: Tool Interaction UX Pass** — Contextual cursors (`IDC_CROSS`, `IDC_SIZEALL`, `IDC_SIZENWSE`/`IDC_SIZENESW`, `IDC_IBEAM`, `IDC_HAND`/`IDC_ARROW`), dual-tone contrast outline (black outer border + bright accent inner border + white corner handles), distinct active/hover toolbar button states, and >= 3px drag movement threshold.
- [x] **C4: Pure GUI Subsystem Configuration** — Compiled with `#![windows_subsystem = "windows"]` and calls `AttachConsole(ATTACH_PARENT_PROCESS)` for CLI arguments. PE subsystem inspection is automated, and the targeted daemon runtime check observed no console `HWND`; this is not a complete UI validation.
- [x] **C5: System Tray Icon & Right-Click Menu** — Persistent notification area icon via `Shell_NotifyIconW`. Left-click or double-click triggers immediate capture. Right-click context popup menu provides **Capture Now** (default bold item), **Settings...**, and **Exit**. Cleanly deleted with `NIM_DELETE` on shutdown.
- [x] **C6: Settings Standalone Panel & Behavior Toggles** — Native checkboxes for `enable_window_snap` (toggle single-click window snapping) and `close_after_action` (keep overlay open after Copy/Save to continue annotating). Save failures leave the dialog open and render the error.
- [x] **C7: Regression Smoke Checks v2** — One post-change run exited 0 and reported `AUTOMATED SMOKE CHECKS PASSED (A1-C8 + C9 packaging checks)`. External runtime budgets and interactive behavior are outside that command's verification scope.
- [x] **C8: Changelog & README Sync** — Documentation describes the implemented desktop, tray, editor, and Settings behavior while separating automated checks from manual/runtime verification.
- [ ] **C9: Packaging and Runtime Reverification** — Build, package, artifact-size, daemon-memory, shutdown, and console checks passed on 2026-09-04, but PrintScreen-to-visible-overlay measured 48.936 ms and failed the ≤30 ms target. C9 remains incomplete.

## System Tray & Background Daemon

When launched without arguments, `isolmass.exe` runs as a pure background GUI process:
- **Zero console window:** No terminal flashes or stays open.
- **Taskbar notification area:** Displays the isolmaSS icon in the system tray.
- **Left-Click or Double-Click:** Immediately freezes the screen and opens the capture overlay (identical to pressing `PrintScreen`).
- **Right-Click Context Menu:**
  - **Capture Now** (default action)
  - **Settings...** (opens native settings panel)
  - **Exit** (cleanly unregisters the tray icon, unhooks the keyboard, and terminates the daemon)

## Keyboard Shortcuts

### Global
- **`PrintScreen`** (or configured hotkey e.g. **`Ctrl+Shift+S`**): Freeze screen and open overlay.

### When Overlay is Active
- **`Esc`** (or **Right-Click**): Hierarchical dismiss:
  1. If editing text: cancels text edit only.
  2. If shape selected: deselects shape.
  3. If selection committed: cancels selection and returns to window-hover mode.
  4. If idle: closes overlay on first press.
- **`Ctrl+C`** or **`Enter`**: Copy flattened selection + annotations to Windows Clipboard.
- **`Ctrl+S`**: Save PNG screenshot to disk.
- **`Ctrl+Z`**: Undo last annotation modification, move, add, or delete.
- **`Ctrl+Y`**: Redo last undone action.
- **`Ctrl+,`**: Open native Settings dialog.
- **`Delete` / `Backspace`**: Delete currently selected annotation object.
- **`R`**: Select Rectangle Tool
- **`A`**: Select Arrow Tool
- **`P`**: Select Pen (Freehand) Tool
- **`T`**: Select Text Tool
- **`B`**: Select Blur / Mosaic Tool

### Text Tool Controls (While in Text Edit Mode)
- **Typing**: Inserts characters at caret position.
- **`Left` / `Right` Arrows**: Move blinking caret (`|`) position.
- **`Backspace`**: Delete character before caret.
- **`Delete`**: Delete character after caret.
- **`Enter`**: Commit text annotation and auto-select it.
- **`Esc`**: Cancel text edit without creating an object.
- **Double-Click**: Double-clicking an existing committed text annotation re-opens it in text editing mode with the caret at the end.

## Selection Manipulation
- **Corner Handles (8x8 px)**: Drag diagonally to resize selection.
- **Border Edge Bands (8 px)**: Drag anywhere along the border line to move the entire selection rectangle and all child annotations smoothly.
- **Dual-Tone Outline**: 1px black outer border + 2px accent border ensures crisp contrast on 100% white, 100% black, or busy backgrounds.

## Settings Dialog
Available via the tray menu (**Settings...**), the overlay button (**Settings**), the **`Ctrl+,`** shortcut, or the **`--settings`** CLI flag:
- **Global Hotkey Presets**: `PrintScreen`, `Ctrl+Shift+S`, `Alt+PrintScreen`. A saved hotkey takes effect on the next daemon launch; the running listener is not restarted in place.
- **Save Folder**: Displays the current target directory (default: `Pictures\Screenshots`). The current dialog does not provide a folder picker or editor.
- **Default Color & Thickness**: Pick default drawing styling.
- **Enable single-click window snap**: Toggle automatic window snapping on hover/click.
- **Close overlay automatically after Copy / Save**: When disabled, keeps the overlay open after copying or saving so you can continue annotating.

Settings are loaded tolerantly: a missing, unreadable, or malformed `%APPDATA%\isolmaSS\settings.json` falls back to defaults. Saving is strict: unavailable `%APPDATA%` returns `NotFound`, and parent-directory creation and file-write failures propagate. **Save & Apply** keeps the dialog open and renders the error instead of reporting success or dismissing the window.

## Building & Running

### Requirements
- Windows 10 / 11 (x86_64)
- Rust toolchain (2024 edition)

### Build Release Binary
```bash
cargo build --release
```
The resulting executable is located at `target/release/isolmass.exe`. In the 2026-09-04 verified build it was 438,784 bytes; `target/release/isolmass-setup.exe` was 262,534 bytes. These measurements apply only to that source/toolchain build and must be refreshed after later changes.

### Automated Smoke Checks
Run the automated smoke command for core model, persistence, selected Win32 integration, and packaging checks:
```bash
./target/release/isolmass.exe --smoke-test
```
(Alias: `--test-capture`)

This command is not read-only: it captures the desktop, replaces clipboard content, creates and removes temporary files, registers Win32 hotkey/tray resources, trims its own working set, and may build the installer if it is missing. It does not replace interactive verification of the overlay, Settings UI, or tray interactions, and its trimmed working-set reading is not the separate daemon measurement recorded above.

### Open Settings Directly
```bash
./target/release/isolmass.exe --settings
```

### Single Immediate Capture Test
```bash
./target/release/isolmass.exe --capture-once
```

### Fix Windows 11 Snipping Tool Conflict
If Windows Snipping Tool intercepts PrintScreen, the following explicit opt-in command writes `0` to the per-user registry value `HKCU\Control Panel\Keyboard\PrintScreenKeyForSnippingEnabled`:
```bash
./target/release/isolmass.exe --fix-printscreen
```
Normal daemon startup and Settings changes do not mutate this registry value.
