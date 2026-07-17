# typelisp 開発 TODO / 引き継ぎ

最終更新: 2026-07-17 / ブランチ: `feature/closure-unification`

このドキュメントは**現在残っている作業のみ**を記録する。完了した実装の詳細な経緯・設計判断は
[implementation-log.md](implementation-log.md) を参照（2026-06-27 にこちらから分離した）。
**言語仕様の確定事項は [language-design.md](language-design.md) を参照。**

> **大規模再設計はほぼ完了**: 「Symbol型導入と Sexpr の裏方化」を
> [symbol-sexpr-redesign.md](symbol-sexpr-redesign.md) で管理（作業ブランチ `feature/symbol-type` は
> main へマージ済み）。Phase 0〜6.6まで完了、残るは **Phase 7（ドキュメント整備）** のみ。

---

## 残っている作業（影響範囲の大きさで優先順位付け——[[feedback-impl-priority]]）

### クロージャ表現統一（labels/closures unification）Stage 7-10（branch `feature/closure-unification`）

**背景**: interpクロージャとcompiledクロージャ(ClosureBox)の二重表現による境界ギャップ
（interpクロージャをcompiled関数に渡せない/compiled関数のFn戻り値がIntに化ける/structフィールド
へのFn格納不可）を解消する10段階計画。方針は「クロージャは定義時にJITコンパイルしてcompiled
表現に統一、捕獲変数へのsetfは共有セル（CL的）」。Phase I（表現統一、Stage 1-6）は2026-07-17
完了済み——Stage 1: mem/rt基盤（`BoxedObj::CompiledClosure`+`rt_closure_*`/`rt_cell_*`）、
Stage 2: compiled側flip+ARC全廃、Stage 3: 境界開通（Fn引数/戻り値/structフィールド）、
Stage 4: 共有セル捕獲（`Ctx::cell_names`/`Ctx::visible_siblings`、`cellvar`/`cellset`タグ）、
Stage 5: compileパイプラインのTarjan SCCベース多パス化（`Interp::compute_sccs`/`compile_scc`、
`CompiledFn::new_multi`）、Stage 6: defun/lambda/labels/matchアームの複数式body対応
（`ast_bridge::single_body_expr`、`(let () e1 e2 ...)`ラップ）。全stage独立コミット・
`scripts/test-serial.sh`全green維持。

**Stage 5実装時の発見（Stage 7設計に影響）**: 別々のトップレベル`defun`同士の構文上の相互再帰は
現行チェッカー（`Checker::check_form_at`がトップレベルformを出現順に1つずつ検査、前方参照不可）
では**そもそも記述不能**（`docs/syntax.md`が`labels`を「相互再帰可能なローカル関数定義」と
明記する通り、相互再帰は`labels`専用の言語機能）。Stage 5のSCC機構は現状ユーザー可視の
プログラムからは（生成した`FnDef`を直接`Interp::fns`へ注入する`src/eval/interp.rs`の
`scc_tests`モジュールでのみ）到達するが、**Stage 7で各`labels`siblingが独立JIT単位になった
時点で実際に必要になる**基盤という位置づけ。

以下、未着手の残りstage（計画書全文は元セッションの`~/.claude/plans/
compile-traitcall-traitcall-cuddly-corbato.md`、リポジトリ外のためこの節が主要な引き継ぎ資料）。

- **Stage 7: 定義時JITドライバ**（Phase II開始点、最大の設計判断を伴う）
  - `Expr::Lambda`/`Labels`/`FnRef`/`MethodRef`のevalアームに「tier判定→コンストラクタ関数JIT
    （定義サイトの`Loc`+型署名でキャッシュ）→失敗時は既存の`make_closure`（interpクロージャ）へ
    fallback+理由ログ」という経路を追加。JIT可能tier判定基準: パラメータ・戻り値・捕獲名の型が
    すべてcompiled表現可能（native型捕獲＝自己ホストコンパイラ島専用、およびマクロ展開中は
    fallback対象）。
  - 新設`Slot::TypedCell`（Stage 4の共有セルをinterp側束縛生成時点から使う経路）。
  - マクロ展開時（check時）はJIT抑止フラグを`Interp`に持たせ、interpクロージャを使う
    （LSPは「コード実行なし方針」を維持するため今後もJITしない）。
  - JITエンジンの寿命管理: `fn_ptr`が`ClosureBox`に残り続けるため実行エンジンをdropしない
    「墓場」`Vec`を`Interp`に新設。再定義（redefine）テストで要注意。
  - 環境変数`TYPELISP_CLOSURE_JIT=off|prefer|require`でモード切替を仕込む（`require`モードは
    このstageでは全green化しなくてよい——網羅性はStage 8で埋める）。fallbackがある限り既存
    テストはgreenのまま。
  - fasl形式は変更不要（JITコードはプロセスローカル）。AOTはcompiler.rs共有のため自動反映、
    `rt_closure_*`/`rt_cell_*`のstaticlib export追加のみ確認。

- **Stage 8: compiled被覆拡大**（Stage 7の`require`モード失敗を計画的に潰す複数コミット群）
  - 新しい`rt_*`シム追加（`random`/`gensym`/`equal`/`equalp`等、未compile対応のbuiltin）。
  - `compile-assoc`のlowering追加（`f64::sqrt`等、既にcompile対応済みのものは除く——
    `docs/dev/TODO.md`の`f64`関連の節を参照して重複しないこと）。
  - `TYPELISP_CLOSURE_JIT=require`での失敗理由ログを起点に網羅的に潰す。

- **Stage 9: JIT必須化**
  - fallbackを「native tier（自己ホストコンパイラ島）+マクロ展開時のみ」へ制限。
  - compilable tierでのJIT失敗は明確な`Panic`にする（サイレントfallback廃止）。
  - 受け入れ基準は全体`scripts/test-serial.sh`の完走。

- **Stage 10: 掃除**
  - 到達不能コード削除（`call_compiled`のinterpクロージャ拒否コメント、
    `compiler.rs`/`freevars.rs`のdocumented gap記述の更新）。
  - `docs/dev/implementation-log.md`・`docs/dev/language-design.md`更新。
  - **フォローアップとして必ずTODO.mdへ記録すること**: interpクロージャの完全削除
    （`BoxedObj::Closure`/`ClosureBody`/`Capture`/`closure_bodies`/`make_closure`/`Apply`の
    interpクロージャアーム）は本計画のスコープ外——自己ホストコンパイラ島自体をAOT化する
    フォローアップ企画が前提になる。

### 「既知の制限・意図的に対象外」7項目の解消（2026-07-15）

これまで「既知の制限」「対象外」として個別に記載していた項目のうち、ユーザーの指示
（「対象外はほぼAIが勝手に決めたもの、客観的に実装可能性を判断せよ」）を受けて再調査した結果、
文書上ユーザー自身の判断と確認できたのは **`Sexpr`への`Iter`トレイト実装のみ**（`language-design.md`に
明記、[[typelisp-sexpr-hashtable-iter]]参照）で、残り7件は技術的に実装可能と判明し全て解消した
（規模の大きい順）。branch `feature/known-limitations`、コミット1件=1項目。

1. **値レベル`&rest`/`apply`の再導入**——`Type::Fn`の第2フィールド（rest要素型）と`FnSig.rest`を
   復元、`(apply f a1..aN rest-list)`特殊形を再実装。固定引数は静的型検査、rest-listは`Sexpr`型で
   渡し`wrap_rest_elem`/`cons_rest_list`で単一リストへ畳む。`FASL_FORMAT_VERSION`を4へbump。
   ~~`compile`（LLVM）側は`&rest`付き関数のcompile自体は引き続き非対応（`unsupported`で明示固定）~~
   **→ この記述は不正確だった（2026-07-16訂正）**: `&rest`付き`defun`の直接compile・`apply`経由の
   compile済み呼び出しはいずれも元から動作していた（`Checker::check_defun`が`&rest`引数を
   `(rest名, Sexpr型)`として`params`末尾へ折り込む脱糖段階で、`Interp::compiled_fn_body`が読む
   名前・型のペア数は最初から一致しているため——`tests/compile_test.rs`の
   `a_variadic_function_compiles_and_dispatches_to_native_code`が元から証明済み）。唯一実在した
   ギャップは、`&rest`関数を**第一級の値として参照する**（`Expr::FnRef`化される）場合のみ:
   `ast_bridge.rs`の`translate_fnref`が合成する転送用クロージャがrestパラメータを一切
   宣言・転送しないまま固定引数のみで組み立てられていた——compileはエラーにならず成功するが、
   実行時に空/未ルートの`xs`を読むため誤った値を返すか、GCが介入するとnullポインタ参照で
   クラッシュする「静かな不正確さ」だった。2026-07-16解消（`translate_fnref`の合成パラメータ
   リストに、`ty`の`rest`フィールドから`check_defun`/`check_lambda`と同じ`(rest名, Sexpr型)`を
   追加、回帰テスト`fnref_of_a_variadic_function_forwards_the_rest_list`で検証）。
   `translate_methodref`にも見た目上同じロジックがあるが、`defmethod`構文に`&rest`が存在しない
   （`MethodSig`に`rest`フィールドが無い）ため到達不可能と確認済み、コード変更は不要だった。
2. **依存ファイル（`use`先）のfaslインメモリキャッシュ**——上記「LSPの依存キャッシュなし」解消
   （2026-07-14）で唯一残っていた「依存*ファイル*自体のパス間キャッシュ」を解消。
   `Loader`に`ModuleCache`（`Rc<RefCell<HashMap<PathBuf, Entry>>>`、`Entry`はdeps（各依存ファイルの
   パス+内容ハッシュ）と`Rc<Fasl>`）を追加、`try_load_module`がキャッシュ照合→ヒット時は
   `Fasl::load_into`で新ヒープへ再構築（deps全ファイルの内容ハッシュ再検証込み）。LSP起動時に
   `ModuleCache`を1つ持ち各診断/補完パスで共有。
3. **ネストしたジェネリック呼び出しの`where`境界検証**——`validate_where_bounds`の2つの穴を解消。
   (1) `Vector<U>`等に包んで転送する際の誤拒否（pin比較で開いた型変数を含む場合はスキップし
   単型化時の再検証に委譲）、(2) 裸の型変数がboundなしで外側関数へ転送されるケースの見逃し
   （`caller_bounds`を新設して外側のwhere節に一致するboundが宣言済みか照合）。
4. **`Option`/`Result`型グローバルのcompile参照**——`RtValue::Data` ⇔ compiled sum-ADTボックス
   （malloc配列`[variant, fields...]`）の相互変換を新設し、JIT/AOT双方から`Option`/`Result`型
   `defvar`の読み取り・書き込みが可能に。~~ユーザー定義`defenum`型グローバルは今も対象外~~
   **→ 2026-07-16 解消**（`TopLevel::Defenum`へのvariantフィールド型焼き込み+`Interp::enum_defs`）。
   ~~これも含め`RtValue::Data`⇔malloc箱という二重表現自体~~ **→ 2026-07-16 enum表現統一で解消**
   （branch `feature/enum-heap-unification`、[[typelisp-enum-heap-unification]]参照）:
   enum値（`Option`/`Result`/ユーザー`defenum`）をdefstructと同じGCヒープオブジェクト
   （`BoxedObj::Enum`、`rt_data_new`/`rt_data_variant`/`rt_data_field`）に統一し、
   interp/compiled両側が同じ表現を共有するようになった。旧`data_to_box`/`decode_data_value`/
   kind=10特殊経路は全廃、`global_field_kind`は他の値同様`kind=6`（struct_field_kind一本化）。
   ただしLLVMハンドル等ヒープ非対応型でインスタンス化されたenum（`Option<llvm-value>`——
   自己ホストコンパイラ本体が多用）は`RtValue::Data`のnativeフォールバックとして存続
   （`Scope<V>`と同型の二重表現、`Checker::is_heap_repr`/`Interp::enum_fields_representable`
   が再帰的に判定）。これにより「enum値のネストしたOption等フィールドのencode不可」
   「enum引数/戻り値のcall_compiled非対応」の2つの残課題も解消。
5. **quoted data内の`Symbol`/`Path`のcompile対応**——`'foo`/`'(a b c)`/`'dep::head`が
   compileを通るように。`rt_intern_symbol`/`rt_intern_path`を新設、`str`リテラルと同じ
   「タグ付き`Sexpr`をrt呼び出しで構築」方式。
6. **ユーザー定義関数のLLVMシンボルに`tl_`プレフィックス**——LLVMの`frem`命令がlibmの`fmod`
   シンボル呼び出しへlowerされるため、`fmod`という名のユーザー関数をcompileすると無限再帰して
   いた（旧「許容された既知のギャップ」）。`USER_SYMBOL_PREFIX = "tl_"`を導入し、通常呼び出し・
   関数値化・メソッド呼び出し・JIT/AOT双方のシンボル解決すべてに一貫適用して解消。
   `sexpr-car`/`sexpr-cdr`/`sexpr-cons`は`rt_car`/`rt_cdr`/`rt_cons`へ直接書き換わる
   コンパイラ組み込み経路のため意図的にプレフィックス対象外。
7. **`float->int`のLLVM `fptosi`飽和化**——素の`fptosi`命令はNaN/範囲外入力でpoison値になり
   インタプリタの`float_to_int`（Rustの`as i64`、飽和キャスト）と食い違っていた（旧「許容された
   既知のギャップ」）。`llvm.fptosi.sat`intrinsic（`i64`/`f64`でオーバーロード）へ切替え、
   NaN→0・+inf/オーバーフロー→`i64::MAX`・-inf/アンダーフロー→`i64::MIN`をインタプリタと一致させた。

### マルチファイル/LSP関連（2026-07-11のファイル↔モジュール対応実装の残課題）

- ~~**マクロ展開が生成した`use`はロードされない**~~ **→ 2026-07-15 解消**。根本原因は2つの
  複合だった: (1)checkerがトップレベルのマクロ呼び出しを式として扱い、展開結果の`use`が
  トップレベル専用の`check_use`に到達しない、(2)依存ロードの事前走査（`scan_form`）は
  マクロ展開前に走るため展開結果が映らない（`defmacro`の登録自体がチェックループの副作用
  なので、一括の事前走査では原理的に展開不可能）。対応: `Checker::try_expand_toplevel_macro`
  を新設し、`check_form_dispatch`が展開結果をトップレベル形として再ディスパッチ
  （`use`だけでなく`defun`/`module`等の定義形の生成も可能に）、Loaderのチェックループが
  フォームごとに事前展開→展開結果を`scan_form`で走査（依存ロード）→展開済みフォームを
  チェック（二重展開なし）。チェック途中の依存ロードは`Checker::suspend_ns_context`で
  名前空間を退避（現在ファイルのモジュール配下へのネスト登録を防ぐ——重要な罠だった）。
  副産物: 式位置の`use`/`module`は「トップレベル専用」の明確なエラーに（従来は紛らわしい
  "unbound variable"）。マクロ生成`use`はマクロ定義より後のフォームでのみ機能するが、これは
  バグではなく仕様（単一パスチェックの原理的帰結、`language-design.md`§3の`defmacro`仕様に
  明記）。同日中に追加解消: モジュール越しのマクロ呼び出し（`resolve_macro_path`を新設、
  `mod::macro-name`が式位置・トップレベル双方で解決可能に）、quoted data内の`::`パス
  （`QuotedSexpr::Path`を新設、`'(dep::head)`が型チェックを通るように——`FASL_FORMAT_VERSION`を
  3へbump）。compile（LLVM）側はPathも`Sym`/`Bignum`/`Ratio`同様unsupportedのまま。
  テスト: `tests/macro_use_test.rs`。
- ~~**LSPの依存キャッシュなし**: 診断パスごとにpreludeソース(822行)をparse+型チェックし直す~~
  **→ 2026-07-14 解消**（真のコスト要因はprelude再ロードだった）。fasl(コンパイル済みモジュール)
  機構を新設し、preludeを一度だけチェックして`Fasl`(ヒープ非依存のシリアライズ)化、各診断パスは
  `Fasl::load_into`で新ヒープへ**確保API経由の再構築**（parse/型チェックなし、値はheap.cons等で
  作り直すのでポインタ問題なし）。LSPは起動時に`Rc<Fasl>`を1つ持ち各パスで再利用。詳細は
  [[typelisp-fasl-compiled-modules]]、`src/fasl.rs`。ユーザー面には`(load "path")`フォームと
  `typl compile-module`サブコマンド、CLI/REPL起動の`prelude::load_cached`（`$TYPL_CACHE_DIR`または`~/.typl/cache`にキャッシュ）
  として一般公開。~~**残**: 依存*ファイル*(use先)自体のパス間キャッシュは未対応~~
  **→ 2026-07-15 解消**（上記「既知の制限・意図的に対象外7項目の解消」2.参照）。
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

直近完了: **リーダーの真のspan対応**（2026-07-14、branch `feature/reader-spans`）——
「reader全体の作り替えが必要」として見送っていたが、位置テーブル機構（`cons_locs`/
`elem_locs`）はそのままに`Loc`へ`end_line`/`end_col`（排他的終端）を追加する増分で解消。
`Reader::read_datum_spanned`が全datumの開始〜終了を捕捉（末尾空白/コメントを消費する
read関数が無いため追加演算ゼロ）、`read_list`はcons_locが`(`〜`)`の完全spanに、
quote/quasiquote/unquote/unquote-splicing合成リストにも明示括弧と同等のcons_loc/elem_loc
を付与（従来は一切位置が付かなかった）。トップレベル裸アトム（例: 単独の`42`）は
`read_all_in_spanned`が`(Value, Loc)`で返しそのspanを`Checker::check_form_at`の`loc_hint`
へ渡すことで初めて位置を持つ。LSP側は`locate_node`が「包含優先(`start<=cursor<end`)+
非包含フォールバック(旧点近似)」の2段探索に、`loc_to_range`が実spanでrange構築（診断の
アンダーラインがフォーム全体に）、hoverにもrangeを付与。`FASL_FORMAT_VERSION`を2へbump
（`Loc`が`DefLocsRepr`/`Typed.loc`両方でserdeシリアライズされるため）。テスト:
read_test 10件・lsp_locate_test 4件追加、miri(read_test/mem_test) green。

`compile`（LLVM JIT/AOT）機能は現状「今のフェーズが実際に使うASTノードだけ本実装、それ以外は
`ast_bridge`が`(unsupported "<Variant>")`を返しコンパイラ本体が明示的にpanicする」設計
（`ast_bridge.rs`冒頭のdocコメント参照）。ユーザーが普通に書けるコードから実際に到達しうる
`unsupported`は2026-07-12時点で**解消済み**——残る`Expr::TraitCall`は単型化後に到達不能な
診断専用ノードと確認済みで対象外（`tests/trait_test.rs`参照）。以下、直近の完了分。

直近完了: **`bignum`/`ratio` の compile 対応**（2026-07-15）——下記の「残る compile 未対応」
（`float->bignum`/`float->ratio`のみ）が実は氷山の一角で、`bignum`/`ratio`型そのものに
compiled表現が一切無い状態だったのを解消。設計: `bignum`/`ratio`は`num_bigint::BigInt`/
`num_rational::BigRational`という可変長Rust構造体で固定bit幅のFFI安全なレイアウトを持たない
ため、`f64`のような「ネイティブ表現+box化」の二重化はできず、`Type::Str`と同じ
「常にタグ付き`TAG_BOXED`ポインタ」規約に統一。`crates/typelisp-rt`に`rt_bignum_new`/
`rt_ratio_from_bignums`（構築）・`rt_bignum_add/sub/mul/div/mod`/`rt_ratio_add/sub/mul/div`
（二項演算）・`rt_bignum_cmp`/`rt_ratio_cmp`（3-way比較、`str-lt-call`と同じ「1プリミティブから
6比較を派生」手法）・変換一式（`rt_int_to_bignum`/`rt_int_to_ratio`/`rt_float_to_bignum`/
`rt_float_to_ratio`/`rt_bignum_to_int`/`rt_bignum_fits_i32`+`rt_bignum_to_int_raw`（`try-bignum
->int`用Option二段呼び出し）/`rt_bignum_to_float`/`rt_bignum_to_ratio`/`rt_ratio_to_bignum`/
`rt_ratio_to_float`/`rt_ratio_numerator`/`rt_ratio_denominator`）を新設（`num-bigint`/
`num-rational`/`num-traits`を直接依存に追加）。ゼロ除算は`BigInt`/`BigRational`のDiv実装が
生Rust panicを起こすため、`extern "C"`境界越えUBを避けて演算前に明示チェック→`fatal`。
`ast_bridge.rs`側: `struct_field_kind`/`binding_kind`に`Type::Bignum`/`Type::Ratio`を
`Str`と同じ扱いで追加（**後者はGC安全性の必須修正**——ヒープ参照なのにルート登録されない
バグになるところだった）、`Expr::Bignum`/`Expr::Ratio`・`QuotedSexpr::Bignum`/`Ratio`を
`unsupported`から実リテラル構築（`bignum_literal_form`/`ratio_literal_form`、`str_literal_form`
と同じ「符号+桁を`(int _)`列として埋め込み、`rt_bignum_new`呼び出し」方式）に変更。
`compiler.rs`側: `compile-bignum-literal`/`compile-ratio-literal`、
`bignum-native-method?`/`ratio-native-method?`、`compile-assoc`への分岐追加、
`compile-sexpr-field`/`compile-construct-sexpr`のvariant 8/9を「panic」から`str`と同じ
passthrough に変更。`interp.rs`側: `call_compiled`の引数エンコード（`RtValue::Bignum`/`Ratio`
を都度ヒープへclone+root、`Str`と同型）と戻り値デコード（`Type::Bignum`/`Ratio`は
`Type::Named`でないため`is_boxed_sexpr_type`に掛からず、専用分岐が必須だった）、
`rt_extern_functions`への26関数追加（JIT `add_global_mapping`用、AOTも同じ関数を再利用）、
`compile_function`の「ネイティブ受け皿型」除外リストに`bignum`/`ratio`追加。
テスト: `compile_test`に13件追加（リテラル・四則演算・比較・全変換・`try-bignum->int`の
Some/None両方、bignum/ratio双方）。落とし穴: 自己ホストコンパイラ本体（`compiler.rs`の`SOURCE`
文字列）は巨大なS式で、括弧の数え間違いが「離れた場所の`if`アリティ不整合」として現れ
デバッグが難航——コメント/文字列/`#\c`文字リテラルを正しく読み飛ばす簡易パーサをその場で
書いて特定した。既存テスト2件（`float->bignum`/`sqrt`をnon-native `f64`メソッドの例に使っていた
もの）は本対応で前提が崩れたため`i64::int->char`（今も非ネイティブ）に差し替え。

直近完了: **`f64` の超越関数/丸め関数 + `float->int` の compile 対応**（2026-07-15）——前回の
`f64`算術/比較 compile対応（下記参照）で「残」としていたtranscendental（`sqrt`/`floor`/`ceiling`/
`round`/`truncate`/`expt`）と`float->int`を解消。算術と同じく「compiled `f64`は生bitパターンを
`i64`に埋め込む表現」を維持したまま、各操作をLLVM組み込み関数（`llvm.sqrt.f64`等、`expt`は
`llvm.pow.f64`）にlowering——新設ビルトイン`build-fsqrt`/`build-ffloor`/`build-fceil`/
`build-fround`/`build-ftrunc`/`build-fpow`が`Intrinsic::get_declaration`でモジュールへ宣言
（同一モジュール内の再呼び出しに対して冪等なため`get-function`のような事前存在チェック不要）
した上で`bitcast`+`build-call`+`bitcast`を行う（`build-fadd`等と同じ「i64-in/i64-out」規約）。
`float->int`は`build-fptosi`——単一の`fptosi`命令のみ、ヒープ確保・モジュール引数とも不要。
`compile-assoc`のf64分岐は先に単項（`a`のみ必要）を判定してから二項（`b2`も必要）へフォール
スルーする構造に組み替え——元の構造は無条件に`b2`も評価していたため、単項メソッドをそのまま
追加すると存在しない2引数目を読もうとして失敗する。~~**残る compile 未対応**:
`float->bignum`/`float->ratio`のみ——`bignum`/`ratio`はcompiled表現が一切無い~~
**→ 2026-07-15 解消**（上記「`bignum`/`ratio` の compile 対応」参照）。~~落とし穴: LLVMの`fptosi`はNaN/範囲外入力に対し値未定義
（poison）——インタプリタ側`float_to_int`（Rustの`as i64`、飽和的キャスト）とはその境界ケースのみ
挙動が発散するが、`frem`/`fmod`名前衝突と同種の「許容された既知のギャップ」として明記に留める。~~
**→ 2026-07-15 `llvm.fptosi.sat`intrinsicへ切替えて解消**（上記「既知の制限・意図的に対象外7項目の解消」7.参照）。
テスト: `compile_test`に4件追加(当時)（transcendental 5種一括+expt+float->int+既存2件の対象差し替え
`sqrt`→`float->bignum`——sqrt自体がcompile可能になったため）。

その前に完了: **Iter トレイトを持つ全型（Vector/HashTable）の compile 対応**（2026-07-13、branch
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
→ transcendental全種+`float->int`は2026-07-15解消（上記参照）、残るは`float->bignum`/`float->ratio`のみ。
~~落とし穴: LLVM `frem`はCの`fmod`呼び出しにlowerされるため`fmod`という名の関数compileはJITシンボル
解決衝突で無限再帰。~~ **→ 2026-07-15 `tl_`シンボルプレフィックス導入で解消**（上記
「既知の制限・意図的に対象外7項目の解消」6.参照）。テスト: compile_test 3件 + typelisp-rt（既存流用）。

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
グローバルはcompile対象から参照できない——既知の制限）。~~`Option`/`Result`型グローバルの
compile参照は2026-07-15解消~~（`data_to_box`/`decode_data_value`新設、上記「既知の制限・
意図的に対象外7項目の解消」4.参照）。~~ユーザー定義`defenum`型グローバルは今も対象外~~
**→ 2026-07-16 解消**（同4.の追記参照——`TopLevel::Defenum`への焼き込み+`Interp::enum_defs`）。
~~`data_to_box`/`decode_data_value`という変換自体~~ **→ 2026-07-16 enum表現統一で全廃**
（同4.の追記参照）。JITは参照時に遅延昇格
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
