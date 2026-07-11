# typelisp-mode (Emacs)

typelisp ソース（`.typl`）を編集するための Emacs メジャーモード。

## 機能

- シンタックスハイライト
  - 特殊形 / 制御構文（`defun` `let` `if` `match` `loop` `lambda` `setf` など）
  - 定義名の強調（`(defun NAME ...)` の `NAME` を関数名、`(defvar (NAME ...))` を変数名として）
  - 名前空間・宣言キーワード（`pub` `module` `use` `impl` `where`）
  - 組み込み関数（`car` `map` `foldl` `unwrap` など）
  - プリミティブ型・組み込み型・`Capitalized` なユーザ型
  - リテラル（`true` `false`、文字リテラル `#\Space`、文字列）
- コメント
  - 行コメント `;`
  - **ネスト可能な**ブロックコメント `#| ... |#`
- S 式ナビゲーションと Lisp 流インデント（`lisp-mode` の仕組みを利用）

## インストール

```elisp
(add-to-list 'load-path "/path/to/typelisp/editor/emacs")
(require 'typelisp-mode)
```

`.typl` ファイルは自動的に `typelisp-mode` で開かれる（`auto-mode-alist` に登録済み）。

`use-package` を使う場合:

```elisp
(use-package typelisp-mode
  :load-path "/path/to/typelisp/editor/emacs"
  :mode "\\.typl\\'")
```

## Language Server (`typl-lsp`)

`typl-lsp`（診断のみ: 構文/型エラーと再定義warningを `textDocument/publishDiagnostics`
で通知）をビルドすれば `eglot`（Emacs 29+ 標準）や `lsp-mode` から利用できる。

```sh
cargo build --release --bin typl-lsp
```

`eglot` の場合:

```elisp
(with-eval-after-load 'eglot
  (add-to-list 'eglot-server-programs
               '(typelisp-mode . ("/path/to/typelisp/target/release/typl-lsp"))))
(add-hook 'typelisp-mode-hook #'eglot-ensure)
```

`lsp-mode` の場合:

```elisp
(with-eval-after-load 'lsp-mode
  (lsp-register-client
   (make-lsp-client
    :new-connection (lsp-stdio-connection "/path/to/typelisp/target/release/typl-lsp")
    :major-modes '(typelisp-mode)
    :server-id 'typl-lsp)))
(add-hook 'typelisp-mode-hook #'lsp)
```

診断のみで、hover・補完・定義ジャンプは未対応。`use` によるファイルをまたぐ参照は解決される
（プロジェクトルートの `typelisp.toml` を上方探索、詳細は `docs/syntax.md` の
「ファイル↔モジュール対応」節）。依存ファイルはディスク上の内容で読まれるため、
未保存の編集は診断に反映されない。

## 備考

- typelisp はシンボルを読み取り時に小文字化するが、ハイライトは大文字始まりの型名を
  区別するためケースセンシティブ。
- インデント設定は typelisp 固有の定義形（`defmethod` `defstruct` `defenum` `deftrait`
  `impl` `match` など）にのみプロパティを付与し、Emacs Lisp と共通の形は `lisp-mode` の
  既定値をそのまま利用する（他の Lisp バッファに影響しない）。
