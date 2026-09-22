use crate::annotation::{ToolKind, bgra_to_colorref};
use crate::capture::Rect;
use crate::settings::{PRESET_COLORS, PRESET_THICKNESSES};
use windows::Win32::Foundation::{COLORREF, HWND, POINT};
use windows::Win32::Graphics::Gdi::{HDC, PS_SOLID, Polyline};
/// Corner radius (96-dpi px, scaled at each use site) for toolbar buttons and
/// color swatches. Panel radii use `crate::theme::RADIUS_CARD` instead.
const RADIUS_CONTROL: i32 = 4;

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
    /// Mirrors the toolbar's active tool/color/thickness for menu check marks.
    pub is_checked: bool,
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
    viewport: Rect,
}

impl Toolbar {
    pub const TOOL_BUTTON: i32 = 28;
    pub const ACTION_HEIGHT: i32 = 34;
    /// Air between the panels and the edge of the work area.
    pub const OUTER_PAD: i32 = 6;
    /// Air between the selection and the panels when they sit outside it.
    pub const GAP: i32 = 6;
    /// Air between the selection edge and the panels when they sit inside it.
    pub const INNER_PAD: i32 = 8;

    /// Builds a Lightshot-style tool rail beside the selection and a compact
    /// action strip below it. Each panel flips to the opposite edge as needed.
    #[allow(clippy::too_many_arguments)]
    pub fn layout(
        selection: &Rect,
        viewport: Rect,
        active_tool: ToolKind,
        active_color: [u8; 4],
        active_thickness: i32,
        show_color: bool,
        show_thickness: bool,
        can_undo: bool,
        can_redo: bool,
        dpi: u32,
    ) -> Self {
        let scale = |value: i32| value * dpi as i32 / 96;
        let outer_pad = scale(Self::OUTER_PAD);
        let gap = scale(Self::GAP);
        let inner_pad = scale(Self::INNER_PAD);
        let pad = scale(4);
        let tool_button = scale(Self::TOOL_BUTTON);
        let tool_items = [
            (ToolbarItem::Tool(ToolKind::Select), true),
            (ToolbarItem::Tool(ToolKind::Rectangle), true),
            (ToolbarItem::Tool(ToolKind::Arrow), true),
            (ToolbarItem::Tool(ToolKind::Pen), true),
            (ToolbarItem::Tool(ToolKind::Text), true),
            (ToolbarItem::Tool(ToolKind::Blur), true),
            (ToolbarItem::Tool(ToolKind::Redact), true),
            (ToolbarItem::Action(ToolbarAction::Undo), can_undo),
            (ToolbarItem::Action(ToolbarAction::Redo), can_redo),
        ];
        let tool_width = tool_button + pad * 2;
        let tool_height = pad * 2
            + tool_items.len() as i32 * tool_button
            + (tool_items.len() as i32 - 1) * scale(2);

        let color_size = scale(18);
        let color_step = scale(22);
        let thickness_width = scale(22);
        let thickness_step = scale(24);
        let action_button = scale(28);
        let action_step = scale(32);
        let section_gap = scale(8);
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
            + scale(48)
            - (action_step - action_button);
        let action_height = scale(Self::ACTION_HEIGHT);
        let safe = Rect::new(
            viewport.left + outer_pad,
            viewport.top + outer_pad,
            // Never invert the insets on a degenerate (impossible tiny) work area.
            (viewport.right - outer_pad).max(viewport.left + outer_pad),
            (viewport.bottom - outer_pad).max(viewport.top + outer_pad),
        );
        let contains = |outer: Rect, inner: Rect| {
            inner.left >= outer.left
                && inner.top >= outer.top
                && inner.right <= outer.right
                && inner.bottom <= outer.bottom
        };
        let make_l = |right: bool, below: bool, shared_x: i32, shared_y: i32| {
            let tool = match (right, below) {
                (true, true) => Rect::new(
                    shared_x - tool_width,
                    shared_y - tool_height,
                    shared_x,
                    shared_y,
                ),
                (false, true) => Rect::new(
                    shared_x,
                    shared_y - tool_height,
                    shared_x + tool_width,
                    shared_y,
                ),
                (true, false) => Rect::new(
                    shared_x - tool_width,
                    shared_y,
                    shared_x,
                    shared_y + tool_height,
                ),
                (false, false) => Rect::new(
                    shared_x,
                    shared_y,
                    shared_x + tool_width,
                    shared_y + tool_height,
                ),
            };
            let action = match (right, below) {
                (true, true) => Rect::new(
                    shared_x - action_width,
                    shared_y,
                    shared_x,
                    shared_y + action_height,
                ),
                (false, true) => Rect::new(
                    shared_x,
                    shared_y,
                    shared_x + action_width,
                    shared_y + action_height,
                ),
                (true, false) => Rect::new(
                    shared_x - action_width,
                    shared_y - action_height,
                    shared_x,
                    shared_y,
                ),
                (false, false) => Rect::new(
                    shared_x,
                    shared_y - action_height,
                    shared_x + action_width,
                    shared_y,
                ),
            };
            (tool, action)
        };

        let outside = [
            (
                true,
                true,
                selection.right + gap + tool_width,
                selection.bottom + gap,
            ),
            (
                false,
                true,
                selection.left - gap - tool_width,
                selection.bottom + gap,
            ),
            (
                true,
                false,
                selection.right + gap + tool_width,
                selection.top - gap,
            ),
            (
                false,
                false,
                selection.left - gap - tool_width,
                selection.top - gap,
            ),
        ]
        .into_iter()
        .map(|(right, below, x, y)| make_l(right, below, x, y))
        .find(|(tool, action)| contains(safe, *tool) && contains(safe, *action));

        let inside = if selection.width() >= action_width + inner_pad * 2
            && selection.height() >= tool_height + action_height + inner_pad * 2
        {
            let shared_x = selection.right - inner_pad;
            let shared_y = selection.bottom - inner_pad - action_height;
            let candidate = make_l(true, true, shared_x, shared_y);
            (contains(safe, candidate.0) && contains(safe, candidate.1)).then_some(candidate)
        } else {
            None
        };

        let (tool_bounds, action_bounds) = outside.or(inside).unwrap_or_else(|| {
            // A tiny selection cannot contain the editor. Keep the complete L bounded to
            // the selected monitor's work area rather than leaking onto another monitor.
            make_l(true, true, safe.right, safe.bottom - action_height)
        });

        let mut buttons = Vec::with_capacity(
            tool_items.len()
                + usize::from(show_color) * PRESET_COLORS.len()
                + usize::from(show_thickness) * PRESET_THICKNESSES.len()
                + 4,
        );
        let mut y = tool_bounds.bottom - pad - tool_button;
        for (item, enabled) in tool_items {
            let is_checked = matches!(item, ToolbarItem::Tool(tool) if tool == active_tool);
            buttons.push(ToolbarButton {
                item,
                rect: Rect::new(
                    tool_bounds.left + pad,
                    y,
                    tool_bounds.left + pad + tool_button,
                    y + tool_button,
                ),
                is_enabled: enabled,
                is_checked,
            });
            y -= tool_button + scale(2);
        }
        let item_y = action_bounds.top + (action_height - color_size) / 2;
        let mut x = action_bounds.left + pad;
        if show_color {
            for color in PRESET_COLORS {
                buttons.push(ToolbarButton {
                    item: ToolbarItem::Color(color),
                    rect: Rect::new(x, item_y, x + color_size, item_y + color_size),
                    is_enabled: true,
                    is_checked: color == active_color,
                });
                x += color_step;
            }
            x += section_gap - (color_step - color_size);
        }
        if show_thickness {
            for thickness in PRESET_THICKNESSES {
                let y = action_bounds.top + (action_height - tool_button) / 2;
                buttons.push(ToolbarButton {
                    item: ToolbarItem::Thickness(thickness),
                    rect: Rect::new(x, y, x + thickness_width, y + tool_button),
                    is_enabled: true,
                    is_checked: thickness == active_thickness,
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
            let y = action_bounds.top + (action_height - action_button) / 2;
            let button_width = action_button
                + if matches!(action, ToolbarAction::Save | ToolbarAction::Copy) {
                    scale(24)
                } else {
                    0
                };
            buttons.push(ToolbarButton {
                item: ToolbarItem::Action(action),
                rect: Rect::new(x, y, x + button_width, y + action_button),
                is_enabled: true,
                is_checked: false,
            });
            x += button_width + scale(3);
        }
        if tool_height + action_height > safe.height() || action_width > safe.width() {
            // A grid keeps every command reachable on short/high-DPI work areas.
            let count = (buttons.len() as i32).max(1);
            let mut cell = scale(32).max(8);
            loop {
                let cols = (safe.width() / cell).max(1);
                let rows = (count + cols - 1) / cols;
                if (rows * cell <= safe.height() && cols * cell <= safe.width()) || cell <= 8 {
                    break;
                }
                cell -= 1;
            }
            let cols = (safe.width() / cell).max(1).min(count);
            let rows = (count + cols - 1) / cols;
            // Clamp into the safe area: an undersized work area must still yield
            // in-bounds rectangles instead of buttons above or beyond the viewport.
            let bounds = Rect::new(
                safe.left,
                (safe.bottom - rows * cell).max(safe.top),
                (safe.left + cols * cell).min(safe.right).max(safe.left),
                safe.bottom,
            );
            let clamp = |value: i32, low: i32, high: i32| value.clamp(low, high.max(low));
            for (index, button) in buttons.iter_mut().enumerate() {
                let x = bounds.left + index as i32 % cols * cell;
                let y = bounds.top + index as i32 / cols * cell;
                let left = clamp(x + 2, bounds.left, bounds.right);
                let right = clamp(x + cell - 2, left, bounds.right);
                let top = clamp(y + 2, bounds.top, bounds.bottom);
                let bottom = clamp(y + cell - 2, top, bounds.bottom);
                button.rect = Rect::new(left, top, right, bottom);
            }
            return Self {
                tool_bounds: bounds,
                action_bounds: bounds,
                buttons,
                hovered_item: None,
                active_tool,
                active_color,
                active_thickness,
                dpi: (cell * 96 / 32).max(24) as u32,
                viewport,
            };
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
            viewport,
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

    /// Flat, high-contrast controls share the settings window's visual language.
    pub fn render(&self, hdc: HDC) {
        use crate::drawing::{icon, label, rounded, with_pen};
        let scale = |value: i32| (value * self.dpi as i32 / 96).max(1);
        // Native Fluent chrome from the shared token set (light/dark aware).
        let tokens = crate::theme::tokens();
        let card = tokens.card;
        let stroke = tokens.stroke;
        let text = tokens.text;
        let muted = tokens.text_disabled;
        let accent = tokens.accent;
        let tint = tokens.accent_tint;
        for (index, panel) in [self.tool_bounds, self.action_bounds]
            .into_iter()
            .enumerate()
        {
            if index == 1 && self.tool_bounds == self.action_bounds {
                continue;
            }
            rounded(
                hdc,
                panel.inflate(1, 1),
                scale(crate::theme::RADIUS_CARD),
                stroke,
                stroke,
            );
            rounded(hdc, panel, scale(crate::theme::RADIUS_CARD), card, stroke);
        }
        for button in &self.buttons {
            let hovered = self.hovered_item == Some(button.item) && button.is_enabled;
            let selected = matches!(button.item, ToolbarItem::Tool(tool) if tool == self.active_tool)
                || matches!(button.item, ToolbarItem::Thickness(value) if value == self.active_thickness);
            let primary = button.item == ToolbarItem::Action(ToolbarAction::Copy);
            let fill = if primary {
                accent
            } else if selected || hovered {
                tint
            } else {
                card
            };
            let ink = if !button.is_enabled {
                muted
            } else if primary {
                tokens.accent_text
            } else if selected {
                accent
            } else {
                text
            };
            rounded(
                hdc,
                button.rect,
                scale(RADIUS_CONTROL),
                fill,
                if selected { accent } else { fill },
            );
            match button.item {
                ToolbarItem::Color(color) => {
                    let swatch = button.rect.inflate(-scale(2), -scale(2));
                    rounded(
                        hdc,
                        swatch,
                        scale(RADIUS_CONTROL),
                        bgra_to_colorref(color),
                        if color == self.active_color {
                            accent
                        } else {
                            stroke
                        },
                    );
                    if color == self.active_color {
                        // Ink against the swatch color itself, so this pair
                        // stays theme-independent black/white.
                        let check = if color[0] as u32 + color[1] as u32 + color[2] as u32 > 450 {
                            COLORREF(0x2a211b)
                        } else {
                            COLORREF(0xffffff)
                        };
                        icon(hdc, swatch, 0xe73e, scale(10), check, true);
                    }
                }
                ToolbarItem::Thickness(value) => {
                    let y = (button.rect.top + button.rect.bottom) / 2;
                    with_pen(hdc, PS_SOLID, scale(value), ink, || unsafe {
                        let _ = Polyline(
                            hdc,
                            &[
                                POINT {
                                    x: button.rect.left + scale(4),
                                    y,
                                },
                                POINT {
                                    x: button.rect.right - scale(4),
                                    y,
                                },
                            ],
                        );
                    });
                }
                item => {
                    // Segoe Fluent Icons codepoints, each verified present in
                    // BOTH official glyph tables (Segoe Fluent Icons on
                    // Windows 11, Segoe MDL2 Assets on Windows 10) so the
                    // shared range still renders as a Windows 10 fallback:
                    //   Select E7C4 TaskView, Rectangle E799 AspectRatio,
                    //   Arrow E72A Forward, Pen E70F Edit (pencil),
                    //   Text E90A Comment (text-callout bubble),
                    //   Undo E7A7, Redo E7A6, Save E74E, Copy E8C8,
                    //   Settings E713, Cancel E711, swatch check E73E.
                    // Suggested values that are wrong in the real tables were
                    // avoided: E734 is FavoriteStar, E74C is OEM, E72C is
                    // Refresh (Redo is E7A6).
                    // Blur (▦ mosaic) and Redact (■ solid bar) stay Segoe UI
                    // text: neither icon font has a shared blur/redaction
                    // glyph — Effects E794, Contrast E7A1, PortraitBlur EABE
                    // and Blocked E733 are Fluent-only and would not resolve
                    // on Windows 10.
                    let codepoint = match item {
                        ToolbarItem::Tool(ToolKind::Select) => Some(0xe7c4u16),
                        ToolbarItem::Tool(ToolKind::Rectangle) => Some(0xe799),
                        ToolbarItem::Tool(ToolKind::Arrow) => Some(0xe72a),
                        ToolbarItem::Tool(ToolKind::Pen) => Some(0xe70f),
                        ToolbarItem::Tool(ToolKind::Text) => Some(0xe90a),
                        ToolbarItem::Action(ToolbarAction::Undo) => Some(0xe7a7),
                        ToolbarItem::Action(ToolbarAction::Redo) => Some(0xe7a6),
                        ToolbarItem::Action(ToolbarAction::Save) => Some(0xe74e),
                        ToolbarItem::Action(ToolbarAction::Copy) => Some(0xe8c8),
                        ToolbarItem::Action(ToolbarAction::Settings) => Some(0xe713),
                        ToolbarItem::Action(ToolbarAction::Cancel) => Some(0xe711),
                        _ => None,
                    };
                    match codepoint {
                        Some(codepoint) => icon(hdc, button.rect, codepoint, scale(14), ink, true),
                        None => {
                            let glyph = match item {
                                ToolbarItem::Tool(ToolKind::Blur) => "▦",
                                ToolbarItem::Tool(ToolKind::Redact) => "■",
                                _ => "",
                            };
                            label(hdc, button.rect, glyph, scale(16), ink, true);
                        }
                    }
                }
            }
        }
        if let Some(item) = self.hovered_item
            && let Some(button) = self.buttons.iter().find(|button| button.item == item)
        {
            let tip = match item {
                ToolbarItem::Tool(ToolKind::Select) => "Select · V".to_owned(),
                ToolbarItem::Tool(ToolKind::Rectangle) => "Rectangle · R".to_owned(),
                ToolbarItem::Tool(ToolKind::Arrow) => "Arrow · A".to_owned(),
                ToolbarItem::Tool(ToolKind::Pen) => "Pen · P".to_owned(),
                ToolbarItem::Tool(ToolKind::Text) => "Text · T".to_owned(),
                ToolbarItem::Tool(ToolKind::Blur) => "Blur · B".to_owned(),
                ToolbarItem::Tool(ToolKind::Redact) => "Redact · M".to_owned(),
                ToolbarItem::Action(ToolbarAction::Undo) => "Undo · Ctrl+Z".to_owned(),
                ToolbarItem::Action(ToolbarAction::Redo) => "Redo · Ctrl+Y".to_owned(),
                ToolbarItem::Action(ToolbarAction::Save) => {
                    "Save · Ctrl+S | Save as · Ctrl+Shift+S".to_owned()
                }
                ToolbarItem::Action(ToolbarAction::Copy) => "Copy · Ctrl+C".to_owned(),
                ToolbarItem::Action(ToolbarAction::Settings) => "Settings · Ctrl+,".to_owned(),
                ToolbarItem::Action(ToolbarAction::Cancel) => "Cancel · Esc".to_owned(),
                ToolbarItem::Color(_) => "Annotation color".to_owned(),
                ToolbarItem::Thickness(value) => format!("{value} px stroke"),
            };
            let margin = scale(4);
            let width = (crate::drawing::measure_text(&tip, scale(11)).0 + scale(16))
                .min((self.viewport.width() - margin * 2).max(1));
            let height = scale(24);
            let x = button.rect.left.clamp(
                self.viewport.left + margin,
                (self.viewport.right - width - margin).max(self.viewport.left + margin),
            );
            let y = (button.rect.top - height - margin).clamp(
                self.viewport.top + margin,
                (self.viewport.bottom - height - margin).max(self.viewport.top + margin),
            );
            let bounds = Rect::new(x, y, x + width, y + height);
            rounded(
                hdc,
                bounds,
                scale(crate::theme::RADIUS_CARD),
                tokens.card,
                tokens.stroke,
            );
            label(hdc, bounds, &tip, scale(11), tokens.text, true);
        }
    }
}

/// Presents the toolbar's commands as a native popup menu for keyboard and screen-reader
/// access (F10 / Apps key). Returns the chosen command; the overlay is never mutated.
pub fn show_command_menu(
    owner: HWND,
    buttons: &[ToolbarButton],
) -> windows::core::Result<Option<ToolbarItem>> {
    use windows::Win32::UI::WindowsAndMessaging::{
        AppendMenuW, CreatePopupMenu, DestroyMenu, GetCursorPos, MF_CHECKED, MF_GRAYED, MF_STRING,
        TPM_RETURNCMD, TPM_RIGHTBUTTON, TrackPopupMenuEx,
    };
    use windows::core::PCWSTR;
    if buttons.is_empty() {
        return Ok(None);
    }
    // The menu runs a modal loop; suspend overlay hotkeys for its lifetime.
    let _suspension = crate::hotkey::OverlayInputSuspension::new();
    struct MenuGuard(windows::Win32::UI::WindowsAndMessaging::HMENU);
    impl Drop for MenuGuard {
        fn drop(&mut self) {
            unsafe {
                let _ = DestroyMenu(self.0);
            }
        }
    }
    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(Some(0)).collect()
    }
    fn group(item: ToolbarItem) -> &'static str {
        match item {
            ToolbarItem::Tool(_) => "Tools",
            ToolbarItem::Action(ToolbarAction::Undo | ToolbarAction::Redo) => "History",
            ToolbarItem::Color(_) => "Color",
            ToolbarItem::Thickness(_) => "Line width",
            ToolbarItem::Action(_) => "Commands",
        }
    }
    fn label(item: ToolbarItem) -> String {
        match item {
            ToolbarItem::Tool(ToolKind::Select) => "Select region".to_owned(),
            ToolbarItem::Tool(ToolKind::Rectangle) => "Rectangle".to_owned(),
            ToolbarItem::Tool(ToolKind::Arrow) => "Arrow".to_owned(),
            ToolbarItem::Tool(ToolKind::Pen) => "Pen".to_owned(),
            ToolbarItem::Tool(ToolKind::Text) => "Text".to_owned(),
            ToolbarItem::Tool(ToolKind::Blur) => "Blur".to_owned(),
            ToolbarItem::Tool(ToolKind::Redact) => "Redact".to_owned(),
            ToolbarItem::Action(ToolbarAction::Undo) => "Undo".to_owned(),
            ToolbarItem::Action(ToolbarAction::Redo) => "Redo".to_owned(),
            ToolbarItem::Action(ToolbarAction::Save) => "Save screenshot".to_owned(),
            ToolbarItem::Action(ToolbarAction::Copy) => "Copy to clipboard".to_owned(),
            ToolbarItem::Action(ToolbarAction::Settings) => "Settings".to_owned(),
            ToolbarItem::Action(ToolbarAction::Cancel) => "Cancel".to_owned(),
            ToolbarItem::Color(color) => PRESET_COLORS
                .iter()
                .position(|preset| *preset == color)
                .and_then(|index| {
                    [
                        "Red", "Orange", "Yellow", "Green", "Blue", "Purple", "White", "Black",
                    ]
                    .get(index)
                })
                .copied()
                .unwrap_or("Custom color")
                .to_owned(),
            ToolbarItem::Thickness(value) => format!("{value} px"),
        }
    }

    let menu = MenuGuard(unsafe { CreatePopupMenu()? });
    let mut mapping: Vec<ToolbarItem> = Vec::with_capacity(buttons.len());
    let mut heading: Option<&'static str> = None;
    for button in buttons {
        let group = group(button.item);
        if heading != Some(group) {
            heading = Some(group);
            let text = wide(group);
            unsafe {
                AppendMenuW(menu.0, MF_STRING | MF_GRAYED, 0, PCWSTR(text.as_ptr()))?;
            }
        }
        let text = wide(&label(button.item));
        let mut flags = MF_STRING;
        if !button.is_enabled {
            flags |= MF_GRAYED;
        }
        if button.is_checked {
            flags |= MF_CHECKED;
        }
        unsafe {
            AppendMenuW(menu.0, flags, mapping.len() + 1, PCWSTR(text.as_ptr()))?;
        }
        mapping.push(button.item);
    }
    let mut point = POINT::default();
    if unsafe { GetCursorPos(&mut point) }.is_err() {
        return Ok(None);
    }
    let command = unsafe {
        TrackPopupMenuEx(
            menu.0,
            (TPM_RETURNCMD | TPM_RIGHTBUTTON).0,
            point.x,
            point.y,
            owner,
            None,
        )
    };
    if command.0 <= 0 {
        return Ok(None);
    }
    Ok(mapping.get(command.0 as usize - 1).copied())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layout(selection: Rect, viewport: Rect) -> Toolbar {
        Toolbar::layout(
            &selection,
            viewport,
            ToolKind::Rectangle,
            PRESET_COLORS[0],
            3,
            true,
            true,
            true,
            true,
            96,
        )
    }

    fn assert_in_viewport(rect: Rect, viewport: Rect) {
        assert!(
            rect.left >= viewport.left && rect.top >= viewport.top,
            "{rect:?}"
        );
        assert!(
            rect.right <= viewport.right && rect.bottom <= viewport.bottom,
            "{rect:?}"
        );
    }

    fn assert_exact_l_corner(toolbar: &Toolbar) {
        let tool_corners = [
            (toolbar.tool_bounds.left, toolbar.tool_bounds.top),
            (toolbar.tool_bounds.right, toolbar.tool_bounds.top),
            (toolbar.tool_bounds.left, toolbar.tool_bounds.bottom),
            (toolbar.tool_bounds.right, toolbar.tool_bounds.bottom),
        ];
        let action_corners = [
            (toolbar.action_bounds.left, toolbar.action_bounds.top),
            (toolbar.action_bounds.right, toolbar.action_bounds.top),
            (toolbar.action_bounds.left, toolbar.action_bounds.bottom),
            (toolbar.action_bounds.right, toolbar.action_bounds.bottom),
        ];
        let shared = tool_corners
            .into_iter()
            .filter(|corner| action_corners.contains(corner))
            .count();
        assert_eq!(
            shared, 1,
            "tool={:?}, action={:?}",
            toolbar.tool_bounds, toolbar.action_bounds
        );
    }

    #[test]
    fn normal_region_uses_padded_outside_bottom_right_l() {
        let selection = Rect::new(200, 100, 800, 600);
        let viewport = Rect::new(0, 0, 1920, 1040);
        let toolbar = layout(selection, viewport);
        assert_eq!(toolbar.tool_bounds.left, selection.right + Toolbar::GAP);
        assert_eq!(toolbar.action_bounds.top, selection.bottom + Toolbar::GAP);
        assert_exact_l_corner(&toolbar);
        assert_in_viewport(toolbar.tool_bounds, viewport);
        assert_in_viewport(toolbar.action_bounds, viewport);
    }

    #[test]
    fn selection_on_second_monitor_never_uses_neighbor() {
        let viewport = Rect::new(1920, 0, 3840, 1040);
        let toolbar = layout(Rect::new(1940, 120, 2520, 680), viewport);
        assert!(toolbar.tool_bounds.left >= viewport.left + 8);
        assert!(toolbar.action_bounds.left >= viewport.left + 8);
        assert_in_viewport(toolbar.tool_bounds, viewport);
        assert_in_viewport(toolbar.action_bounds, viewport);
        assert_exact_l_corner(&toolbar);
    }

    #[test]
    fn full_monitor_selection_uses_padded_inside_bottom_right_l() {
        let viewport = Rect::new(0, 0, 1920, 1040);
        let toolbar = layout(viewport, viewport);
        assert_eq!(
            toolbar.tool_bounds.right,
            viewport.right - Toolbar::INNER_PAD
        );
        assert_eq!(
            toolbar.action_bounds.right,
            viewport.right - Toolbar::INNER_PAD
        );
        assert_eq!(
            toolbar.action_bounds.bottom,
            viewport.bottom - Toolbar::INNER_PAD
        );
        assert_exact_l_corner(&toolbar);
        assert_in_viewport(toolbar.tool_bounds, viewport);
        assert_in_viewport(toolbar.action_bounds, viewport);
    }

    #[test]
    fn tiny_selection_fallback_is_complete_and_monitor_bounded() {
        let viewport = Rect::new(1920, 40, 3200, 900);
        let toolbar = layout(Rect::new(2500, 400, 2510, 410), viewport);
        assert_in_viewport(toolbar.tool_bounds, viewport);
        assert_in_viewport(toolbar.action_bounds, viewport);
        assert_exact_l_corner(&toolbar);
        assert!(toolbar.buttons.iter().all(|button| {
            button.rect.left >= toolbar.tool_bounds.left.min(toolbar.action_bounds.left)
                && button.rect.right <= toolbar.tool_bounds.right.max(toolbar.action_bounds.right)
        }));
    }
    #[test]
    fn high_dpi_short_monitor_keeps_every_command_clickable() {
        for (width, height, dpi) in [
            (1366, 728, 144),
            (1920, 1040, 192),
            (640, 440, 192),
            (320, 240, 96),
        ] {
            let viewport = Rect::new(-width, 0, 0, height);
            let toolbar = Toolbar::layout(
                &Rect::new(-40, 10, -32, 18),
                viewport,
                ToolKind::Text,
                PRESET_COLORS[0],
                2,
                true,
                true,
                true,
                true,
                dpi,
            );
            assert_in_viewport(toolbar.tool_bounds, viewport);
            assert_in_viewport(toolbar.action_bounds, viewport);
            for button in &toolbar.buttons {
                assert_in_viewport(button.rect, viewport);
                assert_eq!(
                    toolbar.hit_test((
                        (button.rect.left + button.rect.right) / 2,
                        (button.rect.top + button.rect.bottom) / 2
                    )),
                    Some(button.item)
                );
            }
        }
    }

    #[test]
    fn missing_hover_target_does_not_crash_rendering() {
        let mut toolbar = Toolbar::layout(
            &Rect::new(10, 10, 20, 20),
            Rect::new(0, 0, 800, 600),
            ToolKind::Blur,
            PRESET_COLORS[0],
            2,
            false,
            false,
            false,
            false,
            96,
        );
        toolbar.hovered_item = Some(ToolbarItem::Color(PRESET_COLORS[0]));
        let dc = unsafe { windows::Win32::Graphics::Gdi::CreateCompatibleDC(None) };
        assert!(!dc.is_invalid());
        toolbar.render(dc);
        unsafe {
            let _ = windows::Win32::Graphics::Gdi::DeleteDC(dc);
        }
    }
}
