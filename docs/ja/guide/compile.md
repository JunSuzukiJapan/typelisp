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
args: #<vector<string> ./hello a b>
fib(25) = 75025
```

`-o` を省略すると、ソースファイル名から `.typl` を除いた名前の実行ファイルが、ソースファイルと
同じフォルダにできます。ソースファイル名が `.typl` で終わらないときは `-o` が必要です。
`-c`（`--compile`）を使うときは、`--image` `--heap-cells` `--feature` は指定できません。

REPL やプログラムの中から `compile-file` を呼んでも同じことができます。

```sh
$ typl
typl> (compile-file "hello.typl" "hello")
typl> :quit
$ ./hello a b
args: #<vector<string> ./hello a b>
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

- 入口のファイルのトップレベルに書けるのは、定義（`defun` `defmethod` `defvar` `defconstant`
  `defstruct` `defenum` `defffi` `impl`）と `use` `module` だけです。`(println ...)` のような
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

## 4. ダンプ

いまのセッションの定義を 1 ファイルに保存し、次回はそこから起動できます。

```sh
$ typl
typl> (defun sq ((n i32)) i32 (* n n))
typl> (compile sq)
typl> (dump "session.typld")
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
