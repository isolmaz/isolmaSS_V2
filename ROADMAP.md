# Product status and roadmap

## Current release: 0.4.1

isolmaSS is a local, native Windows tray capture editor. Its current interface has three settings tabs, System/Light/Dark appearance, recorded keyboard shortcuts with conflict handling, native color selection with three quick colors and a persistent custom swatch, 1–64 px width controls with numeric entry and wheel steps, eight selection handles and movable dimensions label, a four-tool rail with expandable drawing tools, translucent highlighter, numbered steps, opaque redaction, copy/save/export and a native tray menu. The updater checks a public releases-only repository, asks before every installation, pins a publisher signature, waits for active work, and retains an executable rollback until startup health is checked.

Build and trust requirements are in [README.md](README.md), [SECURITY.md](SECURITY.md) and [DISTRIBUTION.md](DISTRIBUTION.md). The source repository stays private; the public repository exposes only release metadata and artifacts.

## Acceptance checks

- Rust formatting, lint and noninteractive unit checks cover the current source.
- The package gate checks the executable and installer versions, size budgets, SHA-256 and pinned signature. A modified signature must be rejected.
- Interactive acceptance belongs on a Windows desktop: both themes and system switching; each tab and shortcut conflict; every tool's preview, selection/edit, undo/redo and exported pixels; save and clipboard results; tray commands; accessibility at scaled DPI; manual and skipped update prompts; startup health and rollback in an isolated profile/VM.
- The first installation of this trust model is manual for users whose installed executable cannot verify the pinned key. SmartScreen warnings are possible and are never bypassed automatically.

The interactive desktop pass is user-owned. The source and release metadata must not represent it as automated proof.

## Deferred candidates

Scrolling capture, OCR, video/GIF capture and cloud uploads are **not** part of 0.4.1. Evaluate them only with a separate privacy, performance and product brief. The current release remains local-first; blur is not a substitute for opaque redaction.
