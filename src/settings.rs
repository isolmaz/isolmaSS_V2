use crate::annotation::bgra_to_colorref;
use crate::hotkey::HotkeyConfig;
use crate::save::default_save_directory;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use windows::core::{w, PCWSTR, Result};
use windows::Win32::Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreateFontW, CreatePen, CreateSolidBrush, DeleteObject, DrawTextW, EndPaint,
    GetStockObject, InvalidateRect, RoundRect, SelectObject, SetBkMode, SetTextColor,
    CLIP_DEFAULT_PRECIS, DEFAULT_CHARSET, DEFAULT_PITCH, DEFAULT_QUALITY, DT_CENTER, DT_LEFT,
    DT_SINGLELINE, DT_VCENTER, FF_DONTCARE, FW_BOLD, FW_NORMAL, HBRUSH, HDC, HGDIOBJ, HPEN,
    NULL_BRUSH, PS_SOLID, TRANSPARENT,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{SetFocus, VK_ESCAPE};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW,
    GetSystemMetrics, GetWindowLongPtrW, PostQuitMessage, RegisterClassExW, SetCursor,
    SetForegroundWindow, SetWindowLongPtrW, ShowWindow, TranslateMessage, GWLP_USERDATA, IDC_ARROW,
    IDC_HAND, MSG, SM_CXSCREEN, SM_CYSCREEN, SW_SHOW, WM_CLOSE, WM_DESTROY, WM_ERASEBKGND,
    WM_KEYDOWN, WM_LBUTTONDOWN, WM_MOUSEMOVE, WM_PAINT, WM_SETCURSOR, WNDCLASSEXW,
    WS_CAPTION, WS_EX_TOPMOST, WS_MINIMIZEBOX, WS_OVERLAPPED, WS_SYSMENU,
};

const SETTINGS_CLASS_NAME: windows::core::PCWSTR = w!("isolmaSS_SettingsClass");

pub const PRESET_COLORS: [[u8; 4]; 8] = [
    [49, 49, 224, 255],   // Red (#E03131)
    [7, 103, 247, 255],   // Orange (#F76707)
    [25, 196, 252, 255],  // Yellow (#FCC419)
    [68, 158, 47, 255],   // Green (#2F9E44)
    [194, 113, 25, 255],  // Blue (#1971C2)
    [181, 54, 156, 255],  // Purple (#9C36B5)
    [255, 255, 255, 255], // White (#FFFFFF)
    [41, 37, 33, 255],    // Black (#212529)
];

pub const PRESET_THICKNESSES: [i32; 3] = [2, 4, 8];

/// Persistent application settings model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    pub hotkey: HotkeyConfig,
    pub save_directory: PathBuf,
    pub default_color: [u8; 4],
    pub default_thickness: i32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            hotkey: HotkeyConfig::default(),
            save_directory: default_save_directory(),
            default_color: PRESET_COLORS[0], // Default vibrant red
            default_thickness: 3,
        }
    }
}

impl Settings {
    /// Canonical path to settings file: `%APPDATA%\isolmaSS\settings.json`.
    pub fn config_path() -> Option<PathBuf> {
        crate::hotkey::settings_path()
    }

    /// Loads settings from disk or returns default configuration.
    pub fn load_or_default() -> Self {
        if let Some(settings) = Self::config_path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|c| serde_json::from_str::<Settings>(&c).ok())
        {
            return settings;
        }
        Self::default()
    }

    /// Saves settings to disk as formatted JSON.
    pub fn save(&self) -> std::io::Result<()> {
        if let Some(path) = Self::config_path() {
            if let Some(parent) = path.parent().filter(|p| !p.exists()) {
                let _ = std::fs::create_dir_all(parent);
            }
            let json = serde_json::to_string_pretty(self)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
            std::fs::write(path, json)?;
        }
        Ok(())
    }
}

struct SettingsWindowState {
    settings: Settings,
    saved: bool,
    hovered_elem: Option<usize>, // 0..8 colors, 10..12 thickness, 20 save, 21 cancel
}

struct GdiResourceGuard {
    hdc: HDC,
    old_pen: HGDIOBJ,
    pen: HPEN,
    old_brush: HGDIOBJ,
    brush: HBRUSH,
}

impl Drop for GdiResourceGuard {
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.hdc, self.old_pen);
            let _ = DeleteObject(HGDIOBJ(self.pen.0));
            SelectObject(self.hdc, self.old_brush);
            let _ = DeleteObject(HGDIOBJ(self.brush.0));
        }
    }
}

unsafe extern "system" fn settings_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let state_ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *mut SettingsWindowState;

    match msg {
        WM_ERASEBKGND => LRESULT(1),

        WM_SETCURSOR => {
            if !state_ptr.is_null() {
                let state = unsafe { &*state_ptr };
                let cursor_id = if state.hovered_elem.is_some() {
                    IDC_HAND
                } else {
                    IDC_ARROW
                };
                let cur = unsafe {
                    windows::Win32::UI::WindowsAndMessaging::LoadCursorW(
                        HINSTANCE::default(),
                        cursor_id,
                    )
                    .unwrap_or_default()
                };
                unsafe {
                    SetCursor(cur);
                }
                return LRESULT(1);
            }
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }

        WM_PAINT => {
            if !state_ptr.is_null() {
                let state = unsafe { &*state_ptr };
                let mut ps = windows::Win32::Graphics::Gdi::PAINTSTRUCT::default();
                let hdc = unsafe { BeginPaint(hwnd, &mut ps) };

                render_settings_ui(hdc, state);

                let _ = unsafe { EndPaint(hwnd, &ps) };
                return LRESULT(0);
            }
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }

        WM_MOUSEMOVE => {
            if !state_ptr.is_null() {
                let state = unsafe { &mut *state_ptr };
                let x = (lparam.0 as i32) as i16 as i32;
                let y = ((lparam.0 >> 16) as i32) as i16 as i32;

                let new_hover = hit_test_settings(x, y);
                if state.hovered_elem != new_hover {
                    state.hovered_elem = new_hover;
                    unsafe {
                        let _ = InvalidateRect(hwnd, None, false);
                    }
                }
            }
            LRESULT(0)
        }

        WM_LBUTTONDOWN => {
            if !state_ptr.is_null() {
                let state = unsafe { &mut *state_ptr };
                let x = (lparam.0 as i32) as i16 as i32;
                let y = ((lparam.0 >> 16) as i32) as i16 as i32;

                if let Some(elem) = hit_test_settings(x, y) {
                    match elem {
                        0..=7 => {
                            // Color preset selected
                            state.settings.default_color = PRESET_COLORS[elem];
                            unsafe {
                                let _ = InvalidateRect(hwnd, None, false);
                            }
                        }
                        10..=12 => {
                            // Thickness preset selected
                            state.settings.default_thickness = PRESET_THICKNESSES[elem - 10];
                            unsafe {
                                let _ = InvalidateRect(hwnd, None, false);
                            }
                        }
                        20 => {
                            // Save button clicked
                            if state.settings.hotkey.vk
                                == windows::Win32::UI::Input::KeyboardAndMouse::VK_SNAPSHOT.0 as u32
                            {
                                let _ = crate::hotkey::disable_windows_snipping_tool_hotkey();
                            }
                            let _ = state.settings.save();
                            state.saved = true;
                            let _ = unsafe { DestroyWindow(hwnd) };
                        }
                        21 => {
                            // Cancel button clicked
                            let _ = unsafe { DestroyWindow(hwnd) };
                        }
                        30 => {
                            // PrintScreen preset selected
                            state.settings.hotkey = HotkeyConfig::default();
                            let _ = crate::hotkey::disable_windows_snipping_tool_hotkey();
                            unsafe {
                                let _ = InvalidateRect(hwnd, None, false);
                            }
                        }
                        31 => {
                            // Ctrl+Shift+S fallback preset selected
                            state.settings.hotkey = HotkeyConfig::fallback();
                            unsafe {
                                let _ = InvalidateRect(hwnd, None, false);
                            }
                        }
                        32 => {
                            // Alt+PrintScreen preset selected
                            state.settings.hotkey = HotkeyConfig::alt_print_screen();
                            let _ = crate::hotkey::disable_windows_snipping_tool_hotkey();
                            unsafe {
                                let _ = InvalidateRect(hwnd, None, false);
                            }
                        }
                        _ => {}
                    }
                }
            }
            LRESULT(0)
        }

        WM_KEYDOWN => {
            if wparam.0 == VK_ESCAPE.0 as usize {
                let _ = unsafe { DestroyWindow(hwnd) };
                return LRESULT(0);
            }
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }

        WM_CLOSE => {
            let _ = unsafe { DestroyWindow(hwnd) };
            LRESULT(0)
        }

        WM_DESTROY => {
            unsafe {
                PostQuitMessage(0);
            }
            LRESULT(0)
        }

        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

fn hit_test_settings(x: i32, y: i32) -> Option<usize> {
    // 1. Color swatches (x: 40 + i * 36, y: 155, w: 26, h: 26)
    for i in 0..8 {
        let cx = 40 + (i as i32) * 36;
        let cy = 155;
        if x >= cx && x <= cx + 26 && y >= cy && y <= cy + 26 {
            return Some(i);
        }
    }

    // 2. Thickness presets (x: 40 + i * 50, y: 225, w: 42, h: 28)
    for i in 0..3 {
        let tx = 40 + (i as i32) * 50;
        let ty = 225;
        if x >= tx && x <= tx + 42 && y >= ty && y <= ty + 28 {
            return Some(10 + i);
        }
    }

    // 3. Save button (x: 240..=350, y: 290..=324)
    if (240..=350).contains(&x) && (290..=324).contains(&y) {
        return Some(20);
    }

    // 4. Cancel button (x: 360..=440, y: 290..=324)
    if (360..=440).contains(&x) && (290..=324).contains(&y) {
        return Some(21);
    }

    // 5. Hotkey presets: PrtScn (30), Ctrl+Shift+S (31), Alt+PrtScn (32)
    if (56..=82).contains(&y) {
        if (140..=220).contains(&x) {
            return Some(30);
        }
        if (228..=330).contains(&x) {
            return Some(31);
        }
        if (338..=440).contains(&x) {
            return Some(32);
        }
    }

    None
}

fn render_settings_ui(hdc: HDC, state: &SettingsWindowState) {
    let width = 480;
    let height = 360;

    // 1. Fill background
    let bg_color = COLORREF(0x00242220); // Dark background
    let border_color = COLORREF(0x0044403C);
    let pen = unsafe { CreatePen(PS_SOLID, 1, bg_color) };
    let brush = unsafe { CreateSolidBrush(bg_color) };
    let op = unsafe { SelectObject(hdc, HGDIOBJ(pen.0)) };
    let ob = unsafe { SelectObject(hdc, HGDIOBJ(brush.0)) };
    let _guard = GdiResourceGuard {
        hdc,
        old_pen: op,
        pen,
        old_brush: ob,
        brush,
    };

    unsafe {
        let _ = RoundRect(hdc, 0, 0, width, height, 0, 0);
        let _ = SetBkMode(hdc, TRANSPARENT);
    }

    // 2. Fonts
    let wide_face: Vec<u16> = "Segoe UI\0".encode_utf16().collect();
    let title_font = unsafe {
        CreateFontW(
            20,
            0,
            0,
            0,
            FW_BOLD.0 as i32,
            0,
            0,
            0,
            DEFAULT_CHARSET.0 as u32,
            CLIP_DEFAULT_PRECIS.0 as u32,
            CLIP_DEFAULT_PRECIS.0 as u32,
            DEFAULT_QUALITY.0 as u32,
            (DEFAULT_PITCH.0 | FF_DONTCARE.0) as u32,
            PCWSTR(wide_face.as_ptr()),
        )
    };
    let normal_font = unsafe {
        CreateFontW(
            14,
            0,
            0,
            0,
            FW_NORMAL.0 as i32,
            0,
            0,
            0,
            DEFAULT_CHARSET.0 as u32,
            CLIP_DEFAULT_PRECIS.0 as u32,
            CLIP_DEFAULT_PRECIS.0 as u32,
            DEFAULT_QUALITY.0 as u32,
            (DEFAULT_PITCH.0 | FF_DONTCARE.0) as u32,
            PCWSTR(wide_face.as_ptr()),
        )
    };
    let bold_font = unsafe {
        CreateFontW(
            13,
            0,
            0,
            0,
            FW_BOLD.0 as i32,
            0,
            0,
            0,
            DEFAULT_CHARSET.0 as u32,
            CLIP_DEFAULT_PRECIS.0 as u32,
            CLIP_DEFAULT_PRECIS.0 as u32,
            DEFAULT_QUALITY.0 as u32,
            (DEFAULT_PITCH.0 | FF_DONTCARE.0) as u32,
            PCWSTR(wide_face.as_ptr()),
        )
    };

    // Header Title
    let old_font = unsafe { SelectObject(hdc, HGDIOBJ(title_font.0)) };
    unsafe {
        let _ = SetTextColor(hdc, COLORREF(0x00FFFFFF));
        let mut title = "isolmaSS Settings\0".encode_utf16().collect::<Vec<u16>>();
        let mut rc = RECT {
            left: 30,
            top: 20,
            right: 400,
            bottom: 50,
        };
        let _ = DrawTextW(hdc, &mut title, &mut rc, DT_LEFT | DT_SINGLELINE);
    }

    // Switch to normal font for labels
    unsafe {
        SelectObject(hdc, HGDIOBJ(normal_font.0));
    }

    // Section 1: Hotkey Presets
    unsafe {
        let _ = SetTextColor(hdc, COLORREF(0x00A09C96));
        let mut label = "Global Hotkey:\0".encode_utf16().collect::<Vec<u16>>();
        let mut rc = RECT {
            left: 30,
            top: 60,
            right: 135,
            bottom: 85,
        };
        let _ = DrawTextW(hdc, &mut label, &mut rc, DT_LEFT | DT_SINGLELINE);
    }

    let hotkey_presets = [
        (30, "PrintScreen", "PrtScn", 140, 220),
        (31, "Ctrl+Shift+S", "Ctrl+Shift+S", 228, 330),
        (32, "Alt+PrintScreen", "Alt+PrtScn", 338, 440),
    ];

    unsafe {
        SelectObject(hdc, HGDIOBJ(bold_font.0));
    }
    for (id, desc, label_str, left, right) in hotkey_presets {
        let is_selected = state.settings.hotkey.description.eq_ignore_ascii_case(desc);
        let is_hovered = state.hovered_elem == Some(id);

        let bg = if is_selected {
            COLORREF(0x00D77800)
        } else if is_hovered {
            COLORREF(0x003A3632)
        } else {
            COLORREF(0x002E2B27)
        };
        let border = if is_selected {
            COLORREF(0x00FA8919)
        } else {
            border_color
        };
        let brush = unsafe { CreateSolidBrush(bg) };
        let pen = unsafe { CreatePen(PS_SOLID, 1, border) };
        let p_old = unsafe { SelectObject(hdc, HGDIOBJ(pen.0)) };
        let b_old = unsafe { SelectObject(hdc, HGDIOBJ(brush.0)) };

        unsafe {
            let _ = RoundRect(hdc, left, 56, right, 82, 4, 4);
            SelectObject(hdc, p_old);
            let _ = DeleteObject(HGDIOBJ(pen.0));
            SelectObject(hdc, b_old);
            let _ = DeleteObject(HGDIOBJ(brush.0));

            let _ = SetTextColor(
                hdc,
                if is_selected {
                    COLORREF(0x00FFFFFF)
                } else {
                    COLORREF(0x00D0CCC8)
                },
            );
            let mut label = format!("{}\0", label_str)
                .encode_utf16()
                .collect::<Vec<u16>>();
            let mut rc = RECT {
                left,
                top: 56,
                right,
                bottom: 82,
            };
            let _ = DrawTextW(hdc, &mut label, &mut rc, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
        }
    }

    // Switch back to normal font
    unsafe {
        SelectObject(hdc, HGDIOBJ(normal_font.0));
    }

    // Section 2: Save Folder
    unsafe {
        let _ = SetTextColor(hdc, COLORREF(0x00A09C96));
        let mut label = "Save Folder:\0".encode_utf16().collect::<Vec<u16>>();
        let mut rc = RECT {
            left: 30,
            top: 92,
            right: 180,
            bottom: 115,
        };
        let _ = DrawTextW(hdc, &mut label, &mut rc, DT_LEFT | DT_SINGLELINE);

        let _ = SetTextColor(hdc, COLORREF(0x00D0CCC8));
        let display_path = state.settings.save_directory.to_string_lossy();
        let mut val = format!("{}\0", display_path)
            .encode_utf16()
            .collect::<Vec<u16>>();
        let mut rc_val = RECT {
            left: 180,
            top: 92,
            right: 450,
            bottom: 115,
        };
        let _ = DrawTextW(hdc, &mut val, &mut rc_val, DT_LEFT | DT_SINGLELINE);
    }

    // Section 3: Default Color
    unsafe {
        let _ = SetTextColor(hdc, COLORREF(0x00A09C96));
        let mut label = "Default Color:\0".encode_utf16().collect::<Vec<u16>>();
        let mut rc = RECT {
            left: 30,
            top: 130,
            right: 440,
            bottom: 150,
        };
        let _ = DrawTextW(hdc, &mut label, &mut rc, DT_LEFT | DT_SINGLELINE);
    }

    // Draw 8 Color Swatches
    for (i, col) in PRESET_COLORS.iter().enumerate() {
        let cx = 40 + (i as i32) * 36;
        let cy = 155;
        let is_selected = state.settings.default_color == *col;
        let is_hovered = state.hovered_elem == Some(i);

        let brush = unsafe { CreateSolidBrush(bgra_to_colorref(*col)) };
        let pen_color = if is_selected {
            COLORREF(0x00FFFFFF)
        } else if is_hovered {
            COLORREF(0x00D77800)
        } else {
            border_color
        };
        let pen = unsafe { CreatePen(PS_SOLID, if is_selected { 2 } else { 1 }, pen_color) };

        let p_old = unsafe { SelectObject(hdc, HGDIOBJ(pen.0)) };
        let b_old = unsafe { SelectObject(hdc, HGDIOBJ(brush.0)) };

        unsafe {
            let _ = RoundRect(hdc, cx, cy, cx + 26, cy + 26, 6, 6);
            SelectObject(hdc, p_old);
            let _ = DeleteObject(HGDIOBJ(pen.0));
            SelectObject(hdc, b_old);
            let _ = DeleteObject(HGDIOBJ(brush.0));
        }
    }

    // Section 4: Default Thickness
    unsafe {
        let _ = SetTextColor(hdc, COLORREF(0x00A09C96));
        let mut label = "Default Thickness:\0".encode_utf16().collect::<Vec<u16>>();
        let mut rc = RECT {
            left: 30,
            top: 200,
            right: 440,
            bottom: 220,
        };
        let _ = DrawTextW(hdc, &mut label, &mut rc, DT_LEFT | DT_SINGLELINE);
    }

    // Draw 3 Thickness buttons
    unsafe {
        SelectObject(hdc, HGDIOBJ(bold_font.0));
    }
    for (i, thick) in PRESET_THICKNESSES.iter().enumerate() {
        let tx = 40 + (i as i32) * 50;
        let ty = 225;
        let is_selected = state.settings.default_thickness == *thick;
        let is_hovered = state.hovered_elem == Some(10 + i);

        let bg = if is_selected {
            COLORREF(0x00D77800)
        } else if is_hovered {
            COLORREF(0x003A3632)
        } else {
            COLORREF(0x002E2B27)
        };
        let brush = unsafe { CreateSolidBrush(bg) };
        let pen = unsafe { CreatePen(PS_SOLID, 1, border_color) };
        let p_old = unsafe { SelectObject(hdc, HGDIOBJ(pen.0)) };
        let b_old = unsafe { SelectObject(hdc, HGDIOBJ(brush.0)) };

        unsafe {
            let _ = RoundRect(hdc, tx, ty, tx + 42, ty + 28, 4, 4);
            SelectObject(hdc, p_old);
            let _ = DeleteObject(HGDIOBJ(pen.0));
            SelectObject(hdc, b_old);
            let _ = DeleteObject(HGDIOBJ(brush.0));

            let _ = SetTextColor(
                hdc,
                if is_selected {
                    COLORREF(0x00FFFFFF)
                } else {
                    COLORREF(0x00D0CCC8)
                },
            );
            let mut label = format!("{}px\0", thick).encode_utf16().collect::<Vec<u16>>();
            let mut rc = RECT {
                left: tx,
                top: ty,
                right: tx + 42,
                bottom: ty + 28,
            };
            let _ = DrawTextW(hdc, &mut label, &mut rc, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
        }
    }

    // Section 5: Save & Cancel Buttons
    // Save button
    let save_hover = state.hovered_elem == Some(20);
    let save_bg = if save_hover {
        COLORREF(0x00FA8919)
    } else {
        COLORREF(0x00D77800)
    };
    let s_brush = unsafe { CreateSolidBrush(save_bg) };
    let s_pen = unsafe { CreatePen(PS_SOLID, 1, save_bg) };
    let po = unsafe { SelectObject(hdc, HGDIOBJ(s_pen.0)) };
    let bo = unsafe { SelectObject(hdc, HGDIOBJ(s_brush.0)) };
    unsafe {
        let _ = RoundRect(hdc, 240, 290, 350, 324, 6, 6);
        SelectObject(hdc, po);
        let _ = DeleteObject(HGDIOBJ(s_pen.0));
        SelectObject(hdc, bo);
        let _ = DeleteObject(HGDIOBJ(s_brush.0));

        let _ = SetTextColor(hdc, COLORREF(0x00FFFFFF));
        let mut label = "Save & Apply\0".encode_utf16().collect::<Vec<u16>>();
        let mut rc = RECT {
            left: 240,
            top: 290,
            right: 350,
            bottom: 324,
        };
        let _ = DrawTextW(hdc, &mut label, &mut rc, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
    }

    // Cancel button
    let cancel_hover = state.hovered_elem == Some(21);
    let cancel_bg = if cancel_hover {
        COLORREF(0x003A3632)
    } else {
        COLORREF(0x002A2825)
    };
    let c_brush = unsafe { CreateSolidBrush(cancel_bg) };
    let c_pen = unsafe { CreatePen(PS_SOLID, 1, border_color) };
    let po = unsafe { SelectObject(hdc, HGDIOBJ(c_pen.0)) };
    let bo = unsafe { SelectObject(hdc, HGDIOBJ(c_brush.0)) };
    unsafe {
        let _ = RoundRect(hdc, 360, 290, 440, 324, 6, 6);
        SelectObject(hdc, po);
        let _ = DeleteObject(HGDIOBJ(c_pen.0));
        SelectObject(hdc, bo);
        let _ = DeleteObject(HGDIOBJ(c_brush.0));

        let _ = SetTextColor(hdc, COLORREF(0x00A09C96));
        let mut label = "Cancel\0".encode_utf16().collect::<Vec<u16>>();
        let mut rc = RECT {
            left: 360,
            top: 290,
            right: 440,
            bottom: 324,
        };
        let _ = DrawTextW(hdc, &mut label, &mut rc, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
    }

    // Cleanup fonts
    unsafe {
        SelectObject(hdc, old_font);
        let _ = DeleteObject(HGDIOBJ(title_font.0));
        let _ = DeleteObject(HGDIOBJ(normal_font.0));
        let _ = DeleteObject(HGDIOBJ(bold_font.0));
    }
}

/// Registers the settings window class once.
fn register_settings_class() -> Result<()> {
    static REGISTERED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if REGISTERED.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return Ok(());
    }

    let wc = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        style: windows::Win32::UI::WindowsAndMessaging::CS_HREDRAW
            | windows::Win32::UI::WindowsAndMessaging::CS_VREDRAW,
        lpfnWndProc: Some(settings_wnd_proc),
        cbClsExtra: 0,
        cbWndExtra: 0,
        hInstance: HINSTANCE::default(),
        hIcon: windows::Win32::UI::WindowsAndMessaging::HICON::default(),
        hCursor: unsafe {
            windows::Win32::UI::WindowsAndMessaging::LoadCursorW(
                HINSTANCE::default(),
                IDC_ARROW,
            )
            .unwrap_or_default()
        },
        hbrBackground: windows::Win32::Graphics::Gdi::HBRUSH(unsafe {
            GetStockObject(NULL_BRUSH).0
        }),
        lpszMenuName: windows::core::PCWSTR::null(),
        lpszClassName: SETTINGS_CLASS_NAME,
        hIconSm: windows::Win32::UI::WindowsAndMessaging::HICON::default(),
    };

    let atom = unsafe { RegisterClassExW(&wc) };
    if atom == 0 {
        return Err(windows::core::Error::from_win32());
    }
    Ok(())
}

/// Displays the native Win32 settings window.
/// Returns `Some(Settings)` if the user clicked "Save & Apply", or `None` if cancelled.
pub fn show_settings_dialog(current: &Settings) -> Result<Option<Settings>> {
    register_settings_class()?;

    let mut state = Box::new(SettingsWindowState {
        settings: current.clone(),
        saved: false,
        hovered_elem: None,
    });

    let width = 480;
    let height = 360;

    let screen_w = unsafe { GetSystemMetrics(SM_CXSCREEN) };
    let screen_h = unsafe { GetSystemMetrics(SM_CYSCREEN) };
    let x = (screen_w - width) / 2;
    let y = (screen_h - height) / 2;

    let hwnd = unsafe {
        CreateWindowExW(
            WS_EX_TOPMOST,
            SETTINGS_CLASS_NAME,
            w!("isolmaSS Settings"),
            WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX,
            x,
            y,
            width,
            height,
            None,
            None,
            HINSTANCE::default(),
            None,
        )?
    };

    unsafe {
        SetWindowLongPtrW(
            hwnd,
            GWLP_USERDATA,
            state.as_mut() as *mut SettingsWindowState as isize,
        );
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = SetForegroundWindow(hwnd);
        let _ = SetFocus(hwnd);
    }

    let mut msg = MSG::default();
    while unsafe { GetMessageW(&mut msg, HWND::default(), 0, 0) }.0 > 0 {
        unsafe {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }

    if state.saved {
        Ok(Some(state.settings))
    } else {
        Ok(None)
    }
}
