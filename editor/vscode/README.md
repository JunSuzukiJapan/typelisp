# typelisp (VS Code)

typelisp ソース（`.typl`）を編集するための VS Code 拡張。
Emacs 版は [../emacs/](../emacs/README.md)。両者は同じキーワード表・同じインデント規則を
持ち、そのことは `cargo test --test editor_keyword_sync_test` で機械的に検証されている（後述）。

## 機能

- **シンタックスハイライト**（TextMate 文法、言語サーバ不要）
  - 特殊形 / 制御構文（`defun` `let` `if` `match` `loop` `lambda` `setf` `as` `apply`、
    `print`/`println`/`format`、`pprint` 系）
  - 定義名（`(defun NAME ...)` は関数、`(defstruct NAME ...)` は型、`(defvar (NAME ...))` は変数。
    `(pub defun NAME ...)` の `pub` 付きも同様）と `(impl Trait Type)` の両名
  - 名前空間・宣言キーワード（`pub` `module` `use` `load` `impl` `where`）、
    ラムダリスト標識（`&rest` `&optional` `&key`）
  - 組み込み関数・プリミティブ型（`bignum` / `ratio` 含む）・組み込みエラー型・
    `Capitalized` なユーザ型・trait オブジェクト型 `:dyn Trait`（ジェネリック引数の内側も）
  - 数値リテラル（10進 / `0xff` / `1.5` / `3.0e10` / `1/3`）、文字リテラル `#\Space`、
    キーワード `:name`、earmuff 付きグローバル `*print-pretty*`
  - **文字列中の `format` 制御ディレクティブ**（`~a` `~5,'0d` `~{...~}` `~^` など）
  - 行コメント `;` と**ネスト可能な**ブロックコメント `#| ... |#`
- **ユーザ定義型の使用箇所**（semantic tokens）
  - `defstruct` / `defenum` / `deftrait` の名前は通常小文字（`rect` `todo-item` `board`）なので
    `Capitalized` 規則では拾えず、TextMate 文法は行単位でファイル全体を見られない。
    semantic tokens なら見られるので、静的型付き言語なのに型注釈だけ色が付かない状態を解消した
  - `typl-lsp` に接続していれば**チェッカが実際に型名として解決した位置**が返る。したがって
    `use` で他ファイルから来た型も色が付き、型と同名の**関数**の呼び出し箇所には色が付かない
    （そこはチェッカが関数として解決したので、そもそもトークンが記録されない）
  - 未接続・未ビルドのときは拡張がファイル内に閉じて解決するテキスト走査のフォールバックに
    切り替わる。こちらは越境した型を拾えず、型と同名の関数も区別できない近似
- **Lisp インデント**（VS Code は Lisp のインデントを標準で持たないので拡張側で実装）
  - ドキュメント整形・選択範囲整形・入力時整形（Enter と `)`、`editor.formatOnType` 有効時）
- **Outline / breadcrumbs / `Ctrl+Shift+O`**（関数・メソッド・マクロ・型・トレイト・
  `impl`・変数・モジュール）
- **`typl-lsp` 連携**（診断・hover・定義ジャンプ・補完・semantic tokens）
- **`typl` CLI コマンド**（実行・REPL）

言語サーバ以外はすべて拡張単体で動くので、`typl-lsp` をビルドしていないチェックアウトでも
ハイライト・インデント・Outline・（ファイル内に閉じた）型ハイライトは使える。

## インストール

Marketplace には出していないので、ローカルにビルドして入れる。

```sh
cd editor/vscode
npm install
npm run compile
```

そのうえで、いずれか:

- **開発ホストで試す**: `editor/vscode` を VS Code で開いて `F5`
- **恒久的に入れる**: `npx @vscode/vsce package` で `.vsix` を作り、
  拡張ビューの「…」→「Install from VSIX...」

`.typl` ファイルが自動的に typelisp モードで開かれる。

## キーバインド

| キー | コマンド | 内容 |
|---|---|---|
| `Ctrl+Alt+R` | `typelisp.runFile` | 保存して `typl FILE` で実行 |
| `Ctrl+Alt+Z` | `typelisp.repl` | `typl` の REPL を起動 |

コマンドパレットには `typelisp: Restart Language Server` もある。

## 設定

| 設定 | 既定 | 内容 |
|---|---|---|
| `typelisp.program` | `typl` | `typl` CLI のパス |
| `typelisp.languageServer.enable` | `true` | `typl-lsp` に接続するか |
| `typelisp.languageServer.path` | （空） | `typl-lsp` のパス。空ならワークスペースの `target/release/typl-lsp` → `target/debug/typl-lsp` → `PATH` の順に探す |
| `typelisp.trace.server` | `off` | LSP の JSON-RPC を記録 |

言語サーバは次でビルドする:

```sh
cargo build --release --bin typl-lsp
```

`use` によるファイルをまたぐ参照はプロジェクトルートの `typelisp.toml` を上方探索して解決される
（詳細は [docs/ja/reference/syntax.md](../../docs/ja/reference/syntax.md#311-ファイルとモジュールの対応複数ファイルのプロジェクト) の「ファイルとモジュールの対応」節）。

## タスクの problem matcher

`typelisp` という problem matcher を提供している。`typl` の診断は
`error: FILE:LINE:COL: message` 形式なので、そのまま Problems パネルに出せる:

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

## 開発

```sh
npm run compile   # tsc
npm run watch     # 監視ビルド
npm test          # node --test（文法・インデント・シンボル・型参照・マニフェスト、46件）
```

テストは `vscode` モジュールを必要としない部分だけを対象にしている。そのために
`src/indent.ts` と `src/symbols.ts` は純粋な関数として書かれ、`src/extension.ts` だけが
エディタ API と接する。

- `src/test/grammar.test.ts` — VS Code と同じエンジン（`vscode-textmate` +
  `vscode-oniguruma`）で文法を実際にトークン化して検証する。
  Oniguruma は Emacs 正規表現と細部が違う（例: 文字クラス先頭の `]` をリテラル扱いしない）ので、
  この差は実エンジンで踏まないと見つからない。
- `src/test/indent.test.ts` — `examples/` の全 `.typl` ファイルについて、
  **インデントを全部潰してから復元し、コミット済みの内容とバイト単位で一致すること**を要求する。
  Emacs モードも同じ 22 ファイルで同じ基準を満たしており、これが「2つのエディタが一致する」を
  検証済みの主張にしている。
  加えて `src/test/fixtures/emacs-indent-reference.txt` は、Emacs の `typelisp-mode` バッファで
  `indent-region` を実際に走らせて採取した15ケースの参照出力。期待値が TS 実装の追認ではなく
  **もう一方のエディタが実際に出す結果**なので、移植の忠実さがそのまま検証される
  （`let*` `do` `doiter` `labels` `impl` `pprint-logical-block` quote 接頭辞などを含む）。
- `src/test/symbols.test.ts` — Outline の内容と、フォールバックの型参照検出。定義数は行頭の
  定義形を数える独立した方法と完全一致することを要求する。型参照は `examples/` 全22ファイルで
  Emacs 版のフォールバックと**同一の97箇所**を返すことを確認済み（両者の境界規則を意図的に
  揃えてある。VS Code は lookbehind、Emacs は先行文字を1つ消費する形で同じ集合を表現）。
- サーバ側の解決駆動トークン (`src/check/semantic.rs`) は
  `cargo test --test lsp_semantic_test` と `scripts/lsp-semantic-smoke.py`
  （実プロセスを stdio で駆動）が検証している。Emacs 側クライアントは
  `scripts/emacs-semantic-smoke.el` が実 eglot 接続で検証する。
- `src/test/manifest.test.ts` — `package.json` はコンパイラが検査しない唯一の部分なので、
  宣言済みコマンドと `registerCommand` の集合一致、キーバインドの参照先、コードが読む設定が
  宣言されているか、problem matcher が `typl` の実際の出力を解析できるかを検査する。

### エディタ定義のドリフト検出

キーワード表は Emacs 版と VS Code 版で二重管理になる。実装が進んだのにエディタ定義だけ
古くなる事故（実際に一度起きた）を防ぐため、Rust 側にテストがある:

```sh
cargo test --test editor_keyword_sync_test
```

prelude を実際にロードしてレジストリを走査し、**どちらかのエディタが知らない名前**を報告する。
特殊形は実行時表現を持たないので、`src/check/checker.rs` の
`// SPECIAL-FORM DISPATCH BEGIN` / `END` の間から読み出す（このコメントは消さないこと）。
失敗したら、報告された名前を**両方**のエディタ定義に追加する。

同じテストが semantic tokens の legend も照合する（`src/bin/lsp.rs` の
`SEMANTIC_TOKEN_TYPES` と、両エディタが持つ対応表が名前・順序ともに一致すること）。
ずれても実行時エラーにはならず全トークンの色が入れ替わるだけなので、機械的に固定してある。

## 備考

- typelisp はシンボルを読み取り時に小文字化するが、ハイライトは大文字始まりの型名を
  区別するためケースセンシティブ。
- インデントは `src/indent.ts` の `INDENT_SPECS` が決める。Emacs 版の
  `typelisp-indent-specs` の移植で、値も規則も同一。`(defun NAME (PARAMS) RETTYPE ...)` の
  ヘッダが3要素であること、`if` が `else` 必須の3要素固定であることなど、
  Emacs Lisp と同名でも形が違う点がそのまま反映されている。
- `#| ... |#` の中身は整形時に再インデントされる。Emacs の `indent-region` と同じ挙動に
  揃えてある。
