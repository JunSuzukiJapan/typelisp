# typelisp-mode (Emacs)

typelisp ソース（`.typl`）を編集するための Emacs メジャーモード。
VS Code 版は [../vscode/](../vscode/README_JP.md)。両者は同じキーワード表・同じインデント規則を
持ち、そのことは `cargo test --test editor_keyword_sync_test` で機械的に検証されている（末尾参照）。

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
  - **ユーザ定義型の使用箇所**（`defstruct`/`defenum`/`deftrait` の名前は通常小文字
    （`rect` `todo-item` `board`）で `Capitalized` 規則では拾えない）。
    `typl-lsp` に接続していればサーバの semantic tokens で着色する（`eglot` でも効く。
    後述）。未接続時はバッファ内の型名を集めるフォールバックに切り替わる
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
定義ジャンプ（goto-definition）・補完（`:` をトリガ文字に登録済み）・semantic tokens に対応。
`use` によるファイルをまたぐ参照は解決される（プロジェクトルートの `typelisp.toml` を上方探索、
詳細は [構文リファレンス 3.11](../../docs/ja/reference/syntax.md#311-ファイルとモジュールの対応複数ファイルのプロジェクト)）。開いているエディタバッファの
未保存編集は依存ファイル・依存元双方の診断に即座に反映される。

### 型名のハイライト（semantic tokens）

サーバは `textDocument/semanticTokens` で、**チェッカが実際に型名として解決した位置**を
報告する。テキスト照合ではないので、

- `use` 経由で他ファイルから来た型も色が付く（バッファ内解決では原理的に届かない範囲）
- 型と同名の**関数**の呼び出し箇所は色が付かない（そこはチェッカが関数として解決したので、
  そもそもトークンが記録されない）

クライアント側:

- **`eglot`（Emacs 31 以降）**: eglot が自前で描画する（`eglot-semantic-tokens-mode`）。
  `typelisp-mode` 側は手を出さない
- **`eglot`（Emacs 30 以前）**: この版の eglot は semanticTokens を扱わない。そこで
  **`typelisp-mode` が自前でリクエストを投げてオーバレイで描画する**
  （`typelisp-semantic-tokens-mode`。eglot 接続時に自動で有効化）
- **`lsp-mode`**: native 対応（`lsp-semantic-tokens-enable` を `t` に）。この場合
  `typelisp-mode` 側は手を出さない

`scripts/emacs-semantic-smoke.el` は実際に eglot で接続し、使っている Emacs で描画を受け持つ
側を検証する。いずれのクライアントでもサーバが答えている間はバッファ内解決のフォールバックは退く
（同じバッファを2つの規則が塗らないようにするため）。

| 設定 | 既定 | 内容 |
|---|---|---|
| `typelisp-semantic-tokens` | `t` | Emacs 30 以前の eglot 利用時に、サーバの semantic tokens で着色するか |
| `typelisp-semantic-tokens-idle-delay` | `0.6` | 編集後に再リクエストするまでのアイドル秒数（`eglot-send-changes-idle-time` より大きくすること） |

## 備考

- typelisp はシンボルを読み取り時に小文字化するが、ハイライトは大文字始まりの型名を
  区別するためケースセンシティブ。
- インデントは専用の `typelisp-indent-function` が `typelisp-indent-specs`（連想リスト）
  を引いて決める。Emacs Lisp と名前を共有する形（`defun` `let` `if` …）も自前で持って
  いるのは、シンボルプロパティが**グローバル**で、typelisp 用の設定が同じセッションの
  他の Lisp バッファのインデントを変えてしまうため。また typelisp の形は Emacs Lisp と
  同名でも形状が違う——`(defun NAME (PARAMS) RETTYPE ...)` はヘッダ要素が3つ、`if` は
  `else` 必須の3要素固定——ので、値も共有できない。
  `examples/` 配下のすべての `.typl` ファイルが、`indent-region` で1バイトも変化しないことと、
  インデントを全部潰してから再インデントすると元に戻ることを確認済み（VS Code 版も同じ
  ファイル群で同じ基準を満たしている）。

## エディタ定義のドリフト検出

キーワード表は VS Code 版と二重管理になる。実装が進んだのにエディタ定義だけ古くなる事故を
防ぐため、Rust 側にテストがある:

```sh
cargo test --test editor_keyword_sync_test
```

prelude を実際にロードしてレジストリを走査し、**どちらかのエディタが知らない名前**を報告する。
特殊形は実行時表現を持たないので、`crates/typelisp-front/src/check/checker.rs` の
`// SPECIAL-FORM DISPATCH BEGIN` / `END` の間から読み出す（このコメントは消さないこと）。
失敗したら、報告された名前を**両方**のエディタ定義に追加する。

同じテストが semantic tokens の legend も照合する（`src/bin/lsp.rs` の
`SEMANTIC_TOKEN_TYPES` と、両エディタが持つ対応表が名前・順序ともに一致すること）。
ずれても実行時エラーにはならず全トークンの色が入れ替わるだけなので、機械的に固定してある。
