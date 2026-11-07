//! Batch default-glyph updates with ancestor bounds and base metric origins preserved.
mod layout;
mod metrics;
mod remap;
use super::{master_glyf, master_gvar, master_rebase::Rebased};
use crate::{Error, Result};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
type Tables = BTreeMap<[u8; 4], Vec<u8>>;
pub(super) fn clear_metric_sidebearings(tables: &mut Tables) -> Result<()> {
    metrics::clear_sidebearing_maps(tables)
}
pub(super) fn change_advances(
    tables: &mut BTreeMap<[u8; 4], Vec<u8>>,
    bounds: &[Option<[i32; 4]>],
    deltas: &BTreeMap<u16, i32>,
    vertical: bool,
) -> Result<()> {
    metrics::update_advances(tables, bounds, bounds, &BTreeSet::new(), vertical, deltas)
}
pub(super) fn word(bytes: &[u8], at: usize) -> Result<u16> {
    Ok(u16::from_be_bytes(
        bytes
            .get(at..at + 2)
            .ok_or(Error::InvalidFile)?
            .try_into()
            .map_err(|_| Error::InvalidFile)?,
    ))
}
fn set(bytes: &mut [u8], at: usize, value: i32) -> Result<()> {
    bytes
        .get_mut(at..at + 2)
        .ok_or(Error::InvalidFile)?
        .copy_from_slice(
            &i16::try_from(value)
                .map_err(|_| Error::InvalidGeometry)?
                .to_be_bytes(),
        );
    Ok(())
}
fn offsets(tables: &Tables, count: usize) -> Result<Vec<usize>> {
    let head = tables.get(b"head").ok_or(Error::InvalidFile)?;
    let loca = tables.get(b"loca").ok_or(Error::InvalidFile)?;
    let glyf = tables.get(b"glyf").ok_or(Error::InvalidFile)?;
    let stride = match word(head, 50)? {
        0 => 2,
        1 => 4,
        _ => return Err(Error::InvalidFile),
    };
    let mut out = Vec::with_capacity(count + 1);
    for index in 0..=count {
        let at = index * stride;
        let value = if stride == 2 {
            usize::from(word(loca, at)?) * 2
        } else {
            u32::from_be_bytes(
                loca.get(at..at + 4)
                    .ok_or(Error::InvalidFile)?
                    .try_into()
                    .map_err(|_| Error::InvalidFile)?,
            ) as usize
        };
        if value > glyf.len() || out.last().is_some_and(|v| *v > value) {
            return Err(Error::InvalidFile);
        }
        out.push(value);
    }
    Ok(out)
}
fn component_records(bytes: &[u8]) -> Result<(Vec<u16>, Vec<usize>, usize)> {
    let mut at = 10usize;
    let mut children = vec![];
    let mut flags_at = vec![];
    loop {
        if children.len() >= 4096 {
            return Err(Error::TooLarge);
        }
        let flags = word(bytes, at)?;
        flags_at.push(at);
        children.push(word(bytes, at + 2)?);
        at += 4 + if flags & 1 != 0 { 4 } else { 2 };
        at += if flags & 8 != 0 {
            2
        } else if flags & 64 != 0 {
            4
        } else if flags & 128 != 0 {
            8
        } else {
            0
        };
        if at > bytes.len() {
            return Err(Error::InvalidFile);
        }
        if flags & 32 == 0 {
            break;
        }
    }
    Ok((children, flags_at, at))
}
fn bounding(bytes: &[u8]) -> Result<Option<[i32; 4]>> {
    if bytes.is_empty() {
        return Ok(None);
    }
    if word(bytes, 0)? == 0 {
        return Ok(None);
    }
    let mut out = [0; 4];
    for (index, value) in out.iter_mut().enumerate() {
        *value = i32::from(word(bytes, 2 + index * 2)? as i16);
    }
    if out[0] > out[2] || out[1] > out[3] {
        return Err(Error::InvalidFile);
    }
    Ok(Some(out))
}
/// Rebased plans retain point topology. Advances and variation metric stores remain unchanged;
/// optional sidebearing caches are cleared so consumers derive them from the edited outlines.
/// HVAR/VVAR correction coefficients are applied by the higher-level master exporter.
pub fn apply(bytes: &[u8], plans: &BTreeMap<u16, Rebased>) -> Result<Vec<u8>> {
    if bytes.len() > 256 * 1024 * 1024 {
        return Err(Error::TooLarge);
    }
    let face = ttf_parser::Face::parse(bytes, 0).map_err(|_| Error::InvalidFile)?;
    let count = usize::from(face.number_of_glyphs());
    let axes: Vec<String> = face
        .variation_axes()
        .into_iter()
        .map(|a| a.tag.to_string())
        .collect();
    if plans.is_empty() {
        return Ok(bytes.to_vec());
    }
    let mut tables = Tables::new();
    for record in face.raw_face().table_records {
        tables.insert(
            record.tag.to_bytes(),
            face.raw_face()
                .table(record.tag)
                .ok_or(Error::InvalidFile)?
                .to_vec(),
        );
    }
    let locations = offsets(&tables, count)?;
    let old = &tables[b"glyf"];
    let mut records: Vec<Vec<u8>> = locations
        .windows(2)
        .map(|w| old[w[0]..w[1]].to_vec())
        .collect();
    let mut parents = vec![vec![]; count];
    for (parent, record) in records.iter().enumerate() {
        if !record.is_empty() && word(record, 0)? as i16 == -1 {
            for child in component_records(record)?.0 {
                parents
                    .get_mut(usize::from(child))
                    .ok_or(Error::InvalidFile)?
                    .push(parent as u16);
            }
        }
    }
    let mut has_outline: Vec<bool> = records
        .iter()
        .map(|r| !r.is_empty() && word(r, 0).is_ok_and(|n| (n as i16) > 0))
        .collect();
    let mut outlined: VecDeque<usize> = has_outline
        .iter()
        .enumerate()
        .filter_map(|(i, has)| has.then_some(i))
        .collect();
    while let Some(child) = outlined.pop_front() {
        for &parent in &parents[child] {
            let p = usize::from(parent);
            if !has_outline[p] {
                has_outline[p] = true;
                outlined.push_back(p);
            }
        }
    }
    let old_bounds = records
        .iter()
        .zip(&has_outline)
        .map(|(r, has)| if *has { bounding(r) } else { Ok(None) })
        .collect::<Result<Vec<_>>>()?;
    let mut affected = BTreeSet::new();
    let mut queue = VecDeque::new();
    let mut source_counts = BTreeMap::new();
    for (&id, plan) in plans {
        let old_count = if let Some(g) = master_glyf::read(bytes, 0, id)? {
            g.points.len()
        } else {
            component_records(&records[usize::from(id)])?.0.len()
        };
        source_counts.insert(id, old_count + 4);
        let verified = master_glyf::Glyph::verify_encoded(&plan.base)?;
        if verified.bytes != plan.base.bytes || verified.bounds != plan.base.bounds {
            return Err(Error::InvalidGeometry);
        }
        records[usize::from(id)] = plan.base.bytes.clone();
        if affected.insert(id) {
            queue.push_back(id);
        }
    }
    while let Some(child) = queue.pop_front() {
        for &parent in &parents[usize::from(child)] {
            if affected.insert(parent) {
                queue.push_back(parent);
            }
        }
    }
    remap::apply(bytes, plans, &mut records, &affected, &mut tables)?;
    // Parent glyph instructions can depend on the moved child points, so remove them as well.
    for &id in &affected {
        let record = &mut records[usize::from(id)];
        if !record.is_empty() && word(record, 0)? as i16 == -1 {
            let (_, flags_at, end) = component_records(record)?;
            for at in flags_at {
                let flags = word(record, at)? & !0x100;
                record[at..at + 2].copy_from_slice(&flags.to_be_bytes());
            }
            record.truncate(end);
        }
    }
    pack(&mut tables, &records)?;
    let interim =
        crate::color_fonts::assemble(tables.clone(), false).map_err(|_| Error::InvalidFile)?;
    let shapes = crate::truetype_components::decode_selected(&interim, 0, &affected)
        .map_err(|_| Error::InvalidFile)?;
    for &id in &affected {
        let shape = shapes.get(&id).ok_or(Error::InvalidFile)?;
        let composite = records[usize::from(id)].get(..2) == Some(&[255, 255]);
        let point_count: usize = shape.rings.iter().map(Vec::len).sum();
        let maxp = tables.get_mut(b"maxp").ok_or(Error::InvalidFile)?;
        for (at, value) in [
            (if composite { 10 } else { 6 }, point_count),
            (if composite { 12 } else { 8 }, shape.rings.len()),
        ] {
            let old = word(maxp, at)?;
            let value = u16::try_from(value).map_err(|_| Error::TooLarge)?;
            maxp.get_mut(at..at + 2)
                .ok_or(Error::InvalidFile)?
                .copy_from_slice(&old.max(value).to_be_bytes());
        }
        let mut bbox: Option<[f64; 4]> = None;
        for (p, _) in shape.rings.iter().flatten() {
            let b = bbox.get_or_insert([p[0], p[1], p[0], p[1]]);
            for axis in 0..2 {
                b[axis] = b[axis].min(p[axis]);
                b[axis + 2] = b[axis + 2].max(p[axis]);
            }
        }
        let bounds = bbox.map_or([0; 4], |b| {
            [
                b[0].floor() as i32,
                b[1].floor() as i32,
                b[2].ceil() as i32,
                b[3].ceil() as i32,
            ]
        });
        let record = &mut records[usize::from(id)];
        if !record.is_empty() {
            for (index, value) in bounds.into_iter().enumerate() {
                set(record, 2 + index * 2, value)?;
            }
        }
    }
    let new_bounds = records
        .iter()
        .zip(&has_outline)
        .map(|(r, has)| if *has { bounding(r) } else { Ok(None) })
        .collect::<Result<Vec<_>>>()?;
    metrics::update(&mut tables, &old_bounds, &new_bounds, &affected, false)?;
    metrics::update(&mut tables, &old_bounds, &new_bounds, &affected, true)?;
    metrics::clear_sidebearing_maps(&mut tables)?;
    let head = tables.get_mut(b"head").ok_or(Error::InvalidFile)?;
    let mut union: Option<[i32; 4]> = None;
    for b in new_bounds.iter().flatten() {
        let u = union.get_or_insert(*b);
        for axis in 0..2 {
            u[axis] = u[axis].min(b[axis]);
            u[axis + 2] = u[axis + 2].max(b[axis + 2]);
        }
    }
    for (index, value) in union.unwrap_or([0; 4]).into_iter().enumerate() {
        set(head, 36 + index * 2, value)?;
    }
    pack(&mut tables, &records)?;
    if let Some(source) = tables.get(b"gvar") {
        let replacements = plans
            .iter()
            .map(|(&id, plan)| (id, (plan.base.points.len() + 4, plan.tuples.as_slice())))
            .collect();
        let gvar = master_gvar::replace_reindexed(
            source,
            &axes,
            count as u16,
            &replacements,
            Some(&source_counts),
        )?;
        tables.insert(*b"gvar", gvar);
    } else if plans.values().any(|p| !p.tuples.is_empty()) {
        return Err(Error::InvalidFile);
    }
    tables.remove(b"DSIG");
    crate::color_fonts::assemble(tables, false).map_err(|_| Error::InvalidFile)
}
fn pack(tables: &mut Tables, records: &[Vec<u8>]) -> Result<()> {
    let mut glyf = vec![];
    let mut loca = vec![];
    for record in records {
        loca.extend(
            u32::try_from(glyf.len())
                .map_err(|_| Error::TooLarge)?
                .to_be_bytes(),
        );
        glyf.extend(record);
        if !glyf.len().is_multiple_of(2) {
            glyf.push(0);
        }
        if glyf.len() > 256 * 1024 * 1024 {
            return Err(Error::TooLarge);
        }
    }
    loca.extend((glyf.len() as u32).to_be_bytes());
    tables.insert(*b"glyf", glyf);
    tables.insert(*b"loca", loca);
    let head = tables.get_mut(b"head").ok_or(Error::InvalidFile)?;
    head.get_mut(50..52)
        .ok_or(Error::InvalidFile)?
        .copy_from_slice(&1u16.to_be_bytes());
    Ok(())
}

pub(super) fn detach_inheritance(tables: &mut Tables, selected: &BTreeSet<u16>) -> Result<()> {
    if selected.is_empty() {
        return Ok(());
    }
    let count = usize::from(word(tables.get(b"maxp").ok_or(Error::InvalidFile)?, 4)?);
    let locations = offsets(tables, count)?;
    let old = tables.get(b"glyf").ok_or(Error::InvalidFile)?;
    let mut records: Vec<_> = locations
        .windows(2)
        .map(|w| old[w[0]..w[1]].to_vec())
        .collect();
    for &id in selected {
        let record = records
            .get_mut(usize::from(id))
            .ok_or(Error::MissingGlyph)?;
        if word(record, 0)? as i16 != -1 {
            return Err(Error::InvalidGeometry);
        }
        let (_, flags, end) = component_records(record)?;
        for at in flags {
            let value = word(record, at)? & !(0x200 | 0x100);
            record[at..at + 2].copy_from_slice(&value.to_be_bytes());
        }
        record.truncate(end);
    }
    pack(tables, &records)
}
pub(super) fn set_bearings(
    tables: &mut Tables,
    values: &BTreeMap<u16, i16>,
    vertical: bool,
) -> Result<()> {
    if values.is_empty() {
        return Ok(());
    }
    let (head, table) = if vertical {
        (*b"vhea", *b"vmtx")
    } else {
        (*b"hhea", *b"hmtx")
    };
    let longs = usize::from(word(tables.get(&head).ok_or(Error::InvalidFile)?, 34)?);
    if longs == 0 {
        return Err(Error::InvalidFile);
    }
    let bytes = tables.get_mut(&table).ok_or(Error::InvalidFile)?;
    for (&id, &value) in values {
        let id = usize::from(id);
        let at = if id < longs {
            id * 4 + 2
        } else {
            longs * 4 + (id - longs) * 2
        };
        bytes
            .get_mut(at..at + 2)
            .ok_or(Error::InvalidFile)?
            .copy_from_slice(&value.to_be_bytes());
    }
    Ok(())
}

pub(super) fn update_outline_bounds(
    tables: &mut Tables,
    old: &[Option<[i32; 4]>],
    new: &[Option<[i32; 4]>],
    affected: &BTreeSet<u16>,
) -> Result<()> {
    metrics::update(tables, old, new, affected, false)?;
    metrics::update(tables, old, new, affected, true)?;
    metrics::clear_sidebearing_maps(tables)?;
    let head = tables.get_mut(b"head").ok_or(Error::InvalidFile)?;
    let mut bbox = None::<[i32; 4]>;
    for b in new.iter().flatten() {
        let union = bbox.get_or_insert(*b);
        for axis in 0..2 {
            union[axis] = union[axis].min(b[axis]);
            union[axis + 2] = union[axis + 2].max(b[axis + 2]);
        }
    }
    for (i, value) in bbox.unwrap_or([0; 4]).into_iter().enumerate() {
        set(head, 36 + i * 2, value)?;
    }
    Ok(())
}

pub(super) fn protect_cff_point_references(bytes: &[u8], affected: &BTreeSet<u16>) -> Result<()> {
    layout::protect_cff_points(bytes, affected)
}

#[cfg(test)]
mod tests;
