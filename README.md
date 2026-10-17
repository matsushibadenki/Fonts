# Fonts

<img src="docs/images/Fonts-image.png">

Tauri 2 + Rustで作るフォント制作アプリ。UIはWebView、文書・編集状態はRust、輪郭描画はRustのwgpu/Metalが担当します。

現在はmacOSで起動・編集できます。静的TTF出力とFontForge backendは次工程です。

## 起動

```sh
npm ci
npm run tauri dev
```

この作業環境では依存パッケージをローカルへコピー済みなので、そのまま `npm run tauri dev` で起動できます。LumaPaint checkoutやFontForge cloneは不要です。Node.js 22.13以降、Rust toolchain、macOSのXcode Command Line Toolsを使用します。

## macOSアプリのビルド

```sh
npm run tauri build -- --debug --bundles app
```

生成先: `target/debug/bundle/macos/Fonts.app`。署名・公証した配布ビルドは未対応です。

## 操作

- パネル見出しをドラッグして左右・下部へドッキング。同じドックではタブにまとまります。
- 見出し右の分離アイコンでフローティング化。分離ウインドウのドックボタン、または閉じる操作で戻せます。
- パネル境界をドラッグして幅・高さを調整。境界へキーボードfocusを移して矢印キーでも調整できます。幅1,000px以下では左右幅を自動調整します。
- 文字を選択して四角いノードをドラッグ。ペンはクリックで点を追加し、Enterかダブルクリックで閉じます。Escで編集中の操作を取り消します。
- `⌘Z` / `⌘⇧Z`でUndo/Redo。`⌘S`でプロジェクトJSONを保存します。

「ファイル」メニューから新規作成・開く・保存・別名保存を利用できます。「編集」はUndo/Redoと入力欄の切り取り・コピー・貼り付け、「表示」は倍率・ノード・塗り、「ウインドウ」はパネル操作に対応します。macOSでは画面上端の標準メニューバーに表示します。

自動復旧、曲線handleの編集、フォントファイルのimport/exportは今後実装します。起動時は編集可能なサンプルAと空の文字を用意します。

## 検証

```sh
cargo test --workspace --offline
cargo clippy --workspace --offline --all-targets --no-deps -- -D warnings
npm run build
# 実GPUにアクセスできるデスクトップ環境で明示的に実行
cargo test -p font-renderer --test gpu --offline -- --ignored
```

[設計書](docs/font-app-design.md) · [実装状況](docs/implementation-status.md)

### OSフォントライブラリ

ファイルメニューの「フォントライブラリ…」（Cmd/Ctrl+Shift+F）から、OS標準・ユーザー・Adobeのフォントを検索できます。TTF/OTF/TTCを別プロジェクトとして読み込み、静的TTFとして出力します。ユーザー領域へのインストールと、このアプリが追加したフォントの有効／無効切替も実装しています。macOSの検出・入出力・CoreText登録解除を確認済み。Windows/Linuxの実機検証とmacOS標準ユーザーフォルダーへのGUIインストール確認は保留です。[保存場所・対応範囲](docs/font-library.md)を参照してください。

### フォント形式の変換

ライブラリ下部で出力形式を選び、行を選択して変換アイコンを押すと、元ファイルを変更せずに別形式へ変換できます。TrueType、OpenType CFF/CID-CFF、Type1 PFA/PFB、CID Type1、単体CFF/CID-CFFに対応。CMapの適用・書き出しも可能です。ヒンティングや高度な組版情報を保持する変換ではありません。[形式ごとの対応範囲](docs/font-formats.md)を参照してください。

メイン画面上部の保存アイコンの隣にある出力アイコンからも、制作中のフォントを書き出せます。ファミリー名と出力形式を選択して保存します。フォントライブラリと同じRustの出力・保存処理を使用します。

確定済みの編集は別ファイルへ自動保存されます。異常終了後は次回起動時の復旧候補から新しいプロジェクトとして開けます。メイン画面の復旧アイコンからも状態・候補を確認できます。[仕様と制約](docs/autosave-recovery.md)。

「試す」で制作中の輪郭から生成した静的TTFを試し組みできます。文字列・文字サイズ・字間・行間を調整し、見本を固定して変更前と比較できます。現在側の文字をクリックすると編集へ戻ります。[対応範囲と制約](docs/proof.md)。

既存グリフのUnicode対応は右側メトリクスの「Unicode割り当て」、フォント名・スタイル・ライセンス等は上部の情報アイコンから編集できます。文字一覧の「＋」では文字セットをまとめて追加できます。[操作と対応範囲](docs/font-editing.md)。

フォントライブラリではチェックボックスや⌘/Ctrlクリックで複数選択できます。出力形式を選び、一括変換のフォルダーアイコンから保存先を指定すると、新しいサブフォルダーへまとめて保存します。[操作と制約](docs/batch-font-conversion.md)。
