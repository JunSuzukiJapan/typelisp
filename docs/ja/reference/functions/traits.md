# 標準トレイト

反復・比較・算術のトレイト。ほかの標準トレイトは、それぞれの章にある:
`Hash`（[HashTable](collections.md#4-hashtablekv)）、`Error`（[エラー型](option-result.md#3-エラー型と-error-トレイト)）、
`print-object`（[印字](printing.md#5-print-object型ごとの印字表現)）、ストリームのトレイト群と
`Pathish`（[ストリームとファイル](streams-files.md)）。どの型がどれを実装しているかは
[型の一覧](../types.md)。トレイトの定義方法は [構文リファレンス](../syntax.md#39-deftrait--impl--トレイト機構)。

## 1. `Iter` トレイトと反復

```lisp
(deftrait Iter ()
  (type Item)
  (next ((self Self)) Option<Item>))
```

`Vector<T>`/`HashTable<K,V>`/`Array<T>` はそれぞれ `vector-iter<T>`/`hashtable-iter<K,V>`/
`array-iter<T>` を介して `Iter` を実装している（`(iter コレクション)` でイテレータを取得）。
`Chan<T>` は自分自身が `Iter`（`recv` が `next` に当たる。[チャネル](concurrency.md#2-chant--チャネル)）。
`Sexpr` のリストには `Iter` を実装していない（要素型が一様でないため）。自前の型に `Iter` を
実装すれば、そのまま `doiter` で回せ、[シーケンス関数](sequences.md#4-iter-上のシーケンス関数)にも渡せる。

## 2. `Eq` / `Ord`（比較）

Rust の `PartialEq`/`PartialOrd` に当たる（名前は `Eq`/`Ord`）。ジェネリック関数の `where` 境界で
要素型の比較を要求するのに使う（`sort`/`member`/`assoc` 等）。

```lisp
(deftrait Eq ()
  (equals ((self Self) (other Self)) bool)                ; 実装必須
  (not-equals ((self Self) (other Self)) bool             ; デフォルト実装
    (not (equals self other))))
(deftrait Ord (Eq)                                        ; Eq を継承
  (less ((self Self) (other Self)) bool)                  ; 実装必須
  (less-equal ((self Self) (other Self)) bool (not (less other self)))
  (greater ((self Self) (other Self)) bool (less other self))
  (greater-equal ((self Self) (other Self)) bool (not (less self other))))
```

`Eq` を実装するには `equals` だけ、`Ord` を実装するには `less` だけ書けばよい。残りはデフォルト
実装が埋める。`Ord` は `Eq` を継承するので、`impl Ord X` の前に `impl Eq X` が必要。

各トレイトメソッドはそのまま関数として呼べる（`where (Eq A)`/`(Ord A)` 境界内、または実装済みの
具体型に対して）:

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `equals` | `(equals a b)` | `(A,A)→bool` where `Eq A` | 等しいか（Rust の `==`） |
| `not-equals` | `(not-equals a b)` | `(A,A)→bool` where `Eq A` | 等しくないか（`!=`） |
| `less` | `(less a b)` | `(A,A)→bool` where `Ord A` | `a < b` |
| `less-equal` | `(less-equal a b)` | `(A,A)→bool` where `Ord A` | `a <= b` |
| `greater` | `(greater a b)` | `(A,A)→bool` where `Ord A` | `a > b` |
| `greater-equal` | `(greater-equal a b)` | `(A,A)→bool` where `Ord A` | `a >= b` |

`Eq` 実装済み: 全数値型（`i8`〜`u32` / `f32` / `f64` / `int` / `ratio`）と `bool` `char`
`string` `symbol` `complex`、`Sexpr`（`eq`、つまり同一性。`match` の値パターンが使う）、
および `cons-cell<A,B>`（要素が `Eq` なら再帰的に）。`Ord` 実装済み: 全数値型と `char` `string`、
および `cons-cell<A,B>`（辞書順、要素が `Ord` なら）。

メソッド名が組み込みの演算子（`= /= < <= > >=`）や `eq`/`lt` と重ならないのは、組み込みは
再定義できず、各実装がそれらへ委譲するため。スカラの比較演算子そのものは受け手の型ごとの
組み込みメソッド（[数値](numbers.md)・[文字列と文字](collections.md)）。境界内では演算子で
書けば、トレイトのメソッドに読み替えられる（3 章）。

## 3. 算術トレイト（`Add` / `Sub` / `Mul` / `Div` / `Rem` / `Bits` / `Number`）

ジェネリックなコードが「足せる型」を要求するための層。**具体型の演算は組み込み演算子**
（[数値](numbers.md)）で、この層は通らない。

```lisp
(deftrait Add () (add ((self Self) (other Self)) Self))
(deftrait Sub () (sub ((self Self) (other Self)) Self))
(deftrait Mul () (mul ((self Self) (other Self)) Self))
(deftrait Div () (div ((self Self) (other Self)) Self))
(deftrait Rem () (remainder ((self Self) (other Self)) Self))
(deftrait Bits ()
  (bit-and ((self Self) (other Self)) Self)
  (bit-or  ((self Self) (other Self)) Self)
  (bit-xor ((self Self) (other Self)) Self)
  (bit-not ((self Self)) Self)
  (shift ((self Self) (count int)) Self))                 ; 距離は常に int（ash と同じ）
(deftrait Number (Add Sub Mul Div Rem Ord))               ; メソッド無し・6つの合成
```

**境界内では演算子で書ける。** `where` で束縛された型変数が受け手のとき、演算子は
トレイトメソッドへ読み替えられる（`+`→`add`、`-`→`sub`、`*`→`mul`、`/`→`div`、
`rem`→`remainder`、`logand`→`bit-and`、`=`→`equals`、`<`→`less` …）:

```lisp
(defun sum3<T> ((a T) (b T) (c T)) T (where (Number T))
  (+ a b c))                                              ; = (add (add a b) c)
```

トレイト側のメソッド名が `+` でないのは、`+` が組み込みメソッド名で `impl` が再定義を
拒むため（`cannot redefine built-in method`）。`Neg` は無い——`(- x)` は
`(- (- x x) x)` へ展開されるので `Sub` だけで足りる。

実装済み: `Add`/`Sub`/`Mul`/`Div`/`Rem`/`Number` は全数値型（`complex` を除く）、`Bits` は全整数型と `int`。
