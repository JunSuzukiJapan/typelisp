# typelisp 開発 TODO

最終更新: 2026-08-17 / ブランチ: `feature/compiled-unwind`

このドキュメントは**現在残っている作業のみ**を記録する。

## 残っている作業

### 素の `Module`/`Builder` の破棄がまだ `COMPILE_LOCK` の外に残りうる

2026-08-17 に `ExecutionEngine` の破棄は引退リスト（`compile::retire_llvm`）でロックの
内側に入れた。同時に、エンジンに渡していない `Rc<RefCell<Module>>` を持つ 4 箇所
（`compile_scc`・`aot::compile_file`・島と prelude の bootstrap）はガードを保持したまま
明示的に `drop` する形にした。

**残っているのは「最後の share を我々が落とす」保証が無いこと**。島の LLVM ハンドル
レジストリ（`NativeHandle::Module`/`Builder`、スレッドローカル）が一時的に share を持ち、
その解放は `add_compiled_function` の中——ロックを保持していない側——で起きる。レジストリの
share のほうが後に落ちれば、実際の破棄はロック外になる。

引退リストに `Rc` の share を*クローンして*預けるのは駄目（所有者スレッドと回収スレッドが
同じ非アトミックカウンタを触る）。直すならレジストリの解放をロック下に入れるのが筋。

`Module` ほどではないが `Builder` も同じ経路にいる。エンジン破棄ほど大きな操作ではない
（値名テーブルを持つのは Module）ので、実際にクラッシュとして観測してはいない。

### checker の quasiquote 経路に GC ルート漏れ

`gc-stress` を最初から有効にすると prelude のロードで落ちる。`check_qq_template` →
`construct_form` の経路。`catch`/`throw` の作業（2026-08-16）で見つかったが、そのときも
今回も触っていない。テストは prelude をロードし終えてから stress を入れる形で回避している。

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
