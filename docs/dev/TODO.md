# typelisp 開発 TODO

最終更新: 2026-09-06 / ブランチ: `main`

このドキュメントは**現在残っている作業のみ**を記録する。終わった作業は
[completed-work.md](completed-work.md)（何がどこまで進んだかの横断的な要約）と
[implementation-log.md](implementation-log.md)（作業 1 件ごとの経緯・設計判断）へ移す。

## 残っている作業

いまは無い。直近に入ったのは C FFI（`defffi` / `unsafe`、2026-09-06）で、これも残作業を
1 つも足していない。それ以前の分も含め、片付いたものの一覧は
[completed-work.md](completed-work.md)。

作業を始めるときはここに項目を足し、終わったら（経緯・設計判断を
[implementation-log.md](implementation-log.md) へ書いたうえで）ここから消す。

残差そのものの地図は
[cl-missing-classes-and-methods.md](cl-missing-classes-and-methods.md)（TODO ではなく測定）。
そこから作った実行計画が [cl-parity-plan.md](cl-parity-plan.md)（2026-09-05 に全 Phase 完了）で、
地図の全行がどの Phase に落ちたか（落ちていないなら理由）は同計画の付録 A にある。

## 見つかっている実装の穴

いまは無い。直近まであった「関連型が総称名の内側にあると `impl` の置換が届かない」は
2026-09-05 に解消（経緯は [implementation-log.md](implementation-log.md) の
「関連型が総称名の内側にある場合」）。

## 関連ドキュメント

| 知りたいこと | 参照先 |
|---|---|
| 片付いた作業の一覧・横断的な教訓 | [completed-work.md](completed-work.md) |
| 完了した実装の経緯・設計判断 | [implementation-log.md](implementation-log.md) |
| 言語仕様の確定事項・非採用と決めた機能 | [language-design.md](language-design.md)（非採用リストは §9） |
| Common Lisp と比べてまだ無いクラス・メソッド | [cl-missing-classes-and-methods.md](cl-missing-classes-and-methods.md) |
| それを埋める実行計画（Phase / 対象外の理由 / 完了判定） | [cl-parity-plan.md](cl-parity-plan.md) |
| ビルド・テストの実行方法 | [development.md](development.md) |
| ユーザ向けの関数・構文リファレンス | [functions.md](../functions.md) / [syntax.md](../syntax.md) |
