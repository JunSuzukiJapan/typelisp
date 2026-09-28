# typelisp ドキュメント（日本語）

typelisp は静的型付きの Lisp です。インストールとビルドの方法はリポジトリ直下の
[README_JP.md](../../README_JP.md) を参照してください。

## チュートリアル

はじめての人は上から順に読んでください。

- [入門](tutorial/intro.md)：REPL、関数、変数、条件分岐、繰り返し、リストと `Vector`
- [型の基本](tutorial/types.md)：静的型、`Option`、`Result`、構造体、列挙型、ジェネリクス
- [トレイト](tutorial/traits.md)：`deftrait` / `impl`、トレイト境界、`:dyn`
- [マクロ](tutorial/macros.md)：`defmacro`、準クオート、`gensym`、`macrolet`
- [エラー処理](tutorial/errors.md)：`Result`、`panic`、`catch` / `throw`、`unwind-protect`
- [並行処理](tutorial/concurrency.md)：タスク、チャネル、`select`、`Mutex`、`thread`

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
