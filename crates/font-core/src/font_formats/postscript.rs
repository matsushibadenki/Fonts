use crate::{font_io::Result, FontDocument, Glyph, Segment};
use std::collections::BTreeSet;
pub(super) fn ps_name(input: &str) -> String {
    let name: String = input
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
        .take(60)
        .collect();
    if name.is_empty() {
        "FontsExport".into()
    } else {
        name
    }
}
pub(super) fn ps_string(input: &str) -> String {
    input
        .as_bytes()
        .iter()
        .map(|b| match *b {
            b'(' => "\\(".into(),
            b')' => "\\)".into(),
            b'\\' => "\\\\".into(),
            32..=126 => (*b as char).to_string(),
            _ => format!("\\{b:03o}"),
        })
        .collect()
}
pub(super) fn glyph_bounds(g: &Glyph) -> [f64; 4] {
    let mut b = [0f64; 4];
    for c in &g.contours {
        for p in std::iter::once(c.start).chain(c.segments.iter().flat_map(|s| match *s {
            Segment::Line { end } => vec![end],
            Segment::Quadratic { control, end } => vec![control, end],
            Segment::Cubic {
                control1,
                control2,
                end,
            } => vec![control1, control2, end],
        })) {
            b[0] = b[0].min(p[0]);
            b[1] = b[1].min(p[1]);
            b[2] = b[2].max(p[0]);
            b[3] = b[3].max(p[1]);
        }
    }
    b
}
pub(super) fn bounds(doc: &FontDocument) -> [f64; 4] {
    doc.glyphs
        .values()
        .map(glyph_bounds)
        .fold([0f64; 4], |a, b| {
            [
                a[0].min(b[0]),
                a[1].min(b[1]),
                a[2].max(b[2]),
                a[3].max(b[3]),
            ]
        })
}

pub(super) fn curves(g: &Glyph, mut emit: impl FnMut(u8, &[f64]) -> Result<()>) -> Result<()> {
    let mut at = [0., 0.];
    for c in &g.contours {
        if !c.closed {
            return Err("font_open_contour".into());
        }
        if c.segments.is_empty() {
            return Err("font_invalid_outline".into());
        }
        emit(21, &[c.start[0] - at[0], c.start[1] - at[1]])?;
        at = c.start;
        for s in &c.segments {
            match *s {
                Segment::Line { end } => {
                    emit(5, &[end[0] - at[0], end[1] - at[1]])?;
                    at = end;
                }
                Segment::Quadratic { control, end } => {
                    let a =
                        std::array::from_fn::<_, 2, _>(|i| at[i] + (control[i] - at[i]) * 2. / 3.);
                    let b = std::array::from_fn::<_, 2, _>(|i| {
                        end[i] + (control[i] - end[i]) * 2. / 3.
                    });
                    emit(
                        8,
                        &[
                            a[0] - at[0],
                            a[1] - at[1],
                            b[0] - a[0],
                            b[1] - a[1],
                            end[0] - b[0],
                            end[1] - b[1],
                        ],
                    )?;
                    at = end;
                }
                Segment::Cubic {
                    control1: a,
                    control2: b,
                    end,
                } => {
                    emit(
                        8,
                        &[
                            a[0] - at[0],
                            a[1] - at[1],
                            b[0] - a[0],
                            b[1] - a[1],
                            end[0] - b[0],
                            end[1] - b[1],
                        ],
                    )?;
                    at = end;
                }
            }
        }
        emit(9, &[])?;
    }
    Ok(())
}
fn integer(output: &mut Vec<u8>, n: i32) {
    match n {
        -107..=107 => output.push((n + 139) as u8),
        108..=1131 => {
            let m = n - 108;
            output.extend([(247 + m / 256) as u8, (m % 256) as u8]);
        }
        -1131..=-108 => {
            let m = -n - 108;
            output.extend([(251 + m / 256) as u8, (m % 256) as u8]);
        }
        _ => {
            output.push(255);
            output.extend(n.to_be_bytes());
        }
    }
}
fn number(output: &mut Vec<u8>, n: f64) -> Result<()> {
    if !n.is_finite() || n.abs() > 1_000_000. {
        return Err("font_coordinate_range".into());
    }
    let scaled = (n * 64.).round();
    if (n - n.round()).abs() < 1e-9 {
        integer(output, n.round() as i32);
    } else {
        integer(output, scaled as i32);
        integer(output, 64);
        output.extend([12, 12]);
    }
    Ok(())
}
pub(super) fn charstring(g: &Glyph) -> Result<Vec<u8>> {
    let mut out = vec![];
    number(&mut out, 0.)?;
    number(&mut out, g.advance)?;
    out.push(13);
    curves(g, |op, args| {
        for n in args {
            number(&mut out, *n)?;
        }
        out.push(op);
        Ok(())
    })?;
    out.push(14);
    Ok(out)
}
fn encrypt(data: &[u8], seed: u16) -> Vec<u8> {
    let mut r = seed;
    data.iter()
        .map(|p| {
            let c = *p ^ (r >> 8) as u8;
            r = (u16::from(c).wrapping_add(r))
                .wrapping_mul(52845)
                .wrapping_add(22719);
            c
        })
        .collect()
}
fn font_info(doc: &FontDocument) -> String {
    format!("/FontInfo 8 dict dup begin\n/FamilyName ({}) def\n/FullName ({} {}) def\n/Weight ({}) def\n/Notice ({}) def\n/ItalicAngle {} def\n/isFixedPitch false def\n/UnderlinePosition -100 def\n/UnderlineThickness 50 def\nend readonly def\n",ps_string(&doc.family),ps_string(&doc.family),ps_string(style(doc)),ps_string(style(doc)),ps_string(&doc.metadata.copyright),doc.metadata.italic_angle)
}
pub(super) fn style(doc: &FontDocument) -> &str {
    if doc.metadata.style.is_empty() {
        "Regular"
    } else {
        &doc.metadata.style
    }
}
pub(super) fn names(doc: &FontDocument) -> Vec<String> {
    let mut used = BTreeSet::new();
    doc.glyphs
        .values()
        .map(|g| {
            let base = if g.id == 0 {
                ".notdef".into()
            } else if let Some(c) = g.codepoints.first() {
                if *c as u32 <= 0xFFFF {
                    format!("uni{:04X}", *c as u32)
                } else {
                    format!("u{:05X}", *c as u32)
                }
            } else {
                // A Unicode-looking old name must not recreate a cleared mapping
                // when a Type1/CFF reader infers Unicode from glyph names.
                format!("glyph{}", g.id)
            };
            let mut name = base.clone();
            let mut suffix = 0;
            while !used.insert(name.clone()) {
                suffix += 1;
                name = format!("{base}.{suffix}");
            }
            name
        })
        .collect()
}
pub(super) fn expand_aliases(doc: &FontDocument) -> Result<FontDocument> {
    let mut doc = doc.clone();
    let mut aliases = vec![];
    let mut next = doc.glyphs.keys().next_back().copied().unwrap_or(0);
    for g in doc.glyphs.values_mut() {
        let codes: Vec<_> = g.codepoints.iter().copied().collect();
        for c in codes.iter().skip(1) {
            next = next.checked_add(1).ok_or("font_too_large")?;
            let mut alias = g.clone();
            alias.id = next;
            alias.codepoints = [*c].into();
            alias.cid = None;
            aliases.push(alias);
        }
        if let Some(c) = codes.first() {
            g.codepoints = [*c].into();
        }
    }
    for g in aliases {
        doc.glyphs.insert(g.id, g);
    }
    if doc.glyphs.len() > 65535 {
        return Err("font_too_large".into());
    }
    Ok(doc)
}
pub fn type1(doc: &FontDocument, pfb: bool) -> Result<Vec<u8>> {
    let doc = expand_aliases(doc)?;
    let name = ps_name(&doc.family);
    let b = bounds(&doc);
    let matrix = 1. / f64::from(doc.units_per_em);
    let names = names(&doc);
    let mut clear=format!("%!PS-AdobeFont-1.0: {name} 1.0\n11 dict begin\n/FontName /{name} def\n/FontType 1 def\n/PaintType 0 def\n/FontMatrix [{matrix:.12} 0 0 {matrix:.12} 0 0] readonly def\n/FontBBox [{} {} {} {}] readonly def\n{}",b[0].floor(),b[1].floor(),b[2].ceil(),b[3].ceil(),font_info(&doc));
    clear.push_str("/Encoding 256 array\n0 1 255 {1 index exch /.notdef put} for\n");
    for (g, n) in doc.glyphs.values().zip(&names) {
        for c in &g.codepoints {
            if (*c as u32) < 256 {
                clear.push_str(&format!("dup {} /{n} put\n", *c as u32));
            }
        }
    }
    clear.push_str("readonly def\ncurrentdict end\ncurrentfile eexec\n");
    let mut private=b"\0\0\0\0dup /Private 8 dict dup begin\n/RD {string currentfile exch readstring pop} executeonly def\n/ND {noaccess def} executeonly def\n/NP {noaccess put} executeonly def\n/lenIV 4 def\n/password 5839 def\n/MinFeature {16 16} def\n/BlueValues [] def\n/Subrs 0 array def\n2 index /CharStrings ".to_vec();
    private.extend(format!("{} dict dup begin\n", doc.glyphs.len()).as_bytes());
    for (g, name) in doc.glyphs.values().zip(&names) {
        let mut data = vec![0; 4];
        data.extend(charstring(g)?);
        let data = encrypt(&data, 4330);
        private.extend(format!("/{name} {} RD ", data.len()).as_bytes());
        private.extend(data);
        private.extend(b" ND\n");
    }
    private.extend(b"end end readonly put noaccess put\ndup /FontName get exch definefont pop\nmark currentfile closefile\n");
    let encrypted = encrypt(&private, 55665);
    let trailer = format!("{}\ncleartomark\n", "0".repeat(512)).into_bytes();
    if pfb {
        let mut output = vec![];
        for (kind, data) in [(1, clear.into_bytes()), (2, encrypted), (1, trailer)] {
            output.extend([0x80, kind]);
            output.extend((data.len() as u32).to_le_bytes());
            output.extend(data);
        }
        output.extend([0x80, 3]);
        Ok(output)
    } else {
        let mut output = clear.into_bytes();
        for chunk in encrypted.chunks(32) {
            for b in chunk {
                output.extend(format!("{b:02X}").as_bytes());
            }
            output.push(b'\n');
        }
        output.extend(trailer);
        Ok(output)
    }
}
pub(super) fn cids(doc: &FontDocument) -> Result<Vec<u16>> {
    let mut used: BTreeSet<u32> = doc.glyphs.values().filter_map(|g| g.cid).collect();
    let mut next = 1u32;
    let mut result = vec![];
    for g in doc.glyphs.values() {
        let cid = if g.id == 0 {
            if g.cid.is_some_and(|n| n != 0) {
                return Err("font_cid_invalid".into());
            }
            0
        } else if let Some(cid) = g.cid {
            if cid == 0 {
                return Err("font_cid_invalid".into());
            }
            cid
        } else {
            while used.contains(&next) {
                next += 1;
            }
            let cid = next;
            used.insert(cid);
            next += 1;
            cid
        };
        result.push(u16::try_from(cid).map_err(|_| "font_cid_invalid")?);
    }
    Ok(result)
}
pub fn cid(doc: &FontDocument) -> Result<Vec<u8>> {
    let ids = cids(doc)?;
    let count = u32::from(*ids.iter().max().unwrap_or(&0)) + 1;
    if count > 65535 {
        return Err("font_cid_invalid".into());
    }
    let mut strings = std::collections::BTreeMap::new();
    for (g, id) in doc.glyphs.values().zip(ids) {
        strings.insert(id, charstring(g)?);
    }
    let mut offset = (count as usize + 1) * 5;
    let mut map = vec![];
    let mut data: Vec<u8> = vec![];
    for id in 0..count {
        map.push(0);
        map.extend((offset as u32).to_be_bytes());
        let empty = [139, 139, 13, 14];
        let cs = strings
            .get(&(id as u16))
            .map(Vec::as_slice)
            .unwrap_or(&empty);
        offset += cs.len();
        data.extend_from_slice(cs);
    }
    map.push(0);
    map.extend((offset as u32).to_be_bytes());
    map.extend(data);
    let system = doc.metadata.cid_system.clone().unwrap_or(crate::CidSystem {
        registry: "Adobe".into(),
        ordering: "Identity".into(),
        supplement: 0,
    });
    let b = bounds(doc);
    let name = ps_name(&doc.family);
    let matrix = 1. / f64::from(doc.units_per_em);
    let header=format!("%!PS-Adobe-3.0 Resource-CIDFont\n%%DocumentNeededResources: ProcSet (CIDInit)\n%%IncludeResource: ProcSet (CIDInit)\n%%BeginResource: CIDFont ({name})\n/CIDInit /ProcSet findresource begin\n20 dict begin\n/CIDFontName /{name} def\n/CIDFontVersion 1 def\n/CIDFontType 0 def\n/CIDSystemInfo << /Registry ({}) /Ordering ({}) /Supplement {} >> def\n/FontBBox [{} {} {} {}] def\n{}/CIDMapOffset 0 def\n/FDBytes 1 def\n/GDBytes 4 def\n/CIDCount {count} def\n/FDArray 1 array dup 0\n%ADOBeginFontDict\n10 dict dup begin\n/FontName /{name} def\n/FontType 1 def\n/PaintType 0 def\n/FontMatrix [{matrix:.12} 0 0 {matrix:.12} 0 0] def\n/Private 8 dict dup begin\n/lenIV -1 def\n/password 5839 def\n/MinFeature [16 16] def\n/BlueValues [] def\n/SubrMapOffset 0 def\n/SubrCount 0 def\n/SDBytes 0 def\nend def\nend\n%ADOEndFontDict\nput def\n(Binary) {} StartData ",ps_string(&system.registry),ps_string(&system.ordering),system.supplement,b[0].floor(),b[1].floor(),b[2].ceil(),b[3].ceil(),font_info(doc),map.len());
    let mut out = header.into_bytes();
    out.extend(map);
    out.extend(b"\n%%EndData\n%%EndResource\n%%EOF\n");
    Ok(out)
}
