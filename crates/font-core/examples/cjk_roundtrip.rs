//! Local real-font QA. Input stays untouched; outputs go to a separate directory.
use font_core::{
    font_formats::{self, Format},
    font_io, storage, Editor,
};
fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().collect();
    let input = std::path::Path::new(args.get(1).ok_or("input font required")?);
    let out = std::path::Path::new(args.get(2).ok_or("output directory required")?);
    std::fs::create_dir_all(out).map_err(|e| e.to_string())?;
    let started = std::time::Instant::now();
    let doc = font_io::import(&font_io::read(input)?, 0)?;
    let count = doc.glyphs.len();
    let segments: usize = doc
        .glyphs
        .values()
        .flat_map(|g| &g.contours)
        .map(|c| c.segments.len())
        .sum();
    println!(
        "Imported {count} glyphs, {segments} segments in {:?}",
        started.elapsed()
    );
    let mut editor = Editor::new(doc).map_err(|e| e.to_string())?;
    let id = editor
        .document()
        .glyphs
        .values()
        .find(|g| g.codepoints.contains(&'中'))
        .ok_or("missing 中")?
        .id;
    let mut g = editor.document().glyphs[&id].clone();
    g.advance += 1.;
    editor.replace_glyph(0, g).map_err(|e| e.to_string())?;
    editor.undo(1).map_err(|e| e.to_string())?;
    editor.redo(2).map_err(|e| e.to_string())?;
    let project = out.join("cjk.fontproj.json");
    storage::save(&project, editor.document()).map_err(|e| e.to_string())?;
    let restored = storage::load(&project).map_err(|e| e.to_string())?;
    assert_eq!(&restored, editor.document());
    drop(restored);
    for format in [Format::Ttf, Format::CidOtf] {
        let bytes = font_formats::export(editor.document(), format)?;
        let restored = font_io::import(&bytes, 0)?;
        assert_eq!(restored.glyphs.len(), count);
        for (id, g) in &editor.document().glyphs {
            assert_eq!(restored.glyphs[id].codepoints, g.codepoints);
            assert_eq!(restored.glyphs[id].advance, g.advance.round());
        }
        storage::write_bytes(&out.join(format!("cjk.{}", format.extension())), &bytes)
            .map_err(|e| e.to_string())?;
        println!(
            "{format:?}: {} bytes, all glyph mappings/advances retained",
            bytes.len()
        );
    }
    println!(
        "Project: {} bytes; total {:?}",
        std::fs::metadata(project).map_err(|e| e.to_string())?.len(),
        started.elapsed()
    );
    Ok(())
}
