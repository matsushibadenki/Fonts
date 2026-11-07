use super::*;
use crate::source_font::master_gvar;
pub(super) struct Initialized {
    pub table: Vec<u8>,
    pub base: BTreeMap<u16, i32>,
}
pub(super) struct Glyph {
    pub(super) points: usize,
    pub(super) inherit: Option<u16>,
    pub(super) advance: u16,
}
#[derive(Clone)]
struct Function {
    advance: u16,
    regions: Vec<(Support, f64)>,
}
fn word(bytes: &[u8], at: usize) -> Result<u16> {
    Ok(u16::from_be_bytes(
        bytes
            .get(at..at + 2)
            .ok_or(Error::InvalidFile)?
            .try_into()
            .map_err(|_| Error::InvalidFile)?,
    ))
}
pub(super) fn create(
    face: &ttf_parser::Face<'_>,
    axes: &[String],
    vertical: bool,
) -> Result<Option<Initialized>> {
    let count = face.number_of_glyphs();
    if count == 0 || axes.is_empty() || axes.len() > 63 {
        return Err(Error::InvalidFile);
    }
    let metadata = metadata(face, vertical)?;
    let raw = face.raw_face();
    let gvar = raw
        .table(ttf_parser::Tag::from_bytes(b"gvar"))
        .map(|bytes| master_gvar::Table::parse(bytes, axes, count))
        .transpose()?;
    let mut resolver = Resolver {
        metadata: &metadata,
        gvar: gvar.as_ref(),
        vertical,
        cache: vec![None; usize::from(count)],
        active: vec![false; usize::from(count)],
        total: 0,
        origin_motion: false,
    };
    for id in 0..count {
        resolver.resolve(id, 0)?;
    }
    if resolver.origin_motion {
        return Ok(None);
    }
    let functions = resolver.cache;
    // A builder can start with no rows; ensure the finished store has at least one for FreeType.
    let mut seed = 1u16.to_be_bytes().to_vec();
    seed.extend(8u32.to_be_bytes());
    seed.extend(0u16.to_be_bytes());
    seed.extend((axes.len() as u16).to_be_bytes());
    seed.extend(0u16.to_be_bytes());
    let mut builder = Builder::new(Store::parse(&seed, axes)?);
    let mut cache = BTreeMap::<Vec<(Vec<[i16; 3]>, i32)>, master_store::Index>::new();
    let mut mapping = vec![(u16::MAX, u16::MAX); usize::from(count)];
    let mut base = BTreeMap::new();
    for (id, function) in functions.into_iter().enumerate() {
        let function = function.ok_or(Error::InvalidFile)?;
        let correction = i32::from(function.advance) - i32::from(metadata[id].advance);
        if correction != 0 {
            base.insert(id as u16, correction);
        }
        if function.regions.is_empty() {
            continue;
        }
        let key: Vec<_> = function
            .regions
            .iter()
            .map(|(support, value)| {
                (
                    axes.iter()
                        .map(|axis| {
                            support
                                .axes
                                .get(axis)
                                .copied()
                                .unwrap_or([0.; 3])
                                .map(|v| (v * 16384.) as i16)
                        })
                        .collect(),
                    *value as i32,
                )
            })
            .collect();
        let index = if let Some(index) = cache.get(&key) {
            *index
        } else {
            let (index, errors) = builder.add((u16::MAX, u16::MAX), &function.regions)?;
            if !errors.is_empty() {
                return Err(Error::InvalidFile);
            }
            cache.insert(key, index);
            index
        };
        mapping[id] = index;
    }
    if cache.is_empty() {
        builder.add((u16::MAX, u16::MAX), &[])?;
    }
    let store = builder.finish()?;
    let header = if vertical { 24 } else { 20 };
    let mut table = vec![0; header];
    table[..4].copy_from_slice(&[0, 1, 0, 0]);
    table[4..8].copy_from_slice(&(header as u32).to_be_bytes());
    table.extend(store);
    let map_at = table.len();
    table.extend(master_store::write_map(&mapping)?);
    table[8..12].copy_from_slice(&(map_at as u32).to_be_bytes());
    if table.len() > 256 * 1024 * 1024 {
        return Err(Error::TooLarge);
    }
    Ok(Some(Initialized { table, base }))
}
pub(super) fn metadata(face: &ttf_parser::Face<'_>, vertical: bool) -> Result<Vec<Glyph>> {
    let count = face.number_of_glyphs();
    let raw = face.raw_face();
    let glyf = raw.table(ttf_parser::Tag::from_bytes(b"glyf"));
    let mut metadata = Vec::with_capacity(usize::from(count));
    if let Some(glyf) = glyf {
        let head = raw
            .table(ttf_parser::Tag::from_bytes(b"head"))
            .ok_or(Error::InvalidFile)?;
        let loca = raw
            .table(ttf_parser::Tag::from_bytes(b"loca"))
            .ok_or(Error::InvalidFile)?;
        let long = match word(head, 50)? {
            0 => false,
            1 => true,
            _ => return Err(Error::InvalidFile),
        };
        let at = |glyph: usize| -> Result<usize> {
            if long {
                offset(loca, glyph * 4)
            } else {
                Ok(usize::from(word(loca, glyph * 2)?) * 2)
            }
        };
        let mut previous = at(0)?;
        for id in 0..count {
            let next = at(usize::from(id) + 1)?;
            let bytes = glyf.get(previous..next).ok_or(Error::InvalidFile)?;
            previous = next;
            let mut points = 0usize;
            let mut inherit = None;
            if !bytes.is_empty() {
                let contours = word(bytes, 0)? as i16;
                if contours > 0 {
                    points = usize::from(word(bytes, 10 + (contours as usize - 1) * 2)?) + 1;
                } else if contours == -1 {
                    let mut p = 10usize;
                    loop {
                        if points >= 4096 {
                            return Err(Error::TooLarge);
                        }
                        let flags = word(bytes, p)?;
                        let child = word(bytes, p + 2)?;
                        if child >= count {
                            return Err(Error::InvalidFile);
                        }
                        if flags & 0x200 != 0 {
                            inherit = Some(child);
                        }
                        points += 1;
                        p += 4 + if flags & 1 != 0 { 4 } else { 2 };
                        p += if flags & 8 != 0 {
                            2
                        } else if flags & 64 != 0 {
                            4
                        } else if flags & 128 != 0 {
                            8
                        } else {
                            0
                        };
                        if p > bytes.len() {
                            return Err(Error::InvalidFile);
                        }
                        if flags & 32 == 0 {
                            break;
                        }
                    }
                } else if contours < 0 {
                    return Err(Error::InvalidFile);
                }
            }
            let gid = ttf_parser::GlyphId(id);
            let advance = if vertical {
                face.glyph_ver_advance(gid)
            } else {
                face.glyph_hor_advance(gid)
            }
            .ok_or(Error::InvalidFile)?;
            metadata.push(Glyph {
                points,
                inherit,
                advance,
            });
        }
    } else {
        // CFF2 has no phantom-point metric variations: without HVAR/VVAR advances are constant.
        if raw.table(ttf_parser::Tag::from_bytes(b"CFF2")).is_none() {
            return Err(Error::InvalidFile);
        }
        for id in 0..count {
            let gid = ttf_parser::GlyphId(id);
            let advance = if vertical {
                face.glyph_ver_advance(gid)
            } else {
                face.glyph_hor_advance(gid)
            }
            .ok_or(Error::InvalidFile)?;
            metadata.push(Glyph {
                points: 0,
                inherit: None,
                advance,
            });
        }
    }
    Ok(metadata)
}
struct Resolver<'a, 'b> {
    metadata: &'a [Glyph],
    gvar: Option<&'a master_gvar::Table<'b>>,
    vertical: bool,
    cache: Vec<Option<Function>>,
    active: Vec<bool>,
    total: usize,
    origin_motion: bool,
}
impl Resolver<'_, '_> {
    fn resolve(&mut self, id: u16, depth: usize) -> Result<()> {
        let index = usize::from(id);
        if self.cache[index].is_some() {
            return Ok(());
        }
        if depth >= 64 || self.active[index] {
            return Err(Error::InvalidFile);
        }
        self.active[index] = true;
        let glyph = &self.metadata[index];
        let (inherit, points, advance) = (glyph.inherit, glyph.points, glyph.advance);
        let function = if let Some(child) = inherit {
            self.resolve(child, depth + 1)?;
            self.cache[usize::from(child)]
                .as_ref()
                .ok_or(Error::InvalidFile)?
                .clone()
        } else {
            let mut regions = vec![];
            if let Some(gvar) = self.gvar {
                for tuple in gvar.glyph(id, points + 4)? {
                    let delta = |point: usize, axis: usize| -> i32 {
                        let index = tuple
                            .points
                            .as_ref()
                            .map_or(Some(point), |indices| indices.binary_search(&point).ok());
                        index
                            .and_then(|i| tuple.deltas.get(i))
                            .map_or(0, |p| i32::from(p[axis]))
                    };
                    // A metric table disables phantom-origin motion in rasterizers.
                    // Keep these sources in the original gvar domain for advance corrections.
                    let origin = if self.vertical {
                        delta(points + 2, 1)
                    } else {
                        delta(points, 0)
                    };
                    self.origin_motion |= origin != 0;
                    let value = if self.vertical {
                        delta(points + 2, 1) - delta(points + 3, 1)
                    } else {
                        delta(points + 1, 0) - delta(points, 0)
                    };
                    if value != 0 {
                        regions.push((tuple.support, f64::from(value)));
                    }
                }
            }
            Function { advance, regions }
        };
        self.total = self
            .total
            .checked_add(function.regions.len())
            .ok_or(Error::TooLarge)?;
        if self.total > 2_000_000 {
            return Err(Error::TooLarge);
        }
        self.active[index] = false;
        self.cache[index] = Some(function);
        Ok(())
    }
}
