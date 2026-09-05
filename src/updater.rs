use serde::Deserialize;
use std::ffi::c_void;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{
    Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};
use windows::Win32::Foundation::{HANDLE, HWND};
use windows::Win32::Networking::WinHttp::WinHttpCloseHandle;
use windows::Win32::Security::WinTrust::{
    WINTRUST_ACTION_GENERIC_VERIFY_V2, WINTRUST_DATA, WINTRUST_DATA_0, WINTRUST_FILE_INFO,
    WTD_CHOICE_FILE, WTD_REVOCATION_CHECK_CHAIN_EXCLUDE_ROOT, WTD_REVOKE_WHOLECHAIN,
    WTD_STATEACTION_CLOSE, WTD_STATEACTION_VERIFY, WTD_UI_NONE, WinVerifyTrust,
};
use windows::Win32::Storage::FileSystem::{
    MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
use windows::core::{PCWSTR, w};

const RELEASE_API_HOST: &str = "api.github.com";
const RELEASE_API_PATH: &str = "/repos/isolmaz/isolmaSS_V2/releases/latest";
const INSTALLER_ASSET_NAME: &str = "isolmass-setup.exe";
const MAX_METADATA_BYTES: usize = 2 * 1024 * 1024;
const MAX_INSTALLER_BYTES: usize = 32 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateInfo {
    pub version: String,
    pub download_url: String,
    pub sha256: String,
    pub release_url: String,
}

#[derive(Deserialize)]
struct GithubRelease {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    draft: bool,
    assets: Vec<GithubAsset>,
}

#[derive(Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
    digest: Option<String>,
}

struct InternetHandle(*mut c_void);

impl Drop for InternetHandle {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                let _ = WinHttpCloseHandle(self.0);
            }
        }
    }
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}

fn parse_https_url(url: &str) -> Result<(&str, String), String> {
    let remainder = url
        .strip_prefix("https://")
        .ok_or_else(|| "Update downloads must use HTTPS.".to_string())?;
    let (host, path) = remainder
        .split_once('/')
        .map(|(host, path)| (host, format!("/{path}")))
        .unwrap_or((remainder, "/".to_string()));
    if !matches!(
        host,
        "api.github.com"
            | "github.com"
            | "release-assets.githubusercontent.com"
            | "objects.githubusercontent.com"
    ) || url.chars().any(|ch| ch.is_control() || ch == '\\')
        || url.contains('#')
    {
        return Err("The update URL contains an unsupported host.".to_string());
    }
    Ok((host, path))
}

fn http_get(host: &str, path: &str, maximum_size: usize) -> Result<Vec<u8>, String> {
    http_get_inner(
        host,
        path,
        maximum_size,
        Instant::now() + Duration::from_secs(45),
        0,
    )
}

fn check_cancelled(deadline: Instant) -> Result<(), String> {
    if CANCELLED.load(Ordering::Acquire) {
        Err("Update operation cancelled.".to_string())
    } else if Instant::now() >= deadline {
        Err("The update request exceeded its time limit.".to_string())
    } else {
        Ok(())
    }
}

fn http_get_inner(
    host: &str,
    path: &str,
    maximum_size: usize,
    deadline: Instant,
    redirects: u8,
) -> Result<Vec<u8>, String> {
    use windows::Win32::Networking::WinHttp::*;
    check_cancelled(deadline)?;
    if redirects > 5 {
        return Err("Too many update redirects.".to_string());
    }
    parse_https_url(&format!("https://{host}{path}"))?;
    let agent = wide(&format!("isolmaSS/{}", env!("CARGO_PKG_VERSION")));
    let session = InternetHandle(unsafe {
        WinHttpOpen(
            PCWSTR(agent.as_ptr()),
            WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
            PCWSTR::null(),
            PCWSTR::null(),
            0,
        )
    });
    if session.0.is_null() {
        return Err(format!(
            "WinHTTP could not start: {}",
            windows::core::Error::from_win32()
        ));
    }

    unsafe { WinHttpSetTimeouts(session.0, 5_000, 5_000, 5_000, 5_000) }
        .map_err(|error| format!("Could not set update timeouts: {error}"))?;
    let host_wide = wide(host);
    let connection = InternetHandle(unsafe {
        WinHttpConnect(
            session.0,
            PCWSTR(host_wide.as_ptr()),
            INTERNET_DEFAULT_HTTPS_PORT,
            0,
        )
    });
    if connection.0.is_null() {
        return Err(format!(
            "Could not connect to {host}: {}",
            windows::core::Error::from_win32()
        ));
    }

    let path_wide = wide(path);
    let request = InternetHandle(unsafe {
        WinHttpOpenRequest(
            connection.0,
            w!("GET"),
            PCWSTR(path_wide.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            std::ptr::null(),
            WINHTTP_FLAG_SECURE,
        )
    });
    if request.0.is_null() {
        return Err(format!(
            "Could not create the HTTPS request: {}",
            windows::core::Error::from_win32()
        ));
    }

    unsafe {
        WinHttpSetOption(
            Some(request.0),
            WINHTTP_OPTION_REDIRECT_POLICY,
            Some(&WINHTTP_OPTION_REDIRECT_POLICY_NEVER.to_ne_bytes()),
        )
    }
    .map_err(|error| format!("Could not enforce the HTTPS redirect policy: {error}"))?;
    let headers: Vec<u16> = concat!(
        "Accept: application/vnd.github+json\r\n",
        "User-Agent: isolmaSS\r\n",
        "X-GitHub-Api-Version: 2022-11-28\r\n"
    )
    .encode_utf16()
    .collect();
    unsafe {
        WinHttpSendRequest(request.0, Some(&headers), None, 0, 0, 0)
            .and_then(|_| WinHttpReceiveResponse(request.0, std::ptr::null_mut()))
    }
    .map_err(|error| format!("The update request failed: {error}"))?;

    let mut status = 0u32;
    let mut status_size = std::mem::size_of::<u32>() as u32;
    let mut index = 0u32;
    unsafe {
        WinHttpQueryHeaders(
            request.0,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            PCWSTR::null(),
            Some((&mut status as *mut u32).cast()),
            &mut status_size,
            &mut index,
        )
    }
    .map_err(|error| format!("Could not read the update response status: {error}"))?;
    check_cancelled(deadline)?;
    if matches!(status, 301 | 302 | 303 | 307 | 308) {
        let mut location = [0u16; 4096];
        let mut size = (location.len() * 2) as u32;
        unsafe {
            WinHttpQueryHeaders(
                request.0,
                WINHTTP_QUERY_LOCATION,
                PCWSTR::null(),
                Some(location.as_mut_ptr().cast()),
                &mut size,
                std::ptr::null_mut(),
            )
        }
        .map_err(|error| format!("The update redirect is invalid: {error}"))?;
        let length = location
            .iter()
            .position(|unit| *unit == 0)
            .unwrap_or(location.len());
        let location =
            String::from_utf16(&location[..length]).map_err(|_| "Invalid redirect encoding")?;
        let (host, path) = parse_https_url(&location)?;
        return http_get_inner(host, &path, maximum_size, deadline, redirects + 1);
    }
    if !(200..300).contains(&status) {
        return Err(format!("The update server returned HTTP {status}."));
    }

    let mut response = Vec::new();
    loop {
        check_cancelled(deadline)?;
        let mut available = 0u32;
        unsafe { WinHttpQueryDataAvailable(request.0, &mut available) }
            .map_err(|error| format!("Could not read update data: {error}"))?;
        if available == 0 {
            break;
        }
        let new_length = response
            .len()
            .checked_add(available as usize)
            .ok_or_else(|| "The update response is too large.".to_string())?;
        if new_length > maximum_size {
            return Err(format!(
                "The update response exceeded {maximum_size} bytes."
            ));
        }
        let offset = response.len();
        response.resize(new_length, 0);
        let mut read = 0u32;
        unsafe {
            WinHttpReadData(
                request.0,
                response[offset..].as_mut_ptr().cast(),
                available,
                &mut read,
            )
        }
        .map_err(|error| format!("Could not receive update data: {error}"))?;
        response.truncate(offset + read as usize);
        if read == 0 {
            break;
        }
    }
    Ok(response)
}

fn parse_version(value: &str) -> Option<[u64; 3]> {
    let core = value.strip_prefix('v').unwrap_or(value);
    if core.is_empty()
        || !core
            .bytes()
            .all(|byte| byte.is_ascii_digit() || byte == b'.')
        || core
            .split('.')
            .any(|part| part.len() > 1 && part.starts_with('0'))
    {
        return None;
    }
    let mut components = core.split('.');
    let parsed = [
        components.next()?.parse().ok()?,
        components.next()?.parse().ok()?,
        components.next()?.parse().ok()?,
    ];
    if components.next().is_some() {
        return None;
    }
    Some(parsed)
}

pub fn check_for_update() -> Result<Option<UpdateInfo>, String> {
    let bytes = http_get(RELEASE_API_HOST, RELEASE_API_PATH, MAX_METADATA_BYTES)?;
    let release: GithubRelease = serde_json::from_slice(&bytes)
        .map_err(|error| format!("The update metadata is invalid: {error}"))?;
    if release.draft || release.prerelease {
        return Ok(None);
    }

    let current = parse_version(env!("CARGO_PKG_VERSION"))
        .ok_or_else(|| "The installed application version is invalid.".to_string())?;
    let available = parse_version(&release.tag_name)
        .ok_or_else(|| "The release tag is not a semantic version.".to_string())?;
    if available <= current {
        return Ok(None);
    }

    let asset = release
        .assets
        .into_iter()
        .find(|asset| asset.name.eq_ignore_ascii_case(INSTALLER_ASSET_NAME))
        .ok_or_else(|| {
            format!(
                "Release {} has no {INSTALLER_ASSET_NAME} asset.",
                release.tag_name
            )
        })?;
    let digest = asset
        .digest
        .and_then(|digest| digest.strip_prefix("sha256:").map(str::to_owned))
        .filter(|digest| digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .ok_or_else(|| "The release installer has no valid SHA-256 digest.".to_string())?;

    parse_https_url(&asset.browser_download_url)?;
    if !release
        .html_url
        .starts_with("https://github.com/isolmaz/isolmaSS_V2/releases/")
    {
        return Err("Unexpected release information URL.".to_string());
    }
    Ok(Some(UpdateInfo {
        version: release.tag_name.trim_start_matches('v').to_string(),
        download_url: asset.browser_download_url,
        sha256: digest.to_ascii_lowercase(),
        release_url: release.html_url,
    }))
}

pub fn download_update(update: &UpdateInfo) -> Result<PathBuf, String> {
    if parse_version(&update.version).is_none() {
        return Err("Invalid update version.".to_string());
    }
    let (host, path) = parse_https_url(&update.download_url)?;
    let bytes = http_get(host, &path, MAX_INSTALLER_BYTES)?;
    let actual = sha256_hex(&bytes)?;
    if actual != update.sha256 {
        return Err(format!(
            "The installer digest does not match the GitHub release metadata (expected {}, got {actual}).",
            update.sha256
        ));
    }

    let root = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .ok_or_else(|| "%LOCALAPPDATA% is unavailable.".to_string())?
        .join("isolmaSS")
        .join("updates");
    std::fs::create_dir_all(&root)
        .map_err(|error| format!("Could not create the update folder: {error}"))?;
    let destination = root.join(format!("isolmass-setup-{}.exe", update.version));
    let temporary = root.join(format!(
        ".download-{}-{}-{}.tmp",
        std::process::id(),
        update.version,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_nanos()
    ));
    let result = (|| {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|error| error.to_string())?;
        file.write_all(&bytes)
            .and_then(|_| file.sync_all())
            .map_err(|error| format!("Could not write the update installer: {error}"))?;
        drop(file);
        check_cancelled(Instant::now() + Duration::from_secs(1))?;
        verify_authenticode(&temporary)?;
        if file_version(&temporary)? != update.version {
            return Err(
                "The installer file version does not match the announced release.".to_string(),
            );
        }
        use std::os::windows::ffi::OsStrExt;
        let source: Vec<u16> = temporary.as_os_str().encode_wide().chain(Some(0)).collect();
        let target: Vec<u16> = destination
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect();
        unsafe {
            MoveFileExW(
                PCWSTR(source.as_ptr()),
                PCWSTR(target.as_ptr()),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        }
        .map_err(|error| format!("Could not finalize the update installer: {error}"))?;
        Ok(destination)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}

fn trusted_signer_key(path: &Path) -> Result<String, String> {
    use std::os::windows::ffi::OsStrExt;
    let path_wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut file_info = WINTRUST_FILE_INFO {
        cbStruct: std::mem::size_of::<WINTRUST_FILE_INFO>() as u32,
        pcwszFilePath: PCWSTR(path_wide.as_ptr()),
        hFile: HANDLE::default(),
        pgKnownSubject: std::ptr::null_mut(),
    };
    let mut data = WINTRUST_DATA {
        cbStruct: std::mem::size_of::<WINTRUST_DATA>() as u32,
        dwUIChoice: WTD_UI_NONE,
        fdwRevocationChecks: WTD_REVOKE_WHOLECHAIN,
        dwUnionChoice: WTD_CHOICE_FILE,
        Anonymous: WINTRUST_DATA_0 {
            pFile: &mut file_info,
        },
        dwStateAction: WTD_STATEACTION_VERIFY,
        dwProvFlags: WTD_REVOCATION_CHECK_CHAIN_EXCLUDE_ROOT,
        ..Default::default()
    };
    let mut action = WINTRUST_ACTION_GENERIC_VERIFY_V2;
    let status = unsafe {
        WinVerifyTrust(
            HWND::default(),
            &mut action,
            (&mut data as *mut WINTRUST_DATA).cast(),
        )
    };
    let key = if status == 0 {
        unsafe { signer_public_key(&data) }
    } else {
        Err(format!(
            "Authenticode verification failed with status 0x{status:08X}."
        ))
    };
    data.dwStateAction = WTD_STATEACTION_CLOSE;
    unsafe {
        let _ = WinVerifyTrust(
            HWND::default(),
            &mut action,
            (&mut data as *mut WINTRUST_DATA).cast(),
        );
    }
    key
}

unsafe fn signer_public_key(data: &WINTRUST_DATA) -> Result<String, String> {
    use windows::Win32::Security::Cryptography::{
        CALG_SHA_256, CryptHashPublicKeyInfo, HCRYPTPROV_LEGACY, X509_ASN_ENCODING,
    };
    use windows::Win32::Security::WinTrust::{
        WTHelperGetProvSignerFromChain, WTHelperProvDataFromStateData,
    };
    let provider = unsafe { WTHelperProvDataFromStateData(data.hWVTStateData) };
    if provider.is_null() {
        return Err("No verified signature provider.".to_string());
    }
    let signer = unsafe { WTHelperGetProvSignerFromChain(provider, 0, false, 0) };
    if signer.is_null() || unsafe { (*signer).csCertChain == 0 || (*signer).pasCertChain.is_null() }
    {
        return Err("No verified publisher certificate.".to_string());
    }
    let certificate = unsafe { (*(*signer).pasCertChain).pCert };
    if certificate.is_null() || unsafe { (*certificate).pCertInfo.is_null() } {
        return Err("The publisher certificate has no public key.".to_string());
    }
    let mut digest = [0u8; 32];
    let mut size = 32u32;
    unsafe {
        CryptHashPublicKeyInfo(
            HCRYPTPROV_LEGACY::default(),
            CALG_SHA_256,
            0,
            X509_ASN_ENCODING,
            &(*(*certificate).pCertInfo).SubjectPublicKeyInfo,
            Some(digest.as_mut_ptr()),
            &mut size,
        )
    }
    .map_err(|error| format!("Publisher public key could not be verified: {error}"))?;
    if size != 32 {
        return Err("Unexpected publisher digest length.".to_string());
    }
    Ok(hex(&digest))
}

/// Read the version embedded in an executable, independent of its filename or timestamp.
pub fn file_version(path: &Path) -> Result<String, String> {
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::Storage::FileSystem::{
        GetFileVersionInfoSizeW, GetFileVersionInfoW, VS_FIXEDFILEINFO, VerQueryValueW,
    };
    let path: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let size = unsafe { GetFileVersionInfoSizeW(PCWSTR(path.as_ptr()), None) };
    if size == 0 || size > 1024 * 1024 {
        return Err("The executable has no valid version resource.".to_string());
    }
    // u32 alignment is sufficient for VS_FIXEDFILEINFO and the Win32 version block.
    let mut data = vec![0u32; (size as usize).div_ceil(4)];
    unsafe { GetFileVersionInfoW(PCWSTR(path.as_ptr()), 0, size, data.as_mut_ptr().cast()) }
        .map_err(|error| error.to_string())?;
    let mut pointer = std::ptr::null_mut();
    let mut length = 0;
    if !unsafe {
        VerQueryValueW(
            data.as_ptr().cast(),
            windows::core::w!("\\"),
            &mut pointer,
            &mut length,
        )
        .as_bool()
    } || pointer.is_null()
        || length < std::mem::size_of::<VS_FIXEDFILEINFO>() as u32
    {
        return Err("The executable has an invalid version resource.".to_string());
    }
    let version = unsafe { std::ptr::read_unaligned(pointer.cast::<VS_FIXEDFILEINFO>()) };
    if version.dwSignature != 0xfeef04bd {
        return Err("The executable version signature is invalid.".to_string());
    }
    Ok(format!(
        "{}.{}.{}",
        version.dwFileVersionMS >> 16,
        version.dwFileVersionMS & 0xffff,
        version.dwFileVersionLS >> 16
    ))
}

pub fn verify_authenticode(path: &Path) -> Result<(), String> {
    let actual = trusted_signer_key(path)?;
    // Trust the publisher of this verified running executable. Rotation keys must
    // ship in an already trusted release, never in unsigned network metadata.
    let configured = option_env!("ISOLMASS_UPDATE_PUBLIC_KEYS").unwrap_or("");
    let own = std::env::current_exe()
        .map_err(|error| error.to_string())
        .and_then(|path| trusted_signer_key(&path));
    if own.as_ref().is_ok_and(|key| key == &actual)
        || configured
            .split(';')
            .any(|key| key.len() == 64 && key.eq_ignore_ascii_case(&actual))
    {
        return Ok(());
    }
    Err("The installer is not signed by the expected isolmaSS publisher. Unsigned development builds cannot establish publisher identity.".to_string())
}

pub fn launch_installer(path: &Path) -> Result<(), String> {
    use std::os::windows::fs::OpenOptionsExt;
    let _locked_file = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(path)
        .map_err(|error| error.to_string())?;
    verify_authenticode(path)?;
    let parameters = wide(&format!("/S /UPDATE /WAITPID={}", std::process::id()));
    use std::os::windows::ffi::OsStrExt;
    let path_wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let result = unsafe {
        ShellExecuteW(
            HWND::default(),
            w!("open"),
            PCWSTR(path_wide.as_ptr()),
            PCWSTR(parameters.as_ptr()),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    if result.0 as isize <= 32 {
        Err(format!(
            "Windows could not launch the installer (code {}).",
            result.0 as isize
        ))
    } else {
        Ok(())
    }
}

mod job;
use job::CANCELLED;
pub use job::{configure, poll, run_automatic_update_check, run_manual_update_check, shutdown};

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn sha256_hex(input: &[u8]) -> Result<String, String> {
    use windows::Win32::Security::Cryptography::{BCRYPT_SHA256_ALG_HANDLE, BCryptHash};
    let mut digest = [0u8; 32];
    unsafe { BCryptHash(BCRYPT_SHA256_ALG_HANDLE, None, input, &mut digest) }
        .ok()
        .map_err(|error| format!("Windows SHA-256 failed: {error}"))?;
    Ok(hex(&digest))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semantic_version_comparison_and_sha256_are_stable() {
        assert_eq!(parse_version("v1.2.3"), Some([1, 2, 3]));
        assert!(parse_version("1.2").is_none());
        for value in [
            "1.2.3/evil",
            "1.2.3-beta",
            "1.2.3.4",
            "01.2.3",
            "1..3",
            "../../x",
        ] {
            assert!(parse_version(value).is_none());
        }
        for url in [
            "http://github.com/x",
            "https://github.com.evil.test/x",
            "https://github.com@evil.test/x",
            "https://github.com/x#fragment",
            "https://evil.test/x",
        ] {
            assert!(parse_https_url(url).is_err(), "{url}");
        }
        assert!(
            parse_https_url(
                "https://github.com/isolmaz/isolmaSS_V2/releases/download/v0.3.0/isolmass-setup.exe"
            )
            .is_ok()
        );
        for key in [0x56, 0x4d, 0x52, 0x54, 0x42] {
            assert!(crate::overlay::is_editor_shortcut(key, 0, false));
        }
        for key in [0x43, 0x53, 0x5a, 0x59] {
            assert!(crate::overlay::is_editor_shortcut(
                key,
                windows::Win32::UI::Input::KeyboardAndMouse::MOD_CONTROL.0,
                true
            ));
        }
        assert!(!crate::overlay::is_editor_shortcut(0x41, 0, true));

        assert_eq!(
            sha256_hex(b"abc").expect("SHA-256"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
