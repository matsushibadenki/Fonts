//! Reusable worker-local face for unhinted, high-precision master sampling.
use super::{
    masters::{Location, Shape},
    variable::Range,
};
use crate::{font_io, variations, Contour, Error, GlyphId, Point, Result, Segment};
use freetype::{face::LoadFlag, ffi};
use std::{
    borrow::Borrow,
    ffi::{c_int, c_void},
    sync::Arc,
};
#[derive(Clone)]
struct Bytes(Arc<Vec<u8>>);
impl Borrow<[u8]> for Bytes {
    fn borrow(&self) -> &[u8] {
        self.0.as_slice()
    }
}
pub struct Sampler {
    face: freetype::Face<Bytes>,
    _library: freetype::Library,
    axes: Vec<variations::Axis>,
}
pub struct Sample {
    pub shape: Shape,
    pub design_location: Location,
    pub location: Location,
}
impl Sampler {
    pub fn new(bytes: Arc<Vec<u8>>, index: u32) -> Result<Self> {
        if bytes.len() as u64 > font_io::MAX_FONT_BYTES || index > 65535 {
            return Err(Error::TooLarge);
        }
        let axes = variations::axes(&bytes, index).map_err(|_| Error::InvalidDocument)?;
        let library = freetype::Library::init().map_err(|_| Error::InvalidDocument)?;
        let face = library
            .new_memory_face2(Bytes(bytes), index as isize)
            .map_err(|_| Error::InvalidDocument)?;
        Ok(Self {
            face,
            _library: library,
            axes,
        })
    }
    pub fn sample_design(&mut self, glyph: GlyphId, location: &Location) -> Result<Sample> {
        if location
            .keys()
            .any(|tag| !self.axes.iter().any(|a| &a.tag == tag))
        {
            return Err(Error::InvalidGeometry);
        }
        let coords = self
            .axes
            .iter()
            .map(|a| {
                let value = location.get(&a.tag).copied().unwrap_or(a.default);
                if !value.is_finite() || value < a.min || value > a.max {
                    return Err(Error::InvalidGeometry);
                }
                let fixed = (f64::from(value) * 65536.).round();
                if fixed < f64::from(i32::MIN) || fixed > f64::from(i32::MAX) {
                    return Err(Error::InvalidGeometry);
                }
                Ok(fixed as ffi::FT_Fixed)
            })
            .collect::<Result<Vec<_>>>()?;
        // SAFETY: The face and fixed-coordinate array live through this synchronous call.
        let status = unsafe {
            ffi::FT_Set_Var_Design_Coordinates(
                self.face.raw_mut(),
                coords.len() as u32,
                coords.as_ptr(),
            )
        };
        if status != 0 {
            return Err(Error::InvalidGeometry);
        }
        self.sample(glyph)
    }
    pub fn sample_normalized(&mut self, glyph: GlyphId, location: &Location) -> Result<Sample> {
        if location.len() != self.axes.len()
            || location
                .keys()
                .any(|tag| !self.axes.iter().any(|a| &a.tag == tag))
        {
            return Err(Error::InvalidGeometry);
        }
        let coords = self
            .axes
            .iter()
            .map(|a| {
                let &value = location.get(&a.tag).ok_or(Error::InvalidGeometry)?;
                if !value.is_finite() || !(-1. ..=1.).contains(&value) {
                    return Err(Error::InvalidGeometry);
                }
                Ok((f64::from(value) * 65536.).round() as ffi::FT_Fixed)
            })
            .collect::<Result<Vec<_>>>()?;
        // SAFETY: This API consumes normalized coordinates, so avar is not applied a second time.
        let status = unsafe {
            ffi::FT_Set_Var_Blend_Coordinates(
                self.face.raw_mut(),
                coords.len() as u32,
                coords.as_ptr(),
            )
        };
        if status != 0 {
            return Err(Error::InvalidGeometry);
        }
        self.sample(glyph)
    }
    pub fn ranges(&self) -> std::collections::BTreeMap<String, Range> {
        self.axes
            .iter()
            .map(|a| {
                (
                    a.tag.clone(),
                    Range {
                        min: a.min,
                        default: a.default,
                        max: a.max,
                    },
                )
            })
            .collect()
    }
    fn sample(&mut self, glyph: GlyphId) -> Result<Sample> {
        if glyph >= u32::try_from(self.face.num_glyphs()).unwrap_or(0) {
            return Err(Error::MissingGlyph);
        }
        let flags = LoadFlag::NO_HINTING | LoadFlag::NO_AUTOHINT | LoadFlag::NO_BITMAP;
        self.face
            .load_glyph(glyph, flags | LoadFlag::NO_SCALE)
            .map_err(|_| Error::InvalidGeometry)?;
        let point_count = self.face.glyph().raw().outline.n_points;
        if point_count < 0 {
            return Err(Error::InvalidGeometry);
        }
        let max = self
            .face
            .glyph()
            .outline()
            // Empty outlines can have a null points buffer; freetype-rs points() requires non-null.
            .filter(|_| point_count > 0)
            .map(|o| {
                o.points()
                    .iter()
                    .map(|p| (p.x as f64).abs().max((p.y as f64).abs()))
                    .fold(0., f64::max)
            })
            .unwrap_or(0.);
        if max > 1_000_000. {
            return Err(Error::TooLarge);
        }
        let upm = u32::try_from(self.face.em_size()).map_err(|_| Error::InvalidDocument)?;
        if !(16..=16384).contains(&upm) {
            return Err(Error::InvalidDocument);
        }
        // Stay below FreeType's 16-bit ppem limit and signed 32-bit outline coordinates.
        let factor = (65535 / upm)
            .min(64)
            .min((f64::from(i32::MAX) / 128. / (max + 1.)).floor() as u32)
            .max(1);
        self.face
            .set_char_size((upm * factor * 64) as isize, 0, 72, 72)
            .map_err(|_| Error::InvalidGeometry)?;
        self.face
            .load_glyph(glyph, flags)
            .map_err(|_| Error::InvalidGeometry)?;
        let scale = f64::from(factor) * 64.;
        let mut builder = Builder {
            scale,
            contours: vec![],
            segments: 0,
        };
        let source = &self.face.glyph().raw().outline;
        let mut outline = ffi::FT_Outline {
            n_contours: source.n_contours,
            n_points: source.n_points,
            points: source.points,
            tags: source.tags,
            contours: source.contours,
            flags: source.flags,
        };
        if outline.n_points < 0 || outline.n_contours < 0 {
            return Err(Error::InvalidGeometry);
        }
        if outline.n_points > 0 {
            // SAFETY: This local header retains the live face-owned outline buffers.
            if unsafe { ffi::FT_Outline_Check(&mut outline) } != 0 {
                return Err(Error::InvalidGeometry);
            }
            let callbacks = ffi::FT_Outline_Funcs {
                move_to,
                line_to,
                conic_to,
                cubic_to,
                shift: 0,
                delta: 0,
            };
            // SAFETY: FreeType owns every outline pointer, and the callbacks borrow this live builder.
            let status = unsafe {
                ffi::FT_Outline_Decompose(
                    &mut outline,
                    &callbacks,
                    (&mut builder as *mut Builder).cast(),
                )
            };
            if status != 0 {
                return Err(Error::InvalidGeometry);
            }
        }
        let shape = Shape {
            advance: self.face.glyph().advance().x as f64 / scale,
            contours: builder.contours,
        };
        shape.validate()?;
        let mut design = vec![0 as ffi::FT_Fixed; self.axes.len()];
        let mut normalized = design.clone();
        // SAFETY: Both output arrays have exactly the face's validated axis count.
        let status = unsafe {
            ffi::FT_Get_Var_Design_Coordinates(
                self.face.raw_mut(),
                design.len() as u32,
                design.as_mut_ptr(),
            )
        };
        let blend = unsafe {
            ffi::FT_Get_Var_Blend_Coordinates(
                self.face.raw_mut(),
                normalized.len() as u32,
                normalized.as_mut_ptr(),
            )
        };
        if status != 0 || blend != 0 {
            return Err(Error::InvalidGeometry);
        }
        Ok(Sample {
            shape,
            design_location: self
                .axes
                .iter()
                .zip(design)
                .map(|(a, v)| (a.tag.clone(), (v as f64 / 65536.) as f32))
                .collect(),
            location: self
                .axes
                .iter()
                .zip(normalized)
                .map(|(a, v)| (a.tag.clone(), (v as f64 / 65536.) as f32))
                .collect(),
        })
    }
}
struct Builder {
    scale: f64,
    contours: Vec<Contour>,
    segments: usize,
}
impl Builder {
    fn point(&self, p: &ffi::FT_Vector) -> Option<Point> {
        let p = [p.x as f64 / self.scale, p.y as f64 / self.scale];
        p.iter()
            .all(|v| v.is_finite() && v.abs() <= 1_000_000.)
            .then_some(p)
    }
    fn segment(&mut self, s: Segment) -> c_int {
        if self.segments >= 65536 {
            return 1;
        }
        let Some(c) = self.contours.last_mut() else {
            return 1;
        };
        c.segments.push(s);
        self.segments += 1;
        0
    }
}
extern "C" fn move_to(to: *const ffi::FT_Vector, user: *mut c_void) -> c_int {
    if to.is_null() || user.is_null() {
        return 1;
    }
    // SAFETY: FT_Outline_Decompose supplies live vector and builder pointers.
    let (b, p) = unsafe { (&mut *user.cast::<Builder>(), &*to) };
    let Some(start) = b.point(p) else { return 1 };
    if b.contours.len() >= 4096 {
        return 1;
    }
    b.contours.push(Contour {
        identity: Default::default(),
        smooth: Default::default(),
        start,
        segments: vec![],
        closed: true,
    });
    0
}
extern "C" fn line_to(to: *const ffi::FT_Vector, user: *mut c_void) -> c_int {
    if to.is_null() || user.is_null() {
        return 1;
    }
    // SAFETY: FT_Outline_Decompose supplies live vector and builder pointers.
    let (b, p) = unsafe { (&mut *user.cast::<Builder>(), &*to) };
    let Some(end) = b.point(p) else { return 1 };
    b.segment(Segment::Line { end })
}
extern "C" fn conic_to(
    control: *const ffi::FT_Vector,
    to: *const ffi::FT_Vector,
    user: *mut c_void,
) -> c_int {
    if control.is_null() || to.is_null() || user.is_null() {
        return 1;
    }
    // SAFETY: FT_Outline_Decompose supplies live vectors and the builder pointer.
    let (b, c, p) = unsafe { (&mut *user.cast::<Builder>(), &*control, &*to) };
    let (Some(control), Some(end)) = (b.point(c), b.point(p)) else {
        return 1;
    };
    b.segment(Segment::Quadratic { control, end })
}
extern "C" fn cubic_to(
    control1: *const ffi::FT_Vector,
    control2: *const ffi::FT_Vector,
    to: *const ffi::FT_Vector,
    user: *mut c_void,
) -> c_int {
    if control1.is_null() || control2.is_null() || to.is_null() || user.is_null() {
        return 1;
    }
    // SAFETY: FT_Outline_Decompose supplies live vectors and the builder pointer.
    let (b, a, c, p) = unsafe { (&mut *user.cast::<Builder>(), &*control1, &*control2, &*to) };
    let (Some(control1), Some(control2), Some(end)) = (b.point(a), b.point(c), b.point(p)) else {
        return 1;
    };
    b.segment(Segment::Cubic {
        control1,
        control2,
        end,
    })
}
