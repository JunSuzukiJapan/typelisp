# typelisp-mode (Emacs)

typelisp ソース（`.typl`）を編集するための Emacs メジャーモード。

## 機能

- シンタックスハイライト
  - 特殊形 / 制御構文（`defun` `let` `if` `match` `loop` `lambda` `setf` `as` `apply`、
    `print`/`println`/`format`、`pprint` 系 など）
  - 定義名の強調（`(defun NAME ...)` の `NAME` を関数名、`(defstruct NAME ...)` を型名、
    `(defvar (NAME ...))` を変数名として。`(pub defun NAME ...)` の `pub` 付きも同様）
  - 名前空間・宣言キーワード（`pub` `module` `use` `load` `impl` `where`）と
    ラムダリスト標識（`&rest` `&optional` `&key`）
  - 組み込み関数（`car` `map` `foldl` `unwrap` `parse-int` `message` `sexpr-car` など）
  - プリミティブ型（`bignum` / `ratio` を含む）・組み込み型・組み込みエラー型
    （`ParseIntError` など）・`Capitalized` なユーザ型・trait オブジェクト型 `:dyn Trait`
  - リテラル（`true` `false`、数値リテラル（10進 / `0xff` / `1.5` / `1/3`）、
    文字リテラル `#\Space`、文字列、キーワード `:name`）
  - 文字列中の `format` 制御ディレクティブ（`~a` `~5,'0d` `~{...~}` など）
  - CL 流の earmuff 付きグローバル（`*print-pretty*` など）
- コメント
  - 行コメント `;`
  - **ネスト可能な**ブロックコメント `#| ... |#`
- S 式ナビゲーションと Lisp 流インデント
- `imenu` による定義一覧（関数 / メソッド / マクロ / 型 / トレイト / `impl` / 変数 / モジュール）
- `typl` CLI の呼び出し（下記）

## キーバインド

| キー | コマンド | 内容 |
|---|---|---|
| `C-c C-c` | `typelisp-run-buffer` | 保存して `typl FILE` で実行（`compile` 経由。エラー行へジャンプ可） |
| `C-c C-k` | `typelisp-compile-module` | `typl compile-module FILE` で fasl へプリコンパイル |
| `C-c C-z` | `typelisp-repl` | `typl` の REPL を comint バッファで起動 |

`typl` の場所は `typelisp-program`（既定 `"typl"`）で指定する。
診断は `error: FILE:LINE:COL: ...` 形式なので `compilation-mode` が解析でき、
`next-error` / `C-x \`` でそのまま該当箇所へ飛べる。

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

`typl-lsp` をビルドすれば `eglot`（Emacs 29+ 標準）や `lsp-mode` から利用できる。

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

診断（構文/型エラーと再定義 warning を `textDocument/publishDiagnostics` で通知）・hover・
定義ジャンプ（goto-definition）・補完に対応（補完は `:` をトリガ文字に登録済み）。`use` に
よるファイルをまたぐ参照は解決される（プロジェクトルートの `typelisp.toml` を上方探索、
詳細は `docs/syntax.md` の「ファイル↔モジュール対応」節）。開いているエディタバッファの
未保存編集は依存ファイル・依存元双方の診断に即座に反映される。

## 備考

- typelisp はシンボルを読み取り時に小文字化するが、ハイライトは大文字始まりの型名を
  区別するためケースセンシティブ。
- インデントは専用の `typelisp-indent-function` が `typelisp-indent-specs`（連想リスト）
  を引いて決める。Emacs Lisp と名前を共有する形（`defun` `let` `if` …）も自前で持って
  いるのは、シンボルプロパティが**グローバル**で、typelisp 用の設定が同じセッションの
  他の Lisp バッファのインデントを変えてしまうため。また typelisp の形は Emacs Lisp と
  同名でも形状が違う——`(defun NAME (PARAMS) RETTYPE ...)` はヘッダ要素が3つ、`if` は
  `else` 必須の3要素固定——ので、値も共有できない。
  `examples/` 配下の 22 ファイルすべてが `indent-region` で1バイトも変化しないことを
  確認済み。
