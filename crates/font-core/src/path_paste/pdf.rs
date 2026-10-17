use super::*;
use lopdf::{content::Content, Dictionary, Document, Object};
const LIMIT: usize = 16 * 1024 * 1024;
pub fn decode(bytes: &[u8]) -> Result<Vec<Contour>> {
    let doc = Document::load_mem_with_options(
        bytes,
        lopdf::LoadOptions {
            max_decompressed_size: Some(LIMIT),
            ..Default::default()
        },
    )
    .map_err(|_| "clipboard_invalid")?;
    if doc.is_encrypted() || doc.objects.len() > 100_000 {
        return Err("clipboard_unsupported");
    }
    let pages = doc.get_pages();
    if pages.len() != 1 {
        return Err("clipboard_unsupported");
    }
    let id = *pages.values().next().ok_or("clipboard_empty")?;
    let data = doc
        .get_page_content_with_limit(id, LIMIT)
        .map_err(|_| "clipboard_too_large")?;
    let (own, parents) = doc
        .get_page_resources(id)
        .map_err(|_| "clipboard_invalid")?;
    let mut resources = Dictionary::new();
    for id in parents.into_iter().rev() {
        let d = doc.get_dictionary(id).map_err(|_| "clipboard_invalid")?;
        for (k, v) in d {
            resources.set(k.clone(), v.clone());
        }
    }
    if let Some(d) = own {
        for (k, v) in d {
            resources.set(k.clone(), v.clone());
        }
    }
    let mut budget = 0;
    let mut out = vec![];
    run(&doc, &data, &resources, IDENTITY, 0, &mut budget, &mut out)?;
    Ok(out)
}
fn numeric(o: &Object) -> Result<f64> {
    match o {
        Object::Integer(n) => Ok(*n as f64),
        Object::Real(n) => Ok(f64::from(*n)),
        _ => Err("clipboard_invalid"),
    }
}
fn run(
    doc: &Document,
    data: &[u8],
    resources: &Dictionary,
    initial: Matrix,
    depth: usize,
    budget: &mut usize,
    out: &mut Vec<Contour>,
) -> Result<()> {
    if depth > 16 {
        return Err("clipboard_too_large");
    }
    let content = Content::decode(data).map_err(|_| "clipboard_invalid")?;
    let mut p = Path::default();
    let mut m = initial;
    let mut stack = vec![];
    for op in content.operations {
        *budget += 1;
        if *budget > 100_000 {
            return Err("clipboard_too_large");
        }
        let n = |i: usize| {
            op.operands
                .get(i)
                .ok_or("clipboard_invalid")
                .and_then(numeric)
        };
        let point = |i| -> Result<Point> { Ok(map(m, [n(i)?, n(i + 1)?])) };
        match op.operator.as_str() {
            "q" => {
                if stack.len() > 64 {
                    return Err("clipboard_too_large");
                }
                stack.push(m);
            }
            "Q" => m = stack.pop().ok_or("clipboard_invalid")?,
            "cm" => m = multiply(m, [n(0)?, n(1)?, n(2)?, n(3)?, n(4)?, n(5)?]),
            "m" => p.move_to(point(0)?)?,
            "l" => p.segment(Segment::Line { end: point(0)? })?,
            "c" => p.segment(Segment::Cubic {
                control1: point(0)?,
                control2: point(2)?,
                end: point(4)?,
            })?,
            "v" => p.segment(Segment::Cubic {
                control1: p.at()?,
                control2: point(0)?,
                end: point(2)?,
            })?,
            "y" => p.segment(Segment::Cubic {
                control1: point(0)?,
                control2: point(2)?,
                end: point(2)?,
            })?,
            "re" => {
                let x = n(0)?;
                let y = n(1)?;
                let w = n(2)?;
                let h = n(3)?;
                p.move_to(map(m, [x, y]))?;
                for v in [[x + w, y], [x + w, y + h], [x, y + h]] {
                    p.segment(Segment::Line { end: map(m, v) })?;
                }
                p.close()?;
            }
            "h" => p.close()?,
            "f" | "F" | "f*" | "B" | "B*" | "b" | "b*" | "S" | "s" => {
                let mut c = p.take()?;
                if op.operator != "S" {
                    for c in &mut c {
                        c.closed = true;
                    }
                }
                if op.operator.ends_with('*') {
                    even_odd(&mut c)?;
                }
                out.extend(c);
                if !out.is_empty() {
                    validate(out)?;
                }
            }
            "n" => {
                p.take()?;
            }
            "Do" => {
                let name = op
                    .operands
                    .first()
                    .and_then(|o| o.as_name().ok())
                    .ok_or("clipboard_invalid")?;
                let (_, objects) = doc
                    .dereference(resources.get(b"XObject").map_err(|_| "clipboard_invalid")?)
                    .map_err(|_| "clipboard_invalid")?;
                let (_, object) = doc
                    .dereference(
                        objects
                            .as_dict()
                            .map_err(|_| "clipboard_invalid")?
                            .get(name)
                            .map_err(|_| "clipboard_invalid")?,
                    )
                    .map_err(|_| "clipboard_invalid")?;
                let stream = object.as_stream().map_err(|_| "clipboard_invalid")?;
                if stream.dict.get(b"Subtype").and_then(Object::as_name).ok()
                    != Some(b"Form".as_slice())
                {
                    return Err("clipboard_outline_required");
                }
                let mut matrix = IDENTITY;
                if let Ok(v) = stream.dict.get(b"Matrix") {
                    let a = v.as_array().map_err(|_| "clipboard_invalid")?;
                    if a.len() != 6 {
                        return Err("clipboard_invalid");
                    }
                    for (i, o) in a.iter().enumerate() {
                        matrix[i] = numeric(o)?;
                    }
                }
                let local = stream
                    .dict
                    .get(b"Resources")
                    .ok()
                    .map(|o| {
                        doc.dereference(o)
                            .map_err(|_| "clipboard_invalid")
                            .and_then(|(_, o)| o.as_dict().map_err(|_| "clipboard_invalid"))
                    })
                    .transpose()?
                    .unwrap_or(resources);
                let bytes = if stream.dict.has(b"Filter") {
                    stream
                        .decompressed_content_with_limit(LIMIT)
                        .map_err(|_| "clipboard_too_large")?
                } else {
                    stream.content.clone()
                };
                run(
                    doc,
                    &bytes,
                    local,
                    multiply(m, matrix),
                    depth + 1,
                    budget,
                    out,
                )?;
            }
            "W" | "W*" | "Tj" | "TJ" | "'" | "\"" | "sh" | "BI" => {
                return Err("clipboard_outline_required")
            }
            "w" | "J" | "j" | "M" | "d" | "ri" | "i" | "gs" | "CS" | "cs" | "SC" | "SCN" | "sc"
            | "scn" | "G" | "g" | "RG" | "rg" | "K" | "k" | "BMC" | "BDC" | "EMC" | "MP" | "DP"
            | "BX" | "EX" | "BT" | "ET" | "Tf" | "Tm" | "Td" | "TD" | "T*" | "Tc" | "Tw" | "Tz"
            | "TL" | "Tr" | "Ts" => {}
            _ => return Err("clipboard_unsupported"),
        }
    }
    if !p.take()?.is_empty() {
        return Err("clipboard_invalid");
    }
    Ok(())
}
