# 外部依存の安定版更新 — 2026-11-02

## 範囲と保持

定期確認fonts-stable。開始時の作業ツリーはクリーン。リポジトリーと親のAGENTS.mdはなく、チャットの指示を適用した。npm公式latest、crates.io max_stable_version、Cargo公式indexと上流リリースを確認。beta／RC／nightlyは採用せず、未導入optional peerも追加しない。Rust Document/State/Renderer、wgpu、三言語、変換時の元ファイル名保持と元ファイル保護は維持する。

## 更新と互換性

- tauri-plugin-updater 2.14.0 → 2.14.1。上流はbase64 0.23とzip 9へ移行。WindowsのError::Extract内側の型がzip 9となるが、アプリはその内部型に依存しない。Windows実行はPending。
- brotli 9.1.0 → 9.1.1。空・短い連結stream、出力再開処理等の上流修正。既存圧縮／展開APIを維持。
- encoding_rs 0.8.42 → 0.8.44。ISO-2022-JPの出力境界、BOM候補、小buffer、長さ計算、SIMD partial変換のsoundness修正を取り込む。0.8.43も含む累積更新。
- libz-sys 1.1.30 → 1.1.31。同梱zlibはzlib.h上でも1.3.2。static feature／FreeTypeのsystem-zlib選択と独自build.rs・FFIを維持。上流build.rsの差分は探索コマンドのログ削除。
- npmはrolldown 1.2.15 → 1.2.16、@oxc-project/types 0.155.0 → 0.156.0、PostCSS 8.5.31 → 8.5.32、全プラットフォームbindingのlockを更新。Rolldownのtree-shaking／chunk順序／設定解決等、PostCSSの位置情報・finally修正を確認。実行ホストはmacOS arm64。npm監査0件。
- FreeType 2.15.0、HarfBuzz 15.0.0、未コンパイルlibpng 1.6.59、zlib 1.3.2、ttfautohint 1.8.4、psautohint 2.4.0は公式stableと一致し変更なし。native vendorのソース・ライセンス・独自修正を保持。
- FontForge公式stableは20251009。調査checkoutは20251009-134-g9666948b4、ローカル変更なし。ビルド／配布へ導入せずcheckoutも巻き戻さない。
- Tauri本体2.12.5、tauri-build 2.7.4、wgpu 30.0.1、React 19.3.1、TypeScript 7.0.2、Vite 8.3.5等、直接依存は最新stableを維持。API・設定の追加変更は不要だった。

## Cargo.lockの差分（削除・追加を含む）

| パッケージ | 更新前 | 更新後 |
| --- | --- | --- |
| arbitrary | 1.5.0 | — |
| base64 | 0.21.7, 0.22.1, 0.23.1 | 0.21.7, 0.23.1 |
| brotli | 9.1.0 | 9.1.1 |
| crc32fast | 1.5.3 | 1.5.4 |
| derive_arbitrary | 1.5.0 | — |
| encoding_rs | 0.8.42 | 0.8.44 |
| js-sys | 0.3.106 | 0.3.107 |
| libz-sys | 1.1.30 | 1.1.31 |
| log | 0.4.34 | 0.4.35 |
| nix | — | 0.31.3 |
| openssl-probe | 0.2.1 | 0.2.2 |
| rustix | 1.1.5 | 1.1.6 |
| tauri-plugin-updater | 2.14.0 | 2.14.1 |
| typed-path | — | 0.13.0 |
| unicode-properties | 0.1.4 | 0.1.5 |
| unicode-width | 0.2.2 | 0.2.3 |
| utf8_iter | 1.0.4 | 1.1.0 |
| utf_types | — | 1.0.0 |
| wasm-bindgen | 0.2.129 | 0.2.130 |
| wasm-bindgen-futures | 0.4.79 | 0.4.80 |
| wasm-bindgen-macro | 0.2.129 | 0.2.130 |
| wasm-bindgen-macro-support | 0.2.129 | 0.2.130 |
| wasm-bindgen-shared | 0.2.129 | 0.2.130 |
| web-sys | 0.3.106 | 0.3.107 |
| xattr | 1.6.1 | 1.7.0 |
| zip | 4.6.1 | 9.2.1 |

## package-lock.jsonの差分

| エントリー | 更新前 | 更新後 |
| --- | --- | --- |
| node_modules/@oxc-project/types | 0.155.0 | 0.156.0 |
| node_modules/@rolldown/binding-android-arm-eabi | 1.2.15 | 1.2.16 |
| node_modules/@rolldown/binding-android-arm64 | 1.2.15 | 1.2.16 |
| node_modules/@rolldown/binding-darwin-arm64 | 1.2.15 | 1.2.16 |
| node_modules/@rolldown/binding-darwin-x64 | 1.2.15 | 1.2.16 |
| node_modules/@rolldown/binding-freebsd-x64 | 1.2.15 | 1.2.16 |
| node_modules/@rolldown/binding-linux-arm-gnueabihf | 1.2.15 | 1.2.16 |
| node_modules/@rolldown/binding-linux-arm64-gnu | 1.2.15 | 1.2.16 |
| node_modules/@rolldown/binding-linux-arm64-musl | 1.2.15 | 1.2.16 |
| node_modules/@rolldown/binding-linux-ppc64-gnu | 1.2.15 | 1.2.16 |
| node_modules/@rolldown/binding-linux-s390x-gnu | 1.2.15 | 1.2.16 |
| node_modules/@rolldown/binding-linux-x64-gnu | 1.2.15 | 1.2.16 |
| node_modules/@rolldown/binding-linux-x64-musl | 1.2.15 | 1.2.16 |
| node_modules/@rolldown/binding-openharmony-arm64 | 1.2.15 | 1.2.16 |
| node_modules/@rolldown/binding-win32-arm64-msvc | 1.2.15 | 1.2.16 |
| node_modules/@rolldown/binding-win32-x64-msvc | 1.2.15 | 1.2.16 |
| node_modules/postcss | 8.5.31 | 8.5.32 |
| node_modules/rolldown | 1.2.15 | 1.2.16 |

## 見送り

更新後のcargo update --dry-run --verboseは変更0。generic-array 0.14.7 → 0.14.9、toml 0.8.2 → 0.8.23、toml_datetime 0.6.3 → 0.6.11、toml_edit 0.20.2 → 0.20.7は既存上流の厳密制約を維持する。nanoid 3.3.20 → 6.0.2もPostCSSの3.x要求のためメジャー強制overrideはしない。未導入optional peer（plugin-babel、oxc-transform-react等）は導入対象に含めない。既存lopdfのbrotli-decompressor 5系も上流制約のまま。

## 公式根拠

- [npm公式](https://registry.npmjs.org/)、[crates.io](https://crates.io/)、[Cargo公式index](https://index.crates.io/)でstable確認。
- [updater 2.14.1](https://github.com/tauri-apps/plugins-workspace/releases/tag/updater-v2.14.1)、[Rolldown 1.2.16](https://github.com/rolldown/rolldown/releases/tag/v1.2.16)、[PostCSS 8.5.32](https://github.com/postcss/postcss/releases/tag/8.5.32)。これらのrelease本文は公式GitHub APIでも確認した。
- [brotli 9.1.1](https://crates.io/crates/brotli/9.1.1)、[encoding_rs 0.8.44](https://crates.io/crates/encoding_rs/0.8.44)、[libz-sys 1.1.31](https://crates.io/crates/libz-sys/1.1.31)。配布ソースのREADME変更履歴・build.rs・zlib.hも確認。
- [FreeType](https://freetype.org/)、[HarfBuzz latest](https://github.com/harfbuzz/harfbuzz/releases/latest)、[libpng](https://www.libpng.org/pub/png/libpng.html)、[zlib](https://zlib.net/)、[ttfautohint](https://freetype.org/ttfautohint/)、[psautohint latest](https://github.com/adobe-type-tools/psautohint/releases/latest)、[FontForge latest](https://github.com/fontforge/fontforge/releases/latest)。

## 検証結果

- 🟢 [Done] `npm run build`：TypeScript／Vite成功。
- 🟢 [Done] `cargo check --workspace --all-targets --locked`：成功。
- 🟢 [Done] `cargo test --workspace --locked`：再実行で367件成功、失敗0、既存5件ignore。TrueType／CFF／CID／Type 1、可変・カラー、WOFF2、ヒント生成・除去、変換元ファイル保護等の既存回帰を含む。
- 初回は通常テスト367件通過後、font-rendererのdoctestでE0463（wgpu参照不可）。アプリビルドを並行実行していたため、両方の終了後にworkspaceテストを単独再実行しdoctestも成功した。原因は断定せず、再現しなかったためソース変更・doctest無効化は行わない。
- 🟢 [Done] `python3 -m unittest discover -s scripts -p 'test_*.py'`：9件成功。
- 🟢 [Done] ライセンス再生成：Rust569件、npm81件、MPLソース5件。両配布用／docs用一覧は一致し、Cargo.lockとpackage-lock.jsonのSHA-256も一致。
- 🟢 [Done] `cargo fmt --all -- --check`、`git diff --check`：成功。
- 🟢 [Done] `npm run tauri build -- --debug --bundles app --config '{"bundle":{"createUpdaterArtifacts":false}}'`：macOS arm64 Fonts.app生成成功（113.18 MiB）。公開鍵・署名設定は変更せず、ローカル検証コマンドだけで更新配布物を無効化した。
- ⭕️ [Pending] 署名付き更新配布物・公証・実更新インストール、Windows/Linux実機、GUI目視と複数DPI。秘密鍵の取得や設定の恒久変更は行わない。

- 🟢 [Done] `cargo test -p font-renderer --locked -- --ignored`：実Metal GPUの4件成功（穴・重なり・試し組み、参照画像、サムネイル）。
- 🟢 [Done] `cargo clippy --workspace --all-targets --locked --no-deps -- -D warnings`：成功。
