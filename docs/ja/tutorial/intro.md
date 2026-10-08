# 入門

REPL で式を評価するところから始めて、関数、変数、条件分岐、繰り返し、リストと `Vector` までを
順に説明します。`typl` のビルド方法は [README_JP.md](../../../README_JP.md) を参照してください。

## 1. REPL を起動する

引数を付けずに `typl` を起動すると、REPL（対話モード）になります。`typl>` のあとに式を入力すると、
その場で評価して値を表示します。`:quit` で終了します。

```
$ typl
typl> (+ 1 2)
3
typl> :quit
```

以下、REPL での入力と結果はこの形で示します。

## 2. 式を評価する

typelisp は Lisp なので、式は括弧で囲んで**先頭に演算子や関数名**を書きます。`1 + 2` ではなく
`(+ 1 2)` と書きます。

```
typl> (* 2 (+ 3 4))
14
typl> (+ 1 2 3 4)
10
typl> "hello"
"hello"
typl> (upcase "hello")
"HELLO"
```

数には次の種類があります。

- **整数**は `int` 型です。大きさに上限はありません。
- **小数**は `f64` 型です。`1.5` や `2.0` のように小数点を付けて書きます。
- `int` と `f64` を混ぜて計算することはできません。`(+ 1 2.0)` は型エラーです。変換するときは
  `(as f64 1)` のように書きます。

```
typl> (* 123456789012345 123456789012345)
15241578753238669120562399025
typl> (/ 7 2)
3
typl> (/ 7.0 2.0)
3.5
```

整数どうしの `/` は、小数点以下を切り捨てた整数になります（Common Lisp のように分数には
なりません）。余りは `(mod 7 2)` で求めます。

真偽値は `true` と `false` です。

```
typl> (> 3 2)
true
typl> (and (> 3 2) (< 3 2))
false
```

## 3. 関数を定義する

関数は `defun` で定義します。**引数の型と戻り値の型は必ず書きます。**

```lisp
(defun square ((n int)) int
  (* n n))
```

- `(n int)` が「`int` 型の引数 `n`」です。引数が複数あれば `((a int) (b int))` と並べます。
- 引数リストの次の `int` が戻り値の型です。
- 本体の最後の式の値が、関数の戻り値になります。`return` は書きません。

```
typl> (defun square ((n int)) int (* n n))
typl> (square 12)
144
typl> (square "a")
error: <stdin>:1:9: type error: type mismatch: expected `int`, found `string`
```

型が合わない呼び出しは、**実行する前に**型エラーとして報告されます。ファイルを実行する場合は、
どこか 1 か所でも型エラーがあれば、プログラムは 1 行も実行されません。

引数を省略できるようにするには `&optional` を使います。既定値を書いておくと、省略したときに
その値が入ります。

```lisp
(defun greet ((name string) &optional (greeting string "Hello")) string
  (format false "~a, ~a!" greeting name))
```

```
typl> (greet "Ann")
"Hello, Ann!"
typl> (greet "Ann" "Hi")
"Hi, Ann!"
```

`format` の第 1 引数の `false` は「出力せずに文字列として返す」という意味です。`~a` の位置に
続く引数が埋め込まれます。

## 4. 変数

局所変数は `let` で作ります。

```lisp
(defun sum-of-squares ((a int) (b int)) int
  (let ((aa (square a))
        (bb (square b)))
    (+ aa bb)))
```

- `let` の変数の型は、初期値から決まります。書く必要はありません。
- `let` の中の変数どうしは互いを参照できません。前の変数を使って次の変数を作るときは `let*` を
  使います。

```
typl> (let* ((a 1) (b (+ a 1))) (* a b))
2
```

変数の値を書き換えるには `setf` を使います。**書き換えても型は変えられません。**

```
typl> (let ((x 1)) (setf x "a"))
error: <stdin>:1:22: type error: type mismatch: expected `int`, found `string`
```

グローバル変数は `defvar` で定義します。こちらは型を書きます。

```lisp
(defvar (counter int) 0)
```

## 5. 条件分岐

### if

`(if 条件 真のとき 偽のとき)` と書きます。**偽のときの式は省略できません。**

```lisp
(defun sign ((n int)) string
  (if (< n 0) "negative" "non-negative"))
```

- 条件に書けるのは `bool` 型の式だけです。`(if 0 ...)` のように数を書くと型エラーになります。
- 真のときと偽のときの式は、同じ型でなければなりません。

偽のときに何もしないなら `when` を使います（逆は `unless`）。

```lisp
(defun report-size ((n int)) ()
  (when (> n 100)
    (println "large")
    (println "really large")))
```

### cond

条件が 3 つ以上あるときは `cond` が読みやすくなります。最後の `else` はどの条件にも当たらなかった
ときです。

```lisp
(defun describe-number ((n int)) string
  (cond ((< n 0) "negative")
        ((= n 0) "zero")
        ((< n 10) "small")
        (else "large")))
```

### match

値の形で分岐するときは `match` を使います。

```lisp
(defun day-name ((d int)) string
  (match d
    (0 "Sun")
    (6 "Sat")
    (_ "weekday")))
```

`_` はどんな値にも当たります。`int` の値は無数にあるので、`_` の腕が無いと
「すべての場合を尽くしていない」というエラーになります。`match` の本領は次の
[型の基本](types.md) で出てくる `Option` や自分で定義した型の分解です。

## 6. 繰り返し

関数は自分自身を呼べます。

```lisp
(defun fact ((n int)) int
  (if (= n 0) 1 (* n (fact (- n 1)))))
```

```
typl> (fact 30)
265252859812191058636308480000000
```

決まった回数の繰り返しには `dotimes` を使います。`i` は 0 から `n - 1` まで変わります。

```lisp
(defun sum-to ((n int)) int
  (let ((total 0))
    (dotimes (i (+ n 1))
      (setf total (+ total i)))
    total))
```

```
typl> (sum-to 100)
5050
```

ほかに `while`、`do`、Common Lisp と同じ拡張 `loop` があります。拡張 `loop` の節の語は
キーワード（`:for`、`:collect` など）で書きます。

```
typl> (loop :for i :from 1 :to 5 :collect (* i i))
#<vector<int> 1 4 9 16 25>
```

## 7. リストと Vector

### Vector

同じ型の値を並べて持つには `Vector<T>` を使います。`T` は要素の型です。

```lisp
(let ((v (the Vector<int> (Vector::new))))
  (push v 3)
  (push v 1)
  (push v 2)
  (println "~a" v))
;; #<vector<int> 3 1 2> と表示される
```

- `(Vector::new)` だけでは要素の型が決まらないので、`(the Vector<int> ...)` で型を指定します。
- `(push v x)` で末尾に追加、`(get v i)` で `i` 番目を読み、`(len v)` で長さを得ます。
- 範囲外の添字で `get` すると、プログラムはエラーで止まります。

### lambda と高階関数

名前の無い関数は `lambda` で作ります。引数と戻り値の型を書くのは `defun` と同じです。

```
typl> ((lambda ((x int)) int (* x 2)) 21)
42
```

`map`、`filter`、`sort`、`foldl` などは、`Vector` を `(iter v)` で**イテレータ**にして渡します。
対象が先、関数が後です。上の `v` は `let` で束縛したので、その `let` の外では使えません。
次の例では、まず `defvar` で `v` を定義します。

```lisp
(defvar (v Vector<int>) (Vector::new))
(push v 3)
(push v 1)
(push v 2)

(map (iter v) (lambda ((x int)) int (* x 10)))                 ; => #<vector<int> 30 10 20>
(filter (iter v) (lambda ((x int)) bool (> x 1)))              ; => #<vector<int> 3 2>
(sort (iter v) (lambda ((a int) (b int)) bool (< a b)))        ; => #<vector<int> 1 2 3>
(foldl (iter v) (lambda ((acc int) (x int)) int (+ acc x)) 0)  ; => 6
```

要素を順に処理するには `doiter` を使います。

```lisp
(doiter (x (iter v))
  (println "item ~a" x))
```

関数を引数に取る関数は、引数の型を `(fn (引数の型...) 戻り値の型)` と書きます。`defun` で
定義した関数も、名前をそのまま値として渡せます。

```lisp
(defun twice ((f (fn (int) int)) (x int)) int
  (f (f x)))

(twice square 3)                                ; => 81
(twice (lambda ((x int)) int (* x 3)) 2)        ; => 18
```

### リスト（S 式）

`'(1 2 3)` や `(list 1 2 3)` で作るリストは **S 式データ**です。要素の型はそろっていなくても
かまいません。

```
typl> '(1 2 3)
(1 2 3)
typl> (list 1 "two" 'three)
(1 "two" three)
```

S 式データは主にマクロ（[マクロ](macros.md)）や `read` でプログラムそのものを扱うためのものです。
要素の型が決まっているデータを持つには `Vector<T>` を使ってください。S 式のリストは `dolist` で
回せます。

```lisp
(dolist (x '(1 2 3))
  (println "x=~a" x))
```

2 つの値の組は `cons` で作り、`car` と `cdr` で取り出します。

```
typl> (let ((p (cons "age" 42))) (cdr p))
42
```

## 8. ファイルに書いて実行する

プログラムはファイル（拡張子 `.typl`）に書いて、`typl ファイル名` で実行できます。
結果を表示するには `println` を使います。

```lisp
;; hello.typl
(defun fact ((n int)) int
  (if (= n 0) 1 (* n (fact (- n 1)))))

(dotimes (i 5)
  (println "~a! = ~a" i (fact i)))
```

```sh
$ typl hello.typl
0! = 1
1! = 1
2! = 2
3! = 6
4! = 24
```

- `println` は `format` と同じ書式で出力し、最後に改行します。改行しないのは `print` です。
- `~a` は人が読む形、`~s` は読み戻せる形（文字列なら `"` 付き）で埋め込みます。
- ファイルは上から順に読まれます。**関数は定義より前では呼べません。**

## 9. 次に読むもの

- [型の基本](types.md)：`Option`、`Result`、構造体、列挙型、ジェネリクス
- [Common Lisp から来た人へ](../guide/from-common-lisp.md)：Common Lisp を知っている人向けの違いの一覧
