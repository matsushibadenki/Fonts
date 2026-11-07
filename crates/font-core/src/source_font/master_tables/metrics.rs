use super::*;
pub(super) fn clear_sidebearing_maps(tables: &mut Tables) -> Result<()> {
    for (tag, size) in [(b"HVAR", 20), (b"VVAR", 24)] {
        if let Some(table) = tables.get_mut(tag) {
            if table.len() < size || table[..4] != [0, 1, 0, 0] {
                return Err(Error::InvalidFile);
            }
            // Optional sidebearing caches no longer describe the edited outline's extrema.
            // Keep advances, vertical origins, the variation store and all payload bytes intact.
            table[12..20].fill(0);
        }
    }
    Ok(())
}
pub(super) fn update(
    tables: &mut Tables,
    old: &[Option<[i32; 4]>],
    new: &[Option<[i32; 4]>],
    affected: &BTreeSet<u16>,
    vertical: bool,
) -> Result<()> {
    update_advances(tables, old, new, affected, vertical, &BTreeMap::new())
}
pub(super) fn update_advances(
    tables: &mut Tables,
    old: &[Option<[i32; 4]>],
    new: &[Option<[i32; 4]>],
    affected: &BTreeSet<u16>,
    vertical: bool,
    advances: &BTreeMap<u16, i32>,
) -> Result<()> {
    let (header_tag, metrics_tag) = if vertical {
        (*b"vhea", *b"vmtx")
    } else {
        (*b"hhea", *b"hmtx")
    };
    if !tables.contains_key(&header_tag) && !tables.contains_key(&metrics_tag) && vertical {
        return Ok(());
    }
    let header = tables.get(&header_tag).ok_or(Error::InvalidFile)?;
    if header.len() < 36 {
        return Err(Error::InvalidFile);
    }
    let longs = usize::from(word(header, 34)?);
    let count = new.len();
    if advances.keys().any(|id| usize::from(*id) >= count) {
        return Err(Error::MissingGlyph);
    }
    if longs == 0 || longs > count {
        return Err(Error::InvalidFile);
    }
    let source = tables.get(&metrics_tag).ok_or(Error::InvalidFile)?;
    let mut metrics = Vec::with_capacity(count);
    let mut max_advance = 0u16;
    let mut min_start = i32::MAX;
    let mut min_end = i32::MAX;
    let mut max_extent = i32::MIN;
    for glyph in 0..count {
        let advance = i64::from(word(source, glyph.min(longs - 1) * 4)?)
            + i64::from(advances.get(&(glyph as u16)).copied().unwrap_or(0));
        let advance = u16::try_from(advance).map_err(|_| Error::InvalidGeometry)?;
        let at = if glyph < longs {
            glyph * 4 + 2
        } else {
            longs * 4 + (glyph - longs) * 2
        };
        let mut bearing = i32::from(word(source, at)? as i16);
        if affected.contains(&(glyph as u16)) {
            let before = old[glyph].unwrap_or([0; 4]);
            let after = new[glyph].unwrap_or([0; 4]);
            bearing += if vertical {
                before[3] - after[3]
            } else {
                after[0] - before[0]
            };
        }
        metrics.extend(advance.to_be_bytes());
        metrics.extend(
            i16::try_from(bearing)
                .map_err(|_| Error::InvalidGeometry)?
                .to_be_bytes(),
        );
        max_advance = max_advance.max(advance);
        if let Some(b) = new[glyph] {
            let extent = if vertical { b[3] - b[1] } else { b[2] - b[0] };
            min_start = min_start.min(bearing);
            min_end = min_end.min(i32::from(advance) - bearing - extent);
            max_extent = max_extent.max(bearing + extent);
        }
    }
    let header = tables.get_mut(&header_tag).ok_or(Error::InvalidFile)?;
    header[10..12].copy_from_slice(&max_advance.to_be_bytes());
    let has_outlines = new.iter().any(Option::is_some);
    for (index, value) in [min_start, min_end, max_extent].into_iter().enumerate() {
        set(header, 12 + index * 2, if has_outlines { value } else { 0 })?;
    }
    header[34..36].copy_from_slice(&(count as u16).to_be_bytes());
    tables.insert(metrics_tag, metrics);
    Ok(())
}
