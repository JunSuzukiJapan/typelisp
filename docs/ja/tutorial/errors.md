# エラー処理

typelisp のエラー処理は、失敗を 2 種類に分けて考えます。

| 失敗の種類 | 例 | 表し方 |
|---|---|---|
| 起こりうる失敗（回復できる） | ファイルが無い、入力が数でない | `Result<T,E>` を返す |
| プログラムの誤り（回復できない） | 範囲外の添字、`none` の `unwrap`、0 での割り算 | `panic` で止まる |

これに加えて、関数を何段もまたいで一気に抜け出す `catch` / `throw` と、どう抜けても後始末を
実行する `unwind-protect` があります。[型の基本](types.md) の `Result` の節を読んでいることを
前提にします。

## 1. `Result` を返して、`match` で受ける

ポート番号を文字列から読み取る関数を書きます。数でない場合と、範囲外の場合の 2 通りの失敗が
あります。

```lisp
(defun parse-port ((s string)) Result<int,string>
  (match (parse-int s)
    ((ok n) (if (and (>= n 1) (<= n 65535))
                (Result::ok n)
                (Result::err (format false "port out of range: ~a" n))))
    ((err e) (Result::err (message e)))))
```

呼び出し側は `match` で成功と失敗を分けます。

```lisp
(match (parse-port "99999")
  ((ok p) (println "port ~a" p))
  ((err m) (println "error: ~a" m)))
;; error: port out of range: 99999
```

- `Result` を返す関数の値は、`match` で `err` の場合を書かなければ使えません。失敗の扱い忘れは
  型エラーになります。
- `parse-int` のエラーは `ParseIntError` 型の値です。`(message e)` でメッセージの文字列を取り出せます。

## 2. 失敗を呼び出し元へ伝える

Rust の `?` のような省略構文はありません。`Result` を返す関数を続けて呼ぶときは、失敗したら
そのまま返す処理を `match` で書きます。

```lisp
(defun add-ports ((a string) (b string)) Result<int,string>
  (match (parse-port a)
    ((err m) (Result::err m))
    ((ok x) (match (parse-port b)
              ((err m) (Result::err m))
              ((ok y) (Result::ok (+ x y)))))))

(add-ports "1" "2")       ; => (ok 3)
(add-ports "1" "x")       ; => (err "parse-int: invalid integer literal: \"x\"")
```

失敗しないと分かっている場合や、失敗したら止まってよい小さなスクリプトでは、`unwrap` で中身を
取り出せます。`err` だった場合は `panic` します。既定値で済むなら `unwrap-or` を使います。

## 3. 自分のエラー型を作る

エラーを文字列ではなく型で表すと、呼び出し側がエラーの種類で分岐できます。エラー型は普通の
`defenum` か `defstruct` で作り、`Error` トレイトを実装します。

```lisp
(defenum config-error
  (missing string)          ; 設定項目が無い
  (invalid string int))     ; 値がおかしい

(impl Error config-error
  (message ((self Self)) string
    (match self
      ((missing key) (format false "missing key: ~a" key))
      ((invalid key v) (format false "invalid value for ~a: ~a" key v))))
  (source ((self Self)) Option<:dyn Error> (Option::none)))

(defun check-workers ((n int)) Result<int,config-error>
  (if (> n 0)
      (Result::ok n)
      (Result::err (config-error::invalid "workers" n))))
```

- `message` はエラーの説明を返します。
- `source` は、このエラーの原因になった別のエラーを返します。原因が無ければ `none` です。

## 4. 種類の違うエラーをまとめる

1 つの関数の中で `parse-int`（`ParseIntError`）と `check-workers`（`config-error`）の両方を
呼ぶと、エラーの型が 2 つになり、1 つの `Result<T,E>` の `E` に書けません。この場合は、
`E` を `:dyn Error`（`Error` を実装した何かのエラー）にします。個々のエラーは `as-dyn-error` で
変換します。

```lisp
(defun load-count ((s string)) Result<int, :dyn Error>
  (match (as-dyn-error (parse-int s))
    ((ok n) (as-dyn-error (check-workers n)))
    ((err e) (Result::err e))))
```

`"4"`、`"-1"`、`"abc"` を渡すと、次のようになります。

```
ok 4
err invalid value for workers: -1
err parse-int: invalid integer literal: "abc"
```

`:dyn` については [トレイト](traits.md) の 5 節を参照してください。

## 5. `panic`：プログラムの誤り

起きてはならない状態に出会ったら、`panic` でプログラムを止めます。

```lisp
(defun safe-get ((v Vector<int>) (i int)) int
  (if (< i (len v))
      (get v i)
      (panic (format false "index ~a out of range" i))))
```

```
error: main.typl:4:7: panic: index 3 out of range
```

- `panic` の型は `!`（戻らない）で、どの型が求められる場所にも書けます。上の例で `if` の両側の型が
  合うのはこのためです。
- 次の操作も `panic` します：`none` や `err` への `unwrap`、範囲外の添字での `get`、整数の
  0 での割り算。
- `panic` はプログラムを止めます。タスクの中で起きても、プログラム全体が止まります。
- REPL では、`panic` しても REPL は終わらず、次の入力を待ちます。
- 「まだ書いていない」は `(todo)`、「ここには来ないはず」は `(unreachable)` と書けます。どちらも
  `panic` します。

`panic` は `Result` の代わりに使うものではありません。ユーザーの入力やファイルの有無のように
起こりうる失敗には `Result` を使ってください。

## 6. `catch` / `throw`：関数をまたいで抜け出す

`throw` は、それを囲む同じタグの `catch` まで一気に抜け出します。間に何段の関数呼び出しが
あってもかまいません。

```lisp
(defun check-all ((v Vector<int>)) ()
  (doiter (x (iter v))
    (if (< x 0)
        (throw 'bad-input (format false "negative: ~a" x))
        ())))

(defun validate ((v Vector<int>)) string
  (catch 'bad-input
    (progn
      (check-all v)
      "all fine")))
```

`v` に負の数が無ければ `validate` は `"all fine"` を返し、`-7` があれば `check-all` の中から
`catch` まで抜けて `"negative: -7"` を返します。

- タグは `'bad-input` のようにシンボルをそのまま書きます。
- **タグごとに、投げる値の型は 1 つに決まります。** 上の例で `'bad-input` は `string` を運ぶので、
  同じタグで `int` を投げると型エラーになります。`catch` の本体の型も、タグの型と合わなければ
  なりません。

  ```
  error: ...: type error: type mismatch: expected `string`, found `int`
  ```

- `throw` の先に同じタグの `catch` が無ければ、エラーになります。

関数の中で途中から戻るだけなら、`catch` / `throw` ではなく `return-from` を使います。
`return-from` は関数をまたげない代わりに、どこへ戻るかがソースを読めば分かります。

```lisp
(defun first-negative ((v Vector<int>)) Option<int>
  (doiter (x (iter v))
    (if (< x 0) (return-from first-negative (Option::some x)) ()))
  (Option::none))
```

## 7. `unwind-protect`：必ず後始末をする

`(unwind-protect 本体 後始末)` は、本体をどう抜けても後始末を実行します。正常に終わったとき、
`throw` で抜けたとき、`panic` したときのどれでも実行されます。

```lisp
(defun with-cleanup ((n int)) int
  (unwind-protect
    (progn
      (println "working on ~a" n)
      (if (< n 0) (throw 'bad-input "negative") ())
      (* n 2))
    (println "cleanup for ~a" n)))
```

```lisp
(println "~a" (with-cleanup 5))
;; working on 5
;; cleanup for 5
;; 10

(println "~a" (catch 'bad-input (progn (with-cleanup -1) "unreached")))
;; working on -1
;; cleanup for -1
;; negative
```

ファイルを開いたら必ず閉じる、ロックを取ったら必ず返す、といった処理に使います。標準ライブラリの
`with-open-file` や `with-lock` も、中で `unwind-protect` を使っています。

## 8. Common Lisp のコンディションシステムについて

typelisp は Common Lisp のコンディションシステム（`handler-case`、`restart-case` など）を
採用していません。関数がどんな失敗を起こしうるかが型に現れず、静的型付けと相性が悪いためです。
起こりうる失敗は `Result` で型に書き、制御の移動は `catch` / `throw` で行います。

## 9. 次に読むもの

- [並行処理](concurrency.md)：タスクとチャネル
- [Option と Result、エラー型](../reference/functions/option-result.md)：関数の一覧
- [エラーメッセージ](../reference/errors.md)：主なエラーの意味と直し方
