//! AICB is data here: only numeric path and graphics operators are interpreted.
use super::*;
pub fn decode(data: &str) -> Result<Vec<Contour>> {
    if !data.starts_with("%!PS") {
        return Err("clipboard_unsupported");
    }
    let body = data
        .split_once("%%EndSetup")
        .or_else(|| data.split_once("%%EndProlog"))
        .map(|(_, v)| v)
        .ok_or("clipboard_unsupported")?;
    let mut p = Path::default();
    let mut out = vec![];
    let mut nums = vec![];
    let mut m = IDENTITY;
    let mut stack = vec![];
    let mut count = 0;
    for line in body.lines() {
        let line = line.split('%').next().unwrap_or("");
        for token in line.split_whitespace() {
            count += 1;
            if count > 200_000 {
                return Err("clipboard_too_large");
            }
            if let Ok(n) = token.parse::<f64>() {
                if !n.is_finite() || nums.len() > 64 {
                    return Err("clipboard_invalid");
                }
                nums.push(n);
                continue;
            }
            let n = |i: usize| nums.get(i).copied().ok_or("clipboard_invalid");
            match token {
                "m" => p.move_to(map(m, [n(0)?, n(1)?]))?,
                "l" | "L" => p.segment(Segment::Line {
                    end: map(m, [n(0)?, n(1)?]),
                })?,
                "c" | "C" => p.segment(Segment::Cubic {
                    control1: map(m, [n(0)?, n(1)?]),
                    control2: map(m, [n(2)?, n(3)?]),
                    end: map(m, [n(4)?, n(5)?]),
                })?,
                "v" | "V" => p.segment(Segment::Cubic {
                    control1: p.at()?,
                    control2: map(m, [n(0)?, n(1)?]),
                    end: map(m, [n(2)?, n(3)?]),
                })?,
                "y" | "Y" => p.segment(Segment::Cubic {
                    control1: map(m, [n(0)?, n(1)?]),
                    control2: map(m, [n(2)?, n(3)?]),
                    end: map(m, [n(2)?, n(3)?]),
                })?,
                "h" => p.close()?,
                "f" | "F" | "S" | "s" | "B" | "b" | "N" | "n" => {
                    let mut c = p.take()?;
                    if matches!(token, "f" | "F" | "B" | "b" | "s") {
                        for c in &mut c {
                            c.closed = true;
                        }
                    }
                    out.extend(c);
                }
                "q" => {
                    if stack.len() > 32 {
                        return Err("clipboard_too_large");
                    }
                    stack.push(m);
                }
                "Q" => m = stack.pop().ok_or("clipboard_invalid")?,
                "cm" | "concat" => m = multiply(m, [n(0)?, n(1)?, n(2)?, n(3)?, n(4)?, n(5)?]),
                "W" | "W*" | "Tj" | "Tx" | "To" | "XI" | "image" | "colorimage" => {
                    return Err("clipboard_outline_required")
                }
                // AICB style, grouping, page and setup operators do not change path coordinates.
                "u"
                | "U"
                | "j"
                | "J"
                | "M"
                | "w"
                | "d"
                | "i"
                | "D"
                | "k"
                | "K"
                | "g"
                | "G"
                | "r"
                | "R"
                | "Xa"
                | "XA"
                | "x"
                | "X"
                | "O"
                | "A"
                | "XR"
                | "showpage"
                | "end"
                | "Adobe_Illustrator_AI5"
                | "begin" => {}
                _ => return Err("clipboard_unsupported"),
            }
            nums.clear();
        }
    }
    out.extend(p.take()?);
    Ok(out)
}
