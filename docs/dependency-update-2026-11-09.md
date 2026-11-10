# 外部依存の安定版更新 — 2026-11-09

## 範囲

fonts-stable定期確認。開始時の作業ツリーはクリーン。リポジトリーと親にAGENTS.mdファイルはなく、チャット指定の指示を適用。npm公式latest、crates.io max_stable_version、Cargo公式index、上流リリース／変更履歴を確認した。beta・RC・nightlyは採用しない。既存0.1.3の編集機能、三言語、Rust Document/State/Renderer、wgpu、大容量データのRust/GPU保持を維持する。

## 直接依存・ネイティブ更新と互換性

- HarfBuzz 15.0.0 → 15.1.0。旧vendor src全体が旧公式アーカイブ（既存記録のSHA-256とも一致）とバイト単位で一致することを確認した後、新公式srcとCOPYING/AUTHORSを適用。独自wrapper/build.rs・FreeType連携・ライセンスを保持。組版（Indic／Arabic／AAT）、COLR、gvar中間tuple、CFF／CFF2／可変instancing、異常フォント処理などの修正。nested blendの一部CFF2変換は輪郭を書き換えず失敗させる上流変更を含む。HarfBuzz GPU機能への描画移行は行わない。
- libz-sys 1.1.31 → 1.1.32、実ネイティブzlib 1.3.2 → 1.4。最新crateにもzlib 1.3.2が含まれるため、libz-sys 1.1.32をvendorしsrc/zlibだけ公式1.4ソースへ置換するCargo patchを追加。Rust FFI、上流static build、zlib-ng fallbackとライセンスは保持。未使用zlib-ngは同梱2.3.3が公式latest stableと一致し、featureを有効化していない。公式SHA-256を照合し、実リンク版を検査する既存テストの期待値も1.4へ更新。
- zlib 1.4のz_off_t既定型の復帰（long）、deflateResetのgzip header reset、raw inflate終了状態、gzip error処理、CMake名変更を評価。既存FreeTypeのFT_CONFIG_OPTION_SYSTEM_ZLIBとlibz-sys static構成で新ヘッダーとCソースを一緒にビルドする。libc::off_tの既存FFIとmacOSの選択型は一致。Windows/Linux実行はPending。CMakeはこのstock zlibビルドでは使用しない。LIBZ_SYS_STATIC=0等の明示的なシステムリンクoverrideは従来通りであり、同梱版検証は既定static構成が対象。
- lopdf 0.46.0 → 0.47.0。object stream／PNG predictorの展開制限、inline image、xref復旧、暗号化filter等の修正を取り込み、既存PDF入出力APIを検証。
- resvg 0.48.1 → 0.49.0（usvgも同版）。transform／clip／filter／gradient等の修正。新BlendMode／CompositeOperator variantとgroup bounding boxのclip反映を評価し、既存SVGフォントの許可範囲・外部参照制限・画像メモリー上限は維持。
- rfd 0.17.2 → 0.18.0。macOS保存filter picker、Linux zenity、GTK4等の上流変更。既存native dialog APIと親window設定を検証。MSRV 1.92要件に対し開発環境Rust 1.99.1で要件を満たす。
- svgtypes 0.16.1 → 0.16.2、flate2 1.1.10 → 1.1.11、uuid 1.29.0 → 1.29.1、間接依存も互換範囲で更新。
- Vite 8.3.5 → 8.3.6（CSS依存のdynamic import解決、chunk hash安定化、dev/HMR等の修正）。@vitejs/plugin-react 6.1.2 → 6.2.1。refreshInclude／refreshExclude追加と修正を確認。未使用React compiler用optional peerは追加しない。Rolldown 1.3.0は公式がVite 8.3.6以前への強制導入を禁止し、更新後Vite 8.3.6も1.2系制約のため1.2.16を維持。@oxc-project/types 0.156.0とbindingも整合する版を保持し、overrideは追加しない。
- FreeType 2.15.0、HarfBuzz以外のttfautohint 1.8.4／psautohint 2.4.0、未コンパイルlibpng 1.6.59は公式stableと一致し変更なし。FreeType配布物に含まれる未使用private zlib 1.3.1も公式原本として保持し、実リンクは上記1.4を使用する。
- FontForge公式stableは20251009。研究用checkoutは現在存在せず、復元・ビルド・実行依存の追加は行わない。

## Cargo.lockの全差分

| パッケージ | 更新前 | 更新後 |
| --- | --- | --- |
| aes | 0.9.3 | 0.9.4 |
| brotli-decompressor | 5.0.3, 6.2.0 | 5.0.3, 6.3.0 |
| cc | 1.10.0 | 1.11.1 |
| constant_time_eq | — | 0.6.1 |
| defmt | 1.2.0 | — |
| defmt-macros | 1.2.0 | — |
| defmt-parser | 1.1.0 | — |
| flate2 | 1.1.10 | 1.1.11 |
| inout | 0.2.5 | 0.2.6 |
| jiff | 0.2.38 | 0.2.41 |
| jiff-core | 0.1.1 | 0.1.4 |
| jiff-static | 0.2.38 | 0.2.41 |
| libz-sys | 1.1.31 | 1.1.32 |
| lopdf | 0.46.0 | 0.47.0 |
| mime | 0.3.17 | 0.3.18 |
| objc2-uniform-type-identifiers | — | 0.3.2 |
| pollster | 0.4.0, 1.0.1 | 1.0.1 |
| resvg | 0.48.1 | 0.49.0 |
| rfd | 0.17.2 | 0.18.0 |
| serde_spanned | 0.6.9, 1.1.2 | 0.6.9, 1.1.3 |
| smallvec | 1.16.4 | 1.16.5 |
| svgtypes | 0.16.1 | 0.16.2 |
| toml | 0.8.2, 1.1.8+spec-1.1.0 | 0.8.2, 1.2.1+spec-1.1.0 |
| toml_datetime | 0.6.3, 1.1.2+spec-1.1.0 | 0.6.3, 1.1.3+spec-1.1.0 |
| toml_edit | 0.19.15, 0.20.2, 0.25.17+spec-1.1.0 | 0.19.15, 0.20.2, 0.25.19+spec-1.1.0 |
| toml_writer | 1.1.3+spec-1.1.0 | 1.2.0+spec-1.1.0 |
| usvg | 0.48.1 | 0.49.0 |
| uuid | 1.29.0 | 1.29.1 |
| winnow | 0.5.40, 1.0.4 | 0.5.40, 1.0.5 |
| zeroize_derive | — | 1.5.0 |
| zip | 9.2.1 | 9.5.0 |

libz-sys 1.1.32のoriginはローカルvendor patchとなる。stock zlib以外の上流ファイルは保持する。

## npm lockの全差分

| エントリー | 更新前 | 更新後 |
| --- | --- | --- |
| node_modules/@vitejs/plugin-react | 6.1.2 | 6.2.1 |
| node_modules/vite | 8.3.5 | 8.3.6 |

## 見送り・変更なし

更新後cargo update --dry-run --verboseは変更0。generic-array 0.14.7、toml 0.8.2、toml_datetime 0.6.3、toml_edit 0.20.2は既存上流の厳密制約による旧版を保持。PostCSS配下のnanoid 3.3.20は最新6.0.2とのメジャー差を評価し3.x制約のため強制overrideを見送る。未導入optional peer・他OS用bindingの未インストール表示はホストへの追加対象にしない。他OS bindingのlockは更新するが実行検証はmacOS arm64のみ。

## 公式根拠

- [npm registry](https://registry.npmjs.org/)、[crates.io](https://crates.io/)、[Cargo sparse index](https://index.crates.io/)。
- [HarfBuzz 15.1.0](https://github.com/harfbuzz/harfbuzz/releases/tag/15.1.0)、[zlib 1.4と移行点](https://zlib.net/)。公式アーカイブURLとSHA-256は[HarfBuzz manifest](../vendor/font-hinting/manifest.json)と[zlib記録](vendor-font-engine.json)に保存。
- [lopdf 0.47.0](https://github.com/J-F-Liu/lopdf/releases/tag/v0.47.0)、[resvg changelog](https://github.com/linebender/resvg/blob/v0.49.0/CHANGELOG.md)、[rfd 0.18.0](https://github.com/PolyMeilex/rfd/releases/tag/0.18.0)。
- [Vite 8.3.6](https://github.com/vitejs/vite/releases/tag/v8.3.6)、[React plugin changelog](https://github.com/vitejs/vite-plugin-react/blob/plugin-react%406.2.1/packages/plugin-react/CHANGELOG.md)、[Rolldown 1.3.0](https://github.com/rolldown/rolldown/releases/tag/v1.3.0)。
- [FreeType](https://freetype.org/)、[libpng](https://libpng.org/pub/png/libpng.html)、[ttfautohint](https://freetype.org/ttfautohint/)、[psautohint latest](https://github.com/adobe-type-tools/psautohint/releases/latest)、[FontForge latest](https://github.com/fontforge/fontforge/releases/latest)。

## 実測した検証

- 🟢 [Done] `npm run build`：Vite 8.3.6／TypeScript成功。npm監査0件。
- 🟢 [Done] `cargo check --workspace --all-targets --locked`：成功。rfd／resvg等のAPI呼び出し追加修正は不要。
- 🟢 [Done] `cargo test --workspace --locked`：578件成功、失敗0、既存18件ignore。フォント入出力、SVG／COLR、CFF／CID、可変・複数マスター、自動ヒント、圧縮・保存・元ファイル保護等を含む。既存FreeType gzipテストは実リンクzlib 1.4とgzip BDF描画・不完全入力の失敗を確認した。
- 🟢 [Done] `python3 -m unittest discover -s scripts -p 'test_*.py'`：9件成功。
- 🟢 [Done] `cargo fmt --all -- --check`、vendorを除く `git diff --check`：成功。上流vendorの空白は原本一致のため保持する。
- 🟢 [Done] ライセンス再生成：Rust590件、npm81件、MPLソース5件。docs用／配布用一覧の同一性、lockfile SHA-256を確認。HarfBuzz新srcとzlib新srcは公式展開ソースとバイト単位で一致。
- ⭕️ [Pending] Windows/Linux実機・GUI操作・複数DPI、署名付き更新配布物・公証・実更新インストール。開発環境で未検証の事項を実装／成功扱いしない。

- 🟢 [Done] `cargo test -p font-renderer --locked -- --ignored`：実Metal GPUの17件成功。SVG／COLRv1・palette、縦組み、可変masterと静的出力の一致、components、参照画像、穴、画像trace、thumbnailを確認。
- 🟢 [Done] `cargo clippy --workspace --all-targets --locked --no-deps -- -D warnings`：成功。今回の検証に失敗はなく、互換性の追加修正による機能変更は不要だった。
