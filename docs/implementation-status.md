# 実装状況 — 2026-10-15

現在の優先順位と完了状態は[ROADMAP](ROADMAP.md)を参照。以下の工程記録には、後続実装で解消済みの制約を含む。

## 第1工程: Rustの文書・編集・保存

- 🟢 [Done] `font-core`: Y上向きf64のline/quadratic/cubic輪郭、Unicode対応、UPM、縦metrics、advance、制作状態。
- 🟢 [Done] `.notdef`を固定ID 0で作成。重複文字入力を除去し、空白を輪郭なしで確認済みにできる。
- 🟢 [Done] revision付き文字単位の編集、Undo/Redo。無効な編集は状態・履歴を変更しない。履歴は変更glyphのみを128操作まで保持。
- 🟢 [Done] ローカルのLumaPaint coreで表示用パスを生成。二次→三次変換とY反転を表示境界に限定。正本の精度を保持。
- 🟢 [Done] schema version付きJSONの保存・読込。入力512 MiB制限、同一directoryへの一時保存・sync・rename、破損／未来versionの拒否。
- 🟢 [Done] Tauri 2 shell、Rust AppState、macOSのwgpu native surface、文字編集画面。
- 🟢 [Done] Rust所有のWorkspaceとドッキング／フローティングパネル。三言語のアイコン・tooltip中心GUI。
- 🟠 [Next] 安定node ID、component・kerning、autosave・復旧、FontForge backendの切り出しとTTF出力。
- 🟢 [Done] 静的OTF／WOFF2出力。可変フォントと画像トレースは🔴 [Later]。
- ⭕️ [Pending] Windows/Linux実機でのnative描画、IME、複数モニター・DPI、フローティングウインドウの挙動。該当環境でGUIが動いた後に検証する。

## 実行

```sh
cargo test --workspace --offline
cargo clippy -p font-core --offline --all-targets --no-deps -- -D warnings
cargo run -p font-core --offline --example project -- /tmp/fonts-first-project.fontproj.json
```

exampleは作成→輪郭編集→Undo→Redo→保存→再読込を通すRustの実行サンプル。出力はプロジェクトJSONであり、インストール可能なフォントファイルではない。

## 現在の制約

macOSのGUI・rendererは起動可能。FontDocumentにはcomponent、pair kerning、stable node ID、永続UUIDが未導入。履歴はメモリ内のみ。ファイル形式はassetのない試作JSONで、schema migrationは未導入のため未対応versionを拒否する。保存中は呼出元がdocumentを所有するかlockを保持すること。実アプリでは保存snapshotと保存revisionを管理する。親directoryのfsyncと停電時の耐久性、Windowsでの既存ファイル置換は未検証。開いた輪郭は編集中の状態として許可し、出力時検証は後工程で導入する。

## 検証結果

追加5件＋既存168件の計173テスト成功。font-coreの全targetに対するClippyは警告なし。実行exampleで保存・再読込成功。Cargo manifest／lock／cratesのみを一時directoryにコピーし、参照元なし・offline・lockedでもfont-coreの5テスト成功。LumaPaintからコピーした既存ソースのhashは変更前の記録と一致し、元プロジェクトは変更していない。

## 第2工程: Tauri・wgpu・パネル

- 🟢 [Done] Rustに文書・選択文字・tool・zoom/pan・編集draftを集約。各WebViewは要約snapshotを表示し、小さなcommandのみ送信。
- 🟢 [Done] Rust所有のretained NSViewとMetal surface。4x MSAA、輪郭・穴のstencil描画、guide・anchor、リサイズ時のsurface再設定。描画は変更時のみで、常時animation loopは設けない。
- 🟢 [Done] ノード移動を1回のUndoへ集約。ペンによる閉じたpolygon入力、塗りプレビュー、zoom/pan、送り幅変更、確認済みマーク。
- 🟢 [Done] パネルの左右・下部ドッキング、タブ化、別Tauriウインドウ化、close時のドック復帰、初期配置への復帰。revisionで古い配置操作を拒否。
- 🟢 [Done] パネル境界のpointer／keyboardリサイズ。配置・サイズ・UI言語をapp config directoryのworkspace.jsonへ保存。フローティングパネルの位置・サイズ・分離状態も復元し、モニター構成が変わった場合は作業領域内へ補正する。[仕様と検証](floating-panels.md)。
- 🟢 [Done] 英語・日本語・简体中文のUIとnative menu。説明はinformation iconのtooltipへ移し、keyboard focusでも読める構成。
- 🟢 [Done] ネイティブ保存dialog、JSON保存、未保存マーク。通常のcloseと終了メニューは親ウインドウ付きの確認dialogを表示。
- 🟢 [Done] 曲線handleの編集、autosave・復旧、RustエンジンによるTTF/OTF出力、ドック内の入れ子split（後続工程の詳細を参照）。安定node IDとFontForge backendは🔴 [Later]。
- 🟢 [Done] floating windowの位置・サイズ復元。複数文書の切り替え・個別保存・復旧も🟢 [Done]、複数canvas windowと可変フォントは🔴 [Later]。
- ⭕️ [Pending] Windows/Linux surfaceの実装と実機検証、外部モニター間のDPI切替、署名・公証。現行のネイティブ描画接続はmacOS限定。

### 実装上の境界

- UI: src/。Rust host: src-tauri/。文書・panel配置: crates/font-core/。GPU: crates/font-renderer/。
- Rendererはglyphのpreviewをeven-oddで塗る。フォントの出力形式ごとのnon-zero winding検証は今後backendへ実装する。
- GPUのMSAA/depth attachmentはサイズが変わるまで保持する。frameや画像bufferをJSへ渡さない。曲線の表示近似にはコピー済みLumaPaint coreを利用する。正本f64輪郭は変更しない。
- 表示用flatteningにはglyph全体100,000点の上限を設け、超過は明示的なエラーとする。巨大フォントの仮想化や性能目標の達成はまだ検証していない。
- パネルは3ドック内でタブ群を入れ子に分割できる。幅1,000px以下では左右ドックを自動幅にし、狭い分割内ではスクロールで操作を維持する。
- 保存形式は現在assetを持たないJSON。font import/export、shaping、自動復旧は未実装。OSによる強制終了への復旧保証はない。

### 検証結果

macOS Apple M4で1280×820と独立した800×600検証アプリを起動。GPU表示、輪郭の穴、文字選択、ノードdrag→Undo、3点のpen入力→Enterで閉鎖、EN/日本語/简体中文切替を確認。パネルdrag→右ドックのタブ化、float→closeで復帰、float内の送り幅変更→main同期→Undo、配置reset、下部境界drag、JSON保存、終了dialogのキャンセルと通常終了を確認した。

通常のRustテスト180件成功。実GPUへのアクセスを許可した明示的なreadback test 1件も成功。サンドボックス内ではMetal adapterが列挙できないため、このGPU testは通常のcargo testではignoreし、明示実行する。Clippyの `-D warnings` とfrontendのtypecheck／production buildを実施。依存block 0.1.6にCargoの将来互換性通知があるが、今回のビルドは成功している。

LumaPaintの元ディレクトリは変更せず、追加流用箇所のhashと改変点をreuse-native.jsonに記録した。元coreコピーのhashも初回記録と一致。Node依存パッケージもローカルにコピーし、外部checkoutへのsymlinkは使わない。

参照元を含まない一時directoryにCargo／npm manifest、crates、src-tauri、src、dist、ローカルnode_modulesのみをコピーし、frontend production buildとRust workspaceのoffline／locked checkが成功。最終通常版を `target/debug/bundle/macos/Fonts.app` へビルドした。

## 第3工程: アプリアイコン・標準メニュー

- 🟢 [Done] ベクター原稿からアプリアイコンを生成し、PNG／ICNS／ICOを設定。macOS bundleのCFBundleIconFileとResources内のicon.icnsを確認。ヘッダーにも同じアイコンを表示。
- 🟢 [Done] Fonts・ファイル・編集・表示・ウインドウ・ヘルプのネイティブメニュー。英語・日本語・简体中文へ切替。
- 🟢 [Done] 新規プロジェクト、プロジェクトJSONの読込、上書き保存、別名保存、未保存確認。
- 🟢 [Done] 文字編集と入力欄それぞれのUndo/Redo。切り取り・コピー・貼り付け・全選択は入力欄の標準操作として提供。
- 🟢 [Done] 表示倍率、全体表示、ノード・塗りのチェック項目、最小化・フルスクリーン、パネル選択・配置リセット、クイックガイド。
- 🟢 [Done] 新規／読込による文書置換時にruntime epochを更新。古い編集commandや保存完了が別文書へ適用されることを防止。ファイル操作を直列化し、失敗した読込で現文書を変更しない。
- ⭕️ [Pending] Windows/Linux実機でのメニュー・ICO・キーボード動作の確認。

検証: 通常Rustテスト183件成功。新規文書での履歴・draftのリセット、無効な読込の状態保持、古い保存完了の拒否を追加検証。macOSアプリで保存→新規→読込、送り幅700→メニューUndoで640→Redoで700→上書き保存、検索入力のUndoとフォント履歴の独立、表示倍率100→120%、別名保存dialogを確認。保存JSONのadvance値も読み出して確認した。

メニュー操作はフォーカス中のウインドウだけへ通知し、フローティングパネルが開いていてもUndoが複数回実行されないようにした。Undo/Redoの無効表示、ノード表示切替、EN/日本語/简体中文のメニュー切替も実機で確認。

## Unicode文字管理

- 🟢 [Done] 全Unicodeスカラー値をRustの`char`として保持。補助平面（例 U+1F600、U+20000）、結合文字、私用領域を含め、文字列・コードポイント・範囲から追加できる。
- 🟢 [Done] 文字一覧の「＋」から「文字列／コードポイント」を選択。`U+3041-U+3096, U+1F600`、`0x1F600`、接頭辞なしの16進数に対応。文字列モードは正規化せず、各スカラー値を個別グリフとして扱う。
- 🟢 [Done] 重複した入力を除外し、既存グリフの輪郭・メトリクスを保持。追加は1トランザクションでUndo/Redo可能。選択グリフがUndoで消えた場合は存在するグリフに戻す。
- 🟢 [Done] サロゲート・範囲外・逆順範囲・不正書式を拒否。入力は1 MiB、1回65534文字、文書全体65535グリフ（.notdefを含む）まで。失敗時は文書を変更しない。
- 🟢 [Done] 文字・U+表記・グリフ名で検索。各セルに16進コードを表示し、不可視文字は代替記号、結合文字は点線円と表示。全マッピングはツールチップで確認。256件ずつ描画。
- 🟢 [Done] JSON保存・再読込で全スカラー値を維持。BMPの自動名は`uniXXXX`、補助平面は`uXXXXX`形式。既存の名前と衝突した場合は接尾辞を追加。
- 🟠 [Next] 既存グリフへの複数コードポイントの割当編集、Unicodeブロックのプリセット。
- 🔴 [Later] 異体字シーケンス、ZWJ絵文字、合字のシーケンスマッピングとシェーピング。現状のモデルは単一スカラー値からグリフへの対応であり、これらのシーケンスを1グリフにはまとめない。静的TTFのcmap出力は次項のOSフォントライブラリ段階で実装済み。CFF OTF出力は未実装。
- ⭕️ [Pending] Windows/Linuxの実画面とIME入力の実機検証。

参考: FontForge `fontforge/encoding.c` のUnicodeBmp/UnicodeFullと`fontforge/namelist.c` の平面別管理を確認。実装はこのプロジェクト内で完結し、参照ディレクトリ・シンボリックリンクへの依存は追加していない。

検証: Rust 188テスト、Clippy（警告をエラー扱い）、TypeScript/Viteビルド、macOSアプリ生成が成功。別の検証用アプリ（1280×820）で文字列追加、U+検索、選択、範囲追加、結合文字、Undo/Redo、サロゲート拒否を確認。文字一覧の字形プレビューはOSのインストール済みフォントに依存するため、未収録文字は代替字形になるが、コードポイントと制作する輪郭は保持される。ブラウザーのコンソール、他OS、実IMEの変換操作は未検証。

## OSフォントライブラリ・静的TTF入出力

以前の「TTF/OTF出力未実装」はこの段階で静的TTFのみ更新した。詳細な保存場所と制約は[font-library.md](font-library.md)を参照。

- 🟢 [Done] Windows/macOS/Linuxの保存場所解決、Adobe候補・隠しファイルの検出、検索・フェイス一覧・ファイル選択。
- 🟢 [Done] TTF/OTF/TTC輪郭の編集用インポート、元ファイルの複製、Unicode対応の静的TTF生成。
- 🟢 [Done] ユーザー領域への登録処理、専用アーカイブと管理情報、このアプリが追加したフォントの有効／無効切替、外部変更の保護。
- 🟢 [Done] OS／Adobe管理ツールへの導線、英語・日本語・简体中文、別ウインドウの管理UI。
- 🟠 [Next] 大規模日本語フォントを保存するプロジェクト形式の容量改善、メタデータ保持。
- 🔴 [Later] 可変・カラー・GSUB/GPOS・CFF OTF/WOFF2出力を含む完全なラウンドトリップ。
- ⭕️ [Pending] Windows/Linuxのビルドと実機検証、再ログイン後の永続性、macOSの標準ユーザーフォルダーへのGUIインストール。CoreTextユーザースコープでの一時フォントの登録・解除は実行確認済み。

## Type1 / CID / CFFを含む形式変換

[font-formats.md](font-formats.md)を現在の対応範囲として参照。前節の「CFF OTF出力は未実装」を更新した。

- 🟢 [Done] TTF、OTF CFF/CID-CFF、PFA/PFB、CID Type1、単体CFF/CID-CFFの読込・静的出力・相互変換。
- 🟢 [Done] 実形式の表示、出力形式選択、選択フォントの直接変換、元ファイル複製との区別。
- 🟢 [Done] Unicode／CID CMapの入出力、CIDとROSの永続化、1回のUndoで戻せる対応表の適用。
- 🟢 [Done] 太さ・斜体・基本ライセンス情報等のOpenTypeメタデータ、PS FontInfoのNoticeの保持。
- 🟢 [Done] FreeType 2.14.3の同梱ビルドとライセンス告知。FontForge等の外部変換コマンドへの実行依存なし。
- 🟠 [Next] 大規模文書のJSON保存容量、CMap継承と標準文字コレクションの対応。
- 🔴 [Later] 高度な組版・ヒンティング・可変・カラーの保持、AFM/PFM、OTC生成、WOFF2入力。
- ⭕️ [Pending] Windows/Linux、PostScriptプリンター等での実機検証。

## 多文字書体

- 🟢 [Done] CJK書体のBMP文字マップ圧縮、512 MiBプロジェクト、1600万輪郭セグメント、65534文字の一括追加、ページ直接移動。実書体の検証結果と制約は[font-formats.md](font-formats.md#日本語中国語などの大きな書体)。
- 🟠 [Next] 異体字シーケンスと縦組みテーブルの保持。
- ⭕️ [Pending] Windows/Linux実機の大規模書体検証。

## 自動保存・復旧

- 🟢 [Done] Rust workerの自動チェックポイント、セッションごとのOSロック、次回起動の候補提示、未保存文書への復旧、明示破棄・正常終了の整理。詳細は[autosave-recovery.md](autosave-recovery.md)。
- ⭕️ [Pending] Windows/Linux実機でのロックと異常終了からの復旧。

## 実フォントの試し組み

- 🟢 [Done] メモリ内で生成・再読込した静的TTFによる試し組み、サンプル・サイズ・字間・行間、折り返し、比較見本、未収録表示、文字クリックによる編集への復帰。詳細は[proof.md](proof.md)。
- 🔴 [Later] 高度なシェーピング、縦組み、禁則、OSラスタライザーとの一致。
- ⭕️ [Pending] Windows/Linux実機。

## 文字割り当て・フォント情報・プリセット

- 🟢 [Done] 既存グリフへの複数Unicode対応と解除、基本フォント情報の編集、8種類のUnicode範囲プリセット。全操作に検証・Undo/Redo・保存を接続。詳しくは[font-editing.md](font-editing.md)。
- 🔴 [Later] 規格別文字セット、全OpenTypeメタデータ、UPMのスケーリング変更。
- ⭕️ [Pending] Windows/Linux実画面。

## フォントライブラリの一括変換

- 🟢 [Done] 最大256フェイスの複数選択、一括形式変換、非上書き保存、進捗・中止と結果一覧。自動保存とは独立したworkerで実行。仕様と検証は[batch-font-conversion.md](batch-font-conversion.md)。
- ⭕️ [Pending] Windows/Linux実機。

## 輪郭の点の追加・削除（2026-10-15）

🟢 [Done] 輪郭パネルの区間選択から中間点追加・終点削除。Rustのde Casteljau分割、閉鎖点と最小輪郭の保護、競合検査、Undo/Redoに対応。詳細は[contour-topology.md](contour-topology.md)。

🟢 [Done] 滑らか／角の切替とキャンバス選択との連動（下記参照）。⭕️ [Pending] Windows/Linux実画面。

## 滑らか／角・共有選択（2026-10-15）

🟢 [Done] Rust所有の選択点をキャンバスと輪郭パネルで共有。wgpuの選択枠、始点の選択、ハンドルから端点への対応、接線方向の連動、角への制約解除を実装。旧JSON読み込みと新しい制約の保存に対応。仕様は[contour-topology.md](contour-topology.md)。

## 環境設定（2026-10-15）

🟢 [Done] アプリ上部右端に歯車アイコンを追加。タブ式の環境設定に一般（英語・日本語・简体中文の表示言語）とワークスペース（パネル配置リセット）を実装し、従来のヘッダー上の言語選択と配置リセットを移動した。既存のRust所有Workspaceへ接続し、言語は即時反映・起動時復元される。ダイアログ表示中はネイティブキャンバスを隠して重なりを防ぐ。タブの矢印・Home/Endキー、Esc、フォーカス制御と三言語ラベルに対応。

検証: TypeScript/Viteビルド成功。macOS独立検証アプリ（1280×820）で右端のアイコン、タブ切替、配置リセット、英語・日本語・简体中文の即時反映、Escでの終了、再起動後の言語保持を確認。環境設定表示中にネイティブ描画が重ならないことも確認した。Windows/Linuxの実画面は⭕️ [Pending]。

## 新規空ドキュメント（2026-10-15）

🟢 [Done] フォントライブラリの左の新規空ドキュメントアイコン、またはファイル→新規空ドキュメント（⌘N／Ctrl+N）で、登録文字・輪郭なしの文書を作成する。内部にはフォント構造上必須の空の.notdefだけを保持する。従来の未保存変更確認と復旧候補の切替を利用し、キャンセル時は現在の文書を保持する。文字追加・プリセットから制作を開始でき、空文書へのUndoとRedoに対応する。起動時の既存サンプル表示は変更しない。英語・日本語・简体中文のメニューに対応。

- 🟢 [Done] パネルの入れ子split・タブ結合・分割境界リサイズ。三言語、旧設定移行、保存・再起動復元、フローティングの元位置復帰と別領域への配置。[仕様と検証](nested-splits.md)。

- 🟢 [Done] 複数文書のタブ、独立したUndo/Redo・表示状態・保存先、三言語の未保存確認、全未保存文書の個別チェックポイントと復旧。[仕様と検証](multiple-documents.md)。

- 🟢 [Done] Webフォント用WOFF2出力。TrueType／CFF・圧縮・三言語・保存後検証、独立デコーダーとブラウザーでの確認。[仕様と検証](woff2.md)。

- 🟢 [Done] [可変フォントの軸設定・GPUプレビュー・静的インスタンス出力](variable-fonts.md)。

- 🟢 [Done] [カラーフォントの読み込み・GPUプレビュー・カラー保持出力](color-fonts.md)。

- 🟢 [Done] [三言語ソフトウェアアップデート・ローカル署名とGitHub Releases配布](software-updates.md)。
