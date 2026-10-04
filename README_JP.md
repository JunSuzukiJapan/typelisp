# TypeLisp

TypeLisp は静的型付きの Lisp です。
文法などは主にCommon Lispを参考にしています。

## 特徴

### 関数定義

関数を定義するときには、引数の型と戻り値の型を指定します。
具体的には、以下のようになります。

```
(defun fun-name ((arg1 type1) (arg2 type2) ...) return-type
    body ...)
```

### メソッド

```
(defstruct Point (x i32) (y i32))

(defmethod show ((p Point)) ()
  (println "Point: (x ~d y ~d)" p::x p::y))

(show (Point::new 1 2))
```

## 例

### factorial

```
(defun factorial ((n int)) int
  (if (<= n 1)
    1
    (* n (factorial (- n 1))) ))

(let ((num (factorial 10)))
  (println "10! = ~d" num) )
```

### wc

```
(defun is-whitespace ((b int)) bool
  (case b
    ((#x20 #x09 #x0a #x0b #x0c #x0d) true)
    (else false)))

(defun main () ()
  (let ((args (command-line-args)))
    (when (< (len args) 2)
      (println "usage: wc file")
      (exit 0))

    (let ((path (get args 1))
          (char-count 0)
          (word-count 0)
          (line-count 0)
          (in-word false))
      (match (open-binary-input path)
        ((ok file)
         (progn
           (loop (match (read-byte file)
                   ((some b)
                    (progn
                      (incf char-count)
                      (when (= b #x0a)
                        (incf line-count))
                      (if in-word
                        (when (is-whitespace b)
                          (setf in-word false))
                        (unless (is-whitespace b)
                          (setf in-word true)
                          (incf word-count)))))
                   ((none) (break))))
           (close file)
           (println "~10d ~10d ~10d ~a" line-count word-count char-count path)))
        ((err e)
         (progn
           (println "wc: ~a" (message e))
           (exit 1)))))))

(main)
```

### Trait

```
(deftrait Animal ()
  (name ((self Self)) string)
  (sound ((self Self)) string)
  (speak ((self Self)) string
    (format false "~a says ~a" (name self) (sound self))))

(defstruct Dog (nick string))
(defstruct Cat (nick string))

(impl Animal Dog
  (name ((self Self)) string self::nick)
  (sound ((self Self)) string "Woof"))

(impl Animal Cat
  (name ((self Self)) string self::nick)
  (sound ((self Self)) string "Meow"))

(defun main () ()
  (let ((animals (the Vector<:dyn Animal> (Vector::new))))
    (push animals (Dog::new "Pochi"))
    (push animals (Cat::new "Tama"))
    (doiter (a (iter animals))
      (println "~a" (speak a)))))

(main)
```

### そのほかの例

[examples/](examples/) にあるプログラムは、ファイル名を指定して実行できます。上の wc と Trait の例も
[examples/wc.typl](examples/wc.typl) と [examples/animals.typl](examples/animals.typl) にあります。

```sh
typl examples/fizzbuzz.typl
typl examples/wc.typl README_JP.md
```

| ファイル | 内容 |
|---|---|
| [bst.typl](examples/bst.typl) | 二分探索木（`defenum` と `match`） |
| [factorial.typl](examples/factorial.typl) | 階乗（63 ビットを超えると自動で多倍長になる） |
| [fibonacci.typl](examples/fibonacci.typl) | フィボナッチ数列（ループ） |
| [fizzbuzz.typl](examples/fizzbuzz.typl) | FizzBuzz |
| [game_of_life.typl](examples/game_of_life.typl) | ライフゲーム |
| [maze_bfs.typl](examples/maze_bfs.typl) | 迷路の最短経路（幅優先探索） |
| [primes.typl](examples/primes.typl) | エラトステネスの篩 |

[examples/projects/](examples/projects/) には、複数のファイルに分かれたプログラムがあります。
`src/main.typl` を指定して実行します（例: `typl examples/projects/todo-cli/src/main.typl`）。

| プロジェクト | 内容 |
|---|---|
| [echo-server](examples/projects/echo-server/) | 接続ごとにタスクを起動するエコーサーバ |
| [expr-eval](examples/projects/expr-eval/) | 四則演算の対話計算機（字句解析、構文解析、評価） |
| [http](examples/projects/http/) | HTTP/1.1 のサーバとクライアント（TLS 対応） |
| [mini-lisp](examples/projects/mini-lisp/) | 小さな Lisp の REPL |
| [shape-canvas](examples/projects/shape-canvas/) | 文字のキャンバスに図形を描く（`:dyn` とエラー型） |
| [todo-cli](examples/projects/todo-cli/) | ToDo を管理するコマンドラインツール |

## インストール

macOS（Intel / Apple Silicon）では、ビルド済みの `typl` と `typl-lsp` を次のコマンドで
インストールできます。

```sh
curl -fsSL https://raw.githubusercontent.com/JunSuzukiJapan/typelisp/main/install.sh | sh
```

`~/.typelisp/bin` に入るので、表示に従って `PATH` に追加してください。`typl -c` で実行ファイルを
作るには Xcode Command Line Tools（`xcode-select --install`）が必要です。ビルド済みの `typl` は
Apple Silicon では macOS 26 以降、Intel では macOS 15 以降で動きます。Apple Silicon で macOS 26 より
前の macOS では、起動時に警告が 1 行出ます。そのまま使えますが、`typl` が実行中にコンパイルした
コードの中で `panic`・`throw`・`unwind-protect` を使うと、異常終了することがあります。

インストール先は環境変数 `TYPELISP_HOME` で、バージョンは `TYPELISP_VERSION` で指定できます
（指定しなければ最新のリリース）。

```sh
curl -fsSL https://raw.githubusercontent.com/JunSuzukiJapan/typelisp/main/install.sh | TYPELISP_VERSION=0.1.1 sh
```

アンインストールするときは、`typl` が書き出したライブラリを削除してから、インストール先を
削除します。

```sh
typl --remove-lib --all
rm -rf ~/.typelisp
```

ソースからビルドする場合は、次の「ビルド」を参照してください。

## ビルド

### 必要なもの

- Rust（cargo）
- LLVM 22
- macOS では Xcode Command Line Tools（リンクに使います）

LLVM 22 は Homebrew で入れられます。

```sh
brew install llvm@22
```

### 初回の設定

クローンした後に一度だけ、次のスクリプトを実行します。

```sh
scripts/setup-cargo-env.sh
```

このスクリプトは `brew --prefix llvm@22` で LLVM 22 の場所を調べ、`.cargo/config.toml` を生成します。
このファイルはマシンごとに内容が違うので、Git の管理対象外です。macOS では、ビルドに使う最低 OS
バージョン（`MACOSX_DEPLOYMENT_TARGET`）も書き込みます。Rust のツールチェーンを更新したときは、
もう一度実行してください。

スクリプトが最低 OS バージョンを読み取れなかった場合は、その旨を表示して止まります。そのときは
値を自分で指定します。

```sh
scripts/setup-cargo-env.sh --deployment-target 15.0
```

設定ファイルを生成したくない場合は、cargo のコマンドを `scripts/with-llvm-env.sh` 経由で実行します。
こちらは実行のたびに同じ値を環境変数に設定します。

```sh
scripts/with-llvm-env.sh cargo build
```

Homebrew を使わない場合は、`LLVM_SYS_221_PREFIX` に LLVM 22 のインストール先を設定してください。
macOS の最低 OS バージョン（`MACOSX_DEPLOYMENT_TARGET`）は、設定しなくてもビルドできます。設定
しなければ、Rust の標準ライブラリが対象とするバージョンを使います。

### ビルドする

```sh
cargo build            # デバッグビルド（target/debug/）
cargo build --release  # リリースビルド（target/release/）
```

次の2つの実行ファイルができます。

| 実行ファイル | 役割 |
|---|---|
| `typl` | 処理系本体（使い方は次の「実行方法」） |
| `typl-lsp` | 言語サーバ（エディタ連携用） |

### cargo install でインストールする

`typl` と `typl-lsp` を `~/.cargo/bin` に入れるには、クローンしたリポジトリで次を実行します。

```sh
cargo install --locked --path .
```

`cargo install --path` は、「初回の設定」で生成した `.cargo/config.toml` を読みます。設定ファイルを
生成していない場合は、`scripts/with-llvm-env.sh cargo install --locked --path .` を実行します。
Homebrew を使わない場合は、`LLVM_SYS_221_PREFIX` を設定してから `cargo install --locked --path .` を
実行します。

インストールした `typl` は、コンパイルに使う静的ライブラリを中に持っているので、インストール後に
リポジトリを移動・削除しても `-c` や `compile-file` が使えます（次の「ファイルをコンパイルする」を
参照）。

## 実行方法

`typl` の起動のしかたは次の3通りです。以下の例では、`typl` にパスが通っているものとします
（`install.sh` や `cargo install` で入れて `PATH` を通した場合、またはビルドした
`target/debug/`・`target/release/` にパスを通した場合）。

### インタラクティブモード（REPL）

引数を付けずに起動すると、インタラクティブモード（REPL）になります。式を1つ入力するたびに、
その場で評価して結果を表示します。`:quit` または `:exit` で終了します。

```sh
typl
```

### ファイルを実行する

ファイル名だけを指定すると、そのファイルを実行します。ファイル名より後ろに書いた引数は、
プログラムに渡されます（`(command-line-args)` で受け取れます）。

```sh
typl foo.typl
typl foo.typl a b c
```

### ファイルをコンパイルする

`-c` または `--compile` を指定すると、そのファイルをコンパイルして実行ファイルを作ります。
`-o` を省くと、ファイル名から `.typl` を除いた名前の実行ファイル（この例では `foo`）ができます。

```sh
typl -c foo.typl
typl --compile foo.typl -o bar
```

コンパイルして作る実行ファイルには、静的ライブラリ `libtypelisp_front.a` をリンクします。
リリースビルドの `typl`（`cargo install` で入れたものを含む）は、このライブラリを中に持って
いて、初めてコンパイルするときに `~/.typelisp/lib/<ビルドID>/` に書き出します。2回目からは
書き出したものを使います。書き出す場所は環境変数 `TYPELISP_HOME` で変えられます（`TYPELISP_HOME`
を設定すると `$TYPELISP_HOME/lib/<ビルドID>/` になります）。ビルドIDはライブラリの中身から
決まるので、別の版の `typl` を入れると、次のコンパイルで新しいディレクトリに書き出されます。

書き出したライブラリは、次のコマンドで削除できます。

```sh
typl --remove-lib           # この typl が書き出したものを削除する
typl --remove-lib --others  # 別の版の typl が書き出したものを削除する
typl --remove-lib --all     # すべての版のものを削除する
```

`typl` をアンインストールする前には `typl --remove-lib` を、入れ直した後には
`typl --remove-lib --others` を実行すると、使われないライブラリが残りません。削除したものは、
`typl` が次にコンパイルするときに書き出し直します。削除するのは `lib/` の下のビルドIDの
ディレクトリだけで、それ以外のファイルには触れません。

デバッグビルドの `typl` は、ビルドしたリポジトリの `target/debug/` にあるものを使い、何も
書き出しません。

別の場所に置いたライブラリを使うには、`--lib-dir` でそのフォルダを指定します。

```sh
cp target/debug/libtypelisp_front.a ~/lib/typelisp/
typl --lib-dir ~/lib/typelisp -c foo.typl
```

`libtypelisp_front.a` は、そのファイルと同時にビルドした `typl` でしか使えません。`typl` を
ビルドし直したら、コピーも取り直してください。

### その他のオプション

```sh
typl --help        # オプションの一覧を表示する
typl --version     # バージョンを表示する
typl --remove-lib  # 書き出したライブラリを削除する（--others / --all も使える）
```

## 文書

日本語の文書の一覧は [docs/ja/README.md](docs/ja/README.md) にあります。

### チュートリアル

- [入門](docs/ja/tutorial/intro.md)
- [型の基本](docs/ja/tutorial/types.md)
- [トレイト](docs/ja/tutorial/traits.md)
- [マクロ](docs/ja/tutorial/macros.md)
- [エラー処理](docs/ja/tutorial/errors.md)
- [並行処理](docs/ja/tutorial/concurrency.md)

### ガイド

- [モジュールとファイル構成](docs/ja/guide/modules.md)
- [コンパイル](docs/ja/guide/compile.md)
- [ファイル I/O、ストリーム、ネットワーク](docs/ja/guide/io.md)
- [C FFI](docs/ja/guide/ffi.md)
- [エディタ連携](docs/ja/guide/editors.md)
- [Common Lisp から来た人へ](docs/ja/guide/from-common-lisp.md)

### リファレンス

- [構文リファレンス](docs/ja/reference/syntax.md)
- [組み込み関数](docs/ja/reference/functions/README.md)
- [型の一覧](docs/ja/reference/types.md)
- [エラーメッセージ](docs/ja/reference/errors.md)

### エディタ連携

- [Emacs（typelisp-mode）](editor/emacs/README.md)
- [VS Code](editor/vscode/README.md)

## ライセンス

typelisp は、次の2つのライセンスのどちらかを選んで利用できます。

- MIT License（[LICENSE-MIT](LICENSE-MIT)）
- Apache License, Version 2.0（[LICENSE-APACHE](LICENSE-APACHE)）

どちらを選んだ場合も、[LICENSE-EXCEPTION](LICENSE-EXCEPTION) の例外が加わります。`typl -c` や
`compile-file` で作った実行ファイルには typelisp の一部が含まれますが、その部分については
typelisp の著作権表示やライセンス文を添えずに配布できます。

typelisp への貢献は、特に断りのない限り、上と同じ条件（2つのライセンスのどちらか、および例外）で
提供されたものとして扱います。
