<!-- translated-from: docs/ja/tutorial/traits.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# trait

trait 是「這個型別支援這些操作」的約定。它讓多個型別擁有同名的操作，使用這些操作的函式就不必為每個型別分別撰寫。其機制與 Rust 的
trait 幾乎相同。閱讀本章前請先讀[型別基礎](types.md)。

## 1. 定義並實作 trait

把回傳圖形面積與名稱的操作定義為 trait `Shape`。

```lisp
(deftrait Shape ()
  (area ((self Self)) int)
  (name ((self Self)) string))
```

- `deftrait` 名稱後面的 `()` 是所繼承的 trait 清單（第 4 節）。沒有時留空。
- 每一行是一個方法宣告。`Self` 指「實作這個 trait 的型別」。

為型別實作 trait 時寫 `impl`。

```lisp
(defstruct circle (r int))
(defstruct rect (w int) (h int))

(impl Shape circle
  (area ((self Self)) int (* 3 self::r self::r))
  (name ((self Self)) string "circle"))

(impl Shape rect
  (area ((self Self)) int (* self::w self::h))
  (name ((self Self)) string "rect"))
```

實作的方法可以像一般函式一樣呼叫。

```lisp
(area (circle::new 2))     ; => 12
(area (rect::new 3 4))     ; => 12
```

只要漏掉 trait 中宣告的任何一個方法，就會在 `impl` 處出現型別錯誤。

## 2. trait 約束：「實作了這個 trait 的任何型別」

可以用 `where` 為泛型函式的型別參數加上條件。這稱為 **trait 約束**。

```lisp
(defun report<T> ((s T)) string
  (where (Shape T))
  (format false "~a: ~a" (name s) (area s)))

(report (circle::new 1))   ; => "circle: 3"
(report (rect::new 2 5))   ; => "rect: 10"
```

因為有 `(where (Shape T))`，本體中可以對 `T` 的值使用 `name` 與 `area`。沒有約束時對 `T` 一無所知，就無法呼叫。

傳入沒有實作 `Shape` 的型別時，會在呼叫處出現型別錯誤。

```
(report 5)
error: ...: type error: report: type `int` does not implement trait `shape` required by `where` clause on type parameter `t`
```

泛型函式會為每個被呼叫的型別產生專用的函式。不會在執行期檢查型別並分支。

## 3. 預設實作

在 trait 的方法中寫上本體，`impl` 省略該方法時就會使用它。

```lisp
(deftrait Describe ()
  (label ((self Self)) string)
  (describe ((self Self)) string
    (format false "<~a>" (label self))))

(impl Describe circle
  (label ((self Self)) string "a circle"))            ; describe 使用預設實作

(impl Describe rect
  (label ((self Self)) string "a rect")
  (describe ((self Self)) string "[rect]"))           ; 自己寫的優先

(describe (circle::new 1))     ; => "<a circle>"
(describe (rect::new 1 1))     ; => "[rect]"
```

## 4. 實作標準 trait

標準函式庫中也有 trait，實作之後，標準函式就能用於該型別。

| trait | 要實作的方法 | 可以使用的功能 |
|---|---|---|
| `Eq` | `equals` | `member`、`find`、`position`、`match` 的 `(= 式)` 模式等 |
| `Ord` | `less` | `less-equal`、`greater` 等。`Ord` 繼承 `Eq` |
| `print-object` | `print-object` | `println` 等顯示時的形式 |
| `Iter` | `next` | `doiter`、`map`、`filter`、`sort` 等 |
| `Error` | `message`、`source` | 作為錯誤型別使用（[錯誤處理](errors.md)） |

為表示金額的型別實作 `Eq` 與 `Ord`。`Ord` 繼承 `Eq`，所以要先寫 `Eq` 的 `impl`。

```lisp
(defstruct money (yen int))

(impl Eq money
  (equals ((self Self) (other Self)) bool (= self::yen other::yen)))

(impl Ord money
  (less ((self Self) (other Self)) bool (< self::yen other::yen)))

(greater (money::new 5) (money::new 3))     ; => true（Ord 的預設實作）
```

實作 `print-object` 就能決定 `println` 的顯示方式。被要求可讀回的形式時（如 `~s`），引數 `escape` 為 `true`。

```lisp
(impl print-object money
  (print-object ((self Self) (escape bool)) string
    (format false "~a yen" self::yen)))

(println "~a" (money::new 500))              ; 500 yen
```

與 trait 約束搭配，就能寫出適用於任何實作了 `Ord` 的型別的函式。

```lisp
(defun largest<T> ((v Vector<T>)) Option<T>
  (where (Ord T))
  (foldl (iter v)
         (lambda ((best Option<T>) (x T)) Option<T>
           (match best
             ((some b) (if (less b x) (Option::some x) best))
             ((none) (Option::some x))))
         (Option::none)))
```

傳入依 300、900、100 的順序放入 `money` 的 `Vector`，會回傳 `(some 900 yen)`。

## 5. `:dyn`：統一處理不同型別的值

`Vector<T>` 的元素都是同一型別，所以 `circle` 與 `rect` 不能放進同一個 `Vector<circle>`。要統一處理「實作了 `Shape` 的某種東西」，
使用 `:dyn Shape` 型別。

```lisp
(defun total-area ((shapes Vector<:dyn Shape>)) int
  (let ((sum 0))
    (doiter (s (iter shapes))
      (setf sum (+ sum (area s))))
    sum))

(let ((shapes (the Vector<:dyn Shape> (Vector::new))))
  (push shapes (circle::new 1))
  (push shapes (rect::new 2 3))
  (doiter (s (iter shapes))
    (println "~a -> ~a" (name s) (area s)))
  (println "total ~a" (total-area shapes)))
```

```
circle -> 3
rect -> 6
total 9
```

- `circle` 或 `rect` 的值放在需要 `:dyn Shape` 的地方時會自動轉換。
- `(area s)` 呼叫哪個型別的 `area`，在執行期由 `s` 內部值的型別決定。
- 把沒有實作 `Shape` 的型別的值放在 `:dyn Shape` 的位置是型別錯誤。

第 2 節的 trait 約束與 `:dyn` 的選擇：

| | trait 約束（`where`） | `:dyn Trait` |
|---|---|---|
| 呼叫對象何時決定 | 執行前 | 執行期 |
| 在一個 `Vector` 中混合不同型別 | 不可以 | 可以 |
| 可用的型別 | 無限制 | 結構、列舉、`int`、`string`、`f64` 等（不包括 `bool`、`char`、`symbol`、`i32` 等） |

可以放入的型別的確切清單見[語法參考 3.9](../reference/syntax.md#39-deftrait--impl--trait-機制)。

也有不能用於 `:dyn` 的 trait：方法在 `self` 以外的引數或回傳值中使用 `Self` 的情況（如 `Eq` 的 `equals`）。因為要到執行期才知道是哪個
型別，無法準備「同一型別的值」。

## 6. 限制

- 請把 trait 的定義、對它的 `impl`，以及透過 `:dyn` 使用該 trait 的程式碼放在同一個模組（檔案）中。目前還不能把 trait 公開給其他模組。
- 型別與 trait 位於同一命名空間。在同一模組中，型別與 trait 不能同名。

## 7. 接下來閱讀

- [巨集](macros.md)：自己定義語法
- [語法參考 3.9](../reference/syntax.md#39-deftrait--impl--trait-機制)：全面實作（blanket implementation）、關聯型別等
- [標準 trait](../reference/functions/traits.md)：標準函式庫中的 trait 一覽
