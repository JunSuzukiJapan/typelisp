# typelisp 開発 TODO / 引き継ぎ

最終更新: 2026-07-02 / ブランチ: `feature/compile-sexpr`

このドキュメントは**現在残っている作業のみ**を記録する。完了した実装の詳細な経緯・設計判断は
[implementation-log.md](implementation-log.md) を参照（2026-06-27 にこちらから分離した）。
**言語仕様の確定事項は [language-design.md](language-design.md) を参照。**

---

## 残っている作業（影響範囲の大きさで優先順位付け——[[feedback-impl-priority]]）

1. **Sexpr/RtValue内部表現統合**（2026-07-02起案、進行中）——`Sexpr`（`crates/typelisp-mem::Value`）
   と`RtValue`（`src/eval/value.rs`）という2つの並行した実行時値表現に分かれているのは
   内部表現として不自然、というユーザー指摘を受けて着手。`defstruct`インスタンス
   （`Vector<T>`/`cons-cell<K,V>`含む）・クロージャ・`HashTable<K,V>`・`Scope<V>`を
   `Sexpr`側の`Value::Boxed`表現に統合し、LLVM builder等コンパイラ内部専用のFFIハンドルだけを
   `RtValue`に残す。設計・ステージ分割の詳細は
   [implementation-log.md](implementation-log.md)の「Sexpr/RtValue内部表現統合 実装計画」節参照。
   - **Stage 0（完了）**: `Value::Boxed`/`BoxedObj::Float`の骨組み、`TAG_FLOAT`→`TAG_BOXED`再利用。
   - **Stage 1（完了）**: `defstruct`/`Vector<T>`/`cons-cell<K,V>`が最終的に載る
     `BoxedObj::Struct`/`StructPayload::Fields`のmem/rt層プラミング
     （`Heap::alloc_struct`等、`rt_struct_new`/`rt_struct_field_get`/`rt_struct_field_set`）。
     まだ`RtValue::Struct`と並存するだけで、インタプリタ/コンパイラのどちらにも未配線。
   - **Stage 2（完了）**: Struct: インタプリタ結線——`RtValue::Struct`/`StructData`を削除し
     `Expr::Construct`/`FieldGet`/`FieldSet`を新表現（`RtValue::Sexpr`が包む
     `Value::Boxed`/`BoxedObj::Struct`）に接続。`Vector<T>`/`cons-cell<K,V>`も自動的に
     新表現に乗った。
   - **Stage 3（完了）**: Struct: コンパイラ結線——`mutable`フラグ分岐、
     `compile-construct-boxed-struct`新設。
   - **Stage 4-5（未着手）**: `HashTable<K,V>`を`BoxedObj::Struct`（`StructPayload::Map`）に統合。
   - **Stage 6a-6b（未着手）**: 変数束縛スロットの`BoxedObj::Cell`化 → `RtValue::Closure`を
     `BoxedObj::Closure`に統合。
   - **Stage 7-8（未着手）**: `Scope<V>`を`BoxedObj::Struct`（`StructPayload::Frames`）に統合
     （自己ホスティングコンパイラ自体がScopeに依存するため最後に単独で着地）。

直近完了: 「型が分からない」を誤った理由とする未対応箇所の一掃（2026-07-02、Stage 3の直後）——
ユーザー指摘「静的型付け言語なのに型が分からない状況があるのはおかしい」を受け、checkerが
持つ型分類情報を利用地点まで運ぶ経路を整備して全件解消。(1)`ast_to_sexpr`に
`structs: &HashSet<Path>`（`Interp::struct_types`）をスレッドし、ネストした
`defstruct`/`Vector<T>`/`cons-cell<K,V>`型フィールドを`struct_field_kind`のkind 6
（パススルー）としてcompile可能に（`compiler.rs`はkind 6が元来パススルーでコード変更ゼロ）。
(2)interpの`Sexpr`宣言フィールドがquoted scalarを`Int`/`Float`等に誤decodeしていたのを
静的型で修正——`Expr::FieldGet`のeval armは自ノードの`ty`、`vector_get`は
`eval_builtin_method`に追加した`ret_ty`、`match`のstructパターンは`Pattern::Ctor`に
check時焼き込みした`sexpr_fields: Vec<bool>`で判定。(3)関連する誤ったdocコメントを訂正。
kind 0（明示的panic）に残るのは正当な表現ギャップのみ: `Fn`型フィールド（Stage 6a-6bの
領分）・一般ADTフィールド（生mallocポインタはGC構造体フィールドに置けない）・ジェネリック
型変数（monomorphizationの問題、型消去の本質）。全31テストバイナリ812件green（+7件）。
詳細は[implementation-log.md](implementation-log.md)の同名節参照。

その前に完了: Sexpr/RtValue内部表現統合Stage 3（2026-07-02）——コンパイラ（`src/compiler.rs`/
`src/compile/ast_bridge.rs`）を新表現に接続。`Expr::Construct`の`mutable`フラグを
`construct`タグの新フィールドとして伝搬し`compile-construct`を3分岐化、`mutable`（`defstruct`/
`Vector<T>`/`cons-cell<K,V>`）は新設`compile-construct-boxed-struct`（`rt_struct_new`）へ、
それ以外は既存の`compile-construct-sexpr`/`compile-construct-box`のまま。`compile-field-get`/
`compile-field-set`も`rt_struct_field_get`/`_set`経由に全面書き換え（旧`malloc`box前提の
`+2`オフセット読み書きを廃止）。フィールド値の非タグ付き⇔タグ付き`Sexpr`変換は
`ast_bridge::struct_field_kind`（`Sexpr`のvariant番号を再利用）+ 新設
`compile-tag-struct-field`（エンコード、`compile-sexpr-field`のデコードと対）が担う。
`compile-trait-call`のレシーバ型ID読み出しは新設`compile-recv-type-id`で表現非依存化
（タグビット判定でboxed-struct/malloc'd-box双方に対応、新設Rust側`rt_struct_type_id_hash`が
構造体の型名文字列から`ast_bridge::type_id_hash`と同じFNV-1aを実行時計算）。
`Interp`に`struct_types: HashSet<Path>`を追加（`TopLevel::Defstruct`実行時+`vector`型を
`Interp::new`でシード）し、`call_compiled`の返り値デコード判定に使用——引数エンコードは
逆に`RtValue`自身の実行時shape（`Sexpr` vs `Int`）で判定するよう変更（ジェネリック関数の
型変数`T`には`struct_types`が答えられないため）。ネスト構造体フィールドは当初
「registry-freeなため区別できない」として未対応だったが、この理由付けは誤りで同日中に
解消済み（上記「直近完了」参照）。詳細は[implementation-log.md](implementation-log.md)参照。

その前に完了: `HashTable<K,V>`への`Iter`実装（2026-07-02）——`keys`/`values`/`entries`
（Rust builtin、`registry::hashtable_def`/`eval_builtin_method`）を新設し、`entries`が返す
`Vector<cons-cell<K,V>>`（`cons-cell<A,B>`は`prelude.rs`の汎用`car`/`cdr`構造体）を
`hashtable-iter<K,V>`が`vector-iter<T>`と同じカーソル走査で辿る形で`Iter`を実装。`HashMap`に
安定した再開可能カーソルがないため、スナップショット方式（呼び出し時点のコピー）。`Sexpr`は
意図的に`Iter`非対応のまま——各`cons`セルの`car`が独立に動的型付けされるため`Iter`が要求する
「1つの`Item`型」を正しく宣言できない。詳細は[implementation-log.md](implementation-log.md)参照。

compile 機能（LLVM JIT/AOT コンパイラ）の残課題は2026-07-01時点で全て解消済み——
Stage 7（文字列対応、計画上最後に残されていたステージ）を含め、詳細は
[implementation-log.md](implementation-log.md)の該当節を参照。

---

## 開発コマンド

```sh
cargo test                                   # 全体
cargo +nightly miri test --test mem_test     # GC/ポインタの UB・リーク検査
cargo +nightly miri test --test read_test
MIRIFLAGS=-Zmiri-disable-isolation cargo +nightly miri test --test numeric_test  # random が SystemTime を使うため isolation 解除が必要
cargo run                                    # REPL（typl、prelude 読み込み済み）
```

### compile 機能のビルド

`inkwell`（LLVM 17 バインディング）に依存するため `LLVM_SYS_170_PREFIX` が必要
（`brew install llvm@17` 済みが前提）。**このパスはマシンごとに異なるため、リポジトリ内の
どのファイルにも絶対パスをハードコードしない**——`scripts/with-llvm-env.sh` が
`brew --prefix llvm@17` で都度動的解決する:

```sh
scripts/with-llvm-env.sh cargo build
scripts/with-llvm-env.sh cargo test
```

`LLVM_SYS_170_PREFIX` を自分のシェルで既に export 済みなら、素の `cargo build`/`cargo test`
でも動く（このスクリプトは便宜上のラッパーであり必須ではない）。シェルの環境変数が古い
LLVMバージョンを指す等でシャドウされていると、素の `cargo` は `LLVMConstShl` 等の未定義
シンボルでリンクエラーになることがある——その場合は上記スクリプト経由で実行すること。
