//! Bounded, offline SVG-in-OpenType rendering. Never hand font XML to a WebView.
use base64::Engine;
use resvg::{tiny_skia, usvg};
use roxmltree::{Document, Node};
use std::{collections::BTreeMap, io::Read};
use ttf_parser::{svg::SvgDocument, Face, GlyphId};

const SVG_NS: &str = "http://www.w3.org/2000/svg";
const XLINK: &str = "http://www.w3.org/1999/xlink";
const BYTE_LIMIT: usize = 2 * 1024 * 1024;
const NODE_LIMIT: usize = 8192;
const WORK_LIMIT: usize = 32768;
const DEPTH_LIMIT: usize = 48;

pub(crate) fn validate_table(face: &Face<'_>) -> Result<(), &'static str> {
    let Some(data) = face.raw_face().table(ttf_parser::Tag::from_bytes(b"SVG ")) else {
        return Ok(());
    };
    validate_records(data, face.number_of_glyphs())
}

fn validate_records(data: &[u8], glyph_count: u16) -> Result<(), &'static str> {
    let word = |at: usize| {
        data.get(at..at.checked_add(2).ok_or("color_svg_invalid")?)
            .map(|b| u16::from_be_bytes([b[0], b[1]]))
            .ok_or("color_svg_invalid")
    };
    let long = |at: usize| {
        data.get(at..at.checked_add(4).ok_or("color_svg_invalid")?)
            .map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]) as usize)
            .ok_or("color_svg_invalid")
    };
    if word(0)? != 0 || long(6)? != 0 {
        return Err("color_svg_invalid");
    }
    let list = long(2)?;
    if list < 10 {
        return Err("color_svg_invalid");
    }
    let count = usize::from(word(list)?);
    let directory_size = 2 + count * 12;
    let records = data
        .get(list..)
        .and_then(|b| b.get(2..directory_size))
        .ok_or("color_svg_invalid")?;
    if count == 0 {
        return Err("color_svg_invalid");
    }
    let mut previous = None;
    for record in records.chunks(12) {
        let start = u16::from_be_bytes([record[0], record[1]]);
        let end = u16::from_be_bytes([record[2], record[3]]);
        let offset = u32::from_be_bytes(record[4..8].try_into().unwrap()) as usize;
        let length = u32::from_be_bytes(record[8..12].try_into().unwrap()) as usize;
        if start > end
            || end >= glyph_count
            || previous.is_some_and(|p| start <= p)
            || offset < directory_size
            || length == 0
        {
            return Err("color_svg_invalid");
        }
        let begin = list.checked_add(offset).ok_or("color_svg_invalid")?;
        let finish = begin.checked_add(length).ok_or("color_svg_invalid")?;
        data.get(begin..finish).ok_or("color_svg_invalid")?;
        previous = Some(end);
    }
    Ok(())
}

fn decode(data: &[u8]) -> Result<String, &'static str> {
    if data.len() > BYTE_LIMIT {
        return Err("color_svg_limit");
    }
    let bytes = if data.starts_with(&[0x1f, 0x8b]) {
        let mut bytes = Vec::new();
        flate2::read::MultiGzDecoder::new(data)
            .take((BYTE_LIMIT + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|_| "color_svg_invalid")?;
        bytes
    } else {
        data.to_vec()
    };
    if bytes.len() > BYTE_LIMIT {
        return Err("color_svg_limit");
    }
    String::from_utf8(bytes).map_err(|_| "color_svg_invalid")
}

fn property(name: &str) -> bool {
    matches!(
        name,
        "fill"
            | "fill-opacity"
            | "fill-rule"
            | "stroke"
            | "stroke-opacity"
            | "stroke-width"
            | "stroke-linecap"
            | "stroke-linejoin"
            | "stroke-miterlimit"
            | "stroke-dasharray"
            | "stroke-dashoffset"
            | "opacity"
            | "color"
            | "stop-color"
            | "stop-opacity"
            | "clip-path"
            | "clip-rule"
            | "display"
            | "visibility"
            | "shape-rendering"
            | "color-interpolation"
    )
}
fn attribute(name: &str) -> bool {
    property(name)
        || matches!(
            name,
            "id" | "version"
                | "d"
                | "points"
                | "transform"
                | "x"
                | "y"
                | "x1"
                | "y1"
                | "x2"
                | "y2"
                | "cx"
                | "cy"
                | "r"
                | "rx"
                | "ry"
                | "fx"
                | "fy"
                | "width"
                | "height"
                | "viewBox"
                | "preserveAspectRatio"
                | "gradientUnits"
                | "gradientTransform"
                | "spreadMethod"
                | "offset"
                | "clipPathUnits"
                | "href"
        )
}
fn identifier(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_-.:".contains(&b))
}
fn references(value: &str) -> Result<Vec<String>, &'static str> {
    // CSS escapes/comments/at-rules are outside the supported static subset.
    if value.contains(['\\', '@', '!']) || value.contains("/*") {
        return Err("color_svg_unsupported");
    }
    let lower = value.to_ascii_lowercase();
    let mut rest = lower.as_str();
    let mut refs = Vec::new();
    let mut consumed = 0;
    while let Some(at) = rest.find("url(") {
        let start = consumed + at + 4;
        let end = value[start..].find(')').ok_or("color_svg_invalid")? + start;
        let target = value[start..end].trim().trim_matches(['\'', '"']);
        let id = target
            .strip_prefix('#')
            .filter(|id| identifier(id))
            .ok_or("color_svg_unsupported")?;
        refs.push(id.to_string());
        consumed = end + 1;
        rest = &lower[consumed..];
    }
    Ok(refs)
}
fn palette_value(value: &str, face: &Face<'_>, palette: u16) -> Result<String, &'static str> {
    if !value.contains("var(") {
        return Ok(value.to_string());
    }
    let inner = value
        .trim()
        .strip_prefix("var(")
        .and_then(|v| v.strip_suffix(')'))
        .ok_or("color_svg_unsupported")?;
    let (key, fallback) = inner
        .split_once(',')
        .map_or((inner, None), |(a, b)| (a, Some(b.trim())));
    let entry: u16 = key
        .trim()
        .strip_prefix("--color")
        .ok_or("color_svg_unsupported")?
        .parse()
        .map_err(|_| "color_svg_unsupported")?;
    // CPAL's parser stores all colors; separately enforce this palette's entry count.
    let count = face
        .raw_face()
        .table(ttf_parser::Tag::from_bytes(b"CPAL"))
        .and_then(|b| b.get(2..4))
        .map(|b| u16::from_be_bytes([b[0], b[1]]))
        .unwrap_or(0);
    if entry < count {
        if let Some(c) = face
            .raw_face()
            .table(ttf_parser::Tag::from_bytes(b"CPAL"))
            .and_then(ttf_parser::cpal::Table::parse)
            .and_then(|t| t.get(palette, entry))
        {
            return Ok(format!(
                "rgba({},{},{},{})",
                c.red,
                c.green,
                c.blue,
                f32::from(c.alpha) / 255.
            ));
        }
    }
    let fallback = fallback.ok_or("color_svg_unsupported")?;
    if fallback != "currentColor" && fallback.parse::<svgtypes::Color>().is_err() {
        return Err("color_svg_unsupported");
    }
    Ok(fallback.into())
}
fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn description(node: Node<'_, '_>) -> bool {
    node.tag_name().namespace() == Some(SVG_NS)
        && matches!(node.tag_name().name(), "metadata" | "title" | "desc")
}

fn cost(
    index: usize,
    edges: &[Vec<usize>],
    weights: &[usize],
    state: &mut [u8],
    costs: &mut [(usize, usize)],
    depth: usize,
) -> Result<(usize, usize), &'static str> {
    if depth > DEPTH_LIMIT {
        return Err("color_svg_limit");
    }
    if state[index] == 1 {
        return Err("color_svg_invalid");
    }
    if state[index] == 2 {
        return Ok(costs[index]);
    }
    state[index] = 1;
    let mut total = weights[index];
    if total > WORK_LIMIT {
        return Err("color_svg_limit");
    }
    let mut height = 1;
    for &next in &edges[index] {
        let (weight, h) = cost(next, edges, weights, state, costs, depth + 1)?;
        total += weight;
        height = height.max(h + 1);
        if total > WORK_LIMIT || height > DEPTH_LIMIT {
            return Err("color_svg_limit");
        }
    }
    state[index] = 2;
    costs[index] = (total, height);
    Ok((total, height))
}

// Decode once under explicit allocation limits, then pass only normalized PNG to resvg.
fn embedded_image(value: &str, pixels: &mut u64) -> Result<String, &'static str> {
    let (header, payload) = value.split_once(',').ok_or("color_svg_unsupported")?;
    let format = match header {
        "data:image/png;base64" => image::ImageFormat::Png,
        "data:image/jpeg;base64" | "data:image/jpg;base64" => image::ImageFormat::Jpeg,
        _ => return Err("color_svg_unsupported"),
    };
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(payload)
        .map_err(|_| "color_svg_invalid")?;
    let mut reader = image::ImageReader::with_format(std::io::Cursor::new(&bytes), format);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(4096);
    limits.max_image_height = Some(4096);
    limits.max_alloc = Some(32 * 1024 * 1024);
    reader.limits(limits);
    let (width, height) = image::ImageReader::with_format(std::io::Cursor::new(&bytes), format)
        .into_dimensions()
        .map_err(|_| "color_svg_invalid")?;
    let area = u64::from(width) * u64::from(height);
    *pixels = pixels.checked_add(area).ok_or("color_svg_limit")?;
    if width == 0 || height == 0 || width > 4096 || height > 4096 || *pixels > 4_000_000 {
        return Err("color_svg_limit");
    }
    let decoded = reader.decode().map_err(|_| "color_svg_invalid")?;
    let mut png = std::io::Cursor::new(Vec::new());
    decoded
        .write_to(&mut png, image::ImageFormat::Png)
        .map_err(|_| "color_svg_invalid")?;
    Ok(format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(png.into_inner())
    ))
}

fn prepare(
    text: &str,
    face: &Face<'_>,
    svg: SvgDocument<'_>,
    glyph: GlyphId,
    palette: u16,
) -> Result<String, &'static str> {
    let doc = Document::parse_with_options(
        text,
        roxmltree::ParsingOptions {
            allow_dtd: false,
            nodes_limit: NODE_LIMIT as u32,
            ..Default::default()
        },
    )
    .map_err(|_| "color_svg_invalid")?;
    if doc.descendants().any(|n| n.is_pi()) {
        return Err("color_svg_unsupported");
    }
    if doc
        .descendants()
        .any(|n| n.ancestors().take(DEPTH_LIMIT + 1).count() > DEPTH_LIMIT)
    {
        return Err("color_svg_limit");
    }
    let root = doc.root_element();
    if root.tag_name().name() != "svg" || root.tag_name().namespace() != Some(SVG_NS) {
        return Err("color_svg_invalid");
    }
    // Descriptive metadata (including foreign XML inside it) never reaches the renderer.
    let nodes: Vec<_> = root
        .descendants()
        .filter(|n| n.is_element() && !n.ancestors().any(description))
        .collect();
    let positions: BTreeMap<_, _> = nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (n.range().start, i))
        .collect();
    let mut ids = BTreeMap::new();
    let mut attrs = Vec::new();
    let mut links = Vec::new();
    let mut image_pixels = 0;
    for (i, node) in nodes.iter().enumerate() {
        if node.ancestors().count() > DEPTH_LIMIT {
            return Err("color_svg_limit");
        }
        if node.tag_name().namespace() != Some(SVG_NS)
            || !matches!(
                node.tag_name().name(),
                "svg"
                    | "g"
                    | "defs"
                    | "path"
                    | "rect"
                    | "circle"
                    | "ellipse"
                    | "line"
                    | "polyline"
                    | "polygon"
                    | "linearGradient"
                    | "radialGradient"
                    | "stop"
                    | "clipPath"
                    | "use"
                    | "image"
            )
            || (i != 0 && node.tag_name().name() == "svg")
        {
            return Err("color_svg_unsupported");
        }
        let mut values = BTreeMap::new();
        for a in node.attributes() {
            if a.namespace().is_some_and(|ns| ns != XLINK)
                || (a.namespace() == Some(XLINK) && a.name() != "href")
            {
                return Err("color_svg_unsupported");
            }
            // OpenType requires the root viewport to be unclipped, regardless of these attributes.
            if i == 0 && matches!(a.name(), "clip" | "overflow") {
                continue;
            }
            if a.name() == "style" {
                continue;
            }
            if !attribute(a.name()) {
                return Err("color_svg_unsupported");
            }
            values.insert(a.name().to_string(), a.value().to_string());
        }
        if let Some(style) = node.attribute("style") {
            for declaration in style.split(';').filter(|s| !s.trim().is_empty()) {
                let (key, value) = declaration.split_once(':').ok_or("color_svg_invalid")?;
                let key = key.trim();
                if i == 0 && matches!(key, "clip" | "overflow") {
                    continue;
                }
                if !property(key) {
                    return Err("color_svg_unsupported");
                }
                values.insert(key.into(), value.trim().into());
            }
        }
        if let Some(id) = values.get("id") {
            if !identifier(id) || ids.insert(id.clone(), i).is_some() {
                return Err("color_svg_invalid");
            }
        }
        if node.tag_name().name() == "image" {
            if node.attributes().filter(|a| a.name() == "href").count() > 1 {
                return Err("color_svg_invalid");
            }
            let href = values.get_mut("href").ok_or("color_svg_invalid")?;
            *href = embedded_image(href, &mut image_pixels)?;
        }
        let mut refs = Vec::new();
        for (name, value) in &mut values {
            if name == "href" && node.tag_name().name() == "image" {
                continue;
            }
            if value.len() > 256 * 1024 {
                return Err("color_svg_limit");
            }
            refs.extend(references(value)?);
            // resvg's vector paint interpolation is sRGB. Do not silently reinterpret linearRGB.
            if name == "color-interpolation" && !matches!(value.trim(), "sRGB" | "auto" | "inherit")
            {
                return Err("color_svg_unsupported");
            }
            if name == "href" {
                let id = value
                    .strip_prefix('#')
                    .filter(|v| identifier(v))
                    .ok_or("color_svg_unsupported")?;
                refs.push(id.into());
            }
            *value = palette_value(value, face, palette)?;
        }
        attrs.push(values);
        links.push(refs);
    }
    let mut edges = vec![Vec::new(); nodes.len()];
    for (i, node) in nodes.iter().enumerate() {
        for child in node
            .children()
            .filter(|n| n.is_element() && !description(*n))
        {
            edges[i].push(positions[&child.range().start]);
        }
        for id in &links[i] {
            edges[i].push(*ids.get(id).ok_or("color_svg_invalid")?);
        }
    }
    let weights: Vec<_> = attrs
        .iter()
        .map(|v| {
            1 + v
                .iter()
                .filter(|(k, v)| !(k.as_str() == "href" && v.starts_with("data:")))
                .map(|(_, v)| v.len())
                .sum::<usize>()
                / 8
        })
        .collect();
    cost(
        0,
        &edges,
        &weights,
        &mut vec![0; nodes.len()],
        &mut vec![(0, 0); nodes.len()],
        0,
    )?;
    let target = format!("glyph{}", glyph.0);
    let selection = ids.get(&target).copied();
    // Older single-glyph fonts sometimes omit glyphN. Never render all glyphs of a shared document.
    if selection.is_none() && svg.start_glyph_id != svg.end_glyph_id {
        return Err("color_svg_invalid");
    }
    attrs[0]
        .entry("width".into())
        .or_insert_with(|| face.units_per_em().to_string());
    attrs[0]
        .entry("height".into())
        .or_insert_with(|| face.units_per_em().to_string());
    let mut out = String::new();
    fn element(
        out: &mut String,
        node: Node<'_, '_>,
        attrs: &[BTreeMap<String, String>],
        positions: &BTreeMap<usize, usize>,
        select: Option<&str>,
    ) {
        let i = positions[&node.range().start];
        let name = node.tag_name().name();
        out.push('<');
        out.push_str(name);
        if i == 0 {
            out.push_str(" xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\"");
        }
        for (k, v) in &attrs[i] {
            out.push(' ');
            out.push_str(if k == "href" { "xlink:href" } else { k });
            out.push_str("=\"");
            out.push_str(&escape(v));
            out.push('"');
        }
        out.push('>');
        if select.is_some() {
            out.push_str("<defs>");
        }
        for child in node
            .children()
            .filter(|n| n.is_element() && !description(*n))
        {
            element(out, child, attrs, positions, None);
        }
        if let Some(target) = select {
            out.push_str("</defs><use xlink:href=\"#");
            out.push_str(target);
            out.push_str("\"/>");
        }
        out.push_str("</");
        out.push_str(name);
        out.push('>');
    }
    element(
        &mut out,
        root,
        &attrs,
        &positions,
        selection.filter(|i| *i != 0).map(|_| target.as_str()),
    );
    Ok(out)
}

// Bound work and temporary isolated layers after <use> expansion and transform resolution.
fn render_budget(
    group: &usvg::Group,
    ts: tiny_skia::Transform,
    depth: usize,
    work: &mut usize,
    pixels: &mut f64,
) -> Result<(), &'static str> {
    if depth > DEPTH_LIMIT {
        return Err("color_svg_limit");
    }
    for node in group.children() {
        *work += 1;
        if let usvg::Node::Image(image) = node {
            let size = image.size();
            *pixels += f64::from(size.width()) * f64::from(size.height());
            if *pixels > 16_000_000. {
                return Err("color_svg_limit");
            }
        }
        if let usvg::Node::Path(p) = node {
            *work += p.data().len();
        }
        if *work > 200_000 {
            return Err("color_svg_limit");
        }
        if let Some(bbox) = node.abs_layer_bounding_box() {
            let b = bbox.transform(ts).ok_or("color_svg_limit")?;
            if [b.x(), b.y(), b.right(), b.bottom()]
                .iter()
                .any(|v| !v.is_finite() || v.abs() > 1e7)
            {
                return Err("color_svg_limit");
            }
            if let usvg::Node::Group(g) = node {
                if g.should_isolate() {
                    *pixels += (f64::from(b.width()) + 4.) * (f64::from(b.height()) + 4.) * 3.;
                    if *pixels > 16_000_000. {
                        return Err("color_svg_limit");
                    }
                }
            }
        }
        if let usvg::Node::Group(g) = node {
            render_budget(g, ts, depth + 1, work, pixels)?;
        }
        let mut result = Ok(());
        node.subroots(|g| {
            if result.is_ok() {
                result = render_budget(g, ts, depth + 1, work, pixels);
            }
        });
        result?;
    }
    Ok(())
}

pub(crate) fn render(
    face: &Face<'_>,
    svg: SvgDocument<'_>,
    glyph: GlyphId,
    size: u32,
    palette: u16,
    dimensions: [u32; 2],
    origin: [i32; 2],
) -> Result<Vec<u8>, String> {
    let text = decode(svg.data)?;
    let prepared = prepare(&text, face, svg, glyph, palette)?;
    let options = usvg::Options {
        default_size: usvg::Size::from_wh(
            f32::from(face.units_per_em()),
            f32::from(face.units_per_em()),
        )
        .ok_or("color_svg_invalid")?,
        // Defense in depth: never use the default resolver, which can read local files.
        image_href_resolver: usvg::ImageHrefResolver {
            resolve_data: Box::new(|mime, bytes, _| {
                (mime == "image/png").then_some(usvg::ImageKind::PNG(bytes))
            }),
            resolve_string: Box::new(|_, _| None),
        },
        ..Default::default()
    };
    let tree = usvg::Tree::from_str(&prepared, &options).map_err(|_| "color_svg_invalid")?;
    let scale = size as f32 / f32::from(face.units_per_em());
    // OpenType SVG is y-down with origin on the baseline. Do not flip it like COLR.
    let ts =
        tiny_skia::Transform::from_row(scale, 0., 0., scale, origin[0] as f32, origin[1] as f32);
    render_budget(tree.root(), ts, 0, &mut 0, &mut 0.)?;
    let mut pixmap =
        tiny_skia::Pixmap::new(dimensions[0], dimensions[1]).ok_or("color_svg_limit")?;
    resvg::render(&tree, ts, &mut pixmap.as_mut());
    Ok(pixmap
        .pixels()
        .iter()
        .flat_map(|p| {
            let p = p.demultiply();
            [p.red(), p.green(), p.blue(), p.alpha()]
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn draw(data: &[u8]) -> Result<Vec<u8>, String> {
        let face = Face::parse(include_bytes!("../tests/data/color/svg.ttf"), 0).unwrap();
        render(
            &face,
            SvgDocument {
                data,
                start_glyph_id: GlyphId(1),
                end_glyph_id: GlyphId(1),
            },
            GlyphId(1),
            100,
            0,
            [256, 256],
            [16, 124],
        )
    }
    fn document(body: &str) -> String {
        format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink">{body}</svg>"#
        )
    }
    fn error(body: &str) -> String {
        draw(document(body).as_bytes()).unwrap_err()
    }
    fn pixel(bytes: &[u8], x: usize, y: usize) -> &[u8] {
        &bytes[(y * 256 + x) * 4..(y * 256 + x + 1) * 4]
    }

    fn image_uri(format: image::ImageFormat) -> String {
        let rgba = image::RgbaImage::from_pixel(2, 2, image::Rgba([240, 30, 20, 128]));
        let mut output = std::io::Cursor::new(Vec::new());
        let image = image::DynamicImage::ImageRgba8(rgba);
        if format == image::ImageFormat::Jpeg {
            image.to_rgb8().write_to(&mut output, format).unwrap();
        } else {
            image.write_to(&mut output, format).unwrap();
        }
        format!(
            "data:image/{};base64,{}",
            if format == image::ImageFormat::Png {
                "png"
            } else {
                "jpeg"
            },
            base64::engine::general_purpose::STANDARD.encode(output.into_inner())
        )
    }

    #[test]
    fn embedded_png_and_jpeg_keep_color_alpha_transform_clip_and_use() {
        for format in [image::ImageFormat::Png, image::ImageFormat::Jpeg] {
            let uri = image_uri(format);
            let svg = document(&format!(
                r##"<defs><clipPath id="clip"><rect y="-700" width="300" height="700"/></clipPath><image id="img" href="{uri}" y="-700" width="700" height="700"/></defs><g opacity=".5" clip-path="url(#clip)"><use href="#img" transform="translate(20 0)"/></g>"##
            ));
            let output = draw(svg.as_bytes()).unwrap();
            let p = pixel(&output, 30, 70);
            assert!((i16::from(p[0]) - 240).abs() < 5, "{p:?}");
            assert!((i16::from(p[1]) - 30).abs() < 5, "{p:?}");
            assert!(
                (i16::from(p[3])
                    - if format == image::ImageFormat::Png {
                        64
                    } else {
                        128
                    })
                .abs()
                    <= 1,
                "{p:?}"
            );
            assert_eq!(pixel(&output, 70, 70), [0; 4]);
            let mut gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
            std::io::Write::write_all(&mut gzip, svg.as_bytes()).unwrap();
            assert_eq!(output, draw(&gzip.finish().unwrap()).unwrap());
        }
    }

    #[test]
    fn embedded_images_reject_wrong_formats_damage_dimensions_and_expansion() {
        for uri in [
            "data:image/svg+xml;base64,AAAA",
            "data:image/gif;base64,AAAA",
            "data:image/png,abc",
            "https://example.invalid/a.png",
            "file:///tmp/a.png",
        ] {
            assert_eq!(
                error(&format!(r#"<image href="{uri}"/>"#)),
                "color_svg_unsupported"
            );
        }
        assert_eq!(
            error(r#"<image href="data:image/png;base64,AAAA"/>"#),
            "color_svg_invalid"
        );
        let oversized = image::RgbaImage::new(4097, 1);
        let mut png = std::io::Cursor::new(Vec::new());
        oversized
            .write_to(&mut png, image::ImageFormat::Png)
            .unwrap();
        let uri = format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(png.into_inner())
        );
        assert_eq!(
            error(&format!(r#"<image href="{uri}"/>"#)),
            "color_svg_limit"
        );
        let uri = image_uri(image::ImageFormat::Png);
        assert_eq!(
            error(&format!(r#"<image href="{uri}" xlink:href="{uri}"/>"#)),
            "color_svg_invalid"
        );
        let large = image::RgbaImage::new(1000, 1000);
        let mut png = std::io::Cursor::new(Vec::new());
        large.write_to(&mut png, image::ImageFormat::Png).unwrap();
        let uri = format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(png.into_inner())
        );
        assert_eq!(
            error(&format!(r#"<image href="{uri}"/>"#).repeat(5)),
            "color_svg_limit"
        );
        let mut body =
            format!(r#"<defs><image id="img" href="{uri}" width="10" height="10"/></defs>"#);
        body += &r##"<use href="#img"/>"##.repeat(17);
        assert_eq!(error(&body), "color_svg_limit");
    }

    #[test]
    fn malformed_svg_record_must_not_silently_fall_back_to_monochrome() {
        let face = Face::parse(include_bytes!("../tests/data/color/svg.ttf"), 0).unwrap();
        let original = face
            .raw_face()
            .table(ttf_parser::Tag::from_bytes(b"SVG "))
            .unwrap();
        assert!(validate_records(original, face.number_of_glyphs()).is_ok());
        let list = u32::from_be_bytes(original[2..6].try_into().unwrap()) as usize;
        for (offset, bytes) in [
            (list + 6, vec![255; 4]),
            (list + 10, vec![255; 4]),
            (list + 2, vec![0, 3]),
            (list + 4, vec![255, 255]),
            (list, vec![0, 0]),
        ] {
            let mut bad = original.to_vec();
            bad[offset..offset + bytes.len()].copy_from_slice(&bytes);
            assert_eq!(
                validate_records(&bad, face.number_of_glyphs()),
                Err("color_svg_invalid")
            );
        }
    }

    #[test]
    fn root_viewbox_is_em_sized_and_unclipped_with_use_semantics() {
        let metadata = draw(document(r#"<metadata><foreign xmlns="urn:metadata">description</foreign></metadata><title>Title</title><rect y="-700" width="700" height="700" fill="red"/>"#).as_bytes()).unwrap();
        assert_eq!(pixel(&metadata, 25, 60), [255, 0, 0, 255]);
        let bytes = draw(br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 1000 1000 1000" overflow="hidden" clip="rect(0 0 0 0)"><g id="glyph1"><rect y="300" width="700" height="700" fill="red"/></g><rect x="700" y="0" width="300" height="1000" fill="blue"/></svg>"#).unwrap();
        assert_eq!(pixel(&bytes, 25, 60), [255, 0, 0, 255]);
        assert_eq!(pixel(&bytes, 100, 100), [0; 4]);
        assert_eq!(pixel(&bytes, 25, 125), [0; 4]);
        let clipped = draw(br##"<svg xmlns="http://www.w3.org/2000/svg" clip-path="url(#clip)"><defs><clipPath id="clip"><rect y="-700" width="200" height="700"/></clipPath></defs><rect id="glyph1" y="-700" width="700" height="700" fill="red"/></svg>"##).unwrap();
        assert_eq!(pixel(&clipped, 25, 60), [255, 0, 0, 255]);
        assert_eq!(pixel(&clipped, 60, 60), [0; 4]);
        let fallback = draw(document(r#"<rect id="glyph1" y="-700" width="700" height="700" fill="var(--color9, red)"/>"#).as_bytes()).unwrap();
        assert_eq!(pixel(&fallback, 25, 60), [255, 0, 0, 255]);
    }
    #[test]
    fn rejects_active_content_and_external_resources() {
        for body in [
            r#"<script>alert(1)</script>"#,
            r#"<foreignObject/>"#,
            r#"<image href="file:///etc/passwd"/>"#,
            r#"<use href="https://example.invalid/a.svg#g"/>"#,
            r#"<use xlink:href="/tmp/font.svg#g"/>"#,
            r#"<rect fill="url(https://example.invalid/a.svg#g)"/>"#,
            r#"<rect onclick="alert(1)"/>"#,
            r#"<g xml:base="/tmp/"/>"#,
            r#"<style>@import url(https://example.invalid/style.css);</style>"#,
            r#"<text>Test</text>"#,
            r#"<animate attributeName="opacity"/>"#,
            r#"<filter/>"#,
            r#"<pattern/>"#,
            r#"<mask/>"#,
            r#"<rect style="fill:u\72l(file:///tmp/a.svg)"/>"#,
            r#"<rect style="fill:red!important"/>"#,
            r#"<linearGradient color-interpolation="linearRGB"/>"#,
        ] {
            assert_eq!(error(body), "color_svg_unsupported", "{body}");
        }
        assert!(draw(br#"<!DOCTYPE svg [<!ENTITY x SYSTEM "file:///etc/passwd">]><svg xmlns="http://www.w3.org/2000/svg">&x;</svg>"#).is_err());
        assert!(draw(
            b"<?xml-stylesheet href='file:///tmp/a.css'?><svg xmlns='http://www.w3.org/2000/svg'/>"
        )
        .is_err());
    }
    #[test]
    fn rejects_cyclic_missing_duplicate_and_exponentially_expanded_references() {
        for body in [
            r##"<g id="a"><use href="#a"/></g>"##,
            r##"<g id="a"><use href="#b"/></g><g id="b"><use href="#a"/></g>"##,
            r##"<use href="#missing"/>"##,
            r#"<g id="a"/><g id="a"/>"#,
        ] {
            assert_eq!(error(body), "color_svg_invalid");
        }
        let mut body = String::from(r#"<g id="g0"><rect width="10" height="10"/></g>"#);
        for i in 1..20 {
            body += &format!(
                r##"<g id="g{i}"><use href="#g{}"/><use href="#g{}"/></g>"##,
                i - 1,
                i - 1
            );
        }
        assert_eq!(error(&body), "color_svg_limit");
        let mut chain = String::from(r#"<g id="g0"/>"#);
        for i in 1..60 {
            chain += &format!(r##"<g id="g{i}"><use href="#g{}"/></g>"##, i - 1);
        }
        assert_eq!(
            error(&chain),
            "color_svg_limit",
            "memoized reference depth is still bounded"
        );
    }
    #[test]
    fn rejects_gzip_bombs_truncation_deep_xml_and_huge_isolated_layers() {
        use std::io::Write;
        let mut gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        gzip.write_all(&vec![b' '; BYTE_LIMIT + 1]).unwrap();
        assert_eq!(
            draw(&gzip.finish().unwrap()).unwrap_err(),
            "color_svg_limit"
        );
        assert_eq!(draw(&[0x1f, 0x8b, 8, 0]).unwrap_err(), "color_svg_invalid");
        assert_eq!(
            error(&format!("{}{}", "<g>".repeat(60), "</g>".repeat(60))),
            "color_svg_limit"
        );
        assert_eq!(
            error(r#"<g opacity=".5"><rect width="1000000" height="1000000"/></g>"#),
            "color_svg_limit"
        );
        assert!(draw(b"<svg>").is_err());
    }
}
