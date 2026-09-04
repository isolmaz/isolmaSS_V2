use crate::tray::WM_SHOW_EXISTING;
use windows::Win32::Foundation::{
    CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HANDLE, LPARAM, WPARAM,
};
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::WindowsAndMessaging::{FindWindowW, PostMessageW};
use windows::core::w;

pub enum InstanceState {
    Primary(SingleInstance),
    ExistingNotified,
}

pub struct SingleInstance(HANDLE);

impl Drop for SingleInstance {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

pub fn acquire_or_notify() -> windows::core::Result<InstanceState> {
    let handle = unsafe { CreateMutexW(None, true, w!("Local\\isolmaSS.Singleton"))? };
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        unsafe {
            let _ = CloseHandle(handle);
        }
        if let Ok(window) = unsafe { FindWindowW(w!("isolmaSS_TrayClass"), None) }
            && !window.is_invalid()
        {
            unsafe {
                let _ = PostMessageW(window, WM_SHOW_EXISTING, WPARAM(0), LPARAM(0));
            }
        }
        return Ok(InstanceState::ExistingNotified);
    }
    Ok(InstanceState::Primary(SingleInstance(handle)))
}
