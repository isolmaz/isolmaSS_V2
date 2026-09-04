use crate::annotation::{
    snap_angle_45, snap_square, AnnotationKind, AnnotationObject, EditCommand, HistoryManager,
    ToolKind,
};
use crate::capture::{CaptureBuffer, Rect};
use crate::clipboard::{copy_dib_to_clipboard, flatten_selection_to_dib};
use crate::save::save_screenshot;
use crate::settings::{show_settings_dialog, Settings};
use crate::toolbar::{Toolbar, ToolbarAction, ToolbarItem};
use crate::window_snap::find_window_at_point;
use std::ffi::c_void;
use std::sync::Arc;
use windows::core::{w, Result};
use windows::Win32::Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, BitBlt, CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, EndPaint,
    GetDC, InvalidateRect, ReleaseDC, SelectObject, SetBkMode, SetTextColor, TextOutW, BITMAPINFO,
    BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HBITMAP, HDC, HGDIOBJ, RGBQUAD, SRCCOPY,
    TRANSPARENT,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyState, ReleaseCapture, SetCapture, SetFocus, VK_BACK, VK_CONTROL, VK_DELETE, VK_ESCAPE,
    VK_OEM_COMMA, VK_RETURN, VK_SHIFT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW,
    GetWindowLongPtrW, LoadCursorW, PostQuitMessage, RegisterClassExW, SetCursor,
    SetForegroundWindow, SetLayeredWindowAttributes, SetWindowLongPtrW, ShowWindow,
    TranslateMessage, GWLP_USERDATA, IDC_CROSS, IDC_HAND, LWA_ALPHA, MA_ACTIVATE, MSG, SW_SHOW,
    WM_CHAR, WM_CLOSE, WM_DESTROY, WM_ERASEBKGND, WM_KEYDOWN, WM_LBUTTONDOWN, WM_LBUTTONUP,
    WM_MOUSEACTIVATE, WM_MOUSEMOVE, WM_PAINT, WM_RBUTTONUP, WM_SETCURSOR, WNDCLASSEXW,
    WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};

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
    Rectangle { start: (i32, i32), current: (i32, i32) },
    Arrow { start: (i32, i32), current: (i32, i32) },
    Pen { points: Vec<(i32, i32)> },
    Blur { start: (i32, i32), current: (i32, i32) },
}

#[derive(Debug, Clone)]
struct TextEditState {
    pos: (i32, i32),
    text: String,
}

#[derive(Debug, Clone, Copy)]
struct DragObjectState {
    id: usize,
    start_pos: (i32, i32),
    last_pos: (i32, i32),
}

struct OverlayState {
    capture: Arc<CaptureBuffer>,
    mode: OverlayMode,
    drag_start: Option<(i32, i32)>,
    committed_selection: Option<Rect>,
    hover_snap_rect: Option<Rect>,

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
        unsafe {
            SelectObject(self.mem_dc, self.old_bmp);
            let _ = DeleteObject(HGDIOBJ(self.dib.0));
            let _ = DeleteDC(self.mem_dc);
        }
    }
}

impl OverlayState {
    fn composite_scene(&mut self) {
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
                    CaptureBuffer::draw_border(
                        buffer,
                        width,
                        height,
                        &sel,
                        [246, 130, 59, 255],
                        2,
                    );
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
                    crate::annotation::apply_pixelate_blur(buffer, width, height, &r, DEFAULT_BLUR_BLOCK);
                }

                // 4. Draw selection border
                CaptureBuffer::draw_border(
                    buffer,
                    width,
                    height,
                    &sel,
                    [246, 130, 59, 255],
                    2,
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
                    Some(InProgressDrawing::Pen { points }) => {
                        let preview = AnnotationObject::new(
                            0,
                            AnnotationKind::Pen {
                                points: points.clone(),
                                color: self.active_color,
                                thickness: self.active_thickness,
                            },
                        );
                        preview.render_gdi(self.mem_dc);
                    }
                    _ => {}
                }

                // 7. Draw selection handles for selected object
                if let Some(obj) = self
                    .selected_id
                    .and_then(|id| self.objects.iter().find(|o| o.id == id))
                {
                    obj.render_selection_indicator(self.mem_dc);
                }

                // 8. Draw active text edit preview
                if let Some(text_edit) = &self.text_edit {
                    unsafe {
                        let _ = SetTextColor(self.mem_dc, COLORREF(0x00EBEB28));
                        let _ = SetBkMode(self.mem_dc, TRANSPARENT);
                        let display_text = format!("{}|", text_edit.text);
                        let wide: Vec<u16> = display_text.encode_utf16().collect();
                        let _ = TextOutW(self.mem_dc, text_edit.pos.0, text_edit.pos.1, &wide);
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
                );
                if let Some(existing) = &self.toolbar {
                    tb.hovered_item = existing.hovered_item;
                }
                tb.render(self.mem_dc);
                self.toolbar = Some(tb);
            }
        }
    }

    fn redraw(&mut self, hwnd: HWND) {
        self.composite_scene();
        unsafe {
            let _ = InvalidateRect(hwnd, None, false);
        }
    }

    fn commit_selection(&mut self, hwnd: HWND, rect: Rect) {
        self.mode = OverlayMode::SelectionActive;
        self.committed_selection = Some(rect);
        self.hover_snap_rect = None;
        self.selected_id = None;
        self.redraw(hwnd);
    }

    fn copy_selection_to_clipboard(&mut self, hwnd: HWND) -> bool {
        let Some(sel) = self.committed_selection else {
            return false;
        };

        let prev_selected = self.selected_id.take();
        let prev_text = self.text_edit.take();
        self.composite_scene();

        let width = self.capture.width;
        let height = self.capture.height;
        let len = (width as usize) * (height as usize) * 4;
        let buffer = unsafe { std::slice::from_raw_parts(self.bits_ptr, len) };

        let dib_res = flatten_selection_to_dib(buffer, width, height, &sel);

        self.selected_id = prev_selected;
        self.text_edit = prev_text;

        if let Ok(dib_data) = dib_res {
            self.committed_result = copy_dib_to_clipboard(Some(hwnd), &dib_data).is_ok();
            return self.committed_result;
        }
        false
    }

    fn save_selection_to_file(&mut self) -> Option<std::path::PathBuf> {
        let sel = self.committed_selection?;

        let prev_selected = self.selected_id.take();
        let prev_text = self.text_edit.take();
        self.composite_scene();

        let width = self.capture.width;
        let height = self.capture.height;
        let len = (width as usize) * (height as usize) * 4;
        let buffer = unsafe { std::slice::from_raw_parts(self.bits_ptr, len) };

        let res = save_screenshot(
            buffer,
            width,
            height,
            &sel,
            Some(&self.settings.save_directory),
        );

        self.selected_id = prev_selected;
        self.text_edit = prev_text;

        match res {
            Ok(path) => {
                println!("[isolmaSS] Screenshot saved to: {}", path.display());
                self.committed_result = true;
                Some(path)
            }
            Err(e) => {
                eprintln!("[isolmaSS] Error saving screenshot: {}", e);
                None
            }
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
                let cur = if let Some(tb) = &state.toolbar {
                    if tb.hovered_item.is_some() {
                        unsafe { LoadCursorW(HINSTANCE::default(), IDC_HAND).unwrap_or_default() }
                    } else {
                        unsafe { LoadCursorW(HINSTANCE::default(), IDC_CROSS).unwrap_or_default() }
                    }
                } else {
                    unsafe { LoadCursorW(HINSTANCE::default(), IDC_CROSS).unwrap_or_default() }
                };
                unsafe {
                    SetCursor(cur);
                }
                return LRESULT(1);
            }
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
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
                        let screen_x = client_x + state.capture.x;
                        let screen_y = client_y + state.capture.y;

                        let new_snap = if let Some(win) = find_window_at_point((screen_x, screen_y), Some(hwnd)) {
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
                            state.hover_snap_rect = new_snap;
                            state.redraw(hwnd);
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
                            state.committed_selection = Some(new_sel);
                            state.redraw(hwnd);
                        }
                    }
                    OverlayMode::SelectionActive => {
                        if let Some(drag) = state.dragging_object {
                            let dx = client_x - drag.last_pos.0;
                            let dy = client_y - drag.last_pos.1;
                            if let Some(obj) = state.objects.iter_mut().find(|o| o.id == drag.id) {
                                obj.translate(dx, dy);
                            }
                            state.dragging_object = Some(DragObjectState {
                                id: drag.id,
                                start_pos: drag.start_pos,
                                last_pos: pt,
                            });
                            state.redraw(hwnd);
                            return LRESULT(0);
                        }

                        if let Some(shape) = &mut state.drawing_shape {
                            match shape {
                                InProgressDrawing::Rectangle { start, current } => {
                                    *current = if shift_down { snap_square(*start, pt) } else { pt };
                                }
                                InProgressDrawing::Arrow { start, current } => {
                                    *current = if shift_down { snap_angle_45(*start, pt) } else { pt };
                                }
                                InProgressDrawing::Pen { points } => points.push(pt),
                                InProgressDrawing::Blur { start, current } => {
                                    *current = if shift_down { snap_square(*start, pt) } else { pt };
                                }
                            }
                            state.redraw(hwnd);
                            return LRESULT(0);
                        }

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
                            match item {
                                ToolbarItem::Tool(k) => {
                                    state.active_tool = k;
                                    state.selected_id = None;
                                    state.text_edit = None;
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
                                    state.save_selection_to_file();
                                    let _ = unsafe { DestroyWindow(hwnd) };
                                }
                                ToolbarItem::Action(ToolbarAction::Copy) => {
                                    state.copy_selection_to_clipboard(hwnd);
                                    let _ = unsafe { DestroyWindow(hwnd) };
                                }
                                ToolbarItem::Action(ToolbarAction::Settings) => {
                                    if let Ok(Some(new_cfg)) = show_settings_dialog(&state.settings) {
                                        state.active_color = new_cfg.default_color;
                                        state.active_thickness = new_cfg.default_thickness;
                                        state.settings = new_cfg;
                                        let _ = state.settings.save();
                                        state.redraw(hwnd);
                                    }
                                }
                                ToolbarItem::Action(ToolbarAction::Cancel) => {
                                    state.mode = OverlayMode::Hovering;
                                    state.committed_selection = None;
                                    state.toolbar = None;
                                    state.objects.clear();
                                    state.selected_id = None;
                                    state.text_edit = None;
                                    state.redraw(hwnd);
                                }
                                ToolbarItem::Color(col) => {
                                    state.active_color = col;
                                    if let Some(obj) = state
                                        .selected_id
                                        .and_then(|id| state.objects.iter_mut().find(|o| o.id == id))
                                    {
                                        let old_kind = obj.kind.clone();
                                        obj.set_color(col);
                                        let new_kind = obj.kind.clone();
                                        state.history.record(EditCommand::Modify {
                                            id: obj.id,
                                            old_kind,
                                            new_kind,
                                        });
                                    }
                                    state.redraw(hwnd);
                                }
                                ToolbarItem::Thickness(thick) => {
                                    state.active_thickness = thick;
                                    if let Some(obj) = state
                                        .selected_id
                                        .and_then(|id| state.objects.iter_mut().find(|o| o.id == id))
                                    {
                                        let old_kind = obj.kind.clone();
                                        obj.set_thickness(thick);
                                        let new_kind = obj.kind.clone();
                                        state.history.record(EditCommand::Modify {
                                            id: obj.id,
                                            old_kind,
                                            new_kind,
                                        });
                                    }
                                    state.redraw(hwnd);
                                }
                            }
                            return LRESULT(0);
                        }

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
                            state.text_edit = None;
                            state.redraw(hwnd);
                            return LRESULT(0);
                        }

                        // 3. Click empty canvas: deselect
                        state.selected_id = None;

                        // 4. Start drawing new shape
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
                                state.drawing_shape = Some(InProgressDrawing::Pen {
                                    points: vec![pt],
                                });
                            }
                            ToolKind::Text => {
                                state.text_edit = Some(TextEditState {
                                    pos: pt,
                                    text: String::new(),
                                });
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
                                let screen_x = client_x + state.capture.x;
                                let screen_y = client_y + state.capture.y;

                                if let Some(win) = find_window_at_point((screen_x, screen_y), Some(hwnd)) {
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
                                let final_pt = if shift_down { snap_square(start, pt) } else { pt };
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
                                    let final_cur = if shift_down { snap_square(start, current) } else { current };
                                    let r = Rect::normalized(start, final_cur)
                                        .clamp(state.capture.width, state.capture.height);
                                    if r.width() > 3 && r.height() > 3 {
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
                                    let final_cur = if shift_down { snap_angle_45(start, current) } else { current };
                                    let dx = (final_cur.0 - start.0).abs();
                                    let dy = (final_cur.1 - start.1).abs();
                                    if dx > 4 || dy > 4 {
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
                                    if points.len() >= 2 {
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
                                    let final_cur = if shift_down { snap_square(start, current) } else { current };
                                    let r = Rect::normalized(start, final_cur)
                                        .clamp(state.capture.width, state.capture.height);
                                    if r.width() > 4 && r.height() > 4 {
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

        WM_CHAR => {
            if !state_ptr.is_null() {
                let state = unsafe { &mut *state_ptr };
                let ch_code = wparam.0 as u32;

                if let Some(edit) = &mut state.text_edit {
                    match ch_code {
                        8 => {
                            edit.text.pop();
                            state.redraw(hwnd);
                            return LRESULT(0);
                        }
                        13 => {
                            let text = edit.text.trim().to_string();
                            let pos = edit.pos;
                            state.text_edit = None;

                            if !text.is_empty() {
                                let obj = AnnotationObject::new(
                                    state.next_id,
                                    AnnotationKind::Text {
                                        pos,
                                        text,
                                        color: state.active_color,
                                        font_size: DEFAULT_FONT_SIZE,
                                    },
                                );
                                state.next_id += 1;
                                state.history.record(EditCommand::Add(obj.clone()));
                                state.selected_id = Some(obj.id);
                                state.objects.push(obj);
                            }
                            state.redraw(hwnd);
                            return LRESULT(0);
                        }
                        27 => {
                            state.text_edit = None;
                            state.redraw(hwnd);
                            return LRESULT(0);
                        }
                        32..=126 | 160..=0x10FFFF => {
                            if let Some(ch) = char::from_u32(ch_code) {
                                edit.text.push(ch);
                                state.redraw(hwnd);
                                return LRESULT(0);
                            }
                        }
                        _ => {}
                    }
                }
            }
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }

        WM_KEYDOWN => {
            if !state_ptr.is_null() {
                let state = unsafe { &mut *state_ptr };
                let ctrl_down = (unsafe { GetKeyState(VK_CONTROL.0 as i32) } as u16 & 0x8000) != 0;

                // 1. Save shortcut: Ctrl+S
                if state.mode == OverlayMode::SelectionActive && ctrl_down && wparam.0 == 'S' as usize {
                    state.save_selection_to_file();
                    let _ = unsafe { DestroyWindow(hwnd) };
                    return LRESULT(0);
                }

                // 2. Settings shortcut: Ctrl+,
                if ctrl_down && wparam.0 == VK_OEM_COMMA.0 as usize {
                    if let Ok(Some(new_cfg)) = show_settings_dialog(&state.settings) {
                        state.active_color = new_cfg.default_color;
                        state.active_thickness = new_cfg.default_thickness;
                        state.settings = new_cfg;
                        let _ = state.settings.save();
                        state.redraw(hwnd);
                    }
                    return LRESULT(0);
                }

                // 3. Copy shortcut: Ctrl+C or Enter
                if state.mode == OverlayMode::SelectionActive
                    && ((ctrl_down && wparam.0 == 'C' as usize)
                        || (state.text_edit.is_none() && wparam.0 == VK_RETURN.0 as usize))
                {
                    state.copy_selection_to_clipboard(hwnd);
                    let _ = unsafe { DestroyWindow(hwnd) };
                    return LRESULT(0);
                }

                // 4. Undo: Ctrl+Z
                if ctrl_down && wparam.0 == 'Z' as usize {
                    state.history.undo(&mut state.objects);
                    state.selected_id = None;
                    state.redraw(hwnd);
                    return LRESULT(0);
                }

                // 5. Redo: Ctrl+Y
                if ctrl_down && wparam.0 == 'Y' as usize {
                    state.history.redo(&mut state.objects);
                    state.selected_id = None;
                    state.redraw(hwnd);
                    return LRESULT(0);
                }

                // 6. Delete / Backspace selected object
                if state.text_edit.is_none()
                    && (wparam.0 == VK_DELETE.0 as usize || wparam.0 == VK_BACK.0 as usize)
                    && let Some(pos) = state
                        .selected_id
                        .take()
                        .and_then(|id| state.objects.iter().position(|o| o.id == id))
                {
                    let removed = state.objects.remove(pos);
                    state.history.record(EditCommand::Delete(removed));
                    state.redraw(hwnd);
                    return LRESULT(0);
                }

                // 7. Tool switching shortcuts (R, A, P, T, B)
                if state.text_edit.is_none() && !ctrl_down {
                    match wparam.0 as u8 as char {
                        'R' | 'r' => {
                            state.active_tool = ToolKind::Rectangle;
                            state.selected_id = None;
                            state.redraw(hwnd);
                            return LRESULT(0);
                        }
                        'A' | 'a' => {
                            state.active_tool = ToolKind::Arrow;
                            state.selected_id = None;
                            state.redraw(hwnd);
                            return LRESULT(0);
                        }
                        'P' | 'p' => {
                            state.active_tool = ToolKind::Pen;
                            state.selected_id = None;
                            state.redraw(hwnd);
                            return LRESULT(0);
                        }
                        'T' | 't' => {
                            state.active_tool = ToolKind::Text;
                            state.selected_id = None;
                            state.redraw(hwnd);
                            return LRESULT(0);
                        }
                        'B' | 'b' => {
                            state.active_tool = ToolKind::Blur;
                            state.selected_id = None;
                            state.redraw(hwnd);
                            return LRESULT(0);
                        }
                        _ => {}
                    }
                }

                // 8. Hierarchical Escape flow (Slice A15)
                if wparam.0 == VK_ESCAPE.0 as usize {
                    if state.text_edit.is_some() {
                        state.text_edit = None;
                        state.redraw(hwnd);
                        return LRESULT(0);
                    }
                    if state.selected_id.is_some() {
                        state.selected_id = None;
                        state.redraw(hwnd);
                        return LRESULT(0);
                    }
                    if state.mode == OverlayMode::SelectionActive {
                        state.mode = OverlayMode::Hovering;
                        state.committed_selection = None;
                        state.toolbar = None;
                        state.objects.clear();
                        state.redraw(hwnd);
                        return LRESULT(0);
                    }
                    let _ = unsafe { DestroyWindow(hwnd) };
                    return LRESULT(0);
                }
            }
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }

        WM_RBUTTONUP => {
            if !state_ptr.is_null() {
                let state = unsafe { &mut *state_ptr };
                if state.text_edit.is_some() {
                    state.text_edit = None;
                    state.redraw(hwnd);
                    return LRESULT(0);
                }
                if state.selected_id.is_some() {
                    state.selected_id = None;
                    state.redraw(hwnd);
                    return LRESULT(0);
                }
                if state.mode == OverlayMode::SelectionActive {
                    state.mode = OverlayMode::Hovering;
                    state.committed_selection = None;
                    state.toolbar = None;
                    state.objects.clear();
                    state.redraw(hwnd);
                    return LRESULT(0);
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
            unsafe {
                PostQuitMessage(0);
            }
            LRESULT(0)
        }

        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

fn register_overlay_class() -> Result<()> {
    static REGISTERED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if REGISTERED.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return Ok(());
    }

    let wc = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        style: windows::Win32::UI::WindowsAndMessaging::CS_HREDRAW
            | windows::Win32::UI::WindowsAndMessaging::CS_VREDRAW,
        lpfnWndProc: Some(overlay_wnd_proc),
        cbClsExtra: 0,
        cbWndExtra: 0,
        hInstance: HINSTANCE::default(),
        hIcon: windows::Win32::UI::WindowsAndMessaging::HICON::default(),
        hCursor: unsafe { LoadCursorW(HINSTANCE::default(), IDC_CROSS).unwrap_or_default() },
        hbrBackground: windows::Win32::Graphics::Gdi::HBRUSH::default(),
        lpszMenuName: windows::core::PCWSTR::null(),
        lpszClassName: OVERLAY_CLASS_NAME,
        hIconSm: windows::Win32::UI::WindowsAndMessaging::HICON::default(),
    };

    let atom = unsafe { RegisterClassExW(&wc) };
    if atom == 0 {
        return Err(windows::core::Error::from_win32());
    }
    Ok(())
}

pub fn show_overlay_session(capture: Arc<CaptureBuffer>) -> Result<Option<Rect>> {
    register_overlay_class()?;

    let width = capture.width;
    let height = capture.height;

    let screen_dc = unsafe { GetDC(HWND::default()) };
    if screen_dc.is_invalid() {
        return Err(windows::core::Error::from_win32());
    }

    let mem_dc = unsafe { CreateCompatibleDC(screen_dc) };
    if mem_dc.is_invalid() {
        unsafe {
            let _ = ReleaseDC(HWND::default(), screen_dc);
        }
        return Err(windows::core::Error::from_win32());
    }

    let bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            biHeight: -height,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            biSizeImage: 0,
            biXPelsPerMeter: 0,
            biYPelsPerMeter: 0,
            biClrUsed: 0,
            biClrImportant: 0,
        },
        bmiColors: [RGBQUAD::default()],
    };

    let mut bits_ptr: *mut c_void = std::ptr::null_mut();
    let dib = unsafe {
        CreateDIBSection(
            screen_dc,
            &bmi,
            DIB_RGB_COLORS,
            &mut bits_ptr,
            None,
            0,
        )?
    };

    unsafe {
        let _ = ReleaseDC(HWND::default(), screen_dc);
    }

    if dib.is_invalid() || bits_ptr.is_null() {
        unsafe {
            let _ = DeleteDC(mem_dc);
        }
        return Err(windows::core::Error::from_win32());
    }

    let old_bmp = unsafe { SelectObject(mem_dc, HGDIOBJ(dib.0)) };

    let buffer_bytes = (width as usize) * (height as usize) * 4;
    unsafe {
        std::ptr::copy_nonoverlapping(capture.dimmed.as_ptr(), bits_ptr as *mut u8, buffer_bytes);
    }

    let settings = Settings::load_or_default();
    let active_color = settings.default_color;
    let active_thickness = settings.default_thickness;

    let mut state = Box::new(OverlayState {
        capture,
        mode: OverlayMode::Hovering,
        drag_start: None,
        committed_selection: None,
        hover_snap_rect: None,

        settings,
        active_tool: ToolKind::Rectangle,
        active_color,
        active_thickness,
        objects: Vec::new(),
        selected_id: None,
        next_id: 1,
        history: HistoryManager::default(),

        drawing_shape: None,
        dragging_object: None,
        text_edit: None,
        toolbar: None,

        mem_dc,
        dib,
        old_bmp,
        bits_ptr: bits_ptr as *mut u8,

        committed_result: false,
    });

    let hwnd = unsafe {
        CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOACTIVATE,
            OVERLAY_CLASS_NAME,
            w!("isolmaSS Overlay"),
            WS_POPUP,
            state.capture.x,
            state.capture.y,
            width,
            height,
            None,
            None,
            HINSTANCE::default(),
            None,
        )?
    };

    if hwnd.is_invalid() {
        return Err(windows::core::Error::from_win32());
    }

    unsafe {
        let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), 255, LWA_ALPHA);
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, state.as_mut() as *mut OverlayState as isize);
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = SetForegroundWindow(hwnd);
        let _ = SetFocus(hwnd);
    }

    let mut msg = MSG::default();
    while unsafe { GetMessageW(&mut msg, HWND::default(), 0, 0) }.0 > 0 {
        unsafe {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }

    let committed = if state.committed_result || state.committed_selection.is_some() {
        state.committed_selection
    } else {
        None
    };

    Ok(committed)
}
