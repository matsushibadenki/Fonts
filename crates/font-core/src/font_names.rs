//! CFF1 name repair without changing outline programs or their relative offsets.
use crate::font_io::Result;
use serde::Serialize;
use ttf_parser::{Face, Tag};
#[derive(Serialize)]
pub struct Inspection {
    pub postscript: String,
    pub cff: String,
    pub mismatch: bool,
}
pub fn postscript_name(doc: &crate::FontDocument) -> String {
    let clean = |s: &str, limit| {
        s.chars()
            .filter(char::is_ascii_alphanumeric)
            .take(limit)
            .collect::<String>()
    };
    let family = clean(&doc.family, 50);
    let family = if family.is_empty() {
        "FontsExport".into()
    } else {
        family
    };
    let style = clean(
        if doc.metadata.style.is_empty() {
            "Regular"
        } else {
            &doc.metadata.style
        },
        40,
    );
    format!("{family}-{style}")
}
fn index(data: &[u8], at: &mut usize) -> Result<Vec<Vec<u8>>> {
    let count = u16::from_be_bytes(
        data.get(*at..*at + 2)
            .ok_or("name_invalid")?
            .try_into()
            .unwrap(),
    ) as usize;
    *at += 2;
    if count == 0 {
        return Ok(vec![]);
    }
    let size = *data.get(*at).ok_or("name_invalid")? as usize;
    *at += 1;
    if !(1..=4).contains(&size) {
        return Err("name_invalid".into());
    }
    let mut offsets = Vec::new();
    for _ in 0..=count {
        let mut n = 0usize;
        for b in data.get(*at..*at + size).ok_or("name_invalid")? {
            n = n * 256 + usize::from(*b)
        }
        *at += size;
        offsets.push(n);
    }
    if offsets[0] != 1 || offsets.windows(2).any(|p| p[0] > p[1]) {
        return Err("name_invalid".into());
    }
    let end = at.checked_add(offsets[count] - 1).ok_or("name_invalid")?;
    if end > data.len() {
        return Err("name_invalid".into());
    }
    let values = offsets
        .windows(2)
        .map(|p| data[*at + p[0] - 1..*at + p[1] - 1].to_vec())
        .collect();
    *at = end;
    Ok(values)
}
fn pack(values: &[Vec<u8>]) -> Vec<u8> {
    let mut b = (values.len() as u16).to_be_bytes().to_vec();
    if values.is_empty() {
        return b;
    }
    b.push(4);
    let mut at = 1u32;
    b.extend(at.to_be_bytes());
    for v in values {
        at += v.len() as u32;
        b.extend(at.to_be_bytes());
    }
    for v in values {
        b.extend(v)
    }
    b
}
// Preserve all operands verbatim except absolute offsets in Top/Font DICTs.
fn dict(data: &[u8], shift: usize, fd_array: Option<usize>) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    let mut operands: Vec<Vec<u8>> = vec![];
    let mut at = 0;
    while at < data.len() {
        let start = at;
        let b = data[at];
        at += 1;
        match b {
            28 => at += 2,
            29 => at += 4,
            30 => loop {
                let n = *data.get(at).ok_or("name_invalid")?;
                at += 1;
                if n >> 4 == 15 || n & 15 == 15 {
                    break;
                }
            },
            32..=246 => {}
            247..=254 => at += 1,
            0..=21 => {
                let op = if b == 12 {
                    let n = *data.get(at).ok_or("name_invalid")?;
                    at += 1;
                    1200 + u16::from(n)
                } else {
                    u16::from(b)
                };
                let target = match op {
                    15 | 16 | 17 | 1236 | 1237 => Some(0),
                    18 => Some(1),
                    _ => None,
                };
                for (i, raw) in operands.iter().enumerate() {
                    if target == Some(i) {
                        let n = integer(raw)?;
                        let changed = if op == 1236 {
                            fd_array.ok_or("name_invalid")?
                        } else if (op == 15 && n <= 2) || (op == 16 && n <= 1) {
                            n
                        } else {
                            n.checked_add(shift).ok_or("name_invalid")?
                        };
                        out.push(29);
                        out.extend(
                            i32::try_from(changed)
                                .map_err(|_| "name_invalid")?
                                .to_be_bytes(),
                        );
                    } else {
                        out.extend(raw)
                    }
                }
                out.extend(&data[start..at]);
                operands.clear();
                continue;
            }
            _ => return Err("name_invalid".into()),
        }
        operands.push(data.get(start..at).ok_or("name_invalid")?.to_vec());
    }
    if !operands.is_empty() {
        return Err("name_invalid".into());
    }
    Ok(out)
}
fn integer(b: &[u8]) -> Result<usize> {
    let n = match b[0] {
        28 => i32::from(i16::from_be_bytes(b[1..3].try_into().unwrap())),
        29 => i32::from_be_bytes(b[1..5].try_into().unwrap()),
        32..=246 => i32::from(b[0]) - 139,
        247..=250 => (i32::from(b[0]) - 247) * 256 + i32::from(b[1]) + 108,
        251..=254 => -(i32::from(b[0]) - 251) * 256 - i32::from(b[1]) - 108,
        _ => return Err("name_invalid".into()),
    };
    usize::try_from(n).map_err(|_| "name_invalid".into())
}
pub fn inspect(bytes: &[u8]) -> Result<Option<Inspection>> {
    if bytes.len() as u64 > crate::font_io::MAX_FONT_BYTES {
        return Err("font_too_large".into());
    }
    if bytes.starts_with(b"ttcf") || bytes.starts_with(b"wOF2") || bytes.starts_with(b"wOFF") {
        return Ok(None);
    }
    let face = Face::parse(bytes, 0).map_err(|_| "font_invalid")?;
    let Some(cff) = face.raw_face().table(Tag::from_bytes(b"CFF ")) else {
        return Ok(None);
    };
    let mut at = usize::from(*cff.get(2).ok_or("name_invalid")?);
    let names = index(cff, &mut at)?;
    if names.len() != 1 {
        return Err("name_invalid".into());
    }
    let cff = String::from_utf8(names[0].clone()).map_err(|_| "name_invalid")?;
    let ps: Vec<_> = face
        .names()
        .into_iter()
        .filter(|n| n.name_id == 6)
        .map(|n| {
            n.to_string()
                .or_else(|| {
                    n.name
                        .is_ascii()
                        .then(|| String::from_utf8_lossy(n.name).into_owned())
                })
                .ok_or_else(|| "name_invalid".to_string())
        })
        .collect::<Result<Vec<_>>>()?;
    let postscript = ps.first().cloned().ok_or("name_invalid")?;
    if ps.iter().any(|s| s != &postscript)
        || postscript.is_empty()
        || postscript.len() > 127
        || !postscript
            .bytes()
            .all(|b| (33..=126).contains(&b) && !b"[](){}<>/%".contains(&b))
    {
        return Err("name_invalid".into());
    }
    Ok(Some(Inspection {
        mismatch: postscript != cff,
        postscript,
        cff,
    }))
}
pub fn repair(bytes: &[u8]) -> Result<Vec<u8>> {
    let info = inspect(bytes)?.ok_or("name_unsupported")?;
    if !info.mismatch {
        return Ok(bytes.to_vec());
    }
    let face = Face::parse(bytes, 0).map_err(|_| "font_invalid")?;
    let cff = face
        .raw_face()
        .table(Tag::from_bytes(b"CFF "))
        .ok_or("name_invalid")?;
    if cff.first() != Some(&1) {
        return Err("name_unsupported".into());
    }
    let mut at = usize::from(cff[2]);
    index(cff, &mut at)?;
    let tops = index(cff, &mut at)?;
    if tops.len() != 1 {
        return Err("name_invalid".into());
    }
    let rest_start = at;
    index(cff, &mut at)?;
    index(cff, &mut at)?;
    let rest = &cff[rest_start..at];
    // Rebase absolute Private offsets in CID Font DICTs as well as the Top DICT.
    let fd_offset = fd_offset(&tops[0])?;
    let fds = if let Some(offset) = fd_offset {
        let mut p = offset;
        index(cff, &mut p)?
    } else {
        vec![]
    };
    let names = pack(&[info.postscript.into_bytes()]);
    let mut shift = 0;
    let mut prefix = vec![];
    let mut fd = vec![];
    let mut converged = false;
    for _ in 0..8 {
        fd = pack(
            &fds.iter()
                .map(|d| dict(d, shift, None))
                .collect::<Result<Vec<_>>>()?,
        );
        let top = dict(&tops[0], shift, Some(shift + cff.len()))?;
        prefix = vec![1, 0, 4, 4];
        prefix.extend(&names);
        prefix.extend(pack(&[top]));
        prefix.extend(rest);
        if prefix.len() == shift {
            converged = true;
            break;
        }
        shift = prefix.len();
    }
    if !converged {
        return Err("name_invalid".into());
    }
    let mut replacement = prefix;
    replacement.extend(cff);
    if !fds.is_empty() {
        replacement.extend(fd)
    }
    let mut tables = std::collections::BTreeMap::new();
    for record in face.raw_face().table_records {
        let tag = record.tag.to_bytes();
        if &tag != b"DSIG" {
            tables.insert(
                tag,
                face.raw_face()
                    .table(record.tag)
                    .ok_or("name_invalid")?
                    .to_vec(),
            );
        }
    }
    tables
        .get_mut(b"head")
        .ok_or("name_invalid")?
        .get_mut(8..12)
        .ok_or("name_invalid")?
        .fill(0);
    tables.insert(*b"CFF ", replacement);
    let output = crate::color_fonts::assemble(tables, true)?;
    if inspect(&output)?.ok_or("name_invalid")?.mismatch {
        return Err("name_invalid".into());
    }
    Ok(output)
}

fn fd_offset(data: &[u8]) -> Result<Option<usize>> {
    let mut at = 0;
    let mut operands = vec![];
    while at < data.len() {
        let start = at;
        let b = data[at];
        at += 1;
        match b {
            28 => at += 2,
            29 => at += 4,
            30 => loop {
                let n = *data.get(at).ok_or("name_invalid")?;
                at += 1;
                if n >> 4 == 15 || n & 15 == 15 {
                    break;
                }
            },
            32..=246 => {}
            247..=254 => at += 1,
            0..=21 => {
                if b == 12 {
                    let op = *data.get(at).ok_or("name_invalid")?;
                    at += 1;
                    if op == 36 {
                        return operands.first().map(|b: &Vec<u8>| integer(b)).transpose();
                    }
                }
                operands.clear();
                continue;
            }
            _ => return Err("name_invalid".into()),
        }
        operands.push(data.get(start..at).ok_or("name_invalid")?.to_vec());
    }
    Ok(None)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn repairs_cff_without_changing_charstrings_and_other_tables() {
        let source = include_bytes!("../tests/data/color/colrv0.otf");
        let f = Face::parse(source, 0).unwrap();
        let mut tables = std::collections::BTreeMap::new();
        for r in f.raw_face().table_records {
            tables.insert(
                r.tag.to_bytes(),
                f.raw_face().table(r.tag).unwrap().to_vec(),
            );
        }
        let cff = tables.get_mut(b"CFF " as &[u8; 4]).unwrap();
        let mut at = usize::from(cff[2]);
        let n = index(cff, &mut at).unwrap();
        let begin = at - n[0].len();
        cff[begin] = b'X';
        let mismatched = crate::color_fonts::assemble(tables, true).unwrap();
        assert!(inspect(&mismatched).unwrap().unwrap().mismatch);
        let output = repair(&mismatched).unwrap();
        let checksum = output.chunks(4).fold(0u32, |sum, b| {
            let mut n = [0; 4];
            n[..b.len()].copy_from_slice(b);
            sum.wrapping_add(u32::from_be_bytes(n))
        });
        assert_eq!(checksum, 0xb1b0afba);
        assert!(!inspect(&output).unwrap().unwrap().mismatch);
        assert_eq!(repair(&output).unwrap(), output);
        let a = Face::parse(&mismatched, 0).unwrap();
        let b = Face::parse(&output, 0).unwrap();
        for r in a.raw_face().table_records {
            if ![b"head", b"CFF "].contains(&&r.tag.to_bytes()) {
                assert_eq!(a.raw_face().table(r.tag), b.raw_face().table(r.tag))
            }
        }
        for id in 0..a.number_of_glyphs() {
            assert_eq!(
                a.glyph_bounding_box(ttf_parser::GlyphId(id)),
                b.glyph_bounding_box(ttf_parser::GlyphId(id))
            );
            assert_eq!(
                a.glyph_hor_advance(ttf_parser::GlyphId(id)),
                b.glyph_hor_advance(ttf_parser::GlyphId(id))
            );
        }
    }
    #[test]
    fn rejects_truncated_inputs() {
        for n in 0..100 {
            assert!(repair(&vec![0; n]).is_err())
        }
    }
}
