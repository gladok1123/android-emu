//! Turning an APK into a Windows "program": install, shortcut, launch.

use crate::qemu::{self, AdbClient, QemuError, QemuInstall, QemuSession};
use crate::store::{self, AppEntry, InstallOptions};
use std::fmt;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub enum InstallerError {
    NotAnApk(String),
    Store(String),
    Shortcut(String),
    Qemu(String),
    Adb(String),
    NoImage,
}

impl fmt::Display for InstallerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InstallerError::NotAnApk(e) => write!(f, "{e}"),
            InstallerError::Store(e) => write!(f, "{e}"),
            InstallerError::Shortcut(e) => write!(f, "shortcut creation failed: {e}"),
            InstallerError::Qemu(e) => write!(f, "{e}"),
            InstallerError::Adb(e) => write!(f, "{e}"),
            InstallerError::NoImage => write!(
                f,
                "no Android system image yet - run `android-emu image add <file.iso>` first"
            ),
        }
    }
}

impl std::error::Error for InstallerError {}

/// Result of installing an APK on disk (not yet pushed into the VM).
pub struct Installed {
    pub entry: AppEntry,
    pub replaced: bool,
    pub shortcut_paths: Vec<PathBuf>,
}

pub struct InstallRequest {
    pub apk: PathBuf,
    pub options: InstallOptions,
    pub desktop_shortcut: bool,
    pub start_menu_shortcut: bool,
}

/// Copy the APK into the app store and create the Windows shortcuts.
pub fn install_to_disk(req: &InstallRequest) -> Result<Installed, InstallerError> {
    if !req.apk.is_file() {
        return Err(InstallerError::NotAnApk(format!(
            "{} does not exist",
            req.apk.display()
        )));
    }
    let outcome = store::install_apk(&req.apk, req.options.clone())
        .map_err(|e| match e {
            store::AppError::Apk(m) => InstallerError::NotAnApk(m),
            other => InstallerError::Store(other.to_string()),
        })?;
    let entry = outcome.entry;

    let mut shortcuts = Vec::new();
    let mut shortcut_error: Option<String> = None;
    if req.desktop_shortcut {
        if let Some(dir) = crate::paths::desktop_dir() {
            match create_app_shortcut(&entry, &dir) {
                Ok(p) => shortcuts.push(p),
                Err(e) => shortcut_error = Some(e),
            }
        }
    }
    if req.start_menu_shortcut {
        if let Some(dir) = crate::paths::start_menu_dir() {
            match create_app_shortcut(&entry, &dir) {
                Ok(p) => shortcuts.push(p),
                // A missing Start Menu permission must not fail the install.
                Err(e) => {
                    if shortcut_error.is_none() {
                        shortcut_error = Some(e);
                    }
                }
            }
        }
    }

    // On Windows a shortcut failure is reported but never loses the install;
    // elsewhere there simply are no shortcuts.
    if cfg!(windows) {
        if let Some(e) = shortcut_error {
            crate::win::console::message_box(
                &format!("Installed, but the shortcut could not be created:\n{e}"),
                "android-emu",
                false,
            );
        }
    }
    Ok(Installed {
        entry,
        replaced: outcome.replaced,
        shortcut_paths: shortcuts,
    })
}

/// Create `<dir>/<Label>.lnk` that runs `android-emu --run <slug>`.
pub fn create_app_shortcut(entry: &AppEntry, dir: &Path) -> Result<PathBuf, String> {
    #[cfg(windows)]
    {
        let exe = crate::win::exe_path();
        let name = crate::util::sanitize_file_name(entry.display_name());
        let lnk = dir.join(format!("{name}.lnk"));
        let args = format!("--run \"{}\"", entry.slug);
        let icon = entry.icon_ico_path();
        let working = exe.parent().map(|p| p.to_path_buf()).unwrap_or_default();
        let description = format!("{} ({}) - Android app", entry.label, entry.package);
        let spec = crate::win::shortcut::ShortcutSpec {
            target: &exe,
            arguments: &args,
            working_dir: &working,
            icon_file: if icon.is_file() { Some(&icon) } else { None },
            description: &description,
        };
        crate::win::shortcut::create_shortcut(&lnk, &spec)?;
        Ok(lnk)
    }
    #[cfg(not(windows))]
    {
        let _ = (entry, dir);
        Err("shortcuts are only supported on Windows".to_string())
    }
}

/// Push the APK into the running VM and start its launcher activity.
pub fn deploy_and_launch(
    entry: &AppEntry,
    session: &QemuSession,
) -> Result<String, InstallerError> {
    let adb = AdbClient::detect(session.adb_port).map_err(|e| InstallerError::Adb(e.to_string()))?;
    adb.connect().map_err(|e| InstallerError::Adb(e.to_string()))?;
    let apk = entry.apk_path();
    adb.install(&apk).map_err(|e| InstallerError::Adb(e.to_string()))?;
    let component = entry
        .launch_component
        .clone()
        .ok_or_else(|| InstallerError::NotAnApk("APK has no launcher activity".into()))?;
    let output = adb
        .start_activity(&component)
        .map_err(|e| InstallerError::Adb(e.to_string()))?;
    Ok(output)
}

/// Launch the shared VM for an app (used by the shortcut) and start the app
/// inside it. Reuses an already running VM so opening a second app is instant.
pub fn launch_app(entry: &AppEntry, wait_for_boot: bool) -> Result<(), InstallerError> {
    let state = crate::coordinator::ensure_session(entry).map_err(InstallerError::Qemu)?;
    let port = match &state {
        crate::coordinator::SessionState::Attached(info) => info.adb_port,
        crate::coordinator::SessionState::Started(session) => session.adb_port,
    };

    if wait_for_boot {
        if let Ok(adb) = AdbClient::detect(port) {
            let _ = adb.connect();
            match entry.launch_component.clone() {
                Some(component) => {
                    // Push the APK if it is not installed in the guest yet; adb
                    // returns an error when it already is, which is harmless.
                    let _ = adb.install(&entry.apk_path());
                    match adb.start_activity(&component) {
                        Ok(out) => {
                            if out.contains("Error") {
                                eprintln!("warning: {out}");
                            } else {
                                println!("started {} inside Android", entry.label);
                            }
                        }
                        Err(e) => eprintln!("warning: could not start the activity: {e}"),
                    }
                }
                None => println!(
                    "{} has no launcher activity; Android is running, open it from the app drawer",
                    entry.label
                ),
            }
        } else {
            println!("adb not found - Android is running, install platform-tools to auto-start apps");
        }
    }

    match crate::coordinator::decide_keep(entry, &state) {
        crate::store::KeepDecision::Wait => {
            if let crate::coordinator::SessionState::Started(mut session) = state {
                let _ = session.wait();
                crate::coordinator::clear_session();
            }
        }
        crate::store::KeepDecision::Detach => {
            println!("Android keeps running in the background (adb port {port})");
        }
    }
    Ok(())
}

/// Human readable summary of what happened, printed after a double-click install.
pub fn summarize(installed: &Installed) -> String {
    let e = &installed.entry;
    let mut lines = Vec::new();
    lines.push(format!("Installed {} ({})", e.label, e.package));
    lines.push(format!(
        "  version {} - {}",
        if e.version_name.is_empty() { "?" } else { &e.version_name },
        crate::util::human_bytes(e.size_bytes)
    ));
    if let Some(c) = &e.launch_component {
        lines.push(format!("  launch: {c}"));
    }
    lines.push(format!(
        "  window: {}x{} ({})",
        e.window_width, e.window_height, e.resolution
    ));
    for s in &installed.shortcut_paths {
        lines.push(format!("  shortcut: {}", s.display()));
    }
    lines.join("\n")
}

pub fn qemu_or_hint() -> Result<QemuInstall, InstallerError> {
    qemu::detect().ok_or_else(|| {
        InstallerError::Qemu(
            "QEMU not found. Install QEMU for Windows (qemu-system-x86_64.exe) and make sure it \
             is on PATH, or run `android-emu setup`."
                .into(),
        )
    })
}

pub fn adb_error(e: QemuError) -> InstallerError {
    InstallerError::Adb(e.to_string())
}
