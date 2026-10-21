# ROADMAP

2026-10-19更新。現在の優先順位をここに集約する。implementation-status.mdの各工程は実装時点の履歴として残し、古い未実装表記より本書とfont-formats.mdを優先する。

## 現在まで

- 🟢 [Done] Rust所有の文書・編集・Undo/Redo、Tauri 2とmacOS wgpu、ドッキング／フローティング、三言語・標準メニュー。
- 🟢 [Done] Unicodeと多文字書体、静的TTF・OTF CFF/CID-CFF・Type1・CID・CFFの入出力、OS／Adobeフォント一覧。
- 🟢 [Done] フォントライブラリの日本語名検索・表示。内部の日本語名・別名で検索し、全角／半角カナ・英字・濁点の表記ゆれを正規化する。日本語UIでは日本語の書体名・スタイル名を優先し、なければ従来の英語名を表示する。[検索仕様](font-library.md)。
- 🟢 [Done] メイン画面からの形式選択付き出力、文字一覧の固定検索・ページ操作。
- 🟢 [Done] 新規空ドキュメント。ファイルメニューと⌘N／Ctrl+Nから作成し、未保存変更の確認後に空の文字一覧・輪郭で開始する。
- 🟢 [Done] ヘッダー最右端の環境設定。一般／ワークスペースのタブ、三言語の切替と保存、パネル配置リセット。
- 🟢 [Done] 二次・三次曲線のハンドル表示と直接編集。端点は四角、制御点は丸。接続線・制御点はRust/wgpuで描画。最も近い点を画面座標で選択し、ドラッグを1回のUndoに集約する。キャンセル、Undo/Redo、非表示ノードの選択拒否に対応。

曲線編集は取り込んだ書体を実際に修正するための優先項目として実装した。三次曲線の端点移動は接続するハンドルを平行移動し、明示的に重複した閉鎖端点も一緒に移動する。二次曲線の共有制御点は端点移動では動かさず、制御点を直接編集する。ハンドルは独立して動き、自動的な接線・対称制約は適用しない。

## 次の優先順位

1. 🟢 [Done] 自動保存・異常終了後の復旧。元のプロジェクトは上書きせず、候補を未保存の文書として開く。詳細は[自動保存・復旧](autosave-recovery.md)。
2. 🟢 [Done] 実フォントの試し組み。生成静的TTFによる文字列・サイズ・字間・行間・比較・編集へ戻る操作を実装。詳細は[proof.md](proof.md)。
3. 🟢 [Done] 既存グリフのUnicode割当・文書メタデータ編集、文字セットのプリセット。Undo/Redo・保存・出力へ接続。詳細は[font-editing.md](font-editing.md)。
4. 🟢 [Done] 輪郭パネルから直線・二次・三次曲線の中間点追加と終点削除。形状を保つ分割、閉鎖点・最小輪郭の保護、Undo/Redo。詳細は[輪郭編集](contour-topology.md)。
   - 🟢 [Done] 滑らか／角の切替、キャンバスの選択点と輪郭パネルの連動。三言語UI、ハンドル連動、JSON保存に対応。
   - 🔴 [Later] 安定node IDと永続UUID。現行の操作はepoch/revisionで守られた輪郭・点アドレスを使用する。
5. 🟢 [Done] 文字一覧に実際のグリフ輪郭サムネイルを表示。Unicode未割り当てのCIDグリフ・異体字も字形で確認できる。[実装と検証](glyph-thumbnails.md)。
   - Rust/wgpuで128pxの実輪郭を描画し、表示ページの画像をlazy load。アプリ所有のworkerと最大512件のLRUキャッシュを使用。小さなPNGをネイティブ画像プロトコルで配信し、輪郭・生フレームをJavaScriptのIPCへ転送しない。
   - Unicode／CIDラベルを保持し、空白・輪郭なし・描画失敗を区別。編集・Undo/Redo・文書切替で更新。macOSの独立検証アプリで300グリフのページ切替、CID検索、編集・Undo/Redo、ドッキング／フローティング表示を確認。大規模実書体・別OSの追加検証はPending。
6. 🟢 [Done] アタリ画像の配置。透明度、バウンディングボックスによる位置・サイズ・角度調整、配置画像の削除。詳細は[アタリ画像の実装と設計](reference-images.md)。
   - 🟢 [Done] 第1工程: Rust所有のPNG／JPEG資産・グリフ別配置、容量・寸法制限、埋め込み保存・復旧、設定変更・削除・重なり順のUndo/Redo。画像は履歴・スナップショットで共有し、不要な履歴から解放する。フォント出力・試し組みへの非混入も検証。
   - 🟢 [Done] ファイル選択・worker・Tauri/Model接続、wgpu背面描画、8ハンドル拡縮・回転・移動、透明度、数値編集と三言語のアイコンUI。連続操作は1回のUndoで戻せる。macOSで保存・再読込と分離パネルからの編集も確認。
   - ⭕️ [Pending] Windows/Linux実機、複数モニター・DPIと最大画像予算の負荷検証。
7. 🟢 [Done] 独立コピー型の部首・図形スニペット。複数輪郭の選択登録、改名・タグ・分類・出典・ライセンス・お気に入り、複製・削除、検索とwgpuサムネイルを実装。UPM換算、配置前の移動・拡縮・回転、確定／取消、三言語UI、Undo/Redo・埋め込み保存・復旧に接続。[仕様と検証](components-snippets.md)。
   - 🟢 [Done] 独自作成の長方形・楕円・三角形・ひし形・「口」「工」の幾何形。プロジェクト内ライブラリとして保持し、元グリフや雛形の削除後も配置済み輪郭を保持する。
   - ⭕️ [Pending] Windows/Linux実機、複数モニター・DPIと大規模ライブラリの実機負荷検証。
   - 🔴 [Later] 安定IDに基づく部品の参照配置・共有更新・利用先一覧・参照解除。既存のcomponent計画をここへ統合する。
8. 🟢 [Done] Illustrator・LumaPaintからパスを直接ペースト。macOSの専用JSON／SVG／PDF／制限付きAICBをRustで解析し、文字枠へ独立コピー。三言語、1回のUndo/Redo、保存・復旧へ接続。両アプリの実コピーと入力欄の文字貼り付けを確認。[仕様と検証](clipboard-paths.md)。
   - ⭕️ [Pending] Windows/Linuxのクリップボード実装・実機検証。
9. 🟢 [Done] pair kerningの編集・試し組み・出力。グリフIDの明示ペア、三言語メトリクスUI、Undo/Redo・保存・復旧、試し組みのON/OFF、TTF／OTF GPOSへ接続。[仕様と検証](pair-kerning.md)。
   - 🟢 [Done] class kerningの編集・試し組み・出力。左右クラスの登録・改名・所属編集・削除、明示ペア／0例外の優先、GPOS Format 2、三言語・Undo/Redo・保存・復旧。[仕様と検証](class-kerning.md)。
   - 🔴 [Later] 縦組み・異体字シーケンスと、外部フォントの組版情報の保持。
   - ⭕️ [Pending] Windows/Linuxの実画面と、外部組版アプリごとの表示検証。
10. 🟢 [Done] フローティングパネルの位置・サイズをRustのワークスペースへ保存し、再起動時に復元。モニター切断・配置・解像度・DPI変更時は作業領域内へ補正し、必要に応じて縮小する。[仕様と検証](floating-panels.md)。
11. 🟢 [Done] パネルの入れ子split。上下・左右分割、タブ結合、境界リサイズ、三言語、旧設定からの移行、レイアウト保存・復元、フローティングとの相互切り替え。[仕様と検証](nested-splits.md)。
12. 🟢 [Done] 複数文書のタブ切り替えと文書単位の保存・復旧。三言語、文書別Undo/Redo・表示状態、未保存確認、非表示文書の自動保存と個別復旧、重複保存先の保護。[仕様と検証](multiple-documents.md)。
13. 🟢 [Done] Webフォント用WOFF2出力。TrueType／CFF・圧縮設定、三言語、保存ダイアログ、保存後の読み戻し検証、ブラウザーでの読み込みを接続。[仕様と検証](woff2.md)。
14. 🟢 [Done] 可変フォントの軸設定・wgpuプレビュー・静的インスタンスへの変換。三言語、独立文書、TTF／OTF／WOFF2、保存ダイアログと保存後検証。[仕様と検証](variable-fonts.md)。
   - 🟢 [Done] 名前付きインスタンスの選択。全軸値の一括適用、三言語名、手動変更時のカスタム切替、既定への復帰、プレビュー・静的3形式・独立文書のスタイル名保持。
15. 🟢 [Done] カラーフォントの読み込み・形式表示、COLRv0／sbix／CBDTのwgpuプレビュー、COLRv1／SVGを含むカラー保持出力。三言語、保存ダイアログ、全テーブルと保存後検証。[仕様と対応範囲](color-fonts.md)。
   - 🔴 [Later] COLRv1／SVGプレビュー、カラー編集・プロジェクト保存、カラーWOFF2。
16. 🟢 [Done] hintingの取り扱い。TrueTypeの検査・ヒント保持／除去、コレクションからの単体出力、三言語UIと保存後検証。編集・変換では古い命令を再利用せずヒントなしで出力する。[仕様と検証](hinting.md)。
   - 🟢 [Done] CFF1／CID-CFF／CFF2と単体CFFのヒント除去、ttfautohintによるTrueType命令生成、TTF／WOFF2への接続。[エンジンと検証](autohint-engines.md)。
   - 🟢 [Done] Adobe psautohintによる静的CFF1／CID-CFFの自動ヒント生成。OTF／単体CFF／CFF WOFF2、三言語UI、輪郭・文字対応の保持検証。
   - 🟢 [Done] 小サイズ比較。元のヒント／ヒントなし／自動生成を9・12・16・24ppemで並べ、サンプル入力、1×／2×、未収録文字数、三言語UIを実装。[仕様](hinting-comparison.md)。
   - 🔴 [Later] Type 1の直接dehint、CFF2用ヒント生成、命令編集。
   - ⭕️ [Pending] Windows/Linux実機、外部アプリでの小サイズ表示。
17. 🔴 [Later] 画像トレース、WOFF2圧縮率の最適化、可変フォント自体の編集／出力・軸状態のプロジェクト保存。

- ⭕️ [Pending] Windows/Linuxのネイティブ描画と実機検証、複数モニター・DPI、署名・公証。

FontForgeは仕様・挙動の調査元として使用し、現行の変換エンジンはプロジェクト内のRust実装と同梱FreeTypeで動作する。FontForge backendの導入を、実装済みの入出力を使うための必須工程にはしない。

## 検証

曲線編集の自動テストは、二次・三次制御点、端点と接続ハンドル、閉鎖接合、無効座標での状態保持、複数move→1回のUndo、キャンセルとRedoを対象とする。対応形式・大きな書体の制約は[font-formats.md](font-formats.md)を参照。

検証結果: 通常Rustテスト207件、Clippy `-D warnings`、TypeScript/Viteビルド成功。独立したmacOS検証アプリで三次ハンドルの描画・ドラッグ・1回のUndo・Redo・JSON保存を確認し、保存ファイルで対象control1だけが変更されたことを確認した。

## フォントライブラリの一括変換

- 🟢 [Done] 複数選択・ページ横断の選択保持・フォルダーへの一括変換・重複名の回避・進捗と中止・個別結果。詳しくは[batch-font-conversion.md](batch-font-conversion.md)。
- ⭕️ [Pending] Windows/Linux実機の保存ダイアログと出力検証。

## 輪郭の点の追加・削除の検証

通常Rustテスト223件、Clippy `-D warnings` 成功。macOSの独立検証アプリで点の追加・削除・Undo/Redoと最小輪郭の保護を確認。操作と制約は[contour-topology.md](contour-topology.md)を参照。

## 滑らか／角と共有選択の検証

通常Rustテスト226件、Clippy `-D warnings`、TypeScript/Vite、通常版macOSアプリのビルドが成功。独立検証アプリで双方向の選択連動、ハンドル連動ドラッグ、滑らか／角切替を確認し、保存JSONでも接線整列と角切替時の座標保持を検証した。


## 外部ライブラリの保守

- 🟢 [Done] 最新stableへの依存更新と互換性調整。Tauri 2.12.4 / React 19.3.1 / wgpu 30.0.1ほかを適用。毎週月曜09:00（日本時間）の自動確認・更新を設定。[更新・検証記録](dependency-update-2026-10-15.md)。
- ⭕️ [Pending] 更新後のWindows/Linux実機検証、OS由来のsurface喪失・再構成イベント、GUI目視確認。


## 実輪郭サムネイルの検証（2026-10-17）

通常Rustテスト232件、実GPUテスト3件、Clippy `-D warnings`、TypeScript/Vite成功。独立したmacOSアプリでUnicodeなしCID字形、空白、300グリフのページ操作・検索、編集とUndo/Redoによる画像更新、ドッキング／フローティングを確認。[詳細と制約](glyph-thumbnails.md)。この時点での次工程はアタリ画像の配置（下記で実装済み）。


## アタリ画像の保存・復旧基盤（2026-10-17）

第1工程を実装。新規18件のテストで画像読込・上限・履歴・保存・復旧・出力への非混入を検証。第2工程の読み込みworker、Tauri/Model接続、wgpu背面描画・変形操作・UIも下記の通り実装済み。[詳細](reference-images.md)。

Rust全体テスト253件と追加の容量事前検証1件、Clippy（全target、`-D warnings`）、整形検査、TypeScript/Viteビルド成功。第1工程時点ではGUI/GPUは未実装だったが、下記の第2工程で接続と実機検証を完了した。

## アタリ画像の描画・変形・三言語UI（2026-10-17）

🟢 [Done] Rust/GPU所有の画像描画、移動・拡縮・回転・透明度、表示・ロック・削除・重なり順をUIへ接続。保存・再読込、ドッキング／フローティング、ジェスチャー単位のUndo、三言語をmacOS実画面で確認。通常Rustテスト261件、実GPUテスト4件、Clippy、整形検査、TypeScript/Vite・macOSビルド成功。[詳細](reference-images.md)。

🟢 [Done] 部首・図形スニペットの独立コピー配置を下記の通り実装。

## 部首・図形スニペットの独立コピー（2026-10-17）

🟢 [Done] Rust所有のプロジェクト内ライブラリ、複数輪郭の登録、管理、独立コピー配置を実装。三言語のドッキング／フローティングパネルとGPUサムネイル、変形プレビューと確定、Undo/Redo、保存・再読込・復旧を接続。元の雛形を削除しても配置済み輪郭を維持する。[詳細と検証](components-snippets.md)。

🟢 [Done] pair kerningの編集・試し組み・出力。class kerningも🟢 [Done]。フローティングパネルの位置・サイズの復元も🟢 [Done]。パネルの入れ子splitも🟢 [Done]。複数文書も🟢 [Done]。WOFF2出力も🟢 [Done]。可変フォントも🟢 [Done]（上記参照）。共有更新を伴う部品参照は🔴 [Later]として、独立コピーと区別する。

検証: 通常Rustテスト272件、実GPUテスト4件、Clippy（全target、`-D warnings`）、整形検査、TypeScript/Vite・通常版macOSアプリのビルドが成功。復旧データの復元は自動テスト、三言語の登録・管理・配置・保存と再読込はmacOSの独立アプリで確認。

- 🟢 [Done] [三言語ソフトウェアアップデート・ローカル署名とGitHub Releases配布](software-updates.md)。

- 🟢 [Done] GitHub Actions Secretによる更新署名、タグからのビルド・独立署名検証・GitHub Releases公開の自動化。[運用手順](software-updates.md)。

- 🟢 [Done] Apple Developer ID署名・app/DMG公証・staple・Gatekeeper検証をReleaseワークフローへ接続。⭕️ [Pending] Apple証明書・公証API認証情報の登録と実公開検証。[設定手順](software-updates.md#apple-developer-id署名公証の自動化2026-10-21)。
