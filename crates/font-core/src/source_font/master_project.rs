//! Persisted master registry. Source bytes remain immutable across edits and undo.
use super::{
    master_export, master_glyf, master_gvar,
    master_points::Topology,
    master_sample::Sampler,
    masters::{Master, Masters, Shape, Support},
    SourceFont,
};
use crate::{identity::Uuid, Change, Editor, Error, GlyphId, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Project {
    #[serde(default)]
    pub operations: BTreeMap<u16, Vec<super::master_topology::Operation>>,
    pub masters: Masters,
    pub topologies: BTreeMap<u16, Topology>,
}
impl Project {
    pub(crate) fn advanced(&self, bytes: &[u8]) -> Result<bool> {
        if bytes.starts_with(b"OTTO") || !self.operations.is_empty() {
            return Ok(true);
        }
        for &id in self.topologies.keys() {
            if super::master_composite::is_composite(bytes, id)? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub fn validate(&self, bytes: &[u8]) -> Result<()> {
        self.masters.validate()?;
        if self.operations.len() > self.topologies.len()
            || self.operations.iter().any(|(id, ops)| {
                !self.topologies.contains_key(id) || ops.is_empty() || ops.len() > 128
            })
            || self.operations.values().map(Vec::len).sum::<usize>() > 4096
        {
            return Err(Error::TooLarge);
        }
        let points = self.topologies.values().try_fold(0usize, |n, t| {
            n.checked_add(t.point_count()?).ok_or(Error::TooLarge)
        })?;
        if points > 1_000_000 {
            return Err(Error::TooLarge);
        }
        if self.topologies.len() > 65535 {
            return Err(Error::TooLarge);
        }
        let face = ttf_parser::Face::parse(bytes, 0).map_err(|_| Error::InvalidFile)?;
        let axes: Vec<_> = face
            .variation_axes()
            .into_iter()
            .map(|a| a.tag.to_string())
            .collect();
        for master in &self.masters.entries {
            if master.location.len() != axes.len()
                || axes.iter().any(|a| !master.location.contains_key(a))
            {
                return Err(Error::InvalidGeometry);
            }
            for (&id, shape) in &master.glyphs {
                let gid = u16::try_from(id).map_err(|_| Error::MissingGlyph)?;
                self.topologies
                    .get(&gid)
                    .ok_or(Error::InvalidGeometry)?
                    .physical_points(&shape.contours)?;
            }
        }
        let prepared = self.prepared(bytes)?;
        for (&id, topology) in &self.topologies {
            let (original, points) = source_geometry(&prepared, id, None)?;
            if original.rings() != topology.rings() || original.is_cubic() != topology.is_cubic() {
                return Err(Error::InvalidGeometry);
            }
            topology.physical_points(&topology.contours(&points)?)?;
        }
        Ok(())
    }
    pub fn export(&self, bytes: &[u8]) -> Result<master_export::Exported> {
        self.validate(bytes)?;
        let prepared = self.prepared_report(bytes)?;
        let mut output = master_export::export(&prepared.bytes, &self.masters, &self.topologies)?;
        for (id, errors) in prepared.errors {
            output.outline_errors.entry(id).or_default().extend(errors);
        }
        output.native_outline_rounding = prepared.native;
        for (id, values) in prepared.metric_origins {
            let row = output.metrics.native_origin_rounding.entry(id).or_default();
            for axis in 0..2 {
                row[axis] += values[axis];
            }
        }
        for (id, values) in prepared.metric_advances {
            let row = output
                .metrics
                .native_advance_rounding
                .entry(id)
                .or_default();
            for axis in 0..2 {
                row[axis] += values[axis];
            }
        }
        super::master_composite::propagate_reports(
            &output.metrics.bytes,
            &mut output.outline_errors,
            &mut output.native_outline_rounding,
        )?;
        Ok(output)
    }
    pub(crate) fn prepared(&self, bytes: &[u8]) -> Result<Vec<u8>> {
        Ok(self.prepared_report(bytes)?.bytes)
    }
    fn prepared_report(&self, bytes: &[u8]) -> Result<super::master_composite::Normalized> {
        let ensured = master_gvar::ensure(bytes)?;
        let bytes = ensured.as_slice();
        let face = ttf_parser::Face::parse(bytes, 0).map_err(|_| Error::InvalidFile)?;
        let mut normalized = if face
            .raw_face()
            .table(ttf_parser::Tag::from_bytes(b"CFF2"))
            .is_some()
        {
            super::master_composite::Normalized {
                metric_origins: BTreeMap::new(),
                metric_advances: BTreeMap::new(),
                bytes: bytes.to_vec(),
                errors: BTreeMap::new(),
                native: BTreeMap::new(),
            }
        } else {
            let selected = self
                .topologies
                .keys()
                .copied()
                .filter_map(
                    |id| match super::master_composite::is_composite(bytes, id) {
                        Ok(true) => Some(Ok(id)),
                        Ok(false) => None,
                        Err(e) => Some(Err(e)),
                    },
                )
                .collect::<Result<_>>()?;
            super::master_composite::normalize_report(bytes, &selected)?
        };
        let topology = super::master_topology::prepare_report(&normalized.bytes, &self.operations)?;
        normalized.bytes = topology.bytes;
        for (id, errors) in topology.errors {
            normalized.errors.entry(id).or_default().extend(errors);
        }
        Ok(normalized)
    }
    pub fn frame(&self, source: &SourceFont, master: Uuid, glyph: GlyphId) -> Result<crate::Point> {
        let m = self
            .masters
            .entries
            .iter()
            .find(|m| m.id == master)
            .ok_or(Error::InvalidDocument)?;
        let prepared = self.prepared(&source.bytes)?;
        let (topology, points) = source_geometry(
            &prepared,
            u16::try_from(glyph).map_err(|_| Error::MissingGlyph)?,
            Some(&m.location),
        )?;
        let contours = topology.contours(&points)?;
        let mut sampler = Sampler::new(std::sync::Arc::new(prepared), 0)?;
        let native = sampler.sample_normalized(glyph, &m.location)?;
        match (contours.first(), native.shape.contours.first()) {
            (Some(a), Some(b)) => Ok([b.start[0] - a.start[0], 0.]),
            _ => Ok([0.; 2]),
        }
    }
    pub fn shape(&self, source: &SourceFont, master: Uuid, glyph: GlyphId) -> Result<Shape> {
        let m = self
            .masters
            .entries
            .iter()
            .find(|m| m.id == master)
            .ok_or(Error::InvalidDocument)?;
        if let Some(shape) = m.glyphs.get(&glyph) {
            return Ok(shape.clone());
        }
        let gid = u16::try_from(glyph).map_err(|_| Error::MissingGlyph)?;
        let prepared = self.prepared(&source.bytes)?;
        let (original, points) = source_geometry(&prepared, gid, Some(&m.location))?;
        let topology = self.topologies.get(&gid).unwrap_or(&original);
        let mut sampler = Sampler::new(std::sync::Arc::new(prepared), 0)?;
        Ok(Shape {
            advance: sampler.sample_normalized(glyph, &m.location)?.shape.advance,
            contours: topology.contours(&points)?,
        })
    }
}
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Edit {
    Topology {
        glyph: GlyphId,
        operation: super::master_topology::Operation,
    },
    Register {
        name: String,
    },
    Rename {
        id: Uuid,
        name: String,
    },
    Delete {
        id: Uuid,
    },
    Replace {
        id: Uuid,
        glyph: GlyphId,
        shape: Shape,
    },
    Point {
        id: Uuid,
        glyph: GlyphId,
        index: usize,
        position: crate::Point,
    },
    Advance {
        id: Uuid,
        glyph: GlyphId,
        value: f64,
    },
    Capture {
        id: Uuid,
        glyph: GlyphId,
    },
    ResetGlyph {
        id: Uuid,
        glyph: GlyphId,
    },
}
impl Editor {
    pub fn edit_source_master(&mut self, expected: u64, operation: Edit) -> Result<u64> {
        let revision = self.check_revision_unrestricted(expected)?;
        let before = self
            .document
            .source_font
            .clone()
            .ok_or(Error::InvalidDocument)?;
        let mut after = before.clone();
        if after.variable.is_none() {
            return Err(Error::InvalidDocument);
        }
        let mut project = after.masters.as_deref().cloned().unwrap_or_default();
        match operation {
            Edit::Topology { glyph, operation } => {
                let gid = u16::try_from(glyph).map_err(|_| Error::MissingGlyph)?;
                if project.operations.get(&gid).is_some_and(|v| v.len() >= 128) {
                    return Err(Error::TooLarge);
                }
                let prepared = project.prepared(&after.bytes)?;
                let old = project
                    .topologies
                    .get(&gid)
                    .ok_or(Error::InvalidGeometry)?
                    .clone();
                let (_, points) = source_geometry(&prepared, gid, None)?;
                let mut baseline = old.contours(&points)?;
                operation.apply(&mut baseline)?;
                let mut topology = if old.is_cubic() {
                    Topology::path(&baseline)?
                } else {
                    Topology::quadratic(&baseline)?.0
                };
                topology.adopt(&baseline)?;
                for index in 0..project.masters.entries.len() {
                    let master = project.masters.entries[index].id;
                    let mut shape = project.shape(&after, master, glyph)?;
                    operation.apply(&mut shape.contours)?;
                    topology.align(&mut shape.contours)?;
                    project.masters.entries[index].glyphs.insert(glyph, shape);
                }
                project.topologies.insert(gid, topology);
                project.operations.entry(gid).or_default().push(operation);
            }
            Edit::Register { name } => {
                let settings = after.variable.as_ref().ok_or(Error::InvalidDocument)?;
                let mut sampler = Sampler::new(after.bytes.clone(), 0)?;
                let sample = sampler.sample_design(0, &settings.coordinates)?;
                let support = Support {
                    axes: sample
                        .location
                        .iter()
                        .filter_map(|(tag, &v)| {
                            if v == 0. {
                                None
                            } else {
                                let peak = (v * 16384.).round() / 16384.;
                                Some((
                                    tag.clone(),
                                    if peak > 0. {
                                        [0., peak, 1.]
                                    } else {
                                        [-1., peak, 0.]
                                    },
                                ))
                            }
                        })
                        .collect(),
                };
                if sample.location.values().any(|&v| v != 0.)
                    && support.axes.values().all(|a| a[1] == 0.)
                {
                    return Err(Error::InvalidGeometry);
                }
                project.masters.entries.push(Master {
                    id: Uuid::new_v4(),
                    name,
                    design_location: sample.design_location,
                    location: sample.location,
                    support,
                    glyphs: BTreeMap::new(),
                });
            }
            Edit::Rename { id, name } => {
                project
                    .masters
                    .entries
                    .iter_mut()
                    .find(|m| m.id == id)
                    .ok_or(Error::InvalidDocument)?
                    .name = name
            }
            Edit::Delete { id } => {
                let index = project
                    .masters
                    .entries
                    .iter()
                    .position(|m| m.id == id)
                    .ok_or(Error::InvalidDocument)?;
                project.masters.entries.remove(index);
            }
            Edit::Replace { id, glyph, shape } => {
                let gid = u16::try_from(glyph).map_err(|_| Error::MissingGlyph)?;
                let topology = project
                    .topologies
                    .entry(gid)
                    .or_insert(source_geometry(&after.bytes, gid, None)?.0);
                topology.physical_points(&shape.contours)?;
                project
                    .masters
                    .entries
                    .iter_mut()
                    .find(|m| m.id == id)
                    .ok_or(Error::InvalidDocument)?
                    .glyphs
                    .insert(glyph, shape);
            }
            Edit::Point {
                id,
                glyph,
                index,
                position,
            } => {
                let mut shape = project.shape(&after, id, glyph)?;
                let topology = project
                    .topologies
                    .get(&u16::try_from(glyph).map_err(|_| Error::MissingGlyph)?)
                    .ok_or(Error::InvalidGeometry)?;
                let mut points = topology.physical_points(&shape.contours)?;
                *points.get_mut(index).ok_or(Error::InvalidGeometry)? = position;
                shape.contours = topology.contours(&points)?;
                project
                    .masters
                    .entries
                    .iter_mut()
                    .find(|m| m.id == id)
                    .ok_or(Error::InvalidDocument)?
                    .glyphs
                    .insert(glyph, shape);
            }
            Edit::Advance { id, glyph, value } => {
                let mut shape = project.shape(&after, id, glyph)?;
                shape.advance = value;
                project
                    .masters
                    .entries
                    .iter_mut()
                    .find(|m| m.id == id)
                    .ok_or(Error::InvalidDocument)?
                    .glyphs
                    .insert(glyph, shape);
            }
            Edit::Capture { id, glyph } => {
                let gid = u16::try_from(glyph).map_err(|_| Error::MissingGlyph)?;
                if let std::collections::btree_map::Entry::Vacant(entry) =
                    project.topologies.entry(gid)
                {
                    entry.insert(source_geometry(&after.bytes, gid, None)?.0);
                }
                let shape = project.shape(&after, id, glyph)?;
                project
                    .masters
                    .entries
                    .iter_mut()
                    .find(|m| m.id == id)
                    .ok_or(Error::InvalidDocument)?
                    .glyphs
                    .entry(glyph)
                    .or_insert(shape);
            }
            Edit::ResetGlyph { id, glyph } => {
                project
                    .masters
                    .entries
                    .iter_mut()
                    .find(|m| m.id == id)
                    .ok_or(Error::InvalidDocument)?
                    .glyphs
                    .remove(&glyph);
            }
        }
        project.validate(&after.bytes)?;
        // Validate representability before accepting an undoable edit or allowing it to be saved.
        project.export(&after.bytes)?;
        after.masters = Some(std::sync::Arc::new(project));
        if after == before {
            return Ok(self.revision);
        }
        let previous_schema = self.document.schema_version;
        let schema = if after
            .masters
            .as_ref()
            .is_some_and(|p| p.advanced(&after.bytes).unwrap_or(true))
        {
            8
        } else {
            previous_schema.max(7)
        };
        self.document.source_font = Some(after.clone());
        self.document.schema_version = schema;
        self.undo.push_back(Change {
            before_schema: Some(previous_schema),
            after_schema: Some(schema),
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

pub(crate) fn source_geometry(
    bytes: &[u8],
    glyph: u16,
    location: Option<&super::masters::Location>,
) -> Result<(Topology, Vec<crate::Point>)> {
    let face = ttf_parser::Face::parse(bytes, 0).map_err(|_| Error::InvalidFile)?;
    let axes: Vec<_> = face
        .variation_axes()
        .into_iter()
        .map(|a| a.tag.to_string())
        .collect();
    let zero = axes.iter().map(|a| (a.clone(), 0.)).collect();
    let location = location.unwrap_or(&zero);
    if let Some(cff) = face.raw_face().table(ttf_parser::Tag::from_bytes(b"CFF2")) {
        let font = super::master_cff::Font::parse(
            cff,
            &axes,
            face.number_of_glyphs(),
            face.units_per_em(),
        )?;
        let g = font.glyph(glyph)?;
        let contours = g.contours(&font.weights(location)?)?;
        return Ok((
            Topology::path(&contours)?,
            g.points(&font.weights(location)?)?,
        ));
    }
    if face
        .raw_face()
        .table(ttf_parser::Tag::from_bytes(b"gvar"))
        .is_none()
    {
        let prepared = master_gvar::ensure(bytes)?;
        return source_geometry(&prepared, glyph, Some(location));
    }
    let Some(g) = master_glyf::read(bytes, 0, glyph)? else {
        let prepared = super::master_composite::normalize(bytes, &[glyph].into())?;
        return source_geometry(&prepared, glyph, Some(location));
    };
    let tuples = master_gvar::read(
        face.raw_face()
            .table(ttf_parser::Tag::from_bytes(b"gvar"))
            .ok_or(Error::InvalidFile)?,
        &axes,
        face.number_of_glyphs(),
        glyph,
        g.points.len() + 4,
    )?;
    let points = g.sample_points(&tuples, location)?;
    Ok((g.topology, points))
}
