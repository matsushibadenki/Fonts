use super::*;
type IO<T> = crate::font_io::Result<T>;
fn u16at(b: &[u8], p: usize) -> IO<u16> {
    Ok(u16::from_be_bytes(
        b.get(p..p + 2)
            .ok_or("layout_invalid")?
            .try_into()
            .map_err(|_| "layout_invalid")?,
    ))
}
fn u32at(b: &[u8], p: usize) -> IO<u32> {
    Ok(u32::from_be_bytes(
        b.get(p..p + 4)
            .ok_or("layout_invalid")?
            .try_into()
            .map_err(|_| "layout_invalid")?,
    ))
}
pub(super) fn variation_cmap(cmap: &[u8]) -> IO<Option<Vec<u8>>> {
    let count = usize::from(u16at(cmap, 2)?);
    if count > 1024 {
        return Err("layout_invalid".into());
    }
    for i in 0..count {
        let at = u32at(cmap, 4 + i * 8 + 4)? as usize;
        if u16at(cmap, at)? == 14 {
            let len = u32at(cmap, at + 2)? as usize;
            if len > LIMIT {
                return Err("layout_invalid".into());
            }
            return Ok(Some(
                cmap.get(at..at + len).ok_or("layout_invalid")?.to_vec(),
            ));
        }
    }
    Ok(None)
}
fn append_variations(cmap: &[u8], variation: &[u8]) -> IO<Vec<u8>> {
    let n = usize::from(u16at(cmap, 2)?);
    let header = 4 + n * 8;
    if header > cmap.len() || n >= 1024 {
        return Err("layout_invalid".into());
    }
    let mut out = vec![];
    out.extend(0u16.to_be_bytes());
    out.extend(((n + 1) as u16).to_be_bytes());
    for i in 0..n {
        let p = 4 + i * 8;
        out.extend(&cmap[p..p + 4]);
        out.extend(
            u32at(cmap, p + 4)?
                .checked_add(8)
                .ok_or("layout_invalid")?
                .to_be_bytes(),
        );
    }
    out.extend(0u16.to_be_bytes());
    out.extend(5u16.to_be_bytes());
    out.extend(((cmap.len() + 8) as u32).to_be_bytes());
    out.extend(&cmap[header..]);
    out.extend(variation);
    Ok(out)
}
pub(super) fn default_vertical(
    doc: &FontDocument,
    face: &ttf_parser::Face<'_>,
) -> IO<(Vec<u8>, Vec<u8>)> {
    vertical(&ImportedLayout::default(), doc, face)
}
fn vertical(
    layout: &ImportedLayout,
    doc: &FontDocument,
    face: &ttf_parser::Face<'_>,
) -> IO<(Vec<u8>, Vec<u8>)> {
    let custom = layout.tables.contains_key("vhea");
    let mut h = if custom {
        layout.tables["vhea"].data.as_ref().clone()
    } else {
        let mut h = vec![0; 36];
        h[..4].copy_from_slice(&0x00010000u32.to_be_bytes());
        h[4..6].copy_from_slice(&((doc.units_per_em / 2) as i16).to_be_bytes());
        h[6..8]
            .copy_from_slice(&(-((doc.units_per_em - doc.units_per_em / 2) as i16)).to_be_bytes());
        h[20..22].copy_from_slice(&1i16.to_be_bytes());
        h
    };
    if h.len() < 36 {
        return Err("layout_invalid".into());
    }
    let count = if custom {
        usize::from(u16at(&h, 34)?)
    } else {
        0
    };
    if custom && (count == 0 || count > layout.glyphs.len()) {
        return Err("layout_invalid".into());
    }
    let mut out = vec![];
    let mut max_advance = 0u16;
    let mut min_top = i32::MAX;
    let mut min_bottom = i32::MAX;
    let mut max_extent = i32::MIN;
    let vorg = layout.tables.get("VORG");
    for (i, _) in doc.glyphs.values().enumerate() {
        let bbox = face.glyph_bounding_box(ttf_parser::GlyphId(i as u16));
        let ymin = bbox.map_or(0, |b| i32::from(b.y_min));
        let ymax = bbox.map_or(0, |b| i32::from(b.y_max));
        let height = ymax - ymin;
        let (advance, mut bearing) = if custom && i < layout.glyphs.len() {
            let m = layout.tables["vmtx"].data.as_ref();
            (
                u16at(m, if i < count { i * 4 } else { (count - 1) * 4 })?,
                i32::from(u16at(
                    m,
                    if i < count {
                        i * 4 + 2
                    } else {
                        count * 4 + (i - count) * 2
                    },
                )? as i16),
            )
        } else {
            (doc.units_per_em, doc.ascender.round() as i32 - ymax)
        };
        if let Some(vorg) = vorg.filter(|_| i < layout.glyphs.len()) {
            let data = vorg.data.as_ref();
            let mut origin = i32::from(u16at(data, 4)? as i16);
            let count = usize::from(u16at(data, 6)?);
            for n in 0..count {
                if usize::from(u16at(data, 8 + n * 4)?) == i {
                    origin = i32::from(u16at(data, 10 + n * 4)? as i16);
                    break;
                }
            }
            bearing = origin - ymax;
        }
        if !custom && vorg.is_none() {
            let upper = (i32::from(i16::MAX) - height)
                .min(i32::from(advance) - height - i32::from(i16::MIN))
                .min(i32::from(i16::MAX));
            bearing = bearing.clamp(i32::from(i16::MIN), upper);
        }
        let encoded = i16::try_from(bearing).map_err(|_| "font_coordinate_range")?;
        out.extend(advance.to_be_bytes());
        out.extend(encoded.to_be_bytes());
        max_advance = max_advance.max(advance);
        min_top = min_top.min(bearing);
        min_bottom = min_bottom.min(i32::from(advance) - bearing - height);
        max_extent = max_extent.max(bearing + height);
    }
    for (offset, value) in [(12, min_top), (14, min_bottom), (16, max_extent)] {
        h[offset..offset + 2].copy_from_slice(
            &i16::try_from(value)
                .map_err(|_| "font_coordinate_range")?
                .to_be_bytes(),
        );
    }
    h[10..12].copy_from_slice(&max_advance.to_be_bytes());
    h[34..36].copy_from_slice(&(doc.glyphs.len() as u16).to_be_bytes());
    Ok((h, out))
}
pub(super) fn apply(
    layout: &ImportedLayout,
    doc: &FontDocument,
    font: Vec<u8>,
    cff: bool,
) -> IO<Vec<u8>> {
    layout.validate().map_err(|_| "layout_invalid")?;
    if !layout.compatible(doc) {
        return Err("layout_index_changed".into());
    }
    if layout.positioning()
        && (layout.kerning != doc.kerning || layout.classes != doc.class_kerning)
    {
        return Err("layout_positioning_linked".into());
    }
    let raw = ttf_parser::RawFace::parse(&font, 0).map_err(|_| "layout_invalid")?;
    let mut tables: BTreeMap<_, _> = raw
        .table_records
        .into_iter()
        .map(|r| {
            (
                r.tag.to_bytes(),
                raw.table(r.tag).unwrap_or_default().to_vec(),
            )
        })
        .collect();
    for (tag, value) in &layout.tables {
        if ["UVS ", "vhea", "vmtx"].contains(&tag.as_str()) || (!cff && tag == "VORG") {
            continue;
        }
        let tag: [u8; 4] = tag.as_bytes().try_into().map_err(|_| "layout_invalid")?;
        tables.insert(tag, value.data.as_ref().clone());
    }
    if layout.positioning()
        && layout.tables.contains_key("kern")
        && !layout.tables.contains_key("GPOS")
    {
        tables.remove(b"GPOS");
    }
    if let Some(uvs) = layout.tables.get("UVS ") {
        let cmap = append_variations(tables.get(b"cmap").ok_or("layout_invalid")?, &uvs.data)?;
        tables.insert(*b"cmap", cmap);
    }
    {
        let face = ttf_parser::Face::parse(&font, 0).map_err(|_| "layout_invalid")?;
        let (h, m) = vertical(layout, doc, &face)?;
        tables.insert(*b"vhea", h);
        tables.insert(*b"vmtx", m);
    }
    tables.get_mut(b"head").ok_or("layout_invalid")?[8..12].fill(0);
    crate::color_fonts::assemble(tables, cff)
}

pub(super) fn validate_tables(layout: &ImportedLayout) -> IO<()> {
    for (tag, t) in &layout.tables {
        let b = t.data.as_slice();
        match tag.as_str() {
            "GSUB" | "GPOS" => {
                let version = u32at(b, 0)?;
                if version != 0x10000 && version != 0x10001 {
                    return Err("layout_invalid".into());
                }
                if version == 0x10001 {
                    let n = u32at(b, 10)? as usize;
                    if n != 0 && b.get(n..n + 8).is_none() {
                        return Err("layout_invalid".into());
                    }
                }
                for (field, stride, minimum) in [(4, 6, 4), (6, 6, 4), (8, 2, 6)] {
                    let at = usize::from(u16at(b, field)?);
                    if at == 0 {
                        return Err("layout_invalid".into());
                    }
                    let count = usize::from(u16at(b, at)?);
                    let length = 2 + count * stride;
                    if b.get(at..at + length).is_none() {
                        return Err("layout_invalid".into());
                    }
                    for i in 0..count {
                        let p = at + 2 + i * stride + stride - 2;
                        let offset = usize::from(u16at(b, p)?);
                        if offset == 0 || b.get(at + offset..at + offset + minimum).is_none() {
                            return Err("layout_invalid".into());
                        }
                    }
                }
            }
            "GDEF" => {
                let v = u32at(b, 0)?;
                let size = match v {
                    0x10000 => 12,
                    0x10002 => 14,
                    0x10003 => 18,
                    _ => return Err("layout_invalid".into()),
                };
                if b.len() < size {
                    return Err("layout_invalid".into());
                }
            }
            "BASE" => {
                if ![0x10000, 0x10001].contains(&u32at(b, 0)?) || b.len() < 8 {
                    return Err("layout_invalid".into());
                }
            }
            "JSTF" => {
                if u32at(b, 0)? != 0x10000 || b.len() < 6 {
                    return Err("layout_invalid".into());
                }
            }
            "vhea" => {
                if ![0x10000, 0x11000].contains(&u32at(b, 0)?) || b.len() < 36 {
                    return Err("layout_invalid".into());
                }
                let n = usize::from(u16at(b, 34)?);
                if n == 0 || n > layout.glyphs.len() {
                    return Err("layout_invalid".into());
                }
                let m = layout.tables.get("vmtx").ok_or("layout_invalid")?;
                if m.data.len() < n * 4 + (layout.glyphs.len() - n) * 2 {
                    return Err("layout_invalid".into());
                }
            }
            "vmtx" => {}
            "VORG" => {
                if u32at(b, 0)? != 0x10000 {
                    return Err("layout_invalid".into());
                }
                let n = usize::from(u16at(b, 6)?);
                if b.len() < 8 + n * 4 {
                    return Err("layout_invalid".into());
                }
                let mut last = None;
                for i in 0..n {
                    let gid = u16at(b, 8 + i * 4)?;
                    if usize::from(gid) >= layout.glyphs.len() || last.is_some_and(|p| p >= gid) {
                        return Err("layout_invalid".into());
                    }
                    last = Some(gid);
                }
            }
            "kern" => {
                if b.len() < 4 {
                    return Err("layout_invalid".into());
                }
            }
            "UVS " => {
                if u16at(b, 0)? != 14 || u32at(b, 2)? as usize != b.len() {
                    return Err("layout_invalid".into());
                }
                let count = u32at(b, 6)? as usize;
                if count > 256 || b.len() < 10 + count * 11 {
                    return Err("layout_invalid".into());
                }
                let read24 = |p: usize| -> IO<u32> {
                    let a = b.get(p..p + 3).ok_or("layout_invalid")?;
                    Ok(u32::from_be_bytes([0, a[0], a[1], a[2]]))
                };
                let mut previous = 0;
                for i in 0..count {
                    let p = 10 + i * 11;
                    let selector = read24(p)?;
                    if !((0xfe00..=0xfe0f).contains(&selector)
                        || (0xe0100..=0xe01ef).contains(&selector))
                        || selector <= previous
                    {
                        return Err("layout_invalid".into());
                    }
                    previous = selector;
                    for (field, stride) in [(p + 3, 4), (p + 7, 5)] {
                        let at = u32at(b, field)? as usize;
                        if at == 0 {
                            continue;
                        }
                        let n = u32at(b, at)? as usize;
                        if n > 0x110000 || b.get(at..at + 4 + n * stride).is_none() {
                            return Err("layout_invalid".into());
                        }
                        let mut last = None;
                        for j in 0..n {
                            let offset = at + 4 + j * stride;
                            let base = read24(offset)?;
                            let end = if stride == 4 {
                                base + u32::from(b[offset + 3])
                            } else {
                                if usize::from(u16at(b, offset + 3)?) >= layout.glyphs.len() {
                                    return Err("layout_invalid".into());
                                }
                                base
                            };
                            if end > 0x10ffff
                                || (base <= 0xdfff && end >= 0xd800)
                                || last.is_some_and(|v| v >= base)
                            {
                                return Err("layout_invalid".into());
                            }
                            last = Some(end);
                        }
                    }
                }
            }
            _ => return Err("layout_invalid".into()),
        }
    }
    Ok(())
}
