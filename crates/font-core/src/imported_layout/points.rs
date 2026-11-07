//! Normalize outline-point anchors before contours are reconstructed or edited.
use super::*;
use std::collections::BTreeSet;
type IO<T> = crate::font_io::Result<T>;
fn word(b: &[u8], p: usize) -> IO<u16> {
    Ok(u16::from_be_bytes(
        b.get(p..p + 2)
            .ok_or("layout_invalid")?
            .try_into()
            .map_err(|_| "layout_invalid")?,
    ))
}
fn dword(b: &[u8], p: usize) -> IO<u32> {
    Ok(u32::from_be_bytes(
        b.get(p..p + 4)
            .ok_or("layout_invalid")?
            .try_into()
            .map_err(|_| "layout_invalid")?,
    ))
}
fn relative(b: &[u8], base: usize, field: usize) -> IO<Option<usize>> {
    let n = usize::from(word(b, field)?);
    Ok(if n == 0 {
        None
    } else {
        Some(
            base.checked_add(n)
                .filter(|&p| p < b.len())
                .ok_or("layout_invalid")?,
        )
    })
}
struct Anchors<'a> {
    bytes: &'a mut Vec<u8>,
    seen: BTreeSet<usize>,
    budget: usize,
    changed: bool,
}
impl Anchors<'_> {
    fn anchor(&mut self, p: usize) -> IO<()> {
        if !self.seen.insert(p) {
            return Ok(());
        }
        self.budget = self.budget.checked_sub(1).ok_or("layout_limit")?;
        match word(self.bytes, p)? {
            1 => {
                self.bytes.get(p..p + 6).ok_or("layout_invalid")?;
            }
            2 => {
                self.bytes.get(p..p + 8).ok_or("layout_invalid")?;
                self.bytes[p..p + 2].copy_from_slice(&1u16.to_be_bytes());
                self.changed = true;
            }
            3 => {
                self.bytes.get(p..p + 10).ok_or("layout_invalid")?;
            }
            _ => return Err("layout_invalid".into()),
        }
        Ok(())
    }
    fn offset(&mut self, base: usize, field: usize) -> IO<()> {
        self.budget = self.budget.checked_sub(1).ok_or("layout_limit")?;
        if let Some(p) = relative(self.bytes, base, field)? {
            self.anchor(p)?;
        }
        Ok(())
    }
    fn marks(&mut self, p: usize) -> IO<()> {
        let n = usize::from(word(self.bytes, p)?);
        self.bytes.get(p..p + 2 + n * 4).ok_or("layout_invalid")?;
        for i in 0..n {
            self.offset(p, p + 2 + i * 4 + 2)?;
        }
        Ok(())
    }
    fn array(&mut self, p: usize, classes: usize) -> IO<()> {
        let n = usize::from(word(self.bytes, p)?);
        let total = n.checked_mul(classes).ok_or("layout_invalid")?;
        self.bytes
            .get(p..p + 2 + total * 2)
            .ok_or("layout_invalid")?;
        if total > 100000 {
            return Err("layout_invalid".into());
        }
        for i in 0..total {
            self.offset(p, p + 2 + i * 2)?;
        }
        Ok(())
    }
    fn lookup(&mut self, kind: u16, p: usize) -> IO<()> {
        self.budget = self.budget.checked_sub(1).ok_or("layout_limit")?;
        if kind == 9 {
            if word(self.bytes, p)? != 1 {
                return Err("layout_invalid".into());
            }
            let kind = word(self.bytes, p + 2)?;
            if kind == 9 {
                return Err("layout_invalid".into());
            }
            let at = p
                .checked_add(dword(self.bytes, p + 4)? as usize)
                .filter(|&p| p < self.bytes.len())
                .ok_or("layout_invalid")?;
            return self.lookup(kind, at);
        }
        if !(3..=6).contains(&kind) {
            return Ok(());
        }
        if word(self.bytes, p)? != 1 {
            return Err("layout_invalid".into());
        }
        if kind == 3 {
            let n = usize::from(word(self.bytes, p + 4)?);
            self.bytes.get(p..p + 6 + n * 4).ok_or("layout_invalid")?;
            for i in 0..n {
                self.offset(p, p + 6 + i * 4)?;
                self.offset(p, p + 8 + i * 4)?;
            }
            return Ok(());
        }
        let classes = usize::from(word(self.bytes, p + 6)?);
        let marks = relative(self.bytes, p, p + 8)?.ok_or("layout_invalid")?;
        self.marks(marks)?;
        let bases = relative(self.bytes, p, p + 10)?.ok_or("layout_invalid")?;
        if kind == 5 {
            let n = usize::from(word(self.bytes, bases)?);
            self.bytes
                .get(bases..bases + 2 + n * 2)
                .ok_or("layout_invalid")?;
            for i in 0..n {
                let at = relative(self.bytes, bases, bases + 2 + i * 2)?.ok_or("layout_invalid")?;
                self.array(at, classes)?;
            }
        } else {
            self.array(bases, classes)?;
        }
        Ok(())
    }
}
fn gpos(bytes: &mut Vec<u8>) -> IO<bool> {
    let list = usize::from(word(bytes, 8)?);
    let count = usize::from(word(bytes, list)?);
    let mut a = Anchors {
        bytes,
        seen: BTreeSet::new(),
        budget: 100000,
        changed: false,
    };
    for i in 0..count {
        let p = relative(a.bytes, list, list + 2 + i * 2)?.ok_or("layout_invalid")?;
        let kind = word(a.bytes, p)?;
        let n = usize::from(word(a.bytes, p + 4)?);
        a.bytes.get(p..p + 6 + n * 2).ok_or("layout_invalid")?;
        for j in 0..n {
            let sub = relative(a.bytes, p, p + 6 + j * 2)?.ok_or("layout_invalid")?;
            a.lookup(kind, sub)?;
        }
    }
    Ok(a.changed)
}
pub(crate) fn coverage(b: &[u8], p: usize) -> IO<Vec<u16>> {
    let n = usize::from(word(b, p + 2)?);
    let mut out = vec![];
    match word(b, p)? {
        1 => {
            for i in 0..n {
                out.push(word(b, p + 4 + i * 2)?);
            }
        }
        2 => {
            for i in 0..n {
                let start = word(b, p + 4 + i * 6)?;
                let end = word(b, p + 6 + i * 6)?;
                let index = usize::from(word(b, p + 8 + i * 6)?);
                if start > end
                    || index != out.len()
                    || out.len() + usize::from(end - start) + 1 > 65535
                {
                    return Err("layout_invalid".into());
                }
                out.extend(start..=end);
            }
        }
        _ => return Err("layout_invalid".into()),
    }
    if out.windows(2).any(|p| p[0] >= p[1]) {
        return Err("layout_invalid".into());
    }
    Ok(out)
}
fn gdef(bytes: &mut Vec<u8>, source: &[u8], index: u32, native_origin: bool) -> IO<bool> {
    let Some(list) = relative(bytes, 0, 8)? else {
        return Ok(false);
    };
    let cp = relative(bytes, list, list)?.ok_or("layout_invalid")?;
    let glyphs = coverage(bytes, cp)?;
    let count = usize::from(word(bytes, list + 2)?);
    if count != glyphs.len() {
        return Err("layout_invalid".into());
    }
    let mut native = None;
    let mut shapes = None;
    let is_glyf = ttf_parser::RawFace::parse(source, index)
        .map_err(|_| "layout_invalid")?
        .table(ttf_parser::Tag::from_bytes(b"glyf"))
        .is_some();
    let mut total_points = 0usize;
    let mut coordinates = BTreeMap::<u16, Vec<i32>>::new();
    let mut rows = vec![];
    let mut changed = false;
    let mut budget = 100000usize;
    for (i, &glyph) in glyphs.iter().enumerate() {
        let g = relative(bytes, list, list + 4 + i * 2)?.ok_or("layout_invalid")?;
        let n = usize::from(word(bytes, g)?);
        bytes.get(g..g + 2 + n * 2).ok_or("layout_invalid")?;
        let mut row = vec![0u8; 2 + n * 2];
        row[..2].copy_from_slice(&(n as u16).to_be_bytes());
        for j in 0..n {
            budget = budget.checked_sub(1).ok_or("layout_limit")?;
            let caret = relative(bytes, g, g + 2 + j * 2)?.ok_or("layout_invalid")?;
            let position = u16::try_from(row.len()).map_err(|_| "layout_limit")?;
            row[2 + j * 2..4 + j * 2].copy_from_slice(&position.to_be_bytes());
            match word(bytes, caret)? {
                1 => {
                    row.extend(bytes.get(caret..caret + 4).ok_or("layout_invalid")?);
                }
                2 => {
                    if let std::collections::btree_map::Entry::Vacant(entry) =
                        coordinates.entry(glyph)
                    {
                        if is_glyf {
                            if shapes.is_none() {
                                shapes = Some(crate::truetype_components::decode(source, index)?);
                            }
                            let shape = shapes
                                .as_ref()
                                .and_then(|s| s.get(&glyph))
                                .ok_or("layout_invalid")?;
                            entry.insert(
                                shape
                                    .rings
                                    .iter()
                                    .flatten()
                                    .map(|(p, _)| p[0].round() as i32)
                                    .collect(),
                            );
                        } else {
                            if native.is_none() {
                                let library = freetype::Library::init()
                                    .map_err(|_| "layout_anchor_unsupported")?;
                                let face = library
                                    .new_memory_face(source.to_vec(), index as isize)
                                    .map_err(|_| "layout_anchor_unsupported")?;
                                native = Some((library, face));
                            }
                            let face = &native.as_ref().ok_or("layout_invalid")?.1;
                            face.load_glyph(
                                u32::from(glyph),
                                freetype::face::LoadFlag::NO_SCALE
                                    | freetype::face::LoadFlag::NO_HINTING
                                    | freetype::face::LoadFlag::NO_BITMAP,
                            )
                            .map_err(|_| "layout_anchor_unsupported")?;
                            let outline = face.glyph().outline().ok_or("layout_invalid")?;
                            total_points += outline.points().len();
                            if total_points > 1_000_000 {
                                return Err("layout_limit".into());
                            }
                            let points = outline
                                .points()
                                .iter()
                                .map(|p| {
                                    i32::try_from(p.x)
                                        .map_err(|_| "layout_anchor_unsupported".into())
                                })
                                .collect::<IO<Vec<_>>>()?;
                            entry.insert(points);
                        }
                    }
                    let point = usize::from(word(bytes, caret + 2)?);
                    let mut x = *coordinates[&glyph].get(point).ok_or("layout_invalid")?;
                    if is_glyf && native_origin {
                        let face =
                            ttf_parser::Face::parse(source, index).map_err(|_| "layout_invalid")?;
                        let id = ttf_parser::GlyphId(glyph);
                        if let (Some(b), Some(lsb)) =
                            (face.glyph_bounding_box(id), face.glyph_hor_side_bearing(id))
                        {
                            x += i32::from(lsb) - i32::from(b.x_min);
                        }
                    }

                    let x = i16::try_from(x).map_err(|_| "layout_anchor_unsupported")?;
                    row.extend(1u16.to_be_bytes());
                    row.extend(x.to_be_bytes());
                    changed = true;
                }
                3 => {
                    row.extend(3u16.to_be_bytes());
                    row.extend(word(bytes, caret + 2)?.to_be_bytes());
                    if let Some(device) = relative(bytes, caret, caret + 4)? {
                        let start = word(bytes, device)?;
                        let end = word(bytes, device + 2)?;
                        let format = word(bytes, device + 4)?;
                        let size = if format == 0x8000 {
                            6
                        } else {
                            if !(1..=3).contains(&format) || start > end {
                                return Err("layout_invalid".into());
                            }
                            6 + ((usize::from(end - start) + 1) * (1usize << format)).div_ceil(16)
                                * 2
                        };
                        let data = bytes.get(device..device + size).ok_or("layout_invalid")?;
                        row.extend(6u16.to_be_bytes());
                        row.extend(data);
                    } else {
                        row.extend(0u16.to_be_bytes());
                    }
                }
                _ => return Err("layout_invalid".into()),
            }
        }
        rows.push(row);
    }
    if !changed {
        return Ok(false);
    }
    let coverage_at = 4 + glyphs.len() * 2;
    let mut body = vec![0u8; coverage_at];
    body[..2].copy_from_slice(
        &u16::try_from(coverage_at)
            .map_err(|_| "layout_limit")?
            .to_be_bytes(),
    );
    body[2..4].copy_from_slice(&(glyphs.len() as u16).to_be_bytes());
    body.extend(1u16.to_be_bytes());
    body.extend((glyphs.len() as u16).to_be_bytes());
    for &id in &glyphs {
        body.extend(id.to_be_bytes());
    }
    let mut pool = BTreeMap::new();
    for (i, row) in rows.into_iter().enumerate() {
        let at = if let Some(&p) = pool.get(&row) {
            p
        } else {
            let p = u16::try_from(body.len()).map_err(|_| "layout_limit")?;
            body.extend(&row);
            pool.insert(row, p);
            p
        };
        body[4 + i * 2..6 + i * 2].copy_from_slice(&at.to_be_bytes());
    }
    let version = dword(bytes, 0)?;
    let header = match version {
        0x10000 => 12,
        0x10002 => 14,
        0x10003 => 18,
        _ => return Err("layout_invalid".into()),
    };
    let mut out = bytes.get(..header).ok_or("layout_invalid")?.to_vec();
    for field in [4, 6, 10, 12] {
        if field >= header || (field == 12 && header == 12) {
            continue;
        }
        let old = word(bytes, field)?;
        if old != 0 {
            let offset = usize::from(old) + body.len();
            out[field..field + 2].copy_from_slice(
                &u16::try_from(offset)
                    .map_err(|_| "layout_limit")?
                    .to_be_bytes(),
            );
        }
    }
    if version == 0x10003 {
        let at = dword(bytes, 14)?;
        if at != 0 {
            out[14..18].copy_from_slice(
                &at.checked_add(body.len() as u32)
                    .ok_or("layout_limit")?
                    .to_be_bytes(),
            );
        }
    }
    out[8..10].copy_from_slice(&(header as u16).to_be_bytes());
    out.extend(body);
    out.extend(&bytes[header..]);
    if out.len() > super::LIMIT {
        return Err("layout_limit".into());
    }
    *bytes = out;
    Ok(true)
}
fn base(bytes: &mut Vec<u8>) -> IO<bool> {
    let mut a = Anchors {
        bytes,
        seen: BTreeSet::new(),
        budget: 100000,
        changed: false,
    };
    // BASE coordinates are shorter than GPOS anchors but the explicit coordinate stays in place.
    let mut coords = BTreeSet::new();
    let mut minmax = BTreeSet::new();
    let mut scripts_seen = BTreeSet::new();
    let mut work = 0usize;
    for field in [4, 6] {
        if let Some(axis) = relative(a.bytes, 0, field)? {
            if let Some(scripts) = relative(a.bytes, axis, axis + 2)? {
                let n = usize::from(word(a.bytes, scripts)?);
                a.bytes
                    .get(scripts..scripts + 2 + n * 6)
                    .ok_or("layout_invalid")?;
                for i in 0..n {
                    let script =
                        relative(a.bytes, scripts, scripts + 6 + i * 6)?.ok_or("layout_invalid")?;
                    work += 1;
                    if work > 100000 {
                        return Err("layout_limit".into());
                    }
                    if !scripts_seen.insert(script) {
                        continue;
                    }
                    if let Some(values) = relative(a.bytes, script, script)? {
                        let n = usize::from(word(a.bytes, values + 2)?);
                        a.bytes
                            .get(values..values + 4 + n * 2)
                            .ok_or("layout_invalid")?;
                        for j in 0..n {
                            work += 1;
                            if work > 100000 {
                                return Err("layout_limit".into());
                            }
                            if let Some(c) = relative(a.bytes, values, values + 4 + j * 2)? {
                                coords.insert(c);
                            }
                        }
                    }
                    if let Some(m) = relative(a.bytes, script, script + 2)? {
                        minmax.insert(m);
                    }
                    let n = usize::from(word(a.bytes, script + 4)?);
                    a.bytes
                        .get(script..script + 6 + n * 6)
                        .ok_or("layout_invalid")?;
                    for j in 0..n {
                        work += 1;
                        if work > 100000 {
                            return Err("layout_limit".into());
                        }
                        if let Some(m) = relative(a.bytes, script, script + 10 + j * 6)? {
                            minmax.insert(m);
                        }
                    }
                }
            }
        }
    }
    for m in minmax {
        for field in [m, m + 2] {
            if let Some(c) = relative(a.bytes, m, field)? {
                coords.insert(c);
            }
        }
        let n = usize::from(word(a.bytes, m + 4)?);
        a.bytes.get(m..m + 6 + n * 8).ok_or("layout_invalid")?;
        for i in 0..n {
            work += 1;
            if work > 100000 {
                return Err("layout_limit".into());
            }
            for field in [m + 10 + i * 8, m + 12 + i * 8] {
                if let Some(c) = relative(a.bytes, m, field)? {
                    coords.insert(c);
                }
            }
        }
    }
    if coords.len() > 100000 {
        return Err("layout_invalid".into());
    }
    for p in coords {
        match word(a.bytes, p)? {
            1 => {
                a.bytes.get(p..p + 4).ok_or("layout_invalid")?;
            }
            2 => {
                a.bytes.get(p..p + 8).ok_or("layout_invalid")?;
                a.bytes[p..p + 2].copy_from_slice(&1u16.to_be_bytes());
                a.changed = true;
            }
            3 => {
                a.bytes.get(p..p + 6).ok_or("layout_invalid")?;
            }
            _ => return Err("layout_invalid".into()),
        }
    }
    Ok(a.changed)
}
pub(super) fn normalize(
    layout: &mut ImportedLayout,
    source: &[u8],
    index: u32,
    native_origin: bool,
) -> IO<()> {
    if let Some(table) = layout.tables.get_mut("GPOS") {
        layout.normalized_points |= gpos(Arc::make_mut(&mut table.data))?;
    }
    if let Some(table) = layout.tables.get_mut("GDEF") {
        layout.normalized_points |=
            gdef(Arc::make_mut(&mut table.data), source, index, native_origin)?;
    }
    if let Some(table) = layout.tables.get_mut("BASE") {
        layout.normalized_points |= base(Arc::make_mut(&mut table.data))?;
    }
    Ok(())
}

pub(crate) fn materialize_edit_anchors(
    tables: &mut BTreeMap<[u8; 4], Vec<u8>>,
) -> crate::Result<()> {
    if let Some(bytes) = tables.get_mut(b"GPOS") {
        gpos(bytes).map_err(|_| crate::Error::InvalidFile)?;
    }
    if let Some(bytes) = tables.get_mut(b"BASE") {
        base(bytes).map_err(|_| crate::Error::InvalidFile)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shared_point_carets_are_materialized_separately_for_each_glyph() {
        let mut b = vec![];
        b.extend(0x10000u32.to_be_bytes());
        for n in [0u16, 0, 12, 0, 8, 2, 16, 16, 1, 2, 2, 6, 1, 4, 2, 1] {
            b.extend(n.to_be_bytes());
        }
        let original = b.clone();
        let source = include_bytes!("../../tests/data/layout/shaping.ttf");
        let cff = crate::font_formats::export(
            &crate::font_io::import_outlines(source, 0).unwrap(),
            crate::font_formats::Format::Otf,
        )
        .unwrap();
        for source in [source.as_slice(), cff.as_slice()] {
            b = original.clone();
            assert!(gdef(&mut b, source, 0, false).unwrap());
            let list = usize::from(word(&b, 8).unwrap());
            let mut coords = vec![];
            for i in 0..2 {
                let glyph = relative(&b, list, list + 4 + i * 2).unwrap().unwrap();
                let caret = relative(&b, glyph, glyph + 2).unwrap().unwrap();
                assert_eq!(word(&b, caret).unwrap(), 1);
                coords.push(word(&b, caret + 2).unwrap() as i16);
            }
            assert_eq!(coords, vec![320, 360]);
        }
    }
    #[test]
    fn base_point_anchor_keeps_design_coordinate_without_stale_point_index() {
        let mut b = 0x10000u32.to_be_bytes().to_vec();
        for n in [8u16, 0, 4, 10, 1] {
            b.extend(n.to_be_bytes());
        }
        b.extend(b"romn");
        b.extend(1u16.to_be_bytes());
        b.extend(b"latn");
        for n in [8u16, 6, 0, 0, 0, 1, 6, 2, 12, 2, 1] {
            b.extend(n.to_be_bytes());
        }
        assert!(base(&mut b).unwrap());
        assert_eq!(word(&b, 38).unwrap(), 1);
        assert_eq!(word(&b, 40).unwrap(), 12);
    }
}
