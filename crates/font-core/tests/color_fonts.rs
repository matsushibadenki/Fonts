use font_core::{color_fonts as color, font_io};
const COLR: &[u8] = include_bytes!("data/color/colrv0.ttf");
const V1: &[u8] = include_bytes!("data/color/colrv1.ttf");
const SVG: &[u8] = include_bytes!("data/color/svg.ttf");
const SBIX: &[u8] = include_bytes!("data/color/sbix.ttf");
const PAINTS: &[u8] = include_bytes!("data/color/colrv1-paints.ttf");
const SVG_PAINTS: &[u8] = include_bytes!("data/color/svg-paints.ttf");
const SVG_GZIP: &[u8] = include_bytes!("data/color/svg-paints-gzip.ttf");

fn svg_paint(text: &str, palette: u16) -> color::Preview {
    color::preview(SVG_PAINTS, 0, text, 100, palette, [256, 256]).unwrap()
}

#[test]
fn svg_shared_glyphs_colors_gradients_and_group_alpha() {
    let info = color::info(SVG_PAINTS, 0).unwrap();
    assert!(info.preview);
    assert_eq!(info.palettes, 2, "CPAL without COLR is available");
    let a = svg_paint("A", 0);
    near(pixel(&a, 25, 110), [255, 0, 0, 255]);
    near(pixel(&a, 60, 80), [127, 0, 128, 255]);
    assert_eq!(pixel(&a, 100, 80), [0; 4], "other glyphs are not drawn");
    assert_eq!(
        pixel(&a, 25, 50),
        [0; 4],
        "y-down SVG has negative ascenders"
    );
    let linear = svg_paint("B", 0);
    near(pixel(&linear, 23, 90), [228, 0, 27, 255]);
    near(pixel(&linear, 78, 90), [27, 0, 228, 255]);
    let radial = svg_paint("C", 0);
    assert!(pixel(&radial, 51, 89)[0] > 240);
    assert!(pixel(&radial, 83, 89)[2] > 230);
    let group = svg_paint("D", 0);
    near(pixel(&group, 25, 110), [255, 0, 0, 128]);
    near(pixel(&group, 60, 80), [0, 0, 255, 128]);
}

#[test]
fn svg_palette_alpha_clip_transform_style_currentcolor_and_holes() {
    near(pixel(&svg_paint("F", 0), 51, 90), [0, 0, 255, 64]);
    near(pixel(&svg_paint("F", 1), 51, 90), [255, 255, 0, 128]);
    let clip = svg_paint("E", 0);
    near(pixel(&clip, 45, 85), [255, 0, 0, 255]);
    for (x, y) in [(30, 85), (60, 85), (45, 70), (45, 105)] {
        assert_eq!(pixel(&clip, x, y)[3], 0);
    }
    near(pixel(&svg_paint("G", 0), 51, 90), [0, 128, 0, 255]);
    near(pixel(&svg_paint("H", 0), 51, 90), [0, 0, 0, 255]);
    let translated = svg_paint("I", 0);
    near(pixel(&translated, 90, 50), [255, 128, 0, 255]);
    assert_eq!(pixel(&translated, 20, 50)[3], 0);
    let hole = svg_paint("J", 0);
    assert_eq!(pixel(&hole, 51, 90)[3], 0);
    near(pixel(&hole, 25, 90), [0, 255, 255, 255]);
}

#[test]
fn svg_gzip_and_woff2_preserve_pixels_and_palette_variants() {
    for source in [SVG_PAINTS, SVG_GZIP] {
        let output = color::export_woff2(source, 0, 5).unwrap();
        color::verify_woff2(&output, source, 0).unwrap();
        let decoded = font_core::font_formats::woff2::decode(&output).unwrap();
        for glyph in 'A'..='J' {
            for palette in [0, 1] {
                let expected = svg_paint(&glyph.to_string(), palette);
                for bytes in [source, decoded.as_slice()] {
                    let actual =
                        color::preview(bytes, 0, &glyph.to_string(), 100, palette, [256, 256])
                            .unwrap();
                    assert_eq!(expected.image.rgba(), actual.image.rgba());
                }
            }
        }
    }
    assert_eq!(svg_paint("X", 0).missing, 1);
    assert!(svg_paint(" ", 0).image.rgba().iter().all(|b| *b == 0));
    assert!(svg_paint("K", 0)
        .image
        .rgba()
        .chunks(4)
        .any(|p| p == [0, 0, 0, 255]));
}

fn paint(text: &str, palette: u16) -> color::Preview {
    color::preview(PAINTS, 0, text, 100, palette, [256, 256]).unwrap()
}
fn pixel(p: &color::Preview, x: usize, y: usize) -> [u8; 4] {
    let offset = (y * p.image.width() as usize + x) * 4;
    p.image.rgba()[offset..offset + 4].try_into().unwrap()
}
fn near(actual: [u8; 4], expected: [u8; 4]) {
    for (a, b) in actual.into_iter().zip(expected) {
        assert!(a.abs_diff(b) <= 6, "{actual:?} != {expected:?}");
    }
}

#[test]
fn colrv1_gradients_palette_alpha_and_foreground() {
    let linear = paint("A", 0);
    near(pixel(&linear, 23, 90), [228, 0, 27, 255]);
    near(pixel(&linear, 51, 90), [126, 0, 129, 255]);
    near(pixel(&linear, 78, 90), [27, 0, 228, 255]);
    assert_eq!(
        pixel(&linear, 50, 50)[3],
        0,
        "glyph points up from baseline"
    );
    assert_eq!(pixel(&linear, 50, 125)[3], 0);
    let palette = paint("A", 1);
    near(pixel(&palette, 51, 90), [129, 255, 0, 255]);
    let radial = paint("B", 0);
    assert!(pixel(&radial, 51, 89)[0] > 240);
    assert!(pixel(&radial, 83, 89)[2] > 230);
    near(pixel(&radial, 51, 65), pixel(&radial, 74, 89));
    let sweep = paint("C", 0);
    // OpenType sweep: 0 degrees points right; angles increase counterclockwise.
    near(pixel(&sweep, 75, 89), [1, 0, 254, 255]);
    near(pixel(&sweep, 51, 113), [63, 0, 192, 255]);
    near(pixel(&sweep, 27, 89), [127, 0, 128, 255]);
    let alpha = paint("I", 0);
    near(pixel(&alpha, 51, 90), [126, 0, 129, 128]);
    near(pixel(&paint("H", 1), 51, 90), [0, 0, 0, 255]);
}

#[test]
fn colrv1_extend_transform_clip_composite_and_nested_paints() {
    let repeat = paint("D", 0);
    near(pixel(&repeat, 21, 90), pixel(&repeat, 41, 90));
    near(pixel(&repeat, 21, 90), pixel(&repeat, 61, 90));
    let reflect = paint("E", 0);
    near(pixel(&reflect, 21, 90), pixel(&reflect, 50, 90));
    assert_ne!(pixel(&repeat, 41, 90), pixel(&reflect, 41, 90));
    let clipped = paint("F", 0);
    near(pixel(&clipped, 45, 85), [255, 0, 0, 255]);
    for (x, y) in [(30, 85), (60, 85), (45, 70), (45, 105)] {
        assert_eq!(pixel(&clipped, x, y)[3], 0);
    }
    let composite = paint("G", 0);
    near(pixel(&composite, 25, 110), [255, 0, 0, 255]);
    near(pixel(&composite, 60, 80), [127, 0, 128, 255]);
    let nested = paint("J", 0);
    near(pixel(&nested, 25, 110), pixel(&paint("A", 0), 25, 110));
    assert!(pixel(&nested, 60, 80)[2] > pixel(&paint("A", 0), 60, 80)[2]);
}

#[test]
fn colrv1_layout_fallback_limits_and_woff2_round_trip() {
    assert!(paint("K", 0)
        .image
        .rgba()
        .chunks(4)
        .any(|p| p == [0, 0, 0, 255]));
    assert!(paint(" ", 0).image.rgba().iter().all(|p| *p == 0));
    assert_eq!(paint("X", 0).missing, 1);
    let wrapped = color::preview(PAINTS, 0, "AAA", 100, 0, [256, 512]).unwrap();
    near(pixel(&wrapped, 23, 90), pixel(&wrapped, 23, 290));
    assert!(color::preview(PAINTS, 0, "AAA", 100, 0, [256, 256]).is_err());
    assert!(color::preview(PAINTS, 0, "A", 100, 2, [256, 256]).is_err());
    assert!(color::preview(PAINTS, 0, "\t", 100, 0, [256, 256]).is_err());
    let output = color::export_woff2(PAINTS, 0, 5).unwrap();
    color::verify_woff2(&output, PAINTS, 0).unwrap();
    let decoded = font_core::font_formats::woff2::decode(&output).unwrap();
    for glyph in 'A'..='K' {
        let text = glyph.to_string();
        for palette in [0, 1] {
            let before = paint(&text, palette);
            let after = color::preview(&decoded, 0, &text, 100, palette, [256, 256]).unwrap();
            assert_eq!(before.image.rgba(), after.image.rgba());
        }
    }
}
#[test]
fn color_formats_keep_every_table_on_export() {
    for (source, format, preview) in [
        (COLR, "COLRv0", true),
        (V1, "COLRv1", true),
        (SVG, "SVG", true),
        (SBIX, "sbix", true),
    ] {
        let info = color::info(source, 0).unwrap();
        assert_eq!(info.formats, [format]);
        assert_eq!(info.preview, preview);
        assert!(info.sample.contains('A'));
        let bytes = color::export(source, 0).unwrap();
        color::verify(&bytes, source, 0).unwrap();
        assert!(font_io::import(source, 0).is_err());
        let mut bad = bytes.clone();
        let end = bad.len() - 1;
        bad[end] ^= 1;
        assert!(color::verify(&bad, source, 0).is_err());
        assert!(color::verify(&bytes[..bytes.len() / 2], source, 0).is_err());
    }
}
#[test]
fn colrv0_renders_palette_colors_and_alpha_in_rust() {
    let a = color::preview(COLR, 0, "A", 96, 0, [1024, 512]).unwrap();
    let b = color::preview(COLR, 0, "A", 96, 1, [1024, 512]).unwrap();
    assert_ne!(a.image, b.image);
    assert_eq!(a.missing, 0);
    let p = a.image.rgba();
    assert!(p
        .chunks(4)
        .any(|p| p[0] > 200 && p[1] < 10 && p[2] < 10 && p[3] > 200));
    assert!(p.chunks(4).any(|p| p[0] > 80 && p[2] > 80));
    assert!(b.image.rgba().chunks(4).any(|p| p[1] > 200 && p[0] < 10));
    assert_eq!(
        color::preview(COLR, 0, "X", 96, 0, [1024, 512])
            .unwrap()
            .missing,
        1
    );
}
#[test]
fn sbix_preview_keeps_color_and_unpremultiplies_alpha() {
    let p = color::preview(SBIX, 0, "A", 96, 0, [1024, 512]).unwrap();
    assert!(p.image.rgba().chunks(4).any(|p| p[0] > 200 && p[3] > 200));
    assert!(p
        .image
        .rgba()
        .chunks(4)
        .any(|p| p[2] > 200 && (100..150).contains(&p[3])));
}
#[test]
fn unsupported_previews_and_invalid_requests_are_explicit() {
    for (text, size, palette, dimensions) in [
        ("", 96, 0, [1024, 512]),
        ("A", 193, 0, [1024, 512]),
        ("A", 96, 2, [1024, 512]),
        ("A", 96, 0, [4096, 512]),
    ] {
        assert!(color::preview(COLR, 0, text, size, palette, dimensions).is_err());
    }
    assert!(color::info(b"bad", 0).is_err());
    assert!(color::export(COLR, 1).is_err());
    assert!(color::preview(COLR, 0, &"A".repeat(65), 96, 0, [1024, 512]).is_err());
}

#[test]
fn cbdt_png_and_cff_color_outlines_preview_and_preserve() {
    for bytes in [
        include_bytes!("data/color/cbdt.ttf").as_slice(),
        include_bytes!("data/color/colrv0.otf").as_slice(),
    ] {
        let p = color::preview(bytes, 0, "A", 96, 0, [1024, 512]).unwrap();
        assert!(p
            .image
            .rgba()
            .chunks(4)
            .any(|p| p[0] > 200 && p[1] < 10 && p[3] > 200));
        let output = color::export(bytes, 0).unwrap();
        color::verify(&output, bytes, 0).unwrap();
    }
    assert_eq!(
        color::info(include_bytes!("data/color/colrv0.otf"), 0)
            .unwrap()
            .extension,
        "otf"
    );
}
#[test]
fn collection_faces_are_extracted_and_verified_individually() {
    let collection = include_bytes!("data/color/collection.ttc");
    for index in [0, 1] {
        let output = color::export(collection, index).unwrap();
        assert!(!output.starts_with(b"ttcf"));
        color::verify(&output, collection, index).unwrap();
        let p = color::preview(&output, 0, "A", 96, 0, [1024, 512]).unwrap();
        assert_eq!(p.missing, 0);
    }
    assert!(color::export(collection, 2).is_err());
}

#[test]
fn color_woff2_preserves_all_formats_and_collection_faces() {
    let cases: &[(&[u8], u32)] = &[
        (COLR, 0),
        (V1, 0),
        (SVG, 0),
        (SBIX, 0),
        (include_bytes!("data/color/cbdt.ttf"), 0),
        (include_bytes!("data/color/colrv0.otf"), 0),
        (include_bytes!("data/color/collection.ttc"), 0),
        (include_bytes!("data/color/collection.ttc"), 1),
    ];
    for &(source, index) in cases {
        for quality in [1, 5, 11] {
            let bytes = color::export_woff2(source, index, quality).unwrap();
            assert!(bytes.starts_with(b"wOF2"));
            color::verify_woff2(&bytes, source, index).unwrap();
            let decoded = font_core::font_formats::woff2::decode(&bytes).unwrap();
            let info = color::info(source, index).unwrap();
            assert_eq!(color::info(&decoded, 0).unwrap().formats, info.formats);
            if info.preview {
                let a = color::preview(source, index, "A", 96, 0, [256, 256]).unwrap();
                let b = color::preview(&decoded, 0, "A", 96, 0, [256, 256]).unwrap();
                assert_eq!(a.image.rgba(), b.image.rgba());
            }
            assert!(color::verify_woff2(&bytes[..bytes.len() / 2], source, index).is_err());
        }
    }
}

#[test]
fn color_woff2_rejects_bad_settings_and_mismatched_content() {
    for quality in [0, 12, u32::MAX] {
        assert_eq!(
            color::export_woff2(COLR, 0, quality).unwrap_err(),
            "woff2_options_invalid"
        );
    }
    let bytes = color::export_woff2(COLR, 0, 5).unwrap();
    assert!(color::verify_woff2(&bytes, SBIX, 0).is_err());
    assert!(color::export_woff2(COLR, 99, 5).is_err());
    assert!(color::export_woff2(b"bad", 0, 5).is_err());
}
