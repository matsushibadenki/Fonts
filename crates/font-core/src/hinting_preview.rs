//! Bounded FreeType comparison, with automatic rasterizer hints explicitly disabled.
use crate::{font_io::Result, hinting};
use freetype::face::LoadFlag;
use image::{GrayImage, ImageEncoder};

pub const SIZES: [u32; 4] = [9, 12, 16, 24];
pub struct Preview {
    pub images: Vec<Vec<u8>>,
    pub sample: String,
    pub missing: usize,
    pub auto_error: Option<String>,
}
/// Columns are original, unhinted, and (when available) generated hints.
/// Images remain native resources; callers return only their resource IDs to the UI.
pub fn compare(bytes: &[u8], index: u32, text: &str) -> Result<Preview> {
    if text.is_empty() || text.chars().count() > 12 || text.chars().any(char::is_control) {
        return Err("hinting_sample".into());
    }
    let info = hinting::info(bytes, index)?;
    if !info.can_autohint {
        return Err("autohint_unsupported".into());
    }
    let original = hinting::export(bytes, index, false)?;
    let removed = hinting::export(bytes, index, true)?;
    let generated = hinting::autohint(bytes, index);
    let library = freetype::Library::init().map_err(|_| "hinting_preview")?;
    let source = library
        .new_memory_face(original.clone(), 0)
        .map_err(|_| "hinting_preview")?;
    let missing = text
        .chars()
        .filter(|c| source.get_char_index(*c as usize).is_none())
        .count();
    drop(source);
    let mut images = vec![];
    for (font, unhinted) in [(&original, false), (&removed, true)]
        .into_iter()
        .chain(generated.as_ref().ok().map(|b| (b, false)))
    {
        let face = library
            .new_memory_face(font.clone(), 0)
            .map_err(|_| "hinting_preview")?;
        for size in SIZES {
            face.set_pixel_sizes(0, size)
                .map_err(|_| "hinting_preview")?;
            let mut image = GrayImage::from_pixel(320, 64, image::Luma([255]));
            let mut pen: freetype::ffi::FT_Pos = 8 * 64;
            for c in text.chars() {
                let mut flags = LoadFlag::RENDER | LoadFlag::NO_AUTOHINT | LoadFlag::NO_BITMAP;
                if unhinted {
                    flags |= LoadFlag::NO_HINTING;
                }
                face.load_char(c as usize, flags)
                    .map_err(|_| "hinting_preview")?;
                let slot = face.glyph();
                let bitmap = slot.bitmap();
                if bitmap.width() > 0 && bitmap.rows() > 0 {
                    if bitmap.pixel_mode().map_err(|_| "hinting_preview")?
                        != freetype::bitmap::PixelMode::Gray
                    {
                        return Err("hinting_preview".into());
                    }
                    let width = bitmap.width() as usize;
                    let rows = bitmap.rows() as usize;
                    let pitch = bitmap.pitch().unsigned_abs() as usize;
                    if pitch < width || bitmap.buffer().len() < pitch.saturating_mul(rows) {
                        return Err("hinting_preview".into());
                    }
                    for y in 0..rows {
                        let row = if bitmap.pitch() < 0 { rows - 1 - y } else { y };
                        for x in 0..width {
                            let px = pen / 64
                                + slot.bitmap_left() as freetype::ffi::FT_Pos
                                + x as freetype::ffi::FT_Pos;
                            let py = 44 - i64::from(slot.bitmap_top()) + y as i64;
                            if (0..320).contains(&px) && (0..64).contains(&py) {
                                let coverage = u16::from(bitmap.buffer()[row * pitch + x]);
                                let pixel = &mut image.get_pixel_mut(px as u32, py as u32).0[0];
                                *pixel = (u16::from(*pixel) * (255 - coverage) / 255) as u8;
                            }
                        }
                    }
                }
                pen = pen.saturating_add(slot.advance().x);
                if pen >= 312 * 64 {
                    break;
                }
            }
            let mut png = vec![];
            image::codecs::png::PngEncoder::new(&mut png)
                .write_image(image.as_raw(), 320, 64, image::ExtendedColorType::L8)
                .map_err(|_| "hinting_preview")?;
            images.push(png);
        }
    }
    Ok(Preview {
        images,
        sample: text.to_owned(),
        missing,
        auto_error: generated.err(),
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn truetype_comparison_executes_generated_bytecode() {
        let doc = crate::font_io::import(include_bytes!("../tests/data/hinting/cff-hints.otf"), 0)
            .unwrap();
        let bytes = crate::font_io::export(&doc).unwrap();
        let preview = compare(&bytes, 0, "A").unwrap();
        assert!(preview.auto_error.is_none());
        assert_eq!(preview.images.len(), 12);
        for row in 0..4 {
            assert_eq!(preview.images[row], preview.images[row + 4]);
        }
        assert!((0..4).any(|row| preview.images[row + 4] != preview.images[row + 8]));
    }
    #[test]
    fn generation_failure_keeps_original_and_unhinted_comparison() {
        let doc = crate::FontDocument::new("Empty", "A").unwrap();
        let bytes = crate::font_io::export(&doc).unwrap();
        let preview = compare(&bytes, 0, "A").unwrap();
        assert_eq!(preview.images.len(), 8);
        assert!(preview.auto_error.is_some());
    }
    #[test]
    fn compares_real_programs_and_bounds_the_input() {
        for bytes in [
            include_bytes!("../tests/data/hinting/cff-hints.otf").as_slice(),
            include_bytes!("../tests/data/hinting/cid-hints.otf").as_slice(),
        ] {
            let preview = compare(bytes, 0, "A水").unwrap();
            assert_eq!(preview.images.len(), 12);
            assert_eq!(preview.missing, 1);
            assert!(preview.auto_error.is_none());
            for png in &preview.images {
                let image = image::load_from_memory(png).unwrap().to_luma8();
                assert_eq!(image.dimensions(), (320, 64));
                assert!(image.pixels().any(|p| p.0[0] < 255));
            }
            for invalid in ["", "A\n", &"A".repeat(13)] {
                assert!(compare(bytes, 0, invalid).is_err());
            }
        }
    }
}
