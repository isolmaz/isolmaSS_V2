use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::mpsc::Sender;
use windows::core::{w, PCWSTR, Result};
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu, DestroyWindow,
    GetCursorPos, GetWindowLongPtrW, LoadIconW, PostQuitMessage, RegisterClassExW,
    SetForegroundWindow, SetMenuDefaultItem, SetWindowLongPtrW, TrackPopupMenu, GWLP_USERDATA,
    HICON, IDI_APPLICATION, MF_SEPARATOR, MF_STRING, TPM_BOTTOMALIGN, TPM_RETURNCMD,
    TPM_RIGHTBUTTON, WM_APP, WM_CLOSE, WM_DESTROY, WM_LBUTTONDBLCLK, WM_LBUTTONUP, WM_RBUTTONUP,
    WNDCLASSEXW,
};

pub const WM_TRAYICON: u32 = WM_APP + 101;
const TRAY_CLASS_NAME: PCWSTR = w!("isolmaSS_TrayClass");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayCommand {
    Capture,
    Settings,
    Exit,
}

static TRAY_HWND: AtomicIsize = AtomicIsize::new(0);

struct TrayWindowState {
    event_tx: Sender<TrayCommand>,
}

unsafe extern "system" fn tray_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let state_ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *mut TrayWindowState;

    match msg {
        WM_TRAYICON => {
            if !state_ptr.is_null() {
                let state = unsafe { &*state_ptr };
                let mouse_msg = lparam.0 as u32;

                match mouse_msg {
                    WM_LBUTTONUP | WM_LBUTTONDBLCLK => {
                        let _ = state.event_tx.send(TrayCommand::Capture);
                        notify_tray_wakeup();
                    }
                    WM_RBUTTONUP => {
                        let mut pt = POINT::default();
                        unsafe {
                            let _ = GetCursorPos(&mut pt);
                        }

                        if let Ok(hmenu) = unsafe { CreatePopupMenu() } {
                            unsafe {
                                let _ = AppendMenuW(hmenu, MF_STRING, 1, w!("Capture Now"));
                                let _ = AppendMenuW(hmenu, MF_STRING, 2, w!("Settings..."));
                                let _ = AppendMenuW(hmenu, MF_SEPARATOR, 0, PCWSTR::null());
                                let _ = AppendMenuW(hmenu, MF_STRING, 3, w!("Exit"));

                                // Bold the default item (Capture Now)
                                let _ = SetMenuDefaultItem(hmenu, 1, 0);

                                // Required by TrackPopupMenu for tray icons
                                let _ = SetForegroundWindow(hwnd);

                                let cmd = TrackPopupMenu(
                                    hmenu,
                                    TPM_RIGHTBUTTON | TPM_BOTTOMALIGN | TPM_RETURNCMD,
                                    pt.x,
                                    pt.y,
                                    0,
                                    hwnd,
                                    None,
                                );

                                let _ = DestroyMenu(hmenu);

                                match cmd.0 {
                                    1 => {
                                        let _ = state.event_tx.send(TrayCommand::Capture);
                                        notify_tray_wakeup();
                                    }
                                    2 => {
                                        let _ = state.event_tx.send(TrayCommand::Settings);
                                        notify_tray_wakeup();
                                    }
                                    3 => {
                                        let _ = state.event_tx.send(TrayCommand::Exit);
                                        notify_tray_wakeup();
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            LRESULT(0)
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

fn register_tray_class() -> Result<()> {
    static REGISTERED: AtomicBool = AtomicBool::new(false);
    if REGISTERED.swap(true, Ordering::SeqCst) {
        return Ok(());
    }

    let wc = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        style: windows::Win32::UI::WindowsAndMessaging::WNDCLASS_STYLES::default(),
        lpfnWndProc: Some(tray_wnd_proc),
        cbClsExtra: 0,
        cbWndExtra: 0,
        hInstance: HINSTANCE::default(),
        hIcon: HICON::default(),
        hCursor: windows::Win32::UI::WindowsAndMessaging::HCURSOR::default(),
        hbrBackground: windows::Win32::Graphics::Gdi::HBRUSH::default(),
        lpszMenuName: PCWSTR::null(),
        lpszClassName: TRAY_CLASS_NAME,
        hIconSm: HICON::default(),
    };

    let atom = unsafe { RegisterClassExW(&wc) };
    if atom == 0 {
        return Err(windows::core::Error::from_win32());
    }
    Ok(())
}

/// System tray icon manager that ensures cleanup on drop via Shell_NotifyIconW(NIM_DELETE).
pub struct TrayManager {
    hwnd: HWND,
    nid: NOTIFYICONDATAW,
    _state: Box<TrayWindowState>,
}

impl TrayManager {
    pub fn create(event_tx: Sender<TrayCommand>) -> Result<Self> {
        register_tray_class()?;

        let mut state = Box::new(TrayWindowState { event_tx });

        let hwnd = unsafe {
            CreateWindowExW(
                windows::Win32::UI::WindowsAndMessaging::WINDOW_EX_STYLE::default(),
                TRAY_CLASS_NAME,
                w!("isolmaSS_TrayWindow"),
                windows::Win32::UI::WindowsAndMessaging::WINDOW_STYLE::default(),
                0,
                0,
                0,
                0,
                HWND::default(),
                None,
                HINSTANCE::default(),
                None,
            )?
        };

        unsafe {
            SetWindowLongPtrW(
                hwnd,
                GWLP_USERDATA,
                state.as_mut() as *mut TrayWindowState as isize,
            );
        }

        TRAY_HWND.store(hwnd.0 as isize, Ordering::SeqCst);

        let icon = unsafe { LoadIconW(HINSTANCE::default(), IDI_APPLICATION).unwrap_or_default() };

        let mut nid = NOTIFYICONDATAW {
            cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: hwnd,
            uID: 1001,
            uFlags: NIF_ICON | NIF_MESSAGE | NIF_TIP,
            uCallbackMessage: WM_TRAYICON,
            hIcon: icon,
            ..Default::default()
        };

        let tip_wide: Vec<u16> = "isolmaSS - Screenshot Utility\0".encode_utf16().collect();
        let copy_len = tip_wide.len().min(nid.szTip.len());
        nid.szTip[..copy_len].copy_from_slice(&tip_wide[..copy_len]);

        let added = unsafe { Shell_NotifyIconW(NIM_ADD, &nid).as_bool() };
        if !added {
            eprintln!("[isolmaSS] Warning: Shell_NotifyIconW(NIM_ADD) returned false.");
        }

        Ok(Self {
            hwnd,
            nid,
            _state: state,
        })
    }
}

impl Drop for TrayManager {
    fn drop(&mut self) {
        unsafe {
            let _ = Shell_NotifyIconW(NIM_DELETE, &self.nid);
            let _ = DestroyWindow(self.hwnd);
        }
        TRAY_HWND.store(0, Ordering::SeqCst);
    }
}

/// Posts WM_NULL to the tray window to wake up its GetMessage loop.
pub fn notify_tray_wakeup() {
    let raw = TRAY_HWND.load(Ordering::SeqCst);
    if raw != 0 {
        unsafe {
            let _ = windows::Win32::UI::WindowsAndMessaging::PostMessageW(
                HWND(raw as *mut _),
                windows::Win32::UI::WindowsAndMessaging::WM_NULL,
                WPARAM(0),
                LPARAM(0),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::channel;

    #[test]
    fn test_tray_manager_lifecycle() {
        let (tx, _rx) = channel::<TrayCommand>();
        let manager = TrayManager::create(tx);
        assert!(manager.is_ok(), "TrayManager::create should succeed");

        notify_tray_wakeup();

        // Dropping manager should unregister NIM_DELETE cleanly
        drop(manager);
        assert_eq!(TRAY_HWND.load(Ordering::SeqCst), 0);
    }
}
