use crate::cloudflare_setup::{self, CloudCredentials, RequestBody};
use crate::settings::Settings;
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
use windows::Win32::Foundation::{
    ERROR_CLASS_ALREADY_EXISTS, GetLastError, HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM,
};
use windows::Win32::Graphics::Gdi::{CreateFontW, DeleteObject, HFONT, HGDIOBJ, InvalidateRect};
use windows::Win32::UI::Controls::SetScrollInfo;
use windows::Win32::UI::Input::KeyboardAndMouse::{EnableWindow, SetActiveWindow};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{PCWSTR, Result, w};

const CLASS: PCWSTR = w!("isolmaSS_CloudSettings");
const WM_CLOUD_RESULT: u32 = WM_APP + 215;
const PAGE_HEIGHT: i32 = 1032;
const DEPLOY_URL: &str = "https://deploy.workers.cloudflare.com/?url=https://github.com/isolmaz/isolmaSS_V2/tree/v0.5.1/cloudflare";
const ID_GENERATE: i32 = 1001;
const ID_DEPLOY: i32 = 1002;
const ID_ORIGIN: i32 = 1003;
const ID_UPLOAD_TOKEN: i32 = 1004;
const ID_ADMIN_TOKEN: i32 = 1005;
const ID_PASSWORD: i32 = 1006;
const ID_COPY_UPLOAD: i32 = 1007;
const ID_COPY_ADMIN: i32 = 1008;
const ID_PAIR: i32 = 1009;
const ID_FORGET: i32 = 1010;
const ID_REFRESH: i32 = 1011;
const ID_STATUS: i32 = 1012;
const ID_STATS: i32 = 1013;
const ID_LIMIT_FIRST: i32 = 1020;
const ID_MODE: i32 = 1027;
const ID_SAVE_LIMITS: i32 = 1028;
const ID_IMAGES: i32 = 1029;
const ID_DELETE: i32 = 1030;
const ID_CLOSE: i32 = 1031;

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
    guided: bool,
    for_upload: bool,
    setup_started: bool,
    pending: bool,
    loaded: bool,
    scroll: i32,
    images: Vec<String>,
    response: Arc<Mutex<Option<Completion>>>,
    font: HFONT,
}
impl Drop for State {
    fn drop(&mut self) {
        if !self.font.is_invalid() {
            unsafe {
                let _ = DeleteObject(HGDIOBJ(self.font.0));
            }
        }
    }
}
enum Completion {
    Pair(std::result::Result<CloudCredentials, String>),
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
fn set_text(hwnd: HWND, id: i32, value: &str) {
    if let Some(child) = control(hwnd, id) {
        let value = wide(value);
        unsafe {
            let _ = SetWindowTextW(child, PCWSTR(value.as_ptr()));
        }
    }
}
fn get_text(hwnd: HWND, id: i32) -> std::result::Result<String, String> {
    let child = control(hwnd, id).ok_or_else(|| "Missing Cloudflare field.".to_string())?;
    let size = unsafe { GetWindowTextLengthW(child) };
    if !(0..=512).contains(&size) {
        return Err("Cloudflare field is too long.".to_string());
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
fn layout(hwnd: HWND, state: &mut State) {
    let dpi = unsafe { windows::Win32::UI::HiDpi::GetDpiForWindow(hwnd) }.max(96);
    let mut client = RECT::default();
    unsafe {
        let _ = GetClientRect(hwnd, &mut client);
    }
    if state.guided {
        let right = (client.right * 96 / dpi as i32 - 20).max(300);
        let wide = right - 20;
        let rows: &[(i32, Position)] = if state.setup_started {
            &[
                (1100, (20, 15, wide, 30)),
                (1101, (20, 53, wide, 47)),
                (ID_DEPLOY, (20, 106, 225, 32)),
                (1103, (20, 153, wide, 24)),
                (ID_UPLOAD_TOKEN, (20, 180, wide - 96, 30)),
                (ID_COPY_UPLOAD, (right - 88, 180, 88, 30)),
                (1104, (20, 218, wide, 24)),
                (ID_ADMIN_TOKEN, (20, 245, wide - 96, 30)),
                (ID_COPY_ADMIN, (right - 88, 245, 88, 30)),
                (1102, (20, 287, wide, 24)),
                (ID_ORIGIN, (20, 313, wide, 30)),
                (ID_STATUS, (20, 361, wide, 84)),
                (ID_PAIR, (20, 461, 187, 34)),
                (ID_CLOSE, (right - 116, 461, 116, 34)),
            ]
        } else {
            &[
                (1100, (20, 18, wide, 30)),
                (1101, (20, 60, wide, 62)),
                (ID_DEPLOY, (20, 136, 270, 36)),
                (ID_STATUS, (20, 185, wide, 61)),
                (ID_CLOSE, (right - 116, 255, 116, 32)),
            ]
        };
        for &(id, rect) in rows {
            place(hwnd, id, rect, dpi, 0);
        }
        return;
    }
    let max = (PAGE_HEIGHT * dpi as i32 / 96 - client.bottom).max(0);
    state.scroll = state.scroll.clamp(0, max);
    let info = SCROLLINFO {
        cbSize: std::mem::size_of::<SCROLLINFO>() as u32,
        fMask: SIF_RANGE | SIF_PAGE | SIF_POS,
        nMin: 0,
        nMax: PAGE_HEIGHT * dpi as i32 / 96 - 1,
        nPage: client.bottom.max(1) as u32,
        nPos: state.scroll,
        ..Default::default()
    };
    unsafe {
        SetScrollInfo(hwnd, SB_VERT, &info, true);
    }
    let right = (client.right * 96 / dpi as i32 - 20).max(300);
    let wide = right - 20;
    type Position = (i32, i32, i32, i32);
    let rows: &[(i32, Position)] = &[
        (1100, (20, 14, wide, 28)),
        (ID_GENERATE, (20, 50, 208, 30)),
        (ID_DEPLOY, (238, 50, 202, 30)),
        (1101, (20, 91, wide, 22)),
        (1102, (20, 123, 120, 22)),
        (ID_ORIGIN, (145, 120, wide - 125, 28)),
        (1103, (20, 159, 120, 22)),
        (ID_UPLOAD_TOKEN, (145, 156, wide - 235, 28)),
        (ID_COPY_UPLOAD, (right - 82, 156, 82, 28)),
        (1104, (20, 195, 120, 22)),
        (ID_ADMIN_TOKEN, (145, 192, wide - 235, 28)),
        (ID_COPY_ADMIN, (right - 82, 192, 82, 28)),
        (1105, (20, 231, 120, 22)),
        (ID_PASSWORD, (145, 228, wide - 125, 28)),
        (ID_PAIR, (20, 270, 138, 30)),
        (ID_FORGET, (169, 270, 138, 30)),
        (ID_STATUS, (20, 306, wide, 42)),
        (1106, (20, 360, wide, 26)),
        (ID_REFRESH, (right - 115, 358, 115, 28)),
        (ID_STATS, (20, 392, wide, 96)),
        (1107, (20, 503, wide, 26)),
        (1108, (20, 674, 135, 24)),
        (ID_MODE, (158, 670, 290, 150)),
        (ID_SAVE_LIMITS, (right - 153, 708, 153, 30)),
        (1110, (20, 754, wide, 26)),
        (ID_IMAGES, (20, 784, wide, 130)),
        (ID_DELETE, (20, 922, 180, 30)),
        (1109, (20, 960, wide, 25)),
        (ID_CLOSE, (right - 112, 990, 112, 30)),
    ];
    for &(id, rect) in rows {
        place(hwnd, id, rect, dpi, state.scroll);
    }
    for (index, _) in LIMITS.iter().enumerate() {
        let col = index % 2;
        let row = index / 2;
        let x = 20 + col as i32 * 310;
        let y = 532 + row as i32 * 34;
        place(
            hwnd,
            1200 + index as i32,
            (x, y, 153, 26),
            dpi,
            state.scroll,
        );
        place(
            hwnd,
            ID_LIMIT_FIRST + index as i32,
            (x + 163, y, 110, 26),
            dpi,
            state.scroll,
        );
    }
    unsafe {
        let _ = InvalidateRect(hwnd, None, true);
    }
}
fn show_guided_step(hwnd: HWND, state: &mut State) {
    for id in [
        1102,
        ID_ORIGIN,
        1103,
        ID_UPLOAD_TOKEN,
        ID_COPY_UPLOAD,
        1104,
        ID_ADMIN_TOKEN,
        ID_COPY_ADMIN,
        ID_PAIR,
    ] {
        if let Some(child) = control(hwnd, id) {
            unsafe {
                let _ = ShowWindow(
                    child,
                    if state.setup_started {
                        SW_SHOW
                    } else {
                        SW_HIDE
                    },
                );
            }
        }
    }
    set_text(
        hwnd,
        ID_DEPLOY,
        if state.setup_started {
            "Cloudflare sayfasını aç"
        } else {
            "Kurulumu başlat"
        },
    );
    let dpi = unsafe { windows::Win32::UI::HiDpi::GetDpiForWindow(hwnd) }.max(96);
    unsafe {
        let _ = SetWindowPos(
            hwnd,
            None,
            0,
            0,
            560 * dpi as i32 / 96,
            (if state.setup_started { 570 } else { 330 }) * dpi as i32 / 96,
            SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
    layout(hwnd, state);
}
fn set_status(hwnd: HWND, text: &str) {
    set_text(hwnd, ID_STATUS, text);
}
fn set_busy(hwnd: HWND, state: &mut State, busy: bool) {
    state.pending = busy;
    for id in [
        ID_PAIR,
        ID_REFRESH,
        ID_SAVE_LIMITS,
        ID_DELETE,
        ID_FORGET,
        ID_DEPLOY,
        ID_GENERATE,
    ] {
        if let Some(control) = control(hwnd, id) {
            unsafe {
                let _ = EnableWindow(control, !busy);
            }
        }
    }
}
fn start(hwnd: HWND, state: &mut State, task: impl FnOnce() -> Completion + Send + 'static) {
    if state.pending {
        return;
    }
    set_busy(hwnd, state, true);
    let result = state.response.clone();
    let window = hwnd.0 as usize;
    match std::thread::Builder::new()
        .name("isolmass-cloud-settings".into())
        .spawn(move || {
            let outcome = task();
            if let Ok(mut slot) = result.lock() {
                *slot = Some(outcome);
            } else {
                crate::diagnostics::record("cloud settings", "Cloud response lock failed.");
                return;
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
        }) {
        Ok(_) => {}
        Err(error) => {
            set_busy(hwnd, state, false);
            set_status(hwnd, &format!("Could not start request: {error}"));
        }
    }
}
fn connect(hwnd: HWND, state: &mut State) {
    let data = (|| {
        let credentials = CloudCredentials {
            origin: get_text(hwnd, ID_ORIGIN)?,
            upload_token: get_text(hwnd, ID_UPLOAD_TOKEN)?,
            admin_token: get_text(hwnd, ID_ADMIN_TOKEN)?,
            share_password: if state.guided {
                None
            } else {
                match get_text(hwnd, ID_PASSWORD)?.as_str() {
                    "" => None,
                    text => Some(text.to_string()),
                }
            },
        };
        if !cloudflare_setup::valid_cloud_origin(&credentials.origin) {
            return Err("Cloudflare kurulumunda verilen https://...workers.dev adresini yapıştırın; alan adı gerekmez.".to_string());
        }
        Ok(credentials)
    })();
    let credentials = match data {
        Ok(data) => data,
        Err(error) => {
            crate::ui::error(hwnd, "Cloudflare setup", &error);
            return;
        }
    };
    set_status(hwnd, "Connecting to your Worker…");
    start(hwnd, state, move || {
        let result = cloudflare_setup::api_request(
            &credentials.origin,
            "/api/setup",
            "POST",
            &credentials.admin_token,
            None,
            None,
            RequestBody::Bytes(&[]),
        )
        .and_then(|(status, data)| {
            if status == 200 {
                Ok(credentials)
            } else if status == 401 {
                Err("ADMIN_TOKEN Cloudflare'daki gizli alanla eşleşmiyor. Buradaki Kopyala düğmesiyle doğru anahtarı Worker sırrına girin.".to_string())
            } else {
                let message = serde_json::from_slice::<Value>(&data)
                    .ok()
                    .and_then(|value| {
                        value
                            .get("error")
                            .and_then(Value::as_str)
                            .map(str::to_owned)
                    })
                    .unwrap_or_else(|| format!("Worker returned HTTP {status}."));
                Err(message)
            }
        });
        Completion::Pair(result)
    });
}
fn refresh(hwnd: HWND, state: &mut State) {
    let Some(origin) = state.settings.cloud_url.clone() else {
        set_status(hwnd, "Pair a Worker before loading statistics.");
        return;
    };
    set_status(hwnd, "Loading statistics and images…");
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
        crate::ui::error(hwnd, "Cloudflare", "Connect your Worker first.");
        return;
    };
    if !state.loaded {
        crate::ui::error(
            hwnd,
            "Cloudflare",
            "Load the current limits before changing them.",
        );
        return;
    }
    let mut object = serde_json::Map::new();
    for (index, (_label, field, multiplier)) in LIMITS.iter().enumerate() {
        let text = match get_text(hwnd, ID_LIMIT_FIRST + index as i32) {
            Ok(text) => text,
            Err(error) => {
                crate::ui::error(hwnd, "Invalid limit", &error);
                return;
            }
        };
        let Some(value) = text
            .parse::<u64>()
            .ok()
            .and_then(|number| number.checked_mul(*multiplier))
        else {
            crate::ui::error(
                hwnd,
                "Invalid limit",
                "Enter whole positive numbers for every limit.",
            );
            return;
        };
        object.insert((*field).to_string(), json!(value));
    }
    let mode = if let Some(combo) = control(hwnd, ID_MODE) {
        unsafe { SendMessageW(combo, CB_GETCURSEL, WPARAM(0), LPARAM(0)).0 }
    } else {
        -1
    };
    let action = match mode {
        0 => "warn",
        1 => "block_upload",
        2 => "block_all",
        _ => {
            crate::ui::error(hwnd, "Cloudflare", "Select a limit action.");
            return;
        }
    };
    object.insert("limit_action".into(), json!(action));
    let value = Value::Object(object);
    set_status(hwnd, "Saving Worker limits…");
    start(hwnd, state, move || {
        Completion::Limits(cloudflare_setup::admin_json(
            &origin,
            "/api/settings",
            "PUT",
            Some(&value),
        ))
    });
}
fn show_stats(hwnd: HWND, state: &mut State, stats: &Value, images: &Value) {
    let integer = |record: &Value, key: &str| record.get(key).and_then(Value::as_u64).unwrap_or(0);
    let daily = &stats["daily"];
    let monthly = &stats["monthly"];
    let stored = &stats["images"];
    let active = integer(stored, "active");
    let bytes = integer(stored, "stored_bytes");
    let estimate = integer(&stats["estimates"], "worker_requests");
    let max_uploads = integer(&stats["settings"], "daily_upload_limit").max(1);
    let max_views = integer(&stats["settings"], "daily_view_limit").max(1);
    let upload_percent = integer(daily, "uploads").saturating_mul(100) / max_uploads;
    let view_percent = integer(daily, "views").saturating_mul(100) / max_views;
    let paid_usd = stats["estimates"]["paid_usd"].as_f64().unwrap_or(0.0);
    set_text(
        hwnd,
        ID_STATS,
        &format!(
            "Bugün: {} yükleme · {} görüntülenme     Bu ay: {} yükleme · {} görüntülenme\r\nAktif: {active} resim · {:.1} MB     İstek (ay): {estimate}\r\nGünlük sınır: yükleme %{upload_percent} · görüntüleme %{view_percent}\r\nÜcretli plan tahmini: ${paid_usd:.2} / ay; Free plan ve gerçek fatura farklı olabilir.",
            integer(daily, "uploads"),
            integer(daily, "views"),
            integer(monthly, "uploads"),
            integer(monthly, "views"),
            bytes as f64 / 1_048_576.0
        ),
    );
    for (index, (_label, field, multiplier)) in LIMITS.iter().enumerate() {
        if let Some(number) = stats["settings"][field].as_u64() {
            set_text(
                hwnd,
                ID_LIMIT_FIRST + index as i32,
                &(number / multiplier).to_string(),
            );
        }
    }
    let choice = match stats["settings"]["limit_action"].as_str() {
        Some("warn") => 0,
        Some("block_upload") => 1,
        Some("block_all") => 2,
        _ => -1,
    };
    if let Some(combo) = control(hwnd, ID_MODE) {
        unsafe {
            SendMessageW(combo, CB_SETCURSEL, WPARAM(choice as usize), LPARAM(0));
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
                    let text = wide(&format!(
                        "{}… · {} görüntülenme",
                        &id[..8.min(id.len())],
                        integer(record, "views")
                    ));
                    unsafe {
                        SendMessageW(
                            list,
                            LB_ADDSTRING,
                            WPARAM(0),
                            LPARAM(text.as_ptr() as isize),
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
        .and_then(|index| state.images.get(index))
        .cloned()
    else {
        crate::ui::error(hwnd, "Resim sil", "Önce listeden bir resim seçin.");
        return;
    };
    if !crate::ui::confirm(
        hwnd,
        "Resmi sil",
        "Bu bağlantı geçersiz olacak. İşlem geri alınamaz. Devam edilsin mi?",
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
fn start_guided_setup(hwnd: HWND, state: &mut State) -> std::result::Result<bool, String> {
    let tokens = match cloudflare_setup::load_pending_tokens() {
        Ok(Some(tokens)) => tokens,
        Ok(None) => cloudflare_setup::create_pending_tokens()?,
        Err(error) => {
            if !crate::ui::confirm(
                hwnd,
                "Kurulumu baştan başlat",
                &format!(
                    "{error}\nEski Cloudflare anahtarları artık çalışmayacak. Yeni anahtar oluşturulsun mu?"
                ),
            ) {
                return Ok(false);
            }
            cloudflare_setup::create_pending_tokens()?
        }
    };
    set_text(hwnd, ID_UPLOAD_TOKEN, &tokens.upload_token);
    set_text(hwnd, ID_ADMIN_TOKEN, &tokens.admin_token);
    state.setup_started = true;
    show_guided_step(hwnd, state);
    set_status(
        hwnd,
        "Cloudflare'da giriş yapıp R2'yi onaylayın. İki anahtarı ayrı sır alanlarına yapıştırın. Kurulum bitince Worker adresini aşağıya girin.",
    );
    Ok(true)
}

fn command(hwnd: HWND, state: &mut State, id: i32) {
    if state.pending && id != ID_CLOSE {
        return;
    }
    match id {
        ID_GENERATE => match (
            cloudflare_setup::generate_token(),
            cloudflare_setup::generate_token(),
        ) {
            (Ok(upload), Ok(admin)) => {
                set_text(hwnd, ID_UPLOAD_TOKEN, &upload);
                set_text(hwnd, ID_ADMIN_TOKEN, &admin);
                set_status(
                    hwnd,
                    "Yeni anahtarlar hazır. Cloudflare formuna iki anahtarı yapıştırın; ardından Worker adresini eşleştirin.",
                );
            }
            (Err(error), _) | (_, Err(error)) => {
                crate::ui::error(hwnd, "Anahtar üretilemedi", &error)
            }
        },
        ID_COPY_UPLOAD | ID_COPY_ADMIN => {
            match get_text(
                hwnd,
                if id == ID_COPY_UPLOAD {
                    ID_UPLOAD_TOKEN
                } else {
                    ID_ADMIN_TOKEN
                },
            )
            .and_then(|token| {
                if token.is_empty() {
                    Err("Önce anahtar oluşturun veya yazın.".to_string())
                } else {
                    Ok(token)
                }
            })
            .and_then(|token| {
                crate::clipboard::copy_text_to_clipboard(Some(hwnd), &token)
                    .map_err(|error| error.to_string())
            }) {
                Ok(()) => set_status(
                    hwnd,
                    "Anahtar panoda. Yalnızca Cloudflare sır alanına yapıştırın.",
                ),
                Err(error) => crate::ui::error(hwnd, "Pano", &error),
            }
        }
        ID_DEPLOY => {
            if state.guided {
                match start_guided_setup(hwnd, state) {
                    Ok(true) => {}
                    Ok(false) => return,
                    Err(error) => {
                        crate::ui::error(hwnd, "Kurulum başlatılamadı", &error);
                        return;
                    }
                }
            }
            let url = wide(DEPLOY_URL);
            unsafe {
                let result = ShellExecuteW(
                    hwnd,
                    w!("open"),
                    PCWSTR(url.as_ptr()),
                    PCWSTR::null(),
                    PCWSTR::null(),
                    SW_SHOWNORMAL,
                );
                if result.0 as isize <= 32 {
                    crate::ui::error(
                        hwnd,
                        "Cloudflare kurulumu",
                        "Tarayıcı açılamadı. Kurulum bağlantısını dokümanlardan açın.",
                    );
                }
            }
        }
        ID_PAIR => connect(hwnd, state),
        ID_REFRESH => refresh(hwnd, state),
        ID_SAVE_LIMITS => save_limits(hwnd, state),
        ID_DELETE => delete_image(hwnd, state),
        ID_FORGET => {
            if !crate::ui::confirm(
                hwnd,
                "Bağlantıyı kaldır",
                "Yalnızca bu bilgisayardaki Cloudflare bağlantısı silinecek; buluttaki resimler kalacak. Devam edilsin mi?",
            ) {
                return;
            }
            match cloudflare_setup::forget_credentials() {
                Ok(()) => {
                    let mut changed = state.settings.clone();
                    changed.cloud_url = None;
                    if let Err(error) = changed.save() {
                        crate::ui::error(hwnd, "Ayarlar kaydedilemedi", &error.to_string());
                        return;
                    }
                    state.settings = changed;
                    state.saved = true;
                    state.loaded = false;
                    set_status(
                        hwnd,
                        "Yerel bağlantı kaldırıldı. Buluttaki resimler değişmedi.",
                    );
                    set_text(hwnd, ID_ORIGIN, "");
                    set_text(hwnd, ID_UPLOAD_TOKEN, "");
                    set_text(hwnd, ID_ADMIN_TOKEN, "");
                    set_text(hwnd, ID_PASSWORD, "");
                    set_text(hwnd, ID_STATS, "Bağlı bir Worker yok.");
                }
                Err(error) => crate::ui::error(hwnd, "Bağlantı silinemedi", &error),
            }
        }
        ID_CLOSE => unsafe {
            let _ = DestroyWindow(hwnd);
        },
        _ => {}
    }
}
unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let pointer = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *mut State;
    match msg {
        WM_CLOUD_RESULT if !pointer.is_null() => {
            let state = unsafe { &mut *pointer };
            set_busy(hwnd, state, false);
            let completion = state.response.lock().ok().and_then(|mut slot| slot.take());
            match completion {
                Some(Completion::Pair(Ok(credentials))) => {
                    let mut updated = state.settings.clone();
                    updated.cloud_url = Some(credentials.origin.clone());
                    if let Err(error) = cloudflare_setup::save_credentials(&credentials)
                        .and_then(|()| updated.save().map_err(|error| error.to_string()))
                    {
                        crate::ui::error(hwnd, "Kurulum kaydedilemedi", &error);
                    } else {
                        state.settings = updated;
                        state.saved = true;
                        if let Err(error) = cloudflare_setup::forget_pending_tokens() {
                            crate::diagnostics::record("Cloudflare setup cleanup", &error);
                        }
                        if state.guided {
                            unsafe {
                                let _ = DestroyWindow(hwnd);
                            }
                        } else {
                            set_status(
                                hwnd,
                                "Cloudflare kurulumu bağlandı; istatistikler yükleniyor…",
                            );
                            refresh(hwnd, state);
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
                    set_status(hwnd, "Bağlantı silindi.");
                    refresh(hwnd, state);
                }
                Some(Completion::Pair(Err(error)))
                | Some(Completion::Refresh(Err(error)))
                | Some(Completion::Limits(Err(error)))
                | Some(Completion::Delete(Err(error))) => {
                    set_status(hwnd, "İşlem başarısız. Ayrıntı için uyarıya bakın.");
                    crate::ui::error(hwnd, "Cloudflare", &error);
                }
                None => {
                    set_status(hwnd, "Sunucudan yanıt alınamadı.");
                    crate::diagnostics::record(
                        "cloud settings",
                        "Completion message without response.",
                    );
                }
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
        WM_CLOSE => {
            unsafe {
                let _ = DestroyWindow(hwnd);
            }
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
    static REGISTERED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if REGISTERED.load(std::sync::atomic::Ordering::Acquire) {
        return Ok(());
    }
    let class = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        lpfnWndProc: Some(wnd_proc),
        hInstance: HINSTANCE::default(),
        hCursor: unsafe { LoadCursorW(HINSTANCE::default(), IDC_ARROW).unwrap_or_default() },
        hbrBackground: windows::Win32::Graphics::Gdi::HBRUSH(
            (windows::Win32::Graphics::Gdi::COLOR_WINDOW.0 + 1) as *mut _,
        ),
        lpszClassName: CLASS,
        ..Default::default()
    };
    if unsafe { RegisterClassExW(&class) } == 0
        && unsafe { GetLastError() } != ERROR_CLASS_ALREADY_EXISTS
    {
        return Err(windows::core::Error::from_win32());
    }
    REGISTERED.store(true, std::sync::atomic::Ordering::Release);
    Ok(())
}
fn build_controls(hwnd: HWND, state: &State) -> Result<()> {
    if state.guided {
        label(hwnd, 1100, "Görüntü paylaşımını aç")?;
        label(
            hwnd,
            1101,
            "Cloudflare kendi hesabınızda özel depolama kurar. Alan adı gerekmez; hesap ve R2 onayı sizdedir.",
        )?;
        button(hwnd, ID_DEPLOY, "Kurulumu başlat")?;
        label(hwnd, 1103, "1. UPLOAD_TOKEN alanına yapıştırın")?;
        let upload = edit(hwnd, ID_UPLOAD_TOKEN, true)?;
        unsafe {
            SendMessageW(
                upload,
                windows::Win32::UI::Controls::EM_SETREADONLY,
                WPARAM(1),
                LPARAM(0),
            );
        }
        button(hwnd, ID_COPY_UPLOAD, "Kopyala")?;
        label(hwnd, 1104, "2. ADMIN_TOKEN alanına yapıştırın")?;
        let admin = edit(hwnd, ID_ADMIN_TOKEN, true)?;
        unsafe {
            SendMessageW(
                admin,
                windows::Win32::UI::Controls::EM_SETREADONLY,
                WPARAM(1),
                LPARAM(0),
            );
        }
        button(hwnd, ID_COPY_ADMIN, "Kopyala")?;
        label(
            hwnd,
            1102,
            "3. Cloudflare'ın verdiği Worker adresini yapıştırın",
        )?;
        edit(hwnd, ID_ORIGIN, false)?;
        label(
            hwnd,
            ID_STATUS,
            "Başlamak için düğmeye basın. Görüntünüz kurulum sırasında yüklenmez.",
        )?;
        button(
            hwnd,
            ID_PAIR,
            if state.for_upload {
                "Bağlan ve yükle"
            } else {
                "Bağlan"
            },
        )?;
        button(hwnd, ID_CLOSE, "Vazgeç")?;
    } else {
        label(hwnd, 1100, "Cloudflare ile paylaşım")?;
        button(hwnd, ID_GENERATE, "Anahtar üret")?;
        button(hwnd, ID_DEPLOY, "Cloudflare'da kur")?;
        label(
            hwnd,
            1101,
            "Kendi hesabınızdaki Worker'a kurun; bu uygulama resimleri bizim sunucumuza göndermez.",
        )?;
        label(hwnd, 1102, "Worker adresi")?;
        edit(hwnd, ID_ORIGIN, false)?;
        label(hwnd, 1103, "Yükleme anahtarı")?;
        edit(hwnd, ID_UPLOAD_TOKEN, true)?;
        button(hwnd, ID_COPY_UPLOAD, "Kopyala")?;
        label(hwnd, 1104, "Yönetici anahtarı")?;
        edit(hwnd, ID_ADMIN_TOKEN, true)?;
        button(hwnd, ID_COPY_ADMIN, "Kopyala")?;
        label(hwnd, 1105, "Resim şifresi")?;
        edit(hwnd, ID_PASSWORD, true)?;
        button(hwnd, ID_PAIR, "Eşleştir")?;
        button(hwnd, ID_FORGET, "Bağlantıyı kaldır")?;
        label(
            hwnd,
            ID_STATUS,
            "Yükleme kapalı. Anahtar üretip Cloudflare'a kurun; sonra Worker adresini eşleştirin.",
        )?;
        label(hwnd, 1106, "İstatistikler")?;
        button(hwnd, ID_REFRESH, "Yenile")?;
        label(hwnd, ID_STATS, "Bağlı bir Worker yok.")?;
        label(hwnd, 1107, "Kota ve saklama ayarları")?;
        for (index, (label_text, _, _)) in LIMITS.iter().enumerate() {
            label(hwnd, 1200 + index as i32, label_text)?;
            edit(hwnd, ID_LIMIT_FIRST + index as i32, false)?;
        }
        label(hwnd, 1108, "Sınırda davranış")?;
        let combo = create(
            hwnd,
            w!("COMBOBOX"),
            ID_MODE,
            "",
            WINDOW_STYLE(WS_TABSTOP.0 | CBS_DROPDOWNLIST as u32 | WS_VSCROLL.0),
        )?;
        for item in [
            "Yalnızca uyar",
            "Yeni yüklemeleri durdur",
            "Yükleme ve görüntülemeyi durdur",
        ] {
            let wide = wide(item);
            unsafe {
                SendMessageW(
                    combo,
                    CB_ADDSTRING,
                    WPARAM(0),
                    LPARAM(wide.as_ptr() as isize),
                );
            }
        }
        button(hwnd, ID_SAVE_LIMITS, "Sınırları kaydet")?;
        label(
            hwnd,
            1110,
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
            1109,
            "Sırrınızı paylaşmayın. Ücret uyarıları tahmindir; Cloudflare faturasını garanti etmez.",
        )?;
        button(hwnd, ID_CLOSE, "Kapat")?;
        set_text(
            hwnd,
            ID_ORIGIN,
            state.settings.cloud_url.as_deref().unwrap_or(""),
        );
        if let Some(origin) = state.settings.cloud_url.as_deref()
            && let Ok(credentials) = cloudflare_setup::load_credentials(origin)
        {
            set_text(hwnd, ID_UPLOAD_TOKEN, &credentials.upload_token);
            set_text(hwnd, ID_ADMIN_TOKEN, &credentials.admin_token);
            set_text(
                hwnd,
                ID_PASSWORD,
                credentials.share_password.as_deref().unwrap_or(""),
            );
            set_text(
                hwnd,
                ID_STATUS,
                "Bağlı Worker hazır. İstatistikleri yüklemek için Yenile'ye basın.",
            );
        }
    }
    if !state.font.is_invalid() {
        for id in 1001..=1206 {
            if let Some(child) = control(hwnd, id) {
                unsafe {
                    SendMessageW(child, WM_SETFONT, WPARAM(state.font.0 as usize), LPARAM(1));
                }
            }
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
    show_impl(current, owner, false)
}

pub fn show_for_upload(current: &Settings, owner: HWND) -> Result<Option<Settings>> {
    show_impl(current, owner, true)
}

fn show_impl(current: &Settings, owner: HWND, for_upload: bool) -> Result<Option<Settings>> {
    register()?;
    let guided = current.cloud_url.is_none();
    let pending_tokens = if guided {
        cloudflare_setup::load_pending_tokens()
    } else {
        Ok(None)
    };
    let mut state = Box::new(State {
        settings: current.clone(),
        saved: false,
        guided,
        for_upload,
        setup_started: matches!(&pending_tokens, Ok(Some(_))),
        pending: false,
        loaded: false,
        scroll: 0,
        images: Vec::new(),
        response: Arc::new(Mutex::new(None)),
        font: HFONT::default(),
    });
    let hwnd = unsafe {
        CreateWindowExW(
            Default::default(),
            CLASS,
            w!("isolmaSS Cloudflare"),
            WS_OVERLAPPEDWINDOW
                | WS_CLIPCHILDREN
                | if guided { WINDOW_STYLE(0) } else { WS_VSCROLL },
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            760,
            660,
            owner,
            None,
            HINSTANCE::default(),
            None,
        )
    }?;
    let _window = crate::ui::OwnedWindow(hwnd);
    let _suspend = crate::hotkey::OverlayInputSuspension::new();
    let _owner = OwnerGuard(owner);
    unsafe {
        let _ = EnableWindow(owner, false);
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, state.as_mut() as *mut State as isize);
    }
    let dpi = unsafe { windows::Win32::UI::HiDpi::GetDpiForWindow(hwnd) }.max(96);
    let face = wide(crate::theme::ui_face());
    state.font = unsafe {
        CreateFontW(
            -14 * dpi as i32 / 96,
            0,
            0,
            0,
            400,
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
    };
    build_controls(hwnd, &state)?;
    if guided {
        match pending_tokens {
            Ok(Some(tokens)) => {
                set_text(hwnd, ID_UPLOAD_TOKEN, &tokens.upload_token);
                set_text(hwnd, ID_ADMIN_TOKEN, &tokens.admin_token);
                set_status(
                    hwnd,
                    "Kurulum yaptıysanız tekrar kurmayın: mevcut Worker adresini girin. Kurulum bitmediyse Cloudflare sayfasında iki anahtarı yapıştırın.",
                );
            }
            Ok(None) => {}
            Err(error) => set_status(
                hwnd,
                &format!("Yarım kurulum açılamadı: {error} Kurulumu başlat ile yeniden oluşturun."),
            ),
        }
        show_guided_step(hwnd, &mut state);
    } else {
        layout(hwnd, &mut state);
    }
    unsafe {
        crate::theme::apply_window_theme(
            hwnd,
            crate::theme::theme() == crate::theme::Theme::Dark,
            true,
        );
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = SetForegroundWindow(hwnd);
    }
    crate::ui::window_loop(hwnd, crate::ui::WindowKind::CloudSettings)?;
    if state.saved {
        Ok(Some(state.settings.clone()))
    } else {
        Ok(None)
    }
}
