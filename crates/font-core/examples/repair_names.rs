fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<_> = std::env::args().collect();
    let b = font_core::font_io::read(std::path::Path::new(&a[1]))?;
    let fixed = font_core::font_names::repair(&b)?;
    std::fs::write(&a[2], fixed)?;
    Ok(())
}
