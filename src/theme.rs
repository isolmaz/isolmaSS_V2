//! Windows 11 Fluent design tokens: the single source for colors, metrics
//! and typography across settings, toolbar and overlay chrome.
//!
//! Colors are dynamic: the current system theme and accent color are read
//! from the registry once and cached until [`invalidate_theme_cache`] runs
//! (window procs call it from `WM_SETTINGCHANGE`).

use std::sync::Mutex;
use std::sync::atomic::{AtomicU8, Ordering};
use windows::Win32::Foundation::COLORREF;

/// Current system theme, derived from `AppsUseLightTheme`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Theme {
    Light,
    Dark,
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
    match THEME.load(Ordering::Acquire) {
        1 => Theme::Light,
        2 => Theme::Dark,
        _ => {
            let light = read_dword(
                r"Software\Microsoft\Windows\CurrentVersion\Explorer\Personalize",
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
// Typography — Segoe UI pixel sizes and weights (GDI takes pixels here).
// ---------------------------------------------------------------------------
pub const FONT_TITLE_PX: i32 = 20;
pub const FONT_BODY_PX: i32 = 14;
pub const FONT_SECTION_PX: i32 = 12;
pub const FONT_WEIGHT_TITLE: i32 = 600;
pub const FONT_WEIGHT_SECTION: i32 = 600;

// ---------------------------------------------------------------------------
// Metrics — Fluent layout grid, all values in 96-DPI pixels.
// ---------------------------------------------------------------------------
/// Corner radius for cards and panels.
pub const RADIUS_CARD: i32 = 8;
/// Base spacing grid.
pub const GRID: i32 = 4;
/// Standard control height (buttons, inputs).
pub const CONTROL_HEIGHT: i32 = 32;
/// Page margin around window content.
pub const PAGE_MARGIN: i32 = 24;
/// Inner padding inside a card.
pub const CARD_PADDING: i32 = 16;
