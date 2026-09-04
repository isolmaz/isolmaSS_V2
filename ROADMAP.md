# isolmaSS Roadmap

This document tracks durable implementation status, outstanding gaps, and future direction. See [README.md](README.md) for current features and usage, and [CHANGELOG.md](CHANGELOG.md) for release history. The completed A–C work remains a native, offline-first Win32/GDI application; no web UI or cloud/network dependency is part of those phases.

## Current Status

- **Phase A — Core capture and annotation:** Implemented. The explicit `V` Select tool owns existing-object and screenshot-region editing; drawing tools stay active and do not auto-select newly committed annotations. Selected rectangles, blurs, arrows, pen strokes, and text move from their interiors and resize through type-appropriate handles with one undoable command per completed gesture.
- **Phase B — Quality of life:** Implemented. Configurable PNG/JPEG saving, contextual named style controls, angle/square snapping, and a 720×720 Simple/Advanced native Settings window are present.
- **Phase C — Production hardening:** C0–C8 are implemented, including keyboard/text routing, commit-before-copy flattening, bounded bottom-right L-toolbar layout and tooltips, deterministic full-frame rendering, GUI-subsystem operation, tray actions, settings behavior toggles, regression coverage, and synchronized user documentation.
- **C9 — Packaging and runtime reverification:** Incomplete. Packaging, artifact-size, daemon-memory, shutdown, and console checks passed, but the latency target did not.

## Verification State

The latest recorded verification is from 2026-09-04:

- `cargo fmt --check`, `cargo check`, `cargo clippy --all-targets -- -D warnings`, `cargo test` (15 passed), the release build, and `cmd /c package.bat` passed after the UI/rendering changes.
- `--smoke-test` was intentionally not rerun after UI-automation cleanup; current editor interactions received model/source and non-interactive verification only.
- The release executable measured **542,720 bytes**, below the 2.5 MiB target.
- The NSIS installer measured **286,337 bytes**, below the 3 MiB target.
- During a **60.039-second** daemon sample, the maximum working set was **10.640625 MiB**, below the 15 MiB target.
- In a targeted runtime scenario, Esc closed the overlay in 15.801 ms, the daemon remained alive, clean `WM_CLOSE` shutdown exited successfully, and no console `HWND` was observed.
- A prior pass with MSI Afterburner and RTSS visually confirmed deterministic rendering during selection, rectangle, and selection-region movement. After cleanup of disruptive UI automation, the current Select/object-transform and L-toolbar changes received model/source and non-interactive build/test verification only; their editor interactions were not runtime-automated.
- The redesigned 720×720 Settings UI was exercised at 96 DPI in both Simple and Advanced views: all controls were visible without clipping, unsaved state survived view switches, the footer remained available, and Cancel exited normally with `settings.json` byte-identical. No alternate-DPI monitor was available.

## Open Gap

PrintScreen-to-visible-overlay latency measured **48.936 ms**, above the **≤30 ms** target. C9 remains open. A previous incomplete optimization was removed to preserve the coherent GDI `BitBlt` capture path; future latency work must retain capture correctness and be validated with an actual visible-overlay measurement.

## Deferred Features

These remain intentionally outside the completed A–C scope and should be added only in response to demonstrated user need:

- Cloud upload, custom domains, QR codes, and delete tokens
- OCR text extraction
- Pin-to-screen reference cards
- HDR/DXGI capture
- Magnetic alignment guides and a step/counter tool
- Auto-save-on-copy and shutter sound
- High-DPI downscaling and hotkey-conflict diagnostics

## Future Priorities

1. Reduce PrintScreen-to-visible-overlay latency to ≤30 ms without weakening capture correctness, then repeat the full C9 runtime measurement.
2. Complete a manual editor pass for every Select/object-transform and contextual-toolbar path without global-key automation; Settings has already been exercised at 96 DPI, but alternate-DPI coverage remains open.
3. Re-run release artifact and runtime budgets after relevant source or toolchain changes.
4. Use real-world feedback to choose among deferred features rather than expanding scope speculatively.
