//! Native Windows UI (user32 + comctl32 + comdlg32, no external crates).
//!
//! Four pages in one tab control:
//!   1. Applications - install APK, run, remove, per-app window size / GPU / mode
//!   2. Android system - install an image from a file, pick the active image
//!   3. Settings - RAM/CPU, shortcuts, ARM translation, QEMU/adb paths
//!   4. Diagnostics - system report + one-click "enable Hypervisor Platform"

#![allow(non_snake_case)]

use super::ffi::*;
use std::ffi::c_void;

type HINSTANCE = HANDLE;
type HICON = HANDLE;
type HFONT = HANDLE;
type HGDIOBJ = HANDLE;
type LRESULT = isize;
type WPARAM = usize;
type LPARAM = isize;

#[repr(C)]
#[derive(Default)]
struct WndClassExW {
    cb_size: u32,
    style: u32,
    lpfn_wnd_proc: Option<unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT>,
    cb_cls_extra: i32,
    cb_wnd_extra: i32,
    h_instance: HINSTANCE,
    h_icon: HICON,
    h_cursor: HANDLE,
    hbr_background: HANDLE,
    lpsz_menu_name: *const u16,
    lpsz_class_name: *const u16,
    h_icon_sm: HICON,
}

#[repr(C)]
#[derive(Default, Clone, Copy)]
struct Msg {
    hwnd: HWND,
    message: u32,
    w_param: WPARAM,
    l_param: LPARAM,
    time: u32,
    pt_x: i32,
    pt_y: i32,
}

#[repr(C)]
#[derive(Default)]
struct InitCommonControlsEx {
    size: u32,
    icc: u32,
}

#[repr(C)]
struct OpenFileNameW {
    l_struct_size: u32,
    hwnd_owner: HWND,
    h_instance: HINSTANCE,
    lpstr_filter: *const u16,
    lpstr_custom_filter: *const u16,
    n_max_cust_filter: u32,
    n_filter_index: u32,
    lpstr_file: *mut u16,
    n_max_file: u32,
    lpstr_file_title: *mut u16,
    n_max_file_title: u32,
    lpstr_initial_dir: *const u16,
    lpstr_title: *const u16,
    flags: u32,
    n_file_offset: u16,
    n_file_extension: u16,
    lpstr_def_ext: *const u16,
    lpstr_cust_data: *const u16,
    lpfn_hook: *const c_void,
    lp_template_name: *const u16,
    pv_reserved: *mut c_void,
    dw_reserved: u32,
    flags_ex: u32,
}

#[link(name = "comctl32")]
extern "system" {
    fn InitCommonControlsEx(iccex: *const InitCommonControlsEx) -> BOOL;
}

#[link(name = "comdlg32")]
extern "system" {
    fn GetOpenFileNameW(ofn: *mut OpenFileNameW) -> BOOL;
}

#[link(name = "gdi32")]
extern "system" {
    fn CreateFontW(
        height: i32,
        width: i32,
        esc: i32,
        orient: i32,
        weight: i32,
        italic: u32,
        underline: u32,
        strike: u32,
        charset: u32,
        out_precision: u32,
        clip_precision: u32,
        quality: u32,
        pitch: u32,
        face: *const u16,
    ) -> HFONT;
    fn DeleteObject(obj: HGDIOBJ) -> BOOL;
}

#[link(name = "user32")]
extern "system" {
    fn RegisterClassExW(class: *const WndClassExW) -> u16;
    fn CreateWindowExW(
        ex_style: u32,
        class: *const u16,
        text: *const u16,
        style: u32,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        parent: HWND,
        menu: HANDLE,
        instance: HINSTANCE,
        param: *mut c_void,
    ) -> HWND;
    fn DefWindowProcW(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT;
    fn GetMessageW(msg: *mut Msg, hwnd: HWND, min: u32, max: u32) -> BOOL;
    fn TranslateMessage(msg: *const Msg) -> BOOL;
    fn DispatchMessageW(msg: *const Msg) -> LRESULT;
    fn PostQuitMessage(code: i32);
    fn DestroyWindow(hwnd: HWND) -> BOOL;
    fn SendMessageW(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT;
    fn GetDlgItem(hwnd: HWND, id: i32) -> HWND;
    fn SetWindowTextW(hwnd: HWND, text: *const u16) -> BOOL;
    fn GetWindowTextW(hwnd: HWND, buf: *mut u16, max: i32) -> i32;
    fn GetWindowTextLengthW(hwnd: HWND) -> i32;
    fn SetWindowPos(hwnd: HWND, after: HWND, x: i32, y: i32, cx: i32, cy: i32, flags: u32) -> BOOL;
    fn ShowWindow(hwnd: HWND, cmd: i32) -> BOOL;
    fn SetTimer(hwnd: HWND, id: usize, ms: u32, func: *const c_void) -> usize;
    fn KillTimer(hwnd: HWND, id: usize) -> BOOL;
    fn MessageBoxW(hwnd: HWND, text: *const u16, caption: *const u16, utype: u32) -> i32;
    fn LoadCursorW(instance: HINSTANCE, name: *const u16) -> HANDLE;
    fn LoadIconW(instance: HINSTANCE, name: *const u16) -> HICON;
    fn GetModuleHandleW(name: *const u16) -> HINSTANCE;
    fn UpdateWindow(hwnd: HWND) -> BOOL;
    fn SetForegroundWindow(hwnd: HWND) -> BOOL;
    fn FindWindowW(class: *const u16, title: *const u16) -> HWND;
    fn IsWindow(hwnd: HWND) -> BOOL;
    fn GetClientRect(hwnd: HWND, rect: *mut RectL) -> BOOL;
    fn SetWindowLongPtrW(hwnd: HWND, index: i32, value: isize) -> isize;
    fn GetWindowLongPtrW(hwnd: HWND, index: i32) -> isize;
}

// ---- messages / styles -----------------------------------------------------
const WM_CREATE: u32 = 0x0001;
const WM_DESTROY: u32 = 0x0002;
const WM_SIZE: u32 = 0x0005;
const WM_COMMAND: u32 = 0x0111;
const WM_TIMER: u32 = 0x0113;
const WM_CLOSE: u32 = 0x0010;
const WM_SETFONT: u32 = 0x0030;

const WS_OVERLAPPEDWINDOW: u32 = 0x00CF_0000;
const WS_CHILD: u32 = 0x4000_0000;
const WS_VISIBLE: u32 = 0x1000_0000;
const WS_TABSTOP: u32 = 0x0001_0000;
const WS_BORDER: u32 = 0x0080_0000;
const WS_VSCROLL: u32 = 0x0020_0000;
const ES_AUTOHSCROLL: u32 = 0x0080;
const ES_MULTILINE: u32 = 0x0004;
const ES_AUTOVSCROLL: u32 = 0x0040;
const ES_READONLY: u32 = 0x0800;
const CBS_DROPDOWNLIST: u32 = 0x0003;
const LBS_NOTIFY: u32 = 0x0001;
const SS_LEFT: u32 = 0x0000;
const BS_AUTOCHECKBOX: u32 = 0x0003;

const BM_SETCHECK: u32 = 0x00F1;
const BM_GETCHECK: u32 = 0x00F0;
const BST_CHECKED: usize = 1;
const CB_ADDSTRING: u32 = 0x0143;
const CB_SETCURSEL: u32 = 0x014E;
const CB_GETCURSEL: u32 = 0x0147;
const LB_ADDSTRING: u32 = 0x0180;
const LB_RESETCONTENT: u32 = 0x0182;
const LB_SETCURSEL: u32 = 0x0186;
const LB_GETCURSEL: u32 = 0x0188;
const LB_SETITEMDATA: u32 = 0x0199;
const LB_GETITEMDATA: u32 = 0x0198;

const TCM_INSERTITEMW: u32 = 0x1300 + 62;
const TCM_GETCURSEL: u32 = 0x1300 + 11;
const TCM_SETCURSEL: u32 = 0x1300 + 12;
const ICC_TAB_CLASSES: u32 = 0x0000_0008;
const ICC_STANDARD_CLASSES: u32 = 0x0000_4000;
const MB_ICONINFORMATION: u32 = 0x0000_0040;
const MB_ICONWARNING: u32 = 0x0000_0030;
const MB_YESNO: u32 = 0x0000_0004;
const IDYES: i32 = 6;
const IDC_ARROW: *const u16 = 32512 as *const u16;
const IDI_APPLICATION: *const u16 = 32512 as *const u16;
const GWLP_USERDATA: i32 = -21;
const SW_SHOW: i32 = 5;
const SW_HIDE: i32 = 0;
const SW_RESTORE: i32 = 9;
const SWP_NOZORDER: u32 = 0x0004;
const OFN_FILEMUSTEXIST: u32 = 0x0000_1000;
const OFN_PATHMUSTEXIST: u32 = 0x0000_0800;
const OFN_NOCHANGEDIR: u32 = 0x0000_0008;

// ---- control ids -----------------------------------------------------------
const ID_TABS: i32 = 100;
const ID_STATUS: i32 = 101;

const ID_APP_LIST: i32 = 110;
const ID_OPEN_APK: i32 = 111;
const ID_RUN_APP: i32 = 112;
const ID_REMOVE_APP: i32 = 113;
const ID_PHONE: i32 = 114;
const ID_WIN_W: i32 = 115;
const ID_WIN_H: i32 = 116;
const ID_DPI: i32 = 117;
const ID_GPU: i32 = 118;
const ID_MODE: i32 = 119;
const ID_SAVE_APP: i32 = 120;
const ID_APP_INFO: i32 = 121;

const ID_IMAGE_LIST: i32 = 130;
const ID_IMAGE_ADD: i32 = 131;
const ID_IMAGE_USE: i32 = 132;
const ID_IMAGE_REMOVE: i32 = 133;
const ID_IMAGE_CATALOG: i32 = 134;
const ID_IMAGE_HINT: i32 = 135;

const ID_MEM: i32 = 140;
const ID_CORES: i32 = 141;
const ID_AUTOSTART: i32 = 142;
const ID_DESKTOP_SC: i32 = 143;
const ID_STARTMENU_SC: i32 = 144;
const ID_ARM: i32 = 145;
const ID_ARM_STATUS: i32 = 146;
const ID_QEMU_PATH: i32 = 147;
const ID_BROWSE_QEMU: i32 = 148;
const ID_ADB_PATH: i32 = 149;
const ID_BROWSE_ADB: i32 = 150;
const ID_SAVE_SETTINGS: i32 = 151;
const ID_OPEN_DATA: i32 = 152;
const ID_ASSOC_ON: i32 = 153;

const ID_DOCTOR_TEXT: i32 = 160;
const ID_DOCTOR_RUN: i32 = 161;
const ID_WHVP_BUTTON: i32 = 162;
const ID_WHVP_HINT: i32 = 163;
const ID_WHVP_STATUS: i32 = 164;

/// Page membership: which controls are visible on which tab.
fn page_controls(page: usize) -> &'static [i32] {
    match page {
        0 => &[
            ID_APP_LIST, ID_OPEN_APK, ID_RUN_APP, ID_REMOVE_APP, ID_PHONE, ID_WIN_W, ID_WIN_H,
            ID_DPI, ID_GPU, ID_MODE, ID_SAVE_APP, ID_APP_INFO,
        ],
        1 => &[ID_IMAGE_LIST, ID_IMAGE_ADD, ID_IMAGE_USE, ID_IMAGE_REMOVE, ID_IMAGE_CATALOG, ID_IMAGE_HINT],
        2 => &[
            ID_MEM, ID_CORES, ID_AUTOSTART, ID_DESKTOP_SC, ID_STARTMENU_SC, ID_ARM, ID_ARM_STATUS,
            ID_QEMU_PATH, ID_BROWSE_QEMU, ID_ADB_PATH, ID_BROWSE_ADB, ID_SAVE_SETTINGS, ID_OPEN_DATA,
            ID_ASSOC_ON,
        ],
        _ => &[ID_DOCTOR_TEXT, ID_DOCTOR_RUN, ID_WHVP_BUTTON, ID_WHVP_HINT, ID_WHVP_STATUS],
    }
}

struct AppState {
    font: HFONT,
    apps: Vec<crate::store::AppEntry>,
    images: Vec<crate::config::ImageEntry>,
    settings: crate::config::Settings,
    current_app: Option<usize>,
    current_page: usize,
}

/// Open the settings window; blocks until it is closed.
pub fn run() -> i32 {
    unsafe {
        CoInitializeEx(std::ptr::null_mut(), COINIT_APARTMENTTHREADED);
        SetProcessDPIAware();
        InitCommonControlsEx(&InitCommonControlsEx {
            size: std::mem::size_of::<InitCommonControlsEx>() as u32,
            icc: ICC_TAB_CLASSES | ICC_STANDARD_CLASSES,
        });

        let instance = GetModuleHandleW(std::ptr::null());
        let class_name = to_wide("AndroidEmuMainWindow");
        let title = to_wide("android-emu — Android для Windows");
        let wc = WndClassExW {
            cb_size: std::mem::size_of::<WndClassExW>() as u32,
            lpfn_wnd_proc: Some(wnd_proc),
            h_instance: instance,
            h_icon: LoadIconW(std::ptr::null_mut(), IDI_APPLICATION),
            h_cursor: LoadCursorW(std::ptr::null_mut(), IDC_ARROW),
            hbr_background: (COLOR_WINDOW_BRUSH) as HANDLE,
            lpsz_class_name: class_name.as_ptr(),
            h_icon_sm: LoadIconW(std::ptr::null_mut(), IDI_APPLICATION),
            ..Default::default()
        };
        if RegisterClassExW(&wc) == 0 {
            return 2;
        }
        let state = Box::into_raw(Box::new(AppState {
            font: std::ptr::null_mut(),
            apps: Vec::new(),
            images: Vec::new(),
            settings: crate::config::Settings::load(),
            current_app: None,
            current_page: 0,
        }));
        let hwnd = CreateWindowExW(
            0,
            class_name.as_ptr(),
            title.as_ptr(),
            WS_OVERLAPPEDWINDOW | WS_VISIBLE,
            140,
            90,
            940,
            640,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            instance,
            state as *mut c_void,
        );
        if hwnd.is_null() {
            return 3;
        }
        ShowWindow(hwnd, SW_SHOW);
        UpdateWindow(hwnd);
        SetForegroundWindow(hwnd);

        let mut msg = Msg::default();
        while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        CoUninitialize();
        msg.w_param as i32
    }
}

const COLOR_WINDOW_BRUSH: i32 = 6; // (COLOR_WINDOW + 1)

unsafe fn state(hwnd: HWND) -> Option<&'static mut AppState> {
    let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut AppState;
    if ptr.is_null() {
        None
    } else {
        Some(&mut *ptr)
    }
}

unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_CREATE => {
            let create = &*(lp as *const CREATESTRUCTW);
            let st = &mut *(create.lp_create_params as *mut AppState);
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, st as *mut AppState as isize);
            build_ui(hwnd, st);
            0
        }
        WM_SIZE => {
            layout(hwnd);
            0
        }
        WM_TIMER => {
            if let Some(st) = state(hwnd) {
                sync_page(hwnd, st);
                refresh_status_bar(hwnd, st);
            }
            0
        }
        WM_COMMAND => {
            let id = (wp & 0xFFFF) as i32;
            let notification = ((wp >> 16) & 0xFFFF) as u32;
            match id {
                ID_APP_LIST if notification == 1 => {
                    if let Some(st) = state(hwnd) {
                        select_app(hwnd, st);
                    }
                    0
                }
                ID_APP_LIST if notification == 2 => {
                    run_selected(hwnd);
                    0
                }
                ID_OPEN_APK => {
                    if let Some(st) = state(hwnd) {
                        install_apk(hwnd, st);
                    }
                    0
                }
                ID_RUN_APP => {
                    run_selected(hwnd);
                    0
                }
                ID_REMOVE_APP => {
                    if let Some(st) = state(hwnd) {
                        remove_app(hwnd, st);
                    }
                    0
                }
                ID_PHONE => {
                    launch_detached(&["phone"]);
                    0
                }
                ID_SAVE_APP => {
                    if let Some(st) = state(hwnd) {
                        save_app_settings(hwnd, st);
                    }
                    0
                }
                ID_IMAGE_ADD => {
                    if let Some(st) = state(hwnd) {
                        add_image(hwnd, st);
                    }
                    0
                }
                ID_IMAGE_USE => {
                    if let Some(st) = state(hwnd) {
                        set_active_image(hwnd, st);
                    }
                    0
                }
                ID_IMAGE_REMOVE => {
                    if let Some(st) = state(hwnd) {
                        remove_image(hwnd, st);
                    }
                    0
                }
                ID_IMAGE_CATALOG => {
                    if let Some(st) = state(hwnd) {
                        open_catalog(hwnd, st);
                    }
                    0
                }
                ID_SAVE_SETTINGS => {
                    if let Some(st) = state(hwnd) {
                        save_settings(hwnd, st);
                    }
                    0
                }
                ID_BROWSE_QEMU => {
                    if let Some(st) = state(hwnd) {
                        if let Some(path) = browse_file(
                            hwnd,
                            "qemu-system-x86_64.exe\0qemu-system-x86_64.exe\0Программы (*.exe)\0*.exe\0\0",
                        ) {
                            st.settings.qemu_path = path.clone();
                            set_text(hwnd, ID_QEMU_PATH, &path);
                        }
                    }
                    0
                }
                ID_BROWSE_ADB => {
                    if let Some(st) = state(hwnd) {
                        if let Some(path) = browse_file(hwnd, "adb.exe\0adb.exe\0Программы (*.exe)\0*.exe\0\0") {
                            st.settings.adb_path = path.clone();
                            set_text(hwnd, ID_ADB_PATH, &path);
                        }
                    }
                    0
                }
                ID_ARM => {
                    if let Some(st) = state(hwnd) {
                        st.settings.arm_translation = get_check(hwnd, ID_ARM);
                        let hint = crate::arm::status_summary(st.settings.arm_translation);
                        set_text(hwnd, ID_ARM_STATUS, &hint);
                        st.settings.save().ok();
                    }
                    0
                }
                ID_DOCTOR_RUN => {
                    if let Some(st) = state(hwnd) {
                        run_doctor(hwnd, st);
                    }
                    0
                }
                ID_WHVP_BUTTON => {
                    enable_hypervisor_platform(hwnd);
                    0
                }
                ID_ASSOC_ON => {
                    let exe = super::shortcut::exe_path();
                    match super::fileassoc::register_apk_association(&exe) {
                        Ok(()) => info_box(
                            hwnd,
                            "Готово: .apk связаны с android-emu.\n\n\
                             Теперь двойной клик по APK устанавливает приложение\n\
                             и создаёт ярлык на рабочем столе.",
                        ),
                        Err(e) => warn_box(hwnd, &format!("Не удалось изменить реестр:\n{e}")),
                    }
                    0
                }
                ID_OPEN_DATA => {
                    let _ = super::shortcut::shell_open(&crate::paths::data_dir());
                    0
                }
                _ => DefWindowProcW(hwnd, msg, wp, lp),
            }
        }
        WM_CLOSE => {
            DestroyWindow(hwnd);
            0
        }
        WM_DESTROY => {
            if let Some(st) = state(hwnd) {
                st.settings.save().ok();
                let raw = st as *mut AppState;
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                KillTimer(hwnd, 1);
                if !st.font.is_null() {
                    DeleteObject(st.font);
                }
                drop(Box::from_raw(raw));
            }
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

#[repr(C)]
struct CREATESTRUCTW {
    lp_create_params: *mut c_void,
    h_instance: HINSTANCE,
    h_menu: HANDLE,
    hwnd_parent: HWND,
    cy: i32,
    cx: i32,
    y: i32,
    x: i32,
    style: i32,
    lpsz_name: *const u16,
    lpsz_class: *const u16,
    dw_ex_style: u32,
}

// ---- widget helpers --------------------------------------------------------

fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn from_wide(buf: &[u16]) -> String {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..end])
}

unsafe fn create(
    parent: HWND,
    class: &str,
    text: &str,
    style: u32,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    id: i32,
) -> HWND {
    let class_w = to_wide(class);
    let text_w = to_wide(text);
    CreateWindowExW(
        0,
        class_w.as_ptr(),
        text_w.as_ptr(),
        style,
        x,
        y,
        w,
        h,
        parent,
        id as usize as HANDLE,
        std::ptr::null_mut(),
        std::ptr::null_mut(),
    )
}

unsafe fn button(parent: HWND, text: &str, x: i32, y: i32, w: i32, h: i32, id: i32) -> HWND {
    create(parent, "BUTTON", text, WS_CHILD | WS_TABSTOP | 0x0000_0000, x, y, w, h, id)
}

unsafe fn label(parent: HWND, text: &str, x: i32, y: i32, w: i32, h: i32, id: i32) -> HWND {
    create(parent, "STATIC", text, WS_CHILD | SS_LEFT, x, y, w, h, id)
}

unsafe fn checkbox(parent: HWND, text: &str, x: i32, y: i32, w: i32, id: i32) -> HWND {
    create(parent, "BUTTON", text, WS_CHILD | WS_TABSTOP | BS_AUTOCHECKBOX, x, y, w, 22, id)
}

unsafe fn edit(parent: HWND, text: &str, x: i32, y: i32, w: i32, id: i32) -> HWND {
    create(
        parent,
        "EDIT",
        text,
        WS_CHILD | WS_TABSTOP | WS_BORDER | ES_AUTOHSCROLL,
        x,
        y,
        w,
        24,
        id,
    )
}

unsafe fn combo(parent: HWND, items: &[&str], x: i32, y: i32, w: i32, id: i32) -> HWND {
    let h = create(
        parent,
        "COMBOBOX",
        "",
        WS_CHILD | WS_TABSTOP | WS_VSCROLL | CBS_DROPDOWNLIST,
        x,
        y,
        w,
        220,
        id,
    );
    for item in items {
        let wtext = to_wide(item);
        SendMessageW(h, CB_ADDSTRING, 0, wtext.as_ptr() as LPARAM);
    }
    SendMessageW(h, CB_SETCURSEL, 0, 0);
    h
}

unsafe fn listbox(parent: HWND, x: i32, y: i32, w: i32, h: i32, id: i32) -> HWND {
    create(
        parent,
        "LISTBOX",
        "",
        WS_CHILD | WS_TABSTOP | WS_BORDER | WS_VSCROLL | LBS_NOTIFY,
        x,
        y,
        w,
        h,
        id,
    )
}

fn set_text(hwnd: HWND, id: i32, text: &str) {
    unsafe {
        let ctl = GetDlgItem(hwnd, id);
        if !ctl.is_null() {
            let w = to_wide(text);
            SetWindowTextW(ctl, w.as_ptr());
        }
    }
}

fn get_text(hwnd: HWND, id: i32) -> String {
    unsafe {
        let ctl = GetDlgItem(hwnd, id);
        if ctl.is_null() {
            return String::new();
        }
        let len = GetWindowTextLengthW(ctl).max(0) as usize;
        let mut buf = vec![0u16; len + 2];
        GetWindowTextW(ctl, buf.as_mut_ptr(), buf.len() as i32);
        from_wide(&buf)
    }
}

fn set_check(hwnd: HWND, id: i32, value: bool) {
    unsafe {
        let ctl = GetDlgItem(hwnd, id);
        if !ctl.is_null() {
            SendMessageW(ctl, BM_SETCHECK, if value { BST_CHECKED } else { 0 }, 0);
        }
    }
}

fn get_check(hwnd: HWND, id: i32) -> bool {
    unsafe {
        let ctl = GetDlgItem(hwnd, id);
        !ctl.is_null() && SendMessageW(ctl, BM_GETCHECK, 0, 0) as usize == BST_CHECKED
    }
}

fn set_combo(hwnd: HWND, id: i32, index: usize) {
    unsafe {
        let ctl = GetDlgItem(hwnd, id);
        if !ctl.is_null() {
            SendMessageW(ctl, CB_SETCURSEL, index, 0);
        }
    }
}

fn get_combo(hwnd: HWND, id: i32) -> usize {
    unsafe {
        let ctl = GetDlgItem(hwnd, id);
        if ctl.is_null() {
            return 0;
        }
        SendMessageW(ctl, CB_GETCURSEL, 0, 0) as usize
    }
}

fn list_selection(hwnd: HWND, id: i32) -> i32 {
    unsafe {
        let ctl = GetDlgItem(hwnd, id);
        if ctl.is_null() {
            return -1;
        }
        SendMessageW(ctl, LB_GETCURSEL, 0, 0) as i32
    }
}

unsafe fn list_add(hwnd: HWND, id: i32, text: &str) {
    let ctl = GetDlgItem(hwnd, id);
    if !ctl.is_null() {
        let w = to_wide(text);
        SendMessageW(ctl, LB_ADDSTRING, 0, w.as_ptr() as LPARAM);
    }
}

// ---- layout ----------------------------------------------------------------

unsafe fn build_ui(hwnd: HWND, st: &mut AppState) {
    let face = to_wide("Segoe UI");
    let font = CreateFontW(-15, 0, 0, 0, 400, 0, 0, 0, 1, 0, 0, 0, 0, face.as_ptr());
    st.font = font;

    // Tabs
    let tabs = create(hwnd, "SysTabControl32", "", WS_CHILD | WS_VISIBLE, 10, 10, 910, 540, ID_TABS);
    let pages = [
        "Приложения",
        "Android система",
        "Настройки",
        "Диагностика",
    ];
    #[repr(C)]
    struct TabItemW {
        mask: u32,
        state: u32,
        state_mask: u32,
        text: *const u16,
        text_len: i32,
        image: i32,
    }
    for (i, title) in pages.iter().enumerate() {
        let title_w = to_wide(title);
        let item = TabItemW {
            mask: 0x0001,
            state: 0,
            state_mask: 0,
            text: title_w.as_ptr(),
            text_len: title_w.len() as i32,
            image: 0,
        };
        SendMessageW(tabs, TCM_INSERTITEMW, i, &item as *const TabItemW as LPARAM);
    }

    // ---- page 0: applications
    listbox(hwnd, 24, 52, 430, 400, ID_APP_LIST);
    button(hwnd, "Установить APK…", 470, 52, 200, 32, ID_OPEN_APK);
    button(hwnd, "Запустить приложение", 470, 90, 200, 32, ID_RUN_APP);
    button(hwnd, "Удалить приложение", 470, 128, 200, 32, ID_REMOVE_APP);
    button(hwnd, "Открыть Android (как телефон)", 470, 166, 200, 32, ID_PHONE);
    label(hwnd, "Размер окна и экрана приложения", 470, 210, 300, 20, ID_APP_INFO);
    label(hwnd, "Ширина", 470, 236, 60, 20, ID_APP_INFO);
    edit(hwnd, "720", 530, 232, 70, ID_WIN_W);
    label(hwnd, "Высота", 610, 236, 55, 20, ID_APP_INFO);
    edit(hwnd, "1280", 670, 232, 70, ID_WIN_H);
    label(hwnd, "DPI", 470, 268, 60, 20, ID_APP_INFO);
    edit(hwnd, "240", 530, 264, 70, ID_DPI);
    label(hwnd, "GPU", 610, 268, 55, 20, ID_APP_INFO);
    combo(hwnd, &["swiftshader", "angle", "guest", "off"], 670, 264, 110, ID_GPU);
    label(hwnd, "Режим", 470, 300, 60, 20, ID_APP_INFO);
    combo(hwnd, &["window", "fullscreen", "headless"], 530, 296, 130, ID_MODE);
    button(hwnd, "Сохранить настройки приложения", 470, 334, 310, 32, ID_SAVE_APP);
    label(
        hwnd,
        "Выберите приложение слева, чтобы изменить его окно.\n\
         Двойной клик по приложению — запуск.\n\
         APK ставится как обычная программа: со ярлыком\n\
         на рабочем столе и в меню «Пуск».",
        470,
        380,
        360,
        90,
        ID_APP_INFO,
    );

    // ---- page 1: images
    listbox(hwnd, 24, 52, 700, 240, ID_IMAGE_LIST);
    button(hwnd, "Добавить образ с диска…", 24, 306, 220, 32, ID_IMAGE_ADD);
    button(hwnd, "Сделать активным", 254, 306, 180, 32, ID_IMAGE_USE);
    button(hwnd, "Удалить", 444, 306, 120, 32, ID_IMAGE_REMOVE);
    button(hwnd, "Скачать из каталога", 574, 306, 190, 32, ID_IMAGE_CATALOG);
    label(
        hwnd,
        "Поддерживаются: Android-x86, BlissOS, PrimeOS (ISO) и образы дисков\n\
         .img / .qcow2 с уже установленным Android (изменения сохраняются).\n\n\
         Для ARM-приложений выберите образ с libhoudini или libndk и включите\n\
         «Поддержку ARM» на вкладке «Настройки».",
        24,
        350,
        740,
        120,
        ID_IMAGE_HINT,
    );

    // ---- page 2: settings
    label(hwnd, "RAM, МБ", 24, 56, 90, 20, 0);
    edit(hwnd, "2048", 120, 52, 90, ID_MEM);
    label(hwnd, "Ядра CPU", 230, 56, 80, 20, 0);
    edit(hwnd, "2", 315, 52, 60, ID_CORES);
    checkbox(hwnd, "Запускать Android сразу после установки APK", 24, 92, 480, ID_AUTOSTART);
    checkbox(hwnd, "Создавать ярлык на рабочем столе", 24, 118, 480, ID_DESKTOP_SC);
    checkbox(hwnd, "Создавать ярлык в меню «Пуск»", 24, 144, 480, ID_STARTMENU_SC);
    checkbox(
        hwnd,
        "Поддержка ARM-приложений (libhoudini / libndk в госте)",
        24,
        176,
        700,
        ID_ARM,
    );
    label(hwnd, "", 24, 200, 740, 44, ID_ARM_STATUS);
    label(hwnd, "Путь к QEMU", 24, 252, 110, 20, 0);
    edit(hwnd, "", 140, 248, 500, ID_QEMU_PATH);
    button(hwnd, "Обзор…", 650, 246, 90, 28, ID_BROWSE_QEMU);
    label(hwnd, "Путь к adb", 24, 288, 110, 20, 0);
    edit(hwnd, "", 140, 284, 500, ID_ADB_PATH);
    button(hwnd, "Обзор…", 650, 282, 90, 28, ID_BROWSE_ADB);
    button(hwnd, "Сохранить настройки", 24, 330, 220, 32, ID_SAVE_SETTINGS);
    button(hwnd, "Папка данных", 254, 330, 170, 32, ID_OPEN_DATA);
    button(hwnd, "Связать .apk с эмулятором", 434, 330, 280, 32, ID_ASSOC_ON);

    // ---- page 3: diagnostics
    create(
        hwnd,
        "EDIT",
        "",
        WS_CHILD | WS_BORDER | WS_VSCROLL | ES_MULTILINE | ES_AUTOVSCROLL | ES_READONLY,
        24,
        52,
        780,
        300,
        ID_DOCTOR_TEXT,
    );
    button(hwnd, "Проверить систему", 24, 366, 200, 32, ID_DOCTOR_RUN);
    button(
        hwnd,
        "Включить Windows Hypervisor Platform",
        234,
        366,
        330,
        32,
        ID_WHVP_BUTTON,
    );
    label(
        hwnd,
        "Hypervisor Platform (WHPX) — аппаратное ускорение: Android грузится\n\
         в 10–20 раз быстрее. Нужны права администратора и перезагрузка.\n\
         Кнопка запускает системный скрипт с повышением прав.",
        24,
        410,
        760,
        70,
        ID_WHVP_HINT,
    );
    label(hwnd, "", 24, 480, 780, 40, ID_WHVP_STATUS);

    // Status bar text (always visible, outside the tabs)
    label(hwnd, "", 14, 556, 900, 40, ID_STATUS);

    // Font for every child control.
    let mut child = GetWindowChild(hwnd);
    while !child.is_null() {
        SendMessageW(child, WM_SETFONT, font as WPARAM, 1);
        child = GetWindowNext(child);
    }

    // Initial values.
    set_check(hwnd, ID_AUTOSTART, st.settings.autostart_vm);
    set_check(hwnd, ID_DESKTOP_SC, st.settings.create_desktop_shortcut);
    set_check(hwnd, ID_STARTMENU_SC, st.settings.create_start_menu_shortcut);
    set_check(hwnd, ID_ARM, st.settings.arm_translation);
    set_text(hwnd, ID_MEM, &st.settings.memory_mb.to_string());
    set_text(hwnd, ID_CORES, &st.settings.cores.to_string());
    set_text(hwnd, ID_QEMU_PATH, &st.settings.qemu_path);
    set_text(hwnd, ID_ADB_PATH, &st.settings.adb_path);
    set_text(
        hwnd,
        ID_ARM_STATUS,
        &crate::arm::status_summary(st.settings.arm_translation),
    );
    set_text(
        hwnd,
        ID_WHVP_STATUS,
        if super::is_whpx_enabled() {
            "Статус: Hypervisor Platform включена — ускорение активно."
        } else {
            "Статус: Hypervisor Platform выключена — сейчас работает медленная программная эмуляция."
        },
    );

    refresh_apps(hwnd, st);
    refresh_images(hwnd, st);
    run_doctor(hwnd, st);
    show_page(hwnd, 0);
    SetTimer(hwnd, 1, 1200, std::ptr::null());
}

#[link(name = "user32")]
extern "system" {
    fn GetWindow(hwnd: HWND, cmd: u32) -> HWND;
}

unsafe fn GetWindowChild(hwnd: HWND) -> HWND {
    GetWindow(hwnd, 5) // GW_CHILD
}

unsafe fn GetWindowNext(hwnd: HWND) -> HWND {
    GetWindow(hwnd, 2) // GW_HWNDNEXT
}

unsafe fn layout(hwnd: HWND) {
    let mut rect = RectL::default();
    GetClientRect(hwnd, &mut rect);
    let tabs = GetDlgItem(hwnd, ID_TABS);
    if !tabs.is_null() {
        SetWindowPos(
            tabs,
            std::ptr::null_mut(),
            10,
            10,
            rect.right - 20,
            rect.bottom - 70,
            SWP_NOZORDER,
        );
    }
    let status = GetDlgItem(hwnd, ID_STATUS);
    if !status.is_null() {
        SetWindowPos(
            status,
            std::ptr::null_mut(),
            14,
            rect.bottom - 46,
            rect.right - 28,
            36,
            SWP_NOZORDER,
        );
    }
}

unsafe fn show_page(hwnd: HWND, page: usize) {
    for p in 0..4 {
        let visible = p == page;
        for id in page_controls(p) {
            let ctl = GetDlgItem(hwnd, *id);
            if !ctl.is_null() {
                ShowWindow(ctl, if visible { SW_SHOW } else { SW_HIDE });
            }
        }
    }
}

unsafe fn sync_page(hwnd: HWND, st: &mut AppState) {
    let tabs = GetDlgItem(hwnd, ID_TABS);
    if tabs.is_null() {
        return;
    }
    let current = SendMessageW(tabs, TCM_GETCURSEL, 0, 0) as i32;
    if current >= 0 && current as usize != st.current_page {
        st.current_page = current as usize;
        show_page(hwnd, st.current_page);
    }
}

// ---- data refresh ----------------------------------------------------------

unsafe fn refresh_apps(hwnd: HWND, st: &mut AppState) {
    st.apps = crate::store::list();
    let list = GetDlgItem(hwnd, ID_APP_LIST);
    if list.is_null() {
        return;
    }
    SendMessageW(list, LB_RESETCONTENT, 0, 0);
    for app in &st.apps {
        let label = format!("{}  —  {}", app.label, app.package);
        list_add(hwnd, ID_APP_LIST, &label);
    }
    if st.apps.is_empty() {
        list_add(
            hwnd,
            ID_APP_LIST,
            "Пока нет приложений — нажмите «Установить APK…»",
        );
    }
}

unsafe fn refresh_images(hwnd: HWND, st: &mut AppState) {
    let registry = crate::config::ImageRegistry::load();
    st.images = registry.images.clone();
    let list = GetDlgItem(hwnd, ID_IMAGE_LIST);
    if list.is_null() {
        return;
    }
    SendMessageW(list, LB_RESETCONTENT, 0, 0);
    if st.images.is_empty() {
        list_add(hwnd, ID_IMAGE_LIST, "Нет образов — нажмите «Добавить образ с диска…»");
    }
    for img in &st.images {
        let active = registry.active.as_deref() == Some(img.name.as_str());
        let text = format!(
            "{}   [{}]   {}{}",
            img.name,
            img.kind.as_str(),
            crate::util::human_bytes(img.size_bytes),
            if active { "   ✔ активный" } else { "" }
        );
        list_add(hwnd, ID_IMAGE_LIST, &text);
    }
}

unsafe fn refresh_status_bar(hwnd: HWND, st: &AppState) {
    let registry = crate::config::ImageRegistry::load();
    let qemu = crate::qemu::detect();
    let image = registry.active_image().map(|i| i.name.clone());
    let accel = crate::qemu::detect_accel().label();
    let status = match (qemu, image) {
        (Some(q), Some(img)) => format!(
            "QEMU: {}   •   Образ: {}   •   Ускорение: {}   •   Приложений: {}",
            q.version,
            img,
            accel,
            st.apps.len()
        ),
        (Some(q), None) => format!(
            "QEMU: {}   •   образ Android не выбран (вкладка «Android система»)",
            q.version
        ),
        (None, _) => "QEMU не найден — укажите qemu-system-x86_64.exe на вкладке «Настройки»"
            .to_string(),
    };
    set_text(hwnd, ID_STATUS, &status);
}

// ---- actions ---------------------------------------------------------------

unsafe fn select_app(hwnd: HWND, st: &mut AppState) {
    let sel = list_selection(hwnd, ID_APP_LIST);
    if sel < 0 || sel as usize >= st.apps.len() {
        return;
    }
    st.current_app = Some(sel as usize);
    let app = st.apps[sel as usize].clone();
    set_text(hwnd, ID_WIN_W, &app.window_width.to_string());
    set_text(hwnd, ID_WIN_H, &app.window_height.to_string());
    set_text(hwnd, ID_DPI, &app.dpi.to_string());
    let gpu_index = match app.gpu {
        crate::store::GpuMode::Swiftshader => 0,
        crate::store::GpuMode::Angle => 1,
        crate::store::GpuMode::Guest => 2,
        crate::store::GpuMode::Off => 3,
    };
    set_combo(hwnd, ID_GPU, gpu_index);
    let mode_index = match app.display_mode {
        crate::store::DisplayMode::Window => 0,
        crate::store::DisplayMode::Fullscreen => 1,
        crate::store::DisplayMode::Headless => 2,
    };
    set_combo(hwnd, ID_MODE, mode_index);
}

unsafe fn save_app_settings(hwnd: HWND, st: &mut AppState) {
    let Some(index) = st.current_app else {
        info_box(hwnd, "Сначала выберите приложение в списке.");
        return;
    };
    let Some(app) = st.apps.get_mut(index) else {
        return;
    };
    if let Ok(w) = get_text(hwnd, ID_WIN_W).trim().parse::<u32>() {
        app.window_width = w.clamp(200, 4096);
    }
    if let Ok(h) = get_text(hwnd, ID_WIN_H).trim().parse::<u32>() {
        app.window_height = h.clamp(200, 4096);
    }
    if let Ok(dpi) = get_text(hwnd, ID_DPI).trim().parse::<u32>() {
        app.dpi = dpi.clamp(80, 640);
    }
    app.gpu = match get_combo(hwnd, ID_GPU) {
        1 => crate::store::GpuMode::Angle,
        2 => crate::store::GpuMode::Guest,
        3 => crate::store::GpuMode::Off,
        _ => crate::store::GpuMode::Swiftshader,
    };
    app.display_mode = match get_combo(hwnd, ID_MODE) {
        1 => crate::store::DisplayMode::Fullscreen,
        2 => crate::store::DisplayMode::Headless,
        _ => crate::store::DisplayMode::Window,
    };
    app.resolution = format!("{}x{}", app.window_width, app.window_height);
    let slug = app.slug.clone();
    let snapshot = app.clone();
    if let Err(e) = crate::store::save_entry_config(&snapshot) {
        warn_box(hwnd, &format!("Не удалось сохранить: {e}"));
        return;
    }
    let mut index_json = crate::store::load_index();
    if let Some(pos) = index_json.apps.iter().position(|a| a.slug == slug) {
        index_json.apps[pos] = snapshot;
    }
    let _ = crate::store::save_index(&index_json);
    info_box(hwnd, "Настройки приложения сохранены.");
}

unsafe fn install_apk(hwnd: HWND, st: &mut AppState) {
    let Some(path) = browse_file(hwnd, "Приложения Android (*.apk)\0*.apk\0Все файлы\0*.*\0\0")
    else {
        return;
    };
    set_text(hwnd, ID_STATUS, "Установка APK…");
    let request = crate::installer::InstallRequest {
        apk: std::path::PathBuf::from(&path),
        options: crate::store::InstallOptions::default(),
        desktop_shortcut: st.settings.create_desktop_shortcut,
        start_menu_shortcut: st.settings.create_start_menu_shortcut,
    };
    match crate::installer::install_to_disk(&request) {
        Ok(installed) => {
            refresh_apps(hwnd, st);
            let message = crate::installer::summarize(&installed);
            if st.settings.autostart_vm {
                launch_detached(&["--run", &installed.entry.slug]);
            }
            info_box(hwnd, &message);
        }
        Err(e) => warn_box(hwnd, &format!("Не удалось установить APK:\n\n{e}")),
    }
}

unsafe fn run_selected(hwnd: HWND) {
    let sel = list_selection(hwnd, ID_APP_LIST);
    let Some(st) = state(hwnd) else {
        return;
    };
    if sel < 0 || sel as usize >= st.apps.len() {
        warn_box(hwnd, "Выберите приложение в списке.");
        return;
    }
    let slug = st.apps[sel as usize].slug.clone();
    launch_detached(&["--run", &slug]);
}

unsafe fn remove_app(hwnd: HWND, st: &mut AppState) {
    let sel = list_selection(hwnd, ID_APP_LIST);
    if sel < 0 || sel as usize >= st.apps.len() {
        return;
    }
    let app = st.apps[sel as usize].clone();
    let question = format!(
        "Удалить «{}» ({})?\n\nЯрлык на рабочем столе тоже будет удалён.",
        app.label, app.package
    );
    if !confirm(hwnd, &question) {
        return;
    }
    let _ = crate::store::uninstall(&app.slug);
    if let Some(desktop) = crate::paths::desktop_dir() {
        let _ = std::fs::remove_file(desktop.join(format!(
            "{}.lnk",
            crate::util::sanitize_file_name(&app.label)
        )));
    }
    st.current_app = None;
    refresh_apps(hwnd, st);
}

unsafe fn add_image(hwnd: HWND, st: &mut AppState) {
    let Some(path) = browse_file(
        hwnd,
        "Образ Android (*.iso;*.img;*.qcow2;*.raw)\0*.iso;*.img;*.qcow2;*.raw\0Все файлы\0*.*\0\0",
    ) else {
        return;
    };
    set_text(hwnd, ID_STATUS, "Копирование образа в папку данных…");
    let mut registry = crate::config::ImageRegistry::load();
    match registry.add(std::path::Path::new(&path), None) {
        Ok(entry) => {
            let mut text = format!(
                "Образ добавлен и готов к запуску:\n\n{}\nТип: {}\nРазмер: {}\nПуть: {}",
                entry.name,
                entry.kind.as_str(),
                crate::util::human_bytes(entry.size_bytes),
                entry.path.display()
            );
            if entry.kind == crate::config::ImageKind::Iso {
                text.push_str(
                    "\n\nЭто ISO: изменения не сохраняются между запусками.\n\
                     Для постоянной системы используйте .img/.qcow2 образ.",
                );
            }
            if registry.active.is_none() || confirm(hwnd, "Сделать этот образ активным?") {
                let _ = registry.set_active(&entry.name);
            }
            refresh_images(hwnd, st);
            info_box(hwnd, &text);
        }
        Err(e) => warn_box(hwnd, &e),
    }
}

unsafe fn set_active_image(hwnd: HWND, st: &mut AppState) {
    let sel = list_selection(hwnd, ID_IMAGE_LIST);
    if sel < 0 || sel as usize >= st.images.len() {
        warn_box(hwnd, "Выберите образ в списке.");
        return;
    }
    let name = st.images[sel as usize].name.clone();
    let mut registry = crate::config::ImageRegistry::load();
    match registry.set_active(&name) {
        Ok(()) => {
            refresh_images(hwnd, st);
            info_box(hwnd, &format!("Активный образ: {name}"));
        }
        Err(e) => warn_box(hwnd, &e),
    }
}

unsafe fn remove_image(hwnd: HWND, st: &mut AppState) {
    let sel = list_selection(hwnd, ID_IMAGE_LIST);
    if sel < 0 || sel as usize >= st.images.len() {
        return;
    }
    let name = st.images[sel as usize].name.clone();
    if !confirm(hwnd, &format!("Удалить образ «{name}»?\n\nФайл будет удалён с диска.")) {
        return;
    }
    let mut registry = crate::config::ImageRegistry::load();
    if let Err(e) = registry.remove(&name) {
        warn_box(hwnd, &e);
    }
    refresh_images(hwnd, st);
}

unsafe fn open_catalog(hwnd: HWND, _st: &mut AppState) {
    set_text(hwnd, ID_STATUS, "Загрузка каталога образов…");
    let url = std::env::var("ANDROID_EMU_CATALOG")
        .unwrap_or_else(|_| crate::remote::DEFAULT_CATALOG_URL.to_string());
    match crate::remote::fetch_catalog(&url) {
        Ok(images) if !images.is_empty() => {
            let mut text = String::from("Доступные образы:\n\n");
            for img in &images {
                text.push_str(&format!(
                    "• {}  ({}, {})\n   {}\n\n",
                    img.name,
                    img.kind,
                    crate::util::human_bytes(img.size_bytes),
                    img.description
                ));
            }
            text.push_str(
                "Скачивание: меню командной строки\n\
                 android-emu image get \"<имя>\"\n\n\
                 или скачайте файл в браузере и нажмите «Добавить образ с диска…»",
            );
            set_text(hwnd, ID_STATUS, &format!("В каталоге {} образ(ов).", images.len()));
            info_box(hwnd, &text);
        }
        Ok(_) => warn_box(hwnd, "Каталог пуст."),
        Err(e) => warn_box(
            hwnd,
            &format!(
                "Не удалось получить каталог:\n{e}\n\n\
                 Скачайте образ вручную и нажмите «Добавить образ с диска…»"
            ),
        ),
    }
}

unsafe fn save_settings(hwnd: HWND, st: &mut AppState) {
    if let Ok(mem) = get_text(hwnd, ID_MEM).trim().parse::<u32>() {
        st.settings.memory_mb = mem.clamp(512, 32768);
    }
    if let Ok(cores) = get_text(hwnd, ID_CORES).trim().parse::<u32>() {
        st.settings.cores = cores.clamp(1, 32);
    }
    st.settings.autostart_vm = get_check(hwnd, ID_AUTOSTART);
    st.settings.create_desktop_shortcut = get_check(hwnd, ID_DESKTOP_SC);
    st.settings.create_start_menu_shortcut = get_check(hwnd, ID_STARTMENU_SC);
    st.settings.arm_translation = get_check(hwnd, ID_ARM);
    st.settings.qemu_path = get_text(hwnd, ID_QEMU_PATH);
    st.settings.adb_path = get_text(hwnd, ID_ADB_PATH);
    match st.settings.save() {
        Ok(()) => info_box(
            hwnd,
            "Настройки сохранены.\n\n\
             Настройки ARM применяются при следующем запуске Android:\n\
             при включении эмулятор предложит установить libhoudini/libndk в гостя.",
        ),
        Err(e) => warn_box(hwnd, &format!("Не удалось сохранить: {e}")),
    }
}

unsafe fn run_doctor(hwnd: HWND, st: &mut AppState) {
    let mut lines = Vec::new();
    lines.push(format!("android-emu {}", crate::cli::VERSION));
    lines.push(format!("Папка данных: {}", crate::paths::data_dir().display()));
    match crate::qemu::detect() {
        Some(q) => {
            lines.push(format!("QEMU: {}", q.version));
            lines.push(format!("      {}", q.exe.display()));
        }
        None => lines.push("QEMU: НЕ НАЙДЕН".to_string()),
    }
    lines.push(format!("Ускорение: {}", crate::qemu::detect_accel().label()));
    lines.push(format!(
        "Windows Hypervisor Platform: {}",
        if super::is_whpx_enabled() { "включена" } else { "выключена" }
    ));
    match crate::qemu::find_adb() {
        Some(p) => lines.push(format!("adb: {}", p.display())),
        None => lines.push("adb: НЕ НАЙДЕН".to_string()),
    }
    let registry = crate::config::ImageRegistry::load();
    match registry.active_image() {
        Some(img) => lines.push(format!(
            "Образ Android: {} ({}, {})",
            img.name,
            img.kind.as_str(),
            crate::util::human_bytes(img.size_bytes)
        )),
        None => lines.push("Образ Android: не выбран".to_string()),
    }
    lines.push(format!("Приложений установлено: {}", st.apps.len()));
    lines.push(format!(
        "ARM-трансляция: {}",
        if st.settings.arm_translation {
            "включена"
        } else {
            "выключена"
        }
    ));
    if let Some(step) = crate::arm::guest_support_hint() {
        lines.push(step);
    }
    for (key, value) in super::system::summary() {
        lines.push(format!("{key}: {value}"));
    }
    set_text(hwnd, ID_DOCTOR_TEXT, &lines.join("\r\n"));
    set_text(
        hwnd,
        ID_WHVP_STATUS,
        if super::is_whpx_enabled() {
            "Статус: Hypervisor Platform включена — ускорение активно."
        } else {
            "Статус: Hypervisor Platform выключена — работает программная эмуляция."
        },
    );
}

unsafe fn enable_hypervisor_platform(hwnd: HWND) {
    if super::is_whpx_enabled() && !confirm(hwnd, "Hypervisor Platform уже включена.\n\nЗапустить установку ещё раз?") {
        return;
    }
    if !confirm(
        hwnd,
        "Включить Windows Hypervisor Platform?\n\n\
         • появится запрос прав администратора (UAC)\n\
         • после установки нужна перезагрузка Windows\n\
         • Android будет загружаться в 10–20 раз быстрее",
    ) {
        return;
    }
    match super::elevate::enable_hypervisor_platform() {
        Ok(path) => info_box(
            hwnd,
            &format!(
                "Запущен системный скрипт с правами администратора.\n\n\
                 Дождитесь сообщения об успехе и перезагрузите компьютер.\n\
                 Файл: {}",
                path.display()
            ),
        ),
        Err(e) => warn_box(hwnd, &format!("Не удалось запустить скрипт: {e}")),
    }
}

fn launch_detached(args: &[&str]) {
    let exe = super::shortcut::exe_path();
    let _ = std::process::Command::new(exe).args(args).spawn();
}

unsafe fn browse_file(hwnd: HWND, filter: &str) -> Option<String> {
    let mut buffer = vec![0u16; 1024];
    let filter_units: Vec<u16> = filter
        .chars()
        .map(|c| if c == '\0' { 0 } else { c as u16 })
        .collect();
    let initial = to_wide(&crate::paths::data_dir().to_string_lossy());
    let title = to_wide("Выберите файл");
    let mut ofn = OpenFileNameW {
        l_struct_size: std::mem::size_of::<OpenFileNameW>() as u32,
        hwnd_owner: hwnd,
        h_instance: std::ptr::null_mut(),
        lpstr_filter: filter_units.as_ptr(),
        lpstr_custom_filter: std::ptr::null(),
        n_max_cust_filter: 0,
        n_filter_index: 1,
        lpstr_file: buffer.as_mut_ptr(),
        n_max_file: 1024,
        lpstr_file_title: std::ptr::null_mut(),
        n_max_file_title: 0,
        lpstr_initial_dir: initial.as_ptr(),
        lpstr_title: title.as_ptr(),
        flags: OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST | OFN_NOCHANGEDIR,
        n_file_offset: 0,
        n_file_extension: 0,
        lpstr_def_ext: std::ptr::null(),
        lpstr_cust_data: std::ptr::null(),
        lpfn_hook: std::ptr::null(),
        lp_template_name: std::ptr::null(),
        pv_reserved: std::ptr::null_mut(),
        dw_reserved: 0,
        flags_ex: 0,
    };
    if GetOpenFileNameW(&mut ofn) == 0 {
        return None;
    }
    Some(from_wide(&buffer))
}

fn info_box(hwnd: HWND, text: &str) {
    unsafe {
        let t = to_wide(text);
        let c = to_wide("android-emu");
        MessageBoxW(hwnd, t.as_ptr(), c.as_ptr(), MB_ICONINFORMATION);
    }
}

fn warn_box(hwnd: HWND, text: &str) {
    unsafe {
        let t = to_wide(text);
        let c = to_wide("android-emu");
        MessageBoxW(hwnd, t.as_ptr(), c.as_ptr(), MB_ICONWARNING);
    }
}

fn confirm(hwnd: HWND, text: &str) -> bool {
    unsafe {
        let t = to_wide(text);
        let c = to_wide("android-emu");
        MessageBoxW(hwnd, t.as_ptr(), c.as_ptr(), MB_YESNO | MB_ICONWARNING) == IDYES
    }
}

/// Bring the already running window to the front.
pub fn focus_existing() -> bool {
    unsafe {
        let class = to_wide("AndroidEmuMainWindow");
        let hwnd = FindWindowW(class.as_ptr(), std::ptr::null());
        if hwnd.is_null() || IsWindow(hwnd) == 0 {
            return false;
        }
        ShowWindow(hwnd, SW_RESTORE);
        SetForegroundWindow(hwnd);
        true
    }
}
