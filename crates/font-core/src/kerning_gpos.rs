//! One PairPos lookup: explicit Format 1 first, class Format 2 fallback.
use super::*;
use crate::class_kerning::{Class, ClassKerning, MAX_CLASSES, MAX_MEMBERS};
use std::collections::BTreeMap;
fn put(v: &mut Vec<u8>, n: u16) {
    v.extend(n.to_be_bytes());
}
fn set(v: &mut [u8], i: usize, n: u16) {
    v[i..i + 2].copy_from_slice(&n.to_be_bytes());
}
fn word(v: &[u8], i: usize) -> Option<u16> {
    Some(u16::from_be_bytes(v.get(i..i + 2)?.try_into().ok()?))
}
fn offset(v: &mut [u8], i: usize, n: usize) -> Result<()> {
    set(v, i, u16::try_from(n).map_err(|_| Error::TooLarge)?);
    Ok(())
}
fn coverage(v: &mut Vec<u8>, ids: impl Iterator<Item = u16>) {
    let ids: Vec<_> = ids.collect();
    put(v, 1);
    put(v, ids.len() as u16);
    for id in ids {
        put(v, id);
    }
}
fn explicit(doc: &FontDocument, dense: &BTreeMap<u32, u16>) -> Result<Vec<u8>> {
    let mut groups: BTreeMap<u16, Vec<(u16, i16)>> = BTreeMap::new();
    for p in &doc.kerning.0 {
        groups
            .entry(dense[&p.left])
            .or_default()
            .push((dense[&p.right], p.value));
    }
    let mut sub = vec![];
    for n in [1, 0, 4, 0, groups.len() as u16] {
        put(&mut sub, n);
    }
    sub.resize(10 + 2 * groups.len(), 0);
    for (i, pairs) in groups.values().enumerate() {
        let at = sub.len();
        offset(&mut sub, 10 + i * 2, at)?;
        put(&mut sub, pairs.len() as u16);
        for (r, v) in pairs {
            put(&mut sub, *r);
            put(&mut sub, *v as u16);
        }
    }
    let at = sub.len();
    offset(&mut sub, 2, at)?;
    coverage(&mut sub, groups.keys().copied());
    Ok(sub)
}
fn definition(out: &mut Vec<u8>, members: &BTreeMap<u16, u16>) {
    let mut ranges: Vec<(u16, u16, u16)> = vec![];
    for (&id, &class) in members {
        if let Some(last) = ranges.last_mut() {
            if last.1.checked_add(1) == Some(id) && last.2 == class {
                last.1 = id;
                continue;
            }
        }
        ranges.push((id, id, class));
    }
    put(out, 2);
    put(out, ranges.len() as u16);
    for (a, b, c) in ranges {
        put(out, a);
        put(out, b);
        put(out, c);
    }
}
fn classes(doc: &FontDocument, dense: &BTreeMap<u32, u16>) -> Result<Vec<u8>> {
    let c = &doc.class_kerning;
    let left: BTreeMap<_, _> = c
        .left
        .iter()
        .enumerate()
        .flat_map(|(i, c)| c.glyphs.iter().map(move |g| (dense[g], i as u16 + 1)))
        .collect();
    let right: BTreeMap<_, _> = c
        .right
        .iter()
        .enumerate()
        .flat_map(|(i, c)| c.glyphs.iter().map(move |g| (dense[g], i as u16 + 1)))
        .collect();
    let mut sub = vec![];
    for n in [
        2,
        0,
        4,
        0,
        0,
        0,
        c.left.len() as u16 + 1,
        c.right.len() as u16 + 1,
    ] {
        put(&mut sub, n);
    }
    for l in 0..=c.left.len() {
        for r in 0..=c.right.len() {
            let value = if l == 0 || r == 0 {
                0
            } else {
                c.pairs
                    .iter()
                    .find(|p| p.left == c.left[l - 1].id && p.right == c.right[r - 1].id)
                    .map(|p| p.value)
                    .unwrap_or(0)
            };
            put(&mut sub, value as u16);
        }
    }
    let at = sub.len();
    offset(&mut sub, 2, at)?;
    coverage(&mut sub, left.keys().copied());
    let at = sub.len();
    offset(&mut sub, 8, at)?;
    definition(&mut sub, &left);
    let at = sub.len();
    offset(&mut sub, 10, at)?;
    definition(&mut sub, &right);
    Ok(sub)
}
pub fn encode(doc: &FontDocument) -> Result<Option<Vec<u8>>> {
    doc.kerning.validate(doc)?;
    doc.class_kerning.validate(doc)?;
    let dense: BTreeMap<_, _> = doc
        .glyphs
        .keys()
        .enumerate()
        .map(|(i, id)| (*id, i as u16))
        .collect();
    let mut subs = vec![];
    if !doc.kerning.is_empty() {
        subs.push(explicit(doc, &dense)?);
    }
    if !doc.class_kerning.pairs.is_empty() {
        subs.push(classes(doc, &dense)?);
    }
    if subs.is_empty() {
        return Ok(None);
    }
    let mut out = vec![];
    for n in [1, 0, 10, 30, 44, 1] {
        put(&mut out, n);
    }
    out.extend(b"DFLT");
    for n in [8, 4, 0, 0, 0xffff, 1, 0, 1] {
        put(&mut out, n);
    }
    out.extend(b"kern");
    for n in [8, 0, 1, 0, 1, 4, 2, 0, subs.len() as u16] {
        put(&mut out, n);
    }
    out.resize(54 + subs.len() * 2, 0);
    for (i, s) in subs.into_iter().enumerate() {
        let at = out.len() - 48;
        offset(&mut out, 54 + i * 2, at)?;
        out.extend(s);
    }
    Ok(Some(out))
}
fn read_explicit(sub: &[u8]) -> Option<Kerning> {
    let at = usize::from(word(sub, 2)?);
    let count = usize::from(word(sub, 8)?);
    if count > MAX_PAIRS || word(sub, at)? != 1 || usize::from(word(sub, at + 2)?) != count {
        return None;
    }
    let mut pairs = vec![];
    for i in 0..count {
        let left = u32::from(word(sub, at + 4 + i * 2)?);
        let offset = usize::from(word(sub, 10 + i * 2)?);
        let n = usize::from(word(sub, offset)?);
        if pairs.len() + n > MAX_PAIRS {
            return None;
        }
        for j in 0..n {
            pairs.push(Pair {
                left,
                right: u32::from(word(sub, offset + 2 + j * 4)?),
                value: word(sub, offset + 4 + j * 4)? as i16,
            });
        }
    }
    Some(Kerning(pairs))
}
fn read_definition(sub: &[u8], at: usize, count: usize) -> Option<Vec<BTreeSet<u32>>> {
    if word(sub, at)? != 2 {
        return None;
    }
    let n = usize::from(word(sub, at + 2)?);
    if n > MAX_MEMBERS {
        return None;
    }
    let mut groups = vec![BTreeSet::new(); count];
    let mut total = 0;
    let mut previous = None;
    for i in 0..n {
        let o = at + 4 + i * 6;
        let start = word(sub, o)?;
        let end = word(sub, o + 2)?;
        let class = usize::from(word(sub, o + 4)?);
        if start > end || class == 0 || class >= count || previous.is_some_and(|p| p >= start) {
            return None;
        }
        total += usize::from(end - start) + 1;
        if total > MAX_MEMBERS {
            return None;
        }
        groups[class].extend((start..=end).map(u32::from));
        previous = Some(end);
    }
    Some(groups)
}
fn read_classes(sub: &[u8]) -> Option<ClassKerning> {
    let lc = usize::from(word(sub, 12)?);
    let rc = usize::from(word(sub, 14)?);
    if !(2..=MAX_CLASSES + 1).contains(&lc) || !(2..=MAX_CLASSES + 1).contains(&rc) {
        return None;
    }
    let left = read_definition(sub, usize::from(word(sub, 8)?), lc)?;
    let right = read_definition(sub, usize::from(word(sub, 10)?), rc)?;
    let at = usize::from(word(sub, 2)?);
    if word(sub, at)? != 1 {
        return None;
    }
    let covered = usize::from(word(sub, at + 2)?);
    let members = left
        .iter()
        .skip(1)
        .flatten()
        .copied()
        .collect::<BTreeSet<_>>();
    if covered != members.len()
        || members
            .iter()
            .enumerate()
            .any(|(i, g)| word(sub, at + 4 + i * 2).map(u32::from) != Some(*g))
    {
        return None;
    }
    if left
        .iter()
        .skip(1)
        .chain(right.iter().skip(1))
        .any(BTreeSet::is_empty)
    {
        return None;
    }
    let mut classes = ClassKerning::default();
    for (i, glyphs) in left.into_iter().enumerate().skip(1) {
        classes.left.push(Class {
            id: i as u32,
            name: format!("Left {i}"),
            glyphs,
        });
    }
    for (i, glyphs) in right.into_iter().enumerate().skip(1) {
        classes.right.push(Class {
            id: (lc + i - 1) as u32,
            name: format!("Right {i}"),
            glyphs,
        });
    }
    classes.next_id = (lc + rc - 1) as u32;
    for l in 0..lc {
        for r in 0..rc {
            let value = word(sub, 16 + (l * rc + r) * 2)? as i16;
            if l == 0 || r == 0 {
                if value != 0 {
                    return None;
                }
            } else if value != 0 {
                classes.pairs.push(Pair {
                    left: classes.left[l - 1].id,
                    right: classes.right[r - 1].id,
                    value,
                });
            }
        }
    }
    Some(classes)
}
/// Only our global, unflagged pair lookup; no arbitrary imported shaping is flattened.
pub fn decode(data: &[u8]) -> Option<(Kerning, ClassKerning)> {
    if data.get(..10)? != [0, 1, 0, 0, 0, 10, 0, 30, 0, 44]
        || data.get(10..18)? != [0, 1, b'D', b'F', b'L', b'T', 0, 8]
        || data.get(18..30)? != [0, 4, 0, 0, 0, 0, 255, 255, 0, 1, 0, 0]
        || data.get(30..52)?
            != [
                0, 1, b'k', b'e', b'r', b'n', 0, 8, 0, 0, 0, 1, 0, 0, 0, 1, 0, 4, 0, 2, 0, 0,
            ]
    {
        return None;
    }
    let n = usize::from(word(data, 52)?);
    if !(1..=2).contains(&n) {
        return None;
    }
    let (mut pairs, mut classes) = (None, None);
    for i in 0..n {
        let sub = data.get(48 + usize::from(word(data, 54 + i * 2)?)..)?;
        if word(sub, 4)? != 4 || word(sub, 6)? != 0 {
            return None;
        }
        match word(sub, 0)? {
            1 => {
                if pairs.is_some() || classes.is_some() {
                    return None;
                }
                pairs = Some(read_explicit(sub)?);
            }
            2 => {
                if classes.is_some() {
                    return None;
                }
                classes = Some(read_classes(sub)?);
            }
            _ => return None,
        }
    }
    Some((pairs.unwrap_or_default(), classes.unwrap_or_default()))
}
