//! Integration coverage for stable selection and real project/recovery storage.
use crate::model::{Action, Model};
use font_core::{
    geometry::EditPoint,
    identity::{self, NodeRef, Uuid},
    workspace::Workspace,
    Contour, Segment,
};
fn model() -> Model {
    let mut m = Model::new(Workspace::default());
    let mut g = m.editor.document().glyphs[&m.glyph].clone();
    g.contours = vec![Contour {
        identity: Default::default(),
        smooth: Default::default(),
        start: [0., 0.],
        segments: vec![
            Segment::Cubic {
                control1: [20., 60.],
                control2: [80., 60.],
                end: [100., 0.],
            },
            Segment::Quadratic {
                control: [150., -60.],
                end: [200., 0.],
            },
            Segment::Line { end: [300., 100.] },
        ],
        closed: false,
    }];
    m.editor.replace_glyph(m.editor.revision(), g).unwrap();
    m
}
fn reference(m: &Model, anchor: usize) -> NodeRef {
    NodeRef {
        definition: None,
        document: m.editor.document().document_uuid,
        node: identity::node_id(
            &m.editor.document().glyphs[&m.glyph].contours[0],
            EditPoint::Anchor(anchor),
        )
        .unwrap(),
    }
}
#[test]
fn stable_selection_follows_topology_and_deleted_ids_never_select_another_point() {
    let mut m = model();
    let r = reference(&m, 2);
    let doc_id = r.document;
    m.action(Action::SplitSegment {
        id: m.glyph,
        contour: 0,
        segment: 0,
        revision: m.editor.revision(),
    })
    .unwrap();
    m.action(Action::SelectStableNode {
        reference: r,
        revision: m.editor.revision(),
    })
    .unwrap();
    assert_eq!(m.selected, Some((0, EditPoint::Anchor(3))));
    m.action(Action::SplitPath {
        id: m.glyph,
        contour: 0,
        anchor: 2,
        revision: m.editor.revision(),
    })
    .unwrap();
    m.action(Action::SelectStableNode {
        reference: r,
        revision: m.editor.revision(),
    })
    .unwrap();
    assert_eq!(m.selected, Some((1, EditPoint::Anchor(1))));
    m.action(Action::JoinPaths {
        id: m.glyph,
        contour: 0,
        anchor: 2,
        target: 1,
        target_end: false,
        revision: m.editor.revision(),
    })
    .unwrap();
    m.action(Action::SelectStableNode {
        reference: r,
        revision: m.editor.revision(),
    })
    .unwrap();
    assert_eq!(m.selected, Some((0, EditPoint::Anchor(3))));
    let state = serde_json::to_value(m.snapshot()).unwrap();
    assert_eq!(state["selection"]["node_id"], r.node.to_string());
    assert_eq!(state["document_uuid"], doc_id.to_string());
    m.action(Action::DeleteSegmentEnd {
        id: m.glyph,
        contour: 0,
        segment: 2,
        revision: m.editor.revision(),
    })
    .unwrap();
    let selected = m.selected;
    assert_eq!(
        m.action(Action::SelectStableNode {
            reference: r,
            revision: m.editor.revision()
        }),
        Err("node_reference_missing".into())
    );
    assert_eq!(m.selected, selected);
    m.action(Action::Undo {
        revision: m.editor.revision(),
    })
    .unwrap();
    m.action(Action::SelectStableNode {
        reference: r,
        revision: m.editor.revision(),
    })
    .unwrap();
    assert_eq!(m.selected, Some((0, EditPoint::Anchor(3))));
    assert_eq!(m.editor.document().document_uuid, doc_id);
    assert!(m
        .action(Action::SelectStableNode {
            reference: NodeRef {
                definition: None,
                document: Uuid::new_v4(),
                node: r.node
            },
            revision: m.editor.revision()
        })
        .is_err());
}
#[test]
fn stable_references_survive_project_save_restart_and_recovery_restart() {
    let m = model();
    let r = reference(&m, 2);
    let original = m.editor.document().clone();
    let root = std::env::temp_dir().join(format!("fonts-stable-ref-{}", Uuid::new_v4()));
    std::fs::create_dir(&root).unwrap();
    let path = root.join("project.json");
    font_core::storage::save(&path, &original).unwrap();
    let loaded = font_core::storage::load(&path).unwrap();
    let mut reopened = Model::new(Workspace::default());
    reopened.replace_document(loaded, Some(path)).unwrap();
    reopened
        .action(Action::SelectStableNode {
            reference: r,
            revision: 0,
        })
        .unwrap();
    assert_eq!(reopened.selected, Some((0, EditPoint::Anchor(2))));
    let store = crate::recovery_store::Store::new(root.join("recovery")).unwrap();
    store
        .checkpoint(&original, m.epoch, m.editor.revision(), None)
        .unwrap();
    drop(store);
    let scanner = crate::recovery_store::Store::new(root.join("recovery")).unwrap();
    let (list, _) = scanner.candidates().unwrap();
    assert_eq!(list.len(), 1);
    let (doc, lease) = scanner.candidate(&list[0].id).unwrap();
    assert_eq!(doc, original);
    assert!(identity::resolve(&doc, r).is_some());
    let mut recovered = Model::new(Workspace::default());
    recovered.replace_document(doc, None).unwrap();
    recovered
        .action(Action::SelectStableNode {
            reference: r,
            revision: 0,
        })
        .unwrap();
    assert_eq!(recovered.selected, Some((0, EditPoint::Anchor(2))));
    drop(lease);
    drop(scanner);

    let legacy_path = root.join("legacy.json");
    let mut legacy = serde_json::to_value(&original).unwrap();
    legacy["schema_version"] = 1.into();
    legacy.as_object_mut().unwrap().remove("document_uuid");
    for glyph in legacy["glyphs"].as_object_mut().unwrap().values_mut() {
        for c in glyph["contours"].as_array_mut().unwrap() {
            c.as_object_mut().unwrap().remove("identity");
        }
    }
    let bytes = serde_json::to_vec(&legacy).unwrap();
    std::fs::write(&legacy_path, &bytes).unwrap();
    let (doc, migrated) = font_core::storage::load_with_migration(&legacy_path).unwrap();
    assert!(migrated);
    let mut migrated_model = Model::new(Workspace::default());
    migrated_model
        .open_loaded_document(doc, Some(legacy_path.clone()), migrated)
        .unwrap();
    assert!(migrated_model.unsaved());
    assert_eq!(std::fs::read(&legacy_path).unwrap(), bytes);
    font_core::storage::save(&legacy_path, migrated_model.editor.document()).unwrap();
    let epoch = migrated_model.epoch;
    let revision = migrated_model.editor.revision();
    migrated_model.mark_saved(epoch, revision, legacy_path.clone());
    assert!(!migrated_model.unsaved());
    let (reloaded, migrated) = font_core::storage::load_with_migration(&legacy_path).unwrap();
    assert!(!migrated);
    assert_eq!(
        reloaded.document_uuid,
        migrated_model.editor.document().document_uuid
    );

    std::fs::remove_dir_all(root).unwrap();
}
