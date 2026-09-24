//! Per-installation Cloudflare connection. Upload credentials never enter settings.json.
use serde::{Deserialize, Serialize};
use std::ffi::c_void;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use windows::Win32::Foundation::{HLOCAL, LocalFree};
use windows::Win32::Networking::WinHttp::*;
use windows::Win32::Security::Cryptography::{
    BCRYPT_ALG_HANDLE, BCRYPT_USE_SYSTEM_PREFERRED_RNG, BCryptGenRandom, CRYPT_INTEGER_BLOB,
    CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData, CryptUnprotectData,
};
use windows::Win32::Storage::FileSystem::{
    MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
};
use windows::core::{PCWSTR, w};

const MAX_JSON_BYTES: usize = 128 * 1024;
const CREDENTIAL_FILE: &str = "cloud-credentials.bin";

#[derive(Clone, Serialize, Deserialize)]
pub struct CloudCredentials {
    pub origin: String,
    pub upload_token: String,
    pub admin_token: String,
    pub share_password: Option<String>,
}

pub fn valid_cloud_origin(origin: &str) -> bool {
    let Some(host) = origin.strip_prefix("https://") else {
        return false;
    };
    !host.is_empty()
        && host.len() <= 253
        && host.contains('.')
        && host.is_ascii()
        && host
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-'))
        && host.split('.').all(|part| {
            !part.is_empty() && part.len() <= 63 && !part.starts_with('-') && !part.ends_with('-')
        })
        && host == host.to_ascii_lowercase()
}

pub fn generate_token() -> Result<String, String> {
    let mut random = [0u8; 32];
    unsafe {
        BCryptGenRandom(
            BCRYPT_ALG_HANDLE::default(),
            &mut random,
            BCRYPT_USE_SYSTEM_PREFERRED_RNG,
        )
    }
    .ok()
    .map_err(|error| format!("Windows could not create a secure token: {error}"))?;
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut token = String::with_capacity(43);
    for part in random.chunks(3) {
        let word = ((part[0] as usize) << 16)
            | ((part.get(1).copied().unwrap_or(0) as usize) << 8)
            | part.get(2).copied().unwrap_or(0) as usize;
        token.push(ALPHABET[(word >> 18) & 63] as char);
        token.push(ALPHABET[(word >> 12) & 63] as char);
        if part.len() > 1 {
            token.push(ALPHABET[(word >> 6) & 63] as char);
        }
        if part.len() > 2 {
            token.push(ALPHABET[word & 63] as char);
        }
    }
    Ok(token)
}

fn credential_path() -> Result<PathBuf, String> {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .map(|root| root.join("isolmaSS").join(CREDENTIAL_FILE))
        .ok_or_else(|| {
            "%LOCALAPPDATA% is unavailable; upload credentials cannot be protected.".to_string()
        })
}

struct ProtectedBlob(CRYPT_INTEGER_BLOB);
impl Drop for ProtectedBlob {
    fn drop(&mut self) {
        if !self.0.pbData.is_null() {
            unsafe {
                let _ = LocalFree(HLOCAL(self.0.pbData.cast()));
            }
        }
    }
}

fn protect(plain: &[u8]) -> Result<Vec<u8>, String> {
    let input = CRYPT_INTEGER_BLOB {
        cbData: plain.len() as u32,
        pbData: plain.as_ptr().cast_mut(),
    };
    let mut result = ProtectedBlob(CRYPT_INTEGER_BLOB::default());
    unsafe {
        CryptProtectData(
            &input,
            w!("isolmaSS Cloudflare credentials"),
            None,
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut result.0,
        )
    }
    .map_err(|error| format!("Windows could not protect Cloudflare credentials: {error}"))?;
    let len = result.0.cbData as usize;
    if result.0.pbData.is_null() || len > 65536 {
        return Err("Windows returned invalid protected credentials.".to_string());
    }
    Ok(unsafe { std::slice::from_raw_parts(result.0.pbData, len) }.to_vec())
}
fn unprotect(ciphertext: &[u8]) -> Result<Vec<u8>, String> {
    let input = CRYPT_INTEGER_BLOB {
        cbData: ciphertext.len() as u32,
        pbData: ciphertext.as_ptr().cast_mut(),
    };
    let mut result = ProtectedBlob(CRYPT_INTEGER_BLOB::default());
    unsafe {
        CryptUnprotectData(
            &input,
            None,
            None,
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut result.0,
        )
    }
    .map_err(|error| {
        format!("Windows could not unlock Cloudflare credentials for this user: {error}")
    })?;
    let len = result.0.cbData as usize;
    if result.0.pbData.is_null() || len > 65536 {
        return Err("Protected Cloudflare credentials are invalid.".to_string());
    }
    Ok(unsafe { std::slice::from_raw_parts(result.0.pbData, len) }.to_vec())
}

pub fn save_credentials(credentials: &CloudCredentials) -> Result<(), String> {
    if !valid_cloud_origin(&credentials.origin)
        || credentials.upload_token.len() != 43
        || credentials.admin_token.len() != 43
        || credentials.upload_token == credentials.admin_token
        || !credentials
            .upload_token
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        || !credentials
            .admin_token
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        || credentials.share_password.as_ref().is_some_and(|password| {
            password.len() < 12 || password.len() > 128 || password.chars().any(char::is_control)
        })
    {
        return Err("Cloudflare address, tokens or image password are invalid.".to_string());
    }
    let plain = serde_json::to_vec(credentials).map_err(|error| error.to_string())?;
    let ciphertext = protect(&plain)?;
    let path = credential_path()?;
    let parent = path
        .parent()
        .ok_or_else(|| "Credential path has no folder.".to_string())?;
    std::fs::create_dir_all(parent)
        .map_err(|error| format!("Could not create credential folder: {error}"))?;
    let temporary = parent.join(format!(
        ".cloud-{}-{}.tmp",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_nanos()
    ));
    let result = (|| -> Result<(), String> {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|error| format!("Could not stage protected credentials: {error}"))?;
        file.write_all(&ciphertext)
            .and_then(|()| file.sync_all())
            .map_err(|error| format!("Could not persist protected credentials: {error}"))?;
        drop(file);
        use std::os::windows::ffi::OsStrExt;
        let from: Vec<u16> = temporary.as_os_str().encode_wide().chain(Some(0)).collect();
        let to: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        unsafe {
            MoveFileExW(
                PCWSTR(from.as_ptr()),
                PCWSTR(to.as_ptr()),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        }
        .map_err(|error| format!("Could not activate Cloudflare credentials: {error}"))
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(temporary);
    }
    result
}

pub fn load_credentials(origin: &str) -> Result<CloudCredentials, String> {
    let path = credential_path()?;
    let bytes =
        std::fs::read(path).map_err(|error| format!("Cloudflare setup is incomplete: {error}"))?;
    if bytes.len() > 65536 || bytes.is_empty() {
        return Err("Protected credentials have an invalid size.".to_string());
    }
    let plain = unprotect(&bytes)?;
    let data: CloudCredentials = serde_json::from_slice(&plain)
        .map_err(|_| "Protected credentials are corrupt.".to_string())?;
    if data.origin != origin {
        return Err(
            "Cloudflare address changed. Pair this installation again before uploading."
                .to_string(),
        );
    }
    Ok(data)
}

pub fn forget_credentials() -> Result<(), String> {
    match std::fs::remove_file(credential_path()?) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("Could not remove protected credentials: {error}")),
    }
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

pub enum RequestBody<'a> {
    Bytes(&'a [u8]),
    File(&'a Path),
}

/// Performs one HTTPS request to exactly the configured Worker origin; redirects
/// and credential forwarding to other hosts are forbidden.
pub fn api_request(
    origin: &str,
    path: &str,
    method: &str,
    bearer: &str,
    content_type: Option<&str>,
    password: Option<&str>,
    body: RequestBody<'_>,
) -> Result<(u32, Vec<u8>), String> {
    if !valid_cloud_origin(origin)
        || !path.starts_with("/api/")
        || path.contains("..")
        || path.contains(['#', '\\', '\r', '\n'])
        || !matches!(method, "GET" | "POST" | "PUT" | "DELETE")
        || bearer.len() != 43
        || !bearer
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
    {
        return Err("Cloudflare request contains an invalid address or credential.".to_string());
    }
    let host = &origin[8..];
    let mut file = match body {
        RequestBody::File(path) => Some(
            std::fs::File::open(path)
                .map_err(|error| format!("Screenshot file is unavailable: {error}"))?,
        ),
        RequestBody::Bytes(_) => None,
    };
    let size = match body {
        RequestBody::File(path) => std::fs::metadata(path)
            .map_err(|error| format!("Screenshot size is unknown: {error}"))?
            .len(),
        RequestBody::Bytes(bytes) => bytes.len() as u64,
    };
    if size > 104857600 {
        return Err("Screenshot exceeds the maximum server upload size.".to_string());
    }
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
    unsafe { WinHttpSetTimeouts(session.0, 5_000, 5_000, 15_000, 20_000) }
        .map_err(|error| format!("Could not set Cloudflare request timeouts: {error}"))?;
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
            "Cloudflare host is unavailable: {}",
            windows::core::Error::from_win32()
        ));
    }
    let path_wide = wide(path);
    let method_wide = wide(method);
    let request = InternetHandle(unsafe {
        WinHttpOpenRequest(
            connection.0,
            PCWSTR(method_wide.as_ptr()),
            PCWSTR(path_wide.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            std::ptr::null(),
            WINHTTP_FLAG_SECURE,
        )
    });
    if request.0.is_null() {
        return Err(format!(
            "Could not create the Cloudflare request: {}",
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
    .map_err(|error| format!("Could not disable Cloudflare redirects: {error}"))?;
    let mut headers = format!("Authorization: Bearer {bearer}\r\nAccept: application/json\r\n");
    if let Some(content_type) = content_type {
        if !matches!(
            content_type,
            "application/json" | "image/png" | "image/jpeg"
        ) {
            return Err("Unsupported upload content type.".to_string());
        }
        headers.push_str(&format!("Content-Type: {content_type}\r\n"));
    }
    if let Some(password) = password {
        if password.len() < 12 || password.len() > 128 || password.chars().any(char::is_control) {
            return Err("Invalid image password.".to_string());
        }
        const ALPHABET: &[u8; 64] =
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
        let mut value = String::with_capacity((password.len() * 4).div_ceil(3));
        for part in password.as_bytes().chunks(3) {
            let word = ((part[0] as usize) << 16)
                | ((part.get(1).copied().unwrap_or(0) as usize) << 8)
                | part.get(2).copied().unwrap_or(0) as usize;
            value.push(ALPHABET[(word >> 18) & 63] as char);
            value.push(ALPHABET[(word >> 12) & 63] as char);
            if part.len() > 1 {
                value.push(ALPHABET[(word >> 6) & 63] as char);
            }
            if part.len() > 2 {
                value.push(ALPHABET[word & 63] as char);
            }
        }
        headers.push_str("X-Image-Password: ");
        headers.push_str(&value);
        headers.push_str("\r\n");
    }
    let headers: Vec<u16> = headers.encode_utf16().collect();
    unsafe { WinHttpSendRequest(request.0, Some(&headers), None, 0, size as u32, 0) }
        .map_err(|error| format!("Could not send the Cloudflare request: {error}"))?;
    let mut buffer = [0u8; 64 * 1024];
    match body {
        RequestBody::Bytes(bytes) => {
            for chunk in bytes.chunks(buffer.len()) {
                let mut written = 0;
                unsafe {
                    WinHttpWriteData(
                        request.0,
                        Some(chunk.as_ptr().cast()),
                        chunk.len() as u32,
                        &mut written,
                    )
                }
                .map_err(|error| format!("Could not send Cloudflare data: {error}"))?;
                if written as usize != chunk.len() {
                    return Err("Cloudflare connection stopped before upload finished.".to_string());
                }
            }
        }
        RequestBody::File(_) => {
            let source = file.as_mut().expect("file body opened above");
            loop {
                let count = source
                    .read(&mut buffer)
                    .map_err(|error| format!("Could not read the screenshot file: {error}"))?;
                if count == 0 {
                    break;
                }
                let mut written = 0;
                unsafe {
                    WinHttpWriteData(
                        request.0,
                        Some(buffer.as_ptr().cast()),
                        count as u32,
                        &mut written,
                    )
                }
                .map_err(|error| format!("Could not send the screenshot: {error}"))?;
                if written as usize != count {
                    return Err("Cloudflare connection stopped before upload finished.".to_string());
                }
            }
        }
    }
    unsafe { WinHttpReceiveResponse(request.0, std::ptr::null_mut()) }
        .map_err(|error| format!("Cloudflare did not respond: {error}"))?;
    let mut status = 0u32;
    let mut status_bytes = std::mem::size_of::<u32>() as u32;
    let mut index = 0u32;
    unsafe {
        WinHttpQueryHeaders(
            request.0,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            PCWSTR::null(),
            Some((&mut status as *mut u32).cast()),
            &mut status_bytes,
            &mut index,
        )
    }
    .map_err(|error| format!("Could not read the Cloudflare response code: {error}"))?;
    let mut data = Vec::new();
    loop {
        let mut available = 0u32;
        unsafe { WinHttpQueryDataAvailable(request.0, &mut available) }
            .map_err(|error| format!("Could not read the Cloudflare response: {error}"))?;
        if available == 0 {
            break;
        }
        if data.len().saturating_add(available as usize) > MAX_JSON_BYTES {
            return Err("Cloudflare response exceeds the safe size limit.".to_string());
        }
        let mut remaining = available as usize;
        while remaining > 0 {
            let mut count = 0u32;
            let limit = remaining.min(buffer.len());
            unsafe {
                WinHttpReadData(
                    request.0,
                    buffer.as_mut_ptr().cast(),
                    limit as u32,
                    &mut count,
                )
            }
            .map_err(|error| format!("Could not read Cloudflare data: {error}"))?;
            if count == 0 {
                return Err("Cloudflare response ended early.".to_string());
            }
            data.extend_from_slice(&buffer[..count as usize]);
            remaining -= count as usize;
        }
    }
    if data
        .windows(bearer.len())
        .any(|part| part == bearer.as_bytes())
    {
        return Err(
            "Cloudflare endpoint returned credential data; response was discarded.".to_string(),
        );
    }
    Ok((status, data))
}

pub fn admin_json(
    origin: &str,
    path: &str,
    method: &str,
    value: Option<&serde_json::Value>,
) -> Result<serde_json::Value, String> {
    let credentials = load_credentials(origin)?;
    let encoded = value
        .map(serde_json::to_vec)
        .transpose()
        .map_err(|error| error.to_string())?;
    let (status, body) = api_request(
        origin,
        path,
        method,
        &credentials.admin_token,
        value.map(|_| "application/json"),
        None,
        RequestBody::Bytes(encoded.as_deref().unwrap_or_default()),
    )?;
    let response: serde_json::Value = serde_json::from_slice(&body)
        .map_err(|_| format!("Cloudflare sent an invalid response (HTTP {status})."))?;
    if !(200..300).contains(&status) {
        return Err(response
            .get("error")
            .and_then(|v| v.as_str())
            .unwrap_or("Cloudflare rejected the request.")
            .to_string());
    }
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_protects_pairing_credentials() {
        let first = generate_token().expect("secure Windows random source");
        let second = generate_token().expect("secure Windows random source");
        assert_eq!(first.len(), 43);
        assert_eq!(second.len(), 43);
        assert_ne!(first, second);
        let plain = format!("pairing:{first}:{second}");
        let sealed = protect(plain.as_bytes()).expect("Windows DPAPI encryption");
        assert_ne!(sealed, plain.as_bytes());
        assert_eq!(
            unprotect(&sealed).expect("Windows DPAPI decryption"),
            plain.as_bytes()
        );
    }
}
