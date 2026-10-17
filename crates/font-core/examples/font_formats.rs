//! Generate static interchange samples for independent validators.
use font_core::{
    font_formats::{self, Format},
    font_io, Contour, FontDocument, Segment,
};
fn main() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let directory = std::path::PathBuf::from(args.next().ok_or("output directory required")?);
    std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    let d = if let Some(input) = args.next() {
        font_io::import(
            &font_io::read(std::path::Path::new(&input))?,
            args.next().and_then(|n| n.parse().ok()).unwrap_or(0),
        )?
    } else {
        let mut d = FontDocument::new("Fonts Formats QA", "A😀𠀀").map_err(|e| e.to_string())?;
        d.metadata.copyright = "Copyright Fonts QA".into();
        d.metadata.license = "Test font".into();
        d.glyphs.get_mut(&1).unwrap().contours = vec![
            Contour {
                smooth: Default::default(),
                start: [10., 20.],
                segments: vec![Segment::Cubic {
                    control1: [80., 700.],
                    control2: [500., 700.],
                    end: [590., 20.],
                }],
                closed: true,
            },
            Contour {
                smooth: Default::default(),
                start: [200., 100.],
                segments: vec![Segment::Quadratic {
                    control: [300., 320.],
                    end: [400., 100.],
                }],
                closed: true,
            },
        ];
        d
    };
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
        let bytes = font_formats::export(&d, format)?;
        let path = directory.join(format!("{:?}.{}", format, format.extension()));
        font_core::storage::write_bytes(&path, &bytes).map_err(|e| e.to_string())?;
        println!("{}: {} bytes", format.label(), bytes.len());
    }
    Ok(())
}
