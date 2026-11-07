//! Coordinate-independent binding from TrueType physical points to editor nodes.
use crate::{
    geometry::{self, EditPoint},
    identity::{self, Identities},
    Contour, Error, Point, Result, Segment,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Binding {
    Physical(usize),
    Midpoint(usize, usize),
}
impl Binding {
    fn position(self, points: &[Point]) -> Result<Point> {
        let at = |index| points.get(index).copied().ok_or(Error::InvalidGeometry);
        match self {
            Self::Physical(index) => at(index),
            Self::Midpoint(a, b) => {
                let (a, b) = (at(a)?, at(b)?);
                Ok([(a[0] + b[0]) / 2., (a[1] + b[1]) / 2.])
            }
        }
    }
    fn midpoint(a: usize, b: usize) -> Self {
        if a == b {
            Self::Physical(a)
        } else {
            Self::Midpoint(a, b)
        }
    }
}
type Bindings = BTreeMap<EditPoint, Binding>;
/// The topology and identities are shared by every master, including coincident points.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Topology {
    rings: Vec<Vec<bool>>,
    identities: Vec<Identities>,
}
impl Topology {
    pub(crate) fn rings(&self) -> &[Vec<bool>] {
        &self.rings
    }
    pub fn new(rings: Vec<Vec<bool>>) -> Result<Self> {
        let count = checked_count(&rings)?;
        let (mut contours, _) = layout(&rings, &vec![[0.; 2]; count])?;
        for contour in &mut contours {
            identity::ensure_contour(contour)?;
        }
        Ok(Self {
            rings,
            identities: contours.into_iter().map(|c| c.identity).collect(),
        })
    }
    pub fn point_count(&self) -> Result<usize> {
        checked_count(&self.rings)
    }
    pub fn contours(&self, points: &[Point]) -> Result<Vec<Contour>> {
        let (mut contours, _) = self.bound(points)?;
        for (c, ids) in contours.iter_mut().zip(&self.identities) {
            c.identity = ids.clone();
            geometry::validate(c)?;
        }
        Ok(contours)
    }
    /// Implied anchors are derived from adjacent off-curve points, never silently discarded.
    pub fn binding(&self, contour: usize, point: EditPoint) -> Result<Binding> {
        let (_, bindings) = self.bound(&vec![[0.; 2]; self.point_count()?])?;
        bindings
            .get(contour)
            .and_then(|b| b.get(&point))
            .copied()
            .ok_or(Error::InvalidGeometry)
    }
    /// Convert compatible edited contours back to physical point coordinates.
    /// Editing an implied anchor independently requires an explicit topology change.
    pub fn physical_points(&self, contours: &[Contour]) -> Result<Vec<Point>> {
        let count = self.point_count()?;
        let (expected, bindings) = self.bound(&vec![[0.; 2]; count])?;
        if contours.len() != expected.len() {
            return Err(Error::InvalidGeometry);
        }
        let mut out = vec![None; count];
        let mut implied = vec![];
        for (index, ((c, model), map)) in contours.iter().zip(expected).zip(bindings).enumerate() {
            geometry::validate(c)?;
            if !c.closed
                || c.identity != self.identities[index]
                || c.segments.len() != model.segments.len()
                || c.segments
                    .iter()
                    .zip(&model.segments)
                    .any(|(a, b)| std::mem::discriminant(a) != std::mem::discriminant(b))
            {
                return Err(Error::InvalidGeometry);
            }
            for (address, binding) in map {
                let position =
                    geometry::point_position(c, address).ok_or(Error::InvalidGeometry)?;
                if let Binding::Physical(index) = binding {
                    if out[index].is_some_and(|old| old != position) {
                        return Err(Error::InvalidGeometry);
                    }
                    out[index] = Some(position);
                } else {
                    implied.push((binding, position));
                }
            }
        }
        let out = out
            .into_iter()
            .collect::<Option<Vec<_>>>()
            .ok_or(Error::InvalidGeometry)?;
        for (binding, position) in implied {
            if binding.position(&out)? != position {
                return Err(Error::InvalidGeometry);
            }
        }
        Ok(out)
    }
    /// Move a physical node, or translate both adjacent controls for an implied anchor.
    /// Rebuilding from physical points keeps every other implied anchor consistent.
    pub fn move_node(
        &self,
        contours: &[Contour],
        contour: usize,
        node: EditPoint,
        position: Point,
    ) -> Result<Vec<Contour>> {
        if position
            .iter()
            .any(|v| !v.is_finite() || v.abs() > 1_000_000.)
        {
            return Err(Error::InvalidGeometry);
        }
        let mut points = self.physical_points(contours)?;
        match self.binding(contour, node)? {
            Binding::Physical(index) => points[index] = position,
            Binding::Midpoint(a, b) => {
                let old = Binding::Midpoint(a, b).position(&points)?;
                for index in [a, b] {
                    for axis in 0..2 {
                        points[index][axis] += position[axis] - old[axis];
                    }
                }
            }
        }
        let mut out = self.contours(&points)?;
        for (next, old) in out.iter_mut().zip(contours) {
            next.smooth = old.smooth.clone();
            geometry::validate(next)?;
        }
        Ok(out)
    }
    fn bound(&self, points: &[Point]) -> Result<(Vec<Contour>, Vec<Bindings>)> {
        if self.identities.len() != self.rings.len() {
            return Err(Error::InvalidGeometry);
        }
        let result = layout(&self.rings, points)?;
        let mut seen = HashSet::new();
        for (c, ids) in result.0.iter().zip(&self.identities) {
            let mut c = c.clone();
            c.identity = ids.clone();
            if ids.is_empty() {
                return Err(Error::InvalidGeometry);
            }
            identity::validate_contour(&c)?;
            for id in ids
                .contours
                .iter()
                .chain(ids.anchors.iter().flatten())
                .chain(ids.controls.iter().flatten().flatten())
            {
                if id.is_nil() || !seen.insert(*id) {
                    return Err(Error::InvalidGeometry);
                }
            }
        }
        Ok(result)
    }
}
fn checked_count(rings: &[Vec<bool>]) -> Result<usize> {
    if rings.len() > 4096 || rings.iter().any(Vec::is_empty) {
        return Err(Error::InvalidGeometry);
    }
    let count = rings
        .iter()
        .try_fold(0usize, |n, r| n.checked_add(r.len()).ok_or(Error::TooLarge))?;
    if count > 65536 {
        return Err(Error::TooLarge);
    }
    Ok(count)
}
fn layout(rings: &[Vec<bool>], points: &[Point]) -> Result<(Vec<Contour>, Vec<Bindings>)> {
    if checked_count(rings)? != points.len()
        || points
            .iter()
            .flatten()
            .any(|v| !v.is_finite() || v.abs() > 1_000_000.)
    {
        return Err(Error::InvalidGeometry);
    }
    let mut contours = vec![];
    let mut maps = vec![];
    let mut offset = 0;
    for ring in rings {
        let n = ring.len();
        let physical = |i| Binding::Physical(offset + i);
        let (start, from, count) = if ring[0] {
            (physical(0), 1, n - 1)
        } else if ring[n - 1] {
            (physical(n - 1), 0, n - 1)
        } else {
            (Binding::midpoint(offset + n - 1, offset), 0, n)
        };
        let mut c = Contour {
            start: start.position(points)?,
            segments: vec![],
            closed: true,
            smooth: Default::default(),
            identity: Default::default(),
        };
        let mut map = Bindings::from([(EditPoint::Anchor(0), start)]);
        let mut last = start;
        let mut i = 0;
        while i < count {
            let at = (from + i) % n;
            let p = physical(at);
            let index = c.segments.len();
            let end = if ring[at] {
                c.segments.push(Segment::Line {
                    end: p.position(points)?,
                });
                i += 1;
                p
            } else {
                let (end, consume) = if i + 1 < count {
                    let next = (from + i + 1) % n;
                    if ring[next] {
                        (physical(next), 2)
                    } else {
                        (Binding::midpoint(offset + at, offset + next), 1)
                    }
                } else {
                    (start, 1)
                };
                c.segments.push(Segment::Quadratic {
                    control: p.position(points)?,
                    end: end.position(points)?,
                });
                map.insert(EditPoint::Control1(index), p);
                i += consume;
                end
            };
            map.insert(EditPoint::Anchor(index + 1), end);
            last = end;
        }
        // Closure depends on source point identity, never on accidental coordinate equality.
        if last != start || c.segments.is_empty() {
            c.segments.push(Segment::Line { end: c.start });
            map.insert(EditPoint::Anchor(c.segments.len()), start);
        }
        contours.push(c);
        maps.push(map);
        offset += n;
    }
    Ok((contours, maps))
}
