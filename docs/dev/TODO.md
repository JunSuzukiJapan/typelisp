# typelisp 開発 TODO

最終更新: 2026-08-20 / ブランチ: `feature/typelisp-dump`

このドキュメントは**現在残っている作業のみ**を記録する。

## 残っている作業

### ダンプ（ビットコード + 型情報を対にする）

コンパイラが本体を作る場所では型情報も一緒に作り、その対を 1 ファイルに書く。
prelude と島は移行済み（起動 1.50s → 1.07s）。残り:

- `compile-file` の eval 環境を `snapshot.rs` からダンプ単位へ寄せる（front 側に置いた
  prelude 単位を再利用して、実行ファイルから ~0.9MB 削る）
- ユーザから見える `(dump path)` と、そこから起動する `typl --image`
- 後片付け: `embed_source_hash`/`read_embedded_source_hash`/`CompiledLibrary::expected_hash`、
  `snapshot.rs`、regen スクリプトの説明

**測ってわかった別件**: 起動 1.50s の内訳は read+check が 597ms、**ビットコードの
install（LLVM のパース + JIT）が 781ms**。ダンプが消すのは前者だけなので、これ以上
起動を詰めるなら install の側（遅延マテリアライズ、あるいは prelude/島を typl 本体へ
AOT リンクする）が次の相手になる。いまは着手しない。

2026-08-19 に「呼ぶとコンパイルできなくなるもの」（[syntax.md](../syntax.md) §10）の最後の1つ
`eval` を閉じ、あの表は空になった。経緯・設計判断は
[implementation-log.md](implementation-log.md) の該当節。

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
