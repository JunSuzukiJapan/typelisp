# typelisp 開発 TODO

最終更新: 2026-08-19 / ブランチ: `feature/close-compile-gaps`

このドキュメントは**現在残っている作業のみ**を記録する。

## 残っている作業

### `eval` を呼ぶ関数をコンパイルできるようにする

2026-08-18 に「呼ぶとコンパイルできなくなるもの」（[syntax.md](../syntax.md) §10）を6項目から
1項目へ減らした。経緯は [implementation-log.md](implementation-log.md) の該当節。残る1つが `eval`。

`eval` はチェッカーとインタプリタそのものを要する（`Interp::eval_form` は `Checker::check_form_at`
→ `Interp::exec`）。手口は前の5つと同じ「実装を下ろして shim を書く」だが、下ろす対象が
フロントエンド全体になる。作業は2つに分かれ、**(A) は 2026-08-19 に完了した**。

**(A) `typelisp-front` クレートへの分離**（完了）。`check/` + `eval/` + `types.rs` +
`type_key.rs` + `project.rs` + prelude の SOURCE、約 20,000 行が `crates/typelisp-front` に
移った。`typelisp` は `pub use typelisp_front::{...}` で従来のモジュールパスを再輸出するので、
外から見た `typelisp::Heap` などは変わっていない。

- `llvm-*` ビルダ（`src/compile/llvm_builtins.rs`）と JIT ドライバ（`src/compile/driver.rs`）を
  backend へ移し、インタプリタは関数ポインタの `Backend` 構造体越しにだけ backend を呼ぶ。
- `FnDef.compiled` は `Rc<dyn CompiledBody>`（アドレスを返すだけのトレイト）になった。
  事前に「一番深い結合」と見立てたが、`CompiledFn` は `{engine, addr}` の2フィールドで、
  フロント側の利用は全部 `.address()` だったので実際には浅かった。
- prelude はソースがフロント・ビットコード導入が backend に割れた
  （`typelisp::load_prelude` は `compile::prelude_bootstrap::load` を指す）。
- AOT がリンクするアーカイブは `libtypelisp_rt.a` → `libtypelisp_front.a`。front は rt の上に
  あるので、1つのアーカイブに両方入るのは外側だけ。**`eval` を呼ばないプログラムのサイズは
  +784 バイト、front のシンボルは 0 個**（`(defun main () i32 42)` で実測）。

**(B) 単体実行ファイルの中で `eval` を動かす**（残り）。(A) を済ませても AOT ではまだ動かない。
`eval` は「プログラムの現在のグローバル環境」に対して型検査するので、実行ファイルが起動時に
prelude と**自分自身の定義**を検査済みの形で登録していなければならない。設計の当たりは付いていて、
`build_main_wrapper` が印字向けにすでに持っている「モジュールがそのシムを call しているときだけ
起動時登録を出す」仕掛けをもう一段使う:

1. `rt_eval_source(ptr, len)` — プログラム自身のソースを埋め込んで渡す。
2. `rt_eval_compiled_fn(name, addr)` — コンパイル済み本体のアドレス。eval したフォームからの
   呼び出しが tree-walk に落ちないように、replay 後に `FnDef.compiled` へ入れる。
3. `rt_eval_global(name, id)` — コンパイル済みグローバルのスロット id。インタプリタは
   `compiled_globals` を先に見る（`global_core`）ので、これを入れれば記憶域が共有される。
   **`defvar` の初期化子を二度走らせないこと**が設計上の要点で、replay とコンパイル済み
   グローバル初期化のどちらが値を書くのかを1つに決める必要がある。

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
