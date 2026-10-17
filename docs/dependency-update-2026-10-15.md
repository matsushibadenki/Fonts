# 外部依存の安定版更新 — 2026-10-15

## 定期実行

🟢 [Done] このチャットの自動実行 `fonts-stable` を有効化。毎週月曜09:00（Asia/Tokyo）に最新版stableの確認、実更新、互換性調整、検証を行う。更新・重要な失敗・必要な判断がある時だけ通知する。

## 更新

公式npmレジストリのlatestとcrates.ioのmax_stable_versionを取得し、プレリリースを除外して確認した。Tauri 2とRust所有のDocument/State/Renderer、Rust/GPU側の描画・データ保持を維持している。

| 直接依存 | 更新前 | 更新後 |
| --- | --- | --- |
| @tauri-apps/api | 2.12.3 | 2.12.4 |
| @tauri-apps/cli | 2.12.2 | 2.12.3 |
| react / react-dom | 19.3.0 | 19.3.1 |
| tauri | 2.12.3 | 2.12.4 |
| tauri-build | 2.7.2 | 2.7.3 |
| wgpu | 27.0.1 | 30.0.1 |
| pollster | 0.4 | 1.0.1 |
| dirs | 6.0.0 | 7.0.0 |
| sha2 | 0.10.9 | 0.11.1 |
| winreg（Windows） | 0.55.0 | 0.56.0 |
| roxmltree | 0.20.0 | 0.21.1 |
| svgtypes | 0.15.3 | 0.16.1 |

Cargo.lockの間接依存も上流のバージョン制約内で更新した。npm更新後の `npm outdated --json` は空オブジェクトで、直接依存に更新候補はない。npm installの監査では脆弱性0件。

serde 1.0.229、serde_json 1.0.154、unicode-segmentation 1.13.3は既存のmanifest範囲で最新stableを解決。bytemuck 1.25.2、ttf-parser 0.25.1、freetype-rs 0.38.0、rfd 0.17.2、raw-window-handle 0.6.2、objc2 0.6.5、objc2-app-kit / foundation 0.3.2、windows-sys 0.61.2は確認時点ですでに最新stable。

同梱FreeTypeは公式最新の2.14.3を維持。vendorの独自ビルド修正とライセンス表示を保持した。使用しないPNGデコードの同梱ソースは更新対象外。FontForge checkoutは仕様・挙動の調査資料であり現在のアプリの実行依存ではないため、今回のランタイム更新では変更していない。

## 互換性対応

- wgpu 30のInstanceDescriptor所有権・初期化、adapter設定、頂点バッファのOption、depth設定のOption、multiview_mask、surface color_spaceに移行。
- CurrentSurfaceTextureによる取得とQueue::presentへ移行。Occluded/Timeoutでは描画をスキップし、Outdatedは再設定、Suboptimalはpresent後に再設定する。LostではAppKit側で保持中のNSViewからsurfaceを再作成し、同じGPU状態で再描画する。
- GPUテストのreadbackをResultを返すget_mapped_range APIに対応。
- sha2 0.11で廃止されたdigest出力のLowerHexに代わり、64文字の小文字hexを明示生成。既存manifestとファイル名のIDを維持する既知ベクトル回帰テストを追加。
- UIの文言変更はなく、英語・日本語・简体中文の構成を保持。

## 検証

- 🟢 [Done] `npm run build`：TypeScript / Vite成功。
- 🟢 [Done] `npm outdated --json`：更新候補なし。
- 🟢 [Done] `cargo check --workspace --all-targets --locked --offline`：macOSホストで成功。
- 🟢 [Done] `cargo test --workspace --locked --offline`：228件成功、GPU 2件とOSフォント登録1件は通常実行ではignore。
- 🟢 [Done] `cargo test -p font-renderer --locked --offline -- --ignored`：実Metal GPUで輪郭の穴と試し組みの重なり・穴の2件成功。
- 🟢 [Done] `cargo clippy --workspace --all-targets --locked --offline --no-deps -- -D warnings`：成功。
- 🟢 [Done] `npm run tauri build -- --debug --bundles app`：macOS Fonts.app生成成功。
- 🟢 [Done] `cargo fmt --all -- --check`：成功。
- ⭕️ [Pending] Windows/Linuxの実機・ターゲット検証。macOSのall-targetsは別OSの検証を意味しない。
- ⭕️ [Pending] OSユーザーフォントへの一時登録テスト、surface Lost/Outdated等のOS由来イベント、GUIの目視・操作確認。

## 公式根拠

- [npm公式レジストリ](https://registry.npmjs.org/)：直接依存のlatest確認。
- [crates.io API](https://crates.io/api/v1/crates/wgpu)：各直接依存のmax_stable_version確認。
- [wgpu 30.0.1](https://github.com/gfx-rs/wgpu/releases/tag/v30.0.1)とダウンロードした30.0.1のAPIソースを移行時に確認。
- [FreeType公式リリース情報](https://freetype.org/index)：最新2.14.3を確認。
