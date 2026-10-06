<!-- translated-from: editor/vscode/README_JP.md @ e5e6bdf72dbe4cf76a395c536f23b887cdae8fea -->
# typelisp（VS Code）

用來編輯 typelisp 原始碼（`.typl`）的 VS Code 擴充功能。
Emacs 版見 [../emacs/](../emacs/README_zh-TW.md)。兩者使用相同的關鍵字表與相同的縮排規則，
這一點由 `cargo test --test editor_keyword_sync_test` 機械地檢查（見下文）。

## 功能

- **語法高亮**（TextMate 語法，不需要語言伺服器）
  - 特殊形式與控制結構（`defun` `let` `if` `match` `loop` `lambda` `setf` `as` `apply`、
    `print`/`println`/`format`、`pprint` 系列）
  - 定義名（`(defun NAME ...)` 為函式，`(defstruct NAME ...)` 為型別，`(defvar (NAME ...))` 為變數。
    帶 `pub` 的 `(pub defun NAME ...)` 也一樣）以及 `(impl Trait Type)` 的兩個名稱
  - 命名空間與宣告關鍵字（`pub` `module` `use` `load` `impl` `where`）、
    lambda 串列標記（`&rest` `&optional` `&key`）
  - 內建函式、基本型別（包含 `bignum` / `ratio`）、內建錯誤型別、
    `Capitalized` 的使用者型別、trait 物件型別 `:dyn Trait`（泛型引數內部也是）
  - 數字字面值（十進位 / `0xff` / `1.5` / `3.0e10` / `1/3`）、字元字面值 `#\Space`、
    關鍵字 `:name`、帶耳罩的全域變數 `*print-pretty*`
  - **字串中的 `format` 控制指令**（`~a` `~5,'0d` `~{...~}` `~^` 等）
  - 行註解 `;` 與**可巢狀的**區塊註解 `#| ... |#`
- **使用者定義型別的使用處**（semantic tokens）
  - `defstruct` / `defenum` / `deftrait` 的名稱通常是小寫（`rect` `todo-item` `board`），
    `Capitalized` 規則抓不到，而 TextMate 語法逐行運作，看不到整個檔案。
    semantic tokens 看得到，從而解決了靜態型別語言卻只有型別註記不著色的狀況
  - 連接 `typl-lsp` 時，回傳的是**檢查器實際解析為型別名稱的位置**。因此透過 `use` 從其他檔案引入的
    型別也會著色，而與型別同名的**函式**的呼叫處不會著色
    （檢查器在那裡將其解析為函式，因此根本不會記錄 token）
  - 未連接或未建置時，擴充功能會退回到在檔案內部解析的文字掃描。這只是近似：
    找不到來自其他檔案的型別，也無法區分與型別同名的函式
- **Lisp 縮排**（VS Code 本身沒有 Lisp 縮排，因此由擴充功能實作）
  - 格式化文件、格式化選取範圍、輸入時格式化（Enter 與 `)`，啟用 `editor.formatOnType` 時）
- **Outline / breadcrumbs / `Ctrl+Shift+O`**（函式、方法、巨集、型別、trait、
  `impl`、變數、模組）
- **`typl-lsp` 整合**（診斷、hover、跳到定義、補全、semantic tokens）
- **`typl` CLI 命令**（執行、REPL）

語言伺服器以外的功能都只靠擴充功能本身就能運作，因此即使在沒有建置 `typl-lsp` 的檢出中，
高亮、縮排、Outline 與（限於檔案內的）型別高亮都可以使用。

## 安裝

擴充功能沒有發布到 Marketplace，需要在本機建置後安裝。

```sh
cd editor/vscode
npm install
npm run compile
```

然後擇一：

- **在開發主機中試用**：用 VS Code 開啟 `editor/vscode` 並按 `F5`
- **永久安裝**：用 `npx @vscode/vsce package` 產生 `.vsix`，
  然後在擴充功能檢視中選擇「…」→「Install from VSIX...」

`.typl` 檔案會自動以 typelisp 模式開啟。

## 按鍵綁定

| 按鍵 | 命令 | 作用 |
|---|---|---|
| `Ctrl+Alt+R` | `typelisp.runFile` | 儲存並以 `typl FILE` 執行 |
| `Ctrl+Alt+Z` | `typelisp.repl` | 啟動 `typl` 的 REPL |

命令選擇區中還有 `typelisp: Restart Language Server`。

## 設定

| 設定 | 預設值 | 作用 |
|---|---|---|
| `typelisp.program` | `typl` | `typl` CLI 的路徑 |
| `typelisp.languageServer.enable` | `true` | 是否連接 `typl-lsp` |
| `typelisp.languageServer.path` | （空） | `typl-lsp` 的路徑。為空時依工作區的 `target/release/typl-lsp` → `target/debug/typl-lsp` → `PATH` 的順序尋找 |
| `typelisp.trace.server` | `off` | 記錄 LSP 的 JSON-RPC |

用下面的命令建置語言伺服器：

```sh
cargo build --release --bin typl-lsp
```

透過 `use` 跨檔案的參照，會向上尋找專案根目錄的 `typelisp.toml` 來解析
（詳見 [語法參考 3.11](../../docs/zh-TW/reference/syntax.md#311-檔案與模組的對應多檔案專案)）。

## 工作的 problem matcher

擴充功能提供名為 `typelisp` 的 problem matcher。`typl` 的診斷格式是
`error: FILE:LINE:COL: message`，因此可以直接顯示在「問題」面板中：

```jsonc
{
  "version": "2.0.0",
  "tasks": [
    {
      "label": "typl: run",
      "type": "shell",
      "command": "typl ${file}",
      "problemMatcher": "$typelisp"
    }
  ]
}
```

## 開發

```sh
npm run compile   # tsc
npm run watch     # 監看建置
npm test          # node --test（語法、縮排、符號、型別參照、資訊清單）
```

測試只涵蓋不需要 `vscode` 模組的部分。為此，`src/indent.ts` 與 `src/symbols.ts` 寫成純函式，
只有 `src/extension.ts` 接觸編輯器 API。

- `src/test/grammar.test.ts` —— 以與 VS Code 相同的引擎（`vscode-textmate` +
  `vscode-oniguruma`）實際對語法進行斷詞並檢查。
  Oniguruma 與 Emacs 正規表示式在細節上不同（例如不把字元類別開頭的 `]` 當作字面值），
  這種差異只有實際執行引擎才能發現。
- `src/test/indent.test.ts` —— 對 `examples/` 下的所有 `.typl` 檔案，
  要求**把縮排全部去掉後還原的結果與已提交的內容逐位元組一致**。
  Emacs 模式在同一批檔案上也滿足同樣的標準，這使「兩個編輯器一致」成為經過驗證的說法。
  此外，`src/test/fixtures/emacs-indent-reference.txt` 是在 Emacs 的 `typelisp-mode` 緩衝區中
  實際執行 `indent-region` 採集的參考輸出。預期值不是照抄 TS 實作，而是
  **另一個編輯器實際產生的結果**，因此移植的忠實度可以直接得到檢驗
  （包含 `let*` `do` `doiter` `labels` `impl` `pprint-logical-block`、quote 前綴等）。
- `src/test/symbols.test.ts` —— Outline 的內容以及備援方式對型別參照的偵測。定義數必須與
  計算行首定義形式的另一種獨立方法完全一致。型別參照的邊界規則刻意與 Emacs 版的備援方式對齊
  （VS Code 用 lookbehind，Emacs 用消耗一個前導字元的形式表達同一集合）。
- 伺服器端由解析驅動的 token（`crates/typelisp-front/src/check/semantic.rs`）由
  `cargo test --test lsp_semantic_test` 與 `scripts/lsp-semantic-smoke.py`
  （透過 stdio 驅動真實行程）檢查。Emacs 端的用戶端由
  `scripts/emacs-semantic-smoke.el` 透過真實的 eglot 連線檢查。
- `src/test/manifest.test.ts` —— `package.json` 是編譯器不檢查的唯一部分，因此這裡檢查：
  已宣告的命令與 `registerCommand` 的集合一致、按鍵綁定的參照對象、程式碼讀取的設定是否已宣告、
  problem matcher 能否解析 `typl` 實際的輸出。

### 偵測編輯器定義的偏差

關鍵字表在 Emacs 版與 VS Code 版中雙重維護。為防止實作前進了而編輯器定義卻落後，
Rust 一側有測試：

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

## 備註

- typelisp 在讀取時會把符號轉為小寫，但高亮區分大小寫，以便區分大寫字母開頭的型別名稱。
- 縮排由 `src/indent.ts` 的 `INDENT_SPECS` 決定。它移植自 Emacs 版的
  `typelisp-indent-specs`，值和規則都相同。`(defun NAME (PARAMS) RETTYPE ...)` 的標頭有 3 個元素、
  `if` 固定為必須有 `else` 的 3 個元素等，與 Emacs Lisp 同名但形狀不同的地方都照樣反映出來。
- `#| ... |#` 的內容在格式化時會重新縮排。這與 Emacs 的 `indent-region` 行為一致。
