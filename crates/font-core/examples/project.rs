//! A runnable vertical slice until the Tauri shell is connected.
use font_core::{storage, Contour, Editor, FontDocument, GlyphStatus, Segment};
use std::path::PathBuf;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp/fonts-demo.fontproj.json"));
    let mut editor = Editor::new(FontDocument::new("My First Font", "A Bあ中")?)?;
    let mut glyph = editor.document().glyphs[&1].clone();
    glyph.contours = vec![Contour {
        smooth: Default::default(),
        start: [50., 0.],
        segments: vec![
            Segment::Line { end: [300., 700.] },
            Segment::Line { end: [550., 0.] },
        ],
        closed: true,
    }];
    glyph.status = GlyphStatus::Reviewed;
    editor.replace_glyph(0, glyph)?;
    editor.undo(1)?;
    editor.redo(2)?;
    storage::save(&path, editor.document())?;
    let loaded = storage::load(&path)?;
    println!(
        "Saved and reloaded {} glyphs at revision {}: {}",
        loaded.glyphs.len(),
        editor.revision(),
        path.display()
    );
    Ok(())
}
