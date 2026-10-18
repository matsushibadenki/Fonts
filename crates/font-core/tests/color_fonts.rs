use font_core::{color_fonts as color, font_io};
const COLR: &[u8] = include_bytes!("data/color/colrv0.ttf");
const V1: &[u8] = include_bytes!("data/color/colrv1.ttf");
const SVG: &[u8] = include_bytes!("data/color/svg.ttf");
const SBIX: &[u8] = include_bytes!("data/color/sbix.ttf");
#[test]
fn color_formats_keep_every_table_on_export() {
    for (source, format, preview) in [
        (COLR, "COLRv0", true),
        (V1, "COLRv1", false),
        (SVG, "SVG", false),
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
    for source in [V1, SVG] {
        assert_eq!(
            color::preview(source, 0, "A", 96, 0, [1024, 512])
                .err()
                .unwrap(),
            "color_preview_unsupported"
        );
    }
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
