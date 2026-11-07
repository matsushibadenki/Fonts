//! Freeze unedited inherited metrics before detaching a composite's shared metric source.
use super::*;
use crate::source_font::{
    master_gvar::{self, Tuple},
    master_tables,
};
use std::collections::{BTreeSet, VecDeque};
type Tables = BTreeMap<[u8; 4], Vec<u8>>;
#[derive(Default)]
pub(super) struct Report {
    pub origins: BTreeMap<u16, [f64; 2]>,
    pub advances: BTreeMap<u16, [f64; 2]>,
}
fn leaf(metadata: &[initialize::Glyph], mut id: u16) -> Result<u16> {
    for _ in 0..64 {
        if let Some(child) = metadata[usize::from(id)].inherit {
            id = child;
        } else {
            return Ok(id);
        }
    }
    Err(Error::InvalidFile)
}
pub(super) fn apply(
    face: &ttf_parser::Face<'_>,
    axes: &[String],
    horizontal: &BTreeMap<u16, Vec<(Support, f64)>>,
    vertical: &BTreeMap<u16, Vec<(Support, f64)>>,
    bounds: &[Option<[i32; 4]>],
    tables: &mut Tables,
    force_all: bool,
) -> Result<Report> {
    let mut report = Report::default();
    if face
        .raw_face()
        .table(ttf_parser::Tag::from_bytes(b"glyf"))
        .is_none()
    {
        return Ok(report);
    }
    let metadata = initialize::metadata(face, false)?;
    let has_vertical = face.glyph_ver_advance(ttf_parser::GlyphId(0)).is_some();
    let vertical_metadata = if has_vertical {
        Some(initialize::metadata(face, true)?)
    } else {
        None
    };
    let mut parents = vec![vec![]; metadata.len()];
    for (id, glyph) in metadata.iter().enumerate() {
        if let Some(child) = glyph.inherit {
            parents[usize::from(child)].push(id as u16);
        }
    }
    let changed: BTreeSet<_> = horizontal
        .iter()
        .chain(vertical.iter())
        .filter_map(|(&id, row)| row.iter().any(|(_, v)| *v != 0.).then_some(id))
        .collect();
    let mut selected: BTreeSet<_> = changed
        .iter()
        .filter(|&&id| metadata[usize::from(id)].inherit.is_some())
        .copied()
        .collect();
    if force_all {
        selected.extend(
            metadata
                .iter()
                .enumerate()
                .filter_map(|(id, g)| g.inherit.map(|_| id as u16)),
        );
    }
    let mut queue: VecDeque<_> = changed.into_iter().collect();
    while let Some(child) = queue.pop_front() {
        for &parent in &parents[usize::from(child)] {
            if selected.insert(parent) {
                queue.push_back(parent);
            }
        }
    }
    if selected.is_empty() {
        return Ok(report);
    }
    let copy_horizontal = !tables.contains_key(b"HVAR");
    let copy_vertical = !tables.contains_key(b"VVAR");
    let raw = face.raw_face();
    let gvar = raw
        .table(ttf_parser::Tag::from_bytes(b"gvar"))
        .map(|bytes| master_gvar::Table::parse(bytes, axes, face.number_of_glyphs()))
        .transpose()?;
    let mut horizontal_defaults = BTreeMap::new();
    let mut vertical_defaults = BTreeMap::new();
    let mut horizontal_bearings = BTreeMap::new();
    let mut vertical_bearings = BTreeMap::new();
    let mut replacements = BTreeMap::new();
    for &id in &selected {
        let mut moving_phantoms = [false; 4];
        let child = leaf(&metadata, id)?;
        let child_id = ttf_parser::GlyphId(child);
        let parent_box = bounds[usize::from(id)].unwrap_or([0; 4]);
        let child_box = bounds[usize::from(child)].unwrap_or([0; 4]);
        let origin = child_box[0]
            - i32::from(
                face.glyph_hor_side_bearing(child_id)
                    .ok_or(Error::InvalidFile)?,
            );
        horizontal_bearings.insert(
            id,
            i16::try_from(parent_box[0] - origin).map_err(|_| Error::InvalidGeometry)?,
        );
        if copy_horizontal {
            horizontal_defaults.insert(
                id,
                i32::from(metadata[usize::from(child)].advance)
                    - i32::from(metadata[usize::from(id)].advance),
            );
        }
        if let Some(vertical_metadata) = &vertical_metadata {
            let origin = child_box[3]
                + i32::from(
                    face.glyph_ver_side_bearing(child_id)
                        .ok_or(Error::InvalidFile)?,
                );
            vertical_bearings.insert(
                id,
                i16::try_from(origin - parent_box[3]).map_err(|_| Error::InvalidGeometry)?,
            );
            if copy_vertical {
                vertical_defaults.insert(
                    id,
                    i32::from(vertical_metadata[usize::from(child)].advance)
                        - i32::from(vertical_metadata[usize::from(id)].advance),
                );
            }
        }
        if let Some(gvar) = &gvar {
            if copy_horizontal || copy_vertical {
                let count = metadata[usize::from(id)].points;
                let mut result = vec![];
                for tuple in gvar.glyph(id, count + 4)? {
                    let mut values: Vec<[i16; 2]> = tuple
                        .expand(&vec![[0.; 2]; count + 4], &[])?
                        .into_iter()
                        .map(|p| p.map(|v| v as i16))
                        .collect();
                    if copy_horizontal {
                        values[count] = [0; 2];
                        values[count + 1] = [0; 2];
                    }
                    if copy_vertical {
                        values[count + 2] = [0; 2];
                        values[count + 3] = [0; 2];
                    }
                    if values.iter().any(|v| *v != [0; 2]) {
                        result.push(Tuple {
                            support: tuple.support,
                            points: None,
                            deltas: values,
                        });
                    }
                }
                let child_count = metadata[usize::from(child)].points;
                for tuple in gvar.glyph(child, child_count + 4)? {
                    let mut values = vec![[0; 2]; count + 4];
                    for phantom in 0..4 {
                        if (phantom < 2 && copy_horizontal) || (phantom >= 2 && copy_vertical) {
                            let index = child_count + phantom;
                            let source = tuple
                                .points
                                .as_ref()
                                .map_or(Some(index), |points| points.binary_search(&index).ok());
                            if let Some(source) = source {
                                values[count + phantom] =
                                    *tuple.deltas.get(source).ok_or(Error::InvalidFile)?;
                                let axis = if phantom < 2 { 0 } else { 1 };
                                moving_phantoms[phantom] |= values[count + phantom][axis] != 0;
                            }
                        }
                    }
                    if values.iter().any(|v| *v != [0; 2]) {
                        result.push(Tuple {
                            support: tuple.support,
                            points: None,
                            deltas: values,
                        });
                    }
                }
                if result.len() > 4095
                    || result
                        .len()
                        .checked_mul(count + 4)
                        .is_none_or(|n| n > 4_000_000)
                {
                    return Err(Error::TooLarge);
                }
                replacements.insert(id, (count + 4, result));
            }
        }
        let origins = [
            if moving_phantoms[0] { 0.5 } else { 0. },
            // FreeType also rounds the derived vertical bearing back to font units.
            if moving_phantoms[2] { 1. } else { 0. },
        ];
        let advances = [
            f64::from(u8::from(moving_phantoms[0]) + u8::from(moving_phantoms[1])) * 0.5,
            f64::from(u8::from(moving_phantoms[2]) + u8::from(moving_phantoms[3])) * 0.5,
        ];
        if origins != [0.; 2] {
            report.origins.insert(id, origins);
        }
        if advances != [0.; 2] {
            report.advances.insert(id, advances);
        }
    }
    master_tables::detach_inheritance(tables, &selected)?;
    master_tables::set_bearings(tables, &horizontal_bearings, false)?;
    master_tables::set_bearings(tables, &vertical_bearings, true)?;
    master_tables::clear_metric_sidebearings(tables)?;
    master_tables::change_advances(tables, bounds, &horizontal_defaults, false)?;
    if has_vertical {
        master_tables::change_advances(tables, bounds, &vertical_defaults, true)?;
    }
    if !replacements.is_empty() {
        let changes = replacements
            .iter()
            .map(|(&id, (count, tuples))| (id, (*count, tuples.as_slice())))
            .collect();
        let old = tables.get(b"gvar").ok_or(Error::InvalidFile)?;
        let rewritten = master_gvar::replace_many(old, axes, face.number_of_glyphs(), &changes)?;
        tables.insert(*b"gvar", rewritten);
    }
    Ok(report)
}
