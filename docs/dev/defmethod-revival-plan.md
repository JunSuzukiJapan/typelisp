# defmethod の復活 — 実装計画

作成: 2026-11-07

## 1. 決まったこと（2026-11-07、ユーザー決定）

2026-10-10 のコンストラクタ再設計（[constructors-redesign.md](constructors-redesign.md)）は
イニシャライザの変更のつもりだったが、`defmethod` の廃止まで広がっていた。これを戻し、
メソッドの書き方を次のように決め直す。

- **`impl` の中のメソッドは `defmethod` で書く**。公開するものだけ `pub` を付ける。

  ```lisp
  (impl dog
    (pub defmethod hello (self (name string)) string
      (string+ "Hello, " name))
    (defmethod helper (self) int 1))
  ```

  - 第 1 引数が裸の `self` ならインスタンスメソッド、`self` が無ければ静的メソッド
    （`(Type::name args...)`）。これは今と同じ。
  - `self` 以外の引数には型を書く。
- **トレイトの `impl` の項目も `deftrait` のメソッド宣言も `defmethod` で書く**。

  ```lisp
  (deftrait Drawable ()
    (defmethod draw (self) string))
  (impl Drawable circle
    (defmethod draw (self) string "circle"))
  ```
- **今の項目の形（`(name (self ...) Ret ...)`、`(pub name ...)`）は廃止してエラーにする**。
- **`impl` の外にも `defmethod` を書ける**。`impl` の中のものとの違いは、第 1 引数に `self` を
  取れないことだけ。第 1 引数の型のメソッドになり、メソッドは第 1 引数の型だけから探すので、
  同じ名前を第 1 引数の型ごとに定義できる。

  ```lisp
  (defmethod some-method ((x int) (y int)) int (+ x y))
  (defmethod some-method ((x f64) (y f64)) f64 (* x y))
  ```

  - **どの型にでも付けられる**。`int`・`Vector` のような標準ライブラリの型や、別のモジュールの型も
    含む（固有 `impl` は今どおり、型を定義した側だけが書ける）。
  - `pub defmethod` にしたものだけ、ほかのモジュールから使える。
- 文書は全言語（日本語の原本と 20 言語の訳）を今回直す。

## 2. 実装

- チェッカー（`crates/typelisp-front/src/check/analyze.rs`）
  - `impl`/`deftrait` の項目の入口（固有 `impl`、トレイトの `impl`、ブランケット `impl`、
    `deftrait`）で `(defmethod ...)`／`(pub defmethod ...)` を剥がし、今の内部形に渡す。
    それ以外の形の項目は断る。
  - トップレベルと `(module ...)` の中の `defmethod`／`pub defmethod` を
    `check_defmethod_in` につなぐ。受け手は第 1 引数 `(name Type)`。`self` は断る。
- リーダ（`crates/typelisp-read/src/reader.rs`）の `impl_item_name`/`member_name` が
  `(defmethod name ...)` を読めるようにする。
- 標準ライブラリ（prelude）・テスト・examples・projects・文書・エディタ定義の `impl` と
  `deftrait` の項目を新しい形に書き換える。prelude を再生成する。
