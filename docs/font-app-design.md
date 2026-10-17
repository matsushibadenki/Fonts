# フォント制作アプリ設計書

現在の優先順位と完了状態は[ROADMAP](ROADMAP.md)を参照。以下の工程記録には、後続実装で解消済みの制約を含む。

作成日: 2026-10-02。仮称: Fonts。対象: Tauri 2 + Rustの独立したデスクトップアプリ。

## 1. 目的と決定事項

専門知識がない人でも「描く → 揃える → 試す → 完成」の工程でフォントを制作できるようにする。LumaPaintに機能を追加するのではなく、独立した製品として開発する。専門機能は削除せず、適切な初期値、自動処理、詳細設定に分けて提供する。

参照チャット「FontForge移植計画」のアイデアを出発点にし、以下のローカルソースで実装可能性を確認した。チャットで提案された機能を実装済みとは扱わない。

- FontForge: commit `dffac91b54519a7033c909df24eb1e6109fac7cb`。
- LumaPaint: commit `f2cf8225a9d07fe238b70f17bfeb5d5f7eed519e`。作業ツリーの実ファイルをコピーし、SHA-256を `reuse-manifest.json` に記録した。コミットと実ファイルの同一性は前提にしない。

2026-10-15に実装を開始。コピー済みのLumaPaint coreに加えて、`font-core` に文書モデル、revision付き文字編集、Undo/Redo、表示座標adapter、保存・再読み込みを実装した。Tauri画面、macOSネイティブwgpu描画、パネルのドッキング・フローティング、標準メニュー、新規・読込・保存を実装済み。フォント出力とFontForge連携は未実装。

## 2. ユーザー体験

### 新規作成

フォント名、作りたい文字、用途を選ぶ。文字セットは英数字、かな・記号、自分で入力から開始する。日本語・简体中文の大量の漢字は段階的なセット追加で対応し、最初から数万の空欄を表示しない。用途は本文・見出し・手書きなどの初期設定を選ぶものとし、完成品質を保証するラベルにはしない。

### 描く

文字一覧は制作進捗表として表示する。未作成、作成中、確認済み、問題ありを区別する。スペースのように輪郭を持たない文字も完成扱いにできる。文字を開くと、ペン、選択、ノード編集、図形、ズーム、元に戻すを提供する。基準線・文字の上端・下端・左右の余白を日本語で説明し、詳細表示でbaseline、ascender、descenderなどを併記する。

手書き入力の線はそのままフォント輪郭にならない。線幅を持つストロークを閉じた塗り輪郭に変換し、変換結果を確認してから採用する。画像の自動トレースは後期機能とする。

### 揃える

高さ、左右の余白、文字の間隔をサンプル文とともに調整する。基本画面では視覚操作を優先し、詳細設定でadvance width、左右のside bearing、pair kerningを編集する。全体の余白調整と特定の文字ペアの字間調整は別の操作とする。自動調整は提案をプレビューし、一括適用をUndo可能にする。

### 試す

任意の文章を入力し、本文・見出しのサイズで試す。編集中の輪郭を並べる簡易表示と、実際に生成したフォントをshapingする検証表示を区別する。書き出し前には後者を必須にし、欠けた文字、合字、結合文字、字間の問題を示す。システムフォントへのfallbackは欠字を隠さない表示にする。

### 完成

用途「パソコンで使う」を静的TTFへ対応させるのをMVPとする。OTF（CFF）、Web用WOFF2、可変フォントは後続工程。フォント名、スタイル、著作者、ライセンス、対象文字を確認し、問題を直すボタンを文字編集へつなげる。書き出しで元のプロジェクトを変更しない。インストールはファイル保存と別の明示的な操作にする。

## 3. MVPと非目標

MVPは単一masterのモノクロ輪郭、Unicodeマッピング、文字一覧、ベジェ編集、基本metrics、pair kerning、保存・復旧、実フォント試し組み、静的TTF出力。英数字の小さな一式を最初の受け入れ対象とし、日本語・简体中文UIと任意のUnicode文字の作成を設計に含める。多言語UI対応と日本語全文字セットの完成は別の達成条件である。

初期段階でFontForgeの全機能を再現しない。複雑なGSUB/GPOS編集、縦組み、CID、variable font、color font、手動hinting、既存フォントの無損失往復は後期に扱う。未対応テーブルのあるフォントは取り込み時に説明し、黙って破棄・上書きしない。

## 4. アーキテクチャ

```text
Tauri WebView（画面・入力・小さな状態DTO）
                │ Commands / Events
Rust Application State（唯一の状態所有者）
 ├ DocumentStore / EditTransactions / Undo
 ├ FontDocument / Glyph / Metrics / Features
 ├ Renderer（wgpu、必要に応じSkiaによるパス演算）
 ├ FontBackend（import / validate / export）
 └ JobManager（生成・shaping・復旧）
                │ owned snapshot / bounded jobs
FontForge adapter（将来ローカルに収容するC core）
```

Document、State、RendererはRust側に置く。各Tauriウインドウはdocument ID、view IDを指定するUIであり、ドキュメントの正本を持たない。選択・ツール・viewportなどのview stateもRustがview IDごとに管理する。変更commandにはdocument revisionを付け、古いrevisionの操作は競合として返す。編集・保存・書き出しに同じtransaction境界を使う。

GPU描画はRustのwgpuとネイティブsurfaceで行う。WebViewとの重ね合わせ、透過、DPI、resize、focus、IME、複数ウインドウについてmacOSで先行検証し、Windows/Linuxの受け入れ条件も用意する。GPU処理をWebViewに移さない。Skiaはパス演算などに限定して導入可否を検討する。

Rust↔JavaScriptでは操作、要約、ページ単位の一覧、小さなエラー情報のみ交換する。フレームbuffer、全glyph輪郭、大量のサムネイルをイベントで送らない。画像・下絵・GPU cacheはRust側に保持し、ファイル取り込みもパスまたは限定されたresource handleで行う。一覧は仮想化し、描画結果はネイティブ描画領域で表示する。

command例: `create_project`、`open_glyph`、`apply_edit`、`set_spacing`、`undo`、`save_project`、`start_export`、`cancel_job`。イベント例: `document_changed`（revisionと変更ID）、`job_progress`、`validation_summary`。キャンセル・失敗時は完成ファイルを公開しない。

## 5. FontDocumentと幾何モデル

| モデル | 保持する情報 |
|---|---|
| FontDocument | schema version、UUID、revision、family/style、UPM、global metrics、glyph順序、設定 |
| Glyph | 安定ID、名称、Unicode scalarの対応集合、輪郭、components、advance、状態 |
| Contour | 閉じた輪郭、安定node ID、line/quadratic/cubic segment、向き |
| Component | 参照glyph ID、affine transform。循環参照を拒否 |
| Spacing | glyphのadvanceと余白、pair/class kerning、script/language適用範囲 |
| Feature | 将来GSUB/GPOSを拡張できる型付きモデル。未対応データは明示 |

正本はfont units、Y上向き、f64座標を基本とする。表示用LumaPaintパスのf32、SVGのY下向きとの変換はadapterに閉じ込める。既存coreをフォント正本としてそのまま保存しない。Unicodeの文字とglyphは1対1ではなく、未割当glyph、複数コードポイントによる同じglyphの参照、合字を将来許容する。

二次曲線→三次曲線は幾何的に変換できるが、三次→二次のTTF出力は許容誤差を指定した近似が必要。制御点、点番号、hint情報の保持は別問題なので、読み込み・再出力の同一性は保証しない。正本の輪郭をflattenして保存しない。輪郭の向きは出力形式と穴の包含関係に基づき判定する。

## 6. FontForgeから確認した機能境界

以下は調査時点の相対パスを記録したもので、削除後のビルド依存ではない。

| 調査対象 | 設計への反映 |
|---|---|
| `fontforge/splinefont.h`, `splinechar.h` | SplineFont/SplineCharと輪郭・参照・metricsの分離。UIへC構造体を露出しない |
| `fontforge/splineorder2.c`, `splinefit.c` | 曲線次数の変換・近似は出力処理として扱う |
| `fontforge/splineoverlap.c`, `splineutil.h` | 重なり・輪郭処理はバックエンドの役割。非破壊の提案と出力snapshotで処理 |
| `fontforge/splinefont.h` のSCValidate/SFValidate | glyphとfont全体の検証を区別する |
| `fontforge/autowidth.c`, `fvmetrics.c` | 自動幅とmetrics編集を分ける。自動幅だけで良いkerningは完成しない |
| `fontforge/lookups.c`, `tottfgpos.c`, `parsettfatt.c` | lookupとGSUB/GPOSはscript/languageを伴う独立した機能 |
| `fontforge/parsettf.c`, `tottf.c`, `savefont.c` | 読み込み、テーブル生成、出力経路を分ける |
| `fontforge/sfd.cpp`, `sfd1.c`, `ufo.c` | SFD/UFOは交換形式の候補。新アプリの正本形式は独立 |
| `fontforge/mm.c`, `parsettfvar.c`, `tottfvar.c` | 可変フォントはmaster互換性が必要な後期機能 |
| `CMakeLists.txt`, `fontforge/CMakeLists.txt` | ENABLE_GUIとfontforge library targetを確認。GUIなしでも依存ライブラリ・初期化の検証は必要 |

FontForgeのCソースを数ファイルだけコピーしてコンパイルできるとは判断しない。次工程でGUI無効のheadlessビルド、依存閉包、初期化・終了、エラーcallback、global state、スレッド制約を検証する。CのポインタをJSへ渡さず、Rust側のopaque handleと明確なfree関数を持つ狭いC ABIを設計する。FontForge操作はまず専用単一workerに直列化し、UI threadでは実行しない。バックグラウンドジョブはrevisionを固定したsnapshotを処理する。

採用する場合は必要なソース・headers・CMake・通知を `vendor/fontforge-core` へ収容し、独立ビルドを確認する。採用しない場合はFontBackend interfaceを維持し、Rust実装に置換する。現時点でどちらも完成していない。旧GUI、gdraw、Python scriptingは製品の必須構成にしない。

## 7. LumaPaintからの流用

今回は `crates/lumapaint-core` を一式コピーした。ベジェ編集はVectorObjectなどの型と関連処理に依存するため、最初から単一ファイルに切り詰めず、テスト可能な単位を保存した。元プロジェクトへのpath dependencyやsymlinkは追加していない。

| コピーした領域 | 用途と制限 |
|---|---|
| `bezier.rs` | SVG曲線変換、制御点・輪郭操作、分割など。font座標へのadapterが必要 |
| `vector.rs` | ベクター型、変換、hit testing。テキスト・paintの型はfont正本に混ぜない |
| `graph.rs`, `selection.rs` | 汎用編集の検討材料。glyph固有の選択へ適応する |
| `document.rs`, `tiles.rs`ほか | 依存閉包と既存テストの保持用。ペイントdocumentをFontDocumentの代替にしない |

既存rendererにはwgpu 27、optional Skia、resvg、rustybuzzなどの依存があり、Skiaのパス処理にもformats/core依存がある。初回はrenderer、Tauri起動コード、UI、vendorパッチをコピーしなかった。第2工程では `native_frame` のsafe-area座標変換をコピー・適応し、所有権・surface構築・main-thread dispatchのパターンを参考に独立した `font-renderer` とTauri hostを実装した。由来は `reuse-native.json` に記録する。旧renderer全体への依存はない。既存Undoがそのままfont編集に利用できるとは前提にせず、FontDocument transactionを新設する。

コピー履歴とhashは `reuse-manifest.json` に保存する。コピー元のroot LICENSEとcoreのlicense宣言は確認できなかったため、配布前に権利・ライセンスを確認する。FontForgeはroot LICENSEで全体GPLv3以降と多くの部分のBSDライセンスが説明されており、選択ファイルと依存ライブラリ単位の確認が必要。単にFFIや別プロセスにするだけで配布条件が消えるとは扱わない。フォント作品のライセンスはアプリのライセンスと別にユーザーが指定する。

## 8. 保存・検証・書き出し

独自プロジェクトはversion付きmanifest、glyphごとのデータ、下絵などのassetを収めるcontainerとする。拡張子は未確定。安定IDで参照し、schema migrationを設ける。temporary fileへ書き、検証後にatomic renameする。autosave journalと復旧snapshotを用意し、手動保存と世代管理を分ける。再起動後の復旧とUndo履歴の再開は別要件とする。

出力はsnapshot作成→検証→輪郭の出力用変換→テーブル生成→再読み込み→shaping確認→atomic保存の順。`.notdef`、空白、cmap、name、head、hhea/hmtx、maxp、OS/2、post、glyf/locaなどの整合をバックエンドで担保する。nameの英語・日本語・简体中文値はUI翻訳とは別のfont metadataとして保持する。

開いた輪郭、非有限座標、循環component、範囲外のUnicode、致命的テーブル不整合は出力を止める。重なり、極小輪郭、欠字などは種類に応じて警告を示す。意図した重なりを編集documentから勝手に消さない。自動修正はbefore/afterを示し、取り消せる操作とする。

入力フォント・SVG・画像にはサイズ、点数、component深さ、処理時間の上限を設ける。既存コピーコードの検証だけで全入力の安全性が保証されるとは扱わない。

## 9. 多言語・画面設計

パネルはドッキング・タブ化・フローティングに対応させる。RustのWorkspaceStateにpanel ID、配置tree、window ID、復元用boundsを保持し、各ウインドウは正本を持たない。ドラッグ中の配置候補表示、閉じたパネルの再表示、画面外に出たfloating windowの復帰、レイアウト初期化を提供する。現在は左右・下部のドック、タブ化、フローティング化、close時のドック復帰、境界リサイズ、レイアウト初期化を実装済み。配置とサイズ・UI言語をRustで保持・保存する。任意に入れ子にしたsplit treeとフローティングウインドウのbounds復元は後続工程。再起動時はfloating panelをドックへ戻し、画面外のウインドウを発生させない。

GUIはコンパクトなアイコン主体のツールバーとし、常時表示する説明文を減らす。説明はinformationアイコンのhover tooltipへまとめ、keyboard focusでも読めるようにする。アイコンには三言語のaccessible nameと短いtooltipを付ける。密度を上げても操作領域、選択状態、パネル境界は明確にする。

UI言語はEnglish、日本語、简体中文。翻訳キーで管理し、backendはerror codeと引数を返す。翻訳された専門用語には短い説明を添える。制作対象の言語とUI言語は独立して選択する。

見出しは意味のまとまりで改行し、文字ごとの強制改行や固定高さに頼らない。左右16px以上の余白を基本とし、狭いウインドウでは詳細パネルをdrawerに移す。初期検証サイズは1024×768と1440×900、200%表示、長い翻訳文。キーボード操作、focusの可視化、色以外の状態表示、IMEを検証する。文字一覧の完了数は対象文字セットに対して算出し、空輪郭glyphや共有glyphを正しく数える。

## 10. 実装順序と受け入れ条件

- 🟢 [Done] 設計書、FontForge調査記録、コピー履歴を作成。
- 🟢 [Done] LumaPaint coreをローカルへコピーし、独立Cargo workspaceを構成。
- 🟢 [Done] FontDocument、文字単位のrevision付きtransaction、Undo/Redo、font座標から表示用パスへのadapter、version付きJSON保存・読込を実装。複数輪郭、二次曲線の形状、三次曲線の保存往復を検証。
- 🟠 [Next] 安定node ID、component、kerning、文書metadata編集、autosave・復旧とasset containerを追加する。表示用f32パスから正本への逆変換は未実装。
- 🟢 [Done] Tauri 2画面、Rust所有のAppState、wgpu/Metalのネイティブglyph描画、ノード移動・ペン入力・Undo/Redo、フローティングパネル間の状態同期を実装。
- 🟢 [Done] 左右・下部ドッキング、タブ化、パネル分離、close時の復帰、境界リサイズ、配置・UI言語の永続化を実装。
- 🟢 [Done] アプリアイコンとネイティブメニュー、プロジェクト新規・読込・上書き保存・別名保存を実装。
- ⭕️ [Pending] Windows/Linuxのネイティブsurface接続と実機検証、複数モニター間のDPI変更、署名・公証。
- 🟠 [Next] FontForge headless backendの独立ビルドとライセンス確認。小さなフォントの読込・生成・再読込を通し、採用を決定する。
- 🟠 [Next] 保存・復旧、四工程UI、三言語、静的TTF、実フォント試し組みを接続。英数字一式を作り、再起動後に修正し、書き出しフォントを外部アプリでも表示する。
- 🔴 [Later] 日本語・简体中文の段階的文字セット、部品glyph、大規模一覧、縦組み、class kerning。
- 🔴 [Later] OTF/WOFF2、OpenType詳細編集、variable font、hinting、画像トレース。
- 🔴 [Later] LumaPaintとのSVGファイル交換。相手のインストールを起動条件にしない。

最初の性能目標は基準機を決めたうえで通常編集のframe time 16.7ms以内、入力反応50ms以内、10,000 glyph一覧の仮想化とする。未測定の目標であり達成値ではない。重い輪郭演算・出力は進捗とキャンセルを提供する。

## 11. 参照元削除の条件

現在のRust workspaceはコピーしたcoreのみで成立する。FontForgeは実行時依存として導入していない。今回、参照元ディレクトリとsymlinkは削除しない。

最終削除前に、参照元を含まない一時ディレクトリへCargo.toml、Cargo.lock、cratesと必要な製品ファイルだけを複製し、build/testを行う。後続のrenderer/backend導入後も同じ確認を必須にする。依存manifest、build scripts、resourceパスに参照元の絶対パスや`../LumaPaint`などがないことを確認する。通知とソース由来の記録を残す。ネットワークからの通常のcrate取得は許容するが、開発者のLumaPaint checkoutや削除予定のFontForge cloneを要求しない。

## 12. 今回の検証結果

2026-10-02: `cargo test --workspace --offline` で既存168テスト成功。さらにCargo.toml、Cargo.lock、cratesのみを一時ディレクトリへコピーし、参照元を含まない構成で `cargo test --workspace --offline --locked` を実行して168テスト成功。コピー先全ファイルのSHA-256が記録と一致することを確認した。これはRust coreの独立性の検証であり、未実装の描画・FontForge連携・フォント出力の検証ではない。

### Unicode入力とマッピング（実装済み範囲）

文字の識別はUTF-16のコード単位ではなくUnicodeスカラー値で行う。Rustが追加・検証・重複排除・履歴を管理し、WebViewは文字列またはコードポイント範囲と期待revision/epochを送る。入力は正規化せず、結合文字と基底文字の対応を維持する。UIは全マッピングを数値配列で受け取り、`codePointAt(0)`だけを文字の識別として使わない。不可視文字はコード表示で区別する。

追加の上限は1回65534文字。追加前に全入力と容量を検証し、成功時に全件を1つのUndo単位にする。既存の文字は変更しない。Unicode空間の全てのスロットを空グリフとして生成する方式は採用せず、実際に制作する文字だけを文書に保持する。一覧のDOM生成は256件ずつとする。サロゲート値は文字として受理しない。

既存JSONスキーマ1を継続使用し、既存のグリフ名は変更しない。補助平面で新規生成する名前は`uXXXXX`形式。静的TTFのUnicode cmapは[OSフォントライブラリ](font-library.md)の段階で実装済み。異体字・合字・シェーピングは別段階で実装する。

### OSフォントの入出力と管理

Windows/macOS/Linuxの保存場所、Adobe候補、Rust側のスキャン・TTF/OTF/TTCインポート・静的TTF生成・ユーザー領域への登録・専用アーカイブ管理を実装した。[font-library.md](font-library.md)に対応形式、原本保護、プラットフォーム別の検証状態を記載する。管理用WebViewはRustのカタログの64件分を照会し、フォントのバイナリや輪郭のバルクデータは受け取らない。

### Type1 / CID / OpenType変換

Rustのfont_formatsモジュールにPFA/PFB、CID Type1、単体CFF/CID-CFFとOpenType CFF/CID-CFF生成を追加した。各形式の読込・直接変換・CMap適用・フォーマット固有の保持範囲は[font-formats.md](font-formats.md)を参照。CID／ROSと基本メタデータはRust Documentで保持し、ウインドウは形式選択などの表示状態のみ持つ。
