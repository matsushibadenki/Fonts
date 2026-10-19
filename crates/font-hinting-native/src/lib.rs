//! Small ownership boundary around bundled HarfBuzz, ttfautohint and Adobe psautohint.
use std::ffi::{c_int, c_uint};
// Ensure the same bundled FreeType dependency is linked, rather than a system copy.
use freetype_sys as _;
extern "C" {
    fn fonts_cff_dehint(
        data: *const u8,
        length: c_uint,
        index: c_uint,
        out: *mut *mut u8,
        out_length: *mut usize,
    ) -> c_int;
    fn fonts_autohint(
        data: *const u8,
        length: usize,
        out: *mut *mut u8,
        out_length: *mut usize,
        minimum: c_int,
        maximum: c_int,
    ) -> c_int;
    fn fonts_hinting_free(pointer: *mut u8);
    fn fonts_ps_autohint(
        bez: *const std::ffi::c_char,
        info: *const std::ffi::c_char,
        out: *mut *mut u8,
        length: *mut usize,
    ) -> c_int;
}
const LIMIT: usize = 256 * 1024 * 1024;
fn process(
    bytes: &[u8],
    operation: impl FnOnce(*mut *mut u8, *mut usize) -> c_int,
) -> Result<Vec<u8>, String> {
    static NATIVE: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _guard = NATIVE.lock().map_err(|_| "state_poisoned")?;
    if bytes.len() > LIMIT {
        return Err("font_too_large".into());
    }
    let mut pointer = std::ptr::null_mut();
    let mut length = 0;
    let status = operation(&mut pointer, &mut length);
    let result = if status != 0 {
        Err(format!("autohint_native_{status}"))
    } else if pointer.is_null() || length == 0 || length > LIMIT {
        Err("hinting_invalid".into())
    } else {
        // SAFETY: On success the bridge owns a malloc buffer of exactly this length.
        Ok(unsafe { std::slice::from_raw_parts(pointer, length) }.to_vec())
    };
    // SAFETY: The bridge allocator accepts null and all returned allocations, including errors.
    unsafe { fonts_hinting_free(pointer) };
    result
}
pub fn dehint_cff(bytes: &[u8], index: u32) -> Result<Vec<u8>, String> {
    process(bytes, |out, length| {
        // SAFETY: Input lives through the call; out pointers are writable and freed above.
        unsafe { fonts_cff_dehint(bytes.as_ptr(), bytes.len() as c_uint, index, out, length) }
    })
}
pub fn autohint(bytes: &[u8], minimum: u8, maximum: u8) -> Result<Vec<u8>, String> {
    if !(8..=50).contains(&minimum) || maximum < minimum || maximum > 96 {
        return Err("autohint_options".into());
    }
    process(bytes, |out, length| {
        // SAFETY: Same owned buffer contract; all variadic arguments are confined to C.
        unsafe {
            fonts_autohint(
                bytes.as_ptr(),
                bytes.len(),
                out,
                length,
                minimum.into(),
                maximum.into(),
            )
        }
    })
}

/// Adobe CFF hinting core. Calls are serialized with the other native engines.
pub fn autohint_bez(bez: &str, info: &str) -> Result<String, String> {
    let source = std::ffi::CString::new(bez).map_err(|_| "cff_hint_invalid")?;
    let info = std::ffi::CString::new(info).map_err(|_| "cff_hint_invalid")?;
    let result = process(bez.as_bytes(), |out, len| {
        // SAFETY: Both NUL-terminated inputs live through the call; output uses the same C allocator contract.
        unsafe { fonts_ps_autohint(source.as_ptr(), info.as_ptr(), out, len) }
    })?;
    String::from_utf8(result).map_err(|_| "cff_hint_invalid".into())
}
#[cfg(test)]
mod tests {
    #[test]
    fn ps_hinting_smoke() {
        let info="FontName FontsGenerated\nOrigEmSqUnits 1000\nLanguageGroup 0\nBaselineYCoord 0\nBaselineOvershoot -10\nCapHeight 700\nCapOvershoot 10\nDominantH [60]\nDominantV [60]\nFlexOK false\nBlueFuzz 1\n";
        let result = super::autohint_bez(
            "% g1\nsc\n0 0 mt\n60 0 dt\n60 700 dt\n0 700 dt\ncp\ned\n",
            info,
        )
        .unwrap();
        assert!(result.contains("rb") || result.contains("ry"));
    }
}
