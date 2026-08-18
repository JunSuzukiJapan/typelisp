# typelisp 開発 TODO

最終更新: 2026-08-18 / ブランチ: `main`

このドキュメントは**現在残っている作業のみ**を記録する。

## 残っている作業

### `eval` を呼ぶ関数をコンパイルできるようにする

2026-08-18 に「呼ぶとコンパイルできなくなるもの」（[syntax.md](../syntax.md) §10）を6項目から
1項目へ減らした。経緯は [implementation-log.md](implementation-log.md) の該当節。残る1つが `eval`。

`eval` はチェッカーとインタプリタそのものを要する（`Interp::eval_form` は `Checker::check_form_at`
→ `Interp::exec`）。手口は前の5つと同じ「実装を下ろして shim を書く」だが、下ろす対象が
フロントエンド全体（`check/` + `eval/` + prelude + 島、約 25,000 行）になる。**調べた結果、
これは2つの別々の作業に分かれる**:

**(A) `typelisp-front` クレートへの分離**。shim がインタプリタを名指しできるようにする前提条件。
`check/` は inkwell を1箇所も参照していないので、そのまま下りる。障害は `eval/interp.rs` の
3箇所:

- `llvm-*` ビルダ組み込み（約1,550行、`eval_llvm_builtin_method` 以下）。既存の `rt_llvm_call`
  と同じフック方式で `typelisp` 側へ上げられる。
- `Interp` の JIT ドライバ（`install_compiled_library`/`compile_function`/`add_compiled_function`/
  `compile_scc`/`call_compiled` 等）。`Module`/`MemoryBuffer`/`COMPILE_LOCK` を直接触る。
- **`FnDef.compiled: RefCell<Option<Rc<CompiledFn>>>`**。フロントエンドの型がバックエンドの型を
  持っている。ここが一番深い結合で、不透明ハンドルか型引数に変える必要がある。

**(B) 単体実行ファイルの中で `eval` を動かす**。(A) を済ませても AOT ではまだ動かない。
`eval` は「プログラムの現在のグローバル環境」に対して型検査するので、実行ファイルが起動時に
prelude と**自分自身の定義**を検査済みの形で登録していなければならない。既存の FASL 機構
（`prelude::load_cached`）が検査済み状態のシリアライズをすでに持っているので土台はあるが、
実行ファイルへの埋め込みと起動時復元は新規。サイズは数 MB 増える見込み——ただしそれを払うのは
`eval` を呼ぶプログラムだけ、という 2026-08-18 の分割方針はそのまま適用できる。

(A) だけを先に landing させると、`compile`（JIT）では `eval` が通り `compile-file`（AOT）では
通らない、という中途半端な状態になる。その場合は AOT 側でチェック時に落とすこと——実行時に
abort する shim を置くのは、コンパイル時の拒否より悪い。

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
