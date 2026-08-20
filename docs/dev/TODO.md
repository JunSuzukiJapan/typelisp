# typelisp 開発 TODO

最終更新: 2026-08-20 / ブランチ: `feature/cl-parity`

このドキュメントは**現在残っている作業のみ**を記録する。

## 残っている作業

**CL 残差を埋める実装計画**（[cl-parity-plan.md](cl-parity-plan.md)、2026-08-20 策定）。
Common Lisp にあって typelisp に無いものを Phase 0〜9 に落とした計画。
着手順の目安は同計画の「実施順序と依存」節。

進捗:

| Phase | 状態 |
|---|---|
| 0（実装前に確かめる 6 件） | **完了** 2026-08-20。結果と、それが計画本体に強いた訂正は同計画の該当節 |
| 1（数値層 1a〜1d） | **1c 完了** 2026-08-20（2 項目保留: 乱数のシード指定と `ldb` 系の幅拡張）。1a/1b/1d 未着手——1a は島の分岐を増やす作業なので、Phase 2 が残した「char/int の native lowering 5 つ」とまとめると安い |
| 2（文字・文字列 2a/2b） | **完了** 2026-08-20 |
| 3（リスト・シーケンス 3a〜3d） | **3a/3b/3c/3d 完了** 2026-08-20。残るは 3e（`:key`/`:test` 等のキーワード引数、Phase 5b 依存） |
| 4（制御構造・マクロ） | **4a 部分完了** 2026-08-20（`prog1`/`prog2`/`do*`/`ecase`/`ccase`/`setq`/`psetq`/`psetf`/`pushnew`）。残るは `block`/`return-from`（島の引数引き回しに全面的に触る）・`prog`/`prog*`・`destructuring-bind`・`sleep`。4b/4c 未着手 |
| 5〜8 | 未着手 |
| 9（シンボル・パッケージ・環境） | **9c 完了** 2026-08-20（コマンドライン引数・環境変数・ファイルシステム問い合わせ・日時の分解合成・`y-or-n-p`）。保留は `libc` が要る 4 群と REPL ツール層。9a/9b/9d 未着手 |
| 付録 C（小さな不整合 4 件） | **完了** 2026-08-20 |
| 付録 D（範囲外の既存問題 2 件） | **未着手**。どちらも main で再現。設計判断が要るので直していない |

Phase 2/3 の副産物として checker のバグを 4 件見つけて直した。4 件とも
**「型変数の名前がたまたま一致したときだけ動いていた」同じ形**（詳細は同計画の Phase 0 / Phase 3 の節）:
境界越しの `Self` 戻り型、境界付きジェネリック同士の委譲、ジェネリック `defmethod` の受け手の
型パラメータ名、そして compiled 経路で `format` が `f64` パラメータを生ワードのまま渡していた件。

Phase 9c の作業中に、**この計画の範囲外の既存問題**を 2 件見つけた（同計画の付録 D）。
どちらも main で再現し、どちらも設計判断が要るので直していない:

1. **AOT 実行ファイルから prelude の関数が一切呼べない**。`aot::compile_file` が
   `load_prelude` を呼ばないため、`abs`/`gcd`/`identity` すら `no such function` になる。
   この計画で足したものはほぼ全て prelude にあるので、影響は小さくない。
   JIT（`(compile name)`）は無関係で、そちらは通る。
2. **小さいヒープを渡すと GC がスラッシュする**。`gc()` が 1 セルでも回収すると `grow()`
   が呼ばれないので、生存量が容量に近いと毎回全体マークが走る。
   `editor_keyword_sync_test` が 6 テストで 650 秒かかっているのがこれ。

2026-08-20 に「ダンプ」（ビットコードと型情報を 1 ファイルに対で持つ）を入れ、起動は
1.50s → 1.07s になった。経緯は [implementation-log.md](implementation-log.md) の該当節。

2026-08-19 に「呼ぶとコンパイルできなくなるもの」（[syntax.md](../syntax.md) §10）の最後の1つ
`eval` を閉じ、あの表は空になった。経緯・設計判断は
[implementation-log.md](implementation-log.md) の該当節。

作業を始めるときはここに項目を足し、終わったら（経緯・設計判断を
[implementation-log.md](implementation-log.md) へ書いたうえで）ここから消す。

残差そのものの地図は
[cl-missing-classes-and-methods.md](cl-missing-classes-and-methods.md)（TODO ではなく測定）。
そこから作った実行計画が上記の [cl-parity-plan.md](cl-parity-plan.md) で、地図の全行が
どの Phase に落ちたか（落ちていないなら理由）は同計画の付録 A にある。

## 関連ドキュメント

| 知りたいこと | 参照先 |
|---|---|
| 完了した実装の経緯・設計判断 | [implementation-log.md](implementation-log.md) |
| 言語仕様の確定事項・非採用と決めた機能 | [language-design.md](language-design.md)（非採用リストは §9） |
| Common Lisp と比べてまだ無いクラス・メソッド | [cl-missing-classes-and-methods.md](cl-missing-classes-and-methods.md) |
| それを埋める実行計画（Phase / 対象外の理由 / 完了判定） | [cl-parity-plan.md](cl-parity-plan.md) |
| ビルド・テストの実行方法 | [development.md](development.md) |
| ユーザ向けの関数・構文リファレンス | [functions.md](../functions.md) / [syntax.md](../syntax.md) |
