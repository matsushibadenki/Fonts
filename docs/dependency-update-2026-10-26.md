# 外部依存の安定版更新 — 2026-10-26

## 確認範囲

`fonts-stable` の定期確認。開始時の作業ツリーはクリーン。親ディレクトリーを含めAGENTS.mdファイルはなく、チャットで指定された指示と既存の検証手順を適用した。npm公式latest、crates.ioのmax_stable_versionおよび公式sparse index、上流のリリース・変更履歴を確認し、beta／RC／nightlyや未導入のoptional peerは採用していない。Rust側のDocument/State/Rendererとwgpu、Rust/GPU側のデータ保持、三言語は維持する。

## 直接依存・ネイティブ

| 対象 | 更新前 | 更新後 |
| --- | --- | --- |
| @tauri-apps/cli | 2.12.3 | 2.12.4 |
| vite | 8.3.4 | 8.3.5 |
| tauri | 2.12.4 | 2.12.5 |
| tauri-build | 2.7.3 | 2.7.4 |
| tauri-plugin-updater | 2.13.3 | 2.14.0 |
| brotli | 9.0.0 | 9.1.0 |
| lopdf | 0.45.0 | 0.46.0 |
| roxmltree | 0.21.1 | 0.21.2 |
| src-tauriのdev用base64 | 0.22.1 | 0.23.1 |
| 同梱FreeType | 2.14.3 | 2.15.0 |
| 同梱HarfBuzz | 14.6.0 | 15.0.0 |
| 同梱libpng（未コンパイル） | 1.6.44 | 1.6.59 |

- メジャー更新HarfBuzz 15と、0.x互換性境界を含むlopdf 0.46を評価して採用。HarfBuzzの新OFF5テーブル対応、数値パーサー変更、CFF/CFF2のsubroutine処理の厳格化・instancing修正を確認した。既存C APIと静的ライブラリ構成を維持する。新しいraster/vector/GPUライブラリへ描画処理を移していない。
- FreeTypeの旧vendorツリー全体とHarfBuzzの旧srcは、旧公式アーカイブとバイト単位で一致していた。公式新ソースへ更新し、wrapper、独自build.rs、FFIの修正、zlib 1.3.2の静的選択を保持した。新しい既定module一覧が参照する `hvf/hvf` のstubをビルド対象に追加。実験的HVF機能のオプションは無効のまま。公開Face/GlyphSlotの構造体とFT_Span等の差分も確認し、既存FFIのFT_Span.xはすでにsigned shortで新ヘッダーと一致する。
- FreeTypeのOFF5／24-bit glyph等の新機能を、アプリ全体の対応範囲が広がったとは扱わない。既存Rustパーサー・Document・出力の上限と対応形式を維持する。
- libpngの旧C/Hソースも旧公式版と一致していた。新stableを適用し、vendor独自の生成済みpnglibconf.h等は保持。既存buildのPNGデコーダー無効化は維持し、画像の読み込みはRust image/pngを使用する。
- brotli 9.1のメタデータcallback・動的ring buffer・デコーダー変更、lopdf 0.46のPDF parser・inline image・xref・暗号化関連修正、updater 2.14のHTTP header・ダウンロード・Linux AppImage更新処理の修正を配布ソースの変更履歴でも確認した。アプリの既存API呼び出しは変更不要だった。
- 既存の `bundled_engine_version_is_pinned` が `FT_Library_Version` で実リンク版を検査する。初回テストは旧期待値2.14.3に対して実値2.15.0となって失敗したため、更新対象に合わせて期待値を2.15.0へ調整した。zlib実リンク版1.3.2のgzip/BDF描画テストも維持する。
- libz-sys 1.1.30、pkg-config 0.3.34、wgpu 30.0.1、freetype-rs 0.38.0、freetype-sys 0.23.0、ttf-parser 0.25.1等は確認時点のstableを維持。ttfautohint 1.8.4、psautohint 2.4.0も維持した。
- FontForgeの公式stableは20251009。調査用checkoutは `20251009-124-g5ceb1d784`（ローカル変更なし）であり、アプリのビルド／実行／配布に組み込まない。研究用checkoutを巻き戻していない。
- Rust 569パッケージ・npm 81パッケージとMPLソース5件の一覧、第三者告知を再生成。HVFのApple著作権とlibpngの原文ライセンスも収録する。ネイティブソースと元のライセンス表示は保持する。

## Cargo.lockの全変更

| パッケージ | 更新前 | 更新後 |
| --- | --- | --- |
| brotli | 9.0.0 | 9.1.0 |
| brotli-decompressor | 5.0.3, 6.0.1 | 5.0.3, 6.2.0 |
| cc | 1.8.0 | 1.10.0 |
| crc32fast | 1.5.2 | 1.5.3 |
| erased-serde | 0.4.10 | 0.4.11 |
| find-msvc-tools | 0.1.14 | 0.1.15 |
| ico | 0.5.0 | 0.6.0 |
| icu_collections | 2.3.0 | 2.4.0 |
| icu_locale_core | 2.3.0 | 2.4.0 |
| icu_normalizer | 2.3.0 | 2.4.0 |
| icu_normalizer_data | 2.3.0 | 2.4.0 |
| icu_properties | 2.3.0 | 2.4.0 |
| icu_properties_data | 2.3.0 | 2.4.0 |
| icu_provider | 2.3.1 | 2.4.0 |
| inout | 0.2.4 | 0.2.5 |
| lopdf | 0.45.0 | 0.46.0 |
| ordered-float | 5.5.0 | 5.5.1 |
| plist | 1.10.1 | 1.10.3 |
| png | 0.17.16, 0.18.1 | 0.18.1 |
| roxmltree | 0.21.1 | 0.21.2 |
| rustls-webpki | 0.103.15 | 0.103.16 |
| smallvec | 1.16.3 | 1.16.4 |
| tao | 0.37.1 | 0.37.2 |
| tauri | 2.12.4 | 2.12.5 |
| tauri-build | 2.7.3 | 2.7.4 |
| tauri-codegen | 2.7.3 | 2.7.4 |
| tauri-macros | 2.7.3 | 2.7.4 |
| tauri-plugin-updater | 2.13.3 | 2.14.0 |
| tauri-runtime-wry | 2.12.2 | 2.12.3 |
| tauri-winres | 0.3.6 | 0.3.7 |
| tinystr | 0.8.4 | 0.8.5 |
| tokio | 1.53.3 | 1.54.1 |
| uuid | 1.28.0 | 1.29.0 |
| webpki-root-certs | 1.0.9 | 1.0.11 |
| wry | 0.57.0 | 0.57.1 |
| yoke | 0.8.3 | 0.8.4 |
| zerovec | 0.11.8 | 0.11.9 |
| zerovec-derive | 0.11.6 | 0.11.7 |

base64は他の上流利用側の0.21／0.22系列が引き続き残るため、lockに存在する版の集合は変わらない。font-coreの直接依存はすでに0.23.1で、今回src-tauriのdev依存も揃えた。

## package-lock.jsonの全変更

| エントリー | 更新前 | 更新後 |
| --- | --- | --- |
| node_modules/@oxc-project/types | 0.154.0 | 0.155.0 |
| node_modules/@rolldown/binding-android-arm-eabi | 1.2.14 | 1.2.15 |
| node_modules/@rolldown/binding-android-arm64 | 1.2.14 | 1.2.15 |
| node_modules/@rolldown/binding-darwin-arm64 | 1.2.14 | 1.2.15 |
| node_modules/@rolldown/binding-darwin-x64 | 1.2.14 | 1.2.15 |
| node_modules/@rolldown/binding-freebsd-x64 | 1.2.14 | 1.2.15 |
| node_modules/@rolldown/binding-linux-arm-gnueabihf | 1.2.14 | 1.2.15 |
| node_modules/@rolldown/binding-linux-arm64-gnu | 1.2.14 | 1.2.15 |
| node_modules/@rolldown/binding-linux-arm64-musl | 1.2.14 | 1.2.15 |
| node_modules/@rolldown/binding-linux-ppc64-gnu | 1.2.14 | 1.2.15 |
| node_modules/@rolldown/binding-linux-s390x-gnu | 1.2.14 | 1.2.15 |
| node_modules/@rolldown/binding-linux-x64-gnu | 1.2.14 | 1.2.15 |
| node_modules/@rolldown/binding-linux-x64-musl | 1.2.14 | 1.2.15 |
| node_modules/@rolldown/binding-openharmony-arm64 | 1.2.14 | 1.2.15 |
| node_modules/@rolldown/binding-win32-arm64-msvc | 1.2.14 | 1.2.15 |
| node_modules/@rolldown/binding-win32-x64-msvc | 1.2.14 | 1.2.15 |
| node_modules/@tauri-apps/cli | 2.12.3 | 2.12.4 |
| node_modules/@tauri-apps/cli-darwin-arm64 | 2.12.3 | 2.12.4 |
| node_modules/@tauri-apps/cli-darwin-x64 | 2.12.3 | 2.12.4 |
| node_modules/@tauri-apps/cli-linux-arm-gnueabihf | 2.12.3 | 2.12.4 |
| node_modules/@tauri-apps/cli-linux-arm64-gnu | 2.12.3 | 2.12.4 |
| node_modules/@tauri-apps/cli-linux-arm64-musl | 2.12.3 | 2.12.4 |
| node_modules/@tauri-apps/cli-linux-riscv64-gnu | 2.12.3 | 2.12.4 |
| node_modules/@tauri-apps/cli-linux-x64-gnu | 2.12.3 | 2.12.4 |
| node_modules/@tauri-apps/cli-linux-x64-musl | 2.12.3 | 2.12.4 |
| node_modules/@tauri-apps/cli-win32-arm64-msvc | 2.12.3 | 2.12.4 |
| node_modules/@tauri-apps/cli-win32-ia32-msvc | 2.12.3 | 2.12.4 |
| node_modules/@tauri-apps/cli-win32-x64-msvc | 2.12.3 | 2.12.4 |
| node_modules/rolldown | 1.2.14 | 1.2.15 |
| node_modules/vite | 8.3.4 | 8.3.5 |

`npm install --ignore-scripts` と `npm update --ignore-scripts` により更新。監査は脆弱性0件。全OS bindingのlockエントリーは更新したが、実行対象はmacOS arm64。

## 見送り・変更なし

- 更新後の `cargo update --dry-run --verbose` は変更0件。残るgeneric-array 0.14.7（候補0.14.9）はcrypto-common 0.1.7の厳密指定、toml_datetime 0.6.3／toml_edit 0.20.2（候補0.6.11／0.20.7）はproc-macro-crate 2.0.2の厳密指定。system-deps配下のtoml 0.8.2（候補0.8.23）もこれらの制約と整合が必要で、強制overrideは入れない。
- brotli-decompressorの旧5.0.3（最新6.2.0）は最新lopdf 0.46の上流制約。`cargo tree --invert brotli-decompressor@5.0.3` でも確認し、互換性の異なる系列への強制overrideは行わない。
- `npm outdated --all` で導入済みかつ最新との差があるのはnanoid 3.3.20（最新6.0.2）。PostCSSの3.x要求のためメジャー強制置換は見送る。他OS binding・未導入optional peerの表示は、ホストに導入されていない項目である。
- npm自体のグローバル更新はアプリの依存更新に含めず、開発環境のpackage managerを勝手に変更していない。
- crates.io APIが一度429を返したため、vendor用libz-sys／pkg-configの確認は公式sparse indexへ切り替えて成功した。ダウンロードしたコードはすべて公式配布先のもの。

## アーカイブ・独自修正の保持

SHA-256と公式URLは[FreeType/libpng記録](vendor-font-engine.json)と[HarfBuzz記録](../vendor/font-hinting/manifest.json)に記載した。FreeType 2.15.0、HarfBuzz 15.0.0の展開元と更新後vendorは一致し、libpngも新公式ソースと一致する（上記vendor-only設定は別途保持）。

上流のconfigure生成物・ドキュメント等には既存のtrailing whitespace／タブ混在が含まれる。`git diff --check` はその上流空白を報告するため、ネイティブの原本一致を優先して改変せず保持。アプリ所有の変更（vendor以外）とRust整形は別途検証する。

## 検証

- 🟢 [Done] `npm run build`：TypeScript／Vite 8.3.5成功。
- 🟢 [Done] `cargo check --workspace --all-targets --locked`：成功。
- 🟢 [Done] `cargo test --workspace --locked --offline`：367件成功、失敗0、通常実行では既存5件ignore。フォント入出力、Type 1／CFF／CID、CFF2除去、自動ヒント、WOFF2、画像・ペースト、可変・カラー、更新機能の関連回帰を含む。
- 🟢 [Done] `cargo test -p font-renderer --locked --offline -- --ignored`：実Metal GPUの4件成功。輪郭の穴、試し組み、参照画像、サムネイルを確認。
- 🟢 [Done] 全target Clippy `--no-deps -- -D warnings` 成功。既存リリース用Python unittest 6件成功。
- 🟢 [Done] Rust整形検査と `git diff --check -- . ':!vendor'` 成功。vendor全体の空白警告は前述の通り公式原本のまま保持した。
- 🟢 [Done] lockfile SHA-256とライセンス一覧の一致、配布用・docs用一覧の同一性、ネイティブ公式アーカイブSHA-256との一致を確認。
- 🟢 [Done] ローカル検証用 `npm run tauri build -- --debug --bundles app --config '{"bundle":{"createUpdaterArtifacts":false}}'` 成功。macOS arm64 Fonts.app（113.15 MiB）を生成した。
- ⭕️ [Pending] 通常の `npm run tauri build -- --debug --bundles app` はアプリと更新用アーカイブの生成後、設定済み公開鍵に対応する `TAURI_SIGNING_PRIVATE_KEY` が環境にないため署名段階で終了1。秘密鍵を読み出したり設定を恒久的に弱めたりせず、上記コマンド限定で更新配布物の生成を外してアプリビルドを検証した。署名付き更新配布物・公証・実際の更新インストールは保留。リポジトリーの公開鍵・署名設定は変更していない。
- ⭕️ [Pending] Windows/Linuxのビルド・実機、GUI目視・操作、複数DPIやOS由来のsurface Lost/Outdated等の追加検証。macOSのall-targetsを他OSの検証とは扱わない。

## 公式根拠

- [npm公式レジストリ](https://registry.npmjs.org/)、[crates.io](https://crates.io/)と[公式sparse index](https://index.crates.io/)。
- [Tauri 2.12.5](https://github.com/tauri-apps/tauri/releases/tag/tauri-v2.12.5)、[CLI 2.12.4](https://github.com/tauri-apps/tauri/releases/tag/tauri-cli-v2.12.4)、[updater 2.14.0](https://github.com/tauri-apps/plugins-workspace/releases/tag/updater-v2.14.0)、[Vite変更履歴](https://github.com/vitejs/vite/blob/main/packages/vite/CHANGELOG.md)。
- [FreeType 2.15.0](https://freetype.org/)、配布ソースのdocs/CHANGES・include/freetype/config/ftmodule.h・公開ヘッダー。
- [HarfBuzz 15.0.0](https://github.com/harfbuzz/harfbuzz/releases/tag/15.0.0)と配布NEWS。
- [libpng 1.6.59](https://www.libpng.org/pub/png/libpng.html)、[zlib 1.3.2](https://zlib.net/)。
- [lopdf 0.46](https://github.com/J-F-Liu/lopdf/releases/tag/v0.46.0)、[brotli 9.1](https://crates.io/crates/brotli/9.1.0)と配布README、[roxmltree変更履歴](https://github.com/RazrFalcon/roxmltree/blob/master/CHANGELOG.md)。
- [FontForge stable](https://github.com/fontforge/fontforge/releases/tag/20251009)、[ttfautohint](https://freetype.org/ttfautohint/)、[psautohint 2.4.0](https://github.com/adobe-type-tools/psautohint/releases/tag/v2.4.0)。
