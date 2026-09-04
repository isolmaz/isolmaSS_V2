use crate::capture::Rect;
use std::ffi::c_void;
use std::path::{Path, PathBuf};
use windows::core::{GUID, PCWSTR};
use windows::Win32::System::SystemInformation::GetLocalTime;

#[repr(C)]
struct GdiplusStartupInput {
    gdiplus_version: u32,
    debug_event_callback: usize,
    suppress_background_thread: i32,
    suppress_external_codecs: i32,
}

#[link(name = "gdiplus")]
unsafe extern "system" {
    fn GdiplusStartup(
        token: *mut usize,
        input: *const GdiplusStartupInput,
        output: *mut c_void,
    ) -> i32;
    fn GdiplusShutdown(token: usize);
    fn GdipCreateBitmapFromScan0(
        width: i32,
        height: i32,
        stride: i32,
        format: i32,
        scan0: *mut u8,
        bitmap: *mut *mut c_void,
    ) -> i32;
    fn GdipSaveImageToFile(
        image: *mut c_void,
        filename: PCWSTR,
        clsid_encoder: *const GUID,
        encoder_params: *const c_void,
    ) -> i32;
    fn GdipDisposeImage(image: *mut c_void) -> i32;
}

// Standard Windows PNG Encoder CLSID: {557cf406-1a04-11d3-9a73-0000f81ef32e}
pub const CLSID_PNG: GUID = GUID::from_u128(0x557cf406_1a04_11d3_9a73_0000f81ef32e);

// PixelFormat32bppARGB = 0x0026200A (top-down 32-bit BGRA in Windows memory)
const PIXEL_FORMAT_32BPP_ARGB: i32 = 0x0026200A;

/// Generates a timestamped filename: `Screenshot_YYYY-MM-DD_HH-MM-SS.png`.
pub fn generate_screenshot_filename() -> String {
    let st = unsafe { GetLocalTime() };
    format!(
        "Screenshot_{:04}-{:02}-{:02}_{:02}-{:02}-{:02}.png",
        st.wYear, st.wMonth, st.wDay, st.wHour, st.wMinute, st.wSecond
    )
}

/// Returns the default screenshot save directory: `%USERPROFILE%\Pictures\Screenshots`.
/// Creates the directory if it does not exist.
pub fn default_save_directory() -> PathBuf {
    if let Some(user_profile) = std::env::var_os("USERPROFILE") {
        let mut p = PathBuf::from(user_profile);
        p.push("Pictures");
        p.push("Screenshots");
        if !p.exists() {
            let _ = std::fs::create_dir_all(&p);
        }
        if p.exists() {
            return p;
        }
    }
    PathBuf::from(".")
}

/// Saves a rectangular sub-region from a 32-bit BGRA buffer directly to a PNG file using GDI+.
pub fn save_buffer_to_png(
    buffer: &[u8],
    full_width: i32,
    full_height: i32,
    selection: &Rect,
    output_path: &Path,
) -> Result<PathBuf, String> {
    let clamped = selection.clamp(full_width, full_height);
    if clamped.is_empty() || full_width <= 0 || full_height <= 0 {
        return Err("Invalid or empty selection rectangle".to_string());
    }

    let w = clamped.width();
    let h = clamped.height();
    let src_stride = full_width as usize * 4;
    let dst_stride = w as usize * 4;

    // Extract contiguous sub-region pixels
    let mut pixels = vec![0u8; (w as usize) * (h as usize) * 4];
    for y in 0..h {
        let src_y = (clamped.top + y) as usize;
        let src_offset = (src_y * src_stride) + (clamped.left as usize * 4);
        let dst_offset = (y as usize) * dst_stride;

        if src_offset + dst_stride <= buffer.len() && dst_offset + dst_stride <= pixels.len() {
            pixels[dst_offset..dst_offset + dst_stride]
                .copy_from_slice(&buffer[src_offset..src_offset + dst_stride]);
        } else {
            return Err("Buffer bounds exceeded during image extraction".to_string());
        }
    }

    // 1. Startup GDI+
    let mut token = 0usize;
    let startup_input = GdiplusStartupInput {
        gdiplus_version: 1,
        debug_event_callback: 0,
        suppress_background_thread: 0,
        suppress_external_codecs: 0,
    };

    let start_status = unsafe {
        GdiplusStartup(
            &mut token,
            &startup_input,
            std::ptr::null_mut(),
        )
    };
    if start_status != 0 {
        return Err(format!("GdiplusStartup failed with status {}", start_status));
    }

    // 2. Create Bitmap from pixel buffer
    let mut bitmap: *mut c_void = std::ptr::null_mut();
    let create_status = unsafe {
        GdipCreateBitmapFromScan0(
            w,
            h,
            dst_stride as i32,
            PIXEL_FORMAT_32BPP_ARGB,
            pixels.as_mut_ptr(),
            &mut bitmap,
        )
    };

    if create_status != 0 || bitmap.is_null() {
        unsafe {
            GdiplusShutdown(token);
        }
        return Err(format!("GdipCreateBitmapFromScan0 failed with status {}", create_status));
    }

    // 3. Save Image to PNG
    let wide_path: Vec<u16> = output_path
        .to_string_lossy()
        .encode_utf16()
        .chain(Some(0))
        .collect();

    let save_status = unsafe {
        GdipSaveImageToFile(
            bitmap,
            PCWSTR(wide_path.as_ptr()),
            &CLSID_PNG,
            std::ptr::null(),
        )
    };

    // 4. Dispose Bitmap and Shutdown GDI+
    unsafe {
        GdipDisposeImage(bitmap);
        GdiplusShutdown(token);
    }

    if save_status != 0 {
        return Err(format!("GdipSaveImageToFile failed with status {}", save_status));
    }

    Ok(output_path.to_path_buf())
}

/// High-level function: saves the current screenshot selection to a PNG file.
pub fn save_screenshot(
    buffer: &[u8],
    full_width: i32,
    full_height: i32,
    selection: &Rect,
    custom_dir: Option<&Path>,
) -> Result<PathBuf, String> {
    let dir = match custom_dir {
        Some(d) => d.to_path_buf(),
        None => default_save_directory(),
    };

    if !dir.exists() {
        let _ = std::fs::create_dir_all(&dir);
    }

    let filename = generate_screenshot_filename();
    let mut file_path = dir;
    file_path.push(filename);

    save_buffer_to_png(buffer, full_width, full_height, selection, &file_path)
}
