# 設計ドキュメント

- [フォント制作アプリ設計書](font-app-design.md)
- [LumaPaint coreのコピー履歴・SHA-256](reuse-manifest.json)
- [ネイティブ座標変換の流用記録](reuse-native.json)

- [実装状況と実行方法](implementation-status.md)

Rustの文書・編集・保存、Tauri画面、macOS wgpu描画、ドッキング・フローティングパネルを実装済み。フォント出力は今後実装する。

- [フォント形式の入出力・変換](font-formats.md)
- [OSフォントライブラリ](font-library.md)

- [ROADMAP — 現在の実装優先順位](ROADMAP.md)

- [自動保存・異常終了後の復旧](autosave-recovery.md)

- [実フォントの試し組み](proof.md)

- [Unicode割り当て・フォント情報・文字セット](font-editing.md)

- [フォントの複数選択・一括変換](batch-font-conversion.md)

- [輪郭の点の追加・削除](contour-topology.md)

- [アタリ画像の描画・変形・保存・三言語UI](reference-images.md)

- [部首・図形スニペットの登録・独立コピー配置](components-snippets.md)

- [フローティングパネルの位置・サイズの保存と復元](floating-panels.md)

- [パネルの入れ子split・タブ結合・保存](nested-splits.md)

- [複数文書の切り替え・保存・復旧](multiple-documents.md)

- [Webフォント用WOFF2出力・設定・検証](woff2.md)

- 🟢 [Done] [可変フォントの軸設定・GPUプレビュー・静的インスタンス出力](variable-fonts.md)。

- 🟢 [Done] [カラーフォントの読み込み・GPUプレビュー・カラー保持出力](color-fonts.md)。

- 🟢 [Done] [ヒンティングの検査・保持・TrueType命令除去](hinting.md)。

- 🟢 [Done] [CFFのヒント除去・TrueType自動生成](autohint-engines.md)。

- [ヒンティングの小サイズ比較](hinting-comparison.md)

- 🟢 [Done] [三言語ソフトウェアアップデート・ローカル署名とGitHub Releases配布](software-updates.md)。
