use crate::capture::Rect;
use serde::{Deserialize, Serialize};
use windows::Win32::Foundation::{COLORREF, POINT, RECT};
use windows::Win32::Graphics::Gdi::{
    CLIP_DEFAULT_PRECIS, CreateFontW, CreatePen, CreateSolidBrush, DEFAULT_CHARSET, DEFAULT_PITCH,
    DEFAULT_QUALITY, DT_LEFT, DT_NOCLIP, DT_TOP, DeleteObject, DrawTextW, FF_DONTCARE, FW_BOLD,
    GetStockObject, HBRUSH, HDC, HFONT, HGDIOBJ, HPEN, NULL_BRUSH, PS_DOT, PS_SOLID, Polygon,
    Polyline, Rectangle as GdiRectangle, SelectObject, SetBkMode, SetTextColor, TRANSPARENT,
};
use windows::core::PCWSTR;

/// Available annotation tools.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ToolKind {
    #[default]
    Rectangle,
    Arrow,
    Pen,
    Text,
    Blur,
    Select,
}

/// Converts a 32-bit BGRA color `[B, G, R, A]` to a Win32 `COLORREF` (0x00bbggrr).
#[inline]
pub fn bgra_to_colorref(bgra: [u8; 4]) -> COLORREF {
    COLORREF((bgra[2] as u32) | ((bgra[1] as u32) << 8) | ((bgra[0] as u32) << 16))
}

// RAII cleanup guards for GDI objects
struct PenGuard {
    hdc: HDC,
    old: HGDIOBJ,
    pen: HPEN,
}

impl Drop for PenGuard {
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.hdc, self.old);
            let _ = DeleteObject(HGDIOBJ(self.pen.0));
        }
    }
}

struct BrushGuard {
    hdc: HDC,
    old: HGDIOBJ,
    brush: Option<HBRUSH>,
}

impl Drop for BrushGuard {
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.hdc, self.old);
            if let Some(b) = self.brush {
                let _ = DeleteObject(HGDIOBJ(b.0));
            }
        }
    }
}

struct FontGuard {
    hdc: HDC,
    old: HGDIOBJ,
    font: HFONT,
}

impl Drop for FontGuard {
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.hdc, self.old);
            let _ = DeleteObject(HGDIOBJ(self.font.0));
        }
    }
}

/// The specific data for each annotation kind.
#[derive(Debug, Clone, PartialEq)]
pub enum AnnotationKind {
    Rectangle {
        rect: Rect,
        color: [u8; 4],
        thickness: i32,
    },
    Arrow {
        start: (i32, i32),
        end: (i32, i32),
        color: [u8; 4],
        thickness: i32,
    },
    Pen {
        points: Vec<(i32, i32)>,
        color: [u8; 4],
        thickness: i32,
    },
    Text {
        pos: (i32, i32),
        text: String,
        color: [u8; 4],
        font_size: i32,
    },
    Blur {
        rect: Rect,
        block_size: i32,
    },
}

/// A standalone annotation object with an ID and geometry.
#[derive(Debug, Clone, PartialEq)]
pub struct AnnotationObject {
    pub id: usize,
    pub kind: AnnotationKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnnotationResizeHandle {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
    ArrowStart,
    ArrowEnd,
}

type ResizeHandlePoints = ([(AnnotationResizeHandle, (i32, i32)); 4], usize);

pub fn render_pen_preview(hdc: HDC, points: &[(i32, i32)], color: [u8; 4], thickness: i32) {
    if points.len() < 2 {
        return;
    }
    let pen = unsafe { CreatePen(PS_SOLID, thickness, bgra_to_colorref(color)) };
    let old_pen = unsafe { SelectObject(hdc, HGDIOBJ(pen.0)) };
    let _pen_guard = PenGuard {
        hdc,
        old: old_pen,
        pen,
    };
    let win_points: Vec<POINT> = points
        .iter()
        .map(|point| POINT {
            x: point.0,
            y: point.1,
        })
        .collect();
    unsafe {
        let _ = Polyline(hdc, &win_points);
    }
}

impl AnnotationObject {
    pub fn new(id: usize, kind: AnnotationKind) -> Self {
        Self { id, kind }
    }

    /// Returns the bounding box of the annotation in client coordinates.
    pub fn bounds(&self) -> Rect {
        match &self.kind {
            AnnotationKind::Rectangle {
                rect, thickness, ..
            } => rect.inflate(*thickness, *thickness),
            AnnotationKind::Arrow {
                start,
                end,
                thickness,
                ..
            } => Rect::normalized(*start, *end).inflate(*thickness * 4, *thickness * 4),
            AnnotationKind::Pen {
                points, thickness, ..
            } => {
                if points.is_empty() {
                    return Rect::default();
                }
                let mut left = points[0].0;
                let mut right = points[0].0;
                let mut top = points[0].1;
                let mut bottom = points[0].1;
                for p in points {
                    left = left.min(p.0);
                    right = right.max(p.0);
                    top = top.min(p.1);
                    bottom = bottom.max(p.1);
                }
                Rect::new(left, top, right + 1, bottom + 1).inflate(*thickness, *thickness)
            }
            AnnotationKind::Text {
                pos,
                text,
                font_size,
                ..
            } => {
                let estimated_width = (text.len() as i32 * (*font_size * 55 / 100)).max(*font_size);
                let estimated_height = *font_size + 4;
                Rect::new(
                    pos.0,
                    pos.1,
                    pos.0 + estimated_width,
                    pos.1 + estimated_height,
                )
            }
            AnnotationKind::Blur { rect, .. } => *rect,
        }
    }

    /// Tests if point `pt` hits this annotation object.
    pub fn hit_test(&self, pt: (i32, i32)) -> bool {
        match &self.kind {
            AnnotationKind::Rectangle {
                rect, thickness, ..
            } => {
                let tol = (*thickness).max(6);
                let outer = rect.inflate(tol, tol);
                if !outer.contains(pt.0, pt.1) {
                    return false;
                }
                // Click on border or inside selection
                rect.inflate(tol, tol).contains(pt.0, pt.1)
            }
            AnnotationKind::Arrow {
                start,
                end,
                thickness,
                ..
            } => {
                let tol = (*thickness as f64 + 6.0).max(8.0);
                dist_to_segment(
                    (pt.0 as f64, pt.1 as f64),
                    (start.0 as f64, start.1 as f64),
                    (end.0 as f64, end.1 as f64),
                ) <= tol
            }
            AnnotationKind::Pen {
                points, thickness, ..
            } => {
                let tol = (*thickness as f64 + 6.0).max(8.0);
                if points.len() == 1 {
                    let d = ((pt.0 - points[0].0).pow(2) + (pt.1 - points[0].1).pow(2)) as f64;
                    return d.sqrt() <= tol;
                }
                for i in 0..points.len().saturating_sub(1) {
                    let d = dist_to_segment(
                        (pt.0 as f64, pt.1 as f64),
                        (points[i].0 as f64, points[i].1 as f64),
                        (points[i + 1].0 as f64, points[i + 1].1 as f64),
                    );
                    if d <= tol {
                        return true;
                    }
                }
                false
            }
            AnnotationKind::Text { .. } => self.bounds().inflate(4, 4).contains(pt.0, pt.1),
            AnnotationKind::Blur { rect, .. } => rect.contains(pt.0, pt.1),
        }
    }

    /// Translates this annotation object by `(dx, dy)`.
    pub fn translate(&mut self, dx: i32, dy: i32) {
        match &mut self.kind {
            AnnotationKind::Rectangle { rect, .. } => {
                rect.left += dx;
                rect.right += dx;
                rect.top += dy;
                rect.bottom += dy;
            }
            AnnotationKind::Arrow { start, end, .. } => {
                start.0 += dx;
                start.1 += dy;
                end.0 += dx;
                end.1 += dy;
            }
            AnnotationKind::Pen { points, .. } => {
                for p in points {
                    p.0 += dx;
                    p.1 += dy;
                }
            }
            AnnotationKind::Text { pos, .. } => {
                pos.0 += dx;
                pos.1 += dy;
            }
            AnnotationKind::Blur { rect, .. } => {
                rect.left += dx;
                rect.right += dx;
                rect.top += dy;
                rect.bottom += dy;
            }
        }
    }

    pub fn geometry_bounds(&self) -> Rect {
        match &self.kind {
            AnnotationKind::Rectangle { rect, .. } | AnnotationKind::Blur { rect, .. } => *rect,
            AnnotationKind::Arrow { start, end, .. } => Rect::normalized(*start, *end),
            AnnotationKind::Pen { points, .. } => {
                let Some(first) = points.first() else {
                    return Rect::default();
                };
                let (mut left, mut top) = *first;
                let (mut right, mut bottom) = *first;
                for &(x, y) in &points[1..] {
                    left = left.min(x);
                    right = right.max(x);
                    top = top.min(y);
                    bottom = bottom.max(y);
                }
                Rect::new(left, top, right, bottom)
            }
            AnnotationKind::Text { .. } => self.bounds(),
        }
    }

    fn resize_handle_points(&self) -> ResizeHandlePoints {
        if let AnnotationKind::Arrow { start, end, .. } = &self.kind {
            return (
                [
                    (AnnotationResizeHandle::ArrowStart, *start),
                    (AnnotationResizeHandle::ArrowEnd, *end),
                    (AnnotationResizeHandle::ArrowEnd, *end),
                    (AnnotationResizeHandle::ArrowEnd, *end),
                ],
                2,
            );
        }
        let b = self.geometry_bounds();
        (
            [
                (AnnotationResizeHandle::TopLeft, (b.left, b.top)),
                (AnnotationResizeHandle::TopRight, (b.right, b.top)),
                (AnnotationResizeHandle::BottomLeft, (b.left, b.bottom)),
                (AnnotationResizeHandle::BottomRight, (b.right, b.bottom)),
            ],
            4,
        )
    }

    pub fn hit_resize_handle(&self, pt: (i32, i32), radius: i32) -> Option<AnnotationResizeHandle> {
        let (handles, count) = self.resize_handle_points();
        handles[..count]
            .iter()
            .find(|(_, (x, y))| (pt.0 - *x).abs() <= radius && (pt.1 - *y).abs() <= radius)
            .map(|(handle, _)| *handle)
    }

    fn resized_rect(
        source: Rect,
        handle: AnnotationResizeHandle,
        cursor: (i32, i32),
        limit: Rect,
        minimum: i32,
    ) -> Rect {
        let cursor = (
            cursor.0.clamp(limit.left, limit.right),
            cursor.1.clamp(limit.top, limit.bottom),
        );
        let fixed_left = source.left.clamp(limit.left, limit.right);
        let fixed_top = source.top.clamp(limit.top, limit.bottom);
        let fixed_right = source.right.clamp(limit.left, limit.right);
        let fixed_bottom = source.bottom.clamp(limit.top, limit.bottom);
        let moving_left = cursor
            .0
            .min((fixed_right - minimum).max(limit.left))
            .max(limit.left);
        let moving_top = cursor
            .1
            .min((fixed_bottom - minimum).max(limit.top))
            .max(limit.top);
        let moving_right = cursor
            .0
            .max((fixed_left + minimum).min(limit.right))
            .min(limit.right);
        let moving_bottom = cursor
            .1
            .max((fixed_top + minimum).min(limit.bottom))
            .min(limit.bottom);
        match handle {
            AnnotationResizeHandle::TopLeft => {
                Rect::new(moving_left, moving_top, fixed_right, fixed_bottom)
            }
            AnnotationResizeHandle::TopRight => {
                Rect::new(fixed_left, moving_top, moving_right, fixed_bottom)
            }
            AnnotationResizeHandle::BottomLeft => {
                Rect::new(moving_left, fixed_top, fixed_right, moving_bottom)
            }
            AnnotationResizeHandle::BottomRight => {
                Rect::new(fixed_left, fixed_top, moving_right, moving_bottom)
            }
            AnnotationResizeHandle::ArrowStart | AnnotationResizeHandle::ArrowEnd => source,
        }
    }

    pub fn resize_from(
        &mut self,
        original: &AnnotationKind,
        handle: AnnotationResizeHandle,
        cursor: (i32, i32),
        limit: Rect,
    ) {
        let cursor = (
            cursor.0.clamp(limit.left, limit.right),
            cursor.1.clamp(limit.top, limit.bottom),
        );
        self.kind = match original {
            AnnotationKind::Rectangle {
                rect,
                color,
                thickness,
            } => AnnotationKind::Rectangle {
                rect: Self::resized_rect(*rect, handle, cursor, limit, 6),
                color: *color,
                thickness: *thickness,
            },
            AnnotationKind::Blur { rect, block_size } => AnnotationKind::Blur {
                rect: Self::resized_rect(*rect, handle, cursor, limit, 6),
                block_size: *block_size,
            },
            AnnotationKind::Arrow {
                start,
                end,
                color,
                thickness,
            } => {
                let (candidate_start, candidate_end) = match handle {
                    AnnotationResizeHandle::ArrowStart => (cursor, *end),
                    AnnotationResizeHandle::ArrowEnd => (*start, cursor),
                    _ => (*start, *end),
                };
                let long_enough = (candidate_end.0 - candidate_start.0).abs() >= 4
                    || (candidate_end.1 - candidate_start.1).abs() >= 4;
                let (new_start, new_end) = if long_enough {
                    (candidate_start, candidate_end)
                } else {
                    (*start, *end)
                };
                AnnotationKind::Arrow {
                    start: new_start,
                    end: new_end,
                    color: *color,
                    thickness: *thickness,
                }
            }
            AnnotationKind::Pen {
                points,
                color,
                thickness,
            } => {
                let source = AnnotationObject::new(0, original.clone()).geometry_bounds();
                let target = Self::resized_rect(source, handle, cursor, limit, 2);
                let source_w = (source.right - source.left).max(1) as f64;
                let source_h = (source.bottom - source.top).max(1) as f64;
                let target_w = target.right - target.left;
                let target_h = target.bottom - target.top;
                let scaled = points
                    .iter()
                    .map(|&(x, y)| {
                        let nx = target.left
                            + (((x - source.left) as f64 / source_w) * target_w as f64).round()
                                as i32;
                        let ny = target.top
                            + (((y - source.top) as f64 / source_h) * target_h as f64).round()
                                as i32;
                        (nx, ny)
                    })
                    .collect();
                AnnotationKind::Pen {
                    points: scaled,
                    color: *color,
                    thickness: *thickness,
                }
            }
            AnnotationKind::Text {
                pos: _,
                text,
                color,
                font_size,
            } => {
                let source = AnnotationObject::new(0, original.clone()).geometry_bounds();
                let anchor = match handle {
                    AnnotationResizeHandle::TopLeft => (source.right, source.bottom),
                    AnnotationResizeHandle::TopRight => (source.left, source.bottom),
                    AnnotationResizeHandle::BottomLeft => (source.right, source.top),
                    _ => (source.left, source.top),
                };
                let source_w = source.width().max(1) as f64;
                let source_h = source.height().max(1) as f64;
                let requested_w = (cursor.0 - anchor.0).abs().max(1) as f64;
                let requested_h = (cursor.1 - anchor.1).abs().max(1) as f64;
                let scale = (requested_w / source_w).min(requested_h / source_h);
                let requested_font = (*font_size as f64 * scale).round() as i32;
                let available_width = limit.width().max(1);
                let available_height = limit.height().max(1);
                let chars = text.len().max(1) as i32;
                let width_limited_font = if chars == 1 {
                    available_width
                } else {
                    available_width.saturating_mul(100) / chars.saturating_mul(55)
                };
                let max_font = width_limited_font
                    .min(available_height.saturating_sub(4))
                    .max(1);
                let new_font = requested_font.clamp(8.min(max_font), max_font);
                let applied_scale = new_font as f64 / (*font_size).max(1) as f64;
                let width = (source_w * applied_scale).round() as i32;
                let height = (source_h * applied_scale).round() as i32;
                let new_pos = match handle {
                    AnnotationResizeHandle::TopLeft => (anchor.0 - width, anchor.1 - height),
                    AnnotationResizeHandle::TopRight => (anchor.0, anchor.1 - height),
                    AnnotationResizeHandle::BottomLeft => (anchor.0 - width, anchor.1),
                    _ => anchor,
                };
                AnnotationKind::Text {
                    pos: (
                        new_pos
                            .0
                            .clamp(limit.left, (limit.right - width).max(limit.left)),
                        new_pos
                            .1
                            .clamp(limit.top, (limit.bottom - height).max(limit.top)),
                    ),
                    text: text.clone(),
                    color: *color,
                    font_size: new_font,
                }
            }
        };
    }

    /// Updates the color of this annotation if applicable.
    pub fn set_color(&mut self, new_color: [u8; 4]) {
        match &mut self.kind {
            AnnotationKind::Rectangle { color, .. } => *color = new_color,
            AnnotationKind::Arrow { color, .. } => *color = new_color,
            AnnotationKind::Pen { color, .. } => *color = new_color,
            AnnotationKind::Text { color, .. } => *color = new_color,
            AnnotationKind::Blur { .. } => {}
        }
    }

    /// Updates the stroke thickness of this annotation if applicable.
    pub fn set_thickness(&mut self, new_thickness: i32) {
        match &mut self.kind {
            AnnotationKind::Rectangle { thickness, .. } => *thickness = new_thickness,
            AnnotationKind::Arrow { thickness, .. } => *thickness = new_thickness,
            AnnotationKind::Pen { thickness, .. } => *thickness = new_thickness,
            AnnotationKind::Text { .. } | AnnotationKind::Blur { .. } => {}
        }
    }

    /// Returns the color of this annotation if applicable.
    pub fn get_color(&self) -> Option<[u8; 4]> {
        match &self.kind {
            AnnotationKind::Rectangle { color, .. }
            | AnnotationKind::Arrow { color, .. }
            | AnnotationKind::Pen { color, .. }
            | AnnotationKind::Text { color, .. } => Some(*color),
            AnnotationKind::Blur { .. } => None,
        }
    }

    /// Returns the thickness of this annotation if applicable.
    pub fn get_thickness(&self) -> Option<i32> {
        match &self.kind {
            AnnotationKind::Rectangle { thickness, .. }
            | AnnotationKind::Arrow { thickness, .. }
            | AnnotationKind::Pen { thickness, .. } => Some(*thickness),
            AnnotationKind::Text { .. } | AnnotationKind::Blur { .. } => None,
        }
    }

    /// Renders vector shapes (Rectangle, Arrow, Pen, Text) via native Win32 GDI.
    pub fn render_gdi(&self, hdc: HDC) {
        match &self.kind {
            AnnotationKind::Rectangle {
                rect,
                color,
                thickness,
            } => {
                let pen = unsafe { CreatePen(PS_SOLID, *thickness, bgra_to_colorref(*color)) };
                let old_pen = unsafe { SelectObject(hdc, HGDIOBJ(pen.0)) };
                let null_brush = unsafe { GetStockObject(NULL_BRUSH) };
                let old_brush = unsafe { SelectObject(hdc, null_brush) };

                let _pen_guard = PenGuard {
                    hdc,
                    old: old_pen,
                    pen,
                };
                let _brush_guard = BrushGuard {
                    hdc,
                    old: old_brush,
                    brush: None,
                };

                unsafe {
                    let _ = GdiRectangle(hdc, rect.left, rect.top, rect.right, rect.bottom);
                }
            }
            AnnotationKind::Arrow {
                start,
                end,
                color,
                thickness,
            } => {
                let dx = (end.0 - start.0) as f64;
                let dy = (end.1 - start.1) as f64;
                let len = (dx * dx + dy * dy).sqrt();
                if len < 2.0 {
                    return;
                }

                let colorref = bgra_to_colorref(*color);
                let pen = unsafe { CreatePen(PS_SOLID, *thickness, colorref) };
                let old_pen = unsafe { SelectObject(hdc, HGDIOBJ(pen.0)) };
                let _pen_guard = PenGuard {
                    hdc,
                    old: old_pen,
                    pen,
                };

                let u = (dx / len, dy / len);
                let perp = (-u.1, u.0);
                let head_len = (*thickness as f64 * 4.5).clamp(16.0, 36.0).min(len * 0.5);
                let head_width = head_len * 0.65;

                let base = (end.0 as f64 - u.0 * head_len, end.1 as f64 - u.1 * head_len);
                let p_left = (base.0 + perp.0 * head_width, base.1 + perp.1 * head_width);
                let p_right = (base.0 - perp.0 * head_width, base.1 - perp.1 * head_width);

                // Draw shaft
                let shaft_points = [
                    POINT {
                        x: start.0,
                        y: start.1,
                    },
                    POINT {
                        x: base.0.round() as i32,
                        y: base.1.round() as i32,
                    },
                ];
                unsafe {
                    let _ = Polyline(hdc, &shaft_points);
                }

                // Draw filled arrowhead triangle
                let brush = unsafe { CreateSolidBrush(colorref) };
                let old_brush = unsafe { SelectObject(hdc, HGDIOBJ(brush.0)) };
                let _brush_guard = BrushGuard {
                    hdc,
                    old: old_brush,
                    brush: Some(brush),
                };

                let arrow_points = [
                    POINT { x: end.0, y: end.1 },
                    POINT {
                        x: p_left.0.round() as i32,
                        y: p_left.1.round() as i32,
                    },
                    POINT {
                        x: p_right.0.round() as i32,
                        y: p_right.1.round() as i32,
                    },
                ];
                unsafe {
                    let _ = Polygon(hdc, &arrow_points);
                }
            }
            AnnotationKind::Pen {
                points,
                color,
                thickness,
            } => render_pen_preview(hdc, points, *color, *thickness),
            AnnotationKind::Text {
                pos,
                text,
                color,
                font_size,
            } => {
                if text.is_empty() {
                    return;
                }
                let wide_face: Vec<u16> = "Segoe UI\0".encode_utf16().collect();
                let font = unsafe {
                    CreateFontW(
                        *font_size,
                        0,
                        0,
                        0,
                        FW_BOLD.0 as i32,
                        0,
                        0,
                        0,
                        DEFAULT_CHARSET.0 as u32,
                        CLIP_DEFAULT_PRECIS.0 as u32,
                        CLIP_DEFAULT_PRECIS.0 as u32,
                        DEFAULT_QUALITY.0 as u32,
                        (DEFAULT_PITCH.0 | FF_DONTCARE.0) as u32,
                        PCWSTR(wide_face.as_ptr()),
                    )
                };
                let old_font = unsafe { SelectObject(hdc, HGDIOBJ(font.0)) };
                let _font_guard = FontGuard {
                    hdc,
                    old: old_font,
                    font,
                };

                unsafe {
                    let _ = SetTextColor(hdc, bgra_to_colorref(*color));
                    let _ = SetBkMode(hdc, TRANSPARENT);
                }

                let mut wide_text: Vec<u16> = text.encode_utf16().collect();
                let mut draw_rect = RECT {
                    left: pos.0,
                    top: pos.1,
                    right: pos.0 + 2000,
                    bottom: pos.1 + 1000,
                };
                unsafe {
                    let _ = DrawTextW(
                        hdc,
                        &mut wide_text,
                        &mut draw_rect,
                        DT_LEFT | DT_TOP | DT_NOCLIP,
                    );
                }
            }
            AnnotationKind::Blur { .. } => {
                // Blur modifies the underlying pixel buffer directly via `render_blur`
            }
        }
    }

    /// Renders the blur / pixelate effect directly onto the 32-bit BGRA buffer.
    pub fn render_blur(&self, buffer: &mut [u8], width: i32, height: i32) {
        if let AnnotationKind::Blur { rect, block_size } = &self.kind {
            apply_pixelate_blur(buffer, width, height, rect, *block_size);
        }
    }

    /// Renders selection bounding indicators (dashed rectangle + corner handles) around this object.
    pub fn render_selection_indicator(&self, hdc: HDC) {
        let b = self.bounds().inflate(4, 4);
        let pen = unsafe { CreatePen(PS_DOT, 1, COLORREF(0x00D77800)) }; // Accent blue in BGR
        let old_pen = unsafe { SelectObject(hdc, HGDIOBJ(pen.0)) };
        let null_brush = unsafe { GetStockObject(NULL_BRUSH) };
        let old_brush = unsafe { SelectObject(hdc, null_brush) };

        let _pen_guard = PenGuard {
            hdc,
            old: old_pen,
            pen,
        };
        let _brush_guard = BrushGuard {
            hdc,
            old: old_brush,
            brush: None,
        };

        unsafe {
            let _ = GdiRectangle(hdc, b.left, b.top, b.right, b.bottom);
        }

        // Corner handles
        let handle_brush = unsafe { CreateSolidBrush(COLORREF(0x00D77800)) };
        let old_hbrush = unsafe { SelectObject(hdc, HGDIOBJ(handle_brush.0)) };
        let _handle_guard = BrushGuard {
            hdc,
            old: old_hbrush,
            brush: Some(handle_brush),
        };

        let (handles, count) = self.resize_handle_points();
        for (_, (cx, cy)) in &handles[..count] {
            unsafe {
                let _ = GdiRectangle(hdc, cx - 4, cy - 4, cx + 4, cy + 4);
            }
        }
    }
}

/// Applies a fast mosaic pixelation / box-average blur directly to a 32-bit BGRA buffer.
/// Makes underlying text completely unreadable while keeping smooth performance.
pub fn apply_pixelate_blur(
    buffer: &mut [u8],
    width: i32,
    height: i32,
    rect: &Rect,
    block_size: i32,
) {
    if rect.is_empty() || width <= 0 || height <= 0 {
        return;
    }
    let clamped = rect.clamp(width, height);
    if clamped.is_empty() {
        return;
    }

    let b_size = block_size.max(4);
    let stride = width as usize * 4;

    let mut by = clamped.top;
    while by < clamped.bottom {
        let by_end = (by + b_size).min(clamped.bottom);
        let mut bx = clamped.left;
        while bx < clamped.right {
            let bx_end = (bx + b_size).min(clamped.right);

            // 1. Compute average color for this block
            let mut sum_b = 0u64;
            let mut sum_g = 0u64;
            let mut sum_r = 0u64;
            let mut count = 0u64;

            for y in by..by_end {
                let row_start = y as usize * stride;
                for x in bx..bx_end {
                    let offset = row_start + x as usize * 4;
                    if offset + 4 <= buffer.len() {
                        sum_b += buffer[offset] as u64;
                        sum_g += buffer[offset + 1] as u64;
                        sum_r += buffer[offset + 2] as u64;
                        count += 1;
                    }
                }
            }

            if let Some(non_zero_count) = std::num::NonZeroU64::new(count) {
                let avg_b = (sum_b / non_zero_count) as u8;
                let avg_g = (sum_g / non_zero_count) as u8;
                let avg_r = (sum_r / non_zero_count) as u8;

                // 2. Fill all pixels in block with average color
                for y in by..by_end {
                    let row_start = y as usize * stride;
                    for x in bx..bx_end {
                        let offset = row_start + x as usize * 4;
                        if offset + 4 <= buffer.len() {
                            buffer[offset] = avg_b;
                            buffer[offset + 1] = avg_g;
                            buffer[offset + 2] = avg_r;
                            buffer[offset + 3] = 255;
                        }
                    }
                }
            }

            bx += b_size;
        }
        by += b_size;
    }
}

/// Helper: calculates the shortest distance from point `p` to line segment `(a, b)`.
fn dist_to_segment(p: (f64, f64), a: (f64, f64), b: (f64, f64)) -> f64 {
    let l2 = (b.0 - a.0).powi(2) + (b.1 - a.1).powi(2);
    if l2 == 0.0 {
        return ((p.0 - a.0).powi(2) + (p.1 - a.1).powi(2)).sqrt();
    }
    let t = (((p.0 - a.0) * (b.0 - a.0) + (p.1 - a.1) * (b.1 - a.1)) / l2).clamp(0.0, 1.0);
    let proj = (a.0 + t * (b.0 - a.0), a.1 + t * (b.1 - a.1));
    ((p.0 - proj.0).powi(2) + (p.1 - proj.1).powi(2)).sqrt()
}

/// Snaps a rectangle drag end point to form a 1:1 aspect ratio square.
pub fn snap_square(start: (i32, i32), current: (i32, i32)) -> (i32, i32) {
    let dx = current.0 - start.0;
    let dy = current.1 - start.1;
    let side = dx.abs().max(dy.abs());
    let sign_x = if dx >= 0 { 1 } else { -1 };
    let sign_y = if dy >= 0 { 1 } else { -1 };
    (start.0 + side * sign_x, start.1 + side * sign_y)
}

/// Snaps an arrow end point to the nearest 45-degree angle increment.
pub fn snap_angle_45(start: (i32, i32), current: (i32, i32)) -> (i32, i32) {
    let dx = (current.0 - start.0) as f64;
    let dy = (current.1 - start.1) as f64;
    let len = (dx * dx + dy * dy).sqrt();
    if len < 2.0 {
        return current;
    }
    let angle = dy.atan2(dx);
    let step = std::f64::consts::PI / 4.0; // 45 degrees
    let snapped_angle = (angle / step).round() * step;
    (
        start.0 + (len * snapped_angle.cos()).round() as i32,
        start.1 + (len * snapped_angle.sin()).round() as i32,
    )
}

/// Command for Undo/Redo history.
#[derive(Debug, Clone)]
pub enum EditCommand {
    Add(AnnotationObject),
    Delete(AnnotationObject),
    Modify {
        id: usize,
        old_kind: AnnotationKind,
        new_kind: AnnotationKind,
    },
}

/// Manages the Undo / Redo history stack (capped at 50 commands).
pub struct HistoryManager {
    undo_stack: Vec<EditCommand>,
    redo_stack: Vec<EditCommand>,
    max_entries: usize,
}

impl Default for HistoryManager {
    fn default() -> Self {
        Self::new(50)
    }
}

impl HistoryManager {
    pub fn new(max_entries: usize) -> Self {
        Self {
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            max_entries,
        }
    }

    pub fn record(&mut self, cmd: EditCommand) {
        self.undo_stack.push(cmd);
        if self.undo_stack.len() > self.max_entries {
            self.undo_stack.remove(0);
        }
        self.redo_stack.clear();
    }

    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    /// Reverses the last action, updating `objects`.
    pub fn undo(&mut self, objects: &mut Vec<AnnotationObject>) -> bool {
        let Some(cmd) = self.undo_stack.pop() else {
            return false;
        };

        match cmd {
            EditCommand::Add(obj) => {
                // To undo Add: remove the object and save to redo stack
                if let Some(pos) = objects.iter().position(|o| o.id == obj.id) {
                    let removed = objects.remove(pos);
                    self.redo_stack.push(EditCommand::Add(removed));
                }
            }
            EditCommand::Delete(obj) => {
                // To undo Delete: restore the object and save to redo stack
                let id = obj.id;
                objects.push(obj);
                self.redo_stack.push(EditCommand::Delete(
                    objects.iter().find(|o| o.id == id).unwrap().clone(),
                ));
            }
            EditCommand::Modify {
                id,
                old_kind,
                new_kind,
            } => {
                if let Some(obj) = objects.iter_mut().find(|o| o.id == id) {
                    obj.kind = old_kind.clone();
                    self.redo_stack.push(EditCommand::Modify {
                        id,
                        old_kind,
                        new_kind,
                    });
                }
            }
        }
        true
    }

    /// Re-applies the previously undone action, updating `objects`.
    pub fn redo(&mut self, objects: &mut Vec<AnnotationObject>) -> bool {
        let Some(cmd) = self.redo_stack.pop() else {
            return false;
        };

        match cmd {
            EditCommand::Add(obj) => {
                let id = obj.id;
                objects.push(obj);
                self.undo_stack.push(EditCommand::Add(
                    objects.iter().find(|o| o.id == id).unwrap().clone(),
                ));
            }
            EditCommand::Delete(obj) => {
                if let Some(pos) = objects.iter().position(|o| o.id == obj.id) {
                    let removed = objects.remove(pos);
                    self.undo_stack.push(EditCommand::Delete(removed));
                }
            }
            EditCommand::Modify {
                id,
                old_kind,
                new_kind,
            } => {
                if let Some(obj) = objects.iter_mut().find(|o| o.id == id) {
                    obj.kind = new_kind.clone();
                    self.undo_stack.push(EditCommand::Modify {
                        id,
                        old_kind,
                        new_kind,
                    });
                }
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const COLOR: [u8; 4] = [49, 49, 224, 255];

    fn assert_within(rect: Rect, limit: Rect) {
        assert!(rect.left >= limit.left && rect.top >= limit.top, "{rect:?}");
        assert!(
            rect.right <= limit.right && rect.bottom <= limit.bottom,
            "{rect:?}"
        );
    }

    #[test]
    fn rectangle_and_tiny_edge_blur_resize_stay_bounded() {
        let limit = Rect::new(0, 0, 20, 20);
        let rectangle = AnnotationKind::Rectangle {
            rect: Rect::new(5, 5, 10, 10),
            color: COLOR,
            thickness: 2,
        };
        let mut object = AnnotationObject::new(1, rectangle.clone());
        object.resize_from(
            &rectangle,
            AnnotationResizeHandle::BottomRight,
            (18, 19),
            limit,
        );
        assert_eq!(object.geometry_bounds(), Rect::new(5, 5, 18, 19));

        let blur = AnnotationKind::Blur {
            rect: Rect::new(0, 0, 3, 3),
            block_size: 8,
        };
        let mut object = AnnotationObject::new(2, blur.clone());
        object.resize_from(&blur, AnnotationResizeHandle::TopLeft, (-20, -20), limit);
        assert_eq!(object.geometry_bounds(), Rect::new(0, 0, 3, 3));
        assert_within(object.geometry_bounds(), limit);
    }

    #[test]
    fn arrow_endpoint_resize_clamps_to_selection() {
        let limit = Rect::new(0, 0, 20, 20);
        let arrow = AnnotationKind::Arrow {
            start: (5, 5),
            end: (10, 10),
            color: COLOR,
            thickness: 2,
        };
        let mut object = AnnotationObject::new(1, arrow.clone());
        object.resize_from(&arrow, AnnotationResizeHandle::ArrowEnd, (100, -5), limit);
        let AnnotationKind::Arrow { start, end, .. } = object.kind else {
            unreachable!();
        };
        assert_eq!(start, (5, 5));
        assert_eq!(end, (20, 0));
    }

    #[test]
    fn pen_resize_scales_points_from_fixed_corner() {
        let pen = AnnotationKind::Pen {
            points: vec![(10, 10), (15, 15), (20, 20)],
            color: COLOR,
            thickness: 2,
        };
        let mut object = AnnotationObject::new(1, pen.clone());
        object.resize_from(
            &pen,
            AnnotationResizeHandle::BottomRight,
            (30, 40),
            Rect::new(0, 0, 50, 50),
        );
        let AnnotationKind::Pen { points, .. } = object.kind else {
            unreachable!();
        };
        assert_eq!(points, vec![(10, 10), (20, 25), (30, 40)]);
    }

    #[test]
    fn text_resize_changes_font_proportionally_and_stays_bounded() {
        let limit = Rect::new(0, 0, 60, 40);
        let text = AnnotationKind::Text {
            pos: (10, 10),
            text: "Hi".to_string(),
            color: COLOR,
            font_size: 20,
        };
        let mut object = AnnotationObject::new(1, text.clone());
        object.resize_from(
            &text,
            AnnotationResizeHandle::BottomRight,
            (1000, 1000),
            limit,
        );
        let AnnotationKind::Text { pos, font_size, .. } = &object.kind else {
            unreachable!();
        };
        assert_eq!(*font_size, 25);
        assert_eq!(*pos, (10, 10));
        assert_within(object.geometry_bounds(), limit);
    }

    #[test]
    fn modify_transform_is_one_undoable_and_redoable_command() {
        let old_kind = AnnotationKind::Rectangle {
            rect: Rect::new(2, 2, 12, 12),
            color: COLOR,
            thickness: 2,
        };
        let new_kind = AnnotationKind::Rectangle {
            rect: Rect::new(4, 5, 18, 19),
            color: COLOR,
            thickness: 2,
        };
        let mut objects = vec![AnnotationObject::new(7, new_kind.clone())];
        let mut history = HistoryManager::default();
        history.record(EditCommand::Modify {
            id: 7,
            old_kind: old_kind.clone(),
            new_kind: new_kind.clone(),
        });

        assert!(history.undo(&mut objects));
        assert_eq!(objects[0].kind, old_kind);
        assert!(!history.can_undo());
        assert!(history.can_redo());
        assert!(history.redo(&mut objects));
        assert_eq!(objects[0].kind, new_kind);
        assert!(history.can_undo());
        assert!(!history.can_redo());
    }
}
