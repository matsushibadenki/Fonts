use super::*;
use font_hinting_native::shape::{self, Glyph as NativeGlyph};
use std::{
    ops::Deref,
    sync::{Arc, Mutex},
};
type Key = (String, Direction, Orientation, bool, String);
pub(super) type Cache = Arc<Mutex<BTreeMap<Key, Arc<Vec<Line>>>>>;
struct Glyph {
    data: NativeGlyph,
    rotated: bool,
    blank: bool,
    word: bool,
}
impl Deref for Glyph {
    type Target = NativeGlyph;
    fn deref(&self) -> &NativeGlyph {
        &self.data
    }
}
pub(super) struct Line {
    text: String,
    glyphs: Vec<Glyph>,
}
fn ignored(c: char) -> bool {
    matches!(c,'\u{200c}'|'\u{200d}'|'\u{fe00}'..='\u{fe0f}'|'\u{e0100}'..='\u{e01ef}')
}
impl Prepared {
    pub fn warm_shaping(&self, settings: &Settings) -> Result<(), String> {
        self.shaped_lines(settings).map(|_| ())
    }
    pub(super) fn shaped_missing(&self, text: &str) -> Result<Vec<char>, String> {
        let lines = self.shaped_lines(&Settings {
            text: text.into(),
            ..Default::default()
        })?;
        let mut missing = BTreeSet::new();
        for line in lines.iter() {
            let mut clusters: Vec<_> = line.glyphs.iter().map(|g| g.cluster as usize).collect();
            clusters.sort_unstable();
            clusters.dedup();
            for g in line.glyphs.iter().filter(|g| g.glyph == 0 && !g.blank) {
                let at = g.cluster as usize;
                let end = clusters
                    .get(clusters.partition_point(|&n| n <= at))
                    .copied()
                    .unwrap_or(line.text.len());
                let span = line.text.get(at..end).unwrap_or("");
                let absent: Vec<_> = span
                    .chars()
                    .filter(|c| !c.is_control() && !ignored(*c) && !self.mapping.contains_key(c))
                    .collect();
                if absent.is_empty() {
                    if let Some(c) = span.chars().find(|c| !c.is_control() && !ignored(*c)) {
                        missing.insert(c);
                    }
                } else {
                    missing.extend(absent);
                }
            }
        }
        Ok(missing.into_iter().collect())
    }
    fn shaped_lines(&self, settings: &Settings) -> Result<Arc<Vec<Line>>, String> {
        settings.validate()?;
        let text = settings.text.replace("\r\n", "\n").replace('\r', "\n");
        let key = (
            text.clone(),
            settings.direction,
            settings.orientation,
            settings.kerning,
            settings.language.clone(),
        );
        if let Some(value) = self.shaped.lock().map_err(|_| "state_poisoned")?.get(&key) {
            return Ok(value.clone());
        }
        let native = shape::Font::new(&self.bytes)?;
        let mut lines = vec![];
        let mut count = 0;
        for line in text.split('\n') {
            let text: String = line
                .chars()
                .filter(|c| !c.is_control() || *c == '\t')
                .collect();
            let vertical = settings.direction == Direction::Vertical;
            let mixed = vertical && settings.orientation == Orientation::Mixed;
            let mut segments = vec![];
            let bidi = unicode_bidi::BidiInfo::new(&text, None);
            for paragraph in &bidi.paragraphs {
                let (_, runs) = bidi.visual_runs(paragraph, paragraph.range.clone());
                for range in runs {
                    let rtl = bidi.levels[range.start].is_rtl();
                    for (from, to, script) in script_runs(&text, range, rtl) {
                        let mut orientation_runs: Vec<(usize, usize, bool, u32, bool)> = vec![];
                        let mut previous = false;
                        for (local, c) in text[from..to].char_indices() {
                            let at = from + local;
                            let rotate = if mixed {
                                if shape::extender(c) && local > 0 {
                                    previous
                                } else {
                                    orientation::orientation(c) == orientation::Orientation::R
                                }
                            } else {
                                false
                            };
                            if let Some(last) =
                                orientation_runs.last_mut().filter(|s| s.2 == rotate)
                            {
                                last.1 = at + c.len_utf8();
                            } else {
                                orientation_runs.push((at, at + c.len_utf8(), rotate, script, rtl));
                            }
                            previous = rotate;
                        }
                        if rtl {
                            orientation_runs.reverse();
                        }
                        segments.extend(orientation_runs);
                    }
                }
            }
            let mut glyphs = vec![];
            for (start, end, rotate, script, rtl) in segments {
                let part = &text[start..end];
                let data = native.shape_direction(
                    part,
                    &settings.language,
                    vertical && !rotate,
                    settings.kerning,
                    Some(rtl),
                    script,
                )?;
                let transformed = mixed
                    && !rotate
                    && part
                        .chars()
                        .any(|c| orientation::orientation(c) == orientation::Orientation::Tr);
                let mut horizontal = BTreeMap::<u32, Vec<NativeGlyph>>::new();
                if transformed {
                    for glyph in native.shape_direction(
                        part,
                        &settings.language,
                        false,
                        settings.kerning,
                        Some(rtl),
                        script,
                    )? {
                        horizontal.entry(glyph.cluster).or_default().push(glyph);
                    }
                }
                let mut at = 0;
                while at < data.len() {
                    let mut end = at + 1;
                    while end < data.len() && data[end].cluster == data[at].cluster {
                        end += 1;
                    }
                    let group = &data[at..end];
                    let c = part
                        .get(group[0].cluster as usize..)
                        .and_then(|s| s.chars().next())
                        .unwrap_or(' ');
                    let h = horizontal
                        .get(&group[0].cluster)
                        .map(Vec::as_slice)
                        .unwrap_or(&[]);
                    let fallback = transformed
                        && orientation::orientation(c) == orientation::Orientation::Tr
                        && group.iter().map(|g| g.glyph).eq(h.iter().map(|g| g.glyph));
                    let group = if fallback { h } else { group };
                    for mut g in group.iter().copied() {
                        let blank =
                            c == '\t' || (ignored(c) && g.x_advance == 0 && g.y_advance == 0);
                        if c == '\t' {
                            g.x_advance = i32::from(self.document.units_per_em) * 2;
                            g.y_advance = -i32::from(self.document.units_per_em) * 2;
                        }
                        g.cluster += start as u32;
                        glyphs.push(Glyph {
                            data: g,
                            rotated: rotate || fallback,
                            blank,
                            word: !cjk_script(script)
                                && (c.is_alphanumeric() || c == '\'' || shape::extender(c)),
                        });
                    }
                    at = end;
                }
            }
            count += glyphs.len();
            if count > 16384 {
                return Err("layout_shaping_failed".into());
            }
            lines.push(Line { text, glyphs });
        }
        let lines = Arc::new(lines);
        let mut cache = self.shaped.lock().map_err(|_| "state_poisoned")?;
        if cache.len() >= 8 {
            cache.clear();
        }
        cache.insert(key, lines.clone());
        Ok(lines)
    }
}
fn advance(g: &Glyph, vertical: bool) -> f64 {
    f64::from(if vertical && !g.rotated {
        g.y_advance.abs()
    } else {
        g.x_advance.abs()
    })
}
fn cjk_script(script: u32) -> bool {
    [
        *b"Hani", *b"Hira", *b"Kana", *b"Hrkt", *b"Hang", *b"Bopo", *b"Yiii",
    ]
    .iter()
    .any(|s| u32::from_be_bytes(*s) == script)
}
pub(super) fn layout(font: &Prepared, settings: &Settings, width: f64) -> Layout {
    let Ok(lines) = font.shaped_lines(settings) else {
        return Layout {
            placements: vec![],
            height: 48.,
        };
    };
    let scale = settings.size / f64::from(font.document.units_per_em);
    let margin = 24.;
    let left = margin;
    let right = (width - margin).max(left + settings.size);
    let leading = (font.document.ascender - font.document.descender)
        .max(f64::from(font.document.units_per_em))
        * scale
        * settings.leading;
    let mut placements = vec![];
    let vertical = settings.direction == Direction::Vertical;
    let mut x = if vertical {
        right - settings.size / 2.
    } else {
        left
    };
    let mut y = if vertical {
        margin
    } else {
        margin + font.document.ascender * scale
    };
    let mut section = margin;
    for (line_index, line) in lines.iter().enumerate() {
        if line_index > 0 {
            if vertical {
                next_column(&mut x, &mut y, &mut section, left, right, leading, settings);
            } else {
                x = left;
                y += leading;
            }
        }
        let mut at = 0;
        let mut was_word = false;
        while at < line.glyphs.len() {
            let mut end = at + 1;
            while end < line.glyphs.len() && line.glyphs[end].cluster == line.glyphs[at].cluster {
                end += 1;
            }
            let group = &line.glyphs[at..end];
            let span = group.iter().map(|g| advance(g, vertical)).sum::<f64>() * scale
                + settings.tracking * settings.size;
            if group[0].word && !was_word {
                let token: Vec<_> = line.glyphs[at..].iter().take_while(|g| g.word).collect();
                let mut last = None;
                let mut total = 0.;
                for g in token {
                    total += advance(g, vertical) * scale;
                    if last != Some(g.cluster) {
                        total += settings.tracking * settings.size;
                        last = Some(g.cluster);
                    }
                }
                if vertical {
                    if total <= settings.column_height
                        && y > section
                        && y + total > section + settings.column_height
                    {
                        next_column(&mut x, &mut y, &mut section, left, right, leading, settings);
                    }
                } else if total <= right - left && x > left && x + total > right {
                    x = left;
                    y += leading;
                }
            }
            was_word = group[0].word;
            if vertical {
                if y > section && y + span > section + settings.column_height {
                    next_column(&mut x, &mut y, &mut section, left, right, leading, settings);
                }
            } else if x > left && x + span > right {
                x = left;
                y += leading;
            }
            for g in group {
                if let Some(source) = font.document.glyphs.get(&g.glyph).filter(|_| !g.blank) {
                    let rotated = vertical && g.rotated;
                    let px = if rotated {
                        x + (f64::from(g.y_offset)
                            - (font.document.ascender + font.document.descender) / 2.)
                            * scale
                    } else {
                        x + f64::from(g.x_offset) * scale
                    };
                    let baseline = if rotated {
                        y + f64::from(g.x_offset) * scale
                    } else {
                        y - f64::from(g.y_offset) * scale
                    };
                    let (hit_left, hit_width, top, bottom) = if rotated {
                        let (min, max) = crate::snippets::bounds(&source.contours).unwrap_or((
                            [0., font.document.descender],
                            [source.advance, font.document.ascender],
                        ));
                        (
                            px + min[1] * scale,
                            ((max[1] - min[1]) * scale).max(1.),
                            baseline + min[0] * scale,
                            baseline + max[0] * scale,
                        )
                    } else {
                        (
                            px,
                            (if vertical {
                                source.advance * scale
                            } else {
                                advance(g, false) * scale
                            })
                            .max(1.),
                            baseline - font.document.ascender * scale,
                            baseline - font.document.descender * scale,
                        )
                    };
                    placements.push(Placement {
                        rotated,
                        left: hit_left,
                        glyph: g.glyph,
                        source: if g.glyph == 0 {
                            None
                        } else {
                            font.original.get(&g.glyph).copied()
                        },
                        missing: g.glyph == 0,
                        x: px,
                        baseline,
                        width: hit_width,
                        top,
                        bottom,
                    });
                }
                if vertical {
                    y += advance(g, true) * scale;
                } else {
                    x += advance(g, false) * scale;
                }
            }
            if vertical {
                y += settings.tracking * settings.size;
            } else {
                x += settings.tracking * settings.size;
            }
            at = end;
        }
    }
    let height = if placements.is_empty() {
        48.
    } else if vertical {
        section + settings.column_height + margin
    } else {
        y - font.document.descender * scale + margin
    };
    Layout { placements, height }
}
fn next_column(
    x: &mut f64,
    y: &mut f64,
    section: &mut f64,
    left: f64,
    right: f64,
    leading: f64,
    settings: &Settings,
) {
    *x -= leading;
    if *x < left + settings.size / 2. {
        *x = right - settings.size / 2.;
        *section += settings.column_height + 48.;
    }
    *y = *section;
}

fn script_runs(text: &str, range: std::ops::Range<usize>, rtl: bool) -> Vec<(usize, usize, u32)> {
    let strong = |s: u32| {
        s != u32::from_be_bytes(*b"Zyyy")
            && s != u32::from_be_bytes(*b"Zinh")
            && s != u32::from_be_bytes(*b"Zzzz")
    };
    let mut script = text[range.clone()]
        .chars()
        .map(shape::script)
        .find(|&s| strong(s))
        .unwrap_or(0);
    let mut out: Vec<(usize, usize, u32)> = vec![];
    for (offset, c) in text[range.clone()].char_indices() {
        let at = range.start + offset;
        let value = shape::script(c);
        if strong(value) && !shape::extender(c) {
            script = value;
        }
        if let Some(last) = out.last_mut().filter(|r| r.2 == script) {
            last.1 = at + c.len_utf8();
        } else {
            out.push((at, at + c.len_utf8(), script));
        }
    }
    if rtl {
        out.reverse();
    }
    out
}
