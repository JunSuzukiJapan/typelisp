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
- 0b: `print-object` の登録。生成する `main` が、ファイルと prelude にある全ての `print-object` を `rt_print_object_method` で登録している。このため、使わない型の `print-object` もその先も残る。たとえば `file-stream` の `print-object` から `rt_stream_describe`、`decoded-time` の `print-object` から `rt_float_to_int` と bignum が付いてくる。その型の値が作られうるときだけ登録する。値を作るのは生成コードだけではない。Rust の shim が作って返す値（エラーの型など）もあるので、「作られうる」の判定はその両方を見る。
- 0c: ランタイムの共有状態（`RtShared`）のデストラクタ。ストリームの表を通って TLS サーバの drop まで届き、rustls を残している。共有状態の破棄が、使われたものだけに届く形にする。
- 0d: 0b と 0c の後に測り直し、残った大きなもの（書式エンジン、std の panic と backtrace の処理など）を同じ方法で調べる。調べ方は、`-Wl,-why_live,<記号>` で `main` からの参照の鎖を出す。

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
