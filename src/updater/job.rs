use super::*;
use std::thread::JoinHandle;

pub(super) static CANCELLED: AtomicBool = AtomicBool::new(false);
static RUNNING: AtomicBool = AtomicBool::new(false);
static AUTOMATIC: AtomicBool = AtomicBool::new(false);
static WORKER: Mutex<Option<JoinHandle<()>>> = Mutex::new(None);
static EVENT: Mutex<Option<Event>> = Mutex::new(None);
static READY: Mutex<Option<PathBuf>> = Mutex::new(None);
static RECHECK: Mutex<Option<bool>> = Mutex::new(None);

enum Event {
    Checked(Result<Option<UpdateInfo>, String>, bool, bool),
    Downloaded(Result<PathBuf, String>),
}

fn start(automatic: bool, work: impl FnOnce() -> Event + Send + 'static) {
    if RUNNING
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        if !automatic {
            crate::tray::show_notification(
                "Update in progress",
                "The current update operation is still running.",
            );
        }
        return;
    }
    if let Some(previous) = WORKER
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .take()
    {
        let _ = previous.join();
    }
    CANCELLED.store(false, Ordering::Release);
    AUTOMATIC.store(automatic, Ordering::Release);
    let handle = std::thread::spawn(move || {
        let event = work();
        if !CANCELLED.load(Ordering::Acquire) {
            *EVENT.lock().unwrap_or_else(|error| error.into_inner()) = Some(event);
        }
        RUNNING.store(false, Ordering::Release);
        crate::tray::notify_tray_wakeup();
    });
    *WORKER.lock().unwrap_or_else(|error| error.into_inner()) = Some(handle);
}

pub fn run_manual_update_check() {
    start(false, || Event::Checked(check_for_update(), true, false));
}
pub fn run_automatic_update_check(install: bool) {
    start(true, move || {
        Event::Checked(check_for_update(), false, install)
    });
}

pub fn configure(previous: &crate::settings::Settings, current: &crate::settings::Settings) {
    if previous.check_updates_automatically != current.check_updates_automatically
        || previous.install_updates_automatically != current.install_updates_automatically
    {
        if AUTOMATIC.load(Ordering::Acquire) {
            CANCELLED.store(true, Ordering::Release);
            *EVENT.lock().unwrap_or_else(|error| error.into_inner()) = None;
            *READY.lock().unwrap_or_else(|error| error.into_inner()) = None;
        }
        *RECHECK.lock().unwrap_or_else(|error| error.into_inner()) = current
            .check_updates_automatically
            .then_some(current.install_updates_automatically);
    }
}

/// Called by UI loops without a borrowed window state. Installation waits until editing ends.
pub fn poll(owner: HWND, allow_install: bool) {
    if RUNNING.load(Ordering::Acquire) {
        return;
    }
    let recheck = RECHECK
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .take();
    if let Some(install) = recheck {
        run_automatic_update_check(install);
        return;
    }
    let event = EVENT
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .take();
    match event {
        Some(Event::Checked(Ok(Some(update)), manual, auto_install)) => {
            let install = auto_install
                || manual
                    && crate::ui::confirm(
                        owner,
                        "An update is ready",
                        &format!(
                            "isolmaSS {} is available. Download the verified update? Installation starts after you finish editing.",
                            update.version
                        ),
                    );
            if install {
                crate::tray::show_notification(
                    "Downloading update",
                    "Verifying the download and isolmaSS publisher.",
                );
                start(!manual, move || Event::Downloaded(download_update(&update)));
            } else {
                crate::tray::show_notification(
                    "Update available",
                    &format!("isolmaSS {} is available.", update.version),
                );
            }
        }
        Some(Event::Checked(Ok(None), true, _)) => crate::tray::show_notification(
            "You're up to date",
            concat!(
                "isolmaSS ",
                env!("CARGO_PKG_VERSION"),
                " is the latest release."
            ),
        ),
        Some(Event::Checked(Err(error), manual, _)) => {
            crate::diagnostics::record("update", &error);
            if manual {
                crate::ui::error(owner, "Update check failed", &error);
            }
        }
        Some(Event::Downloaded(Ok(path))) => {
            *READY.lock().unwrap_or_else(|error| error.into_inner()) = Some(path);
            if !allow_install {
                crate::tray::show_notification(
                    "Update ready",
                    "Finish your current capture or settings changes to install and restart.",
                );
            }
        }
        Some(Event::Downloaded(Err(error))) => {
            crate::ui::error(owner, "Update could not be installed", &error)
        }
        _ => {}
    }
    let ready = if allow_install {
        READY
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .take()
    } else {
        None
    };
    if let Some(path) = ready {
        match launch_installer(&path) {
            Ok(()) => crate::tray::request_exit(),
            Err(error) => crate::ui::error(owner, "Update could not be started", &error),
        }
    }
}

pub fn shutdown() {
    CANCELLED.store(true, Ordering::Release);
    if let Some(worker) = WORKER
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .take()
        && worker.join().is_err()
    {
        crate::diagnostics::record("update", "Update worker terminated unexpectedly.");
    }
}
