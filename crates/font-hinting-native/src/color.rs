//! Safe, bounded ownership boundary for the bundled COLRv1 paint rasterizer.
extern "C" {
    fn fonts_color_render(
        data: *const u8,
        length: u32,
        index: u32,
        glyph: u32,
        size: u32,
        palette: u32,
        width: u32,
        height: u32,
        x: i32,
        baseline: i32,
        rgba: *mut u8,
    ) -> std::ffi::c_int;
}

/// Paint one color glyph into a transparent canvas, with a top-left origin.
/// All font data stays borrowed for this call; native objects are local to it.
pub fn render(
    bytes: &[u8],
    index: u32,
    glyph: u16,
    size: u32,
    palette: u16,
    dimensions: [u32; 2],
    origin: [i32; 2],
) -> Result<Vec<u8>, String> {
    let [width, height] = dimensions;
    if bytes.is_empty()
        || bytes.len() > super::LIMIT
        || !(12..=192).contains(&size)
        || !(1..=2048).contains(&width)
        || !(1..=1024).contains(&height)
        || origin.iter().any(|v| !(-4096..=4096).contains(v))
    {
        return Err("color_settings".into());
    }
    let mut rgba = vec![0; width as usize * height as usize * 4];
    // SAFETY: Input and writable RGBA buffer live throughout the synchronous call.
    // Native code checks dimensions, writes exactly width*height*4 bytes, and frees
    // all borrowed-font wrappers before returning. No pointers escape the call.
    let status = unsafe {
        fonts_color_render(
            bytes.as_ptr(),
            bytes.len() as u32,
            index,
            glyph.into(),
            size,
            palette.into(),
            width,
            height,
            origin[0],
            origin[1],
            rgba.as_mut_ptr(),
        )
    };
    if status != 0 {
        return Err("color_render".into());
    }
    Ok(rgba)
}

#[cfg(test)]
mod tests {
    use super::render;
    #[test]
    fn rejects_invalid_fonts_faces_glyphs_and_unbounded_buffers() {
        let font = include_bytes!("../../font-core/tests/data/color/colrv1-paints.ttf");
        assert!(render(b"bad", 0, 1, 100, 0, [256, 256], [16, 124]).is_err());
        assert!(render(font, 99, 1, 100, 0, [256, 256], [16, 124]).is_err());
        assert!(render(font, 0, u16::MAX, 100, 0, [256, 256], [16, 124]).is_err());
        assert!(render(font, 0, 4, 100, 2, [256, 256], [16, 124]).is_err());
        assert!(render(font, 0, 4, 100, 0, [u32::MAX, 256], [16, 124]).is_err());
        assert!(render(font, 0, 4, 100, 0, [256, 256], [i32::MAX, 124]).is_err());
        assert!(render(font, 0, 4, 100, 0, [256, 256], [16, 124]).is_ok());
    }
}
