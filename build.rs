use std::path::{Path, PathBuf};
use std::process::Command;

fn resource_compiler() -> PathBuf {
    if let Some(configured) = std::env::var_os("RC") {
        return PathBuf::from(configured);
    }
    let kits = Path::new(r"C:\Program Files (x86)\Windows Kits\10\bin");
    let mut versions: Vec<PathBuf> = std::fs::read_dir(kits)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .map(|entry| entry.path().join("x64").join("rc.exe"))
        .filter(|path| path.is_file())
        .collect();
    versions.sort();
    versions.pop().unwrap_or_else(|| PathBuf::from("rc.exe"))
}

fn main() {
    let manifest_dir = PathBuf::from(
        std::env::var_os("CARGO_MANIFEST_DIR").expect("Cargo must provide CARGO_MANIFEST_DIR"),
    );
    let resources = manifest_dir.join("resources");
    let output = PathBuf::from(std::env::var_os("OUT_DIR").expect("Cargo must provide OUT_DIR"))
        .join("app.res");
    let status = Command::new(resource_compiler())
        .current_dir(&resources)
        .args(["/nologo", "/fo"])
        .arg(&output)
        .arg("app.rc")
        .status()
        .expect("failed to launch the Windows resource compiler (set RC to rc.exe)");
    assert!(status.success(), "Windows resource compilation failed");

    println!("cargo:rerun-if-changed=resources/app.rc");
    println!("cargo:rerun-if-changed=resources/app.ico");
    println!("cargo:rerun-if-changed=resources/app.manifest");
    println!("cargo:rustc-link-arg={}", output.display());
}
