//! High level APK analysis: package name, application label, launcher icon,
//! launch activity, SDK levels and architecture.

use crate::arsc::{Config, ResTable};
use crate::axml::{self, AttrValue};
use crate::png;
use crate::zip::ZipArchive;
use std::fmt;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub enum ApkError {
    Io(String),
    Zip(crate::zip::ZipError),
    Manifest(String),
    Resources(String),
    NotAnApk(String),
}

impl fmt::Display for ApkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ApkError::Io(e) => write!(f, "cannot read APK: {e}"),
            ApkError::Zip(e) => write!(f, "{e}"),
            ApkError::Manifest(e) => write!(f, "invalid AndroidManifest.xml: {e}"),
            ApkError::Resources(e) => write!(f, "invalid resources.arsc: {e}"),
            ApkError::NotAnApk(e) => write!(f, "not an Android package: {e}"),
        }
    }
}

impl std::error::Error for ApkError {}

#[derive(Debug, Clone, Default)]
pub struct ApkInfo {
    pub path: PathBuf,
    pub file_name: String,
    pub size_bytes: u64,
    pub package: String,
    pub label: String,
    pub version_name: String,
    pub version_code: u32,
    pub min_sdk: Option<u32>,
    pub target_sdk: Option<u32>,
    pub launch_activity: Option<String>,
    pub activities: Vec<String>,
    pub permissions: Vec<String>,
    pub icon_in_apk: Option<String>,
    pub icon_png: Option<Vec<u8>>,
    pub abis: Vec<String>,
    pub is_split: bool,
    pub features: Vec<String>,
}

impl ApkInfo {
    /// `package/activity` component for `am start -n`.
    pub fn launch_component(&self) -> Option<String> {
        let activity = self
            .launch_activity
            .clone()
            .or_else(|| self.activities.first().cloned())?;
        let activity = if activity.starts_with('.') {
            format!("{}{}", self.package, activity)
        } else if !activity.contains('.') {
            format!("{}.{}", self.package, activity)
        } else {
            activity
        };
        Some(format!("{}/{}", self.package, activity))
    }

    pub fn display_label(&self) -> &str {
        if self.label.trim().is_empty() {
            &self.package
        } else {
            &self.label
        }
    }

    pub fn density(&self) -> u32 {
        self.target_sdk.unwrap_or(0)
    }
}

/// Read only what is needed for the launcher icon, from `AndroidManifest.xml`.
pub fn parse_apk(path: &Path) -> Result<ApkInfo, ApkError> {
    let zip = ZipArchive::open(path).map_err(ApkError::Zip)?;
    let size_bytes = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);

    let manifest_name = if zip.find("AndroidManifest.xml").is_some() {
        "AndroidManifest.xml"
    } else {
        return Err(ApkError::NotAnApk(
            "AndroidManifest.xml is missing (not an APK?)".to_string(),
        ));
    };

    let manifest_data = zip
        .read_entry_limited(manifest_name, 8 * 1024 * 1024)
        .map_err(ApkError::Zip)?;
    let doc = axml::parse(&manifest_data).map_err(|e| ApkError::Manifest(e.to_string()))?;
    let manifest = doc
        .find_child("manifest")
        .ok_or_else(|| ApkError::Manifest("<manifest> element not found".into()))?;

    let package = manifest
        .attr("package")
        .and_then(|a| a.value.as_str())
        .unwrap_or_default()
        .to_string();
    if package.is_empty() {
        return Err(ApkError::NotAnApk("package attribute is missing".into()));
    }

    let mut info = ApkInfo {
        path: path.to_path_buf(),
        file_name: path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default(),
        size_bytes,
        package,
        ..Default::default()
    };

    if let Some(v) = manifest.android_attr("versionName") {
        info.version_name = match &v.value {
            AttrValue::Reference(_id) => String::new(),
            other => other.to_display(),
        };
    }
    info.version_code = manifest
        .android_attr("versionCode")
        .and_then(|a| a.value.as_u32())
        .unwrap_or(0);

    // uses-sdk
    if let Some(sdk) = manifest.find_child("uses-sdk") {
        info.min_sdk = sdk.android_attr("minSdkVersion").and_then(|a| a.value.as_u32());
        info.target_sdk = sdk
            .android_attr("targetSdkVersion")
            .and_then(|a| a.value.as_u32());
    }

    // permissions
    for el in manifest.children_named("uses-permission") {
        if let Some(name) = el.android_attr("name").and_then(|a| a.value.as_str()) {
            info.permissions.push(name.to_string());
        }
    }
    for el in manifest.children_named("uses-feature") {
        if let Some(name) = el.android_attr("name").and_then(|a| a.value.as_str()) {
            info.features.push(name.to_string());
        }
    }

    let resources = load_resources(&zip);
    if info.version_name.is_empty() {
        if let Some(rt) = &resources {
            if let Some(v) = manifest.android_attr("versionName") {
                if let AttrValue::Reference(id) = v.value {
                    info.version_name = rt.resolve_string(id).unwrap_or_default();
                }
            }
        }
    }

    // activities & launch activity
    let mut label_ref: Option<u32> = None;
    let mut icon_ref: Option<u32> = None;
    let mut icon_str: Option<String> = None;
    let mut label_str: Option<String> = None;

    if let Some(app) = manifest.find_child("application") {
        if let Some(a) = app.android_attr("label") {
            match &a.value {
                AttrValue::Str(s) => label_str = Some(s.clone()),
                AttrValue::Reference(id) => label_ref = Some(*id),
                _ => {}
            }
        }
        for key in ["icon", "roundIcon"] {
            if let Some(a) = app.android_attr(key) {
                match &a.value {
                    AttrValue::Str(s) if !s.is_empty() => icon_str = Some(s.clone()),
                    AttrValue::Reference(id) => icon_ref = Some(*id),
                    _ => {}
                }
                if icon_ref.is_some() || icon_str.is_some() {
                    break;
                }
            }
        }

        for el in app.children.iter() {
            if el.tag == "activity" || el.tag == "activity-alias" {
                let name = el
                    .android_attr("name")
                    .and_then(|a| a.value.as_str())
                    .unwrap_or_default()
                    .to_string();
                let target = el
                    .android_attr("targetActivity")
                    .and_then(|a| a.value.as_str())
                    .map(|s| s.to_string());
                let component = if el.tag == "activity-alias" && target.is_some() {
                    target.clone().unwrap()
                } else {
                    name.clone()
                };
                if !component.is_empty() {
                    info.activities.push(component.clone());
                }
                if info.launch_activity.is_none() && has_launcher_intent(el) {
                    info.launch_activity = Some(component);
                    // Labels/icons may be overridden per activity.
                    if label_ref.is_none() && label_str.is_none() {
                        if let Some(a) = el.android_attr("label") {
                            match &a.value {
                                AttrValue::Str(s) => label_str = Some(s.clone()),
                                AttrValue::Reference(id) => label_ref = Some(*id),
                                _ => {}
                            }
                        }
                    }
                    if icon_ref.is_none() && icon_str.is_none() {
                        if let Some(a) = el.android_attr("icon") {
                            match &a.value {
                                AttrValue::Str(s) => icon_str = Some(s.clone()),
                                AttrValue::Reference(id) => icon_ref = Some(*id),
                                _ => {}
                            }
                        }
                    }
                }
            }
        }
    }

    info.label = label_str
        .or_else(|| {
            label_ref
                .and_then(|id| resources.as_ref().and_then(|rt| rt.resolve_string(id)))
        })
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| info.package.clone());

    // Icon: prefer the explicit icon, then well-known resource names/paths.
    let mut icon_candidates: Vec<String> = Vec::new();
    if let Some(s) = icon_str {
        if !s.is_empty() {
            icon_candidates.push(s);
        }
    }
    if let Some((cfg, path)) = icon_ref
        .and_then(|id| resources.as_ref().and_then(|rt| rt.resolve_path(id)))
    {
        let _ = cfg;
        icon_candidates.push(path);
    }
    if icon_candidates.is_empty() {
        icon_candidates.extend(guess_icon_paths(&zip));
    }

    for candidate in &icon_candidates {
        if let Some(png) = load_icon_png(&zip, candidate) {
            info.icon_in_apk = Some(candidate.clone());
            info.icon_png = Some(png);
            break;
        }
        // Adaptive icon XML: pull a PNG out of the referenced layer.
        if candidate.ends_with(".xml") {
            if let Ok(xml) = zip.read_entry_limited(candidate, 256 * 1024) {
                if let Ok(doc) = axml::parse(&xml) {
                    if let Some(drawable) = doc.find_descendant("foreground")
                        .or_else(|| doc.find_descendant("background"))
                    {
                        if let Some(a) = drawable.android_attr("drawable") {
                            if let AttrValue::Reference(id) = a.value {
                                if let Some((_, p)) = resources
                                    .as_ref()
                                    .and_then(|rt| rt.resolve_path(id))
                                {
                                    if let Some(png) = load_icon_png(&zip, &p) {
                                        info.icon_in_apk = Some(p.clone());
                                        info.icon_png = Some(png);
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    if info.icon_png.is_none() {
        info.icon_png = Some(png::letter_icon(&info.label, &info.package));
        info.icon_in_apk = None;
    }

    // ABIs used by native libraries.
    for abi in ["arm64-v8a", "armeabi-v7a", "x86_64", "x86", "armeabi"] {
        if zip
            .names()
            .any(|n| n.starts_with(&format!("lib/{abi}/")))
        {
            info.abis.push(abi.to_string());
        }
    }
    info.is_split = zip.find("split_name") .is_some()
        || manifest.find_child("uses-split").is_some();

    Ok(info)
}

fn load_resources(zip: &ZipArchive) -> Option<ResTable> {
    if zip.find("resources.arsc").is_none() {
        return None;
    }
    let data = zip.read_entry("resources.arsc").ok()?;
    ResTable::parse(&data).ok()
}

fn has_launcher_intent(activity: &axml::Element) -> bool {
    let mut has_main = false;
    let mut has_launcher = false;
    for filter in activity.children_named("intent-filter") {
        for action in filter.children_named("action") {
            if action
                .android_attr("name")
                .and_then(|a| a.value.as_str())
                .map(|s| s == "android.intent.action.MAIN")
                .unwrap_or(false)
            {
                has_main = true;
            }
        }
        for category in filter.children_named("category") {
            let name = category
                .android_attr("name")
                .and_then(|a| a.value.as_str())
                .unwrap_or_default();
            if name == "android.intent.category.LAUNCHER"
                || name == "android.intent.category.LEANBACK_LAUNCHER"
            {
                has_launcher = true;
            }
        }
    }
    has_main && has_launcher
}

fn load_icon_png(zip: &ZipArchive, path: &str) -> Option<Vec<u8>> {
    let data = zip.read_entry_limited(path, 16 * 1024 * 1024).ok()?;
    if data.len() > 8 && data[..4] == [0x89, b'P', b'N', b'G'] {
        Some(data)
    } else if data.len() > 8 && data[..3] == [0xFF, 0xD8, 0xFF] {
        // JPEG icons exist; keep them as-is (the launcher can display them).
        Some(data)
    } else {
        None
    }
}

/// Try well-known launcher icon locations when the manifest is unhelpful.
fn guess_icon_paths(zip: &ZipArchive) -> Vec<String> {
    let densities = [
        "xxxhdpi", "xxhdpi", "xhdpi", "hdpi", "mdpi", "ldpi", "anydpi",
    ];
    let mut out = Vec::new();
    for d in densities {
        for dir in ["mipmap", "drawable", "mipmap-anydpi-v26"] {
            for name in ["ic_launcher.png", "ic_launcher_round.png", "icon.png", "app_icon.png"] {
                out.push(format!("res/{dir}-{d}/{name}"));
            }
        }
    }
    out.retain(|p| zip.find(p).is_some());
    out
}

/// Extract every PNG stored in a dex/ELF blob to find a launcher icon.
pub fn icon_from_binary(blob: &[u8]) -> Option<Vec<u8>> {
    let found = crate::util::extract_pngs(blob, 64);
    found
        .into_iter()
        .filter(|(_, png)| png.len() > 500)
        .max_by_key(|(_, png)| png.len())
        .map(|(_, png)| png)
}

pub fn best_icon(info: &ApkInfo) -> Vec<u8> {
    info.icon_png
        .clone()
        .unwrap_or_else(|| png::letter_icon(&info.label, &info.package))
}

/// Build a Windows `.ico` file from PNG bytes (PNG-in-ICO, supported since Vista).
pub fn png_to_ico(png: &[u8]) -> Vec<u8> {
    let (w, h) = png_size(png).unwrap_or((64, 64));
    let mut ico = Vec::new();
    // ICONDIR
    ico.extend_from_slice(&0u16.to_le_bytes());
    ico.extend_from_slice(&1u16.to_le_bytes()); // type: icon
    ico.extend_from_slice(&1u16.to_le_bytes()); // image count
    // ICONDIRENTRY
    ico.push(if w >= 256 { 0 } else { w as u8 });
    ico.push(if h >= 256 { 0 } else { h as u8 });
    ico.push(0); // palette
    ico.push(0); // reserved
    ico.extend_from_slice(&1u16.to_le_bytes()); // planes
    ico.extend_from_slice(&32u16.to_le_bytes()); // bpp
    ico.extend_from_slice(&(png.len() as u32).to_le_bytes());
    ico.extend_from_slice(&22u32.to_le_bytes()); // offset
    ico.extend_from_slice(png);
    ico
}

pub fn png_size(png: &[u8]) -> Option<(u32, u32)> {
    if png.len() < 24 || png[..8] != [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A] {
        return None;
    }
    let w = u32::from_be_bytes([png[16], png[17], png[18], png[19]]);
    let h = u32::from_be_bytes([png[20], png[21], png[22], png[23]]);
    Some((w, h))
}

/// Resolve `@string/...`-style values that some manifests carry unresolved.
pub fn resolve_label_with(table: &ResTable, res_id: u32) -> Option<String> {
    table.resolve_string(res_id)
}

/// Configurations used for a given package (for tests/diagnostics).
pub fn best_config(configs: &[(Config, String)]) -> Option<(Config, String)> {
    configs.first().cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name)
    }

    #[test]
    fn parses_test_activity_apk() {
        let info = parse_apk(&fixture("TestActivity.apk")).expect("apk parses");
        assert_eq!(info.package, "tests.androguard");
        assert_eq!(info.label, "TestsAndroguardApplication");
        assert_eq!(info.version_name, "1.0");
        assert_eq!(info.version_code, 1);
        assert_eq!(info.min_sdk, Some(9));
        assert_eq!(
            info.launch_component().as_deref(),
            Some("tests.androguard/tests.androguard.TestActivity")
        );
        assert!(info.icon_png.as_ref().map(|p| p.len() > 100).unwrap_or(false));
    }

    #[test]
    fn parses_third_party_apk() {
        let info = parse_apk(&fixture("a2dp.Vol_137.apk")).expect("apk parses");
        assert_eq!(info.package, "a2dp.Vol");
        assert_eq!(info.label, "A2DP Volume");
        assert_eq!(info.version_name, "2.12.9.2");
        assert_eq!(info.launch_component().as_deref(), Some("a2dp.Vol/a2dp.Vol.main"));
        assert!(info.permissions.iter().any(|p| p.contains("BLUETOOTH")));
    }

    #[test]
    fn parses_debug_apk() {
        let info = parse_apk(&fixture("TC-debug.apk")).expect("apk parses");
        assert_eq!(info.package, "org.t0t0.androguard.TC");
        assert_eq!(info.label, "TCActivity");
        assert_eq!(
            info.launch_component().as_deref(),
            Some("org.t0t0.androguard.TC/org.t0t0.androguard.TC.TCActivity")
        );
    }

    #[test]
    fn ico_wrapper_is_valid() {
        let info = parse_apk(&fixture("TC-debug.apk")).unwrap();
        let ico = png_to_ico(&best_icon(&info));
        assert_eq!(&ico[..4], &[0, 0, 1, 0]);
        assert_eq!(u16::from_le_bytes([ico[4], ico[5]]), 1);
    }
}
