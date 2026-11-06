use super::*;
use crate::{components, identity, Glyph};
pub struct Export {
    pub bytes: Vec<u8>,
    pub expected: FontDocument,
    pub composites: usize,
    pub helpers: usize,
}
fn helper(doc: &mut FontDocument, contours: Vec<Contour>) -> Result<u32> {
    if doc.glyphs.len() >= 65535 {
        return Err("font_component_limit".into());
    }
    let id = doc
        .glyphs
        .keys()
        .next_back()
        .copied()
        .unwrap_or(0)
        .checked_add(1)
        .ok_or_else(bad)?;
    let mut glyph = Glyph::new(id, ' ');
    glyph.codepoints.clear();
    glyph.advance = 0.;
    glyph.contours = contours;
    for c in &mut glyph.contours {
        c.identity = Default::default();
        identity::ensure_contour(c).map_err(|_| bad())?;
    }
    let mut n = id;
    loop {
        glyph.name = format!("_fonts.part.{n}");
        if !doc.glyphs.values().any(|g| g.name == glyph.name) {
            break;
        }
        n = n.checked_add(1).ok_or_else(bad)?;
    }
    doc.glyphs.insert(id, glyph);
    Ok(id)
}
fn matrix(m: [f64; 6]) -> Result<[i16; 6]> {
    let mut out = [0; 6];
    for i in 0..6 {
        let n = if i < 4 { m[i] * 16384. } else { m[i] };
        if !n.is_finite()
            || n < i16::MIN as f64
            || n > i16::MAX as f64
            || (n - n.round()).abs() > 1e-8
        {
            return Err("font_component_transform".into());
        }
        out[i] = n.round() as i16;
    }
    Ok(out)
}
/// Emit shared helper glyphs and nested composites, keeping the project untouched.
pub fn export(source: &FontDocument) -> Result<Export> {
    source.validate().map_err(|e| e.to_string())?;
    let mut doc = source.clone();
    doc.components = components::Library::default();
    let mut helper_ids = BTreeMap::new();
    let mut records = BTreeMap::<u32, Vec<(u32, [i16; 6])>>::new();
    let linked_glyphs: BTreeSet<_> = source
        .components
        .instances
        .values()
        .map(|i| i.glyph)
        .collect();
    let mut needed: BTreeSet<_> = source
        .components
        .instances
        .values()
        .map(|i| i.definition)
        .collect();
    let mut pending: Vec<_> = needed.iter().copied().collect();
    while let Some(id) = pending.pop() {
        for child in source.components.definitions[&id].children.values() {
            if needed.insert(child.definition) {
                pending.push(child.definition);
            }
        }
    }
    for id in source
        .components
        .order()
        .map_err(|e| e.to_string())?
        .into_iter()
        .filter(|id| needed.contains(id))
    {
        let d = &source.components.definitions[&id];
        let matches = |g: &&Glyph| {
            d.children.is_empty()
                && !linked_glyphs.contains(&g.id)
                && g.contours.len() == d.contours.len()
                && g.contours.iter().zip(d.contours.iter()).all(|(a, b)| {
                    a.start == b.start && a.segments == b.segments && a.closed == b.closed
                })
        };
        let target = match source.glyphs.get(&d.source_glyph).filter(matches) {
            Some(g) => g.id,
            None => helper(&mut doc, d.contours.as_ref().clone())?,
        };
        helper_ids.insert(id, target);
        if !d.children.is_empty() {
            let mut children: Vec<_> = d.children.values().collect();
            children.sort_by_key(|child| {
                let first = source.components.definitions[&child.definition]
                    .contours
                    .first()
                    .and_then(|c| c.identity.contours.first())
                    .and_then(|id| child.nodes.get(id));
                d.contours
                    .iter()
                    .position(|c| c.identity.contours.first() == first)
                    .unwrap_or(usize::MAX)
            });
            let mut parts = children
                .iter()
                .map(|child| Ok((helper_ids[&child.definition], matrix(child.matrix)?)))
                .collect::<Result<Vec<_>>>()?;
            let linked = source.components.definition_linked_contours(id);
            add_ordinary(&mut doc, &mut parts, &d.contours, &linked)?;
            records.insert(target, parts);
        }
    }
    let mut instances: Vec<_> = source.components.instances.values().collect();
    instances.sort_by_key(|i| {
        let first = source.components.definitions[&i.definition]
            .contours
            .first()
            .and_then(|c| c.identity.contours.first())
            .and_then(|id| i.nodes.get(id));
        (
            i.glyph,
            source.glyphs[&i.glyph]
                .contours
                .iter()
                .position(|c| c.identity.contours.first() == first)
                .unwrap_or(usize::MAX),
        )
    });
    for i in instances {
        records
            .entry(i.glyph)
            .or_default()
            .push((helper_ids[&i.definition], matrix(i.matrix)?));
    }
    for &glyph in &linked_glyphs {
        let parts = records.get_mut(&glyph).ok_or_else(bad)?;
        add_ordinary(
            &mut doc,
            parts,
            &source.glyphs[&glyph].contours,
            &source.components.linked_contours(glyph),
        )?;
    }
    doc.validate().map_err(|e| e.to_string())?;
    let base = font_io::export(&doc)?;
    if records.is_empty() {
        return Ok(Export {
            bytes: base,
            expected: doc,
            composites: 0,
            helpers: 0,
        });
    }
    let indices: BTreeMap<_, _> = doc
        .glyphs
        .keys()
        .enumerate()
        .map(|(i, &id)| (id, i as u16))
        .collect();
    let raw = ttf_parser::RawFace::parse(&base, 0).map_err(|_| bad())?;
    let mut tables: BTreeMap<[u8; 4], Vec<u8>> = raw
        .table_records
        .into_iter()
        .map(|r| {
            (
                r.tag.to_bytes(),
                raw.table(r.tag).unwrap_or_default().to_vec(),
            )
        })
        .collect();
    let old = tables.get(b"glyf").ok_or_else(bad)?;
    let loca = tables.get(b"loca").ok_or_else(bad)?;
    let offsets: Vec<_> = loca
        .as_chunks::<4>()
        .0
        .iter()
        .map(|b| u32::from_be_bytes(*b) as usize)
        .collect();
    let mut glyf = vec![];
    let mut locations = vec![];
    let mut max_parts = 0u16;
    for (i, &id) in doc.glyphs.keys().enumerate() {
        locations.extend((glyf.len() as u32).to_be_bytes());
        if let Some(parts) = records.get(&id) {
            max_parts = max_parts.max(u16::try_from(parts.len()).map_err(|_| bad())?);
            glyf.extend((-1i16).to_be_bytes());
            glyf.extend([0u8; 8]);
            for (n, (child, m)) in parts.iter().enumerate() {
                let flags = 0x1483u16 | if n + 1 < parts.len() { 0x20 } else { 0 };
                glyf.extend(flags.to_be_bytes());
                glyf.extend(indices[child].to_be_bytes());
                for v in [m[4], m[5], m[0], m[1], m[2], m[3]] {
                    glyf.extend(v.to_be_bytes());
                }
            }
        } else {
            glyf.extend(old.get(offsets[i]..offsets[i + 1]).ok_or_else(bad)?);
        }
        while glyf.len() % 4 != 0 {
            glyf.push(0);
        }
    }
    locations.extend((glyf.len() as u32).to_be_bytes());
    tables.insert(*b"glyf", glyf);
    tables.insert(*b"loca", locations);
    let head = tables.get_mut(b"head").ok_or_else(bad)?;
    head[8..12].fill(0);
    let flags = u16::from_be_bytes([head[16], head[17]]) & !2;
    head[16..18].copy_from_slice(&flags.to_be_bytes());
    let temp = crate::color_fonts::assemble(tables.clone(), false)?;
    let shapes = decode(&temp, 0)?;
    let mut max_points = 0;
    let mut max_contours = 0;
    let mut global = [0i16; 4];
    let loc = tables[b"loca"].clone();
    let glyf = tables.get_mut(b"glyf").ok_or_else(bad)?;
    for (&id, s) in &shapes {
        if s.rings.is_empty() {
            continue;
        }
        let mut min = [f64::INFINITY; 2];
        let mut max = [f64::NEG_INFINITY; 2];
        for (p, _) in s.rings.iter().flatten() {
            for a in 0..2 {
                min[a] = min[a].min(p[a]);
                max[a] = max[a].max(p[a]);
            }
        }
        let b = [min[0].floor(), min[1].floor(), max[0].ceil(), max[1].ceil()];
        if b.iter()
            .any(|&n| n < i16::MIN as f64 || n > i16::MAX as f64)
        {
            return Err("font_coordinate_range".into());
        }
        let b = b.map(|v| v as i16);
        global = [
            global[0].min(b[0]),
            global[1].min(b[1]),
            global[2].max(b[2]),
            global[3].max(b[3]),
        ];
        if !s.parts.is_empty() {
            let points =
                u16::try_from(s.rings.iter().map(Vec::len).sum::<usize>()).map_err(|_| bad())?;
            max_points = max_points.max(points);
            max_contours = max_contours.max(s.rings.len() as u16);
            let offset = usize::from(id) * 4;
            let at = u32::from_be_bytes(loc[offset..offset + 4].try_into().unwrap()) as usize;
            for (j, v) in b.iter().enumerate() {
                glyf[at + 2 + j * 2..at + 4 + j * 2].copy_from_slice(&v.to_be_bytes());
            }
        }
    }
    let head = tables.get_mut(b"head").unwrap();
    for (j, n) in global.iter().enumerate() {
        head[36 + j * 2..38 + j * 2].copy_from_slice(&n.to_be_bytes());
    }
    fn depth(
        id: u32,
        records: &BTreeMap<u32, Vec<(u32, [i16; 6])>>,
        memo: &mut BTreeMap<u32, u16>,
    ) -> u16 {
        if let Some(&n) = memo.get(&id) {
            return n;
        }
        let n = records.get(&id).map_or(0, |parts| {
            1 + parts
                .iter()
                .map(|(id, _)| depth(*id, records, memo))
                .max()
                .unwrap_or(0)
        });
        memo.insert(id, n);
        n
    }
    let mut memo = BTreeMap::new();
    let max_depth = records
        .keys()
        .map(|&id| depth(id, &records, &mut memo))
        .max()
        .unwrap_or(0);
    let maxp = tables.get_mut(b"maxp").ok_or_else(bad)?;
    for (offset, n) in [
        (10, max_points),
        (12, max_contours),
        (28, max_parts),
        (30, max_depth),
    ] {
        maxp[offset..offset + 2].copy_from_slice(&n.to_be_bytes());
    }
    let bytes = crate::color_fonts::assemble(tables, false)?;
    crate::variations::verify(&bytes, &doc)?;
    Ok(Export {
        bytes,
        composites: records.len(),
        helpers: doc.glyphs.len() - source.glyphs.len(),
        expected: doc,
    })
}

fn add_ordinary(
    doc: &mut FontDocument,
    parts: &mut Vec<(u32, [i16; 6])>,
    contours: &[Contour],
    linked: &BTreeSet<identity::Uuid>,
) -> Result<()> {
    let own: Vec<_> = contours
        .iter()
        .filter(|c| {
            !c.identity
                .contours
                .first()
                .is_some_and(|id| linked.contains(id))
        })
        .cloned()
        .collect();
    if !own.is_empty() {
        parts.insert(0, (helper(doc, own)?, [16384, 0, 0, 16384, 0, 0]));
    }
    Ok(())
}
