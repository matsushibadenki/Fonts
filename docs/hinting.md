# Hintingの取り扱い

## 操作

フォントライブラリで書体を1つ選び、ヒンティングのアイコンを開く。TrueTypeでは命令付きグリフ数、グリフ・fpgm・prepの命令バイト数、関連テーブルを検査して表示する。ヒントなしの書体も扱える。CFF／CFF2は保持・除去、静的CFF1は自動生成にも対応し、ヒントの有無や数を推測して表示しない。

- **元のヒントを保持**：選択したコレクションのfaceを単体TTF／OTFへ抽出する。輪郭、ヒント命令、組版、カラー、可変情報、未知のテーブルも保持する。無効になるDSIGは除去し、ディレクトリ・チェックサムを再構築する。ファイル全体をそのまま残す場合は既存の「元ファイルを出力」を使う。
- **ヒントを除去**：通常・空・合成グリフの命令とfpgm／prep／cvt／cvar、ヒンティング依存のhdmx／LTSH／VDMX／gaspを除去する。合成グリフの命令フラグを解除し、glyf／loca、head／maxp、チェックサムを再構築する。点・合成構造・グリフ順・メトリクスは保持する。
- **編集／変換／静的インスタンス**：既定では新しいヒントなし輪郭を出力する。メインのTTF／OTF／単体CFF／WOFF2出力では自動生成を選択できる。点番号やグリフ構造が変わるため、元の命令を再適用しない。メインの出力ダイアログにもヒントなしを表示し、理由は情報アイコンで確認できる。

TrueType／OpenType CFF／CFF2とコレクションが対象。単体CFFは専用アイコンで保持・除去ができる。Type 1は元ファイル出力で保持、輪郭変換でヒントなしにする。CFFのstem／mask／Private DICTの除去と、TrueType命令の自動生成は[実装済み](autohint-engines.md)。静的CFF1／CID-CFFの自動生成にも対応する。CFF2の自動生成と命令編集は未対応。OSやラスタライザー側の自動ヒンティングを無効にする機能ではない。

## 実装

`font-core::hinting` が最大256 MiBの入力、locaの範囲・単調性、グリフ命令の長さ、合成グリフの引数・変換構造を検査する。命令自体は実行しない。短いlocaも読み、除去後は長いlocaへ再構築する。カラー保持出力と共通のSFNT組み立て処理を使う。

Tauriはアプリ所有のカタログIDから読み込み、workerで解析・変換する。WebViewには集計結果のみを返す。文書を変更せず、元ファイルの上書きと保護ディレクトリへの出力を拒否する。保存後はファイル全体の一致と再解析を確認する。三言語、処理中の二重実行防止、取消に対応する。

## 対応状況と検証

- 🟢 [Done] TrueType検査、保持／除去、単体face抽出、三言語UI、保存後の検証。
- 🟢 [Done] 自作データによる7件の回帰テスト。通常・空・合成グリフ、複数成分、合成の各拡縮・変形形式、短いloca、コレクション抽出、破損命令・loca、CFF保持・除去、関連外テーブルの保持、冪等性、輪郭・メトリクス維持を検証。FreeTypeで除去後の通常・合成グリフを12pxで描画した。
- 🟢 [Done] 最終構成のRust全体テスト347件、hintingテスト7件を含め成功。Clippyの全target `-D warnings`、TypeScript／Viteビルドも成功。
- 🟢 [Done] `npm run tauri build -- --debug --bundles app`：通常版macOS Fonts.app生成成功（82.03 MiB）。`cargo fmt --all -- --check` と `git diff --check` も成功。
- 🟢 [Done] Metal GPU回帰4件成功。サンドボックス内ではアダプターを取得できず2件失敗したため、実GPUへのアクセスを許可して再実行し、全4件の成功を確認した。
- 🟢 [Done] 独立したmacOS検証アプリでArialの検査・保持／除去・保存後検証と、メイン出力のヒントなし表示を操作・目視確認。日本語ダイアログの配置・改行に問題なし。Arialの元ファイル773,236 Bから保持763,956 B／除去337,988 Bを生成し、保持ではDSIGとhead調整値以外の全テーブル、除去では関連外テーブルの一致を別途確認した。
- 🟢 [Done] CFF除去・TrueType自動生成を追加。[詳細と追加検証](autohint-engines.md)。
- 🔴 [Later] Type 1の直接dehint、CFF2用ヒント生成、命令編集。
- ⭕️ [Pending] Windows/Linux実機、外部アプリごとの小サイズ表示、大規模な商用フォント群での追加検証。

仕様根拠：[OpenType glyf](https://learn.microsoft.com/en-us/typography/opentype/spec/glyf)、[maxp](https://learn.microsoft.com/en-us/typography/opentype/spec/maxp)、[head](https://learn.microsoft.com/en-us/typography/opentype/spec/head)。元のヒントが保持できても、外部アプリでの見え方の完全一致を保証するものではない。

- 🟢 [Done] [小サイズ比較](hinting-comparison.md)：元のヒント／ヒントなし／自動生成を並べて確認。
