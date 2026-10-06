<!-- translated-from: editor/emacs/README_JP.md @ e5e6bdf72dbe4cf76a395c536f23b887cdae8fea -->
# typelisp-mode（Emacs）

用來編輯 typelisp 原始碼（`.typl`）的 Emacs 主模式。
VS Code 版見 [../vscode/](../vscode/README_zh-TW.md)。兩者使用相同的關鍵字表與相同的縮排規則，
這一點由 `cargo test --test editor_keyword_sync_test` 機械地檢查（見本文末尾）。

## 功能

- 語法高亮
  - 特殊形式與控制結構（`defun` `let` `if` `match` `loop` `lambda` `setf` `as` `apply`、
    `print`/`println`/`format`、`pprint` 系列等）
  - 定義名（`(defun NAME ...)` 的 `NAME` 作為函式名，`(defstruct NAME ...)` 作為型別名，
    `(defvar (NAME ...))` 作為變數名。帶 `pub` 的 `(pub defun NAME ...)` 也一樣）
  - 命名空間與宣告關鍵字（`pub` `module` `use` `load` `impl` `where`）以及 lambda 串列標記
    （`&rest` `&optional` `&key`）
  - 內建函式（`car` `map` `foldl` `unwrap` `parse-int` `message` `sexpr-car` 等）
  - 基本型別（包含 `bignum` / `ratio`）、內建型別、內建錯誤型別（`ParseIntError` 等）、
    `Capitalized` 的使用者型別、trait 物件型別 `:dyn Trait`
  - **使用者定義型別的使用處**（`defstruct`/`defenum`/`deftrait` 的名稱通常是小寫
    （`rect` `todo-item` `board`），`Capitalized` 規則抓不到）。
    連接 `typl-lsp` 時依伺服器的 semantic tokens 著色（`eglot` 也可用，見下文）。
    未連接時退回到收集緩衝區內型別名稱的方式
  - 字面值（`true` `false`、數字字面值（十進位 / `0xff` / `1.5` / `1/3`）、
    字元字面值 `#\Space`、字串、關鍵字 `:name`）
  - 字串中的 `format` 控制指令（`~a` `~5,'0d` `~{...~}` 等）
  - CL 風格帶耳罩的全域變數（`*print-pretty*` 等）
- 註解
  - 行註解 `;`
  - **可巢狀的**區塊註解 `#| ... |#`
- S 運算式導覽與 Lisp 風格縮排
- 透過 `imenu` 提供定義清單（函式 / 方法 / 巨集 / 型別 / trait / `impl` / 變數 / 模組）
- 呼叫 `typl` CLI 的命令（見下文）

## 按鍵綁定

| 按鍵 | 命令 | 作用 |
|---|---|---|
| `C-c C-c` | `typelisp-run-buffer` | 儲存並以 `typl FILE` 執行（經由 `compile`，可跳到錯誤行） |
| `C-c C-z` | `typelisp-repl` | 在 comint 緩衝區中啟動 `typl` 的 REPL |

`typl` 的位置以 `typelisp-program`（預設 `"typl"`）指定。
診斷的格式是 `error: FILE:LINE:COL: ...`，`compilation-mode` 能夠解析，
用 `next-error` / `C-x \`` 可以直接跳到對應位置。

## 安裝

```elisp
(add-to-list 'load-path "/path/to/typelisp/editor/emacs")
(require 'typelisp-mode)
```

`.typl` 檔案會自動以 `typelisp-mode` 開啟（已登錄到 `auto-mode-alist`）。

使用 `use-package` 時：

```elisp
(use-package typelisp-mode
  :load-path "/path/to/typelisp/editor/emacs"
  :mode "\\.typl\\'")
```

## 語言伺服器（`typl-lsp`）

建置 `typl-lsp` 之後，就能從 `eglot`（Emacs 29+ 內建）或 `lsp-mode` 使用。

```sh
cargo build --release --bin typl-lsp
```

使用 `eglot` 時：

```elisp
(with-eval-after-load 'eglot
  (add-to-list 'eglot-server-programs
               '(typelisp-mode . ("/path/to/typelisp/target/release/typl-lsp"))))
(add-hook 'typelisp-mode-hook #'eglot-ensure)
```

使用 `lsp-mode` 時：

```elisp
(with-eval-after-load 'lsp-mode
  (lsp-register-client
   (make-lsp-client
    :new-connection (lsp-stdio-connection "/path/to/typelisp/target/release/typl-lsp")
    :major-modes '(typelisp-mode)
    :server-id 'typl-lsp)))
(add-hook 'typelisp-mode-hook #'lsp)
```

支援診斷（語法/型別錯誤與重新定義警告，透過 `textDocument/publishDiagnostics` 通知）、hover、
跳到定義（goto-definition）、補全（`:` 已登錄為觸發字元）以及 semantic tokens。
透過 `use` 跨檔案的參照會被解析（向上尋找專案根目錄的 `typelisp.toml`，詳見
[語法參考 3.11](../../docs/zh-TW/reference/syntax.md#311-檔案與模組的對應多檔案專案)）。
已開啟的編輯器緩衝區中尚未儲存的編輯，會立即反映到相依檔案與被相依檔案雙方的診斷中。

### 型別名稱高亮（semantic tokens）

伺服器透過 `textDocument/semanticTokens` 回報**檢查器實際解析為型別名稱的位置**。
由於這不是文字比對：

- 透過 `use` 從其他檔案引入的型別也會著色（這是緩衝區內解析在原理上達不到的範圍）
- 與型別同名的**函式**的呼叫處不會著色（檢查器在那裡將其解析為函式，因此根本不會記錄 token）

用戶端方面：

- **`eglot`（Emacs 31 以後）**：eglot 自己繪製（`eglot-semantic-tokens-mode`）。
  `typelisp-mode` 不介入
- **`eglot`（Emacs 30 以前）**：這個版本的 eglot 不處理 semanticTokens。因此
  **`typelisp-mode` 自行送出請求並以 overlay 繪製**
  （`typelisp-semantic-tokens-mode`，eglot 連線時自動啟用）
- **`lsp-mode`**：原生支援（把 `lsp-semantic-tokens-enable` 設為 `t`）。此時
  `typelisp-mode` 不介入

`scripts/emacs-semantic-smoke.el` 會實際透過 eglot 連線，檢查在所用 Emacs 中負責繪製的一方。
無論哪種用戶端，在伺服器回應期間，緩衝區內解析的備援方式都會退讓（以免兩套規則替同一個緩衝區著色）。

| 設定 | 預設值 | 作用 |
|---|---|---|
| `typelisp-semantic-tokens` | `t` | 使用 Emacs 30 以前的 eglot 時，是否依伺服器的 semantic tokens 著色 |
| `typelisp-semantic-tokens-idle-delay` | `0.6` | 編輯後再次請求前的閒置秒數（應大於 `eglot-send-changes-idle-time`） |

## 備註

- typelisp 在讀取時會把符號轉為小寫，但高亮區分大小寫，以便區分大寫字母開頭的型別名稱。
- 縮排由專用的 `typelisp-indent-function` 查詢 `typelisp-indent-specs`（關聯串列）來決定。
  與 Emacs Lisp 同名的形式（`defun` `let` `if` …）也由本模式自己保存，是因為符號屬性是
  **全域的**，為 typelisp 所做的設定會改變同一工作階段中其他 Lisp 緩衝區的縮排。
  而且 typelisp 的形式即使與 Emacs Lisp 同名，形狀也不同——`(defun NAME (PARAMS) RETTYPE ...)`
  有 3 個標頭元素，`if` 固定為必須有 `else` 的 3 個元素——所以值也無法共用。
  已確認 `examples/` 下的所有 `.typl` 檔案用 `indent-region` 後一個位元組也不變，並且把縮排全部
  去掉後重新縮排會回到原樣（VS Code 版在同一批檔案上也滿足同樣的標準）。

## 偵測編輯器定義的偏差

關鍵字表與 VS Code 版是雙重維護的。為防止實作前進了而編輯器定義卻落後，Rust 一側有測試：

```sh
cargo test --test editor_keyword_sync_test
```

它實際載入 prelude，走訪登錄表，回報**任一編輯器不認識的名稱**。
特殊形式沒有執行期表示，因此從 `crates/typelisp-front/src/check/checker.rs` 中
`// SPECIAL-FORM DISPATCH BEGIN` / `END` 之間讀取（不要刪除這些註解）。
失敗時，把回報的名稱加到**兩個**編輯器定義中。

同一個測試也會核對 semantic tokens 的 legend（`src/bin/lsp.rs` 的 `SEMANTIC_TOKEN_TYPES` 與
兩個編輯器持有的對應表，名稱和順序都必須一致）。不一致不會造成執行期錯誤，只會讓所有 token 的
顏色互換，因此以機械方式固定下來。
