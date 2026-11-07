//! Generate explicit vertical metrics from typographic height and default glyph bounds.
use super::*;
use crate::source_font::master_tables;
pub(super) fn vertical(
    face: &ttf_parser::Face<'_>,
    axes: &[String],
    bounds: &[Option<[i32; 4]>],
    tables: &mut BTreeMap<[u8; 4], Vec<u8>>,
) -> Result<bool> {
    let has_header = tables.contains_key(b"vhea");
    let has_metrics = tables.contains_key(b"vmtx");
    if has_header && has_metrics {
        return Ok(false);
    }
    if has_header || has_metrics || tables.contains_key(b"VVAR") {
        return Err(Error::InvalidFile);
    }
    let (ascender, descender) = if let Some(os2) = face.tables().os2 {
        (os2.typographic_ascender(), os2.typographic_descender())
    } else {
        (face.ascender(), face.descender())
    };
    let mut height = i64::from(ascender) - i64::from(descender);
    let mut variations = vec![];
    if face.tables().os2.is_some() {
        for (tag, sign) in [(b"hasc", 1.), (b"hdsc", -1.)] {
            for (support, value) in font_metric(face, axes, tag)? {
                let value = value * sign;
                if support.axes.values().all(|v| v[1] == 0.) {
                    height += value as i64;
                } else {
                    variations.push((support, value));
                }
            }
        }
    }
    if height <= 0 {
        height = i64::from(face.units_per_em());
    }
    let height = u16::try_from(height).map_err(|_| Error::InvalidGeometry)?;
    let count = face.number_of_glyphs();
    let mut header = vec![0; 36];
    header[..4].copy_from_slice(&0x00011000u32.to_be_bytes());
    let upper = i32::from(height) / 2;
    let lower = upper - i32::from(height);
    header[4..6].copy_from_slice(&(upper as i16).to_be_bytes());
    header[6..8].copy_from_slice(&(lower as i16).to_be_bytes());
    header[20..22].copy_from_slice(&1u16.to_be_bytes());
    header[34..36].copy_from_slice(&count.to_be_bytes());
    let mut metrics = Vec::with_capacity(usize::from(count) * 4);
    for (glyph, bbox) in bounds.iter().enumerate() {
        let size = bbox.map_or(0, |b| b[3] - b[1]);
        let top = if face.tables().glyf.is_none() {
            face.tables()
                .vorg
                .map(|vorg| {
                    i32::from(vorg.glyph_y_origin(ttf_parser::GlyphId(glyph as u16)))
                        - bbox.map_or(0, |b| b[3])
                })
                .unwrap_or((i32::from(height) - size) / 2)
        } else {
            (i32::from(height) - size) / 2
        };
        metrics.extend(height.to_be_bytes());
        metrics.extend(
            i16::try_from(top)
                .map_err(|_| Error::InvalidGeometry)?
                .to_be_bytes(),
        );
    }
    tables.insert(*b"vhea", header);
    tables.insert(*b"vmtx", metrics);
    master_tables::change_advances(tables, bounds, &BTreeMap::new(), true)?;
    let mut seed = 1u16.to_be_bytes().to_vec();
    seed.extend(8u32.to_be_bytes());
    seed.extend(0u16.to_be_bytes());
    seed.extend((axes.len() as u16).to_be_bytes());
    seed.extend(0u16.to_be_bytes());
    let mut builder = Builder::new(Store::parse(&seed, axes)?);
    let (index, errors) = builder.add((u16::MAX, u16::MAX), &variations)?;
    if !errors.is_empty() {
        return Err(Error::InvalidFile);
    }
    let store = builder.finish()?;
    let mut vvar = vec![0; 24];
    vvar[..4].copy_from_slice(&[0, 1, 0, 0]);
    vvar[4..8].copy_from_slice(&24u32.to_be_bytes());
    vvar.extend(store);
    let map_at = vvar.len();
    vvar.extend(master_store::write_map(&vec![index; usize::from(count)])?);
    vvar[8..12].copy_from_slice(&(map_at as u32).to_be_bytes());
    tables.insert(*b"VVAR", vvar);
    Ok(true)
}
fn word(bytes: &[u8], at: usize) -> Result<u16> {
    Ok(u16::from_be_bytes(
        bytes
            .get(at..at + 2)
            .ok_or(Error::InvalidFile)?
            .try_into()
            .map_err(|_| Error::InvalidFile)?,
    ))
}
fn font_metric(
    face: &ttf_parser::Face<'_>,
    axes: &[String],
    tag: &[u8; 4],
) -> Result<Vec<(Support, f64)>> {
    let Some(table) = face.raw_face().table(ttf_parser::Tag::from_bytes(b"MVAR")) else {
        return Ok(vec![]);
    };
    if table.len() < 12 || table[..6] != [0, 1, 0, 0, 0, 0] {
        return Err(Error::InvalidFile);
    }
    let size = usize::from(word(table, 6)?);
    let count = usize::from(word(table, 8)?);
    let store_at = usize::from(word(table, 10)?);
    if count == 0 {
        return Ok(vec![]);
    }
    if size < 8 || store_at < 12 + count * size {
        return Err(Error::InvalidFile);
    }
    let records = table.get(12..12 + count * size).ok_or(Error::InvalidFile)?;
    let store = Store::parse(table.get(store_at..).ok_or(Error::InvalidFile)?, axes)?;
    let mut result = vec![];
    for record in records.chunks(size) {
        if &record[..4] == tag {
            for (region, delta) in store.row((word(record, 4)?, word(record, 6)?))? {
                if delta != 0 {
                    result.push((store.support(region)?, f64::from(delta)));
                }
            }
        }
    }
    Ok(result)
}
