# クロスコンパイル（組み込み、Cortex-M）計画

2026-11-22 決定。ブランチ `feature/cross-compile-embedded`。前提は [cross-compile-wasm-plan.md](cross-compile-wasm-plan.md) の段階 1〜5（資源の制限、`--no-cons`、巻き戻しの状態語化、ターゲット別の拒否、出力等の `typelisp_mem::host` への集約）。

## 目標

`typl -c` で Cortex-M のマイコン向けの実行ファイル（ELF）を作る。

- ターゲット: `thumbv7em-none-eabihf`（Cortex-M4F/M7）を先に作る。続けて `thumbv6m-none-eabi`（Cortex-M0+）と `thumbv8m.main-none-eabihf`（Cortex-M33）。
- AVR（Arduino Uno など）は、Cortex-M で実行ファイルの大きさと RAM の使用量が分かってから、実現できるかを判断する（段階 7）。
- ハードウェアには volatile の読み書きで触る（段階 5）。
- 使っていない関数とデータは実行ファイルに入れない。これは組み込みに限らず、全ターゲットに効かせる（段階 0）。

## コマンド行

wasm と同じ置き場。

```
typl -c SOURCE [-o OUTPUT] --target thumbv7em-none-eabihf [--heap-cells N | --no-cons] [--max-memory SIZE] [--linker-script FILE]
```

- `--linker-script FILE`: メモリの配置（フラッシュと RAM の番地と大きさ）。省略したときは、テストに使う QEMU のボードの配置になる。

## 段階

### 段階 0: 使わないものをリンクしない

測った値（`(defun main () () (println "hello"))`、`--release --no-cons`、macOS x86_64）:

| | `__text` |
|---|---|
| 変更前 | 6,729,696 |
| 0a の後 | 1,776,448 |

wasm32-unknown-unknown では、strip しても 2,042,594 バイトあった。

- 0a（済）: ネイティブのリンクに `-Wl,-dead_strip`（Linux は `-Wl,--gc-sections`）を渡す。これが無いと、アーカイブのオブジェクトがまるごと入っていた。
- 0b（済）: `print-object` の登録。生成する `main` が、ファイルと prelude にある全ての `print-object` を `rt_print_object_method` で登録していたため、使わない型の `print-object` もその先も残っていた（`file-stream` から `rt_stream_describe`、`decoded-time` から bignum）。今は、その型の値が作られうるときだけ登録する（`aot.rs` の `live_print_objects`）。
  - 値を作るのは、型表から型を読んでランタイムへ渡すコンパイル済みコードか、ランタイム自身（`typelisp_mem::runtime_builds`: 基本型、`BUILTIN_TYPES`、`RUNTIME_BUILT`）。`main` から届く関数が読む型（`aot/reach.rs` の `TypeReach`）とその型引数、ランタイムが作る型を生きているとし、生きている型の `print-object` から届くものも同じように数える。印字器の登録の関数は数えない。`eval` を呼ぶプログラムは全てを登録する。
  - 落とした型は関数の番地 0 で登録し、その値を印字すると内部エラーになる。`RUNTIME_BUILT` の漏れは `type_identity_guard_test` がソースの `TypeSpec` から見つける。
  - 測定（release のランタイム、hello）: `__text` 1,062,832 → 1,017,296。hello では 18 個を落とした。残る大きなものは、印字器の登録の関数そのもの（27,808 バイト、登録ごとに呼び出しを並べている）、std の backtrace（gimli、addr2line、rustc_demangle）、rustls と ring（0c）、基本型の `print-object` から届く bignum と `Renderer`。
- 0c: ランタイムの共有状態（`RtShared`）のデストラクタ。ストリームの表を通って TLS サーバの drop まで届き、rustls を残している。共有状態の破棄が、使われたものだけに届く形にする。
- 0e: 書式をコンパイル時に展開する（2026-11-22 ユーザー決定、下の「書式の展開」）。
- 0d: 0b、0c、0e の後に測り直し、残った大きなもの（書式エンジン、std の panic と backtrace の処理など）を同じ方法で調べる。調べ方は、`-Wl,-why_live,<記号>` で `main` からの参照の鎖を出す。

### 書式の展開（段階 0e）

release のランタイムでの測定（上と同じ hello）: `__text` 1,040,368 バイトのうち、書式エンジン（`typelisp_print`）が 176KB、そこから届く bignum が 50KB。`println "hello"` は `rt_println` から解釈器全体（`interp_seq` → `interp_dir` → 全ての指示子の描画）に届く。指示子を 1 つも使わない文字列でも同じ。

今の形: チェッカーは制御文字列を検査し（`typelisp_print::format::call_sites`）、引数を 1 個ずつ `Sexpr` に包んで `F_FORMAT_RT`/`F_PRINT_RT`/`F_PRINTLN_RT` を呼ぶ。実行時にエンジンが文字列を解析し（`parse`）、解釈する（`State::interp_seq`）。

変える形: 制御文字列は必ずリテラルなので、lowering で「出力に書き足す」小さな組み込み関数の呼び出しの列に変える。実行時に制御文字列を解析・解釈するコードは無くなる。

- **置き場所は lowering。** ジェネリックな本体は特殊化ごとに lowering されるので、そこでは引数の型が具体的に決まっている。インタプリタ、JIT、AOT は lowering の結果を実行するので、実装は 1 つのまま。
- **引数の型で描画を選ぶ。** `~a` の引数が `string` なら文字列をそのまま書く関数、`int` なら整数の描画、それ以外の型なら値の描画（`print-object` の呼び分けを含む）。引数を `Sexpr` に包まない。
- **`~/name/` は呼び先の直接呼び出しになる。** 名前から定義を引く表（`rt_format_call_method`）は要らなくなる。
- **引数のカーソル。** 位置が 1 つに決まる場所（ほとんど）は、展開時に引数を名指しする。`~[` の節ごとに消費の数が違う場合のように、取りうる位置が複数あるときは、整数の局所変数をカーソルにし、取りうる位置ごとの `case` に展開する。位置の集合は既存の `Sim` が求めている。リストの要素（`~{…~}`、`~<…~:>`）は `Sexpr` のリストをたどるループになる。
- **出力のバッファ**（`pprint::Out`、文字列と pretty printer の命令）は GC ヒープの箱にする。書式の途中で throw や panic があってもバッファが漏れない（スタックで持つと、抜けた書式の分と入れ子の書式の分を見分けられない）。
- 結果の出し方（文字列を返す、stdout に書く、開いている logical block に合流する）は今の `emit` と同じ。

進め方:

1. 実行時の部品: バッファの箱と、指示子ごとの書き足し関数（`typelisp-print`）。
2. 展開器: 制御文字列と引数の型から、呼び出しの列を作る（`typelisp-print` に置き、型の分類は `ArgKind`）。
3. lowering から展開器を呼ぶ。`F_FORMAT_RT`/`F_PRINT_RT`/`F_PRINTLN_RT` とエンジンの解釈器（`State`）を消す。
4. 島と prelude の再生成、全体テスト、大きさの測定。

### 段階 1: ランタイムを `no_std` + `alloc` で動かす

`thumbv7em-none-eabihf` には std が無い（wasm32-unknown-unknown には中身の無い std があった）。`typelisp-abi`、`typelisp-mem`、`typelisp-rt`、`typelisp-print`、`typelisp-read`、`typelisp-front` が対象になる。

- feature `std`（既定で有効）を設け、std が要るもの（OS スレッド、ネットワーク、ファイル、環境変数、プロセス）はその下に置く。
- `thread_local!` は、単一コアの組み込みでは大域の 1 つにする。置き換える口は 1 か所に作り、各所はそこを通す。
- `std::collections::HashMap` は `hashbrown`（依存に既にある）に、`std::io` の出力は `typelisp_mem::host` に寄せる。
- 組み込みで使えない shim を使うプログラムは、wasm と同じくコンパイル時にエラーにする（`aot/reach.rs`）。wasm32-unknown-unknown の拒否の表に近いが、C FFI は使える（ベンダーの HAL を呼ぶため）。

### 段階 2: ターゲットの追加とリンク

- コード生成: wasm と同じく、結合したモジュールにターゲットの triple と datalayout を設定してから passes を流す。prelude のビットコードはポインタを `ptrtoint`/`inttoptr` で i64 と行き来するだけなので、32bit のターゲットでも成り立つ見込み。
- 起動: ベクタテーブル、リセットハンドラ（`.data` のコピーと `.bss` の 0 埋め）、ヒープの領域。ランタイムの Rust 側に置く。
- リンク: rustup の `rust-lld`（`ld.lld` として起動）とリンカスクリプト。
- ランタイムは `scripts/build-embedded-runtime.sh` がターゲットごとにビルドする（wasm と同じ形）。

### 段階 3: ホスト関数

`typelisp_mem::host` の write/exit/now/unix_time/sleep を、組み込みでは外から与える関数にする。

- QEMU では semihosting（出力と終了）で与える。テストはこれを使う。
- 実機では、ボードに合わせた実装（UART への出力、SysTick による時刻）を、利用者が `defffi` で渡す C の関数か typelisp の関数で与える。どちらにするかは段階 3 で決める。

### 段階 4: テスト

`qemu-system-arm`（Cortex-M4 のボード、`-semihosting`）で実行し、出力と終了コードを比べる。wasm で node を使ったのと同じ役。

### 段階 5: volatile の読み書き

メモリマップドレジスタに触る組み込み関数。

- 読み: `u8`/`u16`/`u32` の幅ごとに 1 つ。書き: 同じく幅ごとに 1 つ。番地は整数で渡す。
- `unsafe` の中でだけ呼べる（`defffi` の呼び出しと同じ扱い）。
- コンパイルすると LLVM の `load volatile`/`store volatile` になる。インタプリタでも同じ意味で動かす。

### 段階 6: 割り込み

割り込みハンドラを typelisp の関数で書く。ベクタテーブルに置けるのは、FFI のコールバックと同じく自由変数の無い関数だけにする。

### 段階 7: AVR の判断

LLVM 22 には AVR のバックエンドがある。判断の材料は次の 3 つ。

- 大きさ: ATmega328P（Uno）はフラッシュ 32KB、RAM 2KB、ATmega2560（Mega）はフラッシュ 256KB、RAM 8KB。段階 0〜4 の後の Cortex-M の hello world の大きさと比べる。
- ポインタが 16bit。値を i64 の語で扱う呼び出し規約はそのまま使えるが、遅くて大きい。
- Rust の `avr-none` は tier 3 で、nightly と `-Zbuild-std` が要る。使うかどうかはユーザーが決める。
