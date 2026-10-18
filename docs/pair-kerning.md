# ペアカーニング

2026-10-18更新。

🟢 [Done] 明示的な横組みグリフペアの編集・試し組み・TTF／OTF出力。文書と履歴はRustに保持し、各ウインドウは命令と要約だけを共有する。

## 操作

1. 左側の文字を文字一覧で選ぶ。
2. メトリクスの「ペアカーニング」で、右のグリフを文字・グリフ名・`#ID`で指定する。入力候補は最大24件。
3. フォント単位の整数を入力してチェックアイコン、またはEnterで適用する。負の値で詰め、正の値で広げる。0または削除アイコンで解除する。
4. 目のアイコンで、そのペアを繰り返した文字列の試し組みへ移る。試し組みのAVアイコンで適用／無効を比較できる。固定見本は設定時のペアを保持する。
5. TTF、OpenType CFF、OpenType CID-CFFとして書き出す。プロジェクトJSONにも保存され、自動復旧で保持される。

ペアはUnicode値ではなく左右のグリフIDに結び付く。別のUnicode割当や複数のUnicode別名でも同じ値を使う。Unicode未割り当てのグリフも`#ID`で編集・出力できる。文字列による試し組みには両方のグリフへのUnicode割当が必要。

## データと整合性

- `kerning`は左ID・右ID順の重複しない配列。空の場合はJSONから省略し、従来のプロジェクトは空の設定として読み込む。
- 調整値は−32768〜32767の整数。最大4096ペア。存在しないID、.notdef、重複・未整列の保存データを拒否する。通常の0適用はペア削除だが、「0の例外ペア」で明示0を登録するとクラス値を打ち消せる。
- 1回の適用・変更・削除は1回のUndoで戻せる。同値・未登録ペアへの0は履歴を増やさない。編集途中や古いepoch／revisionでの命令は確定しない。
- フローティングのメトリクスも同じRust文書へ編集する。ペアの試し組みはメインウインドウへ反映される。

## 試し組みと出力

試し組みは生成したTTFのペアを使う。抽出文字セットに残った左右のペアだけを保持し、元の疎なIDから出力用IDへ付け替える。字間は別の加算として併用できる。明示改行・折返し・タブ・未収録文字をまたいだペアは適用しない。カーニングOFFは文書の値を削除せず、プレビューのみを切り替える。

TTF／OTFではGPOS 1.0の`kern` feature、DFLT scriptの既定LangSys、Lookup Type 2／PairPos Format 1、Value1.xAdvanceとして出力する。クラス設定がある場合は同じlookupの後ろにFormat 2を置き、明示ペアを優先する。旧`kern`テーブルは併記しない。このアプリが生成した単純なGPOSペア構造は再取り込みできる。仕様は[Microsoft OpenType GPOS](https://learn.microsoft.com/en-us/typography/opentype/spec/gpos#pair-adjustment-positioning-format-1-adjustments-for-glyph-pairs)、[共通テーブル形式](https://learn.microsoft.com/en-us/typography/opentype/spec/chapter2)に従う。

単体CFF／CID-CFF、PFA／PFB、CID Type1にはペアを埋め込まない。ペアがある文書をこれらの形式で出力しようとするとTTF／OTFを促すエラーにし、黙って設定を落とさない。CMapは対応表だけの出力なので影響しない。

## 対応外

🔴 [Later] 外部フォントの任意のGSUB／GPOS、旧kern、言語・script別の条件、contextual positioning、可変・縦組み・RTL、AFM／PFMの読み書き。複雑な組版を単純なペアへ展開したり、そのままラウンドトリップする機能ではない。

## 検証

🟢 [Done] ワークスペースの通常Rustテスト291件、Clippy（`-D warnings`）が成功。Rustで追加・変更・0削除・Undo/Redo・履歴分岐、不正データ・revision・4096件上限、Unicode別名と疎なID、未割当グリフ、試し組みのON/OFF・字間・折返し・改行、JSONの新旧互換と復旧チェックポイントを検証する。

🟢 [Done] TTF／OTF CFF／OTF CID-CFFをttf-parserと独立したfontToolsで読み、GPOSの構造・値・チェックサムを確認。HarfBuzzで`AV = −120`／`To = +25`を検証し、送り幅600がそれぞれ480／625、`kern=0`では600へ戻ることを確認。

🟢 [Done] macOSの独立した検証アプリでペアの登録、Undo/Redoによる値の復元、wgpu試し組みのON/OFF、保存、画面からのTTF出力を確認。分離メトリクスからのペア選択・試し組み・再ドッキングも確認。保存JSONと実際の出力TTFにも−120が残ることを確認。

⭕️ [Pending] Windows/Linuxの実画面、複数モニター・DPI、外部組版アプリごとの表示確認。

🟢 [Done] [クラスカーニング](class-kerning.md)と0の例外ペアへ接続済み。
