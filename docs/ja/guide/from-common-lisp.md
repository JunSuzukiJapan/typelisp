# Common Lisp から来た人へ

typelisp は Common Lisp（以下 CL）の構文と多くの関数名を受け継いでいますが、静的型付きの
言語です。この違いから、CL の書き方がそのままでは通らない場面があります。ここでは、CL に
慣れた人がつまずきやすい点を、書き換え方と一緒にまとめます。

## 1. `nil` と `t` が無い

真偽値は `true` と `false` です。`nil` と `t` は定義されていません。

```lisp
(if (> x 0) "pos" "non-pos")
(defvar (debug bool) false)
```

- **条件に置けるのは `bool` だけです。** `0` や空リストを条件に書くと型エラーになります。
  CL の「nil 以外はすべて真」という規則はありません。
- **`if` の else は省略できません。** `(if c x)` はエラーです。else が要らないときは `when` /
  `unless` を使います。
- **「値が無い」は `Option<T>` で表します。** CL で nil を返して「見つからなかった」を表して
  いた関数は、ここでは `(some x)` か `none` を返します。

  ```lisp
  (match (position 3 (iter v))
    ((some i) (println "found at ~a" i))
    ((none) (println "not found")))
  ```

- 空リスト `()` は、文脈によって Unit 型の値（何も返さない関数の戻り値）か、S 式データの
  空リストになります。`false` とは別の値です。

## 2. 型を書く

関数の引数と戻り値には型が必須です。

```lisp
(defun area ((w i32) (h i32)) i32
  (* w h))

(defun first-or<T> ((v Vector<T>) (default T)) T      ; ジェネリック関数
  (unwrap-or (first (iter v)) default))
```

- `(defun f (x) x)` のような型の無い定義は書けません。
- `defvar` などのグローバル変数にも型を書きます：`(defvar (count int) 0)`。
- `the` は実行時の検査ではなく、型検査器への注釈です。
- **実行時に型を調べる手段はありません。** `typep` や `type-of` はありません。値の型は
  コンパイル時に決まっているからです。いくつかの型のどれかを受け取りたいときは、`defenum`
  で直和型を作るか、トレイトを使います。
- `deftype` は型の別名です。`(deftype small () '(integer 0 9))` のような値の範囲を表す型は
  作れません。

整数の既定の型 `int` は任意精度で、CL の integer と同じく大きさの上限がありません。固定幅の
`i8`〜`i32` / `u8`〜`u32` もあります。64bit 幅の固定長整数型はありません。

## 3. 関数を値として扱う

typelisp は関数と変数の名前空間を分けていません。関数名はそのまま値として渡せます。
`#'` と `funcall` はありません。

```lisp
(defun twice ((x int)) int (* 2 x))
(defun apply-to ((f (fn (int) int)) (x int)) int
  (f x))                                  ; funcall ではなく、そのまま呼ぶ

(apply-to twice 5)                        ; #'twice ではなく twice
```

- `+` や `1+` などの組み込みの関数も、引数の型が `(fn (int) int)` のように決まっている位置なら、
  そのまま値として渡せます。`foldl` や `map` のようなジェネリックな関数に渡すときは、どの型の
  `+` かが決まらないので `lambda` で包んでください。

  ```lisp
  (apply-to 1+ 5)                                          ; => 6
  (foldl (iter v) (lambda ((acc int) (x int)) int (+ acc x)) 0)
  ```

- シーケンス関数は**対象が先、関数が後**の順で引数を取ります：`(map it f)`、
  `(filter it f)`、`(foldl it f init)`。CL の `(mapcar f list)` とは逆です。
- `lambda` では `&optional` と `&key` を使えません（`&rest` は使えます）。
- **定義より前の関数は呼べません。** CL では後で定義する関数を先に呼んでおけますが、ここでは
  `no such function` になります。相互再帰する関数は `defsignature` で片方を宣言しておきます。

  ```lisp
  (defsignature odd2 (i32) bool)
  (defun even2 ((n i32)) bool (if (= n 0) true (odd2 (- n 1))))
  (defun odd2 ((n i32)) bool (if (= n 0) false (even2 (- n 1))))
  ```

## 4. リストと Vector

CL のリストに当たるものは **S 式データ**で、その型は `Option<Sexpr>` です（空リストが `none`）。
`(list 1 2 3)` や `'(a b c)` はこの型になります。S 式データはマクロや `read` で扱うもので、
普通のデータの入れ物には **`Vector<T>`** を使います。

| やりたいこと | CL | typelisp |
|---|---|---|
| S 式の先頭・残り | `(car xs)` `(cdr xs)` | `(sexpr-car xs)` `(sexpr-cdr xs)` |
| S 式リストを順に回す | `(dolist (x xs) ...)` | 同じ |
| 型の決まった要素の並び | リストかベクタ | `Vector<T>` |
| ペア | `(cons a b)` | `(cons a b)`（型は `cons-cell<A,B>`） |
| 写像 | `(mapcar f xs)` | `(map (iter v) f)` |

`car` / `cdr` は、`cons` で作ったペア `cons-cell<A,B>` のアクセサです。S 式リストには使えません。

`Vector` は CL のベクタと同じく `#(..)` で書けます。印字も `#(..)` です:

```lisp
(let ((v #(1 2)))
  (push v 3)
  (map (iter v) (lambda ((x int)) int (* x 10))))   ; => #(10 20 30)
```

CL との違いは 3 つあります。要素の型がそろっていなければなりません（`#(1 "a")` は型エラー）。
評価するたびに新しいベクタができるので、書き換えても次の評価には響きません（CL ではリテラルを
書き換えた結果は未定義）。要素が無い `#()` は、`(the Vector<int> #())` のように型を添えます。
多次元配列の `#2A((1 2) (3 4))` も CL と同じ書き方です。

`map`・`filter`・`sort`・`find` などのシーケンス関数は `Iter` トレイトを実装した値に働きます。
`Vector` は `(iter v)` でイテレータにして渡します。

## 5. 多値が無い

`values` と `multiple-value-bind` はありません。CL で複数の値を返す関数は、ペアか構造体を返します。

| CL | typelisp |
|---|---|
| `(floor 7 2)` → 3, 1 | `(floor-div 7 2)` → `cons-cell` の `car` が 3、`cdr` が 1 |
| `(decode-universal-time t)` → 9 値 | `decoded-time` 構造体 |
| `(read-from-string s)` → 値, 位置 | `(read-from-string s)` は値と位置の `cons-cell` を `Result` に入れて返す。値だけなら `(read s)` |

## 6. スペシャル変数（動的束縛）が無い

`let` は常に字句的な束縛です。`defvar` で定義した変数を `let` で束縛し直しても、呼び出した先の
関数からは元の値が見えます。

```lisp
(defvar (*depth* int) 1)
(defun show () () (println "~a" *depth*))
(let ((*depth* 2)) (show))       ; CL なら 2、typelisp では 1
```

`*print-base*` のような制御変数を一時的に変えたいときは `dlet` を使います。値を代入し、本体を
どう抜けても元に戻します。

```lisp
(dlet ((*print-base* 16))
  (format false "~a" 255))       ; => "ff"
```

`dlet` はグローバル変数そのものを書き換えるので、スレッドごとの束縛にはなりません。

## 7. コンディションシステムを採用していない

`define-condition`・`handler-case`・`handler-bind`・`restart-case`・`error`・`signal` はありません。
静的型付けと相性が悪いためです。代わりに次の 2 つを使い分けます。

- **回復できる失敗は `Result<T,E>` を返します。** 呼び出し側は `match` で `ok` / `err` を分けます。
  Rust の `?` に当たる省略構文はありません。

  ```lisp
  (match (parse-int "42x")
    ((ok n) n)
    ((err e) (progn (println "bad input: ~a" (message e)) 0)))
  ```

- **回復できない失敗（バグ）は `panic` です。** `(panic "message")`、`unwrap` に `none` を渡す、
  0 で割る、範囲外の添字などがこれにあたり、プログラムは止まります。`unwind-protect` の後始末は
  止まる前に実行されます。

エラー型は `Error` トレイトで統一されていて、`(message e)` でメッセージを取り出せます。
自分のエラー型を作る方法は [Option と Result、エラー型](../reference/functions/option-result.md#3-エラー型と-error-トレイト)
にあります。`assert` と `warn` は CL と同じように使えます。

`catch` / `throw` / `unwind-protect` はあります。ただし `catch` のタグは評価されないシンボルの
リテラル（`'done`）に限られ、同じタグで投げる値の型は 1 つに決まります。

## 8. CLOS が無い

`defclass`・`defgeneric`・メソッドコンビネーションはありません。

- データ型は `defstruct`（構造体）と `defenum`（直和型）で定義します。
- `defmethod` は、**最初の引数の静的な型**だけで呼び出し先が決まるメソッドを定義します。
  多重ディスパッチはしません。
- 型をまたいで共通の操作を持たせるときはトレイト（`deftrait` / `impl`）を使います。実行時に
  中身の型が決まる値を扱うときは `:dyn Trait` 型にします
  （[構文リファレンス 3.9](../reference/syntax.md#39-deftrait--impl--トレイト機構)）。

`defstruct` の違い:

- コンストラクタは `型名::new` です：`(point::new 1 2)`。`make-point` という名前が欲しいときは
  `(:constructor make-point)` オプションで作れます。
- アクセサは `(x p)` のほか `p::x` とも書けます。`(setf p::x 5)` で書き換えます。
- 述語（`point-p`）は作られません。`:conc-name`・`:type`・`:named` もありません。
- `:include` はスロットを引き継ぐだけで、親の型の部分型にはなりません。

## 9. パッケージの代わりにモジュール

パッケージはありません。名前空間はモジュールで、ファイルがそのままモジュールになります。
`pkg:symbol` の代わりに `module::name` と書き、`use` で取り込みます
（[モジュールとファイル構成](modules.md)）。

キーワード `:foo` はあり、自分自身に評価されるシンボルです。パッケージが無いので、コロンも名前の
一部です：`(symbol->string :foo)` は `":foo"` を返します。

## 10. 読み取り・構文の違い

- 大文字と小文字は区別しません（シンボルは読むときに小文字になります）。CL と同じです。
- `#'` はありません（3 節）。複素数のリテラル `#c(...)` は読めません。複素数は `(complex 1.0 2.0)` で作ります。
- 拡張 `loop` の節はキーワードで書きます：`(loop :for i :from 1 :to 3 :collect i)`。
  キーワードで始まらない `loop` は単純な無限ループで、`(break)` か `(return 値)` で抜けます。
  `return` が抜けるのは直近のループです（関数から抜けるのは `return-from`）。
- `format` の出力先は `false`（文字列を返す）か `true`（標準出力）か、ストリームです。
  書式ディレクティブは CL と同じです。
- 文字列から読むのは `(read "...")`、ストリームから読むのは `(read-sexpr s)` です。どちらも
  `Result` を返します。
- `eval` は、渡された式を型検査してから評価し、`Result` を返します。前方参照ができないのは
  ソースの場合と同じです。
- `eval-when` はありません。
- 関数名に `?` や `!` の接尾辞は使いません。述語は CL と同じく `-p` / `p`（`zerop`、`sexpr-null`）
  か、`is-` を前に付けた名前（`is-some`）です。

## 11. 名前が違う主な関数

| CL | typelisp |
|---|---|
| `string-upcase` / `string-downcase` | `upcase` / `downcase` |
| `read`（ストリームから） | `read-sexpr` |
| `pathname` | `to-pathname` |
| `floor` などの 2 引数版 | `floor-div` `ceiling-div` `round-div` `truncate-div` |
| `mapcar` | `map`（引数の順が逆。4 節） |
| `length`（ベクタ） | `len` |
| `hash-table-count` | `count` / `size` |

関数の一覧は [組み込み関数](../reference/functions/README.md) にあります。

## 12. その他、無いもの

- `progv`、`symbol-function`、`symbol-value`
- `*readtable*` と `copy-readtable`、`readtable-case`（リーダマクロ自体は `set-macro-character` で
  定義できます）
- 論理パス名、ワイルドカードパス名
- `input-stream-p` / `output-stream-p`（ストリームの向きは型で決まります）
