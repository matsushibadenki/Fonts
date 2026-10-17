//! Shared Y-up image manipulation geometry; handle sizes are supplied in document units.
use crate::{reference_images::ReferenceImage, Point, Result};
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Handle {
    Move,
    Resize(i8, i8),
    Rotate,
}
pub const RESIZE_HANDLES: [(i8, i8); 8] = [
    (-1, -1),
    (0, -1),
    (1, -1),
    (1, 0),
    (1, 1),
    (0, 1),
    (-1, 1),
    (-1, 0),
];
pub fn handles(image: &ReferenceImage, scale: f64) -> Vec<(Handle, Point)> {
    let mut result: Vec<_> = RESIZE_HANDLES
        .iter()
        .map(|&(x, y)| {
            (
                Handle::Resize(x, y),
                image.to_document([f64::from(x) / 2., f64::from(y) / 2.]),
            )
        })
        .collect();
    result.push((
        Handle::Rotate,
        image.to_document([0., 0.5 + 26. / scale / image.height]),
    ));
    result
}
pub fn transform(
    original: &ReferenceImage,
    handle: Handle,
    start: Point,
    current: Point,
    shift: bool,
) -> Result<ReferenceImage> {
    let mut image = original.clone();
    match handle {
        Handle::Move => {
            image.center = [
                original.center[0] + current[0] - start[0],
                original.center[1] + current[1] - start[1],
            ];
        }
        Handle::Rotate => {
            let angle = |p: Point| (p[1] - original.center[1]).atan2(p[0] - original.center[0]);
            image.angle = (original.angle + (angle(current) - angle(start)).to_degrees() + 180.)
                .rem_euclid(360.)
                - 180.;
            if shift {
                image.angle = (image.angle / 15.).round() * 15.;
            }
        }
        Handle::Resize(x, y) => {
            let a = original.to_local(start);
            let b = original.to_local(current);
            let w = original.width * (1. + (b[0] - a[0]) * f64::from(x));
            let h = original.height * (1. + (b[1] - a[1]) * f64::from(y));
            if x != 0 && y != 0 && !shift {
                let factor = ((w * original.width + h * original.height)
                    / (original.width.powi(2) + original.height.powi(2)))
                .max(0.001 / original.width.min(original.height));
                image.width = original.width * factor;
                image.height = original.height * factor;
            } else {
                image.width = w.max(0.001);
                image.height = h.max(0.001);
            }
            let opposite = [-f64::from(x) / 2., -f64::from(y) / 2.];
            let fixed = original.to_document(opposite);
            let moved = image.to_document(opposite);
            image.center = [
                original.center[0] + fixed[0] - moved[0],
                original.center[1] + fixed[1] - moved[1],
            ];
        }
    }
    image.validate()?;
    Ok(image)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn image() -> ReferenceImage {
        ReferenceImage {
            id: 1,
            asset_id: "test".into(),
            center: [20., 50.],
            width: 400.,
            height: 200.,
            angle: 37.,
            opacity: 0.5,
            visible: true,
            locked: false,
        }
    }
    #[test]
    fn all_resizes_keep_opposite_fixed_and_corners_keep_ratio() {
        let image = image();
        for (x, y) in RESIZE_HANDLES {
            let start = image.to_document([f64::from(x) / 2., f64::from(y) / 2.]);
            let end = image.to_document([f64::from(x) * 0.8, f64::from(y) * 0.9]);
            for shift in [false, true] {
                let resized = transform(&image, Handle::Resize(x, y), start, end, shift).unwrap();
                let opposite = [-f64::from(x) / 2., -f64::from(y) / 2.];
                let a = image.to_document(opposite);
                let b = resized.to_document(opposite);
                assert!((a[0] - b[0]).hypot(a[1] - b[1]) < 1e-8);
                if x != 0 && y != 0 && !shift {
                    assert!((resized.width / resized.height - 2.).abs() < 1e-8);
                }
                if x == 0 {
                    assert_eq!(resized.width, image.width);
                }
                if y == 0 {
                    assert_eq!(resized.height, image.height);
                }
            }
        }
    }
    #[test]
    fn rotation_snaps_and_resize_cannot_flip_or_accept_nonfinite() {
        let image = image();
        let rotated = transform(&image, Handle::Rotate, [120., 50.], [20., 150.], true).unwrap();
        assert_eq!(rotated.angle, 120.);
        assert_eq!(rotated.center, image.center);
        let shrunk = transform(
            &image,
            Handle::Resize(1, 1),
            image.to_document([0.5, 0.5]),
            image.to_document([-2., -2.]),
            false,
        )
        .unwrap();
        assert!(shrunk.width > 0. && shrunk.height > 0.);
        assert!(transform(&image, Handle::Move, [0., 0.], [f64::NAN, 0.], false).is_err());
    }
}
