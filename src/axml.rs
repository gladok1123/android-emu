//! Parser for Android's binary XML format (AXML), used by `AndroidManifest.xml`
//! and every other compiled XML inside an APK.

use std::fmt;

#[derive(Debug)]
pub enum AxmlError {
    Truncated,
    BadHeader(&'static str),
    BadChunk(u16),
    BadStringPool,
}

impl fmt::Display for AxmlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AxmlError::Truncated => write!(f, "binary XML is truncated"),
            AxmlError::BadHeader(m) => write!(f, "invalid binary XML header: {m}"),
            AxmlError::BadChunk(t) => write!(f, "unsupported binary XML chunk 0x{t:04x}"),
            AxmlError::BadStringPool => write!(f, "invalid string pool in binary XML"),
        }
    }
}

impl std::error::Error for AxmlError {}

const RES_XML_TYPE: u16 = 0x0003;
const RES_STRING_POOL_TYPE: u16 = 0x0001;
const RES_XML_RESOURCE_MAP_TYPE: u16 = 0x0180;
const RES_XML_START_NAMESPACE_TYPE: u16 = 0x0100;
const RES_XML_END_NAMESPACE_TYPE: u16 = 0x0101;
const RES_XML_START_ELEMENT_TYPE: u16 = 0x0102;
const RES_XML_END_ELEMENT_TYPE: u16 = 0x0103;
const RES_XML_CDATA_TYPE: u16 = 0x0104;

const TYPE_REFERENCE: u8 = 0x01;
const TYPE_ATTRIBUTE: u8 = 0x02;
const TYPE_STRING: u8 = 0x03;
const TYPE_FLOAT: u8 = 0x04;
const TYPE_DIMENSION: u8 = 0x05;
const TYPE_FRACTION: u8 = 0x06;
const TYPE_DYNAMIC_REFERENCE: u8 = 0x07;
const TYPE_INT_DEC: u8 = 0x10;
const TYPE_INT_HEX: u8 = 0x11;
const TYPE_INT_BOOLEAN: u8 = 0x12;

const ANDROID_NS: &str = "http://schemas.android.com/apk/res/android";

/// String pool as found in AXML and ARSC files.
#[derive(Debug, Clone, Default)]
pub struct StringPool {
    strings: Vec<String>,
}

impl StringPool {
    pub fn len(&self) -> usize {
        self.strings.len()
    }

    pub fn is_empty(&self) -> bool {
        self.strings.is_empty()
    }

    pub fn get(&self, idx: u32) -> &str {
        self.strings
            .get(idx as usize)
            .map(|s| s.as_str())
            .unwrap_or("")
    }

    pub fn get_opt(&self, idx: u32) -> Option<&str> {
        self.strings.get(idx as usize).map(|s| s.as_str())
    }

    /// Parse a `RES_STRING_POOL_TYPE` chunk that starts at `off`.
    /// Returns the pool and the total chunk size.
    pub fn parse(data: &[u8], off: usize) -> Result<(StringPool, usize), AxmlError> {
        let chunk_type = le_u16(data, off)?;
        if chunk_type != RES_STRING_POOL_TYPE {
            return Err(AxmlError::BadStringPool);
        }
        let header_size = le_u16(data, off + 2)? as usize;
        let chunk_size = le_u32(data, off + 4)? as usize;
        if header_size < 28 || off + chunk_size > data.len() || chunk_size < header_size {
            return Err(AxmlError::BadStringPool);
        }
        let string_count = le_u32(data, off + 8)? as usize;
        let style_count = le_u32(data, off + 12)? as usize;
        let flags = le_u32(data, off + 16)?;
        let strings_start = le_u32(data, off + 20)? as usize;
        let _styles_start = le_u32(data, off + 24)? as usize;
        let utf8 = flags & 0x100 != 0;

        let offsets_base = off + header_size;
        let strings_base = off + strings_start;
        let chunk_end = off + chunk_size;

        let mut strings = Vec::with_capacity(string_count);
        for i in 0..string_count {
            let off_pos = offsets_base + i * 4;
            if off_pos + 4 > data.len() {
                return Err(AxmlError::BadStringPool);
            }
            let str_off = le_u32(data, off_pos)? as usize;
            let mut p = strings_base + str_off;
            if p >= chunk_end {
                strings.push(String::new());
                continue;
            }
            if utf8 {
                // Two lengths (UTF-16 units, then byte count), each 1-2 bytes.
                let (_utf16_len, adv) = decode_len8(data, p)?;
                p += adv;
                let (byte_len, adv2) = decode_len8(data, p)?;
                p += adv2;
                let end = (p + byte_len).min(chunk_end);
                strings.push(String::from_utf8_lossy(&data[p..end]).to_string());
            } else {
                let (char_len, adv) = decode_len16(data, p)?;
                p += adv;
                let end = (p + char_len * 2).min(chunk_end);
                let mut units = Vec::with_capacity(char_len);
                let mut q = p;
                while q + 2 <= end {
                    units.push(u16::from_le_bytes([data[q], data[q + 1]]));
                    q += 2;
                }
                strings.push(String::from_utf16_lossy(&units));
            }
        }
        let _ = style_count;
        Ok((StringPool { strings }, chunk_size))
    }
}

fn decode_len8(data: &[u8], at: usize) -> Result<(usize, usize), AxmlError> {
    let b = *data.get(at).ok_or(AxmlError::Truncated)?;
    if b & 0x80 != 0 {
        let b2 = *data.get(at + 1).ok_or(AxmlError::Truncated)?;
        Ok(((((b & 0x7F) as usize) << 8) | b2 as usize, 2))
    } else {
        Ok((b as usize, 1))
    }
}

fn decode_len16(data: &[u8], at: usize) -> Result<(usize, usize), AxmlError> {
    let v = le_u16(data, at)?;
    if v & 0x8000 != 0 {
        let v2 = le_u16(data, at + 2)?;
        Ok(((((v & 0x7FFF) as usize) << 16) | v2 as usize, 4))
    } else {
        Ok((v as usize, 2))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum AttrValue {
    Str(String),
    Int(i32),
    Bool(bool),
    /// Resource reference (`@0x7f...`), not resolved.
    Reference(u32),
    /// Raw typed value we do not interpret.
    Other(String),
}

impl AttrValue {
    pub fn as_str(&self) -> Option<&str> {
        match self {
            AttrValue::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_u32(&self) -> Option<u32> {
        match self {
            AttrValue::Int(v) => Some(*v as u32),
            AttrValue::Reference(v) => Some(*v),
            AttrValue::Bool(b) => Some(*b as u32),
            _ => None,
        }
    }

    pub fn to_display(&self) -> String {
        match self {
            AttrValue::Str(s) => s.clone(),
            AttrValue::Int(v) => v.to_string(),
            AttrValue::Bool(b) => b.to_string(),
            AttrValue::Reference(v) => format!("@{v:08X}"),
            AttrValue::Other(s) => s.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Attribute {
    /// Namespace URI (empty when the attribute is not namespaced).
    pub namespace: String,
    /// Attribute name as stored in the string pool, e.g. `label`.
    pub name: String,
    /// Resolved Android resource id when the attribute is a known framework
    /// attribute (taken from the resource map), e.g. `0x01010001` for `label`.
    pub resource_id: Option<u32>,
    pub value: AttrValue,
}

impl Attribute {
    /// Canonical Android attribute key, e.g. `android:label` or `package`.
    pub fn key(&self) -> String {
        if self.namespace == ANDROID_NS {
            format!("android:{}", self.name)
        } else {
            self.name.clone()
        }
    }
}

#[derive(Debug, Clone)]
pub struct Element {
    pub tag: String,
    pub attrs: Vec<Attribute>,
    pub children: Vec<Element>,
    pub text: String,
}

impl Element {
    pub fn attr(&self, name: &str) -> Option<&Attribute> {
        self.attrs.iter().find(|a| a.name == name || a.key() == name)
    }

    /// Look up `android:<name>` (namespace-aware) or a plain attribute.
    pub fn android_attr(&self, name: &str) -> Option<&Attribute> {
        self.attrs
            .iter()
            .find(|a| a.namespace == ANDROID_NS && a.name == name)
            .or_else(|| {
                // Some obfuscated APKs drop the namespace; fall back to the raw name.
                self.attrs.iter().find(|a| a.name == name && a.namespace.is_empty())
            })
    }

    pub fn android_str(&self, name: &str) -> Option<String> {
        self.android_attr(name).map(|a| a.value.to_display())
    }

    pub fn children_named<'a>(&'a self, tag: &'a str) -> impl Iterator<Item = &'a Element> {
        self.children.iter().filter(move |c| c.tag == tag)
    }

    pub fn find_child(&self, tag: &str) -> Option<&Element> {
        self.children.iter().find(|c| c.tag == tag)
    }

    /// Depth-first search for the first element with the given tag.
    pub fn find_descendant(&self, tag: &str) -> Option<&Element> {
        for child in &self.children {
            if child.tag == tag {
                return Some(child);
            }
            if let Some(found) = child.find_descendant(tag) {
                return Some(found);
            }
        }
        None
    }

    pub fn descendants<'a>(&'a self, out: &mut Vec<&'a Element>) {
        for child in &self.children {
            out.push(child);
            child.descendants(out);
        }
    }
}

/// Parse an AXML document and return its root element.
pub fn parse(data: &[u8]) -> Result<Element, AxmlError> {
    let mut root = Element {
        tag: String::from("#document"),
        attrs: Vec::new(),
        children: Vec::new(),
        text: String::new(),
    };
    let mut stack: Vec<Element> = Vec::new();
    let mut pool = StringPool::default();
    let mut resource_map: Vec<u32> = Vec::new();

    if data.len() < 8 {
        return Err(AxmlError::Truncated);
    }
    let file_type = le_u16(data, 0)?;
    let header_size = le_u16(data, 2)? as usize;
    let file_size = le_u32(data, 4)? as usize;
    if header_size != 8 {
        return Err(AxmlError::BadHeader("chunk header size is not 8"));
    }
    if file_type != RES_XML_TYPE {
        // Tolerated: some packers set a bogus type.
    }
    let end = file_size.min(data.len()).max(8);
    let mut pos = header_size;

    while pos + 8 <= end {
        let chunk_type = le_u16(data, pos)?;
        let chunk_header_size = le_u16(data, pos + 2)? as usize;
        let chunk_size = le_u32(data, pos + 4)? as usize;
        if chunk_size < 8 || pos + chunk_size > data.len() {
            break;
        }
        match chunk_type {
            RES_STRING_POOL_TYPE => {
                let (p, _) = StringPool::parse(data, pos)?;
                pool = p;
            }
            RES_XML_RESOURCE_MAP_TYPE => {
                let count = (chunk_size - chunk_header_size) / 4;
                resource_map.clear();
                for i in 0..count {
                    resource_map.push(le_u32(data, pos + chunk_header_size + i * 4)?);
                }
            }
            RES_XML_START_ELEMENT_TYPE => {
                // Chunk header(8) + line(4) + comment(4) + ns(4) + name(4) + attrStart(2)
                // + attrSize(2) + attrCount(4) + idAttr(4) + classAttr(4)
                let base = pos + 16;
                let name_idx = le_u32(data, base + 4)?;
                let attr_start = le_u16(data, base + 8)? as usize;
                let attr_size = le_u16(data, base + 10)? as usize;
                let attr_count = le_u16(data, base + 12)? as usize;
                let tag = pool.get(name_idx).to_string();
                let mut attrs = Vec::with_capacity(attr_count);
                // `attributeStart` is relative to the beginning of the
                // ResXMLTree_attrExt structure (chunk header + line + comment).
                let attrs_base = base + attr_start;
                for i in 0..attr_count {
                    let a = attrs_base + i * attr_size.max(20);
                    if a + 20 > data.len() {
                        break;
                    }
                    let ns_idx = le_u32(data, a)?;
                    let attr_name_idx = le_u32(data, a + 4)?;
                    let raw_value_idx = le_u32(data, a + 8)?;
                    let value_type = data[a + 15];
                    let value_data = le_u32(data, a + 16)?;
                    let namespace = pool.get(ns_idx).to_string();
                    let name = pool.get(attr_name_idx).to_string();
                    let resource_id = if attr_name_idx < resource_map.len() as u32 {
                        match resource_map[attr_name_idx as usize] {
                            0 => None,
                            v => Some(v),
                        }
                    } else {
                        None
                    };
                    let value = match value_type {
                        TYPE_STRING => AttrValue::Str(pool.get(value_data).to_string()),
                        TYPE_REFERENCE | TYPE_DYNAMIC_REFERENCE => AttrValue::Reference(value_data),
                        TYPE_ATTRIBUTE => AttrValue::Other(format!("?{value_data:08X}")),
                        TYPE_INT_DEC => AttrValue::Int(value_data as i32),
                        TYPE_INT_HEX => AttrValue::Int(value_data as i32),
                        TYPE_INT_BOOLEAN => AttrValue::Bool(value_data != 0),
                        TYPE_FLOAT => AttrValue::Other(format!(
                            "{}",
                            f32::from_bits(value_data)
                        )),
                        TYPE_DIMENSION | TYPE_FRACTION => {
                            AttrValue::Other(complex_to_string(value_data))
                        }
                        _ => {
                            if raw_value_idx != 0xFFFF_FFFF {
                                if let Some(s) = pool.get_opt(raw_value_idx) {
                                    AttrValue::Str(s.to_string())
                                } else {
                                    AttrValue::Other(format!("0x{value_data:08X}"))
                                }
                            } else {
                                AttrValue::Other(format!("0x{value_data:08X}"))
                            }
                        }
                    };
                    attrs.push(Attribute {
                        namespace,
                        name,
                        resource_id,
                        value,
                    });
                }
                stack.push(Element {
                    tag,
                    attrs,
                    children: Vec::new(),
                    text: String::new(),
                });
            }
            RES_XML_END_ELEMENT_TYPE => {
                if let Some(el) = stack.pop() {
                    match stack.last_mut() {
                        Some(parent) => parent.children.push(el),
                        None => root.children.push(el),
                    }
                }
            }
            RES_XML_CDATA_TYPE => {
                let base = pos + 16;
                let data_idx = le_u32(data, base)?;
                if let Some(top) = stack.last_mut() {
                    top.text.push_str(pool.get(data_idx));
                }
            }
            RES_XML_START_NAMESPACE_TYPE | RES_XML_END_NAMESPACE_TYPE => {}
            other => {
                // Unknown chunk: skip it rather than failing on packer oddities.
                let _ = other;
            }
        }
        pos += chunk_size;
    }

    // Unclosed elements (truncated files) are attached to the root so that
    // partially readable manifests still yield useful information.
    while let Some(el) = stack.pop() {
        match stack.last_mut() {
            Some(parent) => parent.children.push(el),
            None => root.children.push(el),
        }
    }
    Ok(root)
}

fn complex_to_string(v: u32) -> String {
    const RADIX: [f32; 4] = [1.0 / (1 << 8) as f32, 1.0 / (1 << 15) as f32, 1.0 / (1 << 23) as f32, 1.0 / (1 << 31) as f32];
    let mantissa = (v & 0xFFFF_FF00) as f32;
    let unit = (v >> 4) & 3;
    let val = mantissa * RADIX[unit as usize];
    format!("{val}")
}

fn le_u16(d: &[u8], at: usize) -> Result<u16, AxmlError> {
    d.get(at..at + 2)
        .map(|s| u16::from_le_bytes([s[0], s[1]]))
        .ok_or(AxmlError::Truncated)
}

fn le_u32(d: &[u8], at: usize) -> Result<u32, AxmlError> {
    d.get(at..at + 4)
        .map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
        .ok_or(AxmlError::Truncated)
}

/// Well-known framework attribute resource ids (subset we care about).
pub mod attr_ids {
    pub const LABEL: u32 = 0x0101_0001;
    pub const ICON: u32 = 0x0101_0002;
    pub const NAME: u32 = 0x0101_0003;
    pub const VERSION_CODE: u32 = 0x0101_021b;
    pub const VERSION_NAME: u32 = 0x0101_021c;
    pub const DEBUGGABLE: u32 = 0x0101_000f;
    pub const MIN_SDK_VERSION: u32 = 0x0101_020c;
    pub const TARGET_SDK_VERSION: u32 = 0x0101_021d;
    pub const ROUND_ICON: u32 = 0x0101_053e;
    pub const ALLOW_BACKUP: u32 = 0x0101_0280;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_manifest_of_fixture() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/manifest.axml");
        if let Ok(data) = std::fs::read(path) {
            let root = parse(&data).expect("manifest parses");
            let manifest = &root.children[0];
            assert_eq!(manifest.tag, "manifest");
            assert_eq!(
                manifest.attr("package").and_then(|a| a.value.as_str()),
                Some("tests.androguard")
            );
            assert!(manifest.find_descendant("activity").is_some());
        }
    }
}
