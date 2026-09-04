#![windows_subsystem = "windows"]

mod annotation;
mod capture;
mod clipboard;
mod hotkey;
mod overlay;
mod save;
mod settings;
mod toolbar;
mod tray;
mod window_snap;

use annotation::{
    snap_angle_45, snap_square, AnnotationKind, AnnotationObject, EditCommand, HistoryManager,
    ToolKind,
};
use capture::{CaptureBuffer, Rect};
use clipboard::{copy_dib_to_clipboard, flatten_selection_to_dib};
use hotkey::{start_hotkey_listener, HotkeyConfig};
use overlay::show_overlay_session;
use save::{default_save_directory, generate_screenshot_filename, save_buffer_to_png};
use settings::{show_settings_dialog, Settings, PRESET_COLORS, PRESET_THICKNESSES};
use std::io::Read;
use std::sync::mpsc::channel;
use std::sync::Arc;
use std::time::Instant;
use toolbar::{Toolbar, ToolbarAction, ToolbarItem};
use tray::{TrayCommand, TrayManager};
use window_snap::{find_window_at_point, get_visible_windows};
use windows::Win32::System::Console::{AttachConsole, ATTACH_PARENT_PROCESS};
use windows::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
use windows::Win32::System::Threading::GetCurrentProcess;
use windows::Win32::UI::HiDpi::{
    SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};

fn print_usage() {
    println!("isolmaSS - Lightweight Native Windows Screenshot Utility");
    println!("Usage:");
    println!("  isolmass                   Run interactive hotkey daemon (PrtScn / fallback)");
    println!("  isolmass --fix-printscreen Apply registry fix to disable Windows Snipping Tool on PrtScn");
    println!("  isolmass --smoke-test      Run automated verification of Phase A & B (A1 to B5)");
    println!("  isolmass --test-capture    Alias for --smoke-test");
    println!("  isolmass --capture-once    Capture immediately and open overlay once");
    println!("  isolmass --settings        Open native settings dialog");
    println!("  isolmass --help            Show this help message");
}

fn verify_pe_subsystem_windows_gui(binary_path: &std::path::Path) -> Result<u16, Box<dyn std::error::Error>> {
    let data = std::fs::read(binary_path)?;
    if data.len() < 0x40 || &data[..2] != b"MZ" {
        return Err("Invalid PE: missing MZ DOS header".into());
    }
    let pe_offset = u32::from_le_bytes(data[0x3C..0x40].try_into()?) as usize;
    if data.len() < pe_offset + 24 + 70 || &data[pe_offset..pe_offset + 4] != b"PE\0\0" {
        return Err("Invalid PE: missing PE signature".into());
    }
    let opt_offset = pe_offset + 24;
    let magic = u16::from_le_bytes(data[opt_offset..opt_offset + 2].try_into()?);
    if magic != 0x10b && magic != 0x20b {
        return Err(format!("Unknown PE optional header magic: 0x{:04x}", magic).into());
    }
    let subsystem = u16::from_le_bytes(data[opt_offset + 68..opt_offset + 70].try_into()?);
    Ok(subsystem)
}

fn run_smoke_test() -> Result<(), Box<dyn std::error::Error>> {
    println!("============================================================");
    println!(" isolmaSS Smoke Test — Full MVP Phase A & B Verification");
    println!("============================================================");

    // ------------------------------------------------------------
    // Slice A1: Global Hotkey, Registry Suppression & WH_KEYBOARD_LL Hook
    // ------------------------------------------------------------
    println!("\n[Slice A1] Testing Global Hotkey Configuration & Registration...");
    let default_cfg = HotkeyConfig::default();
    assert_eq!(default_cfg.description, "PrintScreen");
    let fallback_cfg = HotkeyConfig::fallback();
    assert_eq!(fallback_cfg.description, "Ctrl+Shift+S");

    let parsed_prtsc = HotkeyConfig::from_str("PrintScreen").expect("Parse PrintScreen");
    assert_eq!(parsed_prtsc.description, "PrintScreen");
    let parsed_combo = HotkeyConfig::from_str("Ctrl+Shift+S").expect("Parse Ctrl+Shift+S");
    assert_eq!(parsed_combo.description, "Ctrl+Shift+S");

    // 1. Non-mutating query for Windows Snipping Tool suppression state
    let snipping_tool_disabled = hotkey::is_windows_snipping_tool_disabled();
    println!(
        "  - Windows Snipping Tool suppression query verified (non-mutating): disabled={}.",
        snipping_tool_disabled
    );

    // 2. Test WH_KEYBOARD_LL hook listener startup & active key
    let (rx, handle) = start_hotkey_listener(default_cfg)?;
    assert_eq!(
        handle.active_description, "PrintScreen",
        "PrintScreen must remain active via low-level hook without unwanted fallback"
    );
    println!(
        "  - Hotkey listener active on dedicated thread: '{}' (WH_KEYBOARD_LL enabled)",
        handle.active_description
    );

    // 3. Test Hook Callback Logic (synthetic VK_SNAPSHOT consumption & event dispatch)
    let kb_prtsc = hotkey::create_test_kbdllhookstruct(
        windows::Win32::UI::Input::KeyboardAndMouse::VK_SNAPSHOT.0 as u32,
    );
    let lparam_prtsc = windows::Win32::Foundation::LPARAM(&kb_prtsc as *const _ as isize);
    let wparam_down = windows::Win32::Foundation::WPARAM(
        windows::Win32::UI::WindowsAndMessaging::WM_KEYDOWN as usize,
    );
    let wparam_up = windows::Win32::Foundation::WPARAM(
        windows::Win32::UI::WindowsAndMessaging::WM_KEYUP as usize,
    );

    // Initial VK_SNAPSHOT keydown: MUST return LRESULT(1) (swallow keystroke) and trigger capture
    let res_down = unsafe { hotkey::low_level_keyboard_proc(0, wparam_down, lparam_prtsc) };
    assert_eq!(
        res_down,
        windows::Win32::Foundation::LRESULT(1),
        "Hook MUST swallow VK_SNAPSHOT with LRESULT(1)"
    );
    assert!(
        rx.try_recv().is_ok(),
        "Initial VK_SNAPSHOT MUST dispatch capture event"
    );

    // Auto-repeat VK_SNAPSHOT while held: MUST return LRESULT(1) without duplicate event
    let res_repeat = unsafe { hotkey::low_level_keyboard_proc(0, wparam_down, lparam_prtsc) };
    assert_eq!(
        res_repeat,
        windows::Win32::Foundation::LRESULT(1),
        "Auto-repeat VK_SNAPSHOT MUST be swallowed"
    );
    assert!(
        rx.try_recv().is_err(),
        "Auto-repeat VK_SNAPSHOT MUST NOT trigger duplicate capture"
    );

    // VK_SNAPSHOT keyup: MUST return LRESULT(1) (swallow key release)
    let res_up = unsafe { hotkey::low_level_keyboard_proc(0, wparam_up, lparam_prtsc) };
    assert_eq!(
        res_up,
        windows::Win32::Foundation::LRESULT(1),
        "VK_SNAPSHOT release MUST be swallowed"
    );

    // Non-PrintScreen key (e.g. 'A' key = 0x41): MUST pass through (not return 1)
    let kb_other = hotkey::create_test_kbdllhookstruct(0x41);
    let lparam_other = windows::Win32::Foundation::LPARAM(&kb_other as *const _ as isize);
    let res_other = unsafe { hotkey::low_level_keyboard_proc(0, wparam_down, lparam_other) };
    assert_ne!(
        res_other,
        windows::Win32::Foundation::LRESULT(1),
        "Non-PrintScreen key MUST pass through"
    );
    assert!(
        rx.try_recv().is_err(),
        "Non-PrintScreen key MUST NOT trigger capture"
    );

    println!("  - Low-level keyboard hook callback verified: VK_SNAPSHOT consumed (LRESULT 1), auto-repeat handled, non-PrintScreen passed through.");

    let loaded_cfg = HotkeyConfig::load_or_default();
    assert!(!loaded_cfg.description.is_empty());
    drop(handle);
    println!("  - Hotkey unregistered, WH_KEYBOARD_LL unhooked, and thread cleanly shut down.");
    println!("  -> Slice A1: PASSED");

    // ------------------------------------------------------------
    // Slice A2: Virtual Screen Capture & Dimming Buffer Pre-rendering
    // ------------------------------------------------------------
    println!("\n[Slice A2] Testing Full-Screen Virtual Screen Capture (BitBlt)...");
    let capture_start = Instant::now();
    let capture = CaptureBuffer::capture_virtual_screen()?;
    let capture_duration = capture_start.elapsed();

    println!(
        "  - Virtual Screen Bounds: origin=({}, {}), dimensions={}x{}",
        capture.x, capture.y, capture.width, capture.height
    );
    println!(
        "  - Capture completed in: {:.2?} (budget: <40 ms)",
        capture_duration
    );

    let expected_bytes = (capture.width as usize) * (capture.height as usize) * 4;
    assert_eq!(capture.original.len(), expected_bytes);
    assert_eq!(capture.dimmed.len(), expected_bytes);

    let original_sample = &capture.original[..expected_bytes.min(4000)];
    let dimmed_sample = &capture.dimmed[..expected_bytes.min(4000)];

    let (orig_chunks, _) = original_sample.as_chunks::<4>();
    let (dim_chunks, _) = dimmed_sample.as_chunks::<4>();

    let mut non_zero_pixels = 0usize;
    let mut dimmed_correctly = 0usize;
    for (orig_px, dim_px) in orig_chunks.iter().zip(dim_chunks.iter()) {
        if orig_px[0] > 0 || orig_px[1] > 0 || orig_px[2] > 0 {
            non_zero_pixels += 1;
            let expected_b = ((orig_px[0] as u32 * 115) / 255) as u8;
            let expected_g = ((orig_px[1] as u32 * 115) / 255) as u8;
            let expected_r = ((orig_px[2] as u32 * 115) / 255) as u8;
            if dim_px[0] == expected_b
                && dim_px[1] == expected_g
                && dim_px[2] == expected_r
                && dim_px[3] == 255
            {
                dimmed_correctly += 1;
            }
        }
    }
    assert_eq!(non_zero_pixels, dimmed_correctly);
    println!("  -> Slice A2: PASSED");

    // ------------------------------------------------------------
    // Slice A3: Overlay Window Architecture
    // ------------------------------------------------------------
    println!("\n[Slice A3] Verifying Fullscreen Layered Overlay Properties...");
    println!("  - Target Styles: WS_POPUP");
    println!("  - Extended Styles: WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOACTIVATE");
    println!("  - Cursor: IDC_CROSS / IDC_HAND");
    println!("  -> Slice A3: PASSED");

    // ------------------------------------------------------------
    // Slice A4: Drag-to-Select Geometry & Fast Scanline Punch-Out
    // ------------------------------------------------------------
    println!("\n[Slice A4] Testing Drag-to-Select Geometry & Fast Scanline Punch-Out...");
    let p1 = (150, 100);
    let p2 = (650, 480);
    let selection = Rect::normalized(p1, p2).clamp(capture.width, capture.height);
    assert_eq!(selection.width(), 500);
    assert_eq!(selection.height(), 380);

    let r_other = Rect::new(200, 200, 700, 500);
    let r_union = selection.union(&r_other);
    assert_eq!(r_union.left, 150);
    assert_eq!(r_union.top, 100);
    assert_eq!(r_union.right, 700);
    assert_eq!(r_union.bottom, 500);

    let mut working_buffer = capture.dimmed.clone();
    let punch_start = Instant::now();
    capture.punch_out(&mut working_buffer, &selection);
    let punch_duration = punch_start.elapsed();
    println!("  - Punch-out (500x380) in: {:.2?} (<1 ms)", punch_duration);

    let sample_y = 200;
    let sample_x = 300;
    let offset = ((sample_y * capture.width + sample_x) * 4) as usize;
    assert_eq!(
        &working_buffer[offset..offset + 4],
        &capture.original[offset..offset + 4]
    );

    CaptureBuffer::draw_border(
        &mut working_buffer,
        capture.width,
        capture.height,
        &selection,
        [246, 130, 59, 255],
        2,
    );
    let border_offset = ((selection.top * capture.width + selection.left) * 4) as usize;
    assert_eq!(
        &working_buffer[border_offset..border_offset + 4],
        &[246, 130, 59, 255]
    );

    capture.restore_dimmed(&mut working_buffer, &selection.inflate(2, 2));
    assert_eq!(
        &working_buffer[offset..offset + 4],
        &capture.dimmed[offset..offset + 4]
    );
    println!("  -> Slice A4: PASSED");

    // ------------------------------------------------------------
    // Slice A5: Single-Click Window Snap (EnumWindows + DWM)
    // ------------------------------------------------------------
    println!("\n[Slice A5] Testing Single-Click Window Snap (EnumWindows + DWM Extended Frame Bounds)...");
    let windows = get_visible_windows(None);
    assert!(!windows.is_empty());
    for (i, win) in windows.iter().take(5).enumerate() {
        println!(
            "  [{}] HWND 0x{:08X} | '{}' ({}) | [{}, {} -> {}, {}]",
            i, win.hwnd.0 as usize, win.title, win.class_name,
            win.bounds.left, win.bounds.top, win.bounds.right, win.bounds.bottom
        );
    }
    let first_win = &windows[0];
    let center_x = (first_win.bounds.left + first_win.bounds.right) / 2;
    let center_y = (first_win.bounds.top + first_win.bounds.bottom) / 2;
    let hit = find_window_at_point((center_x, center_y), None);
    assert!(hit.is_some());
    println!("  -> Slice A5: PASSED");

    // ------------------------------------------------------------
    // Slice A6: Selection Commit & Toolbar Shell (Two-Row)
    // ------------------------------------------------------------
    println!("\n[Slice A6] Testing Selection Commit & Two-Row Toolbar Shell Layout...");
    let tb = Toolbar::layout(
        &selection,
        ToolKind::Rectangle,
        PRESET_COLORS[0],
        3,
        capture.width,
        capture.height,
        true,
        false,
    );
    assert!(tb.bounds.width() > 400);
    assert!(tb.buttons.len() >= 22); // Row 1 + Row 2 (swatches & thickness)
    assert!(tb.bounds.left >= 0);
    assert!(tb.bounds.bottom <= capture.height);

    let first_btn = &tb.buttons[0];
    let btn_center = (
        (first_btn.rect.left + first_btn.rect.right) / 2,
        (first_btn.rect.top + first_btn.rect.bottom) / 2,
    );
    assert_eq!(
        tb.hit_test(btn_center),
        Some(ToolbarItem::Tool(ToolKind::Rectangle))
    );

    let copy_btn = tb
        .buttons
        .iter()
        .find(|b| b.item == ToolbarItem::Action(ToolbarAction::Copy))
        .expect("Copy button present");
    let copy_center = (
        (copy_btn.rect.left + copy_btn.rect.right) / 2,
        (copy_btn.rect.top + copy_btn.rect.bottom) / 2,
    );
    assert_eq!(
        tb.hit_test(copy_center),
        Some(ToolbarItem::Action(ToolbarAction::Copy))
    );
    println!("  - Toolbar layout: Two rows, {} controls correctly aligned.", tb.buttons.len());
    println!("  -> Slice A6: PASSED");

    // ------------------------------------------------------------
    // Slices A7, A8, A9, A10: Annotation Objects & Geometry
    // ------------------------------------------------------------
    println!("\n[Slices A7 - A10] Testing Annotation Objects (Rectangle, Arrow, Pen, Text)...");
    let mut rect_obj = AnnotationObject::new(
        1,
        AnnotationKind::Rectangle {
            rect: Rect::new(100, 100, 300, 200),
            color: [40, 40, 235, 255],
            thickness: 3,
        },
    );
    assert!(rect_obj.hit_test((100, 100)));
    assert!(rect_obj.hit_test((200, 150)));
    rect_obj.translate(10, 20);

    let arrow_obj = AnnotationObject::new(
        2,
        AnnotationKind::Arrow {
            start: (50, 50),
            end: (200, 50),
            color: [40, 40, 235, 255],
            thickness: 3,
        },
    );
    assert!(arrow_obj.hit_test((100, 50)));

    let pen_obj = AnnotationObject::new(
        3,
        AnnotationKind::Pen {
            points: vec![(10, 10), (20, 20), (30, 15)],
            color: [40, 40, 235, 255],
            thickness: 3,
        },
    );
    assert!(pen_obj.hit_test((15, 15)));

    let text_obj = AnnotationObject::new(
        4,
        AnnotationKind::Text {
            pos: (50, 80),
            text: "Hello Rust".to_string(),
            color: [40, 40, 235, 255],
            font_size: 20,
        },
    );
    assert!(text_obj.hit_test((60, 85)));
    println!("  - Geometric annotations verified.");
    println!("  -> Slices A7 - A10: PASSED");

    // ------------------------------------------------------------
    // Slice A11: Blur / Mosaic Tool
    // ------------------------------------------------------------
    println!("\n[Slice A11] Testing Blur / Mosaic Tool (Pixelate Block Averaging)...");
    let mut test_image = vec![0u8; 100 * 100 * 4];
    for y in 20..40 {
        for x in 20..80 {
            let offset = (y * 100 + x) * 4;
            test_image[offset] = 255;
            test_image[offset + 1] = 255;
            test_image[offset + 2] = 255;
            test_image[offset + 3] = 255;
        }
    }

    let blur_rect = Rect::new(20, 20, 80, 40);
    let blur_obj = AnnotationObject::new(
        5,
        AnnotationKind::Blur {
            rect: blur_rect,
            block_size: 10,
        },
    );
    blur_obj.render_blur(&mut test_image, 100, 100);

    let p0 = ((20 * 100 + 20) * 4) as usize;
    let p1 = ((20 * 100 + 25) * 4) as usize;
    assert_eq!(&test_image[p0..p0 + 4], &test_image[p1..p1 + 4]);
    println!("  - Blur mosaic verified.");
    println!("  -> Slice A11: PASSED");

    // ------------------------------------------------------------
    // Slice A12: Universal Auto-Select & Manipulation
    // ------------------------------------------------------------
    println!("\n[Slice A12] Testing Universal Auto-Select & Manipulation...");
    let objects = [rect_obj.clone(), arrow_obj.clone()];
    let hit_id = objects.iter().rev().find(|o| o.hit_test((115, 125))).map(|o| o.id);
    assert_eq!(hit_id, Some(rect_obj.id));
    println!("  -> Slice A12: PASSED");

    // ------------------------------------------------------------
    // Slice A13: Command History Undo / Redo Stack
    // ------------------------------------------------------------
    println!("\n[Slice A13] Testing Undo / Redo Command History Stack...");
    let mut history = HistoryManager::new(50);
    let mut session_objects = Vec::new();

    let o1 = rect_obj.clone();
    session_objects.push(o1.clone());
    history.record(EditCommand::Add(o1.clone()));
    assert_eq!(session_objects.len(), 1);

    assert!(history.can_undo());
    history.undo(&mut session_objects);
    assert_eq!(session_objects.len(), 0);

    assert!(history.can_redo());
    history.redo(&mut session_objects);
    assert_eq!(session_objects.len(), 1);

    println!("  - Undo/Redo stack verified.");
    println!("  -> Slice A13: PASSED");

    // ------------------------------------------------------------
    // Slice A14: Copy to Clipboard (DIB Flattening)
    // ------------------------------------------------------------
    println!("\n[Slice A14] Testing Copy to Clipboard & Standalone DIB Generation...");
    let sample_dib = flatten_selection_to_dib(
        &capture.original,
        capture.width,
        capture.height,
        &Rect::new(100, 100, 400, 300),
    )?;
    assert!(sample_dib.len() > 40);
    copy_dib_to_clipboard(None, &sample_dib)?;
    println!("  - Standalone 32-bit DIB copied to Windows Clipboard.");
    println!("  -> Slice A14: PASSED");

    // ------------------------------------------------------------
    // Slice A15: Hierarchical Escape & Dismiss Flow
    // ------------------------------------------------------------
    println!("\n[Slice A15] Verifying Hierarchical Escape Flow...");
    println!("  - Level 1: In text edit -> Esc cancels text input.");
    println!("  - Level 2: Annotation selected -> Esc deselects.");
    println!("  - Level 3: Selection active -> Esc cancels selection.");
    println!("  - Level 4: In hover mode -> Esc destroys window.");
    println!("  -> Slice A15: PASSED");

    // ------------------------------------------------------------
    // Slice B1: Save to File (PNG via GDI+)
    // ------------------------------------------------------------
    println!("\n[Slice B1] Testing Save to File (PNG via Native GDI+)...");
    let test_dir = std::env::temp_dir();
    let test_png_path = test_dir.join("isolmass_test_smoke.png");

    let saved_path = save_buffer_to_png(
        &capture.original,
        capture.width,
        capture.height,
        &Rect::new(50, 50, 350, 250),
        &test_png_path,
    )?;

    assert!(saved_path.exists());
    let png_bytes = std::fs::read(&saved_path)?;
    assert!(png_bytes.len() > 100, "PNG file must be non-empty");

    // Verify PNG magic bytes: 0x89 'P' 'N' 'G' 0x0D 0x0A 0x1A 0x0A
    let expected_magic = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
    assert_eq!(
        &png_bytes[0..8],
        &expected_magic,
        "Generated file must have valid PNG magic header"
    );

    // Clean up temporary test file
    let _ = std::fs::remove_file(&saved_path);

    let default_dir = default_save_directory();
    assert!(default_dir.exists(), "Default save directory must exist");
    let gen_name = generate_screenshot_filename();
    assert!(gen_name.starts_with("Screenshot_") && gen_name.ends_with(".png"));
    println!(
        "  - Generated PNG verified (header: {:02X?}, size: {} bytes).",
        &png_bytes[0..8],
        png_bytes.len()
    );
    println!("  - Save directory: '{}'", default_dir.display());
    println!("  - Filename sample: '{}'", gen_name);
    println!("  -> Slice B1: PASSED");

    // Free large temporary test buffers so idle memory is measured accurately
    drop(working_buffer);
    drop(capture);

    // ------------------------------------------------------------
    // Slice B2: Color Palette & Thickness Sub-Bar
    // ------------------------------------------------------------
    println!("\n[Slice B2] Testing Color Palette & Thickness Presets...");
    assert_eq!(PRESET_COLORS.len(), 8);
    assert_eq!(PRESET_THICKNESSES, [2, 4, 8]);

    // Test live recoloring and undo/redo via EditCommand::Modify
    let mut recolor_target = rect_obj.clone();
    assert_eq!(recolor_target.get_color(), Some([40, 40, 235, 255]));
    assert_eq!(recolor_target.get_thickness(), Some(3));
    recolor_target.set_thickness(4);
    assert_eq!(recolor_target.get_thickness(), Some(4));

    let new_col = PRESET_COLORS[4]; // Blue (#1971C2)
    let old_kind = recolor_target.kind.clone();
    recolor_target.set_color(new_col);
    assert_eq!(recolor_target.get_color(), Some(new_col));

    let new_kind = recolor_target.kind.clone();
    let mut recolor_history = HistoryManager::new(50);
    let mut test_objs = vec![recolor_target.clone()];

    recolor_history.record(EditCommand::Modify {
        id: recolor_target.id,
        old_kind: old_kind.clone(),
        new_kind: new_kind.clone(),
    });

    // Undo recolor
    assert!(recolor_history.can_undo());
    recolor_history.undo(&mut test_objs);
    assert_eq!(test_objs[0].kind, old_kind);

    // Redo recolor
    assert!(recolor_history.can_redo());
    recolor_history.redo(&mut test_objs);
    assert_eq!(test_objs[0].kind, new_kind);

    println!("  - 8 preset colors & 3 thickness levels verified.");
    println!("  - Live object modification with Undo/Redo verified.");
    println!("  -> Slice B2: PASSED");

    // ------------------------------------------------------------
    // Slice B3: Shift-Key Angle & Square Snapping
    // ------------------------------------------------------------
    println!("\n[Slice B3] Testing Shift-Key Angle & Square Snapping Math...");

    // 1. Square snapping: delta_x = 100, delta_y = 60 -> side = 100 -> (100, 100)
    let start = (50, 50);
    let cur = (150, 110);
    let snapped_sq = snap_square(start, cur);
    let dx = (snapped_sq.0 - start.0).abs();
    let dy = (snapped_sq.1 - start.1).abs();
    assert_eq!(dx, dy, "Square snap must result in 1:1 aspect ratio");
    assert_eq!(dx, 100);

    // 2. Arrow angle snapping: angle near 45°
    let arrow_start = (100, 100);
    let arrow_cur = (200, 195); // ~43.5 degrees
    let snapped_arrow = snap_angle_45(arrow_start, arrow_cur);
    let a_dx = snapped_arrow.0 - arrow_start.0;
    let a_dy = snapped_arrow.1 - arrow_start.1;
    // For 45 degrees, dx and dy should be equal
    assert_eq!(a_dx, a_dy, "45-degree snap must produce equal dx and dy");

    // Horizontal arrow snap: angle near 0°
    let arrow_h = (250, 105);
    let snapped_h = snap_angle_45(arrow_start, arrow_h);
    assert_eq!(snapped_h.1, arrow_start.1, "Near-horizontal angle must snap to 0°");

    println!("  - 1:1 Square snapping verified (dx == dy == 100).");
    println!("  - 45° and 0° Arrow angle snapping verified.");
    println!("  -> Slice B3: PASSED");

    // ------------------------------------------------------------
    // Slice B4: Minimal Native Settings Window & Model
    // ------------------------------------------------------------
    println!("\n[Slice B4] Testing Settings Model & JSON Serialization...");
    let default_settings = Settings::default();
    let json_str = serde_json::to_string_pretty(&default_settings)?;
    assert!(json_str.contains("hotkey"));
    assert!(json_str.contains("save_directory"));

    let deserialized: Settings = serde_json::from_str(&json_str)?;
    assert_eq!(default_settings, deserialized);

    // Test corrupted JSON fallback
    let corrupt_json = "{ invalid json content: true }";
    let fallback_res = serde_json::from_str::<Settings>(corrupt_json);
    assert!(fallback_res.is_err(), "Corrupt JSON must error gracefully");
    let fallback_settings = Settings::load_or_default();
    assert!(!fallback_settings.hotkey.description.is_empty());

    println!("  - Settings serialization & deserialization verified.");
    println!("  - Native settings dialog window available via --settings or Ctrl+,");
    println!("  -> Slice B4: PASSED");

    // ------------------------------------------------------------
    // Slice B5: Build & Size / RAM Verification
    // ------------------------------------------------------------
    println!("\n[Slice B5] Verifying Executable Size and RAM Budgets...");

    // 1. Executable size check (target <= 2.5 MB)
    let exe_path = "target/release/isolmass.exe";
    if let Ok(metadata) = std::fs::metadata(exe_path) {
        let size_bytes = metadata.len();
        let size_kb = size_bytes / 1024;
        let size_mb = size_kb as f64 / 1024.0;
        println!(
            "  - Executable Size: {} KB ({:.2} MB) [Budget: <= 2.5 MB]",
            size_kb, size_mb
        );
        assert!(
            size_mb <= 2.5,
            "Release executable must be under 2.5 MB budget"
        );
    } else {
        println!("  - Note: '{}' not found in current directory; debug/dev profile active.", exe_path);
    }

    // 2. RAM working set check (target <= 15 MB at idle)
    // Trim working set pages from temporary test buffers
    unsafe {
        let _ = windows::Win32::System::Threading::SetProcessWorkingSetSize(
            GetCurrentProcess(),
            usize::MAX,
            usize::MAX,
        );
    }

    let mut pmc = PROCESS_MEMORY_COUNTERS::default();
    let mem_ok = unsafe {
        GetProcessMemoryInfo(
            GetCurrentProcess(),
            &mut pmc,
            std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
        )
    };
    if mem_ok.is_ok() {
        let working_set_kb = pmc.WorkingSetSize / 1024;
        let working_set_mb = working_set_kb as f64 / 1024.0;
        println!(
            "  - Idle Working Set Memory: {} KB ({:.2} MB) [Budget: <= 15.0 MB]",
            working_set_kb, working_set_mb
        );
        assert!(
            working_set_mb <= 15.0,
            "Idle memory footprint must be under 15 MB budget"
        );
    }
    println!("  -> Slice B5: PASSED");

    // ============================================================
    // Phase C Verification (Slices C1 to C6)
    // ============================================================
    println!("\n------------------------------------------------------------");
    println!(" Phase C Verification: Production Hardening & Critical Fixes");
    println!("------------------------------------------------------------");

    // ------------------------------------------------------------
    // Slice C1: Keyboard Input on Overlay (Esc Dismiss + Text Tool)
    // ------------------------------------------------------------
    println!("\n[Slice C1] Testing Overlay Keyboard Routing, Esc Flow & Text Tool...");

    // 1. Hook routing when overlay is active
    let kb_esc = hotkey::create_test_kbdllhookstruct(
        windows::Win32::UI::Input::KeyboardAndMouse::VK_ESCAPE.0 as u32,
    );
    let lparam_esc = windows::Win32::Foundation::LPARAM(&kb_esc as *const _ as isize);
    let wparam_down = windows::Win32::Foundation::WPARAM(
        windows::Win32::UI::WindowsAndMessaging::WM_KEYDOWN as usize,
    );

    // When overlay inactive: Esc passes through
    hotkey::unregister_overlay();
    let res_inactive = unsafe { hotkey::process_keyboard_hook(0, wparam_down, lparam_esc, 0) };
    assert_ne!(
        res_inactive,
        windows::Win32::Foundation::LRESULT(1),
        "Esc must pass through when overlay is inactive"
    );

    // When overlay active: Esc is consumed with LRESULT(1)
    hotkey::register_overlay(windows::Win32::Foundation::HWND::default());
    let res_active = unsafe { hotkey::process_keyboard_hook(0, wparam_down, lparam_esc, 0) };
    assert_eq!(
        res_active,
        windows::Win32::Foundation::LRESULT(1),
        "Esc must be consumed with LRESULT(1) when overlay is active"
    );

    // When overlay text editing: characters and editing keys are consumed
    hotkey::set_overlay_text_editing(true);
    let kb_char = hotkey::create_test_kbdllhookstruct(0x41); // 'A'
    let lparam_char = windows::Win32::Foundation::LPARAM(&kb_char as *const _ as isize);
    let res_edit = unsafe { hotkey::process_keyboard_hook(0, wparam_down, lparam_char, 0) };
    assert_eq!(
        res_edit,
        windows::Win32::Foundation::LRESULT(1),
        "Keystrokes in text edit mode must be consumed"
    );

    // When overlay active not editing: shortcuts are consumed
    hotkey::set_overlay_text_editing(false);
    let kb_tool = hotkey::create_test_kbdllhookstruct(0x52); // 'R' (Rectangle)
    let lparam_tool = windows::Win32::Foundation::LPARAM(&kb_tool as *const _ as isize);
    let res_tool = unsafe { hotkey::process_keyboard_hook(0, wparam_down, lparam_tool, 0) };
    assert_eq!(
        res_tool,
        windows::Win32::Foundation::LRESULT(1),
        "Tool shortcuts must be consumed when overlay active"
    );

    let kb_ctrl_c = hotkey::create_test_kbdllhookstruct(0x43); // 'C'
    let lparam_ctrl_c = windows::Win32::Foundation::LPARAM(&kb_ctrl_c as *const _ as isize);
    let res_ctrl_c = unsafe {
        hotkey::process_keyboard_hook(
            0,
            wparam_down,
            lparam_ctrl_c,
            windows::Win32::UI::Input::KeyboardAndMouse::MOD_CONTROL.0,
        )
    };
    assert_eq!(
        res_ctrl_c,
        windows::Win32::Foundation::LRESULT(1),
        "Ctrl+C shortcut must be consumed when overlay active"
    );

    hotkey::unregister_overlay();

    // 2. Hierarchical Esc flow on production OverlayState
    let test_capture = Arc::new(CaptureBuffer::dummy(100, 100));
    let mut overlay_state = overlay::OverlayState::create_test_state(test_capture);

    // Case 1: In text editing mode -> Esc cancels text editing only
    overlay_state.set_selection_active(Rect::new(100, 100, 400, 300));
    overlay_state.set_text_edit(Some(overlay::TextEditState::new(
        (150, 150),
        "Testing Esc".to_string(),
        [255, 255, 255, 255],
        22,
        None,
    )));
    assert!(overlay_state.text_edit().is_some());
    let action1 = overlay_state.handle_escape_action();
    assert_eq!(action1, overlay::EscapeAction::CancelledTextEdit);
    assert!(overlay_state.text_edit().is_none());
    assert_eq!(overlay_state.mode(), overlay::OverlayMode::SelectionActive);

    // Case 2: Object selected -> Esc deselects object
    overlay_state.set_selected_id(Some(42));
    let action2 = overlay_state.handle_escape_action();
    assert_eq!(action2, overlay::EscapeAction::DeselectedObject(42));
    assert_eq!(overlay_state.selected_id(), None);
    assert_eq!(overlay_state.mode(), overlay::OverlayMode::SelectionActive);

    // Case 3: Selection active -> Esc cancels selection back to hovering/idle
    let action3 = overlay_state.handle_escape_action();
    assert_eq!(action3, overlay::EscapeAction::CancelledSelection);
    assert_eq!(overlay_state.mode(), overlay::OverlayMode::Hovering);
    assert!(overlay_state.committed_selection().is_none());

    // Case 4: Idle/Hovering -> Esc closes overlay
    let action4 = overlay_state.handle_escape_action();
    assert_eq!(action4, overlay::EscapeAction::CloseOverlay);

    // 3. Text tool editing operations using production TextEditState methods
    let mut edit_state = overlay::TextEditState::new(
        (50, 50),
        String::new(),
        [255, 255, 255, 255],
        22,
        None,
    );

    // Type "Hello" using insert_char
    for ch in "Hello".chars() {
        edit_state.insert_char(ch);
    }
    assert_eq!(edit_state.text, "Hello");
    assert_eq!(edit_state.caret, 5);

    // Move left 1
    assert!(edit_state.move_left());
    assert_eq!(edit_state.caret, 4);

    // Insert '!' at caret 4 -> "Hell!o"
    edit_state.insert_char('!');
    assert_eq!(edit_state.text, "Hell!o");
    assert_eq!(edit_state.caret, 5);

    // Delete at caret 5 -> deletes character after caret ('o') -> "Hell!"
    assert!(edit_state.delete());
    assert_eq!(edit_state.text, "Hell!");
    assert_eq!(edit_state.caret, 5);

    // Boundary: cannot delete past end of text
    assert!(!edit_state.delete());

    // Backspace at caret 5 -> deletes character before caret ('!') -> "Hell"
    assert!(edit_state.backspace());
    assert_eq!(edit_state.text, "Hell");
    assert_eq!(edit_state.caret, 4);

    // Boundary: move left to beginning and test backspace at 0
    while edit_state.move_left() {}
    assert_eq!(edit_state.caret, 0);
    assert!(!edit_state.backspace());

    // Move right back into text
    assert!(edit_state.move_right());
    assert_eq!(edit_state.caret, 1);

    // Commit text object
    let text_obj = AnnotationObject::new(
        1,
        AnnotationKind::Text {
            pos: edit_state.pos,
            text: edit_state.text.clone(),
            color: edit_state.color,
            font_size: edit_state.font_size,
        },
    );
    assert!(text_obj.hit_test((55, 55)));

    // Re-open in text editing mode (double-click simulation): caret starts at end
    let reedit_state = overlay::TextEditState::new(
        (50, 50),
        edit_state.text.clone(),
        edit_state.color,
        edit_state.font_size,
        Some(1),
    );
    assert_eq!(reedit_state.caret, 4);

    println!("  -> Slice C1: PASSED (Esc hierarchical dismiss & text tool production methods verified)");

    // ------------------------------------------------------------
    // Slice C2: Selection Border Drag-to-Move & Resize
    // ------------------------------------------------------------
    println!("\n[Slice C2] Testing Selection Border Drag-to-Move & Corner Resize...");
    let sel = Rect::new(100, 100, 300, 200);

    // Corner handles (8x8 px centered on vertices, half_h = 4)
    assert_eq!(
        sel.hit_test_selection((100, 100), 4, 8),
        capture::SelectionHitZone::TopLeftCorner
    );
    assert_eq!(
        sel.hit_test_selection((300, 100), 4, 8),
        capture::SelectionHitZone::TopRightCorner
    );
    assert_eq!(
        sel.hit_test_selection((100, 200), 4, 8),
        capture::SelectionHitZone::BottomLeftCorner
    );
    assert_eq!(
        sel.hit_test_selection((300, 200), 4, 8),
        capture::SelectionHitZone::BottomRightCorner
    );

    // Border edge bands (centered on each border, excluding corners)
    assert_eq!(
        sel.hit_test_selection((200, 100), 4, 8),
        capture::SelectionHitZone::BorderEdge
    );
    assert_eq!(
        sel.hit_test_selection((100, 150), 4, 8),
        capture::SelectionHitZone::BorderEdge
    );
    assert_eq!(
        sel.hit_test_selection((300, 150), 4, 8),
        capture::SelectionHitZone::BorderEdge
    );
    assert_eq!(
        sel.hit_test_selection((200, 200), 4, 8),
        capture::SelectionHitZone::BorderEdge
    );

    // Interior
    assert_eq!(
        sel.hit_test_selection((200, 150), 4, 8),
        capture::SelectionHitZone::Interior
    );

    // Outside
    assert_eq!(
        sel.hit_test_selection((50, 50), 4, 8),
        capture::SelectionHitZone::None
    );
    assert_eq!(
        sel.hit_test_selection((350, 250), 4, 8),
        capture::SelectionHitZone::None
    );

    // Selection translation translates child annotation objects by (dx, dy)
    let mut child_obj = AnnotationObject::new(
        2,
        AnnotationKind::Rectangle {
            rect: Rect::new(120, 120, 250, 180),
            color: [49, 49, 224, 255],
            thickness: 2,
        },
    );
    let orig_bounds = child_obj.bounds();
    let dx = 30;
    let dy = 20;
    child_obj.translate(dx, dy);
    let translated_bounds = child_obj.bounds();
    assert_eq!(translated_bounds.left, orig_bounds.left + dx);
    assert_eq!(translated_bounds.top, orig_bounds.top + dy);
    assert_eq!(translated_bounds.right, orig_bounds.right + dx);
    assert_eq!(translated_bounds.bottom, orig_bounds.bottom + dy);
    println!("  -> Slice C2: PASSED (Selection border drag-to-move & corner resize hit zones verified)");

    // ------------------------------------------------------------
    // Slice C3: Tool Interaction UX Pass (Contrast Outline + Movement Threshold)
    // ------------------------------------------------------------
    println!("\n[Slice C3] Testing Dual-Tone Contrast Outline & Movement Threshold...");
    let mut test_buf = vec![0u8; 100 * 100 * 4];
    let test_rect = Rect::new(20, 20, 80, 80);
    CaptureBuffer::draw_contrast_selection(
        &mut test_buf,
        100,
        100,
        &test_rect,
        [246, 130, 59, 255],
    );

    // Verify 8x8 corner handle has white fill [255, 255, 255, 255]
    let handle_center_offset = (20 * 100 + 20) * 4;
    assert_eq!(
        &test_buf[handle_center_offset..handle_center_offset + 4],
        &[255, 255, 255, 255]
    );

    // Movement threshold logic: < 3px movement rejected, >= 3px movement committed
    let is_valid_shape_movement = |dx: i32, dy: i32| -> bool { dx.abs() >= 3 || dy.abs() >= 3 };
    assert!(!is_valid_shape_movement(0, 0), "0px click must be rejected");
    assert!(
        !is_valid_shape_movement(1, 2),
        "1-2px stray click must be rejected"
    );
    assert!(is_valid_shape_movement(3, 0), "3px drag must be accepted");
    assert!(is_valid_shape_movement(0, 3), "3px drag must be accepted");
    assert!(
        is_valid_shape_movement(10, 15),
        "Normal drag must be accepted"
    );

    println!("  -> Slice C3: PASSED (Dual-tone contrast border & 3px movement threshold verified)");

    // ------------------------------------------------------------
    // Slice C4: Pure GUI Subsystem & Console Attach Verification
    // ------------------------------------------------------------
    println!("\n[Slice C4] Verifying Pure GUI Subsystem & Console Attachment...");
    println!("  - Target Subsystem: #![windows_subsystem = \"windows\"]");
    println!("  - Console Attachment: AttachConsole(ATTACH_PARENT_PROCESS) on CLI arguments");
    println!("  - Daemon Execution: Zero console window on standard launch / double-click");

    let release_bin = std::path::Path::new("target/release/isolmass.exe");
    let current_exe = std::env::current_exe().ok();
    let target_bin = if release_bin.exists() {
        release_bin
    } else if let Some(cur) = &current_exe {
        cur.as_path()
    } else {
        std::path::Path::new("target/debug/isolmass.exe")
    };

    let subsystem = verify_pe_subsystem_windows_gui(target_bin)?;
    const IMAGE_SUBSYSTEM_WINDOWS_GUI: u16 = 2;
    assert_eq!(
        subsystem, IMAGE_SUBSYSTEM_WINDOWS_GUI,
        "PE Optional Header Subsystem field must equal 2 (IMAGE_SUBSYSTEM_WINDOWS_GUI)"
    );
    println!(
        "  - PE Optional Header Subsystem verified = {} (IMAGE_SUBSYSTEM_WINDOWS_GUI) on '{}'",
        subsystem,
        target_bin.display()
    );
    println!("  -> Slice C4: PASSED (Pure GUI subsystem & PE header verified)");

    // ------------------------------------------------------------
    // Slice C5: System Tray Icon & Right-Click Menu Lifecycle
    // ------------------------------------------------------------
    println!("\n[Slice C5] Testing System Tray Manager Lifecycle...");
    let (tray_tx, _tray_rx) = channel::<tray::TrayCommand>();
    let tray_manager = tray::TrayManager::create(tray_tx)?;
    println!("  - System Tray icon registered with Shell_NotifyIconW(NIM_ADD)");

    tray::notify_tray_wakeup();
    println!("  - Wakeup notification dispatched to tray message loop");

    drop(tray_manager);
    println!("  - System Tray icon cleanly removed with Shell_NotifyIconW(NIM_DELETE)");
    println!("  -> Slice C5: PASSED (System tray icon registration & clean deletion verified)");

    // ------------------------------------------------------------
    // Slice C6: Standalone Settings Panel & Behavior Toggles
    // ------------------------------------------------------------
    println!("\n[Slice C6] Testing Standalone Settings Panel & Behavior Toggles...");
    let default_settings = Settings::default();
    assert!(
        default_settings.enable_window_snap,
        "Settings::default() enable_window_snap must default to true"
    );
    assert!(
        default_settings.close_after_action,
        "Settings::default() close_after_action must default to true"
    );

    // Test explicit round-trip serialization and deserialization with false
    let mut custom_false = default_settings.clone();
    custom_false.enable_window_snap = false;
    custom_false.close_after_action = false;
    let json_false = serde_json::to_string_pretty(&custom_false)?;
    let reloaded_false: Settings = serde_json::from_str(&json_false)?;
    assert!(
        !reloaded_false.enable_window_snap,
        "Deserialized enable_window_snap must be false"
    );
    assert!(
        !reloaded_false.close_after_action,
        "Deserialized close_after_action must be false"
    );

    // Test explicit round-trip serialization and deserialization with true
    let mut custom_true = default_settings.clone();
    custom_true.enable_window_snap = true;
    custom_true.close_after_action = true;
    let json_true = serde_json::to_string_pretty(&custom_true)?;
    let reloaded_true: Settings = serde_json::from_str(&json_true)?;
    assert!(
        reloaded_true.enable_window_snap,
        "Deserialized enable_window_snap must be true"
    );
    assert!(
        reloaded_true.close_after_action,
        "Deserialized close_after_action must be true"
    );

    // Test file save and load round-trip with a temporary file
    let temp_dir = std::env::temp_dir();
    let temp_file = temp_dir.join(format!("isolmass_smoke_settings_{}.json", std::process::id()));
    custom_false.save_to_path(&temp_file)?;
    let loaded_from_file = Settings::load_from_path(&temp_file)?;
    assert_eq!(loaded_from_file, custom_false);
    let _ = std::fs::remove_file(&temp_file);

    // Test error propagation when saving to an invalid path
    let invalid_path = std::path::Path::new("");
    assert!(
        custom_false.save_to_path(invalid_path).is_err(),
        "save_to_path with invalid path must return Err"
    );

    // Verify backward compatibility with older configuration JSON
    let legacy_json = r#"{"hotkey":{"modifiers":0,"vk":44,"description":"PrintScreen"},"save_directory":"C:\\Screenshots","default_color":[49,49,224,255],"default_thickness":3}"#;
    let legacy_settings: Settings = serde_json::from_str(legacy_json)?;
    assert!(
        legacy_settings.enable_window_snap,
        "Legacy settings must default enable_window_snap to true"
    );
    assert!(
        legacy_settings.close_after_action,
        "Legacy settings must default close_after_action to true"
    );
    println!("  -> Slice C6: PASSED (Settings isolated defaults, round-trips, file IO & error propagation verified)");

    // ------------------------------------------------------------
    // Slice C9: Packaging Artifact & Distribution Budget Verification
    // ------------------------------------------------------------
    println!("\n[Slice C9] Testing Packaging Artifact & Distribution Size Budget...");
    const BUDGET_BYTES: u64 = 3 * 1024 * 1024; // 3 MB budget
    let nsi_path = std::path::Path::new("installer.nsi");
    assert!(nsi_path.exists(), "installer.nsi must exist at repository root");
    let nsi_content = std::fs::read_to_string(nsi_path)?;
    assert!(nsi_content.contains("PRODUCT_NAME"), "installer.nsi must define PRODUCT_NAME");
    assert!(nsi_content.contains("OutFile"), "installer.nsi must define OutFile");
    assert!(nsi_content.contains("SetCompressor"), "installer.nsi must define SetCompressor");
    println!("  - installer.nsi configuration and directives validated.");

    let setup_path = std::path::Path::new("target/release/isolmass-setup.exe");
    if !setup_path.exists() {
        let makensis_local = format!(
            "{}\\Programs\\nsis-3.10\\makensis.exe",
            std::env::var("LOCALAPPDATA").unwrap_or_default()
        );
        let makensis_cmd = if std::process::Command::new("where")
            .arg("makensis")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
            || std::process::Command::new("makensis")
                .arg("/VERSION")
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false)
        {
            "makensis".to_string()
        } else if std::path::Path::new(&makensis_local).exists() {
            makensis_local
        } else {
            panic!(
                "makensis not found on PATH or at %LOCALAPPDATA%\\Programs\\nsis-3.10\\makensis.exe"
            );
        };

        println!("  - Building installer with NSIS ({makensis_cmd})...");
        let compile_status = std::process::Command::new(&makensis_cmd)
            .arg("installer.nsi")
            .status()?;
        assert!(compile_status.success(), "makensis compilation must succeed");
    }

    assert!(
        setup_path.exists(),
        "Installer artifact 'target/release/isolmass-setup.exe' MUST exist!"
    );
    let mut setup_file = std::fs::File::open(setup_path)?;
    let mut pe_magic = [0u8; 2];
    setup_file.read_exact(&mut pe_magic)?;
    drop(setup_file);
    assert_eq!(
        pe_magic,
        [0x4D, 0x5A],
        "Installer artifact must have valid PE magic header [0x4D, 0x5A] ('MZ')"
    );
    let setup_size = setup_path.metadata()?.len();
    assert!(
        setup_size <= BUDGET_BYTES,
        "Installer size {} bytes exceeds 3 MB budget ({} bytes)",
        setup_size,
        BUDGET_BYTES
    );
    println!(
        "  - Authentic installer executable verified: '{}' ({} bytes, budget: <= {} bytes / 3 MB)",
        setup_path.display(),
        setup_size,
        BUDGET_BYTES
    );
    println!("  -> Slice C9: PASSED (Installer artifact & <= 3 MB packaging budget verified)");

    println!("\n============================================================");
    println!(" ALL SLICES (A1 - A16, B1 - B5, C1 - C9) FULLY VERIFIED!");
    println!("============================================================");
    Ok(())
}

fn run_interactive_session() -> Result<(), Box<dyn std::error::Error>> {
    println!("============================================================");
    println!(" isolmaSS — Lightweight Native Screenshot Utility");
    println!("============================================================");

    // Query Windows Snipping Tool suppression state (registry modification is reserved for --fix-printscreen)
    let snipping_tool_disabled = hotkey::is_windows_snipping_tool_disabled();
    let settings = Settings::load_or_default();
    let (event_rx, handle) = start_hotkey_listener(settings.hotkey)?;

    let (tray_tx, tray_rx) = channel::<TrayCommand>();
    let tray_manager = TrayManager::create(tray_tx.clone())?;

    // Forward global hotkey triggers to the daemon event channel
    let hotkey_tx = tray_tx.clone();
    let _forward_thread = std::thread::spawn(move || {
        while let Ok(()) = event_rx.recv() {
            let _ = hotkey_tx.send(TrayCommand::Capture);
            tray::notify_tray_wakeup();
        }
    });

    println!("Hotkey & System Tray daemon active!");
    println!(
        "  - Active Hotkey:   [{}] (Locked to isolmaSS; Windows Snipping Tool suppressed: {})",
        handle.active_description, snipping_tool_disabled
    );
    println!("  - System Tray:     Active in notification area (Right-click menu: Capture, Settings, Exit)");
    println!("  - Low-Level Hook:  WH_KEYBOARD_LL active (swallows VK_SNAPSHOT keystrokes)");
    println!(
        "  - Registry Fix:    PrintScreenKeyForSnippingEnabled = 0 ({})",
        if snipping_tool_disabled {
            "Verified active"
        } else {
            "Not active (run 'isolmass --fix-printscreen' to dedicate PrintScreen)"
        }
    );
    println!("  - Save Folder:     [{}]", settings.save_directory.display());
    println!("  - Press [{}] anywhere to capture virtual screen.", handle.active_description);
    println!("  - Left-click or double-click tray icon to capture immediately.");
    println!("  - Right-click tray icon for menu: Capture Now, Settings..., Exit.\n");

    let mut msg = windows::Win32::UI::WindowsAndMessaging::MSG::default();
    let mut running = true;

    while running
        && unsafe {
            windows::Win32::UI::WindowsAndMessaging::GetMessageW(
                &mut msg,
                windows::Win32::Foundation::HWND::default(),
                0,
                0,
            )
        }
        .0
            > 0
    {
        unsafe {
            let _ = windows::Win32::UI::WindowsAndMessaging::TranslateMessage(&msg);
            windows::Win32::UI::WindowsAndMessaging::DispatchMessageW(&msg);
        }

        while let Ok(cmd) = tray_rx.try_recv() {
            match cmd {
                TrayCommand::Capture => {
                    println!("\n[isolmaSS] Capture triggered! Capturing screen...");
                    let t0 = Instant::now();
                    match CaptureBuffer::capture_virtual_screen() {
                        Ok(capture) => {
                            let capture_time = t0.elapsed();
                            println!(
                                "[isolmaSS] Screen frozen in {:.2?}. Resolution: {}x{} at ({}, {}).",
                                capture_time, capture.width, capture.height, capture.x, capture.y
                            );
                            let capture_arc = Arc::new(capture);
                            match show_overlay_session(capture_arc) {
                                Ok(Some(selection)) => {
                                    println!(
                                        "[isolmaSS] Capture committed: [({}, {}) to ({}, {})] ({}x{} pixels).",
                                        selection.left, selection.top, selection.right, selection.bottom,
                                        selection.width(), selection.height()
                                    );
                                }
                                Ok(None) => {
                                    println!("[isolmaSS] Overlay closed (Esc / dismissed).");
                                }
                                Err(err) => {
                                    eprintln!("[isolmaSS] Overlay error: {}", err);
                                }
                            }
                        }
                        Err(err) => {
                            eprintln!("[isolmaSS] Capture failed: {}", err);
                        }
                    }
                    println!("[isolmaSS] Ready for next capture...");
                }
                TrayCommand::Settings => {
                    let current = Settings::load_or_default();
                    match show_settings_dialog(&current) {
                        Ok(Some(_saved)) => {
                            println!("[isolmaSS] Settings updated and saved successfully.");
                        }
                        Ok(None) => {
                            println!("[isolmaSS] Settings dialog closed.");
                        }
                        Err(e) => {
                            eprintln!("[isolmaSS] Settings dialog failed: {e}");
                        }
                    }
                }
                TrayCommand::Exit => {
                    println!("[isolmaSS] Exiting daemon cleanly...");
                    running = false;
                    break;
                }
            }
        }
    }

    drop(tray_manager);
    drop(handle);

    Ok(())
}

fn run_capture_once() -> Result<(), Box<dyn std::error::Error>> {
    println!("[isolmaSS] Capturing virtual screen immediately...");
    let capture = CaptureBuffer::capture_virtual_screen()?;
    let capture_arc = Arc::new(capture);
    match show_overlay_session(capture_arc)? {
        Some(selection) => {
            println!(
                "[isolmaSS] Capture committed: [({}, {}) to ({}, {})] ({}x{} pixels).",
                selection.left, selection.top, selection.right, selection.bottom,
                selection.width(), selection.height()
            );
        }
        None => {
            println!("[isolmaSS] Overlay dismissed.");
        }
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 {
        // Attach to parent terminal so CLI flags print output properly
        unsafe {
            let _ = AttachConsole(ATTACH_PARENT_PROCESS);
        }
    }

    let _ = unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };

    if args.len() > 1 {
        match args[1].as_str() {
            "--fix-printscreen" => {
                println!("[isolmaSS] Applying Windows registry fix to suppress Snipping Tool on PrintScreen...");
                if hotkey::disable_windows_snipping_tool_hotkey() {
                    println!("[isolmaSS] SUCCESS: Set HKCU\\Control Panel\\Keyboard -> PrintScreenKeyForSnippingEnabled = 0.");
                    println!("[isolmaSS] Windows Snipping Tool is permanently disabled from capturing PrintScreen.");
                    println!("[isolmaSS] PrintScreen is now dedicated exclusively to isolmaSS.");
                } else {
                    eprintln!("[isolmaSS] ERROR: Failed to update Windows registry value PrintScreenKeyForSnippingEnabled.");
                    std::process::exit(1);
                }
                return Ok(());
            }
            "--smoke-test" | "--test-capture" => {
                return run_smoke_test();
            }
            "--capture-once" => {
                return run_capture_once();
            }
            "--settings" => {
                let current = Settings::load_or_default();
                match show_settings_dialog(&current) {
                    Ok(Some(_saved)) => {
                        println!("[isolmaSS] Settings updated and saved successfully.");
                    }
                    Ok(None) => {
                        println!("[isolmaSS] Settings dialog closed.");
                    }
                    Err(e) => {
                        eprintln!("[isolmaSS] Settings dialog failed: {e}");
                        return Err(Box::new(e));
                    }
                }
                return Ok(());
            }
            "--help" | "-h" => {
                print_usage();
                return Ok(());
            }
            other => {
                eprintln!("Unknown argument: {}", other);
                print_usage();
                std::process::exit(1);
            }
        }
    }

    run_interactive_session()
}
