# Repository Guidelines

> **Version:** 1.2.0 · **Last updated:** 2026-09-25
> MUST/SHOULD/MAY follow RFC 2119. Higher-priority instructions and explicit user requests take precedence; more specific directory guidance overrides this file.

## Project Overview

isolmaSS (`isolmass.exe`) is a local-first Rust/Win32 tray screenshot editor: capture, annotate, copy, or save. Optional sharing installs a Worker in the user's own Cloudflare account. The independent public site hosts downloads/docs, not screenshots. Updates require a pinned publisher signature.

## Architecture & Data Flow

- `src/main.rs` handles CLI, single-instance startup, COM/DPI, Win32 message loop, tray/hotkey events, capture scheduling, and shutdown. A hotkey queues a tray command; `src/capture.rs` captures the virtual screen; `src/overlay.rs` and `src/overlay/` run the editor.
- `OverlayState` owns selection, tools, render cache, and editor lifetime. `src/annotation.rs` models objects and undo/redo commands; `src/toolbar.rs` lays out controls. Export flattens the selection without editor chrome/previews, then `src/clipboard.rs` or `src/save.rs` writes it.
- `src/settings.rs` validates and atomically persists settings; native dialogs live in `src/settings_window.rs` and `src/cloud_settings_window.rs`. `src/instance.rs` supplies locks; `src/tray.rs`, `src/hotkey.rs`, `src/updater.rs`, and `src/updater/job.rs` handle background app activity.
- For optional uploads, `src/cloudflare_oauth.rs`/`src/cloudflare_setup.rs` provision the user's Worker; `src/upload.rs` sends the flattened image on a background thread. `cloudflare/worker.mjs` checks upload/admin tokens and routes to the SQLite `ShareStore` Durable Object, which retains and serves `/i/:id` images. Credentials use local Windows DPAPI, not `settings.json`.

## Key Directories

- `src/`: native app and in-module tests; `src/overlay/`, `src/updater/`, `src/save/`, `src/tray/`: focused submodules.
- `cloudflare/`: per-user sharing Worker and Wrangler config; `site/`: unrelated static information site and its own Wrangler config.
- `resources/`: PE icon/manifest and pinned update public key; `scripts/`: publisher signing helper; `.github/workflows/`: Windows CI.

## Development Commands

On Windows x64, use the pinned toolchain and `Cargo.lock`:

```powershell
cargo fmt --check
cargo check --locked
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked -- --test-threads=1 --skip test_tray_manager_lifecycle
cargo build --release --locked
cargo run --locked -- --help
cmd /c package.bat
```

Plain `cargo run --locked` starts the tray app; `--capture-once` requires an interactive desktop. `package.bat` requires NSIS and checks size/version/SHA-256; use `ISOLMASS_BUILD_DIR=target\release-candidate` if the release EXE is running. `release.bat` requires the publisher's non-exportable signing key; do not run for routine builds. The site owner deploys from `site/` with `wrangler deploy`; user Worker installation runs through the app's OAuth setup.

## Code Conventions & Common Patterns

- Rust 2024, rustfmt, Clippy `-D warnings`; `snake_case` functions/modules, `PascalCase` types, uppercase constants. Keep localized UI text consistent with nearby code.
- Win32/COM/GDI `unsafe` calls are paired with RAII `Drop` guards; release handles/files on every path. Avoid blocking/allocations in hooks, window procedures, render paths, and callbacks.
- Propagate/contextualize errors with `Result`; surface UI failures through `src/ui.rs`/tray and diagnostics through `src/diagnostics.rs`. Validate settings, files, network receipts, and Worker requests at boundaries. Blur is **not** secure redaction; `AnnotationKind::Redact` is opaque.
- Explicit structs (`OverlayState`, `Settings`, `HistoryManager`) own state; no DI container. Pass dependencies such as settings/capture and injectable persistence paths directly. Native background work uses `std::thread` plus queues/posted Win32 messages (e.g., `WM_UPLOAD_DONE`) to return to the UI thread; the Worker uses async `fetch` and SQLite transactions.
- Settings writes validate first, hold a cross-process lock, and atomically replace via a same-directory temporary file. Preserve invalid JSON before reset; propagate other I/O failures. Keep Worker auth, request-size bounds, quotas, retention, and transactional counters intact.

## Important Files

- `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`: crate/locked dependencies/Rust 1.98.0; `build.rs`: Windows resources via `rc.exe`; `installer.nsi`, `package.bat`, `release.bat`: distribution.
- `src/main.rs`, `src/overlay.rs`, `src/annotation.rs`, `src/settings.rs`, `src/cloudflare_setup.rs`, `src/upload.rs`, `cloudflare/worker.mjs`: principal implementation paths.
- `README.md`: usage/build, sharing HTTP contract, and manual acceptance; `DISTRIBUTION.md`, `SECURITY.md`: trust/release rules; `THIRD_PARTY_NOTICES.md`: dependency licenses; `.github/workflows/windows.yml`: CI commands.

## Runtime/Tooling Preferences

Requires Windows x64, Rust **1.98.0** with rustfmt/Clippy, Windows SDK (`rc.exe`), Visual Studio C++ Build Tools, and NSIS for packaging. Cargo is the package manager; no Node/Bun manifest or JS test runner is configured. Wrangler is an account-owner deployment tool, not a Windows app dependency. External dependencies need explicit approval; update manifests and lockfiles together. Never commit/log secrets, `.env` files, certificates, private keys, or screenshot pixels; report discovered uncommitted secrets without revealing values.

## Testing & QA

Existing Rust `#[cfg(test)]` modules live next to code (e.g., `src/annotation.rs`, `src/settings.rs`). CI runs formatting, check, Clippy, serialized noninteractive tests with `test_tray_manager_lifecycle` skipped, release build, and unsigned packaging; no coverage threshold is configured. Run focused checks during development and the broadest practical relevant checks before delivery. Existing tests should assert behavior, not source shape; **new test files/helpers require explicit user approval**. Report any verification limitations.

`isolmass.exe --smoke-test` requires an interactive desktop, uses hooks/tray resources, and **changes the clipboard**; preserve it or use a disposable desktop. UI/theme/DPI and real cross-account Cloudflare installation/upload require manual acceptance; local Worker checks are not live-account proof. Test updater rollback in an isolated Windows profile/VM. `--verify-update PATH` checks a signed installer without executing it.

## Working Rules

Preserve unrelated user changes; work only within the request, fix root causes, migrate all callers, remove obsolete code in scope, and avoid speculative abstractions. Obtain approval before destructive/irreversible commands, including recursive deletion, forced history changes, and data migrations; explain loss and rollback. Validate untrusted boundaries, bound hot-path work/resources, release finite-lived resources, and never silently swallow unexpected errors. Do not add dependencies or new test files without approval. Update existing public/API/configuration docs when behavior changes; describe current behavior and constraints, not version-by-version retrospectives. Keep scratch files outside the tracked tree, remove introduced artifacts, commit only if requested, and ground delivery in observed checks. Update this version/date when rules change.
