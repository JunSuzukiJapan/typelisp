<!-- translated-from: docs/ja/README.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# typelisp 文件（繁體中文）

typelisp 是一種靜態型別的 Lisp。安裝與建置方法請參閱儲存庫根目錄的 [README.md](../../README.md)（英文）。

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

設定步驟見[編輯器整合指南](guide/editors.md)。各編輯器的按鍵與設定一覽見以下文件（日文）：

- [Emacs（typelisp-mode）](../../editor/emacs/README.md)
- [VS Code](../../editor/vscode/README.md)
