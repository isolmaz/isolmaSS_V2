use crate::hotkey::HotkeyConfig;
use crate::settings::{PRESET_COLORS, PRESET_THICKNESSES, SaveFormat, Settings};
use std::path::{Path, PathBuf};
use windows::Win32::Foundation::{
    COLORREF, ERROR_CLASS_ALREADY_EXISTS, GetLastError, HINSTANCE, HWND, LPARAM, LRESULT, POINT,
    RECT, WPARAM,
};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, COLOR_WINDOW, CreatePen, CreateSolidBrush, DeleteObject, EndPaint, FillRect,
    GetMonitorInfoW, HBRUSH, HDC, HGDIOBJ, InvalidateRect, MONITOR_DEFAULTTONEAREST, MONITORINFO,
    MonitorFromWindow, PAINTSTRUCT, PS_SOLID, RoundRect, SelectObject, SetBkMode, SetTextColor,
    TRANSPARENT,
};
use windows::Win32::UI::Controls::{SetScrollInfo, SetScrollPos};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    EnableWindow, IsWindowEnabled, SetActiveWindow, SetFocus, VK_ESCAPE,
};
use windows::Win32::UI::WindowsAndMessaging::{
    BringWindowToTop, CreateWindowExW, DefWindowProcW, DestroyWindow, GWLP_USERDATA, GetClientRect,
    GetDlgCtrlID, GetWindowLongPtrW, GetWindowRect, IDC_ARROW, IsWindow, RegisterClassExW, SW_HIDE,
    SW_SHOW, SetForegroundWindow, SetWindowLongPtrW, ShowWindow, WM_CLOSE, WM_CTLCOLORBTN,
    WM_CTLCOLORSTATIC, WM_DESTROY, WM_ERASEBKGND, WM_KEYDOWN, WM_PAINT, WNDCLASSEXW,
};
use windows::core::{PCWSTR, Result, w};

const SETTINGS_CLASS_NAME: PCWSTR = w!("isolmaSS_SettingsClass");

#[derive(Clone, Copy, PartialEq, Eq)]
enum SettingsView {
    Simple,
    Advanced,
}

pub struct SettingsWindowState {
    settings: Settings,
    saved: bool,
    active_view: SettingsView,
    dpi: u32,
    font: windows::Win32::Graphics::Gdi::HFONT,
    title_font: windows::Win32::Graphics::Gdi::HFONT,
    heading_font: windows::Win32::Graphics::Gdi::HFONT,
    background_brush: HBRUSH,
    card_brush: HBRUSH,
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
const ID_VIEW_SIMPLE: i32 = 105;
const ID_VIEW_ADVANCED: i32 = 106;
const ID_HOTKEY_PRINT: i32 = 200;
const ID_HOTKEY_CTRL_SHIFT_S: i32 = 201;
const ID_HOTKEY_ALT_PRINT: i32 = 202;
const ID_COLOR_FIRST: i32 = 300;
const ID_THICKNESS_FIRST: i32 = 320;
const ID_WINDOW_SNAP: i32 = 400;
const ID_CLOSE_AFTER_ACTION: i32 = 401;
const ID_START_WITH_WINDOWS: i32 = 402;
const ID_NOTIFY_AFTER_SAVE: i32 = 403;
const ID_CHECK_UPDATES: i32 = 404;
const ID_AUTO_INSTALL: i32 = 405;
const ID_DELAY_FIRST: i32 = 500;
const ID_FORMAT_PNG: i32 = 600;
const ID_FORMAT_JPEG: i32 = 601;
const ID_QUALITY_FIRST: i32 = 610;
const SETTINGS_WIDTH: i32 = 800;
const SETTINGS_HEIGHT: i32 = 600;
/// Left edge of the content column: page margin + sidebar band + sidebar gap.
const CONTENT_LEFT: i32 = 184;
/// Right edge of the content column — the window keeps the same 60 px right
/// band the 960-wide original had (960 - 900), so the horizontal scroll extent
/// (`CONTENT_RIGHT` + page margin = 764) still fits the client width even when
/// the vertical scrollbar is visible.
const CONTENT_RIGHT: i32 = 740;
/// Scrollable content height: footer band bottom (664 + 32) plus the bottom
/// page margin — the same footer + bottom-air relationship the old
/// `754 = 740 + 14` had, with the margin snapped to `theme::GRID`.
const CONTENT_HEIGHT: i32 = 712;
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

fn move_control(hwnd: HWND, id: i32, x: i32, y: i32, width: i32, height: i32, dpi: u32) {
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
                true,
            );
        }
    }
}

fn create_font_px(dpi: u32, pixels: i32, weight: i32) -> windows::Win32::Graphics::Gdi::HFONT {
    let face = wide_string("Segoe UI");
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
        if let Ok(child) = unsafe { windows::Win32::UI::WindowsAndMessaging::GetDlgItem(hwnd, id) }
        {
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
    if let Ok(control) = unsafe { windows::Win32::UI::WindowsAndMessaging::GetDlgItem(hwnd, id) } {
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

fn card_rects(view: SettingsView) -> &'static [(i32, i32, i32, i32)] {
    // Every card sits between CONTENT_LEFT and CONTENT_RIGHT; tops/bottoms are
    // GRID-snapped and carry `theme::CARD_PADDING` (16) of inner padding.
    match view {
        SettingsView::Simple => &[
            (CONTENT_LEFT, 100, CONTENT_RIGHT, 252),
            (CONTENT_LEFT, 268, CONTENT_RIGHT, 460),
            (CONTENT_LEFT, 476, CONTENT_RIGHT, 612),
        ],
        SettingsView::Advanced => &[
            (CONTENT_LEFT, 100, CONTENT_RIGHT, 276),
            (CONTENT_LEFT, 292, CONTENT_RIGHT, 428),
            (CONTENT_LEFT, 444, CONTENT_RIGHT, 648),
        ],
    }
}

fn paint_settings_surface(hwnd: HWND, state: &SettingsWindowState, hdc: HDC) {
    let mut client = RECT::default();
    unsafe {
        let _ = GetClientRect(hwnd, &mut client);
        let _ = FillRect(hdc, &client, state.background_brush);
    }

    unsafe {
        let _ = windows::Win32::Graphics::Gdi::SetViewportOrgEx(
            hdc,
            -windows::Win32::UI::WindowsAndMessaging::GetScrollPos(
                hwnd,
                windows::Win32::UI::WindowsAndMessaging::SB_HORZ,
            ),
            -windows::Win32::UI::WindowsAndMessaging::GetScrollPos(
                hwnd,
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
    let toggle = (ID_WINDOW_SNAP..=ID_AUTO_INSTALL).contains(&id);
    let disabled = draw.uItemState.contains(CDIS_DISABLED);
    let hot = draw.uItemState.contains(CDIS_HOT) || draw.uItemState.contains(CDIS_SELECTED);
    let fill = if primary {
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
    let border = if checked && !toggle || draw.uItemState.contains(CDIS_FOCUS) {
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
    text_rect.left += scale(12, dpi);
    text_rect.right -= scale(12, dpi);
    if (ID_COLOR_FIRST..ID_COLOR_FIRST + 8).contains(&id) {
        let color =
            crate::annotation::bgra_to_colorref(PRESET_COLORS[(id - ID_COLOR_FIRST) as usize]);
        let x = rect.left + scale(12, dpi);
        let y = (rect.top + rect.bottom - scale(16, dpi)) / 2;
        crate::drawing::with_brush(hdc, color, || {
            crate::drawing::with_pen(hdc, PS_SOLID, 1, color_border(), || unsafe {
                let _ = Ellipse(hdc, x, y, x + scale(16, dpi), y + scale(16, dpi));
            })
        });
        text_rect.left += scale(22, dpi);
    }
    if toggle {
        let x = rect.right - scale(48, dpi);
        let y = (rect.top + rect.bottom - scale(22, dpi)) / 2;
        let color = if checked {
            color_accent()
        } else {
            color_control_fill()
        };
        crate::drawing::with_brush(hdc, color, || {
            crate::drawing::with_pen(hdc, PS_SOLID, 1, color, || unsafe {
                let _ = RoundRect(
                    hdc,
                    x,
                    y,
                    x + scale(40, dpi),
                    y + scale(22, dpi),
                    scale(22, dpi),
                    scale(22, dpi),
                );
            })
        });
        let knob = x + scale(if checked { 21 } else { 3 }, dpi);
        crate::drawing::with_brush(hdc, color_card(), || {
            crate::drawing::with_pen(hdc, PS_SOLID, 1, color_card(), || unsafe {
                let _ = Ellipse(
                    hdc,
                    knob,
                    y + scale(3, dpi),
                    knob + scale(16, dpi),
                    y + scale(19, dpi),
                );
            })
        });
        text_rect.right = x - scale(12, dpi);
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
        700 | 709 | 716 | 717 | 718 | ID_VIEW_SIMPLE | ID_VIEW_ADVANCED | ID_SAVE | ID_CANCEL
    )
}

fn layout_controls(hwnd: HWND, dpi: u32) {
    use crate::theme::{CARD_PADDING, PAGE_MARGIN};
    // Every coordinate below is 96-DPI design pixels snapped to theme::GRID (4).
    const ROW: i32 = crate::theme::CONTROL_HEIGHT;
    const MARGIN: i32 = PAGE_MARGIN;
    const SIDE: i32 = 144; // sidebar band (was 180)
    const GAP: i32 = crate::theme::GRID * 4; // sidebar -> content column
    const LBL: i32 = 96; // property label column
    const COL_W: i32 = CONTENT_RIGHT - CONTENT_LEFT;
    const CTL: i32 = CONTENT_LEFT + CARD_PADDING; // first control x (200)
    const VAL: i32 = CTL + LBL + GAP; // value/chip column x (312)
    const CTL_W: i32 = CONTENT_RIGHT - CARD_PADDING - CTL;
    // Sidebar: brand, subtitle, navigation, version block.
    move_control(hwnd, 700, MARGIN, MARGIN, SIDE, 40, dpi);
    move_control(hwnd, 709, MARGIN, 72, SIDE, 40, dpi);
    move_control(hwnd, ID_VIEW_SIMPLE, MARGIN, 136, SIDE, ROW, dpi);
    move_control(hwnd, ID_VIEW_ADVANCED, MARGIN, 176, SIDE, ROW, dpi);
    move_control(hwnd, 718, MARGIN, 612, SIDE, 76, dpi);
    // Content header (title band) shared by both views.
    move_control(hwnd, 716, CONTENT_LEFT, MARGIN, COL_W, 40, dpi);
    move_control(hwnd, 717, CONTENT_LEFT, 68, COL_W, 24, dpi);
    // Capture card (General).
    move_control(hwnd, 710, CTL, 116, CTL_W, 24, dpi);
    move_control(hwnd, 701, CTL, 156, LBL, 24, dpi);
    move_control(hwnd, ID_HOTKEY_PRINT, VAL, 148, 124, ROW, dpi);
    move_control(hwnd, ID_HOTKEY_CTRL_SHIFT_S, VAL + 132, 148, 124, ROW, dpi);
    move_control(hwnd, ID_HOTKEY_ALT_PRINT, VAL + 264, 148, 148, ROW, dpi);
    move_control(hwnd, 705, CTL, 196, LBL, 40, dpi);
    for i in 0..4 {
        move_control(hwnd, ID_DELAY_FIRST + i, VAL + i * 104, 188, 96, ROW, dpi);
    }
    // Saving card (General).
    move_control(hwnd, 711, CTL, 284, CTL_W, 24, dpi);
    move_control(hwnd, 702, CTL, 324, LBL, 24, dpi);
    move_control(hwnd, ID_FOLDER_LABEL, VAL, 324, 276, 24, dpi);
    move_control(hwnd, ID_BROWSE, 604, 316, 120, ROW, dpi);
    move_control(hwnd, 706, CTL, 364, LBL, 24, dpi);
    move_control(hwnd, ID_FORMAT_PNG, VAL, 356, 96, ROW, dpi);
    move_control(hwnd, ID_FORMAT_JPEG, VAL + 104, 356, 96, ROW, dpi);
    move_control(hwnd, 707, CTL, 404, LBL, 40, dpi);
    for i in 0..3 {
        move_control(
            hwnd,
            ID_QUALITY_FIRST + i,
            VAL + i * 140,
            396,
            132,
            ROW,
            dpi,
        );
    }
    // After capture card (General).
    move_control(hwnd, 712, CTL, 492, CTL_W, 24, dpi);
    move_control(hwnd, ID_WINDOW_SNAP, CTL, 524, CTL_W, ROW, dpi);
    move_control(hwnd, ID_CLOSE_AFTER_ACTION, CTL, 564, CTL_W, ROW, dpi);
    // Annotation defaults card (Editor and system).
    move_control(hwnd, 713, CTL, 116, CTL_W, 24, dpi);
    move_control(hwnd, 703, CTL, 156, LBL, 24, dpi);
    for i in 0..8 {
        move_control(
            hwnd,
            ID_COLOR_FIRST + i,
            VAL + i % 4 * 104,
            148 + i / 4 * 40,
            96,
            ROW,
            dpi,
        );
    }
    move_control(hwnd, 704, CTL, 236, LBL, 24, dpi);
    for i in 0..3 {
        move_control(
            hwnd,
            ID_THICKNESS_FIRST + i,
            VAL + i * 140,
            228,
            132,
            ROW,
            dpi,
        );
    }
    // Windows card (Editor and system).
    move_control(hwnd, 714, CTL, 308, CTL_W, 24, dpi);
    move_control(hwnd, ID_START_WITH_WINDOWS, CTL, 340, CTL_W, ROW, dpi);
    move_control(hwnd, ID_NOTIFY_AFTER_SAVE, CTL, 380, CTL_W, ROW, dpi);
    // Updates card (Editor and system).
    move_control(hwnd, 715, CTL, 460, CTL_W, 24, dpi);
    move_control(hwnd, ID_CHECK_UPDATES, CTL, 492, CTL_W, ROW, dpi);
    move_control(hwnd, ID_AUTO_INSTALL, CTL, 532, CTL_W, ROW, dpi);
    move_control(hwnd, ID_CHECK_UPDATE, CTL, 572, 148, ROW, dpi);
    move_control(hwnd, 708, CTL + 164, 572, 360, 60, dpi);
    // Footer band (both views).
    move_control(hwnd, ID_CANCEL, 428, 664, 128, ROW, dpi);
    move_control(hwnd, ID_SAVE, 568, 664, 172, ROW, dpi);
}

fn update_scrollbars(hwnd: HWND, dpi: u32) {
    use windows::Win32::UI::WindowsAndMessaging::*;
    thread_local! { static UPDATING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) }; }
    if UPDATING.with(|flag| flag.replace(true)) {
        return;
    }
    let mut client = RECT::default();
    unsafe {
        let _ = GetClientRect(hwnd, &mut client);
    }
    let style = unsafe { GetWindowLongPtrW(hwnd, GWL_STYLE) } as u32;
    let bar_width = unsafe { GetSystemMetrics(SM_CXVSCROLL) };
    let bar_height = unsafe { GetSystemMetrics(SM_CYHSCROLL) };
    let full_width = client.right
        + if style & WS_VSCROLL.0 != 0 {
            bar_width
        } else {
            0
        };
    let full_height = client.bottom
        + if style & WS_HSCROLL.0 != 0 {
            bar_height
        } else {
            0
        };
    let width = scale(CONTENT_RIGHT + crate::theme::PAGE_MARGIN, dpi);
    let height = scale(CONTENT_HEIGHT, dpi);
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
            nPos: unsafe { GetScrollPos(hwnd, bar) }.clamp(0, (length - page).max(0)),
            nTrackPos: 0,
        };
        unsafe {
            SetScrollInfo(hwnd, bar, &info, true);
        }
    }
    layout_controls(hwnd, dpi);
    unsafe {
        let _ = InvalidateRect(hwnd, None, true);
    }
    UPDATING.with(|flag| flag.set(false));
}

fn scroll_by(hwnd: HWND, horizontal: bool, delta: i32) {
    use windows::Win32::UI::WindowsAndMessaging::*;
    let bar = if horizontal { SB_HORZ } else { SB_VERT };
    let mut info = SCROLLINFO {
        cbSize: std::mem::size_of::<SCROLLINFO>() as u32,
        fMask: SIF_ALL,
        ..Default::default()
    };
    unsafe {
        let _ = GetScrollInfo(hwnd, bar, &mut info);
    }
    let position = (info.nPos + delta).clamp(0, (info.nMax - info.nPage as i32 + 1).max(0));
    unsafe {
        SetScrollPos(hwnd, bar, position, true);
    }
    let dpi = unsafe { windows::Win32::UI::HiDpi::GetDpiForWindow(hwnd) }.max(96);
    layout_controls(hwnd, dpi);
    unsafe {
        let _ = InvalidateRect(hwnd, None, true);
    }
}

pub fn ensure_focus_visible(hwnd: HWND) {
    use windows::Win32::UI::WindowsAndMessaging::IsChild;
    let focus = unsafe { windows::Win32::UI::Input::KeyboardAndMouse::GetFocus() };
    if !unsafe { IsChild(hwnd, focus).as_bool() } {
        return;
    }
    let mut rect = RECT::default();
    let mut client = RECT::default();
    unsafe {
        let _ = GetWindowRect(focus, &mut rect);
        let _ = GetClientRect(hwnd, &mut client);
    }
    let mut point = POINT {
        x: rect.left,
        y: rect.top,
    };
    unsafe {
        let _ = windows::Win32::Graphics::Gdi::ScreenToClient(hwnd, &mut point);
    }
    let bottom = point.y + rect.bottom - rect.top;
    let right = point.x + rect.right - rect.left;
    if point.y < 0 {
        scroll_by(hwnd, false, point.y - 8);
    } else if bottom > client.bottom {
        scroll_by(hwnd, false, bottom - client.bottom + 8);
    }
    if point.x < 0 {
        scroll_by(hwnd, true, point.x - 8);
    } else if right > client.right {
        scroll_by(hwnd, true, right - client.right + 8);
    }
}

fn show_controls(hwnd: HWND, ids: &[i32], show: bool) {
    for id in ids {
        if let Ok(control) =
            unsafe { windows::Win32::UI::WindowsAndMessaging::GetDlgItem(hwnd, *id) }
        {
            unsafe {
                let _ = ShowWindow(control, if show { SW_SHOW } else { SW_HIDE });
            }
        }
    }
}

fn set_active_view(hwnd: HWND, view: SettingsView) {
    const SIMPLE_CONTROLS: &[i32] = &[
        710,
        711,
        712,
        701,
        702,
        705,
        706,
        707,
        ID_FOLDER_LABEL,
        ID_BROWSE,
        ID_HOTKEY_PRINT,
        ID_HOTKEY_CTRL_SHIFT_S,
        ID_HOTKEY_ALT_PRINT,
        ID_FORMAT_PNG,
        ID_FORMAT_JPEG,
        ID_WINDOW_SNAP,
        ID_CLOSE_AFTER_ACTION,
    ];
    const ADVANCED_CONTROLS: &[i32] = &[
        713,
        714,
        715,
        703,
        704,
        ID_START_WITH_WINDOWS,
        ID_NOTIFY_AFTER_SAVE,
        ID_CHECK_UPDATES,
        ID_AUTO_INSTALL,
        ID_CHECK_UPDATE,
        708,
    ];

    let simple = view == SettingsView::Simple;
    show_controls(hwnd, SIMPLE_CONTROLS, simple);
    show_controls(hwnd, ADVANCED_CONTROLS, !simple);
    for id in ID_DELAY_FIRST..=ID_DELAY_FIRST + 3 {
        show_controls(hwnd, &[id], simple);
    }
    for id in ID_QUALITY_FIRST..=ID_QUALITY_FIRST + 2 {
        show_controls(hwnd, &[id], simple);
    }
    for id in ID_COLOR_FIRST..=ID_COLOR_FIRST + PRESET_COLORS.len() as i32 - 1 {
        show_controls(hwnd, &[id], !simple);
    }
    for id in ID_THICKNESS_FIRST..=ID_THICKNESS_FIRST + PRESET_THICKNESSES.len() as i32 - 1 {
        show_controls(hwnd, &[id], !simple);
    }
    check_radio(
        hwnd,
        ID_VIEW_SIMPLE,
        ID_VIEW_ADVANCED,
        if simple {
            ID_VIEW_SIMPLE
        } else {
            ID_VIEW_ADVANCED
        },
    );
    unsafe {
        let _ = InvalidateRect(hwnd, None, true);
    }
}

fn set_check(hwnd: HWND, id: i32, checked: bool) {
    unsafe {
        let _ = windows::Win32::UI::Controls::CheckDlgButton(
            hwnd,
            id,
            if checked {
                windows::Win32::UI::Controls::BST_CHECKED
            } else {
                windows::Win32::UI::Controls::BST_UNCHECKED
            },
        );
    }
}

fn check_radio(hwnd: HWND, first: i32, last: i32, selected: i32) {
    unsafe {
        let _ = windows::Win32::UI::Controls::CheckRadioButton(hwnd, first, last, selected);
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

/// Shows a custom (non-preset) value beside its existing section label.
fn set_label_note(hwnd: HWND, id: i32, base: &str, note: Option<String>) {
    let text = match note {
        Some(note) => format!("{base} ({note})"),
        None => base.to_owned(),
    };
    let text = wide_string(&text);
    if let Ok(child) = unsafe { windows::Win32::UI::WindowsAndMessaging::GetDlgItem(hwnd, id) } {
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
        if let Ok(control) =
            unsafe { windows::Win32::UI::WindowsAndMessaging::GetDlgItem(hwnd, id) }
        {
            unsafe {
                let _ = EnableWindow(control, enabled);
            }
        }
    }
}

fn initialize_control_values(hwnd: HWND, settings: &Settings) {
    // Custom (non-preset) values keep their originals in state and show no checked
    // preset; the section label carries the actual value instead.
    let hotkey_id = if settings
        .hotkey
        .description
        .eq_ignore_ascii_case("Ctrl+Shift+S")
    {
        Some(ID_HOTKEY_CTRL_SHIFT_S)
    } else if settings
        .hotkey
        .description
        .eq_ignore_ascii_case("Alt+PrintScreen")
    {
        Some(ID_HOTKEY_ALT_PRINT)
    } else if settings
        .hotkey
        .description
        .eq_ignore_ascii_case("PrintScreen")
    {
        Some(ID_HOTKEY_PRINT)
    } else {
        None
    };
    set_radio_group(hwnd, ID_HOTKEY_PRINT, ID_HOTKEY_ALT_PRINT, hotkey_id);
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
    let thickness_id = PRESET_THICKNESSES
        .iter()
        .position(|value| *value == settings.default_thickness)
        .map(|index| ID_THICKNESS_FIRST + index as i32);
    set_radio_group(
        hwnd,
        ID_THICKNESS_FIRST,
        ID_THICKNESS_FIRST + PRESET_THICKNESSES.len() as i32 - 1,
        thickness_id,
    );
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
        "JPEG quality",
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
    set_check(
        hwnd,
        ID_AUTO_INSTALL,
        settings.install_updates_automatically,
    );
    update_folder_label(hwnd, &settings.save_directory);
}

fn update_folder_label(hwnd: HWND, path: &Path) {
    let label = wide_string(&path.display().to_string());
    if let Ok(child) =
        unsafe { windows::Win32::UI::WindowsAndMessaging::GetDlgItem(hwnd, ID_FOLDER_LABEL) }
    {
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
        BS_AUTOCHECKBOX, BS_AUTORADIOBUTTON, BS_DEFPUSHBUTTON, BS_PUSHBUTTON, BS_PUSHLIKE,
        WS_GROUP, WS_TABSTOP,
    };
    let label = |id, text| create_control(hwnd, w!("STATIC"), text, Default::default(), id);

    label(700, "isolmaSS")?;
    label(716, "Capture settings")?;
    label(717, "Choose how you capture, edit, and save.")?;
    label(
        718,
        concat!(
            "Version ",
            env!("CARGO_PKG_VERSION"),
            "\nPrivate by design. MIT licensed."
        ),
    )?;
    label(709, "Your capture studio.\nMade for Windows.")?;
    create_button(
        hwnd,
        ID_VIEW_SIMPLE,
        "General",
        BS_AUTORADIOBUTTON | BS_PUSHLIKE | WS_GROUP.0 as i32 | WS_TABSTOP.0 as i32,
    )?;
    create_button(
        hwnd,
        ID_VIEW_ADVANCED,
        "Editor and system",
        BS_AUTORADIOBUTTON | BS_PUSHLIKE | WS_TABSTOP.0 as i32,
    )?;

    label(710, "Capture")?;
    label(701, "Global hotkey")?;
    create_button(
        hwnd,
        ID_HOTKEY_PRINT,
        "PrintScreen",
        BS_AUTORADIOBUTTON | WS_GROUP.0 as i32 | WS_TABSTOP.0 as i32,
    )?;
    create_button(
        hwnd,
        ID_HOTKEY_CTRL_SHIFT_S,
        "Ctrl+Shift+S",
        BS_AUTORADIOBUTTON | WS_TABSTOP.0 as i32,
    )?;
    create_button(
        hwnd,
        ID_HOTKEY_ALT_PRINT,
        "Alt+PrintScreen",
        BS_AUTORADIOBUTTON | WS_TABSTOP.0 as i32,
    )?;
    label(705, "Capture delay")?;
    for (index, name) in ["None", "1 second", "3 seconds", "5 seconds"]
        .into_iter()
        .enumerate()
    {
        create_button(
            hwnd,
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

    label(711, "Saving")?;
    label(702, "Save folder")?;
    create_control(
        hwnd,
        w!("STATIC"),
        "",
        windows::Win32::UI::WindowsAndMessaging::WINDOW_STYLE(0x0000_4000),
        ID_FOLDER_LABEL,
    )?;
    create_button(
        hwnd,
        ID_BROWSE,
        "Choose folder",
        BS_PUSHBUTTON | WS_TABSTOP.0 as i32,
    )?;
    label(706, "Image format")?;
    create_button(
        hwnd,
        ID_FORMAT_PNG,
        "PNG",
        BS_AUTORADIOBUTTON | WS_GROUP.0 as i32 | WS_TABSTOP.0 as i32,
    )?;
    create_button(
        hwnd,
        ID_FORMAT_JPEG,
        "JPEG",
        BS_AUTORADIOBUTTON | WS_TABSTOP.0 as i32,
    )?;
    label(707, "JPEG quality")?;
    for (index, quality) in [80, 90, 100].into_iter().enumerate() {
        let label = quality.to_string();
        create_button(
            hwnd,
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

    label(712, "After capture")?;
    create_button(
        hwnd,
        ID_WINDOW_SNAP,
        "Snap to a window with one click",
        BS_AUTOCHECKBOX | WS_GROUP.0 as i32 | WS_TABSTOP.0 as i32,
    )?;
    create_button(
        hwnd,
        ID_CLOSE_AFTER_ACTION,
        "Close the editor after saving or copying",
        BS_AUTOCHECKBOX | WS_TABSTOP.0 as i32,
    )?;

    label(713, "Annotation defaults")?;
    label(703, "Default color")?;
    for (index, name) in [
        "Red", "Orange", "Yellow", "Green", "Blue", "Purple", "White", "Black",
    ]
    .into_iter()
    .enumerate()
    {
        create_button(
            hwnd,
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
    label(704, "Line thickness")?;
    for (index, value) in PRESET_THICKNESSES.iter().enumerate() {
        let label = format!("{value} px");
        create_button(
            hwnd,
            ID_THICKNESS_FIRST + index as i32,
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

    label(714, "Windows")?;
    create_button(
        hwnd,
        ID_START_WITH_WINDOWS,
        "Start isolmaSS when I sign in to Windows",
        BS_AUTOCHECKBOX | WS_GROUP.0 as i32 | WS_TABSTOP.0 as i32,
    )?;
    create_button(
        hwnd,
        ID_NOTIFY_AFTER_SAVE,
        "Show a notification after saving",
        BS_AUTOCHECKBOX | WS_TABSTOP.0 as i32,
    )?;

    label(715, "Updates")?;
    create_button(
        hwnd,
        ID_CHECK_UPDATES,
        "Check for updates automatically",
        BS_AUTOCHECKBOX | WS_GROUP.0 as i32 | WS_TABSTOP.0 as i32,
    )?;
    create_button(
        hwnd,
        ID_AUTO_INSTALL,
        "Automatically install verified signed updates",
        BS_AUTOCHECKBOX | WS_TABSTOP.0 as i32,
    )?;
    create_button(
        hwnd,
        ID_CHECK_UPDATE,
        "Check for updates",
        BS_PUSHBUTTON | WS_TABSTOP.0 as i32,
    )?;
    label(
        708,
        "Manual and automatic installs require HTTPS, a GitHub SHA-256 digest, and a valid Authenticode signature.",
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
        ID_VIEW_SIMPLE => {
            state.active_view = SettingsView::Simple;
            set_active_view(hwnd, state.active_view);
        }
        ID_VIEW_ADVANCED => {
            state.active_view = SettingsView::Advanced;
            set_active_view(hwnd, state.active_view);
        }
        ID_HOTKEY_PRINT => state.settings.hotkey = HotkeyConfig::default(),
        ID_HOTKEY_CTRL_SHIFT_S => state.settings.hotkey = HotkeyConfig::fallback(),
        ID_HOTKEY_ALT_PRINT => state.settings.hotkey = HotkeyConfig::alt_print_screen(),
        ID_COLOR_FIRST..=307 => {
            state.settings.default_color = PRESET_COLORS[(id - ID_COLOR_FIRST) as usize];
        }
        ID_THICKNESS_FIRST..=322 => {
            state.settings.default_thickness =
                PRESET_THICKNESSES[(id - ID_THICKNESS_FIRST) as usize];
        }
        ID_WINDOW_SNAP => state.settings.enable_window_snap = !state.settings.enable_window_snap,
        ID_CLOSE_AFTER_ACTION => {
            state.settings.close_after_action = !state.settings.close_after_action;
        }
        ID_START_WITH_WINDOWS => {
            state.settings.start_with_windows = !state.settings.start_with_windows;
        }
        ID_NOTIFY_AFTER_SAVE => {
            state.settings.notify_after_save = !state.settings.notify_after_save;
        }
        ID_CHECK_UPDATES => {
            state.settings.check_updates_automatically =
                !state.settings.check_updates_automatically;
            if !state.settings.check_updates_automatically {
                state.settings.install_updates_automatically = false;
                set_check(hwnd, ID_AUTO_INSTALL, false);
            }
        }
        ID_AUTO_INSTALL => {
            state.settings.install_updates_automatically =
                !state.settings.install_updates_automatically;
            if state.settings.install_updates_automatically {
                state.settings.check_updates_automatically = true;
                set_check(hwnd, ID_CHECK_UPDATES, true);
            }
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
        ID_CHECK_UPDATE => crate::updater::run_manual_update_check(),
        ID_CANCEL => unsafe {
            let _ = DestroyWindow(hwnd);
        },
        _ => {}
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
        windows::Win32::UI::WindowsAndMessaging::WM_NCDESTROY => {
            unsafe {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            }
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }
        windows::Win32::UI::WindowsAndMessaging::WM_SIZE => {
            if !state_ptr.is_null() {
                update_scrollbars(hwnd, unsafe { (*state_ptr).dpi });
            }
            LRESULT(0)
        }
        windows::Win32::UI::WindowsAndMessaging::WM_MOUSEWHEEL => {
            scroll_by(hwnd, false, -((wparam.0 >> 16) as u16 as i16 as i32) / 2);
            LRESULT(0)
        }
        windows::Win32::UI::WindowsAndMessaging::WM_VSCROLL
        | windows::Win32::UI::WindowsAndMessaging::WM_HSCROLL => {
            use windows::Win32::UI::WindowsAndMessaging::*;
            let horizontal = msg == WM_HSCROLL;
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
            scroll_by(hwnd, horizontal, delta);
            LRESULT(0)
        }
        windows::Win32::UI::WindowsAndMessaging::WM_NOTIFY => {
            use windows::Win32::UI::Controls::*;
            if !state_ptr.is_null() && lparam.0 != 0 {
                let header = unsafe { &*(lparam.0 as *const NMHDR) };
                if header.code == NM_CUSTOMDRAW {
                    let draw = unsafe { &*(lparam.0 as *const NMCUSTOMDRAW) };
                    if draw.dwDrawStage == CDDS_PREPAINT {
                        draw_settings_button(draw, unsafe { &*state_ptr });
                        return LRESULT(CDRF_SKIPDEFAULT as isize);
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
                let mut client = RECT::default();
                unsafe {
                    let _ = GetClientRect(hwnd, &mut client);
                    let _ = FillRect(HDC(wparam.0 as *mut _), &client, state.background_brush);
                }
                return LRESULT(1);
            }
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
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
                } else if matches!(id, 708 | 709) {
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
        windows::Win32::UI::WindowsAndMessaging::WM_COMMAND => {
            if state_ptr.is_null() || wparam.0 >> 16 != 0 {
                return LRESULT(0);
            }
            let id = (wparam.0 & 0xffff) as i32;
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
                let settings = unsafe { (*state_ptr).settings.clone() };
                let result = save_settings(&settings);
                match result {
                    Ok(()) => unsafe {
                        (*state_ptr).saved = true;
                        let _ = DestroyWindow(hwnd);
                    },
                    Err(error) => crate::ui::error(hwnd, "Settings could not be saved", &error),
                }
            } else {
                apply_button_action(hwnd, unsafe { &mut *state_ptr }, id);
                if unsafe { IsWindow(hwnd).as_bool() } {
                    initialize_control_values(hwnd, unsafe { &(*state_ptr).settings });
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
                set_control_font(hwnd, 716, new_title_font);
                for id in 710..=715 {
                    set_control_font(hwnd, id, new_heading_font);
                }
                center_dialog(hwnd, None);
                update_scrollbars(hwnd, state.dpi);
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
        if let Ok(first) = windows::Win32::UI::WindowsAndMessaging::GetDlgItem(hwnd, ID_VIEW_SIMPLE)
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
            dwICC: ICC_STANDARD_CLASSES,
        };
        let _ = InitCommonControlsEx(&init);
    }
    crate::diagnostics::record("settings", "Opening settings window");
    register_settings_class()?;
    let mut state = Box::new(SettingsWindowState {
        settings: current.clone(),
        saved: false,
        active_view: SettingsView::Simple,
        dpi: 96,
        font: Default::default(),
        title_font: Default::default(),
        heading_font: Default::default(),
        background_brush: unsafe { CreateSolidBrush(color_background()) },
        card_brush: unsafe { CreateSolidBrush(color_card()) },
    });
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
                | windows::Win32::UI::WindowsAndMessaging::WS_VSCROLL
                | windows::Win32::UI::WindowsAndMessaging::WS_HSCROLL
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
    crate::diagnostics::record("settings", "Controls created");
    set_controls_font(hwnd, state.font);
    set_control_font(hwnd, 700, state.title_font);
    set_control_font(hwnd, 716, state.title_font);
    for id in 710..=715 {
        set_control_font(hwnd, id, state.heading_font);
    }
    layout_controls(hwnd, state.dpi);
    initialize_control_values(hwnd, &state.settings);
    set_active_view(hwnd, state.active_view);
    center_dialog(hwnd, owner);
    update_scrollbars(hwnd, state.dpi);
    unsafe {
        let _ = ShowWindow(hwnd, SW_SHOW);
    }
    activate_dialog(hwnd);
    crate::diagnostics::record("settings", "Window activated");

    crate::ui::window_loop(hwnd, crate::ui::WindowKind::Settings)?;
    if state.saved {
        Ok(Some(state.settings.clone()))
    } else {
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
