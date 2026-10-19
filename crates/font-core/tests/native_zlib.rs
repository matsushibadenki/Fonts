//! Native gzip compatibility after selecting libz-sys's bundled zlib.
use freetype::{face::LoadFlag, ffi, Library};
use std::ffi::{c_char, c_int, c_long, c_ulong, c_void, CStr};
extern "C" {
    fn malloc(size: usize) -> *mut c_void;
    fn free(pointer: *mut c_void);
    fn realloc(pointer: *mut c_void, size: usize) -> *mut c_void;
    fn FT_Gzip_Uncompress(
        memory: ffi::FT_Memory,
        output: *mut u8,
        output_len: *mut c_ulong,
        input: *const u8,
        input_len: c_ulong,
    ) -> c_int;
    fn zlibVersion() -> *const c_char;
}
extern "C" fn allocate(_: ffi::FT_Memory, size: c_long) -> *mut c_void {
    // SAFETY: FreeType requests a positive allocation size.
    unsafe { malloc(size as usize) }
}
extern "C" fn release(_: ffi::FT_Memory, pointer: *mut c_void) {
    // SAFETY: FreeType returns a pointer allocated by these callbacks.
    unsafe { free(pointer) }
}
extern "C" fn resize(
    _: ffi::FT_Memory,
    _: c_long,
    size: c_long,
    pointer: *mut c_void,
) -> *mut c_void {
    // SAFETY: The pointer and size follow FreeType's allocator contract.
    unsafe { realloc(pointer, size as usize) }
}
#[test]
fn freetype_uses_stable_zlib_and_reads_gzip_font_data() {
    // SAFETY: zlibVersion returns a process-lifetime, NUL-terminated version string.
    let version = unsafe { CStr::from_ptr(zlibVersion()) }.to_str().unwrap();
    assert_eq!(version, "1.3.2");
    let library = Library::init().unwrap();
    // Project-authored minimal 8x8 BDF fixture, gzip-compressed with mtime=0.
    let compressed = include_bytes!("fixtures/dependency-zlib.bdf.gz");
    let mut memory = ffi::FT_MemoryRec {
        user: std::ptr::null_mut(),
        alloc: allocate,
        free: release,
        realloc: resize,
    };
    let mut output = vec![0u8; 4096];
    let mut length = output.len() as c_ulong;
    // SAFETY: Live buffers, valid lengths and C allocator callbacks cover this synchronous call.
    let result = unsafe {
        FT_Gzip_Uncompress(
            &mut memory,
            output.as_mut_ptr(),
            &mut length,
            compressed.as_ptr(),
            compressed.len() as c_ulong,
        )
    };
    assert_eq!(result, 0);
    output.truncate(length as usize);
    let face = library.new_memory_face(output, 0).unwrap();
    assert!(face.get_char_index(65).is_some());
    face.select_size(0).unwrap();
    face.load_char(65, LoadFlag::RENDER).unwrap();
    let bitmap = face.glyph().bitmap();
    assert_eq!((bitmap.width(), bitmap.rows()), (8, 8));
    assert!(bitmap.buffer().iter().any(|byte| *byte != 0));
    let mut length = 4096;
    let mut output = vec![0u8; length as usize];
    // SAFETY: Same valid buffers and allocator; the input is deliberately truncated.
    let result = unsafe {
        FT_Gzip_Uncompress(
            &mut memory,
            output.as_mut_ptr(),
            &mut length,
            compressed.as_ptr(),
            12,
        )
    };
    assert_ne!(result, 0);
}
