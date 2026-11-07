//! Bounded monochrome tracing. Pixels remain on the Rust side; contours use font units.
use crate::{
    reference_images::{ImageAsset, ReferenceImage},
    Contour, Error, Point, Result, Segment,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    pub threshold: u8,
    pub invert: bool,
    /// Area in source pixels. Small foreground components and holes are omitted.
    pub min_area: f64,
    /// Maximum polygon simplification distance in source pixels.
    pub tolerance: f64,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            threshold: 128,
            invert: false,
            min_area: 4.,
            tolerance: 0.5,
        }
    }
}
const MAX_EDGES: usize = 500_000;
const MAX_POINTS: usize = 65_536;
const MAX_PIXELS: usize = 4_000_000;
fn area(points: &[Point]) -> f64 {
    points
        .iter()
        .zip(points.iter().cycle().skip(1))
        .map(|(a, b)| a[0] * b[1] - b[0] * a[1])
        .sum::<f64>()
        * 0.5
}
fn distance(p: Point, a: Point, b: Point) -> f64 {
    let v = [b[0] - a[0], b[1] - a[1]];
    let n = v[0] * v[0] + v[1] * v[1];
    let t = if n == 0. {
        0.
    } else {
        ((p[0] - a[0]) * v[0] + (p[1] - a[1]) * v[1]) / n
    }
    .clamp(0., 1.);
    (p[0] - a[0] - t * v[0]).hypot(p[1] - a[1] - t * v[1])
}
fn simplify(points: &[Point], tolerance: f64, budget: &mut usize) -> Vec<Point> {
    // Split a closed ring at a distant point, then use iterative RDP on both arcs.
    let split = (1..points.len())
        .max_by(|&a, &b| {
            distance(points[a], points[0], points[0])
                .total_cmp(&distance(points[b], points[0], points[0]))
        })
        .unwrap_or(1);
    let mut p = points.to_vec();
    p.push(points[0]);
    let mut keep = vec![false; p.len()];
    keep[0] = true;
    keep[split] = true;
    keep[points.len()] = true;
    let mut stack = vec![(0, split), (split, points.len())];
    while let Some((a, b)) = stack.pop() {
        let mut best = (0., a);
        for i in a + 1..b {
            *budget += 1;
            // Bound worst-case quadratic work on adversarial staircases.
            if *budget > 8_000_000 {
                return points.to_vec();
            }
            let d = distance(p[i], p[a], p[b]);
            if d > best.0 {
                best = (d, i);
            }
        }
        if best.0 > tolerance {
            keep[best.1] = true;
            stack.push((a, best.1));
            stack.push((best.1, b));
        }
    }
    let out: Vec<_> = p[..points.len()]
        .iter()
        .enumerate()
        .filter_map(|(i, p)| keep[i].then_some(*p))
        .collect();
    if out.len() < 3 || area(&out) * area(points) <= 0. {
        points.to_vec()
    } else {
        out
    }
}
pub fn trace(
    asset: &ImageAsset,
    placement: &ReferenceImage,
    settings: Settings,
) -> Result<Vec<Contour>> {
    placement.validate()?;
    if !settings.min_area.is_finite()
        || !(0. ..=1_000_000.).contains(&settings.min_area)
        || !settings.tolerance.is_finite()
        || !(0. ..=4.).contains(&settings.tolerance)
    {
        return Err(Error::InvalidGeometry);
    }
    let (w, h) = (asset.width() as usize, asset.height() as usize);
    if w * h > MAX_PIXELS {
        return Err(Error::TooLarge);
    }
    let mask: Vec<_> = asset
        .rgba()
        .as_chunks::<4>()
        .0
        .iter()
        .map(|p| {
            let l =
                (54 * u32::from(p[0]) + 183 * u32::from(p[1]) + 19 * u32::from(p[2]) + 128) / 256;
            // Composite onto white before thresholding; transparent pixels never become ink.
            let value = (l * u32::from(p[3]) + 255 * (255 - u32::from(p[3])) + 127) / 255;
            p[3] > 0
                && if settings.invert {
                    value > u32::from(settings.threshold)
                } else {
                    value < u32::from(settings.threshold)
                }
        })
        .collect();
    let stride = w + 1;
    let mut graph = vec![0u8; stride * (h + 1)];
    let mut edges = 0;
    for y in 0..h {
        for x in 0..w {
            if mask[y * w + x] {
                for (ex, ey, d, boundary) in [
                    (x, y, 0, y == 0 || !mask[(y.saturating_sub(1)) * w + x]),
                    (x + 1, y, 1, x + 1 == w || !mask[y * w + (x + 1).min(w - 1)]),
                    (
                        x + 1,
                        y + 1,
                        2,
                        y + 1 == h || !mask[(y + 1).min(h - 1) * w + x],
                    ),
                    (x, y + 1, 3, x == 0 || !mask[y * w + x.saturating_sub(1)]),
                ] {
                    if boundary {
                        graph[ey * stride + ex] |= 1 << d;
                        edges += 1;
                        if edges > MAX_EDGES {
                            return Err(Error::TooLarge);
                        }
                    }
                }
            }
        }
    }
    let mut contours = vec![];
    let mut total = 0;
    let mut budget = 0;
    for start in 0..graph.len() {
        while graph[start] != 0 {
            let mut index = start;
            let mut direction = graph[start].trailing_zeros() as usize;
            let mut ring = vec![];
            loop {
                ring.push([(index % stride) as f64, (index / stride) as f64]);
                graph[index] &= !(1 << direction);
                index = match direction {
                    0 => index + 1,
                    1 => index + stride,
                    2 => index - 1,
                    _ => index - stride,
                };
                if index == start {
                    break;
                }
                direction = [
                    (direction + 1) % 4,
                    direction,
                    (direction + 3) % 4,
                    (direction + 2) % 4,
                ]
                .into_iter()
                .find(|d| graph[index] & (1 << d) != 0)
                .ok_or(Error::InvalidGeometry)?;
                if ring.len() > MAX_EDGES {
                    return Err(Error::TooLarge);
                }
            }
            if area(&ring).abs() < settings.min_area {
                continue;
            }
            // Remove collinear grid points before distance simplification.
            let clean: Vec<_> = (0..ring.len())
                .filter_map(|i| {
                    let a = ring[(i + ring.len() - 1) % ring.len()];
                    let b = ring[i];
                    let c = ring[(i + 1) % ring.len()];
                    ((b[0] - a[0]) * (c[1] - b[1]) != (b[1] - a[1]) * (c[0] - b[0])).then_some(b)
                })
                .collect();
            let points = simplify(&clean, settings.tolerance, &mut budget);
            total += points.len();
            if contours.len() >= 4096 || total > MAX_POINTS {
                return Err(Error::TooLarge);
            }
            let points: Vec<_> = points
                .into_iter()
                .map(|p| placement.to_document([p[0] / w as f64 - 0.5, 0.5 - p[1] / h as f64]))
                .collect();
            let mut c = Contour {
                identity: Default::default(),
                smooth: Default::default(),
                start: points[0],
                segments: points[1..]
                    .iter()
                    .map(|&end| Segment::Line { end })
                    .collect(),
                closed: true,
            };
            crate::identity::ensure_contour(&mut c)?;
            contours.push(c);
        }
    }
    Ok(contours)
}
