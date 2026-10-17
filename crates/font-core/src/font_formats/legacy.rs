use crate::{font_io::Result, CidSystem, Contour, FontDocument, Glyph, GlyphStatus, Segment};
use freetype::{face::LoadFlag, ffi};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::{c_char, c_void, CStr},
};
#[repr(C)]
struct PsInfo {
    version: *const c_char,
    notice: *const c_char,
    full_name: *const c_char,
    family_name: *const c_char,
    weight: *const c_char,
    italic_angle: std::ffi::c_long,
    is_fixed_pitch: u8,
    underline_position: i16,
    underline_thickness: u16,
}
unsafe extern "C" {
    fn FT_Get_PS_Font_Info(face: ffi::FT_Face, info: *mut PsInfo) -> ffi::FT_Error;
    fn FT_Get_CID_Registry_Ordering_Supplement(
        face: ffi::FT_Face,
        registry: *mut *const c_char,
        ordering: *mut *const c_char,
        supplement: *mut ffi::FT_Int,
    ) -> ffi::FT_Error;
    fn FT_Get_CID_From_Glyph_Index(
        face: ffi::FT_Face,
        glyph: ffi::FT_UInt,
        cid: *mut ffi::FT_UInt,
    ) -> ffi::FT_Error;
}
fn ps_text(pointer: *const c_char, escaped: bool) -> String {
    if pointer.is_null() {
        return String::new();
    }
    // SAFETY: strings come from a live FreeType face and are NUL terminated.
    let data = unsafe { CStr::from_ptr(pointer).to_bytes() };
    let mut bytes = vec![];
    let mut i = 0;
    while i < data.len() {
        if escaped && data[i] == b'\\' {
            i += 1;
            if i == data.len() {
                break;
            }
            if (b'0'..=b'7').contains(&data[i]) {
                let mut n = 0u16;
                let mut count = 0;
                while i < data.len() && count < 3 && (b'0'..=b'7').contains(&data[i]) {
                    n = n * 8 + u16::from(data[i] - b'0');
                    i += 1;
                    count += 1;
                }
                bytes.push(n as u8);
                continue;
            }
            match data[i] {
                b'n' => bytes.push(b'\n'),
                b'r' => bytes.push(b'\r'),
                b't' => bytes.push(b'\t'),
                b'b' => bytes.push(8),
                b'f' => bytes.push(12),
                b'\n' => {}
                b'\r' => {
                    if data.get(i + 1) == Some(&b'\n') {
                        i += 1;
                    }
                }
                b => bytes.push(b),
            }
        } else {
            bytes.push(data[i]);
        }
        i += 1;
    }
    String::from_utf8(bytes.clone()).unwrap_or_else(|_| bytes.into_iter().map(char::from).collect())
}
fn raw_cff(bytes: &[u8]) -> Option<ttf_parser::cff::Table<'_>> {
    if bytes.starts_with(&[1, 0]) {
        ttf_parser::cff::Table::parse(bytes)
    } else {
        None
    }
}
fn system(face: &freetype::Face) -> Option<CidSystem> {
    let (mut r, mut o, mut s) = (std::ptr::null(), std::ptr::null(), 0);
    // SAFETY: the face is alive; returned C strings are borrowed until it is dropped.
    unsafe {
        if FT_Get_CID_Registry_Ordering_Supplement(
            face.raw() as *const _ as _,
            &mut r,
            &mut o,
            &mut s,
        ) == 0
            && !r.is_null()
            && !o.is_null()
        {
            Some(CidSystem {
                registry: CStr::from_ptr(r).to_string_lossy().into(),
                ordering: CStr::from_ptr(o).to_string_lossy().into(),
                supplement: s,
            })
        } else {
            None
        }
    }
}
pub fn cid_system(bytes: &[u8], index: u32) -> Option<CidSystem> {
    let library = freetype::Library::init().ok()?;
    let face = library
        .new_memory_face(bytes.to_vec(), index as isize)
        .ok()?;
    system(&face)
}
pub struct LegacyInfo {
    pub family: String,
    pub style: String,
    pub glyphs: u16,
    pub outline: bool,
    pub cid: bool,
}
pub fn info(bytes: &[u8], index: u32) -> Result<LegacyInfo> {
    let library = freetype::Library::init().map_err(|_| "font_engine_unavailable")?;
    let face = library
        .new_memory_face(bytes.to_vec(), index as isize)
        .map_err(|_| "font_invalid")?;
    Ok(LegacyInfo {
        family: face.family_name().unwrap_or_else(|| "Imported".into()),
        style: face.style_name().unwrap_or_else(|| "Regular".into()),
        glyphs: raw_cff(bytes)
            .map(|c| Ok(c.number_of_glyphs()))
            .unwrap_or_else(|| u16::try_from(face.num_glyphs()).map_err(|_| "font_too_large"))?,
        outline: face.is_scalable(),
        cid: face.is_cid_keyed(),
    })
}
#[derive(Default)]
struct Outline {
    contours: Vec<Contour>,
    points: usize,
}
fn point(p: *const ffi::FT_Vector) -> [f64; 2] {
    // SAFETY: FreeType only supplies valid callback pointers during decomposition.
    unsafe { [(*p).x as f64, (*p).y as f64] }
}
extern "C" fn moveto(p: *const ffi::FT_Vector, user: *mut c_void) -> i32 {
    // SAFETY: callback user points to this call's live Outline; no callback unwinds.
    let b = unsafe { &mut *(user as *mut Outline) };
    if b.contours.len() >= 4096 {
        return 1;
    }
    b.contours.push(Contour {
        smooth: Default::default(),
        start: point(p),
        segments: vec![],
        closed: true,
    });
    0
}
fn segment(user: *mut c_void, s: Segment) -> i32 {
    let b = unsafe { &mut *(user as *mut Outline) };
    b.points += 1;
    if b.points > 65536 {
        return 1;
    }
    if let Some(c) = b.contours.last_mut() {
        c.segments.push(s);
        0
    } else {
        1
    }
}
extern "C" fn lineto(p: *const ffi::FT_Vector, u: *mut c_void) -> i32 {
    segment(u, Segment::Line { end: point(p) })
}
extern "C" fn conicto(c: *const ffi::FT_Vector, p: *const ffi::FT_Vector, u: *mut c_void) -> i32 {
    segment(
        u,
        Segment::Quadratic {
            control: point(c),
            end: point(p),
        },
    )
}
extern "C" fn cubicto(
    c1: *const ffi::FT_Vector,
    c2: *const ffi::FT_Vector,
    p: *const ffi::FT_Vector,
    u: *mut c_void,
) -> i32 {
    segment(
        u,
        Segment::Cubic {
            control1: point(c1),
            control2: point(c2),
            end: point(p),
        },
    )
}
pub fn legacy_import(bytes: &[u8], index: u32) -> Result<FontDocument> {
    if bytes.len() as u64 > crate::font_io::MAX_FONT_BYTES {
        return Err("font_too_large".into());
    }
    let library = freetype::Library::init().map_err(|_| "font_engine_unavailable")?;
    let face = library
        .new_memory_face(bytes.to_vec(), index as isize)
        .map_err(|_| "font_invalid")?;
    if !face.is_scalable() {
        return Err("font_no_outlines".into());
    }
    let cff = raw_cff(bytes);
    let count = cff
        .map(|c| u32::from(c.number_of_glyphs()))
        .unwrap_or(u32::try_from(face.num_glyphs()).map_err(|_| "font_too_large")?);
    if count == 0 || count > 65535 {
        return Err("font_too_large".into());
    }
    let mut doc = FontDocument::new(
        format!(
            "{} Edited",
            face.family_name().unwrap_or_else(|| "Imported".into())
        ),
        "",
    )
    .map_err(|e| e.to_string())?;
    doc.units_per_em = face.em_size() as u16;
    doc.ascender = f64::from(face.ascender().max(1));
    doc.descender = f64::from(face.descender().min(0));
    doc.metadata.style = face.style_name().unwrap_or_else(|| "Regular".into());
    doc.metadata.cid_system = system(&face);
    doc.metadata.bold = face.raw().style_flags & ffi::FT_STYLE_FLAG_BOLD != 0;
    doc.metadata.italic = face.raw().style_flags & ffi::FT_STYLE_FLAG_ITALIC != 0;
    doc.metadata.weight = if doc.metadata.bold { 700 } else { 400 };
    let mut info = std::mem::MaybeUninit::<PsInfo>::zeroed();
    // SAFETY: FreeType fills the public PS_FontInfoRec layout; its strings live with the face.
    unsafe {
        if FT_Get_PS_Font_Info(face.raw() as *const _ as _, info.as_mut_ptr()) == 0 {
            let info = info.assume_init();
            doc.metadata.italic_angle = info.italic_angle as f64 / 65536.;
            if !info.notice.is_null() {
                doc.metadata.copyright = ps_text(info.notice, cff.is_none());
            }
            if !info.family_name.is_null() {
                doc.family = format!("{} Edited", ps_text(info.family_name, cff.is_none()));
            }
        }
    }
    let mut mapping: BTreeMap<u32, BTreeSet<char>> = BTreeMap::new();
    // A Unicode charmap may be synthesized by FreeType from Type1 glyph names.
    // Never reinterpret a custom 8-bit encoding or CID as Unicode.
    let unicode =
        unsafe { ffi::FT_Select_Charmap(face.raw() as *const _ as _, ffi::FT_ENCODING_UNICODE) }
            == 0;
    if unicode {
        for (code, id) in face.chars() {
            if let Some(c) = u32::try_from(code).ok().and_then(char::from_u32) {
                mapping.entry(id.get()).or_default().insert(c);
            }
        }
    }
    let mut total = 0;
    let mut names = BTreeSet::new();
    for id in 0..count {
        let load_id = cff
            .and_then(|c| c.glyph_cid(ttf_parser::GlyphId(id as u16)))
            .map(u32::from)
            .unwrap_or(id);
        face.load_glyph(
            load_id,
            LoadFlag::NO_SCALE | LoadFlag::NO_HINTING | LoadFlag::NO_BITMAP,
        )
        .map_err(|_| "font_invalid_outline")?;
        let mut b = Outline::default();
        if face.glyph().outline().is_some() && face.glyph().raw().outline.n_contours > 0 {
            let o = &face.glyph().raw().outline;
            let mut raw = ffi::FT_Outline {
                n_contours: o.n_contours,
                n_points: o.n_points,
                points: o.points,
                tags: o.tags,
                contours: o.contours,
                flags: o.flags,
            };
            let funcs = ffi::FT_Outline_Funcs {
                move_to: moveto,
                line_to: lineto,
                conic_to: conicto,
                cubic_to: cubicto,
                shift: 0,
                delta: 0,
            };
            let result =
                unsafe { ffi::FT_Outline_Decompose(&mut raw, &funcs, &mut b as *mut _ as _) };
            if result != 0 {
                return Err("font_invalid_outline".into());
            }
        }
        total += b.points;
        if total > crate::font_io::MAX_OUTLINE_SEGMENTS {
            return Err("font_too_large".into());
        }
        let mut name_buf = [0u8; 256];
        let raw_name = if unsafe {
            ffi::FT_Get_Glyph_Name(
                face.raw() as *const _ as _,
                id,
                name_buf.as_mut_ptr() as _,
                256,
            )
        } == 0
        {
            let end = name_buf.iter().position(|b| *b == 0).unwrap_or(256);
            String::from_utf8_lossy(&name_buf[..end]).to_string()
        } else {
            String::new()
        };
        let mut cid = 0;
        let has_cid =
            unsafe { FT_Get_CID_From_Glyph_Index(face.raw() as *const _ as _, id, &mut cid) } == 0;
        let base = if id == 0 {
            ".notdef".into()
        } else if !raw_name.is_empty() {
            raw_name
        } else if has_cid {
            format!("cid{cid:05}")
        } else {
            format!("glyph{id}")
        };
        let mut name = base.clone();
        let mut suffix = 0;
        while !names.insert(name.clone()) {
            suffix += 1;
            name = format!("{base}.{suffix}");
        }
        doc.glyphs.insert(
            id,
            Glyph {
                id,
                name,
                codepoints: if id == 0 {
                    BTreeSet::new()
                } else {
                    mapping.remove(&id).unwrap_or_default()
                },
                cid: has_cid.then_some(cid),
                advance: face.glyph().metrics().horiAdvance as f64,
                contours: b.contours,
                status: GlyphStatus::Draft,
            },
        );
    }
    doc.validate().map_err(|e| e.to_string())?;
    Ok(doc)
}
