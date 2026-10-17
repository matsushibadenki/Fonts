//! Shared catalog selection and uncommitted outline placement; windows own no outlines.
use crate::{model::Model, AppState};
use font_core::{
    reference_images::ReferenceImage,
    reference_transform::{self, Handle},
    snippets::{self, Info, Snippet},
    workspace::{Panel, Tool},
    GlyphId, Point,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use tauri::{Manager, WebviewWindow};
#[derive(Default)]
pub struct State {
    pub contours: BTreeSet<usize>,
    pub selected: Option<String>,
    pub draft: Option<Draft>,
}
pub struct Draft {
    pub source: Snippet,
    pub placement: ReferenceImage,
    pub owner: String,
    drag: Option<Drag>,
}
struct Drag {
    original: ReferenceImage,
    start: Point,
    handle: Handle,
    token: String,
}
#[derive(Serialize)]
pub struct Entry {
    pub key: String,
    pub builtin: bool,
    pub info: Info,
    pub contours: usize,
}
#[derive(Serialize)]
pub struct Summary {
    pub entries: Vec<Entry>,
    pub selected: Option<String>,
    pub contours: BTreeSet<usize>,
    pub placement: Option<ReferenceImage>,
}
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Action {
    Choose { key: String },
    Contour { index: usize, selected: bool },
    All { selected: bool },
    Capture { info: Info },
    Duplicate { key: String },
    Update { key: String, info: Info },
    Remove { key: String },
    Begin { key: String },
    Transform { placement: ReferenceImage },
    Commit,
    Cancel,
    Open,
}
impl Model {
    pub fn snippet(&self, key: &str) -> Result<Snippet, String> {
        let (kind, id) = key.split_once(':').ok_or("InvalidDocument")?;
        let id = id.parse::<u64>().map_err(|_| "InvalidDocument")?;
        match kind {
            "builtin" => snippets::builtins().get(&id),
            "user" => self.editor.document().snippets.entries.get(&id),
            _ => None,
        }
        .cloned()
        .ok_or_else(|| "InvalidDocument".into())
    }
    pub fn snippet_summary(&self) -> Summary {
        let entry = |(id, s): (&u64, &Snippet), builtin| Entry {
            key: format!("{}:{id}", if builtin { "builtin" } else { "user" }),
            builtin,
            info: s.info.clone(),
            contours: s.contours.len(),
        };
        Summary {
            entries: snippets::builtins()
                .iter()
                .map(|e| entry(e, true))
                .chain(
                    self.editor
                        .document()
                        .snippets
                        .entries
                        .iter()
                        .map(|e| entry(e, false)),
                )
                .collect(),
            selected: self
                .snippets
                .selected
                .clone()
                .filter(|k| self.snippet(k).is_ok()),
            contours: self.snippets.contours.clone(),
            placement: self.snippets.draft.as_ref().map(|d| d.placement.clone()),
        }
    }
    pub fn snippet_action(&mut self, action: Action, owner: &str) -> Result<(), String> {
        if self.proof.active
            || self.drag.is_some()
            || self.images.drag.is_some()
            || !self.pen.is_empty()
        {
            return Err("edit_in_progress".into());
        }
        if self.snippets.draft.is_some()
            && !matches!(
                action,
                Action::Transform { .. } | Action::Commit | Action::Cancel | Action::Open
            )
        {
            return Err("edit_in_progress".into());
        }
        let revision = self.editor.revision();
        match action {
            Action::Choose { key } => {
                self.snippet(&key)?;
                self.snippets.selected = Some(key);
            }
            Action::Contour { index, selected } => {
                if index >= self.editor.document().glyphs[&self.glyph].contours.len() {
                    return Err("InvalidGeometry".into());
                }
                if selected {
                    self.snippets.contours.insert(index);
                } else {
                    self.snippets.contours.remove(&index);
                }
            }
            Action::All { selected } => {
                self.snippets.contours = if selected {
                    (0..self.editor.document().glyphs[&self.glyph].contours.len()).collect()
                } else {
                    BTreeSet::new()
                };
            }
            Action::Capture { info } => {
                let id = self
                    .editor
                    .capture_snippet(revision, self.glyph, &self.snippets.contours, info)
                    .map_err(|e| e.to_string())?;
                self.snippets.selected = Some(format!("user:{id}"));
            }
            Action::Duplicate { key } => {
                let mut snippet = self.snippet(&key)?;
                use font_core::workspace::Locale;
                let names = match self.workspace.locale {
                    Locale::En => [
                        "Rectangle",
                        "Ellipse",
                        "Triangle",
                        "Diamond",
                        "口 · geometric",
                        "工 · geometric",
                    ],
                    Locale::Ja => [
                        "長方形",
                        "楕円",
                        "三角形",
                        "ひし形",
                        "口・幾何形",
                        "工・幾何形",
                    ],
                    Locale::ZhCn => [
                        "矩形",
                        "椭圆",
                        "三角形",
                        "菱形",
                        "口 · 几何形",
                        "工 · 几何形",
                    ],
                };
                let base = key
                    .strip_prefix("builtin:")
                    .and_then(|s| s.parse::<usize>().ok())
                    .and_then(|i| i.checked_sub(1))
                    .and_then(|i| names.get(i))
                    .copied()
                    .unwrap_or(&snippet.info.name);
                let suffix = match self.workspace.locale {
                    Locale::En => "copy",
                    Locale::Ja => "コピー",
                    Locale::ZhCn => "副本",
                };
                let mut name = base.to_string();
                while name.len() > 230 {
                    name.pop();
                }
                snippet.info.name = format!("{name} ({suffix})");
                let id = self
                    .editor
                    .add_snippet(revision, snippet)
                    .map_err(|e| e.to_string())?;
                self.snippets.selected = Some(format!("user:{id}"));
            }
            Action::Update { key, info } => {
                let id = user_id(&key)?;
                self.editor
                    .update_snippet(revision, id, info)
                    .map_err(|e| e.to_string())?;
            }
            Action::Remove { key } => {
                self.editor
                    .remove_snippet(revision, user_id(&key)?)
                    .map_err(|e| e.to_string())?;
                if self.snippets.selected.as_ref() == Some(&key) {
                    self.snippets.selected = None;
                }
            }
            Action::Begin { key } => {
                let source = self.snippet(&key)?;
                let doc = self.editor.document();
                let advance = doc.glyphs[&self.glyph].advance;
                let placement = source
                    .placement(
                        doc.units_per_em,
                        [
                            if advance > 0. {
                                advance / 2.
                            } else {
                                f64::from(doc.units_per_em) / 2.
                            },
                            (doc.ascender + doc.descender) / 2.,
                        ],
                    )
                    .map_err(|e| e.to_string())?;
                source.transformed(&placement).map_err(|e| e.to_string())?;
                self.snippets.selected = Some(key);
                self.snippets.draft = Some(Draft {
                    source,
                    placement,
                    owner: owner.into(),
                    drag: None,
                });
                self.images = Default::default();
                self.selected = None;
                self.tool = Tool::Snippets;
            }
            Action::Transform { placement } => {
                let d = self.snippets.draft.as_mut().ok_or("InvalidDocument")?;
                if d.drag.is_some() {
                    return Err("edit_in_progress".into());
                }
                d.source
                    .transformed(&placement)
                    .map_err(|e| e.to_string())?;
                d.placement = placement;
            }
            Action::Commit => {
                let d = self.snippets.draft.as_ref().ok_or("InvalidDocument")?;
                if d.drag.is_some() {
                    return Err("edit_in_progress".into());
                }
                let first = self.editor.document().glyphs[&self.glyph].contours.len();
                self.editor
                    .insert_snippet(revision, self.glyph, &d.source, &d.placement)
                    .map_err(|e| e.to_string())?;
                self.snippets.contours =
                    (first..self.editor.document().glyphs[&self.glyph].contours.len()).collect();
                self.snippets.draft = None;
                self.tool = Tool::Nodes;
            }
            Action::Cancel => {
                self.snippets.draft = None;
                self.tool = Tool::Nodes;
            }
            Action::Open => {
                if let Some(font_core::workspace::Placement::Docked { .. }) =
                    self.workspace.panels.get(&Panel::Snippets)
                {
                    self.workspace
                        .activate(self.workspace.revision, Panel::Snippets)
                        .map_err(|e| e.to_string())?;
                }
            }
        }
        Ok(())
    }
    pub fn snippet_pointer(
        &mut self,
        phase: &str,
        screen: Point,
        shift: bool,
        token: &str,
    ) -> Result<(), String> {
        if screen.iter().any(|v| !v.is_finite()) || token.is_empty() || token.len() > 100 {
            return Err("InvalidGeometry".into());
        }
        let v = self.viewport();
        let p = v.document(screen);
        let d = self.snippets.draft.as_mut().ok_or("InvalidDocument")?;
        match phase {
            "down" => {
                if d.drag.is_some() {
                    return Err("edit_in_progress".into());
                }
                let handle = reference_transform::handles(&d.placement, v.scale())
                    .into_iter()
                    .find(|(_, p)| {
                        let q = v.screen(*p);
                        (q[0] - screen[0]).hypot(q[1] - screen[1]) <= 8.
                    })
                    .map(|(h, _)| h)
                    .or_else(|| d.placement.hit_test(p).then_some(Handle::Move));
                if let Some(handle) = handle {
                    d.drag = Some(Drag {
                        original: d.placement.clone(),
                        start: p,
                        handle,
                        token: token.into(),
                    });
                }
            }
            "move" => {
                if let Some(drag) = &d.drag {
                    if drag.token != token {
                        return Err("Conflict".into());
                    }
                    let placement = reference_transform::transform(
                        &drag.original,
                        drag.handle,
                        drag.start,
                        p,
                        shift,
                    )
                    .map_err(|e| e.to_string())?;
                    d.source
                        .transformed(&placement)
                        .map_err(|e| e.to_string())?;
                    d.placement = placement;
                }
            }
            "up" | "cancel" => {
                if let Some(drag) = &d.drag {
                    if drag.token != token {
                        return Err("Conflict".into());
                    }
                    if phase == "cancel" {
                        d.placement = drag.original.clone();
                    }
                    d.drag = None;
                }
            }
            _ => return Err("invalid_pointer_phase".into()),
        }
        Ok(())
    }
}
fn user_id(key: &str) -> Result<u64, String> {
    key.strip_prefix("user:")
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| "snippet_readonly".into())
}
#[tauri::command]
pub fn snippet_action(
    window: WebviewWindow,
    state: tauri::State<AppState>,
    epoch: u64,
    revision: u64,
    glyph: GlyphId,
    action: Action,
) -> Result<(), String> {
    crate::allowed(&window)?;
    {
        let mut m = state.0.lock().map_err(|_| "state_poisoned")?;
        if m.epoch != epoch || m.editor.revision() != revision || m.glyph != glyph {
            return Err("Conflict".into());
        }
        m.snippet_action(action, window.label())?;
    }
    crate::persist(window.app_handle());
    crate::publish(window.app_handle());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Action as Edit;
    use font_core::workspace::Workspace;
    fn model() -> Model {
        Model::new(Workspace::default())
    }
    #[test]
    fn preview_pointer_numeric_cancel_and_commit_only_change_document_on_confirm() {
        let mut m = model();
        let before = m.editor.document().clone();
        m.snippet_action(
            Action::Begin {
                key: "builtin:2".into(),
            },
            "panel-snippets",
        )
        .unwrap();
        assert!(m.has_draft());
        assert_eq!(m.visible_glyph().contours.len(), 3);
        let center = m
            .viewport()
            .screen(m.snippets.draft.as_ref().unwrap().placement.center);
        m.snippet_pointer("down", center, false, "a").unwrap();
        m.snippet_pointer("move", [center[0] + 20., center[1] + 30.], false, "a")
            .unwrap();
        assert_eq!(m.editor.document(), &before);
        assert_eq!(m.editor.revision(), 0);
        assert!(m.snippet_pointer("up", center, false, "old").is_err());
        m.snippet_pointer("up", center, false, "a").unwrap();
        m.snippet_action(Action::Commit, "main").unwrap();
        assert!(!m.has_draft());
        assert_eq!(m.editor.revision(), 1);
        assert_eq!(m.snippets.contours, BTreeSet::from([2]));
        m.action(Edit::Undo { revision: 1 }).unwrap();
        assert_eq!(m.editor.document(), &before);
        m.action(Edit::Redo { revision: 2 }).unwrap();
        assert_eq!(m.editor.document().glyphs[&1].contours.len(), 3);
        m.snippet_action(
            Action::Begin {
                key: "builtin:1".into(),
            },
            "main",
        )
        .unwrap();
        m.action(Edit::Cancel).unwrap();
        assert!(!m.has_draft());
        assert_eq!(m.editor.revision(), 3);
    }
    #[test]
    fn preview_blocks_interleaving_and_switching_discards_draft() {
        let mut m = model();
        m.snippet_action(
            Action::Begin {
                key: "builtin:5".into(),
            },
            "main",
        )
        .unwrap();
        assert!(m
            .action(Edit::Advance {
                value: 900.,
                revision: 0,
                id: 1
            })
            .is_err());
        assert!(m.image_context(0, 0, 1).is_err());
        assert!(m
            .snippet_action(
                Action::Remove {
                    key: "user:1".into()
                },
                "main"
            )
            .is_err());
        m.action(Edit::SelectGlyph { id: 2 }).unwrap();
        assert!(!m.has_draft());
        assert!(m.editor.document().glyphs[&2].contours.is_empty());
        assert!(m.snippets.contours.is_empty());
        m.snippet_action(
            Action::Begin {
                key: "builtin:3".into(),
            },
            "main",
        )
        .unwrap();
        m.action(Edit::Tool { tool: Tool::Pen }).unwrap();
        assert!(!m.has_draft());
    }
    #[test]
    fn capture_management_and_snapshot_keep_outline_data_in_rust() {
        let mut m = model();
        m.snippet_action(Action::All { selected: true }, "main")
            .unwrap();
        let info = snippets::builtins()[&1].info.clone();
        m.snippet_action(Action::Capture { info }, "main").unwrap();
        assert_eq!(m.editor.document().snippets.entries[&1].contours.len(), 2);
        m.snippet_action(
            Action::Duplicate {
                key: "user:1".into(),
            },
            "main",
        )
        .unwrap();
        assert_eq!(m.snippets.selected.as_deref(), Some("user:2"));
        assert!(m
            .snippet_action(
                Action::Remove {
                    key: "builtin:1".into()
                },
                "main"
            )
            .is_err());
        let s = serde_json::to_value(m.snapshot()).unwrap();
        assert_eq!(s["snippets"]["entries"][6]["contours"], 2);
        assert!(s["snippets"]["entries"][6].get("segments").is_none());
        m.snippet_action(
            Action::Begin {
                key: "user:2".into(),
            },
            "main",
        )
        .unwrap();
        let s = serde_json::to_value(m.snapshot()).unwrap();
        assert_eq!(s["images"].as_array().unwrap().len(), 0);
        assert!(s["snippets"]["placement"].is_object());
        m.replace_document(font_core::FontDocument::new("Other", "A").unwrap(), None)
            .unwrap();
        assert!(!m.has_draft());
        assert!(m.snippets.selected.is_none());
    }
}
