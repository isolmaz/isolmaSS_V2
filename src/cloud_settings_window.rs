use crate::cloudflare_oauth::{self, Account, Authorization};
use crate::cloudflare_setup;
use crate::settings::Settings;
use serde_json::{Value, json};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use windows::Win32::Foundation::{
    ERROR_CLASS_ALREADY_EXISTS, GetLastError, HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM,
};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreateSolidBrush, DT_CENTER, DT_END_ELLIPSIS, DT_SINGLELINE, DT_VCENTER,
    DeleteObject, DrawTextW, EndPaint, FillRect, HBRUSH, HDC, HFONT, HGDIOBJ, InvalidateRect,
    PAINTSTRUCT, RedrawWindow, SetBkColor, SetBkMode, SetTextColor, TRANSPARENT,
};
use windows::Win32::UI::Controls::{
    CDDS_PREPAINT, CDIS_DISABLED, CDIS_FOCUS, CDIS_HOT, CDIS_SELECTED, CDRF_SKIPDEFAULT,
    NM_CUSTOMDRAW, NMCUSTOMDRAW, NMHDR, SetScrollInfo,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{EnableWindow, IsWindowEnabled, SetActiveWindow};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{PCWSTR, Result, w};

const CLASS: PCWSTR = w!("isolmaSS_CloudSettings");
const WM_CLOUD_RESULT: u32 = WM_APP + 215;
const ID_CONNECT: i32 = 1001;
const ID_ACCOUNT: i32 = 1002;
const ID_INSTALL: i32 = 1003;
const ID_FORGET: i32 = 1004;
const ID_REFRESH: i32 = 1005;
const ID_STATUS: i32 = 1006;
const ID_STATS: i32 = 1007;
const ID_LIMIT_FIRST: i32 = 1020;
const ID_MODE: i32 = 1027;
const ID_SAVE_LIMITS: i32 = 1028;
const ID_IMAGES: i32 = 1029;
const ID_DELETE: i32 = 1030;
const ID_CLOSE: i32 = 1031;
const ID_PASSWORD: i32 = 1032;
const ID_SAVE_PASSWORD: i32 = 1033;

const LIMITS: [(&str, &str, u64); 7] = [
    ("Saklanan son resim", "max_active", 1),
    ("Günlük yükleme", "daily_upload_limit", 1),
    ("Günlük görüntüleme", "daily_view_limit", 1),
    ("Resim boyutu (MB)", "max_image_bytes", 1024 * 1024),
    ("Toplam alan (MB)", "max_storage_bytes", 1024 * 1024),
    ("Saklama (gün)", "retention_days", 1),
    ("Uyarı/durma (%)", "warning_percent", 1),
];

struct State {
    settings: Settings,
    saved: bool,
    for_upload: bool,
    pending: bool,
    authorizing: bool,
    cancel: Arc<AtomicBool>,
    authorization: Option<Authorization>,
    loaded: bool,
    scroll: i32,
    images: Vec<String>,
    response: Arc<Mutex<Option<Completion>>>,
    dpi: u32,
    font: HFONT,
    title_font: HFONT,
    heading_font: HFONT,
    page_brush: HBRUSH,
    card_brush: HBRUSH,
}
impl State {
    fn refresh_brushes(&mut self) {
        unsafe {
            for brush in [self.page_brush, self.card_brush] {
                if !brush.is_invalid() {
                    let _ = DeleteObject(HGDIOBJ(brush.0));
                }
            }
            let tokens = crate::theme::tokens();
            self.page_brush = CreateSolidBrush(tokens.page);
            self.card_brush = CreateSolidBrush(tokens.card);
        }
    }

    fn refresh_fonts(&mut self, hwnd: HWND) {
        let previous = [self.font, self.title_font, self.heading_font];
        self.font = crate::theme::create_ui_font(self.dpi, crate::theme::FONT_BODY_PX, 400);
        self.title_font = crate::theme::create_ui_font(
            self.dpi,
            crate::theme::FONT_TITLE_PX,
            crate::theme::FONT_WEIGHT_TITLE,
        );
        self.heading_font = crate::theme::create_ui_font(
            self.dpi,
            crate::theme::FONT_SECTION_PX,
            crate::theme::FONT_WEIGHT_SECTION,
        );
        for id in 1000..=1206 {
            if let Some(child) = control(hwnd, id) {
                let font = if id == 1100 {
                    self.title_font
                } else if matches!(id, 1102..=1107 | 1110) {
                    self.heading_font
                } else {
                    self.font
                };
                unsafe {
                    SendMessageW(child, WM_SETFONT, WPARAM(font.0 as usize), LPARAM(1));
                }
            }
        }
        for font in previous {
            if !font.is_invalid() {
                unsafe {
                    let _ = DeleteObject(HGDIOBJ(font.0));
                }
            }
        }
    }
}
impl Drop for State {
    fn drop(&mut self) {
        for font in [self.font, self.title_font, self.heading_font] {
            if !font.is_invalid() {
                unsafe {
                    let _ = DeleteObject(HGDIOBJ(font.0));
                }
            }
        }
        for brush in [self.page_brush, self.card_brush] {
            if !brush.is_invalid() {
                unsafe {
                    let _ = DeleteObject(HGDIOBJ(brush.0));
                }
            }
        }
    }
}
enum Completion {
    Authorization(std::result::Result<Authorization, String>),
    Install(std::result::Result<cloudflare_oauth::Installed, String>),
    Refresh(std::result::Result<(Value, Value), String>),
    Limits(std::result::Result<Value, String>),
    Delete(std::result::Result<String, String>),
}
fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}
fn control(hwnd: HWND, id: i32) -> Option<HWND> {
    unsafe { GetDlgItem(hwnd, id).ok() }
}
fn set_text(hwnd: HWND, id: i32, text: &str) {
    if let Some(child) = control(hwnd, id) {
        let value = wide(text);
        unsafe {
            let _ = SetWindowTextW(child, PCWSTR(value.as_ptr()));
        }
    }
}
fn get_text(hwnd: HWND, id: i32) -> std::result::Result<String, String> {
    let child = control(hwnd, id).ok_or("Cloudflare field is missing.")?;
    let size = unsafe { GetWindowTextLengthW(child) };
    if !(0..=512).contains(&size) {
        return Err("Cloudflare field is too long.".into());
    }
    let mut buffer = vec![0u16; size as usize + 1];
    let count = unsafe { GetWindowTextW(child, &mut buffer) };
    Ok(String::from_utf16_lossy(&buffer[..count.max(0) as usize])
        .trim()
        .to_string())
}
fn create(hwnd: HWND, class: PCWSTR, id: i32, text: &str, style: WINDOW_STYLE) -> Result<HWND> {
    let text = wide(text);
    unsafe {
        CreateWindowExW(
            Default::default(),
            class,
            PCWSTR(text.as_ptr()),
            WS_CHILD | WS_VISIBLE | style,
            0,
            0,
            0,
            0,
            hwnd,
            HMENU(id as *mut _),
            HINSTANCE::default(),
            None,
        )
    }
}
fn button(hwnd: HWND, id: i32, text: &str) -> Result<HWND> {
    create(
        hwnd,
        w!("BUTTON"),
        id,
        text,
        WINDOW_STYLE(WS_TABSTOP.0 | BS_PUSHBUTTON as u32),
    )
}
fn label(hwnd: HWND, id: i32, text: &str) -> Result<HWND> {
    create(hwnd, w!("STATIC"), id, text, WINDOW_STYLE(0))
}
fn edit(hwnd: HWND, id: i32, password: bool) -> Result<HWND> {
    create(
        hwnd,
        w!("EDIT"),
        id,
        "",
        WINDOW_STYLE(
            WS_BORDER.0
                | WS_TABSTOP.0
                | ES_AUTOHSCROLL as u32
                | if password { ES_PASSWORD as u32 } else { 0 },
        ),
    )
}
fn place(hwnd: HWND, id: i32, rect: (i32, i32, i32, i32), dpi: u32, scroll: i32) {
    let Some(child) = control(hwnd, id) else {
        return;
    };
    let (x, y, width, height) = rect;
    unsafe {
        let _ = MoveWindow(
            child,
            x * dpi as i32 / 96,
            y * dpi as i32 / 96 - scroll,
            width * dpi as i32 / 96,
            height * dpi as i32 / 96,
            true,
        );
    }
}
fn cloud_geometry(width: i32) -> (i32, i32, i32, i32) {
    let columns = if width < 700 { 1 } else { 2 };
    let rows = (LIMITS.len() as i32 + columns - 1) / columns;
    let mode_y = 588 + rows * 64;
    let images_y = mode_y + 104;
    (columns, mode_y, images_y, images_y + 286)
}
fn layout(hwnd: HWND, state: &mut State) {
    let dpi = state.dpi as i32;
    let mut client = RECT::default();
    unsafe {
        let _ = GetClientRect(hwnd, &mut client);
    }
    let design_width = client.right * 96 / dpi;
    let right = design_width - 24;
    let content = right - 24;
    let guided = state.settings.cloud_url.is_none();
    let (columns, mode_y, images_y, page_height) = if guided {
        (1, 0, 0, 504)
    } else {
        cloud_geometry(design_width)
    };
    state.scroll = state
        .scroll
        .clamp(0, (page_height * dpi / 96 - client.bottom).max(0));
    let info = SCROLLINFO {
        cbSize: std::mem::size_of::<SCROLLINFO>() as u32,
        fMask: SIF_RANGE | SIF_PAGE | SIF_POS,
        nMin: 0,
        nMax: page_height * dpi / 96 - 1,
        nPage: client.bottom.max(1) as u32,
        nPos: state.scroll,
        ..Default::default()
    };
    unsafe {
        SetScrollInfo(hwnd, SB_VERT, &info, true);
    }
    if guided {
        for (id, rect) in [
            (1100, (24, 28, content, 40)),
            (1101, (24, 82, content, 68)),
            (1110, (44, 180, content - 40, 26)),
            (1111, (44, 216, content - 40, 44)),
            (ID_CONNECT, (44, 274, 300, 42)),
            (1102, (44, 180, content - 40, 27)),
            (ID_ACCOUNT, (44, 225, content - 40, 196)),
            (ID_INSTALL, (44, 278, 300, 42)),
            (ID_STATUS, (44, 372, content - 40, 62)),
            (ID_CLOSE, (right - 132, 452, 132, 38)),
        ] {
            place(hwnd, id, rect, state.dpi, state.scroll);
        }
    } else {
        for (id, rect) in [
            (1100, (24, 28, content - 200, 40)),
            (1101, (24, 76, content, 30)),
            (ID_FORGET, (right - 184, 32, 184, 38)),
            (ID_STATUS, (44, 138, content - 40, 44)),
            (1103, (44, 226, content - 40, 26)),
            (ID_REFRESH, (right - 148, 218, 124, 38)),
            (ID_STATS, (44, 270, content - 40, 78)),
            (1104, (44, 398, content - 40, 26)),
            (ID_PASSWORD, (44, 446, content - 222, 38)),
            (ID_SAVE_PASSWORD, (right - 168, 446, 168, 38)),
            (1105, (44, 538, content - 40, 26)),
            (1106, (44, mode_y, content - 40, 26)),
            (ID_MODE, (44, mode_y + 32, content - 228, 180)),
            (ID_SAVE_LIMITS, (right - 168, mode_y + 32, 168, 38)),
            (1107, (44, images_y + 22, content - 40, 26)),
            (ID_IMAGES, (44, images_y + 64, content - 40, 90)),
            (ID_DELETE, (44, images_y + 166, 188, 38)),
            (1108, (44, images_y + 216, content - 40, 24)),
            (ID_CLOSE, (right - 132, images_y + 242, 132, 38)),
        ] {
            place(hwnd, id, rect, state.dpi, state.scroll);
        }
        let field_width = if columns == 2 {
            (content - 60) / 2
        } else {
            content - 40
        };
        for (index, _) in LIMITS.iter().enumerate() {
            let index = index as i32;
            let x = 44 + index % columns * (field_width + 20);
            let y = 582 + index / columns * 64;
            place(
                hwnd,
                1200 + index,
                (x, y, field_width, 23),
                state.dpi,
                state.scroll,
            );
            place(
                hwnd,
                ID_LIMIT_FIRST + index,
                (x, y + 25, field_width, 34),
                state.dpi,
                state.scroll,
            );
        }
    }
    unsafe {
        let _ = InvalidateRect(hwnd, None, true);
    }
}
fn account_controls(hwnd: HWND, visible: bool) {
    for id in [1102, ID_ACCOUNT, ID_INSTALL, 1110, 1111, ID_CONNECT] {
        if let Some(child) = control(hwnd, id) {
            let account_step = matches!(id, 1102 | ID_ACCOUNT | ID_INSTALL);
            unsafe {
                let _ = ShowWindow(
                    child,
                    if visible == account_step {
                        SW_SHOW
                    } else {
                        SW_HIDE
                    },
                );
            }
        }
    }
}
fn set_status(hwnd: HWND, text: &str) {
    set_text(hwnd, ID_STATUS, text);
}
fn set_busy(hwnd: HWND, state: &mut State, busy: bool) {
    state.pending = busy;
    for id in [
        ID_CONNECT,
        ID_INSTALL,
        ID_REFRESH,
        ID_SAVE_LIMITS,
        ID_SAVE_PASSWORD,
        ID_DELETE,
        ID_FORGET,
    ] {
        if let Some(child) = control(hwnd, id) {
            unsafe {
                let _ = EnableWindow(child, !busy);
            }
        }
    }
}
fn start(hwnd: HWND, state: &mut State, task: impl FnOnce() -> Completion + Send + 'static) {
    if state.pending {
        return;
    }
    set_busy(hwnd, state, true);
    let output = state.response.clone();
    let window = hwnd.0 as usize;
    if let Err(error) = std::thread::Builder::new()
        .name("isolmass-cloud-settings".into())
        .spawn(move || {
            let completion = task();
            match output.lock() {
                Ok(mut slot) => *slot = Some(completion),
                Err(_) => {
                    crate::diagnostics::record("cloud settings", "Cloud response lock failed.");
                    return;
                }
            }
            if let Err(error) = unsafe {
                PostMessageW(
                    HWND(window as *mut _),
                    WM_CLOUD_RESULT,
                    WPARAM(0),
                    LPARAM(0),
                )
            } {
                crate::diagnostics::record(
                    "cloud settings",
                    &format!("Cloud response notification failed: {error}"),
                );
            }
        })
    {
        set_busy(hwnd, state, false);
        set_status(
            hwnd,
            &format!("Could not start Cloudflare request: {error}"),
        );
    }
}
fn begin_authorization(hwnd: HWND, state: &mut State) {
    state.cancel.store(false, Ordering::Relaxed);
    state.authorizing = true;
    set_status(
        hwnd,
        "Tarayıcıda Cloudflare'a giriş yapıp izin verin. Görüntü bu sırada yüklenmez.",
    );
    let cancel = state.cancel.clone();
    start(hwnd, state, move || {
        Completion::Authorization(cloudflare_oauth::authorize(&cancel))
    });
}
fn begin_install(hwnd: HWND, state: &mut State, account: Account) {
    let Some(auth) = state.authorization.take() else {
        return;
    };
    state.authorizing = false;
    set_status(
        hwnd,
        &format!(
            "{} hesabına yeni Worker kuruluyor; pencereyi kapatmayın…",
            account.name
        ),
    );
    let cancel = state.cancel.clone();
    start(hwnd, state, move || {
        Completion::Install(cloudflare_oauth::install(
            &auth.access_token,
            &account,
            &cancel,
        ))
    });
}
fn refresh(hwnd: HWND, state: &mut State) {
    let Some(origin) = state.settings.cloud_url.clone() else {
        return;
    };
    set_status(hwnd, "İstatistikler ve resimler yükleniyor…");
    start(hwnd, state, move || {
        Completion::Refresh(
            cloudflare_setup::admin_json(&origin, "/api/stats", "GET", None).and_then(|stats| {
                cloudflare_setup::admin_json(&origin, "/api/images?limit=50&offset=0", "GET", None)
                    .map(|images| (stats, images))
            }),
        )
    });
}
fn save_limits(hwnd: HWND, state: &mut State) {
    let Some(origin) = state.settings.cloud_url.clone() else {
        return;
    };
    if !state.loaded {
        crate::ui::error(
            hwnd,
            "Cloudflare",
            "Önce güncel sınırları Yenile ile yükleyin.",
        );
        return;
    }
    let mut values = serde_json::Map::new();
    for (index, (_, field, multiplier)) in LIMITS.iter().enumerate() {
        let parsed = get_text(hwnd, ID_LIMIT_FIRST + index as i32)
            .ok()
            .and_then(|text| text.parse::<u64>().ok())
            .and_then(|number| number.checked_mul(*multiplier));
        let Some(number) = parsed else {
            crate::ui::error(
                hwnd,
                "Geçersiz sınır",
                "Bütün alanlara pozitif tam sayı girin.",
            );
            return;
        };
        values.insert((*field).into(), json!(number));
    }
    let choice = control(hwnd, ID_MODE)
        .map(|combo| unsafe { SendMessageW(combo, CB_GETCURSEL, WPARAM(0), LPARAM(0)).0 })
        .unwrap_or(-1);
    let action = match choice {
        0 => "warn",
        1 => "block_upload",
        2 => "block_all",
        _ => {
            crate::ui::error(hwnd, "Cloudflare", "Sınırda davranışı seçin.");
            return;
        }
    };
    values.insert("limit_action".into(), json!(action));
    set_status(hwnd, "Sınırlar kaydediliyor…");
    start(hwnd, state, move || {
        Completion::Limits(cloudflare_setup::admin_json(
            &origin,
            "/api/settings",
            "PUT",
            Some(&Value::Object(values)),
        ))
    });
}
fn show_stats(hwnd: HWND, state: &mut State, stats: &Value, images: &Value) {
    let count = |value: &Value, key: &str| value.get(key).and_then(Value::as_u64).unwrap_or(0);
    let daily = &stats["daily"];
    let monthly = &stats["monthly"];
    let stored = &stats["images"];
    let usage = stored["stored_bytes"].as_u64().unwrap_or(0);
    set_text(
        hwnd,
        ID_STATS,
        &format!(
            "Bugün: {} yükleme · {} görüntülenme\r\nBu ay: {} yükleme · {} görüntülenme\r\nAktif resim: {} · {:.1} MB\r\nCloudflare Free hesabının genel kullanımını kendi panelinizden izleyin.",
            count(daily, "uploads"),
            count(daily, "views"),
            count(monthly, "uploads"),
            count(monthly, "views"),
            count(stored, "active"),
            usage as f64 / 1_048_576.0
        ),
    );
    for (index, (_, field, multiplier)) in LIMITS.iter().enumerate() {
        if let Some(number) = stats["settings"][field].as_u64() {
            set_text(
                hwnd,
                ID_LIMIT_FIRST + index as i32,
                &(number / multiplier).to_string(),
            );
        }
    }
    let mode = match stats["settings"]["limit_action"].as_str() {
        Some("warn") => 0,
        Some("block_upload") => 1,
        Some("block_all") => 2,
        _ => -1,
    };
    if let Some(combo) = control(hwnd, ID_MODE) {
        unsafe {
            SendMessageW(combo, CB_SETCURSEL, WPARAM(mode as usize), LPARAM(0));
        }
    }
    state.loaded = true;
    state.images.clear();
    if let Some(list) = control(hwnd, ID_IMAGES) {
        unsafe {
            SendMessageW(list, LB_RESETCONTENT, WPARAM(0), LPARAM(0));
        }
        if let Some(records) = images["images"].as_array() {
            for record in records {
                if let Some(id) = record["id"].as_str() {
                    state.images.push(id.to_string());
                    let label = wide(&format!(
                        "{}… · {} görüntülenme",
                        &id[..8.min(id.len())],
                        count(record, "views")
                    ));
                    unsafe {
                        SendMessageW(
                            list,
                            LB_ADDSTRING,
                            WPARAM(0),
                            LPARAM(label.as_ptr() as isize),
                        );
                    }
                }
            }
        }
    }
    set_status(hwnd, "Bağlı · Güncel istatistikler alındı.");
}
fn delete_image(hwnd: HWND, state: &mut State) {
    let Some(origin) = state.settings.cloud_url.clone() else {
        return;
    };
    let Some(list) = control(hwnd, ID_IMAGES) else {
        return;
    };
    let index = unsafe { SendMessageW(list, LB_GETCURSEL, WPARAM(0), LPARAM(0)).0 };
    let Some(id) = usize::try_from(index)
        .ok()
        .and_then(|i| state.images.get(i))
        .cloned()
    else {
        crate::ui::error(hwnd, "Resim sil", "Önce listeden bir resim seçin.");
        return;
    };
    if !crate::ui::confirm(
        hwnd,
        "Resmi sil",
        "Bağlantı geri alınamaz biçimde kapanacak. Devam edilsin mi?",
    ) {
        return;
    }
    set_status(hwnd, "Resim siliniyor…");
    start(hwnd, state, move || {
        Completion::Delete(
            cloudflare_setup::admin_json(&origin, &format!("/api/images/{id}"), "DELETE", None)
                .map(|_| id),
        )
    });
}
fn save_password(hwnd: HWND, state: &mut State) {
    let Some(origin) = state.settings.cloud_url.as_deref() else {
        return;
    };
    let password = match get_text(hwnd, ID_PASSWORD) {
        Ok(value) if value.is_empty() => None,
        Ok(value)
            if value.chars().count() >= 12
                && value.len() <= 128
                && !value.chars().any(char::is_control) =>
        {
            Some(value)
        }
        Ok(_) => {
            crate::ui::error(
                hwnd,
                "Resim şifresi",
                "Şifre en az 12 karakter ve en fazla 128 UTF-8 bayt olmalı; isterseniz boş bırakın.",
            );
            return;
        }
        Err(error) => {
            crate::ui::error(hwnd, "Resim şifresi", &error);
            return;
        }
    };
    match cloudflare_setup::load_credentials(origin).and_then(|mut credentials| {
        credentials.share_password = password;
        cloudflare_setup::save_credentials(&credentials)
    }) {
        Ok(()) => set_status(
            hwnd,
            "Yeni yüklemelerin şifresi bu bilgisayarda korumalı saklandı.",
        ),
        Err(error) => crate::ui::error(hwnd, "Resim şifresi", &error),
    }
}
fn disconnect(hwnd: HWND, state: &mut State) {
    if !crate::ui::confirm(
        hwnd,
        "Bağlantıyı kaldır",
        "Yalnızca bu bilgisayardaki Cloudflare bağlantısı silinecek; buluttaki Worker ve resimler kalacak. Devam edilsin mi?",
    ) {
        return;
    }
    let mut changed = state.settings.clone();
    changed.cloud_url = None;
    if let Err(error) = changed.save() {
        crate::ui::error(hwnd, "Ayarlar kaydedilemedi", &error.to_string());
        return;
    }
    if let Err(error) = cloudflare_setup::forget_credentials() {
        crate::ui::error(hwnd, "Kimlik bilgileri silinemedi", &error);
        return;
    }
    state.settings = changed;
    state.saved = true;
    unsafe {
        let _ = DestroyWindow(hwnd);
    }
}
fn command(hwnd: HWND, state: &mut State, id: i32) {
    if state.pending && id != ID_CLOSE {
        return;
    }
    match id {
        ID_CONNECT => begin_authorization(hwnd, state),
        ID_INSTALL => {
            let selection = control(hwnd, ID_ACCOUNT)
                .map(|combo| unsafe { SendMessageW(combo, CB_GETCURSEL, WPARAM(0), LPARAM(0)).0 });
            let selected = selection
                .and_then(|i| usize::try_from(i).ok())
                .and_then(|i| state.authorization.as_ref()?.accounts.get(i))
                .cloned();
            if let Some(account) = selected {
                begin_install(hwnd, state, account);
            } else {
                crate::ui::error(hwnd, "Cloudflare", "Kurulum yapılacak hesabı seçin.");
            }
        }
        ID_REFRESH => refresh(hwnd, state),
        ID_SAVE_LIMITS => save_limits(hwnd, state),
        ID_SAVE_PASSWORD => save_password(hwnd, state),
        ID_DELETE => delete_image(hwnd, state),
        ID_FORGET => disconnect(hwnd, state),
        ID_CLOSE => close(hwnd, state),
        _ => {}
    }
}
fn close(hwnd: HWND, state: &mut State) {
    if state.pending {
        if state.authorizing {
            state.cancel.store(true, Ordering::Relaxed);
            set_status(hwnd, "Cloudflare giriş isteği iptal ediliyor…");
        } else {
            set_status(
                hwnd,
                "Cloudflare işlemi bitmeden pencere kapatılamaz; birkaç saniye bekleyin.",
            );
        }
        return;
    }
    unsafe {
        let _ = DestroyWindow(hwnd);
    }
}
fn cloud_uses_card(id: i32) -> bool {
    !matches!(id, 1100 | 1101 | ID_FORGET | ID_CLOSE | 1108)
}

fn paint_surface(hwnd: HWND, state: &State, hdc: HDC) {
    let mut client = RECT::default();
    unsafe {
        let _ = GetClientRect(hwnd, &mut client);
        let _ = FillRect(hdc, &client, state.page_brush);
    }
    let scale = |value: i32| value * state.dpi as i32 / 96;
    let right = client.right * 96 / state.dpi as i32 - 24;
    let tokens = crate::theme::tokens();
    let guided = state.settings.cloud_url.is_none();
    let (_, mode_y, images_y, _) = cloud_geometry(client.right * 96 / state.dpi as i32);
    let cards: &[(i32, i32, i32, i32)] = if guided {
        &[(24, 160, right, 334), (24, 352, right, 446)]
    } else {
        &[
            (24, 122, right, 196),
            (24, 208, right, 366),
            (24, 380, right, 504),
            (24, 520, right, mode_y + 90),
            (24, images_y, right, images_y + 212),
        ]
    };
    for &(left, top, card_right, bottom) in cards {
        let top = scale(top) - state.scroll;
        let bottom = scale(bottom) - state.scroll;
        if bottom <= 0 || top >= client.bottom {
            continue;
        }
        crate::drawing::rounded(
            hdc,
            crate::capture::Rect::new(scale(left), top, scale(card_right), bottom),
            scale(crate::theme::RADIUS_CARD),
            tokens.card,
            tokens.stroke,
        );
    }
}

fn draw_cloud_button(draw: &NMCUSTOMDRAW, state: &State) {
    let id = draw.hdr.idFrom as i32;
    let disabled = draw.uItemState.contains(CDIS_DISABLED);
    let focused = draw.uItemState.contains(CDIS_FOCUS);
    let hovered = draw.uItemState.contains(CDIS_HOT) || draw.uItemState.contains(CDIS_SELECTED);
    let primary = matches!(id, ID_CONNECT | ID_INSTALL | ID_SAVE_LIMITS);
    let tokens = crate::theme::tokens();
    let mut rect = RECT::default();
    unsafe {
        let _ = GetClientRect(draw.hdr.hwndFrom, &mut rect);
        let _ = FillRect(
            draw.hdc,
            &rect,
            if cloud_uses_card(id) {
                state.card_brush
            } else {
                state.page_brush
            },
        );
    }
    let fill = if disabled {
        tokens.control_fill
    } else if primary {
        tokens.accent
    } else if hovered {
        tokens.control_hover
    } else {
        tokens.control_fill
    };
    let stroke = if focused {
        tokens.accent
    } else {
        tokens.stroke
    };
    let inset = state.dpi as i32 / 96;
    crate::drawing::rounded(
        draw.hdc,
        crate::capture::Rect::new(inset, inset, rect.right - inset, rect.bottom - inset),
        crate::theme::RADIUS_CARD * state.dpi as i32 / 96,
        fill,
        stroke,
    );
    let mut label = [0u16; 128];
    let length = unsafe { GetWindowTextW(draw.hdr.hwndFrom, &mut label) }.max(0) as usize;
    crate::drawing::with_font(
        draw.hdc,
        -(crate::theme::FONT_BODY_PX * state.dpi as i32 / 96),
        600,
        || unsafe {
            let _ = SetBkMode(draw.hdc, TRANSPARENT);
            let _ = SetTextColor(
                draw.hdc,
                if disabled {
                    tokens.text_disabled
                } else if primary {
                    tokens.accent_text
                } else {
                    tokens.text
                },
            );
            let _ = DrawTextW(
                draw.hdc,
                &mut label[..length],
                &mut rect,
                DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
            );
        },
    );
}

unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let pointer = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *mut State;
    match msg {
        WM_ERASEBKGND if !pointer.is_null() => {
            let state = unsafe { &*pointer };
            let mut client = RECT::default();
            unsafe {
                let _ = GetClientRect(hwnd, &mut client);
                let _ = FillRect(HDC(wparam.0 as *mut _), &client, state.page_brush);
            }
            LRESULT(1)
        }
        WM_PAINT if !pointer.is_null() => {
            let mut paint = PAINTSTRUCT::default();
            let hdc = unsafe { BeginPaint(hwnd, &mut paint) };
            paint_surface(hwnd, unsafe { &*pointer }, hdc);
            unsafe {
                let _ = EndPaint(hwnd, &paint);
            }
            LRESULT(0)
        }
        WM_NOTIFY if !pointer.is_null() && lparam.0 != 0 => {
            let header = unsafe { &*(lparam.0 as *const NMHDR) };
            if header.code == NM_CUSTOMDRAW {
                let draw = unsafe { &*(lparam.0 as *const NMCUSTOMDRAW) };
                if draw.dwDrawStage == CDDS_PREPAINT {
                    draw_cloud_button(draw, unsafe { &*pointer });
                    return LRESULT(CDRF_SKIPDEFAULT as isize);
                }
            }
            LRESULT(0)
        }
        WM_CTLCOLOREDIT | WM_CTLCOLORLISTBOX if !pointer.is_null() => {
            let state = unsafe { &*pointer };
            let tokens = crate::theme::tokens();
            let hdc = HDC(wparam.0 as *mut _);
            unsafe {
                let _ = SetTextColor(hdc, tokens.text);
                let _ = SetBkColor(hdc, tokens.card);
            }
            LRESULT(state.card_brush.0 as isize)
        }
        WM_CTLCOLORSTATIC | WM_CTLCOLORBTN if !pointer.is_null() => {
            let state = unsafe { &*pointer };
            let child = HWND(lparam.0 as *mut _);
            let id = unsafe { GetDlgCtrlID(child) };
            let hdc = HDC(wparam.0 as *mut _);
            let tokens = crate::theme::tokens();
            unsafe {
                let _ = SetBkMode(hdc, TRANSPARENT);
                let _ = SetTextColor(
                    hdc,
                    if !IsWindowEnabled(child).as_bool() {
                        tokens.text_disabled
                    } else if matches!(id, 1101 | 1108 | 1111 | ID_STATUS) {
                        tokens.text_secondary
                    } else {
                        tokens.text
                    },
                );
            }
            LRESULT(if cloud_uses_card(id) {
                state.card_brush.0 as isize
            } else {
                state.page_brush.0 as isize
            })
        }
        WM_SETTINGCHANGE if !pointer.is_null() => {
            crate::theme::invalidate_theme_cache();
            let state = unsafe { &mut *pointer };
            state.refresh_brushes();
            unsafe {
                crate::theme::apply_window_theme(
                    hwnd,
                    crate::theme::theme() == crate::theme::Theme::Dark,
                    true,
                );
            }

            unsafe {
                let _ = RedrawWindow(
                    hwnd,
                    None,
                    None,
                    windows::Win32::Graphics::Gdi::RDW_INVALIDATE
                        | windows::Win32::Graphics::Gdi::RDW_ALLCHILDREN,
                );
            }
            LRESULT(0)
        }
        WM_DWMCOLORIZATIONCOLORCHANGED if !pointer.is_null() => {
            crate::theme::invalidate_accent();
            unsafe {
                (*pointer).refresh_brushes();
            }
            unsafe {
                let _ = RedrawWindow(
                    hwnd,
                    None,
                    None,
                    windows::Win32::Graphics::Gdi::RDW_INVALIDATE
                        | windows::Win32::Graphics::Gdi::RDW_ALLCHILDREN,
                );
            }
            LRESULT(0)
        }
        WM_DPICHANGED if !pointer.is_null() => {
            let state = unsafe { &mut *pointer };
            state.dpi = ((wparam.0 & 0xffff) as u32).max(96);
            if lparam.0 != 0 {
                let rect = unsafe { &*(lparam.0 as *const RECT) };
                unsafe {
                    let _ = SetWindowPos(
                        hwnd,
                        None,
                        rect.left,
                        rect.top,
                        rect.right - rect.left,
                        rect.bottom - rect.top,
                        SWP_NOZORDER | SWP_NOACTIVATE,
                    );
                }
            }
            state.refresh_fonts(hwnd);
            layout(hwnd, state);
            LRESULT(0)
        }
        WM_GETMINMAXINFO if !pointer.is_null() && lparam.0 != 0 => {
            let state = unsafe { &*pointer };
            let info = unsafe { &mut *(lparam.0 as *mut MINMAXINFO) };
            let minimum = if state.settings.cloud_url.is_none() {
                540
            } else {
                600
            };
            info.ptMinTrackSize.x = minimum * state.dpi as i32 / 96;
            info.ptMinTrackSize.y = 400 * state.dpi as i32 / 96;
            LRESULT(0)
        }
        WM_CLOUD_RESULT if !pointer.is_null() => {
            let state = unsafe { &mut *pointer };
            set_busy(hwnd, state, false);
            state.authorizing = false;
            let response = state.response.lock().ok().and_then(|mut slot| slot.take());
            if matches!(response, Some(Completion::Authorization(_))) {
                // The browser holds the foreground after the consent page.
                crate::ui::bring_to_front(hwnd);
            }
            match response {
                Some(Completion::Authorization(Ok(auth)))
                    if state.cancel.load(Ordering::Relaxed) =>
                {
                    drop(auth);
                    set_status(
                        hwnd,
                        "Cloudflare bağlantısı iptal edildi; görüntü yerelde kaldı.",
                    );
                }
                Some(Completion::Authorization(Ok(auth))) => {
                    let only = auth.accounts.len() == 1;
                    if let Some(combo) = control(hwnd, ID_ACCOUNT) {
                        unsafe {
                            SendMessageW(combo, CB_RESETCONTENT, WPARAM(0), LPARAM(0));
                        }
                        for account in &auth.accounts {
                            let text = wide(&format!("{} · {}", account.name, &account.id[24..]));
                            unsafe {
                                SendMessageW(
                                    combo,
                                    CB_ADDSTRING,
                                    WPARAM(0),
                                    LPARAM(text.as_ptr() as isize),
                                );
                            }
                        }
                        unsafe {
                            SendMessageW(combo, CB_SETCURSEL, WPARAM(0), LPARAM(0));
                        }
                    }
                    state.authorization = Some(auth);
                    if only {
                        if let Some(account) = state
                            .authorization
                            .as_ref()
                            .and_then(|auth| auth.accounts.first())
                            .cloned()
                        {
                            begin_install(hwnd, state, account);
                        }
                    } else {
                        account_controls(hwnd, true);
                        set_status(
                            hwnd,
                            "Hesabı seçin; yalnızca seçilen hesabınızda yeni Worker oluşturulur.",
                        );
                    }
                }
                Some(Completion::Install(Ok(installed))) => {
                    let credentials = installed.credentials;
                    let mut updated = state.settings.clone();
                    updated.cloud_url = Some(credentials.origin.clone());
                    if let Err(error) = cloudflare_setup::save_credentials(&credentials)
                        .and_then(|()| updated.save().map_err(|error| error.to_string()))
                    {
                        set_status(hwnd, "Worker kuruldu ama yerel bağlantı kaydedilemedi.");
                        crate::ui::error(hwnd, "Kurulum kaydedilemedi", &error);
                    } else {
                        state.settings = updated;
                        state.saved = true;
                        let message = match (installed.ready, state.for_upload) {
                            (true, true) => "Bağlantı kuruldu. Görüntü şimdi yükleniyor.",
                            (true, false) => {
                                "Bağlantı kuruldu. Artık Yükle düğmesini kullanabilirsiniz."
                            }
                            (false, true) => {
                                "Worker hesabınıza kuruldu ve bu bilgisayarla eşleştirildi. workers.dev adresinin etkinleşmesi birkaç dakika sürebilir; yükleme şimdi denenecek, olmazsa biraz sonra tekrar deneyin."
                            }
                            (false, false) => {
                                "Worker hesabınıza kuruldu ve bu bilgisayarla eşleştirildi. workers.dev adresinin etkinleşmesi birkaç dakika sürebilir."
                            }
                        };
                        crate::ui::info(hwnd, "Cloudflare hazır", message);
                        unsafe {
                            let _ = DestroyWindow(hwnd);
                        }
                    }
                }
                Some(Completion::Refresh(Ok((stats, images)))) => {
                    show_stats(hwnd, state, &stats, &images)
                }
                Some(Completion::Limits(Ok(_))) => {
                    set_status(hwnd, "Sınırlar kaydedildi.");
                    refresh(hwnd, state);
                }
                Some(Completion::Delete(Ok(_))) => {
                    set_status(hwnd, "Resim silindi.");
                    refresh(hwnd, state);
                }
                Some(Completion::Install(Err(error))) => {
                    account_controls(hwnd, false);
                    set_status(
                        hwnd,
                        "Kurulum tamamlanamadı. Yeniden Cloudflare ile devam edin.",
                    );
                    crate::ui::error(hwnd, "Cloudflare", &error);
                }
                Some(Completion::Authorization(Err(error)))
                | Some(Completion::Refresh(Err(error)))
                | Some(Completion::Limits(Err(error)))
                | Some(Completion::Delete(Err(error))) => {
                    set_status(hwnd, "Cloudflare işlemi başarısız; ayrıntı uyarıda.");
                    crate::ui::error(hwnd, "Cloudflare", &error);
                }
                None => crate::diagnostics::record(
                    "cloud settings",
                    "Completion message without response.",
                ),
            }
            LRESULT(0)
        }
        WM_SIZE if !pointer.is_null() => {
            layout(hwnd, unsafe { &mut *pointer });
            LRESULT(0)
        }
        WM_VSCROLL if !pointer.is_null() => {
            let state = unsafe { &mut *pointer };
            let mut client = RECT::default();
            unsafe {
                let _ = GetClientRect(hwnd, &mut client);
            }
            let step = 40;
            state.scroll = match (wparam.0 & 0xffff) as i32 {
                value if value == SB_LINEUP.0 => state.scroll - step,
                value if value == SB_LINEDOWN.0 => state.scroll + step,
                value if value == SB_PAGEUP.0 => state.scroll - client.bottom,
                value if value == SB_PAGEDOWN.0 => state.scroll + client.bottom,
                value if value == SB_THUMBTRACK.0 || value == SB_THUMBPOSITION.0 => {
                    (wparam.0 >> 16) as i32
                }
                _ => state.scroll,
            };
            layout(hwnd, state);
            LRESULT(0)
        }
        WM_MOUSEWHEEL if !pointer.is_null() => {
            let state = unsafe { &mut *pointer };
            let wheel = ((wparam.0 >> 16) as u16 as i16) as i32;
            state.scroll -= wheel / 120 * 60;
            layout(hwnd, state);
            LRESULT(0)
        }
        WM_COMMAND if !pointer.is_null() && wparam.0 >> 16 == 0 => {
            command(hwnd, unsafe { &mut *pointer }, (wparam.0 & 0xffff) as i32);
            LRESULT(0)
        }
        WM_CLOSE if !pointer.is_null() => {
            close(hwnd, unsafe { &mut *pointer });
            LRESULT(0)
        }
        WM_NCDESTROY => unsafe {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            DefWindowProcW(hwnd, msg, wparam, lparam)
        },
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}
fn register() -> Result<()> {
    static REGISTERED: AtomicBool = AtomicBool::new(false);
    if REGISTERED.load(Ordering::Acquire) {
        return Ok(());
    }
    let class = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        lpfnWndProc: Some(wnd_proc),
        hInstance: HINSTANCE::default(),
        hIcon: crate::tray::app_icon(),
        hIconSm: crate::tray::app_icon(),
        hCursor: unsafe { LoadCursorW(HINSTANCE::default(), IDC_ARROW).unwrap_or_default() },
        hbrBackground: HBRUSH::default(),
        lpszClassName: CLASS,
        ..Default::default()
    };
    if unsafe { RegisterClassExW(&class) } == 0
        && unsafe { GetLastError() } != ERROR_CLASS_ALREADY_EXISTS
    {
        return Err(windows::core::Error::from_win32());
    }
    REGISTERED.store(true, Ordering::Release);
    Ok(())
}
fn build_controls(hwnd: HWND, state: &State) -> Result<()> {
    if state.settings.cloud_url.is_none() {
        label(hwnd, 1100, "Cloudflare'a bağlan")?;
        label(
            hwnd,
            1101,
            "Ekran görüntüleriniz yalnızca sizin Cloudflare hesabınızdaki Worker'a yüklenir. Bağlanana kadar hiçbir görüntü gönderilmez.",
        )?;
        label(hwnd, 1110, "1 · Cloudflare hesabında izin verin")?;
        label(
            hwnd,
            1111,
            "İzin ekranı tarayıcıda açılır. Kurulumdan önce kullanılacak hesabı siz seçersiniz.",
        )?;
        button(hwnd, ID_CONNECT, "Cloudflare ile devam et")?;
        label(hwnd, 1102, "2 · Kurulacak hesabı seçin")?;
        create(
            hwnd,
            w!("COMBOBOX"),
            ID_ACCOUNT,
            "",
            WINDOW_STYLE(WS_TABSTOP.0 | CBS_DROPDOWNLIST as u32 | WS_VSCROLL.0),
        )?;
        button(hwnd, ID_INSTALL, "Bu hesaba kur")?;
        label(
            hwnd,
            ID_STATUS,
            "Bağlantı kurulmadı · Ekran görüntüsü bu bilgisayarda kalır.",
        )?;
        button(hwnd, ID_CLOSE, "Vazgeç")?;
        account_controls(hwnd, false);
    } else {
        label(hwnd, 1100, "Bulut paylaşımı")?;
        label(
            hwnd,
            1101,
            state.settings.cloud_url.as_deref().unwrap_or(""),
        )?;
        button(hwnd, ID_FORGET, "Bağlantıyı kaldır")?;
        label(
            hwnd,
            ID_STATUS,
            "Bağlı Worker hazır. İstatistikleri Yenile ile yükleyin.",
        )?;
        label(hwnd, 1103, "İstatistikler")?;
        button(hwnd, ID_REFRESH, "Yenile")?;
        label(hwnd, ID_STATS, "Henüz veri yüklenmedi.")?;
        label(
            hwnd,
            1104,
            "Yeni yüklemeler için resim şifresi · Boş bırakılırsa şifresiz",
        )?;
        edit(hwnd, ID_PASSWORD, true)?;
        button(hwnd, ID_SAVE_PASSWORD, "Şifreyi kaydet")?;
        label(hwnd, 1105, "Kota ve saklama ayarları")?;
        for (index, (name, _, _)) in LIMITS.iter().enumerate() {
            label(hwnd, 1200 + index as i32, name)?;
            edit(hwnd, ID_LIMIT_FIRST + index as i32, false)?;
        }
        label(hwnd, 1106, "Sınırda davranış")?;
        let combo = create(
            hwnd,
            w!("COMBOBOX"),
            ID_MODE,
            "",
            WINDOW_STYLE(WS_TABSTOP.0 | CBS_DROPDOWNLIST as u32 | WS_VSCROLL.0),
        )?;
        for text in [
            "Yalnızca uyar",
            "Yeni yüklemeleri durdur",
            "Yükleme ve görüntülemeyi durdur",
        ] {
            let value = wide(text);
            unsafe {
                SendMessageW(
                    combo,
                    CB_ADDSTRING,
                    WPARAM(0),
                    LPARAM(value.as_ptr() as isize),
                );
            }
        }
        button(hwnd, ID_SAVE_LIMITS, "Sınırları kaydet")?;
        label(
            hwnd,
            1107,
            "Son resimler · Silinen bağlantılar hemen kapanır",
        )?;
        create(
            hwnd,
            w!("LISTBOX"),
            ID_IMAGES,
            "",
            WINDOW_STYLE(WS_BORDER.0 | WS_TABSTOP.0 | WS_VSCROLL.0 | LBS_NOTIFY as u32),
        )?;
        button(hwnd, ID_DELETE, "Seçili resmi sil")?;
        label(
            hwnd,
            1108,
            "Ücret uyarıları tahmindir; Cloudflare faturasını garanti etmez.",
        )?;
        button(hwnd, ID_CLOSE, "Kapat")?;
        if let Some(origin) = state.settings.cloud_url.as_deref()
            && let Ok(credentials) = cloudflare_setup::load_credentials(origin)
        {
            set_text(
                hwnd,
                ID_PASSWORD,
                credentials.share_password.as_deref().unwrap_or(""),
            );
        }
    }
    Ok(())
}
struct OwnerGuard(HWND);
impl Drop for OwnerGuard {
    fn drop(&mut self) {
        unsafe {
            let _ = EnableWindow(self.0, true);
            let _ = SetActiveWindow(self.0);
        }
    }
}
pub fn show(current: &Settings, owner: HWND) -> Result<Option<Settings>> {
    show_impl(current, Some(owner), false)
}
/// Opened from the capture editor, which hides itself meanwhile: the dialog is
/// a normal top-level window with its own taskbar button so the user can move
/// between it and the browser that shows the Cloudflare consent page.
pub fn show_for_upload(current: &Settings) -> Result<Option<Settings>> {
    show_impl(current, None, true)
}
fn fit_dialog_to_work_area(hwnd: HWND, owner: Option<HWND>, width: i32, height: i32, dpi: u32) {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint, MonitorFromWindow,
    };
    let monitor = match owner {
        Some(owner) => unsafe { MonitorFromWindow(owner, MONITOR_DEFAULTTONEAREST) },
        None => {
            let mut cursor = POINT::default();
            unsafe {
                let _ = GetCursorPos(&mut cursor);
                MonitorFromPoint(cursor, MONITOR_DEFAULTTONEAREST)
            }
        }
    };
    let mut monitor_info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    if !unsafe { GetMonitorInfoW(monitor, &mut monitor_info).as_bool() } {
        crate::diagnostics::record("Cloudflare görünümü", "Çalışma alanı ölçülemedi.");
        return;
    }
    let work = monitor_info.rcWork;
    let width = (width * dpi as i32 / 96).min(work.right - work.left);
    let height = (height * dpi as i32 / 96).min(work.bottom - work.top);
    let mut anchor = work;
    if let Some(owner) = owner
        && unsafe { GetWindowRect(owner, &mut anchor) }.is_err()
    {
        anchor = work;
    }
    let x = ((anchor.left + anchor.right - width) / 2).clamp(work.left, work.right - width);
    let y = ((anchor.top + anchor.bottom - height) / 2).clamp(work.top, work.bottom - height);
    if let Err(error) = unsafe {
        SetWindowPos(
            hwnd,
            None,
            x,
            y,
            width,
            height,
            SWP_NOZORDER | SWP_NOACTIVATE,
        )
    } {
        crate::diagnostics::record("Cloudflare görünümü", &error.to_string());
    }
}

fn show_impl(
    current: &Settings,
    owner: Option<HWND>,
    for_upload: bool,
) -> Result<Option<Settings>> {
    register()?;
    crate::theme::set_preference(current.theme_preference);
    let guided = current.cloud_url.is_none();
    let mut state = Box::new(State {
        settings: current.clone(),
        saved: false,
        for_upload,
        pending: false,
        authorizing: false,
        cancel: Arc::new(AtomicBool::new(false)),
        authorization: None,
        loaded: false,
        scroll: 0,
        images: Vec::new(),
        response: Arc::new(Mutex::new(None)),
        dpi: 96,
        font: HFONT::default(),
        title_font: HFONT::default(),
        heading_font: HFONT::default(),
        page_brush: HBRUSH::default(),
        card_brush: HBRUSH::default(),
    });
    state.refresh_brushes();
    let hwnd = unsafe {
        CreateWindowExW(
            Default::default(),
            CLASS,
            w!("isolmaSS Cloudflare"),
            WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN | WS_VSCROLL,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            if guided { 680 } else { 800 },
            if guided { 560 } else { 700 },
            owner.unwrap_or_default(),
            None,
            HINSTANCE::default(),
            None,
        )
    }?;
    let _window = crate::ui::OwnedWindow(hwnd);
    let _suspend = crate::hotkey::OverlayInputSuspension::new();
    let _owner = owner.map(|owner| {
        unsafe {
            let _ = EnableWindow(owner, false);
        }
        OwnerGuard(owner)
    });
    unsafe {
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, state.as_mut() as *mut State as isize);
    }
    state.dpi = unsafe { windows::Win32::UI::HiDpi::GetDpiForWindow(hwnd) }.max(96);
    build_controls(hwnd, &state)?;
    state.refresh_fonts(hwnd);
    fit_dialog_to_work_area(
        hwnd,
        owner,
        if guided { 680 } else { 800 },
        if guided { 560 } else { 700 },
        state.dpi,
    );
    let current_dpi = unsafe { windows::Win32::UI::HiDpi::GetDpiForWindow(hwnd) }.max(96);
    if current_dpi != state.dpi {
        state.dpi = current_dpi;
        state.refresh_fonts(hwnd);
    }

    layout(hwnd, &mut state);
    unsafe {
        crate::theme::apply_window_theme(
            hwnd,
            crate::theme::theme() == crate::theme::Theme::Dark,
            true,
        );
        let _ = ShowWindow(hwnd, SW_SHOW);
    }
    crate::ui::bring_to_front(hwnd);
    crate::ui::window_loop(hwnd, crate::ui::WindowKind::CloudSettings)?;
    if state.saved {
        Ok(Some(state.settings.clone()))
    } else {
        Ok(None)
    }
}
