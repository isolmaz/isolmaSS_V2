# Security model

isolmaSS keeps captures local **unless you explicitly configure your own Cloudflare installation and choose Upload**. Copy/Save never upload. Upload sends the flattened selection over HTTPS to your Worker, then copies its link only after the Worker confirms success. **Karart** writes opaque dark pixels after all other annotations. Blur and highlighter preserve underlying visual information and must not be used to hide secrets. Verify the flattened PNG/JPEG before uploading sensitive material.

## Optional Cloudflare share threat model

Every user installs their own private R2 bucket, D1 records and Worker; `ss.isolmaz.com` is an information/download site, not a central image host. The Worker requires separate 256-bit upload and administrator bearer secrets. The app stores the raw values with per-user Windows DPAPI outside `settings.json`; Cloudflare keeps them as Worker secrets. Never disclose either token, place them in URL parameters, or publish local credential files. An attacker who gains the upload token can consume your storage/quota; the administrator token also controls settings and deletion. Rotate secrets and re-pair the client after a suspected leak.

Each public link contains an independent 192-bit random ID. There is no public listing or sequential ID space, the R2 bucket has no public route and GET checks D1 state before object access. A **valid link remains a bearer capability**: anyone who sees it can view an unprotected image and retain a local copy. Optional image passwords are transported over HTTPS, stored as per-image salted PBKDF2 verifiers, and never placed in URLs; repeated wrong guesses are limited. The same default password reused on many images links their access—use a strong, unique secret when privacy matters. No web service can guarantee absolute security against link disclosure, endpoint compromise or previously downloaded copies.

The API accepts PNG/JPEG with bounded streaming request size and basic image header/dimension checks, never SVG/HTML. It sends fixed image MIME, `nosniff`, CSP and `no-store` headers. It does not perform a full antivirus scan or decode/re-encode untrusted JPEG; grant upload credentials only to your own devices. Deletes first hide a D1 record, then remove R2 bytes with retry via Cron. A response already sent before deletion or stored by a viewer cannot be revoked. The 90% warning/stop options count app traffic, not Cloudflare account-wide usage, and blocked requests may still consume Worker resources; they do not guarantee a zero bill. Cloudflare logs/traces and account-level access records remain under the installation owner's Cloudflare settings.


## Signed updates

The installed application checks `https://api.github.com/repos/isolmaz/isolmaSS-updates/releases/latest`. It accepts only a newer, non-draft, non-prerelease numeric three-part version with one `isolmass-setup.exe` and one `isolmass-setup.exe.sig`. Download URLs require HTTPS and an allowed GitHub asset host. Redirects remain HTTPS and bounded; metadata and installer downloads have size limits.

GitHub's SHA-256 asset digest is compared with the downloaded executable. The detached 384-byte RSA-3072/PKCS#1 v1.5 SHA-256 signature authenticates the exact UTF-8 message `isolmaSS-update-v1\n<VERSION>\n<SHA256-LOWERCASE-HEX>\n`. Windows BCrypt verifies it against the embedded publisher public key. The pinned key is `resources/update-public-key.blob` (SHA-256 `67906f6348c629e2c34c1cd849b742249bc32ebc104275a990bae5a7ddc2f20e`). The executable's file version must equal the signed release version. Immediately before launching a staged installer, the app checks its canonical staging path, digest, detached signature and file version again.

The publisher's private key is **non-exportable** in the release workstation's `Cert:\CurrentUser\My` Windows certificate store. It is never stored in this repository, on CI or in the public release repository. `scripts/sign-update.ps1` refuses any certificate whose public key differs from the pinned key. If that workstation/key is lost, future releases cannot be installed automatically by apps pinned to this key; recovery requires a new manually installed trust root. Keep the signing account and workstation protected.

The signature is application-level and does **not** establish Authenticode trust in Windows. SmartScreen may warn on a downloaded installer or portable executable. Do not disable SmartScreen or automatically dismiss its warning. An installed build without this public-key verifier needs a one-time manual installation of the signed-update-capable version.

## Consent and rollback

Checking for updates never authorizes installation. The native prompt offers **Yükle / Daha sonra / Bu sürümü atla**. A version skipped during automatic checks remains available through a manual check. Download and installer launch happen only after explicit installation consent. The consent prompt warns that the app will close for installation and start again; manual checks expose progress and results in Settings or the tray. The app waits for active editing, settings and synchronous copy/save work to finish before handing control to the installer.

The installer stages the new executable beside the installation, retains the rollback executable, waits for the old process, and runs `--health-check` against the installed new executable. That check exercises settings, tray and shortcut startup. The installer confirms the restarted tray is present before deleting the rollback executable. On failure it attempts to restore the executable and display version and surfaces an error. If a process or permission prevents restoration, it leaves the backup on disk and reports its path. The rollback protects executable activation; it is not a backup of screenshots or personal configuration.

## Input, persistence and diagnostics

Configuration is validated when read: path length, color alpha, 1–64 px width, supported shortcut, save format/quality, delay, last tool and skipped release version. Older JSON with omitted fields receives defaults. The settings file is bounded to 64 KiB and saved using a cross-process lock, staged file and atomic rename; malformed content is backed up before defaults are used. Save output uses temporary files and atomic replacement where supported. Resource handles and clipboard ownership are released on failure paths.

Diagnostic logs are local, rotate near 1 MiB, and omit screenshots, annotations, recorded keys and downloaded private credentials. File paths and operating-system error text may be present; review logs before sharing. The source repository and public update repository have different scopes: the source is public for self-host installation, while only release metadata and downloadable binaries belong in the updates repository. Never commit Worker secrets, local DPAPI files or the signing private key.

For a suspected vulnerability, report it privately to the repository owner; do not put secrets or private captures in a public issue.
