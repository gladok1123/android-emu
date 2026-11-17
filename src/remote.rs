//! Discovery and download of ready-made Android image catalogs (ISO, raw, QCOW2).
//!
//! `android-emu image catalog` queries a JSON index over HTTPS. The default
//! catalog points at this repository's `catalog/images.json`, so the list can be
//! curated without shipping a new binary. Any compatible catalog URL can be used.

use crate::json;
use std::path::Path;

pub const DEFAULT_CATALOG_URL: &str =
    "https://raw.githubusercontent.com/gladok1123/android-emu/main/catalog/images.json";

#[derive(Debug, Clone)]
pub struct RemoteImage {
    pub name: String,
    pub url: String,
    pub kind: String,
    pub size_bytes: u64,
    pub description: String,
    /// `x86_64` / `x86` (only x86 runs in QEMU system-x86_64).
    pub arch: String,
    /// Approximate RAM requirement in MB.
    pub ram_mb: u32,
    /// Optional SHA-256 for integrity checking (hex, lowercase).
    pub sha256: String,
}

pub fn parse_catalog(text: &str) -> Result<Vec<RemoteImage>, String> {
    let v = json::parse(text).map_err(|e| format!("catalog is not valid JSON: {e}"))?;
    let items = v
        .get("images")
        .and_then(|i| i.as_array())
        .ok_or_else(|| "catalog has no \"images\" array".to_string())?;
    let mut out = Vec::new();
    for item in items {
        let Some(name) = item.get("name").and_then(|x| x.as_str()) else {
            continue;
        };
        let Some(url) = item.get("url").and_then(|x| x.as_str()) else {
            continue;
        };
        out.push(RemoteImage {
            name: name.to_string(),
            url: url.to_string(),
            kind: item
                .get("kind")
                .and_then(|x| x.as_str())
                .unwrap_or("iso")
                .to_string(),
            size_bytes: item.get("sizeBytes").and_then(|x| x.as_u64()).unwrap_or(0),
            description: item
                .get("description")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string(),
            arch: item
                .get("arch")
                .and_then(|x| x.as_str())
                .unwrap_or("x86_64")
                .to_string(),
            ram_mb: item.get("ramMb").and_then(|x| x.as_u32()).unwrap_or(2048),
            sha256: item
                .get("sha256")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string(),
        });
    }
    Ok(out)
}

/// Fetch a catalog with `curl.exe` (bundled with Windows 10 1803+).
pub fn fetch_catalog(url: &str) -> Result<Vec<RemoteImage>, String> {
    let curl = crate::util::which("curl").ok_or_else(|| {
        "curl.exe not found - it ships with Windows 10/11, check your PATH".to_string()
    })?;
    let out = std::process::Command::new(curl)
        .args(["-sL", "--fail", "--max-time", "30", url])
        .output()
        .map_err(|e| format!("could not run curl: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "could not download the catalog (HTTP error, offline?) - {url}"
        ));
    }
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    parse_catalog(&text)
}

/// Download an image into the managed folder and return the local path.
pub fn download_image(image: &RemoteImage, mut progress: impl FnMut(u64)) -> Result<std::path::PathBuf, String> {
    let curl = crate::util::which("curl").ok_or_else(|| "curl.exe not found".to_string())?;
    crate::paths::ensure(&crate::paths::images_dir()).map_err(|e| e.to_string())?;
    let file_name = image
        .url
        .rsplit('/')
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or("android.img");
    let dest = crate::paths::images_dir().join(file_name);
    crate::util::download_with_curl(&curl, &image.url, &dest, |p| progress(p.downloaded))
        .map_err(|e| e.to_string())?;
    Ok(dest)
}

/// Naive content check: an image must be at least a few megabytes and either
/// be an ISO (0x43CD001 at offset 0x8000) or look like a disk image.
pub fn looks_like_disk_image(path: &Path) -> bool {
    use std::io::{Read, Seek, SeekFrom};
    let Ok(mut f) = std::fs::File::open(path) else {
        return false;
    };
    let Ok(meta) = f.metadata() else {
        return false;
    };
    if meta.len() < 4 * 1024 * 1024 {
        return false;
    }
    let mut sig = [0u8; 5];
    if f.seek(SeekFrom::Start(0x8000)).is_ok() && f.read_exact(&mut sig).is_ok() {
        if &sig == b"CD001" {
            return true;
        }
    }
    // MBR / GPT disks start with a boot signature at 0x1FE.
    let _ = f.seek(SeekFrom::Start(0x1FE));
    let mut mbr = [0u8; 2];
    if f.read_exact(&mut mbr).is_ok() && mbr == [0x55, 0xAA] {
        return true;
    }
    // Otherwise trust the extension.
    matches!(
        path.extension().and_then(|e| e.to_str()).map(|e| e.to_lowercase()),
        Some(e) if e == "iso" || e == "img" || e == "qcow2" || e == "raw"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_catalog_documents() {
        let text = r#"{
            "images": [
                {"name":"Android-x86 9.0","url":"https://example.com/a.iso","kind":"iso","sizeBytes":900000000,"ramMb":2048,"description":"classic"},
                {"name":"BlissOS","url":"https://example.com/b.img","kind":"raw","arch":"x86_64"}
            ]
        }"#;
        let images = parse_catalog(text).unwrap();
        assert_eq!(images.len(), 2);
        assert_eq!(images[0].name, "Android-x86 9.0");
        assert_eq!(images[1].kind, "raw");
        assert_eq!(images[1].ram_mb, 2048);
    }

    #[test]
    fn rejects_broken_catalogs() {
        assert!(parse_catalog("not json").is_err());
        assert!(parse_catalog("{\"nope\":1}").is_err());
    }
}
