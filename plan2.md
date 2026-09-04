# isolmaSS — MVP + Production Hardening Plan

> **Status:** Phase A (A0–A16) and Phase B (B1–B5) are implemented and passed their build/size/RAM
> budgets. Real-world use surfaced 6 concrete defects and 3 missing UX pieces before this can be
> called production-ready — these are tracked in **Phase C** below. Phase C is the active phase.
>
> **Scope of this document:** Phases A and B (must-have MVP, done) plus Phase C (production
> hardening, in progress). Cloud upload, OCR, pin-to-screen, HDR capture, magnetic guides, etc. are
> still intentionally **out of scope** — see "Deferred Features" near the bottom.
>
> **Design philosophy for this plan:**
> 1. No WebView2 / React anywhere, not even for Settings — build native from day one so there is
>    never a "migrate away from WebView2 later" slice.
> 2. No cloud/network code in A+B+C — the tool must be 100% useful completely offline first.
> 3. Every slice ships something you can actually press a key and see working. No slice should
>    take more than a day or two.
> 4. From Phase C onward, Definition of Done includes a git commit + push and a doc sync step
>    (see C0) — now that the app is heading to real users, that discipline is worth the small
>    overhead it was deliberately skipped for during early A+B prototyping.

---

## 0. Stack Baseline

| Layer | Choice | Why |
|---|---|---|
| Language | Rust | Small binaries, no GC pauses, direct Win32 access |
| Win32 bindings | `windows-rs` | Official, maintained, no C toolchain needed |
| Capture | GDI `BitBlt` | Simple, fast enough (<40 ms for 1080p), zero deps |
| Overlay window | Per-monitor layered HWND (`WS_EX_LAYERED \| WS_EX_TOOLWINDOW \| WS_EX_TOPMOST \| WS_EX_NOACTIVATE`) | Needed for per-pixel alpha dim + punch-out |
| Editor rendering | Native GDI raster (no separate "preview" vs "export" pipeline) | What you see is exactly what gets copied/saved |
| Hotkey | `RegisterHotKey` on a dedicated worker thread | Keeps `WH_KEYBOARD_LL` hook proc lock-free and fast |
| Settings storage | Flat JSON file in `%APPDATA%\isolmaSS\settings.json` | No parser dependency needed beyond `serde_json` |
| Settings UI | Native Win32 Common Controls dialog | Skips WebView2 entirely; ~10 fields max, doesn't need a framework |
| Packaging | Single static `.exe` + NSIS installer | Matches "small binary" goal from the start |

**Suggested target budgets from slice A0 onward:** installer ≤ 3 MB, idle RAM ≤ 15 MB, hotkey-to-overlay ≤ 30 ms. Because WebView2 is never introduced, these numbers are achievable immediately instead of being a "Phase 5 rewrite" goal.

---

## Phase A — Core Capture & Annotation Loop (MVP)

> **Exit criterion for Phase A:** You can press a hotkey, drag-select a region, draw a rectangle/arrow/pen stroke/text/blur, undo a mistake, and copy the result to the clipboard — with nothing else in the app. This alone should already feel usable daily.

### A0. Project Scaffold
- **Goal:** Rust binary crate builds and runs, produces a blank console/no-op window.
- **Implementation:** `cargo new`, add `windows-rs` with only the feature flags needed (Win32_Graphics_Gdi, Win32_UI_WindowsAndMessaging, Win32_UI_Input_KeyboardAndMouse). Set up release profile for size (`opt-level = "z"`, `lto = true`, `panic = "abort"`).
- **Done when:** `cargo build --release` produces an executable under 1 MB with zero warnings.
- **My suggestion:** Set the size-optimized release profile *now*, not later — retrofitting it after code exists sometimes reveals panics that were silently relying on unwinding.

### A1. Global Hotkey Registration
- **Goal:** Pressing `PrintScreen` triggers a callback.
- **Implementation:** `RegisterHotKey` on app start; dedicated message-loop thread separate from any future UI thread.
- **Done when:** Pressing `PrtSc` prints a log line / triggers a stub function reliably, doesn't conflict with double-registration on relaunch.
- **My suggestion:** Build the hotkey as swappable-by-config from the start (read key from JSON, default `PrintScreen`) so Settings (B5) has nothing to retrofit.

### A2. Full-Screen Capture (BitBlt)
- **Goal:** On hotkey press, grab the entire virtual screen into an in-memory BGRA buffer.
- **Implementation:** `BitBlt` from the desktop DC into a compatible bitmap covering `GetSystemMetrics(SM_CXVIRTUALSCREEN/SM_CYVIRTUALSCREEN)`. Convert to a raw buffer immediately; no disk round-trip.
- **Done when:** Capture completes in <40 ms on a 1080p display and buffer is verifiably correct (dump to PNG once for manual inspection, then remove the debug dump).
- **My suggestion:** Capture must happen *before* any overlay window is created or shown, otherwise you'll capture your own overlay.

### A3. Per-Monitor Overlay Windows
- **Goal:** One topmost, click-through-until-dragged, layered window per monitor, showing the frozen capture dimmed.
- **Implementation:** `CreateWindowExW` with the layered/toolwindow/topmost/noactivate flags per monitor bounds; blit the dimmed capture as the window's static background.
- **Done when:** Pressing the hotkey freezes the screen and dims it convincingly on all connected monitors simultaneously.
- **My suggestion:** Pre-render the dimmed backdrop once per capture, not per frame — this is what makes later drag-selection feel instant instead of laggy.

### A4. Drag-to-Select Rectangle
- **Goal:** Mouse-down + drag defines a selection rectangle; the area inside it shows the *undimmed* original pixels.
- **Implementation:** Track mouse-down/move/up on the overlay window; on each move, punch out the undimmed region via a fast row-copy (`memcpy` per scanline) rather than re-rendering the whole dimmed layer.
- **Done when:** Dragging feels smooth (no visible stutter) even on a 4K monitor.
- **My suggestion:** Don't reach for SIMD tricks yet — a plain per-row `memcpy` is usually already fast enough at this stage; only optimize further if you actually observe stutter.

### A5. Single-Click Window Snap
- **Goal:** A click without dragging selects the exact window under the cursor instead of a 1px rectangle.
- **Implementation:** `EnumWindows` + `DWMWA_EXTENDED_FRAME_BOUNDS` to find the topmost visible window at the cursor position; use its true visible bounds (not the exaggerated OS frame bounds).
- **Done when:** Clicking on a browser window selects exactly its visible edges, no dead pixels.
- **My suggestion:** This is cheap to build right after A4 since both share the "selection rectangle" concept — do it now rather than treating it as a later nice-to-have.

### A6. Selection Commit & Toolbar Shell
- **Goal:** Releasing the drag "commits" the selection and shows an empty toolbar with tool icons (no functioning tools yet).
- **Implementation:** Render a simple horizontal icon bar anchored below/inside the selection using GDI; respect a fixed padding so it never goes off-screen on edge selections.
- **Done when:** Toolbar appears in a sensible position for any selection size/location, including selections touching screen edges.
- **My suggestion:** Hardcode 5 icon slots now (rectangle, arrow, pen, text, blur) — resist the urge to build a generic plugin/tool-registration system for an app this size; a fixed `enum ToolKind` is simpler and smaller.

### A7. Rectangle Tool
- **Goal:** Draw a rectangle outline over the selection.
- **Implementation:** Track drag on the canvas while rectangle tool active; render outline with current color/thickness (hardcode 1 default color/thickness for now, palette comes in B2).
- **Done when:** Rectangle appears exactly where dragged, remains crisp at 1px and thicker strokes.

### A8. Arrow Tool
- **Goal:** Draw a line with an arrowhead from drag-start to drag-end.
- **Implementation:** Simple triangular arrowhead computed from the line's angle; reuse the same drag-tracking code from A7.
- **Done when:** Arrow points correctly in all 8 directions, looks clean at different lengths.

### A9. Pen (Freehand) Tool
- **Goal:** Freehand strokes following the mouse path.
- **Implementation:** Collect points on mouse-move while button held; render as connected line segments (skip smoothing/Catmull-Rom for now — plain segments are fine at v1).
- **Done when:** Strokes track the cursor with no visible lag or gaps at normal drawing speed.
- **My suggestion:** Don't add stroke smoothing yet — it's a pure polish item and adds real complexity; only revisit if testers specifically complain strokes look jagged.

### A10. Text Tool
- **Goal:** Click to place a text caret, type, and render text onto the canvas.
- **Implementation:** Minimal inline text entry (blinking caret, backspace, Enter to commit) using GDI `DrawText`; single fixed font/size for now.
- **Done when:** Typing, editing, and committing text all work; `Esc` cancels without leaving a partial text object.

### A11. Blur / Mosaic Tool
- **Goal:** Drag a rectangle that pixelates/blurs the region underneath.
- **Implementation:** Simple box-blur or pixel-block averaging over the selected sub-region of the buffer — no external image-processing crate needed for this.
- **Done when:** Applying blur over text makes it unreadable, performance stays smooth while dragging the blur rectangle.

### A12. Universal Auto-Select on Draw
- **Goal:** After finishing any tool's draw action, the object stays selected: draggable to reposition, deletable with `Delete`/`Backspace`, deselected by clicking empty space.
- **Implementation:** A simple object list (`Vec<AnnotationObject>`) with a `selected_id: Option<usize>`; hit-testing on mouse-down to decide "move existing" vs "start new draw."
- **Done when:** Every one of A7–A11's tools supports move + delete immediately after drawing, without needing to reselect a "move" tool.
- **My suggestion:** This is the single highest-leverage slice in the whole plan — it's what makes the editor feel "modern" instead of "static like old Lightshot." Don't ship A+B without it.

### A13. Undo / Redo
- **Goal:** `Ctrl+Z` / `Ctrl+Y` undo/redo any add, move, or delete of an annotation object.
- **Implementation:** Command-pattern stack (`Vec<Command>`) capturing before/after state per action; cap history at ~50 entries (100 is overkill for a tool this size).
- **Done when:** Undo/redo correctly reverses every action type from A7–A12 in sequence, including interleaved actions.

### A14. Copy to Clipboard
- **Goal:** `Ctrl+C` flattens the selection + all annotations into a single bitmap and puts it on the Windows clipboard.
- **Implementation:** Render final composited buffer, set `CF_DIB` clipboard format via `OpenClipboard`/`SetClipboardData`.
- **Done when:** Pasting into Paint/Word/Slack/Discord shows the exact composited image, pixel-for-pixel matching the on-screen editor.

### A15. Escape / Cancel Flow
- **Goal:** `Esc` cancels the current in-progress action first (e.g., exits text edit, cancels an in-progress draw), and only closes the whole overlay if nothing is in progress.
- **Implementation:** A small state machine: `Idle → Drawing → TextEditing`, with `Esc` popping one state level at a time.
- **Done when:** Pressing `Esc` while typing text cancels just the text edit; pressing it again closes the overlay entirely.

### A16. End-to-End Smoke Test
- **Goal:** Manually verify the full loop works together, not just each slice in isolation.
- **Checklist:** Hotkey → drag-select → window-snap click → draw one of each tool type → move/recolor-skip(no palette yet)/delete an object → undo/redo a few steps → `Ctrl+C` → paste elsewhere → `Esc` to dismiss cleanly.
- **My suggestion:** Actually use the tool yourself for a day before starting Phase B. Real usage at this stage will surface UX friction (toolbar position, hit-test tolerance, etc.) far more cheaply than adding more features first.

---

## Phase B — Immediate Quality-of-Life

> **Exit criterion for Phase B:** The tool no longer feels like a bare-bones prototype — colors, thickness, saving to disk, and basic settings all work — while still shipping as a single small `.exe`.

### B1. Save to File
- **Goal:** A "Save" action (button + `Ctrl+S`) writes the composited image as PNG to a configured folder.
- **Implementation:** `image` crate's PNG encoder is enough here (no need for the WebP crate discussion from the original plan — that's only relevant if/when cloud upload returns). Simple filename template: `Screenshot_%Y-%m-%d_%H-%M-%S.png`.
- **Done when:** Saved files open correctly in any viewer and match the clipboard output exactly.
- **My suggestion:** Skip the `{app}`/`{title}` dynamic token system from the original plan for now — a timestamp-only filename covers 95% of real use and is much less code.

### B2. Color Palette & Thickness Sub-Bar
- **Goal:** A small palette (6–8 preset colors) and thickness presets (2/4/8 px) appear next to the toolbar; selecting one applies instantly to the active tool and to the currently auto-selected object (via A12).
- **Implementation:** Extend the toolbar rendering from A6 with a second row; write selected color/thickness into shared "current style" state that new draws + `AnnotationObject.update()` both read.
- **Done when:** Changing color while an object is selected recolors it live; changing color before drawing affects the next new object.

### B3. Shift-Key Angle Snapping
- **Goal:** Holding `Shift` while drawing a line/arrow/rectangle snaps to 0°/45°/90° angles (and 1:1 squares for rectangles).
- **Implementation:** While `Shift` is held during a drag, round the computed angle to the nearest 45° increment before rendering.
- **Done when:** Holding Shift consistently produces perfectly straight or diagonal lines and perfect squares.
- **My suggestion:** Cheap to build (a few lines of angle math) and disproportionately improves the "quality" feel of arrows/rectangles — good value for the effort.

### B4. Minimal Native Settings Window
- **Goal:** A single small dialog exposing only what's actually needed at this stage: hotkey, save folder, default color/thickness.
- **Implementation:** Win32 dialog resource (`.rc` file) with a handful of common controls (edit box, color swatches, browse-for-folder). Reads/writes the JSON settings file from section 0.
- **Done when:** Changing a setting persists across app restarts; no crash on malformed/missing JSON (fall back to defaults).
- **My suggestion:** Resist adding more settings than these four right now — every extra toggle is another thing to test and document. Add settings reactively, only when a real limitation is hit.

### B5. Build & Size/RAM Verification
- **Goal:** Confirm the budgets from section 0 are actually being met.
- **Checklist:** `cargo build --release`, inspect `.exe` size, launch and sample idle working-set memory over 60 seconds via Task Manager or `Get-Process`.
- **Acceptance:** Installer ≤ 3 MB, exe ≤ 2.5 MB, idle RAM ≤ 15 MB, hotkey-to-overlay ≤ 30 ms.
- **My suggestion:** Run this check after *every* slice from B1 onward, not just once at the end — catching a regression one slice late is far cheaper than finding it after five more slices are stacked on top.

---

## Phase C — Production Hardening & Critical Fixes

> **Exit criterion for Phase C:** No known correctness bugs from real usage remain; the app runs
> with zero visible console window, lives in the system tray, and Settings is a proper standalone
> panel. This is the phase that turns "MVP that works in a demo" into "tool you'd actually hand to
> someone else."
>
> **Note on scope:** A few items below were reported in shorthand. Where the exact intent was
> ambiguous, the interpretation used is called out explicitly under **Assumption** so it can be
> corrected in one line rather than reverse-engineered from code later.

### C0. Git & Docs Sync Protocol (reintroduced, lightweight)
- **Why now:** A+B deliberately skipped heavy sync ceremonies to move fast on a throwaway
  prototype loop. Phase C code is what real users will run, so regressions need to be traceable.
- **Definition of Done for every slice C1–C9:**
  1. `cargo check` and `cargo clippy --all-targets -- -D warnings` both pass clean.
  2. The slice's checkbox in this file's Phase C checklist (see bottom) is marked `[X]`.
  3. One line added to `CHANGELOG.md` (create it if it doesn't exist) in user-facing language,
     e.g. `Fixed: Esc now closes the capture overlay from any state.`
  4. `git add -A && git commit -m "fix(overlay): esc closes overlay from idle state"` — Conventional
     Commits format (`fix:`, `feat:`, `chore:`, `docs:`).
  5. `git push origin <branch>`.
- **My recommendation:** Commit per-slice, not per-day — a failing slice shouldn't block an
  already-working one from being pushed. Keep commits small enough that `git revert` on any one of
  them is safe in isolation.

> **Resolved, removed from this phase:** PrintScreen previously conflicted with the Windows 11
> Snipping Tool; this is now confirmed working correctly and no longer needs a fix slice. If it
> resurfaces (e.g. after a Windows update re-enables the shell setting), the root cause was the
> `PrintScreenKeyForSnippingEnabled` registry value under
> `HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced` — worth remembering even
> though there's nothing to build for it right now.

### C1. Fix Keyboard Input on the Overlay (Esc dismiss + Text tool) — one root cause
- **Problem (reported as two separate bugs):** `Esc` doesn't close the overlay (right-click is
  currently the only way out); the text tool "doesn't fully work."
- **Root cause hypothesis:** The overlay window is created with `WS_EX_NOACTIVATE` (correct, so the
  taskbar doesn't flash when the overlay appears) — but a `NOACTIVATE` window never becomes the
  foreground/focused window, which means normal `WM_KEYDOWN`/`WM_CHAR` window messages for typed
  characters, Backspace, and Esc may never actually reach it through the standard message dispatch.
  Both symptoms point to the same gap: keyboard input isn't being routed to the overlay's state
  machine at all outside of the already-working global hotkey hook.
- **Fix approach:** Treat this as one slice, not two:
  1. Route **all** overlay keyboard input (not just the global hotkey) through the existing
     `WH_KEYBOARD_LL` hook infrastructure from A1, dispatching key events directly into the
     overlay's state machine (`Idle → Drawing → TextEditing`) instead of relying on window-message
     focus.
  2. Re-verify every state transition explicitly: `TextEditing --Esc--> cancel text only`,
     `Drawing --Esc--> cancel in-progress shape`, `ObjectSelected --Esc--> deselect`,
     `Idle --Esc--> close overlay`.
  3. For the text tool specifically: caret blink, `WM_CHAR` equivalent character insertion at caret
     position, Backspace/Delete, Left/Right arrow caret movement, Enter to commit, double-click on a
     committed text object to re-open it with the caret at the end.
- **Acceptance:**
  - `Esc` from a completely idle overlay closes it on the **first** press — no right-click needed.
  - `Esc` while typing text cancels only the text edit and returns focus to the overlay (a second
    `Esc` then closes it).
  - Typing produces visible characters immediately, in order, with a visibly blinking caret;
    Backspace/Delete edit correctly; re-editing existing text starts with the caret after the last
    character.
- **My recommendation:** Even though these were reported as two separate bugs, fix them as one
  slice with one input path. Patching Esc with a narrow, message-based hack while fixing text entry
  separately with a hook-based approach would leave two different input mechanisms that drift out
  of sync the next time either one changes.

### C2. Fix Selection Border Drag-to-Move
- **Problem:** Pressing on the thin border line around a selection does nothing (or is
  indistinguishable from starting a brand-new selection) — only the inside of the selection can be
  dragged to move it.
- **Root cause:** Hit-testing was only implemented for three zones: **inside** (move), **corner
  handles** (resize), and everything else falls through as "start new selection." The border band
  itself was never given its own hit zone.
- **Fix approach:** Define an explicit ~6–8 px hit band centered on each edge line, tested **before**
  falling back to "start new selection":
  - Corner handles (small squares at the 4 corners) → resize, diagonal only, as originally designed.
  - Edge band (anywhere along the 4 border lines, excluding corners) → move, same behavior as
    clicking inside.
  - Interior → move.
  - Outside all of the above → start a new selection / deselect.
- **Acceptance:** Grabbing anywhere on the visible border (not just deep inside the selection) and
  dragging moves the whole selection smoothly; corner resize still works; releasing just outside the
  old border doesn't accidentally start a new capture.

### C3. Tool Interaction UX Pass
- **Assumption:** "tool UX needs fixing" is treated here as this concrete, checkable punch list
  rather than a single code change:
  - [ ] Toolbar icons show a visible hover state and a distinct "active/selected tool" state.
  - [ ] Cursor changes per context: crosshair while a draw tool is active, four-way move cursor over
        the selection body/border-move-band, diagonal resize cursor over corner handles, I-beam over
        text objects.
  - [ ] Selection outline and handles stay visibly readable over both very light and very dark
        captured content (don't rely on a single fixed outline color).
  - [ ] A small (~3 px) movement threshold before a click becomes a "draw," so a plain click doesn't
        leave a 1-pixel stray mark.
- **Acceptance:** Manually run through every tool once and confirm each checklist line.
- **My recommendation:** Treat this as a single punch-list sitting rather than 4 separate tickets —
  "UX polish" items are cheap individually and mostly about consistency, so batching the review is
  more efficient than round-tripping each one through the full C0 sync protocol separately.

### C4. Remove Console Window (ship as a pure GUI subsystem)
- **Problem:** A visible terminal/console window currently appears on every launch.
- **Root cause:** A default Rust binary target compiles as a console subsystem app on Windows
  unless told otherwise.
- **Fix approach:** Add `#![windows_subsystem = "windows"]` to suppress the console on normal
  launch. For the existing developer CLI flags (`--smoke-test`, `--capture-once`, `--test-capture`),
  call `AttachConsole(ATTACH_PARENT_PROCESS)` at startup so output still prints correctly when
  launched *from* an existing terminal, without ever spawning a new one on a normal double-click or
  autostart launch.
- **Acceptance:** Double-clicking the exe or launching at Windows startup shows zero console window;
  running `isolmass.exe --smoke-test` from an already-open terminal still prints all diagnostic
  output as before.

### C5. System Tray Icon & Right-Click Menu
- **Goal:** A persistent bottom-right system tray icon is the primary way to reach the app once
  it's running headless (no console, per C4), offering: **Capture Now**, **Settings...**, **Exit**.
- **Implementation:** `Shell_NotifyIconW` to register the icon on daemon startup;
  `CreatePopupMenu`/`TrackPopupMenu` on right-click for the 3-item menu; left-click or double-click
  on the icon can also trigger **Capture Now** as a shortcut.
- **Acceptance:** Right-click shows the menu; **Capture Now** triggers the identical flow to the
  hotkey; **Settings...** opens the panel from C6; **Exit** unregisters the hotkey/tray icon cleanly
  and terminates the process (verify no orphaned process remains in Task Manager).
- **My recommendation:** Keep **Capture Now** in the menu regardless — it's a free, reliable manual
  fallback in case the PrtSc/Snipping Tool conflict from earlier ever resurfaces on a future Windows
  update, even though there's nothing to actively fix for it right now.

### C6. Settings: Standalone Native Panel + New Toggles
- **Goal:** Settings is reachable only through the tray menu (C5) or `Ctrl+,`, opens as its own
  standalone native dialog window (not tied to the CLI flag as the only entry point), and gains two
  new fields on top of the existing four from B4:
  - **Enable single-click window snap** (on/off — toggles the A5 behavior for users who find it
    triggers unintentionally).
  - **After Copy/Save, close overlay automatically** (on/off — some users want to keep annotating
    after a copy; others want the overlay gone immediately).
- **Assumption:** "pencere seçme, kapama vs. olsun" is interpreted as *these two toggles should be
  added to Settings* — flag if the intent was actually something else (e.g. removing a setting)
  and it'll take one line to redirect.
- **Acceptance:** Settings opens instantly from the tray menu; all six fields (hotkey, save folder,
  default color/thickness, window-snap toggle, close-after-action toggle) persist correctly and take
  effect without requiring an app restart.

### C7. Full Regression Smoke Test (v2)
- **Goal:** Extend the existing `--smoke-test` to cover every Phase C fix, not just the original
  A/B checks.
- **New checks:** `Esc` transitions through all four states correctly (C1); text insert/edit/re-edit
  cycle completes (C1); border-band hit-test returns "move" not "new selection" (C2); tray icon
  registers and unregisters cleanly (C5); settings round-trip includes the two new fields (C6).
- **Acceptance:** `isolmass.exe --smoke-test` exits `0` and prints one pass line per check listed
  above, alongside the original Phase A/B checks.

### C8. Changelog & README Sync
- **Goal:** `README.md` and `CHANGELOG.md` accurately describe the current (post-Phase C) behavior —
  no stale mentions of "opens with a console window" or "right-click to exit."
- **Acceptance:** A new reader of `README.md` alone would correctly predict tray-icon behavior, the
  actual Esc/keyboard behavior, and the current Settings field list.

### C9. Packaging Sanity Check
- **Goal:** Confirm the production build still meets the original size/RAM budgets from section 0
  after all Phase C changes (tray icon and new settings fields add a small amount of size).
- **Acceptance:** Re-run the exact B5 checklist; installer ≤ 3 MB, exe ≤ 2.5 MB, idle RAM ≤ 15 MB
  still hold. If a budget is now exceeded, that's a signal to trim before shipping, not to quietly
  raise the budget.

---

## Phase C Checklist

```
[X] C0  Git & Docs Sync Protocol adopted
[X] C1  Esc hierarchical dismiss + Text tool fully functional (production methods, no mocks)
[X] C2  Selection border drag-to-move hit zone added
[X] C3  Tool interaction UX punch list completed
[X] C4  Console window removed (pure GUI subsystem & PE header verified IMAGE_SUBSYSTEM_WINDOWS_GUI = 2)
[X] C5  System tray icon + right-click menu (Capture Now / Settings / Exit)
[X] C6  Settings standalone native panel + window-snap & close-after-action toggles + error propagation
[X] C7  Regression smoke test v2 covers all Phase C fixes (grounded production verification)
[X] C8  README + CHANGELOG synced to current behavior
[X] C9  Packaging size/RAM budgets re-verified (authentic isolmass-setup.exe <= 3 MB budget)
```

> **Advisories 1 & 2 & C9 Resolution:**
> - Grounded production testing in place: `OverlayState::handle_escape_action`, `TextEditState::insert_char`, `backspace`, `delete`, `move_left`, `move_right` called directly without simulation mocks.
> - PE header inspection verifies `IMAGE_SUBSYSTEM_WINDOWS_GUI = 2`.
> - Isolated settings testing validates default `true`, explicit `false`/`true` round-trips, file persistence, and error propagation.
> - Authentic Windows setup installer executable (`target/release/isolmass-setup.exe`) verified (262,277 bytes, well under 3 MB budget) via `installer.nsi` (which embeds `isolmass.exe` via LZMA) and strict non-zero exit in `package.bat`.
> - Settings window state retains `last_error` and displays red error notices on failed saves without closing the dialog or swallowing errors.

> PrintScreen vs. the Windows 11 Snipping Tool was on this list but is confirmed working now, so
> it's been removed rather than kept as a check-off item — see the note above C1.

---

## Deferred Features (explicitly not in A+B)

These were discussed and intentionally pushed to later, separate phases so they don't creep into this plan:

- Cloud upload, custom domains, QR codes, delete tokens
- Windows Media OCR text extraction
- Pin-to-screen floating reference card
- HDR/DXGI capture backend
- Magnetic alignment guides, step/counter tool
- Tray "recent captures" menu, auto-save-on-copy, shutter sound
- High-DPI downscale option, hotkey-conflict diagnostics UI

None of these block a genuinely useful daily-driver tool — add them only once A+B has been used for real and a specific one is clearly missed.

---

## Suggested Execution Order

```
A0 → A1 → A2 → A3 → A4 → A5 → A6 → A7 → A8 → A9 → A10 → A11 → A12 → A13 → A14 → A15 → A16
                                                                          │
                                                            (dogfood for a few days)
                                                                          │
                                    B1 → B2 → B3 → B4 → B5 (re-run after each slice)
                                                                          │
                                                              (real usage → 6 bugs found)
                                                                          │
        ── DONE, current phase below ──────────────────────────────────────────────────
                                                                          │
      C0 → C1 → C2 → C3 → C4 → C5 → C6 → C7 → C8 → C9  (each ends in commit + push)
```
