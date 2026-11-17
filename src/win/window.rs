//! Controlling the QEMU window from the outside.
//!
//! After QEMU starts, Android's display sits inside a normal Windows window.
//! The launcher finds that window by its QEMU process id and can resize it to
//! the size configured for the app, or make it fullscreen/borderless. This is
//! what makes "настроить размер окна" work for the actual Android screen.

use super::ffi::*;
use std::ffi::c_void;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowMode {
    /// Resize the client area to exactly WxH.
    Resize(u32, u32),
    /// Maximise the window.
    Maximize,
    /// Borderless fullscreen on the primary monitor.
    BorderlessFullscreen,
    /// Restore to a normal window.
    Normal,
}

struct FindContext {
    pid: u32,
    best: HWND,
    best_area: i64,
}

/// Find the main visible top-level window owned by `pid`.
pub fn find_window_for_pid(pid: u32) -> Option<HWND> {
    unsafe extern "system" fn callback(hwnd: HWND, param: *mut c_void) -> BOOL {
        let ctx = &mut *(param as *mut FindContext);
        let mut owner: u32 = 0;
        GetWindowThreadProcessId(hwnd, &mut owner);
        if owner != ctx.pid {
            return 1;
        }
        if IsWindowVisible(hwnd) == 0 {
            return 1;
        }
        let mut rect = RectL::default();
        if GetWindowRect(hwnd, &mut rect) == 0 {
            return 1;
        }
        let width = (rect.right - rect.left).max(0) as i64;
        let height = (rect.bottom - rect.top).max(0) as i64;
        let area = width * height;
        if area > ctx.best_area && width > 80 && height > 80 {
            ctx.best_area = area;
            ctx.best = hwnd;
        }
        1
    }

    let mut ctx = FindContext {
        pid,
        best: std::ptr::null_mut(),
        best_area: 0,
    };
    unsafe {
        EnumWindows(callback, &mut ctx as *mut FindContext as *mut c_void);
    }
    if ctx.best.is_null() {
        None
    } else {
        Some(ctx.best)
    }
}

/// Wait for the QEMU window to appear (the guest can take a while to boot).
pub fn wait_for_window(pid: u32, timeout: std::time::Duration) -> Option<HWND> {
    let start = std::time::Instant::now();
    while start.elapsed() < timeout {
        if let Some(hwnd) = find_window_for_pid(pid) {
            return Some(hwnd);
        }
        std::thread::sleep(std::time::Duration::from_millis(400));
    }
    None
}

/// Resize (and optionally move) the QEMU window so its *client* area is WxH.
pub fn resize_to_client(hwnd: HWND, width: u32, height: u32) -> bool {
    unsafe {
        let mut info = WindowInfo {
            cb_size: std::mem::size_of::<WindowInfo>() as u32,
            ..std::mem::zeroed()
        };
        if GetWindowInfo(hwnd, &mut info) == 0 {
            return false;
        }
        let dw = (info.rc_window.right - info.rc_window.left)
            - (info.rc_client.right - info.rc_client.left);
        let dh = (info.rc_window.bottom - info.rc_window.top)
            - (info.rc_client.bottom - info.rc_client.top);
        let target_w = width as i32 + dw.max(0);
        let target_h = height as i32 + dh.max(0);
        let screen_w = GetSystemMetrics(0);
        let screen_h = GetSystemMetrics(1);
        let x = ((screen_w - target_w) / 2).max(0);
        let y = ((screen_h - target_h) / 2).max(0);
        MoveWindow(hwnd, x, y, target_w, target_h, 1) != 0
    }
}

/// Apply a window mode to the QEMU window.
pub fn apply_mode(hwnd: HWND, mode: WindowMode) -> bool {
    unsafe {
        match mode {
            WindowMode::Resize(w, h) => {
                ShowWindow(hwnd, SW_RESTORE);
                resize_to_client(hwnd, w, h)
            }
            WindowMode::Maximize => ShowWindow(hwnd, SW_MAXIMIZE) != 0,
            WindowMode::Normal => ShowWindow(hwnd, SW_RESTORE) != 0,
            WindowMode::BorderlessFullscreen => {
                let screen_w = GetSystemMetrics(0);
                let screen_h = GetSystemMetrics(1);
                SetWindowPos(
                    hwnd,
                    HWND_TOPMOST,
                    0,
                    0,
                    screen_w,
                    screen_h,
                    SWP_SHOWWINDOW,
                ) != 0
            }
        }
    }
}

/// Fullscreen helper that keeps the taskbar hidden for the emulator window.
pub fn set_borderless(hwnd: HWND, on: bool) -> bool {
    if on {
        apply_mode(hwnd, WindowMode::BorderlessFullscreen)
    } else {
        apply_mode(hwnd, WindowMode::Normal)
    }
}

/// Bring the QEMU window to the foreground.
pub fn focus(hwnd: HWND) -> bool {
    unsafe {
        ShowWindow(hwnd, SW_RESTORE);
        SetForegroundWindow(hwnd) != 0
    }
}

pub fn close(hwnd: HWND) {
    unsafe {
        PostMessageW(hwnd, WM_CLOSE, 0, 0);
    }
}

/// Fallback used when the QEMU window cannot be identified: locate a window
/// whose title contains the session name (QEMU sets `-name` as its title).
pub fn find_window_by_title(needle: &str) -> Option<HWND> {
    struct TitleContext<'a> {
        needle: &'a str,
        found: HWND,
    }
    unsafe extern "system" fn callback(hwnd: HWND, param: *mut c_void) -> BOOL {
        let ctx = &mut *(param as *mut TitleContext<'_>);
        if IsWindowVisible(hwnd) == 0 {
            return 1;
        }
        let len = GetWindowTextLengthW(hwnd);
        if len <= 0 {
            return 1;
        }
        let mut buf = vec![0u16; len as usize + 2];
        GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32);
        let text = from_wide(&buf).to_lowercase();
        if text.contains(&ctx.needle.to_lowercase()) {
            ctx.found = hwnd;
            return 0;
        }
        1
    }
    let mut ctx = TitleContext {
        needle,
        found: std::ptr::null_mut(),
    };
    unsafe {
        EnumWindows(callback, &mut ctx as *mut TitleContext<'_> as *mut c_void);
    }
    if ctx.found.is_null() {
        None
    } else {
        Some(ctx.found)
    }
}

const SW_MAXIMIZE: i32 = 3;
const SW_RESTORE: i32 = 9;
const HWND_TOPMOST: HWND = -1i32 as isize as HWND;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_values_are_distinct() {
        assert_ne!(WindowMode::Maximize, WindowMode::Normal);
        assert_eq!(WindowMode::Resize(10, 20), WindowMode::Resize(10, 20));
    }
}
