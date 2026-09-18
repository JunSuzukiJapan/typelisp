# typelisp 開発 TODO

最終更新: 2026-09-18 / ブランチ: `feature/aot-scheduler`

このドキュメントは**現在残っている作業のみ**を記録する。終わった作業は
[completed-work.md](completed-work.md)（何がどこまで進んだかの横断的な要約）と
[implementation-log.md](implementation-log.md)（作業 1 件ごとの経緯・設計判断）へ移す。

## 残っている作業

| 作業 | 中身 |
|---|---|
| エディタのキーワード表にソケット層の名前を足す | `tests/editor_keyword_sync_test.rs` の `vscode_grammar_knows_every_registry_name` / `emacs_mode_knows_every_registry_name` が赤（2026-09-18 時点で 78 名が未登録）。`editor/vscode/syntaxes/typelisp.tmLanguage.json` と `editor/emacs/typelisp-mode.el` はネットワーク層（TCP/TLS/Unix/UDP/DNS）より前から無変更。テストが未登録の名前を列挙する。ユーザ向けの名前（`tcp-connect`・`tls-listen`・`with-connection` など）は 2 ファイルに足し、ユーザ向けでない名前（prelude の糊が呼ぶ `net-*` の内部ビルトインなど）は同テストの `is_excluded` に分類で足す |

**軽量スレッド（goroutine 相当）は完了した。** プランは
`~/.claude/plans/go-gorutine-adaptive-raccoon.md`。Phase A（評価器の CPS 化）、
B1/B2（スケジューラ・`go`/`Task<T>`/`wait`/`yield`/`sleep`）、B3〜B5
（`Chan<T>`・`select`・`WaitGroup`/`Mutex<T>`/`with-lock`）、C0〜C7
（コンパイル出力の一様コルーチン化）がすべて入っている。経緯は
[implementation-log.md](implementation-log.md) の 3 つの節。

**AOT 実行ファイルにもスケジューラが載った（2026-09-17）。** プランは
`~/.claude/plans/aot-os-quizzical-graham.md`。スケジューラの核は
`typelisp-rt::sched` の `Scheduler<B: TaskBody>` で、`compile-file` した実行ファイルの
`go`・`sleep`・`wait`・`Chan`・`select`・ソケット待ちが `typl` と同じ意味で動く。
その作業が「触らない」と決めて残した制限（`eval` 入り AOT でスケジューラが 2 つ並ぶ、
printer の door 内では `go`/チャネル操作が fatal、インタプリタが compiled クロージャを
`funcall` すると中断できない、`compile-file` が `use` を受理しない）は
[implementation-log.md](implementation-log.md) の同節の末尾にある。

プランが残した制限で**まだ残っているもの**は 2 つあり、どちらも v1 の範囲として
意図的に受け入れたもの：

| 制限 | 中身 |
|---|---|
| マルチコア並列が無い | `Heap` は `!Send` で `ACTIVE_HEAP` は 1 つの thread_local。ヒープを共有可能にする作業は並行機構本体より大きい（プラン B6' に 3 案の比較がある） |
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
| 評価器を CPS 化した設計（Phase A） | [cps-evaluator-design.md](cps-evaluator-design.md) |
| 並行機構のユーザ向けリファレンス | [syntax.md §12](../syntax.md)（`go`/`select`）と [functions.md §20](../functions.md)（型・メソッド） |
| コンパイル出力のコルーチン ABI（Phase C、C0〜C7） | [compiled-cps-design.md](compiled-cps-design.md) |
| Common Lisp と比べてまだ無いクラス・メソッド | [cl-missing-classes-and-methods.md](cl-missing-classes-and-methods.md) |
| それを埋める実行計画（Phase / 対象外の理由 / 完了判定） | [cl-parity-plan.md](cl-parity-plan.md) |
| ビルド・テストの実行方法 | [development.md](development.md) |
| ユーザ向けの関数・構文リファレンス | [functions.md](../functions.md) / [syntax.md](../syntax.md) |
