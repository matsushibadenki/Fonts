//! Project-local reusable outlines. Placement always produces independent contours.
use crate::{
    geometry, reference_images::ReferenceImage, Change, Contour, Editor, Error, GlyphId,
    GlyphStatus, Point, Result, Segment,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

pub const MAX_SNIPPETS: usize = 256;
pub const MAX_POINTS: usize = 65536;
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Info {
    pub name: String,
    pub tags: String,
    pub category: String,
    pub source: String,
    pub license: String,
    pub favorite: bool,
}
impl Info {
    pub fn validate(&self) -> Result<()> {
        if self.name.trim().is_empty()
            || self.name.len() > 256
            || self.tags.len() > 1024
            || self.source.len() > 2048
            || self.license.len() > 2048
            || !["shape", "radical", "other"].contains(&self.category.as_str())
        {
            return Err(Error::InvalidDocument);
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Snippet {
    pub info: Info,
    pub upm: u16,
    pub contours: Arc<Vec<Contour>>,
}
impl Snippet {
    pub fn validate(&self) -> Result<()> {
        self.info.validate()?;
        if !(16..=16384).contains(&self.upm)
            || self.contours.is_empty()
            || self.contours.len() > 128
            || points(&self.contours) > 16384
        {
            return Err(Error::TooLarge);
        }
        for c in self.contours.iter() {
            geometry::validate(c)?;
        }
        let (min, max) = bounds(&self.contours)?;
        if max[0] - min[0] < 0.001 || max[1] - min[1] < 0.001 {
            return Err(Error::InvalidGeometry);
        }
        Ok(())
    }
    pub fn placement(&self, upm: u16, center: Point) -> Result<ReferenceImage> {
        let (min, max) = bounds(&self.contours)?;
        let scale = f64::from(upm) / f64::from(self.upm);
        Ok(ReferenceImage {
            id: 1,
            asset_id: String::new(),
            center,
            width: (max[0] - min[0]) * scale,
            height: (max[1] - min[1]) * scale,
            angle: 0.,
            opacity: 1.,
            visible: true,
            locked: false,
        })
    }
    pub fn transformed(&self, placement: &ReferenceImage) -> Result<Vec<Contour>> {
        placement.validate()?;
        let (min, max) = bounds(&self.contours)?;
        let point = |p: Point| {
            placement.to_document([
                (p[0] - min[0]) / (max[0] - min[0]) - 0.5,
                (p[1] - min[1]) / (max[1] - min[1]) - 0.5,
            ])
        };
        let contours: Vec<_> = self
            .contours
            .iter()
            .map(|c| Contour {
                start: point(c.start),
                smooth: c.smooth.clone(),
                closed: c.closed,
                segments: c
                    .segments
                    .iter()
                    .map(|s| match *s {
                        Segment::Line { end } => Segment::Line { end: point(end) },
                        Segment::Quadratic { control, end } => Segment::Quadratic {
                            control: point(control),
                            end: point(end),
                        },
                        Segment::Cubic {
                            control1,
                            control2,
                            end,
                        } => Segment::Cubic {
                            control1: point(control1),
                            control2: point(control2),
                            end: point(end),
                        },
                    })
                    .collect(),
            })
            .collect();
        for c in &contours {
            geometry::validate(c)?;
        }
        Ok(contours)
    }
}
fn points(contours: &[Contour]) -> usize {
    contours.iter().map(|c| c.segments.len() + 1).sum()
}
pub fn bounds(contours: &[Contour]) -> Result<(Point, Point)> {
    let mut min = [f64::INFINITY; 2];
    let mut max = [f64::NEG_INFINITY; 2];
    for c in contours {
        for (_, p) in geometry::edit_points(c) {
            for i in 0..2 {
                min[i] = min[i].min(p[i]);
                max[i] = max[i].max(p[i]);
            }
        }
    }
    if min.iter().chain(max.iter()).any(|v| !v.is_finite()) {
        return Err(Error::InvalidGeometry);
    }
    Ok((min, max))
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Library {
    pub entries: BTreeMap<u64, Snippet>,
}
impl Library {
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    pub fn validate(&self) -> Result<()> {
        if self.entries.len() > MAX_SNIPPETS
            || self
                .entries
                .keys()
                .any(|id| *id == 0 || *id > 9_007_199_254_740_991)
            || self
                .entries
                .values()
                .map(|s| points(&s.contours))
                .sum::<usize>()
                > MAX_POINTS
        {
            return Err(Error::TooLarge);
        }
        for s in self.entries.values() {
            s.validate()?;
        }
        Ok(())
    }
}
impl Editor {
    pub fn capture_snippet(
        &mut self,
        expected: u64,
        glyph: GlyphId,
        indices: &BTreeSet<usize>,
        info: Info,
    ) -> Result<u64> {
        self.check_revision(expected)?;
        if indices.is_empty() {
            return Err(Error::InvalidGeometry);
        }
        let g = self
            .document
            .glyphs
            .get(&glyph)
            .ok_or(Error::MissingGlyph)?;
        let contours = indices
            .iter()
            .map(|i| g.contours.get(*i).cloned().ok_or(Error::InvalidGeometry))
            .collect::<Result<Vec<_>>>()?;
        self.add_snippet(
            expected,
            Snippet {
                info,
                upm: self.document.units_per_em,
                contours: Arc::new(contours),
            },
        )
    }
    pub fn add_snippet(&mut self, expected: u64, snippet: Snippet) -> Result<u64> {
        self.check_revision(expected)?;
        snippet.validate()?;
        let mut library = self.document.snippets.clone();
        let id = library
            .entries
            .keys()
            .next_back()
            .copied()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or(Error::TooLarge)?;
        library.entries.insert(id, snippet);
        self.commit_snippets(expected, library)?;
        Ok(id)
    }
    pub fn update_snippet(&mut self, expected: u64, id: u64, info: Info) -> Result<()> {
        self.check_revision(expected)?;
        info.validate()?;
        let mut library = self.document.snippets.clone();
        library
            .entries
            .get_mut(&id)
            .ok_or(Error::InvalidDocument)?
            .info = info;
        self.commit_snippets(expected, library)
    }
    pub fn remove_snippet(&mut self, expected: u64, id: u64) -> Result<()> {
        self.check_revision(expected)?;
        let mut library = self.document.snippets.clone();
        library.entries.remove(&id).ok_or(Error::InvalidDocument)?;
        self.commit_snippets(expected, library)
    }
    fn commit_snippets(&mut self, expected: u64, library: Library) -> Result<()> {
        let revision = self.check_revision(expected)?;
        library.validate()?;
        if self.document.snippets == library {
            return Ok(());
        }
        let before = std::mem::replace(&mut self.document.snippets, library.clone());
        self.undo.push_back(Change {
            before_snippets: Some(before),
            after_snippets: Some(library),
            ..Default::default()
        });
        if self.undo.len() > self.history_limit {
            self.undo.pop_front();
        }
        self.redo.clear();
        self.revision = revision;
        Ok(())
    }
    pub fn insert_snippet(
        &mut self,
        expected: u64,
        glyph: GlyphId,
        snippet: &Snippet,
        placement: &ReferenceImage,
    ) -> Result<()> {
        self.check_revision(expected)?;
        snippet.validate()?;
        let mut g = self
            .document
            .glyphs
            .get(&glyph)
            .ok_or(Error::MissingGlyph)?
            .clone();
        let contours = snippet.transformed(placement)?;
        if g.contours.len() + contours.len() > 4096 {
            return Err(Error::TooLarge);
        }
        g.contours.extend(contours);
        g.status = GlyphStatus::Draft;
        self.replace_glyph(expected, g)?;
        Ok(())
    }
}
fn polygon(p: &[Point]) -> Contour {
    Contour {
        smooth: Default::default(),
        start: p[0],
        segments: p[1..].iter().map(|&end| Segment::Line { end }).collect(),
        closed: true,
    }
}
fn rect(x: f64, y: f64, w: f64, h: f64) -> Contour {
    polygon(&[[x, y], [x, y + h], [x + w, y + h], [x + w, y]])
}
pub fn builtins() -> &'static BTreeMap<u64, Snippet> {
    static LIB: std::sync::OnceLock<BTreeMap<u64, Snippet>> = std::sync::OnceLock::new();
    LIB.get_or_init(|| {
        let k = 0.5522847498307936;
        let r = 250.;
        let c = [300., 300.];
        let ellipse = Contour {
            smooth: [0, 1, 2, 3].into(),
            start: [c[0] + r, c[1]],
            closed: true,
            segments: vec![
                Segment::Cubic {
                    control1: [550., 300. + k * r],
                    control2: [300. + k * r, 550.],
                    end: [300., 550.],
                },
                Segment::Cubic {
                    control1: [300. - k * r, 550.],
                    control2: [50., 300. + k * r],
                    end: [50., 300.],
                },
                Segment::Cubic {
                    control1: [50., 300. - k * r],
                    control2: [300. - k * r, 50.],
                    end: [300., 50.],
                },
                Segment::Cubic {
                    control1: [300. + k * r, 50.],
                    control2: [550., 300. - k * r],
                    end: [550., 300.],
                },
            ],
        };
        [
            (1, "rectangle", vec![rect(50., 50., 500., 500.)]),
            (2, "ellipse", vec![ellipse]),
            (
                3,
                "triangle",
                vec![polygon(&[[50., 50.], [300., 550.], [550., 50.]])],
            ),
            (
                4,
                "diamond",
                vec![polygon(&[
                    [50., 300.],
                    [300., 550.],
                    [550., 300.],
                    [300., 50.],
                ])],
            ),
            (
                5,
                "mouth",
                vec![
                    rect(50., 50., 500., 500.),
                    polygon(&[[130., 130.], [470., 130.], [470., 470.], [130., 470.]]),
                ],
            ),
            (
                6,
                "work",
                vec![polygon(&[
                    [50., 50.],
                    [50., 120.],
                    [265., 120.],
                    [265., 480.],
                    [70., 480.],
                    [70., 550.],
                    [530., 550.],
                    [530., 480.],
                    [335., 480.],
                    [335., 120.],
                    [550., 120.],
                    [550., 50.],
                ])],
            ),
        ]
        .into_iter()
        .map(|(id, name, contours)| {
            (
                id,
                Snippet {
                    info: Info {
                        name: name.into(),
                        tags: String::new(),
                        category: if id >= 5 { "radical" } else { "shape" }.into(),
                        source: "Fonts original geometric templates".into(),
                        license: "CC0-1.0".into(),
                        favorite: false,
                    },
                    upm: 1000,
                    contours: Arc::new(contours),
                },
            )
        })
        .collect()
    })
}
