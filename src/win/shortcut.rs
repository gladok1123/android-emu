//! Creation of Windows `.lnk` shortcuts through the `IShellLink` COM interface.
//!
//! This is the piece that makes an installed APK behave like a normal desktop
//! program: a real shortcut with a real embedded icon.

#![allow(non_snake_case)]

use super::ffi::*;
use std::ffi::c_void;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;

// CLSID_ShellLink {00021401-0000-0000-C000-000000000046}
// {00021401-0000-0000-C000-000000000046}
const CLSID_SHELL_LINK: GUID = GUID {
    data1: 0x0002_1401,
    data2: 0x0000,
    data3: 0x0000,
    data4: [0xC0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x46],
};
// IID_IShellLinkW {000214F9-0000-0000-C000-000000000046}
// {000214F9-0000-0000-C000-000000000046}
const IID_ISHELL_LINK_W: GUID = GUID {
    data1: 0x0002_14f9,
    data2: 0x0000,
    data3: 0x0000,
    data4: [0xC0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x46],
};
// IID_IPersistFile {0000010B-0000-0000-C000-000000000046}
// {0000010B-0000-0000-C000-000000000046}
const IID_IPERSIST_FILE: GUID = GUID {
    data1: 0x0000_010b,
    data2: 0x0000,
    data3: 0x0000,
    data4: [0xC0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x46],
};

const SW_SHOWNORMAL: i32 = 1;

#[repr(C)]
struct IShellLinkW {
    vtbl: *const IShellLinkWVtbl,
}

#[repr(C)]
struct IShellLinkWVtbl {
    QueryInterface: unsafe extern "system" fn(*mut IShellLinkW, *const GUID, *mut *mut c_void) -> HRESULT,
    AddRef: unsafe extern "system" fn(*mut IShellLinkW) -> ULONG,
    Release: unsafe extern "system" fn(*mut IShellLinkW) -> ULONG,
    GetPath: unsafe extern "system" fn(*mut IShellLinkW, *mut u16, i32, *mut c_void, u32) -> HRESULT,
    GetIDList: unsafe extern "system" fn(*mut IShellLinkW, *mut *mut c_void) -> HRESULT,
    SetIDList: unsafe extern "system" fn(*mut IShellLinkW, *const c_void) -> HRESULT,
    GetDescription: unsafe extern "system" fn(*mut IShellLinkW, *mut u16, i32) -> HRESULT,
    SetDescription: unsafe extern "system" fn(*mut IShellLinkW, *const u16) -> HRESULT,
    GetWorkingDirectory: unsafe extern "system" fn(*mut IShellLinkW, *mut u16, i32) -> HRESULT,
    SetWorkingDirectory: unsafe extern "system" fn(*mut IShellLinkW, *const u16) -> HRESULT,
    GetArguments: unsafe extern "system" fn(*mut IShellLinkW, *mut u16, i32) -> HRESULT,
    SetArguments: unsafe extern "system" fn(*mut IShellLinkW, *const u16) -> HRESULT,
    GetHotkey: unsafe extern "system" fn(*mut IShellLinkW, *mut u16) -> HRESULT,
    SetHotkey: unsafe extern "system" fn(*mut IShellLinkW, u16) -> HRESULT,
    GetShowCmd: unsafe extern "system" fn(*mut IShellLinkW, *mut i32) -> HRESULT,
    SetShowCmd: unsafe extern "system" fn(*mut IShellLinkW, i32) -> HRESULT,
    GetIconLocation: unsafe extern "system" fn(*mut IShellLinkW, *mut u16, i32, *mut i32) -> HRESULT,
    SetIconLocation: unsafe extern "system" fn(*mut IShellLinkW, *const u16, i32) -> HRESULT,
    SetRelativePath: unsafe extern "system" fn(*mut IShellLinkW, *const u16, u32) -> HRESULT,
    Resolve: unsafe extern "system" fn(*mut IShellLinkW, *mut c_void, u32) -> HRESULT,
    SetPath: unsafe extern "system" fn(*mut IShellLinkW, *const u16) -> HRESULT,
}

#[repr(C)]
struct IPersistFile {
    vtbl: *const IPersistFileVtbl,
}

#[repr(C)]
struct IPersistFileVtbl {
    QueryInterface: unsafe extern "system" fn(*mut IPersistFile, *const GUID, *mut *mut c_void) -> HRESULT,
    AddRef: unsafe extern "system" fn(*mut IPersistFile) -> ULONG,
    Release: unsafe extern "system" fn(*mut IPersistFile) -> ULONG,
    GetClassID: unsafe extern "system" fn(*mut IPersistFile, *mut GUID) -> HRESULT,
    IsDirty: unsafe extern "system" fn(*mut IPersistFile) -> HRESULT,
    Load: unsafe extern "system" fn(*mut IPersistFile, *const u16, u32) -> HRESULT,
    Save: unsafe extern "system" fn(*mut IPersistFile, *const u16, BOOL) -> HRESULT,
    SaveCompleted: unsafe extern "system" fn(*mut IPersistFile, *const u16) -> HRESULT,
    GetCurFile: unsafe extern "system" fn(*mut IPersistFile, *mut *mut u16) -> HRESULT,
}

/// Everything that goes into a shortcut.
pub struct ShortcutSpec<'a> {
    pub target: &'a Path,
    pub arguments: &'a str,
    pub working_dir: &'a Path,
    pub icon_file: Option<&'a Path>,
    pub description: &'a str,
}

/// Ensure COM is initialised on the current thread (idempotent enough in practice).
pub fn init_com() {
    unsafe {
        CoInitializeEx(std::ptr::null_mut(), COINIT_APARTMENTTHREADED);
    }
}

/// Write a `.lnk` file. Returns a human readable error on failure.
pub fn create_shortcut(lnk_path: &Path, spec: &ShortcutSpec<'_>) -> Result<(), String> {
    init_com();
    unsafe {
        let mut link: *mut c_void = std::ptr::null_mut();
        let hr = CoCreateInstance(
            &CLSID_SHELL_LINK,
            std::ptr::null_mut(),
            CLSCTX_INPROC_SERVER,
            &IID_ISHELL_LINK_W,
            &mut link,
        );
        if hr != S_OK || link.is_null() {
            return Err(format!("CoCreateInstance(ShellLink) failed: 0x{hr:08X}"));
        }
        let shell_link = link as *mut IShellLinkW;
        let vtbl = &*(*shell_link).vtbl;

        let target_w = to_wide(&spec.target.to_string_lossy());
        let args_w = to_wide(spec.arguments);
        let wd_w = to_wide(&spec.working_dir.to_string_lossy());
        let desc_w = to_wide(spec.description);

        let _ = (vtbl.SetPath)(shell_link, target_w.as_ptr());
        let _ = (vtbl.SetArguments)(shell_link, args_w.as_ptr());
        let _ = (vtbl.SetWorkingDirectory)(shell_link, wd_w.as_ptr());
        let _ = (vtbl.SetDescription)(shell_link, desc_w.as_ptr());
        let _ = (vtbl.SetShowCmd)(shell_link, SW_SHOWNORMAL);
        if let Some(icon) = spec.icon_file {
            let icon_w = to_wide(&icon.to_string_lossy());
            let _ = (vtbl.SetIconLocation)(shell_link, icon_w.as_ptr(), 0);
        }

        let mut persist: *mut c_void = std::ptr::null_mut();
        let hr = (vtbl.QueryInterface)(shell_link, &IID_IPERSIST_FILE, &mut persist);
        if hr != S_OK || persist.is_null() {
            (vtbl.Release)(shell_link);
            return Err(format!("QueryInterface(IPersistFile) failed: 0x{hr:08X}"));
        }
        let persist_file = persist as *mut IPersistFile;
        let pvtbl = &*(*persist_file).vtbl;

        if let Some(parent) = lnk_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let lnk_w = to_wide(&lnk_path.to_string_lossy());
        let hr = (pvtbl.Save)(persist_file, lnk_w.as_ptr(), 1);
        (pvtbl.Release)(persist_file);
        (vtbl.Release)(shell_link);
        if hr != S_OK {
            return Err(format!("IPersistFile::Save failed: 0x{hr:08X}"));
        }
    }
    Ok(())
}

/// Extract icon `index` from a PE executable into a standalone `.ico` file.
pub fn extract_icon_from_exe(exe: &Path, index: i32, out_ico: &Path) -> Result<(), String> {
    unsafe {
        let exe_w = to_wide(&exe.to_string_lossy());
        let mut hicon: HANDLE = std::ptr::null_mut();
        let mut icon_id: u32 = 0;
        let count = PrivateExtractIconsW(
            exe_w.as_ptr(),
            index,
            256,
            256,
            &mut hicon,
            &mut icon_id,
            1,
            0,
        );
        if count == 0 || hicon.is_null() {
            // Retry with a smaller size; some resources only exist at 32/48 px.
            let count = PrivateExtractIconsW(
                exe_w.as_ptr(),
                index,
                48,
                48,
                &mut hicon,
                &mut icon_id,
                1,
                0,
            );
            if count == 0 || hicon.is_null() {
                return Err("no icon found in the executable".to_string());
            }
        }
        let ok = write_hicon_as_ico(hicon, out_ico);
        DestroyIcon(hicon);
        ok
    }
}

/// Serialise an `HICON` into the `.ico` file format.
unsafe fn write_hicon_as_ico(hicon: HANDLE, out: &Path) -> Result<(), String> {
    #[repr(C)]
    #[derive(Default)]
    struct IconInfo {
        fIcon: i32,
        xHotspot: u32,
        yHotspot: u32,
        hbmMask: HANDLE,
        hbmColor: HANDLE,
    }
    #[repr(C)]
    #[derive(Default)]
    struct Bitmap {
        bmType: i32,
        bmWidth: i32,
        bmHeight: i32,
        bmWidthBytes: i32,
        bmPlanes: u16,
        bmBitsPixel: u16,
        bmBits: *mut c_void,
    }
    #[repr(C)]
    #[derive(Default)]
    struct BitmapInfoHeader {
        biSize: u32,
        biWidth: i32,
        biHeight: i32,
        biPlanes: u16,
        biBitCount: u16,
        biCompression: u32,
        biSizeImage: u32,
        biXPelsPerMeter: i32,
        biYPelsPerMeter: i32,
        biClrUsed: u32,
        biClrImportant: u32,
    }
    #[link(name = "gdi32")]
    extern "system" {
        fn GetObjectW(h: HANDLE, c: i32, pv: *mut c_void) -> i32;
        fn GetDIBits(
            hdc: HANDLE,
            hbm: HANDLE,
            start: u32,
            lines: u32,
            bits: *mut c_void,
            bmi: *mut BitmapInfoHeader,
            usage: u32,
        ) -> i32;
        fn CreateCompatibleDC(hdc: HANDLE) -> HANDLE;
        fn DeleteDC(hdc: HANDLE) -> i32;
        fn DeleteObject(ho: HANDLE) -> i32;
    }
    #[link(name = "user32")]
    extern "system" {
        fn GetIconInfo(hicon: HANDLE, info: *mut IconInfo) -> BOOL;
        fn GetDC(hwnd: HWND) -> HANDLE;
        fn ReleaseDC(hwnd: HWND, hdc: HANDLE) -> i32;
    }

    let mut info = IconInfo::default();
    if GetIconInfo(hicon, &mut info) == 0 {
        return Err("GetIconInfo failed".to_string());
    }
    let mut bm = Bitmap::default();
    GetObjectW(info.hbmColor, std::mem::size_of::<Bitmap>() as i32, &mut bm as *mut _ as *mut c_void);
    let width = if bm.bmWidth > 0 { bm.bmWidth } else { 48 };
    let height = if bm.bmHeight > 0 { bm.bmHeight } else { 48 };

    // Ask GDI for a 32bpp top-down DIB of the colour bitmap.
    let screen_dc = GetDC(std::ptr::null_mut());
    let dc = CreateCompatibleDC(screen_dc);
    let mut bih = BitmapInfoHeader {
        biSize: std::mem::size_of::<BitmapInfoHeader>() as u32,
        biWidth: width,
        biHeight: -height, // top-down
        biPlanes: 1,
        biBitCount: 32,
        biCompression: 0, // BI_RGB
        ..Default::default()
    };
    let mut color = vec![0u8; (width * height * 4) as usize];
    let mut mask = vec![0u8; ((width * height) / 8) as usize + 8];
    let got = GetDIBits(
        dc,
        info.hbmColor,
        0,
        height as u32,
        color.as_mut_ptr() as *mut c_void,
        &mut bih,
        0,
    );
    if got == 0 {
        DeleteDC(dc);
        ReleaseDC(std::ptr::null_mut(), screen_dc);
        if !info.hbmColor.is_null() {
            DeleteObject(info.hbmColor);
        }
        if !info.hbmMask.is_null() {
            DeleteObject(info.hbmMask);
        }
        return Err("GetDIBits failed".to_string());
    }
    // AND mask (1bpp, bottom-up as stored by GDI).
    let got_mask = GetDIBits(
        dc,
        info.hbmMask,
        0,
        height as u32,
        mask.as_mut_ptr() as *mut c_void,
        &mut bih,
        0,
    );
    if got_mask == 0 {
        for b in mask.iter_mut() {
            *b = 0;
        }
    }
    DeleteDC(dc);
    ReleaseDC(std::ptr::null_mut(), screen_dc);
    if !info.hbmColor.is_null() {
        DeleteObject(info.hbmColor);
    }
    if !info.hbmMask.is_null() {
        DeleteObject(info.hbmMask);
    }

    // BITMAPINFOHEADER with doubled height + XOR bitmap + AND mask = icon image.
    let mut image = Vec::new();
    let mut hdr = BitmapInfoHeader {
        biSize: 40,
        biWidth: width,
        biHeight: height * 2,
        biPlanes: 1,
        biBitCount: 32,
        biCompression: 0,
        biSizeImage: (width * height * 4) as u32,
        ..Default::default()
    };
    unsafe {
        let bytes = std::slice::from_raw_parts_mut(
            &mut hdr as *mut BitmapInfoHeader as *mut u8,
            40,
        );
        image.extend_from_slice(bytes);
    }
    image.extend_from_slice(&color);
    // Mask rows are 4-byte aligned, bottom-up; our buffer is top-down so flip.
    let row_bytes = ((width + 31) / 32 * 4) as usize;
    let mut flipped = vec![0u8; row_bytes * height as usize];
    for y in 0..height as usize {
        let src_row = (height as usize - 1 - y) * row_bytes;
        let dst_row = y * row_bytes;
        if src_row + row_bytes <= mask.len() {
            flipped[dst_row..dst_row + row_bytes]
                .copy_from_slice(&mask[src_row..src_row + row_bytes]);
        }
    }
    image.extend_from_slice(&flipped);

    // ICONDIR + one entry + image
    let mut ico = Vec::new();
    ico.extend_from_slice(&0u16.to_le_bytes());
    ico.extend_from_slice(&1u16.to_le_bytes());
    ico.extend_from_slice(&1u16.to_le_bytes());
    ico.push(if width >= 256 { 0 } else { width as u8 });
    ico.push(if height >= 256 { 0 } else { height as u8 });
    ico.push(0);
    ico.push(0);
    ico.extend_from_slice(&1u16.to_le_bytes());
    ico.extend_from_slice(&32u16.to_le_bytes());
    ico.extend_from_slice(&(image.len() as u32).to_le_bytes());
    ico.extend_from_slice(&22u32.to_le_bytes());
    ico.extend_from_slice(&image);
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(out, ico).map_err(|e| e.to_string())
}

/// Open a file or folder with the shell.
pub fn shell_open(path: &Path) -> Result<(), String> {
    unsafe {
        let p = to_wide(&path.to_string_lossy());
        let op = to_wide("open");
        let r = ShellExecuteW(
            std::ptr::null_mut(),
            op.as_ptr(),
            p.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        );
        if (r as usize) <= 32 {
            return Err(format!("ShellExecute failed ({})", r as usize));
        }
    }
    Ok(())
}

/// Notify Explorer that file associations changed so icons/labels refresh.
pub fn notify_assoc_changed() {
    unsafe {
        SHChangeNotify(SHCNE_ASSOCCHANGED, SHCNF_IDLIST, std::ptr::null(), std::ptr::null());
    }
}

/// Absolute path of the running executable.
pub fn exe_path() -> std::path::PathBuf {
    let mut buf = vec![0u16; 32768];
    unsafe {
        let len = GetModuleFileNameW(std::ptr::null_mut(), buf.as_mut_ptr(), buf.len() as u32);
        let s = from_wide(&buf[..len as usize]);
        std::path::PathBuf::from(s)
    }
}

/// Best effort encoding of a path to UTF-16 for the shell.
pub fn wide(path: &Path) -> Vec<u16> {
    path.as_os_str().encode_wide().chain(std::iter::once(0)).collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn guid_layout_is_stable() {
        use super::*;
        let _ = CLSID_SHELL_LINK;
        assert_eq!(std::mem::size_of::<GUID>(), 16);
    }
}
