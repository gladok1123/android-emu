//! Where things live on disk: data directory, images, log files.

use std::path::{Path, PathBuf};

pub const APP_DIR_NAME: &str = "android-emu";

/// `%LOCALAPPDATA%\android-emu` on Windows, `~/.local/share/android-emu` elsewhere.
pub fn data_dir() -> PathBuf {
    #[cfg(windows)]
    {
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            return PathBuf::from(local).join(APP_DIR_NAME);
        }
        if let Some(appdata) = std::env::var_os("APPDATA") {
            return PathBuf::from(appdata).join(APP_DIR_NAME);
        }
    }
    if let Some(xdg) = std::env::var_os("XDG_DATA_HOME") {
        return PathBuf::from(xdg).join(APP_DIR_NAME);
    }
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home).join(".local/share").join(APP_DIR_NAME);
    }
    PathBuf::from(".").join(APP_DIR_NAME)
}

pub fn ensure_data_dir() -> std::io::Result<PathBuf> {
    let dir = data_dir();
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

pub fn apps_dir() -> PathBuf {
    data_dir().join("apps")
}

pub fn images_dir() -> PathBuf {
    data_dir().join("images")
}

pub fn qemu_dir() -> PathBuf {
    data_dir().join("qemu")
}

pub fn logs_dir() -> PathBuf {
    data_dir().join("logs")
}

pub fn adb_dir() -> PathBuf {
    data_dir().join("adb")
}

pub fn desktop_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        use crate::win::ffi::*;
        // CSIDL_DESKTOPDIRECTORY = 0x0010, CSIDL_COMMON_DESKTOPDIRECTORY = 0x0019
        unsafe {
            let mut buf = vec![0u16; 260 + 1];
            if SHGetFolderPathW(std::ptr::null_mut(), 0x0010, std::ptr::null_mut(), 0, buf.as_mut_ptr()) == 0 {
                let s = from_wide(&buf);
                if !s.is_empty() {
                    return Some(PathBuf::from(s));
                }
            }
        }
    }
    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join("Desktop"))
}

pub fn start_menu_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        use crate::win::ffi::*;
        // CSIDL_PROGRAMS = 0x0002
        unsafe {
            let mut buf = vec![0u16; 260 + 1];
            if SHGetFolderPathW(std::ptr::null_mut(), 0x0002, std::ptr::null_mut(), 0, buf.as_mut_ptr()) == 0 {
                let s = from_wide(&buf);
                if !s.is_empty() {
                    return Some(PathBuf::from(s).join("android-emu"));
                }
            }
        }
    }
    None
}

/// Make sure a directory exists and return it.
pub fn ensure(path: &Path) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(path)?;
    Ok(path.to_path_buf())
}

/// Default install location for shortcuts: the Desktop.
pub fn default_shortcut_dir() -> PathBuf {
    desktop_dir().unwrap_or_else(|| data_dir().join("Desktop"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_dir_ends_with_app_name() {
        let d = data_dir();
        assert!(d.ends_with(APP_DIR_NAME), "unexpected data dir {d:?}");
    }
}
