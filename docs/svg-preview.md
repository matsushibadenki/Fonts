# SVG-in-OpenTypeプレビュー

フォントライブラリでSVGフォントを選び、パレットアイコンからカラー専用画面を開く。サンプル、サイズ、CPALパレットを指定し「プレビュー」でメインキャンバスへ表示する。英語・日本語・简体中文に対応。対応範囲の説明はⓘ、拒否理由は画面内のエラーで表示する。編集文書・Undo/Redo・保存データは変更しない。読み込んだ元フォントの保持出力は、プレビュー非対応の内容でも従来どおり利用できる。

## 対応範囲

| 内容 | 状態 |
| --- | --- |
| path、rect、circle、ellipse、line、polyline、polygon、g、defs、use | 🟢 [Done] 塗り・線・変形、fill-rule、ローカル参照 |
| 色・透明度 | 🟢 [Done] sRGB、currentColor（既定は黒）、fill/stroke/stop opacity、group opacity |
| linearGradient、radialGradient、clipPath | 🟢 [Done] グラデーションとクリップ |
| CPAL | 🟢 [Done] COLRのないSVGフォントにも対応。var(--colorN, fallback)を解決し、CPALアルファと描画側の透明度を乗算 |
| インラインstyle | 🟢 [Done] 対応する表示属性の単純な宣言。スタイルシート・CSSエスケープ・!important等は拒否 |
| UTF-8 XML／gzip | 🟢 [Done] 展開量に上限。metadata／title／descは描画前に除外 |
| 共有SVG文書 | 🟢 [Done] glyphNをuseとして参照し、対象以外のグリフを描画しない。単一グリフ文書に限りID省略を互換扱い |
| 埋め込みPNG／JPEG | 🟠 [Next] 現在は画像要素を拒否。既存sbix／CBDTのPNGプレビューは対応済み |
| スタイルシート、フィルター、パターン、マスク、入れ子SVG、linearRGB補間等 | 🔴 [Later] 今回の静的ベクター対応範囲外。異なる色や部分的な表示へ暗黙に置き換えず拒否 |
| スクリプト、イベント、外部参照、DTD／実体、foreignObject、テキスト、アニメーション | 非対応。外部通信・ローカルファイルの読み込み・スクリプト実行は行わない |

SVGの原点はベースライン、Y方向は下向き。初期ビューポートはunitsPerEmを用い、viewBoxと明示的なサイズによる変換を反映する。ルートのclip／overflowによる切り取りは適用しない。グリフ送りはhmtx、文字対応はcmapを使用する。高度な組版、絵文字シーケンス、可変軸の操作はこの画面では行わない。これらの座標とグリフ選択の扱いは[OpenType SVG仕様](https://learn.microsoft.com/en-us/typography/opentype/spec/svg)に基づく。

## 描画経路と上限

Rustがフォントを保持し、worker内でSVGの展開・検査・再構築・ラスタライズを行う。resvg/usvg 0.48.1は既定機能を無効化し、テキスト・システムフォント探索・画像デコーダーを追加しない。画像リソース解決器はdata URL用と通常URL用の両方で常にNoneを返す。元XMLをそのままレンダラーへ渡さず、対応要素と属性だけで再構築する。未知の描画要素や属性はエラーにする。

| 対象 | 上限 |
| --- | --- |
| 元フォント／プレビュー領域 | 256 MiB／2048×1024（既存設定） |
| SVG文書 | 圧縮前後それぞれ2 MiB |
| XMLノード／1属性値 | 8192ノード／256 KiB |
| XML・参照の深さ | 48 |
| 展開前の参照グラフ | 重み付き32768。属性長も重みに含め、循環・参照先欠落・ID重複を拒否 |
| 描画ツリー | ノード＋パス点の作業量200000、深さ48 |
| 一時レイヤー | 変換後のレイヤー面積×3を合計し1600万画素まで。非有限・極端な座標も拒否 |

gzipは上限＋1バイトだけ読み、超過時に拒否する。DOCTYPEによる実体展開は無効。参照グラフは展開前に検査し、指数的なuse展開と長い参照連鎖を遮断する。描画前にもレイヤー面積を検査する。これらはアプリの入力制限であり、全SVG機能を無制限に扱うサンドボックスではない。

resvgのpremultiplied RGBAをstraight RGBAへ変換し、RustのImageAssetに保持。既存のwgpu画像テクスチャで描画する。IPCは設定・処理結果のみで、XML、フォント、画像フレームはWebViewへ転送しない。処理中に文書・revision・描画世代が変わった結果は既存の検査で破棄される。

## 検証

🟢 [Done] 独自のMITテストフォントで、10グリフを共有するSVG、gzip、色・透明度・グラデーション、変形・クリップ・穴、currentColor、CPALの両パレット、単色輪郭へのフォールバック、WOFF2往復後の画素一致を検証。生成器は`crates/font-core/tests/data/color/generate_svg.py`で、FontToolsは開発時のみ使用する。

🟢 [Done] SVGレコードの不正なグリフ範囲・オフセット・長さ、外部URL・ファイル参照・data URL画像・script・イベント・DOCTYPE・処理命令・循環参照・重複ID・参照欠落・指数的use・深い参照連鎖・gzip bomb・破損gzip・巨大な半透明レイヤーを拒否する自動テスト。viewBox・ベースライン・ルートの非クリップ・metadataの除外も確認した。

🟢 [Done] macOSの実GPUからsRGB・透明度・向き・パレット切替後の画素を読み戻し。ChromiumのSVGレンダラーとの比較は40,620画素で最大チャンネル差1/255。三言語UIはPlaywrightで幅920／560pxを確認し、IPCを模擬してパレット指定、翻訳済みエラー9通り、エラー後の再プレビュー、はみ出し・コンソールエラーなしを検証した。

⭕️ [Pending] Windows/Linux実機、今回のSVGによるネイティブ画面全体の手動検証、広範な実フォントでの互換性。

実行例:

```sh
cargo test -p font-core --lib svg_color
cargo test -p font-core --test color_fonts
cargo test -p font-renderer svg_colors -- --ignored
cargo run -p font-core --example color_preview -- FONT.ttf OUTPUT.png ABC 0
```

resvg/usvgは選択式からMIT、tiny-skia系はBSD-3-Clauseを維持。関連する著作権・ライセンスを配布用の第三者告知と依存一覧に収録する。実行時に外部コマンド、Python、システム側のSVGライブラリを要求しない。
