# Product status and roadmap

## Current release: 0.5.2

isolmaSS is a local, native Windows tray capture editor. Its current interface has three compact, content-sized settings tabs with a two-column General view, inline update progress/results, System/Light/Dark appearance, recorded keyboard shortcuts with conflict handling, native color selection with three quick colors and a persistent custom swatch, 1–64 px width controls with numeric entry and wheel steps, eight selection handles and movable dimensions label, a four-tool rail with expandable drawing tools, translucent highlighter, numbered steps, opaque redaction, copy/save/export and a native tray menu. The updater checks a public releases-only repository, asks before every installation, pins a publisher signature, waits for active work, and retains an executable rollback until startup health is checked.

0.5.0 adds an optional Upload action (`Ctrl+U`) for each user's own Cloudflare Worker/R2/D1 installation. The separate Cloudflare settings window provisions pairing tokens, daily limits and retention, exposes upload/view/storage statistics and approximate cost, and allows manual image deletion. The static `site/` serves download/docs/privacy material without hosting user screenshots. In 0.5.1 the first Upload starts guided setup and resumes the same selected upload only after pairing; interrupted setup keys remain DPAPI-protected. No domain is required or connected automatically. Default capture and save remain local. Build and trust requirements are in [README.md](README.md), [SECURITY.md](SECURITY.md) and [DISTRIBUTION.md](DISTRIBUTION.md). The source repository is public for self-host templates; the separate updates repository exposes release metadata and artifacts only.

## Cloudflare sharing in 0.5.2

Version 0.5.2 replaces manual deployment with PKCE login, explicit choice when multiple Cloudflare accounts exist, a new isolated Worker and SQLite Durable Object storage with automatic workers.dev setup. GitHub login, R2 and D1 subscription are not required for this installation. The publisher's domain-verified OAuth client is Public with the owner's approval. Local Worker and Windows checks pass; real deployment and upload in another Cloudflare account remain user-run acceptance tests, not claimed production proof.

## Acceptance checks

- Rust formatting, lint and noninteractive unit checks cover the current source.
- A targeted Windows desktop pass exercised dark/light Settings, automatic tab sizing, retained manual window sizing, inline up-to-date feedback and the tray's native result dialog. A real newer-version install and restart still require an isolated profile/VM.
- The package gate checks the executable and installer versions, size budgets, SHA-256 and pinned signature. A modified signature must be rejected.
- Interactive acceptance belongs on a Windows desktop: both themes and system switching; each tab and shortcut conflict; every tool's preview, selection/edit, undo/redo and exported pixels; save and clipboard results; tray commands; accessibility at scaled DPI; manual and skipped update prompts; startup health and rollback in an isolated profile/VM.
- The first installation of this trust model is manual for users whose installed executable cannot verify the pinned key. SmartScreen warnings are possible and are never bypassed automatically.

The interactive desktop pass and all real Cloudflare account deployment/usage tests are user-owned. Local Worker simulation and compilation are not live Cloudflare proof. The source and release metadata must not represent either as automated production proof.

## Deferred candidates

Scrolling capture, OCR, video/GIF capture and non-Cloudflare backends (PHP, Next.js, other hosting) are deferred. The 0.5.x releases cover only opt-in self-hosted Cloudflare uploads; it remains local-first, and blur is not a substitute for opaque redaction.
