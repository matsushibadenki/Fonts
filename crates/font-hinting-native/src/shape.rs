//! Bounded shaping through the already bundled HarfBuzz; no font buffers cross IPC.
use std::{
    ffi::{c_char, c_int, c_void, CString},
    marker::PhantomData,
    ptr::NonNull,
};
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct Glyph {
    pub glyph: u32,
    pub cluster: u32,
    pub x_advance: i32,
    pub y_advance: i32,
    pub x_offset: i32,
    pub y_offset: i32,
}
extern "C" {
    fn fonts_shape_create(data: *const u8, length: u32) -> *mut c_void;
    fn fonts_shape_destroy(font: *mut c_void);
    fn fonts_unicode_script(c: u32) -> u32;
    fn fonts_unicode_extender(c: u32) -> c_int;
    fn fonts_shape_run(
        font: *mut c_void,
        text: *const c_char,
        text_length: u32,
        language: *const c_char,
        vertical: c_int,
        kerning: c_int,
        rtl: c_int,
        script: u32,
        out: *mut Glyph,
        capacity: u32,
        count: *mut u32,
    ) -> c_int;
}
pub struct Font<'a> {
    raw: NonNull<c_void>,
    borrow: PhantomData<&'a [u8]>,
}
impl<'a> Font<'a> {
    pub fn new(bytes: &'a [u8]) -> Result<Self, String> {
        if bytes.is_empty() || bytes.len() > super::LIMIT {
            return Err("layout_settings".into());
        }
        // SAFETY: The lifetime ties the borrowed bytes to the native HB font. Drop frees it.
        let raw = NonNull::new(unsafe { fonts_shape_create(bytes.as_ptr(), bytes.len() as u32) })
            .ok_or("layout_shaping_failed")?;
        Ok(Self {
            raw,
            borrow: PhantomData,
        })
    }
    pub fn shape(
        &self,
        text: &str,
        language: &str,
        vertical: bool,
        kerning: bool,
    ) -> Result<Vec<Glyph>, String> {
        self.shape_direction(text, language, vertical, kerning, None, 0)
    }
    pub fn shape_direction(
        &self,
        text: &str,
        language: &str,
        vertical: bool,
        kerning: bool,
        rtl: Option<bool>,
        script: u32,
    ) -> Result<Vec<Glyph>, String> {
        let chars = text.chars().count();
        if text.len() > 16384
            || chars > 2048
            || language.len() > 64
            || !language
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
        {
            return Err("layout_settings".into());
        }
        let language = CString::new(language).map_err(|_| "layout_settings")?;
        let mut glyphs = vec![Glyph::default(); (chars * 64).clamp(64, 16384)];
        let mut count = 0;
        // SAFETY: Slices and output remain live for the synchronous call, which checks capacity.
        let result = unsafe {
            fonts_shape_run(
                self.raw.as_ptr(),
                text.as_ptr().cast(),
                text.len() as u32,
                language.as_ptr(),
                vertical.into(),
                kerning.into(),
                rtl.map_or(-1, i32::from),
                script,
                glyphs.as_mut_ptr(),
                glyphs.len() as u32,
                &mut count,
            )
        };
        if result != 0
            || count as usize > glyphs.len()
            || glyphs[..count as usize].iter().any(|g| {
                g.glyph > 65535
                    || !text.is_char_boundary(g.cluster as usize)
                    || [g.x_advance, g.y_advance, g.x_offset, g.y_offset]
                        .iter()
                        .any(|v| v.unsigned_abs() > 1_000_000)
            })
        {
            return Err("layout_shaping_failed".into());
        }
        glyphs.truncate(count as usize);
        Ok(glyphs)
    }
}
impl Drop for Font<'_> {
    fn drop(&mut self) {
        // SAFETY: This object uniquely owns the HB font returned by create.
        unsafe {
            fonts_shape_destroy(self.raw.as_ptr());
        }
    }
}
pub fn extender(c: char) -> bool {
    // SAFETY: Unicode scalar value; this reads immutable HarfBuzz Unicode data.
    unsafe { fonts_unicode_extender(c as u32) != 0 }
}
pub fn shape(
    bytes: &[u8],
    text: &str,
    language: &str,
    vertical: bool,
    kerning: bool,
) -> Result<Vec<Glyph>, String> {
    Font::new(bytes)?.shape(text, language, vertical, kerning)
}

pub fn script(c: char) -> u32 {
    // SAFETY: Unicode scalar; immutable HB Unicode data.
    unsafe { fonts_unicode_script(c as u32) }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shapes_ligatures_variations_vertical_forms_and_rejects_invalid_input() {
        let bytes = include_bytes!("../../font-core/tests/data/layout/shaping.ttf");
        let font = Font::new(bytes).unwrap();
        let liga = font.shape("fi", "en", false, true).unwrap();
        assert_eq!(liga.len(), 1);
        assert_eq!(liga[0].glyph, 6);
        assert_eq!(
            font.shape("字\u{e0100}", "ja", false, true).unwrap()[0].glyph,
            9
        );
        assert_eq!(font.shape("、", "ja", true, true).unwrap()[0].glyph, 11);
        assert!(Font::new(b"bad").is_err());
        assert!(font.shape("A", "日本語", false, true).is_err());
        assert!(font.shape(&"A".repeat(2049), "en", false, true).is_err());
        assert!(extender('\u{301}'));
        assert!(!extender('A'));
    }
}
