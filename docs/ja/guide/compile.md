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

- `install.sh` で入れた `typl` は、そのまま使えます。`cargo install` で入れるときは準備がいります
  （3.7 節の準備も兼ねます）。まず rustup で `rustup target add wasm32-wasip1 wasm32-unknown-unknown` を
  実行します。そのうえで、`--features wasm` を付けてインストールします（`cargo install --locked --path . --features wasm`）。
  付けずに入れた `typl` は WebAssembly のランタイムを持たないので、`--target` はエラーになります。
  リポジトリのデバッグビルドの `typl` を使うときは、代わりにリポジトリで `scripts/build-wasm-runtime.sh`
  を実行します。
- リンクには rustup の toolchain のリンカ（`rust-lld`）を使うので、コンパイルするときにも rustup が要ります。
- 実行ファイルと同じに振る舞います。標準出力・標準エラー・終了コード、`(command-line-args)`、
  ファイルの読み書き、`throw`/`catch`/`panic`、タスク・チャネル・`sleep`、`eval` が使えます。
  タスクは 1 つのスレッドで順に実行されます。ファイルは、実行環境がモジュールに見せている
  ディレクトリの中だけを読み書きできます（wasmtime なら `--dir` で指定します）。
- WebAssembly にないものを使うプログラムは**コンパイルエラー**になります。エラーには、使ったものと、
  `main` からそこへ届く関数の並びが表示されます。使えないのは、ソケット（TCP・UDP・Unix ドメイン・
  TLS・名前解決）、OS スレッド（`Thread`）、C の関数（`defffi`）、`get-internal-run-time`、
  `machine-instance`、`software-version`、`file-author`、`ed` です。
- WASI には時間帯を知る手段がないので、地方時は分かりません。`zone` を省いた
  `decode-universal-time`・`encode-universal-time` と `get-decoded-time` は、実行時に panic します。
  `zone` を渡せば使えます。
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
- 地方時（`zone` を省いた `decode-universal-time` など）は、JavaScript の `Date` が報告する
  時間帯で求めます。
- 3.6 節で使えないものに加えて、ファイルとディレクトリの操作、環境変数、`(command-line-args)`、
  ホームディレクトリ、`dribble` が**コンパイルエラー**になります。標準入力は空として読まれます。

### 3.8 マイコン（Cortex-M）向けにコンパイルする

`--target thumbv7em-none-eabihf` を付けると、OS の無い Cortex-M4F・M7 のボード向けの実行ファイル
（ELF）を作ります。`--target thumbv8m.main-none-eabihf` なら Cortex-M33 のボード向けです。`-o` を
省くと、ソースの `.typl` を `.elf` に変えた名前になります。

```sh
$ typl -c hello.typl --target thumbv7em-none-eabihf          # hello.elf ができる
$ qemu-system-arm -M mps2-an386 -nographic \
    -semihosting-config enable=on,target=native -kernel hello.elf   # QEMU の Cortex-M4 で実行する
$ typl -c hello.typl --target thumbv8m.main-none-eabihf
$ qemu-system-arm -M mps2-an505 -nographic \
    -semihosting-config enable=on,target=native -kernel hello.elf   # QEMU の Cortex-M33 で実行する
```

- `install.sh` で入れた `typl` はそのまま使えます。`cargo install` で入れるときは、
  `rustup target add thumbv7em-none-eabihf thumbv8m.main-none-eabihf` を実行してから `--features embedded` を付けてインストールします
  （wasm と両方なら `--features wasm,embedded`）。リポジトリのデバッグビルドの `typl` では
  `scripts/build-embedded-runtime.sh` を実行します。リンクには rustup の toolchain のリンカを使います。
- ボードのメモリの配置は `--linker-script FILE` で渡します。書き方は `cortex-m-rt` の `memory.x` と同じで、
  FLASH と RAM の番地と大きさを書きます。`_typelisp_stack_size = 32K;` のように書くと、RAM のうちスタックに
  使う大きさになります（書かなければ 64KiB）。RAM の残りがヒープです。省いたときは QEMU の `mps2-an386`（Cortex-M33 では `mps2-an505`）
  の配置になります。

  ```
  MEMORY
  {
    FLASH : ORIGIN = 0x08000000, LENGTH = 512K
    RAM : ORIGIN = 0x20000000, LENGTH = 128K
  }
  _typelisp_stack_size = 16K;
  ```

- 出力・入力・終了・時刻（ボードの関数）を誰が与えるかは `--board-functions` で選びます。
  - `semihosting`（既定）: semihosting で与えます。QEMU（`-semihosting-config enable=on`）やデバッガで動かすときに使います。
  - `c`: ボードが C の関数で与えます。次の関数を定義したオブジェクトか静的ライブラリを `--link FILE` で渡します
    （`--link` は何度でも書けます。`defffi` で呼ぶベンダーのライブラリも同じように渡します）。

  ```c
  void     typelisp_host_write(int32_t fd, const uint8_t *p, size_t len); // fd 1 は標準出力、2 は標準エラー
  size_t   typelisp_host_read(int32_t fd, uint8_t *p, size_t len);        // fd 0、読んだバイト数、終わりなら 0
  void     typelisp_host_exit(int32_t code);                              // 戻らない
  uint64_t typelisp_host_now_us(void);                                    // 逆戻りしない時計のマイクロ秒
  int64_t  typelisp_host_unix_us(void);                                   // 1970-01-01 からのマイクロ秒
  void     typelisp_host_sleep_us(uint64_t us);                           // us マイクロ秒待つ
  ```

- ボードのレジスタ（メモリに割り当てられた周辺機器）は `volatile-read-u8`・`volatile-read-u16`・
  `volatile-read-u32` で読み、`volatile-write-u8`・`volatile-write-u16`・`volatile-write-u32` で書きます。
  番地は `int` で渡し、読んだ値はその幅の型（`u8` など）になります。書いた式の場所で必ず 1 回読み書きし、
  コンパイラは省いたり並べ替えたりしません。どれも `unsafe` の中でだけ使えます。番地が負のときと、
  幅の倍数でないときは、読み書きの前に実行時エラーになります。

  ```lisp
  ;; QEMU の mps2-an386 の UART 0: データ 0x40004000、状態 0x40004004、制御 0x40004008
  (defun put-byte ((c u32)) ()
    (unsafe
      (loop (when (= (logand (volatile-read-u32 #x40004004) 1) 0) (break)))   ; 送信中なら待つ
      (volatile-write-u32 #x40004000 c)))

  (defun main () ()
    (unsafe (volatile-write-u32 #x40004008 1))                               ; 送信を有効にする
    (put-byte 104) (put-byte 105) (put-byte 10))                             ; "hi" と改行
  ```

- 割り込みが来たときに呼ばれる関数は、`defun` に `#[interrupt(N)]`（周辺機器の割り込み `N`）か
  `#[exception(SysTick)]` のような属性を付けて書きます。割り込みは `(enable-interrupt N)` で有効にします。
  ハンドラはその場で実行され、ランタイムの処理の途中に来たときだけ、その処理が終わってから実行されます。
  ハンドラの中では待てません（`sleep` はコンパイルエラーです）。詳しくは
  [構文リファレンス](../reference/syntax.md) の 3.17 節にあります。

  ```lisp
  (use std::board::enable-interrupt)

  #[interrupt(5)]
  (defun on-five () ()
    (println "interrupt 5"))

  (defun main () ()
    (enable-interrupt 5)
    (unsafe (volatile-write-u32 #xE000E200 (as u32 32))))   ; 割り込み 5 を自分で起こす（NVIC の保留レジスタ）
  ```

- 起動（ベクタテーブルと、RAM の準備）とメモリの確保はランタイムが行います。HardFault や処理されない
  割り込みが起きると、panic と同じ終了コード（1）で終わります。
- `throw`/`catch`/`panic`、bignum・ratio・浮動小数、タスク・チャネル・`sleep` は実行ファイルと同じに
  振る舞います。タスクは 1 つのコアで順に実行されます。C の関数（`defffi`）も使えます。
- ボードにないものを使うプログラムは**コンパイルエラー**になります。3.7 節で使えないものに加えて、
  `eval`・`macroexpand` が使えません。地方時は分かりません（3.6 節と同じ）。
- cons アリーナは 1024 セルから始まり、足りなければ伸びます。`--heap-cells`、`--max-memory`、`--no-cons`、
  `-O<N>`、`--release` はそのまま使えます。

### 3.9 Arduino Uno（AVR）向けにコンパイルする

`--target avr-atmega328p` を付けると、Arduino Uno のマイコン（ATmega328P）向けの実行ファイル（ELF）を
作ります。次のプログラムは、ボードの LED を点滅させます。

```lisp
(defconstant (PINB int) #x23)
(defconstant (DDRB int) #x24)
(defconstant (LED u8) 32)                         ; PB5

(defun wait-a-bit () ()
  (let ((i (the u32 0)))
    (while (< i (the u32 400000))
      (unsafe (volatile-read-u8 PINB))
      (setf i (+ i 1)))
    ()))

(defun main () ()
  (unsafe (volatile-write-u8 DDRB LED))           ; 出力にする
  (while true
    (unsafe (volatile-write-u8 PINB LED))         ; 反転する
    (wait-a-bit)))
```

```sh
$ typl -c blink.typl --target avr-atmega328p -O2   # blink.elf ができる
$ qemu-system-avr -M uno -display none -serial stdio -bios blink.elf   # QEMU で実行する
```

- ランタイムが無いので、ランタイムを使うプログラムは**コンパイルエラー**になります。エラーには、
  何が必要で、どの関数から届くのかが出ます。使えるのは、固定幅の整数（`i8` から `u32`）、`bool`、`char` の計算、
  関数呼び出し、条件分岐、ループ、それらの値を持つ `defvar` と `defconstant`、番地を数か定数で書いた `volatile-read-u8` などです。
- 使えないもの: 文字列・リスト・構造体など、ヒープに置く値。`int` の計算。再帰、`catch`、関数値の呼び出し。C の関数の呼び出し。
  割り込みのハンドラ。変数で割る除算（数で割る除算は使えます）。番地を計算して求める `volatile`。
- 何もしないループは `-O1` 以上で消えます。待つループの中では、レジスタを読むなどしてください。
- `main` が終わると、マイコンは止まります。
- メモリの配置は `--linker-script FILE` で変えられます。省いたときは、フラッシュ 32KiB から末尾のブートローダの
  512 バイトを除いた分と、RAM 2KiB です。RAM の番地は `0x800100` のように `0x800000` を足して書きます。
- `f32` と `f64` の計算は、AVR 用の補助関数が無いのでリンクのエラーになります。
- リンクには 3.8 節と同じ、rustup の toolchain のリンカを使います。

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
