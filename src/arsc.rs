//! Minimal `resources.arsc` (ResTable) reader.
//!
//! It is enough to resolve the values the launcher needs: the application
//! label and icon, plus arbitrarily deep string references.

use crate::axml::StringPool;
use std::collections::HashMap;
use std::fmt;

#[derive(Debug)]
pub enum ArscError {
    Truncated,
    BadFormat(&'static str),
    NoPackage,
}

impl fmt::Display for ArscError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ArscError::Truncated => write!(f, "resources.arsc is truncated"),
            ArscError::BadFormat(m) => write!(f, "invalid resources.arsc: {m}"),
            ArscError::NoPackage => write!(f, "resources.arsc contains no package"),
        }
    }
}

impl std::error::Error for ArscError {}

const RES_TABLE_TYPE: u16 = 0x0002;
const RES_STRING_POOL_TYPE: u16 = 0x0001;
const RES_TABLE_PACKAGE_TYPE: u16 = 0x0200;
const RES_TABLE_TYPE_TYPE: u16 = 0x0201;

fn le_u16(d: &[u8], at: usize) -> Result<u16, ArscError> {
    d.get(at..at + 2)
        .map(|s| u16::from_le_bytes([s[0], s[1]]))
        .ok_or(ArscError::Truncated)
}

fn le_u32(d: &[u8], at: usize) -> Result<u32, ArscError> {
    d.get(at..at + 4)
        .map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
        .ok_or(ArscError::Truncated)
}

fn le_u8(d: &[u8], at: usize) -> Result<u8, ArscError> {
    d.get(at).copied().ok_or(ArscError::Truncated)
}

/// Decoded resource configuration (only the parts we use for ranking).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Config {
    pub language: String,
    pub region: String,
    pub density: u16,
}

impl Config {
    fn parse(data: &[u8], start: usize, size: usize) -> Config {
        let mut cfg = Config::default();
        let end = start + size;
        if size >= 12 && end <= data.len() {
            let lang = [data[start + 8], data[start + 9]];
            if lang[0] != 0 {
                cfg.language = String::from_utf8_lossy(&lang).to_string();
                let region = [data[start + 10], data[start + 11]];
                if region[0] != 0 {
                    cfg.region = String::from_utf8_lossy(&region).to_string();
                }
            }
        }
        if size >= 16 && end <= data.len() {
            cfg.density = le_u16(data, start + 14).unwrap_or(0);
        }
        cfg
    }

    pub fn is_default(&self) -> bool {
        self.language.is_empty() && self.region.is_empty() && self.density == 0
    }

    /// Higher is better when picking an image.
    fn density_rank(&self) -> i32 {
        match self.density {
            0 => 100,
            120 => 120,
            160 => 160,
            213 => 213,
            240 => 240,
            320 => 320,
            480 => 480,
            640 => 640,
            other => other as i32,
        }
    }
}

#[derive(Debug, Clone)]
pub enum ResValue {
    Str(String),
    File(String),
    Int(u32),
    Bool(bool),
    Color(u32),
}

#[derive(Clone)]
struct TypeChunk {
    type_id: u8,
    config: Config,
    /// entry index -> raw Res_value (type, data)
    entries: HashMap<u32, (u8, u32)>,
}

pub struct ResTable {
    global_strings: StringPool,
    /// (package_id, type_id, entry_index) -> chunks of that type
    types: HashMap<(u8, u8, u32), Vec<TypeChunk>>,
    pub package_names: Vec<String>,
    /// key string pools indexed by package id, for debugging/inspection
    keys: HashMap<u8, StringPool>,
}

impl ResTable {
    pub fn parse(data: &[u8]) -> Result<ResTable, ArscError> {
        if data.len() < 12 {
            return Err(ArscError::Truncated);
        }
        let chunk_type = le_u16(data, 0)?;
        if chunk_type != RES_TABLE_TYPE {
            return Err(ArscError::BadFormat("not a resource table"));
        }
        let header_size = le_u16(data, 2)? as usize;
        let total_size = (le_u32(data, 4)? as usize).min(data.len());

        let mut global_strings = StringPool::default();
        let mut types: HashMap<(u8, u8, u32), Vec<TypeChunk>> = HashMap::new();
        let mut package_names = Vec::new();
        let mut keys = HashMap::new();

        let mut pos = header_size.max(12);
        while pos + 8 <= total_size {
            let ctype = le_u16(data, pos)?;
            let cheader = le_u16(data, pos + 2)? as usize;
            let csize = le_u32(data, pos + 4)? as usize;
            if csize < 8 || pos + csize > data.len() {
                break;
            }
            match ctype {
                RES_STRING_POOL_TYPE => {
                    if let Ok((pool, _)) = StringPool::parse(data, pos) {
                        global_strings = pool;
                    }
                }
                RES_TABLE_PACKAGE_TYPE => {
                    if cheader < 284 {
                        pos += csize;
                        continue;
                    }
                    let pkg_id = le_u32(data, pos + 8)? as u8;
                    let name_units: Vec<u16> = (0..128)
                        .map(|i| le_u16(data, pos + 12 + i * 2).unwrap_or(0))
                        .take_while(|&u| u != 0)
                        .collect();
                    package_names.push(String::from_utf16_lossy(&name_units));
                    let type_strings_off = le_u32(data, pos + 268)? as usize;
                    let key_strings_off = le_u32(data, pos + 276)? as usize;
                    let type_id_offset = if cheader >= 288 {
                        le_u32(data, pos + 284).unwrap_or(0) as u8
                    } else {
                        0
                    };
                    if key_strings_off != 0 {
                        if let Ok((pool, _)) = StringPool::parse(data, pos + key_strings_off) {
                            keys.insert(pkg_id, pool);
                        }
                    }
                    let _ = type_strings_off;
                    parse_package_body(
                        data,
                        pos + cheader,
                        pos + csize,
                        pkg_id,
                        type_id_offset,
                        &mut types,
                    )?;
                }
                _ => {}
            }
            pos += csize;
        }

        Ok(ResTable {
            global_strings,
            types,
            package_names,
            keys,
        })
    }

    /// Resolve a resource id to all of its values (one per matching configuration).
    pub fn resolve_all(&self, res_id: u32) -> Vec<(Config, ResValue)> {
        let pkg = (res_id >> 24) as u8;
        let type_id = ((res_id >> 16) & 0xFF) as u8;
        let entry = res_id & 0xFFFF;
        let mut out = Vec::new();
        if let Some(chunks) = self.types.get(&(pkg, type_id, entry)) {
            for chunk in chunks {
                if let Some((vtype, vdata)) = chunk.entries.get(&entry) {
                    if let Some(value) = self.decode_value(*vtype, *vdata) {
                        out.push((chunk.config.clone(), value));
                    }
                }
            }
        }
        out
    }

    fn decode_value(&self, vtype: u8, vdata: u32) -> Option<ResValue> {
        match vtype {
            0x03 => Some(ResValue::Str(self.global_strings.get(vdata).to_string())),
            0x01 => self
                .resolve_value(vdata)
                .and_then(|(_, v)| Some(v)),
            0x10 | 0x11 => Some(ResValue::Int(vdata)),
            0x12 => Some(ResValue::Bool(vdata != 0)),
            0x1C..=0x1F => Some(ResValue::Color(vdata)),
            _ => None,
        }
    }

    /// One level of `@string/...` style indirection, following a chain of references.
    fn resolve_value(&self, res_id: u32) -> Option<(Config, ResValue)> {
        #[allow(unused_mut)]
        let mut current = res_id;
        for _ in 0..8 {
            let candidates = self.resolve_all(current);
            if candidates.is_empty() {
                return None;
            }
            let (cfg, value) = pick_best(candidates);
            match &value {
                ResValue::Str(_) | ResValue::Int(_) | ResValue::Bool(_) | ResValue::Color(_) => {
                    return Some((cfg, value))
                }
                ResValue::File(_) => return Some((cfg, value)),
            }
        }
        None
    }

    /// Resolve to a human readable string (label, version, etc.).
    pub fn resolve_string(&self, res_id: u32) -> Option<String> {
        let (_, value) = self.resolve_value(res_id)?;
        match value {
            ResValue::Str(s) => Some(s),
            ResValue::Int(v) => Some(v.to_string()),
            ResValue::Bool(b) => Some(b.to_string()),
            ResValue::File(f) => Some(f),
            ResValue::Color(c) => Some(format!("#{c:08X}")),
        }
    }

    /// Resolve to a file path inside the APK (for icons).
    pub fn resolve_path(&self, res_id: u32) -> Option<(Config, String)> {
        let mut current = res_id;
        for _ in 0..4 {
            let candidates = self.resolve_all(current);
            if candidates.is_empty() {
                return None;
            }
            // Prefer entries that point to a file, choosing the densest image.
            let mut files: Vec<(Config, String)> = Vec::new();
            for (cfg, v) in &candidates {
                match v {
                    ResValue::File(f) => files.push((cfg.clone(), f.clone())),
                    ResValue::Int(v) => current = *v,
                    ResValue::Str(s) if s.starts_with("res/") => {
                        files.push((cfg.clone(), s.clone()))
                    }
                    _ => {}
                }
            }
            if !files.is_empty() {
                files.sort_by_key(|(cfg, _)| -cfg.density_rank());
                return files.into_iter().next();
            }
        }
        None
    }
}

fn pick_best(mut candidates: Vec<(Config, ResValue)>) -> (Config, ResValue) {
    candidates.sort_by_key(|(cfg, _)| {
        let mut score = cfg.density_rank();
        if !cfg.language.is_empty() {
            // Prefer the default locale unless it is English.
            score -= if cfg.language == "en" { 10 } else { 200 };
        }
        -score
    });
    candidates.remove(0)
}

fn parse_package_body(
    data: &[u8],
    start: usize,
    end: usize,
    pkg_id: u8,
    type_id_offset: u8,
    types: &mut HashMap<(u8, u8, u32), Vec<TypeChunk>>,
) -> Result<(), ArscError> {
    let mut pos = start;
    while pos + 8 <= end {
        let ctype = le_u16(data, pos)?;
        let cheader = le_u16(data, pos + 2)? as usize;
        let csize = le_u32(data, pos + 4)? as usize;
        if csize < 8 || pos + csize > data.len() {
            break;
        }
        if ctype == RES_TABLE_TYPE_TYPE && cheader >= 20 + 4 {
            let type_id = le_u8(data, pos + 8)?;
            let entry_count = le_u32(data, pos + 12)? as usize;
            let entries_start = le_u32(data, pos + 16)? as usize;
            let config_size = le_u32(data, pos + 20)? as usize;
            let config = Config::parse(data, pos + 20, config_size.min(csize));
            let offsets_at = pos + cheader;
            let mut entries = HashMap::new();
            for i in 0..entry_count {
                let off_at = offsets_at + i * 4;
                if off_at + 4 > end {
                    break;
                }
                let entry_off = le_u32(data, off_at)?;
                if entry_off == 0xFFFF_FFFF {
                    continue;
                }
                let e = pos + entries_start + entry_off as usize;
                if e + 8 > end {
                    continue;
                }
                let esize = le_u16(data, e)? as usize;
                let flags = le_u16(data, e + 2)?;
                if flags & 0x0001 != 0 {
                    // Complex (bag) entry: not a simple value.
                    continue;
                }
                let v = e + esize.max(8);
                if v + 8 > end {
                    continue;
                }
                let vtype = le_u8(data, v + 3)?;
                let vdata = le_u32(data, v + 4)?;
                entries.insert(i as u32, (vtype, vdata));
            }
            let real_type = type_id.wrapping_add(type_id_offset);
            for (entry_idx, value) in entries {
                types
                    .entry((pkg_id, real_type, entry_idx))
                    .or_default()
                    .push(TypeChunk {
                        type_id: real_type,
                        config: config.clone(),
                        entries: HashMap::from([(entry_idx, value)]),
                    });
            }
        }
        pos += csize;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_app_label_of_fixture() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/resources.arsc");
        if let Ok(data) = std::fs::read(path) {
            let table = ResTable::parse(&data).expect("arsc parses");
            assert!(!table.package_names.is_empty());
            // Label reference from the fixture manifest: 0x7f040001.
            let label = table.resolve_string(0x7f04_0001).unwrap();
            assert_eq!(label, "TestsAndroguardApplication");
            // Icon reference: 0x7f020000 (drawable/icon).
            let icon = table.resolve_path(0x7f02_0000).unwrap();
            assert!(icon.1.ends_with(".png"), "unexpected icon path {}", icon.1);
        }
    }
}
