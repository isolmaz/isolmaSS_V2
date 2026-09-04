# isolmaSS Roadmap

This document tracks durable implementation status, outstanding gaps, and future direction. See [README.md](README.md) for current features and usage, and [CHANGELOG.md](CHANGELOG.md) for release history. The completed A–C work remains a native, offline-first Win32/GDI application; no web UI or cloud/network dependency is part of those phases.

## Current Status

- **Phase A — Core capture and annotation:** Implemented. The native capture overlay supports region and window selection, annotation tools, undo/redo, clipboard copy, and hierarchical cancellation.
- **Phase B — Quality of life:** Implemented. Configurable PNG/JPEG saving, drawing styles, angle/square snapping, native settings, and release-budget checks are present.
- **Phase C — Production hardening:** C0–C8 are implemented, including keyboard routing and text editing, selection manipulation, interaction polish, GUI-subsystem operation, tray update/recent-capture actions, settings behavior toggles, regression smoke coverage, and synchronized user documentation.
- **C9 — Packaging and runtime reverification:** Incomplete. Packaging, artifact-size, daemon-memory, shutdown, and console checks passed, but the latency target did not.

## Verification State

The latest recorded verification is from 2026-09-04:

- `cargo check`, `cargo clippy --all-targets -- -D warnings`, `cargo test` (7 passed), the release build, and `cmd /c package.bat` passed after the UI/rendering changes.
- One post-change `--smoke-test` run exited successfully with `AUTOMATED SMOKE CHECKS PASSED (A1-C8 + C9 packaging checks)`. This command does not verify external runtime budgets or replace interactive UX testing.
- The release executable measured **531,968 bytes**, below the 2.5 MiB target.
- The NSIS installer measured **281,904 bytes**, below the 3 MiB target.
- During a **60.039-second** daemon sample, the maximum working set was **10.640625 MiB**, below the 15 MiB target.
- In a targeted runtime scenario, Esc closed the overlay in 15.801 ms, the daemon remained alive, clean `WM_CLOSE` shutdown exited successfully, and no console `HWND` was observed.
- With MSI Afterburner and RTSS running, a representative native pass created a selection, drew and moved a rectangle annotation, moved and resized the selection, and visually showed only the current selection, annotation, and repositioned toolbar pixels. The visible tray menu exposed update and recent-capture items; Settings opened from it, accepted a temporary JPEG selection, and canceled without saving. This targeted pass is not exhaustive UX coverage.

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
2. Complete an interactive manual pass of annotation, Settings, and tray-menu workflows; keep its results distinct from automated smoke coverage.
3. Re-run release artifact and runtime budgets after relevant source or toolchain changes.
4. Use real-world feedback to choose among deferred features rather than expanding scope speculatively.
