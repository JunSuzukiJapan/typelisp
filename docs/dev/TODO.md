# typelisp 開発 TODO

最終更新: 2026-08-17 / ブランチ: `feature/compiled-unwind`

このドキュメントは**現在残っている作業のみ**を記録する。

## 残っている作業

### prelude ビットコードの起動時コスト（+198〜258 ms）

417 KB のビットコード全体を MCJIT が起動時に解決する分（冗長パースの除去で削れる分は既に
削ってあり、残りは解決そのもの）。実行速度とのトレードオフとして現状は常時有効（ユーザー判断）。

2026-08-14 の「コンパイル経路の穴」を塞ぐ作業で成果物は 244 KB → 417 KB になり、コストもほぼ
比例して増えた（`scripts/bench-prelude.sh`、N=3）:

| | 成果物 | インストール | 空ファイルに対する `typl` 全体（release, N=5） |
|---|---|---|---|
| 穴を塞ぐ前 | 244 KB | +105〜109 ms | 0.84 s |
| 現在 | 417 KB | +198〜258 ms | 0.98 s |

遅延インストール——呼ばれた関数だけ解決する——で削るなら、まず `scripts/bench-prelude.sh` で
測り直してから。

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
