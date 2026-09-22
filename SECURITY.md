# Security model

Screenshots and annotations are processed locally. There is no screenshot upload or telemetry feature. The optional update checker contacts GitHub; automatic installation is a separate preference. Diagnostics are bounded and omit image contents and annotation text, although filesystem errors can reveal local paths to someone who can read the user's log directory.

## Update boundary

- Only HTTPS on `api.github.com`, `github.com`, `release-assets.githubusercontent.com` and `objects.githubusercontent.com` is accepted. Redirects are validated at every hop and limited to five; fragments, control characters and backslashes are rejected.
- Metadata and installer responses are capped at 2 MiB and 32 MiB. WinHTTP operations have explicit five-second timeouts, with a 45-second deadline checked between calls. This is not a hard wall-clock bound over Windows signature/revocation checks.
- A single tracked worker performs update work. Cancellation is tied to configuration changes and application shutdown; the cancellation flag is set under the same lock that publishes worker results, so a concurrent cancel either discards a pending result or clears it afterwards. Failed automatic download attempts are reported through diagnostics and a tray notification rather than a modal error. Download names are unique and created exclusively; completed files are published atomically.
- The download's SHA-256 must match GitHub metadata. Metadata itself is not independently signed by this application.
- Windows Authenticode trust must succeed, and the verified signer must match the application's trusted publisher key or a compiled rotation key. The embedded file version must match the release version. A valid signature from an unrelated publisher is insufficient.
- The installer is verified again immediately before execution while a file handle prevents concurrent writes/deletion. Installation waits for editing/settings to finish.

The Windows trust verification follows [WinVerifyTrust](https://learn.microsoft.com/en-us/windows/win32/api/wintrust/nf-wintrust-winverifytrust). The public-key digest follows [CryptHashPublicKeyInfo](https://learn.microsoft.com/en-us/windows/win32/api/wincrypt/nf-wincrypt-crypthashpublickeyinfo); payload hashing uses Windows CNG. Certificate revocation and network policy may make verification fail when offline. See [DISTRIBUTION.md](DISTRIBUTION.md) for key rotation.

An unsigned development build rejects an unrelated, otherwise trusted signed binary because it has no trusted isolmaSS publisher identity. This negative case was exercised with Microsoft's embedded-signed Edge executable. A successful signed isolmaSS-to-isolmaSS update requires the real publisher certificate and remains a release verification task.

## Local data boundaries

Settings loading fails closed. A missing file yields defaults; invalid JSON is copied to `settings.corrupt.json` under the cross-process write lock before any reset, and a failed backup aborts recovery with the error propagated to the caller; every other read error (sharing violation, permission, antivirus lock) propagates instead of silently substituting defaults. Settings have a 64 KiB size limit and validated numeric/path/hotkey fields — the save folder must be an absolute, NUL-free path — atomic writes, and a cross-process write lock. Saving skips an unreachable save folder, such as an unavailable network share, rather than failing the write.

Screenshot export is opaque and file replacement occurs only after encoding completes. Clipboard text reads are bounded to 32 KiB: a failed size query is an error, and truncation is recorded only when text was actually cut. Clipboard allocations are released on failure; Windows receives ownership only after a successful transfer. Diagnostic-log rotation renames the active file at about 1 MiB; if that rename fails, new records are appended rather than dropped.

Use opaque **Redact** to cover sensitive pixels. Blur/pixelation is an appearance effect, not secure redaction. The exported image excludes editor controls and transient previews.

Dependency versions remain locked; no new external package was introduced in 0.3.0. On 2026-09-21, all **27 locked registry package/version entries (26 package names)** were compared against the **1,238 crate advisories** in the official [RustSec database snapshot](https://github.com/RustSec/advisory-db/tree/57ad4063bb49c1deb04b6fcee30cfbac6b508474), fetched that day. The only matching package advisory was [RUSTSEC-2022-0008](https://rustsec.org/advisories/RUSTSEC-2022-0008.html); locked windows 0.58.0 is within its patched range, >=0.32.0. No affected locked package was found in that snapshot.

This check parsed the database's TOML metadata and directly evaluated its sole matching version range; the cargo-audit/cargo-deny executables were not installed or run. It covers published RustSec crate records, not undisclosed vulnerabilities, malicious packages, every source of security advisories, NSIS or the Windows/SDK/compiler supply chain. Repeat the advisory comparison before a release; this is not a claim that the dependency graph is vulnerability-free.

## Reporting

Report reproducible security issues to the repository maintainer through GitHub's private vulnerability-reporting feature if it is enabled. Otherwise request a private contact without posting exploit details, screenshots, personal data, secrets, or signing material publicly. No dedicated security contact address or response SLA has been established in this repository.
