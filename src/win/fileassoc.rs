//! Registry integration: `.apk` file association plus "Open with android-emu".

use super::ffi::*;
use std::path::Path;

const PROG_ID: &str = "AndroidEmu.Apk";
const APP_KEY: &str = "Software\\AndroidEmu";
const CLASSES_APP_KEY: &str = "Software\\Classes\\Applications\\android-emu.exe";

#[allow(dead_code)]
fn create_key(root: HKEY, path: &str) -> Result<HKEY, String> {
    let w = to_wide(path);
    let mut key: HKEY = std::ptr::null_mut();
    let mut disposition: DWORD = 0;
    let rc = unsafe {
        RegCreateKeyExW(
            root,
            w.as_ptr(),
            0,
            std::ptr::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_ALL_ACCESS,
            std::ptr::null_mut(),
            &mut key,
            &mut disposition,
        )
    };
    if rc != 0 {
        return Err(format!("RegCreateKeyExW('{path}') failed with code {rc}"));
    }
    Ok(key)
}

fn set_string(root: HKEY, key_path: &str, name: &str, value: &str) -> Result<(), String> {
    let key = create_key(root, key_path)?;
    let name_w = to_wide(name);
    let value_w = to_wide(value);
    let bytes = unsafe {
        std::slice::from_raw_parts(value_w.as_ptr() as *const u8, value_w.len() * 2)
    };
    let rc = unsafe {
        RegSetValueExW(
            key,
            name_w.as_ptr(),
            0,
            REG_SZ,
            bytes.as_ptr(),
            bytes.len() as u32,
        )
    };
    unsafe { RegCloseKey(key) };
    if rc != 0 {
        return Err(format!("RegSetValueExW('{key_path}\\{name}') failed with code {rc}"));
    }
    Ok(())
}

fn delete_tree(root: HKEY, path: &str) {
    let w = to_wide(path);
    unsafe {
        RegDeleteTreeW(root, w.as_ptr());
    }
}

pub fn get_string(root: HKEY, key_path: &str, name: &str) -> Option<String> {
    unsafe {
        let key_w = to_wide(key_path);
        let mut key: HKEY = std::ptr::null_mut();
        if RegOpenKeyExW(root, key_w.as_ptr(), 0, KEY_READ, &mut key) != 0 {
            return None;
        }
        let name_w = to_wide(name);
        let mut buf = vec![0u8; 4096];
        let mut len = buf.len() as DWORD;
        let mut ty: DWORD = 0;
        let rc = RegQueryValueExW(
            key,
            name_w.as_ptr(),
            std::ptr::null_mut(),
            &mut ty,
            buf.as_mut_ptr(),
            &mut len,
        );
        RegCloseKey(key);
        if rc != 0 {
            return None;
        }
        let units: Vec<u16> = buf[..len as usize]
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        Some(from_wide(&units))
    }
}

/// Register `.apk` -> android-emu in HKCU so a double-click installs the APK.
pub fn register_apk_association(exe: &Path) -> Result<(), String> {
    let exe_str = exe.to_string_lossy().to_string();
    let icon = format!("\"{exe_str}\",0");
    let command = format!("\"{exe_str}\" --install \"%1\"");

    // ProgID description and commands.
    set_string(
        HKEY_CURRENT_USER,
        &format!("Software\\Classes\\{PROG_ID}"),
        "",
        "Android application package",
    )?;
    set_string(
        HKEY_CURRENT_USER,
        &format!("Software\\Classes\\{PROG_ID}\\DefaultIcon"),
        "",
        &icon,
    )?;
    set_string(
        HKEY_CURRENT_USER,
        &format!("Software\\Classes\\{PROG_ID}\\shell\\open"),
        "",
        "Install in android-emu",
    )?;
    set_string(
        HKEY_CURRENT_USER,
        &format!("Software\\Classes\\{PROG_ID}\\shell\\open\\command"),
        "",
        &command,
    )?;
    set_string(
        HKEY_CURRENT_USER,
        &format!("Software\\Classes\\{PROG_ID}\\shell\\open\\ddeexec"),
        "",
        "",
    )
    .ok();

    // Extension mapping.
    set_string(HKEY_CURRENT_USER, "Software\\Classes\\.apk", "", PROG_ID)?;
    set_string(
        HKEY_CURRENT_USER,
        "Software\\Classes\\.apk",
        "Content Type",
        "application/vnd.android.package-archive",
    )
    .ok();
    set_string(
        HKEY_CURRENT_USER,
        "Software\\Classes\\.apk\\OpenWithProgids",
        PROG_ID,
        "",
    )
    .ok();

    // "Open with" entry for the executable.
    set_string(
        HKEY_CURRENT_USER,
        CLASSES_APP_KEY,
        "FriendlyAppName",
        "android-emu (Android emulator)",
    )
    .ok();
    set_string(
        HKEY_CURRENT_USER,
        &format!("{CLASSES_APP_KEY}\\shell\\open\\command"),
        "",
        &command,
    )
    .ok();
    set_string(
        HKEY_CURRENT_USER,
        &format!("{CLASSES_APP_KEY}\\SupportedTypes"),
        ".apk",
        "",
    )
    .ok();

    // Our own bookkeeping (used by `doctor` and `--unregister`).
    set_string(HKEY_CURRENT_USER, APP_KEY, "InstallLocation", &exe_str)?;
    set_string(HKEY_CURRENT_USER, APP_KEY, "AssocVersion", "1")?;

    super::shortcut::notify_assoc_changed();
    Ok(())
}

pub fn unregister_apk_association() {
    delete_tree(HKEY_CURRENT_USER, &format!("Software\\Classes\\{PROG_ID}"));
    delete_tree(HKEY_CURRENT_USER, "Software\\Classes\\.apk");
    delete_tree(HKEY_CURRENT_USER, CLASSES_APP_KEY);
    super::shortcut::notify_assoc_changed();
}

pub fn is_registered() -> bool {
    get_string(
        HKEY_CURRENT_USER,
        &format!("Software\\Classes\\{PROG_ID}\\shell\\open\\command"),
        "",
    )
    .is_some()
}

/// Registered command line for `.apk` files, if any.
pub fn registered_command() -> Option<String> {
    get_string(
        HKEY_CURRENT_USER,
        &format!("Software\\Classes\\{PROG_ID}\\shell\\open\\command"),
        "",
    )
}

/// Check that resources required by Windows are installed (QEMU needs WHPX/HAXM,
/// which we only report — installation is documented in the README).
pub fn virtualization_hint() -> String {
    let qemu_probe = get_string(HKEY_LOCAL_MACHINE, "HARDWARE\\DESCRIPTION\\System", "SystemBiosVersion");
    let _ = qemu_probe;
    "Enable Windows Hypervisor Platform (optional, dism /online /enable-feature /featurename:HypervisorPlatform)".to_string()
}
