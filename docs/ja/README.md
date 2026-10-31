# typelisp ドキュメント（日本語）

typelisp は静的型付きの Lisp です。インストールとビルドの方法はリポジトリ直下の
[README_JP.md](../../README_JP.md) を参照してください。

## 特徴

### タスクとチャネル

タスク（軽量スレッド）を起動して、チャネルで値を受け渡します。詳しくは [並行処理](tutorial/concurrency.md) を
参照してください。

```lisp
(use std::sync::Chan)
(defun worker ((name string) (jobs Chan<int>) (results Chan<string>)) ()
  (doiter (n jobs)
    (sleep 0.1)
    (send results (format false "~a: ~d * ~d = ~d" name n n (* n n)))))

(defun main () ()
  (let ((jobs (the Chan<int> (Chan::new 0)))
        (results (the Chan<string> (Chan::new 10))))
    (task (worker "A" jobs results))
    (task (worker "B" jobs results))
    (dotimes (i 4)
      (send jobs (+ i 1)))
    (close jobs)
    (dotimes (i 4)
      (println "~a" (unwrap (recv results))))))

(main)
```

### ジェネリック

型や関数の名前に山括弧で型パラメータを書きます。トレイト境界は `:where` 節に書きます。詳しくは
[型の基本](tutorial/types.md) と [トレイト](tutorial/traits.md) を参照してください。

```lisp
(defstruct Pair<A,B> (first A) (second B))

(impl Pair<A,B>
  (swap (self) Pair<B,A>
    (make-instance 'Pair :first self::second :second self::first)))

(defun largest<T> ((xs Vector<T>)) Option<T> (:where (Ord T))
  (let ((best (the Option<T> (Option::none))))
    (doiter (x (iter xs))
      (setf best (Option::some (match best
                                 ((some b) (if (> x b) x b))
                                 ((none) x)))))
    best))

(let ((p (swap (make-instance 'Pair :first 1 :second "one"))))
  (println "~a ~a" p::first p::second))                       ; one 1

(println "~a" (largest (the Vector<int> #(3 9 4))))           ; (some 9)
(println "~a" (largest (the Vector<string> #("pear" "fig")))) ; (some pear)
```

### nil の代わりに Option と Result

`nil` はありません。値が無いかもしれないときは `Option<T>`、失敗するかもしれないときは
`Result<T,E>` を返し、`match` で中身を取り出します。`Option<string>` を `string` として使ったり、
`match` で `none` の場合を書き忘れたりすると、実行する前に型エラーになります。詳しくは
[型の基本](tutorial/types.md) と [エラー処理](tutorial/errors.md) を参照してください。

```lisp
(use std::num::parse-int)
(defun find-user ((id int)) Option<string>
  (case id
    (1 (Option::some "alice"))
    (2 (Option::some "bob"))
    (:else (Option::none))))

(defun parse-port ((s string)) Result<int,string>
  (match (parse-int s)
    ((ok n) (if (<= 1 n 65535)
                (Result::ok n)
                (Result::err (format false "port out of range: ~a" n))))
    ((err e) (Result::err (message e)))))

(match (find-user 3)
  ((some name) (println "user: ~a" name))
  ((none) (println "not found")))        ; not found

(match (parse-port "99999")
  ((ok p) (println "port ~a" p))
  ((err m) (println "error: ~a" m)))     ; error: port out of range: 99999
```

### `->` で処理をつなげる

`->` マクロは、値を次の式の第 1 引数として順に渡します。`(-> x (f a) (g b))` は
`(g (f x a) b)` と同じです。`Option` や `Result` を返す処理を、上から順に書けます。詳しくは
[Option と Result、エラー型](reference/functions/option-result.md) を参照してください。

```lisp
(use std::num::parse-int)
(defun half ((n int)) Option<int>
  (if (= 0 (mod n 2))
      (Option::some (truncate n 2))
      (Option::none)))

(println "~a"
  (-> (Option::some 40)
      (and-then half)                          ; (some 20)
      (and-then half)                          ; (some 10)
      (map (lambda ((x int)) int (+ x 1)))     ; (some 11)
      (unwrap-or 0)))                          ; 11

(println "~a"
  (-> (parse-int "abc")
      (map (lambda ((n int)) int (* n 2)))
      (unwrap-or -1)))                         ; -1
```

### 型検査されるマクロ

マクロは Common Lisp と同じ `defmacro`、準クオート、`gensym` で書けます。展開した結果も
普通のコードと同じように型検査され、型に合わなければマクロを呼んだ場所でエラーになります。
詳しくは [マクロ](tutorial/macros.md) を参照してください。

```lisp
(defmacro swap (a b)
  (let ((tmp (gensym "tmp")))
    `(let ((,tmp ,a))
       (setf ,a ,b)
       (setf ,b ,tmp))))

(let ((x 1) (y 2))
  (swap x y)
  (println "x=~a y=~a" x y))             ; x=2 y=1
```

```lisp
(defmacro add-a (x) `(+ ,x "a"))
(add-a 1)
;; error: type error: type mismatch: expected `int`, found `string`
```

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

- [Emacs（typelisp-mode）](../../editor/emacs/README_JP.md)
- [VS Code](../../editor/vscode/README_JP.md)
