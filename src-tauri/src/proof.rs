use crate::AppState;
use font_core::proof::{self, Prepared, Settings};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::{Manager, WebviewWindow};
#[derive(Default)]
pub struct Proof {
    pub active: bool,
    pub color: Option<Arc<font_core::color_fonts::Preview>>,
    pub settings: Settings,
    pub font: Option<Arc<Prepared>>,
    pub reference: Option<Arc<Prepared>>,
    pub generation: u64,
    pub source: Option<Arc<font_core::FontDocument>>,
}
#[derive(Serialize)]
pub struct Summary {
    pub color: bool,
    pub active: bool,
    pub settings: Settings,
    pub missing: Vec<u32>,
    pub empty: usize,
    pub comparison: bool,
    pub height: f64,
    pub source: Option<String>,
}
impl Proof {
    pub fn summary(&self, width: f64) -> Summary {
        Summary {
            active: self.active,
            color: self.color.is_some(),
            source: self.color.as_ref().map(|c| c.label.clone()).or_else(|| {
                self.source
                    .as_ref()
                    .map(|d| format!("{} · {}", d.family, d.metadata.style))
            }),
            settings: self.settings.clone(),
            missing: self
                .font
                .as_ref()
                .map(|f| f.missing.iter().map(|c| *c as u32).collect())
                .unwrap_or_default(),
            empty: self.font.as_ref().map(|f| f.empty).unwrap_or(0),
            comparison: self.reference.is_some(),
            height: self
                .font
                .iter()
                .chain(self.reference.iter())
                .map(|f| {
                    proof::layout(
                        f,
                        &self.settings,
                        if self.reference.is_some() {
                            width / 2.
                        } else {
                            width
                        },
                    )
                    .height
                })
                .fold(0., f64::max),
        }
    }
}
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Operation {
    Prepare { settings: Settings },
    Close,
    Pin,
    ClearReference,
    Scroll { value: f64 },
}
#[tauri::command]
pub async fn proof_action(
    window: WebviewWindow,
    epoch: u64,
    revision: u64,
    operation: Operation,
) -> Result<(), String> {
    if window.label() != "main" && window.label() != "panel-metrics" {
        return Err("main_window_only".into());
    }
    let app = window.app_handle().clone();
    let prepared = {
        let state = app.state::<AppState>();
        let mut m = state.0.lock().map_err(|_| "state_poisoned")?;
        if m.epoch != epoch || m.editor.revision() != revision {
            return Err("Conflict".into());
        }
        match operation {
            Operation::Prepare { settings } => {
                settings.validate()?;
                if m.has_draft() {
                    return Err("edit_in_progress".into());
                }
                m.proof.generation += 1;
                Some((
                    proof::subset(
                        m.proof.source.as_deref().unwrap_or(m.editor.document()),
                        &settings.text,
                    )?,
                    settings,
                    m.proof.generation,
                ))
            }
            Operation::Close => {
                m.proof.active = false;
                m.proof.source = None;
                m.proof.color = None;
                m.proof.generation += 1;
                None
            }
            Operation::Pin => {
                m.proof.reference = m.proof.font.clone();
                None
            }
            Operation::ClearReference => {
                m.proof.reference = None;
                None
            }
            Operation::Scroll { value } => {
                if !value.is_finite() || !(0. ..=1_000_000.).contains(&value) {
                    return Err("proof_invalid_settings".into());
                }
                m.proof.settings.scroll = value;
                None
            }
        }
    };
    if let Some((subset, settings, generation)) = prepared {
        let text = settings.text.clone();
        let font = tauri::async_runtime::spawn_blocking(move || proof::prepare(subset, &text))
            .await
            .map_err(|e| e.to_string())??;
        let state = app.state::<AppState>();
        let mut m = state.0.lock().map_err(|_| "state_poisoned")?;
        if m.epoch != epoch || m.editor.revision() != revision || m.proof.generation != generation {
            return Err("Conflict".into());
        }
        m.proof.color = None;
        m.proof.font = Some(Arc::new(font));
        m.proof.settings = settings;
        m.proof.active = true;
    }
    crate::publish(&app);
    Ok(())
}
