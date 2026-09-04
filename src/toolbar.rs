use crate::annotation::{ToolKind, bgra_to_colorref};
use crate::capture::Rect;
use crate::settings::{PRESET_COLORS, PRESET_THICKNESSES};
use windows::Win32::Foundation::{COLORREF, POINT, RECT};
use windows::Win32::Graphics::Gdi::{
    CLIP_DEFAULT_PRECIS, CreateFontW, CreatePen, CreateSolidBrush, DEFAULT_CHARSET, DEFAULT_PITCH,
    DEFAULT_QUALITY, DT_CENTER, DT_SINGLELINE, DT_VCENTER, DeleteObject, DrawTextW, FF_DONTCARE,
    FW_BOLD, HDC, HFONT, HGDIOBJ, PS_SOLID, Polyline, RoundRect, SelectObject, SetBkMode,
    SetTextColor, TRANSPARENT,
};
use windows::core::PCWSTR;

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
    pub rect: Rect,
    pub is_enabled: bool,
}

pub struct Toolbar {
    pub tool_bounds: Rect,
    pub action_bounds: Rect,
    pub buttons: Vec<ToolbarButton>,
    pub hovered_item: Option<ToolbarItem>,
    pub active_tool: ToolKind,
    pub active_color: [u8; 4],
    pub active_thickness: i32,
    pub dpi: u32,
    screen_width: i32,
    screen_height: i32,
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
    pub const TOOL_BUTTON: i32 = 34;
    pub const ACTION_HEIGHT: i32 = 42;

    /// Builds a Lightshot-style tool rail beside the selection and a compact
    /// action strip below it. Each panel flips to the opposite edge as needed.
    #[allow(clippy::too_many_arguments)]
    pub fn layout(
        selection: &Rect,
        active_tool: ToolKind,
        active_color: [u8; 4],
        active_thickness: i32,
        show_color: bool,
        show_thickness: bool,
        screen_width: i32,
        screen_height: i32,
        can_undo: bool,
        can_redo: bool,
        dpi: u32,
    ) -> Self {
        let scale = |value: i32| value * dpi as i32 / 96;
        let margin = scale(6);
        let pad = scale(6);
        let tool_button = scale(Self::TOOL_BUTTON);
        // First item is physically closest to the selection's bottom edge.
        let tool_items = [
            (ToolbarItem::Tool(ToolKind::Select), true),
            (ToolbarItem::Tool(ToolKind::Rectangle), true),
            (ToolbarItem::Tool(ToolKind::Arrow), true),
            (ToolbarItem::Tool(ToolKind::Pen), true),
            (ToolbarItem::Tool(ToolKind::Text), true),
            (ToolbarItem::Tool(ToolKind::Blur), true),
            (ToolbarItem::Action(ToolbarAction::Undo), can_undo),
            (ToolbarItem::Action(ToolbarAction::Redo), can_redo),
        ];
        let tool_width = tool_button + pad * 2;
        let tool_height = pad * 2
            + tool_items.len() as i32 * tool_button
            + (tool_items.len() as i32 - 1) * scale(3);

        let color_size = scale(20);
        let color_step = scale(23);
        let thickness_width = scale(28);
        let thickness_step = scale(31);
        let action_button = scale(34);
        let action_step = scale(37);
        let section_gap = scale(12);
        let colors_width = if show_color {
            PRESET_COLORS.len() as i32 * color_step - (color_step - color_size)
        } else {
            0
        };
        let thicknesses_width = if show_thickness {
            PRESET_THICKNESSES.len() as i32 * thickness_step - (thickness_step - thickness_width)
        } else {
            0
        };
        let style_sections = i32::from(show_color) + i32::from(show_thickness);
        let action_width = pad * 2
            + colors_width
            + thicknesses_width
            + style_sections * section_gap
            + 4 * action_step
            - (action_step - action_button);
        let action_height = scale(Self::ACTION_HEIGHT);

        // Place both panels from one shared corner. Clamping that corner, rather than
        // each panel independently, keeps the toolbar a coherent L even when neither
        // selection-anchored orientation fits at a screen edge.
        let panel_width = tool_width.max(action_width);
        let right_join = selection.right + tool_width;
        let left_join = selection.left - tool_width;
        let right_min = margin + panel_width;
        let right_max = screen_width - margin;
        let left_min = margin;
        let left_max = screen_width - margin - panel_width;
        let right_fits = (right_min..=right_max).contains(&right_join);
        let left_fits = (left_min..=left_max).contains(&left_join);
        let place_right = right_fits || !left_fits;

        let below_join = selection.bottom;
        let above_join = selection.top;
        let below_min = margin + tool_height;
        let below_max = screen_height - margin - action_height;
        let above_min = margin + action_height;
        let above_max = screen_height - margin - tool_height;
        let below_fits = (below_min..=below_max).contains(&below_join);
        let above_fits = (above_min..=above_max).contains(&above_join);
        let place_below = below_fits || !above_fits;

        let clamp_join = |value: i32, min: i32, max: i32| {
            if min <= max {
                value.clamp(min, max)
            } else {
                min
            }
        };
        let shared_x = if place_right {
            clamp_join(right_join, right_min, right_max)
        } else {
            clamp_join(left_join, left_min, left_max)
        };
        let shared_y = if place_below {
            clamp_join(below_join, below_min, below_max)
        } else {
            clamp_join(above_join, above_min, above_max)
        };

        let tool_x = if place_right {
            shared_x - tool_width
        } else {
            shared_x
        };
        let action_x = if place_right {
            shared_x - action_width
        } else {
            shared_x
        };
        let (tool_y, action_y) = if place_below {
            (shared_y - tool_height, shared_y)
        } else {
            (shared_y, shared_y - action_height)
        };
        let tool_bounds = Rect::new(tool_x, tool_y, tool_x + tool_width, tool_y + tool_height);
        let action_bounds = Rect::new(
            action_x,
            action_y,
            action_x + action_width,
            action_y + action_height,
        );

        let mut buttons = Vec::with_capacity(
            tool_items.len()
                + usize::from(show_color) * PRESET_COLORS.len()
                + usize::from(show_thickness) * PRESET_THICKNESSES.len()
                + 4,
        );
        let mut y = tool_bounds.bottom - pad - tool_button;
        for (item, enabled) in tool_items {
            buttons.push(ToolbarButton {
                item,
                rect: Rect::new(tool_x + pad, y, tool_x + pad + tool_button, y + tool_button),
                is_enabled: enabled,
            });
            y -= tool_button + scale(3);
        }

        let item_y = action_y + (action_height - color_size) / 2;
        let mut x = action_x + pad;
        if show_color {
            for color in PRESET_COLORS {
                buttons.push(ToolbarButton {
                    item: ToolbarItem::Color(color),
                    rect: Rect::new(x, item_y, x + color_size, item_y + color_size),
                    is_enabled: true,
                });
                x += color_step;
            }
            x += section_gap - (color_step - color_size);
        }
        if show_thickness {
            for thickness in PRESET_THICKNESSES {
                let y = action_y + (action_height - tool_button) / 2;
                buttons.push(ToolbarButton {
                    item: ToolbarItem::Thickness(thickness),
                    rect: Rect::new(x, y, x + thickness_width, y + tool_button),
                    is_enabled: true,
                });
                x += thickness_step;
            }
            x += section_gap - (thickness_step - thickness_width);
        }
        for action in [
            ToolbarAction::Save,
            ToolbarAction::Copy,
            ToolbarAction::Settings,
            ToolbarAction::Cancel,
        ] {
            let y = action_y + (action_height - action_button) / 2;
            buttons.push(ToolbarButton {
                item: ToolbarItem::Action(action),
                rect: Rect::new(x, y, x + action_button, y + action_button),
                is_enabled: true,
            });
            x += action_step;
        }

        Self {
            tool_bounds,
            action_bounds,
            buttons,
            hovered_item: None,
            active_tool,
            active_color,
            active_thickness,
            dpi,
            screen_width,
            screen_height,
        }
    }

    /// Checks if a point is within the toolbar bounds.
    pub fn contains_point(&self, pt: (i32, i32)) -> bool {
        self.tool_bounds.contains(pt.0, pt.1) || self.action_bounds.contains(pt.0, pt.1)
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
        let new_hover = self
            .buttons
            .iter()
            .find(|button| button.rect.contains(pt.0, pt.1))
            .map(|button| button.item);
        if self.hovered_item != new_hover {
            self.hovered_item = new_hover;
            true
        } else {
            false
        }
    }

    /// Renders the bottom-up tool rail and contextual strip as one L onto the HDC.
    pub fn render(&self, hdc: HDC) {
        let scale = |value: i32| value * self.dpi as i32 / 96;
        let panel_bg = COLORREF(0x00282421);
        let panel_border = COLORREF(0x00534B43);
        let shadow = COLORREF(0x00151312);
        let button_bg = COLORREF(0x00322D29);
        let hover_bg = COLORREF(0x00483F38);
        let accent = COLORREF(0x00D77800);
        let text = COLORREF(0x00F2F0ED);
        let disabled = COLORREF(0x00766E67);

        let draw_rounded = |rect: &Rect, fill: COLORREF, border: COLORREF, radius: i32| {
            let brush = unsafe { CreateSolidBrush(fill) };
            let pen = unsafe { CreatePen(PS_SOLID, 1, border) };
            let old_pen = unsafe { SelectObject(hdc, HGDIOBJ(pen.0)) };
            let old_brush = unsafe { SelectObject(hdc, HGDIOBJ(brush.0)) };
            unsafe {
                let _ = RoundRect(
                    hdc,
                    rect.left,
                    rect.top,
                    rect.right,
                    rect.bottom,
                    radius,
                    radius,
                );
                SelectObject(hdc, old_pen);
                SelectObject(hdc, old_brush);
                let _ = DeleteObject(HGDIOBJ(pen.0));
                let _ = DeleteObject(HGDIOBJ(brush.0));
            }
        };

        for panel in [self.tool_bounds, self.action_bounds] {
            draw_rounded(
                &Rect::new(
                    panel.left + scale(2),
                    panel.top + scale(3),
                    panel.right + scale(2),
                    panel.bottom + scale(3),
                ),
                shadow,
                shadow,
                scale(8),
            );
            draw_rounded(&panel, panel_bg, panel_border, scale(8));
        }

        let face: Vec<u16> = "Segoe UI Symbol\0".encode_utf16().collect();
        let font = unsafe {
            CreateFontW(
                -scale(16),
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
                PCWSTR(face.as_ptr()),
            )
        };
        let old_font = unsafe { SelectObject(hdc, HGDIOBJ(font.0)) };
        let _font_guard = FontGuard {
            hdc,
            old_font,
            font,
        };
        unsafe {
            let _ = SetBkMode(hdc, TRANSPARENT);
        }

        for button in &self.buttons {
            let hovered = self.hovered_item == Some(button.item);
            let active = matches!(button.item, ToolbarItem::Tool(tool) if tool == self.active_tool);
            match button.item {
                ToolbarItem::Color(color) => {
                    let ring = if color == self.active_color {
                        text
                    } else if hovered {
                        accent
                    } else {
                        panel_border
                    };
                    draw_rounded(&button.rect, bgra_to_colorref(color), ring, scale(6));
                    if color == self.active_color {
                        let marker = Rect::new(
                            button.rect.left + scale(6),
                            button.rect.bottom - scale(4),
                            button.rect.right - scale(6),
                            button.rect.bottom - scale(2),
                        );
                        draw_rounded(&marker, text, text, scale(2));
                    }
                }
                ToolbarItem::Thickness(thickness) => {
                    let fill = if thickness == self.active_thickness {
                        accent
                    } else if hovered {
                        hover_bg
                    } else {
                        button_bg
                    };
                    draw_rounded(
                        &button.rect,
                        fill,
                        if thickness == self.active_thickness {
                            text
                        } else {
                            panel_border
                        },
                        scale(5),
                    );
                    let pen = unsafe {
                        CreatePen(PS_SOLID, thickness.max(1) * self.dpi as i32 / 96, text)
                    };
                    let old_pen = unsafe { SelectObject(hdc, HGDIOBJ(pen.0)) };
                    let y = (button.rect.top + button.rect.bottom) / 2;
                    let points = [
                        POINT {
                            x: button.rect.left + scale(6),
                            y,
                        },
                        POINT {
                            x: button.rect.right - scale(6),
                            y,
                        },
                    ];
                    unsafe {
                        let _ = Polyline(hdc, &points);
                        SelectObject(hdc, old_pen);
                        let _ = DeleteObject(HGDIOBJ(pen.0));
                    }
                }
                item => {
                    let fill = if active {
                        accent
                    } else if hovered && button.is_enabled {
                        hover_bg
                    } else {
                        button_bg
                    };
                    let border = if active {
                        text
                    } else if hovered {
                        accent
                    } else {
                        panel_border
                    };
                    draw_rounded(&button.rect, fill, border, scale(5));
                    let glyph = match item {
                        ToolbarItem::Tool(ToolKind::Select) => "↖",
                        ToolbarItem::Tool(ToolKind::Rectangle) => "□",
                        ToolbarItem::Tool(ToolKind::Arrow) => "➜",
                        ToolbarItem::Tool(ToolKind::Pen) => "✎",
                        ToolbarItem::Tool(ToolKind::Text) => "T",
                        ToolbarItem::Tool(ToolKind::Blur) => "▦",
                        ToolbarItem::Action(ToolbarAction::Undo) => "↶",
                        ToolbarItem::Action(ToolbarAction::Redo) => "↷",
                        ToolbarItem::Action(ToolbarAction::Save) => "▾",
                        ToolbarItem::Action(ToolbarAction::Copy) => "▣",
                        ToolbarItem::Action(ToolbarAction::Settings) => "⚙",
                        ToolbarItem::Action(ToolbarAction::Cancel) => "×",
                        _ => "",
                    };
                    unsafe {
                        let _ = SetTextColor(hdc, if button.is_enabled { text } else { disabled });
                        let mut chars: Vec<u16> = glyph.encode_utf16().collect();
                        let mut rect = RECT {
                            left: button.rect.left,
                            top: button.rect.top,
                            right: button.rect.right,
                            bottom: button.rect.bottom,
                        };
                        let _ = DrawTextW(
                            hdc,
                            &mut chars,
                            &mut rect,
                            DT_CENTER | DT_VCENTER | DT_SINGLELINE,
                        );
                    }
                }
            }
        }

        let tooltip = self.hovered_item.map(|item| match item {
            ToolbarItem::Tool(ToolKind::Select) => "Select [V]".to_string(),
            ToolbarItem::Tool(ToolKind::Rectangle) => "Rectangle [R]".to_string(),
            ToolbarItem::Tool(ToolKind::Arrow) => "Arrow [A]".to_string(),
            ToolbarItem::Tool(ToolKind::Pen) => "Pen [P]".to_string(),
            ToolbarItem::Tool(ToolKind::Text) => "Text [T]".to_string(),
            ToolbarItem::Tool(ToolKind::Blur) => "Blur [B]".to_string(),
            ToolbarItem::Action(ToolbarAction::Undo) => "Undo [Ctrl+Z]".to_string(),
            ToolbarItem::Action(ToolbarAction::Redo) => "Redo [Ctrl+Y]".to_string(),
            ToolbarItem::Action(ToolbarAction::Save) => "Save [Ctrl+S]".to_string(),
            ToolbarItem::Action(ToolbarAction::Copy) => "Copy [Ctrl+C]".to_string(),
            ToolbarItem::Action(ToolbarAction::Settings) => "Settings [Ctrl+,]".to_string(),
            ToolbarItem::Action(ToolbarAction::Cancel) => "Close [Esc]".to_string(),
            ToolbarItem::Color(color) => match color {
                [49, 49, 224, 255] => "Red color".to_string(),
                [7, 103, 247, 255] => "Orange color".to_string(),
                [25, 196, 252, 255] => "Yellow color".to_string(),
                [68, 158, 47, 255] => "Green color".to_string(),
                [194, 113, 25, 255] => "Blue color".to_string(),
                [181, 54, 156, 255] => "Purple color".to_string(),
                [255, 255, 255, 255] => "White color".to_string(),
                [41, 37, 33, 255] => "Black color".to_string(),
                _ => "Custom color".to_string(),
            },
            ToolbarItem::Thickness(2) => "Thin (2 px)".to_string(),
            ToolbarItem::Thickness(4) => "Medium (4 px)".to_string(),
            ToolbarItem::Thickness(8) => "Thick (8 px)".to_string(),
            ToolbarItem::Thickness(value) => format!("{value} px thickness"),
        });
        if let Some(label) = tooltip {
            let button = self
                .buttons
                .iter()
                .find(|button| Some(button.item) == self.hovered_item)
                .expect("hovered toolbar item must have a button");
            let margin = scale(6);
            let tooltip_width = scale(label.len() as i32 * 7 + 16)
                .min((self.screen_width - margin * 2).max(scale(80)));
            let tooltip_height = scale(28);
            let tooltip_gap = scale(6);
            let is_tool_button = self.tool_bounds.contains(button.rect.left, button.rect.top);
            let (preferred_left, preferred_top) = if is_tool_button {
                let right = self.tool_bounds.right + tooltip_gap;
                let left = self.tool_bounds.left - tooltip_gap - tooltip_width;
                let x = if right + tooltip_width <= self.screen_width - margin {
                    right
                } else {
                    left
                };
                (
                    x,
                    (button.rect.top + button.rect.bottom - tooltip_height) / 2,
                )
            } else {
                let above = self.action_bounds.top - tooltip_gap - tooltip_height;
                let below = self.action_bounds.bottom + tooltip_gap;
                (
                    (button.rect.left + button.rect.right - tooltip_width) / 2,
                    if above >= margin { above } else { below },
                )
            };
            let left = preferred_left.clamp(
                margin,
                (self.screen_width - tooltip_width - margin).max(margin),
            );
            let top = preferred_top.clamp(
                margin,
                (self.screen_height - tooltip_height - margin).max(margin),
            );
            let tooltip_rect = Rect::new(left, top, left + tooltip_width, top + tooltip_height);
            draw_rounded(&tooltip_rect, panel_bg, accent, scale(5));

            let tooltip_face: Vec<u16> = "Segoe UI\0".encode_utf16().collect();
            let tooltip_font = unsafe {
                CreateFontW(
                    -scale(11),
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
                    PCWSTR(tooltip_face.as_ptr()),
                )
            };
            let old_tooltip_font = unsafe { SelectObject(hdc, HGDIOBJ(tooltip_font.0)) };
            let _tooltip_font_guard = FontGuard {
                hdc,
                old_font: old_tooltip_font,
                font: tooltip_font,
            };
            unsafe {
                let _ = SetTextColor(hdc, text);
                let mut chars: Vec<u16> = label.encode_utf16().collect();
                let mut rect = RECT {
                    left: tooltip_rect.left + scale(6),
                    top: tooltip_rect.top,
                    right: tooltip_rect.right - scale(6),
                    bottom: tooltip_rect.bottom,
                };
                let _ = DrawTextW(
                    hdc,
                    &mut chars,
                    &mut rect,
                    DT_CENTER | DT_VCENTER | DT_SINGLELINE,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_in_screen(rect: Rect, width: i32, height: i32) {
        assert!(rect.left >= 0 && rect.top >= 0, "{rect:?}");
        assert!(rect.right <= width && rect.bottom <= height, "{rect:?}");
    }

    #[test]
    fn standard_layout_joins_at_the_selection_bottom_right() {
        let selection = Rect::new(200, 100, 800, 600);
        let toolbar = Toolbar::layout(
            &selection,
            ToolKind::Rectangle,
            PRESET_COLORS[0],
            3,
            true,
            true,
            1920,
            1080,
            true,
            true,
            96,
        );

        assert_eq!(toolbar.tool_bounds.left, selection.right);
        assert_eq!(toolbar.tool_bounds.bottom, selection.bottom);
        assert_eq!(toolbar.tool_bounds.right, toolbar.action_bounds.right);
        assert_eq!(toolbar.tool_bounds.bottom, toolbar.action_bounds.top);
        assert_in_screen(toolbar.tool_bounds, 1920, 1080);
        assert_in_screen(toolbar.action_bounds, 1920, 1080);
    }

    #[test]
    fn constrained_layout_clamps_one_shared_corner() {
        let selection = Rect::new(600, 450, 630, 470);
        let toolbar = Toolbar::layout(
            &selection,
            ToolKind::Rectangle,
            PRESET_COLORS[0],
            3,
            true,
            true,
            640,
            480,
            false,
            false,
            96,
        );

        assert_eq!(toolbar.tool_bounds.right, toolbar.action_bounds.right);
        assert_eq!(toolbar.tool_bounds.bottom, toolbar.action_bounds.top);
        assert_in_screen(toolbar.tool_bounds, 640, 480);
        assert_in_screen(toolbar.action_bounds, 640, 480);
    }
}
