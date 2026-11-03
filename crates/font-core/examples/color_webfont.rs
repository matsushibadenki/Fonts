//! Export a selected color face to WOFF2 for browser interoperability testing.
fn main() {
    let args: Vec<_> = std::env::args().collect();
    let source = font_core::font_io::read(std::path::Path::new(&args[1])).unwrap();
    let output = font_core::color_fonts::export_woff2(&source, 0, 5).unwrap();
    font_core::color_fonts::verify_woff2(&output, &source, 0).unwrap();
    std::fs::write(&args[2], output).unwrap();
}
