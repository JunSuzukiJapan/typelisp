<!-- translated-from: docs/ja/guide/from-common-lisp.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# 給 Common Lisp 使用者

typelisp 繼承了 Common Lisp（以下簡稱 CL）的語法與許多函式名稱，但它是靜態型別語言。因此 CL 的寫法不一定能直接通過。本指南整理了熟悉
CL 的人容易遇到的問題，以及改寫的方式。

## 1. 沒有 `nil` 與 `t`

布林值是 `true` 與 `false`。`nil` 與 `t` 沒有定義。

```lisp
(if (> x 0) "pos" "non-pos")
(defvar (debug bool) false)
```

- **只有 `bool` 能作為條件。** 把 `0` 或空串列寫成條件是型別錯誤。沒有「nil 以外都為真」的規則。
- **`if` 的 else 不能省略。** `(if c x)` 是錯誤。不需要 else 時使用 `when` / `unless`。
- **「沒有值」以 `Option<T>` 表示。** 在 CL 中以回傳 nil 表示「沒找到」的函式，在這裡回傳 `(some x)` 或 `none`。

  ```lisp
  (match (position 3 (iter v))
    ((some i) (println "found at ~a" i))
    ((none) (println "not found")))
  ```

- 空串列 `()` 依上下文，或是 Unit 型別的值（不回傳任何東西的函式的回傳值），或是 S 運算式資料的空串列。它與 `false` 是不同的值。

## 2. 寫出型別

函式的引數與回傳值必須寫型別。

```lisp
(defun area ((w i32) (h i32)) i32
  (* w h))

(defun first-or<T> ((v Vector<T>) (default T)) T      ; 泛型函式
  (unwrap-or (first (iter v)) default))
```

- 不能寫 `(defun f (x) x)` 這樣沒有型別的定義。
- `defvar` 等全域變數也要寫型別：`(defvar (count int) 0)`。
- `the` 不是執行期檢查，而是給型別檢查器的註記。
- **沒有在執行期檢查型別的手段。** 沒有 `typep` 與 `type-of`，因為值的型別在編譯時就已決定。想接受幾種型別之一時，用 `defenum` 建立和型別，
  或使用 trait。
- `deftype` 是型別別名。不能建立 `(deftype small () '(integer 0 9))` 這樣表示值範圍的型別。

整數的預設型別 `int` 是任意精度的，與 CL 的 integer 一樣大小沒有上限。也有固定寬度的 `i8` 至 `i32` / `u8` 至 `u32`。沒有 64 位元的固定
長度整數型別。

## 3. 把函式當作值

typelisp 不區分函式與變數的命名空間。函式名稱可以直接作為值傳遞。沒有 `#'` 與 `funcall`。

```lisp
(defun twice ((x int)) int (* 2 x))
(defun apply-to ((f (fn (int) int)) (x int)) int
  (f x))                                  ; 不用 funcall，直接呼叫

(apply-to twice 5)                        ; 是 twice 而不是 #'twice
```

- `+` 或 `1+` 等內建函式，在引數型別像 `(fn (int) int)` 這樣確定的位置，也可以直接作為值傳遞。傳給 `foldl`、`map` 這類泛型函式時，無法
  確定是哪個型別的 `+`，請以 `lambda` 包住。

  ```lisp
  (apply-to 1+ 5)                                          ; => 6
  (foldl (iter v) (lambda ((acc int) (x int)) int (+ acc x)) 0)
  ```

- 序列函式**對象在前，函式在後**：`(map it f)`、`(filter it f)`、`(foldl it f init)`。與 CL 的 `(mapcar f list)` 順序相反。
- `lambda` 不能使用 `&optional` 與 `&key`（可以使用 `&rest`）。
- **不能呼叫定義之前的函式。** 在 CL 中可以先呼叫之後才定義的函式，在這裡會得到 `no such function`。相互遞迴的函式用 `defsignature`
  先宣告其中一個。

  ```lisp
  (defsignature odd2 (i32) bool)
  (defun even2 ((n i32)) bool (if (= n 0) true (odd2 (- n 1))))
  (defun odd2 ((n i32)) bool (if (= n 0) false (even2 (- n 1))))
  ```

## 4. 串列與 Vector

相當於 CL 串列的是 **S 運算式資料**，其型別是 `Option<Sexpr>`（空串列是 `none`）。`(list 1 2 3)` 與 `'(a b c)` 都是這個型別。S 運算式
資料是給巨集與 `read` 處理的，一般的資料容器請使用 **`Vector<T>`**。

| 想做的事 | CL | typelisp |
|---|---|---|
| S 運算式的開頭與其餘 | `(car xs)` `(cdr xs)` | `(sexpr-car xs)` `(sexpr-cdr xs)` |
| 依序走訪 S 運算式串列 | `(dolist (x xs) ...)` | 相同 |
| 型別確定的元素序列 | 串列或向量 | `Vector<T>` |
| 序對 | `(cons a b)` | `(cons a b)`（型別是 `cons-cell<A,B>`） |
| 映射 | `(mapcar f xs)` | `(map (iter v) f)` |

`car` / `cdr` 是以 `cons` 建立的序對 `cons-cell<A,B>` 的存取器，不能用於 S 運算式串列。

建立 `Vector`：

```lisp
(let ((v (the Vector<int> (Vector::new))))
  (push v 1)
  (push v 2)
  (map (iter v) (lambda ((x int)) int (* x 10))))   ; => #<vector<int> 10 20>
```

`map`、`filter`、`sort`、`find` 等序列函式作用於實作了 `Iter` trait 的值。`Vector` 要用 `(iter v)` 變成迭代器再傳入。

## 5. 沒有多值

沒有 `values` 與 `multiple-value-bind`。在 CL 中回傳多個值的函式，回傳序對或結構。

| CL | typelisp |
|---|---|
| `(floor 7 2)` → 3, 1 | `(floor-div 7 2)` → `car` 為 3、`cdr` 為 1 的 `cons-cell` |
| `(decode-universal-time t)` → 9 個值 | `decoded-time` 結構 |
| `(read-from-string s)` → 值、位置 | `(read-from-string s)` 把值與位置的 `cons-cell` 放在 `Result` 中回傳。只要值時用 `(read s)` |

## 6. 沒有特殊變數（動態繫結）

`let` 一律是詞法繫結。即使用 `let` 重新繫結以 `defvar` 定義的變數，從那裡呼叫的函式看到的仍是原本的值。

```lisp
(defvar (*depth* int) 1)
(defun show () () (println "~a" *depth*))
(let ((*depth* 2)) (show))       ; CL 中為 2，typelisp 中為 1
```

想暫時改變 `*print-base*` 這類控制變數時，使用 `dlet`。它指派新值，並且無論以何種方式離開本體都會恢復原值。

```lisp
(dlet ((*print-base* 16))
  (format false "~a" 255))       ; => "ff"
```

`dlet` 修改的是全域變數本身，所以不是依執行緒的繫結。

## 7. 沒有採用條件系統

沒有 `define-condition`、`handler-case`、`handler-bind`、`restart-case`、`error`、`signal`。它們與靜態型別不相容。取而代之，分別使用
以下兩種機制：

- **可恢復的失敗回傳 `Result<T,E>`。** 呼叫端以 `match` 區分 `ok` / `err`。沒有相當於 Rust 的 `?` 的簡寫語法。

  ```lisp
  (match (parse-int "42x")
    ((ok n) n)
    ((err e) (progn (println "bad input: ~a" (message e)) 0)))
  ```

- **不可恢復的失敗（bug）是 `panic`。** `(panic "message")`、把 `none` 傳給 `unwrap`、除以 0、索引超出範圍等都屬於此類，程式會停止。
  `unwind-protect` 的清理會在停止前執行。

錯誤型別由 `Error` trait 統一，以 `(message e)` 取出訊息。建立自己的錯誤型別的方法見
[Option、Result 與錯誤型別](../reference/functions/option-result.md#3-錯誤型別與-error-trait)。`assert` 與 `warn` 可以像在 CL 中
一樣使用。

`catch` / `throw` / `unwind-protect` 是有的。不過 `catch` 的標籤限於不求值的符號字面值（`'done`），以同一標籤丟出的值的型別只有一種。

## 8. 沒有 CLOS

沒有 `defclass`、`defgeneric` 與方法組合。

- 資料型別以 `defstruct`（結構）與 `defenum`（和型別）定義。
- `defmethod` 定義只依**第一個引數的靜態型別**決定呼叫對象的方法。不進行多重分派。
- 要讓不同型別擁有共同的操作，使用 trait（`deftrait` / `impl`）。處理內部型別在執行期才決定的值時，使用 `:dyn Trait` 型別
  （[語法參考 3.9](../reference/syntax.md#39-deftrait--impl--trait-機制)）。

`defstruct` 的差異：

- 建構函式是 `型別名稱::new`：`(point::new 1 2)`。想要 `make-point` 這樣的名稱時，可以用 `(:constructor make-point)` 選項建立。
- 存取器除了 `(x p)`，也可以寫成 `p::x`。以 `(setf p::x 5)` 修改。
- 不會產生述詞（`point-p`）。沒有 `:conc-name`、`:type`、`:named`。
- `:include` 只是繼承槽，並不會成為父型別的子型別。

## 9. 以模組取代套件

沒有套件。命名空間是模組，檔案本身就是模組。以 `module::name` 取代 `pkg:symbol`，以 `use` 引入（[模組與檔案結構](modules.md)）。

關鍵字 `:foo` 是有的，它是求值為自身的符號。因為沒有套件，冒號也是名稱的一部分：`(symbol->string :foo)` 回傳 `":foo"`。

## 10. 讀取與語法的差異

- 不區分大小寫（符號在讀取時變成小寫）。與 CL 相同。
- 沒有 `#'`（第 3 節）。無法讀取複數字面值 `#c(...)`，複數以 `(complex 1.0 2.0)` 建立。
- 擴充 `loop` 的子句以關鍵字書寫：`(loop :for i :from 1 :to 3 :collect i)`。不以關鍵字開頭的 `loop` 是簡單的無限迴圈，以 `(break)`
  或 `(return 值)` 離開。`return` 離開的是最內層的迴圈（離開函式用 `return-from`）。
- `format` 的輸出目的地是 `false`（回傳字串）、`true`（標準輸出）或串流。格式指令與 CL 相同。
- 從字串讀取用 `(read "...")`，從串流讀取用 `(read-sexpr s)`。兩者都回傳 `Result`。
- `eval` 在求值之前對傳入的運算式進行型別檢查，回傳 `Result`。與原始碼中一樣不能前置參照。
- 沒有 `eval-when`。
- 函式名稱不使用 `?` 或 `!` 字尾。述詞與 CL 一樣以 `-p` / `p`（`zerop`、`sexpr-null`），或在前面加上 `is-`（`is-some`）命名。

## 11. 名稱不同的主要函式

| CL | typelisp |
|---|---|
| `string-upcase` / `string-downcase` | `upcase` / `downcase` |
| `read`（從串流讀取） | `read-sexpr` |
| `pathname` | `to-pathname` |
| `floor` 等的雙引數版本 | `floor-div` `ceiling-div` `round-div` `truncate-div` |
| `mapcar` | `map`（引數順序相反。第 4 節） |
| `length`（向量） | `len` |
| `hash-table-count` | `count` / `size` |

函式一覽見[內建函式](../reference/functions/README.md)。

## 12. 其他沒有的東西

- `progv`、`symbol-function`、`symbol-value`
- `*readtable*` 與 `copy-readtable`、`readtable-case`（讀取巨集本身可以用 `set-macro-character` 定義）
- 邏輯路徑名稱、萬用字元路徑名稱
- `input-stream-p` / `output-stream-p`（串流的方向由型別決定）
