//! Preserve moving phantom origins by editing advances in the original gvar domain.
use super::*;
use crate::source_font::{
    master_gvar::{self, Tuple},
    master_tables,
};
type Errors = BTreeMap<u16, Vec<(Support, f64)>>;
pub(super) fn apply(
    face: &ttf_parser::Face<'_>,
    axes: &[String],
    changes: &BTreeMap<u16, Vec<(Support, f64)>>,
    vertical: bool,
    bounds: &[Option<[i32; 4]>],
    tables: &mut BTreeMap<[u8; 4], Vec<u8>>,
) -> Result<Errors> {
    let metadata = initialize::metadata(face, vertical)?;
    let source = tables.get(b"gvar").ok_or(Error::InvalidFile)?;
    let parsed = master_gvar::Table::parse(source, axes, face.number_of_glyphs())?;
    let mut replacements = BTreeMap::new();
    let mut defaults = BTreeMap::new();
    let mut errors = Errors::new();
    for (&glyph, corrections) in changes {
        if corrections.len() > 64 {
            return Err(Error::TooLarge);
        }
        let count = metadata
            .get(usize::from(glyph))
            .ok_or(Error::MissingGlyph)?
            .points
            + 4;
        let mut tuples = parsed.glyph(glyph, count)?;
        let mut constant = 0f64;
        let mut row_errors = vec![];
        for (support, value) in corrections {
            let peak = support
                .axes
                .iter()
                .map(|(tag, v)| (tag.clone(), v[1]))
                .collect();
            support.weight(&peak)?;
            if !value.is_finite()
                || value.abs() > 1e9
                || support.axes.keys().any(|tag| !axes.contains(tag))
            {
                return Err(Error::InvalidGeometry);
            }
            if support.axes.values().all(|v| v[1] == 0.) {
                constant += value;
                continue;
            }
            let rounded = value.round();
            let error = (value - rounded).abs();
            if error > 0. {
                row_errors.push((support.clone(), error));
            }
            let mut remaining = if vertical {
                -(rounded as i32)
            } else {
                rounded as i32
            };
            let phantom = if vertical { count - 1 } else { count - 3 };
            while remaining != 0 {
                if tuples.len() >= 4095 {
                    return Err(Error::TooLarge);
                }
                let delta = remaining.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16;
                remaining -= i32::from(delta);
                let value = if vertical { [0, delta] } else { [delta, 0] };
                // Only phantom points move. Any bridge point for a large packed index has
                // zero delta, so IUP leaves every physical contour unchanged.
                let (points, deltas) = if phantom > 65535 {
                    (vec![65535, phantom], vec![[0; 2], value])
                } else {
                    (vec![phantom], vec![value])
                };
                tuples.push(Tuple {
                    support: support.clone(),
                    points: Some(points),
                    deltas,
                });
            }
        }
        let rounded = constant.round();
        if rounded < f64::from(i32::MIN) || rounded > f64::from(i32::MAX) {
            return Err(Error::TooLarge);
        }
        if rounded != 0. {
            defaults.insert(glyph, rounded as i32);
        }
        if rounded != constant {
            row_errors.push((Support::default(), (constant - rounded).abs()));
        }
        if !row_errors.is_empty() {
            errors.insert(glyph, row_errors);
        }
        replacements.insert(glyph, (count, tuples));
    }
    let changes = replacements
        .iter()
        .map(|(&id, (count, tuples))| (id, (*count, tuples.as_slice())))
        .collect();
    let encoded = master_gvar::replace_many(source, axes, face.number_of_glyphs(), &changes)?;
    tables.insert(*b"gvar", encoded);
    master_tables::change_advances(tables, bounds, &defaults, vertical)?;
    master_tables::clear_metric_sidebearings(tables)?;
    Ok(errors)
}
