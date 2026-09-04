use std::ffi::c_void;
use windows::core::{Error, Result};
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetDC, ReleaseDC,
    SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, CAPTUREBLT, DIB_RGB_COLORS, HBITMAP, HDC,
    HGDIOBJ, RGBQUAD, SRCCOPY,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN,
    SM_YVIRTUALSCREEN,
};

/// An integer rectangle with inclusive-exclusive bounds.
/// In our coordinate system:
/// - `left` and `top` are inclusive.
/// - `right` and `bottom` are exclusive (e.g. width = right - left).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

/// Hit-testing zones on a committed selection rectangle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionHitZone {
    TopLeftCorner,
    TopRightCorner,
    BottomLeftCorner,
    BottomRightCorner,
    BorderEdge,
    Interior,
    None,
}

impl Rect {
    pub const fn new(left: i32, top: i32, right: i32, bottom: i32) -> Self {
        Self {
            left,
            top,
            right,
            bottom,
        }
    }

    #[inline]
    pub fn width(&self) -> i32 {
        (self.right - self.left).max(0)
    }

    #[inline]
    pub fn height(&self) -> i32 {
        (self.bottom - self.top).max(0)
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.right <= self.left || self.bottom <= self.top
    }

    /// Normalizes two arbitrary points (e.g. drag start and drag current) into a top-left/bottom-right rect.
    pub fn normalized(p1: (i32, i32), p2: (i32, i32)) -> Self {
        let left = p1.0.min(p2.0);
        let right = p1.0.max(p2.0);
        let top = p1.1.min(p2.1);
        let bottom = p1.1.max(p2.1);
        Self {
            left,
            top,
            right,
            bottom,
        }
    }

    /// Clamps the rectangle to fit within `[0, max_width)` and `[0, max_height)`.
    pub fn clamp(&self, max_width: i32, max_height: i32) -> Self {
        let left = self.left.clamp(0, max_width);
        let right = self.right.clamp(0, max_width);
        let top = self.top.clamp(0, max_height);
        let bottom = self.bottom.clamp(0, max_height);
        Self {
            left,
            top,
            right: right.max(left),
            bottom: bottom.max(top),
        }
    }

    /// Computes the bounding box of `self` and `other`.
    pub fn union(&self, other: &Self) -> Self {
        if self.is_empty() {
            return *other;
        }
        if other.is_empty() {
            return *self;
        }
        Self {
            left: self.left.min(other.left),
            top: self.top.min(other.top),
            right: self.right.max(other.right),
            bottom: self.bottom.max(other.bottom),
        }
    }

    /// Inflates the rectangle by `dx` horizontally and `dy` vertically.
    pub fn inflate(&self, dx: i32, dy: i32) -> Self {
        Self {
            left: self.left - dx,
            top: self.top - dy,
            right: self.right + dx,
            bottom: self.bottom + dy,
        }
    }

    /// Checks if a point `(x, y)` is within this rectangle.
    #[inline]
    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.left && x < self.right && y >= self.top && y < self.bottom
    }

    /// Hit-tests a point `pt` against this selection rectangle:
    /// - 4 Corner handles (8x8 px centered on vertices) -> diagonal resize
    /// - 4 Border edge bands (6-8 px centered on edges, excluding corners) -> drag-to-move
    /// - Interior -> move (or draw)
    /// - Outside -> None
    pub fn hit_test_selection(&self, pt: (i32, i32), band: i32, handle_size: i32) -> SelectionHitZone {
        if self.is_empty() {
            return SelectionHitZone::None;
        }

        let half_h = handle_size / 2;
        let (x, y) = pt;

        // 1. 4 Corner handles (8x8 px centered on vertices)
        let tl = Rect::new(self.left - half_h, self.top - half_h, self.left + half_h, self.top + half_h);
        if tl.contains(x, y) {
            return SelectionHitZone::TopLeftCorner;
        }
        let tr = Rect::new(self.right - half_h, self.top - half_h, self.right + half_h, self.top + half_h);
        if tr.contains(x, y) {
            return SelectionHitZone::TopRightCorner;
        }
        let bl = Rect::new(self.left - half_h, self.bottom - half_h, self.left + half_h, self.bottom + half_h);
        if bl.contains(x, y) {
            return SelectionHitZone::BottomLeftCorner;
        }
        let br = Rect::new(self.right - half_h, self.bottom - half_h, self.right + half_h, self.bottom + half_h);
        if br.contains(x, y) {
            return SelectionHitZone::BottomRightCorner;
        }

        // 2. 4 Border edge bands (centered on each border line, thickness = 2 * band, excluding corners)
        let outer = self.inflate(band, band);
        let inner = self.inflate(-band, -band);

        if outer.contains(x, y) && (!inner.contains(x, y) || inner.width() <= 0 || inner.height() <= 0) {
            return SelectionHitZone::BorderEdge;
        }

        // 3. Interior of selection
        if self.contains(x, y) {
            return SelectionHitZone::Interior;
        }

        SelectionHitZone::None
    }
}

/// Holds the full virtual screen capture along with a pre-rendered dimmed buffer.
pub struct CaptureBuffer {
    /// Virtual screen X offset (can be negative on multi-monitor setups).
    pub x: i32,
    /// Virtual screen Y offset (can be negative on multi-monitor setups).
    pub y: i32,
    /// Virtual screen width in physical pixels.
    pub width: i32,
    /// Virtual screen height in physical pixels.
    pub height: i32,
    /// Original 32-bit BGRA pixel data. Length is `width * height * 4`.
    pub original: Vec<u8>,
    /// Pre-rendered dimmed 32-bit BGRA pixel data.
    pub dimmed: Vec<u8>,
}

// RAII cleanup helper for HDC released via ReleaseDC
struct DcReleaseGuard {
    hwnd: HWND,
    hdc: HDC,
}

impl Drop for DcReleaseGuard {
    fn drop(&mut self) {
        unsafe {
            let _ = ReleaseDC(self.hwnd, self.hdc);
        }
    }
}

// RAII cleanup helper for HDC deleted via DeleteDC
struct DcDeleteGuard(HDC);

impl Drop for DcDeleteGuard {
    fn drop(&mut self) {
        unsafe {
            let _ = DeleteDC(self.0);
        }
    }
}

// RAII cleanup helper for GDI object deleted via DeleteObject
struct GdiObjectGuard(HGDIOBJ);

impl Drop for GdiObjectGuard {
    fn drop(&mut self) {
        unsafe {
            let _ = DeleteObject(self.0);
        }
    }
}

impl CaptureBuffer {
    /// Creates a dummy in-memory capture buffer for testing without capturing the display.
    pub fn dummy(width: i32, height: i32) -> Self {
        let len = (width.max(1) as usize) * (height.max(1) as usize) * 4;
        Self {
            x: 0,
            y: 0,
            width,
            height,
            original: vec![0u8; len],
            dimmed: vec![0u8; len],
        }
    }

    /// Captures the full virtual screen across all connected monitors via BitBlt.
    /// Pre-renders a dimmed backdrop buffer so interactive punch-outs are instant.
    pub fn capture_virtual_screen() -> Result<Self> {
        let x = unsafe { GetSystemMetrics(SM_XVIRTUALSCREEN) };
        let y = unsafe { GetSystemMetrics(SM_YVIRTUALSCREEN) };
        let width = unsafe { GetSystemMetrics(SM_CXVIRTUALSCREEN) };
        let height = unsafe { GetSystemMetrics(SM_CYVIRTUALSCREEN) };

        if width <= 0 || height <= 0 {
            return Err(Error::from_win32());
        }

        let buffer_size = (width as usize)
            .checked_mul(height as usize)
            .and_then(|px| px.checked_mul(4))
            .ok_or_else(Error::from_win32)?;

        // 1. Get desktop window DC
        let screen_dc = unsafe { GetDC(HWND::default()) };
        if screen_dc.is_invalid() {
            return Err(Error::from_win32());
        }
        let _screen_guard = DcReleaseGuard {
            hwnd: HWND::default(),
            hdc: screen_dc,
        };

        // 2. Create compatible memory DC
        let mem_dc = unsafe { CreateCompatibleDC(screen_dc) };
        if mem_dc.is_invalid() {
            return Err(Error::from_win32());
        }
        let _mem_guard = DcDeleteGuard(mem_dc);

        // 3. Create a 32-bit top-down DIB section
        let bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: -height, // negative = top-down
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                biSizeImage: 0,
                biXPelsPerMeter: 0,
                biYPelsPerMeter: 0,
                biClrUsed: 0,
                biClrImportant: 0,
            },
            bmiColors: [RGBQUAD::default()],
        };

        let mut bits_ptr: *mut c_void = std::ptr::null_mut();
        let hbitmap: HBITMAP = unsafe {
            CreateDIBSection(
                screen_dc,
                &bmi,
                DIB_RGB_COLORS,
                &mut bits_ptr,
                None,
                0,
            )?
        };
        if hbitmap.is_invalid() || bits_ptr.is_null() {
            return Err(Error::from_win32());
        }
        let _bitmap_guard = GdiObjectGuard(HGDIOBJ(hbitmap.0));

        // 4. Select DIB section into memory DC
        let old_obj = unsafe { SelectObject(mem_dc, HGDIOBJ(hbitmap.0)) };

        // 5. BitBlt from screen DC to memory DC with CAPTUREBLT
        let blt_res = unsafe {
            BitBlt(
                mem_dc,
                0,
                0,
                width,
                height,
                screen_dc,
                x,
                y,
                SRCCOPY | CAPTUREBLT,
            )
        };

        // Restore old object before reading or deleting
        unsafe {
            SelectObject(mem_dc, old_obj);
        }

        blt_res?;

        // 6. Copy bits into Rust Vec<u8>
        let mut original = vec![0u8; buffer_size];
        unsafe {
            std::ptr::copy_nonoverlapping(
                bits_ptr as *const u8,
                original.as_mut_ptr(),
                buffer_size,
            );
        }

        // 7. Pre-render dimmed backdrop buffer
        let dimmed = Self::create_dimmed_buffer(&original);

        Ok(Self {
            x,
            y,
            width,
            height,
            original,
            dimmed,
        })
    }

    /// Pre-renders a dimmed version of the original BGRA pixel buffer.
    /// Applies a ~45% dimming factor to B, G, R channels and sets Alpha to 255.
    pub fn create_dimmed_buffer(original: &[u8]) -> Vec<u8> {
        let dim_lut: [u8; 256] = {
            let mut lut = [0u8; 256];
            let mut i = 0usize;
            while i < 256 {
                lut[i] = ((i as u32 * 115) / 255) as u8;
                i += 1;
            }
            lut
        };

        let mut dimmed = vec![0u8; original.len()];

        let (src_chunks, _) = original.as_chunks::<4>();
        let (dst_chunks, _) = dimmed.as_chunks_mut::<4>();

        for (src, dst) in src_chunks.iter().zip(dst_chunks.iter_mut()) {
            dst[0] = dim_lut[src[0] as usize]; // B
            dst[1] = dim_lut[src[1] as usize]; // G
            dst[2] = dim_lut[src[2] as usize]; // R
            dst[3] = 255;                      // A
        }

        dimmed
    }

    /// Punches out an undimmed rectangle by copying rows from `source` (e.g. `original`)
    /// into `target` (e.g. `composed`).
    pub fn copy_rect(
        target: &mut [u8],
        width: i32,
        height: i32,
        rect: &Rect,
        source: &[u8],
    ) {
        if rect.is_empty() || width <= 0 || height <= 0 {
            return;
        }

        let clamped = rect.clamp(width, height);
        if clamped.is_empty() {
            return;
        }

        let stride = (width as usize) * 4;
        let col_start = clamped.left as usize * 4;
        let col_bytes = (clamped.right - clamped.left) as usize * 4;

        for y in clamped.top..clamped.bottom {
            let row_offset = y as usize * stride;
            let start = row_offset + col_start;
            let end = start + col_bytes;

            if end <= target.len() && end <= source.len() {
                target[start..end].copy_from_slice(&source[start..end]);
            }
        }
    }

    /// Restores a rectangle to the pre-rendered dimmed state.
    pub fn restore_dimmed(&self, target: &mut [u8], rect: &Rect) {
        Self::copy_rect(target, self.width, self.height, rect, &self.dimmed);
    }

    /// Punches out a rectangle with the original undimmed pixels.
    pub fn punch_out(&self, target: &mut [u8], rect: &Rect) {
        Self::copy_rect(target, self.width, self.height, rect, &self.original);
    }

    /// Draws a crisp 1px or 2px rectangle border directly onto the 32-bit BGRA buffer.
    pub fn draw_border(
        target: &mut [u8],
        width: i32,
        height: i32,
        rect: &Rect,
        color_bgra: [u8; 4],
        thickness: i32,
    ) {
        if rect.is_empty() || thickness <= 0 || width <= 0 || height <= 0 {
            return;
        }

        let clamped = rect.clamp(width, height);
        if clamped.is_empty() {
            return;
        }

        let t = thickness.min(clamped.width()).min(clamped.height());
        let stride = width as usize * 4;

        let fill_pixel = |target: &mut [u8], x: i32, y: i32| {
            if x >= 0 && x < width && y >= 0 && y < height {
                let offset = (y as usize * stride) + (x as usize * 4);
                if offset + 4 <= target.len() {
                    target[offset..offset + 4].copy_from_slice(&color_bgra);
                }
            }
        };

        // Top horizontal border (full width)
        for y in clamped.top..(clamped.top + t) {
            for x in clamped.left..clamped.right {
                fill_pixel(target, x, y);
            }
        }

        // Bottom horizontal border (full width)
        for y in (clamped.bottom - t)..clamped.bottom {
            for x in clamped.left..clamped.right {
                fill_pixel(target, x, y);
            }
        }

        // Left vertical border (between top and bottom borders)
        for y in (clamped.top + t)..(clamped.bottom - t) {
            for x in clamped.left..(clamped.left + t) {
                fill_pixel(target, x, y);
            }
        }

        // Right vertical border (between top and bottom borders)
        for y in (clamped.top + t)..(clamped.bottom - t) {
            for x in (clamped.right - t)..clamped.right {
                fill_pixel(target, x, y);
            }
        }
    }

    /// Draws a solid filled rectangle onto the 32-bit BGRA buffer.
    pub fn fill_rect(
        target: &mut [u8],
        width: i32,
        height: i32,
        rect: &Rect,
        color_bgra: [u8; 4],
    ) {
        if rect.is_empty() || width <= 0 || height <= 0 {
            return;
        }
        let clamped = rect.clamp(width, height);
        if clamped.is_empty() {
            return;
        }
        let stride = width as usize * 4;
        for y in clamped.top..clamped.bottom {
            let row_offset = y as usize * stride;
            let start = row_offset + clamped.left as usize * 4;
            let end = row_offset + clamped.right as usize * 4;
            for px in (start..end).step_by(4) {
                if px + 4 <= target.len() {
                    target[px..px + 4].copy_from_slice(&color_bgra);
                }
            }
        }
    }

    /// Draws a dual-tone high-contrast selection border and 4 corner resize handles.
    /// Outer border: 1px black outline.
    /// Main border: 2px accent outline.
    /// Corner handles: 8x8 px squares with white fill and 1px black border.
    pub fn draw_contrast_selection(
        target: &mut [u8],
        width: i32,
        height: i32,
        rect: &Rect,
        accent_bgra: [u8; 4],
    ) {
        if rect.is_empty() || width <= 0 || height <= 0 {
            return;
        }

        let dark_border = [15, 15, 15, 255]; // Deep charcoal/black
        let handle_fill = [255, 255, 255, 255]; // Crisp white

        // 1. Dual-tone selection rectangle:
        // Outer 1px dark border
        let outer = Rect::new(rect.left - 1, rect.top - 1, rect.right + 1, rect.bottom + 1);
        Self::draw_border(target, width, height, &outer, dark_border, 1);

        // Inner 2px accent border
        Self::draw_border(target, width, height, rect, accent_bgra, 2);

        // 2. Four 8x8 corner resize handles
        let half_h = 4;
        let corners = [
            (rect.left, rect.top),
            (rect.right, rect.top),
            (rect.left, rect.bottom),
            (rect.right, rect.bottom),
        ];

        for (cx, cy) in corners {
            let handle_rect = Rect::new(cx - half_h, cy - half_h, cx + half_h, cy + half_h);
            Self::fill_rect(target, width, height, &handle_rect, handle_fill);
            Self::draw_border(target, width, height, &handle_rect, dark_border, 1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_selection_hit_zones() {
        let sel = Rect::new(100, 100, 300, 200);

        // Corner handles (8x8 px centered on vertices, half_h = 4)
        assert_eq!(sel.hit_test_selection((100, 100), 4, 8), SelectionHitZone::TopLeftCorner);
        assert_eq!(sel.hit_test_selection((300, 100), 4, 8), SelectionHitZone::TopRightCorner);
        assert_eq!(sel.hit_test_selection((100, 200), 4, 8), SelectionHitZone::BottomLeftCorner);
        assert_eq!(sel.hit_test_selection((300, 200), 4, 8), SelectionHitZone::BottomRightCorner);

        // Border edge band (excluding corners, e.g. midpoint of top border, +/- 4px)
        assert_eq!(sel.hit_test_selection((200, 100), 4, 8), SelectionHitZone::BorderEdge);
        assert_eq!(sel.hit_test_selection((200, 98), 4, 8), SelectionHitZone::BorderEdge);
        assert_eq!(sel.hit_test_selection((200, 102), 4, 8), SelectionHitZone::BorderEdge);
        assert_eq!(sel.hit_test_selection((100, 150), 4, 8), SelectionHitZone::BorderEdge);
        assert_eq!(sel.hit_test_selection((300, 150), 4, 8), SelectionHitZone::BorderEdge);
        assert_eq!(sel.hit_test_selection((200, 200), 4, 8), SelectionHitZone::BorderEdge);

        // Interior (deep inside selection, far from border band)
        assert_eq!(sel.hit_test_selection((200, 150), 4, 8), SelectionHitZone::Interior);

        // Outside (far from selection)
        assert_eq!(sel.hit_test_selection((50, 50), 4, 8), SelectionHitZone::None);
        assert_eq!(sel.hit_test_selection((400, 400), 4, 8), SelectionHitZone::None);
    }

    #[test]
    fn test_contrast_selection_and_fill() {
        let mut buffer = vec![0u8; 100 * 100 * 4];
        let rect = Rect::new(10, 10, 50, 50);
        CaptureBuffer::draw_contrast_selection(&mut buffer, 100, 100, &rect, [246, 130, 59, 255]);

        // Corner handle should be filled with white [255, 255, 255, 255]
        let offset = (10 * 100 + 10) * 4;
        assert_eq!(&buffer[offset..offset + 4], &[255, 255, 255, 255]);
    }
}
