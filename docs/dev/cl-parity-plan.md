# CL 残差を埋める実装計画 — `cl-missing-classes-and-methods.md` の全行を Phase へ

作成: 2026-08-20 / 状態: **未着手**（Phase 0 の検証もまだ）

[cl-missing-classes-and-methods.md](cl-missing-classes-and-methods.md) は「ANSI Common Lisp に
存在して typelisp に無いもの」の**棚卸し地図**であって TODO ではない、と自ら明記している（同 §4）。
[TODO.md](TODO.md) の「残っている作業」も空で、次の着手候補としてあの地図の §3 を指しているだけ。

つまり今あるのは測定結果だけで、**実行計画が無い**。このドキュメントがそれである。
地図の ❌ 82 行 / ⚠️ 50 行 / ⛔ 57 行を全部拾い、対象外の理由を書くか、Phase へ落とす。
**付録 A の対応表が「地図の全行がこの計画に写像された」ことの証拠**になっている。

このドキュメントは計画であって仕様ではない。実装した Phase は
[implementation-log.md](implementation-log.md) へ経緯を書き、ここには完了マークと
「計画から変えた点」を追記する（[two-pass-toplevel-plan.md](two-pass-toplevel-plan.md) と同じ運用）。

---

## 0. 対象外（やらないと決めたもの）

地図の ⛔ のうち、下記は本計画のスコープ外。理由は
[language-design.md](language-design.md) §7〜§9 が一次情報で、ここはその要約。

| 群 | 根拠 |
|---|---|
| **実行時の型問い合わせ** — `typep`/`type-of`/`subtypep`/`class-of`/`find-class`/MOP/`typecase`/`etypecase`/`ctypecase`/`check-type`/`numberp` 系/`characterp`/`stringp`/`arrayp`/`vectorp`/`streamp`/`hash-table-p`/`bit-vector-p`/`describe`/`describe-object`/`inspect`/`apropos`/`apropos-list` | (D1) 静的型付け。すべての式の型がコンパイル時に決まるので、実行時に型を尋ねる問い自体が成立しない。`Sexpr` に限れば `match` と `sexpr-consp`/`sexpr-symp` 等が相当する |
| **`null` 相当** — 型としての `null`、`endp`、nil を「偽・空リスト・失敗」の三役に使う慣用 | (D2)。偽は `false`、空リストは `Sexpr::Nil`、「値が無い」は `Option<T>`。三役の合流はバグの温床でしかない |
| **多値** — `values`/`values-list`/`multiple-value-bind`/`multiple-value-call`/`multiple-value-list`/`multiple-value-prog1`/`multiple-value-setq`/`nth-value`、`floor` の商剰余多値、`read-from-string` の第2値、`gethash` の第2値 | 非採用。複数の結果は `cons-cell` か `defstruct`、「あるか無いか」は `Option<T>` で表す。既存の `floor-div` 系（`cons-cell` 返し）と `get`→`Option<V>` を正とする |
| **コンディション本体** — `define-condition`/`make-condition`/`signal`/`cerror`/`break`/`handler-case`/`handler-bind`/`ignore-errors`/`restart-case`/`restart-bind`/`with-simple-restart`/`invoke-restart`/`find-restart`/`compute-restarts`/`abort`/`continue`/`muffle-warning`/`store-value`/`use-value`/`invoke-debugger`/`*debugger-hook*`、標準コンディション型階層 | (D3)、language-design.md §9 で 2026-08-16 に非採用が確定。「この関数が何を signal しうるか」が型に出ない。**ただし `Error` トレイトへ統合できる分は実装する — Phase 7a** |
| **CLOS** — `defclass`/`make-instance`/`slot-value`/`with-slots`/`with-accessors`/`slot-boundp`/`slot-makunbound`/`defgeneric`/`call-next-method`/`next-method-p`/`:before` `:after` `:around`/`define-method-combination`/`initialize-instance`/`shared-initialize`/`reinitialize-instance`/`change-class`/`update-instance-for-*`/`make-load-form` | `deftrait`/`impl`/`:dyn` と `defstruct`/`defenum` で置き換える既定方針（language-design.md §5.1/§5.2/§6）。単一・静的ディスパッチであることが型システムの前提になっている |
| **ワイルドカードパス名・論理パス名**、パス名のホスト/デバイス/バージョン成分 | language-design.md §9。この処理系が走る環境に対応物が無い |
| **`set-pprint-dispatch` / `*print-pprint-dispatch*` / `copy-pprint-dispatch`** | language-design.md §9。`print-object` トレイトで置き換え済み |
| **`input-stream-p` / `output-stream-p` / `stream-element-type`** | 方向も要素型も型が持つ（functions.md §18.7） |
| **`?`/`try` 構文、`!`/`?` の命名接尾辞** | language-design.md §7.3 |
| **`Sexpr` への `Iter<Item>` 実装** | language-design.md §9。要素型が固定されないリストにジェネリックな `Iter<Item>` を被せるのは型システム上不適切。反復手段は `dolist` |

### 当初 ⛔ だったが、本計画では**採る**もの

- **(D4) 破壊的操作**: 実現可能なものは入れる。`Vector<T>` は参照型でヒープ上を直接変異するので
  `nreverse`/`delete`/`nsubstitute`/`fill`/`replace`/`vector-push-extend` は素直に載る。
  `rplaca`/`rplacd` は `cons-cell` の `set-car`/`set-cdr`。`nconc` だけは共有構造の書き換えなので
  Phase 0.4 で可否を判定する。→ **Phase 3d**
- **(D5) 動的束縛**: typelisp に対応物がある制御変数は全部入れる。`let` の意味は変えず、
  「保存 → 代入 → `unwind-protect` で復元」するスコープ付き再束縛で CL の用法を賄う。
  `unwind-protect` は 2026-08-16 に実装済みなので新しい機構は要らない。→ **Phase 7b**
- **小整数型・`f32`**: CL 残差ではないが typelisp 側の穴（型登録だけで演算が 1 つも無い）。
  演算をトレイト化したうえで全型に付ける。→ **Phase 1a/1b**

---

## 1. 設計原則（全 Phase を貫く 3 つ）

### 1.1 新しい API は Rust 風・受け手優先

- **生成は静的メソッド `Type::new`**。`(Vector::new)` / `(HashTable::new)` の既存形に揃える。
  CL の `make-array`/`make-string`/`make-hash-table` のような `make-*` 自由関数は一次 API にしない。
- **操作は `defmethod`（第一引数＝受け手の型でディスパッチ）**。`array-rank`/`hash-table-count` の
  ように型名を関数名へ埋め込む CL の命名は一次 API にしない——受け手の静的型が既に型を語っている。
- **CL 互換の名前は薄い別名に留める**。アリティで解決できない CL 名（`(aref a i j k)` の可変添字）は
  `defmethod` では表せないので、**checker の糖衣**で受け手優先形へ展開する。
- **既存 API は改名しない**。`make-random-state`/`make-pathname`/`make-string-input-stream` 等は
  2026-07〜08 に確定した形。この原則が効くのは新規に足すものだけ。

### 1.2 CL のリスト関数は `defmethod` で載せる（`Sexpr` の降格は維持）

1 つの土台を選ぶのではなく、**同じ CL 名を受け手型ごとに `defmethod` で定義する**。
`(length x)` / `(first x)` / `(reverse x)` は `x` の静的型で行き先が変わる。

| 受け手 | 例 |
|---|---|
| `Vector<T>` | `(defmethod first ((self Vector<T>)) Option<T> ...)` |
| `cons-cell<A,B>` のネスト | `(defmethod cadr ((self cons-cell<A,cons-cell<B,C>>)) B (car (cdr self)))` |
| 任意の `Iter` 実装 | 既存の `where (Iter I (Item A))` ジェネリック `defun`（`map`/`filter`/…）を維持 |

**`Sexpr` の降格はそのまま維持する**（`prelude.rs` 95-104 行のコメントは書き換えない）。
あのコメントが問題にしていたのは `length`/`append`/`nth`/`member`/`sort`/`assoc` が
**`Sexpr` 専用の自由関数として裸の CL 名を独占していた**ことで、受け手型でディスパッチする
`defmethod` にすれば独占は起きない——`Sexpr` をユーザ向けリスト型に戻さずに CL 名を復活できる。
したがって本計画は **`Sexpr` を受け手に取る CL リストメソッドを足さない**。`Sexpr` は今後も
read/eval/print/`defmacro`/自己ホスト島の内部型で、`sexpr-*` アクセサ層のままとする。

`(member x lst)` のような item-first の CL 引数順は、既存の
`Checker::try_instance_method_swapped`（checker.rs:9444。2 引数のとき引数を入れ替えて再試行する
汎用フォールバック）がそのまま吸収する。

**この方針の代償**: `cons-cell<A,B>` には空リストに当たる構成子が無いので「終端付きの真のリスト」を
表せない（`(list 1 2 3)` 特殊形が作るのは `Sexpr`）。よって `first`/`rest`/`member`/`union` 等の
受け手は実質 `Vector<T>` と `Iter` 実装型に限られ、`caar`〜`cddddr` は「ネストしたペア」に対する
型付きアクセサとして載る。CL コードの移植ではリスト構築側の書き換えが要る——これは
prelude.rs:95-104 が既に選んでいた道（"redesigned on top of `Vector<T>` and the generic
`cons<T,U>` pair"）そのもので、新しい代償ではない。Nil 終端の同種連結リスト型 `List<T>` を
別途足す案は**採らない**（`Vector<T>` と重複し、`Iter` 上のジェネリック関数がそのまま使える）。

### 1.3 数値は Rust 流の演算トレイトに再編する

`i8`/`i16`/`u8`/`u16`/`u32`/`u64`/`isize`/`usize`/`f32` は現在**型登録だけでメソッドが 1 つも無い**
（registry.rs:551-576 の `_ => BTreeMap::new()`。`+` すら無い）。ここに全演算を入れ、さらに
演算ごとのトレイト（`Add`/`Sub`/`Mul`/`Div`/`Rem`/`Neg`/`Bits`）とそれらを束ねる `Number` トレイトを
置き、全数値型どうしの相互変換を揃える。スーパトレイト機構は 2026-08-01 に実装済み。

---

## 2. 検証済みの事実（実装前提）

策定時に実コードで確かめた。**表を根拠に「無い」と判断する前に必ず grep する**という
地図 §4 の警告はこの計画にも掛かる。

1. **追加は prelude が第一選択**。`registry.rs` の `int_assoc`（1772行）の doc コメントが
   「真にプリミティブな機械命令が要る演算だけをここに置き、CL の残りのカタログ
   （`rem`/`abs`/`signum`/`gcd`/`lcm`）は prelude の typelisp メソッドで書く」と明言している。
2. **prelude だけに足したなら compile 対応は自動**。`driver.rs:519 precheck_compilable` →
   `call_graph_edges` の推移的呼び出しグラフと、`prelude_bootstrap.rs` の「収集して残ったものは
   全部コンパイル」規則による。穴が開けば `PRELUDE_COMPILE_UNSUPPORTED`
   （`prelude_bootstrap.rs:140`、**現在空**）の再計算が毎ビルドで落ちて教えてくれる。
3. **境界付きジェネリックは別の境界付きジェネリックを呼べない**（prelude.rs 1140-1148 行の
   コメント。`elt` が `nth` のループを複製しているのはこのため。`check_call` の境界検証が
   `cannot infer` で落ちる）。Iter 系を 1 つ足すたびに `doiter` ループを書き下ろす必要がある
   — **工数見積りに直結する最大の制約**。
4. **Rust builtin を足す場合の触点は 8 箇所**:
   `check/registry.rs`（シグネチャ）→ `eval/interp.rs`（`eval_builtin` 1595 / `eval_builtin_method`
   2849）→ 必要なら `crates/typelisp-rt/src/lib.rs` の `rt_*` シム →
   `src/compile/externs.rs` の 3 表（`rt_builtin_symbol` 43行 / `native_lowered_primitive_methods`
   190行 / `rt_extern_functions()`。**配列長も手で直す**）→ `src/compiler.rs` SOURCE の
   `*-native-method?` と `compile-assoc` の lowering → `scripts/regen-compiler-island.sh` →
   `scripts/regen-prelude-bitcode.sh`（**島が先**）→ エディタ定義 2 箇所 → `docs/functions.md`。
   **Rust 側の表だけ足して島の lowering を足さないと、島の `get-function` がエラー報告ではなく
   プロセスを abort する**（externs.rs 210行付近の警告。`char->string` で実際に起きた）。
   番人は `the_rust_and_island_native_method_lists_agree`。
5. **新しい特殊形は `checker.rs` 7501-7578 行の `// SPECIAL-FORM DISPATCH BEGIN/END` sentinel 間に
   書く**（`tests/editor_keyword_sync_test.rs` がこのブロックを読んでエディタ定義と突き合わせる）。
   予約語判定は `is_builtin_form_head`（2295行）。
6. **`setf` 等の保護された組み込み形を呼ぶ必要がある形は checker 側に置く**（`defmacro` 展開からは
   呼べない。checker.rs 9139-9146 行のコメント）。それ以外は prelude の `defmacro` で書くのが方針。
7. **糖衣展開の定型**: `heap.intern_symbol` → `forms::list_from_vec_locs` /
   `forms::wrap_let_star` → `heap.push_root` → `self.check(...)` 再帰 → `heap.pop_root`
   （`check_variadic_arith` checker.rs:9260 が最短の実例）。
8. **`defmethod` は `&rest` すら受け付けない**（`parse_defmethod_sig_inner` checker.rs:6641 が
   `parse_param_pairs` を呼ぶだけ）。`lambda` は `&rest` のみで `&optional`/`&key` は
   checker.rs 7671-7674 で明示的にエラー。**地図 §3-2 の「`lambda` と `defmethod` は `&rest` のみ」は
   `defmethod` について不正確**——この計画の副産物として地図も直す。
9. **`defstruct` にオプションリスト構文自体が無い**（`parse_struct_fields` checker.rs:4457 は
   `(name type)` / `(pub name type)` の 2/3 要素しか受けず、`variants` は `"new"` 単一で固定）。
   スロット初期値・`:include`・BOA コンストラクタは「未実装」ではなく「置き場所が無い」。
10. **fasl / `~/.typl/cache` はもう存在しない**（2026-08-14 削除）。今の等価物はコミット済みの
    `.typld` ダンプで、これはキャッシュではなく成果物。digest 不一致は黙って無視されず、
    再生成スクリプト名つきのエラーになる。
11. **`Error` トレイトの具象型は現在 5 つ**: `ParseIntError`/`ParseFloatError`/`ReadError`/
    `EvalError`/`FileError`（`registry.rs` の `builtin_error_defs` 1008行）。
    **`docs/functions.md` §7.1 の表は 4 つしか載せておらず `FileError` が漏れている**。
12. **`panic` は `catch`/`throw` と別系統で捕捉手段が無い**。`catch` は `EvalError::Throw` しか
    見ない（`core_eval.rs` 400-428行）。`unwind-protect` の cleanup だけは走る（同 435-460行）。

---

## Phase 0 — 実装前に確かめる 6 件

いずれも使い捨てテストで 1 時間以内に決着する。**結果が後続 Phase の形を変える**ので先に潰す。

| # | 確かめること | 結果が変えるもの |
|---|---|---|
| 0.1 | `defmethod` の受け手を型変数にして `where (Iter I ...)` で束縛できるか | できないなら Phase 3a の CL 名は受け手型ごとに列挙するしかない（工数 × 型数） |
| 0.1b | `defmethod` の受け手に**ネストした型引数**を書けるか（`(defmethod cadr ((self cons-cell<A,cons-cell<B,C>>)) B ...)`。頭は `cons-cell` だが第 2 引数が構造化された型） | 書けないなら `caar`〜`cddddr` 28 個だけ `defun` に落とす（§1.1 の原則が崩れる唯一の候補なので早めに潰す） |
| 0.2 | `deftrait Add` のメソッド名を `+` にすると、`impl Add i32` の本体 `(+ self other)` が自分自身への再帰になるか（ユーザ `defmethod` は builtin より先に解決される） | なるなら Phase 1a はトレイト側を `add`/`sub` 等の名前にし、**型変数受け手の `(+ a b)` を checker で `(add a b)` へ脱糖する**方式に切り替える（策定時の推奨案） |
| 0.3 | `&key` のデフォルト式に `identity` を置いたとき、戻り型の型変数 `K` が単型化で決まるか（`&optional`/`&key` の宣言型は自分の型パラメータを含められない、checker.rs 3231/3243 の制約に触れないか） | 決まらないなら Phase 3e の `:key` は `-by` 系の別名関数にする |
| 0.4 | `Sexpr` の cons セルを in-place で書き換える手段が現存するか（cons セル化以降、位置情報が同じセルに同居している） | `nconc`/`rplaca` の `Sexpr` 版の可否（Phase 3d） |
| 0.5 | prelude に `Number` のようなメソッド数の多いトレイトを足したとき、単型化とダンプ生成が破綻しないか（現在 prelude の 86 defun 中 61 がジェネリック） | Phase 1a の刻み方。`scripts/bench-prelude.sh` で起動時間も測る |
| 0.6 | `loop` の第 1 要素がキーワードかどうかで DSL と無限ループを分岐できるか（CL 自身も `(loop body...)` の simple loop 形を持つので、規則としては CL 準拠） | Phase 4b の実装形（checker 糖衣か、巨大な `defmacro` か） |

### Phase 0 の結果（2026-08-20 実施・完了）

| # | 結果 | 後続への影響 |
|---|---|---|
| 0.1 | **裸の型変数を `defmethod` の受け手にはできない**（`defmethod: unknown type \`i\``）。`parse_defmethod_sig_inner` の「受け手は登録済みの型を名指しすること」検査を外すのは `precheck_blanket_impl` だけ。**ただしブランケット実装 `(impl<I> Len I (where (Iter I (Item A))))` は端から端まで通る**（単型化して実行まで確認） | Phase 3a は「受け手型ごとに列挙」を回避できる。CL 名 1 つにつきトレイト 1 つ＋ブランケット実装 1 つで全 `Iter` 実装型に一括で載る（1 トレイトにブランケット実装は 1 つまで、という既存規則が「名前ごとにトレイトを立てる」形を強制する） |
| 0.1b | **できない**。受け手の*形*としてネストは通る（`cons-cell<i32,cons-cell<i32,i32>>` は動く）が、**ネスト位置に現れる自由型変数が束縛されない** — `check_defmethod` の `written_vars` は受け手の型引数の**最上位**にある裸の型変数しか拾わず、`written_vars.len() == owner_params` を満たさないと総称テンプレート化されないため、`cons-cell<B,C>` が未知の具体型として扱われて `car` が引けなくなる | 計画どおり **`caar`〜`cddddr` 28 個は `defun` に落とす**。`(defun cadr<A,B,C> ((c cons-cell<A,cons-cell<B,C>>)) B (car (cdr c)))` が動くことを確認済み。§1.1 の原則が崩れるのはここだけ |
| 0.2 | **`+` はトレイトのメソッド名にできない** — `impl` の時点で `cannot redefine built-in method \`+\`` で弾かれる（0.2 が想定した「無限再帰になる」より手前で落ちる）。策定時の推奨案どおり `add`/`sub` 等の名前にする。**副産物で checker のバグを 1 件発見・修正**: 境界越しに呼んだトレイトメソッドの戻り型 `Self` が型変数へ置換されず、`(where (Add T))` 下の `(add a b)` が `expected t, found self` で落ちていた（`check_instance_method` の bounds 分岐が associated type しか置換していなかった）。prelude のトレイトメソッドは 1 つも `Self` を返さない（全て `bool` か関連型）ため今まで露出しなかった | Phase 1a はトレイト側を `add`/`sub`/`mul`/… の名前にし、`(+ a b)` の型変数受け手を checker で脱糖する。`Self` 戻り型の修正はこの Phase の**前提条件**だったので先に入れた |
| 0.3 | **決まらない**。`&key` にデフォルト式を書くと、その型が自分の型パラメータに触れた時点で checker が明示的に拒否する（`&key parameter \`key\` may not default when its type mentions the function's own type parameter — declare it with no default (\`Option<...>\`) instead`）。**ただしデフォルト無し（`Option<fn>`）＋ `match` は端から端まで通る** — キーワードを渡した呼び出しと省略した呼び出しの両方を確認済み | Phase 3e の `:key` は **`-by` 系の別名関数にしなくてよい**。`(&key (key (fn (A) A)))` とデフォルト無しで宣言し、本体で `(match key ((some f) (f y)) ((none) y))` する |
| 0.4 | `Heap::set_car`/`set_cdr` は現存し安全に動く（`Cell.car`/`cdr` を書くだけ）が、**typelisp へは意図的に非公開**（`registry.rs` 650-657 行）。`cons-cell` の `set-car`/`set-cdr` は `defstruct` のアクセサとして既に存在し動く。**`Sexpr` 側の危険は所有権ではなく位置情報**: `Cell.car_loc` は「この `car` の span」なので、`set_car` すると**古い要素の span が残ったまま**新しい要素に付く（メモリ安全ではあるが診断が静かに狂う） | `rplaca`/`rplacd` は `cons-cell` の `set-car`/`set-cdr` への**薄い別名**で済む（Phase 3d）。**`Sexpr` 版の `nconc`/`rplaca` は入れない** — §1.2 の「`Sexpr` を受け手に取る CL リストメソッドを足さない」と、上の `car_loc` 陳腐化の両方が同じ結論を指す。`nconc` は `Vector<T>` 受け手（b の要素を a へ破壊的に足す）としてのみ実装する |
| 0.5 | Phase 1a の実施と不可分なので、そこで実測して記録する（着手前の起動時間を基準値として取る） | — |
| 0.6 | **できる**。`(loop :for i :from 1 :to 3 :collect i)` は**構文としては受理され**、本体を検査した先の `unbound variable: i` で落ちた（`loop` は本体を任意のフォーム列として受ける）。キーワードは `Value::Symbol` で名前が `:` 始まり（`checker.rs` 7322 行の自己評価分岐と同じ判定）なので、`check_loop` が `args[0]` を見て分岐できる | Phase 4b は **checker 糖衣**で実装する。`defmacro` 側には `args[0]` の種別で分岐する手段が無い |

### Phase 0 が計画本体に強いた訂正

- **§2-3 の「境界付きジェネリックは別の境界付きジェネリックを呼べない」は誤りだが、
  最初に書いた訂正も誤りだった（Phase 3 で発覚、両方ここに残す）。**

  当初の検証では `(defun mylen2<I,A> ((it I)) i32 (where (Iter I (Item A))) (mylen it))` が
  通ったので「制約は既に解消済み」と結論した。Phase 3a で `assoc-if` が `find-if` へ委譲
  しようとして落ち、**通っていたのは委譲が実装されていたからではなく、呼び出し側と呼ばれ側が
  たまたま同じ文字（`A`）で項目型変数を綴っていたから**だと分かった。
  `validate_where_bounds` は呼ばれ側の宣言されたピン（呼ばれ側の型パラメータで書かれている）と
  呼び出し側のピン（呼び出し側の型パラメータで書かれている）を**素のまま**比較していた。
  `prelude.rs` の `elt` が `nth` に委譲できていたのも同じ偶然（両方 `A`）で、`B` と綴れば落ち、
  ピンが構造化された型（`(Item cons-cell<K,V>)`）なら一度も一致しなかった。

  **修正済み**: 呼ばれ側のピンをその呼び出しの `subst` で解決してから比較する。
  回帰テストは `generic_defun_test` の
  `a_bounded_generic_can_delegate_when_the_item_variable_is_named_differently` と
  `..._with_a_structured_associated_type_pin`。これで Phase 3 の全定義が委譲で書けている
  （`first` → `nth`、`remove` → `remove-if`、`assoc-if` → `find-if`、…）ので、
  「Iter 系を 1 つ足すたびにループを書き下ろす」前提で積んだ工数はやはり不要。
  `prelude.rs` 1145-1148 行のコメントは付録 C で直す。

  **教訓**: 「動いた」を根拠に制約の不在を結論しない。動いた例が*なぜ*動いたかを確かめる。

---

## Phase 1 — 数値層

### Stage 1a — 演算トレイト化と全型カタログ

- `deftrait Add` / `Sub` / `Mul` / `Div` / `Rem` / `Neg` / `Bits`、それらを束ねる
  `deftrait Number (Add Sub Mul Div Rem Neg Ord)`。メソッド名は Phase 0.2 の結果に従う。
- **既存の `+`/`-`/`*`/`/` 等は builtin インスタンスメソッドのまま残す**（LLVM 直結の速い経路を
  壊さない）。トレイトは「ジェネリックコードが数値演算を要求できる」ための層。
- `i8`/`i16`/`u8`/`u16`/`u32`/`u64`/`isize`/`usize` に `int_assoc` 相当、`f32` に `float_assoc`
  相当を回す（registry.rs:551-576 の `_ => BTreeMap::new()` を埋める）。
- 触点は §2-4 のフルセット（registry / interp / rt / externs 3 表 / 島の `*-native-method?` と
  lowering）。**型が増えるぶん島の分岐も増える**ので、1 型ずつ入れて毎回
  `the_rust_and_island_native_method_lists_agree` を回す。

### Stage 1b — 全数値型間の変換

`as` / `try-as` の変換表を全ペアへ拡張（現状は `i32`/`i64`/`f64`/`bignum`/`ratio`/`char` 間のみ）。
既存規約に従い、範囲外で失敗しうるペアは `as`=panic / `try-as`=`Option<T>`、拡大変換と
`float→int` の切り捨ては常に成功。変換メソッド（`int->char`/`int->bignum` 等）の命名規約も
機械的に拡張する。

### Stage 1c — CL 残差

**状態: 完了（2026-08-20）**、ただし 2 項目は保留（下記）。実装は `prelude.rs` の `SOURCE`
（`gcd`/`lcm` のアリティだけ checker 糖衣）で、`PRELUDE_COMPILE_UNSUPPORTED` に穴を開けずに
通っている。テストは `tests/numeric_catalog_test.rs`（8 本）、ドキュメントは
[functions.md](../functions.md) §1／§2／§4.1／§4.3。

入ったもの: `ffloor`/`fceiling`/`fround`/`ftruncate`（既存の `f64` `floor` 等の別名——
CL では**無印の方が整数を返す**ので、`f` 付きの方がこの言語の挙動に一致する）、
`isqrt`（`i32`/`i64`）、整数の `expt`（`i32`/`i64`、負の指数は panic）、
0/1/n 引数の `gcd`/`lcm`（`(gcd)`=0・`(lcm)`=1・1 引数は `abs`。既存の可変長糖衣に 2 語足しただけ）、
`rationalize`、浮動小数点の内部表現アクセス 7 つ、数値限界定数 10 個。

**計画から変えた点・保留**:

1. **`scale-float` と `rationalize` は `int->float` を呼ばない書き方にした。**
   Phase 2 と同じ理由（島に lowering が無い）。`scale-float` は 2 倍/半分のループ、
   `rationalize` は連分数の収束項を整数でなく `f64`（整数値を保持）で持ち、
   最後に `float->ratio` で組み立てる。最初の草稿はここで穴を 1 つ開けて止まった。
2. **保留: 乱数のシードを外から与える手段。** `random-state` を i64 から作る Rust
   プリミティブが要る（§2-4 の触点フルセット）。既存の `make-random-state-fresh` は
   引数を取らず、`random-state` に書き込む口も無いので prelude だけでは閉じない。
3. **保留: `byte`/`ldb`/`dpb`/`boole` の `i64`・`bignum` 拡張。**
   `defmethod` は受け手でしか解決せず、CL の `(ldb bytespec integer)` は指定子が先なので、
   *整数側の幅*で実装を選べない。引数順を変えて `try_instance_method_swapped` に頼るか、
   バイト指定子自体に幅を持たせるかの設計判断が要る——どちらも計画のリスク表が
   名指しする「解決順が変わって無関係な既存コードが壊れる」側なので、片手間では入れない。
4. **見つけた CL との差（直していない）**: `round` は**0 から遠い方へ**丸める
   （`(round 2.5)`=`3.0`）が、CL は**偶数側へ**丸めるので `2`。Rust の `f64::round` を
   そのまま使っている既存の挙動。`fround` を `round` と一致させる方を優先してこの差は
   引き継いだ（別々に丸める 2 つの名前が並ぶ方が悪い）。直すなら `round` 本体で、
   `round` は島が lowering しているので島側も同時に変わる。functions.md §2 に注記した。


### Stage 1d — 複素数

**状態: 完了（2026-08-21）。ただし方式を変えた。** 詳細は
[implementation-log.md](implementation-log.md) の該当節、カタログは
[functions.md](../functions.md) §2.6、テストは `tests/numeric_widths_test.rs`（6 本）。

着手前の計画は「`bignum`/`ratio` と同じ heap-boxed 方式（`TAG_BOXED` ポインタ）で新設。
前例をそのまま踏襲できる」だったが、**その前例が成り立つ理由がここには無い**——
`bignum`/`ratio` が Rust にあるのは `BigInt`/`BigRational` の算術がこの言語で書けない
からで、`f64` 2 つの複素数は `f64` の算術そのもの。prelude の
`(pub defstruct complex (pub re f64) (pub im f64))` にしたので、新しい `Repr` も `rt_*`
シムも島の lowering も成果物の手術も要らず、通常経路で compile される。

入ったもの: `complex`/`complex::new`、`realpart`/`imagpart`/`conjugate`/`phase`（実数側にも）、
`cis`、`atan2`（CL の 2 引数 `(atan y x)` はチェッカーがここへ綴り替える）、
`+`/`-`/`*`/`/`/`=`/`/=`/`Eq`、`abs`（戻りは実数）/`zerop`/`exp`/`log`/`sqrt`/`expt`、
`print-object`（`#C(re im)`）。

**CL から外れた 2 点、どちらも静的型が強いる**:

1. **成分は `f64` 固定**。CL の complex は有理数も持て `(complex 1 2)` と
   `(complex 1.0 2.0)` は別の型だが、静的型は 1 つ選ぶしかない。
2. **`(sqrt -1.0)` は実数の NaN のまま**。CL が実関数から complex を返せるのは戻りが
   合併型だから。ここでは `f64` の `sqrt` は `f64` を返すしかなく、複素数は複素数の
   引数から出る（`(sqrt (complex::new -1.0 0.0))` = `i`）。これにより計画の
   「`sqrt`/`log`/`expt`/`asin`/`acos` が CL では複素数を返す場面の再定義」は
   **実数側は据え置き**、複素数側にのみ定義した。

`Ord` は入れていない（複素数体は順序体でない。CL の `<` も複素数を撥ねる）。
`as`/`try-as` も入れていない——`f64`↔`complex` は張り替えでなく実際の構築/破棄で、
どちら向きも `complex`/`realpart` という名前のある操作で足りる。

---

## Phase 2 — 文字・文字列層

**状態: 2a / 2b とも完了（2026-08-20）。** 実装は全て `prelude.rs` の `SOURCE` に入り、
`PRELUDE_COMPILE_UNSUPPORTED` に穴を開けずに（＝JIT/AOT 対応込みで）通っている。
テストは `tests/char_string_catalog_test.rs`（18 本）、ドキュメントは
[functions.md](../functions.md) §8/§9。

**計画から変えた点 4 つ**:

1. **`upcase`/`downcase`/`alphap`/`digitp`/`int->char` を一切呼ばない書き方に統一した。**
   この 5 つは島に lowering が無い組み込みで（`externs::native_lowered_primitive_methods` の
   `"char"`/`"i32"` 行）、触れた瞬間この節の全定義がインタプリタ専用に落ちる。最初の草稿が
   実際にそうなり、`PRELUDE_COMPILE_UNSUPPORTED` の照合が 3 件の穴を報告して止めた——
   §2-2 が「穴が開けば毎ビルドで落ちて教えてくれる」と書いたとおりに機能した。
   代わりに全て `char->int` のコードポイント上で書き、文字を*作る*ところは
   `(ref "0123456789ABC..." w)` のように lowering 済みの `string::ref` で引く。
   **この 5 つの lowering を足す作業は本計画の残タスクとして別に立てる**（下記）。
2. **`digitp` は破壊的変更にしなかった。** CL 本来の重み返しは `digit-weight` という別名で
   足し、`digitp`（bool）は据え置き。prelude 自身のリーダ（`reader-scan-atom`）と島が
   述語として呼んでいるため。
3. **`trim`/`left-trim`/`right-trim`/`digit-weight`/`digit->char` は `defmethod` でなく
   `defun`。** CL がこの 5 つに省略可能引数（`bag`/`radix`）を与えており、`defmethod` は
   `&optional` も `&key` も受け付けない（§2-8）。Phase 5b が入れば `defmethod` に移せる。
4. **`(setf (char s i) c)` 相当（可変文字列）は入れなかった。** Stage 2b の冒頭で決めると
   書いた判断: **入れない**。`Vector<char>` ＋ `to-string` で足り、`string` の `eq`/`eql` が
   `Rc::ptr_eq` である前提を崩す代償に見合わない。地図の `nstring-*` 行は Phase 3d へ送った。

**副産物で見つけた既存バグ 1 件（修正済み）**: コンパイル済みコードで
`(format false "~a" x)` の `x` が `f64` **パラメータ**だと、数値でなく
`to_bits(x) >> 3` が印字されていた。`Checker::wrap_rest_elem` が
`is_heap_repr` の真を根拠に構成子を飛ばしていたが、`is_heap_repr` が答えているのは
*インタプリタの*表現で、compiled な `f64` は箱でもタグ付きでもない生のビット列。
`f64` だけこの近道から外した（`string`/`bignum`/`ratio` は両世界で同じタグ付きの語なので
そのまま）。回帰テストは `compile_test::compiled_format_renders_a_float_parameter_as_a_number`。
[[typelisp-crossing-must-be-type-driven]] と同じ形の誤りで、これで 4 回目。

### 残タスク: char / int の native lowering 5 つ

`char::upcase` / `char::downcase` / `char::alphap` / `char::digitp` / `i32`・`i64` の
`int->char` はコンパイルできない。ユーザコードが `(upcase c)` を呼ぶ関数を `compile` すると
（プロセス abort ではなく）クリーンなエラーで断られる、という既知の穴。§2-4 の触点
フルセット（rt シム 5 本 → externs 3 表 → 島の `char-native-method?` と lowering →
島再生成 ×2 → prelude 再生成）が要る。Phase 2 の本文からは独立しているので、
着手は Phase 1a（同じく島の分岐を増やす作業）とまとめるのが安い。

### Stage 2a — 文字

`char/=`、大文字小文字を無視する順序比較（`char-equal`/`char-lessp`/`char-greaterp`/
`char-not-lessp`/`char-not-greaterp`）、`alphanumericp`/`graphic-char-p`/`standard-char-p`/
`upper-case-p`/`lower-case-p`/`both-case-p`、`char-name`/`name-char`/`digit-char`、
`char-code-limit`。
**`digit-char-p` は CL 準拠へ変更**——現在 `digitp` は `bool` を返すが CL は数字の重み。
`Option<i32>` ＋基数引数にする（既存の `digitp` は別名として残すか、破壊的変更にするかを
実施時に判断し、`docs/functions.md` §9 に記す）。

### Stage 2b — 文字列

`string-trim`/`string-left-trim`/`string-right-trim`（**行入力を扱うと即欲しくなる**）、
`string-capitalize`、`string/=`、大文字小文字無視の順序比較（`string-lessp` 等）、
可変長 `concatenate`、`(string::new n c)`（CL の `make-string`）、値の文字列化 `to-string`
（現在は `(format false "~a" x)` で代替）、`(search s sub)` / `(mismatch a b)`（**部分文字列検索が
無い**）、`(split s sep)`（CL 標準にも無いが実用上ほぼ必ず要る）、
`parse-int` の `:radix`/`:junk-allowed`。

`(setf (char s i) c)` 相当は「`string` を可変にするか」という設計判断を伴う（現在 `string` は
不変で `Rc<str>`、`eq`/`eql` が `Rc::ptr_eq`）。**可変文字列型を別に足すか、対象外とするかを
この Stage の冒頭で決める**——`Vector<char>` ＋ `to-string` で足りる可能性が高い。

---

## Phase 3 — リスト・シーケンス層（本計画の最大ブロック）

**状態: 3a / 3b / 3c / 3d すべて完了（2026-08-20）。** 全て `prelude.rs` の `SOURCE` に入り、
`PRELUDE_COMPILE_UNSUPPORTED` に穴を開けずに通っている。テストは
`tests/seq_catalog_test.rs`（18 本）、ドキュメントは [functions.md](../functions.md) §6.1／§6.2。

**計画から変えた点 5 つ**:

1. **受け手は `Iter` 実装型に統一した**（`Vector<T>` 受け手の `defmethod` を並べる案は不採用）。
   §1.2 は「同じ CL 名を受け手型ごとに `defmethod` で定義する」としていたが、既存の
   `Iter` ライブラリ（`map`/`filter`/`length`/`nth`/…）が既にジェネリック `defun` で書かれており、
   そこへ `defmethod` を混ぜると同じ名前が 2 通りに解決されうる——計画のリスク表が
   `member` を名指しで警告していたのと同じ形。`Vector<T>` は `(iter v)` で渡す既存の作法のまま。
   例外は Phase 3d（破壊的操作）で、こちらは受け手を書き換えるので `Vector<T>` の `defmethod`。
2. **`member` を「残りのリストを返す形」にしなかった。** §3a の指示に反する。理由は上の 1 と同じで、
   受け手の形で `member` の戻り型が変わるのは解決順の事故を招く。`member`/`member-if`/
   `member-if-not` は 3 つとも `bool`——既存ライブラリが既に選んでいた departure を揃えた。
   CL の「残り」が要る場面は `position` + `subseq` で書ける。
3. **`caar`〜`cddddr` は `defun`**（Phase 0.1b の結果どおり）。28 個は機械生成した。
4. **対象外にしたもの**: `list*`（「末尾を差し替えた不完全リスト」という概念が無い）、
   `copy-tree`/`copy-alist`/`sublis`/`subst`/`subst-if`（任意深さの異種の木を走査する型が書けない。
   `Sexpr` の木としてなら `equal` が `tree-equal` に当たる）、
   プロパティリスト一式 `getf`/`get-properties`/`symbol-plist`/`remprop`（キーと値が交互に並ぶ
   無型のリストという表現が無く、同じ役割は `assoc` か `HashTable` が担う。`symbol-plist`/`remprop`
   はさらに可変なグローバルのシンボル属性表を要求するので (D5) 側でもある）。
5. **Phase 0.4 の判断どおり `nconc` は入れたが、CL の `nconc` とは別物。** `Vector<T>` に
   `other` の要素を足すだけで、共有構造の書き換えは起きない（`other` は影響を受けない）。
   `Sexpr` 版は入れていない。

**副産物で見つけた既存バグ 2 件（どちらも修正済み、回帰テストは `generic_defun_test`）**——
どちらも Phase 0.2 で直した `Self` 置換漏れと**同じ形**（型変数の名前がたまたま一致したときだけ
動く）で、これで 3 件目・4 件目:

- 境界付きジェネリック同士の委譲（上の Phase 0 訂正を参照）。
- **ジェネリック `defmethod` は受け手の型パラメータを所有型の宣言と同じ名前で綴らないと
  置換されなかった。** `check_assoc_call` は `def.params` を受け手の具体引数と zip して
  特殊化するのに、登録される署名は `defmethod` が書かれたままの名前を保っていた——
  `(defmethod keepif ((self Vector<A>) (pred (fn (A) bool))) ...)` は `a` を置換しないまま
  具体引数を拒否し、`Vector<T>` と綴った同じメソッドは通る。登録時に所有型の名前へ
  書き換えるようにした（本体は書かれたままの名前で検査する——本文がそう書いてあるので）。


§1.2 の方針に従い、**受け手は `Vector<T>` と `cons-cell` ネストと `Iter` 実装型**。`Sexpr` には足さない。
§2-3 の制約（境界付きジェネリック同士は委譲できない）により、Iter 系は毎回ループを書き下ろす。

### Stage 3a — CL 名の `defmethod` 化

`first`〜`tenth` / `rest` / `length` / `reverse` / `nth` / `last` / `butlast` / `member` /
`assoc` / `append` / `list*` / `copy-list` / `copy-tree` / `copy-alist` / `revappend` と
`(Vector::filled n x)`（CL の `make-list` / `make-sequence`）を `Vector<T>` 受け手の `defmethod` で。
`caar`〜`cddddr` の 28 個は `cons-cell` のネストを受け手とする `defmethod`（Phase 0.1b 次第）。

- `member` は CL 本来の「残りのリスト」を返す形（`Vector<T>` 受け手なら `Vector<A>`）にする。
  既存の Iter 版 `member` が `bool` を返す差分は互換のため据え置き、`docs/functions.md` §6 に明記。
- `last` は CL の「最後のセル」ではなく既存どおり「最後の要素」。CL との差を表に残す。

### Stage 3b — 述語版・否定版・写像の穴埋め

`member-if` / `member-if-not` / `assoc-if` / `rassoc` / `rassoc-if` / `acons` / `pairlis`、
`notany` / `notevery` / `count-if-not` / `find-if-not` / `remove-if-not`（**否定版が一律に無い**）、
`remove`（値で消す）/ `remove-duplicates` / `substitute` / `substitute-if`、
`copy-seq` / `fill` / `replace` / `map-into` / `merge` / `concatenate`、
`search` / `mismatch`（部分列）、
`mapcar` の**複数シーケンス同時走査版**（zip 相当。現在 `map` は 1 本しか取れない）/
`mapc` / `mapcan` / `mapl` / `maplist` / `mapcon`。

### Stage 3c — 集合演算・木の書き換え・プロパティリスト

- **集合演算が全滅している**: `union` / `intersection` / `set-difference` /
  `set-exclusive-or` / `subsetp` / `adjoin`
- 木: `sublis` / `subst` / `subst-if` / `tree-equal`（マクロ処理で効く）
- `ldiff` / `tailp`
- プロパティリスト: `getf` / `get-properties` / `symbol-plist` / `remprop`。
  CL の `get` は `Vector`/`HashTable` のメソッド名と衝突するので `symbol-get` 等へ改名する
  （受け手が `Symbol` なので `defmethod get` でも解決はするが、可読性の判断を実施時に行う）。

### Stage 3d — 破壊的操作（(D4) 解禁分）

`nreverse` / `delete` / `delete-if` / `delete-duplicates` / `nsubstitute` / `nbutlast` /
`nstring-upcase` 等 / `vector-push-extend` / `vector-pop`、
`rplaca` / `rplacd`（`cons-cell` の `set-car` / `set-cdr` を足せば既存の `setf` place 機構に乗る）、
`nconc`（Phase 0.4 の結果次第）。

`Vector<T>` はヒープ上で直接変異する参照型なので、破壊版は「新しい `Vector` を返さない」だけの
違いになる。命名は `!` 接尾辞を使わない規約（language-design.md §7.3）に従い CL 名そのまま。

### Stage 3e — シーケンス API のキーワード引数

**状態: 完了（2026-08-21）。** 実装は `prelude.rs` の `SOURCE`、テストは
`tests/seq_keywords_test.rs`、カタログは [functions.md](../functions.md) §6.3。

**前提の訂正**: 本文は「`lambda` と `defmethod` の `&optional`/`&key` 対応（Phase 5b）」を
前提に挙げていたが、**Phase 5b は要らなかった**。Stage 3a が受け手を `Iter` 実装型に
統一した結果、対象は全部 `defmethod` ではなく**ジェネリック `defun`** で、`defun` の
`&optional`/`&key` は既に通っていた。逆にまだ届かないのは `defmethod` の側——
破壊的操作（§3d、`Vector<T>` の `defmethod`）と `search`/`mismatch`（`string` の
`defmethod`）はキーワードを取れないままで、これは Phase 5b 待ち。

**`:test` と `Eq` 境界の噛み合わせ**: 既定は `Eq` 境界の `equals`、`:test` を渡すとその場で
差し替える——CL が `:test` の既定を `eql` としているのと同じ形なので、名前を分ける必要は
無かった。`:test-not` はその否定。

**`:key` は `(fn (A) A)`**（要素型の中に閉じた射影）。Phase 0.3 の結論どおり `-by` 系の
別名関数は要らなかったが、**別の型へ射影することはできない**——`(fn (A) B)` と宣言すると
`:key` 省略時に `B` が決まらず、本体の恒等フォールバックが `A` と `B` の不一致で落ちる
（実際に確かめた）。異なる型への射影は `-if` 系にラムダを渡すほうで書ける。

**副産物・発見 4 件**:

1. **`Option<(fn ...)>` は型として書けない。** リーダはジェネリックトークンを最初の括弧で
   打ち切る（`read::reader::extend_angle_token`、`(a<b c)` を呼び出しとして読み続けるための
   意図的な設計）。`&key` パラメータはその型を*誰も綴らずに*得るので成立するが、
   キーワードの受け渡しを担うヘルパー関数は書けない。だから共有コアは
   `(fn (i32 A) bool)` のクロージャを受け取り、キーワードの開封は宣言した関数の中に残る。
2. **`check_call_opt_key` に関連型ピンの推論が無かった（修正済み）。** `where` の
   `(Iter I (Item A))` だけで決まる型変数を `check_call` は推論するのに、`&key` 版は
   していなかったので `(remove-duplicates (iter v))` が "cannot infer type parameter `a`"
   で落ちた。`check_call` の該当ブロックを `Checker::infer_pinned_assoc_types` として
   括り出し、両方から呼ぶようにした。
3. **`lambda` が `match` の束縛を捕獲すると compile できない（未修正）。**
   `(match o ((some g) (lambda ... (g ...))))` が
   "compile: `g` is referenced but no binder in scope states its representation" で落ちる。
   `core_bridge::translate_match` はアームの本体を `cx` を広げずに変換するので、
   パターン束縛の `Repr` がスコープに入らない。**Phase 3e の範囲外**（島側でセル化も
   要るので一行では済まない）。prelude はこの形を避けて書いてある。[TODO.md](TODO.md) 参照。
4. **`where` 付き `defun` は前方参照できない。** `predeclare_program` は SOURCE の実行前に
   走るので `deftrait Iter` がまだ登録されておらず、`where` 節が解決できずヘッダごと
   黙って捨てられる。この節の定義順が今までどおり必須である理由。

**`remove-duplicates` の既定を変えた**（挙動の変更）。以前は無条件に最初の出現を残していたが、
CL の既定は最後を残す。以前の挙動は `:from-end true`。

**副産物**: `position-if-not` を足した。この節の見出しコメントが「CL が持つ `-if`/`-if-not`
の対を全部」と書いているのに 1 つだけ欠けていた。

---

## Phase 4 — 制御構造とマクロ層

### Stage 4a — 脱出と代入

**状態: 部分完了（2026-08-20）。** マクロで書ける半分は入った。テストは
`tests/control_forms_test.rs`（6 本）、ドキュメントは [syntax.md](../syntax.md) §4／§5／§7。

入ったもの: `prog1`/`prog2`、`do*`、`ecase`/`ccase`、`setq`/`psetq`/`psetf`、`pushnew`。
いずれも prelude の `defmacro`（`pushnew` だけ `Vector<T>` の `defmethod`）。
`defmacro` は保護された組み込み形を*呼べない*が*生成する*のは構わない——展開はチェッカーへ
返って検査されるので、既存の `do` が変数をステップするのと同じ手が使える。

**残っているもの（それぞれ理由つき）**:

- **`block` / `return-from`** — 本 Stage の主役で、いちばん重い。checker 側は
  `loop_stack` と同型の名前付きブロックスタックで足りるが、実行時は
  (1) インタプリタに `EvalError::ReturnFrom(name, value)` と、それを捕まえる `Block` op、
  (2) **島に名前付き脱出先を通す仕組み**が要る。島の `compile-value` は `loop-exit`/
  `loop-slot` を全呼び出し地点に引数として引き回しており、名前付きブロックの*スタック*を
  足すとその引数列がもう一段増える——4000 行の自己ホストコンパイラ全体に触る変更。
  「compiled 側は既存の `break`/`return` の分岐鎖にそのまま乗る」という計画本文の見立ては
  制御フローの形については正しいが、**引数の引き回しの量を見積もっていない**。
- **`prog` / `prog*`** — CL では `block nil` ＋ `tagbody` の糖衣。`block` に依存し、
  `tagbody` は goto なので対象外。`block` が入ったら「`tagbody` 抜きの `prog`」の
  是非を判断する。
- **`destructuring-bind`** — `defmacro` のラムダリストは分配束縛できるが、あれは全て
  無型の `Sexpr`。式としての `destructuring-bind` は束縛される各変数に静的型を与える
  必要があり、`Sexpr` の異種の入れ子から型を取り出す手段が無い（`match` の downcast
  パターンが相当する既存機構）。**設計判断が要る項目**で、片手間には入らない。
- **`remf`** — プロパティリストごと対象外（Phase 3c の判断）。
- **`sleep`** — Rust 組み込みが要る（§2-4 の触点フルセット）。


- **`block` / `return-from`**（本 Phase の主役）。現在 `return` は**直近のループからしか脱出できず**、
  名前付きブロックも関数からの早期リターンも無い。`defun` が関数名の暗黙ブロックを作る CL 規則も
  含めて実装する。既存の `loop_stack`（checker.rs の `check_loop_body`）と同型のブロックスタックで
  足りる。**これは静的な脱出**なので、compiled 側は既存の `break`/`return` の分岐鎖にそのまま乗る
  ——動的な `catch`/`throw` とは混ぜない（language-design.md §7.5 の「静的な脱出と動的な脱出は
  混ぜない」）。
- `setq` / `psetq` / `psetf`（place 機構自体は 2026-07-30 に入っているが、この 3 つは未実装）
- `pushnew` / `remf`
- `prog` / `prog*` / `prog1` / `prog2`、`do*`
- `destructuring-bind`（`defmacro` のラムダリストでは分配束縛できるが、式としては無い）
- `ecase` / `ccase`（網羅性を要求する `case`。checker が枝を検査する）
- `sleep`

### Stage 4b — 拡張 `loop` DSL

**状態: 完了（2026-08-21）、`:named` を除く。** 実装は
`crates/typelisp-front/src/check/loop_dsl.rs`（節の読み取りと再構成）と
`Checker::check_loop_dsl`（型を決める側）、テストは `tests/loop_dsl_test.rs`（20 本）、
ドキュメントは [syntax.md](../syntax.md) §5.1。

分岐は Phase 0.6 の結論どおり **第 1 要素がキーワードかどうか**。CL 自身の simple loop
規則と同じなので、既に書かれている `loop` は 1 つも意味が変わらない。

入った節: `:with`/`:for`（`:in`/`:across`/`:on`/`:from`/`:downfrom`/`:upfrom`/`:to`/`:below`/
`:downto`/`:above`/`:by`/`=`/`:then`）/`:repeat`/`:do`/`:collect`/`:append`/`:sum`/`:count`/
`:maximize`/`:minimize`/`:always`/`:never`/`:thereis`/`:while`/`:until`/`:when`/`:unless`/`:if`/
`:else`/`:return`/`:initially`/`:finally`/`:into`。

**計画の前提が 1 つ誤っていた。** 「`collect` は `(let ((acc (Vector::new))) … (push acc e) …
acc)` へ展開すれば要素型が推論で決まる」——決まらない:

```text
(let ((acc (Vector::new))) (progn (push acc 5) (len acc)))
=> type error: cannot infer type argument `t` for `vector::new`
```

`Vector::new` の型引数は**期待型から前向きに**来るので、後続の `push` からは決まらない。
したがって DSL は「ソースへ展開して再検査するだけ」では済まない。`check_loop_dsl` は
2 パスになった: 変数節ごとに代表式を検査して型を学び、その環境で集約式を検査して要素型を
求め、`(the Vector<T> (Vector::new))` の `T` を自分で書き込む。書けない型（関数型など）は
往復検証で弾いてその旨のエラーにする。`:maximize`/`:minimize`/`:thereis` の `Option<T>` も同じ。

**CL から外した点**:

- **節の語はキーワード**（`:for`/`:collect`…）。裸の `for` はただの変数参照になるし、
  キーワードであることが単純ループとの分かれ目でもある。`=` だけは位置が一意なので裸でも可。
- `:maximize`/`:minimize` は `Option<T>`、`:thereis` は `Option<T>` を取り `Option<T>` を返す
  （nil が無いため。`bool` を試すのは `:always`/`:never`）。
- **`:return` だけで集約も `:finally` も無いのはエラー**。CL は尽きたとき nil を返すが、
  ここにはそれが無いのでループが「尽きたときの値」を言う必要がある。
- `:named`（Phase 4a の `block`/`return-from` 依存なので同項の残件へ）、`:and`、`:being`、
  `:it`、`:nconc` は入っていない。

### Stage 4c — 評価とマクロ

**状態: 部分完了（2026-08-21）。** 入ったのは `macroexpand`/`macroexpand-1`、`complement`、
`gensym` のプレフィクス引数と `*gensym-counter*`。テストは `tests/macro_tools_test.rs`、
ドキュメントは [functions.md](../functions.md) §14。

**`macroexpand-1` は `Option<Sexpr>` を返す。** CL は「展開したか」を第 2 返り値で伝えるが
多値が無いので、`none` が「マクロ呼び出しではない」を表す。CL の真偽値より情報が多い——
自分自身の呼び出しへ展開するマクロと非マクロを取り違えようがない。`macroexpand` は
`none` になるまで繰り返して最終形を返す（CL と同じ）。展開そのものは
`Checker::try_expand_toplevel_macro`、つまり**検査が使うのと同じ 1 段**なので、
プログラムが見るものと検査が見たものがずれない。コンパイル済みコードからは `eval` と
同じ 2 経路（JIT の `with_active_interp` / AOT の `AOT_ENV`）で環境に届く。

**`gensym` は prelude 関数になった**（Rust 組み込みを廃止）。CL の `*gensym-counter*` を
「プログラムが読み書きできる変数」にするには、カウンタが typelisp 側の大域変数である
必要があったから——組み込みのカウンタは `Heap` にあり、誰も名指しできなかった。
解釈と compiled が 1 つの列を共有するという不変条件は、**同じ 1 つの定義と 1 つの大域**を
共有することで保たれる。名前は先頭が空白なので、ソースに書けるどの名前とも衝突しない。

**入れなかったもの（それぞれ理由つき）**:

- **`constantly`** — CL のそれは*引数を無視する関数*を返す。無視される引数の型は
  **戻り型にしか現れない**が、このチェッカーは型パラメータを*引数から*決める
  （明示的な型適用も無い）ので `(the (fn (i32) string) (constantly "hi"))` でも決まらない。
  0 引数のサンクに縮めれば書けるが、CL の用途（`:key` 等）に届かない。
  `(lambda ((x T)) A v)` が同じ字数で同じことを言う。`const`（2 引数版）は既にある。
- **`macrolet` / `symbol-macrolet`** — 障害は 1 つで、はっきりしている。**式の位置の検査は
  `&self`** で、マクロを定義するには (1) レジストリへの登録（`check_defmacro` は
  `&mut self`）と (2) **インタプリタ側でのラムダの登録**（`exec` 相当）の両方が要る。
  やるなら: `check_defmacro` を「本体を検査して部品を返す `&self` 部分」と「登録する
  `&mut self` 部分」に割り、`Checker` にスコープ付きのローカルマクロ表（`RefCell`）を足して
  `resolve_macro` がレジストリより先に引き、`MacroExpander` に「この defmacro コア形を
  定義せよ」という 1 メソッドを足す。**設計は決まっているが片手間には入らない**ので別立て。
- **`eval-when`** — **選ぶべき区別が無い**。`typl` は各トップレベル形を検査→実行と 1 本で
  進み、`compile-file` は**定義形を全部 `exec` する**うえに裸のトップレベル式を受け付けない
  （`aot.rs`)。つまり CL の `:compile-toplevel`/`:load-toplevel`/`:execute` の 3 つは
  ここでは常に一致していて、`eval-when` は恒真のラッパーにしかならない。
- **`define-compiler-macro` / `compiler-macro-function`** — コンパイラマクロ層が無い。
  ここの `compile` は明示的な操作で、島は名前で呼び出しを書き換えたりしない。
- **`load-time-value`** — 実行と別のロード相が無い（上と同じ理由）。
- **`(Symbol::new name)` / `copy-symbol` / `gentemp`** — uninterned シンボル。シンボルは
  名前で intern されるので「同名で別物」を作ること自体は `Heap` に 1 メソッド足せば可能
  だが、**買えるものが無い**: シンボルが束縛子として働く場所は全部*名前*で引かれる
  （`Env::vars` は `String` 鍵）ので、同名の uninterned シンボル 2 つは肝心なところで
  衝突する。データとしてなら intern 済みと区別が付くだけで、その区別に用途が無い。
  `gentemp` は intern された新しい名前を作るもので、それは `gensym` そのもの。

`*macroexpand-hook*` は (D5) 側の判断（Phase 7b）に合流させる。

---

## Phase 5 — 定義形の拡張

### Stage 5a — `defstruct`

状態: 完了（2026-08-21）。`:conc-name` と `:predicate` は「入れない」で確定した。

オプションリスト構文 `(defstruct (Name option...) fields...)` を新設し、フィールドは
`(name Type default)` を取れるようにした。

**生成物は `defmethod` の「ソース」として合成し、`check_defmethod_in` に通す。** アクセサ
（既存）は lowered AST を直接組み立てているが、コンストラクタは違う——`&key`/`&optional` の
埋め込み（Phase 5b）、可視性、そしてジェネリック所有者に対する `MethodTemplate::Form` の
保持（＝単型化できること）が全部要る。ソースを合成して既存の経路に流せば、2 つ目の実装を
書かずに全部手に入る。本体は必ず `(Name::new ...)`——`new` は構造上の唯一のコンストラクタ
のままで、生成するのはその*呼び方*。

- **`:constructor`**。`(:constructor name)` は全スロットを `&key` で取る（**全スロットに
  デフォルトが要る**。CL の「未束縛スロット」に当たるものがこの言語に無いため）。
  `(:constructor name (slot...))` は BOA で、名指さなかったスロットはデフォルトで埋まる。
  `&optional` も書ける。複数宣言可。
- **`:copier`** — 同じスロット値を持つ新しい値を返すインスタンスメソッド。CL 同様に浅い。
- **`:include Parent`** — 親のスロット列を先頭に連結する。デフォルトも引き継ぐ（別ファイルの
  親でもよいよう、デフォルトは `Registry::struct_defaults` という疎な副表に置いた）。
  **型の関係は作らない**: 子は親の部分型ではなく、親のメソッドは子に適用されず、両者を結ぶ
  実行時テストも無い。策定時に「この Stage で最も重い」と書いたのは型の継承を入れる前提
  だったからで、部分型を導入しないと決めた時点で連結だけが残った。共通のインタフェースは
  `deftrait` が受け持つ。
- **スロットのデフォルトは生成されたコンストラクタだけが読む**ので、`:constructor` が 1 つも
  無いのにデフォルトを書いたら死んだ設定になる。その場でエラーにする。

入れないもの:

- **`:conc-name`** — CL ではアクセサに接頭辞を付けて 1 つの平坦な関数名前空間での衝突を
  避ける。ここではアクセサは受け手の型でディスパッチするメソッドなので衝突が起きず、
  接頭辞を付けると `instance::field`（スロット名しか知らない）が壊れる。解く問題が無い。
- **`:predicate`** — 実行時に「この値は `point` か」を答えるもの。型は実行時の witness を
  持たないコンパイル時の分類で、「point かもしれない未知の型の値」が存在する位置も無い
  （`Sexpr` の `match` は封じてあり `:dyn` はダウンキャストできない）ので、生成される述語は
  常に `true` しか返せない。
- `:type` / `:initial-offset` / `:named` は当初のとおり対象外。

**ダンプの走査表**にも 2 つ足した。`dump.rs` の `walk`/`apply_entries` は表をカテゴリごとに
名前で列挙するので、片方しか知らない表があると「定義を黙って落とすデルタ」になる。
Stage 5c の `Namespace::type_aliases` も Stage 5a の `Registry::struct_defaults` も入って
いなかった（`cat::TYPE_ALIAS` / `cat::STRUCT_DEFAULT` を新設、`FORMAT_VERSION` 3 → 4）。
1 プロセス内で完結するテストでは全部 green のまま通ってしまうので、別プロセスがイメージから
起動する `dump_image_test` に 2 本足して往復を確かめた。

**副産物: `check_assoc_call` が引数からも所有者の型引数を推論するようになった。** それまで
`subst` の出所は受け手か期待型だけで、受け手のない**静的**関数（`cell::of`）は結果の型が
既に分かっている場所でしか書けなかった。ジェネリック構造体に生成コンストラクタを付けた
テストが最初に落ちて分かった。`check_call` と同じ 2 段（閉じている時だけ期待型を渡し、
チェック後に `unify`）にしてある。

### Stage 5b — `lambda` / `defmethod` の `&optional` / `&key` / `&rest`

状態: 完了（2026-08-21）。ただし `lambda`/`labels` は「入れない」で確定した。

**`defmethod` は 3 区画すべてを取る。** `parse_defmethod_sig_inner` が受け手の次を
`parse_params_full`（`parse_defun_params_full` を `defun` から切り離したもの）に渡し、
`MethodSig::params` は最初から**実行時パラメータ**——必須、`&optional` の実効型、`&rest` の
`Sexpr` 名、`&key` の実効型——を並べて返す。だから本体の `Env`、生成する定義形、コンパイル
経路のどれも区画の存在を知らない。知っているのは登録される `FnSig` と呼び出し側だけで、
呼び出し側は `check_assoc_call` の分岐（`push_assoc_opt_key_args`）が `check_call_opt_key` と
同じやり方で飽和した実引数列を組み立てる。インスタンスメソッドと静的関数の両方で動く。

自由関数版より単純な点が 1 つある: メソッドのシグネチャは自前の型パラメータを持たない
（`AssocFn` の `FnSig::type_params` は常に空）ので、推論パスが要らない。効いている代入は
受け手（または期待型）の型引数から呼び出し側が既に決めたものだけ。その代わり
**デフォルト式を書いたパラメータの型に所有者の型パラメータを書けない**——`defun` が自分の
型パラメータについて負うのと同じ制限で、理由も同じ（省略時に埋め込むのは検査済みの
ノードなので、その型が抽象変数のままでは下流の表現判定が壊れる）。

**トレイトのメソッドでは使えない。** vtable スロットのアリティは固定で、`:dyn` 受け手の
呼び出しはトレイトの宣言から、具象受け手の呼び出しは `impl` の宣言から引数を埋めるので、
両者が食い違うと同じ呼び出しが 2 通りになる。`deftrait` 側に構文が無い以上、食い違いを
作れるのは `impl` 側だけなので、そこで 2 箇所塞いだ: `subst_method_item`（`impl` ブロックの
中に書いた場合。放っておくと `ImproperList` という無関係なエラーになる）と
`check_impl_conformance`（外で書いた継承メソッドを後から `impl` が拾う場合。`FnSig::params`
は必須引数しか持たないので、区画を見ないと比較を素通りしてしまう）。

**`lambda` / `labels` は入れない**（`&rest` は従来どおり使える）。省略された引数を埋めるには
呼び出し側が**呼ばれる側の検査済みデフォルト式**を読む必要があり、それは名前で解決した
シグネチャからしか手に入らない。`lambda` は値として渡され、その値を説明するのは `Type::Fn`
だけ——パラメータ型・`&rest` の要素型・戻り型しか無い。式を置く場所が無いうえ、置けば
「同じシグネチャでデフォルトだけ違う 2 つのラムダ」が別の型になる。`labels` の関数も
`Type::Fn` 型のローカル変数で値として渡せるので同じ。エラーメッセージ
（`checker::OPT_KEY_NEEDS_A_NAME`）はこの理由をそのまま言う。`&rest` が使えるのは、それが
型の話に閉じていて `Type::Fn` に枠があるから。

地図 §3-2 の「`lambda` と `defmethod` は `&rest` のみ」という記述もここで直した。

**関数値としての参照（2026-08-22 追記）。** 名前で呼ばない側——`(call2 greet)` のように
`defun` を値として渡す形——に穴があった。`FnSig::params` は必須引数しか持たないので
`Checker::fn_ref_node` の作る `Type::Fn` が必須アリティになり、**型検査を通ってから実行時に
アリティ不一致で落ちていた**（実行時の関数は宣言した名前の数だけ引数を取る）。
`fn_value_params`（必須 → `&optional` → `&key`、デフォルトの無いものは本体と同じ
`Option<T>`）を新設して直した。デフォルトは間接呼び出しでは埋まらない——埋める場所が
`Type::Fn` に無いのは上と同じ理由——ので、関数値の呼び手は全引数を自分で渡す。
Phase 4c で `gensym` が `&optional` を得たときに踏んでいたが、その 2 本のテストを
回していなかったので Phase 6c の直列全実行まで見つからなかった。

### Stage 5c — `deftype`

状態: 完了（2026-08-21）。読み通り、checker の型解決にエイリアス表を足すだけで済んだ。

`(deftype Name Type)` / `(deftype Name<T,U> Type)`。名前の位置は `defun` と同じ
（`parse_defun_name`）。`Namespace::type_aliases` に入り、`Checker::canon` が使用位置で
展開する。

**保存する本体は正規化済み・エイリアス展開済み**にした。これで使用位置の展開が不動点探索
ではなく 1 回の代入で済み、**エイリアスの循環が「検出するもの」ではなく「作れないもの」に
なる**——`B` の本体を保存する時点で、その中の `A` は既に `A` の本体になっている。代償は
この言語が随所で採っている「テキスト上の先行順」の規則（`check_supertrait_impls` と同じ）
で、後から `A` を再定義しても `B` には届かない。自己参照だけは登録前なので展開できず、
未解決の裸の名前（＝型変数と区別がつかない）になってしまうため、その場で明示的に拒否する。

**型パーサの中で書き換わる**ので、下流は誰もエイリアスの存在を知らない。`mangle_type`、
単型化のキー、ダンプ、コンパイル経路、そして**エラーメッセージ**——すべて展開後を見せる。
これは意図した取引で、CL の `deftype` も型指定子の略記であって別の型ではない（`typep` は
展開後について答える）。テストで明示的に確かめている。

したがって 2 つのことは**しない**:

- **新しい型を作らない。** `(deftype meters i32)` は `meters` と `i32` を同じ型にする。
  取り違えを捕まえたいなら `defstruct`。
- **述語にならない。** CL の `(deftype small () '(integer 0 9))` は*値の集合*を表し、
  `typep` が実行時に判定する。ここでは型は実行時の witness を持たないコンパイル時の分類
  なので、値を制限するエイリアスには制限する相手がいない。

`canon` は失敗できない（`Result` を返せない場所から呼ばれる）ので、型引数の個数違いは
そこでは黙って展開を見送り、`check_type_alias_arity` が**書かれた**型を歩いて注釈の位置で
報告する。

名前空間は型・トレイトと共有する（`check_type_trait_clash`）。既存の型と同名の
`deftype` は `RedefPolicy` に関わらず拒否する——別の表に入るので、再定義を許すと両方が
登録されたまま使用位置ではエイリアスが黙って勝つ。`(use m::meters)` でも取り込める
（`check_use` に分岐を追加。エイリアス自身は構成子も静的メソッドも持たないので、
取り込むのは名前だけ）。

`Namespace` に serde のフィールドが増えたので `dump::FORMAT_VERSION` を 2 → 3 に上げ、
島と prelude の成果物を再生成した（島 → prelude の順。island は不動点まで 2 回）。

---

## Phase 6 — コレクション

すべて §1.1 の「Rust 風・受け手優先」に従う。生成は `Type::new`、操作は受け手の型で
ディスパッチする `defmethod`、CL 名は必要な分だけ薄い別名か checker 糖衣で被せる。

### Stage 6a — ハッシュ表

状態: 部分完了（2026-08-22）。**ユーザ定義型をキーにする**分だけ残した（理由は下）。

**`Hash` トレイト ＋ `sxhash`** が入った。CL は契約を含意で述べる——`(equal x y)` ならば
`(= (sxhash x) (sxhash y))`。`Eq` をスーパトレイトに持つトレイトにすると、同じことが
この言語の言葉で言える: ハッシュできる型とは値を比較できる型で、`sxhash` はその比較と
一致していなければならない。逆は成り立たない（衝突はありうる）。結果は非負で 30bit に
収まる（CL は fixnum と言う）。スカラ 14 型に impl があり、ユーザ型も `impl Hash` で
書ける。文字列は typelisp で書いた 32bit FNV-1a。

**キーのハッシュ可能性が静的になった。** `HashTable` の `get`/`set`/`remove` が
`(where (Hash K))` を持つので、表が保持できないキー型は**型エラー**になる。以前は
`Heap::lookup_hash_key` の実行時 panic で、そのコメントは「チェッカーはハッシュ可能性の
境界を表現できない（この言語にトレイトが無いので）」と言っていた。トレイトは
2026-06-30 に入っており、これがそのコメントへの回答。代償として組み込みの `HashTable` が
prelude のトレイトに依存する（島は `HashTable` の値を持たず `rt_hashtable_*` の呼び出しを
*出す*だけなので影響しない）。

`maphash` と `size` を受け手優先の `defmethod` で足した。`size` は `count` と同じ値を返す
——この表は Rust の `HashMap` で、占有数と別の「容量」をユーザに見せておらず、
作った数を返すほうが嘘が少ない。CL も `hash-table-size` には非負整数としか約束していない。

**`HashTable::new` の `&key` は入れていない。** `:test` は表が持たない意味論の選択で
（この表は `equal` 一択）、関数値を受け取っても比較できない。`:rehash-size` /
`:rehash-threshold` は `HashMap` にユーザから見える再ハッシュ方針が無い。受け取って無視
するのは、受け取らないより悪い。

**ユーザ定義型をキーにするのは別作業。** mem 層の表は `MemHashKey`（`Int`/`Bool`/`Char`/
`Str` の閉じた集合）で引いており、ユーザ型を入れるには `get`/`set`/`remove`/`keys`/
`values`/`entries` の下にバケット層——ハッシュ衝突を構造的等価で解決する層——を敷く必要が
ある。それを prelude 側に置こうとすると型が合わない（表の宣言型 `V` と、格納したい
「`(K,V)` のバケット」が別物で、組み込みメソッドのシグネチャに後者を書けない）ので、
バケットは Rust 側（typelisp-mem と interp、および `rt_hashtable_*`）に置くことになる。
`sxhash` と `(where (Hash K))` はその作業の入口として先に入れてある。

**副産物: コンパイル済みコードの整数切り詰めを 2 件見つけた**（[TODO.md](TODO.md) に記録）。
1 件は直した——整数リテラルは島へ `Sexpr` として渡るので 3bit タグを引いた 61bit しか
残らず、`4611686018427387903` が `-1` にコンパイルされていた。`float` が最初から採っている
32bit 2 分割にして LLVM 側で組み直す。自己ホストなので移行にはブートストラップの順序が
要った（島の読み手を新形式にして*古い*エミッタで 1 世代作り、そのあとエミッタを切り替える。
二重読みのコードは書いていない）。残る 1 件は幅の広い `i64` **グローバル**が同じ 61bit で
壊れるもので、渡し方ではなくコンパイル済みコードから見たグローバルの表現の問題なので
未修正。

### Stage 6b — 多次元配列 `Array<T>`（完了 2026-08-22）

`Vector<T>` 2 本（次元列と平坦な要素列）の上の **prelude の `defstruct`** として入れた。
Rust 側の追加はゼロ——新しい `Repr` も `rt_*` シムも島の lowering も要らず、書いた日に
JIT/AOT を通る。1d の `complex` と同じ判断（[[typelisp-vector-defstruct-revert]] の原則）。

| 実装した API | CL 名 |
|---|---|
| `(Array::make dims init &key fill-pointer)` | `make-array` |
| `(get a idx)` / `(set a idx x)`（`idx` は `Vector<i32>`） | `aref` / `(setf (aref …))` |
| `(aref a i j …)` / `(setf (aref a i j) v)` | 同上（checker の糖衣） |
| `(row-major-get a i)` / `(row-major-set a i x)` | `row-major-aref` |
| `(rank a)` `(dimension a n)` `(dimensions a)` `(total-size a)` | `array-rank` / `array-dimension` / `array-dimensions` / `array-total-size` |
| `(in-bounds a idx)` `(row-major-index a idx)` | `array-in-bounds-p` / `array-row-major-index` |
| `(adjust a dims init)` `(push-extend a x)` `(pop a)` `fill-pointer` | `adjust-array` / `vector-push-extend` / `vector-pop` / `fill-pointer` |
| `(iter a)` | — |

計画から変わった点:

- **生成は `Array::new` ではなく `Array::make`**。`new` は `defstruct` が必ず生成する
  フィールド順のコンストラクタで、この型のフィールドは*表現*（次元列・平坦な格納・
  fill pointer）であって呼び手が渡したいものではない。`BitVector` も同じ理由で `make`。
- **`(aref a i j)` の展開先は `row-major-get` ではなく `get`**。`(setf (aref a i j) v)` を
  既存の呼び出し形 place 機構（`get`→`set` の特例）にそのまま乗せるため、両方が
  `Array<T>` 自身の `get`/`set` を通る。展開は
  `(let* ((%a a) (%idx (the Vector<i32> (Vector::new)))) (progn (push %idx i) (push %idx j) (get %a %idx)))`
  で、配列を先に束縛するのは*書いた順*（配列→添字）に評価させるため。
- **添字が範囲外なら実行時エラー**。検査を省くと 2x3 の `(aref a 0 5)` が「別の行の実在する
  セル」を静かに読む。`row-major-index` が `in-bounds` を通してから畳む。
- `array-element-type` / `simple-vector-p` / `adjustable-array-p` /
  `array-has-fill-pointer-p` は予定どおり対象外（(D1)。静的型が既に答えている）。
  `svref` / `array-displacement` も入れていない——前者は `Vector<T>` の `get` がその物、
  後者は「別の配列の記憶域を共有する」という、この言語に持ち込む理由の無い概念。
- `adjust` は CL と違って**配列を返さず `()`**。CL が返すのは、非 adjustable な配列だと
  *別の配列*が返りうるからで、ここでは全部 adjustable なので返す第 2 の配列が無い。

テストは `tests/array_test.rs`（37 本）と `tests/compile_test.rs` の 3 本。

### Stage 6c — ビットベクタ `BitVector`（完了 2026-08-22）

`Vector<i64>` に詰めた語＋長さの prelude `defstruct`。`BitVector::make` / `get` / `set` /
`len` / `bit` / `sbit`（と `setf` 版）/ `bit-and` / `bit-ior` / `bit-xor` / `bit-not`、
および CL の残り 7 種（`bit-eqv` / `bit-nand` / `bit-nor` / `bit-andc1` / `bit-andc2` /
`bit-orc1` / `bit-orc2`）。`bit-vector-p` は予定どおり (D1)。

**1 語は 64bit ではなく 32bit**。コンパイル済みコードはコンテナの要素をタグ付きの語
（`typelisp-abi` の `encode`、下位 3bit がタグ）で往復させるので、payload に入らない
`i64` は往復で壊れる——prelude のメソッドは全部コンパイル済みで走るから、64bit で詰めると
上位 3 ビットが黙って消える。これは `BitVector` の問題ではなく既存のバグで、
Phase 6a で見つけた整数切り詰めの 3 件目として [TODO.md](TODO.md) に再現手順つきで
記録した。直し方（payload に入らない整数を `TAG_BOXED` の箱へ逃がす）は 1 フェーズ分の
作業なので、ここでは端に近寄らない語幅を選んである。

長さの先にあるビットは常に 0 に保つ（`bitvector-trim`）。そうしないと `lognot` が
幽霊ビットを残し、同じ長さの 2 本が食い違う。テストは `tests/bit_vector_test.rs`（18 本）と
`tests/compile_test.rs` の 2 本。

---

## Phase 7 — エラーと動的束縛

### Stage 7a — コンディションの `Error` トレイトへの統合

コンディション**システム**は採らない（§0）。ここでやるのは「CL がコンディション型で表していたものを
typelisp の `Error` トレイト＋具象エラー型へ写像し、写像しても残る穴だけを埋める」こと。

**(1) 具象エラー型の整備** — 既存 5 型（`ParseIntError` / `ParseFloatError` / `ReadError` /
`EvalError` / `FileError`）に足りない分を、CL の標準コンディション型のうち
**typelisp で実際に起こりうるもの**に限って足す:

| CL のコンディション型 | typelisp での扱い |
|---|---|
| `simple-error` / `simple-condition` | **`SimpleError` を新設**（任意メッセージの汎用具象型）。ユーザが `Result` に載せる既定の選択肢になる |
| `end-of-file` | `read-sexpr` は入力末尾を `Ok(none)` で表しており既に十分。ストリーム層に足すかは実施時判断 |
| `file-error` | `FileError` として実装済み |
| `parse-error` / `reader-error` | `ParseIntError` / `ParseFloatError` / `ReadError` として実装済み |
| `arithmetic-error` / `division-by-zero` / `floating-point-*` | 現在はゼロ除算が panic（language-design.md §7.6 が Rust の整数除算に忠実であることを選んでいる）。`Result` を返す `checked-` 版を出すかを実施時に判断 |
| `type-error` / `unbound-variable` / `unbound-slot` / `undefined-function` / `control-error` / `program-error` | **起こりえない**。型検査・スロット全必須・名前解決・静的な脱出の検査で全部コンパイル時に潰れる。対応表にその旨を明記する |
| `storage-condition` | cons アリーナ枯渇は現在 panic。据え置き |
| `warning` / `style-warning` | **(2) の `warn` が担う** |
| `cell-error` / `package-error` / `print-not-readable` / `stream-error` | 対応する操作が無いか、上記のいずれかに吸収される |

**(2) 落ちている 2 つの穴を埋める**:

- **`assert`** — 条件が偽なら panic。コンディション抜きなら素直に足せる（地図 §2.7 も
  「コンディション抜きの『条件が偽なら panic』なら追加可能」と書いている）
- **`warn`** — `*error-output*` へ書いて**続行**する。地図 §2.7 の
  「**警告を出して続行する仕組みが無い**」を埋める唯一の項目。`print`/`println` と同じく
  制御文字列を取る特殊形にする

**(3) `Error` トレイト周辺の整備**:

- `source` チェーンを辿って原因の連鎖を印字するヘルパ
- 原因を包む `wrap-error`
- `docs/functions.md` §7.1 の 4 型表を 5 型（`FileError` 追加）へ修正（§2-11）

**(4) 判断が要る 1 件（この計画は非採用を推奨する）**:
`handler-case` / `ignore-errors` 相当（panic を捕まえて継続する手段）は、
`crossing.rs` の `catch_compiled_panic`（コンパイル済みコードの Rust unwind を
`EvalError::Panic` / `EvalError::Throw` に再構築する既存機構）を使えば**技術的には作れる**。
しかし language-design.md §9 が「捕まえて継続する手段は無い」を確定事項としており、
`panic` は「回復不能なバグ・不変条件違反」の側に置くという §7.1 の住み分けとも噛み合っている。
**覆すなら §9 の改訂が先**。この計画では実装しない。

### Stage 7b — 動的束縛の代替と制御変数の完成

`let` は常に字句束縛のまま変えない。代わりに「保存 → 代入 → `unwind-protect` で復元」する
**スコープ付き再束縛**を入れる（`unwind-protect` は 2026-08-16 実装済みなので新機構は不要。
cleanup は正常終了・`throw`・`panic`・`break`/`return` のどれで抜けても走る）。
これで CL の「一時的に `*print-base*` を 16 にする」用法が書ける。

その上で、CL の制御変数のうち typelisp に対応物があるものを全部揃える:

- プリンタ: `*print-base*` / `*print-radix*` / `*print-case*` / `*print-lines*` /
  `*print-gensym*` / `*print-array*` / `*print-readably*` / `*print-escape*`
  （最後の1つは今 `print-object` の `escape` 引数としてしか存在しない）
- リーダ: `*read-base*` / `*read-default-float-format*` / `*read-suppress*` / `*read-eval*`
- ストリーム: `*trace-output*` / `*query-io*` / `*terminal-io*` / `*debug-io*`
- `with-standard-io-syntax`、`*macroexpand-hook*`

`*package*` / `*readtable*` はそれぞれ Phase 9a / 8c に依存するのでそちらで扱う。

---

## Phase 8 — 印字とリーダ

### Stage 8a — プリンタ

- `prin1` / `princ` / `write` / `write-to-string` / `prin1-to-string` / `princ-to-string` / `pprint`
  を **CL 本来の意味で**足す。現在の `print`/`println` は制御文字列を取る format 系であり
  CL の `print`（1 引数、`~s` 相当）とは別物なので、**併存させる**（改名しない）
- format の `~/name/` ディレクティブ（唯一の未対応ディレクティブ）
- バイナリ I/O: `write-byte` / `read-byte`

### Stage 8b — リーダ

`read-preserving-whitespace`、`read-delimited-list`、
`read-from-string`（＝現在の `(read s)`）の**読み終わり位置**を返す形（多値は使わないので
`cons-cell` で返す）。

### Stage 8c — `readtable` とリーダマクロ

**前提条件（この Stage だけ他と性質が違う）**: 現在は「全フォームを読んでからチェック/評価する」
アーキテクチャなので、読み込み中にユーザーコードを走らせられない。`*features*` が
「読み込み中に書き換え不可の固定集合」なのとまったく同じ理由である。
**フォーム単位の「読む→チェック→評価」ループへの転換が先行条件**で、影響範囲は
`prelude::load_interpreted_with`（prelude.rs 2320行）・`project.rs`・`main.rs`・LSP の各ドライバ。
この転換自体を独立した Stage として先に切る。

その上で: `copy-readtable` / `set-macro-character` / `get-macro-character` /
`set-dispatch-macro-character` / `make-dispatch-macro-character` / `readtable-case` / `*readtable*`、
読み込み時制御 `#.`（`#+`/`#-` は 2026-07-30 実装済み）。

---

## Phase 9 — シンボル・パッケージ・環境

### Stage 9a — パッケージ

`in-package`（ファイル冒頭で名前空間を宣言する形。現在は入れ子の `module` とファイル↔モジュール
対応のみ）、`import` / `shadowing-import` / `shadow`（`use` の個別シンボル取り込み・遮蔽）、
`unuse-package`。`*package*` は §0 の「パッケージは実行時オブジェクトでない」に留まるが、
`in-package` を入れるなら「現在のモジュール」の概念は checker 側に既にあるので矛盾しない。

### Stage 9b — システム構築

`require` / `provide` / `*modules*`、`compile-file-pathname` / `*compile-file-pathname*` /
`*load-pathname*`、`defparameter`（`defvar` との「再ロード時に再初期化するか」の区別）。

### Stage 9c — 環境・時間・ファイルシステム

**状態: 完了（2026-08-20）**、ただし 4 群は保留（下記）。テストは
`tests/environment_catalog_test.rs`（12 本）、ドキュメントは
[functions.md](../functions.md) §4.6／§4.7／§4.8／§18.5／§19.2。

入ったもの:

- **Rust プリミティブ 11 個**。`file-*` 5 つ（`file-truename` / `file-modified-date` /
  `file-directory-p` / `file-list-directory` / `file-create-directories`）は
  `stream_builtin.rs` へ、環境まわり 6 つ（`command-line-args` / `getenv` /
  `home-directory` / `lisp-implementation-version` / `machine-type` / `software-type`）は
  `sys_builtin.rs` へ。
- prelude 側の `Pathish` 層: `truename` / `file-write-date` / `directory-p` / `directory` /
  `ensure-directories-exist` / `user-homedir-pathname` / `lisp-implementation-type`。
- 日時の分解・合成: `decoded-time`（`defstruct` 7 フィールド）と
  `decode-universal-time` / `encode-universal-time` / `get-decoded-time`。純粋な typelisp。
- 対話: `y-or-n-p` / `yes-or-no-p`。

**計画から変えた点・保留**:

1. **`file-` 接頭辞は命名規約ではなく*経路規則*だった。** `Interp::eval_builtin` は
   `stream-`/`file-` で始まる名前を全部 `stream_builtin::stream_builtin` へ丸投げするので
   （interp.rs の `name if name.starts_with(...)` アーム）、`file-*` を `sys_builtin.rs` に
   置くとインタプリタからは永久に届かない。最初の草稿はそこに置いていた。
2. **§2-4 の「触点 8 箇所」は*メソッド*の話で、自由関数は安い——ただし島の*再生成*は要る。**
   `externs.rs` の `rt_builtin_symbol` の doc コメントが明言しているとおり、島は
   「bridge が名付けたものを呼ぶ」だけなので、**組み込み*関数*を足すのに
   `src/compiler.rs` の SOURCE を書き換える必要は無い**（`file-exists-p` が
   そこに一度も現れないのが証拠）。触点は registry / interp / rt / externs の 3 表だけ。

   **しかし成果物のバイト列は変わる。** `rt_extern_functions()` に足した 11 個は島の
   ビットコードに extern 宣言として現れるので、`the_committed_island_matches_a_fresh_build`
   が 452 バイト差で落ちた。最初この節に「島の再生成も要らない」と書いたのは誤りで、
   **「SOURCE を書き換えなくてよい」と「成果物が変わらない」を混同していた**。
   `src/compiler.rs` に名前が出てこないことが示すのは前者だけ。

   区別は Phase 1c/2/3/4a との対比で明確に出た——あちらは prelude を大幅に育てたが
   島テストは通り、9c は prelude に加えて registry へ組み込みを足したので落ちた。
   つまり**島の成果物を動かすのは prelude の中身ではなく、extern の表**。
   再生成は 1 回で不動点（島が*吐くもの*は変えていないため）、prelude 成果物は
   バイト単位で不変だった。
3. **`command-line-args` の要素 0 はプログラム名**、という一点を守るために `typl` 側に
   スロットを置いた。`typl script.typl a b` の `std::env::args()` は
   `["typl","script.typl","a","b"]`、AOT の `./prog a b` は `["./prog","a","b"]` で食い違うので、
   `typl` の `main` が「ファイル名以降」を `set_command_line_args` で渡す。
   AOT 側は `build_main_wrapper` の `main` が `argc`/`argv` を取らないが、Rust の `std` は
   プロセス開始時に argv を捕まえている（macOS は `_NSGetArgv`、Linux は `.init_array`）ので
   そのまま読める。**両方の走らせ方で実際に確かめた**——同じソースが同じ添字で同じ引数を読む。
4. **保留: `get-internal-run-time`（CPU 時間）と `file-author`。** どちらも `libc`
   （`getrusage` / uid→名前）が要り、ワークスペースは `libc` に依存していない。
   実時間で CPU 時間を代用すると嘘になる。
5. **保留: `machine-version` / `machine-instance` / `software-version` /
   `short-site-name` / `long-site-name`。** ホスト名に `libc` が要り、残りは CL でも `NIL`
   を返してよい。中身の無い定数を並べるより置かない方を選んだ。
6. **保留: `trace` / `untrace` / `step` / `disassemble` / `room` / `ed` / `dribble`。**
   REPL のツール層で、このカタログとは別の作業。
7. **見つけた CL との差（直していない）**: `decode-universal-time` は zone 省略時に
   **UTC** へ分解する（CL は地方時）。タイムゾーンデータベースが無いため。
   CL の 9 個の返り値のうち `daylight-p` と「既定の分解が使った zone」は、
   偽の値を返すのではなく用意していない。CL にもある明示 zone 引数が代わり。

### Stage 9d — ストリーム残差

`make-synonym-stream`（シンボルを介した間接参照。Phase 7b の後なら意味が出る）、
`clear-output` / `clear-input` / `listen` / `read-char-no-hang`
（**ネイティブ層に `listen` はあるが typelisp へ未公開**）、
`read-sequence` / `write-sequence`（現在は `copy-stream`/`read-all`/`write-lines` で代替）。

---

## 実施順序と依存

```
Phase 0（検証 6 件）
  │
  ├─ Phase 1  1a ─→ 1b ─→ 1c ─→ 1d
  │                  │
  │                  └──────────────→ Phase 6b（Array は 1b の型変換に依存）
  ├─ Phase 2  2a、2b
  │
  ├─ Phase 5  5b ─────────────────→ Phase 3e（:key/:test は &key に依存）
  │           5a、5c
  │
  ├─ Phase 3  3a ─→ 3b ─→ 3c ─→ 3d
  │            │     │
  ├─ Phase 4  4a ←──┴─────┘（loop DSL は 3a/3b の集約関数と 4a の block に依存）
  │            └─→ 4b、4c
  │
  ├─ Phase 6  6a、6b、6c
  │
  ├─ Phase 8  8a ─→ 8b ─→ 8c（8c はアーキテクチャ転換が先行条件）
  │            │
  ├─ Phase 7  7a、7b ←──┘（7b の制御変数は 8a のプリンタに依存）
  │
  └─ Phase 9  9a、9b、9c、9d
```

**費用対効果で先に着手するなら** Phase 2b（文字列ユーティリティ）・Phase 3c（集合演算）・
Phase 4a（`block`/`return-from`）・Phase 9c（コマンドライン引数・環境変数）。
いずれも prelude だけで閉じるか、閉じないものでも影響範囲が局所的で、
CL コードの移植と実用スクリプトで真っ先に当たる。

---

## 各 Phase 共通の完了判定

1. `scripts/regen-compiler-island.sh`（島を触ったなら。**島が「吐くもの」を変えたら 2 回**——
   ビルドは新 SOURCE を*旧*島でコンパイルするので 1 パスでは収束しない）→
   `scripts/regen-prelude-bitcode.sh`。**必ずこの順**（prelude は島によってコンパイルされる）
2. `scripts/test-serial.sh`（LLVM の process-wide Context のため直列必須。先に
   `cargo build -p typelisp-front` で staticlib を作らないと AOT テストが古い `.a` を黙って使う）
3. 最低限通すテスト: `prelude_artifacts_test` / `prelude_compiled_test` / `prelude_test` /
   `compile_test` / `compile_file_test` / `editor_keyword_sync_test`。島を触ったなら
   `island_artifacts_test` / `island_self_compile_test` / `island_aot_load_test` と
   `--lib` の `the_rust_and_island_native_method_lists_agree`
4. `editor/emacs/typelisp-mode.el` と `editor/vscode/syntaxes/typelisp.tmLanguage.json` の
   **両方**に新しい名前を追加
5. `docs/functions.md` / `docs/syntax.md` を更新し、
   [cl-missing-classes-and-methods.md](cl-missing-classes-and-methods.md) の該当行を ✅ へ
6. [implementation-log.md](implementation-log.md) に経緯・設計判断を追記、
   [TODO.md](TODO.md) から当該 Phase を消す
7. **`cargo check` が通っただけで完了としない** — dead code 警告の件数を数え、テストを最低 1 本走らせる

---

## リスク一覧

| リスク | 兆候 | 対処 |
|---|---|---|
| Rust 側の native メソッド表だけ足して島の lowering を忘れる | コンパイル時にプロセスが **abort**（エラー報告ですらない） | `the_rust_and_island_native_method_lists_agree` を各 Stage で回す |
| 島を再生成しても収束しない | `the_committed_island_matches_a_fresh_build` が落ちたまま | 島の codegen を変えたら regen は 2 回 |
| prelude だけ再生成して島を後回しにする | prelude 成果物が古いのに digest テストは気付かない（見ているのは prelude.rs だけ） | 常に島 → prelude の順。バイト比較テストのほうが番人 |
| 境界付きジェネリック同士の委譲不可を忘れて設計する | `cannot infer` | Iter 系は毎回ループを書き下ろす前提で工数を積む（§2-3） |
| 新しい名前がエディタ定義から漏れる | `editor_keyword_sync_test` が落ちる | 完了判定 4 |
| GC ルート漏れ（`Vec<Value>` はコレクタから見えない） | `gc_stress` でのみ再現し、通常のテストは通る | 新しい構文再構築を書いたら `checker_gc_stress_test` を回す |
| 表面積の増加でダンプ生成・単型化が重くなる | 起動時間の退行 | **2026-08-20 実測（下記）。+9.2% で収まっている** |
| CL 名が既存メソッド名と衝突する（`get`/`values`/`count`/`member`/`some`） | 解決順が変わって無関係な既存コードが壊れる | 名前を足す前に `registry.rs` と `prelude.rs` を grep。`some` は `Some` 構成子と衝突するので使えない（language-design.md §7.3） |
| Phase 5a の `:include` が単型化・`repr`・パターンマッチへ波及する | 一見無関係なテストが落ちる | `:include` は Phase 5a の最後に、単独の commit で入れる |

### 起動時間の実測（2026-08-20、Phase 1c/2/3/4a/9c 完了時点）

計画着手直前の `bad8b82` と、この計画で入れたもの全部込みを、それぞれ release でビルドして
`(defun main () i32 0)` だけのスクリプトを走らせた時間。交互に各 15 回、同一負荷下。

| | min | 中央 | max | `prelude.typld` |
|---|---|---|---|---|
| `bad8b82`（着手前） | 0.922s | 0.926s | 0.933s | 1,305,970 B |
| 1c/2/3/4a/9c 込み | 1.002s | **1.011s** | 1.025s | 1,867,175 B |

**prelude 成果物 +43.0% に対して起動 +0.085s（+9.2%）。** 表面積に対して線形よりずっと
緩いので、この計画の残りの Phase を入れても起動が破綻する兆候は無い。

測定中に一度だけ 27.4 秒という外れ値が出た。付録 D-2 の GC スラッシュを疑って
再現を試みたが**再現しなかった**——15 回とも 1.00〜1.03 秒に収まり、`--heap-cells 262144`
（既定の 4 倍）でも変わらない。並行して走っていた別のテストが CPU を奪っただけだった。
D-2 は実在するが、`typl` の通常の起動には出ていない。

---

## 付録 A — 地図の全行の写像

[cl-missing-classes-and-methods.md](cl-missing-classes-and-methods.md) の節ごとに、
❌/⚠️/⛔ の各行がこの計画のどこへ落ちたかを示す。**「対象外」と書いた行の理由は §0 にある。**

| 地図の節 | 項目 | 行き先 |
|---|---|---|
| §1.1 数値 | `number`/`real`/`rational` の抽象型 | **Phase 1a**（`Number` トレイトが相当。CL の型階層ではなくトレイト階層で表す） |
| | `integer` の自動昇格 | 対象外（型が別なのは (D1) の帰結。変換は Phase 1b で明示的に） |
| | `fixnum` — 小整数型に演算が無い | **Phase 1a** |
| | `float` — `f32` に演算が無い | **Phase 1a** |
| | `short-`/`single-`/`double-`/`long-float` | **Phase 1a**（`f32`=single、`f64`=double。残り 2 つは対象外） |
| | `complex` | **Phase 1d** |
| §1.2 文字・シンボル | `base-char`/`standard-char`/`extended-char` | 対象外 (D1)。ただし `standard-char-p` 相当の述語は **Phase 2a** |
| | `keyword` の独立型 | 対象外（`keywordp` で足りる） |
| | `null` / `t`（型としての）/ `atom` | 対象外 (D1)(D2) |
| §1.3 リスト・配列 | `list` / `sequence` の抽象型 | 対象外（`Iter` トレイトが相当） |
| | `array` 多次元 | **Phase 6b** |
| | `vector` の `fill-pointer`/`adjustable` | **Phase 6b** |
| | `simple-vector`/`simple-array`/`base-string`/`simple-string` | 対象外 (D1) |
| | `bit-vector` | **Phase 6c** |
| | `string` が不変 | **Phase 2b** で可否判断 |
| §1.4 関数・OOP | `compiled-function`/`generic-function`/`method`/`class`/`standard-object` | 対象外（CLOS・(D1)） |
| §1.5 実行時オブジェクト | `package` | **Phase 9a**（`in-package` まで。実行時オブジェクト化はしない） |
| | `readtable` | **Phase 8c** |
| | `restart` / `condition` 階層 | 対象外 (D3)。写像は **Phase 7a** の表 |
| §2.1 評価・コンパイル | `macroexpand`/`macroexpand-1` | **Phase 4c** |
| | `*macroexpand-hook*` | **Phase 7b** |
| | `eval-when` / `macrolet` / `symbol-macrolet` / `define-compiler-macro` / `load-time-value` | **Phase 4c** |
| | `declare`/`declaim`/`proclaim`/`locally` | 対象外 (D1) |
| | `#'` / `funcall` | 対象外（Lisp-1 なので不要） |
| | `compile`/`compile-file` の fasl 意味 | 対象外（`.typld` ダンプが等価物、§2-10） |
| | `constantly` / `complement` | **Phase 4c** |
| §2.2 型とクラス | `typep`/`type-of`/`subtypep`/`check-type`/`type-error` | 対象外 (D1)(D3) |
| | `coerce`（数値・文字） | **Phase 1b** |
| | `coerce`（シーケンス変換） | 対象外（functions.md §10 の結論。`Vector<T>`↔`Sexpr` は表現が根本的に違う） |
| | `deftype` | **Phase 5c** |
| §2.3 データと制御 | 多値一式 | 対象外 |
| | `setf` の `defsetf`/`define-setf-expander` | 対象外（静的型があるので不要、地図 §3-3 の結論） |
| | `psetf`/`psetq`/`setq` | **Phase 4a** |
| | `pushnew` / `remf` | **Phase 4a** |
| | `block` / `return-from` | **Phase 4a** |
| | `tagbody` / `go` | 対象外（goto） |
| | `destructuring-bind` | **Phase 4a** |
| | `prog`/`prog*`/`prog1`/`prog2` | **Phase 4a** |
| | `typecase`/`etypecase`/`ctypecase` | 対象外 (D1) |
| | `ecase` / `ccase` | **Phase 4a** |
| | `sleep` | **Phase 4a** |
| §2.4 反復 | 拡張 `loop` DSL | **Phase 4b** |
| | `do*` | **Phase 4a** |
| | `mapc`/`mapcar` 多引数/`mapcan`/`mapl`/`maplist`/`mapcon` | **Phase 3b** |
| §2.5 CLOS | 全項目 | 対象外。`describe`/`inspect` も (D1) |
| §2.6 構造体 | `:include` / `:constructor` / `:conc-name` / `:predicate` / `:copier` / スロット初期値 | **Phase 5a** |
| | `:type`/`:initial-offset`/`:named` | 対象外 (D1) |
| §2.7 コンディション | 全体 | 対象外 (D3)。写像・`assert`・`warn`・`Error` 整備は **Phase 7a** |
| §2.8 シンボル | `make-symbol`/`copy-symbol`/`gentemp`、`gensym` 引数、`*gensym-counter*` | **Phase 4c** |
| | `intern` のパッケージ引数 | **Phase 9a** |
| | `symbol-value`/`set`/`boundp`/`symbol-function`/`fboundp`/`symbol-package` | 対象外 (D1)(D5) |
| | `symbol-plist`/`get`/`remprop`/`getf`/`get-properties` | **Phase 3c** |
| | `defparameter` | **Phase 9b** |
| §2.9 パッケージ | `in-package` / `import` / `shadowing-import` / `shadow` / `unuse-package` | **Phase 9a** |
| | `find-package`/`package-name`/`find-symbol`/`do-symbols`/`*package*` 等 | 対象外（パッケージは実行時オブジェクトでない、(D5)） |
| §2.10 数値 | `numberp`/`integerp`/`rationalp`/`floatp`/`realp`/`complexp` | 対象外 (D1) |
| | `gcd`/`lcm` 可変長、整数 `expt` | **Phase 1c** |
| | `ffloor`/`fceiling`/`fround`/`ftruncate`、`isqrt` | **Phase 1c** |
| | `rationalize` | **Phase 1c** |
| | 複素数一式（`complex`/`realpart`/`imagpart`/`conjugate`/`phase`/`cis`） | **Phase 1d** |
| | 浮動小数点の内部表現アクセス | **Phase 1c** |
| | `random` のシード外部指定 | **Phase 1c** |
| | `most-positive-fixnum` 等の定数 | **Phase 1c** |
| | `byte`/`ldb`/`dpb`/`boole` の i64・bignum 拡張 | **Phase 1c** |
| §2.11 文字 | `char/=`、大文字小文字無視の順序比較 | **Phase 2a** |
| | `alphanumericp`/`graphic-char-p`/`standard-char-p`/`upper-case-p`/`lower-case-p`/`both-case-p` | **Phase 2a** |
| | `characterp` | 対象外 (D1) |
| | `char-name`/`name-char`/`char-int`/`digit-char`/`char-code-limit` | **Phase 2a** |
| | `digit-char-p` を CL 準拠へ | **Phase 2a** |
| §2.12 コンス | `caar`〜`cddddr` 28 個 | **Phase 3a**（`cons-cell` ネスト受け手） |
| | `first`〜`tenth`/`rest` | **Phase 3a** |
| | `list*`/`make-list`/`copy-list`/`copy-tree`/`copy-alist` | **Phase 3a** |
| | `nth`/`nthcdr`/`last`/`butlast` | **Phase 3a** |
| | `list-length`/`endp`/`null`/`consp`/`atom`/`listp` | 対象外 (D1)(D2) |
| | `rplaca`/`rplacd`、`nconc`/`nreverse`/`nbutlast`/`nsubst` 等 | **Phase 3d**（(D4) 解禁） |
| | `revappend`/`nreconc`、可変長 `append` | **Phase 3a** |
| | `member`（残りリスト）/`member-if`/`member-if-not` | **Phase 3a / 3b** |
| | `assoc-if`/`rassoc`/`rassoc-if`/`acons`/`pairlis` | **Phase 3b** |
| | `sublis`/`subst`/`subst-if`/`tree-equal` | **Phase 3c** |
| | `union`/`intersection`/`set-difference`/`set-exclusive-or`/`subsetp`/`adjoin` | **Phase 3c** |
| | `ldiff`/`tailp` | **Phase 3c** |
| §2.13 配列 | `make-array`/`aref`/`array-*`/`fill-pointer`/`vector-push-extend`/`adjust-array` 等 | **Phase 6b** |
| | `bit`/`sbit`/`bit-and` 系 | **Phase 6c** |
| | `arrayp`/`vectorp`/`simple-vector-p`/`array-element-type`/`upgraded-array-element-type` | 対象外 (D1) |
| §2.14 文字列 | `string/=`、`string-equal`/`string-lessp` 系 | **Phase 2b** |
| | `string-capitalize`/`nstring-*` | **Phase 2b / 3d** |
| | `string-trim`/`string-left-trim`/`string-right-trim` | **Phase 2b** |
| | 可変長 `concatenate`、`make-string`、汎用 `string`（文字列化） | **Phase 2b** |
| | `search`/`mismatch`、`split-sequence` 相当 | **Phase 2b** |
| | `parse-integer` の `:radix`/`:junk-allowed` | **Phase 2b** |
| | `stringp`/`simple-string-p` | 対象外 (D1) |
| §2.15 シーケンス | `:key`/`:test`/`:test-not`/`:start`/`:end`/`:from-end`/`:count` | **Phase 3e** |
| | `merge`/`copy-seq`/`fill`/`replace`/`map-into`/`concatenate` | **Phase 3b** |
| | `substitute`/`substitute-if`/`nsubstitute` | **Phase 3b / 3d** |
| | `remove`/`remove-duplicates`/`delete`/`delete-if`/`delete-duplicates` | **Phase 3b / 3d** |
| | `notany`/`notevery`/`count-if-not`/`find-if-not`/`remove-if-not` | **Phase 3b** |
| | `search`/`mismatch`（部分列） | **Phase 3b** |
| | `make-sequence` | **Phase 3a**（`Vector::filled`） |
| | `coerce`（シーケンス変換）/`nreverse` | 対象外 / **Phase 3d** |
| §2.16 ハッシュ表 | `make-hash-table` の `:test`/`:size`/`:rehash-*` | **Phase 6a** |
| | `maphash`/`with-hash-table-iterator`/`hash-table-size` | **Phase 6a** |
| | `sxhash`（＋ユーザ定義型の `Hash`） | **Phase 6a** |
| | `hash-table-p`/`hash-table-test` | 対象外 (D1) |
| §2.17 パス名・ファイル | `truename`/`file-write-date`/`file-author`/`directory`/`ensure-directories-exist` | **Phase 9c** |
| | ワイルドカード・論理パス名・ホスト/デバイス/バージョン成分 | 対象外 |
| §2.18 ストリーム | `make-synonym-stream` | **Phase 9d** |
| | `clear-output`/`clear-input`/`listen`/`read-char-no-hang` | **Phase 9d** |
| | `read-sequence`/`write-sequence` | **Phase 9d** |
| | `y-or-n-p`/`yes-or-no-p` | **Phase 9c** |
| | `streamp`/`input-stream-p`/`output-stream-p`/`stream-element-type` | 対象外 |
| | `*trace-output*`/`*query-io*`/`*terminal-io*`/`*debug-io*` | **Phase 7b** |
| §2.19 プリンタ | `~/name/` ディレクティブ | **Phase 8a** |
| | `prin1`/`princ`/`write`/`write-to-string`/`prin1-to-string`/`princ-to-string`/`pprint` | **Phase 8a** |
| | `*print-base*`/`*print-radix*`/`*print-case*`/`*print-lines*`/`*print-gensym*`/`*print-array*`/`*print-readably*`/`*print-escape*` | **Phase 7b** |
| | `write-byte`/`read-byte` | **Phase 8a** |
| | `set-pprint-dispatch` 系 | 対象外 |
| §2.20 リーダ | `read-from-string` の読み終わり位置 | **Phase 8b** |
| | `read-preserving-whitespace`/`read-delimited-list` | **Phase 8b** |
| | `readtable` 関連一式 | **Phase 8c** |
| | `*read-base*`/`*read-default-float-format*`/`*read-suppress*`/`*read-eval*`/`with-standard-io-syntax` | **Phase 7b** |
| §2.21 システム・環境 | `require`/`provide`/`*modules*` | **Phase 9b** |
| | `*features*` を読み込み中に書き換える | **Phase 8c**（アーキテクチャ転換の副産物） |
| | `compile-file-pathname`/`*compile-file-pathname*`/`*load-pathname*` | **Phase 9b** |
| | `get-internal-run-time` | **Phase 9c** |
| | `decode-`/`encode-universal-time`/`get-decoded-time` | **Phase 9c** |
| | `sleep` | **Phase 4a** |
| | `room`/`ed`/`dribble`/`trace`/`untrace`/`step`/`disassemble` | **Phase 9c** |
| | `apropos`/`apropos-list`/`inspect`/`describe` | 対象外 (D1) |
| | `lisp-implementation-type` ほか環境問い合わせ | **Phase 9c** |
| | `user-homedir-pathname`・環境変数・コマンドライン引数 | **Phase 9c** |
| | `(setf documentation)` | **Phase 4a**（place 機構に乗る）または対象外。実施時判断 |

## 付録 B — 本計画で覆した既存方針

| 覆すもの | 書き換える場所 |
|---|---|
| **(D4) 破壊的操作を原則採らない** → 実現可能なものは採る | language-design.md §0 の (D4)、functions.md §5 の「`set-car`/`set-cdr` は完全に撤去済み」 |
| **(D5) 動的束縛が無いので CL の制御変数は代入可能なグローバルに読み替える** → 読み替えは維持しつつ、スコープ付き再束縛を足す | language-design.md §0 の (D5)、functions.md §15.1 |
| **小整数型・`f32` は型登録だけ** → 全演算を付ける | registry.rs の `with_builtins`（551-576行）、functions.md §1 |

**`prelude.rs` 95-104 行の `Sexpr` 降格は覆さない。**あのコメントが禁じていたのは
「`Sexpr` 専用の自由関数が裸の CL 名を独占すること」であって、CL 名そのものではない。
受け手型でディスパッチする `defmethod` なら独占は起きないので、降格を維持したまま
`length`/`first`/`member` といった名前を `Vector<T>` と `cons-cell` の上で復活できる
（§1.2）。同じ議論が再燃したときは、ここが一次資料になる。

## 付録 C — 策定時に見つけた、計画本体とは別の小さな不整合

**4 件とも解消済み（2026-08-20、Phase 3 と同じコミット）。**

1. ~~**`docs/functions.md` §7.1 のエラー型表が 4 つしかない**~~（`FileError` を追加）。
2. ~~**地図 §3-2 の「`lambda` と `defmethod` はいまも `&rest` のみ」が不正確**~~
   （`lambda` は `&rest` のみ、`defmethod` は `&rest` すら受け付けない、に修正。2 箇所）。
3. ~~**`.gitattributes` が `src/compiler_island.bc binary` を指している**~~
   （`src/compiler_island.typld` へ改名し、同じくバイナリの
   `crates/typelisp-front/src/prelude.typld` も追加）。
4. **`prelude.rs` 1145-1148 行の「兄弟の境界付きジェネリックへ委譲できない」コメント**
   （策定後に発見）。すぐ下の `elt` 自身が反証だったうえ、*なぜ*そう見えたかも
   実際とは違っていた——Phase 3 の節を参照。何が本当に真だったかを書き直した。

---

## 付録 D — 実装中に見つかった、この計画の範囲外の問題

どちらも **main で再現する既存の問題**で、Phase 9c の作業中に偶然踏んだもの。
**2026-08-21 に両方とも実装した**（この節は発見時の記述を残し、各項の末尾に結果を足す）。

### D-1. AOT 実行ファイルから prelude の関数が一切呼べない

`compile::aot::compile_file` は `load_compiler`（＝島の `load_aot`）しか呼ばず、
**`load_prelude` を呼ばない**（`src/compile/aot.rs:146`）。結果として
`(compile-file "src" "out")` で作った実行ファイルは、組み込みと自分の定義しか使えない。

main で確認した範囲（`typl` 経由）:

| 呼ぶもの | 結果 |
|---|---|
| `Vector::new` / `push` / `len`（組み込みメソッド） | 通る |
| `machine-type` / `getenv` / `file-directory-p`（組み込み関数） | 通る |
| `abs` / `gcd` / `zerop` / `identity` / `to-string` | `no such function` |
| `random-state-p` / `parse-namestring`（既存の prelude `defun`） | `no such function` |

**JIT（`(compile name)`）は無関係**——そちらは prelude が載ったプロセスの中で動くので、
既存の `abs` も Phase 9c の `directory-p` も通る（実際に確かめた）。
問題は `compile-file` が作る**独立した実行ファイル**だけ。

**この計画にとっての意味は大きい**: Phase 1c/2/3/4a/9c で足したものはほぼ全て prelude に
あるので、インタプリタと JIT では使えるが AOT 実行ファイルからは使えない。
§2-2 の「prelude だけに足したなら compile 対応は自動」は *prelude 自身の本体が
コンパイルされること*については正しく、*利用者の AOT プログラムから呼べること*は別の話だった。

`tests/compile_file_test.rs` はこの穴を踏んでいない——どのテストも prelude 関数を
コンパイル対象のコードから呼んでいないため（唯一 prelude 関数が出てくる 1056 行は
`(eval (quote ...))` の中、つまり実行時のインタプリタ経由）。

**結果（2026-08-21・実装済み）**: `compile_file` が `load_compiler` の前に
`prelude_bootstrap::load_for_aot` を呼ぶようになった。JIT 用の `load` との違いは
「このプロセスへ入れる」か「実行ファイルが**自分で持つ**」かの一点で、具体的には 2 つ:

1. **委託済みビットコードを出力モジュールへリンクする**（`Module::link_in_module`）。
   位置は `rt_*` の前方宣言の直後、**最初の `add_compiled_function` より前**でなければ
   ならない——島の `compile-call` は呼び先を `get-function` でこのモジュールに探し、
   無ければエラーではなく**プロセスを abort** するので、`tl_abs` は「書き出す時」でなく
   「本体を翻訳し始める時」に居る必要がある。
2. **prelude の `defvar` を実行ファイルの起動列へ足す**。コンパイル済み本体は
   グローバルを**焼き込んだスロット番号**で読むので、番号は付け直せない。
   `$global_init$` の列は prelude のぶんが先、ファイル自身のぶんが後——
   `load_for_aot` が `compile_file` の 1 行目より前に走るので、採番もこの順になる。
   `load_for_aot` は dump が記録している `globals` と自分が拾った `defvar` の**順序と個数を
   突き合わせて**から返す（ずれたら 1 スロットずれた実行ファイルが黙って出来るだけなので）。
3. **ファイルが出す本体を全部、翻訳の前に宣言する**（計画に無かった 3 つ目）。
   単型化の束は**チェッカーが実体を作った順**に並んでいて、これは呼び出し順ではない——
   `length <vector-iter<i32>,i32>` が、それが呼ぶ `vector-iter::next <i32>` より前に来る。
   島は呼び先をこのモジュールに探して**無ければ abort** するので、宣言が先に要る。
   prelude が入るまでこれが問題にならなかったのは、ジェネリックを*名指しできない*
   ファイルはジェネリックを*実体化もしない*から。

副次的に `dump::restore_dump`（AOT の `eval` 環境）も変わった。以前は
「プログラム自身のユニットのグローバルだけが他人の所有物」だったが、いまは
prelude のグローバルも実行ファイルが作るので、**全ユニットぶん `bind_globals` する**。
そうしないと `eval` した `*print-pretty*` がコンパイル済みコードとは別の記憶域を読む。

テストは `compile_file_test` の 3 本（組み込みでない prelude 関数 `abs`/`gcd`/`zerop`、
prelude の**ジェネリック**——こちらは単型化された実体がファイル側に落ちて来る経路——、
prelude のグローバル `*print-right-margin*`）。

### D-2. 小さいヒープを渡すと GC がスラッシュする

`Heap::cons` は「free リストが空になったら `gc()`、それでも空なら `grow()`」という順序なので
（`heap.rs:1508-1522`）、**`gc()` が 1 セルでも回収すると `grow()` は呼ばれない**。
生存量が容量にわずかに届かない状態に入ると、以後ほぼ毎回の割り当てが全体マークを引き起こす。

実測: `tests/editor_keyword_sync_test.rs` は `Heap::with_capacity(1 << 16)` で prelude を
読み込み、**6 テストで 649.74 秒**（1 回あたり約 108 秒）かかる。同じ `load_prelude` を
`1 << 18` で呼ぶ `prelude_test` は 1 回 1.2 秒。約 90 倍。

`1 << 16` は `src/main.rs` の `DEFAULT_HEAP_CELLS` でもあり、
`tests/` の多くも同じ値を使っている。prelude が育つほど悪化する種類の問題なので、
リスク表の「表面積の増加で起動時間が退行する」と同じ根から出ている。

直すなら `grow()` の発火条件（回収後の空き率が閾値を下回ったら伸ばす）だが、
固定アリーナという設計上の約束（`Error::HeapExhausted` を出すこと）と相談が要る。

**結果（2026-08-21・実装済み）**: 予想どおり `grow()` の発火条件だった。`Heap::cons` は
free リストが空になったら `gc()` を回し、**回収後の空きが容量の 1/4 に届かなければ伸ばす**
（`HEADROOM_DIVISOR`）。固定アリーナとの相談は不要だった——`set_growth_limit(0)` の heap では
`grow()` が即座に false を返すので、`Error::HeapExhausted` の意味は
「回収が何も返さず、成長も許されていない」のまま 1 文字も変わらない。

回帰テストは `mem_test` の 2 本（`a_live_set_near_capacity_grows_the_arena_instead_of_...` と
`plentiful_garbage_does_not_grow_the_arena`）で、**壁時計でなく GC 回数**を見る
（`Heap::gc_count` を新設）。生存 900 / 容量 1000 の heap へ 20,000 回割り当てて、
旧規則なら 200 回の全体マークが 6 回に減る。

**ただし `editor_keyword_sync_test` の 649.74 秒は GC だけが原因ではなかった。**
修正後の実測は 502.24 秒で、残りを `sample` で見ると
`install_compiled_library` → `LLVMGetFunctionAddress`（prelude ビットコードの JIT）と
その `COMPILE_LOCK` 待ちが支配的だった——6 テストがそれぞれ `load_prelude` を呼び、
6 回とも prelude 全体を JIT している。この節が「約 90 倍」の根拠にした
`prelude_test`（1 回 1.2 秒）との差は、**heap の容量差ではなく prelude を JIT するか
どうかの差**を多く含んでいた可能性が高い。GC スラッシュは実在し（上のテストが示す）
直したが、この 1 本を速くしたければ次は「テストごとに prelude を JIT し直す」ほうを見る。
