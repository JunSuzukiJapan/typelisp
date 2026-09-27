# typelisp

typelisp は静的型付きの Lisp です。

## インストールとビルド

### 必要なもの

- Rust（cargo）
- LLVM 17
- macOS では Xcode Command Line Tools（ビルド時に `xcrun` を使います）

LLVM 17 は Homebrew で入れられます。

```sh
brew install llvm@17
```

### 初回の設定

クローンした後に一度だけ、次のスクリプトを実行します。

```sh
scripts/setup-cargo-env.sh
```

このスクリプトは `brew --prefix llvm@17` で LLVM 17 の場所を調べ、`.cargo/config.toml` を生成します。
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

Homebrew を使わない場合は、`LLVM_SYS_170_PREFIX` に LLVM 17 のインストール先を設定してください。
macOS ではさらに `MACOSX_DEPLOYMENT_TARGET` も必要です。

### ビルド

```sh
cargo build            # デバッグビルド（target/debug/）
cargo build --release  # リリースビルド（target/release/）
```

次の2つの実行ファイルができます。

| 実行ファイル | 役割 |
|---|---|
| `typl` | 処理系本体。引数なしで REPL、ファイルを渡すとそのファイルを実行します |
| `typl-lsp` | 言語サーバ（エディタ連携用） |

```sh
target/debug/typl              # REPL を起動する
target/debug/typl foo.typl     # foo.typl を実行する
```

`compile-file` で作る実行ファイルは、ビルドしたリポジトリの `target/` にある静的ライブラリを
リンクします。そのため、`typl` をビルドしたリポジトリは移動したり削除したりしないでください。

## 文書

日本語の文書の一覧は [docs/ja/README.md](docs/ja/README.md) にあります。

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
