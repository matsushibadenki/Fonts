//! Bounded, non-executing interchange. Adobe Fonts is a source, not a binary format.
mod cff;
pub mod cmap;
mod legacy;
mod postscript;
use crate::{font_io::Result, FontDocument};
pub use legacy::{cid_system, info as legacy_info, legacy_import};
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Format {
    Ttf,
    Otf,
    CidOtf,
    Pfa,
    Pfb,
    Cid,
    Cff,
    CidCff,
    Cmap,
}
impl Format {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Ttf => "ttf",
            Self::Otf | Self::CidOtf => "otf",
            Self::Pfa => "pfa",
            Self::Pfb => "pfb",
            Self::Cid => "cid",
            Self::Cff | Self::CidCff => "cff",
            Self::Cmap => "cmap",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Ttf => "TrueType",
            Self::Otf => "OpenType CFF",
            Self::CidOtf => "OpenType CID-CFF",
            Self::Pfa => "Type 1 PFA",
            Self::Pfb => "Type 1 PFB",
            Self::Cid => "CID Type 1",
            Self::Cff => "CFF",
            Self::CidCff => "CID-CFF",
            Self::Cmap => "Unicode → CID CMap",
        }
    }
}
pub fn is_sfnt(bytes: &[u8]) -> bool {
    bytes.get(..4).is_some_and(|h| {
        [
            b"\0\x01\0\0".as_slice(),
            b"OTTO".as_slice(),
            b"ttcf".as_slice(),
            b"true".as_slice(),
        ]
        .contains(&h)
    })
}
pub fn identify(bytes: &[u8]) -> &'static str {
    if bytes.starts_with(b"OTTO") {
        if let Ok(face) = ttf_parser::Face::parse(bytes, 0) {
            if face
                .tables()
                .cff
                .is_some_and(|c| c.glyph_cid(ttf_parser::GlyphId(0)).is_some())
            {
                return "OpenType CID-CFF";
            }
            if face.tables().cff2.is_some() {
                return "OpenType CFF2";
            }
        }
        "OpenType CFF"
    } else if bytes.starts_with(b"ttcf") {
        "OpenType Collection"
    } else if is_sfnt(bytes) {
        "TrueType"
    } else if bytes.starts_with(&[0x80, 1]) {
        "Type 1 PFB"
    } else if bytes.starts_with(b"%!PS-Adobe-3.0 Resource-CIDFont") {
        "CID Type 1"
    } else if bytes.starts_with(b"%!") {
        "Type 1 PFA / PostScript"
    } else if bytes.starts_with(&[1, 0]) {
        "CFF"
    } else {
        "Unknown"
    }
}
pub fn supported_signature(bytes: &[u8]) -> bool {
    is_sfnt(bytes)
        || bytes.starts_with(b"%!")
        || bytes.starts_with(&[0x80, 1])
        || bytes.starts_with(&[1, 0])
}
pub fn export(doc: &FontDocument, format: Format) -> Result<Vec<u8>> {
    doc.validate().map_err(|e| e.to_string())?;
    if (!doc.kerning.is_empty() || !doc.class_kerning.pairs.is_empty())
        && !matches!(
            format,
            Format::Ttf | Format::Otf | Format::CidOtf | Format::Cmap
        )
    {
        return Err("kerning_format_unsupported".into());
    }
    match format {
        Format::Ttf => crate::font_io::export(doc),
        Format::Pfa => postscript::type1(doc, false),
        Format::Pfb => postscript::type1(doc, true),
        Format::Cid => postscript::cid(doc),
        Format::Cff => cff::encode_raw(doc),
        Format::CidCff => cff::encode(doc, true),
        Format::Otf => cff::opentype(doc, false),
        Format::CidOtf => cff::opentype(doc, true),
        Format::Cmap => cmap::export(doc),
    }
}
