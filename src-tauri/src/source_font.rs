//! Specialized document previews keep the embedded font and rendered pixels in Rust.
use crate::AppState;
use tauri::{Manager, WebviewWindow};
enum Rendered {
    Color(font_core::color_fonts::Preview),
    Vector(
        Box<(
            font_core::proof::Prepared,
            font_core::FontDocument,
            font_core::proof::Settings,
        )>,
    ),
}
#[tauri::command]
pub async fn source_font_preview(
    window: WebviewWindow,
    epoch: u64,
    revision: u64,
    text: String,
    size: u32,
) -> Result<(), String> {
    crate::allowed(&window)?;
    let app = window.app_handle().clone();
    let (source, generation, dimensions) = {
        let state = app.state::<AppState>();
        let mut m = state.0.lock().map_err(|_| "state_poisoned")?;
        if m.epoch != epoch || m.editor.revision() != revision {
            return Err("Conflict".into());
        }
        if m.master_edit.is_some() {
            return Err("Conflict".into());
        }
        if m.has_draft() {
            return Err("edit_in_progress".into());
        }
        let source = m
            .editor
            .document()
            .source_font
            .clone()
            .ok_or("source_font_missing")?;
        m.proof.generation += 1;
        (
            source,
            m.proof.generation,
            [
                m.size[0].clamp(256., 2048.) as u32,
                m.size[1].clamp(256., 1024.) as u32,
            ],
        )
    };
    let preview = tauri::async_runtime::spawn_blocking(move || match source.kind {
        font_core::source_font::Kind::Color => {
            let bytes = if source.variable.is_some() {
                source.static_bytes()?
            } else {
                source.export()?
            };
            Ok::<_, String>(Rendered::Color(font_core::color_fonts::preview(
                &bytes,
                0,
                &text,
                size,
                source.palette,
                dimensions,
            )?))
        }
        font_core::source_font::Kind::Variable => {
            let doc = source.static_document()?;
            let settings = font_core::proof::Settings {
                text,
                size: f64::from(size),
                ..Default::default()
            };
            settings.validate()?;
            let font = font_core::proof::prepare(
                font_core::proof::subset(&doc, &settings.text)?,
                &settings.text,
            )?;
            font.warm_shaping(&settings)?;
            Ok(Rendered::Vector(Box::new((font, doc, settings))))
        }
    })
    .await
    .map_err(|e| e.to_string())??;
    let state = app.state::<AppState>();
    let mut m = state.0.lock().map_err(|_| "state_poisoned")?;
    if m.epoch != epoch
        || m.editor.revision() != revision
        || m.proof.generation != generation
        || m.master_edit.is_some()
    {
        return Err("Conflict".into());
    }
    match preview {
        Rendered::Color(p) => {
            m.proof.color = Some(std::sync::Arc::new(p));
            m.proof.font = None;
            m.proof.source = None;
        }
        Rendered::Vector(v) => {
            let (font, doc, settings) = *v;
            m.proof.color = None;
            m.proof.font = Some(std::sync::Arc::new(font));
            m.proof.source = Some(std::sync::Arc::new(doc));
            m.proof.settings = settings;
        }
    }
    m.proof.reference = None;
    m.proof.active = true;
    drop(m);
    crate::publish(&app);
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::{
        model::{Action, Model},
        recovery_store::Store,
    };
    use font_core::{source_font::SourceFont, workspace::Workspace};
    #[test]
    fn palette_history_and_recovery_keep_embedded_colors_without_source_file() {
        let bytes = include_bytes!("../../crates/font-core/tests/data/color/colrv1-paints.ttf");
        let mut m = Model::new(Workspace::default());
        m.replace_document(SourceFont::color_document(bytes, 0).unwrap(), None)
            .unwrap();
        let snapshot = serde_json::to_value(m.snapshot()).unwrap();
        assert!(snapshot["source_font"].get("bytes").is_none());
        let original = m.editor.document().clone();
        m.action(Action::SourcePalette {
            revision: 0,
            palette: 0,
            entry: Some(0),
            color: Some([20, 40, 60, 128]),
        })
        .unwrap();
        let edited = m.editor.document().clone();
        let source = edited.source_font.as_ref().unwrap();
        let preview =
            font_core::color_fonts::preview(&source.export().unwrap(), 0, "F", 100, 0, [256, 256])
                .unwrap();
        let at = (85 * 256 + 45) * 4;
        let p = &preview.image.rgba()[at..at + 4];
        for (a, b) in p.iter().zip([20u8, 40, 60, 128]) {
            assert!(
                a.abs_diff(b) <= 2,
                "edited palette must reach renderer: {p:?}"
            );
        }
        m.action(Action::Undo {
            revision: m.editor.revision(),
        })
        .unwrap();
        assert_eq!(m.editor.document(), &original);
        m.action(Action::Redo {
            revision: m.editor.revision(),
        })
        .unwrap();
        assert_eq!(m.editor.document(), &edited);
        let root =
            std::env::temp_dir().join(format!("fonts-color-recovery-{}", edited.document_uuid));
        let store = Store::new(root.clone()).unwrap();
        store
            .checkpoint(&edited, m.epoch, m.editor.revision(), None)
            .unwrap();
        drop(store);
        let store = Store::new(root.clone()).unwrap();
        let (list, _) = store.candidates().unwrap();
        let (doc, lease) = store.candidate(&list[0].id).unwrap();
        assert_eq!(doc, edited);
        m.replace_document(doc, None).unwrap();
        assert_eq!(
            m.editor
                .document()
                .source_font
                .as_ref()
                .unwrap()
                .palette_colors()
                .unwrap()[0],
            [20, 40, 60, 128]
        );
        m.pointer("down", 100., 100.).unwrap();
        m.pointer("move", 150., 150.).unwrap();
        m.pointer("up", 150., 150.).unwrap();
        assert_eq!(m.editor.document(), &edited);
        drop(lease);
        drop(store);
        std::fs::remove_dir_all(root).unwrap();
    }
}

#[tauri::command]
pub async fn source_font_static(
    window: WebviewWindow,
    epoch: u64,
    revision: u64,
) -> Result<(), String> {
    crate::allowed(&window)?;
    let _guard = crate::files::FileGuard::acquire()?;
    let app = window.app_handle().clone();
    let source = {
        let state = app.state::<AppState>();
        let m = state.0.lock().map_err(|_| "state_poisoned")?;
        if m.epoch != epoch || m.editor.revision() != revision {
            return Err("Conflict".into());
        }
        m.editor
            .document()
            .source_font
            .clone()
            .ok_or("source_font_missing")?
    };
    let doc = tauri::async_runtime::spawn_blocking(move || source.static_document())
        .await
        .map_err(|e| e.to_string())??;
    {
        let state = app.state::<AppState>();
        let m = state.0.lock().map_err(|_| "state_poisoned")?;
        if m.epoch != epoch || m.editor.revision() != revision {
            return Err("Conflict".into());
        }
    }
    crate::files::add_document(&window, doc, None).await?;
    Ok(())
}

#[cfg(test)]
mod variable_tests {
    use crate::{
        model::{Action, Model},
        recovery_store::Store,
    };
    use font_core::{
        source_font::{variable::Edit, SourceFont},
        workspace::Workspace,
    };
    #[test]
    fn variable_coordinates_recover_in_their_document_and_keep_history() {
        let bytes = include_bytes!("../../crates/font-core/tests/data/variations/shaping-gvar.ttf");
        let mut m = Model::new(Workspace::default());
        m.replace_document(SourceFont::variable_document(bytes, 0, None).unwrap(), None)
            .unwrap();
        let original = m.editor.document().clone();
        m.action(Action::SourceVariation {
            revision: 0,
            operation: Edit::Coordinates {
                values: [("wght".into(), 900.), ("wdth".into(), 100.)].into(),
            },
        })
        .unwrap();
        let changed = m.editor.document().clone();
        m.action(Action::Undo {
            revision: m.editor.revision(),
        })
        .unwrap();
        assert_eq!(m.editor.document(), &original);
        m.action(Action::Redo {
            revision: m.editor.revision(),
        })
        .unwrap();
        assert_eq!(m.editor.document(), &changed);
        let root =
            std::env::temp_dir().join(format!("fonts-variable-recovery-{}", changed.document_uuid));
        let store = Store::new(root.clone()).unwrap();
        store
            .checkpoint(&changed, m.epoch, m.editor.revision(), None)
            .unwrap();
        drop(store);
        let store = Store::new(root.clone()).unwrap();
        let (list, _) = store.candidates().unwrap();
        let (doc, lease) = store.candidate(&list[0].id).unwrap();
        assert_eq!(doc, changed);
        m.replace_document(doc, None).unwrap();
        let source = m.editor.document().source_font.as_ref().unwrap();
        assert_eq!(source.variable.as_ref().unwrap().coordinates["wght"], 900.);
        let copy = source.static_document().unwrap();
        assert!(copy.imported_layout.positioning());
        assert_eq!(copy.metadata.weight, 900);
        assert_eq!(copy.glyphs[&1].advance, 800.);
        drop(lease);
        drop(store);
        std::fs::remove_dir_all(root).unwrap();
    }
}
