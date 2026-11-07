//! Migrate native component point attachments through stable node identities.
use super::*;
use crate::source_font::master_cff::codec::Reader;
#[derive(Clone)]
struct Part {
    flags: u16,
    child: u16,
    args: [i32; 2],
    transform: Vec<u8>,
}
fn parts(bytes: &[u8]) -> Result<(Vec<Part>, usize)> {
    let mut r = Reader { bytes, at: 10 };
    let mut out = vec![];
    loop {
        if out.len() >= 4096 {
            return Err(Error::TooLarge);
        }
        let flags = r.word()?;
        let child = r.word()?;
        let xy = flags & 2 != 0;
        let args = if flags & 1 != 0 {
            let a = r.word()?;
            let b = r.word()?;
            if xy {
                [i32::from(a as i16), i32::from(b as i16)]
            } else {
                [i32::from(a), i32::from(b)]
            }
        } else {
            let a = r.byte()?;
            let b = r.byte()?;
            if xy {
                [i32::from(a as i8), i32::from(b as i8)]
            } else {
                [i32::from(a), i32::from(b)]
            }
        };
        let transform = r
            .take(if flags & 8 != 0 {
                2
            } else if flags & 64 != 0 {
                4
            } else if flags & 128 != 0 {
                8
            } else {
                0
            })?
            .to_vec();
        out.push(Part {
            flags,
            child,
            args,
            transform,
        });
        if flags & 32 == 0 {
            break;
        }
    }
    Ok((out, r.at))
}
#[derive(Clone)]
struct Mapping {
    points: Vec<Option<usize>>,
    count: usize,
}
struct Resolver<'a> {
    bytes: &'a [u8],
    plans: &'a BTreeMap<u16, Rebased>,
    records: Vec<&'a [u8]>,
    cache: BTreeMap<u16, Mapping>,
    active: BTreeSet<u16>,
    budget: usize,
}
impl Resolver<'_> {
    fn get(&mut self, id: u16) -> Result<Mapping> {
        if let Some(m) = self.cache.get(&id) {
            return Ok(m.clone());
        }
        if self.active.len() >= 16 || !self.active.insert(id) {
            return Err(Error::InvalidFile);
        }
        let map = if let Some(plan) = self.plans.get(&id).filter(|p| p.base.remap.is_some()) {
            let points = plan.base.remap.clone().ok_or(Error::InvalidGeometry)?;
            if points
                .iter()
                .flatten()
                .any(|&i| i >= plan.base.points.len())
            {
                return Err(Error::InvalidGeometry);
            }
            Mapping {
                points,
                count: plan.base.points.len(),
            }
        } else if let Some(g) = master_glyf::read(self.bytes, 0, id)? {
            if self
                .plans
                .get(&id)
                .is_some_and(|p| p.base.points.len() != g.points.len())
            {
                return Err(Error::InvalidGeometry);
            }
            Mapping {
                points: (0..g.points.len()).map(Some).collect(),
                count: g.points.len(),
            }
        } else {
            let (parts, _) = parts(self.records[usize::from(id)])?;
            let mut points = vec![];
            let mut count = 0;
            for part in parts {
                let m = self.get(part.child)?;
                points.extend(m.points.into_iter().map(|v| v.map(|i| i + count)));
                count += m.count;
            }
            Mapping { points, count }
        };
        self.active.remove(&id);
        self.budget += map.points.len();
        if self.budget > 4_000_000 {
            return Err(Error::TooLarge);
        }
        self.cache.insert(id, map.clone());
        Ok(map)
    }
}
pub(super) fn apply(
    bytes: &[u8],
    plans: &BTreeMap<u16, Rebased>,
    records: &mut [Vec<u8>],
    affected: &BTreeSet<u16>,
    tables: &mut Tables,
) -> Result<()> {
    if !plans.values().any(|p| p.base.remap.is_some()) {
        return Ok(());
    }
    let face = ttf_parser::Face::parse(bytes, 0).map_err(|_| Error::InvalidFile)?;
    let raw = face.raw_face();
    let original_tables: Tables = raw
        .table_records
        .into_iter()
        .map(|r| {
            Ok((
                r.tag.to_bytes(),
                raw.table(r.tag).ok_or(Error::InvalidFile)?.to_vec(),
            ))
        })
        .collect::<Result<_>>()?;
    let locations = offsets(&original_tables, records.len())?;
    let glyf = raw
        .table(ttf_parser::Tag::from_bytes(b"glyf"))
        .ok_or(Error::InvalidFile)?;
    let old_records = locations.windows(2).map(|w| &glyf[w[0]..w[1]]).collect();
    let mut resolver = Resolver {
        bytes,
        plans,
        records: old_records,
        cache: BTreeMap::new(),
        active: BTreeSet::new(),
        budget: 0,
    };
    for &id in affected {
        if plans.contains_key(&id) || records[usize::from(id)].get(..2) != Some(&[255, 255]) {
            continue;
        }
        let original = resolver.records[usize::from(id)];
        let (parts, end) = parts(original)?;
        let mut out = original[..10].to_vec();
        let mut prefix = Vec::<Option<usize>>::new();
        let mut count = 0;
        for part in parts {
            let map = resolver.get(part.child)?;
            let mut args = part.args;
            let mut flags = part.flags;
            if flags & 2 == 0 {
                args[0] = i32::try_from(
                    prefix
                        .get(args[0] as usize)
                        .copied()
                        .flatten()
                        .ok_or(Error::InvalidGeometry)?,
                )
                .map_err(|_| Error::TooLarge)?;
                args[1] = i32::try_from(
                    map.points
                        .get(args[1] as usize)
                        .copied()
                        .flatten()
                        .ok_or(Error::InvalidGeometry)?,
                )
                .map_err(|_| Error::TooLarge)?;
                flags |= 1;
            }
            out.extend(flags.to_be_bytes());
            out.extend(part.child.to_be_bytes());
            if flags & 1 != 0 {
                for arg in args {
                    let value = if flags & 2 != 0 {
                        i16::try_from(arg).map_err(|_| Error::InvalidGeometry)? as u16
                    } else {
                        u16::try_from(arg).map_err(|_| Error::InvalidGeometry)?
                    };
                    out.extend(value.to_be_bytes());
                }
            } else {
                out.extend(args.map(|a| a as u8));
            }
            out.extend(part.transform);
            prefix.extend(map.points.into_iter().map(|v| v.map(|i| i + count)));
            count += map.count;
        }
        out.extend_from_slice(&original[end..]);
        records[usize::from(id)] = out;
    }
    super::layout::apply(tables, |glyph, point| {
        resolver
            .get(glyph)?
            .points
            .get(point)
            .copied()
            .flatten()
            .ok_or(Error::InvalidGeometry)
    })?;
    Ok(())
}
