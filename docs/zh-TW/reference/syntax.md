<!-- translated-from: docs/ja/reference/syntax.md @ 6a9ad7ad4f7c098262628978d36af26e7cdd0401 -->
# typelisp 語法參考

typelisp 是一種靜態型別的 Lisp，語法採用 S 運算式。內建函式與方法的一覽見[內建函式](functions/README.md)，型別一覽見 [types.md](types.md)，
錯誤訊息的讀法見 [errors.md](errors.md)。

## 1. 詞法元素

- **不區分大小寫。** 符號在讀取時全部正規化為小寫。
- **註解**：從 `;` 到行尾（行註解）。`#| ... |#`（可以巢狀的區塊註解）。
- **讀取時求值**：`#.(式)` **在讀取的同時執行**後面的形式，並把它的值當作讀到的東西。這是讀取器不只是文字函式的唯一地方。能到達的範圍取決於
  讀取路徑，這一點與 CL 相同：
  - `(load ...)` 與 REPL 逐個形式求值，所以可以呼叫**同一段文字中前面定義的函式**（CL 的 `load`）。
  - 模組檔案作為一個單元檢查，執行由 `use` 它的一方進行，所以 `#.` 能到達的只有標準函式庫與該工作階段已經執行過的東西。檔案本身的定義以及它
    `use` 的模組的定義都**還沒有執行**（與 CL 的 `compile-file` 需要 `eval-when` 相同）。
  - 程式中的 `read` / `read-from-string` 也會對 `#.` 求值（與 CL 相同）。
  - 把 `*read-eval*`（預設 `true`）設為 `false`，`#.` 在任何地方都是讀取錯誤——這是不讓作為資料讀取的文字執行程式碼的開關（與 CL 相同）。每次
    遇到 `#.` 都會讀取它，所以 `setf` 從下一個讀取的形式開始生效。在 `with-standard-io-syntax` 中為 `true`。
- **布林值**：`true` / `false`。
- **整數**：十進位（`42`、`-7`）。可以前置正負號 `+`/`-`。十進位以外以 CL 的基數巨集 `#b`/`#o`/`#x`/`#NNr` 書寫（正負號在標記之後，
  `#x-ff`）。CL 中沒有 `0x` 前綴，所以不採用——`0xff` 會被讀作符號。
  沒有型別註記的整數字面值預設是 `int`（任意精度，[數值](functions/numbers.md#3-任意精度整數-int)）——大小沒有上限。**期望型別是固定寬度整數
  型別時，字面值就是該型別，並會檢查該型別能否容納這個值**——`(the u8 300)` 是型別錯誤（想要截斷時寫 `(as u8 300)`）。`(the u32 4294967295)`
  與 `(the u32 #xFFFFFFFF)` 正是藉由這條規則才能寫出。`int` 的值是放進 63 位元立即值還是成為多倍精度，由值的大小決定，沒有專門的語法
  （與 CL 相同）。
- **浮點數**：包含小數點或指數記號（`e`/`E`）的數（`1.5`、`3.0e10`）。預設為 `f64`（期望型別是 `f32` 時為該型別）。
- **比例（ratio）**：`分子/分母`（只有十進位，例如 `1/3`）。讀取時依 CL 規格約分（`2/4` 為 `1/2`）。值為整數的（`4/2` 等）讀作 `int` 而不是
  `ratio`。分母為 `0`（`1/0`）是讀取錯誤。
- **字元**：`#\` 後接一個字元或具名字元。例如 `#\a` `#\Space` `#\Newline` `#\Tab` `#\Return` `#\Page` `#\Nul`（也可寫 `#\Null`）
  `#\Backspace`。名稱不區分大小寫。
- **字串**：`"..."`。跳脫序列有 `\n` `\t` `\r` `\0` `\\` `\"`（其他的 `\x` 就是 `x`）。
- **符號**：包含英數字與符號的任意記號（`+` `<=` `my-func` 等）。
  `]` 和 `}` 會結束一個記號，所以不能寫在符號中；在資料的開頭遇到它們是讀取錯誤。`[` 和 `{` 可以寫在符號中：與 CL 一樣，它們保留給程式設計師在[讀取巨集](#11-讀取巨集readtable)中使用。
- **關鍵字**：像 `:name` 這樣以冒號開頭的符號（依 CL）。它是自我求值的——不查找繫結，值就是它本身，靜態型別為 `symbol`。同名的關鍵字一律是同一
  個物件（`(eq :foo :FOO)` 為真。與其他符號一樣會被轉為小寫）。冒號本身是名稱的一部分，`(symbol->string :foo)` 是 `":foo"`（typelisp 沒有
  套件機制，所以與 CL 的 `symbol-name` 不同）。單獨的 `:` 或像 `:a:b` 這樣包含額外冒號的是讀取錯誤。以 `keywordp` 判斷。以 `::` 開頭的不是
  關鍵字，而是絕對路徑（見下文）。
  另外，`:dyn` 是只用於型別位置的保留關鍵字，寫在其他位置是錯誤（見[第 2 章](#2-型別的寫法)）。
- **串列**：`(a b c)`。點對 `(a . b)` 也可以讀取。
- **向量**：`#(1 2 3)`（與 CL 相同）。內容全部是字面值，不會求值——`#(a b)` 中的 `a` 是符號而不是變數。元素型別由上下文決定（`(the Vector<i32> #(1 2))`），沒有上下文時取第一個元素的型別（`#(1 2 3)` 是 `Vector<int>`）。元素型別必須一致，`#(1 "a")` 是型別錯誤。既沒有元素也沒有上下文的 `#()` 也是型別錯誤。每次求值都會建立新的向量。在預期 S 運算式資料的位置（`(the Option<Sexpr> #(1 x))`、`'#(..)`、`read` 讀到的資料），它是元素全部為資料的 `Vector<Option<Sexpr>>`——即 `Sexpr` 的 `vector` 變體。
- **陣列**：`#2A((1 2) (3 4))`（與 CL 相同）。`#` 與 `A` 之間的數是維數，內容中串列巢狀的前這麼多層就是各個維度。`#0A x` 是只有一個元素的零維陣列。同一層的串列長度不一致是讀取錯誤。型別的決定方式與向量相同，結果是 `Array<T>`（沒有元素時需要上下文，例如 `(the Array<f64> #2A(()))`）。作為 S 運算式資料，它是 `Array<Option<Sexpr>>`——即 `Sexpr` 的 `array` 變體。
- **空串列 `()`**：依上下文，是 `Unit` 型別的值，或是 `Option<Sexpr>` 的 `none`。**`Sexpr` 沒有空串列的變體**——`Sexpr` 表示「非空的 S
  運算式」，S 運算式資料的型別是 `Option<Sexpr>`（見 [4.3 match](#43-match--模式比對) 的「`Option<Sexpr>` 的模式」）。
- **quote/quasiquote/unquote**：
  - `'x` → `(quote x)`
  - `` `x `` → `(quasiquote x)`
  - `,x` → `(unquote x)`（只在 quasiquote 中有意義）
  - `,@x` → `(unquote-splicing x)`（展開時作為串列元素拼接）
- **路徑 `::`**：`foo::bar` 讀作經由模組、型別、成員的路徑（不會成為一個符號名稱）。像 `::foo` 這樣以 `::` 開頭時是從根開始的絕對路徑。泛型
  引數內部的 `::`（`Vec<a::b>` 等）不當作路徑分隔字元。

## 2. 型別的寫法

在原始碼中，型別寫成一般的符號或串列。

- **基本型別**：`int` `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `bool` `char` `string` `symbol`。
  `int` 是整數（CL 的 integer——在 63 位元立即值與多倍精度之間自動轉換，[數值](functions/numbers.md#3-任意精度整數-int)），6 種固定寬度
  型別以寬度與正負號命名（沒有 64 位元整數型別——見[數值](functions/numbers.md#1-固定寬度整數)）。
- **有理數型別**：`ratio`（既約有理數）。依 CL 在堆積上配置，與 `int`/`f64` 等之間沒有隱式轉換（以 `as`/`try-as` 或轉換方法明確轉換。見
  [數值](functions/numbers.md#5-有理數-ratio)）。
- **C 邊界上的原始字**：`ptr`（不透明指標）、`c-long` / `c-ulong`。只用於 FFI，成為值需要 `(unsafe ...)`，可以出現的位置也有限
  （[3.3 defffi](#ptr--c-long--c-ulong--原始機器字)）。想要 64 位元整數時不要用它們——它們沒有算術。
- **不透明的可變型別**：`random-state`（亂數產生器的狀態）。不能放進 `Vector<T>`/`HashTable<K,V>`/`Sexpr`（可以放進 `Option<T>`/
  `Result<T,E>`）。
- **Unit 型別**：`()`
- **Never 型別**：`!`（`panic`/`unreachable`/`todo`/不回傳的迴圈等發散運算式的型別。適合任何期望型別）
- **函式型別**：`(fn (引數型別...) 回傳值型別)`。帶可變引數的函式型別為 `(fn (引數型別... &rest 元素型別) 回傳值型別)`。
- **泛型型別**：`Name<T1,T2,...>`（以不含空白的一個記號讀取）。
  例如：`Option<i32>` `Result<i32,ParseIntError>` `HashTable<string,i32>` `Vector<T>`。
  型別引數中也可以寫 unit 型別 `()`（`Result<(), FileError>`）。`(`/`)` 本來是切分記號的分隔字元，但只在角括號開啟期間，這一對字元可以通過。
  `()` 也可以當作欄位型別、引數型別使用。
- **泛型型別的套用形式**：`(Name T1 T2 ...)` —— 指稱與 `Name<T1,T2,...>` 相同型別的串列寫法。例如 `(vector char)` 與 `Vector<char>` 相同。
  名稱形式是通常的寫法，這種形式**是為型別引數無法寫進名稱的情況準備的**——型別引數本身是型別運算式，但在一個記號的名稱中只能寫名稱、`()` 與
  `:dyn`，不能寫函式型別（不存在 `Vector<(fn (i32) i32)>` 這種寫法）。實作顯示型別時也可能以這種形式出現，例如把 trait 的關聯型別代入簽章的結果。
- **限定型別名稱**：可以像 `module::Type` 這樣以 `::` 限定。
- **trait 物件型別**：`:dyn Trait`（以空白分隔的兩個詞構成一個型別）。表示具體型別在執行期決定的值，trait 方法的呼叫經由 vtable 進行動態分派。
  有關聯型別的 trait 依宣告順序以位置固定（`:dyn Iter<i32>` 把 `Item` 固定為 `i32`）。也可以寫在泛型引數內部：`Vector<:dyn Drawable>`
  `HashTable<string, :dyn Drawable>`。具體值在期望位置自動裝箱，明確形式為 `(as :dyn Trait 式)`。
  `:dyn Sub` 的值可以直接傳給要求其超 trait（遞移繼承的所有 trait）的 `:dyn Super` 的位置（向上轉型）。不能傳給沒有繼承關係的 trait。可以成為
  `:dyn` 的 trait 的條件見 [3.9 deftrait / impl](#39-deftrait--impl--trait-機制)。在型別位置以外寫 `:dyn` 是錯誤。
- 內建泛型型別：`Option<T>`（`Some(T)` / `None`）、`Result<T,E>`（`Ok(T)` / `Err(E)`）、`HashTable<K,V>`、`Vector<T>`，以及並行機制的
  `Task<T>` / `Thread<T>` / `Chan<T>`（[第 12 章](#12-並行任務)）。還有 S 運算式資料的型別 `Sexpr`。內建的具體錯誤型別有 `ParseIntError` /
  `ParseFloatError` / `ReadError` / `EvalError` / `FileError` / `NetError`，標準函式庫的結構有 `SimpleError` / `WrappedError`（`Error` 不是
  型別而是 trait——以 `:dyn Error` 使用）。一覽見 [types.md](types.md)。
- **型別與 trait 位於同一命名空間**（與 Rust 相同）：在同一模組中，型別（`defstruct`/`defenum`）與 trait（`deftrait`）不能同名。

## 3. 頂層定義

### 3.1 defun — 函式定義

```lisp
(defun name ((arg1 Type1) (arg2 Type2) ...) RetType
  body...)
```

- 引數型別與回傳值型別是必要的。
- 泛型函式在名稱後以角括號寫型別參數：`(defun name<T1,T2...> (params) Ret body...)`（與型別位置的 `Vector<T>` 相同的角括號語法）。
- `defun`/`lambda`/`defmethod` 在尾端寫 `&rest (name Type)` 可以接受可變引數：
  `(defun name ((a Type1) &rest (xs Type2)) Ret body...)`（在本體中 `xs` 一律繫結為 `Option<Sexpr>`——S 運算式的串列。呼叫端的每個實際引數各自
  作為 `Type2` 進行型別檢查後再包裝進 `Sexpr`）。
  `defmacro` 也有自己的 `&rest`，但不同之處在於它一律是無型別的 `Sexpr`（`defun`/`lambda` 明確寫出元素型別）。`fn` 型別也可以寫成
  `(fn (T1... &rest Te) Ret)` 的形式來表示可變引數函式的型別。
- **`&optional` / `&key`**（用於 `defun` 與 `defmethod`。`lambda`/`labels` 由於下述原因不在對象之內，`defmacro` 是下述的另一種實作）。順序依 CL
  為 `必要 &optional &rest &key`。每個參數寫成 `(name Type)` 或 `(name Type 預設式)`：

  ```lisp
  (defun greet ((name string) &optional (suffix string)) string      ; 沒有預設值
    (match suffix ((some s) (append name s)) ((none) name)))         ; 本體中是 Option<string>

  (defun pow ((b i32) &optional (n i32 2)) i32 ...)                  ; 有預設值
  (pow 3)      ; n = 2
  (pow 3 5)    ; n = 5

  (defun mk (&key (a i32 0) (b string "z")) string ...)
  (mk :b "q")  ; 呼叫端寫 `:name 值`，順序任意。省略的取預設值
  ```

  - **沒有寫預設式的參數的型別是 `Option<Type>`。** 省略時為 `none`，傳入時呼叫端寫的裸值自動包裝為 `some`。CL 中「以 supplied-p 變數得知是否
    省略」的東西，在這裡呈現在靜態型別端。
  - 寫了預設式時，型別保持宣告的 `Type`。省略時，那個**經過檢查的式子**原樣嵌入呼叫端（每次呼叫都求值）。
  - **`&key` 不能與 `&optional`/`&rest` 混在同一個引數清單中。** CL 本身有一個歧義（尾端的實際引數是由依位置填入的 `&optional` 接收，還是由依
    標籤比對的 `&key` 接收，取決於*值*），以禁止這種組合來避開。`&optional` 與 `&rest` 可以一起使用。
  - 泛型函式中也可以使用，但**只出現在被省略引數中的型別參數無法推論，是錯誤**（那裡沒有可以比對的值）。
  - **`defmethod` 也可以寫同樣的 3 個區段**（實例方法與靜態函式都可以）。在接收者之後排列 `&optional`/`&rest`/`&key`：

    ```lisp
    (defstruct box (w i32) (h i32))
    (defmethod grow ((self box) &key (dw i32 0) (dh i32 0)) i32 ...)
    (grow (box::new 1 2) :dh 10)

    (defmethod origin (point &key (x i32 0) (y i32 0)) point (point::new x y))   ; 靜態函式
    (point::origin :y 7)
    ```

    泛型型別的方法中也可以使用，但**寫了預設式的參數的型別中不能寫擁有者的型別參數**（與 `defun` 對自身型別參數的限制相同。省略時嵌入的是
    *經過檢查的*式子，其型別不能停留在抽象的變數上）。
  - **trait 的方法中不能使用。** `deftrait` 端沒有這種語法，如果只有 `impl` 端能宣告區段，那麼以 `:dyn` 為接收者的呼叫（從 trait 的宣告填入
    引數）與以具體型別為接收者的呼叫（從 `impl` 的宣告填入）就會成為不同的東西。vtable 槽的引數個數是固定的。
  - **`lambda` / `labels` 中不能使用**（可以使用 `&rest`）。要填入被省略的引數，呼叫端需要讀取**被呼叫端經過檢查的預設式**，而它只能從依名稱
    解析的簽章中得到。`lambda` 作為值傳遞，描述這個值的只有函式型別 `(fn ...)`——那裡沒有放式子的地方，如果放了，「簽章相同而只有預設值不同的
    兩個 lambda」就會成為不同的型別。`&rest` 只涉及型別的問題，所以可以寫進函式型別。
- **前置參照以 `defsignature` 宣告**（見下文）。沒有宣告的名稱不能在定義之前呼叫——因為頂層是依原始碼順序逐個形式檢查與執行的。
- 要求 trait 約束時，在本體之前寫 `where` 子句：`(defun name<T> (params) Ret (where (Trait T (AssocName ConcreteType)...)) body...)`
  （以 `(AssocName ConcreteType)` 固定關聯型別可以省略）。
- **文件字串**：在 `where` 子句（如果有）之後、本體開頭放置字串字面值，它就成為文件字串（依 CL）。但僅當其後至少還有一個本體形式時——單獨的
  字串仍是回傳值，不會被視為文件字串：`(defun f () string "doc" "value")` 帶有文件字串並回傳 `"value"`，而 `(defun f () string "value")`
  沒有文件字串並回傳 `"value"`。可以用 `(documentation name)` 取出（[文件字串](functions/system.md#7-文件字串--documentation)）。

### 3.2 defsignature — 前置宣告

```lisp
(defsignature name (引數型別...) 回傳型別)
(pub defsignature name (引數型別...) 回傳型別)
```

要呼叫在自己**之後**定義的 `defun`，先這樣宣告。相互遞迴只能這樣寫：

```lisp
(defsignature odd2 (i32) bool)
(defun even2 ((n i32)) bool (if (= n 0) true  (odd2 (- n 1))))
(defun odd2  ((n i32)) bool (if (= n 0) false (even2 (- n 1))))
```

引數**只列出型別**。沒有本體，所以沒有需要命名的對象。`&rest` 可以在最後寫成 `&rest 元素型別`。

宣告**會被檢查**：

- 後面的定義必須與宣告一致（引數的個數與型別、回傳型別、`&rest`、是否 `pub`）。不一致時在定義處出錯。
- 宣告了卻不定義是錯誤（在檔案／模組讀取完畢時回報）。REPL 不會在每次輸入後回報——因為宣告與定義應該可以分在不同的行輸入。
- 放在定義**之後**的宣告是錯誤，因為它什麼也做不了。

有 3 種東西不能宣告：

- **泛型函式。** 產生每種型別的實體需要本體，而宣告沒有本體。前置呼叫即使能解析，實體化也會失敗，所以在宣告時就拒絕。
- **`&optional`/`&key`。** 它們的簽章包含每個預設值**經過檢查的**式子（省略引數時原樣嵌入呼叫端），而宣告沒有地方放它。
- **`defun` 以外的東西。** `defmacro` 展開時需要巨集本體**已經執行過**，登記簽章無法取代。型別（`defstruct`/`defenum`/`deftrait`）的登記是
  「登記型別的程式碼本身所需要的東西」，不像簽章那樣自成一體。`defmethod` 登記在所屬的型別上，所以隨型別而定。

CL 中對應的是 `(declaim (ftype (function (i32) bool) even2))`，但那伴隨著整套宣告系統，而且只是**建議**。這裡是靜態型別，所以宣告會被檢查。

### 3.3 defffi — 宣告 C 函式（FFI）

```lisp
(defffi (名稱 "c_symbol") (引數型別...) 回傳型別)
(defffi (名稱 "c_symbol") (引數型別...) 回傳型別 :library "名稱")
(defffi 名稱 (引數型別...) 回傳型別)              ; 名稱 = C 的符號名稱
(pub defffi ...)
```

宣告 C 函式，使其可以呼叫。形式與 `defsignature` 相同——名稱、引數型別、回傳型別、沒有本體——但沒有本體的意義不同。`defsignature` 是「之後由
自己定義」的約定，而 `defffi` 是「本體已經由別人寫好並編譯了」的宣告。

```lisp
(defffi (c-abs "abs") (i32) i32)
(defffi (c-sqrt "sqrt") (f64) f64)
(defffi (c-getpid "getpid") () i32)

(unsafe (c-abs -5))                          ; => 5
```

之所以可以把 typelisp 端的名稱與 C 的符號名稱分開寫，是因為 typelisp 的識別字通常含有 `-`，而 C 的識別字不能含有。省略 C 名稱時，名稱直接作為
C 的符號名稱。

**呼叫需要 `(unsafe ...)`**（即使是只處理純量的函式）。編譯器沒有辦法確認宣告的 C 簽章與真正的是否一致，只能相信宣告——`unsafe` 是承擔這份
責任的標記。預期的寫法是只包一次，做成安全的包裝函式：

```lisp
(defun abs-i32 ((n i32)) i32 (unsafe (c-abs n)))
(abs-i32 -3)                                 ; 之後不再需要 unsafe
```

可以書寫的型別是 `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `bool` `()`（void）`string` `ptr` `c-long` `c-ulong`，以及型別化指標
`(ptr T)`（[見後文](#def-c-struct-與型別化指標--配置-c-結構)）。

`string` 是 `const char *`。typelisp 的字串不以 NUL 結尾，本身也可能含有 NUL，所以**傳遞時複製成 C 字串**，呼叫結束後釋放。字串中有 NUL 時是
錯誤——C 只看到它之前的部分，等於默默傳了另一個字串。

**回傳時也會複製**，不會釋放——C 回傳的東西屬於 C，可能像 `getenv` 那樣指向靜態表。回傳需要呼叫端釋放的記憶體的函式（`strdup` 等）請以 `ptr`
接收並自行釋放。

結果指向引數內部的函式（`strchr`、`strstr`）也能正確運作。順序是先複製再釋放引數。

宣告為回傳 `string` 的函式回傳 NULL 時是錯誤。因為 `string` 沒有表示「沒有」的值。可能為 NULL 時請以 `ptr` 接收。

```lisp
(defffi (c-strlen "strlen") (string) i32)
(defffi (c-getenv "getenv") (string) string)

(unsafe (c-strlen "hello"))                  ; => 5
(unsafe (c-getenv "PATH"))                   ; => "/usr/bin:..."
```

寫了 `:library` 時開啟該共享函式庫並在其中尋找符號。省略時從**行程本身**（已經連結的所有東西——包括 libc）中尋找。`sqlite3` 這樣的短名稱依
`libsqlite3.dylib` / `libsqlite3.so` 的順序尋找，含有 `/` 時視為路徑。開啟的函式庫不會關閉——指向其中函式的程式碼會繼續執行，所以正確的生命
週期只有行程的生命週期。

#### ptr / c-long / c-ulong —— 原始機器字

`ptr` 是不透明指標（`void *`、`FILE *`，或者宣告所指的任何東西）。`c-long` / `c-ulong` 是 C 的 `long` / `unsigned long`（`size_t`、
`int64_t`、`intptr_t` 也一樣）。

```lisp
(defffi (c-malloc "malloc") (c-ulong) ptr)
(defffi (c-free "free") (ptr) ())
(defffi (c-strlen "strlen") (string) c-ulong)

(unsafe (let ((p (c-malloc 16))) (c-free p) ()))
```

**不叫 `i64` / `u64` 是刻意的。** 這個語言沒有 64 位元整數型別——帶標籤的立即值只有 63 位元（[第 2 章](#2-型別的寫法)）。`c-long` 這個名稱在
說「這是跨越與 C 邊界的字，不是這個語言的整數」。

**沒有算術。** 不能寫 `(+ x 1)`。能提供卻沒有提供，是為了不讓無處保存、寬度與其他所有數都不同的值參與計算——與去掉 64 位元整數型別的理由相同。
有的**只是轉換**：

```lisp
(as i32 (unsafe (c-strlen "hello")))         ; 讀取回傳的東西
(as int (unsafe (c-strlen s)))               ; 要精確讀取用這個（int 不會失去 64 位元）
(try-as i32 (unsafe (c-strlen s)))           ; 詢問能否放下
(as c-ulong n)                               ; 從其他整數建立
```

整數**字面值**取期望的型別，所以只是傳遞的話不需要 `as`：

```lisp
(unsafe (c-malloc 16))                       ; 16 讀作 c-ulong
```

超出範圍的字面值與其他寬度一樣會被拒絕（`(c-malloc -1)` 放不進 `c-ulong`）。

**可以出現的位置有限。** 只有引數型別、回傳型別與區域變數。以下都是錯誤：

```lisp
(defstruct handle (p ptr))          ; 結構的欄位
(defenum maybe (none) (some ptr))   ; 列舉的欄位
(defvar (block ptr) ...)            ; 全域變數
(defffi f ((vector ptr)) i32)       ; 型別引數內部
```

理由只有一個：這些都是**槽會為內容加上標籤**的地方。加上標籤，指標的最高位元就會遺失——與去掉 64 位元整數型別的理由相同，所以即使在 `unsafe`
中也不允許。這不是許可的問題，而是那種表示不存在。

出於同樣的理由，它也不能成為被巢狀函式**捕獲**的區域變數（被捕獲的繫結放進單元，單元會為內容加上標籤）。這在編譯時就能知道，由
`(compile f)` 回報。

GC 不追蹤 `ptr`。它指向堆積之外，這樣是正確的。

有 4 種東西不能宣告：

- **可變引數**（`printf`）。可變的部分依與固定引數不同的規則傳遞（在 AArch64 Darwin 上是堆疊），無法從固定的簽章正確呼叫。`&rest` 會被拒絕。
- **以值傳遞與以值回傳結構。** 理由相同（取決於各平台的傳遞規則）。把可寫的型別限定在上面的清單中，使之無法寫出。
- **泛型。** C 中沒有對應物。
- **與內建同名。** 編譯後的呼叫會依那個名稱解析到內建函式，與其默默出錯不如拒絕。

#### 回呼 —— 讓 C 回呼

在引數型別中寫函式型別 `(fn (型別...) 回傳型別)`，該引數就成為 C 回呼的函式（回呼）。

```lisp
(defffi (c-qsort "qsort") (ptr c-ulong c-ulong (fn (ptr ptr) i32)) ())

(defun desc ((a ptr) (b ptr)) i32 ...)

(unsafe
  (c-qsort buf n 8 desc)                                 ; 頂層函式
  (c-qsort buf n 8 (lambda ((a ptr) (b ptr)) i32 ...))   ; lambda
  (labels ((cmp ((a ptr) (b ptr)) i32 ...))
    (c-qsort buf n 8 cmp)))                              ; 區域函式
```

C 的函式指標只是程式碼位址，C 只傳遞宣告的引數來呼叫。沒有傳遞捕獲變數的地方，所以**只能傳遞沒有自由變數的函式**，這在型別檢查時檢查。

- 實際引數中**直接**寫函式名稱或 `lambda` 運算式。不能傳遞存放函式的變數——其中是哪個函式，因而是否有自由變數，要到執行時才知道。
- `lambda` 參照其外部的區域變數時是錯誤。可以參照全域變數與頂層函式。
- 區域函式（`labels`）要求連同它呼叫的兄弟函式在內都沒有自由變數。兄弟函式共享存放捕獲變數的地方，所以被呼叫的兄弟函式的捕獲也是該函式的捕獲。
- 泛型函式的型別由宣告的函式型別決定。
- 函式型別中可以寫的型別與上面的清單相同。但回呼的回傳型別不能寫 `string`（會把沒有人釋放的記憶體交給 C）。`string` 引數會把 C 傳來的字串
  複製成 typelisp 的字串。

C 函式的呼叫只能寫在 `unsafe` 中，所以只有在 `unsafe` 中才能傳遞回呼。

**只有在 typelisp 呼叫的 C 函式執行期間才能回呼。** 從其他地方——沒有執行 typelisp 的執行緒、訊號處理常式、以 `atexit` 登記的函式——呼叫時，
會顯示原因並停止行程。

**失敗不會越過 C 傳播。** 回呼中的 panic 或 `throw` 無法越過 C 的堆疊框展開（會成為未定義行為），所以對 C 回傳 0，在 C 函式返回時再對呼叫端
重新丟出。從失敗到 C 函式返回之間再次被呼叫時，不執行而回傳 0。

回呼中需要等待的操作（從空通道 `recv` 等）是錯誤（[12.6](#126-編譯後的程式碼與任務)）。

重新定義函式後，從下次傳給 C 開始呼叫新的定義。

在 AOT（`compile-file`）中也同樣運作。C 呼叫的進入點會嵌入執行檔中。

**不能作為值傳遞。** 不能把 FFI 宣告直接寫在 `(map f xs)` 的 `f` 中——函式值是包裝定義本體的閉包，而這個宣告沒有可包裝的本體。請以 `lambda`
包住：

```lisp
(run-it (unsafe (lambda ((n i32)) i32 (c-abs n))))
```

`(disassemble c-abs)` 也會被拒絕。能顯示的是 C 端的機器碼，那不是這個編譯器產生的。`(compile c-abs)` 會成功（什麼也不做——因為已經編譯過了）。

**在 AOT（`compile-file`）中也能運作。** C 函式本身由連結器解析。如果有寫了 `:library` 的宣告，該函式庫會作為 `-l` 加到連結命令列（重複的合為
一個）——不需要為 `compile-file` 加引數。讀取原始碼的是 compile-file 自己，所以可以從宣告中收集。

建置時也會尋找符號。宣告了不存在的函式時，會在連結錯誤之前得到點出該名稱的錯誤。

標準函式庫（prelude）不使用 `defffi`。標準函式庫會整個進入每個執行檔，如果其中有帶 `:library` 的宣告，連不使用 FFI 的程式也會連結那個函式庫。

#### def-c-struct 與型別化指標 —— 配置 C 結構

```lisp
(unsafe
  (def-c-struct 名稱 (欄位 型別)...)
  ...)
(unsafe (pub def-c-struct ...))
```

宣告與 C 配置相同的結構。只能寫在頂層的 `unsafe` 中（那個 `unsafe` 中只能寫 `def-c-struct`）。可以在名稱之後放置文件字串。

欄位可以寫的型別是 `i8` `i16` `i32` `u8` `u16` `u32` `c-long` `c-ulong` `f32` `f64` `bool` `ptr`、型別化指標 `(ptr T)`，以及其他
`def-c-struct`（以值嵌入）。配置（各欄位的位移、結構的大小與對齊）依 C 的規則計算（以 LP64 為前提）。可以寫指向自己的欄位，但不能嵌入自己。

```lisp
(unsafe
  (def-c-struct point (x i32) (y f64))              ; x 在 0，y 在 8，大小 16
  (def-c-struct seg (a point) (b point) (next (ptr seg))))
```

`def-c-struct` 的名稱進入型別的命名空間（同一模組中不能有同名的 `defstruct` 等），但**不是值的型別**。不能寫 `(defun f ((p point)) ...)`，它只
作為型別化指標所指的對象出現。

**型別化指標 `(ptr T)`** 是指向 `T` 的位址。`T` 是上面欄位可寫的型別之一。與 `ptr` 一樣是原始機器字，可以出現的位置規則也相同（只有引數、
回傳型別與區域變數，只在 `unsafe` 中能成為值）。

配置與讀寫以下列形式書寫。都只能在 `unsafe` 中使用。

| 形式 | 意義 |
|---|---|
| `(c-alloc T)` / `(c-alloc T n)` | 配置 `n` 個（省略時 1 個）`T`。內容填入 0。回傳 `(ptr T)` |
| `(c-ref p i)` | 從 `p` 起第 `i` 個元素的指標。超出配置範圍時是錯誤 |
| `(c-deref p)` / `(setf (c-deref p) v)` | 讀取／寫入 `p` 所指的純量 |
| `p::field` / `(setf p::field v)` | 讀取／寫入結構的欄位。讀取嵌入的結構欄位會得到其位址（`(ptr 內部型別)`） |
| `(as ptr p)` | 忘掉型別成為 `ptr`（為了傳給 `qsort` 的 `void *` 之類）。沒有反方向的轉換 |

```lisp
(defun sum-x ((n int)) int
  (unsafe
    (let ((ps (c-alloc point n)))
      (dotimes (i n)
        (let ((p (c-ref ps i)))
          (setf p::x (as i32 i))))
      (let ((total 0))
        (dotimes (i n)
          (let ((p (c-ref ps i)))
            (setf total (+ total (as int p::x)))))
        total))))
```

**配置的記憶體會在離開配置它的 `unsafe` 時釋放。** 擁有者是同一函式中詞法上最外層的 `unsafe`。無論正常結束，還是因 panic、`throw`、
`return-from` 離開，都會釋放。`lambda` 與 `labels` 的函式是另外的函式，所以 `c-alloc` 需要在其中有自己的 `unsafe`。

因此，型別化指標不能帶出配置它的 `unsafe`。以下都是型別檢查時的錯誤。

- 作為 `unsafe` 運算式的值（因而也不能從函式回傳）
- 在閉包（`lambda`、`labels`）中捕獲
- 傳給 `task` / `thread`
- 以 `throw` 丟出

想在 `unsafe` 之外使用值時，在 `unsafe` 中複製到 `defstruct` 或數值後回傳。

**不處理 C 端配置的記憶體。** 從 C 作為型別化指標進來的值——`defffi` 的回傳值、回呼的引數、讀取指標型別欄位得到的值——會在執行期檢查它是否
指向某個存活的 `c-alloc` 配置中該型別的值的位置，不是時為錯誤。NULL 也是錯誤。想接收 C 配置的記憶體或 NULL 時，以無型別的 `ptr` 接收（讀不到
內容）。

```lisp
(unsafe (def-c-struct item (key i32) (tag u8)))
(defffi (c-qsort "qsort") (ptr c-ulong c-ulong (fn ((ptr item) (ptr item)) i32)) ())

(unsafe
  (let ((xs (c-alloc item 4)))
    ...
    (c-qsort (as ptr xs) 4 8 (lambda ((a (ptr item)) (b (ptr item))) i32 (- a::key b::key)))
    ...))
```

回呼的引數被檢查拒絕時，與回呼中的失敗一樣，在 C 函式返回時傳給呼叫端。

### 3.4 defvar / defparameter / defconstant — 全域變數

```lisp
(defvar (name Type) init-expr)        ; 只在尚未繫結時初始化
(defparameter (name Type) init-expr)  ; 每次都指派
(defconstant (name Type) init-expr)

; 帶文件字串（與 CL 的 defvar/defparameter/defconstant 相同的順序：在值之後）
(defvar (name Type) init-expr "doc")
(defconstant (name Type) init-expr "doc")
```

**`defvar` 與 `defparameter` 的差異在重新載入時呈現**（與 CL 相同）。如果該全域變數**已經繫結，`defvar` 連初始化式都不求值**，所以編輯設定檔後
重新讀取，工作階段中修改過的值保持不變。`defparameter` 每次都指派，所以重新讀取後會回到檔案中寫的值。

型別註記是必要的（不從初始化式推論）。`defvar` 可以修改，`defconstant` 不可以（`setf` 是錯誤）。

### 3.5 defmethod — 方法定義

```lisp
; 實例方法：可以以 (m obj args...) 的形式呼叫
(defmethod name ((self Type) (arg Type2) ...) RetType body...)

; static / 關聯函式：可以以 (Type::name args...) 的形式呼叫
(defmethod name (Type (arg Type2) ...) RetType body...)
```

呼叫端依 `obj` 的靜態型別解析方法（單一、靜態分派）。可以在與 `defun` 相同的位置、依相同的規則放置文件字串（`where` 子句之後、本體開頭，僅當
其後還有本體形式時）。`impl` 中的方法也一樣——以 `(documentation Type::method)` 取出。

### 3.6 defstruct — 結構（使用者定義型別）

```lisp
(defstruct Name
  (field1 Type1)
  (pub field2 Type2)
  ...)

; 泛型（型別參數寫在角括號中）
(defstruct Name<T1,T2...>
  (field TypeUsingT1)
  ...)
```

- 每個欄位是 `(name type)` 或 `(pub name type)`（依欄位設定公開，與結構本身的 `pub` 無關）。尾端再寫一個式子，就成為該槽的**預設值**
  （`(x i32 0)`）——見後面的選項清單。
- 自動產生以下內容：
  - 建構函式 `Name::new`（依欄位順序傳入引數）
  - 讀取器 `(field-name instance)`，語法糖 `instance::field-name`
  - 設定器 `(set-field-name instance value)`，語法糖 `(setf instance::field-name value)`
- 要讓結構本身 `pub`，像 `(pub defstruct ...)` 這樣在前面加 `pub`。
- **型別要在被指名之前定義。** 欄位的型別可以寫自己（`(next Option<node>)`），但不能寫之後定義的型別——型別沒有相當於 `defsignature` 的前置
  宣告。尚未定義的名稱，在 `defun` 的引數型別與 `the` 中同樣會得到 `unknown type` 錯誤。因此互相參照的兩個型別無法寫出。
- **型別變數只有寫在宣告部分的那些。** `defun`/`defstruct`/`defenum`/`deftype` 是名稱的 `<T>`，`defmethod` 是接收者的型別（`(self box<T>)`，
  靜態方法是 `box<T>`），`impl` 是對象型別與 `impl<T>`，`deftrait` 是 `Self` 以及 `(type Item)` 的關聯型別。在其他地方——引數、回傳值、本體中
  的 `the`/`lambda`——第一次出現的名稱不會成為型別變數，而是 `unknown type`。
- **文件字串**：在名稱之後、欄位清單之前放置字串字面值，它就成為文件字串（`(defstruct Name "doc" (field Type)...)`——與 CL 的 `defstruct`
  位置相同）。欄位一律是 `(name Type ...)` 的形式，不可能是裸字串，所以沒有歧義。以 `(documentation Name)` 取出。

#### 選項清單

在名稱的位置寫清單 `(Name option...)` 可以指定選項（與 CL 位置相同）。

```lisp
(defstruct (point (:constructor make-point)          ; 關鍵字建構函式
                  (:constructor at (x &optional y))  ; BOA 建構函式
                  (:copier copy-point))
  (x i32 0)          ; 第 3 個元素是該槽的預設值
  (y i32 0))

(point::make-point :y 7)   ; x 為 0
(point::at 1)              ; y 為 0
(point::at 1 2)
(copy-point p)             ; 淺複製（與 CL 的 copier 相同）
```

- **`:constructor`** —— 產生的是型別的**靜態函式**（`point::make-point`），本體一定是 `(point::new ...)`。`new` 仍是結構上唯一的建構函式，
  這裡建立的是它的*呼叫方式*。可以宣告多個。
  - `(:constructor name)` —— 以 `&key` 接受所有槽。**所有槽都需要預設值**（這個語言沒有相當於 CL「未繫結槽」的東西）。
  - `(:constructor name (slot...))` —— 以位置引數接受所列的槽（順序任意）。沒有列出的槽以預設值填入，所以**需要預設值**。插入 `&optional`
    後，其後的可以省略（同樣需要預設值）。
- **`:copier`** —— 產生回傳槽值相同的新值的**實例方法**。與 CL 的 copier 一樣是淺複製。
- **`:include Parent`** —— 把父的槽清單接到開頭（預設值也繼承。父可以在別的檔案中）。**不建立型別關係**——子不是父的子型別，父的方法不適用於
  子，也沒有連接兩者的執行期檢查。這個語言沒有子型別，共同的介面由 `deftrait` 負責。連接的只是槽的*清單*。
- **槽的預設值只由產生的建構函式讀取。** 一個 `:constructor` 都沒有宣告卻寫了預設值，因為不可能被使用，所以是錯誤。
- 不加入的選項及其原因：
  - **`:conc-name`** —— 在 CL 中是為存取器加前綴，以避免在一個平坦的函式命名空間中衝突。在這裡存取器是依接收者型別分派的方法，不會衝突，而且
    加前綴會破壞 `instance::field`（只知道槽名稱）。
  - **`:predicate`** —— 在執行期回答「這個值是 `point` 嗎」。在這裡型別是沒有執行期見證的編譯時分類，也不存在「可能是 point 的未知型別的值」
    所在的位置（對 `Sexpr` 的 `match` 是封閉的，`:dyn` 不能向下轉型），所以產生的述詞只能一律回傳 `true`。
  - **`:type` / `:initial-offset` / `:named`** —— 把值的表示換成串列或向量的指定。表示屬於編譯器，從語言中無法觀察。

### 3.7 defenum — 列舉（和型別）

```lisp
(defenum Name
  (Variant1 Type1 Type2...)   ; 帶載荷的變體（位置欄位）
  (Variant2)                  ; 不帶載荷的變體
  ...)

; 泛型
(defenum Option<T>
  (Some T)
  (None))
```

- 每個變體的形式是 `(VariantName FieldType...)`。欄位只能依位置指定（沒有名稱）。至少需要一個變體，名稱不能重複。
- 值的建立與內建的 `Option`/`Result` 一樣，以限定名稱或經由 `use`：`(Name::Variant1 a b)`，或在 `(use Name)` 之後寫 `(Variant1 a b)`。
- 可以用 `match` / `if-let` 拆解。`match` 檢查窮盡性（需要涵蓋所有變體或有 `_`）：
  ```lisp
  (match opt
    ((Some v) v)
    ((None) 0))
  ```
- 方法／關聯函式與 `defstruct` 一樣以 `defmethod`/`impl` 後加。
- 要讓列舉本身 `pub`，寫 `(pub defenum ...)`。
- **文件字串**：與 `defstruct` 相同的位置與規則——名稱之後、變體清單之前（`(defenum Name "doc" (Variant ...)...)`）。以 `(documentation Name)` 取出。

### 3.8 deftype — 型別別名

```lisp
(deftype meters i32)
(deftype fallible<T> Result<T,string>)
(deftype pred (fn (i32) bool))

(defun double ((m meters)) meters (* m 2))
(defun parse ((s string)) fallible<i32> ...)
```

把 CL 的 `deftype` 縮小到在靜態型別語言中有意義的範圍——**是型別的寫法，而不是型別**。

- 名稱的位置與 `defun` 相同，泛型引數寫成 `Name<T,U>`。在使用處需要與宣告個數相同的型別引數（多或少都當場出錯）。
- 展開發生在**型別剖析器內部**。因此下游誰也不知道別名的存在——單型化的鍵、傾印、編譯路徑，以及**錯誤訊息**，全都顯示展開後的形式。
  `(f "x")` 對要求 `meters` 的函式失敗時，訊息中顯示的是 `i32`。
- **不是新的型別。** `(deftype meters i32)` 使 `meters` 與 `i32` 成為同一型別，所以混用什麼也不會被捕捉。要區分請用 `defstruct`。
- **不會成為述詞。** CL 的 `(deftype small () '(integer 0 9))` 表示*值的集合*，由 `typep` 在執行期判斷，但在這裡型別是沒有執行期見證的編譯時
  分類，所以限制值的別名沒有可限制的對象。
- **不能包含自己。** 別名在書寫處展開，所以沒有遞迴的去處。遞迴的資料型別以 `defstruct`/`defenum` 書寫。
- 與型別、trait 共享命名空間（同一模組中不能與 `defstruct`/`defenum`/`deftrait` 同名）。以 `(pub deftype ...)` 公開，以 `(use m::meters)` 引入。
- **文件字串**：名稱之後、型別之前（`(deftype Name "doc" Type)`）。

### 3.9 deftrait / impl — trait 機制

```lisp
(deftrait TraitName (SuperTrait...)      ; 繼承清單是必要的。沒有時為 ()
  (type AssocName)                       ; 關聯型別（可以多個，可以省略）
  (method-name ((self Self) params...) RetType)          ; 沒有本體＝必須實作
  (method-name ((self Self) params...) RetType body...)) ; 有本體＝預設實作

(impl TraitName TargetType
  (where (Trait A)...)                   ; 作用於整個 impl 的約束（可以省略）
  (type AssocName ConcreteType)          ; 具體化關聯型別
  (method-name (recv params...) RetType body...))
```

透過 `impl`，每個方法都作為 `TargetType` 的一般 `defmethod` 登記。在泛型函式的 `where` 子句中作為 trait 約束參照（見
[3.1 defun](#31-defun--函式定義)）。trait 名稱也可以寫成 `m::Trait` 這樣的 `::` 路徑。

**繼承清單（必要）**：一定寫在 trait 名稱之後。元素是裸 trait 名稱，或者在該 trait 有關聯型別時，寫成**固定了所有關聯型別**的
`(Trait (Assoc Type))`。

```lisp
(deftrait Eq () ...)                       ; 沒有繼承
(deftrait Ord (Eq) ...)                    ; Rust 的 trait Ord: Eq
(deftrait CharSource ((Iter (Item char)))  ; 固定關聯型別
  (rewind ((self Self)) ()))
```

繼承有 3 個效果。(1) `impl Ord X` 要求**先**寫 `impl Eq X`（關於書寫順序的規則。是在 REPL 與逐一 `load` 中都能確定判斷的唯一形式，比 Rust
更嚴格）。(2) 只要 `(where (Ord T))` 就能呼叫 `Eq` 的方法。(3) 可以從 `:dyn Ord` 呼叫 `Eq` 的方法，`:dyn Ord` 的值可以直接傳給要求 `:dyn Eq`
的位置（向上轉型）。子 trait 重新宣告與父同名的方法，以及從兩個父繼承同名的方法，都是錯誤（vtable 的槽每個名稱一個）。菱形繼承會合併為一個槽。

**預設實作**：在簽章後寫本體，`impl` 省略該方法時就使用它。本體在寫下 trait 的**模組的命名空間**中解析，所以也可以呼叫該模組中非公開的函式。
有本體的方法也可以寫 `where` 子句與文件字串。本體的型別檢查**在宣告時進行一次**，`Self` 保持為型別變數（以 `Self: 該 trait` 為約束）（與 Rust
相同）——即使是沒有任何 `impl` 省略的預設實作，對任何實作型別都通不過的錯誤也會在那裡被排除。對 `self` 呼叫該 trait 本身及其繼承來源的方法會
藉這個約束通過，關聯型別固定為自己，所以回傳 `Item` 的簽章與本體在不知道具體型別的情況下進行比對。

**全面實作（blanket implementation）**：把對象設為型別變數，就能一次為滿足約束的所有型別實作。

```lisp
(deftrait Clamp (Ord)
  (clamp ((self Self) (lo Self) (hi Self)) Self
    (if (less self lo) lo (if (less hi self) hi self))))
(impl<T> Clamp T (where (Ord T)))          ; 沒有本體——全部使用預設實作
```

**在具體型別實際使用之前不會產生程式碼**（每種型別一次，與一般單型化相同的機制）。一個 trait 最多一個全面實作。同一型別有明確的 `impl` 時，那個
優先。本體的型別檢查與產生是分開的，在宣告時**保持對象為型別變數**進行一次（與 Rust 相同）——即使是一次也沒被使用的實作，在宣告的約束下對任何
對象都通不過的錯誤也會在那裡被排除。約束所允許的呼叫（`(where (Ord T))` 下的 `(less self other)` 等）與泛型 `defun` 的本體同樣對待，可以通過。

**文件字串**：`deftrait` 在繼承清單之後、項目清單之前放置字串字面值，整個 trait 就能有一個文件字串（`(deftrait Name () "doc" (type ...)
(method ...)...)`）。沒有本體的簽章不能寫文件字串——結尾的字串本身會成為預設實作的回傳值，兩者無法區分。

標準函式庫提供的 trait：**`Iter`**（`next`／關聯型別 `Item`。`doiter`／序列函式的基礎）、**`Eq`**（`equals`。`not-equals` 是預設實作）、
**`Ord`**（繼承 `Eq`。只有 `less` 必須實作，`less-equal`／`greater`／`greater-equal` 是預設實作）、**`Error`**（`message`／`source`。統一
處理錯誤型別的 `:dyn Error`）、**`print-object`**（依型別的列印表示）、**`Pathish`**（路徑名稱指定子＝字串或 `pathname`）、串流的階層
**`Stream`** → **`InputStream`**／**`OutputStream`** → **`CharInput`**／**`CharOutput`** → **`PeekInput`**。哪些型別實作了哪些 trait 見
[types.md](types.md)，各 trait 的方法見[標準 trait](functions/traits.md)、[錯誤型別](functions/option-result.md#3-錯誤型別與-error-trait)、
[print-object](functions/printing.md#5-print-object依型別的列印表示)、[串流](functions/streams-files.md)。為自己的集合型別 `impl` `Iter`，
`doiter`（第 5 章）以及 `map`／`filter`／`sort` 等就能直接使用。

trait 的呼叫預設是**靜態的**（依接收者的靜態型別解析）。想處理具體型別在執行期決定的值時，使用 trait 物件型別 `:dyn Trait`（第 2 章），就會經由
vtable 進行動態分派：

```lisp
(deftrait Drawable () (draw ((self Self)) string))
(defstruct circle (r i32))
(defstruct square (side i32))
(impl Drawable circle (draw ((self Self)) string "circle"))
(impl Drawable square (draw ((self Self)) string "square"))

(defun render-all ((xs Vector<:dyn Drawable>)) ()
  (doiter (d (iter xs)) (println "~a" (draw d))))   ; 一個呼叫處，每個實作各自的回答
```

能成為 `:dyn Trait` 的只有「所有方法都有 `self` 接收者，除接收者外不使用 `Self`，方法本身既不是泛型也不是可變引數」的 trait（繼承的方法也必須
滿足同樣的條件）。

能放進 `:dyn` 盒子的只有值在堆積上有表示的型別：

| 可以放入 | 不能放入 |
|---|---|
| `defstruct` / `defenum` 的型別（包括 `Vector<T>`、`cons-cell<A,B>`、`Result<T,E>` 以及標準函式庫的結構）、`HashTable<K,V>`、`Sexpr`、`int`、`ratio`、`f64`、`string`、`random-state` | 固定寬度整數（`i8`〜`u32`）、`f32`、`bool`、`char`、`symbol`、`()`、函式型別、沒有盒子的 `Option<T>`（[Option 的執行期表示](functions/option-result.md#2-optiont-的執行期表示)） |

把不能放入的型別的值放在 `:dyn` 的位置是型別錯誤。想以 `:dyn` 處理這樣的值時，像 `(defstruct flag (v bool))` 這樣以結構包住。

### 3.10 module / use — 命名空間

```lisp
(module path body...)      ; path 是 foo 或 foo::bar 這樣的區段序列
(in-module path)           ; 從這裡到這個單元的結尾都在 path 中（module 的平鋪形式）
(use path...)              ; 把函式、型別、模組以別名引入目前的命名空間
(import path...)           ; 與 use 相同（CL 相容的寫法）
(shadowing-import path...) ; 明知裸名稱已被占用仍要取用的 use
```

- `module` 建立命名空間。**型別不是命名空間**（與 Rust 相同，型別只擁有關聯函式／方法）。
- 以 `use` 引入型別後，該型別的建構函式與公開的 static 方法也可以用裸名稱使用（例如：`(use option)` 之後可以不寫 `option::some`/
  `option::none` 而呼叫 `some`/`none`）。
- 裸名稱（沒有限定的識別字）的解析順序：特殊形式 → 建構函式 → 自由函式（目前的命名空間 → 根）→ 實例方法（依第一個引數的靜態型別解析）。不會
  回溯到中間的父模組。
- 限定路徑 `a::b` 依上述順序解析 `a`，是模組就進入內部，是型別就把最後一個區段作為關聯項目解析。
- **`use` 對其後的形式生效。** 檔案逐個形式讀取，相依關係也在檢查該形式之前解析，所以在 `(use m)` **上面**寫 `m::f` 會得到 `unresolved path`。
  請把 `use` 放在檔案開頭。
- **`use` 可以接受多個路徑**（`(use a::f b::g)`）。`import` 是行為相同的 CL 相容寫法。
- **裸名稱已被占用的 `use` 會被回報。** 裸名稱的解析先看該模組本身的定義，再看別名，所以 `(defun twice ...)` 之後的 `(use m::twice)`
  **什麼也不做**。明知如此仍要這樣做時寫 `shadowing-import`（但它贏不了定義——沒有撤銷定義的手段。它能贏的只有先前的別名）。
- **`in-module` 是 `(module path body...)` 的平鋪形式。** 寫 `(in-module geometry)`，從那裡到該單元（檔案，或外層 `module` 的本體）結尾都在
  `geometry` 中。它進入檔案本身模組的**內部**（對 `main.typl` 是 `main::geometry`）。連續寫兩個會依序巢狀。它與 CL 的 `in-package` 不同，名稱
  也刻意區分——在這個系統中檔案已經是模組，沒有可「選擇」的對象，形式能做的只有巢狀。

### 3.11 檔案與模組的對應（多檔案專案）

從原始碼根目錄起的相對檔案路徑就是模組路徑：`<root>/geo/point.typl` 的內容隱含地包在模組 `geo::point` 中（目錄也是一個區段，Rust/Python 的
方式）。檔案中明確的 `(module bar ...)` 巢狀在其**內部**（`geo::point::bar`）——推導出的路徑與明確的宣告不會衝突。

- **原始碼根目錄**：在專案根目錄放置資訊清單檔案 `typelisp.toml`（可以是空的。可以選擇以 `src = "src"` 一行指定原始碼目錄）。從對象檔案所在目錄
  往上尋找。沒有資訊清單時，進入點檔案所在目錄（REPL 是目前目錄）為根目錄。
- **依需求載入**：`(use geo::point)` 參照尚未讀入的模組時，會自動讀入對應的檔案（`geo/point.typl`），進行型別檢查並登記。`use a::b::c` 依最長
  前綴的順序尋找 `a/b/c.typl` → `a/b.typl` → `a.typl`（因為 `c` 可能是模組內的項目）。從其他模組可見的定義需要 `pub`
  （[3.13 pub](#313-pub--公開)）。
- **循環參照是錯誤**：以 `circular module dependency: a -> b -> a` 的形式回報鏈結。
- **執行**：以 `typl <file.typl>` 執行檔案（不帶引數時為 REPL）。REPL 中的 `use` 也依同樣的約定解析檔案。
- **cons 區域容量**：以 `typl --heap-cells N` 指定 cons 單元區域的**初始容量**（預設 65536。也可以寫 `--heap-cells=N`，執行檔案與 REPL 都適用）。
  區域不足時會**追加增長**。增長的上限是初始容量的 256 倍，超過它的配置會成為 `heap exhausted`——也就是說，初始容量的意思是「一開始先配置這麼多」，
  上限的意思是「超過這裡就視為洩漏」。

### 3.12 load — 平鋪載入

```lisp
(load "path")   ; 只能在頂層。path 是字串字面值
```

- CL 式的**平鋪載入**：把對象檔案的形式**原樣讀入目前的命名空間**（不像 `use` 那樣以模組包住）。只能在頂層（在函式本體中是型別錯誤）。
- `path` 相對於讀入端檔案所在的目錄（從 REPL 讀入時相對於行程的 cwd）。沒有副檔名時補上 `.typl`。
- 讀入的檔案本身的 `(load ...)`/`(use ...)` 也會遞迴處理。
- **逐個形式讀取，並當場執行**（與 CL 的 `load` 相同）。形式 *k* 在 *k+1* 被讀取之前就已經執行完畢——即使途中有語法錯誤或型別錯誤，之前的形式也
  已執行。透過 `use` 讀入的模組檔案與此不同，作為一個單元檢查，執行交給 `use` 它的一方（相當於 CL 的 `compile-file`）。

### 3.13 pub — 公開

```lisp
(pub defun ...)
(pub defsignature ...)
(pub defffi ...)
(pub defvar ...)
(pub defparameter ...)
(pub defconstant ...)
(pub defmacro ...)
(pub defmethod ...)
(pub defstruct ...)
(pub defenum ...)
(pub deftype ...)
```

能加 `pub` 的只有上面 11 種（`module`/`use`/`deftrait`/`impl` 不能加）。不是以括號包住定義形式的 `(pub (defun ...))` 形式，而是在 `pub` 之後
緊接定義關鍵字。一個 `pub` 只能公開一個定義（不能一次指定多個定義）。

### 3.14 defmacro — 巨集定義

```lisp
(defmacro name (必要... &optional opt... &rest rest-name &key key...) body...)
```

- 所有參數與回傳值一律是 `Sexpr`，所以不寫型別註記。
- CL 式的非衛生巨集（以 `gensym` 避免衝突是巨集作者的責任）。
- lambda 清單依 CL 的 `必要 &optional &rest &key` 順序（每個標記至多一次，只能依這個順序）。
  - `&optional` … 可省略的引數。`name` 或 `(name 預設式)`。預設式在展開時求值（可以參照先前已繫結的參數），省略時繫結（不寫預設值時為空串列 `()`）。
  - `&rest name` … 把剩餘的位置引數合為一個 `Sexpr` 串列接收。
  - `&key` … 關鍵字引數。`name` 或 `(name 預設式)`。呼叫端以 `:name 值` 傳入（順序任意）。省略時為預設式（沒有時為空串列 `()`）。未知的關鍵字
    或奇數個的 `:key` 序列是錯誤。
- 例：`(defmacro pair (x &optional (y 1)) ...)` / `(defmacro make (&key (a 0) (b 9)) ...)`。

### 3.15 macrolet / symbol-macrolet — 區域巨集繫結

```lisp
(macrolet ((name (lambda 清單) body...) ...) body...)   ; 詞法範圍的巨集
(symbol-macrolet ((name 展開形式) ...) body...)          ; 名稱代表一個形式
```

兩者都是**運算式**的特殊形式，執行期什麼也不留下（被編譯的是本體展開後的形式）。lambda 清單與 `defmacro` 相同。詳細規則與例子見
[區域巨集繫結](functions/system.md#9-區域巨集繫結macrolet--symbol-macrolet)。

## 4. 繫結與條件分支

```lisp
(let ((name val) ...) body...)      ; 平行繫結
(let* ((name val) ...) body...)     ; 依序繫結（先前的繫結可以用在後面的初始化式中）

(if cond then else)                 ; else 是必要的（固定 3 個元素）
(when cond body...)                 ; 沒有 else 的 if（Unit 型別）。defmacro
(unless cond body...)               ; when 的否定版。defmacro
(cond (test1 body...) (test2 body...) ... (else body...))   ; defmacro
(case expr
  (key1 body...)
  ((key2 key3) body...)             ; 鍵清單：符合其中任一個
  (else body...))                   ; expr 只求值一次。key 以 equal 比較。
                                     ; key 是「字面值」，不求值（與 CL 相同）。
                                     ; 裸符號 a 表示符號 'a。
                                     ; 寫 'a 是錯誤（使用裸的 a）。defmacro
(ecase expr (key body...) ...)      ; 要求窮盡的 case。都不符合時 panic。defmacro
(ccase expr (key body...) ...)      ; CL 的 ccase。沒有可提供的 restart，所以與 ecase 相同。defmacro
(and expr...)                       ; 短路求值。0 個引數時為 true。defmacro
(or expr...)                        ; 短路求值。0 個引數時為 false。defmacro
(progn body...)                     ; 依序執行，回傳最後的值
(unsafe body...)                    ; 與 progn 相同。另外給予書寫 FFI 呼叫
                                     ; 與原始字的許可。見 3.3 defffi
(prog1 form more...)                ; 全部求值，值為 form 的。defmacro
(prog2 a b more...)                 ; 全部求值，值為 b 的。defmacro
(the Type expr)                     ; 型別註記（沒有執行期效果）
```

### 4.1 unsafe — 承擔無法檢查的前提

```lisp
(unsafe body...)
```

與 `progn` 相同——依序對本體求值，回傳最後的值。不建立範圍，也不是函式邊界（`break` / `return-from` 直接穿過到外部）。不同之處在於有些東西只能
寫在它裡面。

目前要求 `unsafe` 的有 3 種：呼叫以 [defffi](#33-defffi--宣告-c-函式ffi) 宣告的 C 函式，把原始機器字（`ptr` / `c-long` / `c-ulong` /
`(ptr T)`）作為值，以及 [`def-c-struct` 與 `c-alloc`](#def-c-struct-與型別化指標--配置-c-結構)。

以 `c-alloc` 配置的記憶體在離開同一函式中最外層的 `unsafe` 時釋放。只有那個 `unsafe` 與 `progn` 不同，離開時有釋放的處理。

`unsafe` 承擔的是編譯器無法確認的以下前提：

- **型別一致。** 宣告的 C 簽章與真正的一致。不一致時，引數會放進錯誤的暫存器，回傳值會以錯誤的寬度讀取。
- **記憶體安全。** C 端如何處理傳給它的東西。
- **行程全域的狀態。** 環境變數、訊號處理常式、`errno`。例如透過 FFI 呼叫 `setenv`，會破壞這個實作的 `decode-universal-time` 求當地時間時的前提。
- **執行緒安全。**

它不是逃避型別檢查的出口。`(unsafe (+ 1 "two"))` 不會通過。允許的是書寫特定的**操作**，而不是書寫亂七八糟的東西。

它在詞法上作用。寫在 `unsafe` 中的 `lambda` 的本體繼承這個許可（與 Rust 的 `unsafe` 區塊中的閉包相同）——這個值之後可能會從 `unsafe` 之外呼叫，
但寫在那裡本身就被視為承擔了責任。

### 4.2 destructuring-bind — 依形狀拆解串列

```lisp
(destructuring-bind lambda 清單 form body...)
```

把 `form` 產生的串列**依形狀**拆解並繫結。lambda 清單是 `defmacro` 的（必要 → `&optional` → `&rest`/`&body` → `&key`，各自帶預設式），理由與
CL 讓兩者共用一個相同——它們是拆解同一種東西的兩種形式。

```lisp
(destructuring-bind (op a b) (quote (+ 1 2)) (format false "~a ~a ~a" op a b))  ; "+ 1 2"
(destructuring-bind (head &rest tail) xs (format false "~a | ~a" head tail))
(destructuring-bind (a &optional (b 9)) (quote (1)) b)                          ; 9
(destructuring-bind (&key (x 0) y) (quote (:y 7)) (format false "~a ~a" x y))   ; "0 7"
```

- **繫結的變數全部是 `Option<Sexpr>`。** 這不是實作的限制，而是被繫結對象的性質：S 運算式串列是這個語言中唯一的串列，沒有其他可以給元素的型別。
  在需要純量的地方轉到 `match`，與 `defmacro` 的本體相同。
- **形狀不符時 panic**（相當於 CL 的錯誤）。元素不足、過多，`&key` 的序列為奇數個，未知的關鍵字，都是如此。`sexpr-car` 對 `()` 回傳 `()`，是
  寬鬆的函式，所以不寫檢查的話，短的串列會默默繫結成空序列。
- **不支援巢狀的 lambda 清單。** `defmacro` 也不接受，以保持規則唯一。`(a (b c))` 不會默默把子串列繫結到 `b`，而是得到說明這一點的錯誤。
- `&optional` / `&key` 的預設式**只在使用時求值**（與 CL 相同）。
- 沒有相當於 CL 的 `&allow-other-keys` 的東西（`defmacro` 也沒有）。

### 4.3 match — 模式比對

```lisp
(match expr
  (pattern body...)
  ...)
```

模式的種類：
- `_` —— 萬用字元
- 變數名稱 —— 繫結模式（一律符合）。但如果被比對值的型別有該名稱的變體，就解析為**下面的裸變體名稱模式**
- 裸變體名稱 —— 符合不帶引數的變體（`(match c (red 1) (blue 2))`）。以裸名稱寫帶欄位的變體會得到引數個數錯誤，所以像 `(circle r)` 這樣以括號寫
- **立即值字面值**：整數 / `true`/`false` / 字元 —— 以字比較
- **值字面值**：字串 / 浮點數 / 符號（`'foo`）/ 多倍精度整數 / ratio —— 以該型別的 `Eq::equals`（[標準 trait](functions/traits.md#2-eq--ord比較)）
  依值比較。字串比較的是內容，而不是同一性
- `(= expr)` —— 對任意式子求值，以 `Eq::equals` 比較。是比較沒有字面值語法的型別（`defstruct` 實例、全域變數、計算結果）的唯一寫法，使用者定義的
  `Eq` 實作直接成為比較規則。`expr` 可以參照從該分支位置可見的任何東西（引數、外層繫結、全域變數）
- `(Ctor sub-pattern...)` —— 建構函式模式（`Some x` `None` `Cons a d` `Ok v` 等）

以值字面值／`(= expr)` 比較沒有實作 `Eq` 的型別是型別錯誤（比起留下默默不符合的分支，選擇說明無法比較）。

**對 `Sexpr` 被比對值的值字面值**：`sexpr` 的 `Eq` 是 `eq`（CL 的同一性），所以立即值——`'foo`（已 intern）/ 整數 / 字元 / `true`/`false`——可以
直接寫，依內容符合：

```lisp
(match s ('add 1) (42 2) (#\a 3) (_ 0))
```

不是立即值的字面值（字串 / 浮點數 / 多倍精度整數 / ratio）**不能**對 `Sexpr` 書寫。它們的 `eq` 比較物件的同一性，會成為「型別能通過但永遠不
符合的分支」，所以作為點出變體模式的錯誤——寫 `(str "hi")` 就會拆解為 `string` 進行內容比較。`(= expr)` 明確要求 `equals`，所以不受這個限制。

**被比對值不必是 ADT。** `string`/`symbol`/`i32`/`f64` 等可以直接 `match`（那正是字串字面值模式派上用場的地方）。但沒有變體的型別無法以列舉
窮盡，所以 `_`（或者作為萬用字元的繫結模式）是必要的：

```lisp
(defun kind ((s string)) i32
  (match s
    ("add" 1)
    ("sub" 2)
    (_     0)))          ; 沒有變體的型別，所以需要 `_`
```

對 `Sexpr` 被比對值，除了上面的內建 18 種變體模式，還可以寫**向下轉型模式**（取出使用者定義 ADT 的實例）——以 `match` 取回像 `(list p 42)`
這樣隱式轉換為 `Sexpr` 的 `defstruct`/`defenum`（第 3 章）實例的語法：

- `(TypeName sub-pattern...)` —— 把**型別名稱**放在開頭的欄位拆解（只用於 struct，`defstruct` 一律只有一個變體，所以以型別名稱而不是變體名稱
  書寫）。例：對 `(defstruct point (x f64) (y f64))` 寫 `(point x y)`。
- 裸變體名稱 `(VariantName sub-pattern...)` —— 取出 `defenum` 的變體。以 `(use EnumType)` 之後可見的裸名稱解析（與呼叫建構函式時的可見性規則
  相同）。例：對 `(defenum color (red) (blue))`，在 `(use color)` 之後寫 `(red)` `(blue)`。多個可見的 enum 的變體名稱衝突時會得到歧義錯誤，所以
  也可以寫限定形式 `(EnumType::VariantName ...)`（不需要 `use`）。
- `(the Type pattern)` —— 以整個型別進行向下轉型（整體繫結）。不拆解欄位，把值原樣交給 `pattern`。是在保持可變 struct 同一性的同時取出它的唯一
  寫法，也是從 `Sexpr` 取出 `Vector<T>`/`HashTable<K,V>` 的唯一手段（兩者沒有欄位拆解形式）。例：在 `(the point p)` 之後 `(setf p::x 9)`，也會
  反映到串列中的原實例。

**`Option<Sexpr>` 的模式**：S 運算式資料的型別不是 `Sexpr` 而是 `Option<Sexpr>`，空串列不是 `Sexpr` 的變體，而是 `Option` 的 `none`。因此
`match` `Option<Sexpr>` 時，`Sexpr` 的 18 種變體與 `none` 可以**平鋪在同一組分支中**（不需要剝去 `Option` 的外層 `match`）：

```lisp
(defun tag ((s Option<Sexpr>)) i32
  (match s
    ((int _)    1)
    ((cons _ _) 2)
    ((str _)    3)
    ((none)     0)          ; 空串列
    (_          9)))
```

窮盡性也在同一個平鋪的全集——`Sexpr` 的 18 種變體加上 `none` 共 19 個——中檢查。忘了寫 `(none)` 時，只要沒有 `_` 就是錯誤。也可以寫 `(some x)`，
繫結「非空的某物」。

這個語法糖**恰好**只適用於 `Option<Sexpr>`。對 `Option<Option<Sexpr>>`，無法決定 `(int n)` 剝去的是哪一層，所以照常寫兩層 `match`。

**trait 物件（`:dyn Trait`，第 2 章）的被比對值**也可以直接使用同樣的向下轉型模式——`match` 先拆開盒子再交給上面的 `Sexpr` 模式機制，所以沒有
額外的語法。實作型別的集合是開放的，所以不會窮盡，`_` 是必要的：

```lisp
(defun area ((d :dyn Drawable)) i32
  (match d
    ((circle r) (* (* r r) 3))     ; 型別名稱在前的欄位拆解
    ((the square s) (* s::side s::side))
    (_ 0)))
```

**分支之間的型別推論**：所有分支必須是同一型別（`panic` 等發散的分支除外）。在沒有期望型別的位置寫的 `match` 中，分支會**互相**補足缺少的型別
引數——`(result::ok v)` 只決定 `T`，`(result::err e)` 只決定 `E`，兩者並列就決定了 `Result<T,E>`。到最後仍有任何分支都無法決定的型別引數時，會
成為該分支本身的錯誤（`cannot infer type argument ...`）。在 `match` 之外，無法決定的型別引數當場就是錯誤。

使用向下轉型模式的 `match` 的窮盡性檢查，不計入 `Sexpr` 本身變體的涵蓋（只列出向下轉型模式的 `match` 需要以 `_` 收尾）。泛型 ADT
（`defstruct point<T> ...` 等）無法推論向下轉型模式的型別引數，所以不能使用欄位拆解形式（`(point ...)`）／裸變體形式，要像 `(the point<i32> p)`
這樣以 `the` 明示。

**向下轉型會看到實體化。** 明示的型別引數用於比對——`(the point<i32> p)` 只放行 `point<i32>` 的值，`point<string>` 會略過到下一個分支。因為值
記得包括自身型別引數在內的型別（與選擇 `print-object` 的機制相同）。

```lisp
(if-let (pattern val) then els)     ; val 符合 pattern 則為 then（帶繫結），否則為 els。defmacro
(while-let (pattern val) body...)   ; 在 val（每次重新求值）符合 pattern 期間迴圈。defmacro
```

## 5. 迭代

```lisp
(loop body...)                      ; 無限迴圈。以 break/return 離開
(while test body...)                ; test 為真期間迴圈。defmacro
(until test body...)                ; test 為假期間迴圈（while 的否定版）。defmacro
(dotimes (var count-expr) body...)  ; 對 count-expr 求值一次，讓 var 走過 0..count-1。defmacro
(do ((var init step) ...)
    (test result...)
  body...)                          ; CL 式的平行步進迭代。defmacro
(do* ((var init step) ...)
     (test result...)
  body...)                          ; do 的依序版（let* 繫結、依序指派）。defmacro
(doiter (var coll-expr) body...)    ; 走訪實作了 Iter trait 的值。defmacro

(break)                             ; 只離開最內層的迴圈。值一律是 Unit
(return)                            ; 只離開最內層的迴圈
(return value)                      ; 帶值離開最內層的迴圈
```

`break`/`return` 都**只離開最內層的外圍迴圈**（不是函式的提早返回。不能越過 `lambda` 的邊界）。`loop` 的型別是內部找到的 `break`/`return` 的值
型別的合併型別（一次也不離開時為 `!`）。要離開函式，使用下面的 `return-from`。

### 5.1 `block` / `return-from` — 具名跳出

```lisp
(block name body...)                ; 具名的跳出目標。值為最後的形式，
                                    ; 或者 return-from 傳來的值
(return-from name)                  ; 以 Unit 離開該 block
(return-from name value)            ; 帶值離開
```

**`defun` / `defmethod` / `labels` 的每個函式都隱含地建立以自己名稱命名的 block**（與 CL 相同）。所以 `(return-from f v)` 就是函式的提早返回：

```lisp
(defun first-even ((a i32) (b i32)) i32
  (if (= (mod a 2) 0) (return-from first-even a) ())
  (if (= (mod b 2) 0) (return-from first-even b) ())
  -1)
```

`block` 是**詞法的**跳出，名稱**在書寫處解析**——檢查器把 `return-from` 對應到外圍的 `block`，把其值的型別合併到區塊的跳出型別。因此：

- 沒有對應 `block` 的 `return-from` 是**型別錯誤**（不是執行期錯誤）。
- 值的型別與其他跳出或本體的型別不符時是**型別錯誤**（與 `match` 分支的規則相同）。
- 同名的 `block` 巢狀時**內層勝出**（CL 的遮蔽規則）。
- **不能越過函式的邊界。** 不能從 `lambda` 內部跳到外部的 `block`（`lambda` 不建立 block——CL 的隱含 block 要求*名稱*，而匿名函式沒有）。需要
  越過的用 `catch`/`throw`（第 8 章，那是**動態的**）。

與 `break`/`return`（第 5 章）一樣是**靜態的**跳出，所以在編譯後的程式碼中是跳到編譯時就決定的基本區塊。途中有 `unwind-protect` 時，其 `cleanup`
會執行（第 8 章）。

一次也不寫 `return-from` 的話，隱含的 block 沒有任何成本。

### 5.2 擴充 `loop`（CL 的 LOOP）

`loop` 的**第 1 個元素是關鍵字時**，以子句序列讀取。否則仍是上面的簡單迴圈，已經寫好的 `loop` 的意義不變（與 CL 本身的 simple loop 規則相同）。

CL 以裸符號書寫子句詞（`(loop for i from 1 to 3 collect i)`），但這裡**全部是關鍵字**——裸的 `for` 只會成為變數參照，而是不是關鍵字也正是與
簡單迴圈的分界。例外是分隔變數與值的 `=`，它的位置是唯一的，所以裸寫與關鍵字（`:=`）都能讀取。

```lisp
(loop :for i :from 1 :to 3 :collect i)              ; #(1 2 3)
(loop :for x :in (iter v) :when (evenp x) :sum x)
(loop :repeat 4 :for x = 1 :then (* x 2) :collect x) ; #(1 2 4 8)
(loop :for i :from 1 :to 4 :sum i :into s :finally (return (* s 2))) ; 20
```

**變數子句**（寫在本體子句之前。這是 CL 的規則：寫在後面會被讀成「只從那裡開始迴圈」，所以是錯誤）：

| 子句 | 意義 |
|---|---|
| `:with v = e` | 只繫結一次。可以讀取前面子句的變數 |
| `:for v :in s` / `:for v :across s` | 依序取 `Iter` 的元素。這裡沒有 CL 的串列／向量之分，所以是同一子句的不同寫法 |
| `:for v :on s` | 依序取之後的**後綴**。CL 傳遞共享的尾端 cons，但 `Iter` 沒有可共享的尾端，所以是新的 `Vector` |
| `:for v :from a [:to b \| :below b \| :downto b \| :above b] [:by s]` | 計數。也可以用 `:downfrom`/`:upfrom` |
| `:for v = e [:then f]` | 從 `e` 開始，第 2 次以後為 `f`（沒有 `:then` 時每次都是 `e`） |
| `:repeat n` | 迴圈這麼多次 |

寫多個 `:for` 時**平行**前進，任何一個用盡時結束。

**本體子句**（每次依書寫順序執行）：

| 子句 | 意義 |
|---|---|
| `:do form...` | 用於副作用 |
| `:collect e [:into v]` | 收集到 `Vector<T>` |
| `:append e [:into v]` | 接上 `Iter` 的內容 |
| `:sum e` / `:count e` | 合計 / 為真的次數 |
| `:maximize e` / `:minimize e` | 最大 / 最小。**`Option<T>`**（與 CL 對空序列回傳 nil 相同。任意的 `Ord` 型別沒有最小元素） |
| `:always e` / `:never e` | 全部滿足則為 `true`，一旦不滿足立即為 `false` |
| `:thereis e` | `e` 是 **`Option<T>`**。回傳第一個 `some`，沒有時為 `none`（相當於 CL 的「第一個非 nil 值」的就是它。要測試 `bool` 用 `:always`/`:never`） |
| `:while e` / `:until e` | 在這裡**正常結束**（`:finally` 會執行，收集的東西就是答案） |
| `:when e clause` / `:unless e clause` / `:if e clause [:else clause]` | 讓一個子句成為有條件的 |
| `:return e` | 立即以該值離開（`:finally` 不執行。與 CL 相同） |
| `:initially form...` / `:finally form...` | 迴圈之前 / 正常結束時 |

**`:named name`**（在所有其他子句之前，只能一個）以 `(block name …)` 包住整個迴圈。`(return-from name e)` 即使從巢狀迴圈中也能一口氣離開，與
`:return` 一樣，`:finally` 不執行。不命名時不建立 block——CL 的無名 `loop` 建立 `block nil`，但這裡沒有 `nil`，而且 `break`/`return`（第 5 章）
已經提供了「離開最內層的迴圈」。

```lisp
(loop :named outer :for i :from 1 :to 3
  :do (loop :for j :from 1 :to 3 :do (if (= (* i j) 4) (return-from outer (* 100 i)) ()))
  :finally (return 0))                                  ; 200
```

省略 `:finally (return 0)` 是**型別錯誤**。這只是 `block` 的規則在作用（5.1）：跳出的型別 `int` 與迴圈用盡時留下的 `()` 不符。

**迴圈的值**：有累積子句時為其累積（有多個時為第一個），`:always`/`:never` 時為 `true`，`:thereis` 時為 `none`，都沒有時為 `()`。`:finally` 的
最後是 `(return e)` 時，它就是值——這是 CL 的 `finally (return …)` 慣用法，是不做累積的迴圈表明自己答案的唯一方法。

**與 CL 的差異 / 沒有加入的東西**：

- **子句詞是關鍵字**（如上）。
- `:maximize`/`:minimize`/`:thereis` 回傳 `Option<T>`（因為沒有 nil）。
- **只寫 `:return` 而既沒有累積也沒有 `:finally` 是錯誤。** CL 用盡時回傳 nil，但這裡沒有它，所以迴圈必須說明用盡時的值。
- 沒有加入以 `:and` 連接平行子句、`:being`／雜湊表專用的迭代、`:it`、`:nconc`。
- `:collect` 的元素型別由被累積的式子的型別決定。要收集函式型別這類**無法寫成型別名稱的型別**時，會得到說明這一點的錯誤。

## 6. 函式值與呼叫

```lisp
(lambda (params) RetType body...)   ; 建立一等函式值（閉包）
(labels ((name (params) RetType body...) ...) body...)   ; 可以相互遞迴的區域函式定義
(apply f arg1 ... argN rest-list)   ; 展開 rest-list 來呼叫 f（帶 &rest 的可變引數函式）
```

具名函式也可以直接作為值傳遞（作為高階函式的引數等）。

## 7. 其他特殊形式

```lisp
(setq var value ...)                ; CL 的變數指派。只是依序排列 (setf var value)。defmacro
(psetq var value ...)               ; 平行指派。先對所有值求值再指派。defmacro
(psetf place value ...)             ; 把 psetq 推廣到 place（同樣的展開）。defmacro
(setf place value)                  ; 對 place 指派。place 是變數名稱 / var::field /
                                     ; (accessor recv key...) 形式的呼叫。recv 的靜態
                                     ; 型別有名為 set-{accessor} 的實例方法時
                                     ; 成立（Vector<T>・HashTable<K,V> 的 get 例外地
                                     ; 對應 set，其他為 set-存取器名稱）。
                                     ; 值是指派的值（與 CL 相同）。因此
                                     ; (if c (setf x 1) ()) 中 then 與 else 的型別不符
(incf place)  (incf place delta)    ; place += delta（省略時 delta=1）。結果與 setf 相同
(decf place)  (decf place delta)    ; place -= delta（省略時 delta=1）
(rotatef place1 place2 ... placeN)  ; 循環位移 N 個 place（新 place1=舊 place2, ...,
                                     ; 新 placeN=舊 place1）。每個 place 的子式只求值一次
(shiftf place1 ... placeN newvalue) ; 把 place2..N 的值左移，把 newvalue 放進 placeN。
                                     ; 回傳值是舊 place1 的值
(list e1 e2 ... en)                 ; 展開為 (cons e1 (cons e2 (... ())))。0 個引數時為 ()
                                     ; 各元素隱式轉換為 Sexpr（與 CL 的 cons 一樣，可以持有任意
                                     ; 值）。純量（int/i32/f64/ratio/char/bool/string/
                                     ; symbol）包裝為對應的 Sexpr 變體，defstruct/defenum/
                                     ; Vector<T>/HashTable<K,V> 等原樣放入（沒有轉換的
                                     ; 成本）。&rest/format 的引數也一樣。
(source-file)                       ; 讀取這個形式的檔案名稱（string）。在檢查時
                                     ; 作為常數決定。相當於 CL 的 *load-pathname*，但不是變數
                                     ; ——模組的本體在檢查之後執行，所以「現在
                                     ; 正在載入」靠不住，而在檢查時一律是已知的。
                                     ; 不是檔案的原始碼為讀取器的稱呼（<stdin>/<input>）
(quote datum)                       ; 與 'datum 相同。不求值，以 Sexpr 資料回傳
(quasiquote template)               ; 與 `template 相同。以 ,/,@ 把式子嵌入範本
(documentation name)                ; 以 Option<string> 回傳 name（裸名稱或 Type::method）的文件字串
(panic message)                     ; message: string。以不可恢復的錯誤異常結束。型別為 !
(unreachable)                       ; 展開為 (panic "unreachable")。defmacro
(todo)                              ; 展開為 (panic "todo")。defmacro
(as Type expr)                      ; 數值/字元的型別轉換。可能失敗的轉換失敗時 panic
(try-as Type expr)                  ; 與 as 相同，但以 Option<Type> 回傳結果（失敗時為 None）
(print control args...)             ; 展開格式寫到標準輸出（不換行）
(println control args...)           ; 同上（最後換行）
(format dest control args...)       ; CL 的 format。回傳展開結果的 string
(pprint x)                          ; 以 pretty printer 美化輸出。依 CL 先輸出換行
(pprint-fill x)                     ; 填滿版面
(pprint-linear x)                   ; 全部一行或每行一個元素
(pprint-tabular x [colinc])         ; 表格版面（預設 16 欄）
(pprint-logical-block (obj :prefix p :suffix s) body...)  ; 自己建構邏輯區塊
```

`print`/`println`/`format`/`pprint` 系列是特殊形式，所以可變引數（`pprint` 系列是一個對象）以各自的型別包裝進 `Sexpr` 傳遞——
`(println "~a" my-struct)` 能直接運作就是這個原因。格式指令與 pretty printer 的詳情見[格式指令](functions/format.md)與
[列印](functions/printing.md#4-pretty-printer)。

`as`/`try-as` 能處理的只有數值與字元的目錄（`int`、固定寬度整數型別、`f32`/`f64`/`ratio`/`char` 之間）。同一型別不轉換。**整數寬度之間（包括
`int`）以及 `f32`↔`f64` 是真正的轉換**——`as` 截斷／捨入，`try-as` 回答能否放進該寬度（精度）。`(as int x)` 是從固定寬度的精確擴大，
`(as i32 n)` 是從 `int` 的截斷。整數→`char` 可能因超出範圍而失敗，所以 `as` 會 panic，`try-as` 為 `None`。其他（擴大轉換以及
`float->int`/`ratio->int` 的截斷）一律成功。`float->int`/`ratio->int`/`char->int` 落在 `int`，要求更窄的寬度時接著呼叫 `int->W`。是展開為
對應轉換方法（[數值](functions/numbers.md)的 `int->char`/`int->int`/`int->W` 等）的語法糖。

`documentation` 與 `quote`/`compile` 一樣是不對 `name` 求值、以未求值的裸符號/`::` 路徑讀取的特殊形式。與 CL 的 `(documentation 'name 'function)`
不同，不接受型別引數——依變數→函式→型別→trait→巨集的順序（與把裸識別字作為運算式求值時的優先順序相同）解析 `name`，回傳找到的定義的文件
字串（`(documentation Type::method)` 專用於方法）。解析本身失敗（沒有該名稱的定義）是檢查時的錯誤，有定義但沒有文件字串時為 `Option::none`。
全部在檢查時作為常數決定——不會發生執行期的查找。不支援帶模組限定的自由名稱（`mod::name`，`Type::method` 除外）。

## 8. 非區域跳出（catch / throw / unwind-protect）

```lisp
(catch 'tag body)                   ; 執行 body。在 body 所能到達範圍的任何地方
                                    ; 發生 (throw 'tag v) 時，以該 v 為值
(throw 'tag value)                  ; 跳到最近的動態外圍 (catch 'tag ...)
(unwind-protect protected cleanup)  ; 無論以何種方式離開 protected 都執行 cleanup
```

與 `break`/`return`（第 5 章）不同，這是**動態的**跳出——`throw` 並不在詞法上尋找包住自己的 `catch`，跨越多少層函式都能到達同一標籤的 `catch`。

```lisp
(defun find-first ((xs Option<Sexpr>)) int
  (catch 'found
    (progn
      (dolist (x xs)
        (match x ((int n) (if (> n 10) (throw 'found n) ())) (_ ())))
      -1)))                         ; 找不到時照常為最後的值
```

- **標籤只能是字面的符號**（`'done`）。與 CL 不同，不會被求值。
- **標籤攜帶型別。** `'tag` 第一次使用時決定型別，之後同一符號的 `throw`/`catch` 全部與它比對。以別的型別使用是型別錯誤。
- `throw` 的型別是 `!`（發散）。`(catch 'tag expr)` 的型別是 `expr` 的型別與標籤型別的合併型別。
- `unwind-protect` 的值是 `protected` 的值。`cleanup` 的值被丟棄。無論以何種方式離開 `protected`，`cleanup` 都會執行——除了正常結束、`throw`、
  panic，以 `break`/`return`/`return-from` 離開時也會執行。`cleanup` 本身的非區域跳出勝過正在進行的跳出。
- 巢狀的 `unwind-protect` 由內而外依序執行。離開 `protected` **內部**迴圈的 `break` 並沒有離開 `protected`，所以其 `cleanup` 不執行。

沒有採用 CL 的條件（`define-condition`/`handler-bind`/`invoke-restart`）。它們與靜態型別不相容，所以可恢復的失敗以 `Result` 表示（第 9 章）。

## 9. 錯誤處理方針

- 可恢復的失敗以 `Result<T,E>` + `match`。不可恢復的失敗（bug、不變式被破壞）以 `panic`。
- 沒有相當於 `?`/try 的語法。分支以 `match` 明確寫出。
- 函式與特殊形式的名稱不使用 `!`（破壞性操作）或 `?`（述詞）作為字尾。述詞以 `-p`/`p` 字尾（`zerop` `consp` 等）或前置 `is-`（`is-some`
  `is-ok` 等）命名。

## 10. 編譯

```lisp
(compile name)                      ; 把已定義的 defun/方法 JIT 編譯為原生碼
(compile-file src-path out-path)    ; 把原始檔 AOT 編譯為原生執行檔（略過最後的 `(main)`）
(dump path)                         ; 把目前的環境（型別資訊 + 編譯後的本體）寫到一個檔案
(disassemble name)                  ; 列印該定義變成了什麼（預設是主機的機器碼，第 2 個引數為 true 時是 LLVM IR）
```

`compile` 是特殊形式，`name` 不被求值，以未求值的裸符號/`::` 路徑讀取（字串是型別錯誤）。泛型函式不能作為對象——每種型別的實體在每個使用處產生，
不存在單一的編譯後本體。**無法解析的名稱是檢查時的錯誤**，不會延後到執行期（型別存在但沒有該方法／型別與函式都沒有／裸的未定義名稱，分別有
不同的訊息）。這裡的可見性與其他參照同樣處理，「存在但從這裡不可見」與「無法解析」一樣在檢查時失敗。

被呼叫者也會被遞移編譯，所以**（即使間接地）呼叫了不能編譯的東西的函式不能編譯**。行程不會當掉，而是以說明這一點的錯誤拒絕。所有內建函式都能
編譯，所以以這種形式被拒絕的只有呼叫以下直譯器專用操作的函式：

```lisp
(defun g () int 1)
(defun f () () (progn (trace g) ()))
(compile f)
; => trace: `(trace ...)` is an interpreter-only action and cannot itself be compiled
```

直譯器專用的是 `compile`/`compile-file`/`dump` 以及 `trace`/`untrace`/`step`/`disassemble`
（[實作工具](functions/system.md#5-實作工具clhs-252)）。與其說它們不能編譯，不如說它們是進行編譯一方的操作（`dump` 寫出的是直譯器的環境本身，
而 AOT 執行檔中沒有那個環境。`trace` 看的、`step` 停住的都是正在執行的直譯器的呼叫路徑，`disassemble` 使用編譯器本身）。`room`/`dribble`/`ed`
不是這一類，可以正常編譯。

**可以**編譯的東西：串流與檔案 I/O、`random`、`gensym`、`symbol->string`/`string->symbol`、`parse-int`/`parse-float`、`get-universal-time`/
`get-internal-real-time`、`exit`、超越函式、位元運算、`catch`/`throw`/`unwind-protect`、`eq`/`eql`/`equal`/`equalp` 全部 4 個（`case` 因此
也能對所有型別編譯）、包括 `print`/`println`/`format`/`pprint` 與 `pprint-logical-block` 在內的全部列印功能、`read`，以及 `eval`。標準函式庫
以已編譯的狀態隨附。

AOT 執行檔中只包含程式使用的功能。不列印的程式不包含格式引擎，不呼叫 `read` 的程式不包含讀取器，不呼叫 `eval` 的程式不包含檢查器與直譯器。

在命令列中，`typl -c src-path [-o out-path]`（`-c` 也可以寫成 `--compile`）做與 `compile-file` 相同的事。省略 `-o` 時，輸出為從 `src-path`
去掉副檔名 `.typl` 的名稱。連結到執行檔的靜態函式庫 `libtypelisp_front.a`，預設情況下：發行版建置的 `typl` 在第一次連結時把內部帶有的函式庫寫出
到 `$TYPELISP_HOME/lib/<建置ID>/`（沒有 `TYPELISP_HOME` 時為 `~/.typelisp/lib/<建置ID>/`）並使用它；除錯版建置使用建置 `typl` 處的函式庫。
`typl --remove-lib` 刪除該 `typl` 寫出的函式庫。加 `--others` 刪除其他建置 ID 的，加 `--all` 刪除所有建置 ID 的。指定 `typl --lib-dir DIR` 時
使用 `DIR` 中的函式庫（對 `-c` 與 `compile-file` 都有效），那裡沒有時在啟動時出錯。

### 10.1 傾印

```lisp
(dump "session.typld")     ; 寫出
```
```sh
typl --image session.typld prog.typl   # 從它啟動
typl --image session.typld             # REPL 也一樣
```

傾印是把型別資訊與編譯後的本體放進一個檔案的東西。`(dump path)` 寫出的是目前工作階段讀入的東西（標準函式庫，或者以 `--image` 傳入的傾印）加上
**工作階段本身定義的東西**。所以輸出是自成一體的，以 `typl --image` 可以啟動相同的環境。工作階段中 `(compile f)` 過的東西以編譯後的形式寫出。

保存的是**定義而不是歷史**：

- 工作階段的頂層運算式（`(println ...)` 等）不包括在內。載入時重新執行會造成麻煩。
- 全域變數以**重新執行初始化式得到的值**還原，而不是傾印時的值。這是與 SBCL 的 `save-lisp-and-die`（把堆積原樣寫出）刻意的不同，這個選擇使
  「無法保存的值」——開啟的串流、閉包的函式指標、外部記憶體——這一整類問題都消失了。
- 與 `save-lisp-and-die` 不同，**行程不會結束**。因為寫出不會破壞映像。

傾印記錄了寫出它的實作的標準函式庫與編譯器的版本。以版本不同的 `typl` 讀入時會出錯，絕不會默默接受。

### 10.2 AOT 執行檔中的 `eval`

`eval` 對「目前的全域環境」進行型別檢查後再求值（[解析與求值](functions/system.md#6-解析與求值)）。那個環境——檢查器查找的簽章、型別、巨集的表，
以及直譯器能執行的本體——**不在機器碼中**。編譯後的函式只是放在某個位址上的符號，既沒有引數的型別，也沒有依名稱查找本體的表。

因此，`compile-file` 只對呼叫 `eval` 的程式，**在編譯時組裝出那個環境並寫進執行檔**。格式與傾印相同，包含標準函式庫的部分與程式本身的部分。
啟動時只做還原，不會重新讀取原始碼，也不會重新進行型別檢查。不呼叫 `eval` 的程式什麼也不加入。

後果：

- **啟動較花時間，執行檔較大。** 因為包含了檢查器與直譯器的程式碼以及環境的快照。堆積也取得稍大一些。
- **eval 的形式被直譯執行。** 即使 eval 呼叫程式本身函式的形式，執行的也是快照所持有的直譯執行用的本體。結果相同，只有速度不同。

全域變數的儲存與編譯後的程式碼**共享**（同一個槽）。`defvar` 的初始化式由編譯後的初始化執行一次，還原時略過——以免有副作用的初始化式執行兩次。

`compile-file` 也會讀取標準函式庫（把其本體嵌入執行檔），所以 `abs`/`gcd` 這樣的標準函式庫函式，以及 `(impl print-object ...)`、
`(defmethod print-object ...)` 都可以在 AOT 中使用。

`compile-file` 也接受 `use`（以及 `import`/`shadowing-import`）。進入點檔案的 `(use m)` 依與 `typl file.typl` 相同的規則尋找檔案，找到的相依
檔案也會被編譯並連結進執行檔——`main.typl` 以 `(use http)` 讀取 `http.typl` 的結構也能直接進行 AOT 編譯。進入點檔案本身的定義也與
`typl file.typl` 一樣放進以檔名命名的模組（`p.typl` 中的 `point` 是 `p::point`）。所以值的顯示（`#<p::point x: 1 y: 2>`）無論以哪種方式執行
都相同。

## 11. 讀取巨集（readtable）

可以從程式中替換讀取器**遇到某個字元時做什麼**（CLHS 23.1）。

```lisp
(set-macro-character c f)             ; 由 f 讀取字元 c
(get-macro-character c)               ; Option<f>
(set-dispatch-macro-character d s f)  ; 由 f 讀取兩個字元的序列 d s
(get-dispatch-macro-character d s)    ; Option<f>
```

`f` 的型別是 `(fn (string-input-stream char) Option<Sexpr>)`。第 1 個引數是**以尚未讀取的文字為內容的串流**，第 2 個引數是**觸發的字元**（分派
時為第 2 個字元）。回傳值成為在那裡讀到的資料。串流是具體型別而不是 `:dyn PeekInput`，是因為讀取器傳遞的一律是這一種——`read-sexpr` /
`read-char` / `peek-char` / `unread-char` / `read-delimited-list` 都是 `(where (PeekInput S))`，所以具體型別可以直接全部使用。

```lisp
(set-macro-character #\!
  (lambda ((s string-input-stream) (c char)) Option<Sexpr>
    (match (read-sexpr s)
      ((ok o) (match o
                ((datum d) (sexpr-cons (quote not) (sexpr-cons d (quote ()))))
                ((eof) (quote ()))))
      ((err e) (quote ())))))

!(equal 1 2)   ; => 讀作 (not (equal 1 2))，即 true
```

讀取器**先於內建語法查看巨集字元**，所以也可以奪取 `(` 與 `'`。`#` 的子字元的登記優先於內建的 `#b`/`#x`/`#.`。`#` 以外的字元傳給
`set-dispatch-macro-character` 也會當場成為分派字元——**沒有**相當於 CL 的 `make-dispatch-macro-character` 的東西。登記本身就發揮了它的作用，
作為另一個步驟保留也沒有事可做。

**何時生效**與 `#.`（第 1 章）相同，取決於讀取路徑：

- REPL 與 `(load ...)` 逐個形式執行，所以可以直接登記**前面形式中定義的函式**。
- 模組檔案作為一個單元檢查，之後才執行——所以 `set-macro-character` / `set-dispatch-macro-character` 的**只有呼叫本身會立即執行**（相當於 CL 的
  `(eval-when (:compile-toplevel) ...)` 的角色）。既然立即執行，**傳入的函式在那個時刻必須已經存在**。同一檔案中的 `defun` 還沒有執行，所以請以
  `lambda` 書寫，或使用標準函式庫或已經執行過的東西。對象只是頂層的呼叫，不會查看 `progn` 或 `let` 內部。

內建的 `read` / `read-from-string` 也會參照 readtable（與 CL 相同）。

**沒有的東西**：`*readtable*` 與 `copy-readtable`，以及 `readtable-case`。前兩者是因為 readtable **不是值**——作為值它必須是「可以交給讀取器的
東西」，但讀取原始碼的讀取器在程式之外，沒有可以交給的去處。`readtable-case` 是因為第 1 章規定了這個語言的讀取器一律轉為小寫（CL 的
`:downcase`）。


## 12. 並行（任務）

**任務是輕量級執行緒**（以 Go 來說就是 `go` 敘述啟動的東西），以協作方式執行（沒有搶占）。切換不經過核心，執行狀態在堆積上而不是機器堆疊上，
所以任務可以低成本地大量建立。

**任務在多個 OS 執行緒上同時執行**（多核心平行）。執行緒數由環境變數 `TYPELISP_THREADS` 決定（包括執行 `main` 的執行緒在內的總數，預設是機器的
平行度）。在 `typl` 中**只有編譯過的任務**會在其他執行緒上執行，直譯執行的任務在直譯器的執行緒上執行（12.7）。共享資料要經過 `Mutex<T>` 或
`Chan<T>`——不經過它們的同時讀寫與 Go 一樣是未定義的（12.7）。

詞彙中**特殊形式只有 `task` / `thread` / `select` 這 3 個**，其餘是一般的函式、方法與巨集（[任務與通道](functions/concurrency.md)）。

### 12.1 `task` — 啟動任務

```lisp
(task (f arg...))                   ; 回傳 Task<T>。T 是 f 的回傳型別
```

**只接受呼叫形式。** `f` 與各 `arg` 都在寫 `task` 的地方依書寫順序求值，在新任務中發生的只有**呼叫**。這與 Go 的 `go f(x)` 規則相同，也是它
接受呼叫形式而不是 thunk 的原因——thunk 會不求值就捕獲引數。

```lisp
(dotimes (i 10)
  (task (worker i ch)))             ; i 每次當場求值。沒有捕獲的陷阱

(task ((lambda () ()                ; 想執行任意本體時呼叫 lambda
         (println "start")
         (send ch 1))))
```

特殊形式（`if` / `let` / `progn` …）不能直接寫在 `task` 之下。

**不能做成函式的原因**：如果寫成 `(spawn (lambda () T body...))`，就需要寫出 `T`，而 `lambda` 必須有回傳型別註記，巨集又不知道 `(f a b)` 的回傳
型別。知道它的只有檢查器。

### 12.2 `thread` — 在專用 OS 執行緒上啟動任務

```lisp
(thread (f arg...))                 ; 回傳 Thread<T>。T 是 f 的回傳型別
(join th)                           ; 等待完成並回傳其值（可以多次）
```

形式與求值規則與 `task` 相同（只接受呼叫形式，`f` 與 `arg` 都在書寫處求值）。不同之處在於執行的地方：**為該任務專門啟動一個 OS 執行緒，只在它
上面執行**。它不與其他任務多工，所以即使在其中呼叫會阻塞的 C 函式（`defffi`），停下的也只有那個執行緒，其他任務照常前進。其中可以直接使用
`task`・`send`・`recv` 等。

- `Thread<T>` 是 `Task<T>` 的對應物。`join` 與 `wait` 一樣停下的是**呼叫它的任務**，值會被快取。任務結束時執行緒也結束。
- panic 的規則與 `task` 相同（整個行程當掉）。`main` 返回時行程結束。
- 想以函式形式書寫時，用 `(Thread::spawn (lambda () T body...))`（Rust 的 `std::thread::spawn`）。也可以傳入具名函式。
- **在專用執行緒上執行的只有編譯過的程式碼。** `typl` 在直譯執行中對 `(thread (f ...))` 或 `Thread::spawn` 求值時，會當場編譯要執行的
  函式（以及從它呼叫的東西）再執行。不能編譯的東西——參照外部區域變數的 `lambda`、結構的建立等——會在啟動執行緒之前成為與
  `(panic ...)` 同樣處理的 panic。參照區域變數的 `lambda`，只要在編譯過的函式中建立，就可以傳入。

### 12.3 `select` — 同時等待多個通道操作

```lisp
(select
  ((v (recv ch1)) body...)          ; 接收分支。v 繫結為 Option<T>
  ((send ch2 x) body...)            ; 傳送分支
  (else body...))                   ; 可以省略。**要寫的話放在最後**
```

- **有 `else` 時不會阻塞**（Go 的 `default`）。沒有時等到某個分支變得可行。
- **同時有多個可行時隨機選擇一個**（依書寫順序的話，後面的分支會餓死）。
- 接收分支的 `v` 是 **`Option<T>`**。關閉的通道是「回答」而不是略過分支的理由，所以在分支中 `match`。
- 型別是**所有分支本體型別的合併型別**（與 `match` 分支的規則相同）。
- 0 個分支的 `(select)` 是型別錯誤（不採用 Go 的 `select{}`＝永久阻塞）。只有 `else` 的 `select` 也一樣——與直接寫本體相同。

**無論選擇哪個分支，通道運算式與要傳送的值都由左至右各求值一次**（與 `case` 對鍵所持的規則相同）。

```lisp
(select                             ; 帶逾時的接收
  ((v (recv ch))          (println "~a" (unwrap v)))
  ((z (recv (after 0.5))) (println "timeout")))
```

`after`（[依時間送達的通道](functions/concurrency.md#5-after--依時間送達的通道)）是「`sec` 秒後送達一個值的通道」，相當於 Go 的 `time.After`。

### 12.4 與其他功能的關係

| 功能 | 與任務的關係 |
|---|---|
| `catch` / `throw` | **不越過任務邊界。** 要跳出任務本體的 `throw` 是 panic |
| `unwind-protect` | 任務自然結束時 cleanup 會執行。**因主任務結束造成的行程結束時不執行** |
| `block` / `return-from` | 是詞法的，所以不越過 `lambda` 邊界 |
| `panic` | 與 Go 一樣整個行程當掉。`wait` 不會把 panic 當作值觀察到 |
| `dlet` | **不是依任務的繫結。** 仍然是「借用全域變數再歸還」，所以任務之間會互相干擾 |
| 標準輸出 | 所有任務共享。一次 `println` 的輸出不會在行中間與其他輸出混在一起 |
| `compile` / `eval` | 沒有限制。任務中的 `(compile f)` 可以通過 |

### 12.5 切換發生的位置

由於是協作式排程，**只在你寫下的地方切換**：`(yield)`、`(sleep ...)`、`(wait ...)`、**需要等待的通道操作**（`send`/`recv`/`select`），以及
**需要等待的 socket 操作**（`accept`／`tcp-connect`（包括名稱解析）／socket 的讀寫／`recv-from`，[網路](functions/network.md)）。socket 全部
是非阻塞的，尚未就緒時只有該任務停下，在 OS 回答已就緒時恢復——與 Go 的 netpoller 形式相同。只有在沒有可執行的任務時，實作才會等待 OS 直到最近的
`sleep` 期限。

能當場得到回答的通道操作——緩衝區有空位的 `send`、有值的 `recv`、`(len ch)`/`(cap ch)`/`(close ch)`/`(Chan::new n)`——**不消耗執行權**。這表示
不會因為讀取而被意外打斷，與作為 CL 式「讓出 0 秒」的 `(sleep 0.0)` 區別對待。

**沒有搶占。** 什麼都不呼叫的緊密迴圈會讓其他任務餓死——不過編譯過的迴圈會定期把控制權交給排程器，所以編譯過的緊密迴圈不會讓其他任務餓死。

### 12.6 編譯後的程式碼與任務

編譯後的程式碼也能暫停任務。以 `compile-file` 產生的執行檔也一樣，`main` 作為排程器的主任務執行——`task`・`sleep`・`wait`・通道・socket 等待，
全部以與 `typl` 相同的意義運作，`main` 返回時行程結束，其餘任務被中斷（與 Go 相同）。不會為了排程器而把直譯器放進執行檔。

例外只有「C 的 FFI 回呼中」，在那裡**需要等待的**操作是錯誤（比默默死結更友善）——以 `defffi` 傳入的函式被 C 呼叫期間，C 的堆疊疊在上面，沒有
暫停任務、之後再恢復的手段。

以下位置也是在任務途中被呼叫的函式，卻不能暫停：`print-object` 方法、`format` 的 `~/name/`、讀取巨集、`eval` 內部、AOT 執行檔的 `defvar`
初始化式。在這些地方，**不等待就能得到回答的操作可以通過**（緩衝區有值的 `(recv ch)`、已收到資料的 socket 的 `read-line`、`(task ...)`、
`(yield)` 等），**真正需要等待的操作是錯誤**（不是當場停止行程，而是作為 `` `recv` cannot block: ... `` 這樣與 `(panic ...)` 同樣處理的 panic）。

### 12.7 與 Go 的差異

- **在 `typl` 中到其他執行緒上執行的只有編譯過的任務。** 直譯器的狀態不能在執行緒之間共享，所以直譯執行的 `task` 的任務在直譯器的執行緒上執行。
  編譯過的任務也會在呼叫直譯執行的函式值、呼叫沒人編譯過的 `:dyn` 方法、呼叫 `eval`/`macroexpand`/`read` 的時刻**移到直譯器的執行緒，之後一直
  留在那裡**（不會回去）。長時間的處理途中只要接觸過一次直譯執行的程式碼，剩下的部分就在直譯器的執行緒上執行。
- **`typl` 的工作執行緒只存活一次頂層求值的時間。** 在 REPL 等待輸入期間以及頂層形式之間，其他執行緒不推進任務（剩下的任務在下次求值時接著
  執行）。求值結束時會等待各執行緒完成目前的一步，所以如果 `thread` 中有持續阻塞的 C 函式（`defffi`），在它返回之前求值不會結束。
- **工作執行緒上的列印**：直譯執行的 `print-object`／`~/name/` 方法不能在其他執行緒上執行，所以在其他執行緒上列印這樣的值會成為與 `(panic ...)`
  同樣處理的 panic（`(compile T::print-object)`，或者從主任務列印）。
- **資料競爭是未定義的**（與 Go 立場相同）。不經過 `Mutex<T>`・`Chan<T>` 從多個任務修改同一個值的結果沒有保證。
- **`task` 回傳值。** 與 Go 的 `go` 敘述不同，它回傳 `Task<T>`，以 `(wait t)` 可以取得結果。
- **沒有 nil 通道。** Go 的 fan-in 慣用法（把關閉的通道設為 `nil` 以從 `select` 的分支中去掉）無法寫出，所以為每個輸入啟動一個任務，再以
  `WaitGroup` 匯合（[WaitGroup](functions/concurrency.md#4-waitgroup--等待-n-個完成)）。這在 Go 中也是推薦的寫法，但卻是**從 Go 轉過來的人
  最先遇到的差異**。
