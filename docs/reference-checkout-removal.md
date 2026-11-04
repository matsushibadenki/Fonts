# 参照checkoutの削除・独立性検証（2026-11-04）

## 削除結果

- 🟢 [Done] プロジェクト直下の`fontforge/`（約184 MiB）を削除。
- 🟢 [Done] プロジェクト直下の`LumaPaint`シンボリックリンクを削除。参照先`/Users/Shared/Program/Tauri/LumaPaint`は保持。
- 🟢 [Done] 必要なcore・ネイティブ描画補助・フォントエンジンは製品内にコピー／同梱済み。流用元の記録、ハッシュ、ライセンス告知を保持。

FontForgeは調査用cloneで、製品の実行・変換エンジンとして使っていない。削除時のcommitは`c5ee1d79180e3f8926acc727f7627a639ded91ea`。削除直前の追跡・未追跡ファイルに変更はなく、無視されたファイルは`.DS_Store`だけだった。LumaPaintとのクリップボード互換処理は製品内に実装されている。

## 削除前の検証

元の参照ディレクトリ、シンボリックリンク、`.git`、既存`target`を含めない一時ディレクトリへ、2518ファイルとローカルのNode依存をコピー。Node依存のリンクもコピー内で完結することを確認した。`CARGO_TARGET_DIR`は一時コピー内の空の`target`に指定し、既存のビルド成果物を再利用せずに実行した。通常のCargoレジストリキャッシュは使用する。

- 🟢 [Done] `cargo test --locked --workspace --offline`: 413件成功、任意検証8件はignore。
- 🟢 [Done] `npm run build`: TypeScript／Vite成功。
- 🟢 [Done] `npm run tauri -- build --debug --bundles app --config '{"bundle":{"createUpdaterArtifacts":false}}'`: 独立コピー内からmacOSアプリ生成成功（122.99 MiB）。
- 🟢 [Done] `otool -L`でFontForge／LumaPaint checkout内のライブラリを参照していないことを確認。
- 🟢 [Done] 両方のパスを削除後、元の作業ディレクトリでも独立性検査成功。LumaPaint本体の存在も確認。
- ⭕️ [Pending] Windows/Linux実機のビルド・実行確認。

## 継続検査

Python 3.11以降で、依存の解決後に実行する。

```sh
python3 scripts/check_self_contained.py
```

この検査は、削除した参照パスの再出現、製品・Node依存の外部リンク、npmのローカル参照、バンドルリソースの外部参照、Cargoのローカル依存・patch／replaceのプロジェクト外参照を検査する。現在のローカルCargoパッケージは6件。外部リンクを一時的に混入すると検査が拒否することも確認した。

Cargo metadataには`--no-deps --locked --offline`を指定する。CIで他OS向けのレジストリソースを追加ダウンロードせずに、workspaceとローカル依存を調べる。一般のレジストリ依存・OS標準ライブラリやユーザーのフォント素材は、この参照checkout検査とは別に扱う。

GitHub ActionsのmacOS検証へ同じ検査を追加した。GitHub上での実行結果は、この変更をpushしてから確認する。一時コピーのビルド成果物は検証後に削除し、検証用コピーへの新たな依存も残さない。
