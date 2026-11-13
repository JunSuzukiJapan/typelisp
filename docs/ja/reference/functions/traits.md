# 標準トレイト

反復・比較・算術・要素の出し入れのトレイト。ほかの標準トレイトは、それぞれの章にある:
`Hash`（[HashTable](collections.md#4-hashtablekv)）、`Error`（[エラー型](option-result.md#3-エラー型と-error-トレイト)）、
`print-object`（[印字](printing.md#5-print-object型ごとの印字表現)）、ストリームのトレイト群と
`Pathish`（[ストリームとファイル](streams-files.md)）。どの型がどれを実装しているかは
[型の一覧](../types.md)。トレイトの定義方法は [構文リファレンス](../syntax.md#39-deftrait--impl--トレイト機構)。

## 1. `Iter` トレイトと反復

```lisp
(deftrait Iter ()
  (:type Item)
  (defmethod next (self) Option<Item>))
```

`Vector<T>`/`HashTable<K,V>`/`Array<T>` はそれぞれ `vector-iter<T>`/`hashtable-iter<K,V>`/
`array-iter<T>` を介して `Iter` を実装している（`(iter コレクション)` でイテレータを取得）。
`Chan<T>` は自分自身が `Iter`（`recv` が `next` に当たる。[チャネル](concurrency.md#2-chant--チャネル)）。
`Sexpr` のリストには `Iter` を実装していない（要素型が一様でないため）。自前の型に `Iter` を
実装すれば、そのまま `doiter` で回せ、[シーケンス関数](sequences.md#4-iter-上のシーケンス関数)にも渡せる。

## 2. `Equal` / `Ord`（比較）

`Equal` は `equal` が使うトレイトで、Rust の `PartialEq` に当たる。`Ord` は Rust の `PartialOrd`
に当たる。ジェネリック関数の `:where` 境界で、要素型の比較を要求するのに使う。

```lisp
(deftrait Equal ()
  (defmethod equal (self (other Self)) bool))                ; 実装必須
(deftrait Ord (Equal)                                     ; Equal を継承
  (defmethod less (self (other Self)) bool)                  ; 実装必須
  (defmethod less-equal (self (other Self)) bool (not (less other self)))
  (defmethod greater (self (other Self)) bool (less other self))
  (defmethod greater-equal (self (other Self)) bool (not (less self other))))
```

`Equal` を実装するには `equal` を、`Ord` を実装するには `less` だけを書けばよい。`Ord` の残りは
デフォルト実装が埋める。`Ord` は `Equal` を継承するので、`impl Ord X` の前に `impl Equal X` が要る。

`(equal a b)` は、`a` の型の `Equal` の実装を呼ぶ。`Equal` を実装しない型に書くと型エラーになる。
同一性で比べるなら `eq`/`eql`、構造で比べるなら `equalp` を使う。この 3 つはどの型にも使える。

各トレイトメソッドはそのまま関数として呼べる（`where (Equal A)`/`(Ord A)` 境界内、または実装済みの
具体型に対して）:

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `equal` | `(equal a b)` | `(A,A)→bool` where `Equal A` | 等しいか（Rust の `==`） |
| `less` | `(less a b)` | `(A,A)→bool` where `Ord A` | `a < b` |
| `less-equal` | `(less-equal a b)` | `(A,A)→bool` where `Ord A` | `a <= b` |
| `greater` | `(greater a b)` | `(A,A)→bool` where `Ord A` | `a > b` |
| `greater-equal` | `(greater-equal a b)` | `(A,A)→bool` where `Ord A` | `a >= b` |

`Equal` 実装済み: 全数値型（`i8`〜`u32` / `f32` / `f64` / `int` / `ratio`）と `bool` `char`
`string` `symbol` `complex`、`Sexpr`、`cons-cell<A,B>` とタプル（要素が `Equal` なら要素ごとに）、
`Option<T>` と `Result<T,E>`（同じ変種で、中身が `equal` のとき）。組み込みの型は、型ごとの組み込み
メソッド `equal` がそのまま `Equal` の実装になる。数値は `=` で、文字列は大文字と小文字を区別して
中身で比べる。`Ord` 実装済み: 全数値型と `char` `string`、および `cons-cell<A,B>`（辞書順、要素が
`Ord` なら）。

リスト（`Sexpr`）の `equal` は CL の `equal` で、コンスセルは構造で、文字列は中身で、それ以外の
アトムは `eql` で比べる。この比較は実行時に値をたどるもので、メソッドを呼ばない。そのため、リストの
中の構造体は、その型の `impl Equal` にかかわらず、CL と同じく同一性で比べる。

`Ord` のメソッド名が組み込みの演算子（`< <= > >=`）や `lt` と重ならないのは、組み込みは再定義
できず、各実装がそれらへ委譲するため。スカラの比較演算子そのものは受け手の型ごとの組み込み
メソッド（[数値](numbers.md)・[文字列と文字](collections.md)）。境界内では演算子で書けば、
トレイトのメソッドに読み替えられる（3 章）。`=` は `equal` に、`/=` は `(not (equal a b))` になる。

## 3. 算術トレイト（`Add` / `Sub` / `Mul` / `Div` / `Rem` / `Bits` / `Number`）

ジェネリックなコードが「足せる型」を要求するための層。**具体型の演算は組み込み演算子**
（[数値](numbers.md)）で、この層は通らない。

```lisp
(deftrait Add () (defmethod add (self (other Self)) Self))
(deftrait Sub () (defmethod sub (self (other Self)) Self))
(deftrait Mul () (defmethod mul (self (other Self)) Self))
(deftrait Div () (:type Output) (defmethod div (self (other Self)) Output))
(deftrait Rem () (defmethod remainder (self (other Self)) Self))
(deftrait Bits ()
  (defmethod bit-and (self (other Self)) Self)
  (defmethod bit-or  (self (other Self)) Self)
  (defmethod bit-xor (self (other Self)) Self)
  (defmethod bit-not (self) Self)
  (defmethod shift (self (count int)) Self))                 ; 距離は常に int（ash と同じ）
(deftrait Number (Add Sub Mul Rem Ord))                   ; メソッド無し・5つの合成
```

**境界内では演算子で書ける。** `:where` で束縛された型変数が受け手のとき、演算子は
トレイトメソッドへ読み替えられる（`+`→`add`、`-`→`sub`、`*`→`mul`、`/`→`div`、
`rem`→`remainder`、`logand`→`bit-and`、`=`→`equal`、`<`→`less` …）:

```lisp
(defun sum3<T> ((a T) (b T) (c T)) T (:where (Number T))
  (+ a b c))                                              ; = (add (add a b) c)
```

トレイト側のメソッド名が `+` でないのは、`+` が組み込みメソッド名で `impl` が再定義を
拒むため（`cannot redefine built-in method`）。`Neg` は無い——`(- x)` は
`(- (- x x) x)` へ展開されるので `Sub` だけで足りる。

実装済み: `Add`/`Sub`/`Mul`/`Div`/`Rem`/`Number` は全数値型（`complex` を除く）、`Bits` は全整数型と `int`。

**`Div` は結果の型を関連型 `Output` で名乗る。** CL と同じく整数どうしの `/` は `rational` なので、
`int` の `Output` は `rational`、ほかの数値型は自分自身。`Output` が型ごとに違うため `Number` には
`Div` を含めない（継承リストでは関連型をすべて固定する必要がある）。割り算をするジェネリック関数は
`(:where (Div T (Output R)))` で結果の型を受け取るか、`(:where (Div T (Output T)))` で `T` の中に
閉じた割り算を要求する（この場合 `int` は渡せない）。

## 4. `Push` トレイト（要素を足す）

```lisp
(deftrait Push ()
  (:type Item)
  (defmethod push (self (x Item)) ()))
```

要素を 1 つずつ足せるコレクション。ジェネリック関数は `(:where (Push C (Item T)))` で
「`T` を足せる型」を要求し、どのコレクションにも同じ `(push c x)` で足せる。

| 型 | `push` の意味 |
|---|---|
| `Vector<T>` | 末尾に足す |
| `Deque<T>` | 末尾に足す（`push-back`） |
| `Array<T>` | fill pointer の位置に足す（`push-extend`）。fill pointer が無ければ panic |
| `HashSet<T>` | 集合に入れる（`insert`）。要素の型が `Hash` を実装しているときだけ `Push` |

```lisp
(use std::collections::Deque)
(defun push-all<C,T> ((c C) (xs Vector<T>)) () (:where (Push C (Item T)))
  (doiter (x (iter xs)) (push c x)))

(let ((v (the Vector<int> (Vector::new)))
      (d (the Deque<int> (Deque::new))))
  (push-all v (the Vector<int> #(1 2)))
  (push-all d (the Vector<int> #(3 4)))
  (println "~a ~a" v d))                                  ; #(1 2) #<deque 3 4>
```

自前の型に `Push` を実装すれば、同じジェネリック関数に渡せる。`:dyn Push<int>` のように
トレイトオブジェクトにもできる。

## 5. `Pop` トレイト（要素を取り出す）

```lisp
(deftrait Pop ()
  (:type Item)
  (defmethod pop (self) Option<Item>))
```

`push` で最後に足した要素を取り出せるコレクション。空なら `none`。ジェネリック関数は
`(:where (Pop C (Item T)))` で要求する。

| 型 | `pop` の意味 |
|---|---|
| `Vector<T>` | 末尾を取り出す |
| `Deque<T>` | 末尾を取り出す（`pop-back`） |
| `Array<T>` | fill pointer の手前を取り出す。fill pointer が無ければ panic |

`HashSet<T>` には「最後の要素」が無いので、`Push` だが `Pop` ではない。

```lisp
(use std::collections::Deque)
(defun pop-all<C,T> ((c C)) Vector<T> (:where (Pop C (Item T)))
  (let ((out (the Vector<T> (Vector::new))) (more true))
    (while more
      (match (pop c)
        ((some x) (push out x))
        ((none) (progn (setf more false) ()))))
    out))

(let ((v (the Vector<int> #(1 2 3)))
      (d (the Deque<int> (Deque::new))))
  (push d 1) (push d 2)
  (println "~a ~a" (pop-all v) (pop-all d)))              ; #(3 2 1) #(2 1)
```

`Push` と同じく、自前の型に実装でき、`:dyn Pop<int>` にもできる。
