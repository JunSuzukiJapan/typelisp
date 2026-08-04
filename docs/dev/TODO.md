# typelisp 開発 TODO

最終更新: 2026-08-04 / ブランチ: `main`

このドキュメントは**現在残っている作業のみ**を記録する。

## 残っている作業

- **モジュール内でユーザ型が組み込み `vector`/`hashtable` を名乗れてしまい、compiled 経路が
  誤認する**。組み込み名の再定義は root では拒否されるが、`(module m (defstruct vector ...))`
  は通る。`ast_bridge` はメソッド呼び出しの振り替えを `type_name.local() == "vector"` /
  `"hashtable"` で判定しているので、compiled 側だけがユーザ型を組み込みと誤認する:

  ```lisp
  (module m
    (pub defstruct vector (a i32))
    (pub defmethod len ((self vector)) i32 (* 100 self::a))
    (pub defun call-len ((v vector)) i32 (len v)))
  (m::call-len (m::vector::new 3))            ; => 300
  (compile m::call-len)
  (m::call-len (m::vector::new 3))            ; => 1  (組み込みのフィールド数)
  ```

  `hashtable` を名乗った場合は `BoxId does not hold a HashTable` で**プロセスが abort** する。
  最小の直し方は分類側に `Path::is_simple()`(=root) を足すこと(組み込みの受け側は常に root)。
  そもそもモジュール内で組み込み型名を再定義させない、という直し方もある。2026-08-04 発見。
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
