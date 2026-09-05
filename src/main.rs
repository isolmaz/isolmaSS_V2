#![windows_subsystem = "windows"]

mod annotation;
mod capture;
mod clipboard;
mod diagnostics;
mod drawing;
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
mod ui;
mod updater;
mod window_snap;

use capture::CaptureBuffer;
use hotkey::{HotkeyConfig, HotkeyHandle, start_hotkey_listener};
use instance::InstanceState;
use overlay::show_overlay_session;
use settings::{Settings, show_settings_dialog};
use std::path::Path;
use std::rc::Rc;
use std::sync::mpsc::{SyncSender, sync_channel};
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
    println!("  isolmass --smoke-test      Run Windows behavior and package verification");
    println!("  isolmass --test-capture    Alias for --smoke-test");
    println!("  isolmass --capture-once    Capture immediately and open overlay once");
    println!("  isolmass --settings        Open native settings dialog");
    println!("  isolmass --check-update    Check GitHub Releases for a newer signed version");
    println!(
        "  isolmass --benchmark [N]   Measure N capture-to-visible samples (default 50; JSON lines)"
    );
    println!("  isolmass --verify-update P Verify installer signature and expected publisher");
    println!("  isolmass --help            Show this help message");
}

fn start_hotkey_runtime(
    config: HotkeyConfig,
    daemon_tx: SyncSender<TrayCommand>,
) -> Result<HotkeyRuntime, Box<dyn std::error::Error>> {
    let requested = config.description.clone();
    let (event_rx, handle) = start_hotkey_listener(config)?;
    tray::set_active_hotkey(&handle.active_description);
    if handle.active_description != requested {
        tray::show_notification(
            "Capture shortcut changed",
            &format!(
                "{requested} is in use. Capturing with {} for this session.",
                handle.active_description
            ),
        );
    }
    let forward_thread = std::thread::spawn(move || {
        while let Ok(triggered) = event_rx.recv() {
            tray::queue_capture(&daemon_tx, triggered);
        }
    });
    Ok(HotkeyRuntime {
        handle: Some(handle),
        forwarder: Some(forward_thread),
    })
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

struct HotkeyRuntime {
    handle: Option<HotkeyHandle>,
    forwarder: Option<std::thread::JoinHandle<()>>,
}
impl Drop for HotkeyRuntime {
    fn drop(&mut self) {
        drop(self.handle.take());
        if let Some(forwarder) = self.forwarder.take()
            && forwarder.join().is_err()
        {
            diagnostics::record(
                "hotkey",
                "The capture queue worker terminated unexpectedly.",
            );
        }
    }
}

struct BackgroundServices;
impl Drop for BackgroundServices {
    fn drop(&mut self) {
        updater::shutdown();
        save::recent::shutdown();
    }
}

fn apply_settings(
    settings: &mut Settings,
    mut latest: Settings,
    runtime: &mut Option<HotkeyRuntime>,
    sender: &SyncSender<TrayCommand>,
) -> Result<(), Box<dyn std::error::Error>> {
    if latest.hotkey != settings.hotkey {
        drop(runtime.take());
        match start_hotkey_runtime(latest.hotkey.clone(), sender.clone()) {
            Ok(next) => *runtime = Some(next),
            Err(error) => {
                *runtime = Some(start_hotkey_runtime(
                    settings.hotkey.clone(),
                    sender.clone(),
                )?);
                let mut detail = error.to_string();
                if let Err(rollback) = rollback_hotkey_setting(&mut latest, &settings.hotkey) {
                    detail.push_str(&format!(" {rollback}"));
                }
                tray::show_notification("Hotkey unchanged", &detail);
            }
        }
    }
    updater::configure(settings, &latest);
    *settings = latest;
    tray::refresh_preferences(settings);
    Ok(())
}

fn capture(triggered: Instant) {
    match CaptureBuffer::capture_virtual_screen()
        .and_then(|capture| show_overlay_session(Rc::new(capture), triggered))
    {
        Ok(_) => {}
        Err(error) => tray::show_notification("Capture failed", &error.to_string()),
    }
    tray::capture_finished();
}

fn run_interactive_session(open_settings: bool) -> Result<(), Box<dyn std::error::Error>> {
    use windows::Win32::UI::WindowsAndMessaging::*;
    let _instance = match instance::acquire_or_notify()? {
        InstanceState::Primary(instance) => instance,
        InstanceState::ExistingNotified => return Ok(()),
    };
    let (mut settings, warning) = Settings::load_with_warning();
    if let Err(error) = startup::set_start_with_windows(settings.start_with_windows) {
        diagnostics::record("startup", &error);
    }
    let (sender, receiver) = sync_channel::<TrayCommand>(16);
    let tray_manager = TrayManager::create(sender.clone())?;
    let services = BackgroundServices;
    tray::refresh_preferences(&settings);
    if let Some(warning) = warning {
        tray::show_notification("Settings recovered", &warning);
    }
    let mut runtime = Some(start_hotkey_runtime(
        settings.hotkey.clone(),
        sender.clone(),
    )?);
    if settings.check_updates_automatically {
        updater::run_automatic_update_check(settings.install_updates_automatically);
    }
    if open_settings {
        sender.try_send(TrayCommand::Settings)?;
        tray::notify_tray_wakeup();
    }
    const CAPTURE_TIMER: usize = 41;
    let mut scheduled: Option<Instant> = None;
    let mut msg = MSG::default();
    let mut running = true;
    while running {
        let status = unsafe { GetMessageW(&mut msg, None, 0, 0) }.0;
        if status == -1 {
            return Err(windows::core::Error::from_win32().into());
        }
        if status == 0 {
            break;
        }
        unsafe {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        let mut reload = false;
        if msg.message == WM_TIMER && msg.wParam.0 == CAPTURE_TIMER {
            unsafe {
                let _ = KillTimer(tray::window_handle(), CAPTURE_TIMER);
            }
            if let Some(triggered) = scheduled.take() {
                capture(triggered);
                reload = true;
            }
        }
        while let Ok(command) = receiver.try_recv() {
            match command {
                TrayCommand::Capture(triggered) => {
                    if tray::capture_is_stale(triggered) {
                        continue;
                    }
                    if settings.capture_delay_ms > 0 {
                        scheduled = Some(triggered);
                        if unsafe {
                            SetTimer(
                                tray::window_handle(),
                                CAPTURE_TIMER,
                                settings.capture_delay_ms,
                                None,
                            )
                        } == 0
                        {
                            scheduled = None;
                            tray::capture_finished();
                            tray::show_notification(
                                "Capture failed",
                                "The countdown timer could not start.",
                            );
                        } else {
                            tray::show_notification(
                                "Capture countdown",
                                &format!(
                                    "Capturing in {} seconds. Press Esc to cancel.",
                                    settings.capture_delay_ms as f64 / 1000.0
                                ),
                            );
                        }
                    } else {
                        capture(triggered);
                        reload = true;
                    }
                }
                TrayCommand::CancelCapture => {
                    unsafe {
                        let _ = KillTimer(tray::window_handle(), CAPTURE_TIMER);
                    }
                    scheduled = None;
                    tray::capture_finished();
                }
                TrayCommand::Settings => {
                    unsafe {
                        let _ = KillTimer(tray::window_handle(), CAPTURE_TIMER);
                    }
                    scheduled = None;
                    tray::capture_finished();
                    match show_settings_dialog(&settings, None) {
                        Ok(Some(latest)) => {
                            apply_settings(&mut settings, latest, &mut runtime, &sender)?
                        }
                        Ok(None) => {}
                        Err(error) => {
                            ui::error(tray::window_handle(), "Settings failed", &error.to_string())
                        }
                    }
                    tray::capture_finished();
                }
                TrayCommand::CheckUpdates => updater::run_manual_update_check(),
                TrayCommand::OpenFolder => {
                    let path = save::recent::last_folder()
                        .unwrap_or_else(|| settings.save_directory.clone());
                    if let Err(error) = open_recent_capture(&path) {
                        tray::show_notification("Folder could not be opened", &error);
                    }
                }
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
        if reload {
            let (latest, warning) = Settings::load_with_warning();
            if let Some(warning) = warning {
                tray::show_notification("Settings recovered", &warning);
            }
            apply_settings(&mut settings, latest, &mut runtime, &sender)?;
        }
        if running {
            updater::poll(tray::window_handle(), true);
            save::recent::refresh(&settings.save_directory);
        }
    }
    unsafe {
        let _ = KillTimer(tray::window_handle(), CAPTURE_TIMER);
    }
    tray::capture_finished();
    drop(services);
    drop(runtime.take());
    drop(tray_manager);
    Ok(())
}

fn run_capture_once() -> Result<(), Box<dyn std::error::Error>> {
    println!("[isolmaSS] Capturing virtual screen immediately...");
    let triggered = Instant::now();
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
    match show_overlay_session(capture_rc, triggered)? {
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

fn run() -> Result<(), Box<dyn std::error::Error>> {
    struct ComApartment;
    impl Drop for ComApartment {
        fn drop(&mut self) {
            unsafe {
                windows::Win32::System::Com::CoUninitialize();
            }
        }
    }
    unsafe {
        windows::Win32::System::Com::CoInitializeEx(
            None,
            windows::Win32::System::Com::COINIT_APARTMENTTHREADED,
        )
    }
    .ok()?;
    let _apartment = ComApartment;
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
            "--benchmark" => {
                let count = args
                    .get(2)
                    .map(|value| value.parse::<usize>())
                    .transpose()?
                    .unwrap_or(50);
                return smoke::run_benchmark(count);
            }
            "--capture-once" => {
                return run_capture_once();
            }
            "--verify-update" => {
                let path = args
                    .get(2)
                    .ok_or("--verify-update requires an installer path")?;
                updater::verify_authenticode(Path::new(path))?;
                println!("Installer trust and publisher identity verified.");
                return Ok(());
            }
            "--settings" => {
                return run_interactive_session(true);
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

    run_interactive_session(false)
}

fn main() {
    let result = run();
    updater::shutdown();
    save::recent::shutdown();
    if let Err(error) = result {
        diagnostics::record("startup", &error.to_string());
        std::eprintln!("isolmaSS: {error}");
        if !std::env::args().any(|arg| {
            matches!(
                arg.as_str(),
                "--smoke-test"
                    | "--test-capture"
                    | "--check-update"
                    | "--verify-update"
                    | "--benchmark"
            )
        }) {
            ui::error(
                windows::Win32::Foundation::HWND::default(),
                "isolmaSS could not complete the operation",
                &error.to_string(),
            );
        }
        std::process::exit(1);
    }
}
