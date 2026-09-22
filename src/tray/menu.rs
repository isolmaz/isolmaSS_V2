//! A small native command palette. Real buttons retain keyboard and accessibility support.
use super::*;
use crate::capture::Rect;
use windows::Win32::Foundation::RECT;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::Controls::{DRAWITEMSTRUCT, ODS_FOCUS, ODS_SELECTED};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetFocus, SetFocus};

struct MenuState {
    rows: Vec<(String, TrayCommand)>,
    selected: Option<TrayCommand>,
    dpi: u32,
    recent_top: i32,
}

unsafe extern "system" fn procedure(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let pointer = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *mut MenuState;
    match message {
        WM_ERASEBKGND => LRESULT(1),
        WM_PAINT => {
            let mut paint = PAINTSTRUCT::default();
            let dc = unsafe { BeginPaint(hwnd, &mut paint) };
            let mut client = RECT::default();
            unsafe {
                let _ = GetClientRect(hwnd, &mut client);
            }
            let bounds = Rect::new(0, 0, client.right, client.bottom);
            // Tokens are read per paint: the palette follows a live theme
            // flip without caching anything that could go stale.
            let t = crate::theme::tokens();
            crate::drawing::rounded(dc, bounds, 10, t.card, t.stroke);
            if !pointer.is_null() {
                let state = unsafe { &*pointer };
                let s = |value| value * state.dpi as i32 / 96;
                crate::drawing::label(
                    dc,
                    Rect::new(s(14), s(8), client.right - s(14), s(28)),
                    "isolmaSS",
                    s(15),
                    t.text,
                    false,
                );
                crate::drawing::label(
                    dc,
                    Rect::new(s(14), s(28), client.right - s(14), s(46)),
                    "Capture. Annotate. Done.",
                    s(11),
                    t.text_secondary,
                    false,
                );
                crate::drawing::label(
                    dc,
                    Rect::new(
                        s(14),
                        state.recent_top - s(20),
                        client.right - s(14),
                        state.recent_top,
                    ),
                    "RECENT CAPTURES",
                    s(10),
                    t.text_secondary,
                    false,
                );
                if state.rows.len() == 5 {
                    crate::drawing::label(
                        dc,
                        Rect::new(
                            s(14),
                            state.recent_top,
                            client.right - s(14),
                            state.recent_top + s(26),
                        ),
                        "Your next screenshot will appear here",
                        s(11),
                        t.text_secondary,
                        false,
                    );
                }
                crate::drawing::label(
                    dc,
                    Rect::new(
                        s(14),
                        client.bottom - s(20),
                        client.right - s(14),
                        client.bottom - s(2),
                    ),
                    concat!("Version ", env!("CARGO_PKG_VERSION"), " · MIT"),
                    s(10),
                    t.text_secondary,
                    false,
                );
            }
            unsafe {
                let _ = EndPaint(hwnd, &paint);
            }
            LRESULT(0)
        }
        WM_DRAWITEM if !pointer.is_null() => {
            let item = unsafe { &*(lparam.0 as *const DRAWITEMSTRUCT) };
            let state = unsafe { &*pointer };
            if let Some((label, command)) = item
                .CtlID
                .checked_sub(100)
                .and_then(|id| state.rows.get(id as usize))
            {
                let focused = item.itemState.0 & (ODS_FOCUS.0 | ODS_SELECTED.0) != 0;
                let primary = matches!(command, TrayCommand::Capture(_));
                let bounds = Rect::new(
                    item.rcItem.left,
                    item.rcItem.top,
                    item.rcItem.right,
                    item.rcItem.bottom,
                );
                let t = crate::theme::tokens();
                let fill = if primary {
                    t.accent
                } else if focused {
                    t.accent_tint
                } else {
                    t.card
                };
                crate::drawing::rounded(
                    item.hDC,
                    bounds.inflate(-1, -1),
                    8,
                    fill,
                    if focused { t.accent } else { fill },
                );
                let padding = 10 * state.dpi as i32 / 96;
                crate::drawing::label(
                    item.hDC,
                    Rect::new(
                        bounds.left + padding,
                        bounds.top,
                        bounds.right - padding,
                        bounds.bottom,
                    ),
                    label,
                    12 * state.dpi as i32 / 96,
                    if primary { t.accent_text } else { t.text },
                    false,
                );
            }
            LRESULT(1)
        }
        WM_COMMAND if !pointer.is_null() => {
            let id = wparam.0 & 0xffff;
            let state = unsafe { &*pointer };
            let command = id
                .checked_sub(100)
                .and_then(|index| state.rows.get(index))
                .map(|(_, command)| command.clone());
            if let Some(mut command) = command {
                if matches!(command, TrayCommand::Capture(_)) {
                    command = TrayCommand::Capture(Instant::now());
                }
                unsafe {
                    (*pointer).selected = Some(command);
                    let _ = DestroyWindow(hwnd);
                }
            }
            LRESULT(0)
        }
        WM_ACTIVATE if wparam.0 & 0xffff == WA_INACTIVE as usize => {
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
        WM_SETTINGCHANGE | WM_DWMCOLORIZATIONCOLORCHANGED => {
            // ui::window_loop invalidates the theme cache before dispatching,
            // so repaint the palette and its row buttons with fresh tokens.
            unsafe {
                let _ = RedrawWindow(hwnd, None, None, RDW_INVALIDATE | RDW_ALLCHILDREN);
            }
            unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
        }
        WM_NCDESTROY => unsafe {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            DefWindowProcW(hwnd, message, wparam, lparam)
        },
        _ => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
    }
}

pub(super) fn show(
    owner: HWND,
    point: POINT,
    recent: Vec<PathBuf>,
    hotkey: &str,
) -> Result<Option<TrayCommand>> {
    let _suspension = crate::hotkey::OverlayInputSuspension::new();
    let class_name = w!("isolmaSS_CommandMenu");
    let class = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        lpfnWndProc: Some(procedure),
        lpszClassName: class_name,
        hCursor: unsafe { LoadCursorW(None, IDC_ARROW)? },
        ..Default::default()
    };
    if unsafe { RegisterClassExW(&class) } == 0
        && unsafe { GetLastError() } != ERROR_CLASS_ALREADY_EXISTS
    {
        return Err(windows::core::Error::from_win32());
    }
    let monitor = unsafe { MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST) };
    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    if !unsafe { GetMonitorInfoW(monitor, &mut info).as_bool() } {
        return Err(windows::core::Error::from_win32());
    }
    let mut dpi = 96;
    let mut dpi_y = 96;
    unsafe {
        let _ = windows::Win32::UI::HiDpi::GetDpiForMonitor(
            monitor,
            windows::Win32::UI::HiDpi::MDT_EFFECTIVE_DPI,
            &mut dpi,
            &mut dpi_y,
        );
    }
    // Compact the palette only when the monitor cannot accommodate its full logical height.
    let logical_height = 52 + 4 * 30 + 24 + recent.len().max(1) as i32 * 26 + 36 + 20;
    dpi = dpi
        .min(((info.rcWork.bottom - info.rcWork.top - 16).max(1) * 96 / logical_height) as u32)
        .max(48);
    let s = |value| value * dpi as i32 / 96;
    let width = s(260).min((info.rcWork.right - info.rcWork.left - 16).max(1));
    let height = s(logical_height);
    let left = (point.x - width).clamp(
        info.rcWork.left + 8,
        (info.rcWork.right - width - 8).max(info.rcWork.left + 8),
    );
    let top = (point.y - height).clamp(
        info.rcWork.top + 8,
        (info.rcWork.bottom - height - 8).max(info.rcWork.top + 8),
    );
    let capture_label = if hotkey.is_empty() {
        "Capture now".to_string()
    } else {
        format!("Capture now   ·   {hotkey}")
    };
    let mut rows = vec![
        (capture_label, TrayCommand::Capture(Instant::now())),
        ("Settings".into(), TrayCommand::Settings),
        ("Open screenshot folder".into(), TrayCommand::OpenFolder),
        ("Check for updates".into(), TrayCommand::CheckUpdates),
    ];
    for path in recent {
        rows.push((
            path.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            TrayCommand::OpenRecent(path),
        ));
    }
    rows.push(("Quit isolmaSS".into(), TrayCommand::Exit));
    let mut state = Box::new(MenuState {
        rows,
        selected: None,
        dpi,
        recent_top: s(52 + 4 * 30 + 24),
    });
    let hwnd = unsafe {
        CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            class_name,
            w!("isolmaSS commands"),
            WS_POPUP | WS_CLIPCHILDREN,
            left,
            top,
            width,
            height,
            owner,
            None,
            None,
            None,
        )?
    };
    let _window = crate::ui::OwnedWindow(hwnd);
    unsafe {
        SetWindowLongPtrW(
            hwnd,
            GWLP_USERDATA,
            state.as_mut() as *mut MenuState as isize,
        );
    }
    let mut y = s(52);
    for (index, (label, _)) in state.rows.iter().enumerate() {
        if index == 4 {
            y = state.recent_top;
        }
        if index == state.rows.len() - 1 {
            y = height - s(56);
        }
        let label: Vec<u16> = label.encode_utf16().chain(Some(0)).collect();
        unsafe {
            CreateWindowExW(
                Default::default(),
                w!("BUTTON"),
                PCWSTR(label.as_ptr()),
                WS_VISIBLE | WS_CHILD | WS_TABSTOP | WINDOW_STYLE(BS_OWNERDRAW as u32),
                s(8),
                y,
                width - s(16),
                s(if index < 4 { 28 } else { 24 }),
                hwnd,
                HMENU((100 + index) as *mut _),
                None,
                None,
            )?;
        }
        y += s(if index < 4 { 30 } else { 26 });
    }
    unsafe {
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = SetForegroundWindow(hwnd);
        if let Ok(first) = GetDlgItem(hwnd, 100) {
            let _ = SetFocus(first);
        }
    }
    crate::ui::window_loop(hwnd, crate::ui::WindowKind::Menu)?;
    Ok(state.selected.take())
}

pub(crate) fn navigate(hwnd: HWND, key: usize) -> bool {
    if matches!(key, 38 | 40 | 13) {
        let focus = unsafe { GetFocus() };
        if key == 13 {
            if unsafe { IsChild(hwnd, focus).as_bool() } {
                unsafe {
                    let _ = PostMessageW(
                        hwnd,
                        WM_COMMAND,
                        WPARAM(GetDlgCtrlID(focus) as usize),
                        LPARAM(focus.0 as isize),
                    );
                }
            }
        } else if let Ok(next) = unsafe { GetNextDlgTabItem(hwnd, focus, key == 38) } {
            unsafe {
                let _ = SetFocus(next);
            }
        }
        true
    } else {
        false
    }
}
