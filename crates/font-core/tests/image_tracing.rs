use font_core::{
    image_trace::{trace, Settings},
    reference_images::{ImageAsset, ReferenceImage},
    Contour, Editor, FontDocument, Segment,
};
fn asset(w: u32, h: u32, pixels: Vec<u8>) -> ImageAsset {
    let image = image::RgbaImage::from_raw(w, h, pixels).unwrap();
    let mut bytes = std::io::Cursor::new(vec![]);
    image::DynamicImage::ImageRgba8(image)
        .write_to(&mut bytes, image::ImageFormat::Png)
        .unwrap();
    ImageAsset::from_bytes(bytes.into_inner()).unwrap()
}
fn placement(a: &ImageAsset) -> ReferenceImage {
    ReferenceImage {
        id: 1,
        asset_id: a.id().into(),
        center: [200., 300.],
        width: a.width() as f64 * 100.,
        height: a.height() as f64 * 100.,
        angle: 0.,
        opacity: 0.2,
        visible: true,
        locked: false,
    }
}
fn signed(c: &Contour) -> f64 {
    let p: Vec<_> = std::iter::once(c.start)
        .chain(c.segments.iter().map(|s| match s {
            Segment::Line { end } | Segment::Quadratic { end, .. } | Segment::Cubic { end, .. } => {
                *end
            }
        }))
        .collect();
    p.iter()
        .zip(p.iter().cycle().skip(1))
        .map(|(a, b)| a[0] * b[1] - b[0] * a[1])
        .sum::<f64>()
        / 2.
}
#[test]
fn holes_diagonal_separation_transparency_threshold_and_transforms() {
    let pixels = (0..25)
        .flat_map(|i| {
            if i == 12 {
                [255, 255, 255, 255]
            } else {
                [0, 0, 0, 255]
            }
        })
        .collect();
    let a = asset(5, 5, pixels);
    let mut p = placement(&a);
    let settings = Settings {
        min_area: 0.,
        tolerance: 0.,
        ..Default::default()
    };
    let c = trace(&a, &p, settings).unwrap();
    assert_eq!(c.len(), 2);
    assert!(signed(&c[0]) * signed(&c[1]) < 0.);
    assert_eq!(c.iter().map(signed).sum::<f64>().abs(), 240000.);
    assert_eq!(
        trace(
            &a,
            &p,
            Settings {
                min_area: 2.,
                ..settings
            }
        )
        .unwrap()
        .len(),
        1
    );
    assert_eq!(
        trace(
            &a,
            &p,
            Settings {
                invert: true,
                ..settings
            }
        )
        .unwrap()
        .len(),
        1
    );
    let start = c[0].start;
    p.angle = 90.;
    let rotated = trace(&a, &p, settings).unwrap();
    assert!((rotated[0].start[0] - (p.center[0] - (start[1] - p.center[1]))).abs() < 1e-8);
    let a = asset(
        2,
        2,
        vec![0, 0, 0, 255, 0, 0, 0, 0, 255, 255, 255, 255, 0, 0, 0, 255],
    );
    let c = trace(&a, &placement(&a), settings).unwrap();
    assert_eq!(c.len(), 2);
    assert!(c.iter().all(|c| c.segments.len() == 3));
    let a = asset(1, 1, vec![0, 0, 0, 0]);
    for invert in [false, true] {
        assert!(trace(&a, &placement(&a), Settings { invert, ..settings })
            .unwrap()
            .is_empty());
    }
    let a = asset(1, 1, vec![0, 0, 0, 100]);
    assert!(trace(&a, &placement(&a), settings).unwrap().is_empty());
    assert_eq!(
        trace(
            &a,
            &placement(&a),
            Settings {
                threshold: 200,
                ..settings
            }
        )
        .unwrap()
        .len(),
        1
    );
}
#[test]
fn traced_paths_survive_history_storage_and_font_exports() {
    let a = asset(
        4,
        4,
        (0..16)
            .flat_map(|i| {
                if [5, 6, 9, 10].contains(&i) {
                    [255, 255, 255, 255]
                } else {
                    [0, 0, 0, 255]
                }
            })
            .collect(),
    );
    let mut e = Editor::new(FontDocument::new("Trace", "A").unwrap()).unwrap();
    e.place_reference_image(0, 1, a.clone()).unwrap();
    let before = e.document().clone();
    let p = &before.reference_images.for_glyph(1)[0];
    let mut g = before.glyphs[&1].clone();
    g.contours = trace(
        &a,
        p,
        Settings {
            min_area: 0.,
            ..Default::default()
        },
    )
    .unwrap();
    e.replace_glyph(1, g).unwrap();
    let traced = e.document().clone();
    e.undo(2).unwrap();
    assert_eq!(e.document(), &before);
    e.redo(3).unwrap();
    assert_eq!(e.document(), &traced);
    let dir = std::env::temp_dir().join(format!(
        "fonts-trace-{}.json",
        font_core::identity::Uuid::new_v4()
    ));
    font_core::storage::save(&dir, &traced).unwrap();
    let loaded = font_core::storage::load(&dir).unwrap();
    std::fs::remove_file(dir).unwrap();
    assert_eq!(loaded, traced);
    for format in [
        font_core::font_formats::Format::Ttf,
        font_core::font_formats::Format::Otf,
        font_core::font_formats::Format::Woff2,
    ] {
        let b = font_core::font_formats::export(&loaded, format).unwrap();
        let sfnt = if format == font_core::font_formats::Format::Woff2 {
            font_core::font_formats::woff2::decode(&b).unwrap()
        } else {
            b
        };
        let imported = font_core::font_io::import(&sfnt, 0).unwrap();
        let glyph = imported
            .glyphs
            .values()
            .find(|g| g.codepoints.contains(&'A'))
            .unwrap();
        assert_eq!(glyph.contours.len(), 2);
        assert!(signed(&glyph.contours[0]) * signed(&glyph.contours[1]) < 0.);
    }
}
#[test]
fn limits_reject_noise_and_invalid_settings_without_document_mutations() {
    let a = asset(
        600,
        600,
        (0..360000)
            .flat_map(|i| {
                if (i / 600 + i % 600) % 2 == 0 {
                    [0, 0, 0, 255]
                } else {
                    [255, 255, 255, 255]
                }
            })
            .collect(),
    );
    assert_eq!(
        trace(&a, &placement(&a), Settings::default()),
        Err(font_core::Error::TooLarge)
    );
    assert_eq!(
        trace(
            &a,
            &placement(&a),
            Settings {
                tolerance: f64::NAN,
                ..Default::default()
            }
        ),
        Err(font_core::Error::InvalidGeometry)
    );
}
