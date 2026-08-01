# typelisp 開発 TODO

最終更新: 2026-07-30 / ブランチ: `main`

このドキュメントは**現在残っている作業のみ**を記録する。

## 残っている作業

**無し。**

作業を始めるときはここに項目を足し、終わったら（経緯・設計判断を
[implementation-log.md](implementation-log.md) へ書いたうえで）ここから消す。

## 関連ドキュメント

| 知りたいこと | 参照先 |
|---|---|
| 完了した実装の経緯・設計判断 | [implementation-log.md](implementation-log.md) |
| 言語仕様の確定事項・非採用と決めた機能 | [language-design.md](language-design.md)（非採用リストは §9） |
| Common Lisp と比べてまだ無いクラス・メソッド | [cl-missing-classes-and-methods.md](cl-missing-classes-and-methods.md) |
| ビルド・テストの実行方法 | [development.md](development.md) |
| ユーザ向けの関数・構文リファレンス | [functions.md](../functions.md) / [syntax.md](../syntax.md) |

## 残っている作業

- **非最左スーパトレイトへの `:dyn` アップキャスト**（2026-08-01 の対象外）。`D(B,C)` の
  `:dyn D` を `:dyn C` へ。現状は誤ディスパッチせず型エラーで拒否する。実現には
  「vtable id → スーパトレイト → vtable id」の変換表を `Interp` と `typelisp-rt` の両方に持たせ、
  島（`compiler.rs` の `SOURCE`）に `dyn-upcast` タグを足して `compiler_island.bc` を
  再生成する必要がある。詳細は [language-design.md](language-design.md) §5.2「対象外（v1）」。
- **使われないブランケット実装の本体が型検査されない**（Rust は先行検査する）。
  受信型が未知の本体を検査する手段が無く、ジェネリック `defun` の本体が
  `Expr::TraitCall` プレースホルダ経由でしか診断されないのと同じ理由。

次に何かを実装するなら、着手候補の地図は
[cl-missing-classes-and-methods.md](cl-missing-classes-and-methods.md) §3（横断的な欠落）。
ただしあれは TODO ではなく残差の測定であり、優先度も筆者の見立てであって確定した方針ではない。
