use super::*;
type IO<T> = crate::font_io::Result<T>;
fn word(b: &[u8], p: usize) -> IO<u16> {
    Ok(u16::from_be_bytes(
        b.get(p..p + 2)
            .ok_or("color_invalid")?
            .try_into()
            .map_err(|_| "color_invalid")?,
    ))
}
fn table(bytes: &[u8]) -> IO<&[u8]> {
    ttf_parser::RawFace::parse(bytes, 0)
        .map_err(|_| "color_invalid")?
        .table(ttf_parser::Tag::from_bytes(b"CPAL"))
        .ok_or("color_no_palette".into())
}
fn location(b: &[u8], palette: u16, entry: u16) -> IO<usize> {
    let entries = word(b, 2)?;
    let palettes = word(b, 4)?;
    let records = word(b, 6)?;
    if palette >= palettes || entry >= entries {
        return Err("color_settings".into());
    }
    let record = usize::from(word(b, 12 + usize::from(palette) * 2)?) + usize::from(entry);
    if record >= usize::from(records) {
        return Err("color_invalid".into());
    }
    let start = u32::from_be_bytes(
        b.get(8..12)
            .ok_or("color_invalid")?
            .try_into()
            .map_err(|_| "color_invalid")?,
    ) as usize;
    let at = start.checked_add(record * 4).ok_or("color_invalid")?;
    b.get(at..at + 4).ok_or("color_invalid")?;
    Ok(at)
}
fn key(s: &str) -> IO<(u16, u16)> {
    let (a, b) = s.split_once(':').ok_or("color_settings")?;
    let p = a.parse::<u16>().map_err(|_| "color_settings")?;
    let e = b.parse::<u16>().map_err(|_| "color_settings")?;
    if s != format!("{p}:{e}") {
        return Err("color_settings".into());
    }
    Ok((p, e))
}
pub(super) fn validate(bytes: &[u8], changes: &BTreeMap<String, [u8; 4]>) -> IO<()> {
    if changes.is_empty() {
        return Ok(());
    }
    let b = table(bytes)?;
    for s in changes.keys() {
        let (p, e) = key(s)?;
        location(b, p, e)?;
    }
    Ok(())
}
pub(super) fn colors(
    bytes: &[u8],
    palette: u16,
    changes: &BTreeMap<String, [u8; 4]>,
) -> IO<Vec<[u8; 4]>> {
    let Ok(b) = table(bytes) else {
        return Ok(vec![]);
    };
    let mut out = vec![];
    for entry in 0..word(b, 2)? {
        let at = location(b, palette, entry)?;
        out.push(
            changes
                .get(&format!("{palette}:{entry}"))
                .copied()
                .unwrap_or([b[at + 2], b[at + 1], b[at], b[at + 3]]),
        );
    }
    Ok(out)
}
pub(super) fn apply(bytes: &[u8], changes: &BTreeMap<String, [u8; 4]>) -> IO<Vec<u8>> {
    if changes.is_empty() {
        return Ok(bytes.to_vec());
    }
    validate(bytes, changes)?;
    let face = ttf_parser::Face::parse(bytes, 0).map_err(|_| "color_invalid")?;
    let mut tables = BTreeMap::new();
    for r in face.raw_face().table_records {
        let tag = r.tag.to_bytes();
        if &tag == b"DSIG" {
            continue;
        }
        let mut b = face
            .raw_face()
            .table(r.tag)
            .ok_or("color_invalid")?
            .to_vec();
        if &tag == b"head" {
            b.get_mut(8..12).ok_or("color_invalid")?.fill(0);
        }
        if &tag == b"CPAL" {
            // Rebuild independent records: original palettes can share or overlap records.
            let count = word(&b, 4)?;
            let entries = word(&b, 2)?;
            let n = usize::from(count) * usize::from(entries);
            if n > 65535 {
                return Err("color_palette_limit".into());
            }
            let start = b.len();
            for p in 0..count {
                for e in 0..entries {
                    let at = location(&b, p, e)?;
                    let c = changes.get(&format!("{p}:{e}")).copied().unwrap_or([
                        b[at + 2],
                        b[at + 1],
                        b[at],
                        b[at + 3],
                    ]);
                    b.extend([c[2], c[1], c[0], c[3]]);
                }
            }
            b[6..8].copy_from_slice(&(n as u16).to_be_bytes());
            b[8..12].copy_from_slice(&(start as u32).to_be_bytes());
            for p in 0..count {
                let at = 12 + usize::from(p) * 2;
                b[at..at + 2].copy_from_slice(&(p * entries).to_be_bytes());
            }
        }
        tables.insert(tag, b);
    }
    let out = crate::color_fonts::assemble(tables, bytes.starts_with(b"OTTO"))?;
    let actual = ttf_parser::Face::parse(&out, 0).map_err(|_| "color_invalid_output")?;
    let cpal = actual
        .raw_face()
        .table(ttf_parser::Tag::from_bytes(b"CPAL"))
        .ok_or("color_invalid_output")?;
    for (s, color) in changes {
        let (p, e) = key(s)?;
        let at = location(cpal, p, e)?;
        if [cpal[at + 2], cpal[at + 1], cpal[at], cpal[at + 3]] != *color {
            return Err("color_invalid_output".into());
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn editing_shared_color_records_changes_only_the_selected_palette() {
        let bytes = include_bytes!("../../tests/data/color/colrv0.ttf");
        let face = ttf_parser::Face::parse(bytes, 0).unwrap();
        let mut tables = BTreeMap::new();
        for r in face.raw_face().table_records {
            let mut b = face.raw_face().table(r.tag).unwrap().to_vec();
            if r.tag.to_bytes() == *b"head" {
                b[8..12].fill(0);
            }
            if r.tag.to_bytes() == *b"CPAL" {
                assert!(word(&b, 4).unwrap() > 1);
                b[14..16].copy_from_slice(&0u16.to_be_bytes());
            }
            tables.insert(r.tag.to_bytes(), b);
        }
        let shared = crate::color_fonts::assemble(tables, false).unwrap();
        let before = colors(&shared, 1, &BTreeMap::new()).unwrap();
        let changes = [("0:0".into(), [12, 34, 56, 78])].into();
        let edited = apply(&shared, &changes).unwrap();
        assert_eq!(
            colors(&edited, 0, &BTreeMap::new()).unwrap()[0],
            [12, 34, 56, 78]
        );
        assert_eq!(colors(&edited, 1, &BTreeMap::new()).unwrap(), before);
    }
}
