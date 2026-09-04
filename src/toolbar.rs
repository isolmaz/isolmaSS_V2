use crate::annotation::{bgra_to_colorref, ToolKind};
use crate::capture::Rect;
use crate::settings::{PRESET_COLORS, PRESET_THICKNESSES};
use windows::core::PCWSTR;
use windows::Win32::Foundation::{COLORREF, POINT, RECT};
use windows::Win32::Graphics::Gdi::{
    CreateFontW, CreatePen, CreateSolidBrush, DeleteObject, DrawTextW, Polyline, RoundRect,
    SelectObject, SetBkMode, SetTextColor, CLIP_DEFAULT_PRECIS, DEFAULT_CHARSET, DEFAULT_PITCH,
    DEFAULT_QUALITY, DT_CENTER, DT_SINGLELINE, DT_VCENTER, FF_DONTCARE, FW_BOLD, HBRUSH, HDC,
    HFONT, HGDIOBJ, HPEN, PS_SOLID, TRANSPARENT,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolbarAction {
    Undo,
    Redo,
    Save,
    Copy,
    Settings,
    Cancel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolbarItem {
    Tool(ToolKind),
    Action(ToolbarAction),
    Color([u8; 4]),
    Thickness(i32),
}

#[derive(Debug, Clone)]
pub struct ToolbarButton {
    pub item: ToolbarItem,
    pub label: String,
    pub rect: Rect,
    pub is_enabled: bool,
}

pub struct Toolbar {
    pub bounds: Rect,
    pub buttons: Vec<ToolbarButton>,
    pub hovered_item: Option<ToolbarItem>,
    pub active_tool: ToolKind,
    pub active_color: [u8; 4],
    pub active_thickness: i32,
}

struct GdiGuard {
    hdc: HDC,
    old_pen: HGDIOBJ,
    pen: HPEN,
    old_brush: HGDIOBJ,
    brush: HBRUSH,
}

impl Drop for GdiGuard {
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.hdc, self.old_pen);
            let _ = DeleteObject(HGDIOBJ(self.pen.0));
            SelectObject(self.hdc, self.old_brush);
            let _ = DeleteObject(HGDIOBJ(self.brush.0));
        }
    }
}

struct FontGuard {
    hdc: HDC,
    old_font: HGDIOBJ,
    font: HFONT,
}

impl Drop for FontGuard {
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.hdc, self.old_font);
            let _ = DeleteObject(HGDIOBJ(self.font.0));
        }
    }
}

impl Toolbar {
    pub const HEIGHT: i32 = 68; // Two-row toolbar
    pub const ROW1_HEIGHT: i32 = 28;
    pub const ROW2_HEIGHT: i32 = 22;

    /// Computes the two-row toolbar layout anchored to `selection`,
    /// ensuring it remains fully within screen boundaries.
    #[allow(clippy::too_many_arguments)]
    pub fn layout(
        selection: &Rect,
        active_tool: ToolKind,
        active_color: [u8; 4],
        active_thickness: i32,
        screen_width: i32,
        screen_height: i32,
        can_undo: bool,
        can_redo: bool,
    ) -> Self {
        // Row 1: Tools + History + Actions
        let mut row1_items: Vec<(ToolbarItem, String, bool, i32)> = Vec::new();
        for tool in ToolKind::ALL {
            let w = match tool {
                ToolKind::Rectangle => 54,
                ToolKind::Arrow => 58,
                ToolKind::Pen => 48,
                ToolKind::Text => 50,
                ToolKind::Blur => 50,
            };
            row1_items.push((ToolbarItem::Tool(tool), tool.name().to_string(), true, w));
        }
        row1_items.push((ToolbarItem::Action(ToolbarAction::Undo), "Undo".to_string(), can_undo, 44));
        row1_items.push((ToolbarItem::Action(ToolbarAction::Redo), "Redo".to_string(), can_redo, 44));
        row1_items.push((ToolbarItem::Action(ToolbarAction::Save), "Save [^S]".to_string(), true, 64));
        row1_items.push((ToolbarItem::Action(ToolbarAction::Copy), "Copy [^C]".to_string(), true, 64));
        row1_items.push((ToolbarItem::Action(ToolbarAction::Settings), "Settings".to_string(), true, 58));
        row1_items.push((ToolbarItem::Action(ToolbarAction::Cancel), "Close [Esc]".to_string(), true, 68));

        let pad = 6;
        let sep_width = 8;
        let mut total_width = pad * 2;
        for (i, (_, _, _, w)) in row1_items.iter().enumerate() {
            total_width += *w;
            if i == 4 || i == 6 || i == 8 {
                total_width += sep_width;
            } else if i + 1 < row1_items.len() {
                total_width += 3;
            }
        }

        // Vertical positioning: default below selection
        let mut y = selection.bottom + 8;
        if y + Self::HEIGHT > screen_height - 6 {
            // Try above selection
            y = selection.top - Self::HEIGHT - 8;
            if y < 6 {
                // Place inside selection
                y = (selection.bottom - Self::HEIGHT - 8).max(selection.top + 6);
            }
        }

        // Horizontal positioning: align right edge of toolbar with right edge of selection
        let mut x = selection.right - total_width;
        if x + total_width > screen_width - 8 {
            x = screen_width - total_width - 8;
        }
        if x < 8 {
            x = 8;
        }

        let bounds = Rect::new(x, y, x + total_width, y + Self::HEIGHT);

        let mut buttons = Vec::new();

        // 1. Layout Row 1
        let row1_y = y + 5;
        let mut cur_x = x + pad;

        for (i, (item, label, enabled, btn_w)) in row1_items.into_iter().enumerate() {
            let btn_rect = Rect::new(cur_x, row1_y, cur_x + btn_w, row1_y + Self::ROW1_HEIGHT);
            buttons.push(ToolbarButton {
                item,
                label,
                rect: btn_rect,
                is_enabled: enabled,
            });

            cur_x += btn_w;
            if i == 4 || i == 6 || i == 8 {
                cur_x += sep_width;
            } else {
                cur_x += 3;
            }
        }

        // 2. Layout Row 2 (Palette & Thickness)
        let row2_y = y + Self::ROW1_HEIGHT + 9;
        let mut r2_x = x + pad + 2;

        // Color Swatches
        for col in PRESET_COLORS {
            let swatch_rect = Rect::new(r2_x, row2_y, r2_x + 22, row2_y + Self::ROW2_HEIGHT);
            buttons.push(ToolbarButton {
                item: ToolbarItem::Color(col),
                label: String::new(),
                rect: swatch_rect,
                is_enabled: true,
            });
            r2_x += 26;
        }

        r2_x += 12; // Separator between colors and thickness

        // Thickness Presets (2px, 4px, 8px)
        for thick in PRESET_THICKNESSES {
            let thick_rect = Rect::new(r2_x, row2_y, r2_x + 36, row2_y + Self::ROW2_HEIGHT);
            buttons.push(ToolbarButton {
                item: ToolbarItem::Thickness(thick),
                label: format!("{}px", thick),
                rect: thick_rect,
                is_enabled: true,
            });
            r2_x += 40;
        }

        Self {
            bounds,
            buttons,
            hovered_item: None,
            active_tool,
            active_color,
            active_thickness,
        }
    }

    /// Checks if a point is within the toolbar bounds.
    pub fn contains_point(&self, pt: (i32, i32)) -> bool {
        self.bounds.contains(pt.0, pt.1)
    }

    /// Hit-tests for a button under cursor.
    pub fn hit_test(&self, pt: (i32, i32)) -> Option<ToolbarItem> {
        if !self.contains_point(pt) {
            return None;
        }
        for btn in &self.buttons {
            if btn.rect.contains(pt.0, pt.1) && btn.is_enabled {
                return Some(btn.item);
            }
        }
        None
    }

    /// Updates hover state based on mouse coordinates.
    pub fn update_hover(&mut self, pt: (i32, i32)) -> bool {
        let new_hover = self.hit_test(pt);
        if self.hovered_item != new_hover {
            self.hovered_item = new_hover;
            true
        } else {
            false
        }
    }

    /// Renders the two-row toolbar onto the HDC.
    pub fn render(&self, hdc: HDC) {
        // 1. Draw outer toolbar container (dark rounded rectangle)
        let bg_color = COLORREF(0x00242220); // Dark charcoal (BGR)
        let border_color = COLORREF(0x0044403C); // Medium dark border (BGR)

        let pen = unsafe { CreatePen(PS_SOLID, 1, border_color) };
        let brush = unsafe { CreateSolidBrush(bg_color) };
        let old_pen = unsafe { SelectObject(hdc, HGDIOBJ(pen.0)) };
        let old_brush = unsafe { SelectObject(hdc, HGDIOBJ(brush.0)) };

        let _guard = GdiGuard {
            hdc,
            old_pen,
            pen,
            old_brush,
            brush,
        };

        unsafe {
            let _ = RoundRect(
                hdc,
                self.bounds.left,
                self.bounds.top,
                self.bounds.right,
                self.bounds.bottom,
                8,
                8,
            );
        }

        // 2. Setup Font for text buttons
        let wide_face: Vec<u16> = "Segoe UI\0".encode_utf16().collect();
        let font = unsafe {
            CreateFontW(
                12,
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
        let _font_guard = FontGuard { hdc, old_font, font };

        unsafe {
            let _ = SetBkMode(hdc, TRANSPARENT);
        }

        let accent_color = COLORREF(0x00D77800); // Windows blue accent (BGR)
        let hover_color = COLORREF(0x003A3632); // Hover dark highlight (BGR)
        let text_normal = COLORREF(0x00E0DCD8); // Soft white text
        let text_disabled = COLORREF(0x0066605A); // Dimmed disabled text
        let text_white = COLORREF(0x00FFFFFF); // Pure white text

        // 3. Render Buttons
        for btn in &self.buttons {
            match btn.item {
                ToolbarItem::Tool(k) => {
                    let is_active = k == self.active_tool;
                    let is_hovered = self.hovered_item == Some(btn.item);
                    let fill_col = if is_active {
                        Some(accent_color)
                    } else if is_hovered {
                        Some(hover_color)
                    } else {
                        None
                    };

                    if let Some(col) = fill_col {
                        let b_brush = unsafe { CreateSolidBrush(col) };
                        let b_pen = unsafe { CreatePen(PS_SOLID, 1, col) };
                        let op = unsafe { SelectObject(hdc, HGDIOBJ(b_pen.0)) };
                        let ob = unsafe { SelectObject(hdc, HGDIOBJ(b_brush.0)) };
                        unsafe {
                            let _ = RoundRect(hdc, btn.rect.left, btn.rect.top, btn.rect.right, btn.rect.bottom, 4, 4);
                            SelectObject(hdc, op);
                            let _ = DeleteObject(HGDIOBJ(b_pen.0));
                            SelectObject(hdc, ob);
                            let _ = DeleteObject(HGDIOBJ(b_brush.0));
                        }
                    }

                    unsafe {
                        let _ = SetTextColor(hdc, if is_active { text_white } else { text_normal });
                        let mut wide_label: Vec<u16> = btn.label.encode_utf16().collect();
                        let mut draw_rc = RECT {
                            left: btn.rect.left,
                            top: btn.rect.top,
                            right: btn.rect.right,
                            bottom: btn.rect.bottom,
                        };
                        let _ = DrawTextW(hdc, &mut wide_label, &mut draw_rc, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
                    }
                }
                ToolbarItem::Action(_) => {
                    let is_hovered = self.hovered_item == Some(btn.item);
                    if is_hovered && btn.is_enabled {
                        let b_brush = unsafe { CreateSolidBrush(hover_color) };
                        let b_pen = unsafe { CreatePen(PS_SOLID, 1, border_color) };
                        let op = unsafe { SelectObject(hdc, HGDIOBJ(b_pen.0)) };
                        let ob = unsafe { SelectObject(hdc, HGDIOBJ(b_brush.0)) };
                        unsafe {
                            let _ = RoundRect(hdc, btn.rect.left, btn.rect.top, btn.rect.right, btn.rect.bottom, 4, 4);
                            SelectObject(hdc, op);
                            let _ = DeleteObject(HGDIOBJ(b_pen.0));
                            SelectObject(hdc, ob);
                            let _ = DeleteObject(HGDIOBJ(b_brush.0));
                        }
                    }

                    unsafe {
                        let txt_color = if !btn.is_enabled { text_disabled } else { text_normal };
                        let _ = SetTextColor(hdc, txt_color);
                        let mut wide_label: Vec<u16> = btn.label.encode_utf16().collect();
                        let mut draw_rc = RECT {
                            left: btn.rect.left,
                            top: btn.rect.top,
                            right: btn.rect.right,
                            bottom: btn.rect.bottom,
                        };
                        let _ = DrawTextW(hdc, &mut wide_label, &mut draw_rc, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
                    }
                }
                ToolbarItem::Color(col) => {
                    let is_active = col == self.active_color;
                    let is_hovered = self.hovered_item == Some(btn.item);

                    let col_ref = bgra_to_colorref(col);
                    let brush = unsafe { CreateSolidBrush(col_ref) };
                    let border = if is_active {
                        COLORREF(0x00FFFFFF)
                    } else if is_hovered {
                        accent_color
                    } else {
                        border_color
                    };
                    let pen = unsafe { CreatePen(PS_SOLID, if is_active { 2 } else { 1 }, border) };

                    let op = unsafe { SelectObject(hdc, HGDIOBJ(pen.0)) };
                    let ob = unsafe { SelectObject(hdc, HGDIOBJ(brush.0)) };
                    unsafe {
                        let _ = RoundRect(hdc, btn.rect.left, btn.rect.top, btn.rect.right, btn.rect.bottom, 4, 4);
                        SelectObject(hdc, op);
                        let _ = DeleteObject(HGDIOBJ(pen.0));
                        SelectObject(hdc, ob);
                        let _ = DeleteObject(HGDIOBJ(brush.0));
                    }
                }
                ToolbarItem::Thickness(thick) => {
                    let is_active = thick == self.active_thickness;
                    let is_hovered = self.hovered_item == Some(btn.item);

                    let bg = if is_active {
                        accent_color
                    } else if is_hovered {
                        hover_color
                    } else {
                        COLORREF(0x002E2B27)
                    };

                    let brush = unsafe { CreateSolidBrush(bg) };
                    let pen = unsafe { CreatePen(PS_SOLID, 1, border_color) };
                    let op = unsafe { SelectObject(hdc, HGDIOBJ(pen.0)) };
                    let ob = unsafe { SelectObject(hdc, HGDIOBJ(brush.0)) };
                    unsafe {
                        let _ = RoundRect(hdc, btn.rect.left, btn.rect.top, btn.rect.right, btn.rect.bottom, 4, 4);
                        SelectObject(hdc, op);
                        let _ = DeleteObject(HGDIOBJ(pen.0));
                        SelectObject(hdc, ob);
                        let _ = DeleteObject(HGDIOBJ(brush.0));

                        let txt_col = if is_active { text_white } else { text_normal };
                        let _ = SetTextColor(hdc, txt_col);
                        let mut wide_label: Vec<u16> = btn.label.encode_utf16().collect();
                        let mut draw_rc = RECT {
                            left: btn.rect.left,
                            top: btn.rect.top,
                            right: btn.rect.right,
                            bottom: btn.rect.bottom,
                        };
                        let _ = DrawTextW(hdc, &mut wide_label, &mut draw_rc, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
                    }
                }
            }
        }

        // 4. Draw vertical separators in Row 1
        let sep_pen = unsafe { CreatePen(PS_SOLID, 1, border_color) };
        let old_sep_pen = unsafe { SelectObject(hdc, HGDIOBJ(sep_pen.0)) };

        let sep_indices = [4, 6, 8];
        for idx in sep_indices {
            if idx < self.buttons.len() && idx + 1 < self.buttons.len() {
                let sep_x = (self.buttons[idx].rect.right + self.buttons[idx + 1].rect.left) / 2;
                let pts = [
                    POINT {
                        x: sep_x,
                        y: self.bounds.top + 7,
                    },
                    POINT {
                        x: sep_x,
                        y: self.bounds.top + Self::ROW1_HEIGHT + 3,
                    },
                ];
                unsafe {
                    let _ = Polyline(hdc, &pts);
                }
            }
        }

        // Separator between Palette and Thickness in Row 2
        // Colors end at button 11 + 8 = 19
        if self.buttons.len() >= 20 {
            let color_end_x = self.buttons[11 + 7].rect.right + 6;
            let row2_top = self.bounds.top + Self::ROW1_HEIGHT + 10;
            let row2_bot = self.bounds.bottom - 6;
            let pts = [
                POINT { x: color_end_x, y: row2_top },
                POINT { x: color_end_x, y: row2_bot },
            ];
            unsafe {
                let _ = Polyline(hdc, &pts);
            }
        }

        unsafe {
            SelectObject(hdc, old_sep_pen);
            let _ = DeleteObject(HGDIOBJ(sep_pen.0));
        }
    }
}
