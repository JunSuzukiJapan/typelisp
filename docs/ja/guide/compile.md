# コンパイル

typelisp のプログラムは、何もしなければインタプリタで実行されます。これに加えて、ネイティブ
コードへのコンパイル手段が 2 つと、環境を保存する手段が 1 つあります。仕様の詳細は
[構文リファレンス 10 章](../reference/syntax.md#10-コンパイル) にあります。

| 手段 | 使い方 | 結果 |
|---|---|---|
| JIT コンパイル | `(compile name)` | 実行中のセッションの関数がネイティブコードに置き換わる |
| AOT コンパイル | `typl -c src.typl` または `(compile-file "src.typl" "out")` | 単体で動く実行ファイルができる |
| ダンプ | `(dump "file.typld")` | 定義を保存し、`typl --image` で同じ環境から起動できる |

## 1. 準備

コンパイルには LLVM 22 を使います。[README_JP.md](../../../README_JP.md) の手順で `typl` を
ビルドできていれば、追加の準備はいりません。

AOT コンパイルで作る実行ファイルには、静的ライブラリ `libtypelisp_front.a` をリンクします。
リリースビルドの `typl`（`cargo install` で入れたものを含む）は、このライブラリを中に持って
いるので、準備はいりません。初めてコンパイルするときに `~/.typelisp/lib/<ビルドID>/`
（環境変数 `TYPELISP_HOME` を設定していれば `$TYPELISP_HOME/lib/<ビルドID>/`）に書き出し、
以後はそれを使います。`typl --remove-lib` で削除できます（`--others` を付けると別の版の
`typl` が書き出したもの、`--all` を付けるとすべて）。デバッグビルドの `typl` は、ビルドした
リポジトリの `target/debug/` にあるものを使います。別の場所に置いたものを使うには、`typl` の
起動時に `--lib-dir` でそのフォルダを指定します（3.2 節）。
macOS ではリンクに Xcode Command Line Tools を使います。

## 2. JIT コンパイル

定義済みの関数を、その場でネイティブコードにします。

```lisp
(defun fib ((n int)) int
  (if (< n 2) n (+ (fib (- n 1)) (fib (- n 2)))))

(compile fib)
(fib 30)           ; 以降の呼び出しはコンパイル済みのコードが走る
```

- `name` は評価されません。関数名をそのまま書きます（文字列ではありません）。メソッドは
  `(compile point::norm)` のように型名を付けて書きます。
- 呼び出している関数も一緒にコンパイルされます。
- **ジェネリック関数はコンパイルできません。** 型ごとの実体は使う場所ごとに作られるためです。
  具体的な型で呼んでいる側の関数をコンパイルしてください。
- `trace`・`step`・`disassemble`・`compile`・`compile-file`・`dump` はインタプリタの操作なので、
  これらを呼ぶ関数はコンパイルできません。コンパイルしようとすると、理由を述べるエラーになります。

コンパイル結果を見るには `disassemble` を使います。

```lisp
(disassemble fib)          ; ホストの機械語
(disassemble fib true)     ; LLVM IR
```

## 3. AOT コンパイルで実行ファイルを作る

### 3.1 プログラムを書く

入口として、**引数を取らない `main` 関数**を定義します。

```lisp
(use std::env::command-line-args)
;; hello.typl
(defun fib ((n int)) int
  (if (< n 2) n (+ (fib (- n 1)) (fib (- n 2)))))

(defun main () ()
  (println "args: ~a" (command-line-args))
  (println "fib(25) = ~a" (fib 25)))

(main)
```

ファイル末尾の `(main)` は、`typl hello.typl` で実行するときに `main` を呼ぶためのものです。
`compile-file` はこの末尾の `(main)` を読み飛ばすので、同じファイルをインタプリタでも AOT でも
使えます。

### 3.2 コンパイルする

コマンドラインからは `typl -c`（`typl --compile` も同じ）を使います。

```sh
$ typl -c hello.typl            # hello ができる
$ typl -c hello.typl -o fib     # 実行ファイルの名前を fib にする
$ ./hello a b
args: #(./hello a b)
fib(25) = 75025
```

`-o` を省略すると、ソースファイル名から `.typl` を除いた名前の実行ファイルが、ソースファイルと
同じフォルダにできます。ソースファイル名が `.typl` で終わらないときは `-o` が必要です。
`-c`（`--compile`）を使うときは、`--image` と `--feature` は指定できません。`--heap-cells` は、`-c` と一緒に使うと実行ファイルの cons アリーナの容量になります（[3.5](#35-実行ファイルが使う資源)）。

`typl` に `--release` を付けると**リリースビルド**になり、`eprint`/`eprintln`（調べるための標準エラー
出力。[印字](../reference/functions/printing.md)）を引数ごと取り除きます。取り除くものも型検査は
するので、そこに型エラーがあればコンパイルは失敗します。`--release` は `typl` 全体のオプションで、
`-c` だけでなく、ファイルの実行・REPL・`typl test` でも同じように効きます。`--release` を付けた
`typl` の中で呼んだ `compile-file` もリリースビルドになり、そうして作った実行ファイルの中で
`eval` したコードからも `eprint`/`eprintln` は取り除かれます。

```sh
$ typl --release -c hello.typl
$ typl -c hello.typl --release     # -c の後ろに書いても同じ
```

`-O0` `-O1` `-O2` `-O3` で**最適化のレベル**を指定します。既定は `-O0` で、最適化しません。`-O1` 以上では、
リテラルだけを束縛した変数をその値で置き換える、定数どうしの整数演算を計算しておく、といった簡単な
最適化をしたうえで、残りを LLVM の同じ名前のレベルの最適化に任せます。どのレベルでも計算結果は
変わりません。`--release` と同じく `typl` 全体のオプションで、`-c` で作る実行ファイルのほか、
ファイルの実行・REPL・`typl test` の中でコンパイルされるコードと、そこで呼んだ `compile-file` にも
効きます。インタプリタが実行するコードは最適化しません。

```sh
$ typl -O2 -c hello.typl
$ typl -c hello.typl -O2     # -c の後ろに書いても同じ
```

REPL やプログラムの中から `compile-file` を呼んでも同じことができます。

```sh
$ typl
typl> (compile-file "hello.typl" "hello")
typl> :quit
$ ./hello a b
args: #(./hello a b)
fib(25) = 75025
```

繰り返しビルドするなら、この 1 行をファイルに書いておき、`typl build.typl` で実行してもかまいません。

```lisp
;; build.typl
(compile-file "hello.typl" "hello")
```

ファイル名は **`typl` を起動したカレントディレクトリ**から解決されます。`build.typl` の場所
からではありません。

`typl` が使うのとは別の場所に置いた `libtypelisp_front.a` をリンクするときは、そのフォルダを
`--lib-dir` で指定します。`typl -c` と `compile-file` のどちらにも効きます。

```sh
$ typl --lib-dir ~/lib/typelisp -c hello.typl
$ typl --lib-dir ~/lib/typelisp build.typl
```

指定したフォルダに `libtypelisp_front.a` が無ければ、エラーで止まります。このファイルは、同時に
ビルドした `typl` でしか使えません。`typl` をビルドし直したら、コピーも取り直してください。

### 3.3 AOT コンパイルできるファイルの形

- 入口のファイルのトップレベルに書けるのは、定義（`defun` `defvar` `defparameter`
  `defconstant` `defmacro` `defsignature` `defstruct` `defenum` `deftype` `deftrait` `impl`
  `defffi`、`(unsafe (def-c-struct ...))`）と `use` `module` だけです。`(println ...)` のような
  トップレベルの式は、末尾の `(main)` を除いて書けません。処理は `main` の中に書いてください。
- 引数なしの `main` が無いとエラーになります。
- `use` しているモジュールのファイルも一緒にコンパイルされ、1 つの実行ファイルにまとまります。
- `defffi` の `:library` で指定したライブラリは自動でリンクされます（[C FFI](ffi.md)）。
- 標準ライブラリの関数はすべて AOT でも使えます。`eval` も使えますが、その場合は型検査器と
  インタプリタが実行ファイルに入るので、実行ファイルが大きくなり、起動にも時間がかかります。
  `eval` を呼ばないプログラムには入りません。

### 3.4 実行ファイルの振る舞い

- `(command-line-args)` は、`typl hello.typl a b` で実行したときも `./hello a b` で実行したときも
  同じ形の `Vector<string>` を返します。先頭の要素がプログラム名です。
- 終了コードは `(exit n)` で指定します。`main` が普通に戻れば 0 です。
- `panic` すればメッセージを表示して 0 以外で終了します。

### 3.5 実行ファイルが使う資源

`typl -c` には、実行ファイルが実行中に使ってよい資源を指定するオプションがあります。

```sh
$ typl --heap-cells 100000 -c hello.typl    # cons アリーナを 100000 セルに固定する
$ typl -c hello.typl --max-memory 64M       # 確保するメモリを 64 MiB までにする
$ typl -c hello.typl --no-cons              # cons セルを使わない
```

- `--heap-cells N` は、cons セルのアリーナの容量を `N` セルに固定します。アリーナはそれ以上
  大きくなりません。ガベージコレクションをしても空きセルが作れなければ、実行時エラーで止まります。
  指定しないときは 65536 セルで始まり、必要に応じて大きくなります。
- `--max-memory SIZE` は、実行ファイルが確保するメモリの合計の上限です。数えるのは、cons アリーナ、
  文字列、ベクタやハッシュ表などの値、ランタイム自身が使うメモリのすべてです。`SIZE` はバイト数で、
  `K` `M` `G`（それぞれ 1024 倍、1024² 倍、1024³ 倍）を付けて書けます。上限を超えて確保しようと
  すると、そのことを表示して終了コード 1 で終了します。この終了は `catch` では捕まえられません。
  指定しないときは上限はありません。
- `--no-cons` は、cons セルを使わない実行ファイルを作ります。cons アリーナを持たないので、
  cons セルが必要な操作をするプログラムは**コンパイルエラー**になります。エラーには、その操作と、
  `main` からそこへ届く関数の並びが表示されます。cons セルが必要になるのは、`Sexpr` のリスト
  （クォートしたリストや準クォートで作るものを含む）や `&rest` 引数を作るとき、`read`、`eval`、
  `macroexpand` を使うときです。数値・文字列・ベクタ・ハッシュ表・構造体・チャネルなどの操作と、
  `print`/`println`/`format` は cons セルを使いません。`--heap-cells` とは一緒に指定できません。

`--max-memory` と `--no-cons` は `-c` の後ろに書きます。`--heap-cells` は `-c` の前にも後ろにも書けます。
cons アリーナもメモリの上限に数えられるので、アリーナがそのまま入らない上限を指定すると、
プログラムが始まる前に止まります。

### 3.6 WebAssembly 向けにコンパイルする

`--target wasm32-wasip1` を付けると、実行ファイルの代わりに WebAssembly のモジュール（WASI
preview 1 のコマンド）を作ります。`-o` を省くと、ソースの `.typl` を `.wasm` に変えた名前になります。

```sh
$ typl -c hello.typl --target wasm32-wasip1      # hello.wasm ができる
$ wasmtime hello.wasm a b                        # WASI に対応した実行環境で実行する
args: #(hello.wasm a b)
fib(25) = 75025
```

- 準備がいります。この機能は、リポジトリでビルドしたデバッグビルドの `typl` でだけ使えます。
  rustup で `rustup target add wasm32-wasip1 wasm32-unknown-unknown` を実行してから、リポジトリで
  `scripts/build-wasm-runtime.sh` を実行してください（3.7 節の準備も兼ねます）。
- 実行ファイルと同じに振る舞います。標準出力・標準エラー・終了コード、`(command-line-args)`、
  ファイルの読み書き、`throw`/`catch`/`panic`、タスク・チャネル・`sleep`、`eval` が使えます。
  タスクは 1 つのスレッドで順に実行されます。ファイルは、実行環境がモジュールに見せている
  ディレクトリの中だけを読み書きできます（wasmtime なら `--dir` で指定します）。
- WebAssembly にないものを使うプログラムは**コンパイルエラー**になります。エラーには、使ったものと、
  `main` からそこへ届く関数の並びが表示されます。使えないのは、ソケット（TCP・UDP・Unix ドメイン・
  TLS・名前解決）、OS スレッド（`Thread`）、C の関数（`defffi`）、`get-internal-run-time`、
  `machine-instance`、`software-version`、`file-author`、ローカルのタイムゾーン、`ed` です。
- `--heap-cells`、`--max-memory`、`--no-cons`、`-O<N>`、`--release` はそのまま使えます。

### 3.7 ブラウザや Node.js で動かす

`--target wasm32-unknown-unknown` を付けると、JavaScript から読み込んで実行する WebAssembly の
モジュールを作ります。モジュール（`hello.wasm`）と一緒に、それを読み込んで実行する JavaScript
（`hello.mjs`）が同じフォルダにできます。準備は 3.6 節と同じです。

```sh
$ typl -c hello.typl --target wasm32-unknown-unknown   # hello.wasm と hello.mjs ができる
$ node hello.mjs                                       # Node.js で実行する
args: #()
fib(25) = 75025
```

ほかの JavaScript からは、`hello.mjs` の `run` を呼びます。`write` には出力が文字列で渡されます
（`fd` は標準出力が 1、標準エラーが 2）。`run` の値は終了コードです。

```js
import { run } from './hello.mjs';
const code = await run({ write: (fd, text) => { /* 画面に出すなど */ } });
```

- `write` を渡さないと、Node.js では標準出力・標準エラーに、ブラウザではコンソールに 1 行ずつ出します。
- ブラウザでは、`hello.wasm` を `hello.mjs` と同じ場所から HTTP で配信してください（`file://` からは
  読み込めません）。
- `sleep` は `Atomics.wait` で待ちます。ブラウザでは、これが使えるのはクロスオリジン分離された
  ページの Worker の中だけなので、`sleep` を使うプログラムは Worker で実行してください。
- 3.6 節で使えないものに加えて、ファイルとディレクトリの操作が**コンパイルエラー**になります。
  標準入力は空として読まれ、`(command-line-args)` は空のベクタです。

## 4. ダンプ

いまのセッションの定義を 1 ファイルに保存し、次回はそこから起動できます。

```sh
$ typl
typl> (defun sq ((n i32)) i32 (* n n))
typl> (compile sq)
true
typl> (dump "session.typld")
true
typl> :quit
$ typl --image session.typld
typl> (sq 9)
81
```

`typl --image session.typld prog.typl` のように、ファイルの実行にも使えます。

- 保存されるのは**定義**です。セッションで評価した式は保存されません。
- `compile` した関数は、コンパイル済みの形で保存されます。
- グローバル変数は、保存した時点の値ではなく、**初期化式をもう一度実行した値**で復元されます。
- ダンプを書いた `typl` と版の違う `typl` では読み込めません（エラーになります）。

ファイルを実行して `(dump ...)` した場合、そのファイルの定義はファイル名のモジュールに入って
います。`dp.typl` で定義した関数は `dp::sq` という名前になり、別のファイルから呼ぶには
`pub` が必要です（[モジュールとファイル構成](modules.md)）。

## 5. コンパイル済みモジュールのファイルについて

Common Lisp の `.fasl` のように、モジュールごとのコンパイル結果をファイルに書き出す形式は
ありません。`compile-file` はソースから直接、実行ファイルを作ります。途中のファイルは残りません。
