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
  - 済（段階 8）: stream の組み込みは全て `stream_builtin` の名前による振り分けを通っていたので、shim を 1 つ使うと全ての操作が残った。今は操作ごとの関数を shim が直接呼ぶ。ファイルとネットワーク（`fs_builtin`、`net_builtin`）は OS の上でしか使わないので、振り分けのまま。
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

済（段階 3 の QEMU の分と段階 4 も）。決めたこと:

- `typl -c --target thumbv7em-none-eabihf [--linker-script FILE]`。出力の既定はソースの `.typl` を `.elf` に変えた名前。`--linker-script` は `cortex-m-rt` の `memory.x` の形（FLASH と RAM）で、省略すると QEMU の `mps2-an386`（0 から 4MiB、`0x20000000` から 4MiB、スタック 1MiB）。スタックの大きさは `_typelisp_stack_size` で、書かなければ 64KiB。
- 起動は `cortex-m-rt`（ベクタテーブル、`Reset`、`link.x`）。入口（`typelisp-rt-embedded` の `start`）がヒープ（`.bss` の後ろからスタックまで、`embedded-alloc` の LLFF）を用意して、生成した `main` を `typelisp_main` の名前で呼ぶ。HardFault と未処理の割り込みは、パニックと同じ終了コードで終える。
- コード生成: triple は正規化してから使う（`thumbv7em-unknown-none-eabihf`）。書いたままだと `eabihf` が OS の位置に読まれ、浮動小数の補助関数を GNU の名前（`__divdf3`）と基本の呼び出し規約で呼び、ハードフロートの規約で作られたランタイムの実装と引数のレジスタが食い違った。関数と大域変数はそれぞれ専用のセクション（`.text.<名前>` など）に置き、`--gc-sections` で使わないものを落とす。置かないと、使わない関数から届くソケットなどの shim が未定義になる。セクション名は inkwell の `set_section` を使わずに付ける（macOS のホストでは先頭にカンマを足す）。
- リンクは rustup の `rust-lld`（`-flavor gnu`）。デバッグビルドでは `scripts/build-embedded-runtime.sh` がランタイムの横に `link.x` と `ld.lld` を置く。release の `typl` は `--features embedded` のときランタイム 2 つと `link.x` を持つ（`build.rs` の `bundled_embedded`、リンカは wasm と同じく rustup のもの）。配布物（`scripts/dist/build.sh`）は `--features wasm,embedded` で作り、QEMU で動くことを確かめる。
- cons のアリーナの初期値は 1024 セル（本体の既定の 65536 セルは 3MiB）。足りなければ伸びる。
- 断るもの: C1、C2、`eval`（`RuntimeCrate::Front`）。C FFI は使える。ボードの C 関数はホストに無いので、ボード向けのコンパイル中は `defffi` が記号を探さない。コンパイル中に呼ばれたら（マクロなど）、理由を言って `typl` を終える。
- 測定（debug のランタイム、`(println "hello from a board")`）: text 601,020 バイト、data 1,196、bss 452。

### 段階 3: ホスト関数

`typelisp_mem::host` の write/exit/now/unix_time/sleep を、組み込みでは外から与える関数にする。

- 2026-11-23 決定: 実機では C の関数で与える。`--board-functions c` で、ボードが `typelisp_host_write`/`read`/`exit`/`now_us`/`unix_us`/`sleep_us` を定義したオブジェクトか静的ライブラリを `--link FILE` で渡す（ヒープの確保は `typelisp-rt-embedded` が与える）。既定は `--board-functions semihosting`。ランタイムは 2 つの静的ライブラリ: `typelisp-rt-embedded`（C の関数の分）と `typelisp-rt-semihosting`（それに semihosting の実装を足したもの）。feature にしなかったのは、crates.io から入れたビルドでは依存先のクレートを `-p` で選ぶことになり、cargo がそれに feature を付けられないため（`cannot specify features for packages outside of workspace`）。2 つの Rust の静的ライブラリはどちらも `core` を持つので、1 つの実行ファイルに両方はリンクできない。テストはボードの関数を LLVM IR で書き（QEMU のボードの UART に出力）、`llc` でオブジェクトにする。
- QEMU では semihosting（出力と終了）で与える。テストはこれを使う。済: `typelisp-rt-semihosting`。出力と入力は `:tt` を開いた handle、終了は `SYS_EXIT_EXTENDED`（終了コードが渡る）、時刻は `SYS_ELAPSED`/`SYS_TICKFREQ`、Unix 時刻は `SYS_TIME` の秒に経過時間を足す、sleep は経過時間を待つ。ヒープの確保はボードによらず `typelisp-rt-embedded` が与える。
- 実機では、ボードに合わせた実装（UART への出力、SysTick による時刻）を、利用者が `defffi` で渡す C の関数か typelisp の関数で与える。どちらにするかは段階 3 で決める。

### 段階 4: テスト

`qemu-system-arm`（Cortex-M4 のボード、`-semihosting`）で実行し、出力と終了コードを比べる。wasm で node を使ったのと同じ役。済: `tests/aot_embedded_test.rs`（本体の実行ファイルと標準出力・標準エラー・終了コードを比べる、断るものの検査、`--linker-script`）。CI は QEMU を入れる。

### 段階 5: volatile の読み書き

メモリマップドレジスタに触る組み込み関数。

- 読み: `u8`/`u16`/`u32` の幅ごとに 1 つ。書き: 同じく幅ごとに 1 つ。番地は整数で渡す。
- `unsafe` の中でだけ呼べる（`defffi` の呼び出しと同じ扱い）。
- コンパイルすると LLVM の `load volatile`/`store volatile` になる。インタプリタでも同じ意味で動かす。

済（2026-11-23）: 特殊形 `volatile-read-u8`/`-u16`/`-u32` と `volatile-write-u8`/`-u16`/`-u32`（`typeck::c_memory`）。番地は `int`、読んだ値はその幅の型、書く形は `()` を返す（レジスタは書いた値を読み返すとは限らない）。`c-deref` と同じく内部の組み込み関数（`" c-volatile-load"`/`" c-volatile-store"`、`c_struct.rs`）に下ろし、コンパイル済みコードはランタイムの `rt_c_volatile_load`/`rt_c_volatile_store`、インタプリタは同じ `c_mem::volatile_load`/`volatile_store` を呼ぶ。そこで `read_volatile`/`write_volatile`、つまり LLVM の `load volatile`/`store volatile` になる。島が命令を直接出す形にはしなかった（呼び出し 1 回ぶん遅いが、島の IR を増やさずに 3 つの実行経路が同じ関数を通る）。番地が負・機械の番地の幅を超える・幅の倍数でないときは、触る前に実行時エラー（整列していない volatile アクセスは未定義動作のため）。テストは `tests/volatile_test.rs`（ホストでは `malloc` の領域）と `aot_embedded_test.rs` の QEMU の UART を直接動かすもの。

### 段階 6: 割り込み

割り込みハンドラを typelisp の関数で書く。ベクタテーブルに置けるのは、FFI のコールバックと同じく自由変数の無い関数だけにする。

済（2026-11-23）。ユーザー決定: 割り込みは「すぐに対応する」。書き方は `#[interrupt(N)]`/`#[exception(NAME)]`（引数は括弧で囲む）。

- 書き方: リーダーは `#[name(args)]` を `(name args...)` に読む。チェッカー（`typeck::attribute`）は引数無し・`()` を返す・型パラメータ無しの `defun` だけを受け、ベクタテーブルの番号（例外の番号、割り込みは 16+N）ごとに 1 つの関数を記録する（`Checker::interrupt_handlers`）。属性の名前は綴りで比べる（語彙に入れると `interrupt` などが予約語になるため）。`HardFault` はランタイムのもの。
- いつ実行するか: その場で実行する。ただし**ランタイムが忙しい間は保留**し、忙しさが解けた瞬間に実行する。GC の最中だけでは足りない（確保の途中、スケジューラ・チャネル・出力の途中、借用中の thread-local も壊れる）ため、「コンパイル済みコードがランタイムの関数を呼んでいる間」を忙しいとした。ボード向けのリンクでは、モジュールが宣言する `rt_*` への呼び出しを、`typelisp_busy_enter`/`typelisp_busy_leave` で挟む内部関数に置き換える（`aot/embedded.rs` の `busy_around_runtime_calls`）。ランタイムがタスクのコンパイル済みコードを走らせる間（`FrameStack::for_task` の鎖の `activation`）だけ忙しさを外す（`interrupt::outside`）。入れ子のドライバ（`print-object`、C のコールバック、初期化）の中では外さない（呼んだ側が状態を使いかけのため）。保留中の割り込みは NVIC で止め、実行後にプログラムが有効にしていれば再開する。ハンドラどうしは割り込み合わない（優先度は全部同じ、実行中は保留）。
- ハンドラの実行: `typelisp_rt::interrupt::run`。コンパイル済みコードとドライバの間の受け渡し（`call_state` と raise の受け皿）を退避してから走らせる（呼び出しを publish した直後に止められていることがある）。GC は止め、cons はアリーナを伸ばす（`Heap::inhibit_collection`、止めたプログラムがレジスタにだけ値を持っていることがある）。待つ操作は、スケジューラがその場で答えられるものだけ答え、他は panic（既存の `run_to_end`）。`sleep` とソケットの待ちはコンパイル時に断る（`externs::ALWAYS_WAITING_SHIMS` をハンドラから `reach` でたどる）。外へ出た panic/throw はプログラムを終わらせる。
- ボードのランタイム: cortex-m-rt の `DefaultHandler` が全ての未処理の例外と割り込みを受けるので、そこから `interrupt::arrived` に渡す（`device` 機能やリンカスクリプトの追加は要らなかった）。NVIC の mask/unmask は `Controller` として起動時に渡す。
- `enable-interrupt`/`disable-interrupt`（`std::board`）はランタイムの関数にした（ランタイムが止めた割り込みを再開してよいかを知るため）。ボード以外の `typl -c` で届けばコンパイルエラー（`externs::BOARD_ONLY_SHIMS`）、インタプリタでは実行時エラー。ハンドラのあるプログラムをボード以外へコンパイルするのもエラー。
- テスト: `aot_embedded_test.rs`（NVIC の保留レジスタで自分で起こす割り込み、cons を作り続ける間の SysTick、ハンドラの panic、`sleep` の拒否、ボード以外での拒否）、`interrupt_test.rs`（属性の検査、インタプリタでは普通の関数）。

### Cortex-M33（済、2026-11-23）

`thumbv8m.main-none-eabihf`（`Target::Thumbv8mMain`）。ランタイムはそのまま通った。ボードごとの違い（LLVM の CPU 名、既定のメモリ配置、QEMU のボード）は `aot/embedded.rs` の `Board` にまとめ、`Target::board()` で引く。QEMU は `mps2-an505`。コアは secure で起動するので、既定の配置は secure 側の番地（コード 0x10000000、RAM 0x38000000、各 4 MiB）。同梱（`build.rs` の `BOARD_TRIPLES`、環境変数は三つ組ごと）、`scripts/build-embedded-runtime.sh`、CI、配布物の確認も両方に。

### Cortex-M0+（未着手）

`thumbv6m-none-eabi` はコアに比較交換のアトミック命令が無く、Rust の `alloc::sync`（`Arc`）と `compare_exchange` が無い。ランタイムをビルドすると `typelisp-mem` で 40 件ほどのエラー（2026-11-23）。`typelisp_mem::sync` を、割り込みを止めて代わりにする実装（`portable-atomic` と `critical-section` など）に差し替えれば通る見込み。QEMU のボードは `microbit`（Cortex-M0）。

### 段階 7: AVR の判断

LLVM 22 には AVR のバックエンドがある。判断の材料は次の 3 つ。

- 大きさ: ATmega328P（Uno）はフラッシュ 32KB、RAM 2KB、ATmega2560（Mega）はフラッシュ 256KB、RAM 8KB。段階 0〜4 の後の Cortex-M の hello world の大きさと比べる。
- ポインタが 16bit。値を i64 の語で扱う呼び出し規約はそのまま使えるが、遅くて大きい。
- Rust の `avr-none` は tier 3 で、nightly と `-Zbuild-std` が要る。使うかどうかはユーザーが決める。

測った値（2026-11-23、release の `typl --features embedded`、`--release`、thumbv7em、semihosting）:

| プログラム | text | data | bss |
|---|---|---|---|
| `(defun main () int 42)` | 373,304 | 332 | 2,444 |
| hello（`println`） | 374,160 | 332 | 2,444 |
| fib | 379,656 | 332 | 2,444 |

`--no-cons` でも変わらない。プログラムによらず約 37 万バイトがランタイムで、大きいのは bignum（`num-bigint`）、浮動小数の印字（`flt2dec`）、印字器（`typelisp_print`）、スケジューラ、記号表・型表の初期化、ハッシュ表の型ごとの実体化。どれも 1 つは 14KB 以下で、長い尾になっている。ATmega328P（32KB/2KB）にも ATmega2560（256KB/8KB）にも、ランタイムだけで入らない（bss だけで Uno の RAM を超える）。AVR のコードは i64 の語の演算が多くの命令になるので、Thumb より大きくなる。今の設計のまま AVR に対応することはできず、対応するなら、bignum・浮動小数の印字・スケジューラを持たない別の小さなランタイムと、16bit の語の表現が要る。

### 段階 8: 使わない機能をリンクしない（2026-11-23〜）

段階 7 の測定のあと、ユーザーの指示で進める。Uno は「Lチカ程度の最小限が動くだけでも楽しい」（ユーザー）ので、最小のプログラムをどこまで小さくできるかが目標。

調べ方: `typl -c` のリンクの入力（`program.o` とスクリプト）を写し取り、release のランタイムで `--emit-relocs` を付けてリンクし直す。再配置から関数の参照のグラフを作り、根ごとに「その参照を切ったら何バイト減るか」を計算する。lld の `--why-live` は経路を 1 本しか出さないので、全体の見積もりには足りない。

最初に分かったこと（`(defun main () int 42)`、thumbv7em）: コード 373,304 バイトのうち約 24 万バイトが、プログラムが使わないのに次の根から必ず参照されていた。

- `run_program` が必ずスケジューラを通す。割り込みの入口（`DefaultHandler`）からも `drive_to_completion` 経由で届く。
- 印字器の登録が、モジュール全体（prelude を含む）が印字の shim を呼ぶかで決まっていたので、常に出ていた。そこで登録される `standard-stream` の `print-object` から、書式エンジンと pretty printer に届く。
- `main` が `int` を返すときの多倍長の検査が、エラー文のために `BigInt` を 10 進で書く。
- prelude の大域変数の初期化（`*standard-output*` など）が `rt_stream_stdout` を呼び、それが `stream_builtin` の振り分けを通ってストリーム操作を全部引き込む。

済:

- スケジューラ: `rt_suspend_yield` と `rt_suspend_main` 以外の `rt_suspend_*` に、できあがった `main` から届かない（`externs::needs_scheduler`）なら、入口を `rt_run_program_alone[_int]` に差し替える（`build_main_wrapper` の最後）。初期化と `main` をタスクと同じ鎖（`FrameStack::for_task`）でこのフレームの上で走らせ、yield、safepoint、インタプリタのスレッドへの移動はその場で続ける（`FrameStack::run_to_end_alone`）。`run_to_end` は答え方を引数に取るループ（`run_to_end_answering`）の上に置き直した。
- 割り込み: ハンドラを走らせる関数は、`rt_interrupt_register` が登録する関数ポインタ（`TYPELISP_INTERRUPT_RUN`）で呼ぶ。外から見える記号にしてあるのは、書き込まれる値が 1 つだけの内部の static だと、LLVM が間接呼び出しを直接呼び出しに戻すため（実際に戻った）。
- 印字器の登録: 名前の書き方（モジュールと import）は常に登録する（実行時エラーの文も使う）。印字器の制御変数は、できあがった `main` から `rt_fmt_*` か `rt_pprint*` に届くときだけ。`print-object`、列挙子の名前、フィールドは、値を書く shim（`PRINT_SHIMS`）に届くときだけ。
- 多倍長: 多倍長の箱を作るのは `Heap::box_bignum` の 1 か所にし、そこで 10 進で書く関数を登録する（`TYPELISP_BIGNUM_TEXT`）。`Heap::bignum_text` はそれを呼ぶ。多倍長を作れないプログラムには、10 進で書くコードが入らない。
- ストリーム: 上の段階 0 の項。

測定（thumbv7em、release のランタイム、コード）:

| 時点 | `(defun main () int 42)` |
|---|---|
| 前 | 373,304 |
| スケジューラ | 328,704 |
| 印字器の登録 | 227,204 |
| 多倍長のエラー文 | 182,824 |
| ストリームの振り分け | 150,084 |

振る舞いの変わったところ:

- WebAssembly のモジュールは、スケジューラの無いプログラムでは `typelisp.now` と `typelisp.sleep` を import しない（`aot_wasm_js_test`）。
- ネイティブの実行ファイルは、タスクの無いプログラムでは `TYPELISP_THREADS` を読まない（ワーカーのスレッドを持たないので）。不正な値を断るのはタスクのあるプログラムだけ（`os_threads_test`）。

番人: `aot_embedded_test` の `what_a_program_does_not_use_is_left_out`。`int` を返すだけのプログラムにスケジューラ・書式エンジン・pretty printer・`num_bigint` のコードが無く、それらを使うプログラムには有ることを、実行ファイルの記号で確かめる。

残り（150,084 バイトの時点、単独で切ったときの減り方）: 記号表の初期化（`SymTable::new`、約 22KB）、名前の登録（約 14KB）、prelude の大域変数の初期化（約 13KB）、型表の初期化（約 11KB）。あとは記号表・型表・ヒープ・ハッシュ表の土台が細かく並ぶ。

