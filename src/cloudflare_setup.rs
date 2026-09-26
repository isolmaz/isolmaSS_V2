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
    .map_err(|error| format!("Windows güvenli anahtar üretemedi: {error}"))?;
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
        .ok_or_else(|| "%LOCALAPPDATA% kullanılamıyor; yükleme anahtarları korunamaz.".to_string())
}

fn valid_token(token: &str) -> bool {
    token.len() == 43
        && token
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
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
    .map_err(|error| format!("Windows Cloudflare anahtarlarını koruyamadı: {error}"))?;
    let len = result.0.cbData as usize;
    if result.0.pbData.is_null() || len > 65536 {
        return Err("Windows geçersiz korumalı anahtar döndürdü.".to_string());
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
        format!("Cloudflare anahtarları bu Windows kullanıcısı için açılamadı: {error}")
    })?;
    let len = result.0.cbData as usize;
    if result.0.pbData.is_null() || len > 65536 {
        return Err("Korumalı Cloudflare anahtarları geçersiz.".to_string());
    }
    Ok(unsafe { std::slice::from_raw_parts(result.0.pbData, len) }.to_vec())
}

pub fn save_credentials(credentials: &CloudCredentials) -> Result<(), String> {
    if !valid_cloud_origin(&credentials.origin)
        || !valid_token(&credentials.upload_token)
        || !valid_token(&credentials.admin_token)
        || credentials.upload_token == credentials.admin_token
        || credentials.share_password.as_ref().is_some_and(|password| {
            password.chars().count() < 12
                || password.len() > 128
                || password.chars().any(char::is_control)
        })
    {
        return Err("Cloudflare adresi, anahtarları veya resim şifresi geçersiz.".to_string());
    }
    let plain = serde_json::to_vec(credentials).map_err(|error| error.to_string())?;
    write_protected(&credential_path()?, &plain)
}

fn write_protected(path: &Path, plain: &[u8]) -> Result<(), String> {
    let ciphertext = protect(plain)?;
    let parent = path
        .parent()
        .ok_or_else(|| "Anahtar dosyasının klasörü yok.".to_string())?;
    std::fs::create_dir_all(parent)
        .map_err(|error| format!("Anahtar klasörü oluşturulamadı: {error}"))?;
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
            .map_err(|error| format!("Korumalı anahtarlar hazırlanamadı: {error}"))?;
        file.write_all(&ciphertext)
            .and_then(|()| file.sync_all())
            .map_err(|error| format!("Korumalı anahtarlar kaydedilemedi: {error}"))?;
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
        .map_err(|error| format!("Cloudflare anahtarları etkinleştirilemedi: {error}"))
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(temporary);
    }
    result
}

pub fn load_credentials(origin: &str) -> Result<CloudCredentials, String> {
    let path = credential_path()?;
    let bytes = std::fs::read(path)
        .map_err(|error| format!("Cloudflare kurulumu tamamlanmamış: {error}"))?;
    if bytes.len() > 65536 || bytes.is_empty() {
        return Err("Korumalı anahtar dosyasının boyutu geçersiz.".to_string());
    }
    let plain = unprotect(&bytes)?;
    let data: CloudCredentials = serde_json::from_slice(&plain)
        .map_err(|_| "Korumalı anahtar dosyası bozuk.".to_string())?;
    if data.origin != origin {
        return Err(
            "Cloudflare adresi değişti. Yüklemeden önce bu bilgisayarı yeniden bağlayın."
                .to_string(),
        );
    }
    Ok(data)
}

pub fn forget_credentials() -> Result<(), String> {
    remove_secret(credential_path()?)
}

fn remove_secret(path: PathBuf) -> Result<(), String> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("Korumalı anahtarlar silinemedi: {error}")),
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

#[derive(Clone, Copy)]
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
        return Err("Cloudflare isteğinde geçersiz adres veya anahtar var.".to_string());
    }
    https_request(
        &origin[8..],
        path,
        method,
        Some(bearer),
        content_type,
        password,
        body,
    )
}

/// Fixed Cloudflare OAuth and management hosts; never send an OAuth bearer to
/// the user-configured screenshot origin or follow a redirect to another host.
pub fn control_request(
    host: &str,
    path: &str,
    method: &str,
    bearer: Option<&str>,
    content_type: Option<&str>,
    body: RequestBody<'_>,
) -> Result<(u32, Vec<u8>), String> {
    if !((host == "dash.cloudflare.com" && path == "/oauth2/token")
        || (host == "api.cloudflare.com" && path.starts_with("/client/v4/")))
        || path.contains(['#', '\\', '\r', '\n'])
        || path.contains("..")
        || !matches!(method, "GET" | "POST" | "PUT")
        || bearer.is_some_and(|value| {
            value.is_empty()
                || value.len() > 4096
                || !value.bytes().all(|byte| {
                    byte.is_ascii_alphanumeric()
                        || matches!(byte, b'-' | b'_' | b'.' | b'~' | b'+' | b'/' | b'=')
                })
        })
    {
        return Err("Geçersiz Cloudflare API hedefi veya erişim anahtarı.".to_string());
    }
    https_request(host, path, method, bearer, content_type, None, body)
}

/// A failed attempt; `retryable` marks transport failures where repeating the
/// request cannot duplicate its effect.
struct Failure {
    message: String,
    retryable: bool,
    /// The host name could not be resolved.
    unresolved: bool,
}
impl From<String> for Failure {
    fn from(message: String) -> Self {
        Self {
            message,
            retryable: false,
            unresolved: false,
        }
    }
}

const NAME_NOT_RESOLVED: u32 = 12007;

fn unresolved(error: &windows::core::Error) -> bool {
    error.code() == windows::core::HRESULT::from_win32(NAME_NOT_RESOLVED)
}

/// Network hiccups (Wi-Fi roaming, VPN reconnects, a slow DNS answer) are
/// common on laptops; such failures are retried a few times before surfacing.
fn transient(error: &windows::core::Error) -> bool {
    const TIMEOUT: u32 = 12002;
    const CANNOT_CONNECT: u32 = 12029;
    const CONNECTION_ERROR: u32 = 12030;
    const INVALID_SERVER_RESPONSE: u32 = 12152;
    [
        TIMEOUT,
        NAME_NOT_RESOLVED,
        CANNOT_CONNECT,
        CONNECTION_ERROR,
        INVALID_SERVER_RESPONSE,
    ]
    .iter()
    .any(|&code| error.code() == windows::core::HRESULT::from_win32(code))
}

fn https_request(
    host: &str,
    path: &str,
    method: &str,
    bearer: Option<&str>,
    content_type: Option<&str>,
    password: Option<&str>,
    body: RequestBody<'_>,
) -> Result<(u32, Vec<u8>), String> {
    const ATTEMPTS: u32 = 4;
    let mut attempt = 1;
    loop {
        match https_attempt(host, path, method, bearer, content_type, password, body) {
            Ok(response) => return Ok(response),
            Err(failure) if failure.retryable && attempt < ATTEMPTS => {
                crate::diagnostics::record(
                    "cloudflare network",
                    &format!("attempt {attempt} failed, retrying: {}", failure.message),
                );
                if failure.unresolved {
                    // A DNS server failure (e.g. an unreachable IPv6 resolver)
                    // is cached by Windows; drop that entry so the retry asks again.
                    flush_dns_entry(host);
                }
                std::thread::sleep(std::time::Duration::from_millis(400 * attempt as u64));
                attempt += 1;
            }
            Err(failure) if failure.unresolved => {
                return Err(format!(
                    "{} Bilgisayarınız {host} adresini çözemedi. Birkaç saniye sonra tekrar deneyin; sürerse ağ bağdaştırıcınızdaki DNS sunucularını (özellikle erişilemeyen IPv6 DNS adreslerini) denetleyin.",
                    failure.message
                ));
            }
            Err(failure) => return Err(failure.message),
        }
    }
}

/// Removes one host from the Windows DNS client cache, including a cached
/// failure. `DnsFlushResolverCacheEntry_W` is exported by dnsapi.dll on every
/// supported Windows version but is not in the SDK headers, so it is bound at
/// run time; if it is missing the retry simply waits for the cache instead.
fn flush_dns_entry(host: &str) {
    use std::sync::LazyLock;
    use windows::Win32::System::LibraryLoader::{
        GetProcAddress, LOAD_LIBRARY_SEARCH_SYSTEM32, LoadLibraryExW,
    };
    type Flush = unsafe extern "system" fn(PCWSTR) -> i32;
    static FLUSH: LazyLock<Option<Flush>> = LazyLock::new(|| unsafe {
        let module = LoadLibraryExW(w!("dnsapi.dll"), None, LOAD_LIBRARY_SEARCH_SYSTEM32).ok()?;
        GetProcAddress(module, windows::core::s!("DnsFlushResolverCacheEntry_W"))
            .map(|function| std::mem::transmute::<_, Flush>(function))
    });
    if let Some(flush) = *FLUSH {
        let name = wide(host);
        unsafe {
            flush(PCWSTR(name.as_ptr()));
        }
    }
}

/// One WinHTTP session for the process: proxy discovery runs once and
/// keep-alive connections to the Worker and the Cloudflare API are reused,
/// which makes repeated uploads noticeably faster. WinHTTP sessions are
/// thread-safe.
fn session() -> Result<*mut c_void, String> {
    use std::sync::OnceLock;
    static SESSION: OnceLock<usize> = OnceLock::new();
    if let Some(&handle) = SESSION.get() {
        return Ok(handle as *mut c_void);
    }
    let agent = wide(&format!("isolmaSS/{}", env!("CARGO_PKG_VERSION")));
    let handle = unsafe {
        WinHttpOpen(
            PCWSTR(agent.as_ptr()),
            WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
            PCWSTR::null(),
            PCWSTR::null(),
            0,
        )
    };
    if handle.is_null() {
        return Err(format!(
            "WinHTTP başlatılamadı: {}",
            windows::core::Error::from_win32()
        ));
    }
    // Resolve, connect, send and receive limits; uploads over slow uplinks and
    // Worker cold starts need more headroom than a JSON API call.
    if let Err(error) = unsafe { WinHttpSetTimeouts(handle, 10_000, 10_000, 30_000, 30_000) } {
        unsafe {
            let _ = WinHttpCloseHandle(handle);
        }
        return Err(format!("Cloudflare istek süreleri ayarlanamadı: {error}"));
    }
    match SESSION.set(handle as usize) {
        Ok(()) => Ok(handle),
        // Another thread won the race; keep its session and close ours.
        Err(_) => {
            unsafe {
                let _ = WinHttpCloseHandle(handle);
            }
            Ok(*SESSION.get().expect("session was just set") as *mut c_void)
        }
    }
}

fn https_attempt(
    host: &str,
    path: &str,
    method: &str,
    bearer: Option<&str>,
    content_type: Option<&str>,
    password: Option<&str>,
    body: RequestBody<'_>,
) -> Result<(u32, Vec<u8>), Failure> {
    let mut file = match body {
        RequestBody::File(path) => Some(
            std::fs::File::open(path)
                .map_err(|error| format!("Ekran görüntüsü dosyası okunamadı: {error}"))?,
        ),
        RequestBody::Bytes(_) => None,
    };
    let size = match body {
        RequestBody::File(path) => std::fs::metadata(path)
            .map_err(|error| format!("Ekran görüntüsü boyutu okunamadı: {error}"))?
            .len(),
        RequestBody::Bytes(bytes) => bytes.len() as u64,
    };
    if size > 10 * 1024 * 1024 {
        return Err(
            "Ekran görüntüsü veya kurulum isteği izin verilen boyutu aşıyor."
                .to_string()
                .into(),
        );
    }
    let session = session()?;
    let host_wide = wide(host);
    let connection = InternetHandle(unsafe {
        WinHttpConnect(
            session,
            PCWSTR(host_wide.as_ptr()),
            INTERNET_DEFAULT_HTTPS_PORT,
            0,
        )
    });
    if connection.0.is_null() {
        return Err(format!(
            "Cloudflare sunucusuna ulaşılamıyor: {}",
            windows::core::Error::from_win32()
        )
        .into());
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
            "Cloudflare isteği oluşturulamadı: {}",
            windows::core::Error::from_win32()
        )
        .into());
    }
    unsafe {
        WinHttpSetOption(
            Some(request.0),
            WINHTTP_OPTION_REDIRECT_POLICY,
            Some(&WINHTTP_OPTION_REDIRECT_POLICY_NEVER.to_ne_bytes()),
        )
    }
    .map_err(|error| format!("Cloudflare yönlendirmeleri kapatılamadı: {error}"))?;
    let mut headers = String::from("Accept: application/json\r\n");
    if let Some(bearer) = bearer {
        headers.push_str(&format!("Authorization: Bearer {bearer}\r\n"));
    }
    if let Some(content_type) = content_type {
        let multipart = content_type
            .strip_prefix("multipart/form-data; boundary=isolmass-")
            .is_some_and(|boundary| {
                (16..=64).contains(&boundary.len())
                    && boundary
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
            });
        if !multipart
            && !matches!(
                content_type,
                "application/json"
                    | "application/x-www-form-urlencoded"
                    | "image/png"
                    | "image/jpeg"
            )
        {
            return Err("Desteklenmeyen Cloudflare içerik türü.".to_string().into());
        }
        headers.push_str(&format!("Content-Type: {content_type}\r\n"));
    }
    if let Some(password) = password {
        if password.chars().count() < 12
            || password.len() > 128
            || password.chars().any(char::is_control)
        {
            return Err("Geçersiz resim şifresi.".to_string().into());
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
    // Nothing has reached the application yet when the connection itself fails,
    // so this stage is safe to repeat for every method.
    unsafe { WinHttpSendRequest(request.0, Some(&headers), None, 0, size as u32, 0) }.map_err(
        |error| Failure {
            retryable: transient(&error),
            unresolved: unresolved(&error),
            message: format!("Cloudflare'a ulaşılamadı; internet bağlantınızı denetleyin: {error}"),
        },
    )?;
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
                .map_err(|error| format!("Cloudflare'a veri gönderilemedi: {error}"))?;
                if written as usize != chunk.len() {
                    return Err("Cloudflare bağlantısı yükleme bitmeden kesildi."
                        .to_string()
                        .into());
                }
            }
        }
        RequestBody::File(_) => {
            let source = file.as_mut().expect("file body opened above");
            loop {
                let count = source
                    .read(&mut buffer)
                    .map_err(|error| format!("Ekran görüntüsü dosyası okunamadı: {error}"))?;
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
                .map_err(|error| format!("Ekran görüntüsü gönderilemedi: {error}"))?;
                if written as usize != count {
                    return Err("Cloudflare bağlantısı yükleme bitmeden kesildi."
                        .to_string()
                        .into());
                }
            }
        }
    }
    // A request that was delivered may already have taken effect; only reads
    // are repeated after a lost response.
    unsafe { WinHttpReceiveResponse(request.0, std::ptr::null_mut()) }.map_err(|error| {
        Failure {
            retryable: method == "GET" && transient(&error),
            unresolved: false,
            message: format!("Cloudflare yanıt vermedi: {error}"),
        }
    })?;
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
    .map_err(|error| format!("Cloudflare yanıt kodu okunamadı: {error}"))?;
    let mut data = Vec::new();
    loop {
        let mut available = 0u32;
        unsafe { WinHttpQueryDataAvailable(request.0, &mut available) }
            .map_err(|error| format!("Cloudflare yanıtı okunamadı: {error}"))?;
        if available == 0 {
            break;
        }
        if data.len().saturating_add(available as usize) > MAX_JSON_BYTES {
            return Err("Cloudflare yanıtı güvenli boyut sınırını aşıyor."
                .to_string()
                .into());
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
            .map_err(|error| format!("Cloudflare verisi okunamadı: {error}"))?;
            if count == 0 {
                return Err("Cloudflare yanıtı erken kesildi.".to_string().into());
            }
            data.extend_from_slice(&buffer[..count as usize]);
            remaining -= count as usize;
        }
    }
    if bearer.is_some_and(|value| {
        data.windows(value.len())
            .any(|part| part == value.as_bytes())
    }) {
        return Err(
            "Cloudflare yanıtı anahtar verisi içeriyordu; yanıt yok sayıldı."
                .to_string()
                .into(),
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
        .map_err(|_| format!("Cloudflare geçersiz yanıt gönderdi (HTTP {status})."))?;
    if !(200..300).contains(&status) {
        return Err(response
            .get("error")
            .and_then(|v| v.as_str())
            .unwrap_or("Cloudflare isteği reddetti.")
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
