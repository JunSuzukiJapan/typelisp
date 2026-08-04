# typelisp 開発 TODO

最終更新: 2026-08-04 / ブランチ: `main`

このドキュメントは**現在残っている作業のみ**を記録する。

## 残っている作業

- **使われないブランケット実装の本体が型検査されない**（Rust は先行検査する）。
  受信型が未知の本体を検査する手段が無く、ジェネリック `defun` の本体が
  `Expr::TraitCall` プレースホルダ経由でしか診断されないのと同じ理由。
- **`match` の腕から `Result` の誤差型が推論されない**。`(result::ok v)` 単独では `E` が
  決まらず、期待型の無い `match` では兄弟の `err` 腕からも回復されない。prelude は戻り型を
  宣言した `io-ok` ヘルパで固定している。腕どうしを単一化すれば済むはず。
- **ストリームの未実装分**。`fresh-line` は `file-stream` 専用（列位置を追うのは
  ネイティブ backed のストリームだけ）、`read` のストリーム版とストリーム宛 `format` は
  未提供（`(write-string s (format false ...))` で書ける）、pathname 層は無い
  （ファイルは文字列で指す）。

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
