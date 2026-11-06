//! Axis restriction and complete instantiation through the bundled HarfBuzz subsetter.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct AxisRange {
    pub tag: u32,
    pub minimum: f32,
    pub maximum: f32,
    pub default: f32,
}
extern "C" {
    fn fonts_variation_subset(
        data: *const u8,
        length: u32,
        index: u32,
        ranges: *const AxisRange,
        count: u32,
        out: *mut *mut u8,
        out_length: *mut u32,
    ) -> i32;
    fn fonts_hinting_free(pointer: *mut u8);
}
pub fn subset(bytes: &[u8], index: u32, ranges: &[AxisRange]) -> Result<Vec<u8>, String> {
    if bytes.is_empty()
        || bytes.len() > super::LIMIT
        || ranges.len() > 64
        || ranges.iter().any(|r| {
            !r.minimum.is_finite()
                || !r.maximum.is_finite()
                || !r.default.is_finite()
                || r.minimum > r.default
                || r.default > r.maximum
        })
    {
        return Err("variation_range".into());
    }
    let mut pointer = std::ptr::null_mut();
    let mut length = 0;
    // SAFETY: All borrowed slices live through this synchronous bounded call; output is malloc-owned.
    let status = unsafe {
        fonts_variation_subset(
            bytes.as_ptr(),
            bytes.len() as u32,
            index,
            ranges.as_ptr(),
            ranges.len() as u32,
            &mut pointer,
            &mut length,
        )
    };
    if status != 0 || pointer.is_null() || length == 0 || length as usize > super::LIMIT {
        if !pointer.is_null() {
            unsafe { fonts_hinting_free(pointer) }
        }
        return Err(if status == 2 {
            "variation_range"
        } else {
            "variation_unsupported"
        }
        .into());
    }
    // SAFETY: The wrapper returns the validated length of its owned allocation, copied before free.
    let output = unsafe { std::slice::from_raw_parts(pointer, length as usize) }.to_vec();
    unsafe { fonts_hinting_free(pointer) };
    Ok(output)
}
