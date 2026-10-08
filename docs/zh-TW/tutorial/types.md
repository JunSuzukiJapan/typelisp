<!-- translated-from: docs/ja/tutorial/types.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# 型別基礎

typelisp 是靜態型別語言。本章說明型別檢查能為你做什麼、常用的型別（`Option`、`Result`、結構、列舉）以及泛型。閱讀本章前請先讀
[入門](intro.md)。

## 1. 什麼是靜態型別

在 typelisp 中，所有運算式的型別都在程式執行前決定。型別不符的運算式在執行前就會出錯。

```lisp
(defun square ((n int)) int (* n n))

(defun main () ()
  (println "start")
  (println "~a" (square "3")))    ; 型別錯誤

(main)
```

執行這個檔案時，連 `start` 都不會顯示，就因型別錯誤而停止。

```
error: main.typl:5:25: type error: type mismatch: expected `int`, found `string`
```

需要寫出型別的地方是函式的引數與回傳值、全域變數，以及結構的欄位。`let` 變數的型別由初始值決定。

主要型別：

| 型別 | 值的例子 |
|---|---|
| `int` | `42`、`-7`（任意精度整數） |
| `i8` `i16` `i32` `u8` `u16` `u32` | 固定寬度整數 |
| `f64` `f32` | `1.5` |
| `bool` | `true`、`false` |
| `char` | `#\a` |
| `string` | `"hello"` |
| `symbol` | `'foo`、`:key` |
| `()` | 不回傳值的函式的回傳型別 |

沒有在執行期查詢值的型別的手段（沒有 Common Lisp 的 `typep` 或 `type-of`），因為所有型別在執行前都已知。

## 2. `Option<T>`：可能沒有的值

typelisp 沒有 `nil`。「可能沒有值」以 `Option<T>` 型別表示。`Option<T>` 的值要嘛是持有一個 `T` 值的 `some`，要嘛是什麼都不持有的
`none`。

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

`Option<int>` 不是 `int`，不能直接用於計算。`(+ (safe-div 10 2) 1)` 是型別錯誤。要使用其中的值，用 `match` 區分 `some` 與 `none`。

```lisp
(defun show-div ((a int) (b int)) string
  (match (safe-div a b)
    ((some q) (format false "~a" q))
    ((none) "undefined")))
```

- 在 `(some q)` 分支中，內容被繫結到變數 `q`。
- `match` 會檢查各分支是否**涵蓋了所有情況**。忘了寫 `(none)` 分支是型別錯誤。

### 為什麼沒有 nil

在許多語言中，`nil`（`null`）可以充當任何型別的值。因此，即使忘了處理「沒有值」的情況，也要到執行時才會發現。在 typelisp 中，可能沒有值的
地方是 `Option<T>` 型別，不在 `match` 中處理 `none` 的情況就無法通過型別檢查。遺漏在執行前就會被發現。

條件也遵循同樣的想法：`if` 的條件只能是 `bool`。沒有 Common Lisp 那種「`nil` 以外都為真」的規則。

### 常用操作

| 寫法 | 意義 |
|---|---|
| `(unwrap-or opt 預設值)` | `some` 時取內容，`none` 時取預設值 |
| `(unwrap opt)` | 取出內容。`none` 時程式停止 |
| `(is-some opt)` / `(is-none opt)` | 判斷是哪一種 |

標準函式庫中也有很多回傳 `Option` 的函式。例如 `position` 找到時以 `some` 回傳位置，找不到時回傳 `none`。

```lisp
(defvar (fruits Vector<string>) (Vector::new))
(push fruits "apple")
(push fruits "banana")

(match (position "banana" (iter fruits))
  ((some i) (println "found at ~a" i))
  ((none) (println "not found")))
```

## 3. `Result<T,E>`：可能失敗的操作

可能失敗的操作回傳 `Result<T,E>`：成功時是持有值 `T` 的 `ok`，失敗時是持有錯誤 `E` 的 `err`。

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

自己的函式也可以回傳 `Result`。

```lisp
(defun checked-age ((n int)) Result<int,string>
  (if (< n 0)
      (Result::err "age must not be negative")
      (Result::ok n)))
```

「沒有」不需要理由時用 `Option`，想說明失敗原因時用 `Result`。錯誤的處理方式在[錯誤處理](errors.md)中詳細說明。

## 4. `defstruct`：結構

帶有具名欄位的型別用 `defstruct` 定義。

```lisp
(defstruct point
  (x int)
  (y int))
```

定義之後可以使用以下功能：

```lisp
(let ((p (point::new 3 4)))     ; 建立（依欄位順序傳入引數）
  (println "~a" p::x)           ; 讀取欄位，也可寫成 (x p)
  (setf p::x 10)                ; 修改
  (println "~a" p))             ; #<point x: 10 y: 4>
```

要讓結構擁有函式，使用 `defmethod`。第一個引數（`self`）的型別決定方法屬於哪個型別。

```lisp
(defmethod norm2 ((self point)) int
  (+ (* self::x self::x) (* self::y self::y)))

(norm2 (point::new 3 4))        ; => 25
```

不寫 `self` 而只寫型別名稱，就會得到以 `point::origin` 形式呼叫的函式。

```lisp
(defmethod origin (point) point
  (point::new 0 0))

(point::origin)                 ; => #<point x: 0 y: 0>
```

## 5. `defenum`：幾種形狀之一

像「圓、矩形或點」這樣屬於幾種形狀之一的值，用 `defenum` 定義。每一種形狀稱為**變體**。每個變體可以持有不同數量、不同型別的值。

```lisp
(defenum shape
  (circle int)        ; 半徑
  (rect int int)      ; 寬與高
  (dot))              ; 不持有值
```

值要像 `shape::circle` 這樣加上型別名稱來建立。在 `match` 中依變體名稱拆解。

```lisp
(defun area ((s shape)) int
  (match s
    ((circle r) (* 3 r r))
    ((rect w h) (* w h))
    ((dot) 0)))

(area (shape::circle 2))        ; => 12
(area (shape::rect 3 4))        ; => 12
```

這裡 `match` 同樣會檢查是否涵蓋所有情況。之後為 `shape` 加上變體時，所有沒有處理該變體的 `match` 都會變成型別錯誤，因此不會漏掉需要修改的地方。

寫了 `(use shape)` 之後，就可以不帶型別名稱寫 `(rect 5 6)`。

`Option` 與 `Result` 也是以這種機制建立的列舉。

## 6. 泛型

適用於任何型別的函式，在名稱後面寫上 `<T>` 這樣的**型別參數**來定義。

```lisp
(defun first-or<T> ((v Vector<T>) (default T)) T
  (if (= (len v) 0)
      default
      (get v 0)))
```

呼叫時不指定型別，`T` 由引數決定。

```lisp
(defvar (ints Vector<int>) (Vector::new))
(defvar (names Vector<string>) (Vector::new))
(push names "Ann")

(first-or ints 7)          ; T 是 int
(first-or names "none")    ; T 是 string
(first-or ints "none")     ; 型別錯誤：ints 是 Vector<int>，所以 T 是 int
```

結構與列舉也可以是泛型的。`Vector<T>`、`Option<T>`、`Result<T,E>` 就是這種型別。

```lisp
(defstruct pair<A,B>
  (left A)
  (right B))

(pair::new 1 "one")        ; pair<int,string>
```

在泛型函式中對 `T` 一無所知，所以不能比較或相加 `T` 的值。要加上「只要能比較的型別就好」之類的條件，使用 trait（[trait](traits.md)）。

## 7. 為型別取別名

`deftype` 可以為型別取別名。

```lisp
(deftype meters int)
(defun double ((m meters)) meters (* m 2))
```

`meters` 只是 `int` 的另一種寫法，並不是新的型別。在需要 `meters` 的地方傳入一般的 `int` 也不會出錯。想區分它們時，像
`(defstruct meters (value int))` 這樣建立結構。

## 8. 接下來閱讀

- [trait](traits.md)：讓型別擁有共同的操作
- [型別一覽](../reference/types.md)：內建型別以及各自實作的 trait
