#![windows_subsystem = "windows"]

use std::path::{Path, PathBuf};
use windows::core::PCWSTR;
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Console::{AttachConsole, ATTACH_PARENT_PROCESS};
use windows::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_ICONINFORMATION, MB_OK};

const APP_NAME: &str = "isolmaSS";
const APP_VERSION: &str = "0.3.0";
const PUBLISHER: &str = "isolmass";

fn get_install_dir() -> PathBuf {
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        PathBuf::from(local).join(APP_NAME)
    } else if let Some(profile) = std::env::var_os("USERPROFILE") {
        PathBuf::from(profile).join("AppData").join("Local").join(APP_NAME)
    } else {
        PathBuf::from("C:\\Program Files").join(APP_NAME)
    }
}

fn get_start_menu_dir() -> PathBuf {
    if let Some(appdata) = std::env::var_os("APPDATA") {
        PathBuf::from(appdata)
            .join("Microsoft")
            .join("Windows")
            .join("Start Menu")
            .join("Programs")
            .join(APP_NAME)
    } else {
        get_install_dir()
    }
}

fn find_source_binary() -> Option<PathBuf> {
    if let Ok(cur_exe) = std::env::current_exe()
        && let Some(parent) = cur_exe.parent()
    {
        let sibling = parent.join("isolmass.exe");
        if sibling.exists() {
            return Some(sibling);
        }
    }
    let release = Path::new("target/release/isolmass.exe");
    if release.exists() {
        return Some(release.to_path_buf());
    }
    let debug = Path::new("target/debug/isolmass.exe");
    if debug.exists() {
        return Some(debug.to_path_buf());
    }
    None
}

fn create_shortcut(target: &Path, link_path: &Path) -> std::io::Result<()> {
    let script = format!(
        "$ws = New-Object -ComObject WScript.Shell; $s = $ws.CreateShortcut('{}'); $s.TargetPath = '{}'; $s.WorkingDirectory = '{}'; $s.Save()",
        link_path.display(),
        target.display(),
        target.parent().unwrap_or(Path::new("")).display()
    );
    let _ = std::process::Command::new("powershell")
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", &script])
        .output();
    Ok(())
}

fn write_registry_uninstall(install_dir: &Path) -> std::io::Result<()> {
    let exe_path = install_dir.join("isolmass.exe");
    let uninst_path = install_dir.join("isolmass-setup.exe");
    let script = format!(
        "Set-ItemProperty -Path 'HKCU:\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\{APP_NAME}' -Name 'DisplayName' -Value '{APP_NAME}' -Force; \
         Set-ItemProperty -Path 'HKCU:\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\{APP_NAME}' -Name 'DisplayVersion' -Value '{APP_VERSION}' -Force; \
         Set-ItemProperty -Path 'HKCU:\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\{APP_NAME}' -Name 'Publisher' -Value '{PUBLISHER}' -Force; \
         Set-ItemProperty -Path 'HKCU:\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\{APP_NAME}' -Name 'InstallLocation' -Value '{install_dir}' -Force; \
         Set-ItemProperty -Path 'HKCU:\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\{APP_NAME}' -Name 'DisplayIcon' -Value '{exe_path}' -Force; \
         Set-ItemProperty -Path 'HKCU:\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\{APP_NAME}' -Name 'UninstallString' -Value '\"{uninst_path}\" --uninstall' -Force",
        APP_NAME = APP_NAME,
        APP_VERSION = APP_VERSION,
        PUBLISHER = PUBLISHER,
        install_dir = install_dir.display(),
        exe_path = exe_path.display(),
        uninst_path = uninst_path.display(),
    );
    let _ = std::process::Command::new("powershell")
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command",
               &format!("New-Item -Path 'HKCU:\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\{}' -Force; {}", APP_NAME, script)])
        .output();
    Ok(())
}

fn remove_registry_uninstall() {
    let _ = std::process::Command::new("powershell")
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command",
               &format!("Remove-Item -Path 'HKCU:\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\{}' -Recurse -Force -ErrorAction SilentlyContinue", APP_NAME)])
        .output();
}

fn show_msg(title: &str, msg: &str, is_error: bool) {
    let wide_title: Vec<u16> = title.encode_utf16().chain(std::iter::once(0)).collect();
    let wide_msg: Vec<u16> = msg.encode_utf16().chain(std::iter::once(0)).collect();
    let flags = if is_error {
        MB_OK | MB_ICONERROR
    } else {
        MB_OK | MB_ICONINFORMATION
    };
    unsafe {
        let _ = MessageBoxW(
            HWND::default(),
            PCWSTR(wide_msg.as_ptr()),
            PCWSTR(wide_title.as_ptr()),
            flags,
        );
    }
}

fn perform_install(silent: bool) -> Result<(), String> {
    let source_exe = find_source_binary()
        .ok_or_else(|| "Could not find isolmass.exe source binary to install.".to_string())?;

    let install_dir = get_install_dir();
    std::fs::create_dir_all(&install_dir)
        .map_err(|e| format!("Failed to create install directory '{}': {e}", install_dir.display()))?;

    let target_exe = install_dir.join("isolmass.exe");
    std::fs::copy(&source_exe, &target_exe)
        .map_err(|e| format!("Failed to copy executable to '{}': {e}", target_exe.display()))?;

    // Copy self as uninstaller
    if let Ok(cur_exe) = std::env::current_exe() {
        let target_uninst = install_dir.join("isolmass-setup.exe");
        let _ = std::fs::copy(&cur_exe, &target_uninst);
    }

    // Start Menu shortcut
    let start_menu_dir = get_start_menu_dir();
    if std::fs::create_dir_all(&start_menu_dir).is_ok() {
        let link_path = start_menu_dir.join(format!("{}.lnk", APP_NAME));
        let _ = create_shortcut(&target_exe, &link_path);
    }

    // Register with Windows Add/Remove Programs
    let _ = write_registry_uninstall(&install_dir);

    if !silent {
        show_msg(
            "isolmaSS Setup",
            &format!("isolmaSS {APP_VERSION} has been successfully installed to:\n{}", install_dir.display()),
            false,
        );
    }

    Ok(())
}

fn perform_uninstall(silent: bool) -> Result<(), String> {
    let install_dir = get_install_dir();
    let target_exe = install_dir.join("isolmass.exe");
    let _ = std::fs::remove_file(&target_exe);

    let start_menu_dir = get_start_menu_dir();
    let _ = std::fs::remove_dir_all(&start_menu_dir);
    remove_registry_uninstall();

    if !silent {
        show_msg(
            "isolmaSS Uninstall",
            "isolmaSS has been successfully uninstalled from this computer.",
            false,
        );
    }

    Ok(())
}

fn main() {
    unsafe {
        let _ = AttachConsole(ATTACH_PARENT_PROCESS);
    }

    let args: Vec<String> = std::env::args().collect();
    let silent = args.iter().any(|a| a == "--silent" || a == "-s");
    let uninstall = args.iter().any(|a| a == "--uninstall" || a == "-u");

    let result = if uninstall {
        perform_uninstall(silent)
    } else {
        perform_install(silent)
    };

    match result {
        Ok(()) => {
            std::process::exit(0);
        }
        Err(err) => {
            eprintln!("[ERROR] {err}");
            if !silent {
                show_msg("isolmaSS Setup Error", &err, true);
            }
            std::process::exit(1);
        }
    }
}
