# CFF除去と自動ヒンティングのエンジン

Fonts is based in part on the work of the FreeType Team and the ttfautohint library.

## CFFの除去

ライブラリのヒンティングアイコンで「ヒントを除去」を選ぶ。OpenType CFF1／CID-CFF／可変CFF2と、単体CFF／CID-CFFに対応する。

HarfBuzz 14.6.0のNO_HINTING処理を使用し、stem命令、hintmask／cntrmask、Private DICTのBlueValues・StemSnap等を取り除く。局所／大域サブルーチンやCIDのFDArray／FDSelectはエンジンが解析・再構築する。全グリフを保持し、変更済みのCFFテーブルだけを元のSFNTへ戻すため、組版・メトリクス・カラー・可変テーブルのバイト列を維持する。DSIGは除去しheadのチェックサム調整値は更新する。

出力時に全グリフの輪郭、送り幅、グリフ名・CID、Unicode対応を元データと比較する。単体CFFは一時SFNTへ格納して処理後に取り出し、FreeTypeで読み戻した文書の一致を確認する。

## 自動生成

- ライブラリ：静的・単色のTrueTypeを選び、ヒンティングアイコンから「自動生成（TrueType）」を選ぶ。
- メイン出力：TTFまたはTrueType輪郭のWOFF2を選び、ヒンティングを「自動生成（TrueType）」にする。既定はヒントなし。

ttfautohint 1.8.4をRustのworkerからライブラリとして呼び、8〜50ppem用のTrueTypeバイトコードを生成する。既存の命令は先に除去し、生成後は輪郭・送り幅・Unicode対応と命令の存在を検証する。ttfautohintが必要とする補助グリフの追加は認める。SFNTのpaddingとディレクトリを正規化してからWOFF2へ格納し、生成したプログラムをそのまま保持する。

可変・カラー・CFFの元書体に対する直接の自動生成は対象外。可変フォントは既存の静的インスタンスを編集用文書へ開いた後、メインからTrueTypeとして生成できる。CFF／Type 1の輪郭も編集用コピーからTrueTypeとして生成できる。CFF用stemヒントの生成や可変ヒントの生成とは異なる。輪郭のない書体はエラーにし、フォント側の変更制限を無視するオプションは使わない。

自動ヒントは手動調整を置き換える品質保証ではない。OSのラスタライザーによって結果は異なる。書き出し時の処理であり、Rust/wgpuの制作画面の輪郭やDocumentは変更しない。

## 組み込み・ライセンス

- ソース・出典・SHA-256：[vendor/font-hinting/manifest.json](../vendor/font-hinting/manifest.json)。
- HarfBuzzはOld MITとファイルごとの条件。ttfautohintはFTLを選択し、生成命令に関する上流の例外も保持する。BSD／X11の補助コード、Bisonパーサー骨格の例外も第三者告知へ収録する。
- `font-hinting-native` はC++17／Cで静的にビルドし、既存の同梱FreeTypeを再利用する。外部コマンド、システム側のHarfBuzz／FreeType、FontForge、Pythonはアプリの実行時には不要。
- ライブラリ呼び出しは直列化し、返却バッファは割り当て元のC側で解放する。入力と出力は256 MiBまで。フォントやフレームはWebViewへ渡さない。

## 検証

- 🟢 [Done] 自作CFF1／CID-CFF／CFF2のstem、mask、サブルーチン、Privateパラメーターの除去。CID 42、他テーブルの一致、CFF2の軸100／400／900での輪郭・メトリクス一致、単体CFFの除去を自動テスト。
- 🟢 [Done] TrueType命令生成、再生成、WOFF2での命令保持、空書体・カラー／可変の拒否、変更制限の尊重を自動テスト。
- 🟢 [Done] 生成したTrueTypeを8／12／16／24／50pxで、FreeTypeのNO_AUTOHINT＋PEDANTICを付けて描画。命令自体が実行できることを確認。
- 🟢 [Done] 最終構成のRustワークスペース351件成功、GPU4件成功、全targetのClippy `-D warnings`、TypeScript／Vite、整形・差分検査が成功。通常版macOS Fonts.appのビルド成功（96.62 MiB）。
- 🟢 [Done] 独立したmacOSアプリでメインの自動生成TTF／WOFF2、ライブラリのCFF除去を操作・保存・目視確認。FontToolsでCFFのstem／mask／Private設定の除去と、TTF／WOFF2に同じグリフ命令51 B・fpgm 3,596 Bが含まれることを独立確認した。CFF出力は760 B、TTFは6,120 B、WOFF2は2,604 B。
- 🟢 [Done] WOFF2テストの初回失敗はttfautohint出力のSFNT paddingと既存null-transform検証の整合が原因。出力SFNTを再構築・正規化し、再生成とWOFF2の読み戻しを含む全テストの成功を確認した。
- 🟢 [Done] ライセンス一覧は523 Rust／81 npmとネイティブ2エンジンを記録。lockfile SHA-256・配布用一覧との一致を検証。動的リンク一覧に外部HarfBuzz／FreeType／ttfautohint依存がないことも確認した。
- ⭕️ [Pending] Windows/Linux実機、DirectWrite／外部アプリでの表示品質と広範な実フォントの追加検証。

## 公式資料

- [ttfautohint公式とライセンス](https://freetype.org/ttfautohint/index.html)、[ライブラリAPI・対応範囲](https://freetype.org/ttfautohint/doc/ttfautohint.html)
- [HarfBuzz 14.6.0](https://github.com/harfbuzz/harfbuzz/releases/tag/14.6.0)、配布ソースの `hb-subset.h`・`hb-subset-cff1.cc`・`hb-subset-cff2.cc`
