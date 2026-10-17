use super::*;
use svgtypes::{SimplePathSegment as S, SimplifyingPathParser};
pub(super) fn path(data: &str, m: Matrix) -> Result<Vec<Contour>> {
    let mut p = Path::default();
    for s in SimplifyingPathParser::from(data) {
        match s.map_err(|_| "clipboard_invalid")? {
            S::MoveTo { x, y } => p.move_to([x, y])?,
            S::LineTo { x, y } => p.segment(Segment::Line { end: [x, y] })?,
            S::Quadratic { x1, y1, x, y } => p.segment(Segment::Quadratic {
                control: [x1, y1],
                end: [x, y],
            })?,
            S::CurveTo {
                x1,
                y1,
                x2,
                y2,
                x,
                y,
            } => p.segment(Segment::Cubic {
                control1: [x1, y1],
                control2: [x2, y2],
                end: [x, y],
            })?,
            S::ClosePath => p.close()?,
        }
    }
    p.take()?.iter().map(|c| transform(c, m)).collect()
}
const FLIP: Matrix = [1., 0., 0., -1., 0., 0.];
// Interchange reads geometry independently of the source app's evolving paint schema.
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ClipboardObject {
    path: lumapaint_core::vector::VectorPath,
    transform: [f64; 6],
    visible: bool,
    #[serde(default = "one")]
    opacity: f64,
    text: Option<serde_json::Value>,
    clipping_group: Option<String>,
    image_frame: Option<serde_json::Value>,
    live_corners: Option<serde_json::Value>,
    rectangle_radii: Option<[f64; 4]>,
    fill: Option<serde_json::Value>,
    fill_gradient: Option<serde_json::Value>,
}
fn one() -> f64 {
    1.
}
pub fn lumapaint(data: &str) -> Result<Vec<Contour>> {
    #[derive(serde::Deserialize)]
    #[serde(tag = "kind", deny_unknown_fields)]
    enum Content {
        Vectors { objects: Vec<ClipboardObject> },
    }
    let Content::Vectors { objects } =
        serde_json::from_str(data).map_err(|_| "clipboard_unsupported")?;
    if objects.len() > 4096 {
        return Err("clipboard_too_large");
    }
    let mut result = vec![];
    for o in objects {
        if !o.opacity.is_finite() || !(0.0..=1.0).contains(&o.opacity) {
            return Err("clipboard_invalid");
        }
        if !o.visible || o.opacity == 0. {
            continue;
        }
        if o.text.is_some()
            || o.clipping_group.is_some()
            || o.image_frame.is_some()
            || o.live_corners.is_some()
            || o.rectangle_radii
                .is_some_and(|r| r.iter().any(|v| *v != 0.))
        {
            return Err("clipboard_outline_required");
        }
        let mut c = path(&o.path.data, multiply(FLIP, o.transform))?;
        if o.fill.is_some() || o.fill_gradient.is_some() {
            for c in &mut c {
                c.closed = true;
            }
        }
        if matches!(o.path.fill_rule, lumapaint_core::vector::FillRule::EvenOdd) {
            even_odd(&mut c)?;
        }
        result.extend(c);
        validate(&result)?;
    }
    Ok(result)
}
pub fn document(data: &str) -> Result<Vec<Contour>> {
    let doc = roxmltree::Document::parse_with_options(
        data,
        roxmltree::ParsingOptions {
            nodes_limit: 100_000,
            ..Default::default()
        },
    )
    .map_err(|_| "clipboard_invalid")?;
    if doc.descendants().filter(|n| n.is_element()).count() > 8192
        || doc
            .descendants()
            .filter(|n| n.has_tag_name("style"))
            .count()
            > 8
    {
        return Err("clipboard_too_large");
    }
    let root = doc.root_element();
    if root.tag_name().name() != "svg" {
        return Err("clipboard_unsupported");
    }
    let mut styles = vec![];
    for style in doc.descendants().filter(|n| n.has_tag_name("style")) {
        let text = style.text().unwrap_or("");
        if text.len() > 65536 {
            return Err("clipboard_too_large");
        }
        for block in text.split('}') {
            if let Some((selectors, body)) = block.split_once('{') {
                for selector in selectors.split(',') {
                    let selector = selector.trim();
                    let name = selector
                        .strip_prefix('.')
                        .or_else(|| selector.strip_prefix('#'))
                        .unwrap_or(selector);
                    if name != "*"
                        && (name.is_empty()
                            || !name
                                .chars()
                                .all(|c| c.is_alphanumeric() || c == '_' || c == '-'))
                    {
                        return Err("clipboard_unsupported");
                    }
                    if !selector
                        .chars()
                        .all(|c| c.is_alphanumeric() || matches!(c, '.' | '#' | '_' | '-' | '*'))
                    {
                        return Err("clipboard_unsupported");
                    }
                    styles.push((selector.to_owned(), body.to_owned()));
                    if styles.len() > 256 {
                        return Err("clipboard_too_large");
                    }
                }
            }
        }
    }
    let mut result = vec![];
    walk(root, FLIP, false, &styles, &mut result, 0)?;
    Ok(result)
}
fn walk(
    n: roxmltree::Node<'_, '_>,
    parent: Matrix,
    inherited_even: bool,
    styles: &[(String, String)],
    out: &mut Vec<Contour>,
    depth: usize,
) -> Result<()> {
    if depth > 32 {
        return Err("clipboard_too_large");
    }
    let tag = n.tag_name().name();
    if matches!(tag, "defs" | "metadata" | "title" | "desc" | "style") {
        return Ok(());
    }
    let prop = |key: &str| property(n, key, styles);
    if prop("display").as_deref() == Some("none")
        || prop("visibility").as_deref() == Some("hidden")
        || prop("opacity").as_deref() == Some("0")
    {
        return Ok(());
    }
    if ["clip-path", "mask", "filter"]
        .iter()
        .any(|k| prop(k).as_deref().is_some_and(|v| v != "none"))
    {
        return Err("clipboard_outline_required");
    }
    if n.attribute("style")
        .is_some_and(|v| v.contains("transform"))
        || styles.iter().any(|(_, v)| v.contains("transform"))
        || n.attribute("preserveAspectRatio")
            .is_some_and(|v| v.trim() == "none")
        || (tag == "svg" && depth > 0)
    {
        return Err("clipboard_unsupported");
    }
    let local = if let Some(t) = n.attribute("transform") {
        let t = t
            .parse::<svgtypes::Transform>()
            .map_err(|_| "clipboard_invalid")?;
        [t.a, t.b, t.c, t.d, t.e, t.f]
    } else {
        IDENTITY
    };
    let m = multiply(parent, local);
    let even = prop("fill-rule")
        .map(|v| v == "evenodd")
        .unwrap_or(inherited_even);
    let num = |key: &str, default: f64| -> Result<f64> {
        n.attribute(key)
            .map(|v| v.parse::<f64>().map_err(|_| "clipboard_invalid"))
            .unwrap_or(Ok(default))
    };
    let d = match tag {
        "svg" | "g" | "a" => {
            for child in n.children().filter(|c| c.is_element()) {
                walk(child, m, even, styles, out, depth + 1)?;
            }
            return Ok(());
        }
        "path" => n.attribute("d").unwrap_or("").to_owned(),
        "rect" => {
            let x = num("x", 0.)?;
            let y = num("y", 0.)?;
            let w = num("width", 0.)?;
            let h = num("height", 0.)?;
            let r = num("rx", num("ry", 0.)?)?.min(w / 2.);
            let s = num("ry", r)?.min(h / 2.);
            if w < 0. || h < 0. || r < 0. || s < 0. {
                return Err("clipboard_invalid");
            }
            if r > 0. && s > 0. {
                format!("M {} {y} H {} A {r} {s} 0 0 1 {} {} V {} A {r} {s} 0 0 1 {} {} H {} A {r} {s} 0 0 1 {x} {} V {} A {r} {s} 0 0 1 {} {y} Z",x+r,x+w-r,x+w,y+s,y+h-s,x+w-r,y+h,x+r,y+h-s,y+s,x+r)
            } else {
                format!("M {x} {y} h {w} v {h} h {} Z", -w)
            }
        }
        "circle" | "ellipse" => {
            let x = num("cx", 0.)?;
            let y = num("cy", 0.)?;
            let r = num("rx", num("r", 0.)?)?;
            let s = num("ry", r)?;
            if r < 0. || s < 0. {
                return Err("clipboard_invalid");
            }
            format!(
                "M {} {y} A {r} {s} 0 1 0 {} {y} A {r} {s} 0 1 0 {} {y} Z",
                x + r,
                x - r,
                x + r
            )
        }
        "line" => format!(
            "M {} {} L {} {}",
            num("x1", 0.)?,
            num("y1", 0.)?,
            num("x2", 0.)?,
            num("y2", 0.)?
        ),
        "polyline" | "polygon" => {
            let values = svgtypes::NumberListParser::from(n.attribute("points").unwrap_or(""))
                .collect::<std::result::Result<Vec<_>, _>>()
                .map_err(|_| "clipboard_invalid")?;
            if values.len() < 4 || values.len() % 2 != 0 {
                return Err("clipboard_invalid");
            }
            let mut d = format!("M {} {}", values[0], values[1]);
            for p in values[2..].as_chunks::<2>().0 {
                d += &format!(" L {} {}", p[0], p[1]);
            }
            if tag == "polygon" {
                d += " Z";
            }
            d
        }
        _ => return Err("clipboard_outline_required"),
    };
    let mut contours = path(&d, m)?;
    if prop("fill").as_deref() != Some("none") && tag != "line" && tag != "polyline" {
        for c in &mut contours {
            c.closed = true;
        }
    }
    if even {
        even_odd(&mut contours)?;
    }
    out.extend(contours);
    if !out.is_empty() {
        validate(out)?;
    }
    Ok(())
}

// Illustrator exports simple class rules. Resolve geometry-related presentation properties,
// including inherited fill-rule, without executing CSS or loading external stylesheets.
fn property(n: roxmltree::Node<'_, '_>, key: &str, styles: &[(String, String)]) -> Option<String> {
    for node in n.ancestors().filter(|v| v.is_element()) {
        let declaration = |s: &str| -> Option<String> {
            s.split(';')
                .filter_map(|v| v.split_once(':'))
                .filter(|(k, _)| k.trim() == key)
                .map(|(_, v)| v.trim().to_owned())
                .next_back()
        };
        if let Some(v) = node.attribute("style").and_then(declaration) {
            return Some(v);
        }
        let mut rule = None;
        for (selector, body) in styles {
            let matches = if let Some(c) = selector.strip_prefix('.') {
                node.attribute("class")
                    .is_some_and(|v| v.split_whitespace().any(|v| v == c))
            } else if let Some(id) = selector.strip_prefix('#') {
                node.attribute("id") == Some(id)
            } else {
                selector == node.tag_name().name() || selector == "*"
            };
            if matches {
                if let Some(v) = declaration(body) {
                    rule = Some(v);
                }
            }
        }
        if rule.is_some() {
            return rule;
        }
        if let Some(v) = node.attribute(key) {
            return Some(v.to_owned());
        }
        if !matches!(key, "fill" | "fill-rule" | "visibility") {
            break;
        }
    }
    None
}
