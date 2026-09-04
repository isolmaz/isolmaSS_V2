use crate::capture::Rect;
use windows::core::{Error, Result};
use windows::Win32::Foundation::{GlobalFree, HANDLE, HWND};
use windows::Win32::Graphics::Gdi::{BITMAPINFOHEADER, BI_RGB};
use windows::Win32::System::DataExchange::{CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData};
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};

pub const CF_DIB_FORMAT: u32 = 8; // Standard Windows CF_DIB format

struct ClipboardGuard;

impl Drop for ClipboardGuard {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseClipboard();
        }
    }
}

/// Flattens a sub-rectangle from a full-screen 32-bit BGRA buffer into a
/// standalone DIB byte vector (BITMAPINFOHEADER + bottom-up BGRA pixels).
pub fn flatten_selection_to_dib(
    buffer: &[u8],
    full_width: i32,
    full_height: i32,
    selection: &Rect,
) -> Result<Vec<u8>> {
    let clamped = selection.clamp(full_width, full_height);
    if clamped.is_empty() || full_width <= 0 || full_height <= 0 {
        return Err(Error::from_win32());
    }

    let w = clamped.width();
    let h = clamped.height();
    let pixel_bytes = (w as usize)
        .checked_mul(h as usize)
        .and_then(|px| px.checked_mul(4))
        .ok_or_else(Error::from_win32)?;

    let header_size = std::mem::size_of::<BITMAPINFOHEADER>();
    let total_size = header_size + pixel_bytes;

    let mut dib_bytes = Vec::with_capacity(total_size);

    // 1. Create standard BITMAPINFOHEADER (positive height = bottom-up DIB)
    let header = BITMAPINFOHEADER {
        biSize: header_size as u32,
        biWidth: w,
        biHeight: h,
        biPlanes: 1,
        biBitCount: 32,
        biCompression: BI_RGB.0,
        biSizeImage: pixel_bytes as u32,
        biXPelsPerMeter: 0,
        biYPelsPerMeter: 0,
        biClrUsed: 0,
        biClrImportant: 0,
    };

    let header_slice = unsafe {
        std::slice::from_raw_parts(
            &header as *const BITMAPINFOHEADER as *const u8,
            header_size,
        )
    };
    dib_bytes.extend_from_slice(header_slice);

    // 2. Copy scanlines in bottom-up order (row 0 in DIB = bottom row of image)
    let src_stride = full_width as usize * 4;
    let row_bytes = w as usize * 4;
    let col_offset = clamped.left as usize * 4;

    for y in 0..h {
        let src_y = clamped.bottom - 1 - y;
        let src_offset = (src_y as usize * src_stride) + col_offset;
        let end = src_offset + row_bytes;

        if end <= buffer.len() {
            dib_bytes.extend_from_slice(&buffer[src_offset..end]);
        } else {
            return Err(Error::from_win32());
        }
    }

    Ok(dib_bytes)
}

/// Copies pre-formatted DIB data (BITMAPINFOHEADER + pixels) to the Windows Clipboard.
pub fn copy_dib_to_clipboard(hwnd: Option<HWND>, dib_data: &[u8]) -> Result<()> {
    if dib_data.is_empty() {
        return Err(Error::from_win32());
    }

    // 1. Allocate global moveable memory
    let hmem = unsafe { GlobalAlloc(GMEM_MOVEABLE, dib_data.len())? };
    if hmem.is_invalid() {
        return Err(Error::from_win32());
    }

    // 2. Lock memory and copy DIB data
    let ptr = unsafe { GlobalLock(hmem) };
    if ptr.is_null() {
        unsafe {
            let _ = GlobalFree(hmem);
        }
        return Err(Error::from_win32());
    }

    unsafe {
        std::ptr::copy_nonoverlapping(dib_data.as_ptr(), ptr as *mut u8, dib_data.len());
        let _ = GlobalUnlock(hmem);
    }

    // 3. Set clipboard data
    unsafe {
        OpenClipboard(hwnd.unwrap_or_default())?;
    }
    let _clip_guard = ClipboardGuard;

    unsafe {
        EmptyClipboard()?;
        let res = SetClipboardData(CF_DIB_FORMAT, HANDLE(hmem.0));
        if res.is_err() {
            let _ = GlobalFree(hmem);
            return Err(Error::from_win32());
        }
    }

    // On success, Windows takes ownership of hmem.
    Ok(())
}
