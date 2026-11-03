//! Development probe: cargo run -p font-core --example color_preview -- font.ttf out.png ABC 0
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 5 {
        return Err("usage: color_preview FONT OUTPUT.png TEXT PALETTE".into());
    }
    let bytes = font_core::font_io::read(std::path::Path::new(&args[1]))?;
    let preview =
        font_core::color_fonts::preview(&bytes, 0, &args[3], 100, args[4].parse()?, [1024, 512])?;
    image::save_buffer(
        &args[2],
        preview.image.rgba(),
        preview.image.width(),
        preview.image.height(),
        image::ColorType::Rgba8,
    )?;
    println!("{}; missing={}", preview.label, preview.missing);
    Ok(())
}
