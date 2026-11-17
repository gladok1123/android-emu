//! System capability probing: virtualization support, WHPX, HAXM.

use super::fileassoc::get_string;
use super::ffi::HKEY_LOCAL_MACHINE;

fn reg_string(path: &str, name: &str) -> Option<String> {
    get_string(HKEY_LOCAL_MACHINE, path, name)
}

/// Windows Hypervisor Platform enabled (required by `-accel whpx`).
pub fn is_whpx_enabled() -> bool {
    if let Some(v) = reg_string(
        "SYSTEM\\CurrentControlSet\\Control\\Windows Hypervisor Platform",
        "Enabled",
    ) {
        if v == "1" {
            return true;
        }
    }
    // Windows 11 enables WHPX through the optional feature list; the service
    // presence is a good proxy.
    reg_string(
        "SYSTEM\\CurrentControlSet\\Services\\vmcompute",
        "DisplayName",
    )
    .is_some()
        || reg_string(
            "SYSTEM\\CurrentControlSet\\Services\\WinHvPlatform",
            "DisplayName",
        )
        .is_some()
}

/// Intel HAXM driver installed.
pub fn is_haxm_installed() -> bool {
    reg_string("SOFTWARE\\Intel\\HAXM", "Version").is_some()
        || reg_string("SYSTEM\\CurrentControlSet\\Services\\intelhaxm", "DisplayName").is_some()
}

/// Is a process with this pid running?
pub fn process_is_alive(pid: u32) -> bool {
    #[repr(C)]
    struct ProcessEntry32W {
        dw_size: u32,
        cnt_usage: u32,
        th32_process_id: u32,
        th32_default_heap_id: usize,
        th32_module_id: u32,
        cnt_threads: u32,
        th32_parent_process_id: u32,
        pc_pri_class_base: i32,
        dw_flags: u32,
        sz_exe_file: [u16; 260],
    }

    impl Default for ProcessEntry32W {
        fn default() -> Self {
            ProcessEntry32W {
                dw_size: 0,
                cnt_usage: 0,
                th32_process_id: 0,
                th32_default_heap_id: 0,
                th32_module_id: 0,
                cnt_threads: 0,
                th32_parent_process_id: 0,
                pc_pri_class_base: 0,
                dw_flags: 0,
                sz_exe_file: [0; 260],
            }
        }
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn CreateToolhelp32Snapshot(flags: u32, pid: u32) -> *mut core::ffi::c_void;
        fn Process32FirstW(snapshot: *mut core::ffi::c_void, entry: *mut ProcessEntry32W) -> i32;
        fn Process32NextW(snapshot: *mut core::ffi::c_void, entry: *mut ProcessEntry32W) -> i32;
        fn CloseHandle(handle: *mut core::ffi::c_void) -> i32;
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> *mut core::ffi::c_void;
        fn GetExitCodeProcess(handle: *mut core::ffi::c_void, code: *mut u32) -> i32;
    }
    const TH32CS_SNAPPROCESS: u32 = 0x0000_0002;
    const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
    const STILL_ACTIVE: u32 = 259;
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if !handle.is_null() {
            let mut code: u32 = 0;
            let ok = GetExitCodeProcess(handle, &mut code);
            CloseHandle(handle);
            return ok != 0 && code == STILL_ACTIVE;
        }
        // Fall back to the process list (OpenProcess fails for higher-integrity
        // processes, but the pid still shows up in the snapshot).
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snap.is_null() || snap as isize == -1 {
            return false;
        }
        let mut entry = ProcessEntry32W {
            dw_size: std::mem::size_of::<ProcessEntry32W>() as u32,
            ..Default::default()
        };
        let mut alive = false;
        let mut ok = Process32FirstW(snap, &mut entry);
        while ok != 0 {
            if entry.th32_process_id == pid {
                alive = true;
                break;
            }
            ok = Process32NextW(snap, &mut entry);
        }
        CloseHandle(snap);
        alive
    }
}

/// Physical memory in MB (for default RAM sizing).
pub fn total_memory_mb() -> Option<u64> {
    use std::mem;
    #[repr(C)]
    #[derive(Default)]
    struct MemoryStatusEx {
        dw_length: u32,
        dw_memory_load: u32,
        ull_total_phys: u64,
        ull_avail_phys: u64,
        ull_total_page_file: u64,
        ull_avail_page_file: u64,
        ull_total_virtual: u64,
        ull_avail_virtual: u64,
        ull_avail_extended_virtual: u64,
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn GlobalMemoryStatusEx(buffer: *mut MemoryStatusEx) -> i32;
    }
    let mut status = MemoryStatusEx {
        dw_length: mem::size_of::<MemoryStatusEx>() as u32,
        ..Default::default()
    };
    let ok = unsafe { GlobalMemoryStatusEx(&mut status) };
    if ok != 0 {
        Some(status.ull_total_phys / (1024 * 1024))
    } else {
        None
    }
}

/// CPU vendor (used for acceleration hints).
pub fn cpu_vendor() -> String {
    std::env::var("PROCESSOR_IDENTIFIER").unwrap_or_else(|_| "unknown".to_string())
}

pub fn summary() -> Vec<(String, String)> {
    let mut rows = Vec::new();
    rows.push((
        "Virtualization".to_string(),
        if is_whpx_enabled() {
            "Windows Hypervisor Platform (enabled)".to_string()
        } else if is_haxm_installed() {
            "Intel HAXM (installed)".to_string()
        } else {
            "not enabled - QEMU will fall back to software emulation (slow)".to_string()
        },
    ));
    if let Some(mem) = total_memory_mb() {
        rows.push(("Physical memory".to_string(), format!("{mem} MB")));
    }
    rows.push(("CPU".to_string(), cpu_vendor()));
    rows
}
