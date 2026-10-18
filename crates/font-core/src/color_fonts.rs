//! Color fonts are preserved independently from editable monochrome documents.
use crate::{font_io, reference_images::ImageAsset};
use freetype::{bitmap::PixelMode, face::LoadFlag};
use serde::Serialize;
use std::collections::BTreeMap;
use ttf_parser::{Face, Tag};
#[derive(Clone, Debug, Serialize)]
pub struct Info {
    pub family: String,
    pub formats: Vec<String>,
    pub palettes: u16,
    pub preview: bool,
    pub extension: String,
    pub sample: String,
}
fn table<'a>(face: &Face<'a>, tag: &[u8; 4]) -> Option<&'a [u8]> {
    face.raw_face().table(Tag::from_bytes(tag))
}
pub fn info(bytes: &[u8], index: u32) -> Result<Info, String> {
    if bytes.len() as u64 > font_io::MAX_FONT_BYTES {
        return Err("font_too_large".into());
    }
    let face = Face::parse(bytes, index).map_err(|_| "font_invalid")?;
    let mut formats = Vec::new();
    let mut unsupported = false;
    if let Some(colr) = table(&face, b"COLR") {
        let v = colr.get(..2).ok_or("color_invalid")?;
        let v = u16::from_be_bytes([v[0], v[1]]);
        formats.push(format!("COLRv{v}"));
        unsupported = v != 0;
        if face.tables().colr.is_none() || face.color_palettes().is_none() {
            return Err("color_invalid".into());
        }
    }
    for (tag, name) in [(b"sbix", "sbix"), (b"CBDT", "CBDT"), (b"SVG ", "SVG")] {
        if table(&face, tag).is_some() {
            formats.push(name.into());
            if name == "SVG" {
                unsupported = true;
            }
        }
    }
    if formats.is_empty() {
        return Err("color_not_font".into());
    }
    let mut sample = String::new();
    if let Some(cmap) = face.tables().cmap {
        for sub in cmap.subtables.into_iter().filter(|s| s.is_unicode()) {
            sub.codepoints(|n| {
                if sample.chars().count() < 12 {
                    if let Some(c) =
                        char::from_u32(n).filter(|c| !c.is_control() && !c.is_whitespace())
                    {
                        if !sample.contains(c) {
                            sample.push(c);
                        }
                    }
                }
            });
            if !sample.is_empty() {
                break;
            }
        }
    }
    Ok(Info {
        family: font_io::name(&face, 1),
        formats,
        palettes: face.color_palettes().map(|n| n.get()).unwrap_or(0),
        preview: !unsupported,
        extension: if table(&face, b"CFF ").is_some() || table(&face, b"CFF2").is_some() {
            "otf"
        } else {
            "ttf"
        }
        .into(),
        sample,
    })
}
fn sum(bytes: &[u8]) -> u32 {
    bytes.chunks(4).fold(0u32, |a, b| {
        let mut v = [0; 4];
        v[..b.len()].copy_from_slice(b);
        a.wrapping_add(u32::from_be_bytes(v))
    })
}
/// Extract one collection face, preserving all table payloads except stale digital signatures.
pub fn export(bytes: &[u8], index: u32) -> Result<Vec<u8>, String> {
    let metadata = info(bytes, index)?;
    let face = Face::parse(bytes, index).map_err(|_| "font_invalid")?;
    let mut tables = BTreeMap::new();
    for r in face.raw_face().table_records {
        let tag = r.tag.to_bytes();
        if &tag == b"DSIG" {
            continue;
        }
        let mut data = table(&face, &tag).ok_or("color_invalid")?.to_vec();
        if &tag == b"head" {
            data.get_mut(8..12).ok_or("color_invalid")?.fill(0);
        }
        if tables.insert(tag, data).is_some() {
            return Err("color_invalid".into());
        }
    }
    if tables.is_empty() || tables.len() > 4095 {
        return Err("color_invalid".into());
    }
    let count = tables.len() as u16;
    let power = 1u16 << count.ilog2();
    let mut out = Vec::new();
    out.extend(if metadata.extension == "otf" {
        *b"OTTO"
    } else {
        [0, 1, 0, 0]
    });
    for n in [
        count,
        power * 16,
        count.ilog2() as u16,
        count * 16 - power * 16,
    ] {
        out.extend(n.to_be_bytes());
    }
    let mut payload = Vec::new();
    let base = 12 + tables.len() * 16;
    let mut head = None;
    for (tag, data) in tables {
        let at = base + payload.len();
        if &tag == b"head" {
            head = Some(at);
        }
        out.extend(tag);
        out.extend(sum(&data).to_be_bytes());
        out.extend((at as u32).to_be_bytes());
        out.extend((data.len() as u32).to_be_bytes());
        payload.extend(data);
        while payload.len() % 4 != 0 {
            payload.push(0);
        }
        if base + payload.len() > font_io::MAX_FONT_BYTES as usize {
            return Err("font_too_large".into());
        }
    }
    out.extend(payload);
    let adjust = 0xB1B0AFBAu32.wrapping_sub(sum(&out));
    let h = head.ok_or("color_invalid")?;
    out[h + 8..h + 12].copy_from_slice(&adjust.to_be_bytes());
    verify(&out, bytes, index)?;
    Ok(out)
}
pub fn verify(output: &[u8], source: &[u8], index: u32) -> Result<(), String> {
    if output.len() as u64 > font_io::MAX_FONT_BYTES
        || !output.len().is_multiple_of(4)
        || sum(output) != 0xB1B0AFBA
    {
        return Err("color_invalid_output".into());
    }
    let original = Face::parse(source, index).map_err(|_| "color_invalid")?;
    let face = Face::parse(output, 0).map_err(|_| "color_invalid_output")?;
    info(output, 0)?;
    let original_count = original
        .raw_face()
        .table_records
        .into_iter()
        .filter(|r| r.tag != Tag::from_bytes(b"DSIG"))
        .count();
    if face.raw_face().table_records.len() as usize != original_count {
        return Err("color_invalid_output".into());
    }
    for r in original.raw_face().table_records {
        let tag = r.tag.to_bytes();
        if &tag == b"DSIG" {
            continue;
        }
        let a = table(&original, &tag).ok_or("color_invalid")?;
        let b = table(&face, &tag).ok_or("color_invalid_output")?;
        if &tag == b"head" {
            if a.len() != b.len() || a.get(..8) != b.get(..8) || a.get(12..) != b.get(12..) {
                return Err("color_invalid_output".into());
            }
        } else if a != b {
            return Err("color_invalid_output".into());
        }
    }
    for r in face.raw_face().table_records {
        let mut data = face
            .raw_face()
            .table(r.tag)
            .ok_or("color_invalid_output")?
            .to_vec();
        if r.tag == Tag::from_bytes(b"head") {
            data.get_mut(8..12).ok_or("color_invalid_output")?.fill(0);
        }
        if sum(&data) != r.check_sum {
            return Err("color_invalid_output".into());
        }
    }
    Ok(())
}
pub struct Preview {
    pub image: ImageAsset,
    pub label: String,
    pub missing: usize,
}
unsafe extern "C" {
    fn FT_Palette_Select(
        face: freetype::ffi::FT_Face,
        index: u16,
        palette: *mut *mut std::ffi::c_void,
    ) -> i32;
}
pub fn preview(
    bytes: &[u8],
    index: u32,
    text: &str,
    size: u32,
    palette: u16,
    dimensions: [u32; 2],
) -> Result<Preview, String> {
    let [width, height] = dimensions;
    if !(256..=2048).contains(&width) || !(256..=1024).contains(&height) {
        return Err("color_settings".into());
    }
    let metadata = info(bytes, index)?;
    if !metadata.preview {
        return Err("color_preview_unsupported".into());
    }
    if text.chars().count() > 64
        || text.is_empty()
        || !(12..=192).contains(&size)
        || palette >= metadata.palettes.max(1)
    {
        return Err("color_settings".into());
    }
    let lib = freetype::Library::init().map_err(|_| "color_render")?;
    let face = lib
        .new_memory_face(bytes.to_vec(), index as isize)
        .map_err(|_| "color_render")?;
    if metadata.palettes > 0 {
        // SAFETY: live FreeType face, checked palette index, optional output pointer.
        if unsafe {
            FT_Palette_Select(
                face.raw() as *const _ as *mut _,
                palette,
                std::ptr::null_mut(),
            )
        } != 0
        {
            return Err("color_render".into());
        }
    }
    let strikes = face.raw().num_fixed_sizes;
    if face.set_pixel_sizes(0, size).is_err() {
        if strikes <= 0 || strikes > 256 {
            return Err("color_render".into());
        }
        // SAFETY: FreeType owns num_fixed_sizes live strike records; count is bounded above.
        let strikes =
            unsafe { std::slice::from_raw_parts(face.raw().available_sizes, strikes as usize) };
        let best = strikes
            .iter()
            .enumerate()
            .min_by_key(|(_, s)| (s.y_ppem / 64 - size as i64).abs())
            .ok_or("color_render")?
            .0;
        face.select_size(best as i32).map_err(|_| "color_render")?;
    }
    let actual = face.size_metrics().ok_or("color_render")?.y_ppem.max(1) as f64;
    let scale = size as f64 / actual;
    let parsed = Face::parse(bytes, index).map_err(|_| "color_render")?;
    let mut canvas = image::RgbaImage::new(width, height);
    let mut x = 16i64;
    let mut baseline = size as i64 + 24;
    let mut missing = 0;
    for c in text.chars() {
        if c == '\n' {
            x = 16;
            baseline += size as i64 * 2;
            continue;
        }
        if c.is_control() {
            return Err("color_settings".into());
        }
        let id = face.get_char_index(c as usize).unwrap_or(0);
        if id == 0 {
            missing += 1;
            let advance = i64::from(size) / 2;
            if x + advance > i64::from(width) - 16 {
                x = 16;
                baseline += i64::from(size) * 2;
            }
            if baseline + i64::from(size) / 4 > i64::from(height) {
                return Err("color_render_limit".into());
            }
            let top = baseline - i64::from(size) * 3 / 4;
            for py in top..baseline {
                for px in x..x + advance {
                    if (px == x || px == x + advance - 1 || py == top || py == baseline - 1)
                        && px >= 0
                        && py >= 0
                        && px < i64::from(width)
                        && py < i64::from(height)
                    {
                        canvas.put_pixel(px as u32, py as u32, image::Rgba([70, 70, 70, 255]));
                    }
                }
            }
            x += advance;
            continue;
        }
        // Decode embedded PNGs with the existing bounded Rust image decoder.
        // The bundled outline engine intentionally has no external libpng dependency.
        if let Some(raster) = parsed.glyph_raster_image(ttf_parser::GlyphId(id as u16), size as u16)
        {
            if raster.format == ttf_parser::RasterImageFormat::PNG {
                let asset =
                    ImageAsset::from_bytes(raster.data.to_vec()).map_err(|_| "color_render")?;
                if asset.width() > 2048 || asset.height() > 2048 || raster.pixels_per_em == 0 {
                    return Err("color_render_limit".into());
                }
                let factor = size as f64 / f64::from(raster.pixels_per_em);
                let advance = (f64::from(
                    parsed
                        .glyph_hor_advance(ttf_parser::GlyphId(id as u16))
                        .unwrap_or(parsed.units_per_em()),
                ) * size as f64
                    / f64::from(parsed.units_per_em()))
                .round() as i64;
                if x + advance > width as i64 - 16 {
                    x = 16;
                    baseline += size as i64 * 2;
                }
                if baseline + size as i64 / 4 > height as i64 {
                    return Err("color_render_limit".into());
                }
                let nw = (f64::from(asset.width()) * factor).round().max(1.) as u32;
                let nh = (f64::from(asset.height()) * factor).round().max(1.) as u32;
                if nw > 2048 || nh > 2048 {
                    return Err("color_render_limit".into());
                }
                let rgba = image::RgbaImage::from_raw(
                    asset.width(),
                    asset.height(),
                    asset.rgba().to_vec(),
                )
                .ok_or("color_render")?;
                let rgba =
                    image::imageops::resize(&rgba, nw, nh, image::imageops::FilterType::Lanczos3);
                image::imageops::overlay(
                    &mut canvas,
                    &rgba,
                    x + (f64::from(raster.x) * factor).round() as i64,
                    baseline
                        - ((f64::from(raster.y) + f64::from(asset.height())) * factor).round()
                            as i64,
                );
                x += advance.max(1);
                continue;
            }
        }
        face.load_glyph(id, LoadFlag::COLOR | LoadFlag::RENDER)
            .map_err(|_| "color_render")?;
        let slot = face.glyph();
        let bitmap = slot.bitmap();
        let w = bitmap.width();
        let h = bitmap.rows();
        if w < 0 || h < 0 || w > 2048 || h > 2048 {
            return Err("color_render_limit".into());
        }
        let advance = ((slot.advance().x as f64 / 64.) * scale).round() as i64;
        if x + advance > width as i64 - 16 {
            x = 16;
            baseline += size as i64 * 2;
        }
        if baseline + size as i64 / 4 > height as i64 {
            return Err("color_render_limit".into());
        }
        if w > 0 && h > 0 {
            let mode = bitmap.pixel_mode().map_err(|_| "color_render")?;
            let channels = match mode {
                PixelMode::Bgra => 4,
                PixelMode::Gray => 1,
                _ => return Err("color_render".into()),
            };
            let pitch = bitmap.pitch().unsigned_abs() as usize;
            if pitch < (w as usize) * channels || bitmap.pitch() < 0 {
                return Err("color_render".into());
            }
            let buffer = bitmap.buffer();
            let mut rgba = image::RgbaImage::new(w as u32, h as u32);
            for row in 0..h as usize {
                for col in 0..w as usize {
                    let at = row * pitch + col * channels;
                    let p = buffer.get(at..at + channels).ok_or("color_render")?;
                    let pixel = if channels == 4 {
                        let a = p[3] as u32;
                        let un = |v: u8| {
                            (v as u32 * 255 + a / 2)
                                .checked_div(a)
                                .unwrap_or(0)
                                .min(255) as u8
                        };
                        [un(p[2]), un(p[1]), un(p[0]), p[3]]
                    } else {
                        [0, 0, 0, p[0]]
                    };
                    rgba.put_pixel(col as u32, row as u32, image::Rgba(pixel));
                }
            }
            let nw = (w as f64 * scale).round().max(1.) as u32;
            let nh = (h as f64 * scale).round().max(1.) as u32;
            if nw > 2048 || nh > 2048 {
                return Err("color_render_limit".into());
            }
            let rgba =
                image::imageops::resize(&rgba, nw, nh, image::imageops::FilterType::Lanczos3);
            image::imageops::overlay(
                &mut canvas,
                &rgba,
                x + (slot.bitmap_left() as f64 * scale).round() as i64,
                baseline - (slot.bitmap_top() as f64 * scale).round() as i64,
            );
        }
        x += advance.max(1);
    }
    let mut encoded = std::io::Cursor::new(Vec::new());
    canvas
        .write_to(&mut encoded, image::ImageFormat::Png)
        .map_err(|_| "color_render")?;
    Ok(Preview {
        image: ImageAsset::from_bytes(encoded.into_inner()).map_err(|_| "color_render")?,
        label: format!(
            "{} · {}{}",
            metadata.family,
            metadata.formats.join(" / "),
            if metadata.palettes > 0 {
                format!(" · CPAL {}", palette + 1)
            } else {
                String::new()
            }
        ),
        missing,
    })
}
