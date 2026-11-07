//! Keep point-index GDEF references live; materialize hinted GPOS/BASE anchor fallbacks.
use super::*;
fn relative(b: &[u8], base: usize, field: usize) -> Result<Option<usize>> {
    let offset = usize::from(word(b, field)?);
    if offset == 0 {
        return Ok(None);
    }
    Ok(Some(
        base.checked_add(offset)
            .filter(|&p| p < b.len())
            .ok_or(Error::InvalidFile)?,
    ))
}
fn coverage(bytes: &[u8], at: usize) -> Result<Vec<u16>> {
    crate::imported_layout::points::coverage(bytes, at).map_err(|_| Error::InvalidFile)
}
fn device(bytes: &[u8], at: usize) -> Result<Vec<u8>> {
    let start = word(bytes, at)?;
    let end = word(bytes, at + 2)?;
    let format = word(bytes, at + 4)?;
    let len = if format == 0x8000 {
        6
    } else {
        if !(1..=3).contains(&format) || start > end {
            return Err(Error::InvalidFile);
        }
        6 + ((usize::from(end - start) + 1) * (1usize << format)).div_ceil(16) * 2
    };
    Ok(bytes.get(at..at + len).ok_or(Error::InvalidFile)?.to_vec())
}
fn list(
    bytes: &[u8],
    at: usize,
    caret: bool,
    map: &mut impl FnMut(u16, usize) -> Result<usize>,
) -> Result<Vec<u8>> {
    let glyphs = coverage(bytes, relative(bytes, at, at)?.ok_or(Error::InvalidFile)?)?;
    if usize::from(word(bytes, at + 2)?) != glyphs.len() {
        return Err(Error::InvalidFile);
    }
    let mut out = vec![0u8; 4 + glyphs.len() * 2];
    out[2..4].copy_from_slice(&(glyphs.len() as u16).to_be_bytes());
    for (i, &glyph) in glyphs.iter().enumerate() {
        let row = relative(bytes, at, at + 4 + i * 2)?.ok_or(Error::InvalidFile)?;
        let offset = u16::try_from(out.len()).map_err(|_| Error::TooLarge)?;
        out[4 + i * 2..6 + i * 2].copy_from_slice(&offset.to_be_bytes());
        let count = usize::from(word(bytes, row)?);
        let mut data = vec![0; 2 + count * 2];
        data[..2].copy_from_slice(&(count as u16).to_be_bytes());
        if !caret {
            let mut points = vec![];
            for p in 0..count {
                points.push(
                    u16::try_from(map(glyph, usize::from(word(bytes, row + 2 + p * 2)?))?)
                        .map_err(|_| Error::TooLarge)?,
                );
            }
            points.sort_unstable();
            points.dedup();
            data = Vec::with_capacity(2 + points.len() * 2);
            data.extend((points.len() as u16).to_be_bytes());
            for p in points {
                data.extend(p.to_be_bytes());
            }
        } else {
            for c in 0..count {
                let at = relative(bytes, row, row + 2 + c * 2)?.ok_or(Error::InvalidFile)?;
                let offset = u16::try_from(data.len()).map_err(|_| Error::TooLarge)?;
                data[2 + c * 2..4 + c * 2].copy_from_slice(&offset.to_be_bytes());
                match word(bytes, at)? {
                    1 => data.extend_from_slice(bytes.get(at..at + 4).ok_or(Error::InvalidFile)?),
                    2 => {
                        data.extend(2u16.to_be_bytes());
                        data.extend(
                            u16::try_from(map(glyph, usize::from(word(bytes, at + 2)?))?)
                                .map_err(|_| Error::TooLarge)?
                                .to_be_bytes(),
                        );
                    }
                    3 => {
                        data.extend(3u16.to_be_bytes());
                        data.extend(word(bytes, at + 2)?.to_be_bytes());
                        if let Some(d) = relative(bytes, at, at + 4)? {
                            data.extend(6u16.to_be_bytes());
                            data.extend(device(bytes, d)?);
                        } else {
                            data.extend(0u16.to_be_bytes());
                        }
                    }
                    _ => return Err(Error::InvalidFile),
                }
            }
        }
        out.extend(data);
        if out.len() > 65535 {
            return Err(Error::TooLarge);
        }
    }
    let offset = u16::try_from(out.len()).map_err(|_| Error::TooLarge)?;
    out[..2].copy_from_slice(&offset.to_be_bytes());
    out.extend(1u16.to_be_bytes());
    out.extend((glyphs.len() as u16).to_be_bytes());
    for glyph in glyphs {
        out.extend(glyph.to_be_bytes());
    }
    Ok(out)
}
pub(super) fn apply(
    tables: &mut Tables,
    mut map: impl FnMut(u16, usize) -> Result<usize>,
) -> Result<()> {
    crate::imported_layout::points::materialize_edit_anchors(tables)?;
    let Some(source) = tables.get(b"GDEF") else {
        return Ok(());
    };
    let minor = word(source, 2)?;
    if word(source, 0)? != 1 || !matches!(minor, 0 | 2 | 3) {
        return Err(Error::InvalidFile);
    }
    let header = if minor == 3 {
        18
    } else if minor == 2 {
        14
    } else {
        12
    };
    source.get(..header).ok_or(Error::InvalidFile)?;
    let mut prefix = vec![];
    let mut replacement = BTreeMap::new();
    for (field, caret) in [(6, false), (8, true)] {
        if let Some(at) = relative(source, 0, field)? {
            replacement.insert(field, header + prefix.len());
            prefix.extend(list(source, at, caret, &mut map)?);
        }
    }
    if prefix.is_empty() {
        return Ok(());
    }
    let mut out = source[..header].to_vec();
    for field in [4, 6, 8, 10, 12]
        .into_iter()
        .filter(|&f| f < header && f != 14)
    {
        let old = usize::from(word(source, field)?);
        let offset = if let Some(&new) = replacement.get(&field) {
            new
        } else if old == 0 {
            0
        } else {
            old.checked_add(prefix.len()).ok_or(Error::TooLarge)?
        };
        out[field..field + 2].copy_from_slice(
            &u16::try_from(offset)
                .map_err(|_| Error::TooLarge)?
                .to_be_bytes(),
        );
    }
    if minor == 3 {
        let offset = u32::from_be_bytes(source[14..18].try_into().map_err(|_| Error::InvalidFile)?);
        let offset = if offset == 0 {
            0
        } else {
            offset
                .checked_add(u32::try_from(prefix.len()).map_err(|_| Error::TooLarge)?)
                .ok_or(Error::TooLarge)?
        };
        out[14..18].copy_from_slice(&offset.to_be_bytes());
    }
    out.extend(prefix);
    out.extend_from_slice(&source[header..]);
    if out.len() > 32 * 1024 * 1024 {
        return Err(Error::TooLarge);
    }
    tables.insert(*b"GDEF", out);
    Ok(())
}

pub(super) fn protect_cff_points(bytes: &[u8], affected: &BTreeSet<u16>) -> Result<()> {
    let face = ttf_parser::Face::parse(bytes, 0).map_err(|_| Error::InvalidFile)?;
    let Some(table) = face.raw_face().table(ttf_parser::Tag::from_bytes(b"GDEF")) else {
        return Ok(());
    };
    for (field, caret) in [(6, false), (8, true)] {
        if let Some(list) = relative(table, 0, field)? {
            let glyphs = coverage(
                table,
                relative(table, list, list)?.ok_or(Error::InvalidFile)?,
            )?;
            if usize::from(word(table, list + 2)?) != glyphs.len() {
                return Err(Error::InvalidFile);
            }
            for (index, glyph) in glyphs.into_iter().enumerate() {
                if !affected.contains(&glyph) {
                    continue;
                }
                let row = relative(table, list, list + 4 + index * 2)?.ok_or(Error::InvalidFile)?;
                let count = usize::from(word(table, row)?);
                if !caret && count > 0 {
                    return Err(Error::InvalidGeometry);
                }
                if caret {
                    for n in 0..count {
                        let at =
                            relative(table, row, row + 2 + n * 2)?.ok_or(Error::InvalidFile)?;
                        if word(table, at)? == 2 {
                            return Err(Error::InvalidGeometry);
                        }
                    }
                }
            }
        }
    }
    Ok(())
}
