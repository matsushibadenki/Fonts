//! Rebase fvar references to default-only name IDs after changing the default location.
use super::*;
type IO<T> = crate::font_io::Result<T>;
fn word(b: &[u8], p: usize) -> IO<u16> {
    Ok(u16::from_be_bytes(
        b.get(p..p + 2)
            .ok_or("variation_instance")?
            .try_into()
            .map_err(|_| "variation_instance")?,
    ))
}
pub(super) fn normalize_instances(font: Vec<u8>) -> IO<Vec<u8>> {
    let face = ttf_parser::Face::parse(&font, 0).map_err(|_| "variation_invalid_output")?;
    let Some(raw) = face.raw_face().table(ttf_parser::Tag::from_bytes(b"fvar")) else {
        return Ok(font);
    };
    let axes = usize::from(word(raw, 8)?);
    let stride = usize::from(word(raw, 10)?);
    let start = usize::from(word(raw, 4)?);
    let count = usize::from(word(raw, 12)?);
    let size = usize::from(word(raw, 14)?);
    if stride != 20 || ![4 + axes * 4, 6 + axes * 4].contains(&size) {
        return Err("variation_instance".into());
    }
    let instances = start + axes * stride;
    raw.get(..instances + count * size)
        .ok_or("variation_instance")?;
    let mut used: std::collections::BTreeSet<u16> =
        face.names().into_iter().map(|n| n.name_id).collect();
    for i in 0..axes {
        used.insert(word(raw, start + i * stride + 18)?);
    }
    for i in 0..count {
        used.insert(word(raw, instances + i * size)?);
        if size == 6 + axes * 4 {
            used.insert(word(raw, instances + i * size + 4 + axes * 4)?);
        }
    }
    let mut fvar = raw.to_vec();
    let mut aliases = BTreeMap::new();
    let mut changed = false;
    for i in 0..count {
        let at = instances + i * size;
        let is_default = (0..axes).all(|n| {
            raw.get(at + 4 + n * 4..at + 8 + n * 4)
                == raw.get(start + n * stride + 8..start + n * stride + 12)
        });
        if is_default {
            continue;
        }
        let id = word(raw, at)?;
        if id == 2 || id == 17 {
            let alias = if let Some(&n) = aliases.get(&id) {
                n
            } else {
                let n = (256..32768)
                    .find(|n| !used.contains(n))
                    .ok_or("variation_instance")?;
                used.insert(n);
                aliases.insert(id, n);
                n
            };
            fvar[at..at + 2].copy_from_slice(&alias.to_be_bytes());
            changed = true;
        }
        if size == 6 + axes * 4 && word(raw, at + 4 + axes * 4)? == 6 {
            fvar[at + 4 + axes * 4..at + 6 + axes * 4].copy_from_slice(&0xffffu16.to_be_bytes());
            changed = true;
        }
    }
    if !changed {
        return Ok(font);
    }
    let mut tables: BTreeMap<_, _> = face
        .raw_face()
        .table_records
        .into_iter()
        .filter(|r| r.tag.to_bytes() != *b"DSIG")
        .map(|r| {
            (
                r.tag.to_bytes(),
                face.raw_face().table(r.tag).unwrap_or_default().to_vec(),
            )
        })
        .collect();
    if !aliases.is_empty() {
        let name = tables.get(b"name").ok_or("variation_instance")?;
        let n = usize::from(word(name, 2)?);
        let storage = usize::from(word(name, 4)?);
        let end = 6 + n * 12;
        if word(name, 0)? > 1 || end > storage || storage > name.len() {
            return Err("variation_instance".into());
        }
        let mut records = vec![];
        for i in 0..n {
            records.push(
                <[u8; 12]>::try_from(
                    name.get(6 + i * 12..18 + i * 12)
                        .ok_or("variation_instance")?,
                )
                .map_err(|_| "variation_instance")?,
            );
        }
        let original = records.clone();
        for (id, alias) in aliases {
            let mut found = false;
            for record in &original {
                if word(record, 6)? == id {
                    let mut r = *record;
                    r[6..8].copy_from_slice(&alias.to_be_bytes());
                    records.push(r);
                    found = true;
                }
            }
            if !found {
                return Err("variation_instance".into());
            }
        }
        records.sort_by(|a, b| a[..8].cmp(&b[..8]));
        let offset =
            u16::try_from(storage + (records.len() - n) * 12).map_err(|_| "variation_instance")?;
        let count = u16::try_from(records.len()).map_err(|_| "variation_instance")?;
        let mut out = name[..6].to_vec();
        out[2..4].copy_from_slice(&count.to_be_bytes());
        out[4..6].copy_from_slice(&offset.to_be_bytes());
        for record in records {
            out.extend(record);
        }
        out.extend(&name[end..]);
        tables.insert(*b"name", out);
    }
    tables.insert(*b"fvar", fvar);
    tables
        .get_mut(b"head")
        .ok_or("variation_instance")?
        .get_mut(8..12)
        .ok_or("variation_instance")?
        .fill(0);
    crate::color_fonts::assemble(tables, font.starts_with(b"OTTO"))
}
