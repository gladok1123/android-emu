//! Installation database: every installed APK becomes a "program" with its own
//! launcher entry, QEMU session settings and icon.
//!
//! Layout of `%LOCALAPPDATA%\android-emu`:
//!
//! ```text
//! apps/<slug>/app.apk          copy of the installed package
//! apps/<slug>/icon.png         launcher icon extracted from the APK
//! apps/<slug>/icon.ico         icon used by the Windows shortcut
//! apps/<slug>/info.txt         human readable summary
//! apps/<slug>/config.json      window size, resolution, GPU mode, ...
//! apps/<slug>/run.bat          debug launcher (keeps the QEMU console visible)
//! apps/index.json              index of all installed apps
//! ```

use crate::apk::{self, ApkInfo};
use crate::json::{self, Value};
use crate::util::{fnv1a64, sanitize_file_name};
use std::fmt;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub enum AppError {
    Io(String),
    Json(String),
    Apk(String),
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppError::Io(e) => write!(f, "{e}"),
            AppError::Json(e) => write!(f, "config file is corrupt: {e}"),
            AppError::Apk(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for AppError {}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        AppError::Io(e.to_string())
    }
}

/// How the Android window is presented.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayMode {
    /// Emulated phone/tablet screen inside a resizable QEMU window.
    Window,
    /// Borderless, maximised (good for TVs / full screen apps).
    Fullscreen,
    /// No QEMU window at all == "headless": only adb works (screen mirroring off).
    Headless,
}

impl DisplayMode {
    pub fn as_str(self) -> &'static str {
        match self {
            DisplayMode::Window => "window",
            DisplayMode::Fullscreen => "fullscreen",
            DisplayMode::Headless => "headless",
        }
    }

    pub fn from_str(s: &str) -> DisplayMode {
        match s {
            "fullscreen" | "full-screen" | "full" => DisplayMode::Fullscreen,
            "headless" | "none" => DisplayMode::Headless,
            _ => DisplayMode::Window,
        }
    }
}

impl Default for DisplayMode {
    fn default() -> Self {
        DisplayMode::Window
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GpuMode {
    /// Pure software rendering, works on every machine (slower).
    Swiftshader,
    /// Host OpenGL via ANGLE-D3D11 (fast, needs a GPU).
    Angle,
    /// Guest-side software rendering.
    Guest,
    /// No GPU at all (only for headless/server use).
    Off,
}

impl GpuMode {
    pub fn as_str(self) -> &'static str {
        match self {
            GpuMode::Swiftshader => "swiftshader",
            GpuMode::Angle => "angle",
            GpuMode::Guest => "guest",
            GpuMode::Off => "off",
        }
    }

    pub fn from_str(s: &str) -> GpuMode {
        match s.to_ascii_lowercase().as_str() {
            "angle" | "host" | "gpu" => GpuMode::Angle,
            "guest" => GpuMode::Guest,
            "off" | "none" | "headless" => GpuMode::Off,
            _ => GpuMode::Swiftshader,
        }
    }
}

impl Default for GpuMode {
    fn default() -> Self {
        GpuMode::Swiftshader
    }
}

impl GpuMode {
    pub fn qemu_arg(self) -> &'static str {
        match self {
            GpuMode::Swiftshader => "swiftshader_indirect",
            GpuMode::Angle => "angle_indirect",
            GpuMode::Guest => "guest",
            GpuMode::Off => "off",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            GpuMode::Swiftshader => "software (works everywhere)",
            GpuMode::Angle => "GPU accelerated (ANGLE/D3D11)",
            GpuMode::Guest => "guest rendering",
            GpuMode::Off => "off (headless)",
        }
    }
}

/// A single installed application.
#[derive(Debug, Clone)]
pub struct AppEntry {
    pub slug: String,
    pub package: String,
    pub label: String,
    pub version_name: String,
    pub version_code: u32,
    pub launch_component: Option<String>,
    pub min_sdk: Option<u32>,
    pub target_sdk: Option<u32>,
    pub size_bytes: u64,
    pub installed_at: u64,
    /// Window size in host pixels when the app opens.
    pub window_width: u32,
    pub window_height: u32,
    /// Emulated screen resolution (Android-x86 modes).
    pub resolution: String,
    pub dpi: u32,
    pub display_mode: DisplayMode,
    pub gpu: GpuMode,
    /// Extra QEMU arguments appended to the session command line.
    pub extra_args: Vec<String>,
    pub keep_running: bool,
}

impl AppEntry {
    pub fn to_json(&self) -> Value {
        let mut v = Value::new_object();
        v.set("slug", Value::Str(self.slug.clone()));
        v.set("package", Value::Str(self.package.clone()));
        v.set("label", Value::Str(self.label.clone()));
        v.set("versionName", Value::Str(self.version_name.clone()));
        v.set("versionCode", Value::Int(self.version_code as i64));
        v.set(
            "launchComponent",
            match &self.launch_component {
                Some(c) => Value::Str(c.clone()),
                None => Value::Null,
            },
        );
        v.set(
            "minSdk",
            self.min_sdk.map(|x| Value::Int(x as i64)).unwrap_or(Value::Null),
        );
        v.set(
            "targetSdk",
            self.target_sdk.map(|x| Value::Int(x as i64)).unwrap_or(Value::Null),
        );
        v.set("sizeBytes", Value::Int(self.size_bytes as i64));
        v.set("installedAt", Value::Int(self.installed_at as i64));
        v.set("windowWidth", Value::Int(self.window_width as i64));
        v.set("windowHeight", Value::Int(self.window_height as i64));
        v.set("resolution", Value::Str(self.resolution.clone()));
        v.set("dpi", Value::Int(self.dpi as i64));
        v.set("displayMode", Value::Str(self.display_mode.as_str().to_string()));
        v.set("gpu", Value::Str(self.gpu.as_str().to_string()));
        v.set(
            "extraArgs",
            Value::Array(self.extra_args.iter().map(|s| Value::Str(s.clone())).collect()),
        );
        v.set("keepRunning", Value::Bool(self.keep_running));
        v
    }

    pub fn from_json(v: &Value) -> Option<AppEntry> {
        let package = v.get("package")?.as_str()?.to_string();
        let slug = v
            .get("slug")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| sanitize_file_name(&package));
        Some(AppEntry {
            slug,
            package,
            label: v
                .get("label")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            version_name: v
                .get("versionName")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            version_code: v.get("versionCode").and_then(|v| v.as_u32()).unwrap_or(0),
            launch_component: v
                .get("launchComponent")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            min_sdk: v.get("minSdk").and_then(|v| v.as_u32()),
            target_sdk: v.get("targetSdk").and_then(|v| v.as_u32()),
            size_bytes: v.get("sizeBytes").and_then(|v| v.as_u64()).unwrap_or(0),
            installed_at: v.get("installedAt").and_then(|v| v.as_u64()).unwrap_or(0),
            window_width: v
                .get("windowWidth")
                .and_then(|v| v.as_u32())
                .unwrap_or_else(default_window),
            window_height: v
                .get("windowHeight")
                .and_then(|v| v.as_u32())
                .unwrap_or_else(default_window),
            resolution: v
                .get("resolution")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(default_resolution),
            dpi: v.get("dpi").and_then(|v| v.as_u32()).unwrap_or(0),
            display_mode: DisplayMode::from_str(
                v.get("displayMode").and_then(|v| v.as_str()).unwrap_or("window"),
            ),
            gpu: GpuMode::from_str(v.get("gpu").and_then(|v| v.as_str()).unwrap_or("swiftshader")),
            extra_args: v
                .get("extraArgs")
                .map(|v| v.str_vec())
                .unwrap_or_default(),
            keep_running: v.get("keepRunning").and_then(|v| v.as_bool()).unwrap_or(false),
        })
    }
}

fn default_window() -> u32 {
    720
}

fn default_resolution() -> String {
    "720x1280".to_string()
}

impl AppEntry {
    /// Human readable name used for shortcuts.
    pub fn display_name(&self) -> &str {
        if self.label.trim().is_empty() {
            &self.package
        } else {
            &self.label
        }
    }

    pub fn dir(&self) -> PathBuf {
        crate::paths::apps_dir().join(&self.slug)
    }

    pub fn apk_path(&self) -> PathBuf {
        self.dir().join("app.apk")
    }

    pub fn icon_png_path(&self) -> PathBuf {
        self.dir().join("icon.png")
    }

    pub fn icon_ico_path(&self) -> PathBuf {
        self.dir().join("icon.ico")
    }

    pub fn config_path(&self) -> PathBuf {
        self.dir().join("config.json")
    }

    /// Split "720x1280" into (width, height).
    pub fn resolution_px(&self) -> (u32, u32) {
        parse_resolution(&self.resolution).unwrap_or((720, 1280))
    }
}

pub fn parse_resolution(s: &str) -> Option<(u32, u32)> {
    let (w, h) = s.split_once(['x', 'X', '*'])?;
    let w: u32 = w.trim().parse().ok()?;
    let h: u32 = h.trim().parse().ok()?;
    if (200..=4096).contains(&w) && (200..=4096).contains(&h) {
        Some((w, h))
    } else {
        None
    }
}

#[derive(Debug, Default)]
pub struct AppIndex {
    pub apps: Vec<AppEntry>,
    pub version: u32,
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn index_path() -> PathBuf {
    crate::paths::apps_dir().join("index.json")
}

pub fn load_index() -> AppIndex {
    let path = index_path();
    let Ok(bytes) = std::fs::read(&path) else {
        return AppIndex { apps: Vec::new(), version: 1 };
    };
    match std::str::from_utf8(&bytes).ok().and_then(|t| json::parse(t).ok()) {
        Some(value) => AppIndex::from_json(&value),
        None => AppIndex { apps: Vec::new(), version: 1 },
    }
}

pub fn save_index(index: &AppIndex) -> Result<(), AppError> {
    let path = index_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, index.to_json().to_pretty())?;
    Ok(())
}

impl AppIndex {
    pub fn to_json(&self) -> Value {
        let mut root = Value::new_object();
        root.set("version", Value::Int(self.version as i64));
        root.set(
            "apps",
            Value::Array(self.apps.iter().map(|a| a.to_json()).collect()),
        );
        root
    }

    pub fn from_json(v: &Value) -> AppIndex {
        let apps = v
            .get("apps")
            .and_then(|a| a.as_array())
            .map(|items| items.iter().filter_map(AppEntry::from_json).collect())
            .unwrap_or_default();
        AppIndex {
            apps,
            version: v.get("version").and_then(|v| v.as_u32()).unwrap_or(1),
        }
    }
}

/// What the launcher does after starting an app inside the VM.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeepDecision {
    /// Wait for the QEMU process to exit (the app window is the only window).
    Wait,
    /// Leave the VM running in the background and return immediately.
    Detach,
}

/// Build a unique, filesystem-safe slug for an app.
pub fn make_slug(package: &str, label: &str, existing: &[AppEntry]) -> String {
    let base = sanitize_file_name(&format!("{} - {}", label, package));
    let mut slug = base.clone();
    let mut n = 2;
    while existing.iter().any(|a| a.slug == slug) {
        slug = format!("{base} ({n})");
        n += 1;
    }
    slug
}

/// Install (copy + analyse + register) an APK file.
pub struct InstallOutcome {
    pub entry: AppEntry,
    pub replaced: bool,
}

pub fn install_apk(
    apk_file: &Path,
    overrides: InstallOptions,
) -> Result<InstallOutcome, AppError> {
    crate::paths::ensure(&crate::paths::apps_dir())?;
    let info: ApkInfo = apk::parse_apk(apk_file).map_err(|e| AppError::Apk(e.to_string()))?;

    let mut index = load_index();
    let existing_pos = index.apps.iter().position(|a| a.package == info.package);
    let replaced = existing_pos.is_some();
    let slug = match existing_pos {
        Some(pos) => index.apps[pos].slug.clone(),
        None => make_slug(&info.package, &info.label, &index.apps),
    };

    let mut entry = match existing_pos {
        Some(pos) => index.apps[pos].clone(),
        None => AppEntry {
            slug: slug.clone(),
            package: info.package.clone(),
            label: info.label.clone(),
            version_name: String::new(),
            version_code: 0,
            launch_component: None,
            min_sdk: None,
            target_sdk: None,
            size_bytes: 0,
            installed_at: 0,
            window_width: default_window(),
            window_height: default_window(),
            resolution: default_resolution(),
            dpi: 0,
            display_mode: DisplayMode::Window,
            gpu: GpuMode::default(),
            extra_args: Vec::new(),
            keep_running: false,
        },
    };

    entry.package = info.package.clone();
    entry.label = info.label.clone();
    entry.version_name = info.version_name.clone();
    entry.version_code = info.version_code;
    entry.launch_component = info.launch_component();
    entry.min_sdk = info.min_sdk;
    entry.target_sdk = info.target_sdk;
    entry.size_bytes = info.size_bytes;
    entry.installed_at = now_secs();

    overrides.apply(&mut entry);

    let dir = crate::paths::apps_dir().join(&entry.slug);
    std::fs::create_dir_all(&dir)?;
    std::fs::copy(apk_file, entry.apk_path())?;

    let icon_png = apk::best_icon(&info);
    std::fs::write(entry.icon_png_path(), &icon_png)?;
    std::fs::write(entry.icon_ico_path(), apk::png_to_ico(&icon_png))?;

    save_entry_config(&entry)?;
    write_info_file(&entry, &info)?;
    write_run_bat(&entry)?;

    match existing_pos {
        Some(pos) => index.apps[pos] = entry.clone(),
        None => index.apps.push(entry.clone()),
    }
    save_index(&index)?;

    Ok(InstallOutcome { entry, replaced })
}

#[derive(Debug, Clone, Default)]
pub struct InstallOptions {
    pub window: Option<(u32, u32)>,
    pub resolution: Option<String>,
    pub dpi: Option<u32>,
    pub gpu: Option<GpuMode>,
    pub display_mode: Option<DisplayMode>,
}

impl InstallOptions {
    fn apply(&self, entry: &mut AppEntry) {
        if let Some((w, h)) = self.window {
            entry.window_width = w;
            entry.window_height = h;
        }
        if let Some(r) = &self.resolution {
            entry.resolution = r.clone();
        }
        if let Some(d) = self.dpi {
            entry.dpi = d;
        }
        if let Some(g) = self.gpu {
            entry.gpu = g;
        }
        if let Some(m) = self.display_mode {
            entry.display_mode = m;
        }
    }
}

pub fn save_entry_config(entry: &AppEntry) -> Result<(), AppError> {
    let path = entry.config_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, entry.to_json().to_pretty())?;
    Ok(())
}

fn write_info_file(entry: &AppEntry, info: &ApkInfo) -> Result<(), AppError> {
    let mut text = String::new();
    text.push_str(&format!("{}\n{}\n\n", entry.label, "=".repeat(entry.label.len().max(3))));
    text.push_str(&format!("Package:        {}\n", info.package));
    text.push_str(&format!("Version:        {} ({})\n", info.version_name, info.version_code));
    text.push_str(&format!("Size:           {}\n", crate::util::human_bytes(info.size_bytes)));
    text.push_str(&format!(
        "SDK:            min {} / target {}\n",
        info.min_sdk.map(|v| v.to_string()).unwrap_or("?".into()),
        info.target_sdk.map(|v| v.to_string()).unwrap_or("?".into())
    ));
    if let Some(c) = entry.launch_component.as_ref() {
        text.push_str(&format!("Launch:         {}\n", c));
    }
    if !info.abis.is_empty() {
        text.push_str(&format!("Native ABIs:    {}\n", info.abis.join(", ")));
    }
    if !info.permissions.is_empty() {
        text.push_str(&format!("\nPermissions ({}):\n", info.permissions.len()));
        for p in &info.permissions {
            text.push_str(&format!("  - {}\n", p));
        }
    }
    text.push_str(&format!("\nInstalled at:   {}\n", entry.installed_at));
    text.push_str("\nThis application runs inside the shared android-emu Android VM.\n");
    std::fs::write(entry.dir().join("info.txt"), text)?;
    Ok(())
}

fn write_run_bat(entry: &AppEntry) -> Result<(), AppError> {
    let exe = crate::win::exe_path();
    let content = format!(
        "@echo off\r\nrem Debug launcher for \"{}\" ({})\r\nrem Remove the 'start' below to watch QEMU output in this console.\r\n\"{}\" --run \"{}\" %*\r\npause\r\n",
        entry.label,
        entry.package,
        exe.display(),
        entry.slug
    );
    std::fs::write(entry.dir().join("run.bat"), content)?;
    Ok(())
}

pub fn find_by_slug(slug: &str) -> Option<AppEntry> {
    load_index().apps.into_iter().find(|a| a.slug == slug)
}

pub fn find_by_package(package: &str) -> Option<AppEntry> {
    load_index()
        .apps
        .into_iter()
        .find(|a| a.package == package)
}

/// Resolve user input that can be a slug, a package name or a label.
pub fn find_fuzzy(query: &str) -> Option<AppEntry> {
    let index = load_index();
    let q = query.to_lowercase();
    index
        .apps
        .iter()
        .find(|a| a.slug.eq_ignore_ascii_case(query) || a.package == query)
        .or_else(|| {
            index.apps.iter().find(|a| {
                a.package.to_lowercase().contains(&q) || a.label.to_lowercase().contains(&q)
            })
        })
        .cloned()
}

pub fn uninstall(slug: &str) -> Result<bool, AppError> {
    let mut index = load_index();
    let before = index.apps.len();
    if let Some(pos) = index.apps.iter().position(|a| a.slug == slug || a.package == slug) {
        let entry = index.apps.remove(pos);
        let dir = entry.dir();
        if dir.exists() {
            std::fs::remove_dir_all(&dir)?;
        }
        save_index(&index)?;
    }
    Ok(index.apps.len() != before)
}

pub fn list() -> Vec<AppEntry> {
    let mut apps = load_index().apps;
    apps.sort_by(|a, b| a.label.to_lowercase().cmp(&b.label.to_lowercase()));
    apps
}

/// Stable short id (for logging and window titles).
pub fn short_id(entry: &AppEntry) -> String {
    format!("{:08x}", (fnv1a64(entry.package.as_bytes()) & 0xFFFF_FFFF) as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolutions() {
        assert_eq!(parse_resolution("720x1280"), Some((720, 1280)));
        assert_eq!(parse_resolution("1920X1080"), Some((1920, 1080)));
        assert_eq!(parse_resolution("12x1280"), None);
        assert_eq!(parse_resolution("nonsense"), None);
    }

    #[test]
    fn slugs_are_unique_and_safe() {
        let existing = vec![AppEntry {
            slug: "App - com.a".to_string(),
            package: "com.a".into(),
            label: "App".into(),
            version_name: String::new(),
            version_code: 0,
            launch_component: None,
            min_sdk: None,
            target_sdk: None,
            size_bytes: 0,
            installed_at: 0,
            window_width: 720,
            window_height: 720,
            resolution: default_resolution(),
            dpi: 0,
            display_mode: DisplayMode::Window,
            gpu: GpuMode::default(),
            extra_args: vec![],
            keep_running: false,
        }];
        let slug = make_slug("com.a", "App", &existing);
        assert_eq!(slug, "App - com.a (2)");
    }

    #[test]
    fn config_roundtrip() {
        let entry = AppEntry {
            slug: "x".into(),
            package: "com.x".into(),
            label: "X".into(),
            version_name: "1".into(),
            version_code: 1,
            launch_component: Some("com.x/.Main".into()),
            min_sdk: Some(21),
            target_sdk: Some(30),
            size_bytes: 42,
            installed_at: 7,
            window_width: 540,
            window_height: 960,
            resolution: "540x960".into(),
            dpi: 240,
            display_mode: DisplayMode::Fullscreen,
            gpu: GpuMode::Angle,
            extra_args: vec!["-smp".into(), "4".into()],
            keep_running: true,
        };
        let json = entry.to_json().to_pretty();
        let back = AppEntry::from_json(&json::parse(&json).unwrap()).unwrap();
        assert_eq!(back.display_mode, DisplayMode::Fullscreen);
        assert_eq!(back.gpu, GpuMode::Angle);
        assert_eq!(back.resolution_px(), (540, 960));
    }
}
