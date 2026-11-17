//! Global settings (`settings.json`) and the list of Android images (`images.json`).

use crate::json::{self, Value};
use crate::paths;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Settings {
    /// Explicit path to `qemu-system-x86_64.exe` (empty = auto-detect).
    pub qemu_path: String,
    /// Explicit path to `adb.exe` (empty = auto-detect).
    pub adb_path: String,
    /// Default memory in MB for new sessions.
    pub memory_mb: u32,
    /// Default number of vCPUs.
    pub cores: u32,
    /// Default GPU mode for new apps.
    pub gpu: String,
    /// Create a desktop shortcut on install.
    pub create_desktop_shortcut: bool,
    /// Create a Start Menu shortcut on install.
    pub create_start_menu_shortcut: bool,
    /// Ask before replacing an existing installation.
    pub confirm_replace: bool,
    /// Automatically start the VM when an app shortcut is opened.
    pub autostart_vm: bool,
    /// Port base used for adb forwarding.
    pub adb_port: u16,
    /// Keep the emulator window on top.
    pub always_on_top: bool,
    /// Enable ARM translation (libhoudini / libndk) for ARM-only apps.
    pub arm_translation: bool,
    /// Resize the QEMU window to the per-app window size after boot.
    pub apply_window_size: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            qemu_path: String::new(),
            adb_path: String::new(),
            memory_mb: 2048,
            cores: 2,
            gpu: "swiftshader".into(),
            create_desktop_shortcut: true,
            create_start_menu_shortcut: true,
            confirm_replace: true,
            autostart_vm: true,
            adb_port: 5600,
            always_on_top: false,
            arm_translation: true,
            apply_window_size: true,
        }
    }
}

fn settings_path() -> PathBuf {
    paths::data_dir().join("settings.json")
}

impl Settings {
    pub fn load() -> Settings {
        let mut s = Settings::default();
        let Ok(bytes) = std::fs::read(settings_path()) else {
            return s;
        };
        let Ok(text) = String::from_utf8(bytes) else {
            return s;
        };
        let Ok(v) = json::parse(&text) else {
            return s;
        };
        if let Some(x) = v.get("qemuPath").and_then(|x| x.as_str()) {
            s.qemu_path = x.to_string();
        }
        if let Some(x) = v.get("adbPath").and_then(|x| x.as_str()) {
            s.adb_path = x.to_string();
        }
        if let Some(x) = v.get("memoryMb").and_then(|x| x.as_u32()) {
            s.memory_mb = x.clamp(512, 16384);
        }
        if let Some(x) = v.get("cores").and_then(|x| x.as_u32()) {
            s.cores = x.clamp(1, 16);
        }
        if let Some(x) = v.get("gpu").and_then(|x| x.as_str()) {
            s.gpu = x.to_string();
        }
        if let Some(x) = v.get("createDesktopShortcut").and_then(|x| x.as_bool()) {
            s.create_desktop_shortcut = x;
        }
        if let Some(x) = v.get("createStartMenuShortcut").and_then(|x| x.as_bool()) {
            s.create_start_menu_shortcut = x;
        }
        if let Some(x) = v.get("confirmReplace").and_then(|x| x.as_bool()) {
            s.confirm_replace = x;
        }
        if let Some(x) = v.get("autostartVm").and_then(|x| x.as_bool()) {
            s.autostart_vm = x;
        }
        if let Some(x) = v.get("adbPort").and_then(|x| x.as_u32()) {
            s.adb_port = (x as u16).max(1024);
        }
        if let Some(x) = v.get("alwaysOnTop").and_then(|x| x.as_bool()) {
            s.always_on_top = x;
        }
        if let Some(x) = v.get("armTranslation").and_then(|x| x.as_bool()) {
            s.arm_translation = x;
        }
        if let Some(x) = v.get("applyWindowSize").and_then(|x| x.as_bool()) {
            s.apply_window_size = x;
        }
        s
    }

    pub fn to_json(&self) -> Value {
        let mut v = Value::new_object();
        v.set("qemuPath", Value::Str(self.qemu_path.clone()));
        v.set("adbPath", Value::Str(self.adb_path.clone()));
        v.set("memoryMb", Value::Int(self.memory_mb as i64));
        v.set("cores", Value::Int(self.cores as i64));
        v.set("gpu", Value::Str(self.gpu.clone()));
        v.set("createDesktopShortcut", Value::Bool(self.create_desktop_shortcut));
        v.set("createStartMenuShortcut", Value::Bool(self.create_start_menu_shortcut));
        v.set("confirmReplace", Value::Bool(self.confirm_replace));
        v.set("autostartVm", Value::Bool(self.autostart_vm));
        v.set("adbPort", Value::Int(self.adb_port as i64));
        v.set("alwaysOnTop", Value::Bool(self.always_on_top));
        v.set("armTranslation", Value::Bool(self.arm_translation));
        v.set("applyWindowSize", Value::Bool(self.apply_window_size));
        v
    }

    pub fn save(&self) -> std::io::Result<()> {
        paths::ensure_data_dir()?;
        std::fs::write(settings_path(), self.to_json().to_pretty())
    }
}

pub fn configured_qemu_path() -> Option<PathBuf> {
    let s = Settings::load();
    if s.qemu_path.is_empty() {
        None
    } else {
        Some(PathBuf::from(s.qemu_path))
    }
}

/// One installed Android system image.
#[derive(Debug, Clone)]
pub struct ImageEntry {
    pub name: String,
    pub path: PathBuf,
    pub kind: ImageKind,
    pub size_bytes: u64,
    pub added_at: u64,
    pub notes: String,
    /// True when the image is writable (raw disk / persistent installation).
    pub writable: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageKind {
    /// Live ISO (read-only boot, no persistence).
    Iso,
    /// Raw disk image with an installed Android.
    Raw,
    /// QCOW2 disk image with an installed Android.
    Qcow2,
}

impl ImageKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ImageKind::Iso => "iso",
            ImageKind::Raw => "raw",
            ImageKind::Qcow2 => "qcow2",
        }
    }

    pub fn from_str(s: &str) -> ImageKind {
        match s {
            "raw" | "img" | "dd" => ImageKind::Raw,
            "qcow2" => ImageKind::Qcow2,
            _ => ImageKind::Iso,
        }
    }

    pub fn detect(path: &std::path::Path) -> ImageKind {
        let ext = path
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        match ext.as_str() {
            "img" | "raw" | "dd" => ImageKind::Raw,
            "qcow2" => ImageKind::Qcow2,
            _ => ImageKind::Iso,
        }
    }
}

fn images_path() -> PathBuf {
    paths::data_dir().join("images.json")
}

#[derive(Debug, Clone, Default)]
pub struct ImageRegistry {
    pub images: Vec<ImageEntry>,
    pub active: Option<String>,
}

impl ImageRegistry {
    pub fn load() -> ImageRegistry {
        let Ok(bytes) = std::fs::read(images_path()) else {
            return ImageRegistry::default();
        };
        let Ok(text) = String::from_utf8(bytes) else {
            return ImageRegistry::default();
        };
        let Ok(v) = json::parse(&text) else {
            return ImageRegistry::default();
        };
        let images = v
            .get("images")
            .and_then(|x| x.as_array())
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| {
                        Some(ImageEntry {
                            name: item.get("name")?.as_str()?.to_string(),
                            path: PathBuf::from(item.get("path")?.as_str()?),
                            kind: ImageKind::from_str(
                                item.get("kind").and_then(|k| k.as_str()).unwrap_or("iso"),
                            ),
                            size_bytes: item.get("sizeBytes").and_then(|s| s.as_u64()).unwrap_or(0),
                            added_at: item.get("addedAt").and_then(|s| s.as_u64()).unwrap_or(0),
                            notes: item
                                .get("notes")
                                .and_then(|s| s.as_str())
                                .unwrap_or("")
                                .to_string(),
                            writable: item
                                .get("writable")
                                .and_then(|s| s.as_bool())
                                .unwrap_or(false),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        let active = v
            .get("active")
            .and_then(|x| x.as_str())
            .map(|s| s.to_string());
        ImageRegistry { images, active }
    }

    pub fn save(&self) -> std::io::Result<()> {
        paths::ensure_data_dir()?;
        let mut v = Value::new_object();
        v.set(
            "images",
            Value::Array(
                self.images
                    .iter()
                    .map(|img| {
                        let mut o = Value::new_object();
                        o.set("name", Value::Str(img.name.clone()));
                        o.set("path", Value::Str(img.path.to_string_lossy().to_string()));
                        o.set("kind", Value::Str(img.kind.as_str().to_string()));
                        o.set("sizeBytes", Value::Int(img.size_bytes as i64));
                        o.set("addedAt", Value::Int(img.added_at as i64));
                        o.set("notes", Value::Str(img.notes.clone()));
                        o.set("writable", Value::Bool(img.writable));
                        o
                    })
                    .collect(),
            ),
        );
        v.set(
            "active",
            self.active
                .clone()
                .map(Value::Str)
                .unwrap_or(Value::Null),
        );
        std::fs::write(images_path(), v.to_pretty())
    }

    pub fn active_image(&self) -> Option<&ImageEntry> {
        if let Some(name) = &self.active {
            if let Some(img) = self.images.iter().find(|i| &i.name == name) {
                return Some(img);
            }
        }
        self.images.first()
    }

    pub fn add(&mut self, path: &std::path::Path, name: Option<String>) -> Result<ImageEntry, String> {
        if !path.is_file() {
            return Err(format!("{} does not exist", path.display()));
        }
        let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        let kind = ImageKind::detect(path);
        let name = name.unwrap_or_else(|| {
            path.file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "android".to_string())
        });
        let rel = paths::images_dir();
        std::fs::create_dir_all(&rel).ok();
        let stored = if path.starts_with(&rel) {
            path.to_path_buf()
        } else {
            // Copy into the managed folder so the image is not lost when the
            // original download is deleted.
            let dest = rel.join(
                path.file_name()
                    .map(|n| n.to_os_string())
                    .unwrap_or_else(|| "android.img".into()),
            );
            if !dest.exists() {
                std::fs::copy(path, &dest).map_err(|e| format!("copy failed: {e}"))?;
            }
            dest
        };
        let entry = ImageEntry {
            name: name.clone(),
            path: stored,
            kind,
            size_bytes: size,
            added_at: now(),
            notes: String::new(),
            writable: matches!(kind, ImageKind::Raw | ImageKind::Qcow2),
        };
        self.images.retain(|i| i.name != name);
        self.images.push(entry.clone());
        if self.active.is_none() {
            self.active = Some(name);
        }
        self.save().map_err(|e| e.to_string())?;
        Ok(entry)
    }

    pub fn set_active(&mut self, name: &str) -> Result<(), String> {
        if !self.images.iter().any(|i| i.name == name) {
            return Err(format!("no image named {name}"));
        }
        self.active = Some(name.to_string());
        self.save().map_err(|e| e.to_string())
    }

    pub fn remove(&mut self, name: &str) -> Result<(), String> {
        let Some(pos) = self.images.iter().position(|i| i.name == name) else {
            return Err(format!("no image named {name}"));
        };
        let img = self.images.remove(pos);
        if img.path.starts_with(paths::images_dir()) {
            std::fs::remove_file(&img.path).ok();
        }
        if self.active.as_deref() == Some(name) {
            self.active = self.images.first().map(|i| i.name.clone());
        }
        self.save().map_err(|e| e.to_string())
    }
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arm_translation_defaults_on() {
        assert!(Settings::default().arm_translation);
    }

    #[test]
    fn default_settings_are_sane() {
        let s = Settings::default();
        assert!(s.memory_mb >= 1024);
        assert!(s.cores >= 1);
        let v = s.to_json();
        assert_eq!(v.get("memoryMb").unwrap().as_u32(), Some(s.memory_mb));
    }

    #[test]
    fn image_kind_detection() {
        assert_eq!(ImageKind::detect(std::path::Path::new("x.iso")), ImageKind::Iso);
        assert_eq!(ImageKind::detect(std::path::Path::new("x.img")), ImageKind::Raw);
        assert_eq!(ImageKind::detect(std::path::Path::new("x.qcow2")), ImageKind::Qcow2);
        assert!(matches!(ImageKind::from_str("raw"), ImageKind::Raw));
    }
}
