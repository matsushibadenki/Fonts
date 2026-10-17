//! Parses CMap data operators only. PostScript is never evaluated.
use crate::{font_io::Result, FontDocument, Glyph};
use std::collections::{BTreeMap, BTreeSet};
pub type Mapping = BTreeMap<u32, BTreeSet<char>>;
fn tokens(input: &str) -> Result<Vec<String>> {
    if input.len() > 4 * 1024 * 1024 {
        return Err("font_too_large".into());
    }
    let b = input.as_bytes();
    let mut at = 0;
    let mut out = vec![];
    while at < b.len() {
        if b[at].is_ascii_whitespace() {
            at += 1;
            continue;
        }
        if b[at] == b'%' {
            while at < b.len() && b[at] != b'\n' {
                at += 1;
            }
            continue;
        }
        if (b[at] == b'<' && b.get(at + 1) == Some(&b'<'))
            || (b[at] == b'>' && b.get(at + 1) == Some(&b'>'))
        {
            out.push(input[at..at + 2].into());
            at += 2;
            continue;
        }
        if b[at] == b'(' {
            let mut depth = 1;
            at += 1;
            while at < b.len() && depth > 0 {
                if b[at] == b'\\' {
                    at += 2;
                    continue;
                }
                if b[at] == b'(' {
                    depth += 1;
                }
                if b[at] == b')' {
                    depth -= 1;
                }
                at += 1;
                if depth > 32 {
                    return Err("font_cmap_invalid".into());
                }
            }
            if depth != 0 {
                return Err("font_cmap_invalid".into());
            }
            out.push("(string)".into());
            continue;
        }
        if b[at] == b'<' && b.get(at + 1) != Some(&b'<') {
            let start = at;
            at += 1;
            while at < b.len() && b[at] != b'>' {
                at += 1;
            }
            if at == b.len() {
                return Err("font_cmap_invalid".into());
            }
            at += 1;
            out.push(input[start..at].into());
        } else if matches!(b[at], b'[' | b']') {
            out.push((b[at] as char).to_string());
            at += 1;
        } else {
            let start = at;
            at += 1;
            while at < b.len()
                && !b[at].is_ascii_whitespace()
                && !matches!(b[at], b'[' | b']' | b'%' | b'<')
            {
                at += 1;
            }
            out.push(input[start..at].into());
        }
        if out.len() > 1_000_000 {
            return Err("font_too_large".into());
        }
    }
    Ok(out)
}
fn hex(s: &str) -> Result<Vec<u8>> {
    let s = s
        .strip_prefix('<')
        .and_then(|s| s.strip_suffix('>'))
        .ok_or("font_cmap_invalid")?;
    let s: String = s.chars().filter(|c| !c.is_ascii_whitespace()).collect();
    if s.is_empty()
        || !s.len().is_multiple_of(2)
        || s.len() > 16
        || !s.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err("font_cmap_invalid".into());
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(|_| "font_cmap_invalid".into()))
        .collect()
}
fn value(s: &str) -> Result<u32> {
    let b = hex(s)?;
    if b.len() > 4 {
        return Err("font_cmap_invalid".into());
    }
    Ok(b.into_iter().fold(0, |n, b| n * 256 + u32::from(b)))
}
fn unicode_hex(s: &str) -> Result<char> {
    let b = hex(s)?;
    if b.len() % 2 != 0 {
        return Err("font_cmap_invalid".into());
    }
    let u: Vec<u16> = b
        .chunks(2)
        .map(|p| u16::from_be_bytes([p[0], p[1]]))
        .collect();
    let s = String::from_utf16(&u).map_err(|_| "font_cmap_invalid")?;
    let mut chars = s.chars();
    let c = chars.next().ok_or("font_cmap_invalid")?;
    if chars.next().is_some() {
        return Err("font_cmap_sequence_unsupported".into());
    }
    Ok(c)
}
fn unicode_code(s: &str) -> Result<char> {
    let n = value(s)?;
    char::from_u32(n)
        .or_else(|| unicode_hex(s).ok())
        .ok_or("font_cmap_invalid".into())
}
fn cid(s: &str) -> Result<u32> {
    let n = s.parse::<u32>().map_err(|_| "font_cmap_invalid")?;
    if n == 0 || n > 65535 {
        return Err("font_cid_invalid".into());
    }
    Ok(n)
}
pub fn parse(input: &str) -> Result<Mapping> {
    let tokens = tokens(input)?;
    if tokens.iter().any(|s| s == "usecmap") {
        return Err("font_cmap_inheritance_unsupported".into());
    }
    if tokens
        .iter()
        .any(|t| t == "begincidchar" || t == "begincidrange")
    {
        if let Some(i) = tokens.iter().position(|t| t == "/CMapName") {
            let name = tokens
                .get(i + 1)
                .ok_or("font_cmap_invalid")?
                .to_ascii_uppercase();
            if !["UTF32", "UTF16", "UCS2", "UCS4", "UNICODE"]
                .iter()
                .any(|encoding| name.contains(encoding))
            {
                return Err("font_cmap_encoding_unsupported".into());
            }
        }
    }
    let mut result: Mapping = BTreeMap::new();
    let mut assigned = BTreeMap::new();
    let mut i = 0;
    let mut insert = |cid: u32, c: char| -> Result<()> {
        if cid == 0 || cid > 65535 {
            return Err("font_cid_invalid".into());
        }
        if assigned.insert(c, cid).is_some_and(|old| old != cid) {
            return Err("font_cmap_invalid".into());
        }
        result.entry(cid).or_default().insert(c);
        if assigned.len() > 1_000_000 {
            return Err("font_too_large".into());
        }
        Ok(())
    };
    while i < tokens.len() {
        let op = &tokens[i];
        let (fields, end) = match op.as_str() {
            "begincidchar" => (2, "endcidchar"),
            "begincidrange" => (3, "endcidrange"),
            "beginbfchar" => (2, "endbfchar"),
            "beginbfrange" => (3, "endbfrange"),
            _ => {
                i += 1;
                continue;
            }
        };
        let count = tokens
            .get(i.wrapping_sub(1))
            .and_then(|s| s.parse::<usize>().ok())
            .filter(|n| *n <= 65535)
            .ok_or("font_cmap_invalid")?;
        i += 1;
        for _ in 0..count {
            let data = tokens.get(i..i + fields).ok_or("font_cmap_invalid")?;
            match op.as_str() {
                "begincidchar" => {
                    insert(cid(&data[1])?, unicode_code(&data[0])?)?;
                    i += 2;
                }
                "beginbfchar" => {
                    insert(value(&data[0])?, unicode_hex(&data[1])?)?;
                    i += 2;
                }
                "begincidrange" => {
                    let start = value(&data[0])?;
                    let last = value(&data[1])?;
                    let first = cid(&data[2])?;
                    if last < start || last - start > 65535 {
                        return Err("font_cmap_invalid".into());
                    }
                    for delta in 0..=last - start {
                        insert(
                            first + delta,
                            char::from_u32(start + delta).ok_or("font_cmap_invalid")?,
                        )?;
                    }
                    i += 3;
                }
                "beginbfrange" => {
                    let first = value(&data[0])?;
                    let last = value(&data[1])?;
                    if last < first || last - first > 65535 {
                        return Err("font_cmap_invalid".into());
                    }
                    let dest = data[2].clone();
                    i += 3;
                    if dest == "[" {
                        for id in first..=last {
                            insert(id, unicode_hex(tokens.get(i).ok_or("font_cmap_invalid")?)?)?;
                            i += 1;
                        }
                        if tokens.get(i).map(String::as_str) != Some("]") {
                            return Err("font_cmap_invalid".into());
                        }
                        i += 1;
                    } else {
                        let n = value(&dest)?;
                        let len = hex(&dest)?.len() * 2;
                        for delta in 0..=last - first {
                            let n = n.checked_add(delta).ok_or("font_cmap_invalid")?;
                            insert(first + delta, unicode_hex(&format!("<{n:0len$X}>"))?)?;
                        }
                    }
                }
                _ => unreachable!(),
            }
        }
        if tokens.get(i).map(String::as_str) != Some(end) {
            return Err("font_cmap_invalid".into());
        }
        i += 1;
    }
    if result.is_empty() {
        return Err("font_cmap_invalid".into());
    }
    Ok(result)
}
pub fn apply(doc: &FontDocument, mapping: &Mapping) -> Result<Vec<Glyph>> {
    let ids = super::postscript::cids(doc)?;
    let lookup: BTreeMap<_, _> = doc
        .glyphs
        .values()
        .zip(ids)
        .map(|(g, cid)| (u32::from(cid), g.id))
        .collect();
    if mapping
        .keys()
        .any(|cid| !lookup.contains_key(cid) || *cid == 0)
    {
        return Err("font_cmap_missing_cid".into());
    }
    let assigned: BTreeSet<_> = mapping.values().flat_map(|v| v.iter().copied()).collect();
    let mut candidate = doc.clone();
    for g in candidate.glyphs.values_mut() {
        g.codepoints.retain(|c| !assigned.contains(c));
    }
    for (cid, codes) in mapping {
        candidate
            .glyphs
            .get_mut(&lookup[cid])
            .unwrap()
            .codepoints
            .extend(codes);
    }
    candidate.validate().map_err(|e| e.to_string())?;
    Ok(candidate.glyphs.into_values().collect())
}
pub fn export(doc: &FontDocument) -> Result<Vec<u8>> {
    let ids = super::postscript::cids(doc)?;
    let mut rows = vec![];
    for (g, cid) in doc.glyphs.values().zip(ids) {
        for c in &g.codepoints {
            rows.push((*c as u32, cid));
        }
    }
    rows.sort_unstable();
    let system = doc.metadata.cid_system.clone().unwrap_or(crate::CidSystem {
        registry: "Adobe".into(),
        ordering: "Identity".into(),
        supplement: 0,
    });
    let mut text=format!("%!PS-Adobe-3.0 Resource-CMap\n/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n/CIDSystemInfo << /Registry ({}) /Ordering ({}) /Supplement {} >> def\n/CMapName /Fonts-UTF32-H def\n/CMapType 1 def\n/WMode 0 def\n1 begincodespacerange\n<00000000> <0010FFFF>\nendcodespacerange\n",super::postscript::ps_string(&system.registry),super::postscript::ps_string(&system.ordering),system.supplement);
    for chunk in rows.chunks(100) {
        text.push_str(&format!("{} begincidchar\n", chunk.len()));
        for (c, id) in chunk {
            text.push_str(&format!("<{c:08X}> {id}\n"));
        }
        text.push_str("endcidchar\n");
    }
    text.push_str("endcmap\nCMapName currentdict /CMap defineresource pop\nend end\n%%EOF\n");
    Ok(text.into_bytes())
}
