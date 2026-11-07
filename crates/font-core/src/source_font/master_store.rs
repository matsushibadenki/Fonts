//! Bounded ItemVariationStore and DeltaSetIndexMap access for metric edits.
use super::masters::{Location, Support};
use crate::{Error, Result};
use std::collections::BTreeMap;
pub type Index = (u16, u16);
struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}
impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self.at.checked_add(n).ok_or(Error::InvalidFile)?;
        let out = self.bytes.get(self.at..end).ok_or(Error::InvalidFile)?;
        self.at = end;
        Ok(out)
    }
    fn byte(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }
    fn word(&mut self) -> Result<u16> {
        Ok(u16::from_be_bytes(
            self.take(2)?.try_into().map_err(|_| Error::InvalidFile)?,
        ))
    }
    fn long(&mut self) -> Result<u32> {
        Ok(u32::from_be_bytes(
            self.take(4)?.try_into().map_err(|_| Error::InvalidFile)?,
        ))
    }
}
pub fn read_map(bytes: &[u8], glyphs: u16) -> Result<Vec<Index>> {
    let mut r = Reader { bytes, at: 0 };
    let format = r.byte()?;
    let entry = r.byte()?;
    if entry & 0xc0 != 0 {
        return Err(Error::InvalidFile);
    }
    let count = match format {
        0 => usize::from(r.word()?),
        1 => r.long()? as usize,
        _ => return Err(Error::InvalidFile),
    };
    if count == 0 || count > 1_000_000 {
        return Err(Error::InvalidFile);
    }
    let size = usize::from((entry >> 4) & 3) + 1;
    let bits = u32::from(entry & 15) + 1;
    if bits > (size * 8) as u32 {
        return Err(Error::InvalidFile);
    }
    let mask = (1u32 << bits) - 1;
    let packed = r.take(count.checked_mul(size).ok_or(Error::InvalidFile)?)?;
    let mut out = Vec::with_capacity(usize::from(glyphs));
    for glyph in 0..usize::from(glyphs) {
        let start = glyph.min(count - 1) * size;
        let value = packed[start..start + size]
            .iter()
            .fold(0u32, |v, b| (v << 8) | u32::from(*b));
        let outer = u16::try_from(value >> bits).map_err(|_| Error::InvalidFile)?;
        out.push((outer, (value & mask) as u16));
    }
    Ok(out)
}
pub fn write_map(indices: &[Index]) -> Result<Vec<u8>> {
    let count = u16::try_from(indices.len()).map_err(|_| Error::TooLarge)?;
    if count == 0 {
        return Err(Error::InvalidGeometry);
    }
    let mut out = vec![0, 0x3f];
    out.extend(count.to_be_bytes());
    for &(outer, inner) in indices {
        out.extend(((u32::from(outer) << 16) | u32::from(inner)).to_be_bytes());
    }
    Ok(out)
}
pub struct Store<'a> {
    bytes: &'a [u8],
    axes: Vec<String>,
    regions: Vec<Vec<[i16; 3]>>,
    offsets: Vec<usize>,
    header: usize,
}
impl<'a> Store<'a> {
    pub fn region_count(&self) -> usize {
        self.regions.len()
    }
    pub fn regions_for_data(&self, index: u16) -> Result<Vec<u16>> {
        let at = *self
            .offsets
            .get(usize::from(index))
            .ok_or(Error::InvalidFile)?;
        let mut r = Reader {
            bytes: self.bytes,
            at,
        };
        r.word()?;
        r.word()?;
        let count = usize::from(r.word()?);
        let mut out = Vec::with_capacity(count);
        for _ in 0..count {
            let region = r.word()?;
            if usize::from(region) >= self.regions.len() {
                return Err(Error::InvalidFile);
            }
            out.push(region);
        }
        Ok(out)
    }

    pub fn support(&self, index: u16) -> Result<Support> {
        let region = self
            .regions
            .get(usize::from(index))
            .ok_or(Error::InvalidFile)?;
        let mut support = Support::default();
        for (tag, &[start, peak, end]) in self.axes.iter().zip(region) {
            if peak != 0 && start <= peak && peak <= end && !(start < 0 && end > 0) {
                support.axes.insert(
                    tag.clone(),
                    [
                        f32::from(start) / 16384.,
                        f32::from(peak) / 16384.,
                        f32::from(end) / 16384.,
                    ],
                );
            }
        }
        Ok(support)
    }
    pub fn parse(bytes: &'a [u8], axes: &[String]) -> Result<Self> {
        if bytes.len() > 256 * 1024 * 1024 || axes.len() > 63 {
            return Err(Error::TooLarge);
        }
        if axes.is_empty()
            || axes.iter().any(|s| s.len() != 4 || !s.is_ascii())
            || axes.iter().collect::<std::collections::BTreeSet<_>>().len() != axes.len()
        {
            return Err(Error::InvalidFile);
        }
        let mut r = Reader { bytes, at: 0 };
        if r.word()? != 1 {
            return Err(Error::InvalidFile);
        }
        let region_at = r.long()? as usize;
        let count = usize::from(r.word()?);
        let offsets = (0..count)
            .map(|_| r.long().map(|v| v as usize))
            .collect::<Result<Vec<_>>>()?;
        let header = r.at;
        if region_at < header {
            return Err(Error::InvalidFile);
        }
        r.at = region_at;
        if usize::from(r.word()?) != axes.len() {
            return Err(Error::InvalidFile);
        }
        let count = usize::from(r.word()?);
        if count * axes.len() > 1_000_000 {
            return Err(Error::TooLarge);
        }
        if count >= 32768 {
            return Err(Error::InvalidFile);
        }
        let mut regions = Vec::with_capacity(count);
        for _ in 0..count {
            let mut region = Vec::with_capacity(axes.len());
            for _ in axes {
                let v = [r.word()? as i16, r.word()? as i16, r.word()? as i16];
                if v.iter().any(|v| !(-16384..=16384).contains(v)) {
                    return Err(Error::InvalidFile);
                }
                region.push(v);
            }
            regions.push(region);
        }
        let mut end = r.at;
        for &at in &offsets {
            if at < header {
                return Err(Error::InvalidFile);
            }
            r.at = at;
            let items = usize::from(r.word()?);
            let words = r.word()?;
            let region_count = usize::from(r.word()?);
            let wide = usize::from(words & 0x7fff);
            if wide > region_count {
                return Err(Error::InvalidFile);
            }
            for _ in 0..region_count {
                if usize::from(r.word()?) >= regions.len() {
                    return Err(Error::InvalidFile);
                }
            }
            let stride = if words & 0x8000 != 0 {
                wide * 4 + (region_count - wide) * 2
            } else {
                wide * 2 + region_count - wide
            };
            r.take(items.checked_mul(stride).ok_or(Error::InvalidFile)?)?;
            end = end.max(r.at);
        }
        Ok(Self {
            bytes: &bytes[..end],
            axes: axes.to_vec(),
            regions,
            offsets,
            header,
        })
    }
    pub fn row(&self, index: Index) -> Result<Vec<(u16, i32)>> {
        if index == (u16::MAX, u16::MAX) {
            return Ok(vec![]);
        }
        let mut r = Reader {
            bytes: self.bytes,
            at: *self
                .offsets
                .get(usize::from(index.0))
                .ok_or(Error::InvalidFile)?,
        };
        let items = r.word()?;
        let words = r.word()?;
        let count = usize::from(r.word()?);
        if index.1 >= items {
            return Err(Error::InvalidFile);
        }
        let regions = (0..count).map(|_| r.word()).collect::<Result<Vec<_>>>()?;
        let wide = usize::from(words & 0x7fff);
        let stride = if words & 0x8000 != 0 {
            wide * 4 + (count - wide) * 2
        } else {
            wide * 2 + count - wide
        };
        r.take(usize::from(index.1) * stride)?;
        regions
            .into_iter()
            .enumerate()
            .map(|(i, region)| {
                let value = if words & 0x8000 != 0 {
                    if i < wide {
                        r.long()? as i32
                    } else {
                        i32::from(r.word()? as i16)
                    }
                } else if i < wide {
                    i32::from(r.word()? as i16)
                } else {
                    i32::from(r.byte()? as i8)
                };
                Ok((region, value))
            })
            .collect()
    }
    pub fn delta(&self, index: Index, location: &Location) -> Result<f64> {
        if location.len() != self.axes.len() || location.keys().any(|k| !self.axes.contains(k)) {
            return Err(Error::InvalidGeometry);
        }
        Support::default().weight(location)?;
        let mut out = 0.;
        for (region, delta) in self.row(index)? {
            let mut weight = 1.;
            for (tag, &[start, peak, end]) in
                self.axes.iter().zip(&self.regions[usize::from(region)])
            {
                if peak == 0 || start > peak || peak > end || (start < 0 && end > 0) {
                    continue;
                }
                let value = f64::from(location[tag]) * 16384.;
                let (start, peak, end) = (f64::from(start), f64::from(peak), f64::from(end));
                weight *= if value == peak {
                    1.
                } else if value <= start || value >= end {
                    0.
                } else if value < peak {
                    (value - start) / (peak - start)
                } else {
                    (end - value) / (end - peak)
                };
            }
            out += weight * f64::from(delta);
        }
        Ok(out)
    }
}
pub struct Builder<'a> {
    store: Store<'a>,
    rows: Vec<Vec<u8>>,
}
impl<'a> Builder<'a> {
    /// CFF2 variation-data records contain region indices without item rows.
    pub fn add_regions(&mut self, supports: &[Support]) -> Result<u16> {
        let outer = self.store.offsets.len() + self.rows.len();
        if outer >= 65535 || supports.len() > 510 {
            return Err(Error::TooLarge);
        }
        let mut bytes = vec![0, 0, 0, 0];
        bytes.extend((supports.len() as u16).to_be_bytes());
        for support in supports {
            let peak = support
                .axes
                .iter()
                .map(|(tag, v)| (tag.clone(), v[1]))
                .collect();
            support.weight(&peak)?;
            if support.axes.keys().any(|a| !self.store.axes.contains(a)) {
                return Err(Error::InvalidGeometry);
            }
            let mut region = vec![];
            for axis in &self.store.axes {
                let values = support.axes.get(axis).copied().unwrap_or([0.; 3]);
                let mut fixed = [0i16; 3];
                for (i, value) in values.into_iter().enumerate() {
                    let f = value * 16384.;
                    if f.round() != f {
                        return Err(Error::InvalidGeometry);
                    }
                    fixed[i] = f as i16;
                }
                region.push(fixed);
            }
            let index = if let Some(index) = self.store.regions.iter().position(|v| *v == region) {
                index
            } else {
                if self.store.regions.len() >= 32767 {
                    return Err(Error::TooLarge);
                }
                self.store.regions.push(region);
                self.store.regions.len() - 1
            };
            bytes.extend((index as u16).to_be_bytes());
        }
        self.rows.push(bytes);
        Ok(outer as u16)
    }

    pub fn new(store: Store<'a>) -> Self {
        Self {
            store,
            rows: vec![],
        }
    }
    /// Append a combined row; existing data offsets and row indices retain their semantics.
    pub fn add(
        &mut self,
        original: Index,
        corrections: &[(Support, f64)],
    ) -> Result<(Index, Vec<(Support, f64)>)> {
        let outer = self.store.offsets.len() + self.rows.len();
        if outer >= 65535 {
            return Err(Error::TooLarge);
        }
        let mut values = BTreeMap::<u16, i64>::new();
        for (region, value) in self.store.row(original)? {
            *values.entry(region).or_default() += i64::from(value);
        }
        let mut errors = vec![];
        for (support, value) in corrections {
            if !value.is_finite() || value.abs() > 1_000_000_000. {
                return Err(Error::InvalidGeometry);
            }
            let peak = support
                .axes
                .iter()
                .map(|(tag, v)| (tag.clone(), v[1]))
                .collect();
            support.weight(&peak)?;
            if support.axes.keys().any(|k| !self.store.axes.contains(k)) {
                return Err(Error::InvalidGeometry);
            }
            if support.axes.values().all(|v| v[1] == 0.) && *value != 0. {
                return Err(Error::InvalidGeometry);
            }
            let rounded = value.round();
            let error = (value - rounded).abs();
            if error > 0. {
                errors.push((support.clone(), error));
            }
            if rounded == 0. {
                continue;
            }
            let mut region = Vec::with_capacity(self.store.axes.len());
            for axis in &self.store.axes {
                let tuple = support.axes.get(axis).copied().unwrap_or([0.; 3]);
                let mut fixed = [0i16; 3];
                for i in 0..3 {
                    let v = tuple[i] * 16384.;
                    if v != v.round() {
                        return Err(Error::InvalidGeometry);
                    }
                    fixed[i] = v as i16;
                }
                region.push(fixed);
            }
            let index = if let Some(index) = self.store.regions.iter().position(|r| *r == region) {
                index
            } else {
                if self.store.regions.len() >= 32767
                    || (self.store.regions.len() + 1) * self.store.axes.len() > 1_000_000
                {
                    return Err(Error::TooLarge);
                }
                self.store.regions.push(region);
                self.store.regions.len() - 1
            };
            *values.entry(index as u16).or_default() += rounded as i64;
        }
        values.retain(|_, v| *v != 0);
        let count = u16::try_from(values.len()).map_err(|_| Error::TooLarge)?;
        if count > 32767 {
            return Err(Error::TooLarge);
        }
        let mut bytes = 1u16.to_be_bytes().to_vec();
        bytes.extend((0x8000 | count).to_be_bytes());
        bytes.extend(count.to_be_bytes());
        for &region in values.keys() {
            bytes.extend(region.to_be_bytes());
        }
        for value in values.values() {
            bytes.extend(
                i32::try_from(*value)
                    .map_err(|_| Error::TooLarge)?
                    .to_be_bytes(),
            );
        }
        self.rows.push(bytes);
        Ok(((outer as u16, 0), errors))
    }
    pub fn finish(self) -> Result<Vec<u8>> {
        let count = self.store.offsets.len() + self.rows.len();
        let header = 8 + count * 4;
        let shift = header - self.store.header;
        let mut tail = self.store.bytes[self.store.header..].to_vec();
        let region_at = header + tail.len();
        tail.extend((self.store.axes.len() as u16).to_be_bytes());
        tail.extend((self.store.regions.len() as u16).to_be_bytes());
        for region in self.store.regions {
            for axis in region {
                for value in axis {
                    tail.extend(value.to_be_bytes());
                }
            }
        }
        let mut offsets: Vec<_> = self.store.offsets.iter().map(|v| v + shift).collect();
        for row in self.rows {
            offsets.push(header + tail.len());
            tail.extend(row);
        }
        if header + tail.len() > 256 * 1024 * 1024 {
            return Err(Error::TooLarge);
        }
        let mut out = 1u16.to_be_bytes().to_vec();
        out.extend((region_at as u32).to_be_bytes());
        out.extend((count as u16).to_be_bytes());
        for offset in offsets {
            out.extend((offset as u32).to_be_bytes());
        }
        out.extend(tail);
        Ok(out)
    }
}
