# typelisp ドキュメント（日本語）

typelisp は静的型付きの Lisp です。インストールとビルドの方法はリポジトリ直下の
[README_JP.md](../../README_JP.md) を参照してください。

## ガイド

- [モジュールとファイル構成](guide/modules.md)：`use`、`pub`、ファイルとモジュールの対応
- [コンパイル](guide/compile.md)：JIT、AOT で実行ファイルを作る方法、ダンプ
- [ファイル I/O、ストリーム、ネットワーク](guide/io.md)：ファイル、パス名、TCP / TLS / UDP、名前解決
- [C FFI](guide/ffi.md)：`defffi` で C の関数を呼ぶ（コールバック、`def-c-struct` による C の構造体を含む）
- [エディタ連携](guide/editors.md)：`typl-lsp` と VS Code / Emacs の設定
- [Common Lisp から来た人へ](guide/from-common-lisp.md)：CL との違いと書き換え方

## リファレンス

- [構文リファレンス](reference/syntax.md)：字句、型の書き方、定義、制御構文、コンパイル、並行機構
- [組み込み関数](reference/functions/README.md)：組み込み関数・メソッド・標準ライブラリ
- [型の一覧](reference/types.md)：型と、各型が実装しているトレイト
- [エラーメッセージ](reference/errors.md)：主なエラーの意味と直し方

## エディタ連携

設定の手順は [エディタ連携ガイド](guide/editors.md) にあります。各エディタのキー操作と設定の一覧は
次の文書にあります。

- [Emacs（typelisp-mode）](../../editor/emacs/README.md)
- [VS Code](../../editor/vscode/README.md)
