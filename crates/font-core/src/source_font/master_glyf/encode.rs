use super::*;
/// A simple glyf record with unchanged point topology and no stale glyph instructions.
pub struct Encoded {
    pub(crate) remap: Option<Vec<Option<usize>>>,
    pub(crate) topology: Topology,
    pub(crate) overlap: bool,
    pub bytes: Vec<u8>,
    pub points: Vec<Point>,
    pub bounds: [i16; 4],
    pub max_error: f64,
}
impl Glyph {
    pub fn encode_base(&self, points: &[Point]) -> Result<Encoded> {
        self.topology.contours(points)?;
        let mut max_error = 0f64;
        let mut rounded = Vec::with_capacity(points.len());
        for point in points {
            let mut out = [0.; 2];
            for axis in 0..2 {
                out[axis] = point[axis].round();
                max_error = max_error.max((out[axis] - point[axis]).abs());
                if !(f64::from(i16::MIN)..=f64::from(i16::MAX)).contains(&out[axis]) {
                    return Err(Error::InvalidGeometry);
                }
            }
            rounded.push(out);
        }
        let mut bounds = [0i16; 4];
        if let Some(first) = rounded.first() {
            bounds = [
                first[0] as i16,
                first[1] as i16,
                first[0] as i16,
                first[1] as i16,
            ];
            for p in &rounded {
                for axis in 0..2 {
                    bounds[axis] = bounds[axis].min(p[axis] as i16);
                    bounds[axis + 2] = bounds[axis + 2].max(p[axis] as i16);
                }
            }
        }
        let mut bytes = i16::try_from(self.ends.len())
            .map_err(|_| Error::TooLarge)?
            .to_be_bytes()
            .to_vec();
        for value in bounds {
            bytes.extend(value.to_be_bytes());
        }
        for &end in &self.ends {
            bytes.extend(
                u16::try_from(end)
                    .map_err(|_| Error::TooLarge)?
                    .to_be_bytes(),
            );
        }
        bytes.extend(0u16.to_be_bytes());
        let mut flags: Vec<u8> = self
            .topology
            .rings()
            .iter()
            .flatten()
            .map(|&on| u8::from(on))
            .collect();
        let mut coordinates = [Vec::new(), Vec::new()];
        if self.overlap {
            if let Some(first) = flags.first_mut() {
                *first |= 0x40;
            }
        }
        for axis in 0..2 {
            let mut previous = 0i32;
            for (index, point) in rounded.iter().enumerate() {
                let value = point[axis] as i32;
                let delta = value - previous;
                previous = value;
                if delta == 0 {
                    flags[index] |= if axis == 0 { 0x10 } else { 0x20 };
                } else {
                    coordinates[axis].extend(
                        i16::try_from(delta)
                            .map_err(|_| Error::InvalidGeometry)?
                            .to_be_bytes(),
                    );
                }
            }
        }
        bytes.extend(flags);
        for axis in coordinates {
            bytes.extend(axis);
        }
        if !bytes.len().is_multiple_of(2) {
            bytes.push(0);
        }
        Ok(Encoded {
            remap: None,
            topology: self.topology.clone(),
            overlap: self.overlap,
            bytes,
            points: rounded,
            bounds,
            max_error,
        })
    }
    pub(crate) fn with_phantoms(&self) -> Vec<Point> {
        let mut points = self.points.clone();
        points.extend([[0.; 2]; 4]);
        points
    }
    pub(crate) fn expanded(&self, tuple: &Tuple) -> Result<Vec<Point>> {
        tuple.expand(&self.with_phantoms(), &self.ends)
    }
    pub(crate) fn relocated(&self, points: Vec<Point>) -> Self {
        Self {
            topology: self.topology.clone(),
            points,
            ends: self.ends.clone(),
            overlap: self.overlap,
        }
    }
}
