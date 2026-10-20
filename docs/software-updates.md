# ソフトウェアアップデートとGitHub公開

🟢 [Done] 環境設定 → 一般 → ソフトウェアアップデートから、更新確認・ダウンロード・署名検証・インストール／再起動を実行できる。英語・日本語・简体中文に対応。自動でネットワークへ接続せず、利用者の操作で確認する。

更新情報は `https://github.com/matsushibadenki/Fonts/releases/latest/download/latest.json` からHTTPSで取得する。新しいバージョンのみを候補とし、同一リポジトリのRelease assetだけをダウンロードする。Tauriの公式updaterをRustから利用し、埋め込んだ公開鍵による署名検証と署名内のバージョン照合を必須にする。検証が終わるまでインストールできない。通信・取得・検証の失敗を「最新」と表示しない。

文書・更新候補・ダウンロードしたバイナリはRustが所有する。WebViewとの通信はバージョン・進捗・結果のみ。設定画面を閉じてもダウンロード状態は維持する。インストール直前に全タブの未保存変更と作業中の編集を確認し、一つでも残っていれば保存／終了を案内して停止する。ファイル操作・一括変換と競合する場合も停止する。更新中に変更を捨てる確認は出さない。インストール中の編集を遮断し、正常完了後にワークスペースを保存して再起動する。

初回公開はmacOS Apple Silicon版。従来の0.1.0はアップデーター未搭載のため、初回だけReleaseのDMGまたはアプリアーカイブからFonts.appをApplicationsへ配置する必要がある。macOSのコード署名・公証とは別の、Tauri更新パッケージ署名を使用する。Apple Developer IDによるコード署名・公証は未実施。

## 署名鍵

秘密鍵は `.release-secrets/updater.key` に0600権限で保存し、ディレクトリーは0700とする。Gitから除外しており、GitHubには送信しない。リポジトリとアプリに含めるのは公開鍵のみ。この秘密鍵を安全にバックアップすること。鍵を失うと、現在の公開鍵を持つアプリへ同じ系列の更新を提供できない。

GitHub Actions Secretへの秘密鍵登録は自動承認レビューで却下されたため実施していない。今後CIで署名したい場合は、そのリポジトリへの秘密鍵送信について明示的な承認・管理方針が必要。現在のGitHub Actionsは秘密鍵なしでテスト・未配布検証ビルドのみを行う。

## 次回の公開手順

1. `Cargo.toml`、`package.json`／lock、`src-tauri/tauri.conf.json`のバージョンを一致させ、`docs/releases/vX.Y.Z.md`を追加する。
2. Rustテスト・Clippy・TypeScriptを確認し、必要なら`python3 scripts/collect_licenses.py`で告知を更新する。
3. `TAURI_SIGNING_PRIVATE_KEY="$PWD/.release-secrets/updater.key" TAURI_SIGNING_PRIVATE_KEY_PASSWORD='' npm run tauri -- build --bundles app,dmg --ci` でローカル署名ビルドする。
4. `python3 scripts/prepare_release.py` を実行する。アプリの公開鍵で署名とバージョンを独立検証し、改ざんしたデータが拒否されることも確認して、`target/release-publish/vX.Y.Z/`に公開ファイルを作成する。
5. コードをGitHubへpushし、そのコミットへ`vX.Y.Z`タグを付ける。`gh release create vX.Y.Z target/release-publish/vX.Y.Z/* --verify-tag --title 'Fonts vX.Y.Z' --notes-file docs/releases/vX.Y.Z.md`で公開する。公開するのは配布物・公開署名・manifest・チェックサムだけであり、秘密鍵を含めない。
6. Releaseの`latest.json`と配布物を再取得し、チェックサム・アプリでの更新確認を検証する。同じタグの配布物を差し替えず、新しいバージョンで修正を公開する。

`prepare_release.py`は現在のmacOSネイティブアーキテクチャの一つだけをmanifestに登録する。複数アーキテクチャを公開する場合は、各実機で署名した配布物を検証したうえでplatformsを統合してから公開する。

⭕️ [Pending] Windows/Linux・Intel Macの配布と実機更新、Appleのコード署名・公証。ダウンロード後のネットワーク切断、権限不足、全OSのインストーラー障害は継続検証対象。

## 公開・検証記録（2026-10-20）

🟢 [Done] [Fonts v0.1.1](https://github.com/matsushibadenki/Fonts/releases/tag/v0.1.1)をmacOS Apple Silicon向けに正式公開。DMG、署名済み更新アーカイブ、公開署名、latest.json、SHA256SUMSを配置した。署名と署名内バージョンを独立した検証プログラムで確認し、改ざんしたデータが拒否されることも確認した。

ローカルのRustテスト367件成功（重い任意検証5件は既存のignore）、Clippy `-D warnings`、TypeScript/Vite、リリースビルドが成功。GitHub Actionsでもテスト・Clippy・macOSビルドが成功した（ソース `e90c5c6`、[実行記録](https://github.com/matsushibadenki/Fonts/actions/runs/39397220197)）。

独立した0.1.0検証アプリを使い、日本語・英語・简体中文、GitHubからの0.1.1更新候補取得、実ダウンロード・署名検証、未保存文書でのインストール拒否、保存後のインストールと再起動を確認した。更新後のアプリ本体のInfo.plistと画面で0.1.1を確認し、検証文書の送り幅641が保存ファイルに残ることも確認した。
