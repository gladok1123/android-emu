//! Shell-known folders (Desktop, Start Menu) via `SHGetFolderPathW`.

use super::ffi::*;
use std::path::PathBuf;

const CSIDL_DESKTOPDIRECTORY: i32 = 0x0010;
const CSIDL_COMMON_DESKTOPDIRECTORY: i32 = 0x0019;
const CSIDL_PROGRAMS: i32 = 0x0002;
const CSIDL_APPDATA: i32 = 0x001A;
const CSIDL_LOCAL_APPDATA: i32 = 0x001C;
const CSIDL_PERSONAL: i32 = 0x0005;

fn folder(csidl: i32) -> Option<PathBuf> {
    unsafe {
        let mut buf = vec![0u16; 1024];
        let hr = SHGetFolderPathW(std::ptr::null_mut(), csidl, std::ptr::null_mut(), 0, buf.as_mut_ptr());
        if hr != 0 {
            return None;
        }
        let s = from_wide(&buf);
        if s.is_empty() {
            None
        } else {
            Some(PathBuf::from(s))
        }
    }
}

pub fn desktop() -> Option<PathBuf> {
    folder(CSIDL_DESKTOPDIRECTORY)
}

pub fn public_desktop() -> Option<PathBuf> {
    folder(CSIDL_COMMON_DESKTOPDIRECTORY)
}

pub fn start_menu_programs() -> Option<PathBuf> {
    folder(CSIDL_PROGRAMS)
}

pub fn documents() -> Option<PathBuf> {
    folder(CSIDL_PERSONAL)
}

pub fn local_appdata() -> Option<PathBuf> {
    folder(CSIDL_LOCAL_APPDATA)
}

pub fn roaming_appdata() -> Option<PathBuf> {
    folder(CSIDL_APPDATA)
}
