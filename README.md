# isolmaSS

Lightweight, native Windows screenshot and annotation utility built in Rust with direct Win32/GDI access. It uses a pure GUI subsystem (zero console window on normal launch) and persistent system tray integration. Release targets are ≤2.5 MiB for the executable, ≤3 MiB for the installer, ≤15 MiB for a separately measured idle daemon working set, and ≤30 ms from PrintScreen to a visible overlay.

## Implementation Status: Phases A and B Implemented; C0–C8 Implemented, C9 Open

See [ROADMAP.md](ROADMAP.md) for the durable phase summary, open latency gap, deferred features, and future priorities. The automated smoke command exercises core model, persistence, Win32 integration, and packaging paths. It is not a substitute for interactive validation of the complete tool UX.

### Verification Snapshot — 2026-09-04

- `cargo fmt --check`: passed after applying the formatter.
- `cargo check`: passed.
- `cargo clippy --all-targets -- -D warnings`: passed.
- `cargo test`: passed (17 passed, 0 failed).
- `cargo build --release`: passed.
- `cmd /c package.bat`: passed, including the strict NSIS installer check; the executable and installer are unsigned because no signing certificate was configured.
- `--smoke-test` was intentionally not rerun. Safe direct-HWND automation exercised normal and full-primary-work-area selections without synthesizing the global PrintScreen hotkey; it did not repeat every object-transform path.
- Release executable: 546,304 bytes (0.520996 MiB), under the 2.5 MiB target.
- NSIS installer: 288,209 bytes (0.274858 MiB), under the 3 MiB target.
- Installer SHA-256: `a29178bbce0b720becd83efbc8ed18a96aaf66b225e589f7d09e8b653bfd3622`.
- Actual daemon sample over 60.039 seconds: 11,157,504 bytes at start, 11,157,504 bytes maximum, and 11,116,544 bytes at end. The 10.640625 MiB maximum is under the 15 MiB target.
- Actual PrintScreen-to-visible-overlay latency: 48.936 ms. This **does not meet** the ≤30 ms target.
- In that targeted runtime scenario, Esc closed the overlay in 15.801 ms, the daemon remained alive, clean `WM_CLOSE` shutdown exited with code 0, and no console `HWND` was observed.

A representative native runtime pass with MSI Afterburner and RTSS active, plus the latest full-window pass, showed deterministic full-frame rendering without trails; this is representative evidence, not exhaustive hardware compatibility. Safe direct-HWND automation exercised a normal selection with its padded L outside and a full 1920×1032 primary-work-area selection with the complete bottom-right L inside at a 10px inset, without crossing the x=1920 seam into the adjacent monitor. Direct image and source inspection confirmed that all four core action icons were visible. The saved 1920×1032 output contained no toolbar, border, handles, caret, tooltip, or in-progress preview. The compact 700×620 Settings window was exercised at 96 DPI in both Simple and Advanced views: controls were unclipped, unsaved state survived view switches, PNG disabled the JPEG-quality controls, the standard Save & Apply then Cancel footer remained visible, and Cancel left `settings.json` byte-identical. No alternate-DPI runtime was performed. A risky incomplete latency optimization was removed; capture remains on the prior single coherent GDI `BitBlt` path.

### Phase A — Core Capture & Annotation Loop
- [x] **A0: Project Scaffold** — Pure Rust binary crate with size-optimized release profile (`opt-level = "z"`, `lto = true`, `panic = "abort"`, `strip = true`).
- [x] **A1: Global Hotkey Registration** — Dedicated background message thread using `WH_KEYBOARD_LL` for `PrintScreen`; configured non-PrintScreen hotkeys fall back to `Ctrl+Shift+S` only when registration fails.
- [x] **A2: Full-Screen Virtual Screen Capture** — Captures the virtual screen via GDI `BitBlt` into an in-memory BGRA buffer with a pre-rendered dimmed backdrop.
- [x] **A3: Fullscreen Layered Overlay** — `WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOACTIVATE` window covering all monitors seamlessly.
- [x] **A4: Drag-to-Select Rectangle** — Real-time mouse tracking and scanline copies reveal undimmed pixels inside the selection.
- [x] **A5: Single-Click Window Snap** — Topmost window detection via `EnumWindows` + `DwmGetWindowAttribute(DWMWA_EXTENDED_FRAME_BOUNDS)` for pixel-perfect window snapping on click.
- [x] **A6: Selection Commit & L Toolbar** — One coherent 90° L keeps the rail and contextual strip joined at exactly one corner. A usual selection places the complete L outside with padding and flips it coherently when needed; a full-work-area or otherwise constrained selection places the complete L inside the selected monitor with padding rather than spilling to a neighbor. Every tool, color, thickness, and core action has a readable, monitor-bounded hover name.
- [x] **A7: Rectangle Tool** — Draw vector rectangles over selection with crisp borders.
- [x] **A8: Arrow Tool** — Draw directional arrows with calculated triangular arrowheads.
- [x] **A9: Pen (Freehand) Tool** — Freehand continuous lines following cursor path.
- [x] **A10: Text Tool** — Inline text entry at click point with caret indicator, Enter to commit, Esc to cancel.
- [x] **A11: Blur / Mosaic Tool** — Pixel-block averaging box blur directly on composited buffer; underlying text becomes unreadable.
- [x] **A12: Explicit Select Tool** — `V` activates Select at the bottom-nearest rail position. Rectangle, Arrow, Pen, Text, and Blur remain active after each draw and commit without selecting the new object. Only Select can choose, move, resize, restyle, rethicken, or delete existing annotations, manipulate the screenshot selection border, or double-click text to edit it.
- [x] **A13: Undo / Redo Stack** — Rolling 50-command history with `Ctrl+Z` (Undo) and `Ctrl+Y` (Redo).
- [x] **A14: Copy to Clipboard** — `Ctrl+C` commits an active text edit first, then flattens the screenshot and annotations—never selection handles—into a 32-bit bottom-up DIB (`CF_DIB`) for universal pasting into Paint, Slack, Discord, and Office.
- [x] **A15: Hierarchical Escape / Cancel Flow** — `Esc` cleanly cancels text editing first, then deselects object, then cancels selection, then dismisses overlay.
- [x] **A16: Automated Smoke Checks** — `--smoke-test` exercises core production methods and selected Win32 integration paths; it is not a substitute for interactive end-to-end verification.

### Phase B — Immediate Quality-of-Life
- [x] **B1: Save to File (PNG/JPEG via GDI+)** — Save button + `Ctrl+S` exports the composited selection in the configured PNG or JPEG format using native Windows GDI+ with zero external dependencies. Files are saved to the configured folder (default `Pictures\Screenshots`) with timestamped `.png` or `.jpg` names.
- [x] **B2: Contextual Style Strip** — Named hover controls expose 8 preset colors (Red, Orange, Yellow, Green, Blue, Purple, White, Black) and 3 named thickness levels (2px, 4px, 8px). Drawing tools show their applicable defaults; Select shows only controls supported by the selected object, while core actions remain available. A completed drag, resize, recolor, or thickness change records one undoable Modify command.
- [x] **B3: Shift-Key Angle & Square Snapping** — Holding `Shift` during drawing constrains rectangles to a 1:1 square aspect ratio and snaps arrows to 45° angle increments (0°, 45°, 90°, 135°, etc.).
- [x] **B4: Simple/Advanced Native Settings Window** — A compact 700×620 light native Win32 surface uses a clear Segoe UI hierarchy and section cards for Simple everyday capture/save choices and Advanced annotation, Windows-integration, and update choices. PNG natively disables JPEG quality; unsaved state survives view switches; the standard footer orders default Save & Apply before Cancel. Both overlay entry paths pass the overlay as an explicit modal owner, while standalone/tray Settings remains non-topmost. Settings remain tolerantly readable and strictly writable with propagated persistence errors.
- [x] **B5: Build & Budget Checks** — The current release executable and installer are under their size targets, and the measured 60.039-second daemon maximum is under 15 MiB. The separate ≤30 ms PrintScreen-to-overlay target is not met.

### Phase C — Production Hardening & Critical Fixes
- [x] **C0: Git & Docs Sync Protocol** — Conventional commit and documentation-sync conventions are defined for Phase C work.
- [x] **C1: Overlay Keyboard Routing & Text Tool Polish** — Routes overlay keyboard input through `WH_KEYBOARD_LL`. Strict hierarchical Esc dismiss (`TextEditing -> cancel text edit`, `ObjectSelected -> deselect`, `SelectionActive -> cancel selection`, `Idle -> close overlay`). Full text editing includes character insertion, Backspace/Delete, caret navigation, Enter commit without auto-selection, a blinking caret, and Select-mode double-click re-editing.
- [x] **C2: Select-Only Transforms** — In Select mode, every annotation moves from its interior and exposes appropriate handles: rectangle/blur bounds, arrow endpoints, scaled pen points, and proportional text font/position. Safe extents and screenshot bounds are enforced where practical, and mouse-up records exactly one Modify command. The selection region's border move/corner resize behavior is also Select-only.
- [x] **C3: Tool Interaction UX Pass** — Contextual cursors (`IDC_CROSS`, `IDC_SIZEALL`, `IDC_SIZENWSE`/`IDC_SIZENESW`, `IDC_IBEAM`, `IDC_HAND`/`IDC_ARROW`), dual-tone contrast outline (black outer border + bright accent inner border + white corner handles), distinct active/hover toolbar button states, and >= 3px drag movement threshold.
- [x] **C4: Pure GUI Subsystem Configuration** — Compiled with `#![windows_subsystem = "windows"]` and calls `AttachConsole(ATTACH_PARENT_PROCESS)` for CLI arguments. PE subsystem inspection is automated, and the targeted daemon runtime check observed no console `HWND`; this is not a complete UI validation.
- [x] **C5: System Tray Icon & Right-Click Menu** — Persistent notification area icon via `Shell_NotifyIconW`. Left-click or double-click triggers immediate capture. The right-click menu provides **Capture Now**, **Settings...**, **Check for Updates**, up to five **Recent Captures**, and **Exit**. Cleanly deleted with `NIM_DELETE` on shutdown.
- [x] **C6: Settings Standalone Panel & Behavior Toggles** — Native checkboxes for `enable_window_snap` (toggle single-click window snapping) and `close_after_action` (keep overlay open after Copy/Save to continue annotating). Save failures leave the dialog open and render the error.
- [x] **C7: Regression Smoke Checks v2** — One post-change run exited 0 and reported `AUTOMATED SMOKE CHECKS PASSED (A1-C8 + C9 packaging checks)`. External runtime budgets and interactive behavior are outside that command's verification scope.
- [x] **C8: Changelog & README Sync** — Documentation describes the implemented desktop, tray, editor, and Settings behavior while separating automated checks from manual/runtime verification.
- [ ] **C9: Packaging and Runtime Reverification** — Build, package, artifact-size, daemon-memory, shutdown, and console checks passed on 2026-09-04, but PrintScreen-to-visible-overlay measured 48.936 ms and failed the ≤30 ms target. C9 remains incomplete; follow-up priorities are tracked in [ROADMAP.md](ROADMAP.md).

## System Tray & Background Daemon

When launched without arguments, `isolmass.exe` runs as a pure background GUI process:
- **Zero console window:** No terminal flashes or stays open.
- **Taskbar notification area:** Displays the isolmaSS icon in the system tray.
- **Left-Click or Double-Click:** Immediately freezes the screen and opens the capture overlay (identical to pressing `PrintScreen`).
- **Right-Click Context Menu:**
  - **Capture Now** (default action)
  - **Settings...** (opens native settings panel)
  - **Check for Updates** (runs a manual release check without auto-installing)
  - **Recent Captures** (opens one of up to five newest screenshots; disabled when none exist)
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
- **`Ctrl+S`**: Save the screenshot to disk in the configured PNG or JPEG format.
- **`Ctrl+Z`**: Undo last annotation modification, move, add, or delete.
- **`Ctrl+Y`**: Redo last undone action.
- **`Ctrl+,`**: Open native Settings dialog.
- **`Delete` / `Backspace`**: Delete currently selected annotation object.
- **`V`**: Select Pointer/Select Tool
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
- **`Enter`**: Commit the text annotation without changing away from the Text tool or selecting the new object.
- **`Esc`**: Cancel text edit without creating an object.
- **Double-Click in Select**: Double-clicking an existing committed text annotation re-opens it in text editing mode with the caret at the end.

## Select Tool and Manipulation
- **Drawing stays active**: Rectangle, Arrow, Pen, Text, and Blur commit without automatic selection, so repeated drawing needs no tool reactivation.
- **Interior move**: In Select mode, drag any selected rectangle, blur, arrow, pen stroke, or text object from its interior.
- **Type-appropriate resize handles**: Resize rectangle/blur bounds, arrow endpoints, scaled pen points, and text position/font proportionally. Safe minimum extents apply, with geometry kept inside the screenshot where practical.
- **One-step history**: A gesture previews freely but records exactly one undoable Modify command on mouse-up.
- **Selection region**: Only Select enables the screenshot region's 8px border move band and four corner resize handles; moving it translates contained annotations.
- **Dual-Tone Outline**: A dark outer border, accent inner border, and visible handles retain contrast on white, black, or busy backgrounds.

## Settings Dialog
Available via the tray menu (**Settings...**), the overlay button (**Settings**), the **`Ctrl+,`** shortcut, or the **`--settings`** CLI flag. The compact 700×620 light native window opens in **Simple** for everyday capture/save cards; **Advanced** exposes annotation defaults, Windows integration, and updates. Switching views preserves unsaved edits, and the standard default **Save & Apply** then **Cancel** footer remains available in both views. Overlay callers use the active overlay as an explicit modal owner; standalone/tray Settings remains non-topmost:
- **Global Hotkey Presets**: `PrintScreen`, `Ctrl+Shift+S`, `Alt+PrintScreen`. The running daemon restarts its listener in place after saving; if registration fails, it restores the previous active and persisted hotkey.
- **Save Folder**: Displays the current target directory (default: `Pictures\Screenshots`) and provides a native **Browse...** folder picker.
- **Default Color & Thickness**: Pick default drawing styling.
- **Image Format & Quality**: Save as PNG or JPEG, with 80/90/100 JPEG quality presets that are natively disabled and keyboard-inactive while PNG is selected.
- **Capture Delay**: Capture immediately or after 1, 3, or 5 seconds.
- **Capture Behavior**: Toggle single-click window snapping and whether Copy/Save closes the overlay.
- **Windows Integration**: Start isolmaSS at sign-in and show save notifications. **Save & Apply** applies the startup setting immediately.
- **Updates**: Toggle background checks and verified automatic installation, or run a manual update check.

Settings are loaded tolerantly: a missing, unreadable, or malformed `%APPDATA%\isolmaSS\settings.json` falls back to defaults. Saving is strict: unavailable `%APPDATA%` returns `NotFound`, and parent-directory creation and file-write failures propagate. **Save & Apply** applies the Windows startup preference before persisting; a persistence failure restores the prior startup state, while any rollback failure is reported explicitly. The dialog remains open on failure.

## Building & Running

### Requirements
- Windows 10 / 11 (x86_64)
- Rust toolchain (2024 edition)

### Build Release Binary
```bash
cargo build --release
```
The resulting executable is located at `target/release/isolmass.exe`. In the 2026-09-04 verified build it was 546,304 bytes; the unsigned `target/release/isolmass-setup.exe` was 288,209 bytes with SHA-256 `a29178bbce0b720becd83efbc8ed18a96aaf66b225e589f7d09e8b653bfd3622`. These measurements apply only to that source/toolchain build and must be refreshed after later changes.

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
