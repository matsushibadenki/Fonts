//! Unicode 17.0 Vertical_Orientation data (Unicode License v3), pinned for reproducibility.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Orientation {
    U,
    R,
    Tu,
    Tr,
}
pub fn orientation(c: char) -> Orientation {
    use std::sync::OnceLock;
    static DATA: OnceLock<Vec<(u32, u32, Orientation)>> = OnceLock::new();
    let data = DATA.get_or_init(|| {
        include_str!("../../../../vendor/unicode/VerticalOrientation-17.0.0.txt")
            .lines()
            .filter_map(|line| {
                let line = line.split('#').next()?.trim();
                if line.is_empty() {
                    return None;
                }
                let (range, property) = line.split_once(';')?;
                let range = range.trim();
                let (a, b) = range.split_once("..").unwrap_or((range, range));
                let property = match property.trim() {
                    "U" => Orientation::U,
                    "R" => Orientation::R,
                    "Tu" => Orientation::Tu,
                    "Tr" => Orientation::Tr,
                    _ => return None,
                };
                Some((
                    u32::from_str_radix(a, 16).expect("pinned Unicode range"),
                    u32::from_str_radix(b, 16).expect("pinned Unicode range"),
                    property,
                ))
            })
            .collect()
    });
    let c = c as u32;
    let i = data.partition_point(|(_, end, _)| *end < c);
    data.get(i)
        .filter(|(start, _, _)| *start <= c)
        .map_or(Orientation::R, |(_, _, p)| *p)
}
