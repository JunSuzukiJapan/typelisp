# マクロ

マクロは、プログラムを受け取ってプログラムを返す関数です。関数では書けない新しい構文を
自分で作れます。typelisp のマクロは Common Lisp の `defmacro` と同じ仕組みです。
[入門](intro.md) の「リスト（S 式）」を読んでいることを前提にします。

## 1. マクロと関数の違い

関数は、引数を**評価した値**を受け取ります。マクロは、引数を**評価する前の式のまま**
（S 式データとして）受け取り、別の式を組み立てて返します。返した式が、マクロを呼んだ場所に
置き換わってから、型検査と実行が行われます。この置き換えを**展開**と呼びます。

たとえば `unless` のような構文は関数では書けません。関数にすると、条件が真でも本体が先に
評価されてしまうからです。

## 2. `defmacro` と準クオート

条件が偽のときだけ本体を実行する `my-unless` を作ります。

```lisp
(defmacro my-unless (test &rest body)
  `(if ,test () (progn ,@body ())))
```

- マクロの引数には型を書きません。引数はすべて S 式データです。
- `&rest body` は、残りの引数をまとめて 1 つのリストとして受け取ります。
- `` ` ``（準クオート）で始めた式は、そのままデータとして組み立てられます。その中で、
  - `,test` は変数 `test` の中身をその位置に埋め込みます。
  - `,@body` はリスト `body` の要素を、その位置に展開して埋め込みます。

```lisp
(my-unless (> 1 2)
  (println "one")
  (println "two"))
;; one
;; two
```

展開した結果は `macroexpand-1` で確かめられます。マクロを書くときは、まず展開結果を見るのが
近道です。

```
typl> (macroexpand-1 '(my-unless (> 1 2) (println "one")))
(ok (if (> 1 2) () (progn (println "one") ())))
```

## 3. 展開結果も型検査される

マクロが返した式は、普通に書いた式と同じように型検査されます。

```
typl> (defmacro add-a (x) `(+ ,x "a"))
typl> (add-a 1)
error: <stdin>:1:1: type error: type mismatch: expected `int`, found `string`
```

エラーの位置は、マクロを呼んだ場所になります。

`if` の偽の側は省略できないこと、`if` の両側の型がそろっていなければならないことも、展開結果に
そのまま当てはまります。上の `my-unless` が `(progn ,@body ())` と最後に `()` を置いているのは、
本体の最後の式が何の型でも、`if` の両側を `()` 型にそろえるためです。

## 4. 変数名の衝突と `gensym`

2 つの変数の値を入れ替えるマクロを素直に書くと、次のようになります。

```lisp
(defmacro swap-bad (a b)
  `(let ((tmp ,a))
     (setf ,a ,b)
     (setf ,b tmp)))
```

ほとんどの場合は動きますが、呼び出し側の変数名がたまたま `tmp` だと壊れます。

```lisp
(let ((tmp 1) (other 2))
  (swap-bad tmp other)
  (println "tmp=~a other=~a" tmp other))
;; tmp=1 other=2   （入れ替わっていない）
```

展開すると `(let ((tmp tmp)) (setf tmp other) (setf other tmp))` になり、マクロが作った `tmp` が
呼び出し側の `tmp` を隠してしまうからです。

これを避けるには、マクロの中で使う変数名を `gensym` で作ります。`gensym` は、プログラムの
どこにも書けない新しいシンボルを返します。

```lisp
(defmacro swap (a b)
  (let ((tmp (gensym "tmp")))
    `(let ((,tmp ,a))
       (setf ,a ,b)
       (setf ,b ,tmp))))
```

```lisp
(let ((tmp 1) (other 2))
  (swap tmp other)
  (println "tmp=~a other=~a" tmp other))
;; tmp=2 other=1
```

typelisp のマクロは Common Lisp と同じく、名前の衝突を自動では防ぎません（非衛生的マクロ）。
**マクロが作る束縛には `gensym` を使う**、と覚えておいてください。

同じ考え方で、回数を指定して本体を繰り返すマクロは次のように書けます。

```lisp
(defmacro repeat (n &rest body)
  (let ((i (gensym "i")))
    `(dotimes (,i ,n) ,@body)))

(repeat 3 (println "hi"))
```

## 5. 引数の形で展開を変える

マクロの本体は普通の typelisp のコードなので、`if` や `match` で引数を調べて、展開する式を
変えられます。引数の型は S 式データ（`Option<Sexpr>`）で、空のリストが `none` です。

すべての条件が真なら `true` を返す `my-and` を作ります。

```lisp
(defmacro my-and (&rest forms)
  (match forms
    ((none) 'true)                                    ; 引数なし
    ((cons f more)
     (if (sexpr-null more)
         f                                            ; 1 つだけ
         `(if ,f (my-and ,@more) false)))             ; 2 つ以上
    (_ (panic "my-and: not a list"))))

(my-and (> 2 1) (> 3 2))      ; => true
(my-and (> 2 1) (> 1 3))      ; => false
```

- `(cons f more)` は、リストの先頭を `f` に、残りを `more` に取り出すパターンです。
- `sexpr-null` は、S 式データが空のリストかどうかを調べます。
- 最後の `_` の腕が必要なのは、S 式データにはリスト以外（数や文字列など）の形もあり、
  `match` がそれらも尽くすよう求めるからです。`&rest` の引数は必ずリストなので、この腕が
  実行されることはありません。
- マクロは展開結果の中で自分自身を呼べます。展開は、マクロ呼び出しが無くなるまで繰り返されます。

## 6. 省略できる引数

`&optional` で省略できる引数を受け取れます。既定値も書けます。

```lisp
(defmacro inc-by (place &optional (n 1))
  `(setf ,place (+ ,place ,n)))

(let ((c 10))
  (inc-by c)
  (inc-by c 5)
  c)                            ; => 16
```

`&key` でキーワード引数も受け取れます（[構文リファレンス 3.14](../reference/syntax.md#314-defmacro--マクロ定義)）。

## 7. `macrolet`：その場だけのマクロ

1 つの式の中だけで使うマクロは `macrolet` で定義できます。外からは見えません。

```lisp
(macrolet ((sq (x) `(* ,x ,x)))
  (+ (sq 3) (sq 4)))            ; => 25
```

## 8. マクロを使う上での注意

- **マクロは定義より後でしか呼べません。** 関数と同じく、ファイルの上のほうで定義してください。
- 他のモジュールから使うマクロは `(pub defmacro ...)` で公開します。
- `when`、`unless`、`cond`、`and`、`or`、`dotimes` など、標準の構文の多くもマクロとして
  定義されています。`(macroexpand '(when true 1))` で中身を見られます。
- 関数で書けるものは関数で書いてください。マクロは値として渡せず、展開結果を読まないと
  動きが分からないためです。

## 9. 次に読むもの

- [エラー処理](errors.md)：`Result`、`panic`、`catch` / `throw`
- [マクロの関数](../reference/functions/system.md#8-マクロ)：`gensym`、`macroexpand` など
