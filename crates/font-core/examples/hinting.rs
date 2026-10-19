//! Standalone developer verification; native engines are linked into font-core.
fn main() -> Result<(), String> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 4 {
        return Err("Usage: hinting INPUT OUTPUT preserve|remove|auto".into());
    }
    let source = font_core::font_io::read(std::path::Path::new(&args[1]))?;
    let output = match args[3].as_str() {
        "preserve" => font_core::hinting::export(&source, 0, false)?,
        "remove" => font_core::hinting::export(&source, 0, true)?,
        "auto" => font_core::hinting::autohint(&source, 0)?,
        _ => return Err("Invalid mode".into()),
    };
    font_core::storage::write_bytes(std::path::Path::new(&args[2]), &output)
        .map_err(|e| e.to_string())
}
