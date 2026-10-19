//! Convert Adobe's hint decisions to Type 2 while retaining the original path operands.
use super::{cff, postscript};
use crate::{font_io::Result, FontDocument, Glyph};
use std::collections::BTreeMap;
const MAX_OPS: usize = 100_000;
pub(super) struct Prepared {
    pub charstrings: Vec<Vec<u8>>,
    pub private: Vec<u8>,
    pub names: Option<Vec<String>>,
    pub seed: Option<Seed>,
}
#[derive(Clone)]
pub(super) enum SourceData {
    Predefined(i32),
    Bytes(Vec<u8>),
}
pub(crate) struct Seed {
    pub(super) strings: Vec<Vec<u8>>,
    pub(super) charset: SourceData,
    pub(super) encoding: SourceData,
}
impl Seed {
    pub(crate) fn parse(bytes: &[u8], glyphs: usize) -> Result<Self> {
        fn index(bytes: &[u8], at: &mut usize) -> Result<Vec<Vec<u8>>> {
            let count = u16::from_be_bytes(
                bytes
                    .get(*at..*at + 2)
                    .ok_or("cff_hint_invalid")?
                    .try_into()
                    .unwrap(),
            ) as usize;
            *at += 2;
            if count == 0 {
                return Ok(vec![]);
            }
            let size = *bytes.get(*at).ok_or("cff_hint_invalid")? as usize;
            *at += 1;
            if !(1..=4).contains(&size) {
                return Err("cff_hint_invalid".into());
            }
            let mut offsets = Vec::with_capacity(count + 1);
            for _ in 0..=count {
                let mut offset = 0usize;
                for b in bytes.get(*at..*at + size).ok_or("cff_hint_invalid")? {
                    offset = offset * 256 + usize::from(*b);
                }
                *at += size;
                offsets.push(offset);
            }
            if offsets[0] != 1 || offsets.windows(2).any(|p| p[0] > p[1]) {
                return Err("cff_hint_invalid".into());
            }
            let end = at
                .checked_add(offsets[count] - 1)
                .ok_or("cff_hint_invalid")?;
            if end > bytes.len() {
                return Err("cff_hint_invalid".into());
            }
            let result = offsets
                .windows(2)
                .map(|p| bytes[*at + p[0] - 1..*at + p[1] - 1].to_vec())
                .collect();
            *at = end;
            Ok(result)
        }
        if bytes.get(..2) != Some(&[1, 0]) || glyphs == 0 {
            return Err("cff_hint_invalid".into());
        }
        let mut at = *bytes.get(2).ok_or("cff_hint_invalid")? as usize;
        index(bytes, &mut at)?;
        let dicts = index(bytes, &mut at)?;
        let dict = dicts.first().ok_or("cff_hint_invalid")?;
        let strings = index(bytes, &mut at)?;
        let mut charset = 0i32;
        let mut encoding = 0i32;
        let mut numbers = vec![];
        let mut at = 0;
        while let Some(&token) = dict.get(at) {
            at += 1;
            let value = match token {
                28 => {
                    let b = dict.get(at..at + 2).ok_or("cff_hint_invalid")?;
                    at += 2;
                    Some(i16::from_be_bytes(b.try_into().unwrap()) as i32)
                }
                29 => {
                    let b = dict.get(at..at + 4).ok_or("cff_hint_invalid")?;
                    at += 4;
                    Some(i32::from_be_bytes(b.try_into().unwrap()))
                }
                30 => {
                    loop {
                        let b = *dict.get(at).ok_or("cff_hint_invalid")?;
                        at += 1;
                        if b & 15 == 15 || b >> 4 == 15 {
                            break;
                        }
                    }
                    Some(0)
                }
                32..=246 => Some(i32::from(token) - 139),
                247..=250 => {
                    let b = *dict.get(at).ok_or("cff_hint_invalid")?;
                    at += 1;
                    Some((i32::from(token) - 247) * 256 + i32::from(b) + 108)
                }
                251..=254 => {
                    let b = *dict.get(at).ok_or("cff_hint_invalid")?;
                    at += 1;
                    Some(-(i32::from(token) - 251) * 256 - i32::from(b) - 108)
                }
                0..=21 => {
                    if token == 15 || token == 16 {
                        if numbers.len() != 1 {
                            return Err("cff_hint_invalid".into());
                        }
                        if token == 15 {
                            charset = numbers[0];
                        } else {
                            encoding = numbers[0];
                        }
                    }
                    if token == 12 {
                        dict.get(at).ok_or("cff_hint_invalid")?;
                        at += 1;
                    }
                    numbers.clear();
                    None
                }
                _ => return Err("cff_hint_invalid".into()),
            };
            if let Some(n) = value {
                if numbers.len() >= 48 {
                    return Err("cff_hint_invalid".into());
                }
                numbers.push(n);
            }
        }
        if charset < 0 || encoding < 0 {
            return Err("cff_hint_invalid".into());
        }
        let charset = if charset <= 2 {
            SourceData::Predefined(charset)
        } else {
            let start = charset as usize;
            let mut end = start + 1;
            let format = *bytes.get(start).ok_or("cff_hint_invalid")?;
            let mut remaining = glyphs - 1;
            match format {
                0 => {
                    end = end.checked_add(remaining * 2).ok_or("cff_hint_invalid")?;
                }
                1 | 2 => {
                    while remaining > 0 {
                        let size = if format == 1 { 3 } else { 4 };
                        let data = bytes.get(end..end + size).ok_or("cff_hint_invalid")?;
                        let left = if format == 1 {
                            usize::from(data[2])
                        } else {
                            u16::from_be_bytes([data[2], data[3]]) as usize
                        };
                        remaining = remaining.checked_sub(left + 1).ok_or("cff_hint_invalid")?;
                        end += size;
                    }
                }
                _ => return Err("cff_hint_invalid".into()),
            }
            SourceData::Bytes(bytes.get(start..end).ok_or("cff_hint_invalid")?.to_vec())
        };
        let encoding = if encoding <= 1 {
            SourceData::Predefined(encoding)
        } else {
            let start = encoding as usize;
            let mut end = start + 2;
            let format = *bytes.get(start).ok_or("cff_hint_invalid")?;
            let count = usize::from(*bytes.get(start + 1).ok_or("cff_hint_invalid")?);
            end += match format & 0x7f {
                0 => count,
                1 => count * 2,
                _ => return Err("cff_hint_invalid".into()),
            };
            if format & 0x80 != 0 {
                let count = usize::from(*bytes.get(end).ok_or("cff_hint_invalid")?);
                end += 1 + count * 3;
            }
            SourceData::Bytes(bytes.get(start..end).ok_or("cff_hint_invalid")?.to_vec())
        };
        Ok(Self {
            strings,
            charset,
            encoding,
        })
    }
}
#[derive(Clone)]
struct Draw {
    op: u8,
    args: Vec<f64>,
    absolute: Vec<f64>,
}
fn path(glyph: &Glyph) -> Result<Vec<Draw>> {
    let mut at = [0., 0.];
    let mut commands = vec![];
    postscript::curves(glyph, |op, args| {
        let mut absolute = vec![];
        if op != 9 {
            for p in args.chunks(2) {
                at[0] += p[0];
                at[1] += p[1];
                absolute.extend(at);
            }
        }
        if commands.len() >= MAX_OPS {
            return Err("font_too_large".into());
        }
        commands.push(Draw {
            op,
            args: args.to_vec(),
            absolute,
        });
        Ok(())
    })?;
    Ok(commands)
}
#[derive(Clone, Default)]
struct Mask {
    at: usize,
    h: Vec<[f64; 2]>,
    v: Vec<[f64; 2]>,
}
fn sorted(mut hints: Vec<[f64; 2]>) -> Vec<[f64; 2]> {
    hints.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
    hints.dedup();
    hints
}
fn mask_bytes(mask: &Mask, h: &[[f64; 2]], v: &[[f64; 2]]) -> Vec<u8> {
    let mut out = vec![0u8; (h.len() + v.len()).div_ceil(8)];
    for (n, pair) in h.iter().chain(v).enumerate() {
        if if n < h.len() {
            mask.h.contains(pair)
        } else {
            mask.v.contains(pair)
        } {
            out[n / 8] |= 0x80 >> (n % 8);
        }
    }
    out
}
fn stems(out: &mut Vec<u8>, hints: &[[f64; 2]], op: u8) -> Result<()> {
    for chunk in hints.chunks(22) {
        let mut last = 0.;
        for [position, width] in chunk {
            cff::number(out, *position - last)?;
            cff::number(out, *width)?;
            last = position + width;
        }
        out.push(op);
    }
    Ok(())
}
fn compile(bez: &str, draws: &[Draw], advance: f64) -> Result<(Vec<u8>, bool)> {
    let mut masks = vec![Mask::default()];
    let mut numbers = vec![];
    let mut at = 0;
    let mut current = [0., 0.];
    let mut counters: Vec<Mask> = vec![];
    let mut counter = Mask::default();
    let mut previous = "";
    for token in bez
        .lines()
        .flat_map(|line| line.split('%').next().unwrap_or("").split_whitespace())
    {
        if let Ok(n) = token.parse::<f64>() {
            if !n.is_finite() || n.abs() > 1_000_000. || numbers.len() >= 16 {
                return Err("cff_hint_invalid".into());
            }
            numbers.push(n);
            continue;
        }
        match token {
            "rb" | "ry" | "rm" | "rv" => {
                if numbers.len() != 2 {
                    return Err("cff_hint_invalid".into());
                }
                let pair = [numbers[0], numbers[1]];
                if pair[1] == 0. || (pair[1] < 0. && ![-20., -21.].contains(&pair[1])) {
                    return Err("cff_hint_invalid".into());
                }
                let vertical = token == "ry" || token == "rm";
                let mask = masks.last_mut().unwrap();
                if vertical {
                    mask.v.push(pair);
                } else {
                    mask.h.push(pair);
                }
                if token == "rm" || token == "rv" {
                    if token != previous && (!counter.h.is_empty() || !counter.v.is_empty()) {
                        counters.push(std::mem::take(&mut counter));
                    }
                    if vertical {
                        counter.v.push(pair);
                    } else {
                        counter.h.push(pair);
                    }
                    if counter.h.len() + counter.v.len() == 3 {
                        counters.push(std::mem::take(&mut counter));
                    }
                }
            }
            "snc" => {
                if !numbers.is_empty() {
                    return Err("cff_hint_invalid".into());
                }
                masks.push(Mask {
                    at,
                    ..Default::default()
                });
            }
            "mt" | "rmt" | "dt" | "ct" | "cp" => {
                let expected = draws.get(at).ok_or("cff_hint_invalid")?;
                let op = match token {
                    "mt" | "rmt" => 21,
                    "dt" => 5,
                    "ct" => 8,
                    _ => 9,
                };
                if op != expected.op || numbers.len() != expected.absolute.len() {
                    return Err("cff_hint_geometry".into());
                }
                if token == "rmt" {
                    numbers[0] += current[0];
                    numbers[1] += current[1];
                }
                // Adobe uses fixed-point internally. Only its hint decisions are used;
                // all original path operands are copied back below, without quantization.
                if numbers
                    .iter()
                    .zip(&expected.absolute)
                    .any(|(a, b)| (a - b).abs() > 0.02)
                {
                    return Err("cff_hint_geometry".into());
                }
                if numbers.len() >= 2 {
                    current.copy_from_slice(&numbers[numbers.len() - 2..]);
                }
                at += 1;
            }
            "sc" | "ed" | "beginsubr" | "endsubr" | "enc" | "newcolors" => {
                if !numbers.is_empty() {
                    return Err("cff_hint_invalid".into());
                }
            }
            _ => return Err("cff_hint_invalid".into()),
        }
        numbers.clear();
        previous = token;
    }
    if at != draws.len() || !numbers.is_empty() {
        return Err("cff_hint_geometry".into());
    }
    if !counter.h.is_empty() || !counter.v.is_empty() {
        counters.push(counter);
    }
    let h = sorted(masks.iter().flat_map(|m| m.h.clone()).collect());
    let v = sorted(masks.iter().flat_map(|m| m.v.clone()).collect());
    if h.len() + v.len() > 96 {
        return Err("cff_hint_limit".into());
    }
    let mut out = vec![];
    cff::number(&mut out, advance)?;
    let hinted = !h.is_empty() || !v.is_empty();
    stems(&mut out, &h, 18)?;
    stems(&mut out, &v, 23)?;
    for counter in counters {
        if counter.h.len() + counter.v.len() == 3 {
            out.push(20);
            out.extend(mask_bytes(&counter, &h, &v));
        }
    }
    // Emit masks even for a single set, keeping the same explicit Type 2 contract.
    let mut next_mask = 0;
    for (i, draw) in draws.iter().enumerate() {
        if hinted {
            while let Some(mask) = masks.get(next_mask).filter(|m| m.at == i) {
                out.push(19);
                out.extend(mask_bytes(mask, &h, &v));
                next_mask += 1;
            }
        }
        if draw.op != 9 {
            for n in &draw.args {
                cff::number(&mut out, *n)?;
            }
            out.push(draw.op);
        }
    }
    out.push(14);
    Ok((out, hinted))
}
fn dominant(doc: &FontDocument, vertical: bool) -> f64 {
    let mut histogram: BTreeMap<i32, usize> = BTreeMap::new();
    let upm = f64::from(doc.units_per_em);
    for g in doc.glyphs.values().take(256) {
        let mut edges = vec![];
        for c in &g.contours {
            let mut at = c.start;
            for s in &c.segments {
                let end = match s {
                    crate::Segment::Line { end }
                    | crate::Segment::Quadratic { end, .. }
                    | crate::Segment::Cubic { end, .. } => *end,
                };
                if matches!(s, crate::Segment::Line { .. }) {
                    let axis = usize::from(!vertical);
                    let other = 1 - axis;
                    if (at[axis] - end[axis]).abs() < 0.01
                        && (at[other] - end[other]).abs() > upm / 20.
                        && edges.len() < 64
                    {
                        edges.push((
                            at[axis],
                            at[other].min(end[other]),
                            at[other].max(end[other]),
                        ));
                    }
                }
                at = end;
            }
        }
        for (i, a) in edges.iter().enumerate() {
            for b in &edges[i + 1..] {
                let width = (a.0 - b.0).abs();
                if width >= upm / 200.
                    && width <= upm / 4.
                    && a.2.min(b.2) - a.1.max(b.1) > upm / 20.
                {
                    *histogram.entry(width.round() as i32).or_default() += 1;
                }
            }
        }
    }
    histogram
        .into_iter()
        .max_by_key(|(w, n)| (*n, std::cmp::Reverse(*w)))
        .map(|(w, _)| f64::from(w))
        .unwrap_or((upm / 12.).round())
        .max(1.)
}
pub(crate) fn generate(
    doc: &FontDocument,
    cid: bool,
    names: Option<Vec<String>>,
    seed: Option<Seed>,
) -> Result<Vec<u8>> {
    doc.validate().map_err(|e| e.to_string())?;
    let upm = f64::from(doc.units_per_em);
    let h = dominant(doc, false);
    let v = dominant(doc, true);
    if doc.glyphs.values().all(|g| g.contours.is_empty()) {
        return Err("autohint_empty".into());
    }
    let cap = ['H', 'I', 'A', 'O']
        .iter()
        .find_map(|c| {
            doc.glyphs
                .values()
                .find(|g| g.codepoints.contains(c) && !g.contours.is_empty())
                .map(|g| postscript::glyph_bounds(g)[3])
        })
        .unwrap_or(postscript::bounds(doc)[3]);
    let cap = if cap > 0. { cap } else { (upm * 0.7).round() };
    let overshoot = (upm / 100.).round().max(1.).min(cap / 10.);
    let info=format!("FontName FontsGenerated\nOrigEmSqUnits {upm}\nLanguageGroup 0\nBaselineYCoord 0\nBaselineOvershoot {}\nCapHeight {cap}\nCapOvershoot {overshoot}\nDominantH [{h}]\nDominantV [{v}]\nFlexOK false\nBlueFuzz 1\n",-overshoot);
    let mut private = vec![];
    // DICT uses real operands, rather than Type 2's fixed-point byte 255.
    for n in [-overshoot, overshoot, cap, overshoot] {
        cff::real(&mut private, n);
    }
    private.push(6);
    cff::real(&mut private, h);
    private.push(10);
    cff::real(&mut private, v);
    private.push(11);
    cff::real(&mut private, h);
    private.extend([12, 12]);
    cff::real(&mut private, v);
    private.extend([12, 13]);
    let mut strings = Vec::with_capacity(doc.glyphs.len());
    let mut hinted = 0;
    for glyph in doc.glyphs.values() {
        if glyph.contours.is_empty() {
            strings.push(cff::charstring(glyph)?);
            continue;
        }
        let draws = path(glyph)?;
        let mut bez = format!("% g{}\nsc\n", glyph.id);
        for draw in &draws {
            for n in &draw.absolute {
                bez.push_str(&format!("{n} "));
            }
            bez.push_str(match draw.op {
                21 => "mt\n",
                5 => "dt\n",
                8 => "ct\n",
                _ => "cp\n",
            });
        }
        bez.push_str("ed\n");
        let result = font_hinting_native::autohint_bez(&bez, &info)?;
        let (program, has_hints) = compile(&result, &draws, glyph.advance)?;
        hinted += usize::from(has_hints);
        strings.push(program);
    }
    if hinted == 0 {
        return Err("autohint_empty".into());
    }
    cff::encode_prepared(
        doc,
        cid,
        Some(&Prepared {
            charstrings: strings,
            private,
            names,
            seed,
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_encoding_strings_and_charset_are_preserved() {
        let bytes = include_bytes!("../../tests/data/hinting/cff-hints.otf");
        let doc = crate::font_io::import(bytes, 0).unwrap();
        // Custom encoding includes a supplement SID pointing into the original strings.
        let encoding = vec![0x80, 1, 65, 1, 66, 1, 135];
        let seed = Seed {
            strings: vec![b"A".to_vec()],
            charset: SourceData::Bytes(vec![0, 1, 135]),
            encoding: SourceData::Bytes(encoding.clone()),
        };
        let output = generate(&doc, false, None, Some(seed)).unwrap();
        let actual = Seed::parse(&output, doc.glyphs.len()).unwrap();
        assert_eq!(actual.strings[0], b"A");
        assert!(matches!(actual.charset, SourceData::Bytes(ref b) if b == &[0, 1, 135]));
        assert!(matches!(actual.encoding, SourceData::Bytes(ref b) if b == &encoding));
        for predefined in [0, 1] {
            let seed = Seed {
                strings: vec![],
                charset: SourceData::Predefined(predefined),
                encoding: SourceData::Predefined(predefined),
            };
            let output = generate(&doc, false, None, Some(seed)).unwrap();
            let actual = Seed::parse(&output, doc.glyphs.len()).unwrap();
            assert!(matches!(actual.charset, SourceData::Predefined(n) if n == predefined));
            assert!(matches!(actual.encoding, SourceData::Predefined(n) if n == predefined));
        }
        for end in 0..output.len() {
            let _ = Seed::parse(&output[..end], doc.glyphs.len());
        }
    }
    #[test]
    fn masks_and_counter_hints_preserve_original_fractional_coordinates() {
        let draws = vec![
            Draw {
                op: 21,
                args: vec![0.123456, 0.],
                absolute: vec![0.123456, 0.],
            },
            Draw {
                op: 5,
                args: vec![60., 0.],
                absolute: vec![60.123456, 0.],
            },
            Draw {
                op: 9,
                args: vec![],
                absolute: vec![],
            },
        ];
        let bez="0 10 rb 20 10 rb 40 10 rb 0 60 ry sc 0.125 0 mt beginsubr snc 0 60 ry endsubr enc newcolors 60.125 0 dt cp ed";
        let (program, hinted) = compile(bez, &draws, 600.).unwrap();
        assert!(hinted);
        assert!(program.contains(&19));
        // Input geometry outside native fixed-point tolerance is rejected.
        assert_eq!(
            compile(&bez.replace("60.125", "61.0"), &draws, 600.).unwrap_err(),
            "cff_hint_geometry"
        );
        assert!(compile("sc 0 0 mt unsupported ed", &draws, 600.).is_err());
        let counter = "0 10 rv 20 10 rv 40 10 rv sc 0.125 0 mt 60.125 0 dt cp ed";
        assert!(compile(counter, &draws, 600.).unwrap().0.contains(&20));
    }
    #[test]
    fn cubic_curves_holes_and_fractional_paths_are_kept() {
        let mut doc = FontDocument::new("Cubic Hints", "H").unwrap();
        doc.glyphs.get_mut(&1).unwrap().contours = vec![
            crate::Contour {
                smooth: Default::default(),
                start: [0.125, 0.25],
                segments: vec![
                    crate::Segment::Cubic {
                        control1: [-25.125, 180.5],
                        control2: [-25.25, 540.75],
                        end: [0.125, 700.25],
                    },
                    crate::Segment::Line {
                        end: [400.125, 700.25],
                    },
                    crate::Segment::Line {
                        end: [400.125, 0.25],
                    },
                ],
                closed: true,
            },
            crate::Contour {
                smooth: Default::default(),
                start: [100.125, 100.25],
                segments: vec![
                    crate::Segment::Line {
                        end: [300.125, 100.25],
                    },
                    crate::Segment::Quadratic {
                        control: [310.125, 350.25],
                        end: [300.125, 600.25],
                    },
                    crate::Segment::Line {
                        end: [100.125, 600.25],
                    },
                ],
                closed: true,
            },
        ];
        for format in [
            super::super::Format::Otf,
            super::super::Format::CidOtf,
            super::super::Format::Cff,
            super::super::Format::CidCff,
        ] {
            let base = super::super::export(&doc, format).unwrap();
            let hinted = crate::hinting::autohint(&base, 0).unwrap();
            let removed = crate::hinting::export(&hinted, 0, true).unwrap();
            assert_ne!(hinted, removed);
        }
    }
    #[test]
    fn empty_and_open_documents_fail_without_output() {
        let doc = FontDocument::new("Empty", "").unwrap();
        assert_eq!(
            generate(&doc, false, None, None).unwrap_err(),
            "autohint_empty"
        );
        let mut doc = FontDocument::new("Open", "A").unwrap();
        doc.glyphs
            .get_mut(&1)
            .unwrap()
            .contours
            .push(crate::Contour {
                smooth: Default::default(),
                start: [0., 0.],
                segments: vec![crate::Segment::Line { end: [100., 700.] }],
                closed: false,
            });
        assert!(generate(&doc, false, None, None).is_err());
    }
}
