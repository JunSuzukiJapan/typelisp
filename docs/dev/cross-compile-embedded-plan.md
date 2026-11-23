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
- 0b（済）: `print-object` の登録。生成する `main` が、ファイルと prelude にある全ての `print-object` を `rt_print_object_method` で登録していたため、使わない型の `print-object` もその先も残っていた（`file-stream` から `rt_stream_describe`、`decoded-time` から bignum）。今は、その型の値が作られうるときだけ登録する（`aot.rs` の `live_types`）。
  - 値を作るのは、型表から型を読んでランタイムへ渡すコンパイル済みコードか、ランタイム自身（`typelisp_mem::runtime_builds`: 基本型と `BUILTIN_TYPES`）。`main` から届く関数が読む型（`aot/reach.rs` の `TypeReach`）とその型引数、ランタイムが作る型を生きているとし、生きている型の `print-object` から届くものも同じように数える。印字器の登録の関数は数えない。`eval` を呼ぶプログラムは全てを登録する。
  - 落とした型は関数の番地 0 で登録し、その値を印字すると内部エラーになる。ランタイムの `TypeSpec` に出る定義は、`BUILTIN_TYPES` か `NAMED_NOT_BUILT`（名前は出るが実行ファイルのランタイムは値を作らない、理由付き）のどちらかに載っていることを `type_identity_guard_test` が確かめる。
  - 測定（release のランタイム、hello）: `__text` 1,062,832 → 1,017,296。hello では 18 個を落とした。残る大きなものは、印字器の登録の関数そのもの（27,808 バイト、登録ごとに呼び出しを並べている）、std の backtrace（gimli、addr2line、rustc_demangle）、rustls と ring（0c）、基本型の `print-object` から届く bignum と `Renderer`。
- 0c（済）: ランタイムの共有状態（`RtShared`）のデストラクタ。スレッドローカルの破棄からストリームの表の drop を通って、TLS の接続とサーバの設定の drop まで届き、rustls を残していた。ストリームの `close` も rustls のメソッドを直に呼んでいた。
  - TLS の接続（`net::TlsConn`）とリスナーの設定（`net::TlsServer`）を、型を名指さない箱に入れた。drop と close は箱の vtable を通るので、rustls を参照するのは接続を作る所（`tls-connect`、`tls-listen`、`accept`）だけになる。
  - 測定（release のランタイム、hello）: `__text` 1,017,296 → 970,336。rustls の記号は 180 → 0。
  - ring のアセンブリ（約 102KB）は残る。アーカイブはコード生成単位ごとに読み込まれ、読み込んだオブジェクトの参照も解決されるので、死んだコードからの参照で ring のオブジェクトも読み込まれる。Rust のオブジェクトは関数ごとに落とせるが、ring のアセンブリのオブジェクトは macOS の ld64 では分けられず（`dont-dead-strip`）、まるごと残る。Linux の `--gc-sections` はセクション単位で届くかを見るので落ちる見込み（未確認）。組み込みのターゲットには net が無いので関係しない。
- 0e: 書式をコンパイル時に展開する（2026-11-22 ユーザー決定、下の「書式の展開」）。
- 0d: 0b、0c、0e の後に測り直し、残った大きなものを同じ方法で調べる。調べ方は、`-Wl,-why_live,<記号>` で `main` からの参照の鎖を、`-Wl,-why_load` でアーカイブのどのオブジェクトがなぜ読み込まれたかを出す（PATH に `cc` のラッパーを置いて渡す）。
  - 済: 印字器の登録（変種の名前、フィールドの型と名前）も、値が作られうる型の定義の分だけにした（`aot.rs` の `Live::has_definition`）。登録の関数は 27,808 → 8,352 バイト。
  - 済: 基本型の `print-object` は登録しない。実行ファイルの印字器が登録を引くのは構造体と enum の値だけで、基本型の値はどちらでもない。
  - 測定（release のランタイム、hello）: `__text` 970,336 → 845,680（ここまでの 2 つ）。
  - 外せないもの: std のバックトレース（gimli、addr2line、rustc_demangle、約 75KB）。std のメモリ確保の失敗とパニックの処理から届く。std を作り直さない限り外せない。組み込み（段階 1 で `no_std`）では入らない。
  - 見送り: stream の組み込みは全て `stream_builtin` の名前による振り分けを通るので、shim を 1 つ使うと全ての操作が残る（約 12KB）。組み込みでは段階 1 でファイルとネットワークが外れる。
  - 済: ランタイムが一部の組み込みでだけ値を作る型（ファイル・ソケット・読み取り・`parse-float` のエラー、`ParseIntError`、時刻の 2 つ、`heap-info`）は、作る組み込みの shim に届くときだけ生きているとする（`aot/made.rs`）。作る組み込みの表は、言語サーバー（rust-analyzer）で型の定数を参照する関数と、その呼び出し元を shim まで引いて作り、マクロが書く呼び出し（`stream_shim!` の短い形）は手で補った。stream、socket、system の組み込みが作る値は、組み込みごとの結果の型の表の行に必ず書かれている（行の無い組み込みは止まる）ので、その表から読む。作る箇所が増減すると `type_identity_guard_test` が落ちる。
  - 測定（release のランタイム、hello）: `__text` 845,680 → 817,280。hello が登録する `print-object` は `standard-stream` の 1 つだけになった。
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

`thumbv7em-none-eabihf` には std が無い（wasm32-unknown-unknown には中身の無い std があった）。対象は `typelisp-abi`、`typelisp-mem`、`typelisp-print`、`typelisp-read`、`typelisp-rt` の 5 つ。`typelisp-front`（チェッカーとインタプリタ）は対象にしない。実行ファイルが `typelisp-front` の中で呼ぶのは `eval` の shim（`rt_eval*`、`rt_macroexpand*`、`rt_run_program_interp*`）だけで、組み込みでは `eval` をコンパイル時に断る。組み込みのランタイムの静的ライブラリは、`typelisp-rt` までを束ねる新しいクレートが作る。

- feature `std`（既定で有効）を設け、std が要るもの（OS スレッド、ネットワーク、ファイル、環境変数、プロセス）はその下に置く。
- `thread_local!` は、単一コアの組み込みでは大域の 1 つにする。置き換える口は 1 か所に作り、各所はそこを通す。
- ロック: parking_lot の型は `lock_api` の型の別名なので、使う側（`read_recursive`、`MappedRwLockReadGuard` など）はそのままに、ロックの本体だけを std では parking_lot、組み込みでは原子操作の自前のものにする。口は `typelisp_mem::sync` の 1 か所。
- `std::collections::HashMap` は、std では今のまま、組み込みでは `hashbrown`（新しい依存）にする。型の別名を `typelisp_mem` の 1 か所に置く。`std::io` の出力は `typelisp_mem::host` に寄せる。
- `stacker`（深い入れ子でスタックを伸ばす）は std の下に置く。組み込みでは伸ばさずにそのまま呼ぶ。
- 組み込みで使えない shim を使うプログラムは、wasm と同じくコンパイル時にエラーにする（`aot/reach.rs`）。wasm32-unknown-unknown の拒否の表に近いが、C FFI は使える（ベンダーの HAL を呼ぶため）。

進め方（下のクレートから。各段で `cargo check --target thumbv7em-none-eabihf --no-default-features` が通り、std の既定の構成で全体テストが通ること）:

1. `typelisp-mem`: `sync`（ロックと一度だけの初期化）、`thread_local!` の口、`core`/`alloc` への書き換え、`host` の組み込み版の口。
2. `typelisp-abi`。
3. `typelisp-print`、`typelisp-read`。
4. `typelisp-rt`: ネットワーク、ファイル、環境変数、プロセス、OS スレッドを std の下へ。
5. 組み込みのランタイムのクレートと、使えない shim の拒否の表。

1〜5 は済（5 は [runtime-split-plan.md](runtime-split-plan.md) でランタイムを分けた上で作った。拒否の表は段階 2 でターゲットと一緒に作る）。5 つのクレートとも `cargo check -p <クレート> --lib --target thumbv7em-none-eabihf --no-default-features` が警告 0。決めたこと:

- 依存の書き方: ワークスペースの依存表では 5 つを `default-features = false` で宣言し、std が要る側（ルート、`typelisp-front`）が `features = ["std"]` を書く。各クレートの `std` は依存先の `std` を有効にする。
- ハッシュ: `sxhash` は std の有無にかかわらず種を固定した `typelisp_mem::collections::FixedHasher`（foldhash）で数える。std の `DefaultHasher` とは値が違う。
- 浮動小数: `trunc`/`powi` は `num_traits::float::FloatCore`、三角関数などは `num_traits::Float`（std なしでは libm）。std があれば std の関数に渡る。
- メモリ: std なしの大域アロケータは、ボードが与える `typelisp_host_alloc`/`typelisp_host_dealloc` の上の `typelisp_mem::host::board::BoardAlloc`。メモリの上限（`mem_limit`）はその上に重なる。
- アーキテクチャと OS の名前は `typelisp_mem::host::ARCH`/`OS` の 1 か所（`machine-type`、`software-type`、リーダーの `*features*`）。std なしで知っているのは arm と none だけで、他は `compile_error!`。
- std なしで外したもの: ソケットとその待ち（`Waiting::Io`）、OS スレッド、ファイル、標準入力、環境変数、コマンド行引数、CPU 時間、エディタ、dribble、`machine-instance`、`software-version`。これらの shim は std なしのビルドに無い。
- std なしで値を返すもの: スレッドは 1 つ（スケジューラの `thread_count` と `thread-available-parallelism` は 1、期限の無い待ちで全てのタスクが止まれば fatal）、ローカルのタイムゾーンは不明（WASI と同じ）、`machine-version` は `None`、標準入力は端末でない。

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
