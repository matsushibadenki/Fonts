# 編集画面の方向性

frontend-app-builderのデザイン検討としてbuilt-in ImageGenで概念図を作成。実装ではRust native surfaceに合わせ、輪郭パネルを下部ドックに配置し、移動後はタブ化できる構成とした。概念図の未実装の操作は画面に並べない。

- Chrome: #f4f3f0、Canvas: #ffffff、Text: #25272b、Accent: #aa563d。
- UI書体: system sans、基本12px。ツール30px、パネル見出し30–42px。
- 枠線で区切るコンパクトな編集領域。説明文はinformationアイコンへ集約。
- Concept: editor-concept.png。ImageGenの概念図であり、動作画面のスクリーンショットではない。

生成時のprompt:

> Use case: ui-mockup. Full desktop screen concept for Fonts, a compact professional font creation desktop editor. 1440x900 macOS app. Restrained warm light gray chrome #f4f3f0, true white canvas, charcoal text, muted terracotta #aa563d accent. Very compact 36px toolbar, square line icons. Layout: top thin title bar Fonts / Untitled Sans, four short workflow tabs Draw Space Test Export. Left dock with Glyphs header, drag grip, info and popout icons, A–Z grid. Center canvas with geometric A, hole, guides and square control nodes. Right Metrics and Outline panels. Dense but legible, sans UI typography, no cards, gradients, photos or explanatory paragraphs.
