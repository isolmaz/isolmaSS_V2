use crate::hotkey::HotkeyConfig;
use crate::settings::{PRESET_COLORS, PRESET_THICKNESSES, SaveFormat, Settings};
use std::path::{Path, PathBuf};
use windows::Win32::Foundation::{
    ERROR_CLASS_ALREADY_EXISTS, GetLastError, HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM,
};
use windows::Win32::Graphics::Gdi::{COLOR_WINDOW, HBRUSH};
use windows::Win32::UI::Input::KeyboardAndMouse::{SetFocus, VK_ESCAPE};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GWLP_USERDATA, GetMessageW,
    GetWindowLongPtrW, IDC_ARROW, MSG, PostQuitMessage, RegisterClassExW, SW_SHOW,
    SetForegroundWindow, SetWindowLongPtrW, ShowWindow, TranslateMessage, WM_CLOSE, WM_DESTROY,
    WM_KEYDOWN, WNDCLASSEXW,
};
use windows::core::{PCWSTR, Result, w};

const SETTINGS_CLASS_NAME: PCWSTR = w!("isolmaSS_SettingsClass");

pub struct SettingsWindowState {
    settings: Settings,
    saved: bool,
    original_start_with_windows: bool,
    dpi: u32,
    font: windows::Win32::Graphics::Gdi::HFONT,
}

impl Drop for SettingsWindowState {
    fn drop(&mut self) {
        if !self.font.is_invalid() {
            unsafe {
                let _ = windows::Win32::Graphics::Gdi::DeleteObject(
                    windows::Win32::Graphics::Gdi::HGDIOBJ(self.font.0),
                );
            }
        }
    }
}

const ID_SAVE: i32 = 100;
const ID_CANCEL: i32 = 101;
const ID_BROWSE: i32 = 102;
const ID_CHECK_UPDATE: i32 = 103;
const ID_FOLDER_LABEL: i32 = 104;
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
const SETTINGS_WIDTH: i32 = 720;
const SETTINGS_HEIGHT: i32 = 760;

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

fn create_settings_font(dpi: u32) -> windows::Win32::Graphics::Gdi::HFONT {
    let face = wide_string("Segoe UI");
    unsafe {
        windows::Win32::Graphics::Gdi::CreateFontW(
            -scale(9, dpi),
            0,
            0,
            0,
            windows::Win32::Graphics::Gdi::FW_NORMAL.0 as i32,
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

fn layout_controls(hwnd: HWND, dpi: u32) {
    let row = 24;
    move_control(hwnd, 700, 24, 16, 670, 30, dpi);
    move_control(hwnd, 709, 24, 46, 670, 22, dpi);

    move_control(hwnd, 710, 16, 76, 688, 72, dpi);
    move_control(hwnd, 701, 34, 101, 116, row, dpi);
    move_control(hwnd, ID_HOTKEY_PRINT, 154, 99, 116, row, dpi);
    move_control(hwnd, ID_HOTKEY_CTRL_SHIFT_S, 276, 99, 132, row, dpi);
    move_control(hwnd, ID_HOTKEY_ALT_PRINT, 414, 99, 146, row, dpi);

    move_control(hwnd, 711, 16, 158, 688, 196, dpi);
    move_control(hwnd, 702, 34, 183, 116, row, dpi);
    move_control(hwnd, ID_FOLDER_LABEL, 154, 183, 424, row, dpi);
    move_control(hwnd, ID_BROWSE, 586, 180, 96, 29, dpi);
    move_control(hwnd, 703, 34, 222, 116, row, dpi);
    for index in 0..PRESET_COLORS.len() {
        move_control(
            hwnd,
            ID_COLOR_FIRST + index as i32,
            154 + (index as i32 % 4) * 126,
            220 + (index as i32 / 4) * 28,
            120,
            25,
            dpi,
        );
    }
    move_control(hwnd, 704, 34, 282, 116, row, dpi);
    for index in 0..PRESET_THICKNESSES.len() {
        move_control(
            hwnd,
            ID_THICKNESS_FIRST + index as i32,
            154 + index as i32 * 98,
            280,
            90,
            25,
            dpi,
        );
    }
    move_control(hwnd, 706, 34, 319, 116, row, dpi);
    move_control(hwnd, ID_FORMAT_PNG, 154, 317, 74, 25, dpi);
    move_control(hwnd, ID_FORMAT_JPEG, 234, 317, 82, 25, dpi);
    move_control(hwnd, 707, 342, 319, 62, row, dpi);
    for index in 0..3 {
        move_control(
            hwnd,
            ID_QUALITY_FIRST + index,
            408 + index * 72,
            317,
            66,
            25,
            dpi,
        );
    }

    move_control(hwnd, 712, 16, 364, 688, 184, dpi);
    move_control(hwnd, 705, 34, 389, 116, row, dpi);
    for index in 0..4 {
        move_control(
            hwnd,
            ID_DELAY_FIRST + index,
            154 + index * 104,
            387,
            98,
            25,
            dpi,
        );
    }
    for (index, id) in [
        ID_WINDOW_SNAP,
        ID_CLOSE_AFTER_ACTION,
        ID_START_WITH_WINDOWS,
        ID_NOTIFY_AFTER_SAVE,
        ID_CHECK_UPDATES,
        ID_AUTO_INSTALL,
    ]
    .into_iter()
    .enumerate()
    {
        let column = index as i32 % 2;
        let line = index as i32 / 2;
        move_control(hwnd, id, 34 + column * 330, 426 + line * 34, 318, 27, dpi);
    }

    move_control(hwnd, 713, 16, 558, 688, 104, dpi);
    move_control(hwnd, ID_CHECK_UPDATE, 34, 584, 148, 30, dpi);
    move_control(hwnd, 708, 196, 579, 486, 52, dpi);
    move_control(hwnd, ID_SAVE, 466, 682, 118, 34, dpi);
    move_control(hwnd, ID_CANCEL, 594, 682, 96, 34, dpi);
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
        BS_AUTOCHECKBOX, BS_AUTORADIOBUTTON, BS_DEFPUSHBUTTON, BS_PUSHBUTTON, WS_GROUP, WS_TABSTOP,
    };
    let label = |id, text| create_control(hwnd, w!("STATIC"), text, Default::default(), id);

    label(700, "isolmaSS Settings")?;
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

    label(702, "Save folder")?;
    label(ID_FOLDER_LABEL, "")?;
    create_button(
        hwnd,
        ID_BROWSE,
        "Browse...",
        BS_PUSHBUTTON | WS_TABSTOP.0 as i32,
    )?;

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
    label(707, "Quality")?;
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

    for (id, text) in [
        (ID_WINDOW_SNAP, "Enable single-click window snap"),
        (ID_CLOSE_AFTER_ACTION, "Close overlay after Copy or Save"),
        (
            ID_START_WITH_WINDOWS,
            "Start isolmaSS when I sign in to Windows",
        ),
        (ID_NOTIFY_AFTER_SAVE, "Show a notification after saving"),
        (ID_CHECK_UPDATES, "Check for updates automatically"),
        (
            ID_AUTO_INSTALL,
            "Automatically install verified signed updates",
        ),
    ] {
        create_button(hwnd, id, text, BS_AUTOCHECKBOX | WS_TABSTOP.0 as i32)?;
    }
    create_button(
        hwnd,
        ID_CHECK_UPDATE,
        "Check for updates",
        BS_PUSHBUTTON | WS_TABSTOP.0 as i32,
    )?;
    label(
        708,
        "Updates require HTTPS, a GitHub SHA-256 digest, and a valid Authenticode signature before installation.",
    )?;
    create_button(
        hwnd,
        ID_SAVE,
        "Save & Apply",
        BS_DEFPUSHBUTTON | WS_TABSTOP.0 as i32,
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
        ID_FORMAT_PNG => state.settings.save_format = SaveFormat::Png,
        ID_FORMAT_JPEG => state.settings.save_format = SaveFormat::Jpeg,
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
                let old_font = std::mem::replace(&mut state.font, new_font);
                set_controls_font(hwnd, new_font);
                layout_controls(hwnd, state.dpi);
                if !old_font.is_invalid() {
                    unsafe {
                        let _ = windows::Win32::Graphics::Gdi::DeleteObject(
                            windows::Win32::Graphics::Gdi::HGDIOBJ(old_font.0),
                        );
                    }
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

pub fn show_settings_dialog(current: &Settings) -> Result<Option<Settings>> {
    register_settings_class()?;
    let mut state = Box::new(SettingsWindowState {
        settings: current.clone(),
        saved: false,
        original_start_with_windows: current.start_with_windows,
        dpi: 96,
        font: Default::default(),
    });
    let hwnd = unsafe {
        CreateWindowExW(
            Default::default(),
            SETTINGS_CLASS_NAME,
            w!("isolmaSS Settings"),
            windows::Win32::UI::WindowsAndMessaging::WS_OVERLAPPED
                | windows::Win32::UI::WindowsAndMessaging::WS_CAPTION
                | windows::Win32::UI::WindowsAndMessaging::WS_SYSMENU
                | windows::Win32::UI::WindowsAndMessaging::WS_MINIMIZEBOX,
            windows::Win32::UI::WindowsAndMessaging::CW_USEDEFAULT,
            windows::Win32::UI::WindowsAndMessaging::CW_USEDEFAULT,
            SETTINGS_WIDTH,
            SETTINGS_HEIGHT,
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
    create_settings_controls(hwnd)?;
    set_controls_font(hwnd, state.font);
    layout_controls(hwnd, state.dpi);
    initialize_control_values(hwnd, &state.settings);
    unsafe {
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = SetForegroundWindow(hwnd);
        if let Ok(first) =
            windows::Win32::UI::WindowsAndMessaging::GetDlgItem(hwnd, ID_HOTKEY_PRINT)
        {
            let _ = SetFocus(first);
        }
    }

    let mut msg = MSG::default();
    while unsafe { GetMessageW(&mut msg, HWND::default(), 0, 0) }.0 > 0 {
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
