use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Mutex;
use std::thread::{self, JoinHandle};
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, RegisterHotKey, UnregisterHotKey, HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL,
    MOD_NOREPEAT, MOD_SHIFT, MOD_WIN, VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT,
    VK_SNAPSHOT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetMessageW, PeekMessageW, PostThreadMessageW, SetWindowsHookExW,
    UnhookWindowsHookEx, HHOOK, KBDLLHOOKSTRUCT, KBDLLHOOKSTRUCT_FLAGS, MSG, PM_NOREMOVE,
    WH_KEYBOARD_LL, WM_HOTKEY, WM_KEYDOWN, WM_KEYUP, WM_QUIT, WM_SYSKEYDOWN, WM_SYSKEYUP,
};

pub const HOTKEY_ID: i32 = 1001;

/// Configuration for a global hotkey.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HotkeyConfig {
    pub modifiers: u32,
    pub vk: u32,
    pub description: String,
}

impl Default for HotkeyConfig {
    fn default() -> Self {
        Self {
            modifiers: MOD_NOREPEAT.0,
            vk: VK_SNAPSHOT.0 as u32,
            description: "PrintScreen".to_string(),
        }
    }
}

impl HotkeyConfig {
    /// Recommended fallback hotkey when PrintScreen is claimed by Windows Snipping Tool.
    pub fn fallback() -> Self {
        Self {
            modifiers: (MOD_CONTROL | MOD_SHIFT | MOD_NOREPEAT).0,
            vk: 0x53, // 'S' key
            description: "Ctrl+Shift+S".to_string(),
        }
    }

    /// Alternative fallback: Alt + PrintScreen
    pub fn alt_print_screen() -> Self {
        Self {
            modifiers: (MOD_ALT | MOD_NOREPEAT).0,
            vk: VK_SNAPSHOT.0 as u32,
            description: "Alt+PrintScreen".to_string(),
        }
    }

    /// Parses a human-readable hotkey string like "Ctrl+Shift+S" or "PrintScreen".
    pub fn from_str(s: &str) -> Option<Self> {
        let trimmed = s.trim();
        if trimmed.eq_ignore_ascii_case("printscreen") || trimmed.eq_ignore_ascii_case("prtsc") {
            return Some(Self::default());
        }

        let parts: Vec<&str> = trimmed.split('+').map(|p| p.trim()).collect();
        if parts.is_empty() {
            return None;
        }

        let mut modifiers = MOD_NOREPEAT.0;
        let mut vk = 0u32;

        for part in &parts {
            match part.to_ascii_lowercase().as_str() {
                "ctrl" | "control" => modifiers |= MOD_CONTROL.0,
                "shift" => modifiers |= MOD_SHIFT.0,
                "alt" => modifiers |= MOD_ALT.0,
                "win" | "windows" => modifiers |= MOD_WIN.0,
                "prtsc" | "printscreen" => vk = VK_SNAPSHOT.0 as u32,
                key if key.len() == 1 => {
                    let ch = key.chars().next().unwrap().to_ascii_uppercase();
                    if ch.is_ascii_alphanumeric() {
                        vk = ch as u32;
                    } else {
                        return None;
                    }
                }
                f_key if f_key.starts_with('f') => {
                    if let Ok(num) = f_key[1..].parse::<u32>() {
                        if (1..=24).contains(&num) {
                            vk = 0x6F + num; // VK_F1 is 0x70
                        } else {
                            return None;
                        }
                    } else {
                        return None;
                    }
                }
                _ => return None,
            }
        }

        if vk == 0 {
            return None;
        }

        Some(Self {
            modifiers,
            vk,
            description: s.to_string(),
        })
    }

    /// Attempts to load hotkey settings from %APPDATA%\isolmaSS\settings.json
    /// or falls back to standard defaults.
    pub fn load_or_default() -> Self {
        if let Some(content) = settings_path().and_then(|p| std::fs::read_to_string(p).ok()) {
            #[derive(Deserialize)]
            struct PartialSettings {
                hotkey: Option<HotkeyConfig>,
            }
            if let Some(hk) = serde_json::from_str::<PartialSettings>(&content)
                .ok()
                .and_then(|s| s.hotkey)
            {
                return hk;
            }
        }
        Self::default()
    }
}

/// Helper to get the canonical settings.json path in AppData.
pub fn settings_path() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(|appdata| {
        let mut p = PathBuf::from(appdata);
        p.push("isolmaSS");
        p.push("settings.json");
        p
    })
}

// -----------------------------------------------------------------------------
// Windows Registry Fix for PrintScreen & Snipping Tool
// -----------------------------------------------------------------------------

/// Disables the Windows Snipping Tool from intercepting the PrintScreen key
/// by setting HKCU\Control Panel\Keyboard -> PrintScreenKeyForSnippingEnabled = 0 (REG_DWORD).
pub fn disable_windows_snipping_tool_hotkey() -> bool {
    let output = std::process::Command::new("reg")
        .args([
            "add",
            "HKCU\\Control Panel\\Keyboard",
            "/v",
            "PrintScreenKeyForSnippingEnabled",
            "/t",
            "REG_DWORD",
            "/d",
            "0",
            "/f",
        ])
        .output();
    match output {
        Ok(out) => out.status.success(),
        Err(_) => false,
    }
}

/// Checks whether Windows Snipping Tool PrintScreen interception is disabled in the registry.
pub fn is_windows_snipping_tool_disabled() -> bool {
    let output = std::process::Command::new("reg")
        .args([
            "query",
            "HKCU\\Control Panel\\Keyboard",
            "/v",
            "PrintScreenKeyForSnippingEnabled",
        ])
        .output();
    output.is_ok_and(|out| {
        out.status.success()
            && (String::from_utf8_lossy(&out.stdout).contains("0x0")
                || String::from_utf8_lossy(&out.stdout).contains("0x00000000"))
    })
}

// -----------------------------------------------------------------------------
// Low-Level Keyboard Hook (WH_KEYBOARD_LL) State & Logic
// -----------------------------------------------------------------------------

struct HookState {
    event_tx: Sender<()>,
    _config: HotkeyConfig,
}

static HOOK_STATE: Mutex<Option<HookState>> = Mutex::new(None);
static TARGET_VK: AtomicU32 = AtomicU32::new(0);
static TARGET_MODS: AtomicU32 = AtomicU32::new(0);
static SNAPSHOT_HANDLED: AtomicBool = AtomicBool::new(false);

/// Reads the current physical modifier keys (Ctrl, Shift, Alt, Win) via GetAsyncKeyState.
pub fn get_current_modifiers() -> u32 {
    let mut mods = 0u32;
    unsafe {
        if (GetAsyncKeyState(VK_CONTROL.0 as i32) as u16 & 0x8000) != 0 {
            mods |= MOD_CONTROL.0;
        }
        if (GetAsyncKeyState(VK_SHIFT.0 as i32) as u16 & 0x8000) != 0 {
            mods |= MOD_SHIFT.0;
        }
        if (GetAsyncKeyState(VK_MENU.0 as i32) as u16 & 0x8000) != 0 {
            mods |= MOD_ALT.0;
        }
        if ((GetAsyncKeyState(VK_LWIN.0 as i32) as u16 & 0x8000) != 0)
            || ((GetAsyncKeyState(VK_RWIN.0 as i32) as u16 & 0x8000) != 0)
        {
            mods |= MOD_WIN.0;
        }
    }
    mods
}

/// Core processing logic for low-level keyboard hook callback.
/// When the keystroke matches target VK and target modifiers:
/// - Consumes / swallows the keystroke with LRESULT(1) so Windows Snipping Tool never receives it.
/// - Dispatches the capture trigger event on initial keydown.
/// - Also consumes keyup with LRESULT(1).
pub unsafe fn process_keyboard_hook(
    ncode: i32,
    wparam: WPARAM,
    lparam: LPARAM,
    current_mods: u32,
) -> LRESULT {
    if ncode < 0 || lparam.0 == 0 {
        return unsafe { CallNextHookEx(None, ncode, wparam, lparam) };
    }

    let kb = unsafe { *(lparam.0 as *const KBDLLHOOKSTRUCT) };
    let target_vk = TARGET_VK.load(Ordering::Relaxed);
    let target_mods = TARGET_MODS.load(Ordering::Relaxed);

    // If no target VK set, or key does not match target, pass through immediately with zero overhead.
    if target_vk == 0 || kb.vkCode != target_vk {
        return unsafe { CallNextHookEx(None, ncode, wparam, lparam) };
    }

    let msg = wparam.0 as u32;
    let mods_match = current_mods == (target_mods & !MOD_NOREPEAT.0);

    if msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN {
        if mods_match {
            let was_down = SNAPSHOT_HANDLED.swap(true, Ordering::SeqCst);
            if !was_down {
                // First keydown: dispatch capture event to listener channel
                if let Some(tx) = HOOK_STATE
                    .lock()
                    .ok()
                    .and_then(|guard| guard.as_ref().map(|s| s.event_tx.clone()))
                {
                    let _ = tx.send(());
                }
            }
            // Return 1 to swallow keystroke: Windows Shell & Snipping Tool will NEVER receive it!
            return LRESULT(1);
        }
    } else if msg == WM_KEYUP || msg == WM_SYSKEYUP {
        let was_handled = SNAPSHOT_HANDLED.swap(false, Ordering::SeqCst);
        if was_handled || mods_match {
            // Consume keyup as well
            return LRESULT(1);
        }
    }

    unsafe { CallNextHookEx(None, ncode, wparam, lparam) }
}

/// Win32 WH_KEYBOARD_LL hook callback procedure.
pub unsafe extern "system" fn low_level_keyboard_proc(
    ncode: i32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let mods = get_current_modifiers();
    unsafe { process_keyboard_hook(ncode, wparam, lparam, mods) }
}

/// Active hotkey handle with thread cleanup and hook removal on Drop.
pub struct HotkeyHandle {
    thread_id: u32,
    join_handle: Option<JoinHandle<()>>,
    pub active_description: String,
}

impl Drop for HotkeyHandle {
    fn drop(&mut self) {
        if self.thread_id != 0 {
            unsafe {
                let _ = PostThreadMessageW(self.thread_id, WM_QUIT, WPARAM(0), LPARAM(0));
            }
        }
        if let Some(handle) = self.join_handle.take() {
            let _ = handle.join();
        }
    }
}

/// Starts the global hotkey listener on a dedicated Win32 message-loop thread.
/// Returns a Receiver for hotkey trigger events and a HotkeyHandle that unregisters on drop.
pub fn start_hotkey_listener(
    requested_config: HotkeyConfig,
) -> windows::core::Result<(Receiver<()>, HotkeyHandle)> {
    // Automatically apply Windows Registry fix for PrintScreen key
    let _ = disable_windows_snipping_tool_hotkey();

    let (event_tx, event_rx) = channel::<()>();
    let (ready_tx, ready_rx) = channel::<std::result::Result<(u32, String), String>>();

    let join_handle = thread::spawn(move || {
        let thread_id = unsafe { GetCurrentThreadId() };

        // Force creation of message queue for this thread
        let mut msg = MSG::default();
        unsafe {
            let _ = PeekMessageW(&mut msg, HWND::default(), 0, 0, PM_NOREMOVE);
        }

        let is_snapshot = requested_config.vk == VK_SNAPSHOT.0 as u32;

        // Set target VK and modifiers for low-level hook
        TARGET_VK.store(requested_config.vk, Ordering::SeqCst);
        TARGET_MODS.store(requested_config.modifiers, Ordering::SeqCst);
        SNAPSHOT_HANDLED.store(false, Ordering::SeqCst);

        if let Ok(mut guard) = HOOK_STATE.lock() {
            *guard = Some(HookState {
                event_tx: event_tx.clone(),
                _config: requested_config.clone(),
            });
        }

        let hinstance = unsafe {
            GetModuleHandleW(None)
                .ok()
                .map(|h| HINSTANCE(h.0))
                .unwrap_or_default()
        };

        let hook_handle: windows::core::Result<HHOOK> = unsafe {
            SetWindowsHookExW(
                WH_KEYBOARD_LL,
                Some(low_level_keyboard_proc),
                hinstance,
                0,
            )
        };

        let active_desc = if is_snapshot {
            // Low-level hook is primary for PrintScreen.
            // Attempt RegisterHotKey as well:
            let reg_ok = unsafe {
                RegisterHotKey(
                    HWND::default(),
                    HOTKEY_ID,
                    HOT_KEY_MODIFIERS(requested_config.modifiers),
                    requested_config.vk,
                )
            }
            .is_ok();

            if !reg_ok {
                eprintln!(
                    "[hotkey] Note: RegisterHotKey for '{}' held by OS. WH_KEYBOARD_LL hook active; Snipping Tool suppressed.",
                    requested_config.description
                );
            }
            // Do not switch to fallback: WH_KEYBOARD_LL intercepts VK_SNAPSHOT and consumes it!
            requested_config.description.clone()
        } else {
            // Non-snapshot key: RegisterHotKey is primary
            let primary_ok = unsafe {
                RegisterHotKey(
                    HWND::default(),
                    HOTKEY_ID,
                    HOT_KEY_MODIFIERS(requested_config.modifiers),
                    requested_config.vk,
                )
            }
            .is_ok();

            if primary_ok {
                requested_config.description.clone()
            } else {
                let fallback = HotkeyConfig::fallback();
                let fallback_ok = unsafe {
                    RegisterHotKey(
                        HWND::default(),
                        HOTKEY_ID,
                        HOT_KEY_MODIFIERS(fallback.modifiers),
                        fallback.vk,
                    )
                }
                .is_ok();

                if fallback_ok {
                    eprintln!(
                        "[hotkey] Note: '{}' was unavailable. Using fallback '{}'.",
                        requested_config.description, fallback.description
                    );
                    TARGET_VK.store(fallback.vk, Ordering::SeqCst);
                    TARGET_MODS.store(fallback.modifiers, Ordering::SeqCst);
                    fallback.description
                } else {
                    let err = format!(
                        "Failed to register hotkeys '{}' or '{}'.",
                        requested_config.description, fallback.description
                    );
                    let _ = ready_tx.send(Err(err));
                    return;
                }
            }
        };

        let _ = ready_tx.send(Ok((thread_id, active_desc)));

        // Run message loop on dedicated thread
        while unsafe { GetMessageW(&mut msg, HWND::default(), 0, 0) }.0 > 0 {
            if msg.message == WM_HOTKEY {
                let _ = event_tx.send(());
            }
            unsafe {
                let _ = windows::Win32::UI::WindowsAndMessaging::TranslateMessage(&msg);
                windows::Win32::UI::WindowsAndMessaging::DispatchMessageW(&msg);
            }
        }

        // Unhook low-level keyboard hook on thread exit
        if let Ok(hook) = hook_handle {
            unsafe {
                let _ = UnhookWindowsHookEx(hook);
            }
        }

        // Unregister hotkey on exit
        unsafe {
            let _ = UnregisterHotKey(HWND::default(), HOTKEY_ID);
        }

        TARGET_VK.store(0, Ordering::SeqCst);
        TARGET_MODS.store(0, Ordering::SeqCst);
        SNAPSHOT_HANDLED.store(false, Ordering::SeqCst);
        if let Ok(mut guard) = HOOK_STATE.lock() {
            *guard = None;
        }
    });

    let (thread_id, active_description) = match ready_rx.recv() {
        Ok(Ok(info)) => info,
        Ok(Err(err)) => return Err(windows::core::Error::new(windows::core::HRESULT(-1), err)),
        Err(_) => {
            return Err(windows::core::Error::new(
                windows::core::HRESULT(-1),
                "Hotkey thread exited unexpectedly",
            ))
        }
    };

    Ok((
        event_rx,
        HotkeyHandle {
            thread_id,
            join_handle: Some(join_handle),
            active_description,
        },
    ))
}

/// Helper to create a synthetic KBDLLHOOKSTRUCT for testing hook callback logic.
pub fn create_test_kbdllhookstruct(vk: u32) -> KBDLLHOOKSTRUCT {
    KBDLLHOOKSTRUCT {
        vkCode: vk,
        scanCode: 0,
        flags: KBDLLHOOKSTRUCT_FLAGS(0),
        time: 0,
        dwExtraInfo: 0,
    }
}
