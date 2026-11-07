use super::*;
use crate::source_font::master_tables;
pub struct Applied {
    pub bytes: Vec<u8>,
    pub horizontal_errors: BTreeMap<u16, Vec<(Support, f64)>>,
    pub vertical_errors: BTreeMap<u16, Vec<(Support, f64)>>,
    /// Conservative design-unit bounds for rasterizer rounding of copied composite phantoms.
    pub native_origin_rounding: BTreeMap<u16, [f64; 2]>,
    pub native_advance_rounding: BTreeMap<u16, [f64; 2]>,
    pub generated_vertical_metrics: bool,
    pub retained_gvar_metrics: [bool; 2],
}
impl Applied {
    pub fn error_bound(&self, glyph: u16, location: &Location, vertical: bool) -> Result<f64> {
        Support::default().weight(location)?;
        let errors = if vertical {
            &self.vertical_errors
        } else {
            &self.horizontal_errors
        };
        let mut bound = 0.;
        if let Some(errors) = errors.get(&glyph) {
            for (support, error) in errors {
                bound += support.weight(location)?.abs() * error;
            }
        }
        Ok(bound)
    }
}
#[derive(Default)]
struct Outline {
    has_points: bool,
}
impl ttf_parser::OutlineBuilder for Outline {
    fn move_to(&mut self, _x: f32, _y: f32) {
        self.has_points = true;
    }
    fn line_to(&mut self, _x: f32, _y: f32) {
        self.has_points = true;
    }
    fn quad_to(&mut self, _x: f32, _y: f32, _a: f32, _b: f32) {
        self.has_points = true;
    }
    fn curve_to(&mut self, _x: f32, _y: f32, _a: f32, _b: f32, _c: f32, _d: f32) {
        self.has_points = true;
    }
    fn close(&mut self) {}
}
/// Apply corrections to variable and default metrics, initializing missing stores from source gvar.
pub fn apply_font(
    bytes: &[u8],
    horizontal: &BTreeMap<u16, Vec<(Support, f64)>>,
    vertical: &BTreeMap<u16, Vec<(Support, f64)>>,
) -> Result<Applied> {
    if bytes.len() > 256 * 1024 * 1024 {
        return Err(Error::TooLarge);
    }
    let face = ttf_parser::Face::parse(bytes, 0).map_err(|_| Error::InvalidFile)?;
    if horizontal
        .keys()
        .chain(vertical.keys())
        .any(|id| *id >= face.number_of_glyphs())
    {
        return Err(Error::MissingGlyph);
    }
    if horizontal.is_empty() && vertical.is_empty() {
        return Ok(Applied {
            bytes: bytes.to_vec(),
            horizontal_errors: BTreeMap::new(),
            vertical_errors: BTreeMap::new(),
            native_origin_rounding: BTreeMap::new(),
            native_advance_rounding: BTreeMap::new(),
            generated_vertical_metrics: false,
            retained_gvar_metrics: [false; 2],
        });
    }
    let axes: Vec<_> = face
        .variation_axes()
        .into_iter()
        .map(|a| a.tag.to_string())
        .collect();
    let mut tables = BTreeMap::new();
    for record in face.raw_face().table_records {
        tables.insert(
            record.tag.to_bytes(),
            face.raw_face()
                .table(record.tag)
                .ok_or(Error::InvalidFile)?
                .to_vec(),
        );
    }
    let mut bounds = Vec::with_capacity(usize::from(face.number_of_glyphs()));
    for glyph in 0..face.number_of_glyphs() {
        let mut outline = Outline::default();
        let rect = face.outline_glyph(ttf_parser::GlyphId(glyph), &mut outline);
        // TrueType phantom origins use the stored design-unit bbox, including composite
        // transforms that can lose a fraction when outlined through ttf-parser's f32 API.
        let rect = if let Some(glyf) = face.tables().glyf {
            glyf.bbox(ttf_parser::GlyphId(glyph))
        } else {
            rect
        };
        bounds.push(rect.filter(|_| outline.has_points).map(|r| {
            [
                i32::from(r.x_min),
                i32::from(r.y_min),
                i32::from(r.x_max),
                i32::from(r.y_max),
            ]
        }));
    }
    let mut horizontal_errors = BTreeMap::new();
    let mut vertical_errors = BTreeMap::new();
    let mut deferred = vec![];
    let mut retained_gvar_metrics = [false; 2];
    let created_vertical = if !vertical.is_empty() {
        super::synthesize::vertical(&face, &axes, &bounds, &mut tables)?
    } else {
        false
    };
    for (vertical, changes) in [(false, horizontal), (true, vertical)] {
        if changes.is_empty() {
            continue;
        }
        let tag = if vertical { *b"VVAR" } else { *b"HVAR" };
        let (header, metrics) = if vertical {
            (*b"vhea", *b"vmtx")
        } else {
            (*b"hhea", *b"hmtx")
        };
        if !tables.contains_key(&header) || !tables.contains_key(&metrics) {
            return Err(Error::InvalidFile);
        }
        let mut defaults = BTreeMap::new();
        if let std::collections::btree_map::Entry::Vacant(entry) = tables.entry(tag) {
            let Some(initialized) = super::initialize::create(&face, &axes, vertical)? else {
                deferred.push((vertical, changes));
                retained_gvar_metrics[usize::from(vertical)] = true;
                continue;
            };
            defaults = initialized.base;
            entry.insert(initialized.table);
        }
        let source = tables.get(&tag).ok_or(Error::InvalidFile)?;
        let edited = edit(source, &axes, face.number_of_glyphs(), changes, vertical)?;
        for (&glyph, &delta) in &edited.base {
            let value = defaults.entry(glyph).or_insert(0);
            *value = value.checked_add(delta).ok_or(Error::TooLarge)?;
        }
        master_tables::change_advances(&mut tables, &bounds, &defaults, vertical)?;
        tables.insert(tag, edited.table);
        if vertical {
            vertical_errors = edited.errors;
        } else {
            horizontal_errors = edited.errors;
        }
    }
    let inheritance = super::inheritance::apply(
        &face,
        &axes,
        horizontal,
        vertical,
        &bounds,
        &mut tables,
        created_vertical,
    )?;
    for (vertical, changes) in deferred {
        let errors = super::gvar::apply(&face, &axes, changes, vertical, &bounds, &mut tables)?;
        if vertical {
            vertical_errors = errors;
        } else {
            horizontal_errors = errors;
        }
    }
    tables.remove(b"DSIG");
    let cff = face
        .raw_face()
        .table(ttf_parser::Tag::from_bytes(b"CFF2"))
        .is_some();
    let output = crate::color_fonts::assemble(tables, cff).map_err(|_| Error::InvalidFile)?;
    Ok(Applied {
        bytes: output,
        horizontal_errors,
        vertical_errors,
        native_origin_rounding: inheritance.origins,
        native_advance_rounding: inheritance.advances,
        generated_vertical_metrics: created_vertical,
        retained_gvar_metrics,
    })
}
