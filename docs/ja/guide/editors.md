# エディタ連携（typl-lsp）

`typl-lsp` は typelisp の言語サーバです。LSP（Language Server Protocol）に対応したエディタに
つなぐと、編集中のファイルについて次の機能が使えます。

- 診断：読み取りエラー・型エラー・再定義の警告
- hover：括弧で書かれた式の型と、呼んでいる定義の docstring（裸の変数名には出ません）
- 定義へのジャンプ
- 補完（`:` を打つと候補が出ます）
- 型名の色分け（semantic tokens）：他のファイルから `use` した型にも色が付きます

`use` によるファイルをまたぐ参照も解決されます。他のファイルを開いて保存前に編集した内容も、
そのファイルを `use` しているファイルの診断にすぐ反映されます。

## 1. ビルド

```sh
cargo build --release --bin typl-lsp
```

`target/release/typl-lsp` ができます。[README_JP.md](../../../README_JP.md) の手順で
`cargo install` した場合は、`typl` と一緒に `~/.cargo/bin/typl-lsp` に入っています。

## 2. VS Code

リポジトリの `editor/vscode` に拡張があります。Marketplace には出していないので、ビルドして
入れます。

```sh
cd editor/vscode
npm install
npm run compile
npx @vscode/vsce package      # .vsix ができる
```

拡張ビューの「…」メニューから「Install from VSIX...」を選び、できた `.vsix` を指定します。

拡張はワークスペースの `target/release/typl-lsp`、`target/debug/typl-lsp`、`PATH` の順に
`typl-lsp` を探します。別の場所に置いた場合は、設定 `typelisp.languageServer.path` に
パスを書いてください。

| 設定 | 既定 | 内容 |
|---|---|---|
| `typelisp.program` | `typl` | `typl` のパス |
| `typelisp.languageServer.enable` | `true` | `typl-lsp` に接続するか |
| `typelisp.languageServer.path` | （空） | `typl-lsp` のパス |

`Ctrl+Alt+R` で編集中のファイルを保存して `typl` で実行し、`Ctrl+Alt+Z` で REPL を起動します。
詳しくは [VS Code 拡張の README](../../../editor/vscode/README_JP.md) を参照してください。

## 3. Emacs

リポジトリの `editor/emacs` に `typelisp-mode` があります。

```elisp
(add-to-list 'load-path "/path/to/typelisp/editor/emacs")
(require 'typelisp-mode)
```

`eglot`（Emacs 29 以降に同梱）で `typl-lsp` につなぐ設定:

```elisp
(with-eval-after-load 'eglot
  (add-to-list 'eglot-server-programs
               '(typelisp-mode . ("/path/to/typelisp/target/release/typl-lsp"))))
(add-hook 'typelisp-mode-hook #'eglot-ensure)
```

`lsp-mode` を使う場合:

```elisp
(with-eval-after-load 'lsp-mode
  (lsp-register-client
   (make-lsp-client
    :new-connection (lsp-stdio-connection "/path/to/typelisp/target/release/typl-lsp")
    :major-modes '(typelisp-mode)
    :server-id 'typl-lsp)))
(add-hook 'typelisp-mode-hook #'lsp)
```

Emacs 31 以降の eglot は、型名の色分け（semantic tokens）を自分で行います。Emacs 30 以前の
eglot はこれに対応していないので、`typelisp-mode` が代わりに型名を色分けします。`lsp-mode` では
`lsp-semantic-tokens-enable` を `t` にしてください。

`C-c C-c` で編集中のファイルを実行し、`C-c C-z` で REPL を起動します。詳しくは
[typelisp-mode の README](../../../editor/emacs/README_JP.md) を参照してください。

## 4. その他のエディタ

`typl-lsp` は標準入出力で LSP を話し、コマンドライン引数を取りません。エディタの LSP クライアントに、
`.typl` ファイルに対して `typl-lsp` を起動するよう設定してください。

## 5. プロジェクトの認識

`typl-lsp` は、開いたファイルのディレクトリから上へ向かって `typelisp.toml` を探し、そこを
ソースルートとして `use` を解決します。`typl` でファイルを実行するときと同じ規則です
（[モジュールとファイル構成](modules.md#2-プロジェクトの作り方)）。複数のファイルからなる
プロジェクトでは、ルートに `typelisp.toml` を置いてください。

## 6. 言語サーバはプログラムを実行しない

`typl-lsp` は、読み取りと型検査だけで診断を出します。編集中のプログラムを実行することは
ありません。キー入力のたびに診断が走るので、そこで副作用のあるコードや終わらないコードを
走らせるわけにはいかないためです。例外は `defmacro` の登録だけで、これは後のマクロ呼び出しを
検査するために必要です。

そのため、`typl` で実行したときにだけ起きるエラー（`panic`、ファイルが無いなど）は、
言語サーバの診断には出ません。
