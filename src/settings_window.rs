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
    WM_CTLCOLORSTATIC, WM_DESTROY, WM_DWMCOLORIZATIONCOLORCHANGED, WM_ERASEBKGND, WM_KEYDOWN,
    WM_PAINT, WM_SETTINGCHANGE, WNDCLASSEXW,
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
const SETTINGS_WIDTH: i32 = 660;
const SETTINGS_HEIGHT: i32 = 580;
/// Smallest window the layout accepts before it has to scroll.
const SETTINGS_MIN_WIDTH: i32 = 520;
const SETTINGS_MIN_HEIGHT: i32 = 430;
/// Sidebar band width (brand, navigation, version block).
const SIDEBAR_WIDTH: i32 = 108;
/// Left edge of the content column: page margin + sidebar band + gap.
const CONTENT_LEFT: i32 = 140;
/// Card column width inside the scroll container.
const CONTENT_WIDTH: i32 = 480;
/// Scroll extent of the tallest view, in scroll-container design pixels.
const CONTENT_HEIGHT: i32 = 416;
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

/// Card rectangles in scroll-container design pixels: x runs from the card
/// column's left edge, y from the top of the scrollable content. Every card
/// carries `theme::CARD_PADDING` (12) of inner padding and a 16 px section
/// label, so the row rhythm below is 12 / 16 / 6 / 26.
fn card_rects(view: SettingsView) -> &'static [(i32, i32, i32, i32)] {
    const W: i32 = CONTENT_WIDTH;
    match view {
        SettingsView::Simple => &[(0, 0, W, 104), (0, 116, W, 252), (0, 264, W, 368)],
        SettingsView::Advanced => &[(0, 0, W, 104), (0, 116, W, 220), (0, 232, W, 404)],
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
        WM_COMMAND | WM_NOTIFY | WM_CTLCOLORSTATIC | WM_CTLCOLORBTN => {
            let parent = unsafe { windows::Win32::UI::WindowsAndMessaging::GetParent(hwnd) }
                .unwrap_or_default();
            LRESULT(unsafe { SendMessageW(parent, message, wparam, lparam) }.0)
        }
        _ => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
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
        return;
    }
    if toggle {
        // Compact Fluent switch: 32 x 18 track with a 12 px thumb.
        let track_width = scale(32, dpi);
        let track_height = scale(18, dpi);
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
        let thumb = x + scale(if checked { 17 } else { 3 }, dpi);
        let thumb_color = if checked { color_card() } else { color_muted() };
        crate::drawing::with_brush(hdc, thumb_color, || {
            crate::drawing::with_pen(hdc, PS_SOLID, 1, thumb_color, || unsafe {
                let _ = Ellipse(
                    hdc,
                    thumb,
                    y + scale(3, dpi),
                    thumb + scale(12, dpi),
                    y + scale(15, dpi),
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
        700 | 709 | 716 | 717 | 718 | ID_VIEW_SIMPLE | ID_VIEW_ADVANCED | ID_SAVE | ID_CANCEL
    )
}

/// Header band: page margin + title + subtitle + air below.
const HEADER_BOTTOM: i32 = 68;
/// Footer band: control row + page margin.
const FOOTER_HEIGHT: i32 = 42;
/// Air between the scroll container and the footer band.
const FOOTER_GAP: i32 = 10;
/// Property label column inside a card.
const LABEL_WIDTH: i32 = 92;

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
    let row = CONTROL_HEIGHT;
    let side = SIDEBAR_WIDTH;
    let content_left = CONTENT_LEFT;
    let content_right = (width - PAGE_MARGIN).max(content_left + 160);
    // Sidebar: brand, subtitle, navigation, version block.
    move_control(hwnd, 700, PAGE_MARGIN, PAGE_MARGIN, side, 20, dpi);
    move_control(hwnd, 709, PAGE_MARGIN, PAGE_MARGIN + 24, side, 32, dpi);
    move_control(
        hwnd,
        ID_VIEW_SIMPLE,
        PAGE_MARGIN,
        PAGE_MARGIN + 68,
        side,
        row,
        dpi,
    );
    move_control(
        hwnd,
        ID_VIEW_ADVANCED,
        PAGE_MARGIN,
        PAGE_MARGIN + 68 + row + 6,
        side,
        row,
        dpi,
    );
    move_control(
        hwnd,
        718,
        PAGE_MARGIN,
        height - PAGE_MARGIN - 32,
        side,
        32,
        dpi,
    );
    // Header shared by both views.
    move_control(
        hwnd,
        716,
        content_left,
        PAGE_MARGIN,
        content_right - content_left,
        20,
        dpi,
    );
    move_control(
        hwnd,
        717,
        content_left,
        PAGE_MARGIN + 22,
        content_right - content_left,
        16,
        dpi,
    );
    // Footer pinned to the bottom edge of the client area.
    let footer_y = height - PAGE_MARGIN - row;
    move_control(hwnd, ID_SAVE, content_right - 108, footer_y, 108, row, dpi);
    move_control(
        hwnd,
        ID_CANCEL,
        content_right - 108 - 8 - 84,
        footer_y,
        84,
        row,
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
    // The container runs to the client's right edge so its scroll bar sits
    // flush with the frame and the 480 px card column still fits beside it.
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
    let pad = CARD_PADDING; // 12
    let row = CONTROL_HEIGHT; // 26
    let section = 16; // section label line
    let label = LABEL_WIDTH; // property label column
    let chips = pad + label + 8; // first chip column
    let right = CONTENT_WIDTH - pad;
    let chip_area = right - chips;
    let full = CONTENT_WIDTH - 2 * pad;
    let place =
        |id: i32, x: i32, y: i32, w: i32, h: i32| move_control(container, id, x, y, w, h, dpi);
    let heading = |id: i32, top: i32| place(id, pad, top, full, section);
    let row_label = |id: i32, top: i32| place(id, pad, top + (row - 16) / 2, label, 16);

    // Capture card (General).
    heading(710, 12);
    row_label(701, 34);
    place(ID_HOTKEY_PRINT, chips, 34, 92, row);
    place(ID_HOTKEY_CTRL_SHIFT_S, chips + 98, 34, 96, row);
    place(ID_HOTKEY_ALT_PRINT, chips + 200, 34, 112, row);
    row_label(705, 66);
    for index in 0..4 {
        place(ID_DELAY_FIRST + index, chips + index * 78, 66, 72, row);
    }
    // Saving card (General).
    heading(711, 128);
    row_label(702, 150);
    place(ID_FOLDER_LABEL, chips, 154, chip_area - 100, 18);
    place(ID_BROWSE, right - 92, 150, 92, row);
    row_label(706, 182);
    place(ID_FORMAT_PNG, chips, 182, 72, row);
    place(ID_FORMAT_JPEG, chips + 78, 182, 72, row);
    row_label(707, 214);
    for index in 0..3 {
        place(ID_QUALITY_FIRST + index, chips + index * 78, 214, 72, row);
    }
    // After capture card (General).
    heading(712, 276);
    place(ID_WINDOW_SNAP, pad, 298, full, row);
    place(ID_CLOSE_AFTER_ACTION, pad, 330, full, row);
    // Annotation defaults card (Editor and system).
    heading(713, 12);
    row_label(703, 34);
    for index in 0..8 {
        // Swatch circles: 8 fit one row, so the color row stays a single line.
        place(ID_COLOR_FIRST + index, chips + index * 44, 34, 44, row);
    }
    row_label(704, 66);
    for index in 0..3 {
        place(ID_THICKNESS_FIRST + index, chips + index * 78, 66, 72, row);
    }
    // Windows card (Editor and system).
    heading(714, 128);
    place(ID_START_WITH_WINDOWS, pad, 150, full, row);
    place(ID_NOTIFY_AFTER_SAVE, pad, 182, full, row);
    // Updates card (Editor and system).
    heading(715, 244);
    place(ID_CHECK_UPDATES, pad, 266, full, row);
    place(ID_AUTO_INSTALL, pad, 298, full, row);
    place(ID_CHECK_UPDATE, chips, 330, 132, row);
    place(708, chips, 362, chip_area, 30);
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
            nPos: unsafe { GetScrollPos(container, bar) }.clamp(0, (length - page).max(0)),
            nTrackPos: 0,
        };
        unsafe {
            SetScrollInfo(container, bar, &info, true);
        }
    }
    layout_content(container, dpi);
    unsafe {
        let _ = InvalidateRect(container, None, true);
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
    unsafe {
        SetScrollPos(container, bar, position, true);
    }
    let dpi = unsafe { windows::Win32::UI::HiDpi::GetDpiForWindow(container) }.max(96);
    layout_content(container, dpi);
    unsafe {
        let _ = InvalidateRect(container, None, true);
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

fn show_controls(hwnd: HWND, ids: &[i32], show: bool) {
    for id in ids {
        if let Some(control) = control(hwnd, *id) {
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

/// Shows a custom (non-preset) value beside its existing section label.
fn set_label_note(hwnd: HWND, id: i32, base: &str, note: Option<String>) {
    let text = match note {
        Some(note) => format!("{base} ({note})"),
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
    label(hwnd, 716, "Capture settings")?;
    label(hwnd, 717, "Choose how you capture, edit, and save.")?;
    label(
        hwnd,
        718,
        concat!(
            "Version ",
            env!("CARGO_PKG_VERSION"),
            "\nPrivate by design. MIT licensed."
        ),
    )?;
    label(hwnd, 709, "Your capture studio.\nMade for Windows.")?;
    create_button(
        hwnd,
        ID_VIEW_SIMPLE,
        "General",
        BS_AUTORADIOBUTTON | BS_PUSHLIKE | WS_GROUP.0 as i32 | WS_TABSTOP.0 as i32,
    )?;
    create_button(
        hwnd,
        ID_VIEW_ADVANCED,
        "Editor",
        BS_AUTORADIOBUTTON | BS_PUSHLIKE | WS_TABSTOP.0 as i32,
    )?;

    label(content, 710, "Capture")?;
    label(content, 701, "Global hotkey")?;
    create_button(
        content,
        ID_HOTKEY_PRINT,
        "PrintScreen",
        BS_AUTORADIOBUTTON | WS_GROUP.0 as i32 | WS_TABSTOP.0 as i32,
    )?;
    create_button(
        content,
        ID_HOTKEY_CTRL_SHIFT_S,
        "Ctrl+Shift+S",
        BS_AUTORADIOBUTTON | WS_TABSTOP.0 as i32,
    )?;
    create_button(
        content,
        ID_HOTKEY_ALT_PRINT,
        "Alt+PrintScreen",
        BS_AUTORADIOBUTTON | WS_TABSTOP.0 as i32,
    )?;
    label(content, 705, "Capture delay")?;
    for (index, name) in ["None", "1 second", "3 seconds", "5 seconds"]
        .into_iter()
        .enumerate()
    {
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
    label(content, 702, "Save folder")?;
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
        "Choose folder",
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
    label(content, 707, "JPEG quality")?;
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

    label(content, 712, "After capture")?;
    create_button(
        content,
        ID_WINDOW_SNAP,
        "Snap to a window with one click",
        BS_AUTOCHECKBOX | WS_GROUP.0 as i32 | WS_TABSTOP.0 as i32,
    )?;
    create_button(
        content,
        ID_CLOSE_AFTER_ACTION,
        "Close the editor after saving or copying",
        BS_AUTOCHECKBOX | WS_TABSTOP.0 as i32,
    )?;

    label(content, 713, "Annotation defaults")?;
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
    label(content, 704, "Line thickness")?;
    for (index, value) in PRESET_THICKNESSES.iter().enumerate() {
        let label = format!("{value} px");
        create_button(
            content,
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

    label(content, 714, "Windows")?;
    create_button(
        content,
        ID_START_WITH_WINDOWS,
        "Start isolmaSS when I sign in to Windows",
        BS_AUTOCHECKBOX | WS_GROUP.0 as i32 | WS_TABSTOP.0 as i32,
    )?;
    create_button(
        content,
        ID_NOTIFY_AFTER_SAVE,
        "Show a notification after saving",
        BS_AUTOCHECKBOX | WS_TABSTOP.0 as i32,
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
        ID_AUTO_INSTALL,
        "Automatically install verified signed updates",
        BS_AUTOCHECKBOX | WS_TABSTOP.0 as i32,
    )?;
    create_button(
        content,
        ID_CHECK_UPDATE,
        "Check for updates",
        BS_PUSHBUTTON | WS_TABSTOP.0 as i32,
    )?;
    label(
        content,
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
            // Theme/accent broadcast: re-read the registry caches, rebuild
            // the theme-colored brushes and repaint with the new palette.
            let name = if lparam.0 == 0 {
                String::new()
            } else {
                unsafe {
                    let pointer = lparam.0 as *const u16;
                    let mut length = 0;
                    while *pointer.add(length) != 0 {
                        length += 1;
                    }
                    String::from_utf16_lossy(std::slice::from_raw_parts(pointer, length))
                }
            };
            if !state_ptr.is_null() {
                let state = unsafe { &mut *state_ptr };
                if name.eq_ignore_ascii_case("AppsUseLightTheme") {
                    crate::theme::invalidate_theme_cache();
                    let current = crate::theme::theme();
                    crate::theme::set_theme(current);
                    unsafe {
                        crate::theme::apply_window_theme(
                            hwnd,
                            current == crate::theme::Theme::Dark,
                            true,
                        );
                    }
                } else {
                    // Accent colors live under HKCU\...\Explorer\Accent and
                    // arrive as plain WM_SETTINGCHANGE broadcasts as well.
                    crate::theme::invalidate_accent();
                }
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
    register_scroll_class()?;
    let mut state = Box::new(SettingsWindowState {
        settings: current.clone(),
        saved: false,
        active_view: SettingsView::Simple,
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
    crate::diagnostics::record("settings", "Controls created");
    set_controls_font(hwnd, state.font);
    set_control_font(hwnd, 700, state.title_font);
    set_control_font(hwnd, 716, state.title_font);
    for id in 710..=715 {
        set_control_font(hwnd, id, state.heading_font);
    }
    layout_chrome(hwnd, state.dpi);
    position_container(hwnd, state.dpi);
    initialize_control_values(hwnd, &state.settings);
    set_active_view(hwnd, state.active_view);
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
