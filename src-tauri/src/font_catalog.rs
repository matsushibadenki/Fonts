use crate::font_paths::Location;
use font_core::font_io;
use serde::Serialize;
use std::{
    collections::BTreeSet,
    fs,
    io::Read,
    path::{Path, PathBuf},
};
use unicode_normalization::UnicodeNormalization;
#[derive(Clone, Serialize)]
pub struct Entry {
    pub id: u64,
    pub family: String,
    pub family_ja: Option<String>,
    pub style_ja: Option<String>,
    #[serde(skip)]
    pub search_text: String,
    pub style: String,
    pub format: String,
    pub cid: bool,
    pub path: PathBuf,
    pub index: u32,
    pub source: String,
    pub glyphs: u16,
    pub variable: bool,
    pub outline: bool,
}
#[derive(Default, Serialize)]
pub struct Catalog {
    pub entries: Vec<Entry>,
    pub locations: Vec<Location>,
    pub skipped: usize,
    pub truncated: bool,
}
pub fn inspect(path: &Path, source: &str) -> Result<Vec<Entry>, String> {
    let data = font_io::read(path)?;
    if !font_core::font_formats::is_sfnt(&data) {
        let info = font_core::font_formats::legacy_info(&data, 0)?;
        let format = if info.cid && data.starts_with(&[1, 0]) {
            "CID-CFF"
        } else {
            font_core::font_formats::identify(&data)
        };
        return Ok(vec![Entry {
            id: 0,
            search_text: search_index(&info.family, &info.style, path, std::iter::empty()),
            family: info.family,
            family_ja: None,
            style_ja: None,
            style: info.style,
            format: format.into(),
            cid: info.cid,
            path: path.into(),
            index: 0,
            source: source.into(),
            glyphs: info.glyphs,
            variable: false,
            outline: info.outline,
        }]);
    }
    let count = ttf_parser::fonts_in_collection(&data).unwrap_or(1);
    if count > 1024 {
        return Err("font_too_large".into());
    }
    let mut entries = vec![];
    for index in 0..count {
        let face = ttf_parser::Face::parse(&data, index).map_err(|_| "font_invalid")?;
        let family = font_io::name(&face, 1);
        let style = font_io::name(&face, 2);
        let search_text = search_index(&family, &style, path, localized_names(&face));
        entries.push(Entry {
            id: 0,
            family: if family.is_empty() {
                path.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into()
            } else {
                family
            },
            style,
            family_ja: japanese_family(face.names()),
            style_ja: japanese_name(face.names(), &[2, 17, 22]),
            search_text,
            format: if face.tables().cff2.is_some() {
                "OpenType CFF2"
            } else if face
                .tables()
                .cff
                .is_some_and(|c| c.glyph_cid(ttf_parser::GlyphId(0)).is_some())
            {
                "OpenType CID-CFF"
            } else if face.tables().cff.is_some() {
                "OpenType CFF"
            } else {
                "TrueType"
            }
            .into(),
            cid: face
                .tables()
                .cff
                .is_some_and(|c| c.glyph_cid(ttf_parser::GlyphId(0)).is_some()),
            path: path.into(),
            index,
            source: source.into(),
            glyphs: face.number_of_glyphs(),
            variable: face.is_variable(),
            outline: face.tables().glyf.is_some()
                || face.tables().cff.is_some()
                || face.tables().cff2.is_some(),
        });
    }
    Ok(entries)
}
/// NFKC folds full-width Latin and half-width kana, including voiced kana.
/// Whitespace-separated terms can match different family/style aliases.
pub fn normalize_query(text: &str) -> String {
    text.nfkc()
        .flat_map(char::to_lowercase)
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}
impl Entry {
    pub fn matches_query(&self, query: &str) -> bool {
        query
            .split_whitespace()
            .all(|term| self.search_text.contains(term))
    }
}
fn decoded_name(name: ttf_parser::name::Name<'_>) -> Option<String> {
    if name.name.len() > 4096 {
        return None;
    }
    if name.is_unicode() {
        return name
            .name
            .len()
            .is_multiple_of(2)
            .then(|| name.to_string())
            .flatten();
    }
    let encoding = match (name.platform_id, name.encoding_id) {
        (ttf_parser::PlatformId::Macintosh, 1) | (ttf_parser::PlatformId::Windows, 2) => {
            encoding_rs::SHIFT_JIS
        }
        (ttf_parser::PlatformId::Macintosh, 0) => encoding_rs::MACINTOSH,
        _ => return None,
    };
    encoding
        .decode_without_bom_handling_and_without_replacement(name.name)
        .map(|s| s.into_owned())
}
/// Match the canonical legacy-family grouping first, then typographic/WWS/full names.
/// Localized labels never replace the canonical family used for exports and previews.
fn japanese_family<'a>(
    names: impl IntoIterator<Item = ttf_parser::name::Name<'a>>,
) -> Option<String> {
    japanese_name(names, &[1, 16, 21, 4, 18])
}
fn japanese_name<'a>(
    names: impl IntoIterator<Item = ttf_parser::name::Name<'a>>,
    ids: &[u16],
) -> Option<String> {
    names
        .into_iter()
        .filter_map(|name| {
            let language_rank = match name.platform_id {
                ttf_parser::PlatformId::Windows if name.language_id == 0x411 => 0,
                ttf_parser::PlatformId::Windows if name.language_id & 0x3ff == 0x11 => 1,
                ttf_parser::PlatformId::Macintosh if name.language_id == 11 => 2,
                _ => return None,
            };
            let name_rank = ids.iter().position(|id| *id == name.name_id)?;
            let decoded = decoded_name(name)?;
            let text = decoded.trim_matches(|c: char| c == '\0' || c.is_whitespace());
            if text.is_empty() || text.chars().any(char::is_control) {
                return None;
            }
            Some(((name_rank, language_rank), text.to_owned()))
        })
        .min_by_key(|(rank, _)| *rank)
        .map(|(_, name)| name)
}

fn localized_names(face: &ttf_parser::Face<'_>) -> Vec<String> {
    let mut records: Vec<_> = face
        .names()
        .into_iter()
        .filter(|n| matches!(n.name_id, 1 | 2 | 4 | 6 | 16 | 17 | 18 | 21 | 22))
        .collect();
    // Keep Japanese aliases even in fonts with many other localized names.
    records.sort_by_key(|n| {
        !((n.platform_id == ttf_parser::PlatformId::Windows && n.language_id & 0x3ff == 0x11)
            || (n.platform_id == ttf_parser::PlatformId::Macintosh && n.language_id == 11))
    });
    records
        .into_iter()
        .filter_map(decoded_name)
        .take(256)
        .collect()
}
fn search_index(
    family: &str,
    style: &str,
    path: &Path,
    names: impl IntoIterator<Item = String>,
) -> String {
    let mut index = normalize_query(&format!("{family}\n{style}\n{}", path.to_string_lossy()));
    let mut seen = BTreeSet::new();
    for name in names {
        let name = normalize_query(name.trim_matches('\0'));
        if name.is_empty() || !seen.insert(name.clone()) {
            continue;
        }
        if index.len() + name.len() > 16384 {
            continue;
        }
        index.push('\n');
        index.push_str(&name);
    }
    index
}

pub fn scan(mut locations: Vec<Location>) -> Catalog {
    let mut output = Catalog::default();
    let mut files = BTreeSet::new();
    let mut dirs = BTreeSet::new();
    let mut visits = 0usize;
    for root in &mut locations {
        let canonical = match fs::canonicalize(&root.path) {
            Ok(p) => p,
            Err(e) => {
                root.status = if e.kind() == std::io::ErrorKind::NotFound {
                    "missing"
                } else {
                    "unreadable"
                }
                .into();
                continue;
            }
        };
        root.status = "ready".into();
        let mut stack = vec![(canonical.clone(), 0usize)];
        while let Some((directory, depth)) = stack.pop() {
            if depth > 16 || !dirs.insert(directory.clone()) {
                continue;
            }
            let listing = match fs::read_dir(directory) {
                Ok(v) => v,
                Err(_) => {
                    root.status = "partial".into();
                    continue;
                }
            };
            for item in listing {
                visits += 1;
                if visits > 100000 || output.entries.len() >= 20000 {
                    output.truncated = true;
                    break;
                }
                let item = match item {
                    Ok(v) => v,
                    Err(_) => {
                        output.skipped += 1;
                        continue;
                    }
                };
                let path = match fs::canonicalize(item.path()) {
                    Ok(v) => v,
                    Err(_) => {
                        output.skipped += 1;
                        continue;
                    }
                };
                // Do not walk links into unrelated directories or Adobe data outside this root.
                if !path.starts_with(&canonical) {
                    continue;
                }
                if path.is_dir() {
                    stack.push((path, depth + 1));
                    continue;
                }
                if !path.is_file() || !files.insert(path.clone()) {
                    continue;
                }
                let mut magic = [0; 4];
                if fs::File::open(&path)
                    .and_then(|mut f| f.read_exact(&mut magic))
                    .is_err()
                {
                    continue;
                }
                if !font_core::font_formats::supported_signature(&magic) {
                    continue;
                }
                match inspect(&path, root.source) {
                    Ok(v) => output.entries.extend(v),
                    Err(_) => output.skipped += 1,
                }
            }
            if output.truncated {
                break;
            }
        }
        if output.truncated {
            root.status = "partial".into();
            break;
        }
    }
    output.entries.sort_by(|a, b| {
        (&a.family, &a.style, &a.path, a.index).cmp(&(&b.family, &b.style, &b.path, b.index))
    });
    output.locations = locations;
    output
}

/// Fontconfig resolves distro/user configuration beyond the conventional directories.
#[cfg(target_os = "linux")]
pub fn configured_locations(roots: &mut Vec<Location>) {
    if let Ok(result) = std::process::Command::new("fc-list")
        .args(["--format", "%{file}\n"])
        .output()
    {
        if result.status.success() && result.stdout.len() < 16 * 1024 * 1024 {
            let parents: BTreeSet<_> = String::from_utf8_lossy(&result.stdout)
                .lines()
                .filter_map(|s| Path::new(s).parent())
                .filter(|p| p.is_absolute())
                .map(Path::to_path_buf)
                .collect();
            for path in parents.into_iter().take(1024) {
                if !roots.iter().any(|r| path.starts_with(&r.path)) {
                    roots.push(Location {
                        source: "configured",
                        path,
                        status: "pending".into(),
                    });
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    /// Replace only the name-table directory entry; no glyph or original name is changed.
    fn font_with_localized_names() -> Vec<u8> {
        let mut bytes =
            font_io::export(&font_core::FontDocument::new("Search Fixture", "A").unwrap()).unwrap();
        let face = ttf_parser::Face::parse(&bytes, 0).unwrap();
        let mut records: Vec<(u16, u16, u16, u16, Vec<u8>)> = face
            .names()
            .into_iter()
            .map(|n| {
                (
                    n.platform_id as u16,
                    n.encoding_id,
                    n.language_id,
                    n.name_id,
                    n.name.to_vec(),
                )
            })
            .collect();
        let utf16 = |s: &str| {
            s.encode_utf16()
                .flat_map(u16::to_be_bytes)
                .collect::<Vec<_>>()
        };
        records.push((3, 1, 0x411, 1, utf16("かな書体")));
        records.push((3, 1, 0x411, 2, utf16("Ｗ３")));
        records.push((3, 1, 0x411, 16, utf16("カナゴシック")));
        records.push((3, 1, 0x411, 4, utf16("かな書体 Ｗ３")));
        records.push((3, 1, 0x804, 1, utf16("简体中文字体")));
        let (legacy, _, errors) = encoding_rs::SHIFT_JIS.encode("旧字体ゴシック");
        assert!(!errors);
        records.push((1, 1, 11, 1, legacy.into_owned()));
        // Broken localized text is ignored, leaving the face and other aliases usable.
        records.push((3, 1, 0x411, 1, vec![0xd8, 0x00]));
        let mut table = Vec::new();
        for n in [0, records.len() as u16, (6 + records.len() * 12) as u16] {
            table.extend(n.to_be_bytes());
        }
        let mut strings = Vec::new();
        for (platform, encoding, language, id, name) in records {
            for n in [
                platform,
                encoding,
                language,
                id,
                name.len() as u16,
                strings.len() as u16,
            ] {
                table.extend(n.to_be_bytes());
            }
            strings.extend(name);
        }
        table.extend(strings);
        let offset = bytes.len() as u32;
        let count = u16::from_be_bytes([bytes[4], bytes[5]]) as usize;
        for i in 0..count {
            let start = 12 + i * 16;
            if &bytes[start..start + 4] == b"name" {
                bytes[start + 8..start + 12].copy_from_slice(&offset.to_be_bytes());
                bytes[start + 12..start + 16].copy_from_slice(&(table.len() as u32).to_be_bytes());
                break;
            }
        }
        bytes.extend(table);
        bytes
    }
    #[test]
    fn localized_aliases_are_searchable_without_changing_display_names_or_font_data() {
        let root =
            std::env::temp_dir().join(format!("fonts-search-test-{}.ttf", std::process::id()));
        let bytes = font_with_localized_names();
        fs::write(&root, &bytes).unwrap();
        let entries = inspect(&root, "file").unwrap();
        let entry = &entries[0];
        assert_eq!(entry.family, "Search Fixture");
        assert_eq!(entry.family_ja.as_deref(), Some("かな書体"));
        assert_eq!(entry.style_ja.as_deref(), Some("Ｗ３"));
        for query in [
            "かな書体",
            "カナゴシック",
            "ｶﾅｺﾞｼｯｸ",
            "かな書体 w3",
            "旧字体",
            "简体中文",
            "ＳＥＡＲＣＨ fixture",
            "  search   fixture  ",
        ] {
            assert!(
                entry.matches_query(&normalize_query(query)),
                "query should match: {query}"
            );
        }
        assert!(!entry.matches_query(&normalize_query("存在しない書体")));
        assert!(!entry.matches_query(&normalize_query("かな書体 Bold")));
        assert_eq!(fs::read(&root).unwrap(), bytes);
        assert!(!serde_json::to_string(entry)
            .unwrap()
            .contains("search_text"));
        fs::remove_file(root).unwrap();
    }
    #[test]
    fn japanese_display_names_fall_back_and_ignore_other_languages_and_broken_records() {
        let english = "English Only"
            .encode_utf16()
            .flat_map(u16::to_be_bytes)
            .collect::<Vec<_>>();
        let chinese = "中文字体"
            .encode_utf16()
            .flat_map(u16::to_be_bytes)
            .collect::<Vec<_>>();
        let japanese = "日本語ファミリー"
            .encode_utf16()
            .flat_map(u16::to_be_bytes)
            .collect::<Vec<_>>();
        let record = |language_id, name_id, name| ttf_parser::name::Name {
            platform_id: ttf_parser::PlatformId::Windows,
            encoding_id: 1,
            language_id,
            name_id,
            name,
        };
        assert_eq!(
            japanese_family([record(0x409, 1, &english), record(0x804, 1, &chinese)]),
            None
        );
        assert_eq!(
            japanese_family([
                record(0x411, 1, &[0xd8, 0x00]),
                record(0x411, 16, &japanese)
            ])
            .as_deref(),
            Some("日本語ファミリー")
        );
        assert_eq!(
            japanese_family([record(0x411, 1, &[0, 32]), record(0x411, 2, &japanese)]),
            None
        );
        let doc = font_core::FontDocument::new("English Only", "A").unwrap();
        let bytes = font_io::export(&doc).unwrap();
        let face = ttf_parser::Face::parse(&bytes, 0).unwrap();
        assert_eq!(japanese_family(face.names()), None);
    }

    #[test]
    fn malformed_legacy_and_odd_utf16_names_are_ignored() {
        let name = |platform, encoding, bytes| ttf_parser::name::Name {
            platform_id: platform,
            encoding_id: encoding,
            language_id: 11,
            name_id: 1,
            name: bytes,
        };
        assert!(decoded_name(name(ttf_parser::PlatformId::Macintosh, 1, &[0x82])).is_none());
        assert!(decoded_name(name(ttf_parser::PlatformId::Windows, 1, &[0x00])).is_none());
    }
    #[test]
    fn normalized_search_preserves_voicing_and_accepts_empty_queries() {
        assert_eq!(normalize_query("  ｶﾞｸ  Ｗ３ "), "ガク w3");
        assert_ne!(normalize_query("カク"), normalize_query("ガク"));
    }
    #[test]
    fn scans_signature_without_extension_and_reports_missing_and_bad_files() {
        let root = std::env::temp_dir().join(format!("fonts-catalog-test-{}", std::process::id()));
        fs::create_dir_all(root.join(".r")).unwrap();
        let font = font_core::FontDocument::new("Catalog Test", "A😀").unwrap();
        let bytes = font_io::export(&font).unwrap();
        fs::write(root.join(".r/12345"), &bytes).unwrap();
        fs::write(root.join("invalid.ttf"), b"OTTOinvalid").unwrap();
        fs::write(root.join("metadata.json"), b"{}").unwrap();
        let result = scan(vec![
            Location {
                source: "adobe",
                path: root.clone(),
                status: "pending".into(),
            },
            Location {
                source: "user",
                path: root.join("missing"),
                status: "pending".into(),
            },
        ]);
        assert_eq!(result.entries.len(), 1);
        assert_eq!(result.entries[0].family, "Catalog Test");
        assert_eq!(result.entries[0].source, "adobe");
        assert_eq!(result.skipped, 1);
        assert_eq!(result.locations[1].status, "missing");
        let _ = fs::remove_dir_all(root);
    }
}
