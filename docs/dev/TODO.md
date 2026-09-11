# typelisp 開発 TODO

最終更新: 2026-09-12 / ブランチ: `feature/compiled-cps`

このドキュメントは**現在残っている作業のみ**を記録する。終わった作業は
[completed-work.md](completed-work.md)（何がどこまで進んだかの横断的な要約）と
[implementation-log.md](implementation-log.md)（作業 1 件ごとの経緯・設計判断）へ移す。

## 残っている作業

**軽量スレッド（goroutine 相当）の Phase B、残り 3 段。** プランは
`~/.claude/plans/go-gorutine-adaptive-raccoon.md`。Phase A（評価器の CPS 化）と
Phase C（コンパイル出力の一様コルーチン化、C0〜C7）は完了し、B も B1/B2
（スケジューラ・`go`/`Task<T>`/`wait`/`yield`/`sleep`）は入っている。

| 段 | 中身 |
|---|---|
| **B3** | `Chan<T>`（`send`/`recv`/`close`/`len`/`cap`）と `(impl Iter Chan<T>)`。`doiter` がそのまま回る |
| **B4** | `select`（多重待ち、`else` 省略時はブロック、同時可能なら一様ランダム） |
| **B5** | `WaitGroup` / `Mutex<T>` / `with-lock` |

**プランが B3 の宿題に挙げていた衝突は Phase C で消えている。** 「`after` は prelude で
`sleep` と `send` を使うが、どちらもタスクを中断するので compile できない」という問題で、
C3 が compiled からの中断を通したので `PRELUDE_COMPILE_UNSUPPORTED` を空のまま書ける。

**`Chan<T>` の未使用型引数も確認済み**（プランの検証節）。`(defstruct chan<T> (h i32))` は
定義でき、`the` で型が決まり、`Vector<chan<i32>>` にも入る。ファントムフィールドは要らない。
ただし**効くのはメソッド形式だけ**——自由関数の戻り型からは型引数が推論されない。

作業を始めるときはここに項目を足し、終わったら（経緯・設計判断を
[implementation-log.md](implementation-log.md) へ書いたうえで）ここから消す。

残差そのものの地図は
[cl-missing-classes-and-methods.md](cl-missing-classes-and-methods.md)（TODO ではなく測定）。
そこから作った実行計画が [cl-parity-plan.md](cl-parity-plan.md)（2026-09-05 に全 Phase 完了）で、
地図の全行がどの Phase に落ちたか（落ちていないなら理由）は同計画の付録 A にある。

## 見つかっている実装の穴

**並行機構のユーザ向けリファレンスが無い。** `go` / `Task<T>` / `wait` / `yield` と
タスクを意識した `sleep` は実装済みだが、[functions.md](../functions.md) にも
[syntax.md](../syntax.md) にも項目が無い。語彙が B3〜B5 で増えるので、そこまで
入れてからまとめて書くほうが自然。

## 関連ドキュメント

| 知りたいこと | 参照先 |
|---|---|
| 片付いた作業の一覧・横断的な教訓 | [completed-work.md](completed-work.md) |
| 完了した実装の経緯・設計判断 | [implementation-log.md](implementation-log.md) |
| 言語仕様の確定事項・非採用と決めた機能 | [language-design.md](language-design.md)（非採用リストは §9） |
| 評価器を CPS 化した設計（Phase A） | [cps-evaluator-design.md](cps-evaluator-design.md) |
| コンパイル出力のコルーチン ABI（Phase C、C0〜C7） | [compiled-cps-design.md](compiled-cps-design.md) |
| Common Lisp と比べてまだ無いクラス・メソッド | [cl-missing-classes-and-methods.md](cl-missing-classes-and-methods.md) |
| それを埋める実行計画（Phase / 対象外の理由 / 完了判定） | [cl-parity-plan.md](cl-parity-plan.md) |
| ビルド・テストの実行方法 | [development.md](development.md) |
| ユーザ向けの関数・構文リファレンス | [functions.md](../functions.md) / [syntax.md](../syntax.md) |
