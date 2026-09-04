use serde::Deserialize;
use std::ffi::c_void;
use std::path::{Path, PathBuf};
use windows::Win32::Foundation::{HANDLE, HWND};
use windows::Win32::Networking::WinHttp::{
    INTERNET_DEFAULT_HTTPS_PORT, WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, WINHTTP_FLAG_SECURE,
    WINHTTP_QUERY_FLAG_NUMBER, WINHTTP_QUERY_STATUS_CODE, WinHttpCloseHandle, WinHttpConnect,
    WinHttpOpen, WinHttpOpenRequest, WinHttpQueryDataAvailable, WinHttpQueryHeaders,
    WinHttpReadData, WinHttpReceiveResponse, WinHttpSendRequest,
};
use windows::Win32::Security::WinTrust::{
    WINTRUST_ACTION_GENERIC_VERIFY_V2, WINTRUST_DATA, WINTRUST_DATA_0, WINTRUST_FILE_INFO,
    WTD_CHOICE_FILE, WTD_REVOCATION_CHECK_CHAIN_EXCLUDE_ROOT, WTD_REVOKE_WHOLECHAIN,
    WTD_STATEACTION_CLOSE, WTD_STATEACTION_VERIFY, WTD_UI_NONE, WinVerifyTrust,
};
use windows::Win32::Storage::FileSystem::{
    MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::{
    IDYES, MB_ICONINFORMATION, MB_YESNO, MessageBoxW, SW_SHOWNORMAL,
};
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
    if host.is_empty() || host.contains('@') || host.contains(':') {
        return Err("The update URL contains an unsupported host.".to_string());
    }
    Ok((host, path))
}

fn http_get(host: &str, path: &str, maximum_size: usize) -> Result<Vec<u8>, String> {
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
    if !(200..300).contains(&status) {
        return Err(format!("The update server returned HTTP {status}."));
    }

    let mut response = Vec::new();
    loop {
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
    let core = value.trim().trim_start_matches('v').split('-').next()?;
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

    Ok(Some(UpdateInfo {
        version: release.tag_name.trim_start_matches('v').to_string(),
        download_url: asset.browser_download_url,
        sha256: digest.to_ascii_lowercase(),
        release_url: release.html_url,
    }))
}

pub fn download_update(update: &UpdateInfo) -> Result<PathBuf, String> {
    let (host, path) = parse_https_url(&update.download_url)?;
    let bytes = http_get(host, &path, MAX_INSTALLER_BYTES)?;
    let actual = sha256_hex(&bytes);
    if actual != update.sha256 {
        return Err(format!(
            "The installer digest does not match the signed release metadata (expected {}, got {actual}).",
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
        ".download-{}-{}.tmp",
        std::process::id(),
        update.version
    ));
    let result = (|| {
        std::fs::write(&temporary, bytes)
            .map_err(|error| format!("Could not write the update installer: {error}"))?;
        verify_authenticode(&temporary)?;
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

pub fn verify_authenticode(path: &Path) -> Result<(), String> {
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
    data.dwStateAction = WTD_STATEACTION_CLOSE;
    unsafe {
        let _ = WinVerifyTrust(
            HWND::default(),
            &mut action,
            (&mut data as *mut WINTRUST_DATA).cast(),
        );
    }
    if status == 0 {
        Ok(())
    } else {
        Err(format!(
            "Authenticode verification failed with status 0x{status:08X}."
        ))
    }
}

pub fn launch_installer(path: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    let path_wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let result = unsafe {
        ShellExecuteW(
            HWND::default(),
            w!("open"),
            PCWSTR(path_wide.as_ptr()),
            w!("/S /UPDATE"),
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

#[derive(Clone, Copy)]
enum UpdateCheckInvocation {
    Manual,
    Background,
}

pub fn run_manual_update_check() {
    run_update_check(UpdateCheckInvocation::Manual, false);
}

pub fn run_automatic_update_check(auto_install: bool) {
    run_update_check(UpdateCheckInvocation::Background, auto_install);
}

fn run_update_check(invocation: UpdateCheckInvocation, auto_install: bool) {
    std::thread::spawn(move || match check_for_update() {
        Ok(None) => {
            if matches!(invocation, UpdateCheckInvocation::Manual) {
                crate::tray::show_notification(
                    "isolmaSS is up to date",
                    &format!(
                        "Version {} is the latest version.",
                        env!("CARGO_PKG_VERSION")
                    ),
                );
            }
        }
        Ok(Some(update)) => {
            let should_install = match invocation {
                UpdateCheckInvocation::Background if auto_install => true,
                UpdateCheckInvocation::Background => {
                    crate::tray::show_notification(
                        "Update available",
                        &format!("isolmaSS {} is available.", update.version),
                    );
                    return;
                }
                UpdateCheckInvocation::Manual => {
                    let title: Vec<u16> = "isolmaSS update available\0".encode_utf16().collect();
                    let message: Vec<u16> = format!(
                        "Version {} is available. Download, verify, and install it now?\0",
                        update.version
                    )
                    .encode_utf16()
                    .collect();
                    (unsafe {
                        MessageBoxW(
                            HWND::default(),
                            PCWSTR(message.as_ptr()),
                            PCWSTR(title.as_ptr()),
                            MB_YESNO | MB_ICONINFORMATION,
                        )
                    }) == IDYES
                }
            };
            if !should_install {
                crate::tray::show_notification(
                    "Update available",
                    &format!("isolmaSS {} is available.", update.version),
                );
                return;
            }

            crate::tray::show_notification(
                "Downloading update",
                &format!("Downloading isolmaSS {} securely...", update.version),
            );
            match download_update(&update).and_then(|path| launch_installer(&path)) {
                Ok(()) => {
                    crate::tray::show_notification(
                        "Installing update",
                        "isolmaSS will restart after the verified update is installed.",
                    );
                    crate::tray::request_exit();
                }
                Err(error) => crate::tray::show_notification("Update failed", &error),
            }
        }
        Err(error) => {
            if matches!(invocation, UpdateCheckInvocation::Manual) {
                crate::tray::show_notification("Update check failed", &error);
            }
        }
    });
}

fn sha256_hex(input: &[u8]) -> String {
    const INITIAL: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];

    let bit_length = (input.len() as u64).wrapping_mul(8);
    let padded_length = (input.len() + 9).div_ceil(64) * 64;
    let mut padded = Vec::with_capacity(padded_length);
    padded.extend_from_slice(input);
    padded.push(0x80);
    padded.resize(padded_length - 8, 0);
    padded.extend_from_slice(&bit_length.to_be_bytes());

    let mut state = INITIAL;
    let mut schedule = [0u32; 64];
    for block in padded.as_chunks::<64>().0 {
        for (index, word) in block.as_chunks::<4>().0.iter().enumerate() {
            schedule[index] = u32::from_be_bytes(*word);
        }
        for index in 16..64 {
            let s0 = schedule[index - 15].rotate_right(7)
                ^ schedule[index - 15].rotate_right(18)
                ^ (schedule[index - 15] >> 3);
            let s1 = schedule[index - 2].rotate_right(17)
                ^ schedule[index - 2].rotate_right(19)
                ^ (schedule[index - 2] >> 10);
            schedule[index] = schedule[index - 16]
                .wrapping_add(s0)
                .wrapping_add(schedule[index - 7])
                .wrapping_add(s1);
        }

        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = state;
        for index in 0..64 {
            let sigma1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let choose = (e & f) ^ (!e & g);
            let temp1 = h
                .wrapping_add(sigma1)
                .wrapping_add(choose)
                .wrapping_add(K[index])
                .wrapping_add(schedule[index]);
            let sigma0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let majority = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = sigma0.wrapping_add(majority);
            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }
        for (slot, value) in state.iter_mut().zip([a, b, c, d, e, f, g, h]) {
            *slot = slot.wrapping_add(value);
        }
    }

    let mut output = String::with_capacity(64);
    for word in state {
        use std::fmt::Write;
        let _ = write!(output, "{word:08x}");
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semantic_version_comparison_and_sha256_are_stable() {
        assert_eq!(parse_version("v1.2.3"), Some([1, 2, 3]));
        assert!(parse_version("1.2").is_none());
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
