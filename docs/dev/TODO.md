# typelisp 開発 TODO / 引き継ぎ

最終更新: 2026-07-12 / ブランチ: `main`

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
- ~~**`use`はソースルート相対のみ**: 兄弟ファイル相対の解決は未対応~~ **→ 2026-07-13 解消**。
  `Loader::ensure_loaded`が各prefix長でroot-relative候補を試した後にsibling-relative候補
  （`use`元ファイルのディレクトリを前置）もフォールバックとして試すよう拡張、
  `Checker::find_module`に対応するsiblingティアを追加（新設`file_ns`スタックで、ファイル自身の
  モジュールパスをネストした`(module ...)`越しにも安定して保持——`self.ns`をそのまま使うと
  ネストmodule内の`use`が誤ってそのmodule自身の親を基準にしてしまう）。root-relativeが常に
  優先されるため既存の解決結果は非破壊。`project.rs`は`enter_module`/`exit_module`ではなく
  ファイル境界専用の`enter_file_module`/`exit_file_module`を使用（nested `(module ...)`は
  従来通り`enter_module`のまま）。テスト: `module_file_test.rs`に3件追加
  （bare名前解決・root優先・nested module内でもfile境界基準）。

#### hover / goto-definition / 補完（すべて実装済み、ローカル変数含む——2026-07-11〜2026-07-12）

`src/check/registry.rs`の`DefLocs`（`Registry.def_locs`）・`src/check/locate.rs`
（`locate_node`/`definition_target`/`hover_text`/`completion_candidates`/`completion_locals`）・
`src/bin/lsp.rs`の`hover_provider`/`definition_provider`/`completion_provider`で実装。
グローバル定義（`defun`/`defmethod`/`defstruct`/`defenum`/`defvar`/`deftrait`/`defmacro`）と
ローカル束縛（`let`/`let*`/`lambda`/`labels`束縛・関数パラメータ・`match`パターン束縛）の
両方をカバーする（当初「既知の制限」だったローカル束縛は2026-07-12に解消——`Heap::elem_locs`で
リスト要素（裸アトム含む）に位置を付与し、`DefLocs::local_refs`で参照位置→束縛位置を解決。
`match`パターン束縛と「スコープ境界直後の補完取りこぼし」も同日解消——`check_pattern`が
束縛位置を返して`extended_with_locs`へ、`scope_typed`に`Match`アーム追加、補完プレースホルダ
`(panic "")`をカーソル位置ちょうどに挿入するよう先頭空白を除去）。
詳細な設計・段階分けは[implementation-log.md](implementation-log.md)の
「LSP: ローカル変数の hover / goto-definition / 補完 対応」節を参照。
単体テスト: `tests/lsp_locate_test.rs`/`tests/lsp_completion_test.rs`（LSPのstdioトランスポートは
介さず`Checker::check_form`を直接駆動してコアロジックのみ検証）。

**チェッカーのエラー回復対応（2026-07-12、当初「既知の制限」だった非catchall `match`アーム内補完を解消）**:
`Checker`に`set_recover(true)`モードを新設——最初のエラーで中断する代わりに、少数の回復境界
（`check_form`/`drain_specializations`/`check_seq`/`check_let`/`check_match`）で`Err`を
`errors`へ蓄積し`Never`型のホールノード（`Expr::Panic`再利用）に差し替えて検査を継続する。
LSPの`diagnostics_for`/`candidates_for`のみがこのモードを使い、CLI/REPL/prelude/テストは従来の
厳格モード（`recover=false`）のまま。これにより(1)型エラーのあるファイルでも部分的な`Typed`木が
得られ補完/hover/gotoが動く、(2)ファイル内の全型エラーを一度に診断できる。とくに補完の
truncate設計（カーソル以降を切り捨て）が非catchall `match`アーム内で後続アームを消して
非網羅エラーにしていた問題は、非網羅を回復可能エラーとして記録しつつ完全な`Match`ノードを
返すことで解消。詳細は[implementation-log.md](implementation-log.md)の
「チェッカーのエラー回復モード」節。

**既知の制限（意図的なMVPスコープ、解消せず残す）**:
- アトム単体でも**リスト要素として現れる場合は**位置を持つ（2026-07-12対応）が、
  リストに一切現れない裸アトム（実質発生しない）は依然として位置を持てない。真のspan対応
  （reader全体の作り替えが必要）は引き続き見送り。

`compile`（LLVM JIT/AOT）機能は現状「今のフェーズが実際に使うASTノードだけ本実装、それ以外は
`ast_bridge`が`(unsupported "<Variant>")`を返しコンパイラ本体が明示的にpanicする」設計
（`ast_bridge.rs`冒頭のdocコメント参照）。ユーザーが普通に書けるコードから実際に到達しうる
`unsupported`は2026-07-12時点で**解消済み**——残る`Expr::TraitCall`は単型化後に到達不能な
診断専用ノードと確認済みで対象外（`tests/trait_test.rs`参照）。以下、直近の完了分。

直近完了: **Iter トレイトを持つ全型（Vector/HashTable）の compile 対応**（2026-07-13、branch
`feature/iter-compile`）——「ジェネリック本体そのものの compile」（旧・将来課題）を解消。
`doiter` / `map`・`filter`・`member` 等のコンビネータ over `Vector<T>`・`HashTable<K,V>` が compile
可能に。ユーザー定義 Iter 型も、その `next` が compile 可能なプリミティブに落ちる限り**専用対応
ゼロで**通る（トレイトディスパッチは単型化で消えるため）。実体は 2 つ: (1) コレクション・
プリミティブ層（`vector-op`/`hashtable-op` ノード + `rt_struct_field_count`/`rt_struct_push_field`/
`rt_hashtable_*` 群、要素型 kind によるタグ/デコード）、(2) `Interp::compile_function_rec`——
`(compile fn)` が呼ぶ単型化インスタンス（`vector::iter <i64>` 等、空白マングル名で名指し不可）を
推移的に自動 compile。当初「反復とは別問題」として対象外にしていた `HashTable::get`/`remove`
（`Option` 返し）も同日中に追加解消——実行時の found/not-found 結果で `Some`/`None` を組み立てる
ため`compile-construct-box`をそのまま使えず、新設`rt_hashtable_contains`/`_get_raw`/`_remove_raw`
を`compile-if`と同型のalloca+分岐+merge（phiビルトインなし）で呼び分け。詳細は
[iter-compile-plan.md](iter-compile-plan.md)。

その前に完了: **`f64` レシーバのメソッド compile**（2026-07-13）——算術（`+`/`-`/`*`/`/`/`mod`）と
比較（`<`/`<=`/`>`/`>=`/`=`/`/=`/`eq`/`eql`/`equal`/`equalp`）を対応。compiled `f64`は生bitパターンを
i64に埋め込む表現なので、新設ビルトイン`build-fadd`/`fsub`/`fmul`/`fdiv`/`frem`が各`bitcast`
i64↔doubleで挟み、`build-fcmp-*`が比較（`<`等はordered、`/=`はRust`!=`に合わせunordered UNE）。
`compile-assoc`にf64分岐+`float-native-method?`、`call_compiled`にf64引数/戻り値マーシャリング
（`to_bits`/`from_bits`）、method-target検証除外に`f64`追加。**残る compile 未対応**: transcendental
（`sqrt`/`floor`/`expt`/...、libm必要）と変換（`float->int`/`float->bignum`/`float->ratio`）。
落とし穴: LLVM `frem`はCの`fmod`呼び出しにlowerされるため`fmod`という名の関数compileはJITシンボル
解決衝突で無限再帰。テスト: compile_test 3件 + typelisp-rt（既存流用）。

その前に完了: **char/string の `equalp`（ASCII大文字小文字無視）の compile**（2026-07-13）——
`eq`/`equal`（生コードポイントの`icmp`/`rt_str_eq`）と違い`equalp`はcase-foldingするため単一命令に
できない。新設`rt_char_equalp`（生i64コードポイント2つ）/`rt_str_equalp`を`compile-assoc`の
char/string分岐から呼ぶ（`char-native-method?`/`string-native-method?`にも追加）。テスト:
compile_test 2件 + typelisp-rt 2件。

その前に完了: **`Panic`/`MethodRef`/`Quote`の compile 対応（残っていた`unsupported`3件を解消）**
（2026-07-12）——`ast_bridge.rs`の`Expr::Panic`/`Expr::MethodRef`/`Expr::Quote`アームを実装。
- **`Panic`**: `(panic msg)` -> `(panic msg-form)`。`msg`は常に`Str`型なので`kind`分岐は不要、
  `compiler.rs`の新設`compile-panic`が`msg-form`を`compile-str`同様にタグ付き`Sexpr::Str`へ
  コンパイルし新設`rt_panic`（`typelisp-rt`）へ渡す。`rt_panic`はインタプリタ経路の
  `EvalError::Panic`と同じ`"panic: {msg}"`書式を出力して`abort()`——コンパイル済みコードは
  JIT/AOTネイティブ境界を安全にunwindできないため、`rt_match_fail`と同じ「abortのみ許容」規約
  に従う意図的な仕様差（インタプリタ側は`Result`として回収可能だが、コンパイル済みコードでは
  プロセスごと落ちる）。
- **`MethodRef`**: `+`等のメソッドを値として使う式（`Checker::method_value`）->
  引数を`(assoc type-name method true ...)`呼び出しへそのまま転送する非キャプチャ`lambda`
  （`translate_fnref`の転送ラッパー手法をinstanceメソッドへ一般化、受け手は`compile-assoc`/
  `compile-assoc-user`——ネイティブ演算もユーザー定義メソッドも既存の`recv::method`呼び出し経路を
  再利用するため`compiler.rs`側の変更ゼロ）。ただし転送先の`(type_name, method)`を
  `ast_bridge::collect_calls`（前方宣言収集）が`Expr::Assoc`と同様に辿るよう追加が必要だった
  ——`Expr::FnRef`が転送`call`ラッパーに対して同じ扱いを受けているのと対称。
- **`Quote`**: `(quote datum)` -> `translate_construct`と全く同じ`(construct true false empty
  variant arg-form...)`ワイヤ形状を`QuotedSexpr`から直接合成（`Cons`は再帰）。
  `Nil`/`Int`/`Float`/`Bool`/`Char`/`Str`/`Cons`は既存の`compile-construct-sexpr`がそのまま
  対応、`Sym`/`Bignum`/`Ratio`（コンパイル表現なし）は入れ子内のどこにあっても`unsupported`へ
  伝播（`unsupported`は`Ok`を返す設計のため、再帰呼び出し結果のタグを見て伝播させる必要があった
  ——素朴に`?`へ任せると`(unsupported ...)`タグがデータとして埋め込まれてしまう落とし穴）。
- 副産物として発覚した既存の欠落も同時解消: `char`リテラルが単体で`compile-value`の
  ディスパッチタグに存在しなかった（`(defun f () char #\A)`が常に失敗していた）ため、
  新設`compile-char`/`sexpr-char`（`registry.rs`/`Interp::eval_builtin`）で対応
  （`char->int`のi32結果を`(as i64 ...)`の無償変換でi64化）。~~JIT呼び出し境界での`char`戻り値
  デコードは既存のまま範囲外~~ **→ 2026-07-13 解消**：`Interp::call_compiled`の戻り値デコードに
  `Type::Char`アームを追加（生i64コードポイント→`char::from_u32`→`RtValue::Char`、引数側の
  `*c as i64`の逆）。`char`を返すcompiled関数がインタプリタと相互運用可能に。テスト:
  `compile_test`に2件（引数passthrough・if分岐で選択したcharリテラル）。
- テスト: `tests/compile_test.rs`に6件（panic branch/char literal/builtin・ユーザー定義method
  reified/quoted list構築/JIT-interp一致/quoted symbolのclean error）、`ast_bridge.rs`単体
  テストに5件。

その前に完了: **`Match`のstruct-kind（`defstruct`/`Vector`のboxed-struct表現）scrutinee対応**
（2026-07-12）——これで`Match`の`unsupported`分岐は解消（sum-ADT box/struct-kind/`Sexpr`の3種
すべてcompile時`match`が可能に）。`translate_match`/`pattern_to_sexpr`の`is-box: bool`を
`scrut-kind: i64`（0=`Sexpr`、1=sum-ADT box、2=boxed struct、`ast_bridge::MATCH_KIND_*`）に
一般化し、`pat-ctor`に`field-kinds`（各フィールドの`struct_field_kind`のリスト、struct-kind時のみ
意味を持つ）を追加。boxed structは単一variant（`new`）なのでタグテストは一切発行せず
（`compiler.rs::compile-pattern-test`の`scrut-kind = 2`分岐）、フィールド抽出だけを行う新設
`compile-struct-field`（`rt_struct_field_get`→`compile-sexpr-field`、`compile-field-get`と
同じ経路の再利用）で対応。sum-ADT boxと異なりboxed structは通常のGC管理ヒープ値なので
`compile-match`のGCルート要否判定（`push-sexpr-root`/`pop-sexpr-root`）も`Sexpr`側と同様に
必要（boxのみ不要）——ここは`scrut-kind`を素朴に真偽反転しただけでは見落としがちな罠だった。
フィールドごとの型情報が必要になったのは今回が初めてで（sum-ADT boxは自身の`variant`だけで
全フィールド共通のデコードができたが、boxed structはフィールドごとに
int/float/char/bool/passthroughが異なる）、`Pattern::Ctor`に`field_types: Vec<Type>`を新設
（`Checker::check_ctor_pattern`が`sexpr_fields`と並べて計算、インタプリタ側は変更なし）。

その前に完了: **`Global`/`SetGlobal`（グローバル変数のcompile対応、JIT/AOT両方）**
（2026-07-11）——`defvar`/`defconstant`をcompile対象の関数から参照・代入できるようになった。
コンパイル済みコードから参照されたグローバルだけをGCの**permanent root**
（`crates/typelisp-mem`の`Heap.permanent_roots`、`rt_push_permanent_sexpr_root`が構造体
フィールド保護に使う既存機構を再利用）に「昇格」させる設計。一度もコンパイルされない
グローバルは今まで通りインタプリタの`Slot`ベース経路のまま（`rtvalue_to_struct_field`が
`RtValue::Data`＝`Option`/`Result`/ユーザー`defenum`を変換できないため、そうした型の
グローバルはcompile対象から参照できない——既知の制限）。JITは参照時に遅延昇格
（`Interp::promote_global`）、AOTは`compile-file`が全`defvar`をファイル宣言順に即座昇格
（`Interp::promote_global`のdocコメント参照——2つのタイムライン（コンパイル時のRustプロセスと
実行ファイル自身のランタイム）でid採番を一致させるため）し、`Interp::add_compiled_global_init`が
生成する初期化関数群を`build_main_wrapper`が`rt_heap_init`と`tl_main`の間に挿入。
昇格済みグローバルは、一度もコンパイルされていなくても、インタプリタ自身の
`Expr::Global`/`Expr::SetGlobal`評価も同じpermanent-root経由に切り替わる（コンパイル側の
書き込みをインタプリタ側の読み取りが取り残さないように）。新設: `crates/typelisp-mem`の
`Heap::permanent_root`/`set_permanent_root`（インデックス指定アクセサ）、`crates/typelisp-rt`の
`rt_global_new`/`rt_global_get`/`rt_global_set`/`reset_global_table`、`ast_bridge.rs`の
`collect_global_targets`/`translate_global`/`translate_set_global`/
`ast_to_sexpr_for_global_init`、`compiler.rs`の`compile-global`/`compile-set-global`/
`compile-global-init`（`ast_bridge::struct_field_kind`/`compile-sexpr-field`/
`compile-tag-struct-field`という既存のdefstructフィールドエンコード/デコード機構を再利用）。

その前に完了: **compile側TraitCall機構の削除**（2026-07-04）——ジェネリック単型化（2026-07-03）で
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
