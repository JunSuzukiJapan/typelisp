<!-- translated-from: docs/ja/guide/modules.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# 模組與檔案結構

本指南說明如何組織由多個檔案構成的程式。詳細的語法規則見[語法參考](../reference/syntax.md#310-module--use--命名空間)的 3.10 至 3.13 節。

## 1. 一個檔案就是一個模組

在 typelisp 中，**檔案本身就是模組**。從原始碼根目錄看到的檔案相對路徑就是模組路徑。

| 檔案 | 模組 |
|---|---|
| `<根目錄>/geometry.typl` | `geometry` |
| `<根目錄>/geo/shapes.typl` | `geo::shapes` |
| `<根目錄>/net/http/client.typl` | `net::http::client` |

不需要在檔案開頭寫模組宣告。

## 2. 建立專案

在專案根目錄放一個名為 `typelisp.toml` 的檔案。內容可以是空的。

```
myapp/
├── typelisp.toml
├── main.typl
└── geometry.typl
```

想把原始碼放在 `src/` 底下時，在 `typelisp.toml` 中寫下這一行：

```toml
src = "src"
```

`typl` 從要執行的檔案所在目錄開始往上尋找 `typelisp.toml`，把找到的位置當作原始碼根目錄。找不到時，要執行的檔案所在目錄就是根目錄
（REPL 中是目前目錄）。

## 3. 公開定義並使用

`geometry.typl`：

```lisp
(pub defstruct point
  (pub x i32)
  (pub y i32)
  (tag string))                        ; 沒有 pub 的欄位從外部無法讀取

(defun square ((n i32)) i32 (* n n))   ; 沒有 pub 的函式從外部也無法呼叫

(pub defun dist2 ((a point) (b point)) i32
  (+ (square (- a::x b::x)) (square (- a::y b::y))))

(pub defmethod origin (point) point (point::new 0 0 "o"))
```

`main.typl`：

```lisp
(use geometry)

(let ((a (geometry::point::origin))
      (b (geometry::point::new 3 4 "b")))
  (println "~a" (geometry::dist2 a b)))
```

```sh
typl main.typl        # => 25
```

寫下 `(use geometry)` 時就會讀入 `geometry.typl`。不需要事先讀入。

### 公開的單位

- 函式、結構、列舉、全域變數、巨集、方法，各自只有加了 `pub` 的才對其他模組可見。寫法是像 `(pub defun ...)` 這樣在定義前面加 `pub`。
- 結構的**型別公開與欄位公開是分開的**。`(pub defstruct point ...)` 讓型別可見，只有寫成 `(pub x i32)` 的欄位才能從外部讀寫。
- 從外部使用未公開的名稱，會得到 `unresolved path: geometry::square` 這樣「無法解析」的錯誤。這與名稱拼錯時的訊息相同，所以拼寫正確卻
  無法解析時，請懷疑是否忘了加 `pub`。

可以加 `pub` 的定義一覽見[語法參考 3.13](../reference/syntax.md#313-pub--公開)。

## 4. `use` 的寫法

```lisp
(use geometry)              ; 引入模組。寫成 geometry::dist2 來使用
(use geometry::dist2)       ; 引入函式。可以用裸名稱 dist2
(use geometry::point)       ; 引入型別。可以寫 point::new、point::origin 以及型別註記中的 point
(use a::f b::g)             ; 可以一次寫多個
```

- **`use` 只對其後的形式生效。** 請放在檔案開頭。在 `use` 之前寫 `geometry::dist2` 會得到 `unresolved path`。
- 不 `use` 模組而直接寫完整路徑 `geometry::dist2` 也無法解析。讀入檔案的契機只有 `use`。
- 對已被占用的裸名稱再 `use` 同名的東西會產生警告。明知如此仍要引入時，使用 `shadowing-import`。
- 目錄中的模組寫成 `(use geo::shapes)`，引入後以最後一部分（`shapes::...`）參照。

### trait 方法的呼叫方式

在 `impl` 中實作的方法**屬於型別**而不是模組的函式，所以呼叫時不加模組名稱。

```lisp
(use shapes::core)
(println "~a" (area (core::square::new 4)))    ; 是 area 而不是 core::area
```

`impl` 中的方法即使不寫 `pub` 也一律公開。

trait 本身不能公開給其他模組。請把 trait 的定義、對它的 `impl`，以及透過 `:dyn` 使用該 trait 的程式碼放在同一個模組中。

## 5. 在檔案內劃分命名空間

想在一個檔案內再劃分命名空間時，使用 `module`。它會巢狀在檔案自身的模組內部。

```lisp
;; 在 main.typl 中
(module util
  (pub defun clamp ((x i32) (lo i32) (hi i32)) i32
    (if (< x lo) lo (if (> x hi) hi x))))

(println "~a" (util::clamp 15 0 10))    ; main::util::clamp
```

要把檔案其餘部分全部放進一個命名空間，也可以不用括號包住，寫成 `(in-module util)`。

## 6. 相依關係的限制

- **不能形成循環。** `a.typl` 中 `(use b)`、`b.typl` 中 `(use a)` 時，會得到 `circular module dependency: a -> b -> a` 錯誤。請把
  雙方都需要的定義移到第三個模組。
- **型別與函式都不能前置參照。** 即使在同一個檔案中，也不能在定義之前使用。相互遞迴的函式用 `defsignature` 先宣告其中一個
  （[語法參考 3.2](../reference/syntax.md#32-defsignature--前置宣告)）。

## 7. 執行順序

用 `typl main.typl` 執行時，依下列順序進行：

1. 讀入 `main.typl` 以及從它 `use` 的所有檔案，並進行型別檢查。**只要任何一處有型別錯誤，就什麼都不執行。**
2. 被 `use` 的模組的頂層運算式先於使用端執行。
3. `main.typl` 的頂層運算式由上而下依序執行。

把程式的進入點整理成 `main` 函式，並在檔案最後呼叫 `(main)`，同一個檔案也能直接用於[AOT 編譯](compile.md#3-用-aot-編譯產生執行檔)。

## 8. 與 `load` 的差異

`(load "path")` 與 Common Lisp 的 `load` 一樣，把檔案內容**原樣讀入目前的命名空間**。它不以模組包住，與 `pub` 也無關。用於讀入設定檔、
在 REPL 中重新讀入手邊的檔案等。把程式拆成元件時，請使用 `use`。
