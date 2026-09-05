//! Shared Win32 lifetime and modal-loop rules. Child windows never quit the UI thread.
use windows::Win32::Foundation::{HWND, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{PCWSTR, Result};

/// Declare after the state stored in GWLP_USERDATA so the window is destroyed first.
pub struct OwnedWindow(pub HWND);

impl Drop for OwnedWindow {
    fn drop(&mut self) {
        if unsafe { IsWindow(self.0).as_bool() } {
            unsafe {
                let _ = DestroyWindow(self.0);
            }
        }
    }
}

#[derive(PartialEq, Eq)]
pub enum WindowKind {
    Overlay,
    Settings,
    Menu,
}

pub fn window_loop(hwnd: HWND, kind: WindowKind) -> Result<()> {
    let dialog = kind != WindowKind::Overlay;
    let mut msg = MSG::default();
    while unsafe { IsWindow(hwnd).as_bool() } {
        let status = unsafe { GetMessageW(&mut msg, None, 0, 0) }.0;
        if status == -1 {
            return Err(windows::core::Error::from_win32());
        }
        if status == 0 {
            // Preserve a real application quit for the enclosing message loop.
            unsafe {
                PostQuitMessage(msg.wParam.0 as i32);
            }
            break;
        }
        crate::updater::poll(hwnd, false);
        if dialog && msg.message == WM_KEYDOWN && msg.wParam == WPARAM(27) {
            unsafe {
                let _ = DestroyWindow(hwnd);
            }
            continue;
        }
        if kind == WindowKind::Menu
            && msg.message == WM_KEYDOWN
            && crate::tray::menu::navigate(hwnd, msg.wParam.0)
        {
            continue;
        }
        if !dialog || !unsafe { IsDialogMessageW(hwnd, &msg).as_bool() } {
            unsafe {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        if unsafe { IsWindow(hwnd).as_bool() } {
            crate::updater::poll(hwnd, false);
        }
        if kind == WindowKind::Settings
            && msg.message == WM_KEYDOWN
            && unsafe { IsWindow(hwnd).as_bool() }
        {
            crate::settings_window::ensure_focus_visible(hwnd);
        }
    }
    Ok(())
}

pub fn error(owner: HWND, title: &str, message: &str) {
    crate::diagnostics::record(title, message);
    let _suspend = crate::hotkey::OverlayInputSuspension::new();
    let title: Vec<u16> = title.encode_utf16().chain(Some(0)).collect();
    let message: Vec<u16> = message.encode_utf16().chain(Some(0)).collect();
    use windows::Win32::UI::Controls::{TD_ERROR_ICON, TDCBF_OK_BUTTON, TaskDialog};
    if unsafe {
        TaskDialog(
            owner,
            None,
            windows::core::w!("isolmaSS"),
            PCWSTR(title.as_ptr()),
            PCWSTR(message.as_ptr()),
            TDCBF_OK_BUTTON,
            TD_ERROR_ICON,
            None,
        )
    }
    .is_err()
    {
        unsafe {
            let _ = MessageBoxW(
                owner,
                PCWSTR(message.as_ptr()),
                PCWSTR(title.as_ptr()),
                MB_OK | MB_ICONERROR,
            );
        }
    }
}

pub fn confirm(owner: HWND, title: &str, message: &str) -> bool {
    use windows::Win32::UI::Controls::{
        TD_INFORMATION_ICON, TDCBF_NO_BUTTON, TDCBF_YES_BUTTON, TaskDialog,
    };
    let _suspend = crate::hotkey::OverlayInputSuspension::new();
    let title: Vec<u16> = title.encode_utf16().chain(Some(0)).collect();
    let message: Vec<u16> = message.encode_utf16().chain(Some(0)).collect();
    let mut selected = 0;
    unsafe {
        TaskDialog(
            owner,
            None,
            windows::core::w!("isolmaSS"),
            PCWSTR(title.as_ptr()),
            PCWSTR(message.as_ptr()),
            TDCBF_YES_BUTTON | TDCBF_NO_BUTTON,
            TD_INFORMATION_ICON,
            Some(&mut selected),
        )
    }
    .is_ok()
        && selected == IDYES.0
}
