//! WOFF2 export and strict validation of our null-transform profile (not a general importer).
const TAGS: [[u8; 4]; 63] = [
    *b"cmap", *b"head", *b"hhea", *b"hmtx", *b"maxp", *b"name", *b"OS/2", *b"post", *b"cvt ",
    *b"fpgm", *b"glyf", *b"loca", *b"prep", *b"CFF ", *b"VORG", *b"EBDT", *b"EBLC", *b"gasp",
    *b"hdmx", *b"kern", *b"LTSH", *b"PCLT", *b"VDMX", *b"vhea", *b"vmtx", *b"BASE", *b"GDEF",
    *b"GPOS", *b"GSUB", *b"EBSC", *b"JSTF", *b"MATH", *b"CBDT", *b"CBLC", *b"COLR", *b"CPAL",
    *b"SVG ", *b"sbix", *b"acnt", *b"avar", *b"bdat", *b"bloc", *b"bsln", *b"cvar", *b"fdsc",
    *b"feat", *b"fmtx", *b"fvar", *b"gvar", *b"hsty", *b"just", *b"lcar", *b"mort", *b"morx",
    *b"opbd", *b"prop", *b"trak", *b"Zapf", *b"Silf", *b"Glat", *b"Gloc", *b"Feat", *b"Sill",
];
use crate::{font_io::Result, FontDocument};
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
const LIMIT: usize = 512 * 1024 * 1024;
#[derive(Clone, Copy, Default, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Outline {
    #[default]
    TrueType,
    Cff,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Options {
    pub outline: Outline,
    pub quality: u32,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            outline: Outline::TrueType,
            quality: 9,
        }
    }
}
impl Options {
    pub fn validate(self) -> Result<()> {
        if !(1..=11).contains(&self.quality) {
            return Err("woff2_options_invalid".into());
        }
        Ok(())
    }
}
fn bad() -> String {
    "woff2_invalid".into()
}
fn u16at(b: &[u8], o: usize) -> Result<u16> {
    Ok(u16::from_be_bytes(
        b.get(o..o + 2)
            .ok_or_else(bad)?
            .try_into()
            .map_err(|_| bad())?,
    ))
}
fn u32at(b: &[u8], o: usize) -> Result<u32> {
    Ok(u32::from_be_bytes(
        b.get(o..o + 4)
            .ok_or_else(bad)?
            .try_into()
            .map_err(|_| bad())?,
    ))
}
fn base128(out: &mut Vec<u8>, n: u32) {
    let count = ((32 - n.leading_zeros()).max(1) as usize).div_ceil(7);
    for i in (0..count).rev() {
        out.push(((n >> (i * 7)) & 127) as u8 | if i > 0 { 128 } else { 0 });
    }
}
fn read128(b: &[u8], p: &mut usize) -> Result<usize> {
    let mut n = 0u32;
    for i in 0..5 {
        let v = *b.get(*p).ok_or_else(bad)?;
        *p += 1;
        if (i == 0 && v == 128) || n & 0xfe000000 != 0 {
            return Err(bad());
        }
        n = (n << 7) | u32::from(v & 127);
        if v & 128 == 0 {
            return Ok(n as usize);
        }
    }
    Err(bad())
}
fn checksum(b: &[u8]) -> u32 {
    b.chunks(4).fold(0u32, |s, c| {
        let mut a = [0; 4];
        a[..c.len()].copy_from_slice(c);
        s.wrapping_add(u32::from_be_bytes(a))
    })
}
type Table = ([u8; 4], Vec<u8>);
fn sfnt_tables(b: &[u8]) -> Result<Vec<Table>> {
    if b.len() > LIMIT || !matches!(b.get(..4), Some(b"\0\x01\0\0" | b"OTTO")) {
        return Err(bad());
    }
    let count = u16at(b, 4)? as usize;
    if count == 0 || count > 4095 {
        return Err(bad());
    }
    let mut tables = Vec::new();
    for i in 0..count {
        let o = 12 + 16 * i;
        let tag: [u8; 4] = b
            .get(o..o + 4)
            .ok_or_else(bad)?
            .try_into()
            .map_err(|_| bad())?;
        let start = u32at(b, o + 8)? as usize;
        let len = u32at(b, o + 12)? as usize;
        let data = b
            .get(start..start.checked_add(len).ok_or_else(bad)?)
            .ok_or_else(bad)?;
        if tables.last().is_some_and(|(t, _): &Table| *t >= tag) {
            return Err(bad());
        }
        tables.push((tag, data.to_vec()));
    }
    Ok(tables)
}
pub fn export(doc: &FontDocument, options: Options) -> Result<Vec<u8>> {
    options.validate()?;
    let sfnt = match options.outline {
        Outline::TrueType => crate::font_io::export(doc)?,
        Outline::Cff => super::cff::opentype(doc, false)?,
    };
    encode_sfnt(sfnt, options.quality)
}
/// Package an already validated SFNT, retaining generated hinting bytecode.
pub fn encode_sfnt(mut sfnt: Vec<u8>, quality: u32) -> Result<Vec<u8>> {
    if !(1..=11).contains(&quality) {
        return Err("woff2_options_invalid".into());
    }
    if sfnt.len() > crate::font_io::MAX_FONT_BYTES as usize {
        return Err("font_too_large".into());
    }
    ttf_parser::Face::parse(&sfnt, 0).map_err(|_| bad())?;
    sfnt_tables(&sfnt)?;
    // WOFF2 requires the lossless-modification flag even with null transforms.
    for i in 0..u16at(&sfnt, 4)? as usize {
        let record = 12 + 16 * i;
        if sfnt.get(record..record + 4) == Some(b"head") {
            let offset = u32at(&sfnt, record + 8)? as usize;
            let len = u32at(&sfnt, record + 12)? as usize;
            let flags = u16at(&sfnt, offset + 16)? | 0x0800;
            sfnt[offset + 16..offset + 18].copy_from_slice(&flags.to_be_bytes());
            sfnt[offset + 8..offset + 12].fill(0);
            let sum = checksum(&sfnt[offset..offset + len]);
            sfnt[record + 4..record + 8].copy_from_slice(&sum.to_be_bytes());
            let adjustment = 0xb1b0afbau32.wrapping_sub(checksum(&sfnt));
            sfnt[offset + 8..offset + 12].copy_from_slice(&adjustment.to_be_bytes());
            break;
        }
    }
    let tables = sfnt_tables(&sfnt)?;
    let mut directory = Vec::new();
    let mut stream = Vec::new();
    for (tag, data) in &tables {
        let index = TAGS.iter().position(|t| t == tag).unwrap_or(63);
        let version = if tag == b"glyf" || tag == b"loca" {
            3
        } else {
            0
        };
        directory.push(index as u8 | version << 6);
        if index == 63 {
            directory.extend(tag);
        }
        base128(&mut directory, data.len().try_into().map_err(|_| bad())?);
        stream.extend(data);
    }
    let mut compressed = Vec::new();
    {
        let mut writer = brotli::CompressorWriter::new(&mut compressed, 65536, quality, 22);
        writer.write_all(&stream).map_err(|_| bad())?;
    }
    let length = (48 + directory.len() + compressed.len() + 3) & !3;
    if length > LIMIT {
        return Err("font_too_large".into());
    }
    let mut out = Vec::with_capacity(length);
    out.extend(b"wOF2");
    out.extend(&sfnt[..4]);
    out.extend((length as u32).to_be_bytes());
    out.extend((tables.len() as u16).to_be_bytes());
    out.extend(0u16.to_be_bytes());
    out.extend((sfnt.len() as u32).to_be_bytes());
    out.extend((compressed.len() as u32).to_be_bytes());
    out.extend(1u16.to_be_bytes());
    out.extend(0u16.to_be_bytes());
    out.extend([0; 20]);
    out.extend(directory);
    out.extend(compressed);
    // Reference decoders used by browsers require the final block to be 4-byte aligned.
    out.resize(length, 0);
    let decoded = decode(&out)?;
    if sfnt_tables(&decoded)? != tables {
        return Err(bad());
    }
    Ok(out)
}
/// Decode only this application's output profile, with bounded expansion. No transformed/external import.
pub fn decode(b: &[u8]) -> Result<Vec<u8>> {
    if b.len() < 48
        || b.len() > LIMIT
        || b.get(..4) != Some(b"wOF2")
        || u32at(b, 8)? as usize != b.len()
        || u16at(b, 14)? != 0
        || b[28..48] != [0; 20]
    {
        return Err(bad());
    }
    if !matches!(b.get(4..8), Some(b"\0\x01\0\0" | b"OTTO")) {
        return Err(bad());
    }
    let count = u16at(b, 12)? as usize;
    if count == 0 || count > 4095 {
        return Err(bad());
    }
    let total = u32at(b, 16)? as usize;
    if total > LIMIT {
        return Err(bad());
    }
    let compressed = u32at(b, 20)? as usize;
    let mut p = 48;
    let mut entries = Vec::new();
    let mut size = 0usize;
    let mut padded = 12 + 16 * count;
    for _ in 0..count {
        let flags = *b.get(p).ok_or_else(bad)?;
        p += 1;
        let index = (flags & 63) as usize;
        let tag = if index == 63 {
            let t = b
                .get(p..p + 4)
                .ok_or_else(bad)?
                .try_into()
                .map_err(|_| bad())?;
            p += 4;
            t
        } else {
            TAGS[index]
        };
        if flags >> 6
            != if tag == *b"glyf" || tag == *b"loca" {
                3
            } else {
                0
            }
            || entries.last().is_some_and(|(t, _)| *t >= tag)
        {
            return Err(bad());
        }
        let len = read128(b, &mut p)?;
        if len > LIMIT {
            return Err(bad());
        }
        size = size.checked_add(len).ok_or_else(bad)?;
        padded = padded.checked_add((len + 3) & !3).ok_or_else(bad)?;
        if size > LIMIT || padded > LIMIT {
            return Err(bad());
        }
        entries.push((tag, len));
    }
    let end = p.checked_add(compressed).ok_or_else(bad)?;
    if padded != total
        || end > b.len()
        || (end + 3) & !3 != b.len()
        || b[end..].iter().any(|v| *v != 0)
    {
        return Err(bad());
    }
    let mut stream = Vec::new();
    brotli::Decompressor::new(&b[p..end], 65536)
        .take(size as u64 + 1)
        .read_to_end(&mut stream)
        .map_err(|_| bad())?;
    if stream.len() != size {
        return Err(bad());
    }
    let power = 1usize << (usize::BITS - 1 - count.leading_zeros());
    let mut out = Vec::with_capacity(total);
    out.extend(&b[4..8]);
    out.extend((count as u16).to_be_bytes());
    out.extend((power as u16 * 16).to_be_bytes());
    out.extend((power.ilog2() as u16).to_be_bytes());
    out.extend(((count - power) as u16 * 16).to_be_bytes());
    let mut payload = Vec::new();
    let mut start = 0;
    let mut offset = 12 + 16 * count;
    let mut head = None;
    for (tag, len) in entries {
        let mut data = stream[start..start + len].to_vec();
        if tag == *b"head" {
            if data.len() < 12 {
                return Err(bad());
            }
            head = Some(offset);
            data[8..12].fill(0);
        }
        out.extend(tag);
        out.extend(checksum(&data).to_be_bytes());
        out.extend((offset as u32).to_be_bytes());
        out.extend((len as u32).to_be_bytes());
        payload.extend(data);
        while payload.len() % 4 != 0 {
            payload.push(0);
        }
        offset = 12 + 16 * count + payload.len();
        start += len;
    }
    out.extend(payload);
    let head = head.ok_or_else(bad)?;
    let adjustment = 0xb1b0afbau32.wrapping_sub(checksum(&out));
    out[head + 8..head + 12].copy_from_slice(&adjustment.to_be_bytes());
    ttf_parser::Face::parse(&out, 0).map_err(|_| bad())?;
    Ok(out)
}
