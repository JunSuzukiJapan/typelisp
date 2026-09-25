# typelisp 開発 TODO

最終更新: 2026-09-25 / ブランチ: `feature/os-threads`

このドキュメントは**現在残っている作業のみ**を記録する。終わった作業は
[completed-work.md](completed-work.md)（何がどこまで進んだかの横断的な要約）と
[implementation-log.md](implementation-log.md)（作業 1 件ごとの経緯・設計判断）へ移す。

## 残っている作業

今は無い。

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

**意図的に受け入れている制限**（ユーザに見えるものは [syntax.md §12.6](../syntax.md) にも
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
| 並行機構のユーザ向けリファレンス | [syntax.md §12](../syntax.md)（`task`/`select`）と [functions.md §20](../functions.md)（型・メソッド） |
| コンパイル出力のコルーチン ABI（Phase C、C0〜C7） | [compiled-cps-design.md](compiled-cps-design.md) |
| Common Lisp と比べてまだ無いクラス・メソッド | [cl-missing-classes-and-methods.md](cl-missing-classes-and-methods.md) |
| それを埋める実行計画（Phase / 対象外の理由 / 完了判定） | [cl-parity-plan.md](cl-parity-plan.md) |
| ビルド・テストの実行方法 | [development.md](development.md) |
| ユーザ向けの関数・構文リファレンス | [functions.md](../functions.md) / [syntax.md](../syntax.md) |
