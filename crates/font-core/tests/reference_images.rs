use font_core::{
    font_formats::{self, Format},
    proof,
    reference_images::{ImageAsset, MAX_FILE_BYTES, MAX_PER_GLYPH},
    storage, Editor, Error, FontDocument,
};
use image::{DynamicImage, ImageFormat, RgbaImage};
use std::{io::Cursor, path::PathBuf};

fn png() -> Vec<u8> {
    let pixels = RgbaImage::from_raw(2, 1, vec![255, 0, 0, 255, 0, 0, 255, 64]).unwrap();
    let mut output = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(pixels)
        .write_to(&mut output, ImageFormat::Png)
        .unwrap();
    output.into_inner()
}
fn editor() -> Editor {
    Editor::new(FontDocument::new("Reference 日本語 中文", "AB").unwrap()).unwrap()
}
fn placed() -> Editor {
    let mut editor = editor();
    editor
        .place_reference_image(0, 1, ImageAsset::from_bytes(png()).unwrap())
        .unwrap();
    editor
}
fn directory(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "fonts-images-{label}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&dir).unwrap();
    dir
}
#[test]
fn png_alpha_and_jpeg_are_decoded_in_rust() {
    let asset = ImageAsset::from_bytes(png()).unwrap();
    assert_eq!((asset.width(), asset.height()), (2, 1));
    assert_eq!(asset.rgba(), &[255, 0, 0, 255, 0, 0, 255, 64]);
    let mut jpeg = Cursor::new(Vec::new());
    DynamicImage::new_rgb8(3, 2)
        .write_to(&mut jpeg, ImageFormat::Jpeg)
        .unwrap();
    let asset = ImageAsset::from_bytes(jpeg.into_inner()).unwrap();
    assert_eq!((asset.width(), asset.height()), (3, 2));
    assert_eq!(asset.rgba().len(), 24);
}
#[test]
fn import_is_bounded_and_rejects_truncated_or_other_formats() {
    for bytes in [
        vec![],
        b"GIF89a".to_vec(),
        png()[..35].to_vec(),
        b"not an image".to_vec(),
    ] {
        assert!(ImageAsset::from_bytes(bytes).is_err());
    }
    assert_eq!(
        ImageAsset::from_bytes(vec![0; MAX_FILE_BYTES + 1]),
        Err(Error::TooLarge)
    );
    let dir = directory("bounded");
    let file = dir.join("large.png");
    std::fs::File::create(&file)
        .unwrap()
        .set_len(MAX_FILE_BYTES as u64 + 1)
        .unwrap();
    assert_eq!(ImageAsset::load(&file), Err(Error::TooLarge));
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn fits_glyph_bounds_and_zero_advance_without_changing_outline() {
    let editor = placed();
    let image = &editor.document().reference_images.for_glyph(1)[0];
    assert_eq!(image.center, [300., 300.]);
    assert_eq!(
        (image.width, image.height, image.opacity),
        (600., 300., 0.5)
    );
    assert!(editor.document().glyphs[&1].contours.is_empty());
    let mut doc = FontDocument::new("Zero", "A").unwrap();
    doc.glyphs.get_mut(&1).unwrap().advance = 0.;
    let mut editor = Editor::new(doc).unwrap();
    editor
        .place_reference_image(0, 1, ImageAsset::from_bytes(png()).unwrap())
        .unwrap();
    let image = &editor.document().reference_images.for_glyph(1)[0];
    assert_eq!((image.width, image.height), (1000., 500.));
}
#[test]
fn repeated_import_deduplicates_pixels_and_history_snapshots_share_them() {
    let mut editor = placed();
    let snapshot = editor.document().clone();
    let asset = snapshot.reference_images.assets().values().next().unwrap();
    editor
        .place_reference_image(1, 2, ImageAsset::from_bytes(png()).unwrap())
        .unwrap();
    assert_eq!(editor.document().reference_images.assets().len(), 1);
    let current = editor
        .document()
        .reference_images
        .assets()
        .values()
        .next()
        .unwrap();
    assert!(std::ptr::eq(asset.rgba().as_ptr(), current.rgba().as_ptr()));
    let mut image = editor.document().reference_images.for_glyph(1)[0].clone();
    image.angle = 90.;
    image.opacity = 0.;
    image.visible = false;
    image.locked = true;
    editor.update_reference_image(2, 1, image).unwrap();
    editor.undo(3).unwrap();
    editor.redo(4).unwrap();
    let current = editor
        .document()
        .reference_images
        .assets()
        .values()
        .next()
        .unwrap();
    assert!(std::ptr::eq(asset.rgba().as_ptr(), current.rgba().as_ptr()));
}
#[test]
fn placement_transform_delete_and_mixed_edits_undo_in_order() {
    let mut editor = editor();
    let original = editor.document().clone();
    let id = editor
        .place_reference_image(0, 1, ImageAsset::from_bytes(png()).unwrap())
        .unwrap();
    let placed = editor.document().clone();
    let mut draft = placed.reference_images.for_glyph(1)[0].clone();
    // A caller may update the draft many times, then commit the gesture once.
    for x in 0..20 {
        draft.center[0] = x as f64;
    }
    draft.angle = 30.;
    draft.width = 500.;
    draft.opacity = 0.25;
    editor.update_reference_image(1, 1, draft).unwrap();
    let transformed = editor.document().clone();
    let mut glyph = transformed.glyphs[&1].clone();
    glyph.advance = 800.;
    editor.replace_glyph(2, glyph).unwrap();
    editor.remove_reference_image(3, 1, id).unwrap();
    assert!(editor.document().reference_images.assets().is_empty());
    editor.undo(4).unwrap();
    assert_eq!(
        editor.document().reference_images,
        transformed.reference_images
    );
    editor.undo(5).unwrap();
    assert_eq!(editor.document(), &transformed);
    editor.undo(6).unwrap();
    assert_eq!(editor.document(), &placed);
    editor.undo(7).unwrap();
    assert_eq!(editor.document(), &original);
    for _ in 0..4 {
        editor.redo(editor.revision()).unwrap();
    }
    assert_eq!(editor.document().glyphs[&1].advance, 800.);
    assert!(editor.document().reference_images.is_empty());
}
#[test]
fn invalid_and_stale_operations_leave_document_revision_and_redo_unchanged() {
    let mut editor = placed();
    let old = editor.document().clone();
    let image = old.reference_images.for_glyph(1)[0].clone();
    let mut bad = image.clone();
    bad.width = -1.;
    assert_eq!(
        editor.update_reference_image(1, 1, bad),
        Err(Error::InvalidGeometry)
    );
    for value in [f64::NAN, f64::INFINITY, -0.01, 1.01] {
        let mut bad = image.clone();
        bad.opacity = value;
        assert_eq!(
            editor.update_reference_image(1, 1, bad),
            Err(Error::InvalidGeometry)
        );
    }
    let mut bad = image.clone();
    bad.asset_id = "missing".into();
    assert_eq!(
        editor.update_reference_image(1, 1, bad),
        Err(Error::InvalidDocument)
    );
    assert_eq!(
        editor.remove_reference_image(0, 1, image.id),
        Err(Error::Conflict)
    );
    assert_eq!(
        editor.remove_reference_image(1, 2, image.id),
        Err(Error::InvalidDocument)
    );
    assert_eq!(
        editor.reorder_reference_image(1, 1, image.id, 1),
        Err(Error::InvalidDocument)
    );
    assert_eq!(
        editor.place_reference_image(1, 999, ImageAsset::from_bytes(png()).unwrap()),
        Err(Error::MissingGlyph)
    );
    assert_eq!(editor.document(), &old);
    assert_eq!(editor.revision(), 1);
    editor.remove_reference_image(1, 1, image.id).unwrap();
    editor.undo(2).unwrap();
    assert_eq!(editor.update_reference_image(3, 1, image), Ok(3));
    assert!(editor.can_redo());
}
#[test]
fn rotated_hit_test_uses_local_rectangle_and_respects_lock_and_visibility() {
    let editor = placed();
    let mut image = editor.document().reference_images.for_glyph(1)[0].clone();
    image.angle = 37.;
    image.opacity = 0.;
    for p in [[0., 0.], [-0.4, 0.49], [0.8, 0.8]] {
        let result = image.to_local(image.to_document(p));
        assert!((result[0] - p[0]).abs() < 1e-10 && (result[1] - p[1]).abs() < 1e-10);
    }
    assert!(image.hit_test(image.to_document([0.4, 0.4])));
    assert!(!image.hit_test(image.to_document([0.51, 0.])));
    assert!(!image.hit_test([f64::NAN, 0.]));
    image.locked = true;
    assert!(!image.hit_test(image.center));
    image.locked = false;
    image.visible = false;
    assert!(!image.hit_test(image.center));
}
#[test]
fn visibility_lock_zero_opacity_and_stack_order_are_undoable() {
    let mut editor = placed();
    let id = editor
        .place_reference_image(1, 1, ImageAsset::from_bytes(png()).unwrap())
        .unwrap();
    editor.reorder_reference_image(2, 1, id, 0).unwrap();
    assert_eq!(editor.document().reference_images.for_glyph(1)[0].id, id);
    let mut image = editor.document().reference_images.for_glyph(1)[0].clone();
    image.visible = false;
    image.locked = true;
    image.opacity = 0.;
    editor.update_reference_image(3, 1, image.clone()).unwrap();
    image.center[0] += 1.;
    assert_eq!(
        editor.update_reference_image(4, 1, image),
        Err(Error::Conflict)
    );
    editor.remove_reference_image(4, 1, id).unwrap();
    editor.undo(5).unwrap();
    let image = &editor.document().reference_images.for_glyph(1)[0];
    assert!(image.locked && !image.visible && image.opacity == 0.);
    editor.undo(6).unwrap();
    editor.undo(7).unwrap();
    assert_eq!(editor.document().reference_images.for_glyph(1)[1].id, id);
}
#[test]
fn too_many_placements_are_rejected_atomically() {
    let mut editor = editor();
    let asset = ImageAsset::from_bytes(png()).unwrap();
    for _ in 0..MAX_PER_GLYPH {
        editor
            .place_reference_image(editor.revision(), 1, asset.clone())
            .unwrap();
    }
    let before = editor.document().clone();
    assert_eq!(
        editor.place_reference_image(editor.revision(), 1, asset),
        Err(Error::TooLarge)
    );
    assert_eq!(editor.document(), &before);
}
#[test]
fn embedded_project_loads_after_source_is_deleted_and_failed_save_keeps_previous() {
    let dir = directory("embedded");
    let source = dir.join("source.png");
    let project = dir.join("project.json");
    std::fs::write(&source, png()).unwrap();
    let mut editor = editor();
    editor
        .place_reference_image(0, 1, ImageAsset::load(&source).unwrap())
        .unwrap();
    storage::save(&project, editor.document()).unwrap();
    let good = std::fs::read(&project).unwrap();
    std::fs::remove_file(source).unwrap();
    assert_eq!(storage::load(&project).unwrap(), *editor.document());
    let mut invalid = editor.document().clone();
    invalid.glyphs.remove(&1);
    assert_eq!(storage::save(&project, &invalid), Err(Error::MissingGlyph));
    assert_eq!(std::fs::read(&project).unwrap(), good);
    editor.remove_reference_image(1, 1, 1).unwrap();
    storage::save(&project, editor.document()).unwrap();
    assert_eq!(storage::load(&project).unwrap(), *editor.document());
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn legacy_projects_load_and_malformed_assets_or_placements_are_rejected() {
    let old = FontDocument::new("Old", "A").unwrap();
    let json = serde_json::to_value(&old).unwrap();
    assert!(json.get("reference_images").is_none());
    assert_eq!(serde_json::from_value::<FontDocument>(json).unwrap(), old);
    let doc = placed().document().clone();
    let json = serde_json::to_value(&doc).unwrap();
    for field in ["version", "next_id"] {
        let mut bad = json.clone();
        bad["reference_images"][field] = 0.into();
        assert!(serde_json::from_value::<FontDocument>(bad).is_err());
    }
    let id = doc.reference_images.assets().keys().next().unwrap();
    let mut bad = json.clone();
    bad["reference_images"]["assets"][id] = "not base64".into();
    assert!(serde_json::from_value::<FontDocument>(bad).is_err());
    let mut bad = json.clone();
    bad["reference_images"]["assets"]
        .as_object_mut()
        .unwrap()
        .clear();
    assert!(serde_json::from_value::<FontDocument>(bad).is_err());
    let mut bad = json.clone();
    bad["reference_images"]["placements"]["1"][0]["width"] = 0.into();
    assert!(serde_json::from_value::<FontDocument>(bad).is_err());
    let mut bad = json.clone();
    bad["reference_images"]["placements"]["2"] = bad["reference_images"]["placements"]["1"].clone();
    assert!(serde_json::from_value::<FontDocument>(bad).is_err());
    let mut bad = json;
    bad["reference_images"]["placements"]
        .as_object_mut()
        .unwrap()
        .clear();
    assert!(serde_json::from_value::<FontDocument>(bad).is_err());
}
#[test]
fn fonts_and_proof_exclude_reference_images_even_for_image_only_glyphs() {
    let editor = placed();
    let doc = editor.document();
    let mut plain = doc.clone();
    plain.reference_images = Default::default();
    for format in [
        Format::Ttf,
        Format::Otf,
        Format::CidOtf,
        Format::Pfa,
        Format::Pfb,
        Format::Cid,
        Format::Cff,
        Format::CidCff,
        Format::Cmap,
    ] {
        assert_eq!(
            font_formats::export(doc, format).unwrap(),
            font_formats::export(&plain, format).unwrap(),
            "{format:?}"
        );
    }
    let subset = proof::subset(doc, "A").unwrap();
    assert!(subset.reference_images.is_empty());
    let proof = proof::prepare(subset, "A").unwrap();
    assert_eq!(proof.empty, 1);
    assert!(proof.document.reference_images.is_empty());
}
