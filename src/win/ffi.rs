//! Raw Win32 / COM bindings used by the launcher.
//!
//! Only the handful of functions needed for shortcuts, file associations and
//! registry work are declared, which keeps the binary free of external crates.

#![allow(non_snake_case, non_camel_case_types, dead_code)]

use std::ffi::c_void;

pub type HRESULT = i32;
pub type BOOL = i32;
pub type HANDLE = *mut c_void;
pub type HWND = *mut c_void;
pub type HKEY = *mut c_void;
pub type DWORD = u32;
pub type ULONG = u32;
pub type LPCOLESTR = *const u16;
pub type LPVOID = *mut c_void;

pub const S_OK: HRESULT = 0;
pub const CLSCTX_INPROC_SERVER: DWORD = 1;

pub const COINIT_APARTMENTTHREADED: DWORD = 0x2;

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct GUID {
    pub data1: u32,
    pub data2: u16,
    pub data3: u16,
    pub data4: [u8; 8],
}

#[repr(C)]
pub struct VARIANT {
    pub vt: u16,
    pub wReserved1: u16,
    pub wReserved2: u16,
    pub wReserved3: u16,
    pub data: [usize; 2],
}

pub const VT_EMPTY: u16 = 0;
pub const VT_BSTR: u16 = 8;
pub const VT_I4: u16 = 3;
pub const VT_DISPATCH: u16 = 9;
pub const VT_BOOL: u16 = 11;

#[repr(C)]
pub struct DISPPARAMS {
    pub rgvarg: *mut VARIANT,
    pub rgdispidNamedArgs: *mut i32,
    pub cArgs: u32,
    pub cNamedArgs: u32,
}

#[repr(C)]
pub struct ITypeInfo {
    pub vtbl: *const ITypeInfoVtbl,
}

#[repr(C)]
pub struct ITypeInfoVtbl {
    pub QueryInterface: unsafe extern "system" fn(*mut ITypeInfo, *const GUID, *mut *mut c_void) -> HRESULT,
    pub AddRef: unsafe extern "system" fn(*mut ITypeInfo) -> ULONG,
    pub Release: unsafe extern "system" fn(*mut ITypeInfo) -> ULONG,
}

#[repr(C)]
pub struct IDispatch {
    pub vtbl: *const IDispatchVtbl,
}

#[repr(C)]
pub struct IDispatchVtbl {
    pub QueryInterface: unsafe extern "system" fn(*mut IDispatch, *const GUID, *mut *mut c_void) -> HRESULT,
    pub AddRef: unsafe extern "system" fn(*mut IDispatch) -> ULONG,
    pub Release: unsafe extern "system" fn(*mut IDispatch) -> ULONG,
    pub GetTypeInfoCount: unsafe extern "system" fn(*mut IDispatch, *mut u32) -> HRESULT,
    pub GetTypeInfo: unsafe extern "system" fn(*mut IDispatch, u32, u32, *mut *mut ITypeInfo) -> HRESULT,
    pub GetIDsOfNames: unsafe extern "system" fn(
        *mut IDispatch,
        *const GUID,
        *const LPCOLESTR,
        u32,
        u32,
        *mut i32,
    ) -> HRESULT,
    pub Invoke: unsafe extern "system" fn(
        *mut IDispatch,
        i32,
        *const GUID,
        u32,
        u16,
        *mut DISPPARAMS,
        *mut VARIANT,
        *mut c_void,
        *mut u32,
    ) -> HRESULT,
}

pub const DISPATCH_METHOD: u16 = 0x1;
pub const DISPATCH_PROPERTYPUT: u16 = 0x4;
pub const DISPATCH_PROPERTYGET: u16 = 0x2;

pub const OLE_E_BLANK: HRESULT = -2147213306; // not critical for us

// Registry
pub const HKEY_CURRENT_USER: HKEY = 0x8000_0001u32 as usize as HKEY;
pub const HKEY_LOCAL_MACHINE: HKEY = 0x8000_0002u32 as usize as HKEY;
pub const HKEY_CLASSES_ROOT: HKEY = 0x8000_0000u32 as usize as HKEY;
pub const KEY_READ: DWORD = 0x20019;
pub const KEY_WRITE: DWORD = 0x20006;
pub const KEY_ALL_ACCESS: DWORD = 0xF003F;
pub const REG_SZ: DWORD = 1;
pub const REG_OPTION_NON_VOLATILE: DWORD = 0;
pub const REG_EXPAND_SZ: DWORD = 2;

#[link(name = "ole32")]
extern "system" {
    pub fn CoInitializeEx(pvReserved: *mut c_void, dwCoInit: DWORD) -> HRESULT;
    pub fn CoUninitialize();
    pub fn CoCreateInstance(
        rclsid: *const GUID,
        pUnkOuter: *mut c_void,
        dwClsContext: DWORD,
        riid: *const GUID,
        ppv: *mut *mut c_void,
    ) -> HRESULT;
    pub fn CoTaskMemFree(pv: *mut c_void);
    pub fn CLSIDFromString(lpsz: *const u16, pclsid: *mut GUID) -> HRESULT;
}

#[link(name = "advapi32")]
extern "system" {
    pub fn RegCreateKeyExW(
        hKey: HKEY,
        lpSubKey: *const u16,
        Reserved: DWORD,
        lpClass: *const u16,
        dwOptions: DWORD,
        samDesired: DWORD,
        lpSecurityAttributes: *mut c_void,
        phkResult: *mut HKEY,
        lpdwDisposition: *mut DWORD,
    ) -> i32;
    pub fn RegSetValueExW(
        hKey: HKEY,
        lpValueName: *const u16,
        Reserved: DWORD,
        dwType: DWORD,
        lpData: *const u8,
        cbData: DWORD,
    ) -> i32;
    pub fn RegQueryValueExW(
        hKey: HKEY,
        lpValueName: *const u16,
        lpReserved: *mut DWORD,
        lpType: *mut DWORD,
        lpData: *mut u8,
        lpcbData: *mut DWORD,
    ) -> i32;
    pub fn RegOpenKeyExW(
        hKey: HKEY,
        lpSubKey: *const u16,
        ulOptions: DWORD,
        samDesired: DWORD,
        phkResult: *mut HKEY,
    ) -> i32;
    pub fn RegCloseKey(hKey: HKEY) -> i32;
    pub fn RegDeleteTreeW(hKey: HKEY, lpSubKey: *const u16) -> i32;
    pub fn RegDeleteKeyW(hKey: HKEY, lpSubKey: *const u16) -> i32;
    pub fn RegDeleteValueW(hKey: HKEY, lpValueName: *const u16) -> i32;
}

#[link(name = "kernel32")]
extern "system" {
    pub fn GetConsoleWindow() -> HWND;
    pub fn GetModuleFileNameW(hModule: HANDLE, lpFilename: *mut u16, nSize: DWORD) -> DWORD;
    pub fn FreeConsole() -> BOOL;
    pub fn AttachConsole(dwProcessId: DWORD) -> BOOL;
    pub fn SetConsoleOutputCP(wCodePageID: u32) -> BOOL;
    pub fn SetConsoleCP(wCodePageID: u32) -> BOOL;
    pub fn GetStdHandle(nStdHandle: DWORD) -> HANDLE;
    pub fn SetConsoleTitleW(lpConsoleTitle: *const u16) -> BOOL;
}

#[link(name = "shell32")]
extern "system" {
    pub fn SHChangeNotify(
        wEventId: i32,
        uFlags: DWORD,
        dwItem1: *const c_void,
        dwItem2: *const c_void,
    );
    pub fn SHGetFolderPathW(
        hwnd: HWND,
        csidl: i32,
        hToken: HANDLE,
        dwFlags: DWORD,
        pszPath: *mut u16,
    ) -> HRESULT;
    pub fn ShellExecuteW(
        hwnd: HWND,
        lpOperation: *const u16,
        lpFile: *const u16,
        lpParameters: *const u16,
        lpDirectory: *const u16,
        nShowCmd: i32,
    ) -> HANDLE;
}

#[link(name = "user32")]
extern "system" {
    pub fn MessageBoxW(hwnd: HWND, text: *const u16, caption: *const u16, utype: u32) -> i32;
    pub fn SetProcessDPIAware() -> BOOL;
    pub fn DestroyIcon(hIcon: HANDLE) -> BOOL;
    pub fn LoadImageW(
        hInst: HANDLE,
        name: *const u16,
        typ: u32,
        cx: i32,
        cy: i32,
        fuLoad: u32,
    ) -> HANDLE;
    pub fn PrivateExtractIconsW(
        szFileName: *const u16,
        nIconIndex: i32,
        cxIcon: i32,
        cyIcon: i32,
        phicon: *mut HANDLE,
        piconid: *mut u32,
        nIcons: u32,
        flags: u32,
    ) -> u32;
}

pub const SHCNE_ASSOCCHANGED: i32 = 0x0800_0000;
pub const SHCNF_IDLIST: DWORD = 0x0000;

pub fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

pub fn from_wide(ptr: &[u16]) -> String {
    let end = ptr.iter().position(|&c| c == 0).unwrap_or(ptr.len());
    String::from_utf16_lossy(&ptr[..end])
}
