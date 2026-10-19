//! Small ownership boundary around bundled HarfBuzz and ttfautohint.
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
