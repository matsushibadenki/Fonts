//! Self-contained print layout with exact source faces, including TTC members.
use std::collections::BTreeMap;
pub fn standalone(data: &[u8], index: u32) -> Result<Vec<u8>, String> {
    let face = ttf_parser::Face::parse(data, index).map_err(|_| "font_invalid")?;
    let mut tables = BTreeMap::new();
    for record in face.raw_face().table_records {
        let tag = record.tag.to_bytes();
        if tag == *b"DSIG" {
            continue;
        }
        let mut data = face
            .raw_face()
            .table(record.tag)
            .ok_or("font_invalid")?
            .to_vec();
        if tag == *b"head" {
            data.get_mut(8..12).ok_or("font_invalid")?.fill(0);
        }
        tables.insert(tag, data);
    }
    crate::color_fonts::assemble(
        tables,
        face.tables().cff.is_some() || face.tables().cff2.is_some(),
    )
}
pub fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
pub fn card(
    id: usize,
    en: &str,
    ja: Option<&str>,
    style: &str,
    samples: &[String],
    missing: usize,
) -> String {
    let mut s = format!(
        "<article data-font='s{id}'><header><h2>{}</h2>",
        escape(ja.unwrap_or(en))
    );
    if ja.is_some() {
        s.push_str(&format!("<small>{}</small>", escape(en)));
    }
    s.push_str(&format!(
        "<span class='style'>{}</span></header>",
        escape(style)
    ));
    if missing > 0 {
        s.push_str(&format!(
            "<div class='missing'>△ {missing} <span class='missing-label'></span></div>"
        ));
    }
    for text in samples {
        s.push_str(&format!(
            "<p class='sample' style='font-family:s{id}'>{}</p>",
            escape(text)
        ));
    }
    s.push_str("</article>");
    s
}
pub fn html(
    title: &str,
    styles: &str,
    body: &str,
    size: u32,
    locale: &str,
    skipped: usize,
    truncated: bool,
) -> String {
    let (print, help, missing, failed) = match locale {
        "ja" => (
            "印刷 / PDF保存",
            "A4・倍率100%・ヘッダーとフッターなし。PDFは印刷ダイアログで保存できます。",
            "見本に含まれる未収録文字（代替フォントで表示）",
            "フォントを読み込めません",
        ),
        "zh_cn" => (
            "打印 / 保存PDF",
            "A4、缩放100%、关闭页眉页脚。在打印对话框中保存PDF。",
            "样文中缺少的字符（使用替代字体）",
            "无法加载字体",
        ),
        _ => (
            "Print / Save PDF",
            "A4, scale 100%, headers and footers off. Save PDF in the print dialog.",
            "unsupported sample characters (fallback font)",
            "Font could not be loaded",
        ),
    };
    let status = match locale {
        "ja" => "対象外",
        "zh_cn" => "已跳过",
        _ => "Skipped",
    };
    let limit = if truncated {
        match locale {
            "ja" => " · 上限に達したため一部のみ掲載",
            "zh_cn" => " · 已达到上限，仅包含部分字体",
            _ => " · Limit reached; partial collection",
        }
    } else {
        ""
    };
    let page_label = match locale {
        "ja" => "ページ",
        "zh_cn" => "页",
        _ => "Pages",
    };
    format!(
        r#"<!doctype html><html lang="{locale}"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>{title}</title><style>{styles}
{layout_css}</style><body style="--sample-size:{size}pt"><nav><button id="print" disabled>{print}</button><span>{help}</span><span>{status}: {skipped}{limit}</span><span>{page_label}: <b id="page-count">—</b></span><span id="layout-error" role="alert"></span></nav><template id="book-title"><h1>{title}</h1></template><template id="cards">{body}</template><span id="missing-label" hidden>{missing}</span><span id="load-error-label" hidden>{failed}</span><main id="pages"></main><script>{layout_js}</script></body></html>"#,
        title = escape(title),
        locale = if ["ja", "en", "zh_cn"].contains(&locale) {
            locale
        } else {
            "en"
        },
        layout_css = include_str!("specimen.css"),
        layout_js = include_str!("specimen-layout.js"),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn names_and_untrusted_text() {
        let s = card(
            0,
            "English",
            Some("日本語"),
            "Regular",
            &["<script>".into()],
            0,
        );
        assert!(s.contains("<h2>日本語</h2><small>English</small>"));
        assert!(s.contains("&lt;script&gt;"));
        assert!(!card(0, "English", None, "", &[], 0).contains("<small>"));
    }
    #[test]
    fn paper_geometry() {
        let h = html("Test", "", "", 24, "en", 0, false);
        assert!(h.contains("size:A4 portrait;margin:15mm"));
        assert!(h.contains("await document.fonts.ready"));
    }
}
