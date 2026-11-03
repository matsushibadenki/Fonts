# ソフトウェアアップデートとGitHub公開

🟢 [Done] 環境設定 → 一般 → ソフトウェアアップデートから、更新確認・ダウンロード・署名検証・インストール／再起動を実行できる。英語・日本語・简体中文に対応。自動でネットワークへ接続せず、利用者の操作で確認する。

更新情報は `https://github.com/matsushibadenki/Fonts/releases/latest/download/latest.json` からHTTPSで取得する。新しいバージョンのみを候補とし、同一リポジトリのRelease assetだけをダウンロードする。Tauriの公式updaterをRustから利用し、埋め込んだ公開鍵による署名検証と署名内のバージョン照合を必須にする。検証が終わるまでインストールできない。通信・取得・検証の失敗を「最新」と表示しない。

文書・更新候補・ダウンロードしたバイナリはRustが所有する。WebViewとの通信はバージョン・進捗・結果のみ。設定画面を閉じてもダウンロード状態は維持する。インストール直前に全タブの未保存変更と作業中の編集を確認し、一つでも残っていれば保存／終了を案内して停止する。ファイル操作・一括変換と競合する場合も停止する。更新中に変更を捨てる確認は出さない。インストール中の編集を遮断し、正常完了後にワークスペースを保存して再起動する。

初回公開はmacOS Apple Silicon版。従来の0.1.0はアップデーター未搭載のため、初回だけReleaseのDMGまたはアプリアーカイブからFonts.appをApplicationsへ配置する必要がある。macOSのコード署名・公証とは別の、Tauri更新パッケージ署名を使用する。Apple Developer IDによるコード署名・公証は未実施。

## 署名鍵

秘密鍵は `.release-secrets/updater.key` に0600権限で保存し、ディレクトリーは0700とする。Gitから除外しており、ソースや配布物には含めない。リポジトリとアプリに含めるのは公開鍵のみ。この秘密鍵を安全にバックアップすること。鍵を失うと、現在の公開鍵を持つアプリへ同じ系列の更新を提供できない。

2026-10-21、所有者の明示的な承認に基づき、matsushibadenki/Fonts のActions Secret `TAURI_SIGNING_PRIVATE_KEY`へ既存鍵を登録した。秘密鍵は署名ビルドのステップだけに渡す。公開鍵は0.1.1と共通で、既存アプリから更新できる。

## 次回の公開手順

1. `Cargo.toml`、`package.json`／lock、`src-tauri/tauri.conf.json`のバージョンを一致させ、`docs/releases/vX.Y.Z.md`を追加する。
2. Rustテスト・Clippy・TypeScriptを確認し、必要なら`python3 scripts/collect_licenses.py`で告知を更新する。
3. コードをmainへpushし、そのコミットへ`vX.Y.Z`タグを付けてpushする。`.github/workflows/release.yml`が自動起動する。失敗時は同じタグをworkflow_dispatchの`tag`に指定して再実行できる（Releaseが既に存在する場合は停止する）。
4. CIがタグ・全バージョン・リリースノート・mainへの所属を検証し、Rustテスト・Clippy・フロントエンドを確認する。macOS Apple Siliconアプリ・DMG・署名済み更新アーカイブをビルドする。
5. `prepare_release.py`が公開鍵で署名と署名内バージョンを独立検証し、改ざんデータの拒否も確認する。チェックサム・manifestを生成し、全ファイルをdraft Releaseへアップロードしてから正式公開する。失敗したdraftが残った場合は内容を調査してdraftを削除してから再実行する。公開済みReleaseの配布物は差し替えず、新しいバージョンで修正する。
6. Releaseの`latest.json`と配布物・SHA256SUMSを確認する。Apple Developer ID署名・公証はこの更新パッケージ署名とは別で、引き続き未実施。

`prepare_release.py`は現在のmacOSネイティブアーキテクチャの一つだけをmanifestに登録する。複数アーキテクチャを公開する場合は、各実機で署名した配布物を検証したうえでplatformsを統合してから公開する。

⭕️ [Pending] Windows/Linux・Intel Macの配布と実機更新、Appleのコード署名・公証。ダウンロード後のネットワーク切断、権限不足、全OSのインストーラー障害は継続検証対象。

## 公開・検証記録（2026-10-20）

🟢 [Done] [Fonts v0.1.1](https://github.com/matsushibadenki/Fonts/releases/tag/v0.1.1)をmacOS Apple Silicon向けに正式公開。DMG、署名済み更新アーカイブ、公開署名、latest.json、SHA256SUMSを配置した。署名と署名内バージョンを独立した検証プログラムで確認し、改ざんしたデータが拒否されることも確認した。

ローカルのRustテスト367件成功（重い任意検証5件は既存のignore）、Clippy `-D warnings`、TypeScript/Vite、リリースビルドが成功。GitHub Actionsでもテスト・Clippy・macOSビルドが成功した（ソース `e90c5c6`、[実行記録](https://github.com/matsushibadenki/Fonts/actions/runs/39397220197)）。

独立した0.1.0検証アプリを使い、日本語・英語・简体中文、GitHubからの0.1.1更新候補取得、実ダウンロード・署名検証、未保存文書でのインストール拒否、保存後のインストールと再起動を確認した。更新後のアプリ本体のInfo.plistと画面で0.1.1を確認し、検証文書の送り幅641が保存ファイルに残ることも確認した。

## 自動公開・検証記録（2026-10-21）

🟢 [Done] 所有者承認済みのActions Secretを使用し、`v0.1.2`タグのpushから[Fonts v0.1.2](https://github.com/matsushibadenki/Fonts/releases/tag/v0.1.2)の正式公開まで自動実行した。[ワークフロー実行](https://github.com/matsushibadenki/Fonts/actions/runs/39535508401)は全工程成功（ソース `b8b2966`）。Rustテスト367件成功・既存の任意検証5件ignore、リリース検査3件成功、整形・Clippy・フロントエンド・macOSアプリとDMGのビルド成功。更新署名・署名内バージョンの独立検証、改ざんデータの拒否を確認した。

公開後の全5アセットをGitHubから再取得し、アーカイブ・署名・DMG・manifestのSHA256SUMS一致、manifestのバージョン・配布URL・署名と、最新Releaseが0.1.2であることを確認した。0.1.2へのGUI経由インストールは今回再実施していない（0.1.1で実機検証済み）。Intel・Windows・LinuxとAppleコード署名・公証は引き続き⭕️ [Pending]。

## Apple Developer ID署名・公証の自動化（2026-10-21）

🟢 [Done] リリースワークフローにDeveloper ID Application署名・Hardened Runtime・セキュアタイムスタンプ、公証、チケット添付（staple）を接続した。Tauriによるapp公証後、最終DMGも署名・公証・stapleする。appと更新アーカイブ内appの署名・Team ID・公証チケット・Gatekeeper、DMGの署名・チケット・Gatekeeperを検証してから更新署名検証とRelease公開へ進む。一時キーチェーンとAPIキーは終了時に削除する。いずれかが失敗・未設定なら公開しない。

必要なリポジトリActions Secrets:

| Secret | 内容 |
| --- | --- |
| `APPLE_CERTIFICATE` | 秘密鍵付きDeveloper ID Application証明書（.p12）のbase64。`openssl base64 -A -in certificate.p12`の結果を登録 |
| `APPLE_CERTIFICATE_PASSWORD` | .p12のエクスポートパスワード |
| `APPLE_SIGNING_IDENTITY` | `Developer ID Application: 名前 (TEAMID)`の完全な署名名 |
| `APPLE_TEAM_ID` | 証明書に対応する10文字のTeam ID |
| `APPLE_API_ISSUER` | App Store ConnectのチームAPIキーのIssuer ID |
| `APPLE_API_KEY` | APIキーのKey ID |
| `APPLE_API_PRIVATE_KEY` | 対応する.p8の内容（base64化しない） |

既存の`TAURI_SIGNING_PRIVATE_KEY`も継続使用する。秘密値をソースやチャットへ貼らず、[GitHub Actions Secrets設定](https://github.com/matsushibadenki/Fonts/settings/secrets/actions)へ登録する。Apple Developer Programの配布証明書と公証認証が必要。設定方法は[Tauri公式ドキュメント](https://v2.tauri.app/distribute/sign/macos/)を参照。

⭕️ [Pending] 現在のMacには有効な署名identityがなく、GitHubにはApple用Secretsが未登録。このため実際のApple署名・公証・公証済みRelease公開は認証情報の準備待ち。既存0.1.2を差し替えず、準備後はバージョンを上げて新しいタグで公開する。

## Apple ID方式への接続（2026-10-29）

🟢 [Done] 公証はAPIキーに加え`APPLE_ID`・`APPLE_PASSWORD`（アプリ用パスワード）・`APPLE_TEAM_ID`でも実行できる。両方式が揃っている場合はApple ID方式を使う。API方式の3項目はApple ID方式では不要。証明書用の3項目`APPLE_CERTIFICATE`・`APPLE_CERTIFICATE_PASSWORD`・`APPLE_SIGNING_IDENTITY`と既存の更新署名鍵はどちらの方式でも必要。`Fonts`というアプリ用パスワードのラベルを署名identityには使わない。

前回のキーチェーン検査はサンドボックス内で行ったため、identityなしという結果になっていた。サンドボックス外で確認したところ、指定Teamに対応するDeveloper ID Application identityが存在した。証明書がないという以前の記載はこの検査結果で訂正する。Secretsの有無とローカル証明書の有無は別に確認する。

## Developer ID署名・公証済み公開の検証（2026-10-29）

🟢 [Done] 指定TeamのDeveloper ID Application証明書を限定して暗号化p12へ一時書き出し、証明書・ランダムなp12パスワード・署名identityとApple ID公証用情報をリポジトリActions Secretsへ登録した。一時p12は削除済み。APIキーは不要。

[自動実行](https://github.com/matsushibadenki/Fonts/actions/runs/40835383183)でテスト・Clippy・Developer ID署名・app公証とstaple・DMG署名と公証とstaple・Gatekeeper・更新署名検証が全工程成功し、[Fonts v0.1.3](https://github.com/matsushibadenki/Fonts/releases/tag/v0.1.3)を正式公開した（ソース `cffcc7e`）。公開後に配布物を再取得し、SHA256SUMSの全件一致、指定Teamの署名・Hardened Runtime・タイムスタンプ、app/DMGの公証チケットを確認。このMacでも両方のGatekeeper結果が`accepted`・`source=Notarized Developer ID`となった。更新署名と署名内バージョン0.1.3、および改ざんデータの拒否もCIで確認した。

Apple署名・公証の以前の⭕️ [Pending]はこの実検証で🟢 [Done]へ更新する。Intel・Windows・Linuxの配布検証は引き続き⭕️ [Pending]。今回GUI経由の更新インストールは再実施していない。
