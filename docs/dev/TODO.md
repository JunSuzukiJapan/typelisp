# typelisp 開発 TODO / 引き継ぎ

最終更新: 2026-07-11 / ブランチ: `main`

このドキュメントは**現在残っている作業のみ**を記録する。完了した実装の詳細な経緯・設計判断は
[implementation-log.md](implementation-log.md) を参照（2026-06-27 にこちらから分離した）。
**言語仕様の確定事項は [language-design.md](language-design.md) を参照。**

> **大規模再設計はほぼ完了**: 「Symbol型導入と Sexpr の裏方化」を
> [symbol-sexpr-redesign.md](symbol-sexpr-redesign.md) で管理（作業ブランチ `feature/symbol-type` は
> main へマージ済み）。Phase 0〜6.6まで完了、残るは **Phase 7（ドキュメント整備）** のみ。

---

## 残っている作業（影響範囲の大きさで優先順位付け——[[feedback-impl-priority]]）

### マルチファイル/LSP関連（2026-07-11のファイル↔モジュール対応実装の残課題）

- **マクロ展開が生成した`use`はロードされない**: 依存ロードはチェック前のフォーム走査
  （`src/project.rs`の`scan_form`）で行うため、`defmacro`の展開結果として現れる`use`は
  走査に映らず「unresolved」になる（既知の制限、`project.rs`のdocコメント参照）。
- **LSPの依存キャッシュなし**: 診断パスごとに依存ファイルを再読込（`Loader::set_overlay`で
  開いている文書の未保存内容は優先、`deps`の逆依存追跡で依存元も連鎖再診断——2026-07-11
  実装——だが、ディスク/overlayいずれにせよパス間キャッシュは無く、毎回ゼロから読み直す。
  大きなプロジェクトでキー入力毎の再読込コストが問題になったら着手）。
- **`use`はソースルート相対のみ**: 兄弟ファイル相対の解決は未対応（MVP判断）。

#### hover / goto-definition（共通基盤・両機能とも実装済み、2026-07-11） / 補完（未着手）

**実装済み**: 当初計画（下記「元の設計メモ」）の想定より軽い基盤で済んだ。`Typed.loc`は
既に存在しリスト形式ノードの開き括弧位置を記録していた（`Checker::check`が
`heap.cons_loc(v)`で埋める）ため、終了位置（span）を新設せずとも
「カーソル位置以下で最大の開始位置を持つノードを深さ優先で探す」ことで正しい最小包含ノードが
求まる（兄弟フォームはソース順に重ならないため）——reader/heap側の変更は不要だった。
- `src/check/registry.rs`: `DefLocs`（`Registry.def_locs`）を新設。`FnSig`/`AssocFn`自体には
  フィールドを足さず（`with_builtins()`だけでリテラルが計130件超あり、ビルトインには
  参照すべき位置がそもそも無いため）、サイドテーブル方式でチェッカーの7つの登録箇所
  （`check_defun`/`check_defmethod`/`check_defstruct`/`check_defenum`/`check_defvar`/
  `check_deftrait`/`check_defmacro`）でだけ書き込む。
- `src/check/locate.rs`（新規）: `locate_node`（カーソル→最小包含ノード）/
  `definition_target`（ノード→`DefLocs`引き、Global/Call/FnRef/Assoc/MethodRef/Constructの
  みが対象）/ `hover_text`（`Typed.ty`を`{:?}`でフォーマット、`Type`に`Display`実装が無いため
  既存のエラーメッセージと同じ流儀）。
- `src/bin/lsp.rs`: `hover_provider`/`definition_provider`を有効化。doc毎に直近成功した
  `Analysis { body: Vec<TopLevel>, def_locs: DefLocs }`をキャッシュ（型エラーがある間は
  直近成功時点のものを保持）し、`textDocument/hover`/`textDocument/definition`を処理。
- 単体テスト: `tests/lsp_locate_test.rs`（LSPのstdioトランスポートは介さず、
  `Checker::check_form`を直接駆動してコアロジックのみ検証）。

**既知の制限（意図的なMVPスコープ）**:
- ローカル変数（`Expr::Var`、`let`/`lambda`束縛）への goto-definition は非対応
  （束縛側の位置も追跡していない）——カーソルがローカル変数参照上にある場合は
  「最小包含ノード」まで遡って解決を試みるが、そのノードが解決可能な参照でなければ
  何も返らない。
- アトム単体（裸のシンボル・リテラル）は自身の位置を持てない
  （`Value::Symbol`はヒープ上で一意な参照ではなく使い回されるため、`cons_locs`と同じ
  仕組みでは追跡不可能）。真のspan対応（reader全体の作り替えが必要）をすればより高精度な
  hover/goto-defになるが、現状のMVPでは「S式は兄弟フォームがソース順に重ならない」性質だけで
  実用上十分な精度が出ているため見送った。

**補完は未着手**。エラー耐性パーサ（壊れた/書きかけの入力でも最善のASTを作れる）が前提で、
typelispの`Reader`は最初の構文エラーで即失敗する設計のため別ラインの作業になる——下記
「元の設計メモ」参照。

<details>
<summary>元の設計メモ（2026-07-11、実装着手前の調査結果）</summary>

2026-07-11に他言語のLSP実装（rust-analyzer/tsserver/clangd等）を調査した上での設計メモ。

**他言語がどう解決しているか**（一般知識）:
- rust-analyzer/tsserverはASTノードが最初から範囲（start~end）を持ち、hoverは
  「カーソル位置を含む最小のノードを探す」木の走査だけで済む。見つけたノードの型は
  チェック時に計算済みの型テーブルから引く。
- goto-definitionも同じ「ノード特定」までは共通で、シンボルIDを使って**定義側の位置
  テーブル**（rust-analyzerの`DefMap`等）を引く——定義側は名前解決だけでなく定義位置も
  記録している。
- 補完は**エラー耐性パーサ**が前提（壊れた/書きかけの入力でも最善のASTを作れる）。
  typelispの`Reader`は最初の構文エラーで即失敗する設計で性質が異なる。ただしS式は
  閉じ括弧の対応が単純なので、「カーソルまでの入力に足りない`)`をヒューリスティックに
  補ってから読む」前処理層の方が、汎用エラー回復パーサを一から作るより現実的
  （Common Lisp/Racket系LSP実装でよく使われる手法）。

補完実装時はこの前処理層＋可視スコープ内の名前を`Registry`/`Scope`チェーンから列挙する
方式を検討する。

</details>

`compile`（LLVM JIT/AOT）機能は現状「今のフェーズが実際に使うASTノードだけ本実装、それ以外は
`ast_bridge`が`(unsupported "<Variant>")`を返しコンパイラ本体が明示的にpanicする」設計
（`ast_bridge.rs`冒頭のdocコメント参照）。以下はその`unsupported`のうち、ユーザーが普通に
書けるコードから実際に到達しうる（＝いずれ本実装が要る）ものの一覧——`Expr::TraitCall`は
単型化後に到達不能な診断専用ノードと確認済みで対象外（`tests/trait_test.rs`参照）。

1. **`Match`: sum-ADT box scrutinee（`Option`/`Result`/`defenum`）は対応済み。残るは
   struct-kind（`defstruct`/`Vector`のboxed-struct表現）scrutineeのみ`unsupported`**
   （`ast_bridge.rs::translate_match`）。2026-07-09に`defenum`実装とあわせて一般ADT boxの
   variantスロットに対するタグテスト（`compiler.rs`の`compile-box-tag-test`/`compile-box-field`、
   `translate_match`/`pattern_to_sexpr`の`is-box`フラグ）を追加し、`Option`/`Result`/ユーザー
   `defenum`のcompile時`match`が可能になった。単一variantの`defstruct`をmatchするのは稀なため
   boxed-struct scrutineeは当面`unsupported`のまま（必要になれば`compile-box-*`と同様に
   boxed-struct表現用のタグ/フィールド抽出を足す）。
2. **`Global`/`SetGlobal`: グローバル変数（`defvar`/`defconstant`）の参照・代入が`unsupported`**
   （`ast_bridge.rs`の`Expr::Global`/`Expr::SetGlobal`アーム）。グローバル参照はごく普通の
   コードで頻出するため次点の影響範囲。
3. **`Panic`: `(panic msg)`が`unsupported`**（同ファイルの`Expr::Panic`アーム）。診断用
   メッセージの文字列化＋`Never`型としての分岐処理が必要。
4. **`MethodRef`: メソッドを値として使う式（例: `+`をそのまま渡す）が`unsupported`**
   （`Expr::MethodRef`アーム）。
5. **`Quote`: `(quote datum)`が`unsupported`**（`Expr::Quote`アーム）。コンパイル対象の関数
   本体にクォートされたリテラルが現れるケースは他より稀。

直近完了: **compile側TraitCall機構の削除**（2026-07-04）——ジェネリック単型化（2026-07-03）で
`Expr::TraitCall`がコンパイル可能ソースから到達不能になったため、compile側の実行時ディスパッチ
機構を全撤去。削除対象: `ast_bridge`の`translate_trait_call`/`collect_trait_call_targets`/
`is_compilable_trait_impl`/`type_id_hash`、`compiler.rs`の`compile-trait-call`/
`compile-trait-dispatch`/`compile-recv-type-id`とディスパッチアーム、ランタイムの
`rt_trait_call_fail`/`rt_struct_type_id_hash`。あわせて一般ADT boxの先頭type-idスロット（trait
ディスパッチ専用の死んだヘッダ）を撤去し、`construct`ノードとboxレイアウトを
`[variant, field...]`に簡素化（`compile-field-get`/`compile-field-set`はもともと
`rt_struct_field_get`経由でこのレイアウトに非依存）。`Expr::TraitCall`ノード自体はチェッカが
ジェネリック定義時本体で生成しインタプリタが内部エラーとしてトラップする診断専用ノードとして
存続するが、`trait_name`/`impls`フィールドと`Checker::trait_impls`はcompile専用だったため撤去し
`{ method, args }`のみに縮小。

その前に完了: **Sexpr/RtValue内部表現統合 Stage 0-8 全完了**（2026-07-02起案、2026-07-04完了）——
`defstruct`インスタンス（`Vector<T>`/`cons-cell<K,V>`含む）・クロージャ・`HashTable<K,V>`・
`Scope<V>`（heap-repr `V`のもの）を`Sexpr`側の`Value::Boxed`表現に統合。到達した最終状態:
`RtValue`に残るのは`Int/Float/Bool/Char/Str/Unit/Data/Sexpr/Builtin/BuiltinMethod`+LLVM系5種+
**native-repr `V`の`RtValue::Scope`**（`Scope<llvm-value>`/`Scope<llvm-function>`等——中身が
FFIハンドルの集合体なので「コンパイラ内部専用のFFIハンドルだけをRtValueに残す」という統合方針
そのものに合致）。最終Stage 8（2026-07-04）はScopeのインタプリタ結線で、**6a型の静的型駆動
表現振り分け**を採用（ユーザー決定）: `Scope<V>`は`V`のランタイム表現が`Value`ならヒープ
`StructPayload::Frames`、それ以外（LLVM系等）は従来の`RtValue::Scope`のまま。振り分けは
`heap_repr_kind`/`is_heap_repr`双子の`Scope<V>`→`V`再帰アーム+`Interp::scope_is_heap`で
**常に静的型から決定**（`new`は自ノードの戻り型`Scope<V>`、インスタンスメソッドはレシーバ
引数のchecked型を`eval_builtin_method`へ新規配管した`recv_ty`から。値形状ディスパッチなし、
不一致は`EvalError::Internal`即死トラップ）。設計比較（サイドテーブル方式との対比）と
実装詳細は[implementation-log.md](implementation-log.md)の
「Sexpr/RtValue内部表現統合 実装計画」節参照。

その前に完了: Sexpr/RtValue統合Stage 7（2026-07-04）——Scope: `StructPayload::Frame`/`Frames`の
mem層プラミング（フレームを独立した箱にして`clone-frames`の参照共有を保持）。
その前: `defvar`/`defconstant`の型注釈必須化（2026-07-03）——`(defvar (name Type) value)`のみ
許可、型なし形式を削除。その前にジェネリック単型化M1-M4 + Sexpr/RtValue統合Stage 6a/6b
（2026-07-03、6コミット）。詳細は[implementation-log.md](implementation-log.md)の
「ジェネリック単型化とSexpr/RtValue統合Stage 6」節参照。

さらにその前に完了（いずれも詳細は[implementation-log.md](implementation-log.md)参照）:
Sexpr/RtValue内部表現統合Stage 5（HashTableのインタプリタ結線、`compiler.rs`の`acc`が
`RtValue::LlvmValue`を格納していた発覚と`Scope<llvm-value>`への付け替え込み、2026-07-03）／
Sexpr/RtValue内部表現統合Stage 4（HashTableのmem層プラミング、2026-07-03）／
「型が分からない」を誤った理由とする未対応箇所の一掃（2026-07-02）／
Sexpr/RtValue内部表現統合Stage 3（コンパイラ結線、2026-07-02）／
`HashTable<K,V>`への`Iter`実装（2026-07-02）／
compile機能（LLVM JIT/AOTコンパイラ）の残課題解消（2026-07-01時点で全完了）。

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
