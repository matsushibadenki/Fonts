use font_core::{path_paste::decode, Editor, FontDocument, Segment};
fn svg(s: &str) -> Vec<font_core::Contour> {
    decode("svg", s.as_bytes()).unwrap()
}
#[test]
fn relative_curves_arcs_and_transforms() {
    let c = svg(
        r#"<svg><g transform="translate(10 20) scale(2)"><path fill="none" d="M 1 2 h 3 v 4 q 1 2 3 4 t 3 4 c 1 2 3 4 5 6 s 3 4 5 6 a 10 10 0 0 1 10 10"/></g></svg>"#,
    );
    assert_eq!(c[0].start, [12., -24.]);
    assert!(!c[0].closed);
    assert!(matches!(c[0].segments[2], Segment::Quadratic { .. }));
    assert!(matches!(c[0].segments.last(), Some(Segment::Cubic { .. })));
}
#[test]
fn holes_and_overlapping_evenodd() {
    let c = svg(r#"<svg><path fill-rule="evenodd" d="M0 0H100V100H0Z M20 20H80V80H20Z"/></svg>"#);
    assert_ne!(c[0].start, c[1].start);
    let area = |c: &font_core::Contour| {
        let mut p = c.start;
        let mut a = 0.;
        for s in &c.segments {
            let end = match s {
                Segment::Line { end } => *end,
                _ => panic!(),
            };
            a += p[0] * end[1] - end[0] * p[1];
            p = end;
        }
        a + p[0] * c.start[1] - c.start[0] * p[1]
    };
    assert!(area(&c[0]) * area(&c[1]) < 0.);
    assert_eq!(
        decode(
            "svg",
            br#"<svg><path fill-rule="evenodd" d="M0 0H100V100H0Z M50 50H150V150H50Z"/></svg>"#
        )
        .unwrap_err(),
        "clipboard_outline_required"
    );
}
#[test]
fn atomic_undo_redo_and_storage() {
    let c = svg(r#"<svg><path d="M0 0 L100 0 Q150 50 100 100 C50 100 0 50 0 0 Z"/></svg>"#);
    let mut e = Editor::new(FontDocument::new("Clipboard QA", "A").unwrap()).unwrap();
    let before = e.document().clone();
    assert_eq!(e.paste_paths(0, 1, c).unwrap(), 0..1);
    let pasted = e.document().clone();
    let bounds = font_core::snippets::bounds(&pasted.glyphs[&1].contours).unwrap();
    assert!(bounds.0[0] >= 0. && bounds.1[0] <= pasted.glyphs[&1].advance);
    e.undo(e.revision()).unwrap();
    assert_eq!(e.document(), &before);
    e.redo(e.revision()).unwrap();
    assert_eq!(e.document(), &pasted);
    let data = serde_json::to_vec(e.document()).unwrap();
    let recovered: FontDocument = serde_json::from_slice(&data).unwrap();
    assert_eq!(recovered, pasted);
    assert!(e
        .paste_paths(0, 1, recovered.glyphs[&1].contours.clone())
        .is_err());
}
#[test]
fn lumapaint_protocol_preserves_affine_and_beziers() {
    let payload = serde_json::json!({"kind":"Vectors","objects":[{"id":"qa","name":"Curve","path":{"data":"M1 2 C3 4 5 6 7 8 Z","fillRule":"nonZero"},"transform":[2,0,0,3,10,20],"fill":{"color":[0,0,0,255],"registration":false},"stroke":null,"strokeWidth":0,"visible":true}]});
    let c = decode("lumapaint", &serde_json::to_vec(&payload).unwrap()).unwrap();
    assert_eq!(c[0].start, [12., -26.]);
    assert!(matches!(
        c[0].segments[0],
        Segment::Cubic {
            control1: [16., -32.],
            control2: [20., -38.],
            end: [24., -44.]
        }
    ));
}
fn pdf(content: &str, form: bool) -> Vec<u8> {
    use lopdf::{dictionary, Document, Object, Stream};
    let mut d = Document::with_version("1.7");
    let pages = d.new_object_id();
    let mut resources = dictionary! {};
    let content = if form {
        let form=d.add_object(Stream::new(dictionary!{"Type"=>"XObject","Subtype"=>"Form","BBox"=>vec![0.into(),0.into(),100.into(),100.into()],"Matrix"=>vec![2.into(),0.into(),0.into(),2.into(),10.into(),20.into()]},content.as_bytes().to_vec()));
        resources.set("XObject", dictionary! {"Shape"=>form});
        "q /Shape Do Q"
    } else {
        content
    };
    let stream = d.add_object(Stream::new(dictionary! {}, content.as_bytes().to_vec()));
    let page=d.add_object(dictionary!{"Type"=>"Page","Parent"=>pages,"MediaBox"=>vec![0.into(),0.into(),300.into(),300.into()],"Resources"=>resources,"Contents"=>stream});
    d.objects.insert(
        pages,
        Object::Dictionary(dictionary! {"Type"=>"Pages","Kids"=>vec![page.into()],"Count"=>1}),
    );
    let catalog = d.add_object(dictionary! {"Type"=>"Catalog","Pages"=>pages});
    d.trailer.set("Root", catalog);
    let mut bytes = vec![];
    d.save_to(&mut bytes).unwrap();
    bytes
}
#[test]
fn pdf_paths_and_form_matrix() {
    let c = decode("pdf", &pdf("0 0 m 10 20 30 40 50 60 c S", true)).unwrap();
    assert_eq!(c[0].start, [10., 20.]);
    assert!(!c[0].closed);
    assert!(matches!(
        c[0].segments[0],
        Segment::Cubic {
            end: [110., 140.],
            ..
        }
    ));
    let c = decode("pdf", &pdf("0 0 100 100 re 20 20 50 50 re f*", false)).unwrap();
    assert_eq!(c.len(), 2);
    assert!(decode("pdf", &pdf("BT (text) Tj ET", false)).is_err());
}
#[test]
fn aicb_never_executes_postscript() {
    let c = decode(
        "aicb",
        b"%!PS-Adobe-3.0\n%%EndSetup\n0 0 m\n10 20 30 40 50 60 c\nf\nshowpage",
    )
    .unwrap();
    assert_eq!(c.len(), 1);
    assert!(decode("aicb", b"%!PS\n%%EndSetup\n(evil) run").is_err());
}
#[test]
fn malformed_external_and_limits() {
    assert!(decode("svg", b"<svg><image href='file:///private/file'/></svg>").is_err());
    assert!(decode("svg", b"<svg><path d='M0 0 LNaN 0'/></svg>").is_err());
    assert!(decode(
        "svg",
        b"<svg><path transform='scale(99999999)' d='M0 0L1 2'/></svg>"
    )
    .is_err());
    assert!(decode(
        "svg",
        b"<!DOCTYPE svg [<!ENTITY x SYSTEM 'file:///tmp/x'>]><svg>&x;</svg>"
    )
    .is_err());
    assert!(decode("svg", &vec![0; 8 * 1024 * 1024 + 1]).is_err());
}

#[test]
fn illustrator_css_fill_rule_and_inherited_open_stroke() {
    let c = svg(
        r#"<svg><style>.st0{fill-rule:evenodd} .st1{fill:none}</style><path class="st0" d="M0 0H100V100H0Z M20 20H80V80H20Z"/><g class="st1"><path d="M0 0Q50 100 100 0"/></g></svg>"#,
    );
    assert_eq!(c.len(), 3);
    assert!(!c[2].closed);
    let p = std::env::temp_dir().join(format!("fonts-path-paste-{}.json", std::process::id()));
    let mut e = Editor::new(FontDocument::new("Saved paths", "A").unwrap()).unwrap();
    e.paste_paths(0, 1, c).unwrap();
    font_core::storage::save(&p, e.document()).unwrap();
    assert_eq!(&font_core::storage::load(&p).unwrap(), e.document());
    std::fs::remove_file(p).unwrap();
}
