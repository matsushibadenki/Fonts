# OSフォントライブラリ

入出力形式は[font-formats.md](font-formats.md)の段階で拡張済み。以下の「静的TTFのみ」「CFF OTF出力は未実装」は初期段階の説明として扱い、現在の形式別の対応は対応表を参照。

「ファイル → フォントライブラリ…」、ライブラリアイコン、または Cmd/Ctrl+Shift+F で開く。英語・日本語・简体中文に対応。

## 書体名の表示言語

日本語UIでは、フォント内の日本語ファミリー名を優先表示する。日本語名がない場合は従来の英語名を維持する。英語・简体中文UIでは従来の代表名を表示する。言語切替は読み込み済みの一覧・チェックボックスの名前・選択中の書体名にも即時反映され、再スキャンは不要。

日本語のfamily名（name ID 1）を優先し、なければtypographic／WWS名、フルネームの順で補う。スタイル名も日本語のname ID 2／17／22を優先し、W3・W6などを区別する。不正・空の名前、スタイル名だけのレコードは書体名として使用しない。プレビューのフォント指定、変換・出力ファイル名、検索用の各言語の別名は元の情報を保持する。

2026-10-17: 関連Rustテスト5件、Clippy `-D warnings`、TypeScript/Vite、通常版macOSアプリのビルド成功。実アプリでシステム／Adobeのヒラギノが日本語名・W3／W6の区別付きで表示され、Arialは英語名のままであることを確認した。日本語名なし、不正レコードからの補完、表示用の名前と元の代表名の分離を自動テストした。

## 日本語名での検索

フォントのnameテーブルに格納された各言語のファミリー名・スタイル名・フルネーム・PostScript名・typographic／WWS名を検索対象にする。表示が英語名の書体も、内部に日本語名があればその名前で検索できる。内部の代表名・ファイル名・フォントデータは変更せず、一覧の表示名だけをUI言語に合わせる。

検索語と名前はUnicode NFKC＋小文字化で正規化する。全角／半角カナ・英字、合成／分解された濁点の違いを吸収し、空白区切りの複数語にも対応する。例えば「かな書体 Ｗ３」と「かな書体 w3」、「カナゴシック」と「ｶﾅｺﾞｼｯｸ」は同じ検索になる。ひらがな・カタカナの相互変換や、フォントに含まれない別名の推測は行わない。

Unicode UTF-16BEに加え、旧Mac日本語／Windows Shift-JISの名前を読み取る。不正な名前データはそのレコードだけ無視する。検索用indexはRust側でスキャン時に作成し、WebViewへ送信しない。「Fontsでインストール」の一覧も同じ正規化でファミリー名を検索する。

追加依存はunicode-normalization 0.1.26とencoding_rs 0.8.42（公式crates.ioの安定版を確認し固定）。日本語・简体中文・英語の別名を持つ合成フォントで検索、旧形式・不正データ、表示名と元ファイルの不変性を回帰テストした。

2026-10-17: 通常Rustテスト235件、Clippy `-D warnings`、TypeScript/Vite、通常版・検証用macOSアプリのビルドが成功。実アプリでは「ヒラギノ角ゴ ProN」で英語表示のシステム／Adobe書体4件を検索し、「ﾋﾗｷﾞﾉ角ｺﾞ ＰｒｏＮ」でも同じ4件が表示されることを確認した。

## 出力操作

ライブラリ下部は選択中のフォント専用。元ファイルの出力、出力形式、単体変換、フォルダーへの一括変換を同じ操作列に配置する。単一選択では元ファイル出力・単体変換、複数選択では一括変換を利用できる。「制作中のフォントを出力」と出力名の入力はメインウインドウに集約した。「TrueTypeとしてインストール」はライブラリ上部の再スキャン直前に配置し、制作中の文書のファミリー名でインストールする。

2026-10-17: macOS独立検証アプリで、メイン側の制作中フォント出力を保持したこと、ライブラリ側からの削除、インストールの位置、単一／複数選択に応じた有効化、選択フォントからのOpenType CFF保存ダイアログを確認。保存はキャンセルし既存フォントは変更していない。

## 保存場所

| OS | 用途 | ディレクトリ |
| --- | --- | --- |
| Windows | 全ユーザー | `%WINDIR%\Fonts`（通常 `C:\Windows\Fonts`） |
| Windows | 現在のユーザー | `%LOCALAPPDATA%\Microsoft\Windows\Fonts` |
| Windows | Adobe | `%APPDATA%\Adobe\CoreSync\plugins\livetype` |
| macOS | OS標準 | `/System/Library/Fonts` |
| macOS | 全ユーザー | `/Library/Fonts` |
| macOS | 現在のユーザー | `~/Library/Fonts` |
| macOS | 追加ダウンロード候補 | `/System/Library/AssetsV2/com_apple_MobileAsset_Font7`（存在時のみ。内部パスの互換性は保証しない） |
| macOS | Adobe | `~/Library/Application Support/Adobe/CoreSync/plugins/livetype` |
| Linux | 全ユーザー | `/usr/share/fonts`、`/usr/local/share/fonts`、`$XDG_DATA_DIRS` 配下の `fonts` |
| Linux | 現在のユーザー | `$XDG_DATA_HOME/fonts`、未設定なら `~/.local/share/fonts`。旧 `~/.fonts` も読込 |
| Linux | Fontconfig設定 | `fc-list --format '%{file}\n'` で得た追加フォントのディレクトリ |

ユーザー名の固定文字列は使わず、HOMEはdirsクレート、WindowsのAPPDATA/LOCALAPPDATA/WINDIRとLinuxのXDGは絶対パスの環境変数から解決する。LinuxにAdobe Fontsの標準キャッシュは設定しない。Windows/macOSのAdobeパスはユーザー指定の候補であり、Adobe公開APIによる安定した保存先ではない。

ディレクトリ単位で未検出・読取不可・一部のみを表示する。隠しファイルを含め、拡張子だけではなくSFNT/TTCシグネチャで判定する。認識できないキャッシュの復号や変換は行わない。リンク先が探索ルート外のファイル・ディレクトリは自動探索しない。見つからない個別ファイルはファイル選択から明示的に追加できる。

## インポート・エクスポート

- 🟢 [Done] TTF、OTF（CFF/CFF2）、TTC/OTCのフェイス一覧。ファミリー・スタイル・保存元・パスの検索と64件単位の表示。
- 🟢 [Done] 選んだフェイスの輪郭、送り幅、UPM、上下端、Unicode cmapをRust Documentへインポート。複合グリフの輪郭は展開する。元ファイルは保持し、ファミリーに` Edited`を付けて別プロジェクトとして開く。
- 🟢 [Done] 元のファイルの書き出しはバイト列をそのまま複製する。コレクションはフェイス単体ではなくコレクション全体を保持する。
- 🟢 [Done] 制作中のDocumentから新しい静的TTFを生成。Unicode cmap format 12、BMP互換のformat 4、glyf/loca/hmtx/head/hhea/maxp/name/OS2/post、チェックサムを生成。疎なGlyphIdは出力時に連番へ変換する。
- 🟢 [Done] 3次曲線は適応分割して2次曲線に変換する。設計単位で近似した後、TTF整数座標へ丸める。開いた輪郭、範囲外の座標、複雑すぎる輪郭はエラーにし、元文書と既存の出力ファイルを保持する。
- 🔴 [Later] 元のヒンティング、GSUB/GPOS、カーニング、可変軸、カラー、元の名前・ライセンス等のメタデータを保持する編集。現段階の編集用インポートは輪郭と基本メトリクスのみであり、完全なラウンドトリップではない。ビットマップ専用・SVG専用フォントは編集不可。
- 🔴 [Later] CFF OTF/WOFF/WOFF2出力、旧FON/dfont形式、可変インスタンスの指定。

フォントファイルの読込上限は256 MiB、インポートは文書全体1600万セグメント、1グリフ65536セグメント、4096輪郭まで。JSONプロジェクトの保存・読込上限は512 MiB。スキャンは10万ディレクトリエントリ、深さ16、約2万フェイスで打ち切り、表示上限到達を表示する。これらの上限はUIスレッドやGPUではなくRustのワーカーで扱う。

## インストールと管理

- 🟢 [Done] Documentから出力したフォントをアプリ専用アーカイブに保管し、ユーザー用フォントフォルダーへ追加する。
- 🟢 [Done] macOSはCoreTextのユーザースコープ、WindowsはHKCUのFonts登録とAddFontResourceExW/WM_FONTCHANGE、Linuxはfc-cacheで反映する。管理者権限によるシステムフォルダーへの書込は行わない。
- 🟢 [Done] アプリが追加したフォントは有効・無効を切替可能。無効化してもアーカイブを残す。同じ内容はSHA-256で重複排除し、インストール先が外部で変更された場合は変更ファイルを消さずにエラーを返す。登録失敗時は作成したファイルを戻す。
- 🟢 [Done] OS管理ツールを開く（macOS Font Book、Windows Fonts設定、Linux font-manager／未導入ならユーザーフォルダー）。Adobe Fontsは公式の管理ページを開く。
- 🟢 [Done] 元フォントの保存場所、OS標準フォルダー、Adobeの同期フォルダーは読取元とし、書き出し先としては保護する。自動登録は専用のインストール操作から行う。
- ⭕️ [Pending] Windows/Linuxのビルドと実機での検出・登録・再ログイン後の持続性、macOSの標準ユーザーフォルダーでのGUIインストール最終確認。一時ディレクトリの固有名テストフォントでは、実際のCoreTextユーザースコープへの登録・解除が成功した。既存フォントは変更していない。
- ⭕️ [Pending] このタスク以前からWindows/Linuxのネイティブwgpu canvas接続は未実装。フォント管理と変換は共通Rustコードだが、各OSのアプリ全体の動作確認とは区別する。

Adobeの有効化状態はフォルダー内のファイルが存在するだけでは判定しない。Creative Cloudの内部データ・有効化状態を書き換えず、個々のフォントの利用条件に従う。公開・配布や外部サービスへのアップロードは本機能では行わない。

## 検証と参考資料

macOSの読取確認では3,083フェイス（Adobe候補990フェイス）を検出。1000×720の別検証アプリで保存元フィルター、ファイル選択、フェイス選択、編集用読込、Unicodeを保持したTTF保存を確認。合成フォントとCourier.ttcの先頭フェイス（1343グリフ）を変換し、ttf-parser・FontToolsの全テーブル読込／再保存・CoreTextのdescriptor作成で検証した。既存フォントやAdobeキャッシュは変更していない。

- [Windows: Font Installation and Deletion](https://learn.microsoft.com/en-us/windows/win32/gdi/font-installation-and-deletion)
- [Windows: ユーザー別インストール先に関するMicrosoft回答](https://learn.microsoft.com/en-ie/answers/questions/1657216/w11-fonts-instal-for-all-users-run-as-admin)
- [Windows: WM_FONTCHANGE](https://learn.microsoft.com/en-us/windows/win32/gdi/wm-fontchange)
- [Apple: Font Bookの保存場所設定](https://support.apple.com/en-my/guide/font-book/fntbk1004/mac)
- [Apple: macOS標準フォントの場所](https://support.apple.com/en-ie/108939)
- [Apple: CoreTextの登録スコープ](https://developer.apple.com/documentation/coretext/ctfontmanagerscope)
- [Fontconfig: 設定・XDGフォントディレクトリ](https://fontconfig.pages.freedesktop.org/fontconfig/fontconfig-user.html)
- [Adobe Fonts: ライセンスFAQ](https://helpx.adobe.com/fonts/web/font-licensing/font-licensing.html)
- [OpenType: ファイル構造](https://learn.microsoft.com/en-us/typography/opentype/spec/otff)、[cmap](https://learn.microsoft.com/en-us/typography/opentype/spec/cmap)、[glyf](https://learn.microsoft.com/en-us/typography/opentype/spec/glyf)

フォント制作に関する構造はローカルFontForgeの`encoding.c`と`tottf.c`も参照した。新しい実装はfont-coreとsrc-tauri内にあり、FontForgeやLumaPaintの外部ディレクトリを実行・参照せず動作する。追加依存はttf-parser（MIT/Apache-2.0）、dirs、sha2、Windows用winreg/windows-sys。Cargo.lockで固定する。

検証結果: 通常のRustテスト194件成功、macOSのCoreText登録・解除テスト1件も明示実行して成功。Clippy（警告をエラー扱い）、TypeScript/Vite、macOSアプリ生成を確認。コンソールのブラウザー検査およびWindows/Linuxの実機画面は未検証。
