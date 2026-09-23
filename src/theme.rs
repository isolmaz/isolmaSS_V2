//! Windows 11 Fluent design tokens: the single source for colors, metrics
//! and typography across settings, toolbar and overlay chrome.
//!
//! Colors are dynamic: the current system theme and accent color are read
//! from the registry once and cached until [`invalidate_theme_cache`] runs
//! (window procs call it from `WM_SETTINGCHANGE`).

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use windows::Win32::Foundation::{COLORREF, HWND};

/// Current system theme, derived from `AppsUseLightTheme`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Theme {
    Light,
    Dark,
}
/// User-selected appearance. System tracks the Windows app theme.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemePreference {
    #[default]
    System,
    Light,
    Dark,
}

// 0 = follow Windows, 1 = light, 2 = dark.
static PREFERENCE: AtomicU8 = AtomicU8::new(0);

pub fn set_preference(preference: ThemePreference) {
    let value = match preference {
        ThemePreference::System => 0,
        ThemePreference::Light => 1,
        ThemePreference::Dark => 2,
    };
    PREFERENCE.store(value, Ordering::Release);
}

/// The full Fluent color set for one theme.
#[derive(Clone, Copy)]
pub struct Tokens {
    /// Solid page background (visible only when the Mica backdrop is unavailable).
    pub page: COLORREF,
    pub card: COLORREF,
    pub stroke: COLORREF,
    pub text: COLORREF,
    pub text_secondary: COLORREF,
    pub text_disabled: COLORREF,
    /// System accent color for the current user.
    pub accent: COLORREF,
    /// Readable text color on top of `accent` fills.
    pub accent_text: COLORREF,
    /// Subtle accent-derived tint for selected/checked surfaces.
    pub accent_tint: COLORREF,
    pub control_fill: COLORREF,
    pub control_hover: COLORREF,
}

// Fluent fallback accents. COLORREF layout is 0x00BBGGRR:
// #005FB8 -> 0x00B85F00 (light), #4CC2FF -> 0x00FFC24C (dark).
const FALLBACK_ACCENT_LIGHT: COLORREF = COLORREF(0x00b85f00);
const FALLBACK_ACCENT_DARK: COLORREF = COLORREF(0x00ffc24c);

// Cached theme: 0 = unread, 1 = light, 2 = dark.
static THEME: AtomicU8 = AtomicU8::new(0);
// Cached system accent; None = unread or unavailable.
static ACCENT: Mutex<Option<u32>> = Mutex::new(None);
static REGISTRY_FALLBACK_LOGGED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);
// One-shot diagnostics flags for the backdrop/DWM helpers below.
static BACKDROP_CHECK_LOGGED: AtomicBool = AtomicBool::new(false);
static DWM_DARK_LOGGED: AtomicBool = AtomicBool::new(false);
static DWM_BACKDROP_LOGGED: AtomicBool = AtomicBool::new(false);

/// Blends `fraction` (0..=256) of color `b` into color `a`.
const fn mix(a: COLORREF, b: COLORREF, fraction: u32) -> COLORREF {
    let (ar, ag, ab) = (a.0 & 0xff, (a.0 >> 8) & 0xff, (a.0 >> 16) & 0xff);
    let (br, bg, bb) = (b.0 & 0xff, (b.0 >> 8) & 0xff, (b.0 >> 16) & 0xff);
    let f = if fraction > 256 { 256 } else { fraction };
    let r = (ar * (256 - f) + br * f) / 256;
    let g = (ag * (256 - f) + bg * f) / 256;
    let bl = (ab * (256 - f) + bb * f) / 256;
    COLORREF((bl << 16) | (g << 8) | r)
}

/// Picks black or white text for readable contrast over `background`.
const fn readable_text(background: COLORREF) -> COLORREF {
    let r = background.0 & 0xff;
    let g = (background.0 >> 8) & 0xff;
    let b = (background.0 >> 16) & 0xff;
    // ITU-R BT.601 luma, plenty for an on-accent binary choice.
    let luma = (r * 299 + g * 587 + b * 114) / 1000;
    if luma > 140 {
        COLORREF(0x0000_0000)
    } else {
        COLORREF(0x00ff_ffff)
    }
}

fn read_dword(subkey: &str, value: &str) -> Option<u32> {
    use std::ffi::c_void;
    use windows::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW};

    let subkey: Vec<u16> = subkey.encode_utf16().chain(Some(0)).collect();
    let value: Vec<u16> = value.encode_utf16().chain(Some(0)).collect();
    let mut data = 0u32;
    let mut size = std::mem::size_of::<u32>() as u32;
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            windows::core::PCWSTR(subkey.as_ptr()),
            windows::core::PCWSTR(value.as_ptr()),
            RRF_RT_REG_DWORD,
            None,
            Some(&mut data as *mut u32 as *mut c_void),
            Some(&mut size),
        )
    };
    (status.0 == 0).then_some(data)
}

fn log_registry_fallback(context: &str) {
    use std::sync::atomic::Ordering;
    if REGISTRY_FALLBACK_LOGGED
        .compare_exchange(false, true, Ordering::Relaxed, Ordering::Relaxed)
        .is_ok()
    {
        crate::diagnostics::record(
            "theme",
            &format!("{context}; using the built-in Fluent fallback"),
        );
    }
}

/// Returns the cached system theme, reading `AppsUseLightTheme` on first use.
pub fn theme() -> Theme {
    match PREFERENCE.load(Ordering::Acquire) {
        1 => return Theme::Light,
        2 => return Theme::Dark,
        _ => {}
    }
    match THEME.load(Ordering::Acquire) {
        1 => Theme::Light,
        2 => Theme::Dark,
        _ => {
            let light = read_dword(
                r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize",
                "AppsUseLightTheme",
            )
            .map(|value| value != 0)
            .unwrap_or_else(|| {
                log_registry_fallback("AppsUseLightTheme is unavailable");
                true
            });
            let known = if light { 1 } else { 2 };
            THEME.store(known, Ordering::Release);
            if light { Theme::Light } else { Theme::Dark }
        }
    }
}

/// The current user's accent color (`AccentColorMenu`, ABGR == COLORREF layout).
pub fn system_accent(fallback: COLORREF) -> COLORREF {
    let mut guard = match ACCENT.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    if let Some(value) = *guard {
        return COLORREF(value);
    }
    match read_dword(
        r"Software\Microsoft\Windows\CurrentVersion\Explorer\Accent",
        "AccentColorMenu",
    ) {
        Some(value) if value & 0x00ff_ffff != 0 => {
            let value = value & 0x00ff_ffff;
            *guard = Some(value);
            COLORREF(value)
        }
        _ => {
            log_registry_fallback("AccentColorMenu is unavailable");
            fallback
        }
    }
}

/// The full token set for the current theme and system accent.
pub fn tokens() -> Tokens {
    tokens_for(theme())
}

/// The full token set for an explicit theme (used by paint code that must
/// stay consistent for the whole frame even if the system flips mid-paint).
pub fn tokens_for(theme: Theme) -> Tokens {
    match theme {
        Theme::Light => {
            let accent = system_accent(FALLBACK_ACCENT_LIGHT);
            Tokens {
                page: COLORREF(0x00f3_f3f3),
                card: COLORREF(0x00ff_ffff),
                stroke: COLORREF(0x00e5_e5e5),
                text: COLORREF(0x001b_1b1b),
                text_secondary: COLORREF(0x005d_5d5d),
                text_disabled: COLORREF(0x00a2_a2a2),
                accent,
                accent_text: readable_text(accent),
                accent_tint: mix(COLORREF(0x00ff_ffff), accent, 31),
                control_fill: COLORREF(0x00ff_ffff),
                control_hover: COLORREF(0x00f9_f9f9),
            }
        }
        Theme::Dark => {
            let accent = system_accent(FALLBACK_ACCENT_DARK);
            Tokens {
                page: COLORREF(0x0020_2020),
                card: COLORREF(0x002b_2b2b),
                stroke: COLORREF(0x003a_3a3a),
                text: COLORREF(0x00ff_ffff),
                text_secondary: COLORREF(0x00cf_cfcf),
                text_disabled: COLORREF(0x007a_7a7a),
                accent,
                accent_text: readable_text(accent),
                accent_tint: mix(COLORREF(0x0020_2020), accent, 64),
                control_fill: COLORREF(0x002b_2b2b),
                control_hover: COLORREF(0x0032_3232),
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Typography — Segoe UI Variable pixel sizes and weights (GDI takes pixels).
// The compact scale keeps a 15 px page title over 12 px body text, so every
// surface stays dense without losing the Fluent hierarchy.
// ---------------------------------------------------------------------------
pub const FONT_TITLE_PX: i32 = 15;
pub const FONT_BODY_PX: i32 = 12;
pub const FONT_SECTION_PX: i32 = 11;
pub const FONT_WEIGHT_TITLE: i32 = 600;
pub const FONT_WEIGHT_SECTION: i32 = 600;

/// The modern Windows 11 UI face, with a Windows 10 fallback.
///
/// GDI silently substitutes a different face when the requested one is not
/// installed, so the choice is probed once and cached: asking for
/// "Segoe UI Variable Text" on Windows 10 would otherwise fall back to an
/// arbitrary default instead of the classic Segoe UI.
pub fn ui_face() -> &'static str {
    use std::sync::LazyLock;
    static FACE: LazyLock<&'static str> = LazyLock::new(|| {
        const MODERN: &str = "Segoe UI Variable Text";
        const CLASSIC: &str = "Segoe UI";
        if face_installed(MODERN) {
            MODERN
        } else {
            CLASSIC
        }
    });
    *FACE
}

/// True when `face` resolves to itself on this system. GDI returns the
/// substituted face name, which is the only reliable installation probe.
fn face_installed(face: &str) -> bool {
    use windows::Win32::Graphics::Gdi::{
        CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS, CreateCompatibleDC, CreateFontW, DEFAULT_CHARSET,
        DEFAULT_PITCH, DeleteDC, DeleteObject, FW_NORMAL, GetTextFaceW, HGDIOBJ,
        OUT_DEFAULT_PRECIS, SelectObject,
    };
    let name: Vec<u16> = face.encode_utf16().chain(Some(0)).collect();
    unsafe {
        let dc = CreateCompatibleDC(None);
        if dc.is_invalid() {
            return false;
        }
        let font = CreateFontW(
            -12,
            0,
            0,
            0,
            FW_NORMAL.0 as i32,
            0,
            0,
            0,
            DEFAULT_CHARSET.0 as u32,
            OUT_DEFAULT_PRECIS.0 as u32,
            CLIP_DEFAULT_PRECIS.0 as u32,
            CLEARTYPE_QUALITY.0 as u32,
            DEFAULT_PITCH.0 as u32,
            windows::core::PCWSTR(name.as_ptr()),
        );
        if font.is_invalid() {
            let _ = DeleteDC(dc);
            return false;
        }
        let previous = SelectObject(dc, HGDIOBJ(font.0));
        let mut buffer = [0u16; 64];
        let length = GetTextFaceW(dc, Some(&mut buffer));
        if !previous.is_invalid() {
            SelectObject(dc, previous);
        }
        let _ = DeleteObject(HGDIOBJ(font.0));
        let _ = DeleteDC(dc);
        if length <= 1 {
            return false;
        }
        let selected = String::from_utf16_lossy(&buffer[..(length as usize - 1).min(buffer.len())]);
        selected.eq_ignore_ascii_case(face)
    }
}

// ---------------------------------------------------------------------------
// Metrics — compact Fluent layout grid, all values in 96-DPI pixels.
// ---------------------------------------------------------------------------
/// Corner radius for cards and panels.
pub const RADIUS_CARD: i32 = 6;
/// Base spacing grid.
pub const GRID: i32 = 4;
/// Standard compact control height (buttons, inputs, chips).
pub const CONTROL_HEIGHT: i32 = 26;
/// Page margin around window content.
pub const PAGE_MARGIN: i32 = 16;
/// Inner padding inside a card.
pub const CARD_PADDING: i32 = 12;

// ---------------------------------------------------------------------------
// Cache control — live theme flips and accent broadcasts.
// ---------------------------------------------------------------------------

/// Drops the theme and accent caches so the next read hits the registry.
pub fn invalidate_theme_cache() {
    THEME.store(0, Ordering::Release);
    invalidate_accent();
}

/// Drops only the accent cache (`WM_DWMCOLORIZATIONCOLORCHANGED` and
/// accent-related `WM_SETTINGCHANGE` broadcasts).
pub fn invalidate_accent() {
    let mut guard = match ACCENT.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    *guard = None;
}

// ---------------------------------------------------------------------------
// Window backdrop — dark title bars and Mica.
// ---------------------------------------------------------------------------

/// System backdrop type values passed with [`DWMWA_SYSTEMBACKDROP_TYPE`].
/// `BACKDROP_NONE` keeps the solid frame; `BACKDROP_MICA` enables Mica.
pub const BACKDROP_NONE: i32 = 1;
pub const BACKDROP_MICA: i32 = 2;
/// `DWMWA_SYSTEMBACKDROP_TYPE` — Windows 11, build 22621+.
pub const DWMWA_SYSTEMBACKDROP_TYPE: i32 = 38;
/// `DWMWA_USE_IMMERSIVE_DARK_MODE` — Windows 10 2004+, build 18985+.
pub const DWMWA_USE_IMMERSIVE_DARK_MODE: i32 = 20;
/// The same attribute under its pre-2004 number.
const DWMWA_USE_IMMERSIVE_DARK_MODE_BEFORE_2004: i32 = 19;

fn log_once(flag: &AtomicBool, message: &str) {
    if flag
        .compare_exchange(false, true, Ordering::Relaxed, Ordering::Relaxed)
        .is_ok()
    {
        crate::diagnostics::record("theme", message);
    }
}

/// True when the OS understands [`DWMWA_SYSTEMBACKDROP_TYPE`] (Windows 11
/// 22H2+, build 22621). A registry read failure assumes "not supported".
pub fn backdrop_supported() -> bool {
    use std::ffi::c_void;
    use windows::Win32::System::Registry::{HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ, RegGetValueW};

    let subkey: Vec<u16> = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion"
        .encode_utf16()
        .chain(Some(0))
        .collect();
    let value: Vec<u16> = "CurrentBuildNumber".encode_utf16().chain(Some(0)).collect();
    let mut data = [0u16; 32];
    let mut size = std::mem::size_of_val(&data) as u32;
    let status = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            windows::core::PCWSTR(subkey.as_ptr()),
            windows::core::PCWSTR(value.as_ptr()),
            RRF_RT_REG_SZ,
            None,
            Some(data.as_mut_ptr() as *mut c_void),
            Some(&mut size),
        )
    };
    let build = (status.0 == 0)
        .then(|| {
            let end = data.iter().position(|&c| c == 0).unwrap_or(data.len());
            String::from_utf16(&data[..end])
                .ok()
                .and_then(|text| text.trim().parse::<u32>().ok())
        })
        .flatten();
    match build {
        Some(build) => build >= 22621,
        None => {
            log_once(
                &BACKDROP_CHECK_LOGGED,
                "CurrentBuildNumber is unavailable; assuming no system backdrop",
            );
            false
        }
    }
}

/// One `DwmSetWindowAttribute` call with a 4-byte payload; `false` when DWM
/// rejected the attribute.
unsafe fn set_window_attribute<T>(hwnd: HWND, attribute: i32, value: &T) -> bool {
    use windows::Win32::Graphics::Dwm::{DWMWINDOWATTRIBUTE, DwmSetWindowAttribute};
    unsafe {
        DwmSetWindowAttribute(
            hwnd,
            DWMWINDOWATTRIBUTE(attribute),
            std::ptr::from_ref(value).cast(),
            std::mem::size_of::<T>() as u32,
        )
        .is_ok()
    }
}

/// Applies the dark title bar and (when requested and supported) the Mica
/// backdrop to `hwnd`. Attribute failures are recorded once through
/// diagnostics instead of being swallowed; the window keeps its default look.
///
/// # Safety
/// `hwnd` must be a valid window handle.
pub unsafe fn apply_window_theme(hwnd: HWND, dark: bool, mica: bool) {
    unsafe {
        // Immersive dark mode is a BOOL (0/1) carried as a u32: attribute 20
        // on modern builds, 19 before Windows 10 2004.
        let dark_value: u32 = if dark { 1 } else { 0 };
        let dark_set = set_window_attribute(hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE, &dark_value)
            || set_window_attribute(hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE_BEFORE_2004, &dark_value);
        if !dark_set {
            log_once(
                &DWM_DARK_LOGGED,
                "DwmSetWindowAttribute failed for the immersive dark mode attribute",
            );
        }
        let backdrop = if mica && backdrop_supported() {
            BACKDROP_MICA
        } else {
            BACKDROP_NONE
        };
        if !set_window_attribute(hwnd, DWMWA_SYSTEMBACKDROP_TYPE, &backdrop) {
            log_once(
                &DWM_BACKDROP_LOGGED,
                "DwmSetWindowAttribute failed for the system backdrop attribute",
            );
        }
    }
}
