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
    format!(
        r#"<!doctype html><html lang="{locale}"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>{title}</title><style>
{styles}
@page{{size:A4 portrait;margin:15mm;}}
*{{box-sizing:border-box}}body{{margin:0;background:#ecebe8;color:#242421;font:12px -apple-system,BlinkMacSystemFont,'Helvetica Neue',sans-serif}}nav{{position:sticky;top:0;padding:12px 20px;background:#fafaf9;border-bottom:1px solid #d8d7d3;display:flex;gap:16px;align-items:center;z-index:1}}button{{background:#252d35;color:white;border:0;padding:10px 20px;border-radius:6px;cursor:pointer}}button:disabled{{opacity:.4}}nav span{{font-size:12px}}main{{width:210mm;margin:24px auto;padding:15mm;background:white;box-shadow:0 3px 24px #0001}}h1{{font-size:24pt;font-weight:500;letter-spacing:-.03em;margin:0 0 10mm;border-bottom:2px solid #252d35;padding-bottom:5mm;overflow-wrap:anywhere}}article{{padding:6mm 0 7mm;border-bottom:1px solid #ccc;break-inside:avoid}}article header{{break-after:avoid;margin-bottom:4mm}}h2{{font-size:15pt;font-weight:600;line-height:1.5;margin:0;overflow-wrap:anywhere}}small{{display:block;font-size:9pt;color:#63635e;line-height:1.6}}.style{{display:block;font-size:8pt;color:#777;margin-top:1mm}}.sample{{font-size:{size}pt;line-height:1.65;margin:3mm 0;white-space:pre-wrap;overflow-wrap:anywhere;orphans:2;widows:2}}.missing{{font-size:8pt;color:#85551b}}.failed .sample{{display:none}}@media print{{nav{{display:none}}body{{background:white}}main{{width:auto;margin:0;padding:0;box-shadow:none}}}}@media screen and (max-width:820px){{main{{width:100%;margin:0;padding:20px}}nav{{flex-wrap:wrap}}}}
</style><nav><button id="print" disabled>{print}</button><span>{help}</span><span>{status}: {skipped}{limit}</span></nav><main><h1>{title}</h1>{body}</main><script>
(async()=>{{for(const a of document.querySelectorAll('article')){{try{{await document.fonts.load('24px '+a.dataset.font);if(!document.fonts.check('24px '+a.dataset.font))throw Error();}}catch{{a.classList.add('failed');const p=document.createElement('p');p.textContent='{failed}';a.append(p);}}}}for(const e of document.querySelectorAll('.missing-label'))e.textContent='{missing}';await document.fonts.ready;document.getElementById('print').disabled=false;}})();document.getElementById('print').onclick=()=>window.print();
</script></html>"#,
        title = escape(title),
        locale = if ["ja", "en", "zh_cn"].contains(&locale) {
            locale
        } else {
            "en"
        }
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
