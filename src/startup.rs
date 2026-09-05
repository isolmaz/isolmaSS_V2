use windows::Win32::Foundation::ERROR_FILE_NOT_FOUND;
use windows::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_SET_VALUE, REG_OPTION_NON_VOLATILE, REG_SZ, RegCloseKey,
    RegCreateKeyExW, RegDeleteValueW, RegSetValueExW,
};
use windows::core::{PCWSTR, w};

const VALUE_NAME: PCWSTR = w!("isolmaSS");
const RUN_KEY: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");

pub struct StartupRegistration(Option<(windows::Win32::System::Registry::REG_VALUE_TYPE, Vec<u8>)>);

impl StartupRegistration {
    pub fn read() -> Result<Self, String> {
        use windows::Win32::System::Registry::{
            REG_VALUE_TYPE, RRF_NOEXPAND, RRF_RT_ANY, RegGetValueW,
        };
        let mut size = 0;
        let mut kind = REG_VALUE_TYPE::default();
        let status = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                RUN_KEY,
                VALUE_NAME,
                RRF_RT_ANY | RRF_NOEXPAND,
                Some(&mut kind),
                None,
                Some(&mut size),
            )
        };
        if status == ERROR_FILE_NOT_FOUND {
            return Ok(Self(None));
        }
        if status.is_err() || size > 65_536 {
            return Err("Could not read the existing startup registration.".to_string());
        }
        let mut data = vec![0u8; size as usize];
        let status = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                RUN_KEY,
                VALUE_NAME,
                RRF_RT_ANY | RRF_NOEXPAND,
                Some(&mut kind),
                Some(data.as_mut_ptr().cast()),
                Some(&mut size),
            )
        };
        if status.is_err() {
            return Err("The startup registration changed while it was being read.".to_string());
        }
        data.truncate(size as usize);
        Ok(Self(Some((kind, data))))
    }

    pub fn restore(self) -> Result<(), String> {
        let key = open_run_key()?;
        let status = match self.0 {
            Some((kind, data)) => unsafe {
                RegSetValueExW(key.0, VALUE_NAME, 0, kind, Some(&data))
            },
            None => unsafe { RegDeleteValueW(key.0, VALUE_NAME) },
        };
        if status.is_ok() || status == ERROR_FILE_NOT_FOUND {
            Ok(())
        } else {
            Err(format!(
                "Could not restore startup registration (error {}).",
                status.0
            ))
        }
    }
}

struct RegistryKey(HKEY);

impl Drop for RegistryKey {
    fn drop(&mut self) {
        unsafe {
            let _ = RegCloseKey(self.0);
        }
    }
}

fn open_run_key() -> Result<RegistryKey, String> {
    let mut key = HKEY::default();
    let status = unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            RUN_KEY,
            0,
            None,
            REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE,
            None,
            &mut key,
            None,
        )
    };
    if status.is_err() {
        Err(format!(
            "Could not open the Windows startup registry key (error {}).",
            status.0
        ))
    } else {
        Ok(RegistryKey(key))
    }
}

pub fn set_start_with_windows(enabled: bool) -> Result<(), String> {
    let key = open_run_key()?;
    if enabled {
        let executable = std::env::current_exe()
            .map_err(|error| format!("Could not locate isolmaSS.exe: {error}"))?;
        let command = format!("\"{}\"", executable.display());
        let wide: Vec<u16> = command.encode_utf16().chain(Some(0)).collect();
        let bytes =
            unsafe { std::slice::from_raw_parts(wide.as_ptr().cast::<u8>(), wide.len() * 2) };
        let status = unsafe { RegSetValueExW(key.0, VALUE_NAME, 0, REG_SZ, Some(bytes)) };
        if status.is_err() {
            return Err(format!(
                "Could not enable startup (registry error {}).",
                status.0
            ));
        }
    } else {
        let status = unsafe { RegDeleteValueW(key.0, VALUE_NAME) };
        if status.is_err() && status != ERROR_FILE_NOT_FOUND {
            return Err(format!(
                "Could not disable startup (registry error {}).",
                status.0
            ));
        }
    }
    Ok(())
}
