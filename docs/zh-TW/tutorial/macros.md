<!-- translated-from: docs/ja/tutorial/macros.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# 巨集

巨集是接收程式並回傳程式的函式。用巨集可以自己創造函式做不到的新語法。typelisp 的巨集與 Common Lisp 的 `defmacro` 是同一種機制。
閱讀本章前請先讀[入門](intro.md)中的「串列（S 運算式）」。

## 1. 巨集與函式的差異

函式接收的是引數**求值後的值**。巨集接收的是引數**求值之前的運算式本身**（作為 S 運算式資料），組裝出另一個運算式並回傳。回傳的
運算式會取代巨集呼叫的位置，之後才進行型別檢查與執行。這種取代稱為**展開**。

例如 `unless` 這樣的語法無法寫成函式。寫成函式的話，即使條件為真，本體也會先被求值。

## 2. `defmacro` 與準引用

建立一個只在條件為假時執行本體的 `my-unless`。

```lisp
(defmacro my-unless (test &rest body)
  `(if ,test () (progn ,@body ())))
```

- 巨集的引數不寫型別。所有引數都是 S 運算式資料。
- `&rest body` 把其餘的引數合在一起，作為一個串列接收。
- 以 `` ` ``（準引用）開頭的運算式會依原樣作為資料組裝。在其中：
  - `,test` 把變數 `test` 的內容嵌入該位置。
  - `,@body` 把串列 `body` 的元素展開並嵌入該位置。

```lisp
(my-unless (> 1 2)
  (println "one")
  (println "two"))
;; one
;; two
```

展開的結果可以用 `macroexpand-1` 確認。撰寫巨集時，先看展開結果是最快的方法。

```
typl> (macroexpand-1 '(my-unless (> 1 2) (println "one")))
(ok (if (> 1 2) () (progn (println "one") ())))
```

## 3. 展開結果也會經過型別檢查

巨集回傳的運算式與手寫的運算式一樣接受型別檢查。

```
typl> (defmacro add-a (x) `(+ ,x "a"))
typl> (add-a 1)
error: <stdin>:1:1: type error: type mismatch: expected `int`, found `string`
```

錯誤的位置是呼叫巨集的地方。

`if` 的為假分支不能省略、`if` 兩邊的型別必須一致，這些規則同樣適用於展開結果。上面的 `my-unless` 在最後寫成 `(progn ,@body ())`，
是為了無論本體最後一個運算式是什麼型別，都能讓 `if` 兩邊統一為 `()` 型別。

## 4. 變數名稱衝突與 `gensym`

直接寫一個交換兩個變數值的巨集，會是這樣：

```lisp
(defmacro swap-bad (a b)
  `(let ((tmp ,a))
     (setf ,a ,b)
     (setf ,b tmp)))
```

大多數情況下能運作，但呼叫端的變數剛好叫 `tmp` 時就會出問題。

```lisp
(let ((tmp 1) (other 2))
  (swap-bad tmp other)
  (println "tmp=~a other=~a" tmp other))
;; tmp=1 other=2   （沒有交換）
```

展開後是 `(let ((tmp tmp)) (setf tmp other) (setf other tmp))`，巨集建立的 `tmp` 遮蔽了呼叫端的 `tmp`。

為了避免這種情況，用 `gensym` 產生巨集內部使用的變數名稱。`gensym` 回傳一個在程式任何地方都寫不出來的新符號。

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

與 Common Lisp 一樣，typelisp 的巨集不會自動防止名稱衝突（非衛生巨集）。請記住：**巨集建立的繫結要用 `gensym`**。

依同樣的想法，指定次數重複執行本體的巨集可以這樣寫：

```lisp
(defmacro repeat (n &rest body)
  (let ((i (gensym "i")))
    `(dotimes (,i ,n) ,@body)))

(repeat 3 (println "hi"))
```

## 5. 依引數的形狀改變展開

巨集的本體是一般的 typelisp 程式碼，所以可以用 `if` 或 `match` 檢查引數，改變要展開的運算式。引數的型別是 S 運算式資料
（`Option<Sexpr>`），空串列是 `none`。

建立一個所有條件都為真時回傳 `true` 的 `my-and`。

```lisp
(defmacro my-and (&rest forms)
  (match forms
    ((none) 'true)                                    ; 沒有引數
    ((cons f more)
     (if (sexpr-null more)
         f                                            ; 只有一個
         `(if ,f (my-and ,@more) false)))             ; 兩個以上
    (_ (panic "my-and: not a list"))))

(my-and (> 2 1) (> 3 2))      ; => true
(my-and (> 2 1) (> 1 3))      ; => false
```

- `(cons f more)` 是把串列開頭取到 `f`、其餘取到 `more` 的模式。
- `sexpr-null` 判斷 S 運算式資料是否為空串列。
- 最後的 `_` 分支是必要的，因為 S 運算式資料還有串列以外的形狀（數字、字串等），`match` 要求涵蓋它們。`&rest` 的引數一定是串列，
  所以這個分支實際上不會執行。
- 巨集可以在展開結果中呼叫自己。展開會反覆進行，直到沒有巨集呼叫為止。

## 6. 可省略的引數

用 `&optional` 可以接收可省略的引數，也可以寫預設值。

```lisp
(defmacro inc-by (place &optional (n 1))
  `(setf ,place (+ ,place ,n)))

(let ((c 10))
  (inc-by c)
  (inc-by c 5)
  c)                            ; => 16
```

用 `&key` 也可以接收關鍵字引數（[語法參考 3.14](../reference/syntax.md#314-defmacro--巨集定義)）。

## 7. `macrolet`：只在局部使用的巨集

只在一個運算式中使用的巨集可以用 `macrolet` 定義，外部看不到。

```lisp
(macrolet ((sq (x) `(* ,x ,x)))
  (+ (sq 3) (sq 4)))            ; => 25
```

## 8. 使用巨集時的注意事項

- **巨集只能在定義之後呼叫。** 與函式一樣，請在檔案前面的位置定義。
- 給其他模組使用的巨集用 `(pub defmacro ...)` 公開。
- `when`、`unless`、`cond`、`and`、`or`、`dotimes` 等許多標準語法也是以巨集定義的。可以用 `(macroexpand '(when true 1))` 查看內容。
- 能用函式寫的請用函式寫。巨集不能作為值傳遞，而且不讀展開結果就不清楚它做了什麼。

## 9. 接下來閱讀

- [錯誤處理](errors.md)：`Result`、`panic`、`catch` / `throw`
- [巨集相關函式](../reference/functions/system.md#8-巨集)：`gensym`、`macroexpand` 等
