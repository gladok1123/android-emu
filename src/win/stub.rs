//! Non-Windows placeholders so the crate also compiles (and its logic can be
//! tested) on Linux/macOS CI machines.

#[cfg(not(windows))]
use std::path::Path;

#[cfg(not(windows))]
pub fn shell_open(path: &Path) -> Result<(), String> {
    let _ = path;
    Err("shell integration is only available on Windows".to_string())
}

#[cfg(not(windows))]
pub fn notify_assoc_changed() {}

#[cfg(not(windows))]
pub fn exe_path() -> std::path::PathBuf {
    std::env::current_exe().unwrap_or_else(|_| std::path::PathBuf::from("android-emu"))
}

#[cfg(not(windows))]
pub fn reg_get(
    _root: usize,
    _key: &str,
    _name: &str,
) -> Option<String> {
    None
}

#[cfg(not(windows))]
pub const HKEY_CURRENT_USER: usize = 0;
#[cfg(not(windows))]
pub const HKEY_LOCAL_MACHINE: usize = 0;

#[cfg(not(windows))]
pub fn assoc_registered() -> bool {
    false
}
