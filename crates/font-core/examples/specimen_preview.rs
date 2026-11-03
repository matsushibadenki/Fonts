//! Generate a print-layout fixture from local font files for visual QA.
fn main() {
    let args: Vec<_> = std::env::args().collect();
    let dir = std::path::Path::new(&args[1]);
    std::fs::create_dir_all(dir).unwrap();
    let mut css = String::new();
    let mut body = String::new();
    for (i, path) in args[2..].iter().enumerate() {
        let bytes = font_core::specimen::standalone(&std::fs::read(path).unwrap(), 0).unwrap();
        ttf_parser::Face::parse(&bytes, 0).unwrap();
        std::fs::write(dir.join(format!("font{i}.otf")), bytes).unwrap();
        css.push_str(&format!(
            "@font-face{{font-family:s{i};src:url('font{i}.otf')}}"
        ));
        for _ in 0..3 {
            body.push_str(&font_core::specimen::card(
                i,
                if i == 0 {
                    "Hiragino Mincho ProN"
                } else {
                    "Arial"
                },
                if i == 0 {
                    Some("ヒラギノ明朝 ProN")
                } else {
                    None
                },
                "Regular",
                &[
                    "美しい文字で、日々の風景を描く。".into(),
                    "The quick brown fox jumps over the lazy dog. 0123456789".into(),
                ],
                0,
            ));
        }
    }
    std::fs::write(
        dir.join("index.html"),
        font_core::specimen::html(
            "文字の見本帳 — Font Specimens",
            &css,
            &body,
            24,
            "ja",
            0,
            false,
        ),
    )
    .unwrap();
}
