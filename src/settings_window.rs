use crate::hotkey::HotkeyConfig;
use crate::settings::{PRESET_COLORS, PRESET_THICKNESSES, SaveFormat, Settings};
use std::path::{Path, PathBuf};
use windows::Win32::Foundation::{
    COLORREF, ERROR_CLASS_ALREADY_EXISTS, GetLastError, HINSTANCE, HWND, LPARAM, LRESULT, POINT,
    RECT, WPARAM,
};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, COLOR_WINDOW, CreatePen, CreateSolidBrush, DeleteObject, EndPaint, FillRect,
    GetMonitorInfoW, HBRUSH, HDC, HGDIOBJ, InvalidateRect, MONITOR_DEFAULTTONEAREST,
    MONITOR_DEFAULTTOPRIMARY, MONITORINFO, MonitorFromPoint, MonitorFromWindow, PAINTSTRUCT,
    PS_SOLID, RoundRect, SelectObject, SetBkMode, SetTextColor, TRANSPARENT,
};
use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    EnableWindow, IsWindowEnabled, SetActiveWindow, SetFocus, VK_ESCAPE,
};
use windows::Win32::UI::WindowsAndMessaging::{
    BringWindowToTop, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
    GWLP_USERDATA, GetClientRect, GetDlgCtrlID, GetForegroundWindow, GetMessageW,
    GetWindowLongPtrW, GetWindowRect, GetWindowThreadProcessId, IDC_ARROW, IsWindow, MSG,
    PM_REMOVE, PeekMessageW, PostQuitMessage, RegisterClassExW, SW_HIDE, SW_SHOW,
    SetForegroundWindow, SetWindowLongPtrW, ShowWindow, TranslateMessage, WM_CLOSE, WM_CTLCOLORBTN,
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
    original_start_with_windows: bool,
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
const SETTINGS_WIDTH: i32 = 700;
const SETTINGS_HEIGHT: i32 = 620;
const COLOR_BACKGROUND: COLORREF = COLORREF(0x00f7f5f2);
const COLOR_CARD: COLORREF = COLORREF(0x00ffffff);
const COLOR_BORDER: COLORREF = COLORREF(0x00e1ddd7);
const COLOR_TEXT: COLORREF = COLORREF(0x002b2927);
const COLOR_PROPERTY: COLORREF = COLORREF(0x005b514a);
const COLOR_MUTED: COLORREF = COLORREF(0x007b6d64);
const COLOR_DISABLED: COLORREF = COLORREF(0x0099948f);

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
                scale(x, dpi),
                scale(y, dpi),
                scale(width, dpi),
                scale(height, dpi),
                true,
            );
        }
    }
}

fn create_ui_font(dpi: u32, points: i32, weight: i32) -> windows::Win32::Graphics::Gdi::HFONT {
    let face = wide_string("Segoe UI");
    let pixel_height = (points * dpi as i32 + 36) / 72;
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
    create_ui_font(dpi, 10, windows::Win32::Graphics::Gdi::FW_NORMAL.0 as i32)
}

fn create_title_font(dpi: u32) -> windows::Win32::Graphics::Gdi::HFONT {
    create_ui_font(dpi, 16, 600)
}

fn create_heading_font(dpi: u32) -> windows::Win32::Graphics::Gdi::HFONT {
    create_ui_font(dpi, 11, 600)
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
    match view {
        SettingsView::Simple => &[
            (24, 114, 676, 224),
            (24, 230, 676, 350),
            (24, 356, 676, 462),
        ],
        SettingsView::Advanced => &[
            (24, 114, 676, 262),
            (24, 268, 676, 368),
            (24, 378, 676, 518),
        ],
    }
}

fn paint_settings_surface(hwnd: HWND, state: &SettingsWindowState, hdc: HDC) {
    let mut client = RECT::default();
    unsafe {
        let _ = GetClientRect(hwnd, &mut client);
        let _ = FillRect(hdc, &client, state.background_brush);
    }

    let pen = unsafe { CreatePen(PS_SOLID, scale(1, state.dpi), COLOR_BORDER) };
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
                scale(8, state.dpi),
                scale(8, state.dpi),
            );
        }
    }
    unsafe {
        let _ = SelectObject(hdc, old_brush);
        let _ = SelectObject(hdc, old_pen);
        let _ = DeleteObject(HGDIOBJ(pen.0));
    }
}

fn control_uses_card(id: i32) -> bool {
    !matches!(
        id,
        700 | 709 | ID_VIEW_SIMPLE | ID_VIEW_ADVANCED | ID_SAVE | ID_CANCEL
    )
}

fn layout_controls(hwnd: HWND, dpi: u32) {
    const ROW: i32 = 26;
    move_control(hwnd, 700, 24, 14, 652, 30, dpi);
    move_control(hwnd, 709, 24, 44, 652, 22, dpi);
    move_control(hwnd, ID_VIEW_SIMPLE, 24, 74, 102, 30, dpi);
    move_control(hwnd, ID_VIEW_ADVANCED, 130, 74, 112, 30, dpi);

    move_control(hwnd, 710, 42, 126, 616, 22, dpi);
    move_control(hwnd, 701, 42, 158, 110, ROW, dpi);
    move_control(hwnd, ID_HOTKEY_PRINT, 160, 157, 104, ROW, dpi);
    move_control(hwnd, ID_HOTKEY_CTRL_SHIFT_S, 270, 157, 120, ROW, dpi);
    move_control(hwnd, ID_HOTKEY_ALT_PRINT, 396, 157, 136, ROW, dpi);
    move_control(hwnd, 705, 42, 190, 110, ROW, dpi);
    for index in 0..4 {
        move_control(
            hwnd,
            ID_DELAY_FIRST + index,
            160 + index * 94,
            189,
            88,
            ROW,
            dpi,
        );
    }

    move_control(hwnd, 711, 42, 242, 616, 22, dpi);
    move_control(hwnd, 702, 42, 274, 110, ROW, dpi);
    move_control(hwnd, ID_FOLDER_LABEL, 160, 275, 398, 24, dpi);
    move_control(hwnd, ID_BROWSE, 568, 271, 90, 30, dpi);
    move_control(hwnd, 706, 42, 308, 110, ROW, dpi);
    move_control(hwnd, ID_FORMAT_PNG, 160, 307, 68, ROW, dpi);
    move_control(hwnd, ID_FORMAT_JPEG, 234, 307, 74, ROW, dpi);
    move_control(hwnd, 707, 338, 308, 96, ROW, dpi);
    for index in 0..3 {
        move_control(
            hwnd,
            ID_QUALITY_FIRST + index,
            438 + index * 64,
            307,
            58,
            ROW,
            dpi,
        );
    }

    move_control(hwnd, 712, 42, 368, 616, 22, dpi);
    move_control(hwnd, ID_WINDOW_SNAP, 42, 398, 280, ROW, dpi);
    move_control(hwnd, ID_CLOSE_AFTER_ACTION, 42, 427, 280, ROW, dpi);

    move_control(hwnd, 713, 42, 126, 616, 22, dpi);
    move_control(hwnd, 703, 42, 158, 110, ROW, dpi);
    for index in 0..PRESET_COLORS.len() {
        move_control(
            hwnd,
            ID_COLOR_FIRST + index as i32,
            160 + (index as i32 % 4) * 112,
            157 + (index as i32 / 4) * 29,
            106,
            ROW,
            dpi,
        );
    }
    move_control(hwnd, 704, 42, 219, 110, ROW, dpi);
    for index in 0..PRESET_THICKNESSES.len() {
        move_control(
            hwnd,
            ID_THICKNESS_FIRST + index as i32,
            160 + index as i32 * 90,
            218,
            82,
            ROW,
            dpi,
        );
    }

    move_control(hwnd, 714, 42, 278, 616, 22, dpi);
    move_control(hwnd, ID_START_WITH_WINDOWS, 42, 308, 350, ROW, dpi);
    move_control(hwnd, ID_NOTIFY_AFTER_SAVE, 42, 337, 310, ROW, dpi);

    move_control(hwnd, 715, 42, 388, 616, 22, dpi);
    move_control(hwnd, ID_CHECK_UPDATES, 42, 418, 310, ROW, dpi);
    move_control(hwnd, ID_AUTO_INSTALL, 42, 447, 380, ROW, dpi);
    move_control(hwnd, ID_CHECK_UPDATE, 42, 478, 146, 30, dpi);
    move_control(hwnd, 708, 202, 475, 456, 38, dpi);

    move_control(hwnd, ID_SAVE, 462, 529, 108, 34, dpi);
    move_control(hwnd, ID_CANCEL, 580, 529, 96, 34, dpi);
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
    let hotkey_id = if settings
        .hotkey
        .description
        .eq_ignore_ascii_case("Ctrl+Shift+S")
    {
        ID_HOTKEY_CTRL_SHIFT_S
    } else if settings
        .hotkey
        .description
        .eq_ignore_ascii_case("Alt+PrintScreen")
    {
        ID_HOTKEY_ALT_PRINT
    } else {
        ID_HOTKEY_PRINT
    };
    check_radio(hwnd, ID_HOTKEY_PRINT, ID_HOTKEY_ALT_PRINT, hotkey_id);
    let color_index = PRESET_COLORS
        .iter()
        .position(|color| *color == settings.default_color)
        .unwrap_or(0) as i32;
    check_radio(
        hwnd,
        ID_COLOR_FIRST,
        ID_COLOR_FIRST + PRESET_COLORS.len() as i32 - 1,
        ID_COLOR_FIRST + color_index,
    );
    let thickness_index = PRESET_THICKNESSES
        .iter()
        .position(|value| *value == settings.default_thickness)
        .unwrap_or(1) as i32;
    check_radio(
        hwnd,
        ID_THICKNESS_FIRST,
        ID_THICKNESS_FIRST + PRESET_THICKNESSES.len() as i32 - 1,
        ID_THICKNESS_FIRST + thickness_index,
    );
    let delay_id = match settings.capture_delay_ms {
        1000 => ID_DELAY_FIRST + 1,
        3000 => ID_DELAY_FIRST + 2,
        5000 => ID_DELAY_FIRST + 3,
        _ => ID_DELAY_FIRST,
    };
    check_radio(hwnd, ID_DELAY_FIRST, ID_DELAY_FIRST + 3, delay_id);
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
    let quality_id = match settings.jpeg_quality {
        1..=85 => ID_QUALITY_FIRST,
        86..=95 => ID_QUALITY_FIRST + 1,
        _ => ID_QUALITY_FIRST + 2,
    };
    check_radio(hwnd, ID_QUALITY_FIRST, ID_QUALITY_FIRST + 2, quality_id);
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

fn choose_folder(owner: HWND) -> Option<PathBuf> {
    let mut display_name = [0u16; 260];
    let title = wide_string("Choose where isolmaSS saves screenshots");
    let browse = windows::Win32::UI::Shell::BROWSEINFOW {
        hwndOwner: owner,
        pszDisplayName: windows::core::PWSTR(display_name.as_mut_ptr()),
        lpszTitle: PCWSTR(title.as_ptr()),
        ulFlags: windows::Win32::UI::Shell::BIF_RETURNONLYFSDIRS
            | windows::Win32::UI::Shell::BIF_NEWDIALOGSTYLE,
        ..Default::default()
    };
    let item = unsafe { windows::Win32::UI::Shell::SHBrowseForFolderW(&browse) };
    if item.is_null() {
        return None;
    }
    let mut path = [0u16; 260];
    let valid = unsafe { windows::Win32::UI::Shell::SHGetPathFromIDListW(item, &mut path) };
    unsafe {
        windows::Win32::System::Com::CoTaskMemFree(Some(item.cast()));
    }
    if !valid.as_bool() {
        return None;
    }
    let length = path
        .iter()
        .position(|value| *value == 0)
        .unwrap_or(path.len());
    Some(PathBuf::from(String::from_utf16_lossy(&path[..length])))
}

fn create_settings_controls(hwnd: HWND) -> Result<()> {
    use windows::Win32::UI::WindowsAndMessaging::{
        BS_AUTOCHECKBOX, BS_AUTORADIOBUTTON, BS_DEFPUSHBUTTON, BS_PUSHBUTTON, BS_PUSHLIKE,
        WS_GROUP, WS_TABSTOP,
    };
    let label = |id, text| create_control(hwnd, w!("STATIC"), text, Default::default(), id);

    label(700, "isolmaSS Settings")?;
    label(
        709,
        "Capture quickly with everyday options, or fine-tune behavior in Advanced.",
    )?;
    create_button(
        hwnd,
        ID_VIEW_SIMPLE,
        "&Simple",
        BS_AUTORADIOBUTTON | BS_PUSHLIKE | WS_GROUP.0 as i32 | WS_TABSTOP.0 as i32,
    )?;
    create_button(
        hwnd,
        ID_VIEW_ADVANCED,
        "&Advanced",
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
        "Browse...",
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
        "Enable single-click window snap",
        BS_AUTOCHECKBOX | WS_GROUP.0 as i32 | WS_TABSTOP.0 as i32,
    )?;
    create_button(
        hwnd,
        ID_CLOSE_AFTER_ACTION,
        "Close overlay after Copy or Save",
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
        "Save && Apply",
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
        ID_BROWSE => {
            if let Some(path) = choose_folder(hwnd) {
                state.settings.save_directory = path;
                update_folder_label(hwnd, &state.settings.save_directory);
            }
        }
        ID_CHECK_UPDATE => crate::updater::run_manual_update_check(),
        ID_SAVE => {
            let requested_startup = state.settings.start_with_windows;
            if let Err(error) = crate::startup::set_start_with_windows(requested_startup) {
                let message = wide_string(&format!(
                    "Settings were not saved because the Windows startup setting could not be applied:\n\n{error}"
                ));
                unsafe {
                    let _ = windows::Win32::UI::WindowsAndMessaging::MessageBoxW(
                        hwnd,
                        PCWSTR(message.as_ptr()),
                        w!("isolmaSS settings error"),
                        windows::Win32::UI::WindowsAndMessaging::MB_OK
                            | windows::Win32::UI::WindowsAndMessaging::MB_ICONERROR,
                    );
                }
                return;
            }

            match state.settings.save() {
                Ok(()) => {
                    state.saved = true;
                    unsafe {
                        let _ = DestroyWindow(hwnd);
                    }
                }
                Err(error) => {
                    let rollback =
                        crate::startup::set_start_with_windows(state.original_start_with_windows);
                    let rollback_note = rollback
                        .err()
                        .map(|rollback_error| {
                            format!(
                                "\n\nThe previous Windows startup setting could not be restored: {rollback_error}"
                            )
                        })
                        .unwrap_or_default();
                    let message = wide_string(&format!(
                        "Settings could not be saved:\n\n{error}{rollback_note}"
                    ));
                    unsafe {
                        let _ = windows::Win32::UI::WindowsAndMessaging::MessageBoxW(
                            hwnd,
                            PCWSTR(message.as_ptr()),
                            w!("isolmaSS settings error"),
                            windows::Win32::UI::WindowsAndMessaging::MB_OK
                                | windows::Win32::UI::WindowsAndMessaging::MB_ICONERROR,
                        );
                    }
                }
            }
        }
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
                    COLOR_DISABLED
                } else if matches!(id, 708 | 709) {
                    COLOR_MUTED
                } else if matches!(id, 701..=707) {
                    COLOR_PROPERTY
                } else {
                    COLOR_TEXT
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
            if !state_ptr.is_null() {
                apply_button_action(hwnd, unsafe { &mut *state_ptr }, (wparam.0 & 0xffff) as i32);
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
                layout_controls(hwnd, state.dpi);
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
        WM_DESTROY => {
            unsafe { PostQuitMessage(0) };
            LRESULT(0)
        }
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
        let point = POINT { x: 0, y: 0 };
        unsafe { MonitorFromPoint(point, MONITOR_DEFAULTTOPRIMARY) }
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
            0,
            0,
            windows::Win32::UI::WindowsAndMessaging::SWP_NOSIZE
                | windows::Win32::UI::WindowsAndMessaging::SWP_NOACTIVATE
                | windows::Win32::UI::WindowsAndMessaging::SWP_NOZORDER,
        );
    }
}

fn activate_dialog(hwnd: HWND) {
    let foreground = unsafe { GetForegroundWindow() };
    let current_thread = unsafe { GetCurrentThreadId() };
    let foreground_thread = if foreground.is_invalid() {
        0
    } else {
        unsafe { GetWindowThreadProcessId(foreground, None) }
    };
    let attached = foreground_thread != 0
        && foreground_thread != current_thread
        && unsafe { AttachThreadInput(current_thread, foreground_thread, true).as_bool() };
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
    if attached {
        unsafe {
            let _ = AttachThreadInput(current_thread, foreground_thread, false);
        }
    }
}

pub fn show_settings_dialog(current: &Settings, owner: Option<HWND>) -> Result<Option<Settings>> {
    register_settings_class()?;
    let mut state = Box::new(SettingsWindowState {
        settings: current.clone(),
        saved: false,
        original_start_with_windows: current.start_with_windows,
        active_view: SettingsView::Simple,
        dpi: 96,
        font: Default::default(),
        title_font: Default::default(),
        heading_font: Default::default(),
        background_brush: unsafe { CreateSolidBrush(COLOR_BACKGROUND) },
        card_brush: unsafe { CreateSolidBrush(COLOR_CARD) },
    });
    let hwnd = unsafe {
        CreateWindowExW(
            Default::default(),
            SETTINGS_CLASS_NAME,
            w!("isolmaSS Settings"),
            windows::Win32::UI::WindowsAndMessaging::WS_OVERLAPPED
                | windows::Win32::UI::WindowsAndMessaging::WS_CAPTION
                | windows::Win32::UI::WindowsAndMessaging::WS_SYSMENU,
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
    if let Err(error) = create_settings_controls(hwnd) {
        unsafe {
            let _ = DestroyWindow(hwnd);
            let mut quit = MSG::default();
            let _ = PeekMessageW(
                &mut quit,
                None,
                windows::Win32::UI::WindowsAndMessaging::WM_QUIT,
                windows::Win32::UI::WindowsAndMessaging::WM_QUIT,
                PM_REMOVE,
            );
        }
        return Err(error);
    }
    set_controls_font(hwnd, state.font);
    set_control_font(hwnd, 700, state.title_font);
    for id in 710..=715 {
        set_control_font(hwnd, id, state.heading_font);
    }
    layout_controls(hwnd, state.dpi);
    initialize_control_values(hwnd, &state.settings);
    set_active_view(hwnd, state.active_view);
    center_dialog(hwnd, owner);
    activate_dialog(hwnd);

    let mut msg = MSG::default();
    loop {
        let status = unsafe { GetMessageW(&mut msg, HWND::default(), 0, 0) };
        if status.0 == -1 {
            if unsafe { IsWindow(hwnd).as_bool() } {
                unsafe {
                    let _ = DestroyWindow(hwnd);
                    let _ = PeekMessageW(
                        &mut msg,
                        None,
                        windows::Win32::UI::WindowsAndMessaging::WM_QUIT,
                        windows::Win32::UI::WindowsAndMessaging::WM_QUIT,
                        PM_REMOVE,
                    );
                }
            }
            return Err(windows::core::Error::from_win32());
        }
        if status.0 == 0 {
            break;
        }
        if msg.message == WM_KEYDOWN && msg.wParam.0 == VK_ESCAPE.0 as usize {
            unsafe {
                let _ = DestroyWindow(hwnd);
            }
            continue;
        }
        if !unsafe { windows::Win32::UI::WindowsAndMessaging::IsDialogMessageW(hwnd, &msg) }
            .as_bool()
        {
            unsafe {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    }
    if state.saved {
        Ok(Some(state.settings.clone()))
    } else {
        Ok(None)
    }
}
