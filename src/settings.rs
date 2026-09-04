use crate::annotation::ToolKind;
use crate::hotkey::HotkeyConfig;
use crate::save::default_save_directory;
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use windows::Win32::Storage::FileSystem::{
    MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
};
use windows::core::PCWSTR;

pub const PRESET_COLORS: [[u8; 4]; 8] = [
    [49, 49, 224, 255],   // Red (#E03131)
    [7, 103, 247, 255],   // Orange (#F76707)
    [25, 196, 252, 255],  // Yellow (#FCC419)
    [68, 158, 47, 255],   // Green (#2F9E44)
    [194, 113, 25, 255],  // Blue (#1971C2)
    [181, 54, 156, 255],  // Purple (#9C36B5)
    [255, 255, 255, 255], // White (#FFFFFF)
    [41, 37, 33, 255],    // Black (#212529)
];

pub const PRESET_THICKNESSES: [i32; 3] = [2, 4, 8];

fn default_true() -> bool {
    true
}

fn default_jpeg_quality() -> u8 {
    90
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum SaveFormat {
    #[default]
    Png,
    Jpeg,
}

impl SaveFormat {
    pub const fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpg",
        }
    }
}

/// Persistent application settings model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    pub hotkey: HotkeyConfig,
    pub save_directory: PathBuf,
    pub default_color: [u8; 4],
    pub default_thickness: i32,
    #[serde(default = "default_true")]
    pub enable_window_snap: bool,
    #[serde(default = "default_true")]
    pub close_after_action: bool,
    #[serde(default)]
    pub start_with_windows: bool,
    #[serde(default = "default_true")]
    pub notify_after_save: bool,
    #[serde(default)]
    pub capture_delay_ms: u32,
    #[serde(default)]
    pub save_format: SaveFormat,
    #[serde(default = "default_jpeg_quality")]
    pub jpeg_quality: u8,
    #[serde(default = "default_true")]
    pub check_updates_automatically: bool,
    #[serde(default)]
    pub install_updates_automatically: bool,
    #[serde(default)]
    pub last_tool: ToolKind,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            hotkey: HotkeyConfig::default(),
            save_directory: default_save_directory(),
            default_color: PRESET_COLORS[0], // Default vibrant red
            default_thickness: 4,
            enable_window_snap: true,
            close_after_action: true,
            start_with_windows: false,
            notify_after_save: true,
            capture_delay_ms: 0,
            save_format: SaveFormat::Png,
            jpeg_quality: default_jpeg_quality(),
            check_updates_automatically: true,
            install_updates_automatically: false,
            last_tool: ToolKind::default(),
        }
    }
}

impl Settings {
    /// Canonical path to settings file: `%APPDATA%\isolmaSS\settings.json`.
    pub fn config_path() -> Option<PathBuf> {
        crate::hotkey::settings_path()
    }

    /// Loads settings and returns a user-visible warning when recovery was required.
    pub fn load_with_warning() -> (Self, Option<String>) {
        let Some(path) = Self::config_path() else {
            return (
                Self::default(),
                Some("Settings could not be loaded because %APPDATA% is unavailable.".to_string()),
            );
        };

        match Self::load_from_path(&path) {
            Ok(settings) => (settings, None),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (Self::default(), None),
            Err(error) => {
                let backup = path.with_extension(format!(
                    "corrupt-{}.json",
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map_or(0, |duration| duration.as_secs())
                ));
                let backup_note = match std::fs::copy(&path, &backup) {
                    Ok(_) => format!(" A copy was preserved at {}.", backup.display()),
                    Err(_) => String::new(),
                };
                (
                    Self::default(),
                    Some(format!(
                        "Settings were reset because settings.json is invalid: {error}.{backup_note}"
                    )),
                )
            }
        }
    }

    /// Loads settings from disk or returns default configuration.
    pub fn load_or_default() -> Self {
        Self::load_with_warning().0
    }

    /// Saves settings through a same-directory temporary file and an atomic replacement.
    pub fn save_to_path(&self, path: &Path) -> std::io::Result<()> {
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "settings path has no parent directory",
                )
            })?;
        std::fs::create_dir_all(parent)?;

        let json = serde_json::to_vec_pretty(self)
            .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |duration| duration.as_nanos());
        let temp_path = parent.join(format!(".settings-{}-{unique}.tmp", std::process::id()));

        let result = (|| -> std::io::Result<()> {
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temp_path)?;
            file.write_all(&json)?;
            file.sync_all()?;
            drop(file);

            use std::os::windows::ffi::OsStrExt;
            let source: Vec<u16> = temp_path.as_os_str().encode_wide().chain(Some(0)).collect();
            let destination: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
            unsafe {
                MoveFileExW(
                    PCWSTR(source.as_ptr()),
                    PCWSTR(destination.as_ptr()),
                    MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
                )
            }
            .map_err(std::io::Error::other)
        })();

        if result.is_err() {
            let _ = std::fs::remove_file(&temp_path);
        }
        result
    }

    /// Loads settings from an arbitrary path on disk.
    pub fn load_from_path(path: &Path) -> std::io::Result<Self> {
        let mut content = String::new();
        std::fs::File::open(path)?.read_to_string(&mut content)?;
        serde_json::from_str::<Settings>(&content)
            .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))
    }

    fn save_to_config_path(&self, path: Option<&std::path::Path>) -> std::io::Result<()> {
        let path = path.ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "cannot save settings: %APPDATA% is unavailable",
            )
        })?;
        self.save_to_path(path)
    }

    /// Saves settings to disk as formatted JSON.
    pub fn save(&self) -> std::io::Result<()> {
        let path = Self::config_path();
        self.save_to_config_path(path.as_deref())
    }
}

pub use crate::settings_window::show_settings_dialog;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_settings_new_toggles_defaults_and_serde() {
        let defaults = Settings::default();
        assert!(defaults.enable_window_snap);
        assert!(defaults.close_after_action);

        // Serialization round-trip
        let json = serde_json::to_string(&defaults).expect("serialize settings");
        assert!(json.contains("enable_window_snap"));
        assert!(json.contains("close_after_action"));

        let mut deserialized: Settings = serde_json::from_str(&json).expect("deserialize settings");
        assert_eq!(deserialized, defaults);

        // Backward compatibility: JSON missing the two new fields
        let legacy_json = serde_json::json!({
            "hotkey": defaults.hotkey,
            "save_directory": defaults.save_directory,
            "default_color": defaults.default_color,
            "default_thickness": defaults.default_thickness,
        })
        .to_string();

        let loaded: Settings =
            serde_json::from_str(&legacy_json).expect("deserialize legacy settings");
        assert!(
            loaded.enable_window_snap,
            "Legacy JSON should default enable_window_snap to true"
        );
        assert!(
            loaded.close_after_action,
            "Legacy JSON should default close_after_action to true"
        );

        // Custom toggles
        deserialized.enable_window_snap = false;
        deserialized.close_after_action = false;
        let json2 = serde_json::to_string(&deserialized).expect("serialize customized");
        let loaded2: Settings = serde_json::from_str(&json2).expect("deserialize customized");
        assert!(!loaded2.enable_window_snap);
        assert!(!loaded2.close_after_action);
    }

    #[test]
    fn test_settings_save_to_path_roundtrip_and_error() {
        let defaults = Settings::default();
        let unique_suffix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock should be after the Unix epoch")
            .as_nanos();
        let temp_root = std::env::temp_dir().join(format!(
            "isolmass_unit_test_settings_{}_{}",
            std::process::id(),
            unique_suffix
        ));
        let temp_file = temp_root.join("roundtrip").join("settings.json");

        defaults
            .save_to_path(&temp_file)
            .expect("save_to_path should create the parent directory");
        let loaded = Settings::load_from_path(&temp_file).expect("load_from_path should succeed");
        assert_eq!(loaded, defaults);

        let blocker = temp_root.join("parent_blocker");
        std::fs::write(&blocker, b"not a directory").expect("create parent blocker");
        let blocked_parent = blocker.join("missing");
        let expected_error = std::fs::create_dir_all(&blocked_parent)
            .expect_err("create_dir_all should fail below a file");
        let save_error = defaults
            .save_to_path(&blocked_parent.join("settings.json"))
            .expect_err("save_to_path should propagate the create_dir_all error");
        assert_eq!(save_error.kind(), expected_error.kind());
        assert_eq!(save_error.raw_os_error(), expected_error.raw_os_error());

        let missing_path_error = defaults
            .save_to_config_path(None)
            .expect_err("a missing config path should prevent saving");
        assert_eq!(missing_path_error.kind(), std::io::ErrorKind::NotFound);
        assert_eq!(
            missing_path_error.to_string(),
            "cannot save settings: %APPDATA% is unavailable"
        );

        std::fs::remove_dir_all(&temp_root).expect("remove settings test directory");
    }
}
