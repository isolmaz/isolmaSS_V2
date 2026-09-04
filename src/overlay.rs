use crate::annotation::{
    AnnotationKind, AnnotationObject, EditCommand, HistoryManager, ToolKind, bgra_to_colorref,
    render_pen_preview, snap_angle_45, snap_square,
};
use crate::capture::{CaptureBuffer, Rect, SelectionHitZone};
use crate::clipboard::{copy_dib_to_clipboard, flatten_selection_to_dib};
use crate::hotkey::{
    WM_OVERLAY_CHAR, WM_OVERLAY_KEYDOWN, register_overlay, set_overlay_text_editing,
    unregister_overlay,
};
use crate::save::save_screenshot;
use crate::settings::{Settings, show_settings_dialog};
use crate::toolbar::{Toolbar, ToolbarAction, ToolbarItem};
use crate::window_snap::{WindowInfo, find_window_in_list, get_visible_windows};
use std::ffi::c_void;
use std::rc::Rc;
use windows::Win32::Foundation::{
    COLORREF, ERROR_CLASS_ALREADY_EXISTS, GetLastError, HINSTANCE, HWND, LPARAM, LRESULT, WPARAM,
};
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BeginPaint, BitBlt, CLIP_DEFAULT_PRECIS,
    CreateCompatibleDC, CreateDIBSection, CreateFontW, DEFAULT_CHARSET, DEFAULT_PITCH,
    DEFAULT_QUALITY, DIB_RGB_COLORS, DeleteDC, DeleteObject, EndPaint, FF_DONTCARE, FW_BOLD,
    GdiFlush, GetDC, HBITMAP, HDC, HGDIOBJ, InvalidateRect, RGBQUAD, ReleaseDC, SRCCOPY,
    ScreenToClient, SelectObject, SetBkMode, SetTextColor, TRANSPARENT, TextOutW,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyState, MOD_CONTROL, ReleaseCapture, SetCapture, SetFocus, VK_BACK, VK_CONTROL, VK_DELETE,
    VK_ESCAPE, VK_LEFT, VK_OEM_COMMA, VK_RETURN, VK_RIGHT, VK_SHIFT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CS_DBLCLKS, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GWLP_USERDATA,
    GetCursorPos, GetMessageW, GetWindowLongPtrW, IDC_ARROW, IDC_CROSS, IDC_HAND, IDC_IBEAM,
    IDC_SIZEALL, IDC_SIZENESW, IDC_SIZENWSE, KillTimer, LWA_ALPHA, LoadCursorW, MA_ACTIVATE,
    MB_ICONERROR, MB_OK, MSG, MessageBoxW, PostQuitMessage, RegisterClassExW, SW_SHOW, SetCursor,
    SetForegroundWindow, SetLayeredWindowAttributes, SetTimer, SetWindowLongPtrW, ShowWindow,
    TranslateMessage, WM_CHAR, WM_CLOSE, WM_DESTROY, WM_DPICHANGED, WM_ERASEBKGND, WM_KEYDOWN,
    WM_LBUTTONDBLCLK, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEACTIVATE, WM_MOUSEMOVE, WM_PAINT,
    WM_RBUTTONDOWN, WM_RBUTTONUP, WM_SETCURSOR, WM_TIMER, WNDCLASSEXW, WS_EX_LAYERED,
    WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};
use windows::core::{PCWSTR, Result, w};

mod session;
pub use session::show_overlay_session;

const OVERLAY_CLASS_NAME: windows::core::PCWSTR = w!("isolmaSS_OverlayClass");

const DEFAULT_FONT_SIZE: i32 = 22;
const DEFAULT_BLUR_BLOCK: i32 = 12;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlayMode {
    Hovering,
    DraggingSelection,
    SelectionActive,
}

#[derive(Debug, Clone)]
enum InProgressDrawing {
    Rectangle {
        start: (i32, i32),
        current: (i32, i32),
    },
    Arrow {
        start: (i32, i32),
        current: (i32, i32),
    },
    Pen {
        points: Vec<(i32, i32)>,
    },
    Blur {
        start: (i32, i32),
        current: (i32, i32),
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DragSelectionAction {
    Move,
    Resize(SelectionHitZone),
}

#[derive(Debug, Clone, Copy)]
struct DragSelectionState {
    action: DragSelectionAction,
    start_pos: (i32, i32),
    last_pos: (i32, i32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EscapeAction {
    CancelledTextEdit,
    DeselectedObject(usize),
    CancelledSelection,
    CloseOverlay,
}

#[derive(Debug, Clone)]
pub struct TextEditState {
    pub pos: (i32, i32),
    pub text: String,
    pub caret: usize,
    pub color: [u8; 4],
    pub font_size: i32,
    pub editing_id: Option<usize>,
    pub caret_visible: bool,
}

impl TextEditState {
    pub fn new(
        pos: (i32, i32),
        text: String,
        color: [u8; 4],
        font_size: i32,
        editing_id: Option<usize>,
    ) -> Self {
        let caret = text.chars().count();
        Self {
            pos,
            text,
            caret,
            color,
            font_size,
            editing_id,
            caret_visible: true,
        }
    }

    pub fn insert_char(&mut self, ch: char) {
        let mut chars: Vec<char> = self.text.chars().collect();
        let caret = self.caret.min(chars.len());
        chars.insert(caret, ch);
        self.text = chars.into_iter().collect();
        self.caret = caret + 1;
        self.caret_visible = true;
    }

    pub fn backspace(&mut self) -> bool {
        let mut chars: Vec<char> = self.text.chars().collect();
        let caret = self.caret.min(chars.len());
        if caret > 0 {
            chars.remove(caret - 1);
            self.text = chars.into_iter().collect();
            self.caret = caret - 1;
            self.caret_visible = true;
            true
        } else {
            false
        }
    }

    pub fn delete(&mut self) -> bool {
        let mut chars: Vec<char> = self.text.chars().collect();
        let caret = self.caret.min(chars.len());
        if caret < chars.len() {
            chars.remove(caret);
            self.text = chars.into_iter().collect();
            self.caret_visible = true;
            true
        } else {
            false
        }
    }

    pub fn move_left(&mut self) -> bool {
        if self.caret > 0 {
            self.caret -= 1;
            self.caret_visible = true;
            true
        } else {
            false
        }
    }

    pub fn move_right(&mut self) -> bool {
        if self.caret < self.text.chars().count() {
            self.caret += 1;
            self.caret_visible = true;
            true
        } else {
            false
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct DragObjectState {
    id: usize,
    start_pos: (i32, i32),
    last_pos: (i32, i32),
}

struct ScreenDcGuard(HDC);

impl Drop for ScreenDcGuard {
    fn drop(&mut self) {
        unsafe {
            let _ = ReleaseDC(HWND::default(), self.0);
        }
    }
}

struct DeleteDcGuard(HDC);

impl Drop for DeleteDcGuard {
    fn drop(&mut self) {
        unsafe {
            let _ = DeleteDC(self.0);
        }
    }
}

struct DeleteBitmapGuard(HBITMAP);

impl Drop for DeleteBitmapGuard {
    fn drop(&mut self) {
        unsafe {
            let _ = DeleteObject(HGDIOBJ(self.0.0));
        }
    }
}

pub struct OverlayState {
    capture: Rc<CaptureBuffer>,
    mode: OverlayMode,
    drag_start: Option<(i32, i32)>,
    committed_selection: Option<Rect>,
    hover_snap_rect: Option<Rect>,
    visible_windows: Vec<WindowInfo>,
    dpi: u32,

    settings: Settings,
    active_tool: ToolKind,
    active_color: [u8; 4],
    active_thickness: i32,
    objects: Vec<AnnotationObject>,
    selected_id: Option<usize>,
    next_id: usize,
    history: HistoryManager,

    drawing_shape: Option<InProgressDrawing>,
    dragging_object: Option<DragObjectState>,
    dragging_selection: Option<DragSelectionState>,
    text_edit: Option<TextEditState>,

    toolbar: Option<Toolbar>,

    mem_dc: HDC,
    dib: HBITMAP,
    old_bmp: HGDIOBJ,
    bits_ptr: *mut u8,

    committed_result: bool,
}

impl Drop for OverlayState {
    fn drop(&mut self) {
        unregister_overlay();
        unsafe {
            if !self.mem_dc.0.is_null() {
                SelectObject(self.mem_dc, self.old_bmp);
                let _ = DeleteDC(self.mem_dc);
            }
            if !self.dib.0.is_null() {
                let _ = DeleteObject(HGDIOBJ(self.dib.0));
            }
        }
    }
}

impl OverlayState {
    pub fn handle_escape_action(&mut self) -> EscapeAction {
        if self.text_edit.is_some() {
            self.text_edit = None;
            EscapeAction::CancelledTextEdit
        } else if let Some(id) = self.selected_id.take() {
            EscapeAction::DeselectedObject(id)
        } else if self.mode == OverlayMode::SelectionActive {
            self.mode = OverlayMode::Hovering;
            self.committed_selection = None;
            self.toolbar = None;
            self.objects.clear();
            EscapeAction::CancelledSelection
        } else {
            EscapeAction::CloseOverlay
        }
    }

    pub fn create_test_state(capture: Rc<CaptureBuffer>) -> Self {
        Self {
            capture,
            mode: OverlayMode::Hovering,
            drag_start: None,
            committed_selection: None,
            hover_snap_rect: None,
            visible_windows: Vec::new(),
            dpi: 96,
            settings: Settings::default(),
            active_tool: ToolKind::Rectangle,
            active_color: [49, 49, 224, 255],
            active_thickness: Settings::default().default_thickness,
            objects: Vec::new(),
            selected_id: None,
            next_id: 1,
            history: HistoryManager::default(),
            drawing_shape: None,
            dragging_object: None,
            dragging_selection: None,
            text_edit: None,
            toolbar: None,
            mem_dc: HDC::default(),
            dib: HBITMAP::default(),
            old_bmp: HGDIOBJ::default(),
            bits_ptr: std::ptr::null_mut(),
            committed_result: false,
        }
    }

    pub fn mode(&self) -> OverlayMode {
        self.mode
    }

    #[allow(dead_code)]
    pub fn set_mode(&mut self, mode: OverlayMode) {
        self.mode = mode;
    }

    pub fn text_edit(&self) -> Option<&TextEditState> {
        self.text_edit.as_ref()
    }

    pub fn set_text_edit(&mut self, edit: Option<TextEditState>) {
        self.text_edit = edit;
    }

    pub fn selected_id(&self) -> Option<usize> {
        self.selected_id
    }

    pub fn set_selected_id(&mut self, id: Option<usize>) {
        self.selected_id = id;
    }

    pub fn committed_selection(&self) -> Option<Rect> {
        self.committed_selection
    }

    #[allow(dead_code)]
    pub fn set_committed_selection(&mut self, rect: Option<Rect>) {
        self.committed_selection = rect;
    }

    pub fn set_selection_active(&mut self, rect: Rect) {
        self.mode = OverlayMode::SelectionActive;
        self.committed_selection = Some(rect);
    }

    fn composite_scene(&mut self) {
        // Synchronize the DIB's GDI target before replacing its pixels on the CPU.
        unsafe {
            let _ = GdiFlush();
        }
        let width = self.capture.width;
        let height = self.capture.height;
        let len = (width as usize) * (height as usize) * 4;
        let buffer = unsafe { std::slice::from_raw_parts_mut(self.bits_ptr, len) };

        match self.mode {
            OverlayMode::Hovering => {
                buffer.copy_from_slice(&self.capture.dimmed);

                if let Some(snap) = self.hover_snap_rect {
                    self.capture.punch_out(buffer, &snap);
                    CaptureBuffer::draw_border(
                        buffer,
                        width,
                        height,
                        &snap,
                        [246, 130, 59, 255],
                        2,
                    );
                }
            }
            OverlayMode::DraggingSelection => {
                buffer.copy_from_slice(&self.capture.dimmed);

                if let Some(sel) = self.committed_selection {
                    self.capture.punch_out(buffer, &sel);
                    CaptureBuffer::draw_border(buffer, width, height, &sel, [246, 130, 59, 255], 2);
                }
            }
            OverlayMode::SelectionActive => {
                let Some(sel) = self.committed_selection else {
                    return;
                };

                // 1. Reset to dimmed
                buffer.copy_from_slice(&self.capture.dimmed);

                // 2. Punch out selection
                self.capture.punch_out(buffer, &sel);

                // 3. Render blur objects
                for obj in &self.objects {
                    obj.render_blur(buffer, width, height);
                }

                if let Some(InProgressDrawing::Blur { start, current }) = self.drawing_shape {
                    let r = Rect::normalized(start, current).clamp(width, height);
                    crate::annotation::apply_pixelate_blur(
                        buffer,
                        width,
                        height,
                        &r,
                        DEFAULT_BLUR_BLOCK,
                    );
                }

                // 4. Draw dual-tone high-contrast selection border and handles
                CaptureBuffer::draw_contrast_selection(
                    buffer,
                    width,
                    height,
                    &sel,
                    [246, 130, 59, 255],
                );

                // 5. Render vector annotations
                for obj in &self.objects {
                    obj.render_gdi(self.mem_dc);
                }

                // 6. Draw in-progress preview
                match &self.drawing_shape {
                    Some(InProgressDrawing::Rectangle { start, current }) => {
                        let r = Rect::normalized(*start, *current).clamp(width, height);
                        let preview = AnnotationObject::new(
                            0,
                            AnnotationKind::Rectangle {
                                rect: r,
                                color: self.active_color,
                                thickness: self.active_thickness,
                            },
                        );
                        preview.render_gdi(self.mem_dc);
                    }
                    Some(InProgressDrawing::Arrow { start, current }) => {
                        let preview = AnnotationObject::new(
                            0,
                            AnnotationKind::Arrow {
                                start: *start,
                                end: *current,
                                color: self.active_color,
                                thickness: self.active_thickness,
                            },
                        );
                        preview.render_gdi(self.mem_dc);
                    }
                    Some(InProgressDrawing::Pen { points }) => render_pen_preview(
                        self.mem_dc,
                        points,
                        self.active_color,
                        self.active_thickness,
                    ),
                    _ => {}
                }

                // 7. Draw selection handles for selected object
                if let Some(obj) = self
                    .selected_id
                    .and_then(|id| self.objects.iter().find(|o| o.id == id))
                {
                    obj.render_selection_indicator(self.mem_dc);
                }

                // 8. Draw active text edit preview with font match & blinking caret
                if let Some(text_edit) = &self.text_edit {
                    let wide_face: Vec<u16> = "Segoe UI\0".encode_utf16().collect();
                    let font = unsafe {
                        CreateFontW(
                            text_edit.font_size,
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
                    let old_font = unsafe { SelectObject(self.mem_dc, HGDIOBJ(font.0)) };

                    unsafe {
                        let _ = SetTextColor(self.mem_dc, bgra_to_colorref(text_edit.color));
                        let _ = SetBkMode(self.mem_dc, TRANSPARENT);
                        let chars: Vec<char> = text_edit.text.chars().collect();
                        let caret_idx = text_edit.caret.min(chars.len());
                        let before: String = chars[..caret_idx].iter().collect();
                        let after: String = chars[caret_idx..].iter().collect();
                        let display_text = if text_edit.caret_visible {
                            format!("{before}|{after}")
                        } else {
                            format!("{before} {after}")
                        };
                        let wide: Vec<u16> = display_text.encode_utf16().collect();
                        let _ = TextOutW(self.mem_dc, text_edit.pos.0, text_edit.pos.1, &wide);

                        SelectObject(self.mem_dc, old_font);
                        let _ = DeleteObject(HGDIOBJ(font.0));
                    }
                }

                // 9. Render Toolbar (Two-row: Tools + Palette/Thickness)
                let can_undo = self.history.can_undo();
                let can_redo = self.history.can_redo();
                let mut tb = Toolbar::layout(
                    &sel,
                    self.active_tool,
                    self.active_color,
                    self.active_thickness,
                    width,
                    height,
                    can_undo,
                    can_redo,
                    self.dpi,
                );
                if let Some(existing) = &self.toolbar {
                    tb.hovered_item = existing.hovered_item;
                }
                tb.render(self.mem_dc);
                self.toolbar = Some(tb);
            }
        }
        // Make the fully rebuilt backbuffer visible to the subsequent WM_PAINT copy.
        unsafe {
            let _ = GdiFlush();
        }
    }

    fn commit_text(&mut self, hwnd: HWND) {
        if let Some(edit) = self.text_edit.take() {
            unsafe {
                let _ = KillTimer(hwnd, 1);
            }
            set_overlay_text_editing(false);
            let trimmed = edit.text.trim().to_string();
            if !trimmed.is_empty() {
                let (obj_id, is_modify) = if let Some(existing_id) = edit.editing_id {
                    (existing_id, true)
                } else {
                    let id = self.next_id;
                    self.next_id += 1;
                    (id, false)
                };

                let new_kind = AnnotationKind::Text {
                    pos: edit.pos,
                    text: trimmed,
                    color: edit.color,
                    font_size: edit.font_size,
                };
                let obj = AnnotationObject::new(obj_id, new_kind.clone());

                if is_modify {
                    if let Some(existing) = self.objects.iter_mut().find(|o| o.id == obj_id) {
                        let old_kind = existing.kind.clone();
                        existing.kind = new_kind.clone();
                        self.history.record(EditCommand::Modify {
                            id: obj_id,
                            old_kind,
                            new_kind,
                        });
                    } else {
                        self.history.record(EditCommand::Add(obj.clone()));
                        self.objects.push(obj);
                    }
                } else {
                    self.history.record(EditCommand::Add(obj.clone()));
                    self.objects.push(obj);
                }
                self.selected_id = Some(obj_id);
            }
            self.redraw(hwnd);
        }
    }

    fn handle_key_down(&mut self, hwnd: HWND, vk: usize, mods: isize) -> LRESULT {
        let ctrl_down = (mods & MOD_CONTROL.0 as isize) != 0
            || ((unsafe { GetKeyState(VK_CONTROL.0 as i32) } as u16 & 0x8000) != 0);

        // 1. Hierarchical Escape flow (production method used directly)
        if vk == VK_ESCAPE.0 as usize {
            match self.handle_escape_action() {
                EscapeAction::CancelledTextEdit => {
                    unsafe {
                        let _ = KillTimer(hwnd, 1);
                    }
                    set_overlay_text_editing(false);
                    self.redraw(hwnd);
                }
                EscapeAction::DeselectedObject(_) | EscapeAction::CancelledSelection => {
                    self.redraw(hwnd);
                }
                EscapeAction::CloseOverlay => {
                    let _ = unsafe { DestroyWindow(hwnd) };
                }
            }
            return LRESULT(0);
        }

        // 2. If currently in TextEditState:
        if self.text_edit.is_some() {
            match vk {
                x if x == VK_RETURN.0 as usize => {
                    self.commit_text(hwnd);
                    return LRESULT(0);
                }
                x if x == VK_BACK.0 as usize => {
                    if let Some(edit) = &mut self.text_edit
                        && edit.backspace()
                    {
                        self.redraw(hwnd);
                    }
                    return LRESULT(0);
                }
                x if x == VK_DELETE.0 as usize => {
                    if let Some(edit) = &mut self.text_edit
                        && edit.delete()
                    {
                        self.redraw(hwnd);
                    }
                    return LRESULT(0);
                }
                x if x == VK_LEFT.0 as usize => {
                    if let Some(edit) = &mut self.text_edit
                        && edit.move_left()
                    {
                        self.redraw(hwnd);
                    }
                    return LRESULT(0);
                }
                x if x == VK_RIGHT.0 as usize => {
                    if let Some(edit) = &mut self.text_edit
                        && edit.move_right()
                    {
                        self.redraw(hwnd);
                    }
                    return LRESULT(0);
                }
                _ => return LRESULT(0),
            }
        }

        // 3. Save shortcut: Ctrl+S
        if self.mode == OverlayMode::SelectionActive && ctrl_down && vk == 'S' as usize {
            match self.save_selection_to_file() {
                Ok(path) => {
                    if self.settings.notify_after_save {
                        crate::tray::show_notification(
                            "Screenshot saved",
                            &path.display().to_string(),
                        );
                    }
                    if self.settings.close_after_action {
                        let _ = unsafe { DestroyWindow(hwnd) };
                    } else {
                        self.redraw(hwnd);
                    }
                }
                Err(error) => Self::show_action_error(hwnd, "Save", &error),
            }
            return LRESULT(0);
        }

        // 4. Settings shortcut: Ctrl+,
        if ctrl_down && vk == VK_OEM_COMMA.0 as usize {
            match show_settings_dialog(&self.settings) {
                Ok(Some(new_cfg)) => {
                    self.active_color = new_cfg.default_color;
                    self.active_thickness = new_cfg.default_thickness;
                    self.settings = new_cfg;
                    self.redraw(hwnd);
                }
                Ok(None) => {}
                Err(e) => {
                    eprintln!("[isolmaSS] Settings dialog failed: {e}");
                }
            }
            return LRESULT(0);
        }

        // 5. Copy shortcut: Ctrl+C or Enter
        if self.mode == OverlayMode::SelectionActive
            && ((ctrl_down && vk == 'C' as usize) || vk == VK_RETURN.0 as usize)
        {
            match self.copy_selection_to_clipboard(hwnd) {
                Ok(()) => {
                    if self.settings.close_after_action {
                        let _ = unsafe { DestroyWindow(hwnd) };
                    } else {
                        self.redraw(hwnd);
                    }
                }
                Err(error) => Self::show_action_error(hwnd, "Copy", &error),
            }
            return LRESULT(0);
        }

        // 6. Undo: Ctrl+Z
        if ctrl_down && vk == 'Z' as usize {
            self.history.undo(&mut self.objects);
            self.selected_id = None;
            self.redraw(hwnd);
            return LRESULT(0);
        }

        // 7. Redo: Ctrl+Y
        if ctrl_down && vk == 'Y' as usize {
            self.history.redo(&mut self.objects);
            self.selected_id = None;
            self.redraw(hwnd);
            return LRESULT(0);
        }

        // 8. Delete / Backspace selected object
        if (vk == VK_DELETE.0 as usize || vk == VK_BACK.0 as usize)
            && let Some(pos) = self
                .selected_id
                .take()
                .and_then(|id| self.objects.iter().position(|o| o.id == id))
        {
            let removed = self.objects.remove(pos);
            self.history.record(EditCommand::Delete(removed));
            self.redraw(hwnd);
            return LRESULT(0);
        }

        // 9. Tool switching shortcuts (R, A, P, T, B)
        if !ctrl_down {
            match vk as u8 as char {
                'R' | 'r' => {
                    self.active_tool = ToolKind::Rectangle;
                    self.persist_editor_preferences();
                    self.selected_id = None;
                    self.redraw(hwnd);
                    return LRESULT(0);
                }
                'A' | 'a' => {
                    self.active_tool = ToolKind::Arrow;
                    self.persist_editor_preferences();
                    self.selected_id = None;
                    self.redraw(hwnd);
                    return LRESULT(0);
                }
                'P' | 'p' => {
                    self.active_tool = ToolKind::Pen;
                    self.persist_editor_preferences();
                    self.selected_id = None;
                    self.redraw(hwnd);
                    return LRESULT(0);
                }
                'T' | 't' => {
                    self.active_tool = ToolKind::Text;
                    self.persist_editor_preferences();
                    self.selected_id = None;
                    self.redraw(hwnd);
                    return LRESULT(0);
                }
                'B' | 'b' => {
                    self.active_tool = ToolKind::Blur;
                    self.persist_editor_preferences();
                    self.selected_id = None;
                    self.redraw(hwnd);
                    return LRESULT(0);
                }
                _ => {}
            }
        }

        LRESULT(0)
    }

    fn handle_char(&mut self, hwnd: HWND, ch_code: u32) -> LRESULT {
        if let Some(edit) = &mut self.text_edit
            && let Some(ch) = char::from_u32(ch_code)
            && (!ch.is_control() || ch == '\t')
        {
            edit.insert_char(ch);
            self.redraw(hwnd);
            return LRESULT(0);
        }
        LRESULT(0)
    }

    fn redraw(&mut self, hwnd: HWND) {
        self.composite_scene();
        unsafe {
            let _ = InvalidateRect(hwnd, None, false);
        }
    }

    fn redraw_region(&mut self, hwnd: HWND, _region: Rect) {
        // Selection movement also relocates annotations and both floating panels. A full
        // frame invalidation is deterministic even when third-party overlays hook GDI.
        self.redraw(hwnd);
    }

    fn commit_selection(&mut self, hwnd: HWND, rect: Rect) {
        self.mode = OverlayMode::SelectionActive;
        self.committed_selection = Some(rect);
        self.hover_snap_rect = None;
        self.selected_id = None;
        self.redraw(hwnd);
    }

    fn show_action_error(hwnd: HWND, action: &str, error: &str) {
        let title: Vec<u16> = format!("isolmaSS - {action} failed\0")
            .encode_utf16()
            .collect();
        let message: Vec<u16> = format!("{error}\0").encode_utf16().collect();
        unsafe {
            let _ = MessageBoxW(
                hwnd,
                PCWSTR(message.as_ptr()),
                PCWSTR(title.as_ptr()),
                MB_OK | MB_ICONERROR,
            );
        }
    }

    fn copy_selection_to_clipboard(&mut self, hwnd: HWND) -> std::result::Result<(), String> {
        let sel = self
            .committed_selection
            .ok_or_else(|| "No screenshot region is selected.".to_string())?;

        let prev_selected = self.selected_id.take();
        let prev_text = self.text_edit.take();
        self.composite_scene();

        let width = self.capture.width;
        let height = self.capture.height;
        let len = (width as usize) * (height as usize) * 4;
        let buffer = unsafe { std::slice::from_raw_parts(self.bits_ptr, len) };
        let result = flatten_selection_to_dib(buffer, width, height, &sel)
            .map_err(|error| format!("Could not prepare the clipboard image: {error}"))
            .and_then(|dib| {
                copy_dib_to_clipboard(Some(hwnd), &dib)
                    .map_err(|error| format!("Windows rejected the clipboard image: {error}"))
            });

        self.selected_id = prev_selected;
        self.text_edit = prev_text;
        if result.is_ok() {
            self.committed_result = true;
        }
        result
    }

    fn save_selection_to_file(&mut self) -> std::result::Result<std::path::PathBuf, String> {
        let sel = self
            .committed_selection
            .ok_or_else(|| "No screenshot region is selected.".to_string())?;

        let prev_selected = self.selected_id.take();
        let prev_text = self.text_edit.take();
        self.composite_scene();

        let width = self.capture.width;
        let height = self.capture.height;
        let len = (width as usize) * (height as usize) * 4;
        let buffer = unsafe { std::slice::from_raw_parts(self.bits_ptr, len) };
        let result = save_screenshot(
            buffer,
            width,
            height,
            &sel,
            Some(&self.settings.save_directory),
            self.settings.save_format,
            self.settings.jpeg_quality,
        );

        self.selected_id = prev_selected;
        self.text_edit = prev_text;
        if result.is_ok() {
            self.committed_result = true;
        }
        result
    }

    fn persist_editor_preferences(&mut self) {
        self.settings.default_color = self.active_color;
        self.settings.default_thickness = self.active_thickness;
        self.settings.last_tool = self.active_tool;
        if let Err(error) = self.settings.save() {
            eprintln!("[isolmaSS] Could not persist editor preferences: {error}");
        }
    }
}

unsafe extern "system" fn overlay_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let state_ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *mut OverlayState;

    match msg {
        WM_MOUSEACTIVATE => LRESULT(MA_ACTIVATE as isize),

        WM_SETCURSOR => {
            if !state_ptr.is_null() {
                let state = unsafe { &*state_ptr };

                let mut pt_screen = windows::Win32::Foundation::POINT::default();
                unsafe {
                    let _ = GetCursorPos(&mut pt_screen);
                }
                let mut pt_client = pt_screen;
                unsafe {
                    let _ = ScreenToClient(hwnd, &mut pt_client);
                }
                let pt = (pt_client.x, pt_client.y);

                let cur_id = if let Some(tb) = &state.toolbar {
                    if tb.contains_point(pt) {
                        if tb.hit_test(pt).is_some() {
                            IDC_HAND
                        } else {
                            IDC_ARROW
                        }
                    } else if state.mode == OverlayMode::SelectionActive {
                        if let Some(sel) = state.committed_selection {
                            match sel.hit_test_selection(
                                pt,
                                4 * state.dpi as i32 / 96,
                                8 * state.dpi as i32 / 96,
                            ) {
                                SelectionHitZone::TopLeftCorner
                                | SelectionHitZone::BottomRightCorner => IDC_SIZENWSE,
                                SelectionHitZone::TopRightCorner
                                | SelectionHitZone::BottomLeftCorner => IDC_SIZENESW,
                                SelectionHitZone::BorderEdge => IDC_SIZEALL,
                                SelectionHitZone::Interior => {
                                    if state.text_edit.is_some() {
                                        IDC_IBEAM
                                    } else if let Some(hit_obj) =
                                        state.objects.iter().rev().find(|o| o.hit_test(pt))
                                    {
                                        if let AnnotationKind::Text { .. } = hit_obj.kind {
                                            IDC_IBEAM
                                        } else {
                                            IDC_SIZEALL
                                        }
                                    } else if state.active_tool == ToolKind::Text {
                                        IDC_IBEAM
                                    } else {
                                        IDC_CROSS
                                    }
                                }
                                SelectionHitZone::None => IDC_CROSS,
                            }
                        } else {
                            IDC_CROSS
                        }
                    } else {
                        IDC_CROSS
                    }
                } else if state.mode == OverlayMode::SelectionActive {
                    if let Some(sel) = state.committed_selection {
                        match sel.hit_test_selection(
                            pt,
                            4 * state.dpi as i32 / 96,
                            8 * state.dpi as i32 / 96,
                        ) {
                            SelectionHitZone::TopLeftCorner
                            | SelectionHitZone::BottomRightCorner => IDC_SIZENWSE,
                            SelectionHitZone::TopRightCorner
                            | SelectionHitZone::BottomLeftCorner => IDC_SIZENESW,
                            SelectionHitZone::BorderEdge => IDC_SIZEALL,
                            SelectionHitZone::Interior => {
                                if state.text_edit.is_some() {
                                    IDC_IBEAM
                                } else if let Some(hit_obj) =
                                    state.objects.iter().rev().find(|o| o.hit_test(pt))
                                {
                                    if let AnnotationKind::Text { .. } = hit_obj.kind {
                                        IDC_IBEAM
                                    } else {
                                        IDC_SIZEALL
                                    }
                                } else if state.active_tool == ToolKind::Text {
                                    IDC_IBEAM
                                } else {
                                    IDC_CROSS
                                }
                            }
                            SelectionHitZone::None => IDC_CROSS,
                        }
                    } else {
                        IDC_CROSS
                    }
                } else {
                    IDC_CROSS
                };

                let cur = unsafe { LoadCursorW(HINSTANCE::default(), cur_id).unwrap_or_default() };
                unsafe {
                    SetCursor(cur);
                }
                return LRESULT(1);
            }
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }

        WM_DPICHANGED => {
            if !state_ptr.is_null() {
                let state = unsafe { &mut *state_ptr };
                state.dpi = (wparam.0 & 0xffff) as u32;
                state.redraw(hwnd);
            }
            LRESULT(0)
        }

        WM_TIMER => {
            if !state_ptr.is_null() {
                let state = unsafe { &mut *state_ptr };
                if let Some(edit) = &mut state.text_edit {
                    edit.caret_visible = !edit.caret_visible;
                    state.redraw(hwnd);
                }
            }
            LRESULT(0)
        }

        WM_LBUTTONDBLCLK => {
            if !state_ptr.is_null() {
                let state = unsafe { &mut *state_ptr };
                let client_x = (lparam.0 as i32) as i16 as i32;
                let client_y = ((lparam.0 >> 16) as i32) as i16 as i32;
                let pt = (client_x, client_y);

                unsafe {
                    let _ = SetForegroundWindow(hwnd);
                    let _ = SetFocus(hwnd);
                }

                if state.mode == OverlayMode::SelectionActive {
                    // Double-click on existing committed text object: re-opens it in text editing mode with caret at the end!
                    if let Some(pos) = state.objects.iter().rposition(|o| {
                        if let AnnotationKind::Text { .. } = o.kind {
                            o.hit_test(pt)
                        } else {
                            false
                        }
                    }) {
                        let obj = state.objects.remove(pos);
                        if let AnnotationKind::Text {
                            pos: t_pos,
                            text,
                            color,
                            font_size,
                        } = obj.kind
                        {
                            state.text_edit = Some(TextEditState::new(
                                t_pos,
                                text,
                                color,
                                font_size,
                                Some(obj.id),
                            ));
                            unsafe {
                                let _ = SetTimer(hwnd, 1, 500, None);
                            }
                            set_overlay_text_editing(true);
                            state.selected_id = None;
                            state.redraw(hwnd);
                            return LRESULT(0);
                        }
                    }
                }
            }
            LRESULT(0)
        }

        WM_PAINT => {
            if !state_ptr.is_null() {
                let state = unsafe { &*state_ptr };
                let mut ps = windows::Win32::Graphics::Gdi::PAINTSTRUCT::default();
                let hdc = unsafe { BeginPaint(hwnd, &mut ps) };

                let dirty_w = ps.rcPaint.right - ps.rcPaint.left;
                let dirty_h = ps.rcPaint.bottom - ps.rcPaint.top;

                if dirty_w > 0 && dirty_h > 0 {
                    let _ = unsafe {
                        BitBlt(
                            hdc,
                            ps.rcPaint.left,
                            ps.rcPaint.top,
                            dirty_w,
                            dirty_h,
                            state.mem_dc,
                            ps.rcPaint.left,
                            ps.rcPaint.top,
                            SRCCOPY,
                        )
                    };
                }

                let _ = unsafe { EndPaint(hwnd, &ps) };
                return LRESULT(0);
            }
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }

        WM_ERASEBKGND => LRESULT(1),

        WM_MOUSEMOVE => {
            if !state_ptr.is_null() {
                let state = unsafe { &mut *state_ptr };
                let client_x = (lparam.0 as i32) as i16 as i32;
                let client_y = ((lparam.0 >> 16) as i32) as i16 as i32;
                let pt = (client_x, client_y);

                let shift_down = (unsafe { GetKeyState(VK_SHIFT.0 as i32) } as u16 & 0x8000) != 0;

                match state.mode {
                    OverlayMode::Hovering => {
                        if state.settings.enable_window_snap {
                            let screen_x = client_x + state.capture.x;
                            let screen_y = client_y + state.capture.y;

                            let new_snap = if let Some(win) =
                                find_window_in_list(&state.visible_windows, (screen_x, screen_y))
                            {
                                Some(
                                    Rect::new(
                                        win.bounds.left - state.capture.x,
                                        win.bounds.top - state.capture.y,
                                        win.bounds.right - state.capture.x,
                                        win.bounds.bottom - state.capture.y,
                                    )
                                    .clamp(state.capture.width, state.capture.height),
                                )
                            } else {
                                None
                            };

                            if state.hover_snap_rect != new_snap {
                                let old_snap = state.hover_snap_rect;
                                state.hover_snap_rect = new_snap;
                                let dirty = match (old_snap, new_snap) {
                                    (Some(old), Some(new)) => old.union(&new),
                                    (Some(old), None) => old,
                                    (None, Some(new)) => new,
                                    (None, None) => Rect::default(),
                                };
                                state.redraw_region(hwnd, dirty.inflate(4, 4));
                            }
                        } else if let Some(old_snap) = state.hover_snap_rect.take() {
                            state.redraw_region(hwnd, old_snap.inflate(4, 4));
                        }
                    }
                    OverlayMode::DraggingSelection => {
                        if let Some(start) = state.drag_start {
                            let adjusted_pt = if shift_down {
                                snap_square(start, pt)
                            } else {
                                pt
                            };
                            let new_sel = Rect::normalized(start, adjusted_pt)
                                .clamp(state.capture.width, state.capture.height);
                            let dirty = state
                                .committed_selection
                                .map_or(new_sel, |old| old.union(&new_sel))
                                .inflate(4, 4);
                            state.committed_selection = Some(new_sel);
                            state.redraw_region(hwnd, dirty);
                        }
                    }
                    OverlayMode::SelectionActive => {
                        // 1. Dragging selection (Move or Resize)
                        if let Some(drag) = state.dragging_selection {
                            let dx = client_x - drag.last_pos.0;
                            let dy = client_y - drag.last_pos.1;
                            let max_w = state.capture.width;
                            let max_h = state.capture.height;
                            let old_selection = state.committed_selection;

                            if let Some(mut sel) = state.committed_selection {
                                match drag.action {
                                    DragSelectionAction::Move => {
                                        let w = sel.width();
                                        let h = sel.height();
                                        let target_left = (sel.left + dx).clamp(0, max_w - w);
                                        let target_top = (sel.top + dy).clamp(0, max_h - h);
                                        let actual_dx = target_left - sel.left;
                                        let actual_dy = target_top - sel.top;

                                        if actual_dx != 0 || actual_dy != 0 {
                                            sel.left += actual_dx;
                                            sel.right += actual_dx;
                                            sel.top += actual_dy;
                                            sel.bottom += actual_dy;
                                            state.committed_selection = Some(sel);

                                            for obj in &mut state.objects {
                                                obj.translate(actual_dx, actual_dy);
                                            }
                                        }
                                    }
                                    DragSelectionAction::Resize(zone) => {
                                        match zone {
                                            SelectionHitZone::TopLeftCorner => {
                                                sel.left = client_x.clamp(0, sel.right - 8);
                                                sel.top = client_y.clamp(0, sel.bottom - 8);
                                            }
                                            SelectionHitZone::TopRightCorner => {
                                                sel.right = client_x.clamp(sel.left + 8, max_w);
                                                sel.top = client_y.clamp(0, sel.bottom - 8);
                                            }
                                            SelectionHitZone::BottomLeftCorner => {
                                                sel.left = client_x.clamp(0, sel.right - 8);
                                                sel.bottom = client_y.clamp(sel.top + 8, max_h);
                                            }
                                            SelectionHitZone::BottomRightCorner => {
                                                sel.right = client_x.clamp(sel.left + 8, max_w);
                                                sel.bottom = client_y.clamp(sel.top + 8, max_h);
                                            }
                                            _ => {}
                                        }
                                        state.committed_selection = Some(sel);
                                    }
                                }
                            }

                            state.dragging_selection = Some(DragSelectionState {
                                action: drag.action,
                                start_pos: drag.start_pos,
                                last_pos: pt,
                            });
                            let dirty = match (old_selection, state.committed_selection) {
                                (Some(old), Some(new)) => old.union(&new).inflate(8, 8),
                                (Some(old), None) => old.inflate(8, 8),
                                (None, Some(new)) => new.inflate(8, 8),
                                (None, None) => Rect::default(),
                            };
                            state.redraw_region(hwnd, dirty);
                            return LRESULT(0);
                        }

                        // 2. Dragging object
                        if let Some(drag) = state.dragging_object {
                            let dx = client_x - drag.last_pos.0;
                            let dy = client_y - drag.last_pos.1;
                            let dirty = if let Some(obj) =
                                state.objects.iter_mut().find(|object| object.id == drag.id)
                            {
                                let old_bounds = obj.bounds();
                                obj.translate(dx, dy);
                                old_bounds.union(&obj.bounds()).inflate(6, 6)
                            } else {
                                Rect::default()
                            };
                            state.dragging_object = Some(DragObjectState {
                                id: drag.id,
                                start_pos: drag.start_pos,
                                last_pos: pt,
                            });
                            state.redraw_region(hwnd, dirty);
                            return LRESULT(0);
                        }

                        // 3. Drawing in-progress shape
                        if let Some(shape) = &mut state.drawing_shape {
                            match shape {
                                InProgressDrawing::Rectangle { start, current } => {
                                    *current = if shift_down {
                                        snap_square(*start, pt)
                                    } else {
                                        pt
                                    };
                                }
                                InProgressDrawing::Arrow { start, current } => {
                                    *current = if shift_down {
                                        snap_angle_45(*start, pt)
                                    } else {
                                        pt
                                    };
                                }
                                InProgressDrawing::Pen { points } => points.push(pt),
                                InProgressDrawing::Blur { start, current } => {
                                    *current = if shift_down {
                                        snap_square(*start, pt)
                                    } else {
                                        pt
                                    };
                                }
                            }
                            state.redraw(hwnd);
                            return LRESULT(0);
                        }

                        // 4. Toolbar hover update
                        if state.toolbar.as_mut().is_some_and(|tb| tb.update_hover(pt)) {
                            state.redraw(hwnd);
                        }
                    }
                }
                return LRESULT(0);
            }
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }

        WM_LBUTTONDOWN => {
            if !state_ptr.is_null() {
                let state = unsafe { &mut *state_ptr };
                let client_x = (lparam.0 as i32) as i16 as i32;
                let client_y = ((lparam.0 >> 16) as i32) as i16 as i32;
                let pt = (client_x, client_y);

                unsafe {
                    let _ = SetForegroundWindow(hwnd);
                    let _ = SetFocus(hwnd);
                    SetCapture(hwnd);
                }

                match state.mode {
                    OverlayMode::Hovering => {
                        state.drag_start = Some(pt);
                        state.mode = OverlayMode::DraggingSelection;
                    }
                    OverlayMode::DraggingSelection => {}
                    OverlayMode::SelectionActive => {
                        // 1. Toolbar button click
                        if let Some(item) = state.toolbar.as_ref().and_then(|tb| tb.hit_test(pt)) {
                            state.commit_text(hwnd);

                            match item {
                                ToolbarItem::Tool(k) => {
                                    state.active_tool = k;
                                    state.persist_editor_preferences();
                                    state.selected_id = None;
                                    state.redraw(hwnd);
                                }
                                ToolbarItem::Action(ToolbarAction::Undo) => {
                                    state.history.undo(&mut state.objects);
                                    state.selected_id = None;
                                    state.redraw(hwnd);
                                }
                                ToolbarItem::Action(ToolbarAction::Redo) => {
                                    state.history.redo(&mut state.objects);
                                    state.selected_id = None;
                                    state.redraw(hwnd);
                                }
                                ToolbarItem::Action(ToolbarAction::Save) => {
                                    match state.save_selection_to_file() {
                                        Ok(path) => {
                                            if state.settings.notify_after_save {
                                                crate::tray::show_notification(
                                                    "Screenshot saved",
                                                    &path.display().to_string(),
                                                );
                                            }
                                            if state.settings.close_after_action {
                                                let _ = unsafe { DestroyWindow(hwnd) };
                                            } else {
                                                state.redraw(hwnd);
                                            }
                                        }
                                        Err(error) => {
                                            OverlayState::show_action_error(hwnd, "Save", &error)
                                        }
                                    }
                                }
                                ToolbarItem::Action(ToolbarAction::Copy) => {
                                    match state.copy_selection_to_clipboard(hwnd) {
                                        Ok(()) => {
                                            if state.settings.close_after_action {
                                                let _ = unsafe { DestroyWindow(hwnd) };
                                            } else {
                                                state.redraw(hwnd);
                                            }
                                        }
                                        Err(error) => {
                                            OverlayState::show_action_error(hwnd, "Copy", &error)
                                        }
                                    }
                                }
                                ToolbarItem::Action(ToolbarAction::Settings) => {
                                    match show_settings_dialog(&state.settings) {
                                        Ok(Some(new_cfg)) => {
                                            state.active_color = new_cfg.default_color;
                                            state.active_thickness = new_cfg.default_thickness;
                                            state.settings = new_cfg;
                                            state.redraw(hwnd);
                                        }
                                        Ok(None) => {}
                                        Err(e) => {
                                            eprintln!("[isolmaSS] Settings dialog error: {e}");
                                        }
                                    }
                                }
                                ToolbarItem::Action(ToolbarAction::Cancel) => {
                                    state.mode = OverlayMode::Hovering;
                                    state.committed_selection = None;
                                    state.toolbar = None;
                                    state.objects.clear();
                                    state.selected_id = None;
                                    state.redraw(hwnd);
                                }
                                ToolbarItem::Color(col) => {
                                    state.active_color = col;
                                    if let Some(obj) = state.selected_id.and_then(|id| {
                                        state.objects.iter_mut().find(|o| o.id == id)
                                    }) {
                                        let old_kind = obj.kind.clone();
                                        obj.set_color(col);
                                        let new_kind = obj.kind.clone();
                                        state.history.record(EditCommand::Modify {
                                            id: obj.id,
                                            old_kind,
                                            new_kind,
                                        });
                                    }
                                    state.persist_editor_preferences();
                                    state.redraw(hwnd);
                                }
                                ToolbarItem::Thickness(thick) => {
                                    state.active_thickness = thick;
                                    if let Some(obj) = state.selected_id.and_then(|id| {
                                        state.objects.iter_mut().find(|o| o.id == id)
                                    }) {
                                        let old_kind = obj.kind.clone();
                                        obj.set_thickness(thick);
                                        let new_kind = obj.kind.clone();
                                        state.history.record(EditCommand::Modify {
                                            id: obj.id,
                                            old_kind,
                                            new_kind,
                                        });
                                    }
                                    state.persist_editor_preferences();
                                    state.redraw(hwnd);
                                }
                            }
                            return LRESULT(0);
                        }

                        // If text edit active, commit it when clicked elsewhere
                        if state.text_edit.is_some() {
                            state.commit_text(hwnd);
                        }

                        let Some(sel) = state.committed_selection else {
                            return LRESULT(0);
                        };

                        let zone = sel.hit_test_selection(
                            pt,
                            4 * state.dpi as i32 / 96,
                            8 * state.dpi as i32 / 96,
                        );
                        match zone {
                            SelectionHitZone::TopLeftCorner
                            | SelectionHitZone::TopRightCorner
                            | SelectionHitZone::BottomLeftCorner
                            | SelectionHitZone::BottomRightCorner => {
                                state.dragging_selection = Some(DragSelectionState {
                                    action: DragSelectionAction::Resize(zone),
                                    start_pos: pt,
                                    last_pos: pt,
                                });
                                state.selected_id = None;
                                state.redraw(hwnd);
                                return LRESULT(0);
                            }
                            SelectionHitZone::BorderEdge => {
                                state.dragging_selection = Some(DragSelectionState {
                                    action: DragSelectionAction::Move,
                                    start_pos: pt,
                                    last_pos: pt,
                                });
                                state.selected_id = None;
                                state.redraw(hwnd);
                                return LRESULT(0);
                            }
                            SelectionHitZone::Interior => {
                                // 2. Click existing object
                                let hit_obj_id = state
                                    .objects
                                    .iter()
                                    .rev()
                                    .find(|o| o.hit_test(pt))
                                    .map(|o| o.id);

                                if let Some(id) = hit_obj_id {
                                    state.selected_id = Some(id);
                                    state.dragging_object = Some(DragObjectState {
                                        id,
                                        start_pos: pt,
                                        last_pos: pt,
                                    });
                                    state.redraw(hwnd);
                                    return LRESULT(0);
                                }

                                // 3. Click empty canvas inside selection
                                state.selected_id = None;

                                match state.active_tool {
                                    ToolKind::Rectangle => {
                                        state.drawing_shape = Some(InProgressDrawing::Rectangle {
                                            start: pt,
                                            current: pt,
                                        });
                                    }
                                    ToolKind::Arrow => {
                                        state.drawing_shape = Some(InProgressDrawing::Arrow {
                                            start: pt,
                                            current: pt,
                                        });
                                    }
                                    ToolKind::Pen => {
                                        state.drawing_shape =
                                            Some(InProgressDrawing::Pen { points: vec![pt] });
                                    }
                                    ToolKind::Text => {
                                        state.text_edit = Some(TextEditState::new(
                                            pt,
                                            String::new(),
                                            state.active_color,
                                            DEFAULT_FONT_SIZE * state.dpi as i32 / 96,
                                            None,
                                        ));
                                        unsafe {
                                            let _ = SetTimer(hwnd, 1, 500, None);
                                        }
                                        set_overlay_text_editing(true);
                                        state.redraw(hwnd);
                                    }
                                    ToolKind::Blur => {
                                        state.drawing_shape = Some(InProgressDrawing::Blur {
                                            start: pt,
                                            current: pt,
                                        });
                                    }
                                }
                            }
                            SelectionHitZone::None => {
                                if state.selected_id.is_some() {
                                    state.selected_id = None;
                                    state.redraw(hwnd);
                                } else {
                                    state.mode = OverlayMode::DraggingSelection;
                                    state.drag_start = Some(pt);
                                    state.committed_selection = None;
                                    state.toolbar = None;
                                    state.objects.clear();
                                    state.redraw(hwnd);
                                }
                            }
                        }
                    }
                }
                return LRESULT(0);
            }
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }

        WM_LBUTTONUP => {
            if !state_ptr.is_null() {
                let state = unsafe { &mut *state_ptr };
                let client_x = (lparam.0 as i32) as i16 as i32;
                let client_y = ((lparam.0 >> 16) as i32) as i16 as i32;
                let pt = (client_x, client_y);

                let _ = unsafe { ReleaseCapture() };
                let shift_down = (unsafe { GetKeyState(VK_SHIFT.0 as i32) } as u16 & 0x8000) != 0;

                match state.mode {
                    OverlayMode::DraggingSelection => {
                        if let Some(start) = state.drag_start.take() {
                            let dx = (client_x - start.0).abs();
                            let dy = (client_y - start.1).abs();

                            if dx <= 3 && dy <= 3 {
                                if state.settings.enable_window_snap {
                                    let screen_x = client_x + state.capture.x;
                                    let screen_y = client_y + state.capture.y;

                                    if let Some(win) = find_window_in_list(
                                        &state.visible_windows,
                                        (screen_x, screen_y),
                                    ) {
                                        let snap_rect = Rect::new(
                                            win.bounds.left - state.capture.x,
                                            win.bounds.top - state.capture.y,
                                            win.bounds.right - state.capture.x,
                                            win.bounds.bottom - state.capture.y,
                                        )
                                        .clamp(state.capture.width, state.capture.height);

                                        state.commit_selection(hwnd, snap_rect);
                                    } else {
                                        state.mode = OverlayMode::Hovering;
                                        state.redraw(hwnd);
                                    }
                                } else {
                                    state.mode = OverlayMode::Hovering;
                                    state.redraw(hwnd);
                                }
                            } else {
                                let final_pt = if shift_down {
                                    snap_square(start, pt)
                                } else {
                                    pt
                                };
                                let drag_rect = Rect::normalized(start, final_pt)
                                    .clamp(state.capture.width, state.capture.height);

                                if drag_rect.width() > 4 && drag_rect.height() > 4 {
                                    state.commit_selection(hwnd, drag_rect);
                                } else {
                                    state.mode = OverlayMode::Hovering;
                                    state.redraw(hwnd);
                                }
                            }
                        }
                    }
                    OverlayMode::SelectionActive => {
                        if let Some(drag) = state.dragging_selection.take() {
                            if let DragSelectionAction::Move = drag.action {
                                let total_dx = drag.last_pos.0 - drag.start_pos.0;
                                let total_dy = drag.last_pos.1 - drag.start_pos.1;
                                if total_dx != 0 || total_dy != 0 {
                                    // Smoothly moved
                                }
                            }
                            state.redraw(hwnd);
                            return LRESULT(0);
                        }

                        if let Some(drag) = state.dragging_object.take() {
                            let total_dx = drag.last_pos.0 - drag.start_pos.0;
                            let total_dy = drag.last_pos.1 - drag.start_pos.1;
                            if total_dx != 0 || total_dy != 0 {
                                state.history.record(EditCommand::Move {
                                    id: drag.id,
                                    dx: total_dx,
                                    dy: total_dy,
                                });
                            }
                            state.selected_id = Some(drag.id);
                            state.redraw(hwnd);
                            return LRESULT(0);
                        }

                        if let Some(shape) = state.drawing_shape.take() {
                            let new_obj = match shape {
                                InProgressDrawing::Rectangle { start, current } => {
                                    let final_cur = if shift_down {
                                        snap_square(start, current)
                                    } else {
                                        current
                                    };
                                    let r = Rect::normalized(start, final_cur)
                                        .clamp(state.capture.width, state.capture.height);
                                    if r.width() >= 3 && r.height() >= 3 {
                                        Some(AnnotationObject::new(
                                            state.next_id,
                                            AnnotationKind::Rectangle {
                                                rect: r,
                                                color: state.active_color,
                                                thickness: state.active_thickness,
                                            },
                                        ))
                                    } else {
                                        None
                                    }
                                }
                                InProgressDrawing::Arrow { start, current } => {
                                    let final_cur = if shift_down {
                                        snap_angle_45(start, current)
                                    } else {
                                        current
                                    };
                                    let dx = (final_cur.0 - start.0).abs();
                                    let dy = (final_cur.1 - start.1).abs();
                                    if dx >= 3 || dy >= 3 {
                                        Some(AnnotationObject::new(
                                            state.next_id,
                                            AnnotationKind::Arrow {
                                                start,
                                                end: final_cur,
                                                color: state.active_color,
                                                thickness: state.active_thickness,
                                            },
                                        ))
                                    } else {
                                        None
                                    }
                                }
                                InProgressDrawing::Pen { points } => {
                                    let min_x = points.iter().map(|p| p.0).min().unwrap_or(0);
                                    let max_x = points.iter().map(|p| p.0).max().unwrap_or(0);
                                    let min_y = points.iter().map(|p| p.1).min().unwrap_or(0);
                                    let max_y = points.iter().map(|p| p.1).max().unwrap_or(0);
                                    if points.len() >= 2
                                        && ((max_x - min_x) >= 3 || (max_y - min_y) >= 3)
                                    {
                                        Some(AnnotationObject::new(
                                            state.next_id,
                                            AnnotationKind::Pen {
                                                points,
                                                color: state.active_color,
                                                thickness: state.active_thickness,
                                            },
                                        ))
                                    } else {
                                        None
                                    }
                                }
                                InProgressDrawing::Blur { start, current } => {
                                    let final_cur = if shift_down {
                                        snap_square(start, current)
                                    } else {
                                        current
                                    };
                                    let r = Rect::normalized(start, final_cur)
                                        .clamp(state.capture.width, state.capture.height);
                                    if r.width() >= 3 && r.height() >= 3 {
                                        Some(AnnotationObject::new(
                                            state.next_id,
                                            AnnotationKind::Blur {
                                                rect: r,
                                                block_size: DEFAULT_BLUR_BLOCK,
                                            },
                                        ))
                                    } else {
                                        None
                                    }
                                }
                            };

                            if let Some(obj) = new_obj {
                                state.next_id += 1;
                                state.history.record(EditCommand::Add(obj.clone()));
                                state.selected_id = Some(obj.id);
                                state.objects.push(obj);
                            }

                            state.redraw(hwnd);
                        }
                    }
                    _ => {}
                }
                return LRESULT(0);
            }
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }

        WM_OVERLAY_CHAR | WM_CHAR => {
            if !state_ptr.is_null() {
                let state = unsafe { &mut *state_ptr };
                return state.handle_char(hwnd, wparam.0 as u32);
            }
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }

        WM_OVERLAY_KEYDOWN | WM_KEYDOWN => {
            if !state_ptr.is_null() {
                let state = unsafe { &mut *state_ptr };
                return state.handle_key_down(hwnd, wparam.0, lparam.0);
            }
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }

        WM_RBUTTONDOWN => {
            unsafe {
                let _ = SetForegroundWindow(hwnd);
                let _ = SetFocus(hwnd);
            }
            LRESULT(0)
        }

        WM_RBUTTONUP => {
            if !state_ptr.is_null() {
                let state = unsafe { &mut *state_ptr };
                match state.handle_escape_action() {
                    EscapeAction::CancelledTextEdit => {
                        unsafe {
                            let _ = KillTimer(hwnd, 1);
                        }
                        set_overlay_text_editing(false);
                        state.redraw(hwnd);
                        return LRESULT(0);
                    }
                    EscapeAction::DeselectedObject(_) | EscapeAction::CancelledSelection => {
                        state.redraw(hwnd);
                        return LRESULT(0);
                    }
                    EscapeAction::CloseOverlay => {
                        let _ = unsafe { DestroyWindow(hwnd) };
                        return LRESULT(0);
                    }
                }
            }
            let _ = unsafe { DestroyWindow(hwnd) };
            LRESULT(0)
        }

        WM_CLOSE => {
            let _ = unsafe { DestroyWindow(hwnd) };
            LRESULT(0)
        }

        WM_DESTROY => {
            unregister_overlay();
            unsafe {
                PostQuitMessage(0);
            }
            LRESULT(0)
        }

        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}
