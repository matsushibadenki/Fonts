//! Restrict the original design space; preview coordinates never modify source outlines.
use super::*;
use crate::variations::{self, Coordinates};
use font_hinting_native::variations::{self as native, AxisRange};
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Range {
    pub min: f32,
    pub default: f32,
    pub max: f32,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Variable {
    pub ranges: BTreeMap<String, Range>,
    pub coordinates: Coordinates,
}
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Edit {
    Coordinates { values: Coordinates },
    Range { tag: String, range: Range },
    Instance { id: usize },
    Reset,
}
impl Variable {
    pub fn new(bytes: &[u8]) -> crate::font_io::Result<Self> {
        let axes = variations::catalog(bytes, 0)?.axes;
        Ok(Self {
            ranges: axes
                .iter()
                .map(|a| {
                    (
                        a.tag.clone(),
                        Range {
                            min: a.min,
                            default: a.default,
                            max: a.max,
                        },
                    )
                })
                .collect(),
            coordinates: axes.iter().map(|a| (a.tag.clone(), a.default)).collect(),
        })
    }
    pub fn validate(&self, bytes: &[u8]) -> crate::font_io::Result<()> {
        let axes = variations::catalog(bytes, 0)?.axes;
        if self.ranges.len() != axes.len() || self.coordinates.len() != axes.len() {
            return Err("variation_axis".into());
        }
        for axis in axes {
            let r = self.ranges.get(&axis.tag).ok_or("variation_axis")?;
            let value = *self.coordinates.get(&axis.tag).ok_or("variation_axis")?;
            if ![r.min, r.default, r.max, value]
                .iter()
                .all(|v| v.is_finite())
                || r.min < axis.min
                || r.max > axis.max
                || r.min > r.default
                || r.default > r.max
                || value < r.min
                || value > r.max
            {
                return Err("variation_range".into());
            }
        }
        Ok(())
    }
    pub fn export(&self, bytes: &[u8]) -> crate::font_io::Result<Vec<u8>> {
        self.validate(bytes)?;
        let unchanged = variations::axes(bytes, 0)?.iter().all(|a| {
            let r = &self.ranges[&a.tag];
            r.min == a.min && r.default == a.default && r.max == a.max
        });
        if unchanged {
            return Ok(bytes.to_vec());
        }
        let ranges = self
            .ranges
            .iter()
            .map(|(tag, r)| {
                Ok(AxisRange {
                    tag: u32::from_be_bytes(
                        tag.as_bytes().try_into().map_err(|_| "variation_axis")?,
                    ),
                    minimum: r.min,
                    maximum: r.max,
                    default: r.default,
                })
            })
            .collect::<crate::font_io::Result<Vec<_>>>()?;
        let out = super::names::normalize_instances(native::subset(bytes, 0, &ranges)?)?;
        let face = ttf_parser::Face::parse(&out, 0).map_err(|_| "variation_invalid_output")?;
        let active: Vec<_> = self.ranges.iter().filter(|(_, r)| r.min < r.max).collect();
        if face.variation_axes().len() as usize != active.len() {
            return Err("variation_invalid_output".into());
        }
        for a in face.variation_axes() {
            let r = self
                .ranges
                .get(&a.tag.to_string())
                .ok_or("variation_invalid_output")?;
            if (a.min_value - r.min).abs() > 1. / 65536.
                || (a.def_value - r.default).abs() > 1. / 65536.
                || (a.max_value - r.max).abs() > 1. / 65536.
            {
                return Err("variation_invalid_output".into());
            }
        }
        if face.is_variable() {
            variations::catalog(&out, 0)?;
        }
        Ok(out)
    }
    pub fn static_bytes(&self, bytes: &[u8]) -> crate::font_io::Result<Vec<u8>> {
        self.validate(bytes)?;
        let ranges = self
            .coordinates
            .iter()
            .map(|(tag, &value)| {
                Ok(AxisRange {
                    tag: u32::from_be_bytes(
                        tag.as_bytes().try_into().map_err(|_| "variation_axis")?,
                    ),
                    minimum: value,
                    maximum: value,
                    default: value,
                })
            })
            .collect::<crate::font_io::Result<Vec<_>>>()?;
        let out = native::subset(bytes, 0, &ranges)?;
        if ttf_parser::Face::parse(&out, 0)
            .map_err(|_| "variation_invalid_output")?
            .is_variable()
        {
            return Err("variation_invalid_output".into());
        }
        Ok(out)
    }
    fn edit(&mut self, bytes: &[u8], operation: Edit) -> crate::font_io::Result<()> {
        match operation {
            Edit::Coordinates { values } => self.coordinates = values,
            Edit::Range { tag, range } => {
                let value = self.coordinates.get_mut(&tag).ok_or("variation_axis")?;
                if !range.min.is_finite() || !range.max.is_finite() || range.min > range.max {
                    return Err("variation_range".into());
                }
                *value = (*value).clamp(range.min, range.max);
                self.ranges.insert(tag, range);
            }
            Edit::Reset => {
                self.coordinates = self
                    .ranges
                    .iter()
                    .map(|(tag, r)| (tag.clone(), r.default))
                    .collect()
            }
            Edit::Instance { id } => {
                self.coordinates = variations::catalog(bytes, 0)?
                    .instances
                    .into_iter()
                    .find(|i| i.id == id)
                    .ok_or("variation_instance")?
                    .coordinates
            }
        }
        self.validate(bytes)
    }
}
impl Editor {
    pub fn edit_source_variations(&mut self, expected: u64, operation: Edit) -> Result<u64> {
        let revision = self.check_revision_unrestricted(expected)?;
        let before = self
            .document
            .source_font
            .clone()
            .ok_or(Error::InvalidDocument)?;
        let mut after = before.clone();
        let settings = after.variable.as_mut().ok_or(Error::InvalidDocument)?;
        settings
            .edit(&after.bytes, operation)
            .map_err(|_| Error::InvalidGeometry)?;
        if before == after {
            return Ok(self.revision);
        }
        self.document.source_font = Some(after.clone());
        self.undo.push_back(Change {
            before_source: Some(Some(before)),
            after_source: Some(Some(after)),
            ..Default::default()
        });
        if self.undo.len() > self.history_limit {
            self.undo.pop_front();
        }
        self.redo.clear();
        self.revision = revision;
        Ok(revision)
    }
}

#[derive(Serialize)]
pub struct Summary {
    pub axes: Vec<variations::Axis>,
    pub instances: Vec<variations::Instance>,
    pub settings: Variable,
}
impl Variable {
    pub fn summary(&self, bytes: &[u8]) -> crate::font_io::Result<Summary> {
        let catalog = variations::catalog(bytes, 0)?;
        Ok(Summary {
            axes: catalog.axes,
            instances: catalog.instances,
            settings: self.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn default_name_references_remain_valid_after_default_axis_changes() {
        let source = include_bytes!("../../tests/data/variations/named-gvar.ttf");
        let face = ttf_parser::Face::parse(source, 0).unwrap();
        let mut tables: BTreeMap<_, _> = face
            .raw_face()
            .table_records
            .into_iter()
            .map(|r| {
                (
                    r.tag.to_bytes(),
                    face.raw_face().table(r.tag).unwrap().to_vec(),
                )
            })
            .collect();
        tables.get_mut(b"head").unwrap()[8..12].fill(0);
        let f = tables.get_mut(b"fvar").unwrap();
        let first = usize::from(u16::from_be_bytes(f[4..6].try_into().unwrap()))
            + usize::from(u16::from_be_bytes(f[8..10].try_into().unwrap())) * 20;
        f[first..first + 2].copy_from_slice(&2u16.to_be_bytes());
        let ps = first + 4 + 8;
        f[ps..ps + 2].copy_from_slice(&6u16.to_be_bytes());
        let bytes = crate::color_fonts::assemble(tables, false).unwrap();
        let mut v = Variable::new(&bytes).unwrap();
        v.ranges.get_mut("wght").unwrap().default = 500.;
        let out = v.export(&bytes).unwrap();
        variations::catalog(&out, 0).unwrap();
    }
}
