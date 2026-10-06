<!-- translated-from: docs/ja/tutorial/errors.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# 錯誤處理

typelisp 的錯誤處理把失敗分成兩類來考慮。

| 失敗的種類 | 例子 | 表示方式 |
|---|---|---|
| 可能發生的失敗（可恢復） | 檔案不存在、輸入不是數字 | 回傳 `Result<T,E>` |
| 程式的錯誤（不可恢復） | 索引超出範圍、對 `none` 的 `unwrap`、除以 0 | 以 `panic` 停止 |

此外還有一次跨越多層函式呼叫跳出的 `catch` / `throw`，以及無論如何離開都會執行清理的 `unwind-protect`。閱讀本章前請先讀
[型別基礎](types.md)中 `Result` 一節。

## 1. 回傳 `Result`，用 `match` 接收

撰寫一個從字串讀取埠號的函式。失敗有兩種情況：不是數字，或者超出範圍。

```lisp
(defun parse-port ((s string)) Result<int,string>
  (match (parse-int s)
    ((ok n) (if (and (>= n 1) (<= n 65535))
                (Result::ok n)
                (Result::err (format false "port out of range: ~a" n))))
    ((err e) (Result::err (message e)))))
```

呼叫端用 `match` 區分成功與失敗。

```lisp
(match (parse-port "99999")
  ((ok p) (println "port ~a" p))
  ((err m) (println "error: ~a" m)))
;; error: port out of range: 99999
```

- 回傳 `Result` 的函式的值，不在 `match` 中處理 `err` 的情況就無法使用。忘了處理失敗是型別錯誤。
- `parse-int` 的錯誤是 `ParseIntError` 型別的值。用 `(message e)` 取出訊息字串。

## 2. 把失敗傳給呼叫端

沒有 Rust 的 `?` 那樣的簡寫語法。連續呼叫回傳 `Result` 的函式時，用 `match` 寫出「失敗就原樣回傳」的處理。

```lisp
(defun add-ports ((a string) (b string)) Result<int,string>
  (match (parse-port a)
    ((err m) (Result::err m))
    ((ok x) (match (parse-port b)
              ((err m) (Result::err m))
              ((ok y) (Result::ok (+ x y)))))))

(add-ports "1" "2")       ; => (ok 3)
(add-ports "1" "x")       ; => (err "parse-int: invalid integer literal: \"x\"")
```

知道不會失敗時，或者在失敗了停止也無妨的小腳本中，可以用 `unwrap` 取出內容。如果是 `err` 就會 panic。用預設值就夠的話，使用
`unwrap-or`。

## 3. 建立自己的錯誤型別

以型別而不是字串表示錯誤，呼叫端就能依錯誤的種類分支。錯誤型別就是一般的 `defenum` 或 `defstruct`，再實作 `Error` trait。

```lisp
(defenum config-error
  (missing string)          ; 缺少設定項目
  (invalid string int))     ; 值不正確

(impl Error config-error
  (message ((self Self)) string
    (match self
      ((missing key) (format false "missing key: ~a" key))
      ((invalid key v) (format false "invalid value for ~a: ~a" key v))))
  (source ((self Self)) Option<:dyn Error> (Option::none)))

(defun check-workers ((n int)) Result<int,config-error>
  (if (> n 0)
      (Result::ok n)
      (Result::err (config-error::invalid "workers" n))))
```

- `message` 回傳錯誤的說明。
- `source` 回傳造成這個錯誤的另一個錯誤。沒有原因時為 `none`。

## 4. 合併不同種類的錯誤

在一個函式中同時呼叫 `parse-int`（`ParseIntError`）與 `check-workers`（`config-error`）時，錯誤型別有兩種，無法都寫在一個
`Result<T,E>` 的 `E` 中。這時把 `E` 設為 `:dyn Error`（實作了 `Error` 的某種錯誤），各個錯誤用 `as-dyn-error` 轉換。

```lisp
(defun load-count ((s string)) Result<int, :dyn Error>
  (match (as-dyn-error (parse-int s))
    ((ok n) (as-dyn-error (check-workers n)))
    ((err e) (Result::err e))))
```

傳入 `"4"`、`"-1"`、`"abc"` 的結果如下：

```
ok 4
err invalid value for workers: -1
err parse-int: invalid integer literal: "abc"
```

關於 `:dyn`，請參閱 [trait](traits.md) 第 5 節。

## 5. `panic`：程式的錯誤

遇到絕不應該出現的狀態時，用 `panic` 停止程式。

```lisp
(defun safe-get ((v Vector<int>) (i int)) int
  (if (< i (len v))
      (get v i)
      (panic (format false "index ~a out of range" i))))
```

```
error: main.typl:4:7: panic: index 3 out of range
```

- `panic` 的型別是 `!`（不回傳），可以寫在需要任何型別的地方。上例中 `if` 兩邊的型別能對上就是這個原因。
- 下列操作也會 panic：對 `none` 或 `err` 的 `unwrap`、以超出範圍的索引 `get`、整數除以 0。
- `panic` 會停止程式。即使發生在任務中，整個程式也會停止。
- 在 REPL 中，panic 不會結束 REPL，而是等待下一個輸入。
- 「還沒寫」可以寫成 `(todo)`，「不應該到達這裡」可以寫成 `(unreachable)`。兩者都會 panic。

`panic` 不是 `Result` 的替代品。使用者輸入、檔案是否存在這類可能發生的失敗，請使用 `Result`。

## 6. `catch` / `throw`：跨函式跳出

`throw` 會一口氣跳到外層同一標籤的 `catch`。中間隔著多少層函式呼叫都沒關係。

```lisp
(defun check-all ((v Vector<int>)) ()
  (doiter (x (iter v))
    (if (< x 0)
        (throw 'bad-input (format false "negative: ~a" x))
        ())))

(defun validate ((v Vector<int>)) string
  (catch 'bad-input
    (progn
      (check-all v)
      "all fine")))
```

`v` 中沒有負數時，`validate` 回傳 `"all fine"`；有 `-7` 時，會從 `check-all` 內部跳到 `catch`，回傳 `"negative: -7"`。

- 標籤直接寫成 `'bad-input` 這樣的符號。
- **每個標籤丟出的值的型別只有一種。** 上例中 `'bad-input` 攜帶 `string`，用同一標籤丟出 `int` 是型別錯誤。`catch` 本體的型別也必須
  與標籤的型別一致。

  ```
  error: ...: type error: type mismatch: expected `string`, found `int`
  ```

- `throw` 找不到同一標籤的 `catch` 時是錯誤。

如果只是想在函式中途返回，請使用 `return-from` 而不是 `catch` / `throw`。`return-from` 不能跨越函式，但作為交換，閱讀原始碼就能知道它
回到哪裡。

```lisp
(defun first-negative ((v Vector<int>)) Option<int>
  (doiter (x (iter v))
    (if (< x 0) (return-from first-negative (Option::some x)) ()))
  (Option::none))
```

## 7. `unwind-protect`：一定執行清理

`(unwind-protect 本體 清理)` 無論以何種方式離開本體，都會執行清理。正常結束、被 `throw` 跳出、發生 panic 時都會執行。

```lisp
(defun with-cleanup ((n int)) int
  (unwind-protect
    (progn
      (println "working on ~a" n)
      (if (< n 0) (throw 'bad-input "negative") ())
      (* n 2))
    (println "cleanup for ~a" n)))
```

```lisp
(println "~a" (with-cleanup 5))
;; working on 5
;; cleanup for 5
;; 10

(println "~a" (catch 'bad-input (progn (with-cleanup -1) "unreached")))
;; working on -1
;; cleanup for -1
;; negative
```

用於「開啟的檔案一定要關閉」「取得的鎖一定要歸還」之類的處理。標準函式庫的 `with-open-file` 與 `with-lock` 內部也使用了
`unwind-protect`。

## 8. 關於 Common Lisp 的條件系統

typelisp 沒有採用 Common Lisp 的條件系統（`handler-case`、`restart-case` 等）。它不會在型別中呈現函式可能引發哪些失敗，與靜態型別
不相容。可能發生的失敗用 `Result` 寫進型別，控制的轉移用 `catch` / `throw` 完成。

## 9. 接下來閱讀

- [並行](concurrency.md)：任務與通道
- [Option、Result 與錯誤型別](../reference/functions/option-result.md)：函式一覽
- [錯誤訊息](../reference/errors.md)：常見錯誤的意義與修正方式
