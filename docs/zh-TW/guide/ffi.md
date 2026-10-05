<!-- translated-from: docs/ja/guide/ffi.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# C FFI（defffi）

本指南說明如何從 typelisp 呼叫 C 函式。可以宣告的型別與限制一覽見[語法參考 3.3](../reference/syntax.md#33-defffi--宣告-c-函式ffi)。

## 1. 宣告並呼叫函式

用 `defffi` 宣告 C 函式的名稱與型別。

```lisp
(defffi (c-getpid "getpid") () i32)            ; typelisp 中的名稱與 C 的符號名稱
(defffi (c-strlen "strlen") (string) c-ulong)
(defffi (c-cos "cos") (f64) f64 :library "m")  ; 在 libm 中尋找
```

呼叫要以 `(unsafe ...)` 包住。

```lisp
(unsafe (c-getpid))        ; => 12345
(unsafe (c-cos 0.0))       ; => 1.0
```

之所以需要 `unsafe`，是因為編譯器無法確認宣告的型別是否與 C 端真正的型別一致。寫下 `unsafe` 表示由撰寫者承擔這項確認。忘了寫會得到說明
這一點的錯誤。

## 2. 撰寫安全的包裝

預期的用法是把 `unsafe` 限制在一處，對外呈現為一般的函式。

```lisp
(defun pid () i32 (unsafe (c-getpid)))
(defun str-len ((s string)) int (as int (unsafe (c-strlen s))))

(pid)              ; 呼叫端不需要 unsafe
(str-len "hello")  ; => 5
```

## 3. 型別的對應

| typelisp | C |
|---|---|
| `i8` `i16` `i32` / `u8` `u16` `u32` | 同寬度的整數 |
| `f32` / `f64` | `float` / `double` |
| `bool` | `bool`（`_Bool`） |
| `()` | `void` |
| `string` | `const char *` |
| `c-long` / `c-ulong` | `long` / `unsigned long`（也包括 `size_t`、`int64_t` 等） |
| `ptr` | 任意指標（`void *`、`FILE *` 等） |
| `(ptr T)` | 指向 `T` 的指標（[第 7 節](#7-c-結構)） |

### 字串

- 傳遞 `string` 時，會先複製成以 NUL 結尾的 C 字串再傳入，呼叫結束後釋放。字串中間有 NUL 會出錯。
- 回傳 `string` 的函式的結果也會被複製，C 端的記憶體不會被釋放。對於回傳需要呼叫端釋放的字串的函式（`strdup` 等），請以 `ptr` 接收
  並自己 `free`。
- 宣告為回傳 `string` 的函式回傳 NULL 時會出錯。可能回傳 NULL 的函式（`getenv` 等）請以 `ptr` 接收。

### `c-long` / `c-ulong` / `ptr`

這些型別只用於跨越與 C 的邊界傳遞值，**不能進行算術運算**。要作為 typelisp 的整數使用時，以 `as` 轉換。

```lisp
(as int (unsafe (c-strlen s)))      ; int 不會失去 64 位元的值
(try-as i32 (unsafe (c-strlen s)))  ; 放不進 i32 時為 none
(unsafe (c-malloc 16))              ; 整數字面值可以直接傳遞
```

`ptr` 是用來再傳回 C 函式的值。在 typelisp 端沒有讀取其內容的手段。

```lisp
(defffi (c-malloc "malloc") (c-ulong) ptr)
(defffi (c-free "free") (ptr) ())

(unsafe (let ((p (c-malloc 16)))
          (c-free p)
          ()))
```

這些型別只能出現在函式的引數、回傳值與區域變數中。不能作為結構的欄位、全域變數，也不能作為 `Vector` 等的型別引數。

## 4. 指定函式庫

省略 `:library` 時，會從行程中已經連結的內容（libc 等）尋找符號。其他函式庫中的函式以 `:library` 指定。

```lisp
(defffi (sqlite-version "sqlite3_libversion") () string :library "sqlite3")
```

- `"sqlite3"` 這樣的短名稱會依序尋找 `libsqlite3.dylib`、`libsqlite3.so`。
- 包含 `/` 的名稱視為路徑。
- 找不到宣告的符號時，會得到點出該名稱的錯誤。

## 5. AOT 編譯

使用了 `defffi` 的程式也可以直接用 [`compile-file`](compile.md#3-用-aot-編譯產生執行檔) 產生執行檔。`:library` 指定的函式庫會在
連結時自動加入，所以不需要為 `compile-file` 加引數。

## 6. 回呼

可以把 typelisp 函式傳給 C 函式，讓它回呼。在 `defffi` 的引數型別中寫函式型別，呼叫時在該位置寫函式名稱或 `lambda` 運算式。

```lisp
(defffi (c-free "free") (ptr) ())
(defffi (c-strdup "strdup") (string) ptr)
(defffi (c-strncmp "strncmp") (ptr ptr c-ulong) i32)
(defffi (text-of "strstr") (ptr string) string)          ; strstr(p, "") 回傳 p
(defffi (c-qsort "qsort") (ptr c-ulong c-ulong (fn (ptr ptr) i32)) ())

(defun sort-chars ((s string)) string
  (unsafe
    (let ((buf (c-strdup s)))
      (c-qsort buf (as c-ulong (length s)) 1
               (lambda ((a ptr) (b ptr)) i32 (c-strncmp a b 1)))
      (let ((r (text-of buf ""))) (c-free buf) r))))

(sort-chars "cadb")    ; => "abcd"
```

- 只能傳遞**沒有自由變數**的函式。頂層函式、`lambda`、`labels` 的區域函式都可以，但如果參照了外層的區域變數，型別檢查時就會出錯。C 只會
  傳遞宣告過的引數，沒有辦法把捕獲的變數送過去。需要保存狀態時請使用全域變數。
- 不能傳遞存放函式的變數。請在該位置直接寫函式名稱或 `lambda` 運算式。
- 回呼中發生的 panic 或 `throw` 會在 C 函式返回後傳到呼叫端。
- 只有在 typelisp 呼叫的 C 函式執行期間才能回呼。不能用於從 `atexit` 或訊號處理常式呼叫之類的情境。

## 7. C 結構

想把結構陣列等傳給 C 函式時，用 `def-c-struct` 宣告與 C 配置相同的結構，並在 `unsafe` 中配置。

```lisp
(unsafe (def-c-struct item (key i32) (tag u8)))    ; 在頂層的 unsafe 中宣告

(defffi (c-qsort "qsort") (ptr c-ulong c-ulong (fn ((ptr item) (ptr item)) i32)) ())

(defun put ((xs (ptr item)) (i int) (k i32)) i32
  (unsafe (let ((p (c-ref xs i))) (setf p::key k))))

(defun key-at ((xs (ptr item)) (i int)) int
  (unsafe (let ((p (c-ref xs i))) (as int p::key))))

(defun sorted-keys () int
  (unsafe
    (let ((xs (c-alloc item 4)))                    ; 4 個 item，內容為 0
      (put xs 0 3) (put xs 1 1) (put xs 2 4) (put xs 3 2)
      (c-qsort (as ptr xs) 4 8
               (lambda ((a (ptr item)) (b (ptr item))) i32 (- a::key b::key)))
      (+ (* 1000 (key-at xs 0)) (* 100 (key-at xs 1))
         (* 10 (key-at xs 2)) (key-at xs 3)))))

(sorted-keys)    ; => 1234
```

- `(c-alloc T n)` 配置 `n` 個 `T` 並回傳 `(ptr T)`。`(c-ref p i)` 是指向第 `i` 個元素的指標，`p::field` 是欄位，`(c-deref p)` 是
  指向 `i32` 等純量的指標所指的內容。它們都可以用 `setf` 寫入。
- 用 `(as ptr p)` 變成無型別的 `ptr`，傳給接受 `void *` 的 C 函式。
- `item` 的大小（這裡是 8）與各欄位的位置依與 C 相同的規則決定。

### 配置的記憶體的生命週期

配置的記憶體會在離開該函式中最外層的 `unsafe` 時釋放。因 panic 或 `throw` 離開時也一樣。因此 `(ptr T)` 的值不能帶出 `unsafe` 之外。
把它作為 `unsafe` 的值、在閉包中捕獲、傳給 `task`、用 `throw` 丟出，都會在型別檢查時出錯。想在外面使用的值，請在 `unsafe` 中複製成數值或
`defstruct`。

在 `lambda` 或 `labels` 的函式中配置時，請在其中寫 `unsafe`。

### C 配置的記憶體

以 `(ptr T)` 從 C 收到的指標（`defffi` 的回傳值、回呼的引數等），如果不是指向以 `c-alloc` 配置的記憶體內部，就會出錯。接收 C 以
`malloc` 配置的記憶體或 NULL 的函式，請以無型別的 `ptr` 宣告。

## 8. 做不到的事

- **可變引數函式**（`printf` 等）不能宣告。可變的部分依與固定引數不同的規則傳遞。請依使用的引數個數分別以不同的名稱宣告。
- **以值傳遞或回傳結構**不可行。請使用透過指標傳遞的函式。
- **泛型宣告**不可行。
- **與內建函式同名**不可行。
- **不能作為函式值傳遞。** 不能像 `(map xs c-abs)` 這樣傳遞，請以 `lambda` 包住。

  ```lisp
  (unsafe (lambda ((n i32)) i32 (c-abs n)))
  ```
