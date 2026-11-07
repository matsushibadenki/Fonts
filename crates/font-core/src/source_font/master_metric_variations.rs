//! Preserve original HVAR/VVAR variation rows and append master advance corrections.
mod font;
mod gvar;
mod inheritance;
mod initialize;
mod synthesize;
use super::{
    master_store::{self, Builder, Store},
    masters::{Location, Support},
};
use crate::{Error, Result};
pub use font::{apply_font, Applied};
use std::collections::BTreeMap;
pub struct Edited {
    pub table: Vec<u8>,
    /// Apply these integer deltas to the corresponding hmtx/vmtx default advances.
    pub base: BTreeMap<u16, i32>,
    pub errors: BTreeMap<u16, Vec<(Support, f64)>>,
}
impl Edited {
    pub fn error_bound(&self, glyph: u16, location: &Location) -> Result<f64> {
        Support::default().weight(location)?;
        let mut bound = 0.;
        if let Some(errors) = self.errors.get(&glyph) {
            for (support, error) in errors {
                bound += support.weight(location)?.abs() * error;
            }
        }
        Ok(bound)
    }
}
fn offset(bytes: &[u8], at: usize) -> Result<usize> {
    Ok(u32::from_be_bytes(
        bytes
            .get(at..at + 4)
            .ok_or(Error::InvalidFile)?
            .try_into()
            .map_err(|_| Error::InvalidFile)?,
    ) as usize)
}
pub fn edit(
    table: &[u8],
    axes: &[String],
    glyphs: u16,
    changes: &BTreeMap<u16, Vec<(Support, f64)>>,
    vertical: bool,
) -> Result<Edited> {
    let header = if vertical { 24 } else { 20 };
    if table.len() > 256 * 1024 * 1024 {
        return Err(Error::TooLarge);
    }
    if table.len() < header || table[..4] != [0, 1, 0, 0] || glyphs == 0 {
        return Err(Error::InvalidFile);
    }
    if changes.is_empty() {
        return Ok(Edited {
            table: table.to_vec(),
            base: BTreeMap::new(),
            errors: BTreeMap::new(),
        });
    }
    let store_at = offset(table, 4)?;
    let map_at = offset(table, 8)?;
    if store_at < header || (map_at != 0 && map_at < header) {
        return Err(Error::InvalidFile);
    }
    let store = Store::parse(table.get(store_at..).ok_or(Error::InvalidFile)?, axes)?;
    let mut indices = if map_at == 0 {
        (0..glyphs).map(|glyph| (0, glyph)).collect()
    } else {
        master_store::read_map(table.get(map_at..).ok_or(Error::InvalidFile)?, glyphs)?
    };
    let mut builder = Builder::new(store);
    let mut base = BTreeMap::new();
    let mut errors = BTreeMap::new();
    for (&glyph, corrections) in changes {
        let original = *indices.get(usize::from(glyph)).ok_or(Error::MissingGlyph)?;
        if corrections.len() > 64 {
            return Err(Error::TooLarge);
        }
        let mut constant = 0f64;
        let mut variable = vec![];
        for (support, value) in corrections {
            let peak = support
                .axes
                .iter()
                .map(|(tag, v)| (tag.clone(), v[1]))
                .collect();
            support.weight(&peak)?;
            if !value.is_finite()
                || value.abs() > 1e9
                || support.axes.keys().any(|k| !axes.contains(k))
            {
                return Err(Error::InvalidGeometry);
            }
            if support.axes.values().all(|r| r[1] == 0.) {
                constant += value;
            } else {
                variable.push((support.clone(), *value));
            }
        }
        let rounded = constant.round();
        if rounded < f64::from(i32::MIN) || rounded > f64::from(i32::MAX) {
            return Err(Error::TooLarge);
        }
        if rounded != 0. {
            base.insert(glyph, rounded as i32);
        }
        let (index, mut row_errors) = builder.add(original, &variable)?;
        if constant != rounded {
            row_errors.push((Support::default(), (constant - rounded).abs()));
        }
        indices[usize::from(glyph)] = index;
        if !row_errors.is_empty() {
            errors.insert(glyph, row_errors);
        }
    }
    let store = builder.finish()?;
    let mapping = master_store::write_map(&indices)?;
    let mut output = table.to_vec();
    let new_store = output.len();
    output.extend(store);
    let new_map = output.len();
    output.extend(mapping);
    if output.len() > 256 * 1024 * 1024 {
        return Err(Error::TooLarge);
    }
    output[4..8].copy_from_slice(&(new_store as u32).to_be_bytes());
    output[8..12].copy_from_slice(&(new_map as u32).to_be_bytes());
    // Changed advances make cached sidebearings stale; outlines remain authoritative.
    output[12..20].fill(0);
    Ok(Edited {
        table: output,
        base,
        errors,
    })
}

pub(super) struct Frozen {
    pub bytes: Vec<u8>,
    pub origins: BTreeMap<u16, [f64; 2]>,
    pub advances: BTreeMap<u16, [f64; 2]>,
}
pub(super) fn freeze_inherited(bytes: &[u8]) -> Result<Frozen> {
    let face = ttf_parser::Face::parse(bytes, 0).map_err(|_| Error::InvalidFile)?;
    let axes: Vec<_> = face
        .variation_axes()
        .into_iter()
        .map(|a| a.tag.to_string())
        .collect();
    let bounds: Vec<_> = (0..face.number_of_glyphs())
        .map(|i| {
            face.glyph_bounding_box(ttf_parser::GlyphId(i)).map(|b| {
                [
                    i32::from(b.x_min),
                    i32::from(b.y_min),
                    i32::from(b.x_max),
                    i32::from(b.y_max),
                ]
            })
        })
        .collect();
    let mut tables: BTreeMap<_, _> = face
        .raw_face()
        .table_records
        .into_iter()
        .map(|r| {
            Ok((
                r.tag.to_bytes(),
                face.raw_face()
                    .table(r.tag)
                    .ok_or(Error::InvalidFile)?
                    .to_vec(),
            ))
        })
        .collect::<Result<_>>()?;
    let report = inheritance::apply(
        &face,
        &axes,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &bounds,
        &mut tables,
        true,
    )?;
    tables.remove(b"DSIG");
    Ok(Frozen {
        bytes: crate::color_fonts::assemble(tables, false).map_err(|_| Error::InvalidFile)?,
        origins: report.origins,
        advances: report.advances,
    })
}

#[cfg(test)]
mod tests;
