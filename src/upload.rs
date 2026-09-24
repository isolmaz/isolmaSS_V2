use crate::capture::Rect;
use crate::cloudflare_setup::{self, RequestBody};
use crate::save::save_buffer_to_image;
use crate::settings::{SaveFormat, Settings};
use serde::Deserialize;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use windows::Win32::Foundation::HWND;
use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

pub const WM_UPLOAD_DONE: u32 = windows::Win32::UI::WindowsAndMessaging::WM_APP + 41;
pub type UploadResult = Arc<Mutex<Option<Result<String, String>>>>;

struct TemporaryScreenshot(PathBuf);
impl Drop for TemporaryScreenshot {
    fn drop(&mut self) {
        if let Err(error) = std::fs::remove_file(&self.0)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            crate::diagnostics::record("temporary upload cleanup", &error.to_string());
        }
    }
}

#[derive(Deserialize)]
struct UploadReceipt {
    id: String,
    url: String,
}

/// Encodes the already-flattened editor selection, then sends its file on a
/// background thread so the editor remains responsive. The caller keeps the
/// overlay open until WM_UPLOAD_DONE has been handled.
pub fn begin_upload(
    pixels: &[u8],
    width: i32,
    height: i32,
    selection: &Rect,
    settings: &Settings,
    shared: UploadResult,
    hwnd: HWND,
) -> Result<(), String> {
    let origin = settings.cloud_url.as_deref().ok_or_else(|| {
        "Set up your own Cloudflare address in Settings before uploading.".to_string()
    })?;
    let credentials = cloudflare_setup::load_credentials(origin)?;
    let unique = cloudflare_setup::generate_token()?;
    let output = std::env::temp_dir().join(format!(
        "isolmass-upload-{unique}.{}",
        settings.save_format.extension()
    ));
    let _guard = TemporaryScreenshot(output.clone());
    save_buffer_to_image(
        pixels,
        width,
        height,
        selection,
        &output,
        settings.save_format,
        settings.jpeg_quality,
    )
    .map_err(|error| format!("Could not prepare screenshot for upload: {error}"))?;
    let format = settings.save_format;
    let target = credentials.origin.clone();
    let handle = hwnd.0 as usize;
    std::thread::Builder::new()
        .name("isolmass-upload".into())
        .spawn(move || {
            let _guard = _guard;
            let outcome = send(
                &target,
                &credentials.upload_token,
                credentials.share_password.as_deref(),
                &output,
                format,
            );
            if let Ok(mut pending) = shared.lock() {
                *pending = Some(outcome);
            } else {
                crate::diagnostics::record("upload completion", "Could not record upload result.");
                return;
            }
            let hwnd = HWND(handle as *mut std::ffi::c_void);
            if let Err(error) = unsafe { PostMessageW(hwnd, WM_UPLOAD_DONE, WPARAM(0), LPARAM(0)) }
            {
                crate::diagnostics::record(
                    "upload completion",
                    &format!("Could not notify editor: {error}"),
                );
            }
        })
        .map_err(|error| format!("Could not start upload: {error}"))?;
    Ok(())
}

fn send(
    origin: &str,
    token: &str,
    password: Option<&str>,
    output: &std::path::Path,
    format: SaveFormat,
) -> Result<String, String> {
    let type_name = match format {
        SaveFormat::Png => "image/png",
        SaveFormat::Jpeg => "image/jpeg",
    };
    let (status, body) = cloudflare_setup::api_request(
        origin,
        "/api/upload",
        "POST",
        token,
        Some(type_name),
        password,
        RequestBody::File(output),
    )?;
    if status != 201 {
        let detail = serde_json::from_slice::<serde_json::Value>(&body)
            .ok()
            .and_then(|value| {
                value
                    .get("error")
                    .and_then(|text| text.as_str())
                    .map(str::to_owned)
            })
            .unwrap_or_else(|| format!("Upload failed with HTTP {status}."));
        return Err(detail
            .chars()
            .filter(|character| !character.is_control())
            .take(240)
            .collect());
    }
    let receipt: UploadReceipt = serde_json::from_slice(&body)
        .map_err(|_| "Cloudflare returned an invalid upload receipt.".to_string())?;
    if receipt.id.len() != 32
        || !receipt
            .id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        || receipt.url != format!("{origin}/i/{}", receipt.id)
    {
        return Err("Cloudflare returned a link outside this installation.".to_string());
    }
    Ok(receipt.url)
}
