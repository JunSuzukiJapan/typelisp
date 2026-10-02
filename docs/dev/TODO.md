# typelisp 開発 TODO

最終更新: 2026-10-02 / ブランチ: `feat/ci`

このドキュメントは**現在残っている作業のみ**を記録する。終わった作業は
[completed-work.md](completed-work.md)（何がどこまで進んだかの横断的な要約）と
[implementation-log.md](implementation-log.md)（作業 1 件ごとの経緯・設計判断）へ移す。

## 残っている作業

**CI/CD（作業中、ブランチ `feat/ci`）。** push/PR ごとにビルドと警告 0 を、
リリース（`v*` タグ）のときだけテスト全体を回す。対象は macOS と Linux
（Ubuntu 24.04・Debian 13・Fedora 44 でテスト全体が通ることを 2026-10-02 に確認済み）。
Arch は対象外（2026-10-02 決定）：公式リポジトリの `llvm` が 23 で、LLVM 22 は
実行時ライブラリ（`llvm22-libs`）しか無く `llvm-config`・ヘッダが無い。LLVM 23 へ移る
ときに改めて考える。crates.io への公開は手動のまま。

**JIT を MCJIT から ORC（LLJIT）へ移す（2026-10-02 決定、未着手）。** 根は MCJIT の
2 つの性質：渡していない名前をプロセス内で探す（探し方は OS ごとに違い、macOS では
先頭の `_` を 1 つ外す）こと、見つからなくてもエラーにせず番地 0 を埋めること。
`compile::jit_engine` はモジュールに宣言がある名前の渡し忘れを断るが、コード生成が自分で
足す呼び出し（`memcpy`、`__divti3` などの補助関数、`_Unwind_Resume`）は宣言が無いので
捕まえられない。Apple Silicon では、Mach-O の FDE の番地を RuntimeDyld が二重に補正して
panic が JIT のフレームを越えられない（`failed to initiate panic, error 5`）不具合も
あり、`jit_engine` の `jit_as_elf` がモジュールを ELF として読ませて避けている。その代償
として、名前が `_` で始まるライブラリ関数はプロセス内検索で見つからない
（`_Unwind_Resume` だけは手で番地を渡している。ほかが現れれば黙って 0 を呼ぶ）。
さらに macOS 15 の libunwind は、MCJIT が `__register_frame` で登録した FDE で landing
pad に入ると落ちるので、`compile::jit_unwind` が独自のメモリマネージャを持ち、
`__unw_add_find_dynamic_unwind_sections` で JIT のコードの `.eh_frame` を libunwind に
直接教えている（MCJIT はメモリマネージャの失敗を捨てるので、`jit_engine` がコード生成を
前倒しして失敗を拾う）。

ORC なら、渡した名前（`LLVMOrcAbsoluteSymbols`）以外は解決せず、見つからない名前は
必ずエラーで返し、プロセス内を探すのは明示的に足したときだけ（`_` の扱いはデータ
レイアウトから決まる）。arm64 の Mach-O は JITLink で読むので RuntimeDyld の不具合を
通らず、ELF の回避策も要らなくなるはず。必要な C API は `llvm-sys` 221.1.0 にある
（inkwell は ORC を持たないので JIT の部分は `llvm-sys` を直接使う）。

進め方は 2 段階：(1) 試作 — `tests/compiled_unwind_test.rs` と同じ最小のモジュールを
ORC + JITLink で動かし、panic がフレームを越えるか・渡していない名前がエラーになるかを
Intel の macOS、Linux、arm64 で確かめる。未確認で最大の不確定要素は、C API で作った
JITLink の層が `.eh_frame` を登録するか。駄目なら MCJIT に戻る。(2) `jit_engine` と
`CompiledFn`（エンジンの寿命、`retire_llvm`）を移し、`jit_as_elf` と `_Unwind_Resume`
の手渡しと `compile::jit_unwind` を外し、全環境でテスト全体を回す。`jit_unwind` を外せる
かは、JITLink が登録する `.eh_frame` で macOS 15 の arm64 の landing pad に入れるかで
決まるので、(1) の試作で macOS 15 も確かめる。

**軽量スレッド（タスク）は完了した。** プランは
`~/.claude/plans/go-gorutine-adaptive-raccoon.md`。Phase A（評価器の CPS 化）、
B1/B2（スケジューラ・`task`/`Task<T>`/`wait`/`yield`/`sleep`）、B3〜B5
（`Chan<T>`・`select`・`WaitGroup`/`Mutex<T>`/`with-lock`）、C0〜C7
（コンパイル出力の一様コルーチン化）がすべて入っている。経緯は
[implementation-log.md](implementation-log.md) の 3 つの節。

**AOT 実行ファイルにもスケジューラが載った（2026-09-17）、残した制限 4 つも
塞いだ（2026-09-18）。** プランは `~/.claude/plans/aot-os-quizzical-graham.md`
（2026-09-17 分）と `~/.claude/plans/buzzing-sleeping-pelican.md`（2026-09-18 分）。
スケジューラの核は `typelisp-rt::sched` の `Scheduler<B: TaskBody>` で、
`compile-file` した実行ファイルの `task`・`sleep`・`wait`・`Chan`・`select`・
ソケット待ちが `typl` と同じ意味で動く。`eval` 入り AOT でスケジューラが 2 つ並ぶ
（`eval` 内の `task` が次の `rt_eval` まで走らない）・printer の door 内で `task`/
チャネル操作が fatal・インタプリタが compiled クロージャを `funcall` すると
中断できない・`compile-file` が `use` を受理しない、の 4 つは 2026-09-18 に解消——
機械フレームの上のドライバ（`FrameStack::run_to_end`）が駆動中のスケジューラに
「今答えられる中断か」を尋ねられるようになったのが核（`sched::answer_now`）。
経緯は [implementation-log.md](implementation-log.md) の 2 つの節
（2026-09-17 分と 2026-09-18 分）。

**タスクが複数の OS スレッドで同時に走るようになった（2026-09-18〜09-24）。**
プランは `~/.claude/plans/goroutine-os-goroutine-os-twinkly-thacker.md`、設計と
計画からの差分は [os-threads-design.md](os-threads-design.md)。`Heap` を「スレッドごとの
ビュー + `Arc<HeapShared>`」に分け、stop-the-world GC を入れ、スケジューラを
`TYPELISP_THREADS` 本（main 込み）で回す。AOT と `typl` の両方。専用 OS スレッドの
`thread`/`Thread<T>`/`join` も入った。経緯は [implementation-log.md](implementation-log.md)
の 2026-09-24 の節。

**意図的に受け入れている制限**（ユーザに見えるものは [syntax.md](../ja/reference/syntax.md) §12.7 にも
書いてある）:

| 制限 | 中身 |
|---|---|
| データ競合は未定義 | Go と同じ立場。`Mutex<T>`/`Chan<T>` を通さずに複数タスクから同じ値を書き換えた結果は保証しない（`Value` の torn write、`Vector` の push や `HashTable` の同時変更は Rust の UB になりうる）。ランタイム内部の共有表だけがロックで守られる（os-threads-design.md §6） |
| `typl` で interpreted に触れたタスクは以後インタプリタのスレッドに固定 | インタプリタ（`Interp`）は `Rc`/`RefCell` で main スレッド専用。ワーカー上の compiled タスクが interpreted な関数値の apply・コンパイルされていない `:dyn` メソッド・`eval`/`macroexpand`/`read` に当たると `NeedsMain` で main へ移送され、戻らない（§8「実装（Phase 5）」） |
| main 以外のスレッドでは interpreted な `print-object`/`format` の `~/name/` を走らせられない | ワーカー・専用スレッドがそういう値を印字すると catchable な panic。`(compile T::print-object)` するか main から印字する。`thread` の推移的コンパイルは印字メソッドを対象に含めない |
| `typl` のワーカーはトップレベルの評価 1 回ぶんだけ生きる | drive の終わりに各スレッドの「今の 1 歩」を待って止める。`thread` の中でブロックし続ける C 関数（`defffi`）があると、その呼び出しが返るまでトップレベルの評価が終わらない。AOT のワーカーは常駐 |
| C の FFI コールバックの中では中断できない | C は「呼んだら結果が返る」しか知らない。Phase C で「compiled では中断できない」からここまで縮んだ |

作業を始めるときはここに項目を足し、終わったら（経緯・設計判断を
[implementation-log.md](implementation-log.md) へ書いたうえで）ここから消す。

残差そのものの地図は
[cl-missing-classes-and-methods.md](cl-missing-classes-and-methods.md)（TODO ではなく測定）。
そこから作った実行計画が [cl-parity-plan.md](cl-parity-plan.md)（2026-09-05 に全 Phase 完了）で、
地図の全行がどの Phase に落ちたか（落ちていないなら理由）は同計画の付録 A にある。

## 関連ドキュメント

| 知りたいこと | 参照先 |
|---|---|
| 片付いた作業の一覧・横断的な教訓 | [completed-work.md](completed-work.md) |
| 完了した実装の経緯・設計判断 | [implementation-log.md](implementation-log.md) |
| 言語仕様の確定事項・非採用と決めた機能 | [language-design.md](language-design.md)（非採用リストは §9） |
| タスクを OS スレッドで走らせる設計（ヒープ 2 層・STW GC・スケジューラ・`Thread<T>`） | [os-threads-design.md](os-threads-design.md) |
| 評価器を CPS 化した設計（Phase A） | [cps-evaluator-design.md](cps-evaluator-design.md) |
| 並行機構のユーザ向けリファレンス | [syntax.md](../ja/reference/syntax.md) §12（`task`/`select`）と [concurrency.md](../ja/reference/functions/concurrency.md)（型・メソッド） |
| コンパイル出力のコルーチン ABI（Phase C、C0〜C7） | [compiled-cps-design.md](compiled-cps-design.md) |
| Common Lisp と比べてまだ無いクラス・メソッド | [cl-missing-classes-and-methods.md](cl-missing-classes-and-methods.md) |
| それを埋める実行計画（Phase / 対象外の理由 / 完了判定） | [cl-parity-plan.md](cl-parity-plan.md) |
| ビルド・テストの実行方法 | [development.md](development.md) |
| ユーザ向けの関数・構文リファレンス | [functions/](../ja/reference/functions/README.md) / [syntax.md](../ja/reference/syntax.md) |
