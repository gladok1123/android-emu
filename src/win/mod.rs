//! Windows-specific integration: shortcuts (`.lnk`), file associations,
//! extracted icons, registry access and Win32 FFI declarations.

#[cfg(windows)]
pub mod ffi;

#[cfg(windows)]
pub mod shortcut;

#[cfg(windows)]
pub mod fileassoc;

#[cfg(windows)]
pub mod known_folders;

#[cfg(windows)]
pub mod system;

/// Console helpers exist on every platform (no-ops elsewhere).
#[cfg(windows)]
#[path = "console.rs"]
pub mod console;

#[cfg(not(windows))]
pub mod console {
    pub fn hide_console_if_owning() {}
    pub fn detach() {}
    pub fn attach_parent() -> bool {
        false
    }
    pub fn set_title(_title: &str) {}
    pub fn color_enabled() -> bool {
        false
    }
    pub fn message_box(text: &str, caption: &str, _error: bool) {
        eprintln!("{caption}: {text}");
    }
}

#[cfg(windows)]
pub use shortcut::exe_path;

#[cfg(not(windows))]
pub use stub::exe_path;

#[cfg(windows)]
pub fn is_whpx_enabled() -> bool {
    system::is_whpx_enabled()
}

#[cfg(windows)]
pub fn is_haxm_installed() -> bool {
    system::is_haxm_installed()
}

#[cfg(not(windows))]
pub fn is_whpx_enabled() -> bool {
    false
}

#[cfg(not(windows))]
pub fn is_haxm_installed() -> bool {
    false
}

/// Is a process with this pid still running?
#[cfg(windows)]
pub fn process_is_alive(pid: u32) -> bool {
    system::process_is_alive(pid)
}

#[cfg(not(windows))]
pub fn process_is_alive(pid: u32) -> bool {
    std::path::Path::new(&format!("/proc/{pid}")).exists()
}

pub mod stub;
