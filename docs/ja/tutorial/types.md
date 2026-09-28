# 型の基本

typelisp は静的型付きの言語です。この章では、型検査が何をしてくれるかと、よく使う型
（`Option`、`Result`、構造体、列挙型）、それにジェネリクスを説明します。[入門](intro.md) を
読んでいることを前提にします。

## 1. 静的型とは

typelisp では、すべての式の型がプログラムを実行する前に決まります。型の合わない式は、実行する
前にエラーになります。

```lisp
(defun square ((n int)) int (* n n))

(defun main () ()
  (println "start")
  (println "~a" (square "3")))    ; 型エラー

(main)
```

このファイルを実行すると、`start` も表示されずに型エラーで止まります。

```
error: main.typl:5:25: type error: type mismatch: expected `int`, found `string`
```

型を書く必要があるのは、関数の引数と戻り値、グローバル変数、構造体のフィールドです。`let` の
変数の型は初期値から決まります。

主な型:

| 型 | 値の例 |
|---|---|
| `int` | `42`、`-7`（任意精度の整数） |
| `i8` `i16` `i32` `u8` `u16` `u32` | 幅の決まった整数 |
| `f64` `f32` | `1.5` |
| `bool` | `true`、`false` |
| `char` | `#\a` |
| `string` | `"hello"` |
| `symbol` | `'foo`、`:key` |
| `()` | 値を返さない関数の戻り値の型 |

実行時に値の型を調べる手段（Common Lisp の `typep` や `type-of`）はありません。型はすべて
実行前に分かっているからです。

## 2. `Option<T>`：値が無いかもしれない

typelisp には `nil` がありません。「値が無いかもしれない」ことは、型 `Option<T>` で表します。
`Option<T>` の値は、`T` の値を 1 つ持つ `some` か、何も持たない `none` のどちらかです。

```lisp
(defun safe-div ((a int) (b int)) Option<int>
  (if (= b 0)
      (Option::none)
      (Option::some (/ a b))))
```

```
typl> (safe-div 10 2)
(some 5)
typl> (safe-div 1 0)
none
```

`Option<int>` は `int` ではないので、そのまま計算には使えません。`(+ (safe-div 10 2) 1)` は型エラー
です。中身を使うには `match` で `some` と `none` を分けます。

```lisp
(defun show-div ((a int) (b int)) string
  (match (safe-div a b)
    ((some q) (format false "~a" q))
    ((none) "undefined")))
```

- `(some q)` の腕では、中身が変数 `q` に入ります。
- `match` は腕が**すべての場合を尽くしているか**を検査します。`(none)` の腕を書き忘れると
  型エラーになります。

### なぜ nil が無いのか

多くの言語では、`nil`（`null`）がどの型の値の代わりにもなれます。そのため、「値が無い」場合を
扱い忘れても、実行するまで気付きません。typelisp では値が無いかもしれない場所は `Option<T>` 型に
なり、`match` で `none` の場合を書かないと型検査を通りません。扱い忘れは実行前に見つかります。

条件式も同じ考え方で、`if` の条件に書けるのは `bool` だけです。Common Lisp のように「`nil` 以外は
真」という規則はありません。

### よく使う操作

| 書き方 | 意味 |
|---|---|
| `(unwrap-or opt 既定値)` | `some` なら中身、`none` なら既定値 |
| `(unwrap opt)` | 中身を取り出す。`none` ならプログラムが止まる |
| `(is-some opt)` / `(is-none opt)` | どちらかを調べる |

標準ライブラリにも `Option` を返す関数がたくさんあります。たとえば `position` は、見つかれば
位置を `some` で、見つからなければ `none` を返します。

```lisp
(match (position "banana" (iter fruits))
  ((some i) (println "found at ~a" i))
  ((none) (println "not found")))
```

## 3. `Result<T,E>`：失敗するかもしれない

失敗する可能性のある処理は `Result<T,E>` を返します。成功なら値 `T` を持つ `ok`、失敗なら
エラー `E` を持つ `err` です。

```lisp
(match (parse-int "42")
  ((ok n) (println "n = ~a" (+ n 1)))
  ((err e) (println "error: ~a" (message e))))
;; n = 43

(match (parse-int "4x2")
  ((ok n) (println "n = ~a" (+ n 1)))
  ((err e) (println "error: ~a" (message e))))
;; error: parse-int: invalid integer literal: "4x2"
```

自分の関数でも `Result` を返せます。

```lisp
(defun checked-age ((n int)) Result<int,string>
  (if (< n 0)
      (Result::err "age must not be negative")
      (Result::ok n)))
```

`Option` は「無い」ことに理由が要らない場合、`Result` は失敗の理由を伝えたい場合に使います。
エラーの扱い方は [エラー処理](errors.md) で詳しく説明します。

## 4. `defstruct`：構造体

名前の付いたフィールドを持つ型は `defstruct` で定義します。

```lisp
(defstruct point
  (x int)
  (y int))
```

定義すると、次のものが使えるようになります。

```lisp
(let ((p (point::new 3 4)))     ; 作る（フィールドの順に引数を渡す）
  (println "~a" p::x)           ; フィールドを読む。(x p) とも書ける
  (setf p::x 10)                ; 書き換える
  (println "~a" p))             ; #<point x: 10 y: 4>
```

構造体に関数を持たせるには `defmethod` を使います。最初の引数（`self`）の型で、どの型の
メソッドかが決まります。

```lisp
(defmethod norm2 ((self point)) int
  (+ (* self::x self::x) (* self::y self::y)))

(norm2 (point::new 3 4))        ; => 25
```

`self` を取らずに型名だけを書くと、`point::origin` の形で呼ぶ関数になります。

```lisp
(defmethod origin (point) point
  (point::new 0 0))

(point::origin)                 ; => #<point x: 0 y: 0>
```

## 5. `defenum`：いくつかの形のどれか

「円か、長方形か、点か」のように、いくつかの形のどれかである値は `defenum` で定義します。
それぞれの形を**変種**と呼びます。変種ごとに違う数・型の値を持てます。

```lisp
(defenum shape
  (circle int)        ; 半径
  (rect int int)      ; 幅と高さ
  (dot))              ; 値を持たない
```

値は `shape::circle` のように型名を付けて作ります。`match` では変種の名前で分解します。

```lisp
(defun area ((s shape)) int
  (match s
    ((circle r) (* 3 r r))
    ((rect w h) (* w h))
    ((dot) 0)))

(area (shape::circle 2))        ; => 12
(area (shape::rect 3 4))        ; => 12
```

`match` はここでも網羅性を検査します。後で `shape` に変種を足すと、その変種を扱っていない
`match` がすべて型エラーになるので、直す場所を漏らしません。

`(use shape)` と書くと、それより後では型名を付けずに `(rect 5 6)` と書けます。

`Option` と `Result` も、この仕組みで作られた列挙型です。

## 6. ジェネリクス

どんな型にも使える関数は、名前の後ろに `<T>` と**型パラメータ**を書いて定義します。

```lisp
(defun first-or<T> ((v Vector<T>) (default T)) T
  (if (= (len v) 0)
      default
      (get v 0)))
```

呼び出すときは型を指定しません。引数から `T` が決まります。

```lisp
(first-or ints 7)          ; T は int
(first-or names "none")    ; T は string
(first-or ints "none")     ; 型エラー：ints が Vector<int> なので T は int
```

構造体と列挙型もジェネリックにできます。`Vector<T>`、`Option<T>`、`Result<T,E>` はこの形の型です。

```lisp
(defstruct pair<A,B>
  (left A)
  (right B))

(pair::new 1 "one")        ; pair<int,string>
```

ジェネリック関数の中では、`T` について何も分からないので、`T` の値どうしを比べたり足したりは
できません。「比べられる型なら何でもよい」のような条件を付けるにはトレイトを使います
（[トレイト](traits.md)）。

## 7. 型に別名を付ける

`deftype` で型に別名を付けられます。

```lisp
(deftype meters int)
(defun double ((m meters)) meters (* m 2))
```

`meters` は `int` の別の綴りで、新しい型ではありません。`meters` の引数に普通の `int` を渡しても
エラーにはなりません。区別したい場合は `(defstruct meters (value int))` のように構造体を作ります。

## 8. 次に読むもの

- [トレイト](traits.md)：型に共通の操作を持たせる
- [型の一覧](../reference/types.md)：組み込みの型と、それぞれが実装しているトレイト
