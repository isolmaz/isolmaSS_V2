use crate::hotkey::HotkeyConfig;
use crate::settings::{PRESET_COLORS, SaveFormat, Settings};
use crate::theme::ThemePreference;
use std::path::{Path, PathBuf};
use windows::Win32::Foundation::{
    COLORREF, ERROR_CLASS_ALREADY_EXISTS, GetLastError, HINSTANCE, HWND, LPARAM, LRESULT, POINT,
    RECT, WPARAM,
};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, COLOR_WINDOW, CreatePen, CreateSolidBrush, DeleteObject, EndPaint, FillRect,
    GetMonitorInfoW, HBRUSH, HDC, HGDIOBJ, InvalidateRect, MONITOR_DEFAULTTONEAREST, MONITORINFO,
    MonitorFromWindow, PAINTSTRUCT, PS_SOLID, RoundRect, SelectObject, SetBkColor, SetBkMode,
    SetTextColor, TRANSPARENT,
};
use windows::Win32::UI::Controls::{SetScrollInfo, SetScrollPos};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    EnableWindow, IsWindowEnabled, SetActiveWindow, SetFocus, VK_ESCAPE,
};
use windows::Win32::UI::WindowsAndMessaging::{
    BringWindowToTop, CreateWindowExW, DefWindowProcW, DestroyWindow, GWLP_USERDATA, GetClientRect,
    GetDlgCtrlID, GetWindowLongPtrW, GetWindowRect, IDC_ARROW, IsWindow, RegisterClassExW, SW_HIDE,
    SW_SHOW, SetForegroundWindow, SetWindowLongPtrW, ShowWindow, WM_CLOSE, WM_CTLCOLORBTN,
    WM_CTLCOLOREDIT, WM_CTLCOLORSTATIC, WM_DESTROY, WM_DWMCOLORIZATIONCOLORCHANGED, WM_ERASEBKGND,
    WM_KEYDOWN, WM_PAINT, WM_SETTINGCHANGE, WNDCLASSEXW,
};
use windows::core::{PCWSTR, Result, w};

const SETTINGS_CLASS_NAME: PCWSTR = w!("isolmaSS_SettingsClass");
const WM_SETTINGS_RESIZE: u32 = windows::Win32::UI::WindowsAndMessaging::WM_APP + 214;

#[derive(Clone, Copy, PartialEq, Eq)]
enum SettingsView {
    General,
    Editor,
    Updates,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum UpdateStatus {
    Idle,
    Busy,
    Ready,
}

pub struct SettingsWindowState {
    settings: Settings,
    saved: bool,
    active_view: SettingsView,
    update_status: UpdateStatus,
    update_frame: u8,
    user_resized: bool,
    recording_hotkey: bool,
    original_hotkey: HotkeyConfig,
    updating_thickness: bool,
    dpi: u32,
    font: windows::Win32::Graphics::Gdi::HFONT,
    title_font: windows::Win32::Graphics::Gdi::HFONT,
    heading_font: windows::Win32::Graphics::Gdi::HFONT,
    background_brush: HBRUSH,
    card_brush: HBRUSH,
}

impl SettingsWindowState {
    /// Deletes and recreates the cached page/card brushes from the current
    /// theme colors. Called at init and after every theme/accent change;
    /// fonts never change with the theme and are left untouched.
    fn refresh_brushes(&mut self) {
        unsafe {
            for brush in [self.background_brush, self.card_brush] {
                if !brush.is_invalid() {
                    let _ = DeleteObject(HGDIOBJ(brush.0));
                }
            }
            self.background_brush = CreateSolidBrush(color_background());
            self.card_brush = CreateSolidBrush(color_card());
        }
    }
}

impl Drop for SettingsWindowState {
    fn drop(&mut self) {
        for font in [self.font, self.title_font, self.heading_font] {
            if !font.is_invalid() {
                unsafe {
                    let _ = DeleteObject(HGDIOBJ(font.0));
                }
            }
        }
        for brush in [self.background_brush, self.card_brush] {
            if !brush.is_invalid() {
                unsafe {
                    let _ = DeleteObject(HGDIOBJ(brush.0));
                }
            }
        }
    }
}

const ID_SAVE: i32 = 100;
const ID_CANCEL: i32 = 101;
const ID_BROWSE: i32 = 102;
const ID_CHECK_UPDATE: i32 = 103;
const ID_FOLDER_LABEL: i32 = 104;
const ID_VIEW_GENERAL: i32 = 105;
const ID_VIEW_EDITOR: i32 = 106;
const ID_VIEW_UPDATES: i32 = 107;
const ID_UPDATE_STATUS: i32 = 108;
const ID_UPDATE_PROGRESS: i32 = 109;
const ID_HOTKEY_RECORD: i32 = 200;
const ID_COLOR_FIRST: i32 = 300;
const ID_COLOR_CUSTOM: i32 = 308;
const ID_THICKNESS_SLIDER: i32 = 320;
const ID_THICKNESS_EDIT: i32 = 323;
const ID_WINDOW_SNAP: i32 = 400;
const ID_CLOSE_AFTER_ACTION: i32 = 401;
const ID_START_WITH_WINDOWS: i32 = 402;
const ID_NOTIFY_AFTER_SAVE: i32 = 403;
const ID_CHECK_UPDATES: i32 = 404;
const ID_THEME_SYSTEM: i32 = 406;
const ID_THEME_LIGHT: i32 = 407;
const ID_THEME_DARK: i32 = 408;
const ID_DELAY_FIRST: i32 = 500;
const ID_FORMAT_PNG: i32 = 600;
const ID_FORMAT_JPEG: i32 = 601;
const ID_QUALITY_FIRST: i32 = 610;
const SETTINGS_WIDTH: i32 = 660;
const SETTINGS_HEIGHT: i32 = 466;
/// Preserve usable controls when the user resizes the dialog.
const SETTINGS_MIN_WIDTH: i32 = 660;
const SETTINGS_MIN_HEIGHT: i32 = 384;
const CONTENT_LEFT: i32 = 20;
const CONTENT_WIDTH: i32 = 600;
/// Id of the scroll container child window that owns the settings cards.
const ID_SCROLL_CONTAINER: i32 = 900;
/// The scroll container is a real cluster: pages are painted at its origin.
const SCROLL_CLASS_NAME: PCWSTR = w!("isolmaSS_SettingsScrollClass");
fn color_background() -> COLORREF {
    crate::theme::tokens().page
}
fn color_card() -> COLORREF {
    crate::theme::tokens().card
}
fn color_border() -> COLORREF {
    crate::theme::tokens().stroke
}
fn color_text() -> COLORREF {
    crate::theme::tokens().text
}
fn color_property() -> COLORREF {
    crate::theme::tokens().text_secondary
}
fn color_muted() -> COLORREF {
    crate::theme::tokens().text_secondary
}
fn color_accent() -> COLORREF {
    crate::theme::tokens().accent
}
fn color_tint() -> COLORREF {
    crate::theme::tokens().accent_tint
}
fn color_disabled() -> COLORREF {
    crate::theme::tokens().text_disabled
}
fn color_control_fill() -> COLORREF {
    crate::theme::tokens().control_fill
}
fn color_control_hover() -> COLORREF {
    crate::theme::tokens().control_hover
}

fn scale(value: i32, dpi: u32) -> i32 {
    value * dpi as i32 / 96
}

fn wide_string(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}

fn create_control(
    parent: HWND,
    class_name: PCWSTR,
    text: &str,
    style: windows::Win32::UI::WindowsAndMessaging::WINDOW_STYLE,
    id: i32,
) -> Result<HWND> {
    let text = wide_string(text);
    unsafe {
        CreateWindowExW(
            Default::default(),
            class_name,
            PCWSTR(text.as_ptr()),
            windows::Win32::UI::WindowsAndMessaging::WS_CHILD
                | windows::Win32::UI::WindowsAndMessaging::WS_VISIBLE
                | style,
            0,
            0,
            0,
            0,
            parent,
            windows::Win32::UI::WindowsAndMessaging::HMENU(id as *mut std::ffi::c_void),
            HINSTANCE::default(),
            None,
        )
    }
}

fn create_button(parent: HWND, id: i32, text: &str, style: i32) -> Result<HWND> {
    create_control(
        parent,
        w!("BUTTON"),
        text,
        windows::Win32::UI::WindowsAndMessaging::WINDOW_STYLE(style as u32),
        id,
    )
}

fn move_control_with_redraw(
    hwnd: HWND,
    id: i32,
    bounds: (i32, i32, i32, i32),
    dpi: u32,
    redraw: bool,
) {
    let (x, y, width, height) = bounds;
    if let Ok(child) = unsafe { windows::Win32::UI::WindowsAndMessaging::GetDlgItem(hwnd, id) } {
        unsafe {
            let _ = windows::Win32::UI::WindowsAndMessaging::MoveWindow(
                child,
                scale(x, dpi)
                    - windows::Win32::UI::WindowsAndMessaging::GetScrollPos(
                        hwnd,
                        windows::Win32::UI::WindowsAndMessaging::SB_HORZ,
                    ),
                scale(y, dpi)
                    - windows::Win32::UI::WindowsAndMessaging::GetScrollPos(
                        hwnd,
                        windows::Win32::UI::WindowsAndMessaging::SB_VERT,
                    ),
                scale(width, dpi),
                scale(height, dpi),
                redraw,
            );
        }
    }
}

fn move_control(hwnd: HWND, id: i32, x: i32, y: i32, width: i32, height: i32, dpi: u32) {
    move_control_with_redraw(hwnd, id, (x, y, width, height), dpi, true);
}

fn create_font_px(dpi: u32, pixels: i32, weight: i32) -> windows::Win32::Graphics::Gdi::HFONT {
    let face = wide_string(crate::theme::ui_face());
    let pixel_height = pixels * dpi as i32 / 96;
    unsafe {
        windows::Win32::Graphics::Gdi::CreateFontW(
            -pixel_height,
            0,
            0,
            0,
            weight,
            0,
            0,
            0,
            windows::Win32::Graphics::Gdi::DEFAULT_CHARSET.0 as u32,
            windows::Win32::Graphics::Gdi::OUT_DEFAULT_PRECIS.0 as u32,
            windows::Win32::Graphics::Gdi::CLIP_DEFAULT_PRECIS.0 as u32,
            windows::Win32::Graphics::Gdi::CLEARTYPE_QUALITY.0 as u32,
            windows::Win32::Graphics::Gdi::DEFAULT_PITCH.0 as u32,
            PCWSTR(face.as_ptr()),
        )
    }
}

fn create_settings_font(dpi: u32) -> windows::Win32::Graphics::Gdi::HFONT {
    create_font_px(
        dpi,
        crate::theme::FONT_BODY_PX,
        windows::Win32::Graphics::Gdi::FW_NORMAL.0 as i32,
    )
}

fn create_title_font(dpi: u32) -> windows::Win32::Graphics::Gdi::HFONT {
    create_font_px(
        dpi,
        crate::theme::FONT_TITLE_PX,
        crate::theme::FONT_WEIGHT_TITLE,
    )
}

fn create_heading_font(dpi: u32) -> windows::Win32::Graphics::Gdi::HFONT {
    create_font_px(
        dpi,
        crate::theme::FONT_SECTION_PX,
        crate::theme::FONT_WEIGHT_SECTION,
    )
}

fn set_controls_font(hwnd: HWND, font: windows::Win32::Graphics::Gdi::HFONT) {
    for id in 100..=900 {
        if let Some(child) = control(hwnd, id) {
            unsafe {
                windows::Win32::UI::WindowsAndMessaging::SendMessageW(
                    child,
                    windows::Win32::UI::WindowsAndMessaging::WM_SETFONT,
                    WPARAM(font.0 as usize),
                    LPARAM(1),
                );
            }
        }
    }
}

fn set_control_font(hwnd: HWND, id: i32, font: windows::Win32::Graphics::Gdi::HFONT) {
    if let Some(control) = control(hwnd, id) {
        unsafe {
            windows::Win32::UI::WindowsAndMessaging::SendMessageW(
                control,
                windows::Win32::UI::WindowsAndMessaging::WM_SETFONT,
                WPARAM(font.0 as usize),
                LPARAM(1),
            );
        }
    }
}

/// Matching the control grid keeps both columns readable without scrolling at
/// the default size; the scroll container remains for shorter user-resized windows.
fn card_rects(view: SettingsView) -> &'static [(i32, i32, i32, i32)] {
    const W: i32 = CONTENT_WIDTH;
    match view {
        SettingsView::General => &[(0, 0, 294, 132), (306, 0, W, 132), (0, 142, W, 260)],
        SettingsView::Editor => &[(0, 0, W, 116), (0, 128, W, 210)],
        SettingsView::Updates => &[(0, 0, W, 190)],
    }
}

/// Paints the settings window's own surface: the page behind the sidebar,
/// the header and the footer. The cards belong to the scroll container.
fn paint_settings_surface(hwnd: HWND, state: &SettingsWindowState, hdc: HDC) {
    let mut client = RECT::default();
    unsafe {
        let _ = GetClientRect(hwnd, &mut client);
        let _ = FillRect(hdc, &client, state.background_brush);
    }
}

/// The state of the settings window that owns `container`.
///
/// The container is a plain child of this window and never outlives it, so the
/// window data pointer is valid for every container message.
fn scroll_parent_state(container: HWND) -> Option<&'static SettingsWindowState> {
    let parent = unsafe { windows::Win32::UI::WindowsAndMessaging::GetParent(container) }.ok()?;
    let pointer = unsafe { GetWindowLongPtrW(parent, GWLP_USERDATA) } as *const SettingsWindowState;
    unsafe { pointer.as_ref() }
}

/// Paints the scroll container: the page plus the cards of the active view,
/// shifted by the container's own scroll offset so the child controls it also
/// repositions stay aligned with their card.
fn paint_scroll_surface(container: HWND, hdc: HDC) {
    let Some(state) = scroll_parent_state(container) else {
        return;
    };
    let mut client = RECT::default();
    unsafe {
        let _ = GetClientRect(container, &mut client);
        let _ = FillRect(hdc, &client, state.background_brush);
        let _ = windows::Win32::Graphics::Gdi::SetViewportOrgEx(
            hdc,
            -windows::Win32::UI::WindowsAndMessaging::GetScrollPos(
                container,
                windows::Win32::UI::WindowsAndMessaging::SB_HORZ,
            ),
            -windows::Win32::UI::WindowsAndMessaging::GetScrollPos(
                container,
                windows::Win32::UI::WindowsAndMessaging::SB_VERT,
            ),
            None,
        );
    }
    let pen = unsafe { CreatePen(PS_SOLID, scale(1, state.dpi), color_border()) };
    let old_pen = unsafe { SelectObject(hdc, HGDIOBJ(pen.0)) };
    let old_brush = unsafe { SelectObject(hdc, HGDIOBJ(state.card_brush.0)) };
    for &(left, top, right, bottom) in card_rects(state.active_view) {
        unsafe {
            let _ = RoundRect(
                hdc,
                scale(left, state.dpi),
                scale(top, state.dpi),
                scale(right, state.dpi),
                scale(bottom, state.dpi),
                scale(crate::theme::RADIUS_CARD, state.dpi),
                scale(crate::theme::RADIUS_CARD, state.dpi),
            );
        }
    }
    unsafe {
        let _ = SelectObject(hdc, old_brush);
        let _ = SelectObject(hdc, old_pen);
        let _ = DeleteObject(HGDIOBJ(pen.0));
    }
}

/// Registers the scroll container class once per process.
fn register_scroll_class() -> Result<()> {
    static REGISTERED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if REGISTERED.load(std::sync::atomic::Ordering::Acquire) {
        return Ok(());
    }
    let class = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        style: windows::Win32::UI::WindowsAndMessaging::CS_HREDRAW
            | windows::Win32::UI::WindowsAndMessaging::CS_VREDRAW,
        lpfnWndProc: Some(scroll_wnd_proc),
        hInstance: HINSTANCE::default(),
        ..Default::default()
    };
    let class = WNDCLASSEXW {
        lpszClassName: SCROLL_CLASS_NAME,
        ..class
    };
    let atom = unsafe { windows::Win32::UI::WindowsAndMessaging::RegisterClassExW(&class) };
    if atom == 0
        && unsafe { GetLastError() } != windows::Win32::Foundation::ERROR_CLASS_ALREADY_EXISTS
    {
        return Err(windows::core::Error::from_win32());
    }
    REGISTERED.store(true, std::sync::atomic::Ordering::Release);
    Ok(())
}

/// Window procedure for the scroll container.
///
/// The container owns the vertical/horizontal bars, paints the cards and clips
/// its controls to its own rectangle, so scrolled-out content can never draw
/// over the pinned header, sidebar or footer. Controls keep reporting to the
/// settings window: notifications are forwarded unchanged.
unsafe extern "system" fn scroll_wnd_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    use windows::Win32::UI::WindowsAndMessaging::*;
    match message {
        WM_PAINT => {
            let mut paint = PAINTSTRUCT::default();
            let hdc = unsafe { BeginPaint(hwnd, &mut paint) };
            paint_scroll_surface(hwnd, hdc);
            unsafe {
                let _ = EndPaint(hwnd, &paint);
            }
            LRESULT(0)
        }
        WM_ERASEBKGND => {
            if let Some(state) = scroll_parent_state(hwnd) {
                let mut client = RECT::default();
                unsafe {
                    let _ = GetClientRect(hwnd, &mut client);
                    let _ = FillRect(HDC(wparam.0 as *mut _), &client, state.background_brush);
                }
            }
            LRESULT(1)
        }
        WM_SIZE => {
            // The container is stretched by its parent; re-measure the content
            // against the new client area so the bars stay accurate.
            if let Some(state) = scroll_parent_state(hwnd) {
                update_container_scrollbars(hwnd, state.dpi);
            }
            LRESULT(0)
        }
        WM_MOUSEWHEEL => {
            scroll_container_by(hwnd, false, -((wparam.0 >> 16) as u16 as i16 as i32) / 2);
            LRESULT(0)
        }
        WM_VSCROLL | WM_HSCROLL => {
            if lparam.0 != 0 {
                let parent = unsafe { GetParent(hwnd) }.unwrap_or_default();
                return LRESULT(unsafe { SendMessageW(parent, message, wparam, lparam) }.0);
            }
            let horizontal = message == WM_HSCROLL;
            let bar = if horizontal { SB_HORZ } else { SB_VERT };
            let mut info = SCROLLINFO {
                cbSize: std::mem::size_of::<SCROLLINFO>() as u32,
                fMask: SIF_ALL,
                ..Default::default()
            };
            unsafe {
                let _ = GetScrollInfo(hwnd, bar, &mut info);
            }
            let delta = match wparam.0 as u16 as i32 {
                0 => -40,
                1 => 40,
                2 => -(info.nPage as i32),
                3 => info.nPage as i32,
                4 | 5 => info.nTrackPos - info.nPos,
                _ => 0,
            };
            scroll_container_by(hwnd, horizontal, delta);
            LRESULT(0)
        }
        WM_COMMAND | WM_NOTIFY | WM_CTLCOLORSTATIC | WM_CTLCOLORBTN | WM_CTLCOLOREDIT => {
            let parent = unsafe { windows::Win32::UI::WindowsAndMessaging::GetParent(hwnd) }
                .unwrap_or_default();
            LRESULT(unsafe { SendMessageW(parent, message, wparam, lparam) }.0)
        }
        _ => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
    }
}

fn draw_settings_slider(
    draw: &windows::Win32::UI::Controls::NMCUSTOMDRAW,
    state: &SettingsWindowState,
) {
    let mut rect = RECT::default();
    unsafe {
        let _ = GetClientRect(draw.hdr.hwndFrom, &mut rect);
    }
    unsafe {
        let _ = FillRect(draw.hdc, &rect, state.card_brush);
    }
    let left = scale(10, state.dpi);
    let right = rect.right - left;
    let center = (rect.top + rect.bottom) / 2;
    let value = unsafe {
        windows::Win32::UI::WindowsAndMessaging::SendMessageW(
            draw.hdr.hwndFrom,
            windows::Win32::UI::WindowsAndMessaging::WM_USER,
            WPARAM(0),
            LPARAM(0),
        )
        .0 as i32
    }
    .clamp(1, 64);
    let knob = left + (right - left).max(0) * (value - 1) / 63;
    let track = crate::capture::Rect::new(left, center - 2, right + 1, center + 3);
    crate::drawing::rounded(draw.hdc, track, 4, color_border(), color_border());
    if knob > left {
        crate::drawing::rounded(
            draw.hdc,
            crate::capture::Rect::new(left, center - 2, knob + 1, center + 3),
            4,
            color_accent(),
            color_accent(),
        );
    }
    let radius = scale(8, state.dpi);
    crate::drawing::rounded(
        draw.hdc,
        crate::capture::Rect::new(
            knob - radius,
            center - radius,
            knob + radius + 1,
            center + radius + 1,
        ),
        radius * 2,
        color_accent(),
        if draw
            .uItemState
            .contains(windows::Win32::UI::Controls::CDIS_FOCUS)
        {
            color_text()
        } else {
            color_accent()
        },
    );
}

fn draw_update_progress(
    draw: &windows::Win32::UI::Controls::NMCUSTOMDRAW,
    state: &SettingsWindowState,
) {
    let mut rect = RECT::default();
    unsafe {
        let _ = GetClientRect(draw.hdr.hwndFrom, &mut rect);
        let _ = FillRect(draw.hdc, &rect, state.card_brush);
    }
    let width = rect.right - rect.left;
    if width <= 2 {
        return;
    }
    let top = scale(2, state.dpi);
    let bottom = scale(6, state.dpi);
    let track = crate::capture::Rect::new(1, top, width - 1, bottom);
    crate::drawing::rounded(
        draw.hdc,
        track,
        scale(4, state.dpi),
        color_border(),
        color_border(),
    );
    let segment = (width / 6).max(scale(32, state.dpi)).min(width - 2);
    let travel = width - 2 - segment;
    let phase = i32::from(state.update_frame);
    let offset = if phase < 18 {
        travel * phase / 18
    } else {
        travel * (36 - phase) / 18
    };
    crate::drawing::rounded(
        draw.hdc,
        crate::capture::Rect::new(1 + offset, top, 1 + offset + segment, bottom),
        scale(4, state.dpi),
        color_accent(),
        color_accent(),
    );
}

fn draw_settings_button(
    draw: &windows::Win32::UI::Controls::NMCUSTOMDRAW,
    state: &SettingsWindowState,
) {
    use windows::Win32::Graphics::Gdi::*;
    use windows::Win32::UI::Controls::*;
    use windows::Win32::UI::WindowsAndMessaging::{BM_GETCHECK, GetWindowTextW, SendMessageW};
    let hdc = draw.hdc;
    let id = draw.hdr.idFrom as i32;
    let dpi = state.dpi;
    let checked = unsafe { SendMessageW(draw.hdr.hwndFrom, BM_GETCHECK, WPARAM(0), LPARAM(0)).0 }
        == BST_CHECKED.0 as isize;
    let primary = id == ID_SAVE;
    let toggle = (ID_WINDOW_SNAP..=ID_CHECK_UPDATES).contains(&id);
    // Color presets are swatch circles: the square chip stays invisible so the
    // circle carries the state, and a ring marks the selected swatch.
    let swatch = (ID_COLOR_FIRST..ID_COLOR_FIRST + 8).contains(&id);
    let disabled = draw.uItemState.contains(CDIS_DISABLED);
    let hot = draw.uItemState.contains(CDIS_HOT) || draw.uItemState.contains(CDIS_SELECTED);
    let fill = if swatch {
        if hot {
            color_control_hover()
        } else {
            color_card()
        }
    } else if primary {
        color_accent()
    } else if checked && !toggle {
        color_tint()
    } else if hot {
        color_control_hover()
    } else if toggle {
        color_card()
    } else {
        color_control_fill()
    };
    let border = if swatch {
        fill
    } else if draw.uItemState.contains(CDIS_FOCUS) {
        if primary {
            // An accent ring would blend into an accent-filled control;
            // ring with the on-accent color so focus stays visible.
            crate::theme::tokens().accent_text
        } else {
            color_accent()
        }
    } else if checked && !toggle {
        color_accent()
    } else if toggle {
        color_card()
    } else {
        color_border()
    };
    let mut rect = RECT::default();
    unsafe {
        let _ = GetClientRect(draw.hdr.hwndFrom, &mut rect);
        let _ = FillRect(
            hdc,
            &rect,
            if control_uses_card(id) {
                state.card_brush
            } else {
                state.background_brush
            },
        );
    }
    crate::drawing::with_brush(hdc, fill, || {
        crate::drawing::with_pen(hdc, PS_SOLID, scale(1, dpi).max(1), border, || unsafe {
            let _ = RoundRect(
                hdc,
                rect.left + 1,
                rect.top + 1,
                rect.right - 1,
                rect.bottom - 1,
                scale(crate::theme::RADIUS_CARD, dpi),
                scale(crate::theme::RADIUS_CARD, dpi),
            );
        })
    });
    let mut label = [0u16; 256];
    let length = unsafe { GetWindowTextW(draw.hdr.hwndFrom, &mut label) };
    let mut text_rect = rect;
    text_rect.left += scale(10, dpi);
    text_rect.right -= scale(10, dpi);
    if swatch {
        let source = PRESET_COLORS[(id - ID_COLOR_FIRST) as usize];
        let color = crate::annotation::bgra_to_colorref(source);
        let diameter = scale(16, dpi);
        let x = (rect.left + rect.right - diameter) / 2;
        let y = (rect.top + rect.bottom - diameter) / 2;
        if checked {
            let ring = scale(5, dpi);
            crate::drawing::with_brush(hdc, color_accent(), || {
                crate::drawing::with_pen(
                    hdc,
                    PS_SOLID,
                    scale(1, dpi).max(1),
                    color_accent(),
                    || unsafe {
                        let _ = Ellipse(
                            hdc,
                            x - ring,
                            y - ring,
                            x + diameter + ring,
                            y + diameter + ring,
                        );
                    },
                )
            });
        }
        crate::drawing::with_brush(hdc, color, || {
            crate::drawing::with_pen(
                hdc,
                PS_SOLID,
                scale(1, dpi).max(1),
                color_border(),
                || unsafe {
                    let _ = Ellipse(hdc, x, y, x + diameter, y + diameter);
                },
            )
        });
        if checked {
            // Ink against the swatch color itself, so this pair stays
            // theme-independent like the toolbar swatches.
            let luma = source[0] as u32 + source[1] as u32 + source[2] as u32;
            let check = if luma > 450 {
                COLORREF(0x2a211b)
            } else {
                COLORREF(0xffffff)
            };
            crate::drawing::icon(
                hdc,
                crate::capture::Rect::new(x, y, x + diameter, y + diameter),
                0xe73e,
                scale(11, dpi),
                check,
                true,
            );
        }
        if draw.uItemState.contains(CDIS_FOCUS) {
            let focus = RECT {
                left: rect.left + 2,
                top: rect.top + 2,
                right: rect.right - 2,
                bottom: rect.bottom - 2,
            };
            unsafe {
                let _ = DrawFocusRect(hdc, &focus);
            }
        }
        return;
    }
    if toggle {
        // A larger, DPI-scaled track and thumb retain a clear outline at 100–200%.
        let track_width = scale(44, dpi);
        let track_height = scale(24, dpi);
        let x = rect.right - track_width - scale(10, dpi);
        let y = (rect.top + rect.bottom - track_height) / 2;
        let track = if checked {
            color_accent()
        } else {
            color_control_fill()
        };
        crate::drawing::with_brush(hdc, track, || {
            crate::drawing::with_pen(
                hdc,
                PS_SOLID,
                scale(1, dpi).max(1),
                if checked {
                    color_accent()
                } else {
                    color_border()
                },
                || unsafe {
                    let _ = RoundRect(
                        hdc,
                        x,
                        y,
                        x + track_width,
                        y + track_height,
                        track_height,
                        track_height,
                    );
                },
            )
        });
        let thumb_size = scale(18, dpi);
        let thumb = if checked {
            x + track_width - thumb_size - scale(3, dpi)
        } else {
            x + scale(3, dpi)
        };
        let thumb_top = y + (track_height - thumb_size) / 2;
        let thumb_color = if checked { color_card() } else { color_muted() };
        crate::drawing::with_brush(hdc, thumb_color, || {
            crate::drawing::with_pen(hdc, PS_SOLID, 1, thumb_color, || unsafe {
                let _ = Ellipse(
                    hdc,
                    thumb,
                    thumb_top,
                    thumb + thumb_size,
                    thumb_top + thumb_size,
                );
            })
        });
        text_rect.right = x - scale(10, dpi);
    }
    crate::drawing::with_font(
        hdc,
        -scale(crate::theme::FONT_BODY_PX, dpi),
        if primary || checked && !toggle {
            600
        } else {
            400
        },
        || unsafe {
            let _ = SetBkMode(hdc, TRANSPARENT);
            let _ = SetTextColor(
                hdc,
                if disabled {
                    color_disabled()
                } else if primary {
                    crate::theme::tokens().accent_text
                } else {
                    color_text()
                },
            );
            let flags = DT_VCENTER
                | DT_SINGLELINE
                | if toggle || (300..308).contains(&id) {
                    DT_LEFT
                } else {
                    DT_CENTER
                };
            let _ = DrawTextW(
                hdc,
                &mut label[..length.max(0) as usize],
                &mut text_rect,
                flags,
            );
        },
    );
}

fn control_uses_card(id: i32) -> bool {
    !matches!(
        id,
        700 | 718 | ID_VIEW_GENERAL | ID_VIEW_EDITOR | ID_VIEW_UPDATES | ID_SAVE | ID_CANCEL
    )
}

/// Brand and tabs occupy one pinned band; content begins directly below.
const HEADER_BOTTOM: i32 = 88;
const FOOTER_HEIGHT: i32 = 46;
const FOOTER_GAP: i32 = 8;
const LABEL_WIDTH: i32 = 78;

/// Design-space size of a window's client area (96-DPI pixels).
fn client_design_size(hwnd: HWND, dpi: u32) -> (i32, i32) {
    let mut client = RECT::default();
    unsafe {
        let _ = GetClientRect(hwnd, &mut client);
    }
    (
        client.right * 96 / dpi as i32,
        client.bottom * 96 / dpi as i32,
    )
}

/// Places the pinned chrome: sidebar, header and footer. None of it scrolls,
/// so navigation and the Save/Cancel band stay put at every window size.
fn layout_chrome(hwnd: HWND, dpi: u32) {
    use crate::theme::{CONTROL_HEIGHT, PAGE_MARGIN};
    let (width, height) = client_design_size(hwnd, dpi);
    let right = (width - PAGE_MARGIN).max(CONTENT_LEFT + 160);
    move_control(hwnd, 700, CONTENT_LEFT, 12, 150, 24, dpi);
    move_control(
        hwnd,
        ID_VIEW_GENERAL,
        CONTENT_LEFT,
        44,
        104,
        CONTROL_HEIGHT,
        dpi,
    );
    move_control(
        hwnd,
        ID_VIEW_EDITOR,
        CONTENT_LEFT + 112,
        44,
        124,
        CONTROL_HEIGHT,
        dpi,
    );
    move_control(
        hwnd,
        ID_VIEW_UPDATES,
        CONTENT_LEFT + 244,
        44,
        138,
        CONTROL_HEIGHT,
        dpi,
    );
    move_control(
        hwnd,
        718,
        CONTENT_LEFT,
        height - PAGE_MARGIN - 32,
        260,
        32,
        dpi,
    );
    let footer_y = height - PAGE_MARGIN - CONTROL_HEIGHT;
    move_control(
        hwnd,
        ID_SAVE,
        right - 108,
        footer_y,
        108,
        CONTROL_HEIGHT,
        dpi,
    );
    move_control(
        hwnd,
        ID_CANCEL,
        right - 200,
        footer_y,
        84,
        CONTROL_HEIGHT,
        dpi,
    );
}

/// Stretches the scroll container over the content column between the header
/// and the footer band.
fn position_container(hwnd: HWND, dpi: u32) {
    let Ok(container) =
        (unsafe { windows::Win32::UI::WindowsAndMessaging::GetDlgItem(hwnd, ID_SCROLL_CONTAINER) })
    else {
        return;
    };
    let (width, height) = client_design_size(hwnd, dpi);
    let left = CONTENT_LEFT;
    // The container reaches the right edge so its vertical scrollbar stays
    // outside the fixed-width cards, even at the minimum window size.
    let right = width.max(left + 160);
    let top = HEADER_BOTTOM;
    let bottom = (height - FOOTER_HEIGHT - FOOTER_GAP).max(top + crate::theme::CONTROL_HEIGHT);
    unsafe {
        let _ = windows::Win32::UI::WindowsAndMessaging::MoveWindow(
            container,
            scale(left, dpi),
            scale(top, dpi),
            scale(right - left, dpi),
            scale(bottom - top, dpi),
            true,
        );
    }
}

/// Lays out the controls inside the scroll container in container design
/// pixels: x starts at the card column's left edge, y at the top of the
/// scrollable content. `move_control` subtracts the container's scroll
/// offset, so a control always sits on the card it belongs to.
fn layout_content(container: HWND, dpi: u32) {
    use crate::theme::{CARD_PADDING, CONTROL_HEIGHT};
    let pad = CARD_PADDING;
    let row = CONTROL_HEIGHT;
    let label = LABEL_WIDTH;
    let chips = pad + label + 8;
    let full = CONTENT_WIDTH - 2 * pad;
    let place = |id: i32, x: i32, y: i32, w: i32, h: i32| {
        move_control_with_redraw(container, id, (x, y, w, h), dpi, false)
    };
    let heading = |id: i32, top: i32| place(id, pad, top, full, 16);
    let row_label = |id: i32, top: i32| place(id, pad, top + 5, label, 16);

    heading(710, 12);
    row_label(701, 32);
    place(ID_HOTKEY_RECORD, chips, 32, 184, row);
    row_label(705, 66);
    for index in 0..4 {
        place(ID_DELAY_FIRST + index, chips + index * 46, 66, 43, row);
    }

    place(711, 318, 12, 270, 16);
    place(ID_FOLDER_LABEL, 318, 36, 174, 18);
    place(ID_BROWSE, 500, 32, 88, row);
    place(706, 318, 69, label, 16);
    place(ID_FORMAT_PNG, 398, 64, 66, row);
    place(ID_FORMAT_JPEG, 470, 64, 66, row);
    place(707, 318, 101, 84, 16);
    for index in 0..3 {
        place(ID_QUALITY_FIRST + index, 408 + index * 58, 96, 54, row);
    }

    heading(712, 154);
    place(719, 318, 154, 270, 16);
    place(ID_START_WITH_WINDOWS, pad, 178, 274, row);
    place(ID_NOTIFY_AFTER_SAVE, pad, 210, 274, row);
    for (index, id) in [ID_THEME_SYSTEM, ID_THEME_LIGHT, ID_THEME_DARK]
        .into_iter()
        .enumerate()
    {
        place(id, 318 + index as i32 * 86, 178, 78, row);
    }

    heading(713, 12);
    row_label(703, 34);
    for index in 0..8 {
        place(ID_COLOR_FIRST + index, chips + index * 44, 34, 42, row);
    }
    place(ID_COLOR_CUSTOM, chips + 358, 34, 110, row);
    row_label(704, 70);
    place(ID_THICKNESS_SLIDER, chips, 70, 278, row);
    place(ID_THICKNESS_EDIT, chips + 290, 70, 52, row);
    place(720, chips + 346, 74, 26, 18);
    heading(714, 140);
    place(ID_WINDOW_SNAP, pad, 166, 274, row);
    place(ID_CLOSE_AFTER_ACTION, 318, 166, 270, row);

    heading(715, 12);
    place(ID_CHECK_UPDATES, pad, 34, full, row);
    place(ID_CHECK_UPDATE, pad, 74, 164, row);
    place(ID_UPDATE_STATUS, 186, 76, 402, 24);
    place(ID_UPDATE_PROGRESS, pad, 112, full, 8);
    place(708, pad, 138, full, 40);
}

/// Sizes the container's scroll bars against the content extent and refreshes
/// its contents. Both bars are the container's own, so the window frame never
/// shows a scroll bar over the pinned chrome.
fn update_container_scrollbars(container: HWND, dpi: u32) {
    use windows::Win32::UI::WindowsAndMessaging::*;
    thread_local! { static UPDATING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) }; }
    if UPDATING.with(|flag| flag.replace(true)) {
        return;
    }
    // Measure the window itself: `GetClientRect` already excludes any visible
    // bar, while the WS_VSCROLL/WS_HSCROLL style bits stay set even when a bar
    // is hidden, which would overstate the page by one bar for one pass.
    let mut window = RECT::default();
    unsafe {
        let _ = GetWindowRect(container, &mut window);
    }
    let full_width = window.right - window.left;
    let full_height = window.bottom - window.top;
    let bar_width = unsafe { GetSystemMetrics(SM_CXVSCROLL) };
    let bar_height = unsafe { GetSystemMetrics(SM_CYHSCROLL) };
    let width = scale(CONTENT_WIDTH + crate::theme::GRID, dpi);
    let view =
        scroll_parent_state(container).map_or(SettingsView::General, |state| state.active_view);
    let height = scale(
        card_rects(view).last().map_or(0, |rect| rect.3) + crate::theme::CARD_PADDING,
        dpi,
    );
    let mut horizontal = width > full_width;
    let mut vertical = height > full_height;
    for _ in 0..2 {
        horizontal = width > full_width - if vertical { bar_width } else { 0 };
        vertical = height > full_height - if horizontal { bar_height } else { 0 };
    }
    let page_width = full_width - if vertical { bar_width } else { 0 };
    let page_height = full_height - if horizontal { bar_height } else { 0 };
    for (bar, length, page) in [(SB_HORZ, width, page_width), (SB_VERT, height, page_height)] {
        let info = SCROLLINFO {
            cbSize: std::mem::size_of::<SCROLLINFO>() as u32,
            fMask: SIF_RANGE | SIF_PAGE | SIF_POS,
            nMin: 0,
            nMax: length - 1,
            nPage: page.max(1) as u32,
            nPos: unsafe { GetScrollPos(container, bar) }.clamp(0, (length - page).max(0)),
            nTrackPos: 0,
        };
        unsafe {
            SetScrollInfo(container, bar, &info, true);
        }
    }
    layout_content(container, dpi);
    unsafe {
        let _ = windows::Win32::Graphics::Gdi::RedrawWindow(
            container,
            None,
            None,
            windows::Win32::Graphics::Gdi::RDW_INVALIDATE
                | windows::Win32::Graphics::Gdi::RDW_ERASE
                | windows::Win32::Graphics::Gdi::RDW_ALLCHILDREN
                | windows::Win32::Graphics::Gdi::RDW_UPDATENOW,
        );
    }
    UPDATING.with(|flag| flag.set(false));
}

/// Scrolls the container and moves its controls by the same offset, so the
/// cards and the controls on them can never drift apart.
fn scroll_container_by(container: HWND, horizontal: bool, delta: i32) {
    use windows::Win32::UI::WindowsAndMessaging::*;
    let bar = if horizontal { SB_HORZ } else { SB_VERT };
    let mut info = SCROLLINFO {
        cbSize: std::mem::size_of::<SCROLLINFO>() as u32,
        fMask: SIF_ALL,
        ..Default::default()
    };
    unsafe {
        let _ = GetScrollInfo(container, bar, &mut info);
    }
    let position = (info.nPos + delta).clamp(0, (info.nMax - info.nPage as i32 + 1).max(0));
    if position == info.nPos {
        return;
    }
    unsafe {
        SetScrollPos(container, bar, position, true);
    }
    let dpi = unsafe { windows::Win32::UI::HiDpi::GetDpiForWindow(container) }.max(96);
    layout_content(container, dpi);
    unsafe {
        let _ = windows::Win32::Graphics::Gdi::RedrawWindow(
            container,
            None,
            None,
            windows::Win32::Graphics::Gdi::RDW_INVALIDATE
                | windows::Win32::Graphics::Gdi::RDW_ERASE
                | windows::Win32::Graphics::Gdi::RDW_ALLCHILDREN
                | windows::Win32::Graphics::Gdi::RDW_UPDATENOW,
        );
    }
}

/// Brings the focused control into view by scrolling its container. Chrome
/// controls (sidebar, header, footer) never need scrolling.
pub fn ensure_focus_visible(hwnd: HWND) {
    use windows::Win32::UI::WindowsAndMessaging::{GetDlgItem, IsChild};
    let focus = unsafe { windows::Win32::UI::Input::KeyboardAndMouse::GetFocus() };
    let Ok(container) = (unsafe { GetDlgItem(hwnd, ID_SCROLL_CONTAINER) }) else {
        return;
    };
    if !unsafe { IsChild(container, focus).as_bool() } {
        return;
    }
    let mut rect = RECT::default();
    let mut client = RECT::default();
    unsafe {
        let _ = GetWindowRect(focus, &mut rect);
        let _ = GetClientRect(container, &mut client);
    }
    let mut point = POINT {
        x: rect.left,
        y: rect.top,
    };
    unsafe {
        let _ = windows::Win32::Graphics::Gdi::ScreenToClient(container, &mut point);
    }
    let bottom = point.y + rect.bottom - rect.top;
    let right = point.x + rect.right - rect.left;
    if point.y < 0 {
        scroll_container_by(container, false, point.y - 8);
    } else if bottom > client.bottom {
        scroll_container_by(container, false, bottom - client.bottom + 8);
    }
    if point.x < 0 {
        scroll_container_by(container, true, point.x - 8);
    } else if right > client.right {
        scroll_container_by(container, true, right - client.right + 8);
    }
}

/// Finds a settings control by id: content controls live inside the scroll
/// container, chrome controls are direct children of the window.
fn control(hwnd: HWND, id: i32) -> Option<HWND> {
    use windows::Win32::UI::WindowsAndMessaging::GetDlgItem;
    if let Ok(child) = unsafe { GetDlgItem(hwnd, id) } {
        return Some(child);
    }
    let container = unsafe { GetDlgItem(hwnd, ID_SCROLL_CONTAINER) }.ok()?;
    unsafe { GetDlgItem(container, id) }.ok()
}

pub(crate) fn report_update_status(owner: HWND, message: &str, status: UpdateStatus) -> bool {
    if control(owner, ID_UPDATE_STATUS).is_none() {
        return false;
    }
    let state_ptr = unsafe { GetWindowLongPtrW(owner, GWLP_USERDATA) } as *mut SettingsWindowState;
    let Some(state) = (unsafe { state_ptr.as_mut() }) else {
        return false;
    };
    set_update_status(owner, state, message, status);
    true
}

fn set_update_status(
    owner: HWND,
    state: &mut SettingsWindowState,
    message: &str,
    status: UpdateStatus,
) {
    let was_busy = state.update_status == UpdateStatus::Busy;
    state.update_status = status;
    if status == UpdateStatus::Busy && !was_busy {
        state.update_frame = 0;
        if unsafe { windows::Win32::UI::WindowsAndMessaging::SetTimer(owner, 2, 75, None) } == 0 {
            crate::diagnostics::record(
                "settings update progress",
                "Could not start progress animation.",
            );
        }
    } else if status != UpdateStatus::Busy && was_busy {
        unsafe {
            let _ = windows::Win32::UI::WindowsAndMessaging::KillTimer(owner, 2);
        }
    }
    let Some(label) = control(owner, ID_UPDATE_STATUS) else {
        return;
    };
    let text = wide_string(message);
    unsafe {
        let _ =
            windows::Win32::UI::WindowsAndMessaging::SetWindowTextW(label, PCWSTR(text.as_ptr()));
        if let Some(button) = control(owner, ID_CHECK_UPDATE) {
            let _ = EnableWindow(button, status == UpdateStatus::Idle);
        }
        if let Some(progress) = control(owner, ID_UPDATE_PROGRESS) {
            let _ = ShowWindow(
                progress,
                if status == UpdateStatus::Busy && state.active_view == SettingsView::Updates {
                    SW_SHOW
                } else {
                    SW_HIDE
                },
            );
        }
    }
}

fn show_controls(hwnd: HWND, ids: &[i32], show: bool) {
    for id in ids {
        if let Some(control) = control(hwnd, *id) {
            unsafe {
                let _ = ShowWindow(control, if show { SW_SHOW } else { SW_HIDE });
            }
        }
    }
}

fn set_active_view(hwnd: HWND, view: SettingsView, update_status: UpdateStatus) {
    const GENERAL: &[i32] = &[
        710,
        711,
        712,
        701,
        705,
        706,
        707,
        719,
        ID_FOLDER_LABEL,
        ID_BROWSE,
        ID_HOTKEY_RECORD,
        ID_FORMAT_PNG,
        ID_FORMAT_JPEG,
        ID_START_WITH_WINDOWS,
        ID_NOTIFY_AFTER_SAVE,
        ID_THEME_SYSTEM,
        ID_THEME_LIGHT,
        ID_THEME_DARK,
    ];
    const EDITOR: &[i32] = &[
        713,
        714,
        703,
        704,
        720,
        ID_COLOR_CUSTOM,
        ID_THICKNESS_SLIDER,
        ID_THICKNESS_EDIT,
        ID_WINDOW_SNAP,
        ID_CLOSE_AFTER_ACTION,
    ];
    const UPDATES: &[i32] = &[
        715,
        708,
        ID_CHECK_UPDATES,
        ID_CHECK_UPDATE,
        ID_UPDATE_STATUS,
        ID_UPDATE_PROGRESS,
    ];
    for (controls, section) in [
        (GENERAL, SettingsView::General),
        (EDITOR, SettingsView::Editor),
        (UPDATES, SettingsView::Updates),
    ] {
        show_controls(hwnd, controls, view == section);
    }
    for id in ID_DELAY_FIRST..=ID_DELAY_FIRST + 3 {
        show_controls(hwnd, &[id], view == SettingsView::General);
    }
    for id in ID_QUALITY_FIRST..=ID_QUALITY_FIRST + 2 {
        show_controls(hwnd, &[id], view == SettingsView::General);
    }
    for id in ID_COLOR_FIRST..ID_COLOR_FIRST + PRESET_COLORS.len() as i32 {
        show_controls(hwnd, &[id], view == SettingsView::Editor);
    }
    show_controls(
        hwnd,
        &[ID_UPDATE_PROGRESS],
        view == SettingsView::Updates && update_status == UpdateStatus::Busy,
    );
    check_radio(
        hwnd,
        ID_VIEW_GENERAL,
        ID_VIEW_UPDATES,
        match view {
            SettingsView::General => ID_VIEW_GENERAL,
            SettingsView::Editor => ID_VIEW_EDITOR,
            SettingsView::Updates => ID_VIEW_UPDATES,
        },
    );
    if let Some(container) = control(hwnd, ID_SCROLL_CONTAINER) {
        unsafe {
            for bar in [
                windows::Win32::UI::WindowsAndMessaging::SB_HORZ,
                windows::Win32::UI::WindowsAndMessaging::SB_VERT,
            ] {
                windows::Win32::UI::Controls::SetScrollPos(container, bar, 0, true);
            }
        }
        let dpi = unsafe { windows::Win32::UI::HiDpi::GetDpiForWindow(container) }.max(96);
        update_container_scrollbars(container, dpi);
    }
    unsafe {
        let _ = InvalidateRect(hwnd, None, true);
    }
}

fn set_check(hwnd: HWND, id: i32, checked: bool) {
    if let Some(control) = control(hwnd, id) {
        unsafe {
            let _ = windows::Win32::UI::WindowsAndMessaging::SendMessageW(
                control,
                windows::Win32::UI::WindowsAndMessaging::BM_SETCHECK,
                WPARAM(if checked {
                    windows::Win32::UI::Controls::BST_CHECKED.0 as usize
                } else {
                    windows::Win32::UI::Controls::BST_UNCHECKED.0 as usize
                }),
                LPARAM(0),
            );
        }
    }
}

fn check_radio(hwnd: HWND, first: i32, last: i32, selected: i32) {
    for id in first..=last {
        set_check(hwnd, id, id == selected);
    }
}

/// Checks `selected` when it is one of the group's presets; otherwise clears the whole
/// group so a custom value never displays as a preset the user did not pick.
fn set_radio_group(hwnd: HWND, first: i32, last: i32, selected: Option<i32>) {
    match selected {
        Some(id) => check_radio(hwnd, first, last, id),
        None => {
            for id in first..=last {
                set_check(hwnd, id, false);
            }
        }
    }
}

/// Shows non-preset values in the fixed-width property label without clipping.
fn set_label_note(hwnd: HWND, id: i32, base: &str, note: Option<String>) {
    let text = match note {
        Some(note) if id == 705 => format!("Delay: {note}"),
        Some(note) if id == 707 => format!("JPEG: {note}"),
        Some(note) => format!("{base}: {note}"),
        None => base.to_owned(),
    };
    let text = wide_string(&text);
    if let Some(child) = control(hwnd, id) {
        unsafe {
            let _ = windows::Win32::UI::WindowsAndMessaging::SetWindowTextW(
                child,
                PCWSTR(text.as_ptr()),
            );
        }
    }
}

fn set_jpeg_quality_enabled(hwnd: HWND, enabled: bool) {
    for id in std::iter::once(707).chain(ID_QUALITY_FIRST..=ID_QUALITY_FIRST + 2) {
        if let Some(control) = control(hwnd, id) {
            unsafe {
                let _ = EnableWindow(control, enabled);
            }
        }
    }
}

fn initialize_control_values(hwnd: HWND, settings: &Settings) {
    let shortcut = wide_string(&format!("{}  ·  Change", settings.hotkey.description));
    if let Some(button) = control(hwnd, ID_HOTKEY_RECORD) {
        unsafe {
            let _ = windows::Win32::UI::WindowsAndMessaging::SetWindowTextW(
                button,
                PCWSTR(shortcut.as_ptr()),
            );
        }
    }
    let color_id = PRESET_COLORS
        .iter()
        .position(|color| *color == settings.default_color)
        .map(|index| ID_COLOR_FIRST + index as i32);
    set_radio_group(
        hwnd,
        ID_COLOR_FIRST,
        ID_COLOR_FIRST + PRESET_COLORS.len() as i32 - 1,
        color_id,
    );
    if let Some(slider) = control(hwnd, ID_THICKNESS_SLIDER) {
        unsafe {
            windows::Win32::UI::WindowsAndMessaging::SendMessageW(
                slider,
                windows::Win32::UI::Controls::TBM_SETPOS,
                WPARAM(1),
                LPARAM(settings.default_thickness as isize),
            );
        }
    }
    let thickness = wide_string(&settings.default_thickness.to_string());
    if let Some(edit) = control(hwnd, ID_THICKNESS_EDIT) {
        unsafe {
            let _ = windows::Win32::UI::WindowsAndMessaging::SetWindowTextW(
                edit,
                PCWSTR(thickness.as_ptr()),
            );
        }
    }
    let delay_id = [0, 1000, 3000, 5000]
        .iter()
        .position(|&value| value == settings.capture_delay_ms)
        .map(|index| ID_DELAY_FIRST + index as i32);
    set_radio_group(hwnd, ID_DELAY_FIRST, ID_DELAY_FIRST + 3, delay_id);
    set_label_note(
        hwnd,
        705,
        "Capture delay",
        delay_id
            .is_none()
            .then(|| format!("{} ms", settings.capture_delay_ms)),
    );
    check_radio(
        hwnd,
        ID_FORMAT_PNG,
        ID_FORMAT_JPEG,
        if settings.save_format == SaveFormat::Png {
            ID_FORMAT_PNG
        } else {
            ID_FORMAT_JPEG
        },
    );
    let quality_id = [80u8, 90, 100]
        .iter()
        .position(|&value| value == settings.jpeg_quality)
        .map(|index| ID_QUALITY_FIRST + index as i32);
    set_radio_group(hwnd, ID_QUALITY_FIRST, ID_QUALITY_FIRST + 2, quality_id);
    set_label_note(
        hwnd,
        707,
        "Quality",
        quality_id
            .is_none()
            .then(|| format!("{}%", settings.jpeg_quality)),
    );
    set_jpeg_quality_enabled(hwnd, settings.save_format == SaveFormat::Jpeg);
    set_check(hwnd, ID_WINDOW_SNAP, settings.enable_window_snap);
    set_check(hwnd, ID_CLOSE_AFTER_ACTION, settings.close_after_action);
    set_check(hwnd, ID_START_WITH_WINDOWS, settings.start_with_windows);
    set_check(hwnd, ID_NOTIFY_AFTER_SAVE, settings.notify_after_save);
    set_check(hwnd, ID_CHECK_UPDATES, settings.check_updates_automatically);
    check_radio(
        hwnd,
        ID_THEME_SYSTEM,
        ID_THEME_DARK,
        match settings.theme_preference {
            ThemePreference::System => ID_THEME_SYSTEM,
            ThemePreference::Light => ID_THEME_LIGHT,
            ThemePreference::Dark => ID_THEME_DARK,
        },
    );
    update_folder_label(hwnd, &settings.save_directory);
}

fn update_folder_label(hwnd: HWND, path: &Path) {
    let label = wide_string(&path.display().to_string());
    if let Some(child) = control(hwnd, ID_FOLDER_LABEL) {
        unsafe {
            let _ = windows::Win32::UI::WindowsAndMessaging::SetWindowTextW(
                child,
                PCWSTR(label.as_ptr()),
            );
        }
    }
}

fn choose_folder(owner: HWND) -> Result<Option<PathBuf>> {
    use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance, CoTaskMemFree};
    use windows::Win32::UI::Shell::{
        FOS_FORCEFILESYSTEM, FOS_PICKFOLDERS, FileOpenDialog, IFileOpenDialog, SIGDN_FILESYSPATH,
    };
    let dialog: IFileOpenDialog =
        unsafe { CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER)? };
    unsafe {
        dialog.SetOptions(dialog.GetOptions()? | FOS_PICKFOLDERS | FOS_FORCEFILESYSTEM)?;
        dialog.SetTitle(w!("Choose your screenshot folder"))?;
    }
    if let Err(error) = unsafe { dialog.Show(owner) } {
        if error.code() == windows::core::HRESULT::from_win32(1223) {
            return Ok(None);
        }
        return Err(error);
    }
    let raw = unsafe { dialog.GetResult()?.GetDisplayName(SIGDN_FILESYSPATH)? };
    let path = unsafe { raw.to_string() };
    unsafe {
        CoTaskMemFree(Some(raw.0.cast()));
    }
    Ok(Some(PathBuf::from(path?)))
}

fn create_settings_controls(hwnd: HWND) -> Result<()> {
    use windows::Win32::UI::WindowsAndMessaging::{
        BS_AUTOCHECKBOX, BS_AUTORADIOBUTTON, BS_DEFPUSHBUTTON, BS_PUSHBUTTON, BS_PUSHLIKE, HMENU,
        WS_CHILD, WS_CLIPCHILDREN, WS_EX_CONTROLPARENT, WS_GROUP, WS_HSCROLL, WS_TABSTOP,
        WS_VISIBLE, WS_VSCROLL,
    };
    // The scroll container clips its controls, so content that is scrolled out
    // can never draw over the pinned sidebar, header or footer.
    let content = unsafe {
        CreateWindowExW(
            WS_EX_CONTROLPARENT,
            SCROLL_CLASS_NAME,
            PCWSTR::null(),
            WS_CHILD | WS_VISIBLE | WS_VSCROLL | WS_HSCROLL | WS_CLIPCHILDREN,
            0,
            0,
            0,
            0,
            hwnd,
            HMENU(ID_SCROLL_CONTAINER as *mut std::ffi::c_void),
            HINSTANCE::default(),
            None,
        )?
    };
    let label =
        |parent: HWND, id, text| create_control(parent, w!("STATIC"), text, Default::default(), id);

    label(hwnd, 700, "isolmaSS")?;
    label(
        hwnd,
        718,
        concat!(
            "Version ",
            env!("CARGO_PKG_VERSION"),
            "  ·  Local and private"
        ),
    )?;
    create_button(
        hwnd,
        ID_VIEW_GENERAL,
        "Genel",
        BS_AUTORADIOBUTTON | BS_PUSHLIKE | WS_GROUP.0 as i32 | WS_TABSTOP.0 as i32,
    )?;
    create_button(
        hwnd,
        ID_VIEW_EDITOR,
        "Düzenleyici",
        BS_AUTORADIOBUTTON | BS_PUSHLIKE | WS_TABSTOP.0 as i32,
    )?;
    create_button(
        hwnd,
        ID_VIEW_UPDATES,
        "Güncellemeler",
        BS_AUTORADIOBUTTON | BS_PUSHLIKE | WS_TABSTOP.0 as i32,
    )?;

    label(content, 710, "Capture")?;
    label(content, 701, "Global hotkey")?;
    create_button(
        content,
        ID_HOTKEY_RECORD,
        "Record shortcut",
        BS_PUSHBUTTON | WS_GROUP.0 as i32 | WS_TABSTOP.0 as i32,
    )?;
    label(content, 705, "Capture delay")?;
    for (index, name) in ["0 s", "1 s", "3 s", "5 s"].into_iter().enumerate() {
        create_button(
            content,
            ID_DELAY_FIRST + index as i32,
            name,
            BS_AUTORADIOBUTTON
                | if index == 0 {
                    WS_GROUP.0 as i32
                } else {
                    Default::default()
                }
                | WS_TABSTOP.0 as i32,
        )?;
    }

    label(content, 711, "Saving")?;
    create_control(
        content,
        w!("STATIC"),
        "",
        windows::Win32::UI::WindowsAndMessaging::WINDOW_STYLE(0x0000_4000),
        ID_FOLDER_LABEL,
    )?;
    create_button(
        content,
        ID_BROWSE,
        "Browse…",
        BS_PUSHBUTTON | WS_TABSTOP.0 as i32,
    )?;
    label(content, 706, "Image format")?;
    create_button(
        content,
        ID_FORMAT_PNG,
        "PNG",
        BS_AUTORADIOBUTTON | WS_GROUP.0 as i32 | WS_TABSTOP.0 as i32,
    )?;
    create_button(
        content,
        ID_FORMAT_JPEG,
        "JPEG",
        BS_AUTORADIOBUTTON | WS_TABSTOP.0 as i32,
    )?;
    label(content, 707, "Quality")?;
    for (index, quality) in [80, 90, 100].into_iter().enumerate() {
        let label = quality.to_string();
        create_button(
            content,
            ID_QUALITY_FIRST + index as i32,
            &label,
            BS_AUTORADIOBUTTON
                | if index == 0 {
                    WS_GROUP.0 as i32
                } else {
                    Default::default()
                }
                | WS_TABSTOP.0 as i32,
        )?;
    }

    label(content, 712, "Windows")?;
    create_button(
        content,
        ID_WINDOW_SNAP,
        "Snap to window",
        BS_AUTOCHECKBOX | WS_GROUP.0 as i32 | WS_TABSTOP.0 as i32,
    )?;
    create_button(
        content,
        ID_CLOSE_AFTER_ACTION,
        "Close after save or copy",
        BS_AUTOCHECKBOX | WS_TABSTOP.0 as i32,
    )?;

    label(content, 713, "Drawing")?;
    label(content, 703, "Default color")?;
    // The swatches render as circles; the window text stays for screen readers.
    for (index, name) in [
        "Red", "Orange", "Yellow", "Green", "Blue", "Purple", "White", "Black",
    ]
    .into_iter()
    .enumerate()
    {
        create_button(
            content,
            ID_COLOR_FIRST + index as i32,
            name,
            BS_AUTORADIOBUTTON
                | if index == 0 {
                    WS_GROUP.0 as i32
                } else {
                    Default::default()
                }
                | WS_TABSTOP.0 as i32,
        )?;
    }
    create_button(
        content,
        ID_COLOR_CUSTOM,
        "More colors...",
        BS_PUSHBUTTON | WS_TABSTOP.0 as i32,
    )?;
    label(content, 704, "Line thickness")?;
    create_control(
        content,
        w!("msctls_trackbar32"),
        "",
        windows::Win32::UI::WindowsAndMessaging::WINDOW_STYLE(WS_TABSTOP.0),
        ID_THICKNESS_SLIDER,
    )?;
    create_control(
        content,
        w!("EDIT"),
        "4",
        windows::Win32::UI::WindowsAndMessaging::WINDOW_STYLE(WS_TABSTOP.0 | 0x2000),
        ID_THICKNESS_EDIT,
    )?;
    label(content, 720, "px")?;

    label(content, 714, "Selection")?;
    create_button(
        content,
        ID_START_WITH_WINDOWS,
        "Launch at sign-in",
        BS_AUTOCHECKBOX | WS_GROUP.0 as i32 | WS_TABSTOP.0 as i32,
    )?;
    create_button(
        content,
        ID_NOTIFY_AFTER_SAVE,
        "Notify after saving",
        BS_AUTOCHECKBOX | WS_TABSTOP.0 as i32,
    )?;
    label(content, 719, "Appearance")?;
    create_button(
        content,
        ID_THEME_SYSTEM,
        "System",
        BS_AUTORADIOBUTTON | WS_GROUP.0 as i32 | WS_TABSTOP.0 as i32,
    )?;
    create_button(
        content,
        ID_THEME_LIGHT,
        "Light",
        BS_AUTORADIOBUTTON | WS_TABSTOP.0 as i32,
    )?;
    create_button(
        content,
        ID_THEME_DARK,
        "Dark",
        BS_AUTORADIOBUTTON | WS_TABSTOP.0 as i32,
    )?;

    label(content, 715, "Updates")?;
    create_button(
        content,
        ID_CHECK_UPDATES,
        "Check for updates automatically",
        BS_AUTOCHECKBOX | WS_GROUP.0 as i32 | WS_TABSTOP.0 as i32,
    )?;
    create_button(
        content,
        ID_CHECK_UPDATE,
        "Check for updates",
        BS_PUSHBUTTON | WS_TABSTOP.0 as i32,
    )?;
    label(content, ID_UPDATE_STATUS, "Ready to check for updates")?;
    create_control(
        content,
        w!("BUTTON"),
        "",
        windows::Win32::UI::WindowsAndMessaging::WINDOW_STYLE(BS_PUSHBUTTON as u32),
        ID_UPDATE_PROGRESS,
    )?;
    if let Some(progress) = control(hwnd, ID_UPDATE_PROGRESS) {
        unsafe {
            let _ = ShowWindow(progress, SW_HIDE);
        }
    }
    label(
        content,
        708,
        "Signed releases only. Save your work first; installation closes and restarts isolmaSS.",
    )?;

    create_button(
        hwnd,
        ID_SAVE,
        "Save changes",
        BS_DEFPUSHBUTTON | WS_GROUP.0 as i32 | WS_TABSTOP.0 as i32,
    )?;
    create_button(
        hwnd,
        ID_CANCEL,
        "Cancel",
        BS_PUSHBUTTON | WS_TABSTOP.0 as i32,
    )?;
    Ok(())
}

fn apply_button_action(hwnd: HWND, state: &mut SettingsWindowState, id: i32) {
    match id {
        ID_VIEW_GENERAL | ID_VIEW_EDITOR | ID_VIEW_UPDATES => {
            state.active_view = match id {
                ID_VIEW_EDITOR => SettingsView::Editor,
                ID_VIEW_UPDATES => SettingsView::Updates,
                _ => SettingsView::General,
            };
            set_active_view(hwnd, state.active_view, state.update_status);
            if !state.user_resized
                && let Err(error) = unsafe {
                    windows::Win32::UI::WindowsAndMessaging::PostMessageW(
                        hwnd,
                        WM_SETTINGS_RESIZE,
                        WPARAM(0),
                        LPARAM(0),
                    )
                }
            {
                crate::diagnostics::record("settings resize", &error.to_string());
            }
        }
        ID_HOTKEY_RECORD => {
            state.recording_hotkey = true;
            if let Some(button) = control(hwnd, ID_HOTKEY_RECORD) {
                let text = wide_string("Press keys · Esc cancels");
                unsafe {
                    let _ = windows::Win32::UI::WindowsAndMessaging::SetWindowTextW(
                        button,
                        PCWSTR(text.as_ptr()),
                    );
                    let _ = SetFocus(button);
                }
            }
        }
        ID_COLOR_FIRST..=307 => {
            state.settings.default_color = PRESET_COLORS[(id - ID_COLOR_FIRST) as usize];
        }
        ID_COLOR_CUSTOM => match crate::ui::choose_color(hwnd, state.settings.default_color) {
            Ok(Some(color)) => {
                state.settings.default_color = color;
                state.settings.last_custom_color = color;
            }
            Ok(None) => {}
            Err(error) => crate::ui::error(hwnd, "Color picker failed", &error.to_string()),
        },
        ID_THEME_SYSTEM | ID_THEME_LIGHT | ID_THEME_DARK => {
            state.settings.theme_preference = match id {
                ID_THEME_LIGHT => ThemePreference::Light,
                ID_THEME_DARK => ThemePreference::Dark,
                _ => ThemePreference::System,
            };
            crate::theme::set_preference(state.settings.theme_preference);
            unsafe {
                crate::theme::apply_window_theme(
                    hwnd,
                    crate::theme::theme() == crate::theme::Theme::Dark,
                    true,
                );
            }
            state.refresh_brushes();
        }
        ID_WINDOW_SNAP => state.settings.enable_window_snap = !state.settings.enable_window_snap,
        ID_CLOSE_AFTER_ACTION => {
            state.settings.close_after_action = !state.settings.close_after_action
        }
        ID_START_WITH_WINDOWS => {
            state.settings.start_with_windows = !state.settings.start_with_windows
        }
        ID_NOTIFY_AFTER_SAVE => {
            state.settings.notify_after_save = !state.settings.notify_after_save
        }
        ID_CHECK_UPDATES => {
            state.settings.check_updates_automatically = !state.settings.check_updates_automatically
        }
        ID_DELAY_FIRST..=503 => {
            state.settings.capture_delay_ms = [0, 1000, 3000, 5000][(id - ID_DELAY_FIRST) as usize];
        }
        ID_FORMAT_PNG => {
            state.settings.save_format = SaveFormat::Png;
            set_jpeg_quality_enabled(hwnd, false);
        }
        ID_FORMAT_JPEG => {
            state.settings.save_format = SaveFormat::Jpeg;
            set_jpeg_quality_enabled(hwnd, true);
        }
        ID_QUALITY_FIRST..=612 => {
            state.settings.jpeg_quality = [80, 90, 100][(id - ID_QUALITY_FIRST) as usize];
        }
        ID_CHECK_UPDATE => {
            if crate::updater::run_manual_update_check() {
                set_update_status(
                    hwnd,
                    state,
                    "Checking the latest signed release…",
                    UpdateStatus::Busy,
                );
            } else {
                set_update_status(
                    hwnd,
                    state,
                    "An update is already in progress…",
                    UpdateStatus::Busy,
                );
            }
        }
        ID_CANCEL => unsafe {
            let _ = DestroyWindow(hwnd);
        },
        _ => {}
    }
}

/// Intercept keyboard messages before dialog navigation consumes Tab/Alt/Enter.
pub fn handle_key_recording(
    hwnd: HWND,
    message: &windows::Win32::UI::WindowsAndMessaging::MSG,
) -> bool {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        GetKeyState, HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL, MOD_SHIFT, MOD_WIN, RegisterHotKey,
        UnregisterHotKey, VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT, VK_SNAPSHOT,
    };
    use windows::Win32::UI::WindowsAndMessaging::{WM_KEYUP, WM_SYSKEYDOWN, WM_SYSKEYUP};
    let ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *mut SettingsWindowState;
    if ptr.is_null() || !unsafe { (*ptr).recording_hotkey } {
        return false;
    }
    let state = unsafe { &mut *ptr };
    let key = message.wParam.0 as u32;
    if message.message == WM_KEYUP || message.message == WM_SYSKEYUP {
        if key != VK_SNAPSHOT.0 as u32 {
            return true;
        }
    } else if message.message != WM_KEYDOWN && message.message != WM_SYSKEYDOWN {
        return false;
    }
    if key == VK_ESCAPE.0 as u32 {
        state.recording_hotkey = false;
        initialize_control_values(hwnd, &state.settings);
        return true;
    }
    if [VK_CONTROL.0, VK_SHIFT.0, VK_MENU.0, VK_LWIN.0, VK_RWIN.0].contains(&(key as u16)) {
        return true;
    }
    let down = |vk: u16| unsafe { GetKeyState(vk as i32) < 0 };
    let mut parts = Vec::with_capacity(5);
    let mut modifiers = 0u32;
    if down(VK_CONTROL.0) {
        parts.push("Ctrl".to_string());
        modifiers |= MOD_CONTROL.0;
    }
    if down(VK_MENU.0) {
        parts.push("Alt".to_string());
        modifiers |= MOD_ALT.0;
    }
    if down(VK_SHIFT.0) {
        parts.push("Shift".to_string());
        modifiers |= MOD_SHIFT.0;
    }
    if down(VK_LWIN.0) || down(VK_RWIN.0) {
        parts.push("Win".to_string());
        modifiers |= MOD_WIN.0;
    }
    let key_name = match key {
        0x30..=0x39 | 0x41..=0x5a => char::from_u32(key).map(|value| value.to_string()),
        0x70..=0x87 => Some(format!("F{}", key - 0x6f)),
        value if value == VK_SNAPSHOT.0 as u32 => Some("PrintScreen".to_string()),
        _ => None,
    };
    let Some(key_name) = key_name else {
        return true;
    };
    if modifiers == 0 && key != VK_SNAPSHOT.0 as u32 {
        return true;
    }
    parts.push(key_name);
    let description = parts.join("+");
    let Some(candidate) = HotkeyConfig::from_str(&description) else {
        return true;
    };
    state.recording_hotkey = false;
    if candidate != state.settings.hotkey && candidate != state.original_hotkey {
        const PROBE_ID: i32 = 0x5a51;
        match unsafe {
            RegisterHotKey(
                HWND::default(),
                PROBE_ID,
                HOT_KEY_MODIFIERS(candidate.modifiers),
                candidate.vk,
            )
        } {
            Ok(()) => {
                let _ = unsafe { UnregisterHotKey(HWND::default(), PROBE_ID) };
            }
            Err(_) => {
                crate::ui::error(
                    hwnd,
                    "Shortcut conflict",
                    &format!(
                        "{description} is already reserved by Windows or another application. Your current shortcut was kept. Record a different combination."
                    ),
                );
                initialize_control_values(hwnd, &state.settings);
                return true;
            }
        }
    }
    state.settings.hotkey = candidate;
    initialize_control_values(hwnd, &state.settings);
    true
}

unsafe extern "system" fn settings_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let state_ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *mut SettingsWindowState;
    match msg {
        windows::Win32::UI::WindowsAndMessaging::WM_NCDESTROY => {
            unsafe {
                let _ = windows::Win32::UI::WindowsAndMessaging::KillTimer(hwnd, 1);
                let _ = windows::Win32::UI::WindowsAndMessaging::KillTimer(hwnd, 2);
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            }
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }
        windows::Win32::UI::WindowsAndMessaging::WM_TIMER if wparam.0 == 1 => {
            crate::updater::poll(hwnd, false);
            LRESULT(0)
        }
        windows::Win32::UI::WindowsAndMessaging::WM_TIMER if wparam.0 == 2 => {
            if !state_ptr.is_null() && unsafe { (*state_ptr).update_status == UpdateStatus::Busy } {
                let state = unsafe { &mut *state_ptr };
                state.update_frame = (state.update_frame + 1) % 36;
                if state.active_view == SettingsView::Updates
                    && let Some(progress) = control(hwnd, ID_UPDATE_PROGRESS)
                {
                    unsafe {
                        let _ = InvalidateRect(progress, None, false);
                    }
                }
            }
            LRESULT(0)
        }
        WM_SETTINGS_RESIZE if !state_ptr.is_null() => {
            let state = unsafe { &*state_ptr };
            if !state.user_resized
                && !unsafe { windows::Win32::UI::WindowsAndMessaging::IsZoomed(hwnd) }.as_bool()
            {
                let height = match state.active_view {
                    SettingsView::General => SETTINGS_HEIGHT,
                    SettingsView::Editor => 404,
                    SettingsView::Updates => 384,
                };
                let mut rect = RECT::default();
                if unsafe { GetWindowRect(hwnd, &mut rect) }.is_ok() {
                    if let Err(error) = unsafe {
                        windows::Win32::UI::WindowsAndMessaging::SetWindowPos(
                            hwnd,
                            None,
                            rect.left,
                            rect.top,
                            rect.right - rect.left,
                            scale(height, state.dpi),
                            windows::Win32::UI::WindowsAndMessaging::SWP_NOACTIVATE
                                | windows::Win32::UI::WindowsAndMessaging::SWP_NOZORDER,
                        )
                    } {
                        crate::diagnostics::record("settings resize", &error.to_string());
                    }
                    center_dialog(hwnd, None);
                }
            }
            LRESULT(0)
        }
        windows::Win32::UI::WindowsAndMessaging::WM_EXITSIZEMOVE => {
            if !state_ptr.is_null() {
                unsafe { (*state_ptr).user_resized = true };
            }
            LRESULT(0)
        }
        windows::Win32::UI::WindowsAndMessaging::WM_SIZE => {
            if !state_ptr.is_null() {
                let dpi = unsafe { (*state_ptr).dpi };
                layout_chrome(hwnd, dpi);
                position_container(hwnd, dpi);
            }
            LRESULT(0)
        }
        windows::Win32::UI::WindowsAndMessaging::WM_GETMINMAXINFO => {
            // Keep the window at a size where the pinned chrome and a usable
            // slice of the content column are always visible.
            if lparam.0 != 0 {
                let dpi = if state_ptr.is_null() {
                    96
                } else {
                    unsafe { (*state_ptr).dpi }
                };
                let info = unsafe {
                    &mut *(lparam.0 as *mut windows::Win32::UI::WindowsAndMessaging::MINMAXINFO)
                };
                info.ptMinTrackSize.x = scale(SETTINGS_MIN_WIDTH, dpi);
                info.ptMinTrackSize.y = scale(SETTINGS_MIN_HEIGHT, dpi);
            }
            LRESULT(0)
        }
        windows::Win32::UI::WindowsAndMessaging::WM_MOUSEWHEEL => {
            // Wheel over the chrome still scrolls the content column.
            if let Ok(container) = unsafe {
                windows::Win32::UI::WindowsAndMessaging::GetDlgItem(hwnd, ID_SCROLL_CONTAINER)
            } {
                scroll_container_by(
                    container,
                    false,
                    -((wparam.0 >> 16) as u16 as i16 as i32) / 2,
                );
            }
            LRESULT(0)
        }
        windows::Win32::UI::WindowsAndMessaging::WM_NOTIFY => {
            use windows::Win32::UI::Controls::*;
            if !state_ptr.is_null() && lparam.0 != 0 {
                let header = unsafe { &*(lparam.0 as *const NMHDR) };
                if header.code == NM_CUSTOMDRAW {
                    let draw = unsafe { &*(lparam.0 as *const NMCUSTOMDRAW) };
                    if draw.dwDrawStage == CDDS_PREPAINT {
                        let id = draw.hdr.idFrom as i32;
                        if id == ID_THICKNESS_SLIDER
                            && crate::theme::theme() == crate::theme::Theme::Dark
                        {
                            draw_settings_slider(draw, unsafe { &*state_ptr });
                            return LRESULT(CDRF_SKIPDEFAULT as isize);
                        }
                        if id == ID_UPDATE_PROGRESS {
                            draw_update_progress(draw, unsafe { &*state_ptr });
                            return LRESULT(CDRF_SKIPDEFAULT as isize);
                        }
                        if id != ID_THICKNESS_SLIDER {
                            draw_settings_button(draw, unsafe { &*state_ptr });
                            return LRESULT(CDRF_SKIPDEFAULT as isize);
                        }
                    }
                }
            }
            LRESULT(0)
        }
        WM_PAINT => {
            if state_ptr.is_null() {
                return unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) };
            }
            let mut paint = PAINTSTRUCT::default();
            let hdc = unsafe { BeginPaint(hwnd, &mut paint) };
            paint_settings_surface(hwnd, unsafe { &*state_ptr }, hdc);
            unsafe {
                let _ = EndPaint(hwnd, &paint);
            }
            LRESULT(0)
        }
        WM_ERASEBKGND => {
            if !state_ptr.is_null() {
                let state = unsafe { &*state_ptr };
                // Always paint the page: every scroll step must erase the
                // previous frame, or cards and controls leave ghosts behind.
                let mut client = RECT::default();
                unsafe {
                    let _ = GetClientRect(hwnd, &mut client);
                    let _ = FillRect(HDC(wparam.0 as *mut _), &client, state.background_brush);
                }
                return LRESULT(1);
            }
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }
        WM_SETTINGCHANGE => {
            crate::theme::invalidate_theme_cache();
            if !state_ptr.is_null() {
                let state = unsafe { &mut *state_ptr };
                unsafe {
                    crate::theme::apply_window_theme(
                        hwnd,
                        crate::theme::theme() == crate::theme::Theme::Dark,
                        true,
                    );
                }
                state.refresh_brushes();
                unsafe {
                    let _ = windows::Win32::Graphics::Gdi::RedrawWindow(
                        hwnd,
                        None,
                        None,
                        windows::Win32::Graphics::Gdi::RDW_INVALIDATE
                            | windows::Win32::Graphics::Gdi::RDW_ALLCHILDREN,
                    );
                }
            }
            LRESULT(0)
        }
        WM_DWMCOLORIZATIONCOLORCHANGED => {
            if !state_ptr.is_null() {
                let state = unsafe { &mut *state_ptr };
                crate::theme::invalidate_accent();
                state.refresh_brushes();
                unsafe {
                    let _ = InvalidateRect(hwnd, None, true);
                    let _ = windows::Win32::Graphics::Gdi::RedrawWindow(
                        hwnd,
                        None,
                        None,
                        windows::Win32::Graphics::Gdi::RDW_INVALIDATE
                            | windows::Win32::Graphics::Gdi::RDW_ALLCHILDREN,
                    );
                }
            }
            LRESULT(0)
        }
        WM_CTLCOLOREDIT if !state_ptr.is_null() => {
            let state = unsafe { &*state_ptr };
            let hdc = HDC(wparam.0 as *mut _);
            unsafe {
                let _ = SetTextColor(hdc, color_text());
                let _ = SetBkColor(hdc, color_card());
            }
            LRESULT(state.card_brush.0 as isize)
        }
        WM_CTLCOLORSTATIC | WM_CTLCOLORBTN => {
            if state_ptr.is_null() {
                return unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) };
            }
            let state = unsafe { &*state_ptr };
            let child = HWND(lparam.0 as *mut _);
            let id = unsafe { GetDlgCtrlID(child) };
            let hdc = HDC(wparam.0 as *mut _);
            unsafe {
                let _ = SetBkMode(hdc, TRANSPARENT);
                let color = if !IsWindowEnabled(child).as_bool() {
                    color_disabled()
                } else if id == 708 {
                    color_muted()
                } else if matches!(id, 701..=707) {
                    color_property()
                } else {
                    color_text()
                };
                let _ = SetTextColor(hdc, color);
            }
            let brush = if control_uses_card(id) {
                state.card_brush
            } else {
                state.background_brush
            };
            LRESULT(brush.0 as isize)
        }
        windows::Win32::UI::WindowsAndMessaging::WM_HSCROLL if !state_ptr.is_null() => {
            let slider = HWND(lparam.0 as *mut _);
            if unsafe { GetDlgCtrlID(slider) } == ID_THICKNESS_SLIDER {
                let value = unsafe {
                    windows::Win32::UI::WindowsAndMessaging::SendMessageW(
                        slider,
                        windows::Win32::UI::WindowsAndMessaging::WM_USER,
                        WPARAM(0),
                        LPARAM(0),
                    )
                    .0 as i32
                }
                .clamp(1, 64);
                let state = unsafe { &mut *state_ptr };
                state.settings.default_thickness = value;
                if let Some(edit) = control(hwnd, ID_THICKNESS_EDIT) {
                    state.updating_thickness = true;
                    let text = wide_string(&value.to_string());
                    unsafe {
                        let _ = windows::Win32::UI::WindowsAndMessaging::SetWindowTextW(
                            edit,
                            PCWSTR(text.as_ptr()),
                        );
                    }
                    state.updating_thickness = false;
                }
            }
            LRESULT(0)
        }
        windows::Win32::UI::WindowsAndMessaging::WM_COMMAND => {
            if state_ptr.is_null() {
                return LRESULT(0);
            }
            let id = (wparam.0 & 0xffff) as i32;
            if id == ID_THICKNESS_EDIT && unsafe { (*state_ptr).updating_thickness } {
                return LRESULT(0);
            }
            if (wparam.0 >> 16) as u32 == 0x300 && id == ID_THICKNESS_EDIT {
                let mut digits = [0u16; 8];
                let length = unsafe {
                    windows::Win32::UI::WindowsAndMessaging::GetWindowTextW(
                        HWND(lparam.0 as *mut _),
                        &mut digits,
                    )
                };
                if let Ok(value) =
                    String::from_utf16_lossy(&digits[..length.max(0) as usize]).parse::<i32>()
                    && (1..=64).contains(&value)
                {
                    unsafe { (*state_ptr).settings.default_thickness = value };
                    if let Some(slider) = control(hwnd, ID_THICKNESS_SLIDER) {
                        unsafe {
                            windows::Win32::UI::WindowsAndMessaging::SendMessageW(
                                slider,
                                windows::Win32::UI::Controls::TBM_SETPOS,
                                WPARAM(1),
                                LPARAM(value as isize),
                            );
                        }
                    }
                }
                return LRESULT(0);
            }
            if wparam.0 >> 16 != 0 {
                return LRESULT(0);
            }
            if id == 2 {
                // IDCANCEL as translated from Escape by IsDialogMessage in ui::window_loop;
                // with a child control focused the WM_KEYDOWN path never sees the key.
                unsafe {
                    let _ = DestroyWindow(hwnd);
                }
                return LRESULT(0);
            }
            if id == ID_BROWSE {
                match choose_folder(hwnd) {
                    Ok(Some(path)) => unsafe {
                        (*state_ptr).settings.save_directory = path;
                        update_folder_label(hwnd, &(*state_ptr).settings.save_directory);
                    },
                    Ok(None) => {}
                    Err(error) => {
                        crate::ui::error(hwnd, "Folder could not be opened", &error.to_string())
                    }
                }
            } else if id == ID_SAVE {
                let mut digits = [0u16; 8];
                if let Some(edit) = control(hwnd, ID_THICKNESS_EDIT) {
                    let count = unsafe {
                        windows::Win32::UI::WindowsAndMessaging::GetWindowTextW(edit, &mut digits)
                    };
                    if !String::from_utf16_lossy(&digits[..count.max(0) as usize])
                        .parse::<i32>()
                        .is_ok_and(|value| (1..=64).contains(&value))
                    {
                        crate::ui::error(
                            hwnd,
                            "Invalid thickness",
                            "Enter a line thickness from 1 to 64 pixels.",
                        );
                        return LRESULT(0);
                    }
                }
                let mut settings = unsafe { (*state_ptr).settings.clone() };
                match Settings::load() {
                    Ok(latest) => settings.skipped_update_version = latest.skipped_update_version,
                    Err(error) => crate::diagnostics::record("settings merge", &error.to_string()),
                }
                let result = save_settings(&settings);
                match result {
                    Ok(()) => unsafe {
                        (*state_ptr).settings = settings;
                        (*state_ptr).saved = true;
                        let _ = DestroyWindow(hwnd);
                    },
                    Err(error) => crate::ui::error(hwnd, "Settings could not be saved", &error),
                }
            } else {
                apply_button_action(hwnd, unsafe { &mut *state_ptr }, id);
                if unsafe { IsWindow(hwnd).as_bool() } {
                    if !unsafe { (*state_ptr).recording_hotkey } {
                        initialize_control_values(hwnd, unsafe { &(*state_ptr).settings });
                    }
                    unsafe {
                        let _ = windows::Win32::Graphics::Gdi::RedrawWindow(
                            hwnd,
                            None,
                            None,
                            windows::Win32::Graphics::Gdi::RDW_INVALIDATE
                                | windows::Win32::Graphics::Gdi::RDW_ALLCHILDREN,
                        );
                    }
                }
            }
            LRESULT(0)
        }
        windows::Win32::UI::WindowsAndMessaging::WM_DPICHANGED => {
            if !state_ptr.is_null() {
                let state = unsafe { &mut *state_ptr };
                state.dpi = (wparam.0 & 0xffff) as u32;
                let suggested = unsafe { &*(lparam.0 as *const RECT) };
                unsafe {
                    let _ = windows::Win32::UI::WindowsAndMessaging::SetWindowPos(
                        hwnd,
                        None,
                        suggested.left,
                        suggested.top,
                        suggested.right - suggested.left,
                        suggested.bottom - suggested.top,
                        windows::Win32::UI::WindowsAndMessaging::SWP_NOACTIVATE
                            | windows::Win32::UI::WindowsAndMessaging::SWP_NOZORDER,
                    );
                }
                let new_font = create_settings_font(state.dpi);
                let new_title_font = create_title_font(state.dpi);
                let new_heading_font = create_heading_font(state.dpi);
                let old_font = std::mem::replace(&mut state.font, new_font);
                let old_title_font = std::mem::replace(&mut state.title_font, new_title_font);
                let old_heading_font = std::mem::replace(&mut state.heading_font, new_heading_font);
                set_controls_font(hwnd, new_font);
                set_control_font(hwnd, 700, new_title_font);
                for id in 710..=715 {
                    set_control_font(hwnd, id, new_heading_font);
                }
                set_control_font(hwnd, 719, new_heading_font);
                center_dialog(hwnd, None);
                layout_chrome(hwnd, state.dpi);
                position_container(hwnd, state.dpi);
                for font in [old_font, old_title_font, old_heading_font] {
                    if !font.is_invalid() {
                        unsafe {
                            let _ = DeleteObject(HGDIOBJ(font.0));
                        }
                    }
                }
                unsafe {
                    let _ = InvalidateRect(hwnd, None, true);
                }
            }
            LRESULT(0)
        }
        WM_KEYDOWN if wparam.0 == VK_ESCAPE.0 as usize => {
            unsafe {
                let _ = DestroyWindow(hwnd);
            }
            LRESULT(0)
        }
        WM_CLOSE => {
            unsafe {
                let _ = DestroyWindow(hwnd);
            }
            LRESULT(0)
        }
        WM_DESTROY => LRESULT(0),
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

fn register_settings_class() -> Result<()> {
    static REGISTERED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if REGISTERED.load(std::sync::atomic::Ordering::Acquire) {
        return Ok(());
    }
    let class = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        style: windows::Win32::UI::WindowsAndMessaging::CS_HREDRAW
            | windows::Win32::UI::WindowsAndMessaging::CS_VREDRAW,
        lpfnWndProc: Some(settings_wnd_proc),
        hInstance: HINSTANCE::default(),
        hIcon: crate::tray::app_icon(),
        hCursor: unsafe {
            windows::Win32::UI::WindowsAndMessaging::LoadCursorW(HINSTANCE::default(), IDC_ARROW)
                .unwrap_or_default()
        },
        hbrBackground: HBRUSH((COLOR_WINDOW.0 + 1) as *mut _),
        lpszClassName: SETTINGS_CLASS_NAME,
        hIconSm: crate::tray::app_icon(),
        ..Default::default()
    };
    let atom = unsafe { RegisterClassExW(&class) };
    if atom == 0 && unsafe { GetLastError() } != ERROR_CLASS_ALREADY_EXISTS {
        return Err(windows::core::Error::from_win32());
    }
    REGISTERED.store(true, std::sync::atomic::Ordering::Release);
    Ok(())
}

struct ModalOwner {
    hwnd: Option<HWND>,
    restore_enabled: bool,
}

impl ModalOwner {
    fn disable(hwnd: Option<HWND>) -> Self {
        let restore_enabled = hwnd.is_some_and(|owner| unsafe { IsWindowEnabled(owner).as_bool() });
        if let Some(owner) = hwnd.filter(|_| restore_enabled) {
            unsafe {
                let _ = EnableWindow(owner, false);
            }
        }
        Self {
            hwnd,
            restore_enabled,
        }
    }
}

impl Drop for ModalOwner {
    fn drop(&mut self) {
        if let Some(owner) = self
            .hwnd
            .filter(|owner| unsafe { IsWindow(*owner).as_bool() })
        {
            if self.restore_enabled {
                unsafe {
                    let _ = EnableWindow(owner, true);
                }
            }
            unsafe {
                let _ = SetActiveWindow(owner);
                let _ = SetForegroundWindow(owner);
            }
        }
    }
}

fn center_dialog(hwnd: HWND, owner: Option<HWND>) {
    let mut window = RECT::default();
    if unsafe { GetWindowRect(hwnd, &mut window) }.is_err() {
        return;
    }
    let width = window.right - window.left;
    let height = window.bottom - window.top;

    let mut anchor = RECT::default();
    let monitor = if let Some(owner) = owner {
        if unsafe { GetWindowRect(owner, &mut anchor) }.is_err() {
            return;
        }
        unsafe { MonitorFromWindow(owner, MONITOR_DEFAULTTONEAREST) }
    } else {
        unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) }
    };
    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    if !unsafe { GetMonitorInfoW(monitor, &mut info).as_bool() } {
        return;
    }
    if owner.is_none() {
        anchor = info.rcWork;
    }
    let width = width.min((info.rcWork.right - info.rcWork.left - 16).max(1));
    let height = height.min((info.rcWork.bottom - info.rcWork.top - 16).max(1));
    let x = (anchor.left + (anchor.right - anchor.left - width) / 2)
        .clamp(info.rcWork.left, info.rcWork.right - width);
    let y = (anchor.top + (anchor.bottom - anchor.top - height) / 2)
        .clamp(info.rcWork.top, info.rcWork.bottom - height);
    unsafe {
        let _ = windows::Win32::UI::WindowsAndMessaging::SetWindowPos(
            hwnd,
            None,
            x,
            y,
            width,
            height,
            windows::Win32::UI::WindowsAndMessaging::SWP_NOACTIVATE
                | windows::Win32::UI::WindowsAndMessaging::SWP_NOZORDER,
        );
    }
}

fn activate_dialog(hwnd: HWND) {
    // Do not attach to another application's input queue: an unresponsive foreground
    // process must never block opening our settings window.
    unsafe {
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = BringWindowToTop(hwnd);
        let _ = SetActiveWindow(hwnd);
        let _ = SetForegroundWindow(hwnd);
        if let Ok(first) =
            windows::Win32::UI::WindowsAndMessaging::GetDlgItem(hwnd, ID_VIEW_GENERAL)
        {
            let _ = SetFocus(first);
        }
    }
}

pub fn show_settings_dialog(current: &Settings, owner: Option<HWND>) -> Result<Option<Settings>> {
    unsafe {
        use windows::Win32::UI::Controls::*;
        let init = INITCOMMONCONTROLSEX {
            dwSize: std::mem::size_of::<INITCOMMONCONTROLSEX>() as u32,
            dwICC: ICC_STANDARD_CLASSES | ICC_BAR_CLASSES,
        };
        let _ = InitCommonControlsEx(&init);
    }
    crate::diagnostics::record("settings", "Opening settings window");
    register_settings_class()?;
    register_scroll_class()?;
    crate::theme::set_preference(current.theme_preference);
    let mut state = Box::new(SettingsWindowState {
        settings: current.clone(),
        saved: false,
        active_view: SettingsView::General,
        update_status: UpdateStatus::Idle,
        update_frame: 0,
        user_resized: false,
        recording_hotkey: false,
        original_hotkey: current.hotkey.clone(),
        updating_thickness: false,
        dpi: 96,
        font: Default::default(),
        title_font: Default::default(),
        heading_font: Default::default(),
        background_brush: Default::default(),
        card_brush: Default::default(),
    });
    // Initial brush creation from the current theme colors.
    state.refresh_brushes();
    let hwnd = unsafe {
        CreateWindowExW(
            Default::default(),
            SETTINGS_CLASS_NAME,
            w!("isolmaSS Settings"),
            windows::Win32::UI::WindowsAndMessaging::WS_OVERLAPPED
                | windows::Win32::UI::WindowsAndMessaging::WS_CAPTION
                | windows::Win32::UI::WindowsAndMessaging::WS_SYSMENU
                | windows::Win32::UI::WindowsAndMessaging::WS_THICKFRAME
                | windows::Win32::UI::WindowsAndMessaging::WS_MAXIMIZEBOX
                | windows::Win32::UI::WindowsAndMessaging::WS_CLIPCHILDREN,
            windows::Win32::UI::WindowsAndMessaging::CW_USEDEFAULT,
            windows::Win32::UI::WindowsAndMessaging::CW_USEDEFAULT,
            SETTINGS_WIDTH,
            SETTINGS_HEIGHT,
            owner.unwrap_or_default(),
            None,
            HINSTANCE::default(),
            None,
        )?
    };
    crate::diagnostics::record("settings", "Window created");
    unsafe {
        crate::theme::apply_window_theme(
            hwnd,
            crate::theme::theme() == crate::theme::Theme::Dark,
            true,
        );
    }
    let _window = crate::ui::OwnedWindow(hwnd);
    let _suspend = crate::hotkey::OverlayInputSuspension::new();
    let _modal_owner = ModalOwner::disable(owner);
    unsafe {
        SetWindowLongPtrW(
            hwnd,
            GWLP_USERDATA,
            state.as_mut() as *mut SettingsWindowState as isize,
        );
    }
    state.dpi = unsafe { windows::Win32::UI::HiDpi::GetDpiForWindow(hwnd) }.max(96);
    unsafe {
        let _ = windows::Win32::UI::WindowsAndMessaging::SetWindowPos(
            hwnd,
            None,
            0,
            0,
            scale(SETTINGS_WIDTH, state.dpi),
            scale(SETTINGS_HEIGHT, state.dpi),
            windows::Win32::UI::WindowsAndMessaging::SWP_NOMOVE
                | windows::Win32::UI::WindowsAndMessaging::SWP_NOACTIVATE
                | windows::Win32::UI::WindowsAndMessaging::SWP_NOZORDER,
        );
    }
    state.font = create_settings_font(state.dpi);
    state.title_font = create_title_font(state.dpi);
    state.heading_font = create_heading_font(state.dpi);
    create_settings_controls(hwnd)?;
    if let Some(slider) = control(hwnd, ID_THICKNESS_SLIDER) {
        unsafe {
            windows::Win32::UI::WindowsAndMessaging::SendMessageW(
                slider,
                windows::Win32::UI::Controls::TBM_SETRANGE,
                WPARAM(1),
                LPARAM(((64 << 16) | 1) as isize),
            );
        }
    }
    crate::diagnostics::record("settings", "Controls created");
    set_controls_font(hwnd, state.font);
    set_control_font(hwnd, 700, state.title_font);
    for id in 710..=715 {
        set_control_font(hwnd, id, state.heading_font);
    }
    set_control_font(hwnd, 719, state.heading_font);
    unsafe {
        let timer = windows::Win32::UI::WindowsAndMessaging::SetTimer(hwnd, 1, 350, None);
        if timer == 0 {
            crate::diagnostics::record(
                "settings update polling",
                "Could not start update polling timer",
            );
        }
    }
    layout_chrome(hwnd, state.dpi);
    position_container(hwnd, state.dpi);
    initialize_control_values(hwnd, &state.settings);
    set_active_view(hwnd, state.active_view, state.update_status);
    center_dialog(hwnd, owner);
    layout_chrome(hwnd, state.dpi);
    position_container(hwnd, state.dpi);
    if let Ok(container) =
        unsafe { windows::Win32::UI::WindowsAndMessaging::GetDlgItem(hwnd, ID_SCROLL_CONTAINER) }
    {
        update_container_scrollbars(container, state.dpi);
    }
    unsafe {
        let _ = ShowWindow(hwnd, SW_SHOW);
    }
    activate_dialog(hwnd);
    crate::diagnostics::record("settings", "Window activated");

    crate::ui::window_loop(hwnd, crate::ui::WindowKind::Settings)?;
    if state.saved {
        Ok(Some(state.settings.clone()))
    } else {
        crate::theme::set_preference(current.theme_preference);
        Ok(None)
    }
}

fn save_settings(settings: &Settings) -> std::result::Result<(), String> {
    settings.validate().map_err(|error| error.to_string())?;
    let previous = crate::startup::StartupRegistration::read()?;
    // Compare the exact current registry bytes first: an unchanged toggle skips the
    // registry write (and its rollback) entirely; only a real change writes before JSON.
    let startup_changed = previous.needs_update(settings.start_with_windows)?;
    if startup_changed {
        crate::startup::set_start_with_windows(settings.start_with_windows)?;
    }
    if let Err(error) = settings.save() {
        let rollback = if startup_changed {
            previous
                .restore()
                .err()
                .map(|error| format!(" Startup restoration also failed: {error}"))
                .unwrap_or_default()
        } else {
            String::new()
        };
        return Err(format!("{error}{rollback}"));
    }
    Ok(())
}
