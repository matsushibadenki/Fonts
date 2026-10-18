use super::postscript;
use crate::{font_io::Result, FontDocument, Glyph};
use std::collections::BTreeMap;
fn index(items: &[Vec<u8>]) -> Result<Vec<u8>> {
    let count = u16::try_from(items.len()).map_err(|_| "font_too_large")?;
    let mut out = count.to_be_bytes().to_vec();
    if count == 0 {
        return Ok(out);
    }
    out.push(4);
    let mut offset = 1u32;
    out.extend(offset.to_be_bytes());
    for item in items {
        offset = offset
            .checked_add(u32::try_from(item.len()).map_err(|_| "font_too_large")?)
            .ok_or("font_too_large")?;
        out.extend(offset.to_be_bytes());
    }
    for item in items {
        out.extend(item);
    }
    Ok(out)
}
fn int(out: &mut Vec<u8>, n: i32) {
    out.push(29);
    out.extend(n.to_be_bytes());
}
fn dict(out: &mut Vec<u8>, args: &[i32], op: &[u8]) {
    for n in args {
        int(out, *n);
    }
    out.extend(op);
}
fn real(out: &mut Vec<u8>, n: f64) {
    out.push(30);
    let s = format!("{n:.12}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    let mut nibs: Vec<u8> = s
        .chars()
        .map(|c| match c {
            '-' => 14,
            '.' => 10,
            _ => c.to_digit(10).unwrap() as u8,
        })
        .collect();
    nibs.push(15);
    if !nibs.len().is_multiple_of(2) {
        nibs.push(15);
    }
    for p in nibs.chunks(2) {
        out.push(p[0] * 16 + p[1]);
    }
}
fn number(out: &mut Vec<u8>, n: f64) -> Result<()> {
    if !n.is_finite() || n.abs() > 1_000_000. {
        return Err("font_coordinate_range".into());
    }
    if n.abs() > 32767. {
        number(out, n / 64.)?;
        number(out, 64.)?;
        out.extend([12, 24]);
    } else if n == n.round() {
        let v = n as i32;
        match v {
            -107..=107 => out.push((v + 139) as u8),
            108..=1131 => out.extend([(247 + (v - 108) / 256) as u8, ((v - 108) % 256) as u8]),
            -1131..=-108 => out.extend([(251 + (-v - 108) / 256) as u8, ((-v - 108) % 256) as u8]),
            _ => {
                out.push(28);
                out.extend((v as i16).to_be_bytes());
            }
        }
    } else {
        out.push(255);
        out.extend(((n * 65536.).round() as i32).to_be_bytes());
    }
    Ok(())
}
fn charstring(g: &Glyph) -> Result<Vec<u8>> {
    let mut out = vec![];
    number(&mut out, g.advance)?;
    postscript::curves(g, |op, args| {
        if op != 9 {
            for n in args {
                number(&mut out, *n)?;
            }
            out.push(op);
        }
        Ok(())
    })?;
    out.push(14);
    Ok(out)
}
pub fn encode_raw(doc: &FontDocument) -> Result<Vec<u8>> {
    encode(&postscript::expand_aliases(doc)?, false)
}
pub fn encode(doc: &FontDocument, cid: bool) -> Result<Vec<u8>> {
    let glyph_names = postscript::names(doc);
    let name = postscript::ps_name(&doc.family);
    let name_index = index(&[name.into_bytes()])?;
    let mut strings = vec![
        doc.family.as_bytes().to_vec(),
        postscript::style(doc).as_bytes().to_vec(),
        doc.metadata.copyright.as_bytes().to_vec(),
        b"1.0".to_vec(),
    ];
    let system = doc.metadata.cid_system.clone().unwrap_or(crate::CidSystem {
        registry: "Adobe".into(),
        ordering: "Identity".into(),
        supplement: 0,
    });
    strings.push(system.registry.as_bytes().to_vec());
    strings.push(system.ordering.as_bytes().to_vec());
    let mut charset = vec![0];
    if cid {
        for id in postscript::cids(doc)?.iter().skip(1) {
            charset.extend(id.to_be_bytes());
        }
    } else {
        for glyph_name in glyph_names.iter().skip(1) {
            let sid = u16::try_from(391 + strings.len()).map_err(|_| "font_too_large")?;
            strings.push(glyph_name.as_bytes().to_vec());
            charset.extend(sid.to_be_bytes());
        }
    }
    let strings_index = index(&strings)?;
    let charstrings = index(
        &doc.glyphs
            .values()
            .map(charstring)
            .collect::<Result<Vec<_>>>()?,
    )?;
    let fdselect = if cid {
        let mut v = vec![0];
        v.resize(doc.glyphs.len() + 1, 0);
        v
    } else {
        vec![]
    };
    let mut fd = vec![];
    dict(&mut fd, &[0, 0], &[18]);
    let fd_array = if cid { index(&[fd])? } else { vec![] };
    let top = |charset_offset: i32, cs_offset: i32, select_offset: i32, fd_offset: i32| {
        let mut top = vec![];
        if cid {
            dict(&mut top, &[395, 396, system.supplement], &[12, 30]);
            dict(
                &mut top,
                &[u32::from(*postscript::cids(doc).unwrap().iter().max().unwrap()) as i32 + 1],
                &[12, 34],
            );
        }
        dict(&mut top, &[394], &[0]);
        dict(&mut top, &[393], &[1]);
        dict(&mut top, &[391], &[2]);
        dict(&mut top, &[391], &[3]);
        dict(&mut top, &[392], &[4]);
        real(&mut top, doc.metadata.italic_angle);
        top.extend([12, 2]);
        for n in postscript::bounds(doc) {
            real(&mut top, n);
        }
        top.push(5);
        for n in [
            1. / f64::from(doc.units_per_em),
            0.,
            0.,
            1. / f64::from(doc.units_per_em),
            0.,
            0.,
        ] {
            real(&mut top, n);
        }
        top.extend([12, 7]);
        dict(&mut top, &[charset_offset], &[15]);
        dict(&mut top, &[cs_offset], &[17]);
        if cid {
            dict(&mut top, &[fd_offset], &[12, 36]);
            dict(&mut top, &[select_offset], &[12, 37]);
        } else {
            dict(&mut top, &[0, 0], &[18]);
        }
        top
    };
    let dummy = index(&[top(0, 0, 0, 0)])?;
    let charset_offset = 4 + name_index.len() + dummy.len() + strings_index.len() + 2;
    let cs_offset = charset_offset + charset.len();
    let select_offset = cs_offset + charstrings.len();
    let fd_offset = select_offset + fdselect.len();
    let top_index = index(&[top(
        charset_offset as i32,
        cs_offset as i32,
        select_offset as i32,
        fd_offset as i32,
    )])?;
    let mut out = vec![1, 0, 4, 4];
    for data in [
        name_index,
        top_index,
        strings_index,
        vec![0, 0],
        charset,
        charstrings,
        fdselect,
        fd_array,
    ] {
        out.extend(data);
    }
    Ok(out)
}
pub fn opentype(doc: &FontDocument, cid: bool) -> Result<Vec<u8>> {
    let cff = encode(doc, cid)?;
    // Seed shared SFNT metadata without converting cubic curves through glyf.
    let metrics = FontDocument {
        class_kerning: doc.class_kerning.clone(),
        kerning: doc.kerning.clone(),
        snippets: Default::default(),
        reference_images: Default::default(),
        schema_version: doc.schema_version,
        family: doc.family.clone(),
        units_per_em: doc.units_per_em,
        ascender: doc.ascender,
        descender: doc.descender,
        metadata: doc.metadata.clone(),
        glyphs: doc
            .glyphs
            .iter()
            .map(|(id, g)| {
                (
                    *id,
                    Glyph {
                        id: *id,
                        name: g.name.clone(),
                        codepoints: g.codepoints.clone(),
                        cid: g.cid,
                        advance: g.advance,
                        contours: vec![],
                        status: g.status,
                    },
                )
            })
            .collect(),
    };
    let ttf = crate::font_io::export(&metrics)?;
    let mut tables = unpack(&ttf)?;
    tables.remove(b"glyf");
    tables.remove(b"loca");
    let mut maxp = 0x5000u32.to_be_bytes().to_vec();
    maxp.extend((doc.glyphs.len() as u16).to_be_bytes());
    tables.insert(*b"maxp", maxp);
    tables.insert(*b"CFF ", cff);
    let mut all = [0i16; 4];
    let mut max_extent = 0i16;
    let mut min_lsb = 0i16;
    let mut min_rsb = 0i16;
    for (index, g) in doc.glyphs.values().enumerate() {
        let b = postscript::glyph_bounds(g);
        let mut bounds = [0i16; 4];
        for (i, n) in b.into_iter().enumerate() {
            let n = if i < 2 { n.floor() } else { n.ceil() };
            if !(-32768. ..=32767.).contains(&n) {
                return Err("font_coordinate_range".into());
            }
            bounds[i] = n as i16;
        }
        all = [
            all[0].min(bounds[0]),
            all[1].min(bounds[1]),
            all[2].max(bounds[2]),
            all[3].max(bounds[3]),
        ];
        min_lsb = min_lsb.min(bounds[0]);
        max_extent = max_extent.max(bounds[2]);
        min_rsb = min_rsb
            .min((g.advance.round() as i32 - i32::from(bounds[2])).clamp(-32768, 32767) as i16);
        let hmtx = tables.get_mut(b"hmtx").unwrap();
        hmtx[index * 4 + 2..index * 4 + 4].copy_from_slice(&bounds[0].to_be_bytes());
    }
    let head = tables.get_mut(b"head").unwrap();
    for (i, n) in all.iter().enumerate() {
        head[36 + i * 2..38 + i * 2].copy_from_slice(&n.to_be_bytes());
    }
    head[50..52].copy_from_slice(&0u16.to_be_bytes());
    let hhea = tables.get_mut(b"hhea").unwrap();
    hhea[12..14].copy_from_slice(&min_lsb.to_be_bytes());
    hhea[14..16].copy_from_slice(&min_rsb.to_be_bytes());
    hhea[16..18].copy_from_slice(&max_extent.to_be_bytes());
    let os2 = tables.get_mut(b"OS/2").unwrap();
    os2[74..76]
        .copy_from_slice(&(all[3].max(doc.ascender.round() as i16).max(0) as u16).to_be_bytes());
    os2[76..78].copy_from_slice(
        &all[1]
            .min(doc.descender.round() as i16)
            .unsigned_abs()
            .to_be_bytes(),
    );
    let out = pack(tables, *b"OTTO")?;
    ttf_parser::Face::parse(&out, 0).map_err(|_| "font_invalid_output")?;
    Ok(out)
}
fn unpack(bytes: &[u8]) -> Result<BTreeMap<[u8; 4], Vec<u8>>> {
    let count = u16::from_be_bytes(bytes[4..6].try_into().unwrap()) as usize;
    let mut tables = BTreeMap::new();
    for r in bytes[12..12 + count * 16].chunks(16) {
        let tag = r[..4].try_into().unwrap();
        let offset = u32::from_be_bytes(r[8..12].try_into().unwrap()) as usize;
        let len = u32::from_be_bytes(r[12..16].try_into().unwrap()) as usize;
        tables.insert(
            tag,
            bytes
                .get(offset..offset + len)
                .ok_or("font_invalid_output")?
                .to_vec(),
        );
    }
    Ok(tables)
}
fn sum(bytes: &[u8]) -> u32 {
    bytes.chunks(4).fold(0u32, |sum, b| {
        let mut x = [0; 4];
        x[..b.len()].copy_from_slice(b);
        sum.wrapping_add(u32::from_be_bytes(x))
    })
}
fn pack(mut tables: BTreeMap<[u8; 4], Vec<u8>>, signature: [u8; 4]) -> Result<Vec<u8>> {
    tables.get_mut(b"head").ok_or("font_invalid_output")?[8..12].fill(0);
    let count = tables.len() as u16;
    let power = 1u16 << count.ilog2();
    let mut out = signature.to_vec();
    for n in [
        count,
        power * 16,
        count.ilog2() as u16,
        (count - power) * 16,
    ] {
        out.extend(n.to_be_bytes());
    }
    let header = 12 + tables.len() * 16;
    let mut data = vec![];
    let mut head = 0;
    for (tag, bytes) in tables {
        let offset = header + data.len();
        if tag == *b"head" {
            head = offset;
        }
        out.extend(tag);
        for n in [sum(&bytes), offset as u32, bytes.len() as u32] {
            out.extend(n.to_be_bytes());
        }
        data.extend(bytes);
        while data.len() % 4 != 0 {
            data.push(0);
        }
    }
    out.extend(data);
    let adjustment = 0xB1B0AFBAu32.wrapping_sub(sum(&out));
    out[head + 8..head + 12].copy_from_slice(&adjustment.to_be_bytes());
    Ok(out)
}
