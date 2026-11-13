<!-- translated-from: docs/ja/reference/functions/traits.md @ cedb434055399f2f83e17dcdaa55007fe4fc9dad -->
# 標準 trait

用於迭代、比較、算術與存取元素的 trait。其他標準 trait 在各自的章節中：`Hash`（[HashTable](collections.md#4-hashtablekv)）、`Error`
（[錯誤型別](option-result.md#3-錯誤型別與-error-trait)）、`print-object`（[列印](printing.md#5-print-object依型別的列印表示)）、串流的
trait 群與 `Pathish`（[串流與檔案](streams-files.md)）。哪些型別實作了哪些 trait 見[型別一覽](../types.md)。trait 的定義方式見
[語法參考](../syntax.md#39-deftrait--impl--trait-機制)。

## 1. `Iter` trait 與迭代

```lisp
(deftrait Iter ()
  (:type Item)
  (defmethod next (self) Option<Item>))
```

`Vector<T>`/`HashTable<K,V>`/`Array<T>` 分別透過 `vector-iter<T>`/`hashtable-iter<K,V>`/`array-iter<T>` 實作 `Iter`（以 `(iter 集合)` 取得
迭代器）。`Chan<T>` 本身就是 `Iter`（`recv` 相當於 `next`。[通道](concurrency.md#2-chant--通道)）。`Sexpr` 串列沒有實作 `Iter`（元素型別
不統一）。為自己的型別實作 `Iter` 後，就可以直接用 `doiter` 走訪，也可以傳給[序列函式](sequences.md#4-iter-上的序列函式)。

## 2. `Equal` / `Ord`（比較）

`Equal` 是 `equal` 所用的 trait，相當於 Rust 的 `PartialEq`。`Ord` 相當於 Rust 的 `PartialOrd`。在泛型函式的
`:where` 約束中用它們來要求元素型別可比較。

```lisp
(deftrait Equal ()
  (defmethod equal (self (other Self)) bool))                ; 必須實作
(deftrait Ord (Equal)                                     ; 繼承 Equal
  (defmethod less (self (other Self)) bool)                  ; 必須實作
  (defmethod less-equal (self (other Self)) bool (not (less other self)))
  (defmethod greater (self (other Self)) bool (less other self))
  (defmethod greater-equal (self (other Self)) bool (not (less self other))))
```

實作 `Equal` 要寫 `equal`，實作 `Ord` 只需寫 `less`。`Ord` 的其餘部分由預設實作補上。`Ord` 繼承 `Equal`，
所以 `impl Ord X` 之前需要 `impl Equal X`。

`(equal a b)` 呼叫 `a` 的型別的 `Equal` 實作。對未實作 `Equal` 的型別寫它會是型別錯誤。依同一性比較用
`eq`/`eql`，依結構比較用 `equalp`。這三個可用於任何型別的值。

各 trait 方法可以直接當作函式呼叫（在 `where (Equal A)`/`(Ord A)` 約束內，或對已實作它的具體型別）：

| 名稱 | 形式 | 型別 | 說明 |
|---|---|---|---|
| `equal` | `(equal a b)` | `(A,A)→bool` where `Equal A` | 是否相等（Rust 的 `==`） |
| `less` | `(less a b)` | `(A,A)→bool` where `Ord A` | `a < b` |
| `less-equal` | `(less-equal a b)` | `(A,A)→bool` where `Ord A` | `a <= b` |
| `greater` | `(greater a b)` | `(A,A)→bool` where `Ord A` | `a > b` |
| `greater-equal` | `(greater-equal a b)` | `(A,A)→bool` where `Ord A` | `a >= b` |

已實作 `Equal` 的：所有數值型別（`i8`〜`u32` / `f32` / `f64` / `int` / `ratio`）與 `bool` `char` `string`
`symbol` `complex`、`Sexpr`、`cons-cell<A,B>` 和元組（元素為 `Equal` 時逐元素比較）、`Option<T>` 與
`Result<T,E>`（變體相同且內容 `equal` 時）。內建型別各自的內建方法 `equal` 原樣就是 `Equal` 的實作。數值用
`=` 比較，字串區分大小寫依內容比較。已實作 `Ord` 的：所有數值型別以及 `char` `string`，以及
`cons-cell<A,B>`（元素為 `Ord` 時依字典序）。

串列（`Sexpr`）的 `equal` 是 CL 的 `equal`：cons 依結構、字串依內容、其餘原子用 `eql` 比較。這種比較在執行時
走訪值，不呼叫任何方法。因此串列中的結構體與 CL 一樣依同一性比較，與其型別的 `impl Equal` 無關。

`Ord` 的方法名不與內建運算子（`< <= > >=`）或 `lt` 重名，是因為內建的無法重新定義，而各實作都委派給它們。
純量比較運算子本身是依接收者型別的內建方法（[數值](numbers.md)・[字串與字元](collections.md)）。在約束內
寫運算子，會被讀作 trait 的方法（第 3 章）。`=` 變為 `equal`，`/=` 變為 `(not (equal a b))`。

## 3. 算術 trait（`Add` / `Sub` / `Mul` / `Div` / `Rem` / `Bits` / `Number`）

讓泛型程式碼要求「可以相加的型別」的一層。**具體型別的運算使用內建運算子**（[數值](numbers.md)），不經過這一層。

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
  (defmethod shift (self (count int)) Self))                 ; 距離一律是 int（與 ash 相同）
(deftrait Number (Add Sub Mul Rem Ord))                   ; 沒有方法，五個的組合
```

**在約束內可以用運算子書寫。** 當接收者是由 `:where` 約束的型別變數時，運算子會被解讀為 trait 方法（`+`→`add`、`-`→`sub`、`*`→`mul`、
`/`→`div`、`rem`→`remainder`、`logand`→`bit-and`、`=`→`equal`、`<`→`less` …）：

```lisp
(defun sum3<T> ((a T) (b T) (c T)) T (:where (Number T))
  (+ a b c))                                              ; = (add (add a b) c)
```

trait 端的方法名稱不是 `+`，是因為 `+` 是內建方法名稱，`impl` 會拒絕重新定義（`cannot redefine built-in method`）。沒有 `Neg`——`(- x)` 展開
為 `(- (- x x) x)`，所以只需 `Sub`。

已實作：`Add`/`Sub`/`Mul`/`Div`/`Rem`/`Number` 用於所有數值型別（`complex` 除外），`Bits` 用於所有整數型別與 `int`。


**`Div` 用關聯型別 `Output` 表明結果型別。** 與 CL 一樣，整數之間的 `/` 是 `rational`，因此 `int` 的
`Output` 是 `rational`，其他數值型別則是自身。由於 `Output` 因型別而異，`Number` 不包含 `Div`（繼承
列表必須固定所有關聯型別）。做除法的泛型函式用 `(:where (Div T (Output R)))` 接收結果型別，或者用
`(:where (Div T (Output T)))` 要求在 `T` 內封閉的除法（此時不能傳 `int`）。

## 4. `Push` trait（新增元素）

```lisp
(deftrait Push ()
  (:type Item)
  (defmethod push (self (x Item)) ()))
```

可以一個一個新增元素的集合。泛型函式用 `(:where (Push C (Item T)))` 要求「可以新增 `T` 的型別」，並對任何這樣的集合用同一個 `(push c x)` 新增。

| 型別 | `push` 的意義 |
|---|---|
| `Vector<T>` | 追加到末尾 |
| `Deque<T>` | 追加到末尾（`push-back`） |
| `Array<T>` | 在填充指標的位置新增（`push-extend`）。沒有填充指標時 panic |
| `HashSet<T>` | 放入集合（`insert`） |

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

為自己的型別實作 `Push`，就能傳給同樣的泛型函式。也可以像 `:dyn Push<int>` 那樣作為 trait 物件。

## 5. `Pop` trait（取出元素）

```lisp
(deftrait Pop ()
  (:type Item)
  (defmethod pop (self) Option<Item>))
```

可以取回 `push` 最後新增的元素的集合。為空時回傳 `none`。泛型函式以 `(:where (Pop C (Item T)))` 要求它。

| 型別 | `pop` 的意義 |
|---|---|
| `Vector<T>` | 取出末端元素 |
| `Deque<T>` | 取出末端元素（`pop-back`） |
| `Array<T>` | 取出 fill pointer 前面的那個元素。沒有 fill pointer 時 panic |

`HashSet<T>` 沒有「最後的元素」，所以它是 `Push` 但不是 `Pop`。

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

與 `Push` 相同，可以為自己的型別實作它，也可以像 `:dyn Pop<int>` 那樣作為 trait 物件。
