use super::*;
use crate::truetype_components::Shape;
pub(crate) fn import_truetype(
    mut doc: FontDocument,
    shapes: &BTreeMap<u16, Shape>,
) -> Result<FontDocument> {
    for g in doc.glyphs.values_mut() {
        for c in &mut g.contours {
            identity::ensure_contour(c)?;
        }
    }
    let referenced: BTreeSet<_> = shapes
        .values()
        .flat_map(|s| s.parts.iter().map(|p| p.glyph))
        .filter(|id| !shapes[id].rings.is_empty())
        .collect();
    if referenced.len() > 256 {
        return Err(Error::TooLarge);
    }
    let ids: BTreeMap<_, _> = referenced.iter().map(|&g| (g, Uuid::new_v4())).collect();
    let mut lib = Library::default();
    for (&glyph, &id) in &ids {
        lib.definitions.insert(
            id,
            Definition {
                info: Info {
                    name: doc.glyphs[&u32::from(glyph)].name.clone(),
                    tags: "TrueType".into(),
                    category: "other".into(),
                    source: doc.family.clone(),
                    license: if doc.metadata.license.len() <= 2048 {
                        doc.metadata.license.clone()
                    } else {
                        String::new()
                    },
                    favorite: false,
                },
                upm: doc.units_per_em,
                source_glyph: u32::from(glyph),
                contours: Arc::new(doc.glyphs[&u32::from(glyph)].contours.clone()),
                children: Default::default(),
            },
        );
    }
    // Build dependencies before evaluating geometry. Empty components have no outlines to link.
    for (&glyph, &id) in &ids {
        for p in &shapes[&glyph].parts {
            if let Some(&child) = ids.get(&p.glyph) {
                lib.definitions.get_mut(&id).unwrap().children.insert(
                    Uuid::new_v4(),
                    Child {
                        definition: child,
                        matrix: p.matrix,
                        nodes: Default::default(),
                        source_instance: None,
                    },
                );
            }
        }
    }
    for id in lib.order()? {
        if lib.definitions[&id].children.is_empty() {
            continue;
        }
        let mut d = lib.definitions[&id].clone();
        let mut contours = vec![];
        // Preserve original component order, independent of UUID map ordering.
        d.children.clear();
        let source_glyph = d.source_glyph as u16;
        for p in &shapes[&source_glyph].parts {
            if let Some(&child) = ids.get(&p.glyph) {
                let mut i = Instance {
                    definition: child,
                    glyph: 0,
                    matrix: p.matrix,
                    nodes: Default::default(),
                };
                contours.extend(regenerate(&lib.definitions[&child], &mut i)?);
                d.children.insert(
                    Uuid::new_v4(),
                    Child {
                        definition: child,
                        matrix: p.matrix,
                        nodes: i.nodes,
                        source_instance: None,
                    },
                );
            }
        }
        d.contours = Arc::new(contours);
        lib.definitions.insert(id, d);
    }
    for (&glyph, shape) in shapes {
        if shape.parts.is_empty() {
            continue;
        }
        let mut contours = vec![];
        for p in &shape.parts {
            if let Some(&id) = ids.get(&p.glyph) {
                let mut i = Instance {
                    definition: id,
                    glyph: u32::from(glyph),
                    matrix: p.matrix,
                    nodes: Default::default(),
                };
                contours.extend(regenerate(&lib.definitions[&id], &mut i)?);
                lib.instances.insert(Uuid::new_v4(), i);
            }
        }
        doc.glyphs
            .get_mut(&u32::from(glyph))
            .ok_or(Error::MissingGlyph)?
            .contours = contours;
    }
    if !lib.definitions.is_empty() {
        doc.schema_version = doc.schema_version.max(if lib.has_nested() { 4 } else { 3 });
    }
    doc.components = lib;
    doc.validate()?;
    Ok(doc)
}
