# Distribution and release

## Reproducible inputs

The application, manifest, Windows version resource and installer use the version from `Cargo.toml` (currently 0.3.0). Keep its `Cargo.lock` package entry synchronized. Rust 1.98.0 is pinned in `rust-toolchain.toml`; use the x64 MSVC target and the Windows SDK resource compiler. `RC` may point to a specific `rc.exe`.

`package.bat` builds with `--locked`, passes the Cargo version to NSIS, checks embedded versions, enforces an executable limit of 2.5 MiB and installer limit of 3 MiB, and writes `isolmass-setup.exe.sha256` after signing. Output lives in `target/release`. NSIS 3.10 is required. No credential or private signing material belongs in the repository.

The release profile uses `opt-level = 3`, LTO, one codegen unit and stripped symbols. Its measured p95 was lower than size optimization on the development machine; the complete comparison and limitations are in [ROADMAP.md](ROADMAP.md).

## Signing and update trust

For a production build, configure `SIGNTOOL_CERT_SHA1` to select an existing trusted code-signing certificate and put `signtool` on PATH, then run `release.bat`. This wrapper refuses to build a production release without the certificate. Packaging signs both executable and installer with SHA-256 and a timestamp, verifies their signatures, then runs the signed application's `--verify-update` against the installer to verify its publisher relationship. Public development packaging can remain unsigned, but cannot establish an update publisher identity by itself.

The updater accepts the verified public-key digest of its own signed executable, or rotation digests compiled into a previously trusted release via `ISOLMASS_UPDATE_PUBLIC_KEYS`. The latter is a semicolon-separated list of at most eight SHA-256 digests of the DER-encoded public-key information (not certificate thumbprints). Public-key digests are public; the private signing key must remain in the signing environment. Introduce a new key's digest in a release signed by the old trusted key before switching signers. GitHub metadata cannot add a trusted key.

Release assets must contain `isolmass-setup.exe`, a valid GitHub-provided SHA-256 digest, a strict three-part semantic tag (optional `v` prefix), and matching embedded file version. The release page must belong to `isolmaz/isolmaSS_V2`. Publish the checksum file alongside the installer for users performing manual downloads. Publishing, pushing, and creating a release were not performed by this change.

## Installer behavior

Installation is per-user in `%LOCALAPPDATA%\isolmaSS`, with HKCU registration and Start Menu shortcuts. It includes the MIT license and third-party notices. Uninstall preserves screenshots, configuration, logs and cached update downloads.

Automatic updates launch setup with `/S /UPDATE /WAITPID=<old-process-id>`. The presence of the valueless `/UPDATE` flag enables restart. Setup waits up to 30 seconds for the previous process; unknown process status and timeouts abort before modifying installed files. A manual setup/uninstall requests graceful exit through the tray window and also waits. The app defers that exit while editing/settings is open.

The executable is staged as `isolmass.new.exe`, the previous executable is kept as `isolmass.previous.exe`, and activation uses renames. File activation failures attempt to restore the previous executable. Support-file/uninstaller/registration failures are reported, and installation failure attempts executable restoration. This is an executable rollback, not a transaction over every shortcut and registry value: retry setup if registration was only partly written. Successful installation removes the executable backup. Uninstall aborts without deleting registration/shortcuts if the executable cannot be removed.

## Release gate

Before publishing: run the repository verification commands, build/sign the exact release artifacts, verify the same publisher and version, and exercise installation/update/uninstall on a disposable Windows VM. Include successful restart, valueless UPDATE, a process that takes too long to close, a locked executable, and a failed replacement. Also test the editor/settings during an update request.

These installer runtime scenarios and a real isolmaSS signed update have not yet been executed for 0.3.0. The local NSIS compiler check alone does not establish them. The latency target and full interactive DPI/UX validation are also open in [ROADMAP.md](ROADMAP.md).
