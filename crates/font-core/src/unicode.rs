//! Scalar-value input. Sequences are deliberately not normalized or shaped.
use crate::{Error, Result};
use std::collections::BTreeSet;
pub const MAX_BATCH: usize = 65534;

pub fn parse(input: &str, codepoints: bool) -> Result<BTreeSet<char>> {
    if input.len() > 1024 * 1024 {
        return Err(Error::TooLarge);
    }
    let mut values = BTreeSet::new();
    if codepoints {
        for token in input
            .split(|c: char| c.is_whitespace() || c == ',')
            .filter(|s| !s.is_empty())
        {
            let token = token.replace(['–', '—'], "-");
            let mut range = token.split('-');
            let first = scalar(range.next().ok_or(Error::InvalidUnicode)?)?;
            let last = range.next().map(scalar).transpose()?.unwrap_or(first);
            if range.next().is_some() || first > last {
                return Err(Error::InvalidUnicode);
            }
            if last - first + 1 > MAX_BATCH as u32 {
                return Err(Error::TooLarge);
            }
            for n in first..=last {
                values.insert(char::from_u32(n).ok_or(Error::InvalidUnicode)?);
            }
            if values.len() > MAX_BATCH {
                return Err(Error::TooLarge);
            }
        }
    } else {
        for c in input.chars() {
            values.insert(c);
            if values.len() > MAX_BATCH {
                return Err(Error::TooLarge);
            }
        }
    }
    if values.is_empty() {
        return Err(Error::InvalidUnicode);
    }
    Ok(values)
}
fn scalar(input: &str) -> Result<u32> {
    let input = input
        .strip_prefix("U+")
        .or_else(|| input.strip_prefix("u+"))
        .or_else(|| input.strip_prefix("0x"))
        .or_else(|| input.strip_prefix("0X"))
        .unwrap_or(input);
    if input.is_empty() || input.len() > 6 || !input.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(Error::InvalidUnicode);
    }
    let n = u32::from_str_radix(input, 16).map_err(|_| Error::InvalidUnicode)?;
    char::from_u32(n).ok_or(Error::InvalidUnicode)?;
    Ok(n)
}

/// Unicode ranges, not national character-set coverage claims.
pub const PRESETS: &[(&str, &str)] = &[
    ("ascii", "0020-007E"),
    ("latin1", "0020-007E 00A0-00FF"),
    ("hiragana", "3041-3096 3099-309F"),
    ("katakana", "30A0-30FF 31F0-31FF"),
    (
        "japanese_kana",
        "0020-007E 3000-303F 3041-3096 3099-309F 30A0-30FF 31F0-31FF FF01-FF60",
    ),
    ("cjk", "4E00-9FFF"),
    ("cjk_ext_a", "3400-4DBF"),
    ("cjk_ext_b", "20000-2A6DF"),
];
pub fn preset(id: &str) -> Result<BTreeSet<char>> {
    let (_, range) = PRESETS
        .iter()
        .find(|p| p.0 == id)
        .ok_or(Error::InvalidUnicode)?;
    parse(range, true)
}

pub fn preset_counts() -> &'static Vec<(&'static str, usize)> {
    static COUNTS: std::sync::OnceLock<Vec<(&'static str, usize)>> = std::sync::OnceLock::new();
    COUNTS.get_or_init(|| {
        PRESETS
            .iter()
            .map(|(id, _)| (*id, preset(id).expect("valid built-in preset").len()))
            .collect()
    })
}
