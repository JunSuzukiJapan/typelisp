<!-- translated-from: docs/ja/README.md @ 804fb3914b90de67225569295b86c07e97db869d -->
# typelisp 文件（繁體中文）

typelisp 是一種靜態型別的 Lisp。安裝與建置方法請參閱儲存庫根目錄的 [README.md](../../README.md)（英文）。

## 特色

### 任務與通道

啟動任務（輕量級執行緒），透過通道在任務之間傳遞值。詳見[並行](tutorial/concurrency.md)。

```lisp
(use std::sync::Chan)
(defun worker ((name string) (jobs Chan<int>) (results Chan<string>)) ()
  (doiter (n jobs)
    (sleep 0.1)
    (send results (format false "~a: ~d * ~d = ~d" name n n (* n n)))))

(defun main () ()
  (let ((jobs (the Chan<int> (Chan::new 0)))
        (results (the Chan<string> (Chan::new 10))))
    (task (worker "A" jobs results))
    (task (worker "B" jobs results))
    (dotimes (i 4)
      (send jobs (+ i 1)))
    (close jobs)
    (dotimes (i 4)
      (println "~a" (unwrap (recv results))))))

(main)
```

### 用 Option 和 Result 取代 nil

沒有 `nil`。可能沒有值時回傳 `Option<T>`，可能失敗時回傳 `Result<T,E>`，再用 `match` 取出其中的內容。若把 `Option<string>` 當作 `string` 使用，或在 `match` 中漏寫 `none` 的情況，程式執行之前就會出現型別錯誤。詳見[型別基礎](tutorial/types.md)與[錯誤處理](tutorial/errors.md)。

```lisp
(use std::num::parse-int)
(defun find-user ((id int)) Option<string>
  (case id
    (1 (Option::some "alice"))
    (2 (Option::some "bob"))
    (:else (Option::none))))

(defun parse-port ((s string)) Result<int,string>
  (match (parse-int s)
    ((ok n) (if (<= 1 n 65535)
                (Result::ok n)
                (Result::err (format false "port out of range: ~a" n))))
    ((err e) (Result::err (message e)))))

(match (find-user 3)
  ((some name) (println "user: ~a" name))
  ((none) (println "not found")))        ; not found

(match (parse-port "99999")
  ((ok p) (println "port ~a" p))
  ((err m) (println "error: ~a" m)))     ; error: port out of range: 99999
```

### 經過型別檢查的巨集

巨集的寫法與 Common Lisp 相同，使用 `defmacro`、準引用與 `gensym`。展開後的程式碼也會像一般程式碼一樣接受型別檢查，型別不符時會在呼叫巨集的位置回報錯誤。詳見[巨集](tutorial/macros.md)。

```lisp
(defmacro swap (a b)
  (let ((tmp (gensym "tmp")))
    `(let ((,tmp ,a))
       (setf ,a ,b)
       (setf ,b ,tmp))))

(let ((x 1) (y 2))
  (swap x y)
  (println "x=~a y=~a" x y))             ; x=2 y=1
```

```lisp
(defmacro add-a (x) `(+ ,x "a"))
(add-a 1)
;; error: type error: type mismatch: expected `int`, found `string`
```

## 教學

第一次接觸 typelisp 的讀者請依序閱讀。

- [入門](tutorial/intro.md)：REPL、函式、變數、條件分支、迴圈、串列與 `Vector`
- [型別基礎](tutorial/types.md)：靜態型別、`Option`、`Result`、結構、列舉、泛型
- [trait](tutorial/traits.md)：`deftrait` / `impl`、trait 約束、`:dyn`
- [巨集](tutorial/macros.md)：`defmacro`、準引用、`gensym`、`macrolet`
- [錯誤處理](tutorial/errors.md)：`Result`、`panic`、`catch` / `throw`、`unwind-protect`
- [並行](tutorial/concurrency.md)：任務、通道、`select`、`Mutex`、`thread`

## 指南

- [模組與檔案結構](guide/modules.md)：`use`、`pub`、檔案與模組的對應
- [編譯](guide/compile.md)：JIT、用 AOT 編譯產生執行檔、傾印
- [檔案 I/O、串流與網路](guide/io.md)：檔案、路徑名稱、TCP / TLS / UDP、名稱解析
- [C FFI](guide/ffi.md)：用 `defffi` 呼叫 C 函式（包括回呼以及用 `def-c-struct` 定義的 C 結構）
- [編輯器整合](guide/editors.md)：`typl-lsp` 以及 VS Code / Emacs 的設定
- [給 Common Lisp 使用者](guide/from-common-lisp.md)：與 CL 的差異以及改寫方式

## 參考

- [語法參考](reference/syntax.md)：詞法、型別的寫法、定義、控制結構、編譯、並行
- [內建函式](reference/functions/README.md)：內建函式、方法與標準函式庫
- [型別一覽](reference/types.md)：各種型別以及它們實作的 trait
- [錯誤訊息](reference/errors.md)：常見錯誤的意義與修正方式

## 編輯器整合

設定步驟見[編輯器整合指南](guide/editors.md)。各編輯器的按鍵與設定一覽見以下文件：

- [Emacs（typelisp-mode）](../../editor/emacs/README_zh-TW.md)
- [VS Code](../../editor/vscode/README_zh-TW.md)
