<!-- translated-from: docs/ja/reference/functions/traits.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# 標準 trait

用於迭代、比較與算術的 trait。其他標準 trait 在各自的章節中：`Hash`（[HashTable](collections.md#4-hashtablekv)）、`Error`
（[錯誤型別](option-result.md#3-錯誤型別與-error-trait)）、`print-object`（[列印](printing.md#5-print-object依型別的列印表示)）、串流的
trait 群與 `Pathish`（[串流與檔案](streams-files.md)）。哪些型別實作了哪些 trait 見[型別一覽](../types.md)。trait 的定義方式見
[語法參考](../syntax.md#39-deftrait--impl--trait-機制)。

## 1. `Iter` trait 與迭代

```lisp
(deftrait Iter ()
  (type Item)
  (next ((self Self)) Option<Item>))
```

`Vector<T>`/`HashTable<K,V>`/`Array<T>` 分別透過 `vector-iter<T>`/`hashtable-iter<K,V>`/`array-iter<T>` 實作 `Iter`（以 `(iter 集合)` 取得
迭代器）。`Chan<T>` 本身就是 `Iter`（`recv` 相當於 `next`。[通道](concurrency.md#2-chant--通道)）。`Sexpr` 串列沒有實作 `Iter`（元素型別
不統一）。為自己的型別實作 `Iter` 後，就可以直接用 `doiter` 走訪，也可以傳給[序列函式](sequences.md#4-iter-上的序列函式)。

## 2. `Eq` / `Ord`（比較）

相當於 Rust 的 `PartialEq`/`PartialOrd`（名稱為 `Eq`/`Ord`）。在泛型函式的 `where` 約束中用來要求元素型別可以比較（`sort`/`member`/`assoc` 等）。

```lisp
(deftrait Eq ()
  (equals ((self Self) (other Self)) bool)                ; 必須實作
  (not-equals ((self Self) (other Self)) bool             ; 預設實作
    (not (equals self other))))
(deftrait Ord (Eq)                                        ; 繼承 Eq
  (less ((self Self) (other Self)) bool)                  ; 必須實作
  (less-equal ((self Self) (other Self)) bool (not (less other self)))
  (greater ((self Self) (other Self)) bool (less other self))
  (greater-equal ((self Self) (other Self)) bool (not (less self other))))
```

實作 `Eq` 只需寫 `equals`，實作 `Ord` 只需寫 `less`。其餘由預設實作補齊。`Ord` 繼承 `Eq`，所以 `impl Ord X` 之前需要 `impl Eq X`。

各 trait 方法可以直接作為函式呼叫（在 `where (Eq A)`/`(Ord A)` 約束內，或對已實作的具體型別）：

| 名稱 | 形式 | 型別 | 說明 |
|---|---|---|---|
| `equals` | `(equals a b)` | `(A,A)→bool` where `Eq A` | 是否相等（Rust 的 `==`） |
| `not-equals` | `(not-equals a b)` | `(A,A)→bool` where `Eq A` | 是否不相等（`!=`） |
| `less` | `(less a b)` | `(A,A)→bool` where `Ord A` | `a < b` |
| `less-equal` | `(less-equal a b)` | `(A,A)→bool` where `Ord A` | `a <= b` |
| `greater` | `(greater a b)` | `(A,A)→bool` where `Ord A` | `a > b` |
| `greater-equal` | `(greater-equal a b)` | `(A,A)→bool` where `Ord A` | `a >= b` |

已實作 `Eq` 的：所有數值型別（`i8`〜`u32` / `f32` / `f64` / `int` / `ratio`）以及 `bool` `char` `string` `symbol` `complex`、`Sexpr`
（`eq`，即同一性。供 `match` 的值模式使用），以及 `cons-cell<A,B>`（元素為 `Eq` 時遞迴比較）。已實作 `Ord` 的：所有數值型別以及 `char`
`string`，以及 `cons-cell<A,B>`（字典序，元素為 `Ord` 時）。

方法名稱之所以不與內建運算子（`= /= < <= > >=`）或 `eq`/`lt` 重疊，是因為內建的不能重新定義，各實作會委派給它們。純量的比較運算子本身是
各接收者型別的內建方法（[數值](numbers.md)、[字串與字元](collections.md)）。在約束內寫運算子，會被解讀為 trait 方法（第 3 章）。

## 3. 算術 trait（`Add` / `Sub` / `Mul` / `Div` / `Rem` / `Bits` / `Number`）

讓泛型程式碼要求「可以相加的型別」的一層。**具體型別的運算使用內建運算子**（[數值](numbers.md)），不經過這一層。

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
  (shift ((self Self) (count int)) Self))                 ; 距離一律是 int（與 ash 相同）
(deftrait Number (Add Sub Mul Div Rem Ord))               ; 沒有方法，6 個的組合
```

**在約束內可以用運算子書寫。** 當接收者是由 `where` 約束的型別變數時，運算子會被解讀為 trait 方法（`+`→`add`、`-`→`sub`、`*`→`mul`、
`/`→`div`、`rem`→`remainder`、`logand`→`bit-and`、`=`→`equals`、`<`→`less` …）：

```lisp
(defun sum3<T> ((a T) (b T) (c T)) T (where (Number T))
  (+ a b c))                                              ; = (add (add a b) c)
```

trait 端的方法名稱不是 `+`，是因為 `+` 是內建方法名稱，`impl` 會拒絕重新定義（`cannot redefine built-in method`）。沒有 `Neg`——`(- x)` 展開
為 `(- (- x x) x)`，所以只需 `Sub`。

已實作：`Add`/`Sub`/`Mul`/`Div`/`Rem`/`Number` 用於所有數值型別（`complex` 除外），`Bits` 用於所有整數型別與 `int`。
