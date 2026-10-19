# フォント形式の入出力と変換

ライブラリの一覧には保存元とは別に実形式を表示する。下部の「出力形式」で形式を選び、選択行の変換アイコンでファイルを直接変換する。「制作中のフォントを出力」は現在のRust Documentを書き出す。元ファイルの複製は別の操作として残す。

ヒンティングを保持した単体TTF／OTFの抽出と、TrueType命令だけの除去は、ライブラリの専用アイコンから行う。[対応範囲と検証](hinting.md)。通常の輪郭編集・形式変換とは別の処理で、元の組版・カラー・可変テーブルも保持する。

## 対応表

| 形式 | 読込 | 書出・変換先 | 内容 |
| --- | --- | --- | --- |
| TrueType / OpenType glyf `.ttf` | 🟢 [Done] | 🟢 [Done] | Unicode cmap、二次曲線。三次曲線は適応近似 |
| OpenType CFF `.otf` | 🟢 [Done] | 🟢 [Done] | CFF1、Type2 CharStrings、Unicode cmap、三次曲線 |
| OpenType CID-CFF `.otf` | 🟢 [Done] | 🟢 [Done] | ROS、CID charset、FDArray/FDSelect、Unicode cmap |
| OpenType CFF2 | 🟢 [Done] 既定インスタンス | 🔴 [Later] 可変出力 | 読込後の出力は静的CFF1または他の静的形式 |
| TrueType Collection / OpenType Collection `.ttc/.otc` | 🟢 [Done] フェイス単位 | 🔴 [Later] コレクション生成 | 単一フェイスから各形式へ変換できる。元ファイル複製はコレクション全体 |
| Type1 ASCII `.pfa` | 🟢 [Done] | 🟢 [Done] | eexec、Type1 CharStrings。出力は新しいヒンティングなしの静的フォント |
| Type1 binary `.pfb` | 🟢 [Done] | 🟢 [Done] | PFBセクションと暗号化CharStrings。256符号枠とは別にUnicode由来のグリフ名も出力 |
| PostScript CIDFontType 0 `.cid/.ps` | 🟢 [Done] | 🟢 [Done] | CID Type1、単一FDへ展開した輪郭、CIDMapとCIDSystemInfo |
| 単体CFF / CID-CFF `.cff` | 🟢 [Done] | 🟢 [Done] | 名前ベースCFFとCIDベースCFF。Unicodeはグリフ名または別CMapで管理 |
| Unicode／CID CMap `.cmap` | 🟢 [Done] 対応付けの適用 | 🟢 [Done] UTF32 Unicode→CID | フォントの輪郭ファイルとは別の対応表 |
| Adobe Fonts | 🟢 [Done] 検出ファイルの実形式で処理 | 🟢 [Done] 実形式から上記へ変換 | 配信サービスであり独自の出力形式ではない。Creative Cloudのキャッシュ書換・有効化制御・独自データの復号は行わない |

Type1/CID/単体CFFの読込には同梱のFreeType 2.14.3を使う。Rustのワーカー内でフォントデータを解析し、PostScriptプログラムは実行しない。書出側はこのプロジェクト内のRust実装。FontForge、Ghostscript、Python、FontTools、外部のフォント変換コマンド、OSにインストールされたFreeTypeは実行時に必要ない。独立した検証には開発環境のFontToolsを使用した。

## CIDと文字対応

CIDはUnicode値として解釈しない。元データにCIDSystemInfoがあればRegistry/Ordering/Supplementと各グリフのCIDをDocumentに保存する。CIDベースの出力先ではこれを維持し、普通のフォントをCID化する場合はAdobe-Identity-0と安定した連番を使う。CIDを表現できない形式へ変換すると、その形式のUnicode対応やグリフ名を使用する。

単体CIDフォントはUnicode対応を持たないことがある。読み込むと「文字未割当」の数とROSがライブラリ下部に表示される。上部のCMapアイコンから対応表を適用でき、元に戻す／やり直すは1トランザクションとなる。ファイル選択後に文書が変わっていたらepoch/revisionの競合として拒否する。

CMap入力は`begincidchar`、`begincidrange`、`beginbfchar`、`beginbfrange`（配列の宛先を含む）を扱う。ToUnicode側のUTF-16サロゲート対から補助平面文字も読み込む。Unicode→CIDのCMapはUTF32形式で書き出し、同一CIDへのUnicode別名も保持する。名前ベースType1／単体CFFへの出力では、Unicode別名に同一の輪郭を持つCharStringを追加し、文字対応が消えないようにする。

- 🟠 [Next] `usecmap`の継承解決とAdobe文字コレクションの標準CMapプリセット。現在は継承を含むCMapを黙って部分適用せずエラーにする。
- 🔴 [Later] Unicodeの複数文字シーケンス・異体字シーケンスとGSUB/GPOS。現行モデルは単一Unicodeスカラー値の割当で、CMapに複数文字の宛先があればエラーにする。
- 🔴 [Later] Shift-JIS等の非Unicode CMapの文字コード変換。名前付き非Unicode CMapをUnicodeとして誤解釈せず拒否する。

CMapは最大4 MiB。PS手続き・継承先ファイルを実行・読込しない。存在しないCID、重複・衝突するUnicode対応、不正なUTF-16やサロゲート値は文書を変更せずに拒否する。

## 保持する情報と制約

- 🟢 [Done] 輪郭、送り幅、UPM、Unicode対応、CID対応とROS。OpenType/TrueTypeでは上下端、スタイル、太さ・幅区分、斜体角、bold/italicフラグ、著作権・ライセンス・ライセンスURLも保持する。PostScript系ではFontInfoで表現できるスタイル・斜体角・Noticeを読み書きする。
- 🟢 [Done] 二次→三次曲線は幾何変換し、三次→二次曲線は適応近似する。Type1の数値は1/64単位、CFFは16.16形式、TrueTypeは整数座標へ量子化する。
- 🟢 [Done] JSONスキーマ1の旧文書は、新しいmetadata／CIDフィールドがなくても読み込める。新しいフィールドはJSONへ保存する。
- 🔴 [Later] ヒンティング・外部のGSUB/GPOS・可変軸・カラー・SVG、Type1 Multiple Master、元のサブルーチンや複数FD構造の保持。現在は輪郭を展開して新しい静的フォントを生成する変換であり、元のバイナリ構造のラウンドトリップではない。
- 🔴 [Later] AFM/PFMの外部メトリクス・カーニングとPostScript Type0合成フォントのラッパー。単体PostScript系の行メトリクスはFontBBox等から得られる値に依存する。
- ⭕️ [Pending] Windows/Linuxでのビルド・実機読込・出力フォントのOS登録、各種プリンター／PostScriptインタープリターでのType1/CID印刷確認。

現行OSへのインストールボタンは、選択形式によらずTrueTypeとしてインストールすることを明記する。Type1/PFB/CIDそのものが最新OSやAdobeアプリで利用できることを意味しない。

## 検証

Rustの往復テストはTTF、OTF CFF、OTF CID-CFF、PFA、PFB、CID Type1、単体CFF、単体CID-CFFを対象とする。三次・二次曲線、複数輪郭、補助平面、Unicode別名、疎なCIDとAdobe-Japan1-7、スタイル・Notice、CMapのUndo/Redo、不正入力の拒否を検証する。FreeTypeの実行時バージョンも2.14.3に固定してテストする。

開発環境のFontToolsでOTFの全テーブルとCharString描画、単体CFFのCharString描画・再コンパイル、PFA/PFBのeexec解読・CharString描画を確認。CID Type1はFreeTypeで検証する。生成器の手動確認用サンプルは`cargo run -p font-core --example font_formats --offline -- /tmp/fonts-formats-qa`で作成できる。

Rustワークスペースの204テスト、Clippy（警告をエラーとして扱う）、型検査・UIビルドが成功。GPU実機確認とOSフォント有効化の2テストは通常実行の対象外。macOSの検証用アプリでPFB→OTFの直接変換、CID Type1の編集用読込、CMap適用、OTF出力を操作し、画面から出力した2ファイルもFontToolsでUnicode対応・全グリフの描画を検証した。

## ソースと配布

- ローカルFontForgeの`parsepfa.c`、`psread.c`、`dumppfa.c`、`tottf.c`から形式の扱いを確認。FontForgeのコードや実行ファイルへの依存を追加していない。
- FreeTypeのソースとRustのFFIバインディングは`vendor/freetype-sys`に保持する。出典・SHA-256・変更点は[vendor-font-engine.json](vendor-font-engine.json)。ライセンス告知をアプリのresourcesに同梱する。PNGデコーダーは本用途で不要なのでビルドしない。
- [Adobe Type1 Font Format](https://adobe-type-tools.github.io/font-tech-notes/pdfs/T1_SPEC.pdf)
- [Adobe Font Technical Notes（CFF、Type2、CID/CMap仕様）](https://adobe-type-tools.github.io/font-tech-notes/)
- [OpenType CFF仕様](https://learn.microsoft.com/en-us/typography/opentype/spec/cff)
- [FreeTypeのリリース情報](https://freetype.org/index.html)
- [Adobe Fontsの利用条件FAQ](https://helpx.adobe.com/fonts/web/font-licensing/font-licensing.html)

macOSの読込・変換用ダイアログは非同期のネイティブシートを使い、Type1/CID等がOSのUTIフィルターで除外されないようにする。ファイル内容はRust側で検証する。保存時は選択した形式と拡張子を照合し、拡張子の自動追加で別の既存ファイルを上書きしない。

## 日本語・中国語などの大きな書体

- 🟢 [Done] 最大65,535グリフ（.notdefを含む）。1回の文字追加は65,534文字、入力1 MiBまで。上限超過時は全体を変更せずに拒否する。
- 🟢 [Done] 互換用cmap format 4を連続範囲・差分とglyphIdArrayで圧縮。8,188文字を超えてもBMP対応を保持できる。16-bit長を超える病的な疎配置ではformat 12を完全な対応表として維持する。補助平面もformat 12で保持する。
- 🟢 [Done] JSON保存・再読込は512 MiB、インポート輪郭総数は1600万セグメントまで。フォント入力256 MiB、グリフ単位の複雑さ制限は継続する。処理中のメモリ使用量は書体の複雑さに依存する。
- 🟢 [Done] 一覧は256件ずつ描画し、ページ番号で直接移動できる。文字・コードポイント・名前・CIDで検索できる。
- 🟢 [Done] macOSローカル書体で、日本語20,339グリフ／中国語29,352グリフを検証。読込→送り幅編集→Undo/Redo→JSON保存・復元→TTF/CID-CFF出力→再読込で全グリフのUnicode対応と送り幅を比較。JSONはそれぞれ88,545,268／166,372,181 bytes。FontToolsでも両出力の全テーブルとBMP対応を確認した。元ファイルは変更せず、検証データは/tmpに保存し、配布物に同梱しない。
- 🟢 [Done] macOS画面でも166 MBの中国語プロジェクトを開き、一括追加21,060文字の一覧で83ページ目への直接移動を確認。
- 🟢 [Done] 自動テストでは約2万のCJK文字、順序の異なるグリフID、最大65,535グリフと上限を超える追加の原子性を検証。ローカル実書体で再現する手順は `cargo run -p font-core --example cjk_roundtrip --offline -- INPUT_FONT OUTPUT_DIRECTORY`。
- 🟠 [Next] 異体字シーケンス（cmap format 14）・縦組み（vhea/vmtx、GSUB/GPOS）の編集と保持。現在の大規模対応も静的な輪郭・基本情報・単一Unicode対応を対象とする。
- ⭕️ [Pending] Windows/Linux実機での大規模書体の操作と出力後のOS描画確認。

仕様: [OpenType cmap](https://learn.microsoft.com/typography/opentype/spec/cmap)。

🟢 [Done] 作成した横組みの明示ペアはTTF／OTF CFF／OTF CID-CFFのGPOSへ出力し、生成した単純なペア構造の再取り込みも可能。ペアのある文書を単体CFF／Type1等へ出力すると、損失を避けるためエラーで案内する。[対応範囲](pair-kerning.md)。

🟢 [Done] クラスカーニングをGPOS Format 2へ出力。明示ペア・0の例外を優先し、生成した限定的な構造の再取り込みにも対応する。[対応範囲](class-kerning.md)。

## WOFF2出力

🟢 [Done] TrueType／CFFの静的WOFF2、4段階の圧縮、三言語UI・保存ダイアログ・ファイル検証。[仕様と検証](woff2.md)。WOFF2入力や可変・カラーの保持は未対応。

可変フォントの軸設定・静的化は[variable-fonts.md](variable-fonts.md)を参照。ライブラリで可変フォントを一つ選び、軸アイコンから設定する。TrueType gvar／OpenType CFF2、avar、HVAR、MVARを展開し、TTF／OTF／WOFF2を出力する。
