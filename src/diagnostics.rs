//! Local, bounded operational diagnostics. Never record pixels, typed text or keys.
use std::io::Write;
use std::sync::Mutex;
static WRITER: Mutex<()> = Mutex::new(());

pub fn record(category: &str, message: &str) {
    if cfg!(test) {
        return;
    }
    let Ok(_lock) = WRITER.lock() else {
        return;
    };
    let Some(root) = std::env::var_os("LOCALAPPDATA") else {
        return;
    };
    let root = std::path::PathBuf::from(root).join("isolmaSS").join("logs");
    let result = (|| -> std::io::Result<()> {
        std::fs::create_dir_all(&root)?;
        let path = root.join("diagnostic.log");
        if std::fs::metadata(&path).is_ok_and(|metadata| metadata.len() >= 1_048_576) {
            let previous = root.join("diagnostic.previous.log");
            if previous.exists() {
                std::fs::remove_file(&previous)?;
            }
            std::fs::rename(&path, previous)?;
        }
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |time| time.as_secs());
        let message: String = message
            .chars()
            .take(2048)
            .map(|ch| if ch.is_control() { ' ' } else { ch })
            .collect();
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;
        writeln!(
            file,
            "{timestamp} [{}] [{category}] {message}",
            env!("CARGO_PKG_VERSION")
        )
    })();
    if let Err(error) = result {
        std::eprintln!("Diagnostic log unavailable: {error}");
    }
}
