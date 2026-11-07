use crate::{Error, Result};
#[derive(Clone)]
pub struct Dict {
    pub op: u16,
    pub values: Vec<f64>,
    pub raw: Vec<u8>,
}
pub struct Reader<'a> {
    pub bytes: &'a [u8],
    pub at: usize,
}
impl<'a> Reader<'a> {
    pub fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self.at.checked_add(n).ok_or(Error::InvalidFile)?;
        let b = self.bytes.get(self.at..end).ok_or(Error::InvalidFile)?;
        self.at = end;
        Ok(b)
    }
    pub fn byte(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }
    pub fn word(&mut self) -> Result<u16> {
        Ok(u16::from_be_bytes(
            self.take(2)?.try_into().map_err(|_| Error::InvalidFile)?,
        ))
    }
    pub fn long(&mut self) -> Result<u32> {
        Ok(u32::from_be_bytes(
            self.take(4)?.try_into().map_err(|_| Error::InvalidFile)?,
        ))
    }
    pub fn number(&mut self, b: u8, dict: bool) -> Result<f64> {
        Ok(match b {
            32..=246 => f64::from(i16::from(b) - 139),
            247..=250 => f64::from((u16::from(b) - 247) * 256 + u16::from(self.byte()?) + 108),
            251..=254 => -f64::from((u16::from(b) - 251) * 256 + u16::from(self.byte()?) + 108),
            28 => f64::from(self.word()? as i16),
            29 if dict => f64::from(self.long()? as i32),
            255 if !dict => f64::from(self.long()? as i32) / 65536.,
            30 if dict => {
                let mut text = String::new();
                'outer: loop {
                    if text.len() > 80 {
                        return Err(Error::InvalidFile);
                    }
                    let byte = self.byte()?;
                    for n in [byte >> 4, byte & 15] {
                        match n {
                            0..=9 => text.push(char::from(b'0' + n)),
                            10 => text.push('.'),
                            11 => text.push('e'),
                            12 => text.push_str("e-"),
                            14 => text.push('-'),
                            15 => break 'outer,
                            _ => return Err(Error::InvalidFile),
                        }
                    }
                }
                text.parse::<f64>().map_err(|_| Error::InvalidFile)?
            }
            _ => return Err(Error::InvalidFile),
        })
    }
}
pub fn dict(bytes: &[u8]) -> Result<Vec<Dict>> {
    let mut r = Reader { bytes, at: 0 };
    let mut values = vec![];
    let mut out = vec![];
    let mut start = 0;
    while r.at < bytes.len() {
        let b = r.byte()?;
        if b <= 27 && b != 28 {
            let op = if b == 12 {
                1200 + u16::from(r.byte()?)
            } else {
                u16::from(b)
            };
            out.push(Dict {
                op,
                values: std::mem::take(&mut values),
                raw: bytes[start..r.at].to_vec(),
            });
            start = r.at;
        } else {
            values.push(r.number(b, true)?);
            if values.len() > 513 {
                return Err(Error::TooLarge);
            }
        }
    }
    if !values.is_empty() {
        return Err(Error::InvalidFile);
    }
    Ok(out)
}
pub fn offset(dict: &[Dict], op: u16) -> Result<Option<usize>> {
    dict.iter()
        .find(|d| d.op == op)
        .map(|d| {
            if d.values.len() != 1
                || d.values[0] < 0.
                || d.values[0].fract() != 0.
                || d.values[0] > 256. * 1024. * 1024.
            {
                return Err(Error::InvalidFile);
            }
            Ok(d.values[0] as usize)
        })
        .transpose()
}
pub fn index(bytes: &[u8], at: usize) -> Result<(Vec<&[u8]>, usize)> {
    let mut r = Reader { bytes, at };
    let count = r.long()? as usize;
    if count > 65535 {
        return Err(Error::TooLarge);
    }
    if count == 0 {
        return Ok((vec![], r.at));
    }
    let size = usize::from(r.byte()?);
    if !(1..=4).contains(&size) {
        return Err(Error::InvalidFile);
    }
    let mut offsets = Vec::with_capacity(count + 1);
    for _ in 0..=count {
        let v = r
            .take(size)?
            .iter()
            .fold(0usize, |v, &b| (v << 8) | usize::from(b));
        offsets.push(v);
    }
    if offsets[0] != 1 || offsets.windows(2).any(|w| w[0] > w[1]) {
        return Err(Error::InvalidFile);
    }
    let payload = r.take(offsets[count] - 1)?;
    Ok((
        offsets
            .windows(2)
            .map(|w| &payload[w[0] - 1..w[1] - 1])
            .collect(),
        r.at,
    ))
}
pub fn write_index(items: &[Vec<u8>]) -> Result<Vec<u8>> {
    let mut out = u32::try_from(items.len())
        .map_err(|_| Error::TooLarge)?
        .to_be_bytes()
        .to_vec();
    if items.is_empty() {
        return Ok(out);
    }
    out.push(4);
    let mut offset = 1u32;
    out.extend(offset.to_be_bytes());
    for item in items {
        offset = offset
            .checked_add(u32::try_from(item.len()).map_err(|_| Error::TooLarge)?)
            .ok_or(Error::TooLarge)?;
        out.extend(offset.to_be_bytes());
    }
    for item in items {
        out.extend(item);
    }
    Ok(out)
}
pub fn operator(out: &mut Vec<u8>, op: u16) {
    if op >= 1200 {
        out.extend([12, (op - 1200) as u8]);
    } else {
        out.push(op as u8);
    }
}
pub fn dict_values(out: &mut Vec<u8>, op: u16, values: &[i32]) {
    for value in values {
        out.push(29);
        out.extend(value.to_be_bytes());
    }
    operator(out, op);
}
pub fn number(out: &mut Vec<u8>, v: f64) -> Result<f64> {
    if !v.is_finite() || !(-32768. ..32768.).contains(&v) {
        return Err(Error::InvalidGeometry);
    }
    let rounded = (v * 65536.).round() / 65536.;
    if v.fract() == 0. {
        out.push(28);
        out.extend((v as i16).to_be_bytes());
    } else {
        if rounded >= 32768. {
            return Err(Error::InvalidGeometry);
        }
        out.push(255);
        out.extend(((rounded * 65536.) as i32).to_be_bytes());
    }
    Ok((rounded - v).abs())
}
