#![windows_subsystem = "windows"]

mod annotation;
mod capture;
mod clipboard;
mod hotkey;
mod instance;
mod overlay;
mod save;
mod settings;
mod settings_window;
mod smoke;
mod startup;
mod toolbar;
mod tray;
mod updater;
mod window_snap;

use capture::CaptureBuffer;
use hotkey::{HotkeyConfig, HotkeyHandle, start_hotkey_listener};
use instance::InstanceState;
use overlay::show_overlay_session;
use settings::{Settings, show_settings_dialog};
use std::path::Path;
use std::rc::Rc;
use std::sync::mpsc::{Sender, channel};
use std::time::Instant;
use tray::{TrayCommand, TrayManager};
use windows::Win32::System::Console::{ATTACH_PARENT_PROCESS, AttachConsole};
use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext,
};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
use windows::core::{PCWSTR, w};

fn print_usage() {
    println!("isolmaSS - Lightweight Native Windows Screenshot Utility");
    println!("Usage:");
    println!("  isolmass                   Run interactive hotkey daemon (PrtScn / fallback)");
    println!(
        "  isolmass --fix-printscreen Apply registry fix to disable Windows Snipping Tool on PrtScn"
    );
    println!("  isolmass --smoke-test      Run automated verification of Phase A & B (A1 to B5)");
    println!("  isolmass --test-capture    Alias for --smoke-test");
    println!("  isolmass --capture-once    Capture immediately and open overlay once");
    println!("  isolmass --settings        Open native settings dialog");
    println!("  isolmass --check-update    Check GitHub Releases for a newer signed version");
    println!("  isolmass --help            Show this help message");
}

fn start_hotkey_runtime(
    config: HotkeyConfig,
    daemon_tx: Sender<TrayCommand>,
) -> Result<(HotkeyHandle, std::thread::JoinHandle<()>), Box<dyn std::error::Error>> {
    let (event_rx, handle) = start_hotkey_listener(config)?;
    let forward_thread = std::thread::spawn(move || {
        while let Ok(()) = event_rx.recv() {
            let _ = daemon_tx.send(TrayCommand::Capture);
            tray::notify_tray_wakeup();
        }
    });
    Ok((handle, forward_thread))
}

fn rollback_hotkey_setting(settings: &mut Settings, previous: &HotkeyConfig) -> Result<(), String> {
    settings.hotkey = previous.clone();
    settings
        .save()
        .map_err(|error| format!("Could not restore the previous saved hotkey: {error}"))
}

fn open_recent_capture(path: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    let wide_path: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let result = unsafe {
        ShellExecuteW(
            windows::Win32::Foundation::HWND::default(),
            w!("open"),
            PCWSTR(wide_path.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    if result.0 as isize <= 32 {
        Err(format!("Windows could not open '{}'.", path.display()))
    } else {
        Ok(())
    }
}

fn run_interactive_session() -> Result<(), Box<dyn std::error::Error>> {
    let _instance = match instance::acquire_or_notify()? {
        InstanceState::Primary(instance) => instance,
        InstanceState::ExistingNotified => return Ok(()),
    };

    println!("============================================================");
    println!(" isolmaSS — Lightweight Native Screenshot Utility");
    println!("============================================================");

    let snipping_tool_disabled = hotkey::is_windows_snipping_tool_disabled();
    let (mut settings, settings_warning) = Settings::load_with_warning();
    if let Err(error) = startup::set_start_with_windows(settings.start_with_windows) {
        eprintln!("[isolmaSS] Startup registration could not be synchronized: {error}");
    }

    let (tray_tx, tray_rx) = channel::<TrayCommand>();
    let tray_manager = TrayManager::create(tray_tx.clone())?;
    if let Some(warning) = settings_warning {
        tray::show_notification("Settings recovered", &warning);
    }

    let (mut hotkey_handle, mut forward_thread) =
        start_hotkey_runtime(settings.hotkey.clone(), tray_tx.clone())?;

    if settings.check_updates_automatically {
        updater::run_automatic_update_check(settings.install_updates_automatically);
    }

    println!("Hotkey & System Tray daemon active!");
    println!(
        "  - Active Hotkey:   [{}] (Windows Snipping Tool suppressed: {})",
        hotkey_handle.active_description, snipping_tool_disabled
    );
    println!(
        "  - Save Folder:     [{}]",
        settings.save_directory.display()
    );

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
        .0 > 0
    {
        unsafe {
            let _ = windows::Win32::UI::WindowsAndMessaging::TranslateMessage(&msg);
            windows::Win32::UI::WindowsAndMessaging::DispatchMessageW(&msg);
        }

        while let Ok(command) = tray_rx.try_recv() {
            match command {
                TrayCommand::Capture => {
                    if settings.capture_delay_ms > 0 {
                        std::thread::sleep(std::time::Duration::from_millis(u64::from(
                            settings.capture_delay_ms,
                        )));
                    }
                    let triggered = Instant::now();
                    match CaptureBuffer::capture_virtual_screen() {
                        Ok(capture) => {
                            println!(
                                "[isolmaSS] Capture={}us (setup={}us, BitBlt={}us, copy={}us, dim={}us), size={}x{} at ({}, {}).",
                                capture.timings.total.as_micros(),
                                capture.timings.setup.as_micros(),
                                capture.timings.bit_blt.as_micros(),
                                capture.timings.copy.as_micros(),
                                capture.timings.dim.as_micros(),
                                capture.width,
                                capture.height,
                                capture.x,
                                capture.y
                            );
                            match show_overlay_session(Rc::new(capture)) {
                                Ok(Some(selection)) => println!(
                                    "[isolmaSS] Selection {}x{} committed; total={}us.",
                                    selection.width(),
                                    selection.height(),
                                    triggered.elapsed().as_micros()
                                ),
                                Ok(None) => {}
                                Err(error) => {
                                    tray::show_notification("Overlay failed", &error.to_string());
                                }
                            }
                            let mut latest = Settings::load_or_default();
                            if latest.hotkey != settings.hotkey {
                                drop(hotkey_handle);
                                let _ = forward_thread.join();
                                match start_hotkey_runtime(latest.hotkey.clone(), tray_tx.clone()) {
                                    Ok((handle, thread)) => {
                                        hotkey_handle = handle;
                                        forward_thread = thread;
                                        tray::show_notification(
                                            "Hotkey updated",
                                            &format!(
                                                "Active hotkey: {}",
                                                hotkey_handle.active_description
                                            ),
                                        );
                                    }
                                    Err(error) => {
                                        let fallback = start_hotkey_runtime(
                                            settings.hotkey.clone(),
                                            tray_tx.clone(),
                                        )?;
                                        hotkey_handle = fallback.0;
                                        forward_thread = fallback.1;
                                        let mut detail = error.to_string();
                                        if let Err(rollback_error) =
                                            rollback_hotkey_setting(&mut latest, &settings.hotkey)
                                        {
                                            detail.push('\n');
                                            detail.push_str(&rollback_error);
                                        }
                                        tray::show_notification("Hotkey update failed", &detail);
                                    }
                                }
                            }
                            settings = latest;
                        }
                        Err(error) => {
                            tray::show_notification("Capture failed", &error.to_string());
                        }
                    }
                }
                TrayCommand::Settings => match show_settings_dialog(&settings, None) {
                    Ok(Some(mut saved)) => {
                        if saved.hotkey != settings.hotkey {
                            drop(hotkey_handle);
                            let _ = forward_thread.join();
                            match start_hotkey_runtime(saved.hotkey.clone(), tray_tx.clone()) {
                                Ok((handle, thread)) => {
                                    hotkey_handle = handle;
                                    forward_thread = thread;
                                    tray::show_notification(
                                        "Hotkey updated",
                                        &format!(
                                            "Active hotkey: {}",
                                            hotkey_handle.active_description
                                        ),
                                    );
                                }
                                Err(error) => {
                                    let fallback = start_hotkey_runtime(
                                        settings.hotkey.clone(),
                                        tray_tx.clone(),
                                    )?;
                                    hotkey_handle = fallback.0;
                                    forward_thread = fallback.1;
                                    let mut detail = error.to_string();
                                    if let Err(rollback_error) =
                                        rollback_hotkey_setting(&mut saved, &settings.hotkey)
                                    {
                                        detail.push('\n');
                                        detail.push_str(&rollback_error);
                                    }
                                    tray::show_notification("Hotkey update failed", &detail);
                                }
                            }
                        }
                        settings = saved;
                    }
                    Ok(None) => {}
                    Err(error) => tray::show_notification("Settings failed", &error.to_string()),
                },
                TrayCommand::CheckUpdates => updater::run_manual_update_check(),
                TrayCommand::OpenRecent(path) => {
                    if let Err(error) = open_recent_capture(&path) {
                        tray::show_notification("Could not open screenshot", &error);
                    }
                }
                TrayCommand::Exit => {
                    running = false;
                    break;
                }
            }
        }
    }

    drop(tray_manager);
    drop(hotkey_handle);
    let _ = forward_thread.join();
    Ok(())
}

fn run_capture_once() -> Result<(), Box<dyn std::error::Error>> {
    println!("[isolmaSS] Capturing virtual screen immediately...");
    let capture = CaptureBuffer::capture_virtual_screen()?;
    println!(
        "[isolmaSS] Capture={}us (setup={}us, BitBlt={}us, copy={}us, dim={}us).",
        capture.timings.total.as_micros(),
        capture.timings.setup.as_micros(),
        capture.timings.bit_blt.as_micros(),
        capture.timings.copy.as_micros(),
        capture.timings.dim.as_micros(),
    );
    let capture_rc = Rc::new(capture);
    match show_overlay_session(capture_rc)? {
        Some(selection) => {
            println!(
                "[isolmaSS] Capture committed: [({}, {}) to ({}, {})] ({}x{} pixels).",
                selection.left,
                selection.top,
                selection.right,
                selection.bottom,
                selection.width(),
                selection.height()
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
                println!(
                    "[isolmaSS] Applying Windows registry fix to suppress Snipping Tool on PrintScreen..."
                );
                if hotkey::disable_windows_snipping_tool_hotkey() {
                    println!(
                        "[isolmaSS] SUCCESS: Set HKCU\\Control Panel\\Keyboard -> PrintScreenKeyForSnippingEnabled = 0."
                    );
                    println!(
                        "[isolmaSS] Windows Snipping Tool is permanently disabled from capturing PrintScreen."
                    );
                    println!("[isolmaSS] PrintScreen is now dedicated exclusively to isolmaSS.");
                } else {
                    eprintln!(
                        "[isolmaSS] ERROR: Failed to update Windows registry value PrintScreenKeyForSnippingEnabled."
                    );
                    std::process::exit(1);
                }
                return Ok(());
            }
            "--smoke-test" | "--test-capture" => {
                return smoke::run_smoke_test();
            }
            "--capture-once" => {
                return run_capture_once();
            }
            "--settings" => {
                let current = Settings::load_or_default();
                match show_settings_dialog(&current, None) {
                    Ok(Some(_saved)) => {
                        println!("[isolmaSS] Settings updated and applied successfully.");
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
            "--check-update" => {
                match updater::check_for_update() {
                    Ok(Some(update)) => println!(
                        "Update available: {} ({})",
                        update.version, update.release_url
                    ),
                    Ok(None) => println!("isolmaSS {} is up to date.", env!("CARGO_PKG_VERSION")),
                    Err(error) => return Err(error.into()),
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
