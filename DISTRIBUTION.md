# Windows distribution

The public source repository builds the program and supplies the Cloudflare self-host template; the separate **public** [`isolmaz/isolmaSS-updates`](https://github.com/isolmaz/isolmaSS-updates) repository contains release metadata and binaries only. A release tag is `vMAJOR.MINOR.PATCH` and must match `Cargo.toml`, the Rust PE resource and the NSIS installer version. Do not publish source archives, secrets or a private signing key to the update repository.

## Build and sign on the publisher workstation

Windows x64, Visual Studio C++ Build Tools/Windows SDK, Rust 1.98.0, NSIS and the non-exportable **isolmaSS Update Signing** RSA-3072 certificate in `Cert:\CurrentUser\My` are required. The pinned certificate thumbprint on the publisher workstation is `83E1B36A406235CC34E6B913D00BD3C04D235F83`. The signing script compares its public key with `resources/update-public-key.blob` before signing; the private key never leaves Windows certificate storage. Protect this account and machine: losing the key requires a manually installed trust-root replacement for existing users.

```powershell
$env:ISOLMASS_SIGNING_THUMBPRINT = '83E1B36A406235CC34E6B913D00BD3C04D235F83'
$env:ISOLMASS_BUILD_DIR = 'target\release-candidate' # Avoid a running target\release\isolmass.exe.
cmd /c release.bat
& "$env:ISOLMASS_BUILD_DIR\release\isolmass.exe" --verify-update "$env:ISOLMASS_BUILD_DIR\release\isolmass-setup.exe"
```

`release.bat` requires the signer and calls `package.bat`. Packaging builds a locked release, checks the executable (≤2.5 MiB) and installer (≤3 MiB) budgets, checks PE file versions, compiles NSIS, writes `isolmass-setup.exe.sha256`, creates the detached `isolmass-setup.exe.sig` and verifies that signature with the **shipped executable's pinned key**. A package without the publisher key is not eligible for auto-update; it will not overwrite an already signed installer in that output directory. CI has no signing key and does not publish releases.

For a manual portable download, archive the same release executable as `isolmass-portable-windows-x64.zip` with `LICENSE` and `THIRD_PARTY_NOTICES.md`. Portable mode does not modify an installed copy or auto-update it. The ZIP is a manual artifact, not the installer that the in-app updater trusts.

## Public release contract

Publish exactly one each of `isolmass-setup.exe`, `isolmass-setup.exe.sig` (raw 384-byte detached signature) and `isolmass-portable-windows-x64.zip` to the matching `vMAJOR.MINOR.PATCH` GitHub Release. Record the installer's lowercase SHA-256 in the release notes and repository release manifest. Keep the GitHub asset digest available; the app refuses a missing/invalid digest, duplicate asset, draft/prerelease, unexpected release URL, mismatched version or invalid signature. Release builds are immutable once published; a correction uses a **new version** rather than replacing a signed artifact under an existing tag.

The signature covers the UTF-8 bytes `isolmaSS-update-v1\n<VERSION>\n<INSTALLER-SHA256-LOWERCASE-HEX>\n`. `scripts/sign-update.ps1` implements the version-general form. HTTPS delivery and SHA-256 establish transport/integrity; only the pinned publisher signature establishes update authenticity. The app re-verifies the staged file immediately before launch.

An existing installation whose executable lacks this pinned-key verifier needs a **one-time manual installer** of a build with the pinned verifier. The installer is not paid-code-signed; Windows SmartScreen may display a reputation warning. Never automate bypassing or acceptance of that warning. Installation asks for consent in the app before download and launch; `--check-update` only checks availability.

## Installation and rollback

The NSIS installer installs per user at `%LOCALAPPDATA%\isolmaSS` without elevation. `isolmass.exe` runs as a background tray app; installer startup is not tied to double-clicking the EXE. Updates pass `/S /UPDATE /WAITPID=<pid>` after editing/settings and synchronous clipboard/save work finishes. The installer waits for the old process, stages the new executable, keeps `isolmass.previous.exe`, runs `--health-check` on the installed candidate, starts the tray and checks that the window is present. If activation fails, it restores the executable and display version, displays an error and retains the backup when restoration is prevented. Never delete a remaining `.previous.exe` without diagnosing the failed transaction.

The rollback covers application activation, **not** user screenshots or settings. Users should retain backups of personal data independently. Successful updates remove the rollback executable. Uninstall offers to keep personal configuration and screenshots.

## Cloudflare and site deployment

The source release tag contains `cloudflare/` (a Worker with SQLite Durable Object storage) and `site/` (static information/download pages). The Windows app uses the publisher's verified Public OAuth client with Workers Scripts Write and Memberships Read scopes, but exchanges the code on the user's computer and installs only into the account that the user authorizes and selects. GitHub login, R2/D1 activation, secret pasting and a custom domain are unnecessary. `release.bat` and the updater do **not** touch Cloudflare resources; each account owner explicitly authorizes installation. See the [sharing API and deployment guide](README.md#sharing-api-and-deployment). `ss.isolmaz.com` runs as the separate `isolmass-site` Worker and hosts no screenshots.

The owner reserved live cross-account authorization, Worker installation and screenshot upload acceptance testing. Local Worker HTTP exercise, Rust tests and a signed package are **not** proof that Cloudflare will accept a real third-party installation. Release notes must disclose this limit; publish a corrected new version instead of mutating signed assets if real-account testing finds a defect.

## Verification boundaries

```powershell
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked -- --test-threads=1 --skip test_tray_manager_lifecycle
```

The full interactive `--smoke-test` requires a disposable desktop because it captures the active screen and replaces the clipboard; it compiles a temporary installer and removes it afterward. `--verify-update PATH` is safe for a local installer and matching `.exe.sig` sidecar: it checks file version and the pinned signature without executing the installer. Test updater rollback with an isolated Windows profile or VM rather than overwriting a currently running user installation. The public release and source tag must have the same version; the release manifest records the source commit and artifact hashes.
