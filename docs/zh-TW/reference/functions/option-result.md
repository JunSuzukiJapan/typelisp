<!-- translated-from: docs/ja/reference/functions/option-result.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# Option、Result 與錯誤型別

## 1. `Option<T>` / `Result<T,E>`

建構函式：`Option<T>` 是 `Some(T)` / `None`，`Result<T,E>` 是 `Ok(T)` / `Err(E)`。`E` 可以是任何型別——內建的具體錯誤型別，以及以
`defstruct`/`defenum` 撰寫的自己的型別，都可以直接放進去（第 3 章）。

| 名稱 | 形式 | Option | Result | 說明 |
|---|---|---|---|---|
| `unwrap` | `(unwrap self)` | `Option<T>→T` | `Result<T,E>→T` | 取出值。`None`/`Err` 時 panic |
| `unwrap-or` | `(unwrap-or self default)` | `(Option<T>,T)→T` | `(Result<T,E>,T)→T` | 值，或預設值 |
| `is-some` | `(is-some self)` | `Option<T>→bool` | — | 是否為 `Some` |
| `is-none` | `(is-none self)` | `Option<T>→bool` | — | 是否為 `None` |
| `is-ok` | `(is-ok self)` | — | `Result<T,E>→bool` | 是否為 `Ok` |
| `is-err` | `(is-err self)` | — | `Result<T,E>→bool` | 是否為 `Err` |

建構函式是 `Option::some`/`Option::none`/`Result::ok`/`Result::err`（或者在 `(use option)`/`(use result)` 之後使用裸名稱
`some`/`none`/`ok`/`err`）。

分支以 `match` 明確寫出。沒有相當於 Rust 的 `?` 的語法。

## 2. `Option<T>` 的執行期表示

與 Rust 一樣，**`Option<T>` 大多數情況下不建立盒子**。`some v` 就是 `v` 本身，`none` 是空串列的值，既沒有配置也沒有間接存取。
`Option<Sexpr>`（空串列為 `none`）、`Option<int>`、`Option<string>`、`Option<my-struct>`、`Option<f64>`、`Option<(fn ...)>` 都是這種形式。

只有當 `T` 的值無法與空串列的值區分時才裝箱：

| `T` | 表示 | 原因 |
|---|---|---|
| `Option<U>`（巢狀） | 盒子 | 內層的 `none` 會與外層的 `none` 成為同一個值 |
| `()` | 盒子 | `()` 的值就是空串列的值本身 |
| `ptr` / `c-long` / `c-ulong` | 盒子 | 64 位元全部都是值，沒有用於區分的餘地 |
| 其他 | 無盒子 | — |

表示只由型別決定，無法從值讀出。列印時依靜態型別還原 `(some ...)`/`none`，所以 `(format false "~a" opt)` 輸出 `(some 1)`。有兩個限制：

- **不能放進 `:dyn Trait`**（把 `(impl Speak Option<int> ...)` 過的值傳給 `:dyn Speak` 是錯誤）。
- 從 `Sexpr` 進行 `(the Option<T> ...)` 向下轉型時要**指明建構函式**——`(the Option<int> (some x))` / `(the Option<int> (none))`。繫結整體的
  `(the Option<int> o)` 形式是錯誤。

## 3. 錯誤型別與 `Error` trait

仿照 Rust 的 `std::error::Error`，**`Error` 不是型別而是 trait**。表示錯誤的具體型別依用途分開，都實作了 `Error`。

| 型別 | 來源 |
|---|---|
| `ParseIntError` | `parse-int` |
| `ParseFloatError` | `parse-float` |
| `ReadError` | `read` / `read-from-string` / `read-sexpr` |
| `EvalError` | `eval` / `macroexpand` |
| `FileError` | 檔案／串流操作（[串流與檔案](streams-files.md)） |
| `NetError` | 網路操作（[網路](network.md)） |
| `SimpleError` | `(SimpleError::new msg)` / `(simple-error msg)`。CL 的 `simple-error`——「只想說明發生了什麼」時的預設選擇 |
| `WrappedError` | `(wrap-error msg cause)`。同時攜帶自己的訊息與原因的型別，也是 `Error` trait 有 `source` 的原因 |

從 `ParseIntError` 到 `NetError` 都是「持有一個訊息字串的單一變體列舉」，型別名稱與變體名稱相同（`(match e ((ParseIntError m) m))`，建立為
`(ParseIntError::ParseIntError "...")`）。沒有任何特殊處理，與以 `(defstruct my-err (...))` / `(defenum my-err ...)` 撰寫自己的錯誤型別時
完全一樣。

| 名稱 | 形式 | 型別 | 說明 |
|---|---|---|---|
| `message` | `(message e)` | `Self→string` | 錯誤訊息（`Error` trait 的方法） |
| `source` | `(source e)` | `Self→Option<:dyn Error>` | 這個錯誤包裝的原因，沒有時為 `None`（Rust 的 `Error::source`） |
| `as-dyn-error` | `(as-dyn-error r)` | `Result<T,E>→Result<T,:dyn Error>`（`E` 實作 `Error`） | 把具體錯誤型別擴大為 trait 物件 |
| `describe-error` | `(describe-error e)` | `E→string`（`E` 實作 `Error`） | 訊息以及沿 `source` 追溯的原因鏈，每行一個原因。CL 中沒有對應物（Rust 的 "caused by"） |

為自己的錯誤型別實作 `Error`，就能**以同樣的方式**處理它與內建錯誤：

```lisp
(defstruct io-err (path string))
(impl Error io-err
  (message ((self Self)) string (append "io failed: " self::path))
  (source ((self Self)) Option<:dyn Error> (option::none)))

(defun open-it ((p string)) Result<i32, io-err>          ; 把具體型別直接放進 E
  (if (equal p "") (result::err (io-err::new "<empty>")) (result::ok 3)))

(defun describe ((e :dyn Error)) string (message e))     ; 不區分種類統一處理
(describe (io-err::new "/etc/app.conf"))
(describe (ParseIntError::ParseIntError "boom"))
```

把多種錯誤型別收集到一個 `Result` 中時，使用 `Result<T, :dyn Error>`（相當於 Rust 的 `Box<dyn Error>`），具體錯誤以 `as-dyn-error` 擴大。
因為沒有 `?`，這個轉換要明確寫出：

```lisp
(defun run ((s string)) Result<i32, :dyn Error>
  (match (as-dyn-error (parse-int s))            ; ParseIntError -> :dyn Error
    ((ok n) (as-dyn-error (open-it (if (= n 0) "" "f"))))   ; io-err -> :dyn Error
    ((err e) (result::err e))))
```

**型別與 trait 共享一個命名空間**（與 Rust 相同）。在同一模組中，`defstruct`/`defenum` 與 trait 不能同名，在型別的位置寫 trait 名稱會回報
「`error` is a trait, not a type — write `:dyn error`」。

不可恢復的失敗以 panic 表示。panic 的處理與 `catch`/`throw` 見[語法參考](../syntax.md#8-非區域跳出catch--throw--unwind-protect)，錯誤處理
方針見[同第 9 章](../syntax.md#9-錯誤處理方針)。
