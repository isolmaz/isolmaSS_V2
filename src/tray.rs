use crate::save::recent_screenshots;
use crate::settings::Settings;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU32, Ordering};
use std::sync::mpsc::Sender;
use windows::Win32::Foundation::{
    ERROR_CLASS_ALREADY_EXISTS, GetLastError, HINSTANCE, HWND, LPARAM, LRESULT, POINT, WPARAM,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Shell::{
    NIF_ICON, NIF_INFO, NIF_MESSAGE, NIF_TIP, NIIF_INFO, NIIF_RESPECT_QUIET_TIME, NIM_ADD,
    NIM_DELETE, NIM_MODIFY, NIM_SETVERSION, NOTIFYICON_VERSION_4, NOTIFYICONDATAW,
    Shell_NotifyIconW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu, DestroyWindow,
    GWLP_USERDATA, GetCursorPos, GetWindowLongPtrW, HICON, IDI_APPLICATION, LoadIconW,
    MB_ICONERROR, MB_OK, MF_GRAYED, MF_SEPARATOR, MF_STRING, MessageBoxW, PostMessageW,
    PostQuitMessage, RegisterClassExW, RegisterWindowMessageW, SetForegroundWindow,
    SetMenuDefaultItem, SetWindowLongPtrW, TPM_BOTTOMALIGN, TPM_RETURNCMD, TPM_RIGHTBUTTON,
    TrackPopupMenu, WM_APP, WM_CLOSE, WM_DESTROY, WM_LBUTTONDBLCLK, WM_LBUTTONUP, WM_RBUTTONUP,
    WNDCLASSEXW,
};
use windows::core::{PCWSTR, Result, w};

pub const WM_TRAYICON: u32 = WM_APP + 101;
pub const WM_SHOW_EXISTING: u32 = WM_APP + 102;
const TRAY_CLASS_NAME: PCWSTR = w!("isolmaSS_TrayClass");
const TRAY_ICON_ID: u32 = 1001;
const RESOURCE_ICON_ID: usize = 101;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrayCommand {
    Capture,
    Settings,
    CheckUpdates,
    OpenRecent(PathBuf),
    Exit,
}

static TRAY_HWND: AtomicIsize = AtomicIsize::new(0);
static TASKBAR_CREATED: AtomicU32 = AtomicU32::new(0);

struct TrayWindowState {
    event_tx: Sender<TrayCommand>,
}

fn copy_wide<const N: usize>(destination: &mut [u16; N], value: &str) {
    let wide = value.encode_utf16().chain(Some(0));
    for (slot, code_unit) in destination.iter_mut().zip(wide) {
        *slot = code_unit;
    }
}

pub fn app_icon() -> HICON {
    let instance = unsafe {
        GetModuleHandleW(None)
            .ok()
            .map(|module| HINSTANCE(module.0))
            .unwrap_or_default()
    };
    unsafe {
        LoadIconW(instance, PCWSTR(RESOURCE_ICON_ID as *const u16))
            .or_else(|_| LoadIconW(HINSTANCE::default(), IDI_APPLICATION))
            .unwrap_or_default()
    }
}

fn base_notify_data(hwnd: HWND) -> NOTIFYICONDATAW {
    let mut data = NOTIFYICONDATAW {
        cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: TRAY_ICON_ID,
        uFlags: NIF_ICON | NIF_MESSAGE | NIF_TIP,
        uCallbackMessage: WM_TRAYICON,
        hIcon: app_icon(),
        ..Default::default()
    };
    copy_wide(&mut data.szTip, "isolmaSS - Screenshot Utility");
    data
}

fn add_tray_icon(hwnd: HWND) -> Result<NOTIFYICONDATAW> {
    let mut data = base_notify_data(hwnd);
    if !unsafe { Shell_NotifyIconW(NIM_ADD, &data) }.as_bool() {
        return Err(windows::core::Error::from_win32());
    }
    data.Anonymous.uVersion = NOTIFYICON_VERSION_4;
    if !unsafe { Shell_NotifyIconW(NIM_SETVERSION, &data) }.as_bool() {
        let _ = unsafe { Shell_NotifyIconW(NIM_DELETE, &data) };
        return Err(windows::core::Error::from_win32());
    }
    Ok(data)
}

fn send_command(state: &TrayWindowState, command: TrayCommand) {
    let _ = state.event_tx.send(command);
    notify_tray_wakeup();
}

unsafe extern "system" fn tray_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let state_ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *mut TrayWindowState;

    if msg == TASKBAR_CREATED.load(Ordering::SeqCst) && msg != 0 {
        if add_tray_icon(hwnd).is_err() {
            let _ = unsafe {
                MessageBoxW(
                    hwnd,
                    w!("The isolmaSS tray icon could not be restored after Explorer restarted."),
                    w!("isolmaSS tray error"),
                    MB_OK | MB_ICONERROR,
                )
            };
        }
        return LRESULT(0);
    }

    match msg {
        WM_SHOW_EXISTING => {
            if !state_ptr.is_null() {
                send_command(unsafe { &*state_ptr }, TrayCommand::Settings);
            }
            LRESULT(0)
        }
        WM_TRAYICON => {
            if state_ptr.is_null() {
                return LRESULT(0);
            }
            let packed = lparam.0 as u32;
            let event = packed & 0xffff;
            let icon_id = packed >> 16;
            if icon_id != TRAY_ICON_ID {
                return LRESULT(0);
            }
            let state = unsafe { &*state_ptr };
            match event {
                WM_LBUTTONUP | WM_LBUTTONDBLCLK => send_command(state, TrayCommand::Capture),
                WM_RBUTTONUP => show_context_menu(hwnd, state),
                _ => {}
            }
            LRESULT(0)
        }
        WM_CLOSE => {
            let _ = unsafe { DestroyWindow(hwnd) };
            LRESULT(0)
        }
        WM_DESTROY => {
            unsafe { PostQuitMessage(0) };
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

fn show_context_menu(hwnd: HWND, state: &TrayWindowState) {
    let mut point = POINT::default();
    unsafe {
        let _ = GetCursorPos(&mut point);
    }
    let Ok(menu) = (unsafe { CreatePopupMenu() }) else {
        return;
    };

    let settings = Settings::load_or_default();
    let recent = recent_screenshots(&settings.save_directory, 5).unwrap_or_default();
    unsafe {
        let _ = AppendMenuW(menu, MF_STRING, 1, w!("Capture Now"));
        let _ = AppendMenuW(menu, MF_STRING, 2, w!("Settings..."));
        let _ = AppendMenuW(menu, MF_STRING, 3, w!("Check for Updates"));
        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
        if recent.is_empty() {
            let _ = AppendMenuW(menu, MF_STRING | MF_GRAYED, 99, w!("No Recent Captures"));
        } else {
            let _ = AppendMenuW(menu, MF_STRING | MF_GRAYED, 98, w!("Recent Captures"));
            for (index, path) in recent.iter().enumerate() {
                let label = path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("Screenshot");
                let wide: Vec<u16> = label.encode_utf16().chain(Some(0)).collect();
                let _ = AppendMenuW(menu, MF_STRING, 100 + index, PCWSTR(wide.as_ptr()));
            }
        }
        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
        let _ = AppendMenuW(menu, MF_STRING, 4, w!("Exit"));
        let _ = SetMenuDefaultItem(menu, 1, 0);
        let _ = SetForegroundWindow(hwnd);

        let selected = TrackPopupMenu(
            menu,
            TPM_RIGHTBUTTON | TPM_BOTTOMALIGN | TPM_RETURNCMD,
            point.x,
            point.y,
            0,
            hwnd,
            None,
        );
        let _ = DestroyMenu(menu);
        match selected.0 {
            1 => send_command(state, TrayCommand::Capture),
            2 => send_command(state, TrayCommand::Settings),
            3 => send_command(state, TrayCommand::CheckUpdates),
            4 => send_command(state, TrayCommand::Exit),
            id if id >= 100 && (id as usize) < 100 + recent.len() => {
                send_command(
                    state,
                    TrayCommand::OpenRecent(recent[id as usize - 100].clone()),
                );
            }
            _ => {}
        }
    }
}

fn register_tray_class() -> Result<()> {
    static REGISTERED: AtomicBool = AtomicBool::new(false);
    if REGISTERED.load(Ordering::Acquire) {
        return Ok(());
    }

    let class = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        lpfnWndProc: Some(tray_wnd_proc),
        hInstance: HINSTANCE::default(),
        lpszClassName: TRAY_CLASS_NAME,
        ..Default::default()
    };
    let atom = unsafe { RegisterClassExW(&class) };
    if atom == 0 && unsafe { GetLastError() } != ERROR_CLASS_ALREADY_EXISTS {
        return Err(windows::core::Error::from_win32());
    }
    REGISTERED.store(true, Ordering::Release);
    Ok(())
}

pub struct TrayManager {
    hwnd: HWND,
    nid: NOTIFYICONDATAW,
    _state: Box<TrayWindowState>,
}

impl TrayManager {
    pub fn create(event_tx: Sender<TrayCommand>) -> Result<Self> {
        register_tray_class()?;
        TASKBAR_CREATED.store(
            unsafe { RegisterWindowMessageW(w!("TaskbarCreated")) },
            Ordering::SeqCst,
        );
        let mut state = Box::new(TrayWindowState { event_tx });
        let hwnd = unsafe {
            CreateWindowExW(
                Default::default(),
                TRAY_CLASS_NAME,
                w!("isolmaSS_TrayWindow"),
                Default::default(),
                0,
                0,
                0,
                0,
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
                state.as_mut() as *mut TrayWindowState as isize,
            );
        }
        TRAY_HWND.store(hwnd.0 as isize, Ordering::SeqCst);
        let nid = match add_tray_icon(hwnd) {
            Ok(data) => data,
            Err(error) => {
                TRAY_HWND.store(0, Ordering::SeqCst);
                let _ = unsafe { DestroyWindow(hwnd) };
                return Err(error);
            }
        };
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

pub fn show_notification(title: &str, message: &str) {
    let raw = TRAY_HWND.load(Ordering::SeqCst);
    if raw == 0 {
        return;
    }
    let mut data = NOTIFYICONDATAW {
        cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: HWND(raw as *mut _),
        uID: TRAY_ICON_ID,
        uFlags: NIF_INFO,
        dwInfoFlags: NIIF_INFO | NIIF_RESPECT_QUIET_TIME,
        ..Default::default()
    };
    copy_wide(&mut data.szInfoTitle, title);
    copy_wide(&mut data.szInfo, message);
    unsafe {
        let _ = Shell_NotifyIconW(NIM_MODIFY, &data);
    }
}

pub fn request_exit() {
    let raw = TRAY_HWND.load(Ordering::SeqCst);
    if raw != 0 {
        unsafe {
            let _ = PostMessageW(HWND(raw as *mut _), WM_CLOSE, WPARAM(0), LPARAM(0));
        }
    }
}

pub fn notify_tray_wakeup() {
    let raw = TRAY_HWND.load(Ordering::SeqCst);
    if raw != 0 {
        unsafe {
            let _ = PostMessageW(HWND(raw as *mut _), 0, WPARAM(0), LPARAM(0));
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
        drop(manager);
        assert_eq!(TRAY_HWND.load(Ordering::SeqCst), 0);
    }
}
