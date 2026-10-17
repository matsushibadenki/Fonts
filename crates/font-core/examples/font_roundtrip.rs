//! Headless interchange smoke check: cargo run -p font-core --example font_roundtrip -- OUTPUT.ttf [INPUT.ttf|INPUT.ttc] [FACE_INDEX]
use font_core::{font_io, Contour, FontDocument, Segment};
fn main() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let output = args.next().ok_or("output path required")?;
    let doc = if let Some(input) = args.next() {
        let index = args
            .next()
            .map(|s| s.parse::<u32>())
            .transpose()
            .map_err(|e| e.to_string())?
            .unwrap_or(0);
        font_io::import(&font_io::read(std::path::Path::new(&input))?, index)?
    } else {
        let mut doc =
            FontDocument::new("Fonts Interchange Test", "A😀𠀀").map_err(|e| e.to_string())?;
        doc.glyphs.get_mut(&1).unwrap().contours = vec![Contour {
            smooth: Default::default(),
            start: [0., 0.],
            segments: vec![
                Segment::Line { end: [300., 700.] },
                Segment::Line { end: [600., 0.] },
            ],
            closed: true,
        }];
        doc
    };
    let bytes = font_io::export(&doc)?;
    font_core::storage::write_bytes(std::path::Path::new(&output), &bytes)
        .map_err(|e| e.to_string())?;
    println!("{} glyphs -> {} bytes", doc.glyphs.len(), bytes.len());
    Ok(())
}
