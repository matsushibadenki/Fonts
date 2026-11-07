//! Selected simple TrueType glyphs with stable physical-point topology.
mod encode;
use super::{
    master_gvar::{self, Tuple},
    master_points::Topology,
    masters::Location,
};
use crate::{Contour, Error, Point, Result};
pub use encode::Encoded;

#[derive(Clone, Debug)]
pub struct Glyph {
    pub topology: Topology,
    pub points: Vec<Point>,
    ends: Vec<usize>,
    pub(crate) overlap: bool,
}
impl Glyph {
    pub(crate) fn verify_encoded(encoded: &Encoded) -> Result<Encoded> {
        let mut glyph =
            Self::from_points(encoded.topology.rings().to_vec(), encoded.points.clone())?;
        if encoded.topology.is_cubic() {
            return Err(Error::InvalidGeometry);
        }
        glyph.overlap = encoded.overlap;
        glyph.encode_base(&encoded.points)
    }
    pub(crate) fn from_points(rings: Vec<Vec<bool>>, points: Vec<Point>) -> Result<Self> {
        let topology = Topology::new(rings.clone())?;
        topology.contours(&points)?;
        let mut at = 0;
        let mut ends = vec![];
        for ring in rings {
            at += ring.len();
            ends.push(at - 1);
        }
        Ok(Self {
            topology,
            points,
            ends,
            overlap: false,
        })
    }

    /// Physical design coordinates. Phantom coordinates are not used for outline IUP.
    pub fn sample_points(&self, tuples: &[Tuple], location: &Location) -> Result<Vec<Point>> {
        let count = self.topology.point_count()?;
        if self.points.len() != count {
            return Err(Error::InvalidGeometry);
        }
        let mut points = self.points.clone();
        points.extend([[0.; 2]; 4]);
        let mut result = master_gvar::evaluate(tuples, &points, &self.ends, location)?;
        result.truncate(count);
        Ok(result)
    }
    pub fn contours(&self, tuples: &[Tuple], location: &Location) -> Result<Vec<Contour>> {
        self.topology
            .contours(&self.sample_points(tuples, location)?)
    }
    /// Unrounded physical point residuals, suitable for the master interpolation solver.
    pub fn outline_delta(
        &self,
        tuples: &[Tuple],
        location: &Location,
        edited: &[Contour],
    ) -> Result<Vec<Point>> {
        let target = self.topology.physical_points(edited)?;
        let source = self.sample_points(tuples, location)?;
        Ok(target
            .into_iter()
            .zip(source)
            .map(|(target, source)| [target[0] - source[0], target[1] - source[1]])
            .collect())
    }
}
fn word(bytes: &[u8], at: usize) -> Result<u16> {
    Ok(u16::from_be_bytes(
        bytes
            .get(at..at + 2)
            .ok_or(Error::InvalidFile)?
            .try_into()
            .map_err(|_| Error::InvalidFile)?,
    ))
}
/// Composite glyphs return None: their gvar indices address components, not these physical points.
pub fn read(bytes: &[u8], face_index: u32, glyph: u16) -> Result<Option<Glyph>> {
    if bytes.len() > 256 * 1024 * 1024 {
        return Err(Error::TooLarge);
    }
    let raw = ttf_parser::RawFace::parse(bytes, face_index).map_err(|_| Error::InvalidFile)?;
    let table = |tag| {
        raw.table(ttf_parser::Tag::from_bytes(tag))
            .ok_or(Error::InvalidFile)
    };
    let maxp = table(b"maxp")?;
    let count = word(maxp, 4)?;
    if glyph >= count {
        return Err(Error::MissingGlyph);
    }
    let head = table(b"head")?;
    let long = match word(head, 50)? {
        0 => false,
        1 => true,
        _ => return Err(Error::InvalidFile),
    };
    let loca = table(b"loca")?;
    let glyf = table(b"glyf")?;
    let stride = if long { 4 } else { 2 };
    if loca.len() < (usize::from(count) + 1) * stride {
        return Err(Error::InvalidFile);
    }
    let offset = |index: usize| -> Result<usize> {
        let at = index * stride;
        if long {
            Ok(u32::from_be_bytes(
                loca.get(at..at + 4)
                    .ok_or(Error::InvalidFile)?
                    .try_into()
                    .map_err(|_| Error::InvalidFile)?,
            ) as usize)
        } else {
            Ok(usize::from(word(loca, at)?) * 2)
        }
    };
    let (start, end) = (offset(usize::from(glyph))?, offset(usize::from(glyph) + 1)?);
    let data = glyf.get(start..end).ok_or(Error::InvalidFile)?;
    let Some(shape) =
        crate::truetype_components::decode_simple(data).map_err(|_| Error::InvalidFile)?
    else {
        return Ok(None);
    };
    let topology = Topology::new(
        shape
            .rings
            .iter()
            .map(|r| r.iter().map(|p| p.1).collect())
            .collect(),
    )?;
    let mut ends = vec![];
    let mut points = vec![];
    for ring in shape.rings {
        points.extend(ring.into_iter().map(|p| p.0));
        ends.push(points.len() - 1);
    }
    topology.contours(&points)?;
    let overlap = if points.is_empty() {
        false
    } else {
        let instructions_at = 10 + ends.len() * 2;
        let first_flag = instructions_at + 2 + usize::from(word(data, instructions_at)?);
        data.get(first_flag).is_some_and(|flag| flag & 0x40 != 0)
    };
    Ok(Some(Glyph {
        topology,
        points,
        ends,
        overlap,
    }))
}
