# typelisp 開発 TODO

最終更新: 2026-08-04 / ブランチ: `main`

このドキュメントは**現在残っている作業のみ**を記録する。

## 残っている作業

- **`labels`/`lambda` の本体で作った enum 値の変種タグが壊れる**。5行で再現する:

  ```lisp
  (defenum expr (num i32) (add expr expr))
  (use expr)
  (defun mk () expr (labels ((f ((n i32)) expr (num n))) (f 5)))
  (println "~a" (mk))                        ; => (<unknown-variant> 5)
  (match (mk) ((num v) v) ((add a b) -1))    ; => internal error: no matching match arm
  ```

  同じ `(num n)` をトップレベル `defun` の直下で書けば正しい。ネストした関数の本体は
  compile 経路(`compile_function_rec`)を通るので、compiled 側の construct が変種の
  型IDを取り違えている疑いが濃い(`typelisp-interp-closure-removal` で直した
  op-id/f64 リテラルのタグ切り詰めと同種)。**値が黙って壊れる**ので優先度は高い。
  `examples/projects/expr-eval` はこれで実行時に落ちる(parser が `labels` の中で
  構文木を組み立てるため)。2026-08-04 に発見、少なくとも 4a65d95 の時点で存在。
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
