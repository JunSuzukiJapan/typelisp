<!-- translated-from: docs/ja/reference/errors.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# 錯誤訊息

`typl` 提供的主要錯誤訊息的意義與修正方式。

## 1. 如何閱讀

錯誤以下列形式輸出到標準錯誤：

```text
error: 檔案:行:欄: 種類: 訊息
```

從 `種類` 可以知道錯誤是在什麼時候發現的。

| 種類 | 時機 | 意義 |
|---|---|---|
| `type error` | 執行之前（檢查時） | 型別或名稱的錯誤。該形式不會被執行 |
| （無種類） | 讀取時、檢查時 | 括號不對應之類的語法錯誤，或找不到名稱的錯誤 |
| `panic` | 執行中 | 不可恢復的失敗。執行 `unwind-protect` 的 cleanup 後停止 |

以 `warning:` 開頭的行是警告，處理會繼續。

`檔案:行:欄` 指向有錯誤的運算式的位置。在標準函式庫函式內部發生的執行期錯誤，指向程式呼叫該函式的位置。也有沒有位置的錯誤
（如 `error: panic: ...`）。

例：

```text
error: main.typl:1:24: type error: type mismatch: expected `i32`, found `string`
```

意思是 `main.typl` 第 1 行第 24 欄的運算式，在期望 `i32` 的位置是 `string`。

## 2. 檢查時的錯誤

執行之前發現的錯誤。修正之前該形式不會被執行。

### 2.1 型別

| 訊息 | 意義與修正方式 |
|---|---|
| ``type mismatch: expected `T`, found `U` `` | 需要 `T` 型別的位置是 `U` 型別的運算式。沒有隱式型別轉換，數值請以 `(as T x)` 轉換。`int` 與 `i32` 也是不同的型別 |
| ``integer literal 300 is out of range for u8 (0..=255)`` | 字面值放不進該型別。想要截斷時寫 `(as u8 300)` |
| ``unknown type `foo`: no type of that name is visible here. ...`` | 沒有該名稱的型別。型別要在第一次使用它的形式之前定義（型別沒有前置宣告）。如果本意是型別變數，請寫在函式名稱的 `<foo>` 等宣告位置（[語法參考 3.6](syntax.md#36-defstruct--結構使用者定義型別)） |
| ``cannot infer type argument `t` for `vector::new` `` | 無法決定型別引數。像 `(the Vector<int> (Vector::new))` 這樣以 `the` 寫出型別 |
| ``non-exhaustive match on `color`: 1/2 variants covered`` | `match` 沒有處理所有變體。加上缺少的變體分支或 `_` 分支 |
| ``type `pt` does not implement trait `eq` required by `where` clause on type parameter `a` `` | 函式要求某個 trait，但傳入的型別沒有實作。寫 `(impl Eq pt ...)`（[標準 trait](functions/traits.md)） |
| ``` `sq` does not implement `shape`, so it cannot be used as `:dyn shape` ``` | 在 `:dyn` 的位置傳入了沒有實作該 trait 的型別的值。寫 `impl` |
| ``` `error` is a trait, not a type — write `:dyn error` for a trait object ``` | 在型別的位置寫了 trait 名稱。寫成 `:dyn Error` |
| ``if: (if cond then else)`` | `if` 的形式不對。`if` 必須有 else。不需要 else 時使用 `when` |

### 2.2 名稱

| 訊息 | 意義與修正方式 |
|---|---|
| `no such function: bar` | 沒有該名稱的函式或方法。檢查拼寫 |
| ``no method `upcase` for type `int` (the type of the first argument, which selects the method); `upcase` is a method of `char`, `string` `` | 方法由第一個引數的型別選擇。存在該名稱的方法，但第一個引數的型別（這裡是 `int`）沒有。訊息最後是擁有該方法的型別一覽 |
| `unbound variable: y` | 沒有該名稱的變數。檢查拼寫以及繫結的範圍（是否在 `let` 外使用） |
| ``use: unresolved `nosuch` `` | 找不到 `use` 的模組。檔名與模組路徑的對應見[語法參考 3.11](syntax.md#311-檔案與模組的對應多檔案專案) |
| `unresolved path: c::hidden` | 模組存在，但沒有該名稱，或者因為沒有加 `pub` 而不可見 |
| `circular module dependency: a -> b -> a` | 模組之間互相 `use`。把共同的部分分到另一個模組 |
| ``return-from: no enclosing block named `nope` `` | 沒有與 `return-from` 的名稱相符的 `block` 包住它。函式名稱的 block 只能在該函式中使用 |

### 2.3 呼叫

| 訊息 | 意義與修正方式 |
|---|---|
| `f: expected 1 argument(s), got 2` | 引數個數不對 |
| `f: unknown keyword argument :b` | 傳入了該函式沒有的關鍵字引數 |
| `new: expected 1 field(s), got 2` | 傳給結構建構函式的值的個數與欄位數不符 |
| ``setf: cannot assign to constant `k` `` | 對以 `defconstant` 定義的名稱指派。需要修改的話改用 `defvar` |
| ``defsignature: `later` has no definition in this file — a declaration promises one`` | 沒有定義以 `defsignature` 宣告的函式 |
| ``format: ~/nosuch/ — no argument here has a method `nosuch` of the shape ...`` | 沒有任何引數的型別擁有以 `~/name/` 呼叫的方法（[格式指令第 5 章](functions/format.md#5-name)） |

## 3. 讀取錯誤

| 訊息 | 意義與修正方式 |
|---|---|
| `unexpected end of input while reading a list` | 缺少右括號。位置指向讀取結束的地方（檔案結尾等），所以請尋找對應的左括號 |

## 4. 執行期錯誤（panic）

| 訊息 | 意義與修正方式 |
|---|---|
| `panic: divide by zero` | 整數或有理數除以零。浮點數除以零不會 panic，而是得到 `inf`/`NaN` |
| `panic: unwrap: called on none` | 對 `none` 進行了 `unwrap`。以 `match` 或 `unwrap-or` 處理 `none` 的情況 |
| `panic: Vector: index 5 out of bounds` | 索引超出範圍。以 `len` 確認長度，或使用超出範圍時回傳 `none` 的函式（`nth`、`pop` 等） |
| `panic: an integer argument does not fit a fixnum` | 對接受索引或個數的引數傳入了 63 位元放不下的 `int` |
| `throw: no enclosing (catch 'oops) for this throw` | 執行了沒有被同一標籤的 `catch` 包住的 `throw` |
| `panic: <訊息>` | 程式呼叫了 `(panic "<訊息>")`。`assert` 失敗時是 `assertion failed: ...` |

panic 即使發生在任務中也會停止整個行程（[語法參考 12.4](syntax.md#124-與其他功能的關係)）。想要恢復的失敗以 `Result` 表示
（[語法參考第 9 章](syntax.md#9-錯誤處理方針)）。

## 5. 警告

| 訊息 | 意義 |
|---|---|
| ``warning: redefining function `f` `` | 重新定義了同名的函式。後面的定義生效。在 REPL 中修改定義時會正常出現 |
