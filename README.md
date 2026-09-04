# isolmaSS

Lightweight, native Windows screenshot and annotation utility built in Rust with direct Win32/GDI access. No web runtimes, no GC pauses, instant startup, pure GUI subsystem (zero console window), persistent system tray integration, and ultra-low idle memory footprint (~415 KB static binary, ~176 KB idle RAM).

## Status: Phases A, B, and C Complete (Production-Hardened)

### Phase A — Core Capture & Annotation Loop
- [x] **A0: Project Scaffold** — Pure Rust binary crate with size-optimized release profile (`opt-level = "z"`, `lto = true`, `panic = "abort"`, `strip = true`).
- [x] **A1: Global Hotkey Registration** — Dedicated background message thread listening for `PrintScreen` with automatic fallback to `Ctrl+Shift+S`.
- [x] **A2: Full-Screen Virtual Screen Capture** — Sub-40ms capture across all monitors via GDI `BitBlt` into an in-memory BGRA buffer with pre-rendered dimmed backdrop.
- [x] **A3: Fullscreen Layered Overlay** — `WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOACTIVATE` window covering all monitors seamlessly.
- [x] **A4: Drag-to-Select Rectangle** — Real-time mouse tracking, punching out undimmed pixels inside selection via scanline copies (<100 µs), with crisp accent border.
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
- [x] **A16: End-to-End Automated Smoke Test** — Verification across all slices via `--smoke-test`.

### Phase B — Immediate Quality-of-Life
- [x] **B1: Save to File (PNG via GDI+)** — Save button + `Ctrl+S` exports composited selection to PNG using native Windows GDI+ with zero external dependencies. Automatically saved to `Pictures\Screenshots` with timestamped filename `Screenshot_YYYY-MM-DD_HH-MM-SS.png`.
- [x] **B2: Color Palette & Thickness Sub-Bar** — 8 preset colors (Red, Orange, Yellow, Green, Blue, Purple, White, Black) and 3 thickness levels (2px, 4px, 8px). Clicking swatches live-recolors any selected annotation (with Undo/Redo support via `EditCommand::Modify`) and sets default styling for new shapes.
- [x] **B3: Shift-Key Angle & Square Snapping** — Holding `Shift` during drawing constrains rectangles to a 1:1 square aspect ratio and snaps arrows to 45° angle increments (0°, 45°, 90°, 135°, etc.).
- [x] **B4: Minimal Native Settings Window** — Native Win32 settings dialog accessible via `--settings`, toolbar button, or `Ctrl+,`. Persists hotkey, save directory, default color, and thickness to `%APPDATA%\isolmaSS\settings.json` with fallback defaults.
- [x] **B5: Build & Size / RAM Verification** — Release executable size (~415 KB <= 2.5 MB budget) and idle RAM footprint (~176 KB <= 15.0 MB budget) automated assertions.

### Phase C — Production Hardening & Critical Fixes
- [x] **C0: Git & Docs Sync Protocol** — Clean conventional commits, full test passes, and synchronized changelogs for every milestone.
- [x] **C1: Overlay Keyboard Routing & Text Tool Polish** — Routes overlay keyboard input through `WH_KEYBOARD_LL`. Strict hierarchical Esc dismiss (`TextEditing -> cancel text edit`, `ObjectSelected -> deselect`, `SelectionActive -> cancel selection`, `Idle -> close overlay`). Full text editing support (character insertion at caret, Backspace, Delete, Left/Right navigation, Enter commit/auto-select, blinking caret `|`, and double-click to re-edit existing text objects).
- [x] **C2: Selection Border Drag-to-Move & Resize** — Explicit 8px border hit band allows dragging anywhere on the border line to smoothly move the selection and translate all child annotations. 4 corner handles (8x8 px) provide diagonal resizing.
- [x] **C3: Tool Interaction UX Pass** — Contextual cursors (`IDC_CROSS`, `IDC_SIZEALL`, `IDC_SIZENWSE`/`IDC_SIZENESW`, `IDC_IBEAM`, `IDC_HAND`/`IDC_ARROW`), dual-tone contrast outline (black outer border + bright accent inner border + white corner handles), distinct active/hover toolbar button states, and >= 3px drag movement threshold.
- [x] **C4: Pure GUI Subsystem (No Console Window)** — Compiled with `#![windows_subsystem = "windows"]`. Spawns zero terminal window on normal launch or double-click; automatically attaches to caller console via `AttachConsole(ATTACH_PARENT_PROCESS)` when run with CLI arguments.
- [x] **C5: System Tray Icon & Right-Click Menu** — Persistent notification area icon via `Shell_NotifyIconW`. Left-click or double-click triggers immediate capture. Right-click context popup menu provides **Capture Now** (default bold item), **Settings...**, and **Exit**. Cleanly deleted with `NIM_DELETE` on shutdown.
- [x] **C6: Settings Standalone Panel & Behavior Toggles** — Native checkboxes for `enable_window_snap` (toggle single-click window snapping) and `close_after_action` (keep overlay open after Copy/Save to continue annotating).
- [x] **C7: Full Regression Smoke Test v2** — Automated verification covering all Phase A, B, and C fixes in `./target/release/isolmass.exe --smoke-test`.
- [x] **C8: Changelog & README Sync** — Comprehensive documentation of current desktop, tray, and editor capabilities.
- [x] **C9: Packaging Sanity Check** — Production release size (~415 KB) and idle RAM (~176 KB) verified well under budgets.

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
Configurable via the tray menu (**Settings...**), the overlay button (**Settings**), the **`Ctrl+,`** shortcut, or the **`--settings`** CLI flag:
- **Global Hotkey Presets**: `PrintScreen`, `Ctrl+Shift+S`, `Alt+PrintScreen`.
- **Save Folder**: Configurable target directory (defaults to `Pictures\Screenshots`).
- **Default Color & Thickness**: Pick default drawing styling.
- **Enable single-click window snap**: Toggle automatic window snapping on hover/click.
- **Close overlay automatically after Copy / Save**: When disabled, keeps the overlay open after copying or saving so you can continue annotating.

## Building & Running

### Requirements
- Windows 10 / 11 (x86_64)
- Rust toolchain (2024 edition)

### Build Release Binary
```bash
cargo build --release
```
The resulting static executable is located at `target/release/isolmass.exe` (~415 KB).

### Automated Verification Suite
Run the automated verification suite covering all Slices A0 through C7:
```bash
./target/release/isolmass.exe --smoke-test
```
(Alias: `--test-capture`)

### Open Settings Directly
```bash
./target/release/isolmass.exe --settings
```

### Single Immediate Capture Test
```bash
./target/release/isolmass.exe --capture-once
```

### Fix Windows 11 Snipping Tool Conflict
If Windows Snipping Tool intercepts PrintScreen, disable it permanently for isolmaSS:
```bash
./target/release/isolmass.exe --fix-printscreen
```
