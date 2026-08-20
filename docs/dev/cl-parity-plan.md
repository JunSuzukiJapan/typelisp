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

- **§2-3 の「境界付きジェネリックは別の境界付きジェネリックを呼べない」は現時点で誤り。**
  `(defun mylen2<I,A> ((it I)) i32 (where (Iter I (Item A))) (mylen it))` は通る。
  根拠として引いた `prelude.rs` 1145-1148 行のコメントは自分自身が反証になっている——
  そのすぐ下の `elt` は `(nth n it)` と**実際に委譲している**（コメントは「`elt` が `nth` の
  ループを複製している」と書いているが、コードは複製していない）。`check_call` の bounds 検証は
  10192-10208 行で呼び出し側の `where` 節の関連型ピンを伝播するようになっており、
  この制約は既に解消済み。**「Iter 系を 1 つ足すたびにループを書き下ろす」前提で積んだ工数は不要**
  ——これは計画中で最大の見積り誤差。コメントの修正は付録 C に足した。

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

`ffloor`/`fceiling`/`fround`/`ftruncate`、`isqrt`、整数の `expt`、可変長 `gcd`/`lcm`、
`rationalize`、浮動小数点の内部表現アクセス（`float-sign`/`float-digits`/`float-precision`/
`decode-float`/`integer-decode-float`/`scale-float`/`float-radix`）、
`most-positive-fixnum`/`most-negative-fixnum`/`most-positive-double-float`/`least-positive-*`/
`double-float-epsilon` 等の定数一式、**乱数のシードを外から与える手段**（現状
`make-random-state-fresh` は壁時計から採るので実行を跨いだ再現ができない）、
`byte`/`byte-size`/`byte-position`/`ldb`/`ldb-test`/`dpb`/`mask-field`/`deposit-field`/`boole` の
`i64`・`bignum` への拡張（現在 `i32` のみ）。

### Stage 1d — 複素数

`complex` 型を `bignum`/`ratio` と同じ heap-boxed 方式（`TAG_BOXED` ポインタ）で新設。前例を
そのまま踏襲できる（[[typelisp-bignum-ratio]]／`rt_bignum_*` 26 関数の構成）。

- 生成 `(complex::new re im)`（CL の `complex` 関数は薄い別名）
- 成分は受け手優先: `(realpart z)` / `(imagpart z)` / `(conjugate z)` / `(phase z)` / `(cis theta)`
- `sqrt`/`log`/`expt`/`asin`/`acos` が CL では複素数を返す場面（現在は panic か NaN）の再定義
- `Eq`・`print-object`・`as`/`try-as`・JIT/AOT 対応まで

---

## Phase 2 — 文字・文字列層

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

`:key` / `:test` / `:test-not` / `:start` / `:end` / `:from-end` / `:count`。
**前提**: `lambda` と `defmethod` の `&optional`/`&key` 対応（Phase 5b）。

- `:test` は現在トレイト境界（`Eq`）に固定されている等価性を関数引数で差し替える話なので、
  **境界と噛み合わせる設計判断が要る**——`Eq` 境界版と `:test` 版のどちらを既定にするか、
  両立させるなら名前をどう分けるか。
- `:key` は戻り型の型変数が省略時に決まらない懸念がある（Phase 0.3）。

---

## Phase 4 — 制御構造とマクロ層

### Stage 4a — 脱出と代入

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

typelisp の `loop` は**無限ループのみ**で、CL の LOOP DSL とは名前が同じだけの別物。

- 既存の無限ループ `loop` は **CL の simple loop 形**として残し、
  **第 1 要素がキーワードなら DSL に分岐**する（CL 自身の規則と同じ。Phase 0.6 で確認）。
- 節: `for` / `in` / `on` / `across` / `=` / `then` / `from` / `to` / `below` / `by` / `repeat` /
  `with` / `while` / `until` / `collect` / `append` / `sum` / `count` / `maximize` / `minimize` /
  `when` / `unless` / `if` / `do` / `initially` / `finally` / `return` / `named` / `thereis` / `always`。
- **静的型が問題にならない**: `collect` は `(let ((acc (Vector::new))) … (push acc e) … acc)` へ
  展開すれば要素型が推論で決まる。`sum`/`maximize` も同様に初期値の型から決まる。
- `named` は Phase 4a の `block`/`return-from` に依存する。

### Stage 4c — 評価とマクロ

`macroexpand` / `macroexpand-1`（**マクロ展開結果をプログラムから覗く手段が無い**——デバッグに効く）、
`macrolet` / `symbol-macrolet`、`eval-when`、`define-compiler-macro` / `compiler-macro-function`、
`load-time-value`、`constantly`（現在の `const` は 2 引数版で別物）、`complement`、
`gensym` のプレフィクス引数と `*gensym-counter*`、
uninterned シンボル生成 `(Symbol::new name)`（CL の `make-symbol`）/ `copy-symbol` / `gentemp`。

`*macroexpand-hook*` は (D5) 側の判断（Phase 7b）に合流させる。

---

## Phase 5 — 定義形の拡張

### Stage 5a — `defstruct`

**まずオプションリスト構文 `(defstruct (name opts...) fields...)` を新設する**（§2-9。
今は構文自体が無いので、以下の全部がここに依存する）。

- スロットの初期値（フィールドのデフォルト値）
- `:constructor` — BOA コンストラクタ、キーワード引数コンストラクタ、複数コンストラクタ
  （現在は全フィールドを位置引数で受ける `new` 1 つで固定）
- `:conc-name` / `:predicate` / `:copier`
- `:include`（構造体の継承）。フィールドの前置き連結。**型の継承は現在まったく無い**ので、
  単型化・`repr`・パターンマッチの各層に影響する。この Stage で最も重い

`:type` / `:initial-offset` / `:named`（表現を list/vector に変える指定）は (D1) と衝突するので対象外。
`:print-function` / `:print-object` は `impl print-object` が既に相当。

### Stage 5b — `lambda` / `defmethod` の `&optional` / `&key` / `&rest`

- `defmethod` は **`&rest` から**（今はそれすら無い、§2-8）
- `lambda` の `&optional`/`&key`（checker.rs 7671-7674 の明示エラーを外す）
- `deftrait` のメソッドと `labels` も同じ判断が要る
- 副産物として、地図 §3-2 の「`lambda` と `defmethod` は `&rest` のみ」という不正確な記述を直す

### Stage 5c — `deftype`

型別名（ジェネリック引数付きを含む）。checker の型解決にエイリアス表を足すだけで、
実行時表現には影響しない。静的型付けと最も親和的な未実装項目。

---

## Phase 6 — コレクション

すべて §1.1 の「Rust 風・受け手優先」に従う。生成は `Type::new`、操作は受け手の型で
ディスパッチする `defmethod`、CL 名は必要な分だけ薄い別名か checker 糖衣で被せる。

### Stage 6a — ハッシュ表

- **`Hash` トレイト ＋ `sxhash`**。現在ハッシュ値を取り出せず、ユーザ定義型をキーにする手段も無い
- 生成は既存の `(HashTable::new)` を維持し、CL の `make-hash-table` の
  `:test` / `:size` / `:rehash-size` / `:rehash-threshold` を `HashTable::new` の `&key` として足す
- 反復は既存の `iter`/`doiter` と `entries`/`keys`/`values` が一次 API。`maphash` は
  受け手優先の `defmethod`（`(maphash h f)`）として並べる。`with-hash-table-iterator`
- `hash-table-count` / `hash-table-size` は型名を埋めず `count`（既存）/ `size` にする

### Stage 6b — 多次元配列 `Array<T>`

`Vector<T>` と同じく `RtValue::Struct` を流用（専用の `RtValue` バリアントは作らない
——[[typelisp-vector-defstruct-revert]] の原則）。次元列と平坦な要素列を持つ。

| 一次 API（Rust 風） | CL 名 | 備考 |
|---|---|---|
| `(Array::new dims init)` | `make-array` | `dims` は `Vector<i32>`。`Vector::new` と同じ静的メソッド形 |
| `(get a idx)` / `(set a idx x)` | `aref` / `(setf (aref …))` | `idx` は `Vector<i32>`。`Vector<T>` の `get`/`set` と同名・同形 |
| `(row-major-get a i)` / `(row-major-set a i x)` | `row-major-aref` | 平坦添字 |
| `(rank a)` `(dimension a n)` `(dimensions a)` `(total-size a)` | `array-rank` / `array-dimension` / `array-dimensions` / `array-total-size` | 型名を関数名に埋めない |
| `(in-bounds a idx)` `(row-major-index a idx)` | `array-in-bounds-p` / `array-row-major-index` | |
| `(adjust a dims)` `(push-extend a x)` `(pop a)` `(fill-pointer a)` | `adjust-array` / `vector-push-extend` / `vector-pop` / `fill-pointer` | |
| `(iter a)` | — | `Iter` を実装すれば `map`/`filter`/`doiter` がそのまま効く |

`(aref a i j k)` のような**可変個の裸添字**は `defmethod` がアリティで解決しないので、
**checker の糖衣**で `(row-major-get a (row-major-index a <添字の Vector>))` へ展開する
（`check_variadic_arith` checker.rs:9260 と同じ手法）。`(setf (aref a i j) v)` は既存の
呼び出し形 place 機構（`check_setf_call_place` checker.rs:9031。`get`→`set` の特例）に乗る。

`array-element-type` / `simple-vector-p` / `adjustable-array-p` / `array-has-fill-pointer-p` は
受け手の静的型が既に答えている問い（(D1)）なので対象外——対応表にその旨を書く。
`svref` / `array-displacement` は実装可否を実施時に判断する。

### Stage 6c — ビットベクタ `BitVector`

生成は `(BitVector::new n)`、操作は `get` / `set` / `len` / `bit-and` / `bit-ior` / `bit-xor` /
`bit-not` を受け手優先の `defmethod` で。CL の `bit` / `sbit` は薄い別名、`bit-vector-p` は (D1)。

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

- ファイルシステムへの問い合わせ層: `truename` / `file-write-date` / `file-author` / `directory` /
  `ensure-directories-exist`（現在は `probe-file` だけ）
- `user-homedir-pathname`、**環境変数を読む手段**（現在まったく無い）
- **コマンドライン引数の取得** — CL 標準にも無いが、`typl file.typl` でスクリプトを書く以上ほぼ必須
- `get-internal-run-time`（CPU 時間。実時間は 2026-07-31 に実装済み）
- `decode-universal-time` / `encode-universal-time` / `get-decoded-time`
  （**現在は分解・合成が無いので日時として読める形にできない**）
- `lisp-implementation-type` / `-version` / `machine-type` / `machine-version` /
  `machine-instance` / `software-type` / `software-version` / `short-site-name` / `long-site-name`
- 対話環境向け: `y-or-n-p` / `yes-or-no-p`、`trace` / `untrace` / `step` / `disassemble`、
  `room` / `ed` / `dribble`（REPL があるので相性は良い。`apropos`/`describe` は (D1) で対象外）

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
| 表面積の増加でダンプ生成・単型化が重くなる | 起動時間の退行（現在 1.07s） | `scripts/bench-prelude.sh` を Phase 境界で測る |
| CL 名が既存メソッド名と衝突する（`get`/`values`/`count`/`member`/`some`） | 解決順が変わって無関係な既存コードが壊れる | 名前を足す前に `registry.rs` と `prelude.rs` を grep。`some` は `Some` 構成子と衝突するので使えない（language-design.md §7.3） |
| Phase 5a の `:include` が単型化・`repr`・パターンマッチへ波及する | 一見無関係なテストが落ちる | `:include` は Phase 5a の最後に、単独の commit で入れる |

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

いずれも実施のついでに直す。

1. **`docs/functions.md` §7.1 のエラー型表が 4 つしかない**（`FileError` が漏れている。
   実装は `registry.rs` の `builtin_error_defs` に 5 つある）。
2. **地図 §3-2 の「`lambda` と `defmethod` はいまも `&rest` のみ」が不正確**——
   `defmethod` は `&rest` すら受け付けない（`parse_defmethod_sig_inner` checker.rs:6641）。
3. **`.gitattributes` が `src/compiler_island.bc binary` を指している**が、実ファイルは
   2026-08-20 のダンプ化で `src/compiler_island.typld` に改名済み。バイナリ属性が効いていないので
   git が line-diff / text-merge を試みうる。
