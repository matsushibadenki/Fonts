# 外部依存の安定版更新 — 2026-10-19

## 確認と更新

🟢 [Done] `fonts-stable` の定期確認。開始時のGit作業ツリーはクリーンだった。npm公式レジストリのlatest、crates.ioのstable、上流リリースと移行情報を確認し、beta・RC・nightlyは採用していない。既存機能、Tauri 2、Rust所有のDocument/State/Renderer、wgpu描画、Rust/GPU側のデータ保持、英語・日本語・简体中文を維持した。

| 対象 | 更新前 | 更新後 |
| --- | --- | --- |
| font-coreのbase64直接依存 | 0.22.1 | 0.23.1 |
| combine | 4.6.8 | 4.6.9 |
| defmt / defmt-macros | 1.1.1 | 1.2.0 |
| defmt-parser | 1.0.0 | 1.1.0 |
| inout | 0.2.2 | 0.2.4 |
| libc | 0.2.190 | 0.2.191 |
| rolldown / 各OSのbinding | 1.2.12 | 1.2.14 |
| @oxc-project/types | 0.152.0 | 0.154.0 |
| postcss | 8.5.29 | 8.5.31 |
| FreeTypeのgzip処理が使用するzlib | 内部コピー1.3.1 | libz-sys経由1.3.2 |

npmの直接依存はすべて確認時点の最新stableで変更なし。更新後の `npm outdated --json` は `{}`。`npm update --ignore-scripts` の監査は脆弱性0件。Rustの間接依存はCargoの解決可能範囲で更新した。`defmt-macros` は既存lock内のsyn 3.0.9を使う構成になった。全プラットフォームのnpm bindingのlockエントリーも更新したが、実行したのはmacOS arm64のみ。

## 互換性とネイティブコード

- base64 0.23の変更（SIMDエンジン、既定feature、MSRV 1.71、エラー型の説明）をリリースノートで確認。既存の `STANDARD` / `Engine` APIはそのまま使用でき、参照画像の埋め込み・復元テストも成功した。
- vendored FreeType 2.14.3は最新stableのため維持。独自ビルド・FFI修正と原著作権表示は保持した。
- FreeType公式の `FT_CONFIG_OPTION_SYSTEM_ZLIB` を有効にし、既存の `DEP_Z_INCLUDE` とlibz-sys 1.1.30の `static` featureを使用する。libz-sysの同梱zlib 1.3.2をコンパイルし、古いFreeType内部コピーを実行経路から外した。内部ソースを置換して独自修正を失うことを避けた。
- `LIBZ_SYS_STATIC=0` を明示設定したビルド環境では、上流libz-sysの仕様により静的ビルド指定を上書きできる。今回の検証ではこの上書きを使用せず、実際にリンクされた `zlibVersion()` が1.3.2であることを確認した。
- 自作の8×8 BDFをgzip化したfixtureと回帰テストを追加。FreeTypeの `FT_Gzip_Uncompress` による展開、BDFの読み込みとAの描画、切り詰めたgzipの拒否を確認。BDFをgzipのままfaceとして開く初回テストは `UnknownFileFormat` になったため、公開gzip APIで展開してから読み込む検証へ修正し成功した。
- vendorの未使用libpng 1.6.44は、PNGデコードを有効化・コンパイルしていないため保持した。画像読み込み用のRust image/png依存は別途確認対象に含めた。
- FontForgeの公式最新stableは20251009。調査用checkoutは `20251009-123-g785f1c743` で、アプリのビルド・実行・配布に含まれない。研究用の既存checkoutをstableへ巻き戻す操作は行っていない。
- lockfileに合わせてライセンス一覧2部と第三者告知を再生成。522 Rustパッケージ、81 npmパッケージ、既存MPLソース5件を収録し、FreeType内部およびリンク先zlibの告知も保持した。lockfileのSHA-256と一覧の一致、一覧2部の同一性を確認した。

## 更新を見送った依存

親ライブラリの制約を無視するoverrideは追加していない。

| 対象 | 維持した版 / 更新候補 | 理由 |
| --- | --- | --- |
| base64の他のlockエントリー | 0.21.7・0.22.1 / 0.23.1 | 上流利用側の別系列要求。font-coreのみ最新へ移行 |
| brotli-decompressor | 6.0.1 / 6.1.0 | brotli 9.0.0の `~6.0` 指定 |
| generic-array | 0.14.7 / 0.14.9 | crypto-common 0.1.7の厳密指定 |
| toml_datetime / toml_edit | 0.6.3 / 0.6.11、0.20.2 / 0.20.7 | proc-macro-crate 2.0.2の厳密指定 |
| toml | 0.8.2 / 0.8.23 | system-deps 6.2.2配下でtoml_editの制約との整合が必要 |
| nanoid | 3.3.20 / 6.0.2 | PostCSSの3.x要求。メジャー更新を評価したが強制置換は見送り |
| @vitejs/devtools / oxc-transform-react | 未導入 / 0.8.0・0.154.0 | Vite/plugin-reactのoptional peer。使用していないものを更新目的で新規導入しない |

`npm outdated --all` には他OS用bindingや未導入のoptional peerも出る。導入済みパッケージで最新との差が残るのは上記nanoidのみ。

## 実測した検証

- 🟢 [Done] `npm run build`：TypeScript / Vite成功。
- 🟢 [Done] `cargo check --workspace --all-targets --locked --offline`：成功。
- 🟢 [Done] `cargo test --workspace --locked --offline`：340件成功、失敗0、通常実行では5件ignore。
- 🟢 [Done] `cargo test -p font-renderer --locked --offline -- --ignored`：Metal GPUで4件成功（輪郭の穴、試し組み、参照画像、サムネイル）。通常ignoreの残り1件はOSフォント登録テスト。
- 🟢 [Done] フォント入出力、変換、WOFF2、カラーフォント、参照画像、gzip展開の関連回帰テスト成功。
- 🟢 [Done] `cargo clippy --workspace --all-targets --locked --offline --no-deps -- -D warnings`：成功。
- 🟢 [Done] `npm run tauri build -- --debug --bundles app`：macOS arm64のFonts.app生成成功（81.59 MiB）。
- 🟢 [Done] `cargo fmt --all -- --check` / `git diff --check`：成功。
- ⭕️ [Pending] Windows/Linuxのビルド・実機確認。macOSのall-targetsは他OSの検証を意味しない。
- ⭕️ [Pending] OSユーザーフォント登録、GUI目視・操作、surface Lost/Outdated等のOSイベント検証。

## 公式根拠

- [npm公式レジストリ](https://registry.npmjs.org/)と[crates.io](https://crates.io/)：直接・間接依存の公開版と依存制約。
- [base64 0.23.1](https://github.com/marshallpierce/rust-base64/releases/tag/v0.23.1)と配布crateの `RELEASE-NOTES.md`：API・feature・MSRV変更。
- [rolldown 1.2.14](https://github.com/rolldown/rolldown/releases/tag/v1.2.14)、[PostCSS 8.5.31](https://github.com/postcss/postcss/releases/tag/8.5.31)：ビルド依存の安定版。
- [FreeType公式](https://freetype.org/)、[gzip API](https://freetype.org/freetype2/docs/reference/ft2-gzip.html)、同梱 `ftoption.h` / `ftgzip.c`：2.14.3と外部zlib設定。
- [zlib公式](https://zlib.net/)、[libz-sys 1.1.30](https://crates.io/crates/libz-sys/1.1.30)の配布ソース・build.rs：zlib 1.3.2と静的ビルドfeature。
- [FontForge公式リリース](https://github.com/fontforge/fontforge/releases/latest)：stable 20251009。
