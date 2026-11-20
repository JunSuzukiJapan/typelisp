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

- `throw`/`panic` は今、Rust の panic による巻き戻しで、ドライバが活性化の境界 1 か所で `catch_unwind` している（C4）。stable Rust の wasm は `panic=abort` なので、このままでは動かない。
- `rt_panic`/`rt_throw` などは巻き戻さず、「巻き戻し中」の状態を記録して特別な状態語を返す。島が emit する呼び出し地点は、その状態語を受けたら自フレームのハンドラ pc（`FRAME_HANDLER_SLOT`）へ飛ぶか、ドライバへ返す。
- ネイティブも同じ仕組みにして、仕組みを 1 つにする。`catch_unwind` はランタイムの Rust 自身のバグ（本物の panic）を報告するためだけに残る。
- extern の表が変わるので、島の再生成 2 回と prelude の再生成が要る（C4 と同じ手順: 機構の切り替え → regen → 古い方の削除 → regen）。

### 段階 4: wasm32-wasip1

- コード生成: `--target` の triple で `TargetMachine` を作る。
- prelude: コミット済みの `prelude.typld` はホストの datalayout 向けなので、ターゲットごとのダンプを持つ。
- ランタイム: `typelisp-front` を `wasm32-wasip1` の staticlib としてもビルドする。wasm に無い機能は cfg で外す。
  - OS スレッド（`Thread<T>`）、ネットワーク、TLS、C FFI、`libc` の `getrusage`/`uname`/`getpwuid_r`/`localtime_r`。
  - これらを使うプログラムは、wasm 向けのコンパイル時にエラーにする（実行時に失敗させない）。判定は段階 2 と同じ「到達できる shim の表」で行う。
  - タスク（`task`）、チャネル、`select` は単一スレッドのスケジューラで動く。
- リンク: `wasm-ld`（LLVM 22 の lld）。
- テスト: `wasmtime` で実行して出力を比べる。

### 段階 5: wasm32-unknown-unknown

- I/O（標準出力、標準エラー、終了）を JS から import する関数にする。ファイル I/O はコンパイル時にエラー。
- `.wasm` と一緒に、読み込んで実行する最小限の JS を出す。
- テスト: `node` で実行して出力を比べる。

## 必要なツール

- `rustup target add wasm32-wasip1 wasm32-unknown-unknown`
- `brew install lld`（`wasm-ld`。LLVM 22 と同じ版）
- `brew install wasmtime`
