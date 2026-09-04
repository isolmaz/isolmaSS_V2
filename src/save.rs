use crate::capture::Rect;
use crate::settings::SaveFormat;
use std::ffi::c_void;
use std::path::{Path, PathBuf};
use windows::Win32::System::SystemInformation::GetLocalTime;
use windows::core::{GUID, PCWSTR};

#[repr(C)]
struct GdiplusStartupInput {
    gdiplus_version: u32,
    debug_event_callback: usize,
    suppress_background_thread: i32,
    suppress_external_codecs: i32,
}

#[repr(C)]
struct EncoderParameter {
    guid: GUID,
    number_of_values: u32,
    value_type: u32,
    value: *mut c_void,
}

#[repr(C)]
struct EncoderParameters {
    count: u32,
    parameter: [EncoderParameter; 1],
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

pub const CLSID_PNG: GUID = GUID::from_u128(0x557cf406_1a04_11d3_9a73_0000f81ef32e);
const CLSID_JPEG: GUID = GUID::from_u128(0x557cf401_1a04_11d3_9a73_0000f81ef32e);
const ENCODER_QUALITY: GUID = GUID::from_u128(0x1d5be4b5_fa4a_452d_9cdd_5db35105e7eb);
const ENCODER_PARAMETER_VALUE_TYPE_LONG: u32 = 4;
const PIXEL_FORMAT_32BPP_ARGB: i32 = 0x0026200A;

struct GdiPlusToken(usize);

impl GdiPlusToken {
    fn start() -> Result<Self, String> {
        let mut token = 0usize;
        let input = GdiplusStartupInput {
            gdiplus_version: 1,
            debug_event_callback: 0,
            suppress_background_thread: 0,
            suppress_external_codecs: 0,
        };
        let status = unsafe { GdiplusStartup(&mut token, &input, std::ptr::null_mut()) };
        if status == 0 {
            Ok(Self(token))
        } else {
            Err(format!("GDI+ startup failed with status {status}"))
        }
    }
}

impl Drop for GdiPlusToken {
    fn drop(&mut self) {
        unsafe { GdiplusShutdown(self.0) };
    }
}

struct GdiPlusBitmap(*mut c_void);

impl Drop for GdiPlusBitmap {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                let _ = GdipDisposeImage(self.0);
            }
        }
    }
}

pub fn generate_screenshot_filename() -> String {
    generate_screenshot_filename_for(SaveFormat::Png)
}

pub fn generate_screenshot_filename_for(format: SaveFormat) -> String {
    let st = unsafe { GetLocalTime() };
    format!(
        "Screenshot_{:04}-{:02}-{:02}_{:02}-{:02}-{:02}-{:03}.{}",
        st.wYear,
        st.wMonth,
        st.wDay,
        st.wHour,
        st.wMinute,
        st.wSecond,
        st.wMilliseconds,
        format.extension()
    )
}

pub fn default_save_directory() -> PathBuf {
    std::env::var_os("USERPROFILE")
        .map(PathBuf::from)
        .map(|path| path.join("Pictures").join("Screenshots"))
        .unwrap_or_else(|| PathBuf::from("."))
}

fn extract_selection(
    buffer: &[u8],
    full_width: i32,
    full_height: i32,
    selection: &Rect,
) -> Result<(Vec<u8>, i32, i32), String> {
    let clamped = selection.clamp(full_width, full_height);
    if clamped.is_empty() || full_width <= 0 || full_height <= 0 {
        return Err("The screenshot selection is empty or invalid.".to_string());
    }

    let width = clamped.width();
    let height = clamped.height();
    let source_stride = full_width as usize * 4;
    let destination_stride = width as usize * 4;
    let required = (full_width as usize)
        .checked_mul(full_height as usize)
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| "The screenshot dimensions are too large.".to_string())?;
    if buffer.len() < required {
        return Err("The screenshot buffer is smaller than its dimensions.".to_string());
    }

    let mut pixels = vec![0u8; destination_stride * height as usize];
    for row in 0..height {
        let source_start = (clamped.top + row) as usize * source_stride + clamped.left as usize * 4;
        let destination_start = row as usize * destination_stride;
        pixels[destination_start..destination_start + destination_stride]
            .copy_from_slice(&buffer[source_start..source_start + destination_stride]);
    }
    Ok((pixels, width, height))
}

pub fn save_buffer_to_image(
    buffer: &[u8],
    full_width: i32,
    full_height: i32,
    selection: &Rect,
    output_path: &Path,
    format: SaveFormat,
    jpeg_quality: u8,
) -> Result<PathBuf, String> {
    let (mut pixels, width, height) =
        extract_selection(buffer, full_width, full_height, selection)?;
    let _token = GdiPlusToken::start()?;

    let mut raw_bitmap = std::ptr::null_mut();
    let status = unsafe {
        GdipCreateBitmapFromScan0(
            width,
            height,
            width * 4,
            PIXEL_FORMAT_32BPP_ARGB,
            pixels.as_mut_ptr(),
            &mut raw_bitmap,
        )
    };
    if status != 0 || raw_bitmap.is_null() {
        return Err(format!(
            "GDI+ could not create the image (status {status})."
        ));
    }
    let bitmap = GdiPlusBitmap(raw_bitmap);

    let wide_path: Vec<u16> = output_path
        .as_os_str()
        .to_string_lossy()
        .encode_utf16()
        .chain(Some(0))
        .collect();

    let mut quality = u32::from(jpeg_quality.clamp(1, 100));
    let encoder_parameters = EncoderParameters {
        count: 1,
        parameter: [EncoderParameter {
            guid: ENCODER_QUALITY,
            number_of_values: 1,
            value_type: ENCODER_PARAMETER_VALUE_TYPE_LONG,
            value: (&mut quality as *mut u32).cast(),
        }],
    };
    let (encoder, parameters) = match format {
        SaveFormat::Png => (&CLSID_PNG, std::ptr::null()),
        SaveFormat::Jpeg => (
            &CLSID_JPEG,
            (&encoder_parameters as *const EncoderParameters).cast::<c_void>(),
        ),
    };

    let save_status =
        unsafe { GdipSaveImageToFile(bitmap.0, PCWSTR(wide_path.as_ptr()), encoder, parameters) };
    if save_status != 0 {
        return Err(format!(
            "GDI+ could not save the image (status {save_status})."
        ));
    }
    Ok(output_path.to_path_buf())
}

pub fn save_buffer_to_png(
    buffer: &[u8],
    full_width: i32,
    full_height: i32,
    selection: &Rect,
    output_path: &Path,
) -> Result<PathBuf, String> {
    save_buffer_to_image(
        buffer,
        full_width,
        full_height,
        selection,
        output_path,
        SaveFormat::Png,
        100,
    )
}

fn unique_output_path(directory: &Path, format: SaveFormat) -> PathBuf {
    let initial = directory.join(generate_screenshot_filename_for(format));
    if !initial.exists() {
        return initial;
    }

    let stem = initial
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("Screenshot");
    for suffix in 1..=9999 {
        let candidate = directory.join(format!("{stem}_{suffix:04}.{}", format.extension()));
        if !candidate.exists() {
            return candidate;
        }
    }
    directory.join(format!(
        "{stem}_{}.{}",
        std::process::id(),
        format.extension()
    ))
}

pub fn save_screenshot(
    buffer: &[u8],
    full_width: i32,
    full_height: i32,
    selection: &Rect,
    custom_dir: Option<&Path>,
    format: SaveFormat,
    jpeg_quality: u8,
) -> Result<PathBuf, String> {
    let directory = custom_dir
        .map(Path::to_path_buf)
        .unwrap_or_else(default_save_directory);
    std::fs::create_dir_all(&directory).map_err(|error| {
        format!(
            "Could not create the screenshot folder '{}': {error}",
            directory.display()
        )
    })?;
    if !directory.is_dir() {
        return Err(format!(
            "The screenshot destination '{}' is not a directory.",
            directory.display()
        ));
    }

    let output_path = unique_output_path(&directory, format);
    let temp_path = directory.join(format!(
        ".isolmass-{}-{}.tmp",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |duration| duration.as_nanos())
    ));

    let result = (|| {
        save_buffer_to_image(
            buffer,
            full_width,
            full_height,
            selection,
            &temp_path,
            format,
            jpeg_quality,
        )?;
        std::fs::rename(&temp_path, &output_path).map_err(|error| {
            format!(
                "Could not finalize screenshot '{}': {error}",
                output_path.display()
            )
        })?;
        Ok(output_path)
    })();

    if result.is_err() {
        let _ = std::fs::remove_file(&temp_path);
    }
    result
}

pub fn recent_screenshots(directory: &Path, limit: usize) -> std::io::Result<Vec<PathBuf>> {
    if limit == 0 || !directory.exists() {
        return Ok(Vec::new());
    }
    let mut entries = std::fs::read_dir(directory)?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let path = entry.path();
            let extension = path.extension()?.to_str()?.to_ascii_lowercase();
            if !matches!(extension.as_str(), "png" | "jpg" | "jpeg") {
                return None;
            }
            let modified = entry.metadata().ok()?.modified().ok()?;
            Some((modified, path))
        })
        .collect::<Vec<_>>();
    entries.sort_unstable_by_key(|entry| std::cmp::Reverse(entry.0));
    entries.truncate(limit);
    Ok(entries.into_iter().map(|(_, path)| path).collect())
}
