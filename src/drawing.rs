//! Bounded, UI-thread-local GDI resources shared by previews, output and controls.
use std::cell::RefCell;
use windows::Win32::Foundation::{COLORREF, SIZE};
use windows::Win32::Graphics::Gdi::*;
use windows::core::w;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Key {
    Font(i32, i32),
    Pen(i32, i32, u32),
    Brush(u32),
}
struct Cache(Vec<(Key, HGDIOBJ)>);
impl Drop for Cache {
    fn drop(&mut self) {
        for (_, object) in self.0.drain(..) {
            unsafe {
                let _ = DeleteObject(object);
            }
        }
    }
}
type TextMetrics = Vec<(String, i32, (i32, i32))>;
thread_local! { static METRICS: RefCell<TextMetrics> = const { RefCell::new(Vec::new()) }; }
thread_local! { static CACHE: RefCell<Cache> = const { RefCell::new(Cache(Vec::new())) }; }

fn with_object<T>(
    hdc: HDC,
    key: Key,
    create: impl FnOnce() -> HGDIOBJ,
    action: impl FnOnce() -> T,
) -> T {
    let (object, owned) = CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some((_, object)) = cache.0.iter().find(|(existing, _)| *existing == key) {
            return (*object, false);
        }
        let object = create();
        if cache.0.len() < 48 && !object.is_invalid() {
            cache.0.push((key, object));
            (object, false)
        } else {
            (object, true)
        }
    });
    struct Selection {
        hdc: HDC,
        previous: HGDIOBJ,
        object: HGDIOBJ,
        owned: bool,
    }
    impl Drop for Selection {
        fn drop(&mut self) {
            unsafe {
                SelectObject(self.hdc, self.previous);
                if self.owned {
                    let _ = DeleteObject(self.object);
                }
            }
        }
    }
    let _selection = Selection {
        hdc,
        previous: unsafe { SelectObject(hdc, object) },
        object,
        owned,
    };
    action()
}

pub fn with_font<T>(hdc: HDC, height: i32, weight: i32, action: impl FnOnce() -> T) -> T {
    with_object(
        hdc,
        Key::Font(height, weight),
        || unsafe {
            HGDIOBJ(
                CreateFontW(
                    height,
                    0,
                    0,
                    0,
                    weight,
                    0,
                    0,
                    0,
                    DEFAULT_CHARSET.0 as u32,
                    OUT_DEFAULT_PRECIS.0 as u32,
                    CLIP_DEFAULT_PRECIS.0 as u32,
                    CLEARTYPE_QUALITY.0 as u32,
                    DEFAULT_PITCH.0 as u32,
                    w!("Segoe UI"),
                )
                .0,
            )
        },
        action,
    )
}

pub fn with_pen<T>(
    hdc: HDC,
    style: PEN_STYLE,
    width: i32,
    color: COLORREF,
    action: impl FnOnce() -> T,
) -> T {
    with_object(
        hdc,
        Key::Pen(style.0, width, color.0),
        || unsafe { HGDIOBJ(CreatePen(style, width, color).0) },
        action,
    )
}

pub fn with_brush<T>(hdc: HDC, color: COLORREF, action: impl FnOnce() -> T) -> T {
    with_object(
        hdc,
        Key::Brush(color.0),
        || unsafe { HGDIOBJ(CreateSolidBrush(color).0) },
        action,
    )
}

pub fn measure_text(text: &str, font_size: i32) -> (i32, i32) {
    if let Some(size) = METRICS.with(|cache| {
        cache
            .borrow()
            .iter()
            .find(|(value, size, _)| value == text && *size == font_size)
            .map(|(_, _, result)| *result)
    }) {
        return size;
    }
    let dc = unsafe { CreateCompatibleDC(None) };
    if dc.is_invalid() {
        return (font_size.max(1), font_size.max(1));
    }
    let wide: Vec<u16> = text.encode_utf16().collect();
    let size = with_font(dc, font_size, 700, || {
        let mut size = SIZE::default();
        unsafe {
            let _ = GetTextExtentPoint32W(dc, &wide, &mut size);
        }
        size
    });
    unsafe {
        let _ = DeleteDC(dc);
    }
    let measured = (size.cx.max(1), size.cy.max(font_size.max(1)));
    // Bounded and session-local: never retain large user text or grow with every caret move.
    if text.len() <= 1024 {
        METRICS.with(|cache| {
            let mut cache = cache.borrow_mut();
            if cache.len() == 32 {
                cache.remove(0);
            }
            cache.push((text.to_owned(), font_size, measured));
        });
    }
    measured
}

pub fn text(hdc: HDC, pos: (i32, i32), text: &str, font_size: i32, color: COLORREF) {
    with_font(hdc, font_size, 700, || unsafe {
        let _ = SetTextColor(hdc, color);
        let _ = SetBkMode(hdc, TRANSPARENT);
        let wide: Vec<u16> = text.encode_utf16().collect();
        let _ = TextOutW(hdc, pos.0, pos.1, &wide);
    });
}

pub fn clear_text_metrics() {
    METRICS.with(|cache| cache.borrow_mut().clear());
}

pub fn rounded(
    hdc: HDC,
    rect: crate::capture::Rect,
    radius: i32,
    fill: COLORREF,
    border: COLORREF,
) {
    with_brush(hdc, fill, || {
        with_pen(hdc, PS_SOLID, 1, border, || unsafe {
            let _ = RoundRect(
                hdc,
                rect.left,
                rect.top,
                rect.right,
                rect.bottom,
                radius,
                radius,
            );
        })
    });
}

pub fn label(
    hdc: HDC,
    rect: crate::capture::Rect,
    text: &str,
    size: i32,
    color: COLORREF,
    centered: bool,
) {
    with_font(hdc, -size.max(1), 600, || unsafe {
        let _ = SetTextColor(hdc, color);
        let _ = SetBkMode(hdc, TRANSPARENT);
        let mut wide: Vec<u16> = text.encode_utf16().collect();
        let mut bounds = windows::Win32::Foundation::RECT {
            left: rect.left,
            top: rect.top,
            right: rect.right,
            bottom: rect.bottom,
        };
        let _ = DrawTextW(
            hdc,
            &mut wide,
            &mut bounds,
            DT_SINGLELINE
                | DT_VCENTER
                | DT_NOPREFIX
                | DT_END_ELLIPSIS
                | if centered { DT_CENTER } else { DT_LEFT },
        );
    });
}
