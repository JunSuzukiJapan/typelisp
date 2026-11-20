# クロスコンパイル（WebAssembly）計画

2026-11-20 決定。ブランチ `feature/cross-compile-wasm`。

## 目標

`typl -c` で WebAssembly の実行ファイルを作る。あわせて、実行ファイル全般（ネイティブと wasm の両方）に資源の制限を付ける。

- ターゲット: `wasm32-wasip1`（WASI）を先に作り、続けて `wasm32-unknown-unknown`（ブラウザ）。
- cons アリーナの容量を指定できる。
- cons セルを使わない設定。cons が要る操作を含むプログラムは**コンパイル時に**エラーにする。
- 使用メモリの上限。数えるのはプロセスの確保量の全部（cons アリーナ、文字列、箱、ランタイム内部）。上限を超える確保をしようとした時点で実行時エラーにする。

## コマンド行

`-c` の後ろに書く（`-O<N>`、`--release` と同じ置き場）。

```
typl -c SOURCE [-o OUTPUT] [--target TRIPLE] [--heap-cells N] [--no-cons] [--max-memory SIZE]
```

- `--heap-cells N`: 実行ファイルの cons アリーナの容量（セル数）。既定は今と同じ `1 << 16`（`eval` を含むプログラムは `1 << 18`）。アリーナは大きくならない。
- `--no-cons`: cons セルを使わない。容量は 0 になり、cons が要る操作はコンパイル時にエラーになる。`--heap-cells` とは併用できない。（段階 2 で実装）
- `--max-memory SIZE`: 確保量の上限。`SIZE` はバイト数で、`K`/`M`/`G`（1024 の累乗）を付けてよい。
- `--target TRIPLE`: 既定はホスト。`wasm32-wasip1` と `wasm32-unknown-unknown` を受け付ける。

`(compile-file SOURCE OUTPUT)` は既定の設定で作る。

## 段階

### 段階 1: 資源の制限（ネイティブ）

- `typl -c` が `--heap-cells`/`--max-memory` を受け取り、`aot::BuildOptions` で `compile_file_with` に渡す。
- 生成する `main` が `rt_heap_init(容量, 上限)` を呼ぶ。上限 0 は「制限なし」。
- 上限は `typelisp-rt` の `#[global_allocator]`（確保量を数える）が守る。超える確保をしようとしたら、確保せずにメッセージを stderr に出して `EXIT_CODE_PANIC` で終わる。allocator の中では巻き戻せないので、`catch` では捕まらない。

### 段階 2: cons を使わない設定（コンパイル時の検出）

- 「cons セル」は cons アリーナのセル（`--heap-cells` が数えるもの）。`cons` 関数が作る型付きペア `cons-cell<A,B>` は箱で、アリーナを使わない。
- 判定は実行ファイルのモジュール全体（プログラム、リンクした prelude、生成した `main`）で行う（`compile::aot::no_cons`）。`main` から参照をたどる。直接の呼び出しに限らず、オペランドに現れる関数（ドライバ往復の呼び先、クロージャ、vtable や `print-object` の登録）も、定数式と大域の初期化子の中まで含める。
- 届いた先が宣言だけの shim で、`externs::CONS_MAKING_SHIMS` に載っていればエラー。表に載るのは、`Heap::cons` に届く shim（`rt_cons`、`rt_path_to_list`、リーダ、`eval`/`macroexpand`）。
- cons を使わずに済むように変えたもの:
  - `format`/`print`/`println` の書式の引数: `Sexpr` のリストをやめ、1 個ずつ組み込み関数の引数として渡す。
  - `select` の答え: `(腕 . 値)` の cons をやめ、`#{腕 値}` の箱にする。
- `--no-cons` の実行ファイルはアリーナが 0 セルで、`--heap-cells` とは併用できない。

### 段階 3: 巻き戻しを状態語に

今の形: `throw`/`panic` と shim のエラー（`typelisp_abi::raise`）は Rust の panic で、コンパイル済みの 1 活性化ぶんのマシンフレームを抜けて、ドライバの `catch_unwind` が受ける（C4）。ドライバは受けた payload を退避し（`park_activation_unwind`）、`STATUS_UNWIND` としてフレーム鎖をたどってハンドラの `pc` へ再開する。stable Rust の wasm は `panic=abort` なので、この panic が使えない。

変える形:

1. **raise は戻る。** `raise` は payload を退避して呼び出し元へ戻る（`-> !` ではなくなる）。raise しうる Rust の関数は `Result<_, Raised>` を返し、shim の境界まで伝える。shim は 0 を返す。`Raised` はトークンで、raise した事実を型で運ぶ（黙って捨てられない）。
2. **生成コードが確かめる。** raise しうる shim を呼んだ後、生成コードは `rt_unwinding` を呼び、退避された巻き戻しがあれば `coroutine-unwind`（`STATUS_UNWIND` を返す）へ分岐する。どの shim が raise しうるかは extern の表で 1 か所に答える。
3. **ドライバは catch_unwind しない。** 戻ってきた状態語と退避の有無だけで判断する。classic ABI の本体を呼ぶ箇所も同じ。`catch_unwind` は、ランタイムの Rust 自身の不具合（本物の panic）を報告する場所にだけ残る。
4. **コンパイル済みコードを呼ぶ Rust**（`print-object` のフック、C コールバック、`rt_drive_body`）は、戻ったあとに退避を確かめて `Raised` を返す。

ネイティブも同じ仕組みにする。extern の表と島の SOURCE が変わるので、島の再生成 2 回と prelude の再生成が要る。

実装（ABI 0.3.65）:

- `typelisp_abi::raise`/`raise_payload` は payload を thread-local の `RAISED` に置いて `#[must_use] Raised` を返す。shim は `raise(..).into_word()`（0）で戻る。`take_raised` で取り出す。
- 確かめるコードは `src/compile/raise_check.rs` の `after_call` 1 か所で出す。呼び出し元は島の `build-call`/`build-call-with-env`、コルーチンの `emit_rt_call`、FFI の thunk と entry。raise しない shim は `externs::NEVER_RAISING_SHIMS` に挙げ、確かめを省く。
- 抜け方は呼び出し元の型で決まる: `i64 f(i64)`（コルーチン本体）は `STATUS_UNWIND`、それ以外は戻り型のゼロ。ゼロで抜けた関数の呼び出し元も同じ確かめで抜けるので、巻き戻しは活性化の口まで伝わる。
- ドライバ（`coroutine::activation`）は、状態語が `STATUS_UNWIND` なら退避を取り出して `park_activation_unwind` へ渡す。退避があるのに活性化が普通に戻ったら fatal（確かめの漏れ）。
- `print-object` のフック、書式の shim、C コールバック、インタプリタからの呼び出し（`crossing::catch_compiled_panic`）は、呼んだあとに `is_raised`/`take_raised` を見る。

### 段階 4: wasm32-wasip1

- コード生成: `--target` の triple で `TargetMachine` を作る。
- ランタイム: `typelisp-front` を `wasm32-wasip1` の staticlib としてもビルドする。wasm に無い機能は cfg で外す。
  - OS スレッド（`Thread<T>`）、ネットワーク、TLS、C FFI、`libc` の `getrusage`/`uname`/`getpwuid_r`/`localtime_r`。
  - これらを使うプログラムは、wasm 向けのコンパイル時にエラーにする（実行時に失敗させない）。判定は段階 2 と同じ「到達できる shim の表」で行う。
  - タスク（`task`）、チャネル、`select` は単一スレッドのスケジューラで動く。
- リンク: `wasm-ld`。
- テスト: WASI の実行環境で実行して出力を比べる。

実装:

- prelude はターゲットごとのダンプを持たない。コミット済みのビットコードは datalayout を持たず、最適化前で、ポインタは `ptrtoint`/`inttoptr` で i64 と行き来するだけなので、wasm32 でも幅を広げ・縮めるだけで成り立つ。結合したモジュールに wasm の triple と datalayout を設定してから passes を流す（`src/compile/aot/wasm.rs`）。
- 例外が 1 つある。表（型・形・フレームの配置）の初期値にある `ptrtoint (ptr @g to i64)` は、32bit のアドレスを 64bit の枠に置く再配置が wasm の目的ファイルに無いので、バックエンドが断る。wasm では、そういう `[N x i64]` を packed な構造体に置き換え、アドレスの語を `<{ ptr @g, i32 0 }>` にする（`split_address_words`）。リトルエンディアンなので、バイト列は同じになる。
- 入口: wasi-libc の `crt1-command.o` は `__main_void` を呼ぶので、`main` をその名前に変える。引数は Rust の std が WASI から読む。
- リンク: rustup の toolchain の `rust-lld` を `wasm-ld` という名前で起動する。`crt1-command.o`、`libc.a` と並べて、`scripts/build-wasm-runtime.sh` が `<target>/wasm-runtime/<profile>/wasm32-wasip1/` に置く（Homebrew の LLVM 22 には lld が無く、Homebrew の rust には wasm のターゲットが無い）。スタックは 8MiB。
- ランタイム: wasm では `net`、`net/tls`、`x509`、`net_builtin`、`rt_net_*` の shim、ストリームのソケットの変種を cfg で外す（rustls/ring は wasm 以外だけの依存）。スケジューラはスレッドが 1 本: `thread_count` は 1、時間つきの待ちは眠るだけ、時間なしの待ちは fatal、`SelfPipe` は作れるだけで触れたら unreachable。インタプリタ（`eval`）からのソケットは panic。
- 使えないもののコンパイル時の検出は段階 2 の到達判定を一般化した `src/compile/aot/reach.rs` で行う。表は `externs::WASM_UNAVAILABLE_SHIMS` と `rt_net_` の接頭辞。C の関数は `defffi` の記号で判定する。
- リリースビルド: feature `wasm` を付けたときだけ、build.rs の `bundled_wasm` が 2 つのターゲットのランタイムと wasi-libc（`crt1-command.o`、`libc.a`）を入れ子の cargo でビルドして埋め込む（ユーザー決定: 指定されない限り何も埋め込まない）。書き出し先は `$TYPELISP_HOME/lib/<ビルドID>/<triple>/` で、ビルドID は wasm のものも含めたハッシュ。付けなかったリリースビルドの `--target` はエラー。`rust-lld` は toolchain の `libLLVM.dylib` に依存し単体で動かないので埋め込まず、リリースビルドは実行時に `rustup which rustc` の sysroot の `rust-lld` を `-flavor wasm` で使う（デバッグビルドはスクリプトが置いたシンボリックリンク）。
- テスト: `tests/aot_wasm_test.rs`。`scripts/run-wasi.mjs`（node の `node:wasi`、`/` だけを開く。`.` も開くと絶対パスの解決が `.` に引かれて失敗する）で実行し、ネイティブの実行ファイルと出力・終了コードを比べる。

### 段階 5: wasm32-unknown-unknown

- I/O（標準出力、標準エラー、終了）を JS から import する関数にする。ファイル I/O はコンパイル時にエラー。
- `.wasm` と一緒に、読み込んで実行する最小限の JS を出す。
- テスト: `node` で実行して出力を比べる。

実装:

- このターゲットの std は、標準出力・標準エラーへの書き込みを捨て、`Instant::now`/`SystemTime::now`/`thread::sleep` は panic、`process::exit` は trap、`env::args` は空、ファイルは常に `Err`。libc も無い（`mem_limit` の `write`/`_exit`、`isatty` が通らない）。
- 窓口を 1 つ作る: `typelisp_mem::host`。標準出力・標準エラーへの書き込み、終了、時刻（単調時刻と UNIX 時刻）、眠る。wasm32-unknown-unknown 以外は std に渡すだけ、このターゲットでは import モジュール `typelisp` の関数を呼ぶ。新しいクレートにしないのは、公開するクレートの並びを変えないため。最下層の `typelisp-mem` なら全クレートから使える。
- 置き換えるもの: ランタイムの `eprintln!`（fatal、panic/throw の報告）、`Backend::Stdout`/`Stderr` の書き込み、`Instant` はスケジューラ全体で `host::Instant` に、`SystemTime` は `host::unix_time`、`sleep` は `host::sleep`、`exit` は `host::exit`。Rust 自身の panic は panic hook で `host` に書く。
- import: `write(fd, ptr, len)`、`exit(code)`（JS は例外を投げて wasm から抜ける）、`now() -> f64`（ミリ秒、単調）、`unix_time() -> f64`、`sleep(ms)`（node と Worker では `Atomics.wait` で本当に待つ）。
- 入口: crt が無いので、`main` を `typelisp_main` として export し、`--no-entry` でリンクする。JS が呼んで戻り値を終了コードにする。
- 標準入力は読めば入力の終わり、`(command-line-args)` は空。コンパイル時に断るのは、wasip1 の表に加えて、ファイルとディレクトリの操作。
- リンク: crt1/libc 無しで `wasm-ld --no-entry --export=typelisp_main`。ランタイムは `scripts/build-wasm-runtime.sh` が両方のターゲットを作る。
- ローダー: `src/compile/aot/wasm_loader.mjs` を `include_str!` し、モジュールの名前を埋めて `<出力>.mjs` に書く。`node prog.mjs` で実行でき、`run({ write })` で他の JS からも呼べる。Node では `fs.writeSync` で書く（`process.stdout.write` は macOS のパイプで非同期になり、終了の前に失われうる）。ブラウザの既定はコンソールへ 1 行ずつ、最後に改行の無い行も出す。`SharedArrayBuffer` は `sleep` の時に初めて作る（クロスオリジン分離されていないページでは作れないので、sleep しないプログラムを巻き込まない）。
- 断る表: `externs::wasm_unknown_unavailable` = wasip1 の表 + `NO_FILE_SYSTEM_SHIMS`（`rt_stream_open_file`）+ `rt_file_` の接頭辞。
- テスト: `tests/aot_wasm_js_test.rs`。import が `typelisp` の 5 つだけであることも確かめる。ブラウザでの実行は未確認（Node のみ）。

## 必要なツール

- `rustup target add wasm32-wasip1 wasm32-unknown-unknown`
- node（WASI の実行。テストが使う）
