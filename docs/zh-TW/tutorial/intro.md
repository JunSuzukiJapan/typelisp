<!-- translated-from: docs/ja/tutorial/intro.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# 入門

本章從在 REPL 中對運算式求值開始，依序介紹函式、變數、條件分支、迴圈，以及串列與 `Vector`。`typl` 的建置方法請參閱
[README.md](../../../README.md)。

## 1. 啟動 REPL

不帶引數啟動 `typl` 就會進入 REPL（互動模式）。在 `typl>` 之後輸入運算式，它會立即被求值並顯示結果。輸入 `:quit` 結束。

```
$ typl
typl> (+ 1 2)
3
typl> :quit
```

以下以這種形式表示 REPL 中的輸入與結果。

## 2. 對運算式求值

typelisp 是一種 Lisp，所以運算式以括號括起來，**運算子或函式名稱寫在最前面**。要寫 `(+ 1 2)`，而不是 `1 + 2`。

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

數有以下幾種：

- **整數**的型別是 `int`。大小沒有上限。
- **小數**的型別是 `f64`。像 `1.5`、`2.0` 這樣帶小數點書寫。
- `int` 與 `f64` 不能混合計算。`(+ 1 2.0)` 是型別錯誤。需要轉換時寫 `(as f64 1)`。

```
typl> (* 123456789012345 123456789012345)
15241578753238669120562399025
typl> (/ 7 2)
3
typl> (/ 7.0 2.0)
3.5
```

兩個整數之間的 `/` 會得到捨去小數部分的整數（不會像 Common Lisp 那樣得到分數）。餘數用 `(mod 7 2)` 求得。

布林值是 `true` 與 `false`。

```
typl> (> 3 2)
true
typl> (and (> 3 2) (< 3 2))
false
```

## 3. 定義函式

函式用 `defun` 定義。**引數型別與回傳值型別一定要寫出。**

```lisp
(defun square ((n int)) int
  (* n n))
```

- `(n int)` 表示「`int` 型別的引數 `n`」。有多個引數時排列成 `((a int) (b int))`。
- 引數清單後面的 `int` 是回傳值型別。
- 函式本體最後一個運算式的值就是函式的回傳值。不寫 `return`。

```
typl> (defun square ((n int)) int (* n n))
typl> (square 12)
144
typl> (square "a")
error: <stdin>:1:9: type error: type mismatch: expected `int`, found `string`
```

型別不符的呼叫會在**執行之前**以型別錯誤回報。執行檔案時，只要任何一處有型別錯誤，程式就一行也不會執行。

要讓引數可以省略，使用 `&optional`。寫上預設值後，省略時引數會取該值。

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

`format` 的第一個引數 `false` 表示「不輸出，而是以字串回傳」。每個 `~a` 的位置會依序換成後面的引數。

## 4. 變數

區域變數用 `let` 建立。

```lisp
(defun sum-of-squares ((a int) (b int)) int
  (let ((aa (square a))
        (bb (square b)))
    (+ aa bb)))
```

- `let` 變數的型別由初始值決定，不需要寫出。
- 同一個 `let` 中的變數不能互相參照。要用前一個變數建立下一個變數時，使用 `let*`。

```
typl> (let* ((a 1) (b (+ a 1))) (* a b))
2
```

改變變數的值使用 `setf`。**指派不能改變變數的型別。**

```
typl> (let ((x 1)) (setf x "a"))
error: <stdin>:1:22: type error: type mismatch: expected `int`, found `string`
```

全域變數用 `defvar` 定義。這裡要寫出型別。

```lisp
(defvar (counter int) 0)
```

## 5. 條件分支

### if

寫成 `(if 條件 為真時 為假時)`。**為假時的運算式不能省略。**

```lisp
(defun sign ((n int)) string
  (if (< n 0) "negative" "non-negative"))
```

- 只有 `bool` 型別的運算式才能當作條件。像 `(if 0 ...)` 這樣寫數字是型別錯誤。
- 為真時與為假時的運算式必須是同一型別。

為假時什麼都不做的話，使用 `when`（相反的情況用 `unless`）。

```lisp
(when (> n 100)
  (println "large")
  (println "really large"))
```

### cond

條件有三個以上時，`cond` 比較好讀。最後的 `else` 在所有條件都不成立時執行。

```lisp
(defun describe-number ((n int)) string
  (cond ((< n 0) "negative")
        ((= n 0) "zero")
        ((< n 10) "small")
        (else "large")))
```

### match

依值的形狀分支時使用 `match`。

```lisp
(defun day-name ((d int)) string
  (match d
    (0 "Sun")
    (6 "Sat")
    (_ "weekday")))
```

`_` 符合任何值。`int` 的值有無數個，所以沒有 `_` 分支就會出現「沒有涵蓋所有情況」的錯誤。`match` 真正派上用場的地方，是拆解下一章
[型別基礎](types.md)中出現的 `Option` 以及自己定義的型別。

## 6. 迴圈

函式可以呼叫自己。

```lisp
(defun fact ((n int)) int
  (if (= n 0) 1 (* n (fact (- n 1)))))
```

```
typl> (fact 30)
265252859812191058636308480000000
```

固定次數的迴圈使用 `dotimes`。`i` 從 0 變化到 `n - 1`。

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

另外還有 `while`、`do`，以及與 Common Lisp 相同的擴充 `loop`。擴充 `loop` 的子句詞以關鍵字書寫（`:for`、`:collect` 等）。

```
typl> (loop :for i :from 1 :to 5 :collect (* i i))
#<vector<int> 1 4 9 16 25>
```

## 7. 串列與 Vector

### Vector

要並列保存同一型別的值，使用 `Vector<T>`。`T` 是元素的型別。

```lisp
(let ((v (the Vector<int> (Vector::new))))
  (push v 3)
  (push v 1)
  (push v 2)
  (println "~a" v))
;; 顯示 #<vector<int> 3 1 2>
```

- 只寫 `(Vector::new)` 無法決定元素型別，所以用 `(the Vector<int> ...)` 指定型別。
- `(push v x)` 加到尾端，`(get v i)` 讀取第 `i` 個元素，`(len v)` 取得長度。
- 用超出範圍的索引 `get` 時，程式會因錯誤而停止。

### lambda 與高階函式

匿名函式用 `lambda` 建立。與 `defun` 一樣要寫出引數與回傳值的型別。

```
typl> ((lambda ((x int)) int (* x 2)) 21)
42
```

`map`、`filter`、`sort`、`foldl` 等接受以 `(iter v)` 變成**迭代器**的 `Vector`。對象在前，函式在後。
上面的 `v` 是以 `let` 綁定的，在該 `let` 之外無法使用。下例先以 `defvar` 定義 `v`。

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

要逐一處理元素，使用 `doiter`。

```lisp
(doiter (x (iter v))
  (println "item ~a" x))
```

接受函式作為引數的函式，把該引數的型別寫成 `(fn (引數型別...) 回傳值型別)`。用 `defun` 定義的函式也可以直接以名稱作為值傳遞。

```lisp
(defun twice ((f (fn (int) int)) (x int)) int
  (f (f x)))

(twice square 3)                                ; => 81
(twice (lambda ((x int)) int (* x 3)) 2)        ; => 18
```

### 串列（S 運算式）

用 `'(1 2 3)` 或 `(list 1 2 3)` 建立的串列是 **S 運算式資料**。元素的型別可以不一致。

```
typl> '(1 2 3)
(1 2 3)
typl> (list 1 "two" 'three)
(1 "two" three)
```

S 運算式資料主要用於在巨集（[巨集](macros.md)）與 `read` 中處理程式本身。要保存元素型別確定的資料，請使用 `Vector<T>`。S 運算式
串列可以用 `dolist` 走訪。

```lisp
(dolist (x '(1 2 3))
  (println "x=~a" x))
```

兩個值的組合用 `cons` 建立，用 `car` 與 `cdr` 取出。

```
typl> (let ((p (cons "age" 42))) (cdr p))
42
```

## 8. 寫在檔案中執行

程式可以寫在檔案（副檔名 `.typl`）中，用 `typl 檔名` 執行。要顯示結果使用 `println`。

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

- `println` 以與 `format` 相同的格式輸出，並在最後換行。不換行的是 `print`。
- `~a` 以給人閱讀的形式嵌入，`~s` 以可讀回的形式嵌入（字串會帶上 `"`）。
- 檔案由上而下依序讀取。**函式不能在定義之前呼叫。**

## 9. 接下來閱讀

- [型別基礎](types.md)：`Option`、`Result`、結構、列舉、泛型
- [給 Common Lisp 使用者](../guide/from-common-lisp.md)：給熟悉 Common Lisp 的讀者的差異一覽
