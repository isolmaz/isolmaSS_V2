# isolmaSS Roadmap

This document tracks durable implementation status, outstanding gaps, and future direction. See [README.md](README.md) for current features and usage, and [CHANGELOG.md](CHANGELOG.md) for release history. The completed A–C work remains a native, offline-first Win32/GDI application; no web UI or cloud/network dependency is part of those phases.

## Current Status

- **Phase A — Core capture and annotation:** Implemented. The explicit `V` Select tool owns existing-object and screenshot-region editing; drawing tools stay active and do not auto-select newly committed annotations. Selected rectangles, blurs, arrows, pen strokes, and text move from their interiors and resize through type-appropriate handles with one undoable command per completed gesture.
- **Phase B — Quality of life:** Implemented. Configurable PNG/JPEG saving, contextual named style controls, angle/square snapping, and a compact 700×620 light native Simple/Advanced Settings window are present.
- **Phase C — Production hardening:** C0–C8 are implemented, including keyboard/text routing, clean export composition, monitor-work-area-bounded L-toolbar layout and tooltips, deterministic full-frame rendering, GUI-subsystem operation, tray actions, settings behavior toggles, regression coverage, and synchronized user documentation.
- **C9 — Packaging and runtime reverification:** Incomplete. Packaging, artifact-size, daemon-memory, shutdown, and console checks passed, but the latency target did not.

## Verification State

The latest recorded verification is from 2026-09-04:

- `cargo fmt --check`, `cargo check`, `cargo clippy --all-targets -- -D warnings`, `cargo test` (**17 passed**), the release build, and `cmd /c package.bat` passed after the UI/rendering changes.
- `--smoke-test` was intentionally not rerun. Safe direct-HWND automation exercised normal and full-primary-work-area selections without synthesizing PrintScreen; the latest pass did not repeat every object transform.
- The release executable measured **546,304 bytes**, below the 2.5 MiB target.
- The unsigned NSIS installer measured **288,209 bytes**, below the 3 MiB target; SHA-256: `a29178bbce0b720becd83efbc8ed18a96aaf66b225e589f7d09e8b653bfd3622`. No signing certificate was configured.
- During a **60.039-second** daemon sample, the maximum working set was **10.640625 MiB**, below the 15 MiB target.
- In a targeted runtime scenario, Esc closed the overlay in 15.801 ms, the daemon remained alive, clean `WM_CLOSE` shutdown exited successfully, and no console `HWND` was observed.
- A representative pass with MSI Afterburner and RTSS and the latest full-window pass showed deterministic rendering without trails. This is targeted evidence, not exhaustive hardware compatibility.
- Safe direct-HWND runtime automation exercised a normal selection with the padded L outside and a full 1920×1032 primary-work-area selection with the complete bottom-right L inside at a 10px inset. Neither panel crossed the x=1920 seam into the adjacent monitor; direct image/source inspection confirmed all four core action icons were visible. Its saved 1920×1032 image contained no toolbar, border, handles, caret, tooltip, or in-progress preview.
- The compact Settings UI was exercised at 96 DPI in both Simple and Advanced views: all controls were visible without clipping, unsaved state survived view switches, PNG natively disabled JPEG quality, the standard Save & Apply then Cancel footer remained available, and Cancel left `settings.json` byte-identical. Both overlay paths pass an explicit modal owner; standalone/tray Settings remains non-topmost. Alternate-DPI runtime was not performed.

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
2. Complete a manual editor pass for every Select/object-transform path not covered by the latest safe direct-HWND normal/full-selection runs; Settings has been exercised at 96 DPI, while alternate-DPI runtime coverage remains open.
3. Re-run release artifact and runtime budgets after relevant source or toolchain changes.
4. Use real-world feedback to choose among deferred features rather than expanding scope speculatively.
