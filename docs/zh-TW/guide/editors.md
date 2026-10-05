<!-- translated-from: docs/ja/guide/editors.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# 編輯器整合（typl-lsp）

`typl-lsp` 是 typelisp 的語言伺服器。連接到支援 LSP（Language Server Protocol）的編輯器後，可以對正在編輯的檔案使用以下功能：

- 診斷：讀取錯誤、型別錯誤、重新定義的警告
- 懸停：括號運算式的型別以及所呼叫定義的文件字串（裸變數名稱不顯示）
- 跳至定義
- 自動完成（輸入 `:` 時顯示候選）
- 型別名稱著色（semantic tokens）：從其他檔案 `use` 的型別也會著色

透過 `use` 跨檔案的參照也會被解析。在其他檔案中尚未儲存的編輯，也會立即反映到 `use` 該檔案的檔案的診斷中。

## 1. 建置

```sh
cargo build --release --bin typl-lsp
```

會產生 `target/release/typl-lsp`。如果依 [README.md](../../../README.md) 的步驟以 `cargo install` 安裝，它會與 `typl` 一起放在
`~/.cargo/bin/typl-lsp`。

## 2. VS Code

擴充功能位於儲存庫的 `editor/vscode`。它沒有發布到 Marketplace，需要自行建置安裝。

```sh
cd editor/vscode
npm install
npm run compile
npx @vscode/vsce package      # 產生 .vsix
```

在擴充功能檢視的「…」選單中選擇「Install from VSIX...」，指定產生的 `.vsix`。

擴充功能依工作區的 `target/release/typl-lsp`、`target/debug/typl-lsp`、`PATH` 的順序尋找 `typl-lsp`。放在其他位置時，請在設定
`typelisp.languageServer.path` 中寫上路徑。

| 設定 | 預設值 | 內容 |
|---|---|---|
| `typelisp.program` | `typl` | `typl` 的路徑 |
| `typelisp.languageServer.enable` | `true` | 是否連接 `typl-lsp` |
| `typelisp.languageServer.path` | （空） | `typl-lsp` 的路徑 |

`Ctrl+Alt+R` 儲存正在編輯的檔案並以 `typl` 執行，`Ctrl+Alt+Z` 啟動 REPL。詳情請參閱
[VS Code 擴充功能的 README](../../../editor/vscode/README.md)（日文）。

## 3. Emacs

`typelisp-mode` 位於儲存庫的 `editor/emacs`。

```elisp
(add-to-list 'load-path "/path/to/typelisp/editor/emacs")
(require 'typelisp-mode)
```

以 `eglot`（Emacs 29 以後內建）連接 `typl-lsp` 的設定：

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

eglot 不支援 semantic tokens，所以使用 eglot 時由 `typelisp-mode` 代為替型別名稱著色。使用 `lsp-mode` 時，請把
`lsp-semantic-tokens-enable` 設為 `t`。

`C-c C-c` 執行正在編輯的檔案，`C-c C-z` 啟動 REPL。詳情請參閱[typelisp-mode 的 README](../../../editor/emacs/README.md)（日文）。

## 4. 其他編輯器

`typl-lsp` 透過標準輸入輸出以 LSP 通訊，不接受命令列引數。請在編輯器的 LSP 用戶端中設定為對 `.typl` 檔案啟動 `typl-lsp`。

## 5. 專案的辨識

`typl-lsp` 從開啟的檔案所在目錄開始往上尋找 `typelisp.toml`，以該位置作為原始碼根目錄解析 `use`。規則與以 `typl` 執行檔案時相同
（[模組與檔案結構](modules.md#2-建立專案)）。由多個檔案構成的專案，請在根目錄放置 `typelisp.toml`。

## 6. 語言伺服器不會執行程式

`typl-lsp` 只透過讀取與型別檢查來提供診斷，不會執行正在編輯的程式。每次按鍵都會進行診斷，不能在那裡執行有副作用的程式碼或不會結束的程式碼。
唯一的例外是登記 `defmacro`，這是檢查其後的巨集呼叫所必需的。

因此，只在以 `typl` 執行時才會發生的錯誤（panic、檔案不存在等）不會出現在語言伺服器的診斷中。
