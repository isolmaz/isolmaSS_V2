//! Browser-based Cloudflare consent and installation into the selected user's own account.
//! OAuth access tokens stay in memory; only Worker-specific keys enter Windows DPAPI.
use crate::cloudflare_setup::{self, CloudCredentials, RequestBody};
use serde::Deserialize;
use serde_json::{Value, json};
use std::io::{Read, Write};
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use windows::Win32::Foundation::HWND;
use windows::Win32::Security::Cryptography::{BCRYPT_SHA256_ALG_HANDLE, BCryptHash};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
use windows::core::{PCWSTR, w};

const CLIENT_ID: &str = "aab1858e7c76bac3d9a341eef60d0bea";
const REQUIRED_SCOPES: &str = "memberships.read workers-scripts.write";
const REDIRECT_URI: &str = "http://127.0.0.1:38481/oauth/callback";
const CALLBACK_PORT: u16 = 38481;
const MAX_ACCOUNTS: usize = 200;
const WORKER_SOURCE: &str = include_str!("../cloudflare/worker.mjs");

#[derive(Debug, Clone)]
pub struct Account {
    pub id: String,
    pub name: String,
}

pub struct Authorization {
    pub access_token: String,
    pub accounts: Vec<Account>,
}

fn base64url(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut result = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for part in bytes.chunks(3) {
        let word = ((part[0] as usize) << 16)
            | ((part.get(1).copied().unwrap_or(0) as usize) << 8)
            | part.get(2).copied().unwrap_or(0) as usize;
        result.push(ALPHABET[(word >> 18) & 63] as char);
        result.push(ALPHABET[(word >> 12) & 63] as char);
        if part.len() > 1 {
            result.push(ALPHABET[(word >> 6) & 63] as char);
        }
        if part.len() > 2 {
            result.push(ALPHABET[word & 63] as char);
        }
    }
    result
}

fn sha256(bytes: &[u8]) -> Result<[u8; 32], String> {
    let mut digest = [0u8; 32];
    unsafe { BCryptHash(BCRYPT_SHA256_ALG_HANDLE, None, bytes, &mut digest) }
        .ok()
        .map_err(|error| format!("Windows PKCE özeti oluşturulamadı: {error}"))?;
    Ok(digest)
}

fn percent_encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            out.push(byte as char);
        } else {
            out.push('%');
            out.push(
                char::from_digit((byte >> 4) as u32, 16)
                    .unwrap()
                    .to_ascii_uppercase(),
            );
            out.push(
                char::from_digit((byte & 15) as u32, 16)
                    .unwrap()
                    .to_ascii_uppercase(),
            );
        }
    }
    out
}

fn hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}
fn percent_decode(text: &str) -> Result<String, String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 >= bytes.len() {
                return Err("Malformed Cloudflare callback.".into());
            }
            let high = hex(bytes[index + 1]).ok_or("Malformed Cloudflare callback.")?;
            let low = hex(bytes[index + 2]).ok_or("Malformed Cloudflare callback.")?;
            out.push(high << 4 | low);
            index += 3;
        } else {
            out.push(if bytes[index] == b'+' {
                b' '
            } else {
                bytes[index]
            });
            index += 1;
        }
    }
    String::from_utf8(out).map_err(|_| "Malformed Cloudflare callback.".into())
}

fn parse_callback(request: &str, expected_state: &str) -> Result<Option<String>, String> {
    let mut lines = request.split("\r\n");
    let first = lines.next().ok_or("Missing Cloudflare callback.")?;
    let mut parts = first.split_whitespace();
    if parts.next() != Some("GET") {
        return Err("Invalid Cloudflare callback method.".into());
    }
    let target = parts.next().ok_or("Invalid Cloudflare callback path.")?;
    if parts.next() != Some("HTTP/1.1") || parts.next().is_some() {
        return Err("Invalid Cloudflare callback request.".into());
    }
    let host = lines.find_map(|line| {
        let (name, value) = line.split_once(':')?;
        name.eq_ignore_ascii_case("Host").then(|| value.trim())
    });
    if host != Some("127.0.0.1:38481") {
        return Err("Cloudflare callback host does not match.".into());
    }
    let (path, query) = target
        .split_once('?')
        .ok_or("Cloudflare did not return a response.")?;
    if path != "/oauth/callback" {
        return Err("Unrecognized Cloudflare callback path.".into());
    }
    let mut state = None;
    let mut code = None;
    let mut denied = false;
    for pair in query.split('&') {
        let (key, value) = pair
            .split_once('=')
            .ok_or("Malformed Cloudflare callback.")?;
        match key {
            "state" if state.is_none() => state = Some(percent_decode(value)?),
            "code" if code.is_none() => code = Some(percent_decode(value)?),
            "error" if !denied => denied = true,
            "state" | "code" | "error" => return Err("Duplicate OAuth parameter.".into()),
            _ => {}
        }
    }
    let state = state.ok_or("Cloudflare did not return a request identifier.")?;
    if !constant_equal(state.as_bytes(), expected_state.as_bytes()) {
        return Err("Cloudflare callback did not match this request.".into());
    }
    if denied {
        return Err("Cloudflare izni verilmedi; bağlantı kurulmadı.".into());
    }
    let code = code.ok_or("Cloudflare did not return an authorization code.")?;
    if code.is_empty()
        || code.len() > 4096
        || !code.is_ascii()
        || code.chars().any(char::is_control)
    {
        return Err("Cloudflare returned an invalid authorization code.".into());
    }
    Ok(Some(code))
}

fn constant_equal(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let mut difference = 0u8;
    for (&a, &b) in left.iter().zip(right) {
        difference |= a ^ b;
    }
    difference == 0
}

fn callback_reply(socket: &mut TcpStream, accepted: bool) {
    let html = if accepted {
        "<!doctype html><html lang=tr><meta charset=utf-8><title>isolmaSS · Bağlanıyor</title><main><h1>İzin alındı</h1><p>Kurulum uygulamada tamamlanıyor. Bu sekmeyi kapatabilirsiniz.</p></main><script>window.close()</script>"
    } else {
        "<!doctype html><html lang=tr><meta charset=utf-8><title>isolmaSS · İşlem durdu</title><main><h1>Bağlantı kurulamadı</h1><p>Uygulamadaki hata mesajını kontrol edin.</p></main>"
    };
    let status = if accepted {
        "200 OK"
    } else {
        "400 Bad Request"
    };
    let head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\nReferrer-Policy: no-referrer\r\nContent-Security-Policy: default-src 'none'; script-src 'unsafe-inline'\r\nX-Content-Type-Options: nosniff\r\n\r\n",
        html.len()
    );
    let _ = socket
        .write_all(head.as_bytes())
        .and_then(|()| socket.write_all(html.as_bytes()));
}

/// Reads one HTTP request head (bounded to 8 KiB and 5 seconds) and leaves the
/// socket blocking for the reply. `None` means the connection carried no usable
/// request.
fn read_request(socket: &mut TcpStream, cancel: &AtomicBool) -> Option<Vec<u8>> {
    socket.set_nonblocking(true).ok()?;
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut bytes = vec![0u8; 8192];
    let mut length = 0;
    while length < bytes.len() {
        match socket.read(&mut bytes[length..]) {
            Ok(0) => break,
            Ok(count) => {
                length += count;
                if bytes[..length].windows(4).any(|part| part == b"\r\n\r\n") {
                    break;
                }
            }
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                ) =>
            {
                if cancel.load(Ordering::Relaxed) || Instant::now() >= deadline {
                    return None;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(_) => return None,
        }
    }
    if length == 0 {
        return None;
    }
    socket.set_nonblocking(false).ok()?;
    socket
        .set_write_timeout(Some(Duration::from_secs(5)))
        .ok()?;
    bytes.truncate(length);
    Some(bytes)
}

fn callback(listener: &TcpListener, state: &str, cancel: &AtomicBool) -> Result<String, String> {
    // Signing up, verifying e-mail or completing 2FA can take several minutes.
    let deadline = Instant::now() + Duration::from_secs(600);
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err("Cloudflare bağlantısı iptal edildi.".into());
        }
        if Instant::now() >= deadline {
            return Err("Cloudflare giriş süresi doldu. Yeniden bağlanın.".into());
        }
        match listener.accept() {
            Ok((mut socket, peer)) => {
                if !peer.ip().is_loopback() {
                    continue;
                }
                // Browsers open speculative connections that send nothing, and a
                // Windows socket accepted from a non-blocking listener is itself
                // non-blocking. A quiet or broken connection is skipped instead of
                // failing the login; the real redirect arrives on another one.
                let Some(request) = read_request(&mut socket, cancel) else {
                    continue;
                };
                let parsed = std::str::from_utf8(&request)
                    .map_err(|_| "Malformed Cloudflare callback.".to_string())
                    .and_then(|text| parse_callback(text, state));
                callback_reply(&mut socket, parsed.as_ref().is_ok_and(Option::is_some));
                match parsed {
                    Ok(Some(code)) => return Ok(code),
                    Err(error) if error.contains("izni verilmedi") => return Err(error),
                    _ => continue, // Noise from another local process cannot cancel a valid login.
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(25))
            }
            Err(error) => return Err(format!("Cloudflare girişi alınamadı: {error}")),
        }
    }
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    token_type: String,
    expires_in: u64,
    /// Space-separated scopes actually granted; users may deselect optional ones.
    #[serde(default)]
    scope: Option<String>,
}

fn cloudflare_json(
    host: &str,
    path: &str,
    method: &str,
    token: Option<&str>,
    body: Option<&Value>,
) -> Result<Value, String> {
    let encoded = body
        .map(serde_json::to_vec)
        .transpose()
        .map_err(|error| error.to_string())?;
    let (status, bytes) = cloudflare_setup::control_request(
        host,
        path,
        method,
        token,
        body.map(|_| "application/json"),
        RequestBody::Bytes(encoded.as_deref().unwrap_or_default()),
    )?;
    let response: Value = serde_json::from_slice(&bytes)
        .map_err(|_| format!("Cloudflare API HTTP {status}: geçersiz JSON."))?;
    if !(200..300).contains(&status) || response["success"] != true {
        return Err(format!("Cloudflare API {}.", api_error(status, &bytes)));
    }
    Ok(response["result"].clone())
}

pub fn authorize(cancel: &AtomicBool) -> Result<Authorization, String> {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, CALLBACK_PORT)).map_err(|error| {
        format!("Cloudflare dönüş bağlantısı için yerel port 38481 açılamadı: {error}")
    })?;
    listener
        .set_nonblocking(true)
        .map_err(|error| error.to_string())?;
    let verifier = cloudflare_setup::generate_token()?;
    let state = cloudflare_setup::generate_token()?;
    let challenge = base64url(&sha256(verifier.as_bytes())?);
    let url = format!(
        "https://dash.cloudflare.com/oauth2/auth?client_id={CLIENT_ID}&response_type=code&redirect_uri={}&scope={}&code_challenge={challenge}&code_challenge_method=S256&state={state}",
        percent_encode(REDIRECT_URI),
        percent_encode(REQUIRED_SCOPES)
    );
    let wide: Vec<u16> = url.encode_utf16().chain(Some(0)).collect();
    let launched = unsafe {
        ShellExecuteW(
            HWND::default(),
            w!("open"),
            PCWSTR(wide.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    if launched.0 as isize <= 32 {
        return Err("Cloudflare oturum sayfası tarayıcıda açılamadı.".into());
    }
    let code = callback(&listener, &state, cancel)?;
    if cancel.load(Ordering::Relaxed) {
        return Err("Cloudflare bağlantısı iptal edildi.".into());
    }
    let form = format!(
        "grant_type=authorization_code&client_id={CLIENT_ID}&code={}&redirect_uri={}&code_verifier={verifier}",
        percent_encode(&code),
        percent_encode(REDIRECT_URI)
    );
    let (status, bytes) = cloudflare_setup::control_request(
        "dash.cloudflare.com",
        "/oauth2/token",
        "POST",
        None,
        Some("application/x-www-form-urlencoded"),
        RequestBody::Bytes(form.as_bytes()),
    )?;
    if status != 200 {
        return Err(format!(
            "Cloudflare OAuth oturumu açılamadı (HTTP {status})."
        ));
    }
    let issued: TokenResponse = serde_json::from_slice(&bytes)
        .map_err(|_| "Cloudflare geçersiz OAuth yanıtı verdi.".to_string())?;
    if !issued.token_type.eq_ignore_ascii_case("Bearer")
        || issued.expires_in < 60
        || issued.access_token.len() < 20
        || issued.access_token.len() > 4096
        || !issued.access_token.bytes().all(|byte| {
            byte.is_ascii_alphanumeric()
                || matches!(byte, b'-' | b'_' | b'.' | b'~' | b'+' | b'/' | b'=')
        })
    {
        return Err("Cloudflare geçersiz erişim anahtarı verdi.".into());
    }
    if let Some(granted) = issued.scope.as_deref() {
        let missing: Vec<&str> = REQUIRED_SCOPES
            .split(' ')
            .filter(|scope| !granted.split(' ').any(|given| given == *scope))
            .collect();
        if !missing.is_empty() {
            return Err(format!(
                "Cloudflare izin ekranında gerekli izinler verilmedi ({}). Yeniden bağlanıp tüm izinleri onaylayın.",
                missing.join(", ")
            ));
        }
    }
    let accounts = list_accounts(&issued.access_token, cancel)?;
    Ok(Authorization {
        access_token: issued.access_token,
        accounts,
    })
}

fn list_accounts(token: &str, cancel: &AtomicBool) -> Result<Vec<Account>, String> {
    let mut accounts = Vec::new();
    for page in 1..=MAX_ACCOUNTS / 50 {
        if cancel.load(Ordering::Relaxed) {
            return Err("Cloudflare bağlantısı iptal edildi.".into());
        }
        let result = cloudflare_json(
            "api.cloudflare.com",
            &format!("/client/v4/memberships?per_page=50&page={page}&status=accepted"),
            "GET",
            Some(token),
            None,
        )?;
        let rows = result
            .as_array()
            .ok_or("Cloudflare hesap listesi okunamadı.")?;
        for row in rows {
            let Some(id) = row["account"]["id"].as_str() else {
                continue;
            };
            let Some(name) = row["account"]["name"].as_str() else {
                continue;
            };
            if id.len() == 32
                && id.bytes().all(|byte| byte.is_ascii_hexdigit())
                && !accounts.iter().any(|account: &Account| account.id == id)
            {
                let safe_name = !name.is_empty()
                    && name.len() <= 100
                    && !name.chars().any(|character| {
                        character.is_control()
                            || matches!(character, '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
                    });
                accounts.push(Account {
                    id: id.to_ascii_lowercase(),
                    name: if safe_name {
                        name.into()
                    } else {
                        "Cloudflare hesabı".into()
                    },
                });
            }
        }
        if rows.len() < 50 {
            break;
        }
        if page == MAX_ACCOUNTS / 50 {
            return Err("200'den fazla Cloudflare hesabı var; güvenli hesap seçimi için liste sınırı aşıldı.".into());
        }
    }
    if accounts.is_empty() {
        return Err("Cloudflare'da bu izinle kullanılabilen bir hesap bulunamadı.".into());
    }
    Ok(accounts)
}

fn account_path(account: &Account, suffix: &str) -> String {
    format!("/client/v4/accounts/{}/workers/{suffix}", account.id)
}

/// The account's workers.dev subdomain, and whether this call registered it.
fn ensure_subdomain(
    token: &str,
    account: &Account,
    cancel: &AtomicBool,
) -> Result<(String, bool), String> {
    let path = account_path(account, "subdomain");
    let (status, bytes) = cloudflare_setup::control_request(
        "api.cloudflare.com",
        &path,
        "GET",
        Some(token),
        None,
        RequestBody::Bytes(&[]),
    )?;
    if status == 200 {
        let response: Value =
            serde_json::from_slice(&bytes).map_err(|_| "Cloudflare alt alanı okunamadı.")?;
        if response["success"] != true {
            return Err("Cloudflare Workers alt alanına erişim reddedildi.".into());
        }
        return subdomain(&response["result"]["subdomain"]).map(|name| (name, false));
    }
    if status != 404 {
        return Err(format!(
            "Cloudflare Workers alt alanı denetlenemedi ({}).{}",
            api_error(status, &bytes),
            if status == 403 {
                " Seçilen hesapta Workers yetkiniz olmayabilir ya da uygulamanın OAuth istemcisi yalnızca kendi hesabına açık (private) olabilir; başka bir hesap seçmeyi deneyin."
            } else {
                ""
            }
        ));
    }
    for _ in 0..3 {
        if cancel.load(Ordering::Relaxed) {
            return Err("Cloudflare kurulumu iptal edildi.".into());
        }
        let name = format!("isolmass-{}", random_label(10)?);
        let result = cloudflare_json(
            "api.cloudflare.com",
            &path,
            "PUT",
            Some(token),
            Some(&json!({"subdomain": name})),
        );
        if let Ok(response) = result {
            return subdomain(&response["subdomain"]).map(|name| (name, true));
        }
        if let Err(error) = result
            && !error.contains("HTTP 409")
        {
            return Err(error);
        }
    }
    Err("Cloudflare boşta bir workers.dev alt alan adı veremedi; tekrar deneyin.".into())
}

fn subdomain(value: &Value) -> Result<String, String> {
    let name = value
        .as_str()
        .ok_or("Cloudflare geçersiz Workers alt alanı döndürdü.")?;
    if name.is_empty()
        || name.len() > 63
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        || name.starts_with('-')
        || name.ends_with('-')
    {
        return Err("Cloudflare geçersiz Workers alt alanı döndürdü.".into());
    }
    Ok(name.to_string())
}

fn random_label(length: usize) -> Result<String, String> {
    let token = cloudflare_setup::generate_token()?;
    let label: String = token
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .take(length)
        .collect();
    if label.len() != length {
        return Err("Windows geçerli bir Cloudflare kaynak adı üretemedi; tekrar deneyin.".into());
    }
    Ok(label.to_ascii_lowercase())
}

fn random_worker_name() -> Result<String, String> {
    Ok(format!("isolmass-share-{}", random_label(12)?))
}

fn vacant_worker(token: &str, account: &Account, cancel: &AtomicBool) -> Result<String, String> {
    for _ in 0..3 {
        if cancel.load(Ordering::Relaxed) {
            return Err("Cloudflare kurulumu iptal edildi.".into());
        }
        let name = random_worker_name()?;
        let path = account_path(account, &format!("scripts/{name}"));
        let (status, _) = cloudflare_setup::control_request(
            "api.cloudflare.com",
            &path,
            "GET",
            Some(token),
            None,
            RequestBody::Bytes(&[]),
        )?;
        if status == 404 {
            return Ok(name);
        }
        if status != 200 {
            return Err(format!(
                "Mevcut Worker adları denetlenemedi (HTTP {status})."
            ));
        }
    }
    Err("Boş bir Worker adı bulunamadı; tekrar deneyin.".into())
}

fn multipart(upload_token: &str, admin_token: &str) -> Result<(String, Vec<u8>), String> {
    let boundary = format!("isolmass-{}", cloudflare_setup::generate_token()?);
    let metadata = json!({
        "main_module": "worker.mjs",
        "compatibility_date": "2026-09-24",
        "bindings": [
            {"type":"durable_object_namespace", "name":"STORE", "class_name":"ShareStore"},
            {"type":"secret_text", "name":"UPLOAD_TOKEN", "text":upload_token},
            {"type":"secret_text", "name":"ADMIN_TOKEN", "text":admin_token}
        ],
        // Script-upload API shape (SingleStepMigration), not wrangler's `[[migrations]]` list.
        "migrations": {"new_tag":"v1", "new_sqlite_classes":["ShareStore"]}
    });
    let encoded = serde_json::to_vec(&metadata).map_err(|error| error.to_string())?;
    let mut body = Vec::with_capacity(WORKER_SOURCE.len() + encoded.len() + 512);
    body.extend_from_slice(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"metadata\"\r\nContent-Type: application/json\r\n\r\n").as_bytes());
    body.extend_from_slice(&encoded);
    body.extend_from_slice(format!("\r\n--{boundary}\r\nContent-Disposition: form-data; name=\"worker.mjs\"; filename=\"worker.mjs\"\r\nContent-Type: application/javascript+module\r\n\r\n").as_bytes());
    body.extend_from_slice(WORKER_SOURCE.as_bytes());
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    Ok((format!("multipart/form-data; boundary={boundary}"), body))
}

/// A Worker that was created in the user's account. `ready` is false when its
/// workers.dev address did not answer yet; the pairing is still valid.
pub struct Installed {
    pub credentials: CloudCredentials,
    pub ready: bool,
}

pub fn install(token: &str, account: &Account, cancel: &AtomicBool) -> Result<Installed, String> {
    if account.id.len() != 32 || !account.id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("Geçersiz Cloudflare hesap kimliği.".into());
    }
    let (subdomain, created) = ensure_subdomain(token, account, cancel)?;
    let worker = vacant_worker(token, account, cancel)?;
    let upload_token = cloudflare_setup::generate_token()?;
    let admin_token = cloudflare_setup::generate_token()?;
    let (content_type, body) = multipart(&upload_token, &admin_token)?;
    if cancel.load(Ordering::Relaxed) {
        return Err("Cloudflare kurulumu iptal edildi.".into());
    }
    let path = account_path(account, &format!("scripts/{worker}"));
    let (status, bytes) = cloudflare_setup::control_request(
        "api.cloudflare.com",
        &path,
        "PUT",
        Some(token),
        Some(&content_type),
        RequestBody::Bytes(&body),
    )?;
    if !(200..300).contains(&status) {
        return Err(format!(
            "Cloudflare Worker kurulumu reddetti ({}).",
            api_error(status, &bytes)
        ));
    }
    let response: Value = serde_json::from_slice(&bytes)
        .map_err(|_| "Cloudflare Worker kurulum yanıtı okunamadı.".to_string())?;
    if response["success"] != true {
        return Err("Cloudflare Worker oluşturulamadı.".into());
    }
    let route = account_path(account, &format!("scripts/{worker}/subdomain"));
    let enabled = cloudflare_json(
        "api.cloudflare.com",
        &route,
        "POST",
        Some(token),
        Some(&json!({"enabled": true, "previews_enabled": false})),
    )
    .map_err(|error| {
        format!("Worker {worker} oluşturuldu fakat workers.dev etkinleştirilemedi: {error}")
    })?;
    if enabled["enabled"] != true {
        return Err(format!(
            "Worker {worker} oluşturuldu ancak workers.dev erişimi açılamadı."
        ));
    }
    let origin = format!("https://{worker}.{subdomain}.workers.dev");
    let credentials = CloudCredentials {
        origin: origin.clone(),
        upload_token,
        admin_token,
        share_password: None,
    };
    // A new workers.dev name needs DNS/TLS propagation (minutes for a brand-new
    // account subdomain). Asking too early also plants a negative DNS cache
    // entry in Windows, so a fresh subdomain gets a head start.
    let deadline = Instant::now() + Duration::from_secs(if created { 150 } else { 90 });
    if created {
        wait(Duration::from_secs(8), cancel);
    }
    loop {
        if cancel.load(Ordering::Relaxed) {
            // The Worker exists and its keys are known: keep the pairing rather
            // than orphaning the Worker in the user's account.
            return Ok(Installed {
                credentials,
                ready: false,
            });
        }
        if let Ok((200, reply)) = cloudflare_setup::api_request(
            &origin,
            "/api/setup",
            "POST",
            &credentials.admin_token,
            None,
            None,
            RequestBody::Bytes(&[]),
        ) && let Ok(value) = serde_json::from_slice::<Value>(&reply)
            && value["status"] == "ready"
            && value["origin"] == origin
        {
            return Ok(Installed {
                credentials,
                ready: true,
            });
        }
        if Instant::now() >= deadline {
            return Ok(Installed {
                credentials,
                ready: false,
            });
        }
        wait(Duration::from_secs(3), cancel);
    }
}

fn wait(duration: Duration, cancel: &AtomicBool) {
    let end = Instant::now() + duration;
    while Instant::now() < end && !cancel.load(Ordering::Relaxed) {
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// Cloudflare's first error, shortened and stripped of control characters so it
/// can be shown in a dialog.
fn api_error(status: u32, bytes: &[u8]) -> String {
    let first = serde_json::from_slice::<Value>(bytes)
        .ok()
        .and_then(|value| value["errors"].as_array()?.first().cloned());
    let code = first.as_ref().and_then(|error| error["code"].as_u64());
    let message: Option<String> = first
        .as_ref()
        .and_then(|error| error["message"].as_str())
        .map(|text| {
            text.chars()
                .filter(|character| !character.is_control())
                .take(200)
                .collect()
        });
    match (code, message) {
        (Some(code), Some(message)) => format!("HTTP {status}, kod {code}: {message}"),
        (Some(code), None) => format!("HTTP {status}, kod {code}"),
        (None, Some(message)) => format!("HTTP {status}: {message}"),
        (None, None) => format!("HTTP {status}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oauth_callback_rejects_wrong_state_and_duplicate_parameters() {
        let line =
            "GET /oauth/callback?state=correct&code=abc HTTP/1.1\r\nHost: 127.0.0.1:38481\r\n\r\n";
        assert_eq!(
            parse_callback(line, "correct").unwrap().as_deref(),
            Some("abc")
        );
        assert!(parse_callback(line, "wrong").is_err());
        assert!(parse_callback("GET /oauth/callback?state=correct&state=correct&code=abc HTTP/1.1\r\nHost: 127.0.0.1:38481\r\n\r\n", "correct").is_err());
        assert!(
            parse_callback(
                "GET /oauth/callback?state=correct&code=abc HTTP/1.1\r\nHost: bad.test\r\n\r\n",
                "correct"
            )
            .is_err()
        );
    }
}
