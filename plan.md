# isolmaSS — Lean MVP Plan (Phase A + B)

> **Scope of this document:** Only the "must-have" (A) and "ship right after" (B) tiers from the
> feature triage. Cloud upload, OCR, pin-to-screen, HDR capture, magnetic guides, etc. are
> intentionally **out of scope** here — see "Deferred Features" at the bottom.
>
> **Design philosophy for this plan:**
> 1. No WebView2 / React anywhere, not even for Settings — build native from day one so there is
>    never a "migrate away from WebView2 later" slice.
> 2. No cloud/network code in A+B — the tool must be 100% useful completely offline first.
> 3. Every slice ships something you can actually press a key and see working. No slice should
>    take more than a day or two.
> 4. Definition of Done is intentionally light (code compiles, manual smoke test passes, one-line
>    doc update) — heavy multi-artifact sync ceremonies are dropped to keep iteration fast.

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
```
