//! Console helpers: detach/attach the Windows console so the GUI can run
//! without a black window while the CLI still works from a terminal.

#[cfg(windows)]
use super::ffi::*;

/// Hide the console window when the process was launched from Explorer.
pub fn hide_console_if_detached() {
    #[cfg(windows)]
    unsafe {
        // Only hide when the console is not owned by a parent shell session that
        // the user opened deliberately: if there is no console, nothing to do.
        let hwnd = GetConsoleWindow();
        if !hwnd.is_null() {
            // Keep the window when this process is the console's only owner AND
            // the console was created just for us (heuristic: no arguments => GUI).
            // The caller decides; see `detach()`.
        }
    }
}

/// Fully detach from the console (the GUI uses this).
pub fn detach() {
    #[cfg(windows)]
    unsafe {
        if !GetConsoleWindow().is_null() {
            FreeConsole();
        }
    }
}

/// Hide the console window when this process *owns* the console (i.e. it was
/// started by a double-click from Explorer, not from an existing terminal).
/// This is what keeps shortcut launches free of a black window.
pub fn hide_console_if_owning() {
    #[cfg(windows)]
    unsafe {
        #[link(name = "kernel32")]
        extern "system" {
            fn GetConsoleProcessList(list: *mut u32, count: u32) -> u32;
            fn GetConsoleWindow() -> HWND;
        }
        #[link(name = "user32")]
        extern "system" {
            fn ShowWindow(hwnd: HWND, cmd: i32) -> i32;
        }
        const SW_HIDE: i32 = 0;
        let mut pids = [0u32; 4];
        let count = GetConsoleProcessList(pids.as_mut_ptr(), pids.len() as u32);
        let hwnd = GetConsoleWindow();
        if count <= 1 && !hwnd.is_null() {
            ShowWindow(hwnd, SW_HIDE);
        }
    }
}

/// Attach to the parent console, so `android-emu run ... > out.txt` prints.
pub fn attach_parent() -> bool {
    #[cfg(windows)]
    unsafe {
        if AttachConsole(0xFFFF_FFFF) != 0 {
            SetConsoleOutputCP(65001);
            SetConsoleCP(65001);
            return true;
        }
        false
    }
    #[cfg(not(windows))]
    {
        false
    }
}

pub fn set_title(title: &str) {
    #[cfg(windows)]
    unsafe {
        let w = super::ffi::to_wide(title);
        SetConsoleTitleW(w.as_ptr());
        SetConsoleOutputCP(65001);
    }
    #[cfg(not(windows))]
    {
        let _ = title;
    }
}

/// Show a native message box (used for GUI-level errors).
pub fn message_box(text: &str, caption: &str, error: bool) {
    #[cfg(windows)]
    unsafe {
        let t = super::ffi::to_wide(text);
        let c = super::ffi::to_wide(caption);
        let flags: u32 = 0x0000_0040 | 0x0004_0000 | if error { 0x0000_0010 } else { 0x0000_0040 };
        MessageBoxW(std::ptr::null_mut(), t.as_ptr(), c.as_ptr(), flags);
    }
    #[cfg(not(windows))]
    {
        eprintln!("{caption}: {text}");
    }
}

/// Colored console output (disabled automatically when redirected).
pub fn color_enabled() -> bool {
    #[cfg(windows)]
    {
        std::env::var_os("NO_COLOR").is_none() && std::env::var_os("CLICOLOR_FORCE").is_none()
    }
    #[cfg(not(windows))]
    {
        std::env::var_os("TERM").is_some() && std::env::var_os("NO_COLOR").is_none()
    }
}
