# typelisp 開発 TODO

最終更新: 2026-08-07 / ブランチ: `feature/cons-cell-interpreter`

このドキュメントは**現在残っている作業のみ**を記録する。

## 残っている作業

### fasl の速度に本物の番人を置く

fasl は**速度のためだけに**存在する。正しさは round-trip テスト
（`tests/fasl_test.rs` の S1〜S4）が見ているので、fasl が遅くなっても
他のテストは全部通る。つまり今、この機能の存在理由を守っているものが無い。

以前は `bench_fasl_load_beats_source_load` があったが 2026-08-07 に削除した。
番人として成立していなかったため:

- `#[ignore]` なので自動では走らない。手で `--ignored` を打った時だけ
- アサーションが `fasl_time < source`、つまり「少しでも速い」。1.01 倍まで
  劣化しても通るので、意味のある形では落ちない
- prelude をループで読む時間を測っていたが、実際に効く場所は LSP の診断パス
  （そこは 5.2 倍と実測済み）。測る対象が違う

作るなら:

- 対象は **LSP 診断パス**（`typl-lisp` の per-pass、fasl 導入の動機そのもの）
- 閾値は**実比に対して余裕のある倍率**（実測 5.2 倍なら 3 倍など）。
  タイミング assertion は負荷のかかったマシンで揺れるので、
  「速いこと」ではなく「桁が違うこと」を主張する形にする
- **スイートで回す**（`#[ignore]` にしない）。走らない番人は番人ではない

なお Phase 2 で fasl は cons 直列化に作り替わり `FASL_FORMAT_VERSION` が
18→19 に上がる（`~/.claude/plans/lisp-lisp-ast-cons-vivid-galaxy.md`）ので、
着手はその後が素直。

作業を始めるときはここに項目を足し、終わったら（経緯・設計判断を
[implementation-log.md](implementation-log.md) へ書いたうえで）ここから消す。

次に何かを実装するなら、着手候補の地図は
[cl-missing-classes-and-methods.md](cl-missing-classes-and-methods.md) §3（横断的な欠落）。
ただしあれは TODO ではなく残差の測定であり、優先度も筆者の見立てであって確定した方針ではない。

## 関連ドキュメント

| 知りたいこと | 参照先 |
|---|---|
| 完了した実装の経緯・設計判断 | [implementation-log.md](implementation-log.md) |
| 言語仕様の確定事項・非採用と決めた機能 | [language-design.md](language-design.md)（非採用リストは §9） |
| Common Lisp と比べてまだ無いクラス・メソッド | [cl-missing-classes-and-methods.md](cl-missing-classes-and-methods.md) |
| ビルド・テストの実行方法 | [development.md](development.md) |
| ユーザ向けの関数・構文リファレンス | [functions.md](../functions.md) / [syntax.md](../syntax.md) |
