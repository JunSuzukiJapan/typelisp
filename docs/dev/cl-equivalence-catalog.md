# typelisp: CL同等の表現力のための関数・特殊形カタログ

最終更新: 2026-06-18（本文は提案時点のまま。2026-07-11 に完了状況の注記を追加）

> **完了状況（2026-07-11時点）**: 本書で提案された項目はほぼすべて実装済み——`case`/`until`/
> `while-let`/`do`（§1.1、2026-06-19）、プリミティブ型への`defmethod`拡張（§0.1、`char`/`string`等の
> `Eq`/`Ord`実装として実現）、`HashTable<K,V>`（§2.2 a）、`Vector<T>`（§2.2 b）はいずれも
> [implementation-log.md](implementation-log.md)に実装記録がある。本文はあくまで**提案時点の
> 原文**として残し、個々の項目に完了注記を追記する形で更新している。

このドキュメントは [language-design.md](language-design.md) §3（特殊形カタログ）・§4（関数カタログ）を
「CLと同等のコードが書けるか」という観点で再点検し、欠けている項目を追加した上で、各項目を
**Rustで実装すべきもの** / **TypeLispで実装できるもの**（`defun`/`defmacro`）に分類するもの。
実装そのものはここでは行わない。実装順序の参考として末尾に簡単な提案を付す。

## 0. 分類基準

- [language-design.md](language-design.md) §4 の原則を継承: ヒープ/ランタイム/IO/プリミティブ演算/
  ネイティブ表現へのアクセスを要するものは **Rust**、それらの組合せで書けるものは **TypeLisp**
  （`defun`/`defmacro` で実装）。
- `defmacro`/`quote`/`quasiquote`/`gensym` が実装済みになったことで、従来 Rust の checker で
  desugar するしかなかった「構文糖衣のみの特殊形」は、TypeLisp の `defmacro` で実装し直せる
  可能性がある。本書ではこの観点を新たに導入する。
- **ハッシュテーブル/ベクタ/char/string は、CL流のハイフン付き自由関数ではなく `defmethod` による
  メソッドとして設計する**（`(get h k)`/`(upcase s)` 等）。理由は §0.1 を参照。
- 多値 (`values`/`multiple-value-bind`)・`catch`/`throw`・コンディションシステム
  (`handler-case`/`define-condition`/`restart`) は [language-design.md](language-design.md) §7
  の方針（`Result`+`panic` のみ、`?`/try 無し、trait/CLOS 無し）と衝突するため、**対象外と
  明記するだけ**とし、本書では候補として扱わない。

### 0.1 前提となるRust拡張: プリミティブ型への `defmethod` 対応【実装済み】

以下は提案当時（2026-06-18）の現状分析。`check_defmethod`/`check_instance_method`は`prim_type_path`
経由でプリミティブ型をレジストリパスへマップするよう拡張済みで、`i32`/`char`/`string`等への
`defmethod`（`Eq`/`Ord`トレイト実装等）は実際に使われている——以下の記述は提案当時の制約説明として残す。

提案当時の現状: `defmethod` の受け手型は `Type::Named`（`Option`/`Result`/`Sexpr`/ユーザ `defstruct`）限定。
`i32`/`i64`/`f64`/`char`/`bool`/`Str` は `Type` のプリミティブ variant であり、`Type::Named` を
経由しないため、以下2箇所の型分岐をどちらも素通りして `defmethod` を使えない:

- `check_defmethod`（[src/check/checker.rs:653-660](../../crates/typelisp-front/src/check/checker.rs)）— 受け手型が
  `Type::Named` でなければ `"defmethod: receiver must be a data type"` エラー。
- `check_instance_method`（[src/check/checker.rs:997-1000](../../crates/typelisp-front/src/check/checker.rs)）— 受け手の
  静的型が `Type::Named` でなければ即 `type_fq = None` となり、インスタンスメソッド解決が
  発生しない。

**拡張内容**: 上記2箇所に、プリミティブ `Type` をレジストリ上の合成パス（例 `Type::Char →
Path::root("char")`、`Type::Str → Path::root("str")`、`Type::I32 → Path::root("i32")` 等）に
マップするケースを追加し、レジストリにプリミティブ型用の空 `AdtDef`（`variants` は空、`assoc`
のみ使う）を `with_builtins`（[src/check/registry.rs:135](../../crates/typelisp-front/src/check/registry.rs)、
`option_def`/`result_def`/`sexpr_def` と同型のパターン）で登録する。

**trait/動的ディスパッチは不要**。[language-design.md](language-design.md) §8 で対象外とされている
「trait / 動的ディスパッチ」とは別物 — 既存の「受け手の静的型で一意に解決する」という単一・静的
ディスパッチの仕組みを、対象を `Type::Named` からプリミティブ `Type` にも広げるだけ。

この拡張により以下が可能になる:

1. `char`/`Str` へのメソッド追加（§2.2 d）。
2. 型ごとの `eq` をメソッドとして定義でき（`i32`/`char`/`Str`/`bool` 用に Rust で、`Sexpr`/
   ユーザ `defstruct` 用に TypeLisp or Rust で）、`case` 等のマクロは型を意識せず
   `(eq scrutinee key)` を生成するだけでよい（型ごとの解決は展開後の通常のインスタンスメソッド
   解決が行う。詳細は §1.1 の `case` の項）。

`HashTable<K,V>`/`Vector<T>`（§2.2 a/b）は最初から `Type::Named` な新規型として作るので、この
拡張とは独立に（拡張なしで）`defmethod` を使える。

### 0.2 命名規則の見直し: 関数・特殊形の名前に `!` を一切使わない

`!` を操作名の接尾辞として使うのは **Scheme の作法**（`set!`/`vector-set!`等）であり、CL同等の
表現力を目指す typelisp には馴染まない。本書では破壊的（mutating）操作（`set`/`push`/`pop`/
`remove`/`clear` 等）だけでなく、`panic`/`unreachable`/`todo` のような発散（diverge）形も含めて、
関数・特殊形の名前に `!` を一切使わない。

`!` という記号自体は **`Never` 型の表記**（[language-design.md](language-design.md) §7.2、例
`(fn (i32) !)`）としてのみ使われ、命名規則上の接尾辞ではない。

[language-design.md](language-design.md) §3・§4.1・§7.3 もこの方針に合わせて修正済み
（`panic`/`unreachable`/`todo`/`set-car`/`set-cdr`/`vector-set`/`vector-push`、すべて `!` なし）。
実装済みの `panic` キーワード（[src/check/checker.rs](../../crates/typelisp-front/src/check/checker.rs)）と関連テストも
本タスクでリネーム済み。

## 1. 特殊形・マクロ

### 1.1 既存ドキュメントで「未実装」とされている特殊形の再分類

| 特殊形 | 旧分類（想定） | 新分類 | 理由 |
|---|---|---|---|
| `case` | Rust（checker特殊形） | **TypeLisp**（`defmacro`、実装済み2026-06-19） | `(cond ((eq x v1) ...) ...)` への構文展開のみで実現可能。**trait導入は不要** — `eq` を対象型ごとに `defmethod` で定義しておけば（§0.1 の拡張後は i32/char/Str 等にも可能）、マクロは型を意識せず `(eq scrutinee key)` を生成するだけでよく、型ごとのディスパッチは展開後に checker の既存インスタンスメソッド解決が行う。シンボルキーの quote 要否などの細部は実装時に設計確定が必要 |
| `until` | Rust | **TypeLisp**（`defmacro`、実装済み2026-06-19） | `(while (not cond) body...)` への展開のみ |
| `while-let` | Rust | **TypeLisp**（`defmacro`、実装済み2026-06-19） | `if-let` と同様、`(loop (match expr (pat body...) (_ (break))))` 相当に展開可能 |
| `do` | Rust | **TypeLisp**（`defmacro`、実装済み2026-06-19） | `dotimes`/`dolist` と同型の `let`+`while`+`setf` 展開で複数変数・ステップ式も表現可能 |
| `doiter` | 仕様未確定 | **TypeLisp**（`defmacro`、実装済み2026-06-30） | イテレータ抽象（`Iter`トレイト、`next: Self -> Option<Item>`）として実装。`dotimes`/`dolist`と同じ「`gensym`で`coll`を一度だけ評価する隠しbinding」+`while-let`呼び出しだけの薄いマクロ——`var`の型はマクロ展開時には分からないが、展開後の`(some var)`という構成子パターンの型を`Checker::check_ctor_pattern`がscrutinee（`next`の戻り値`Option<Item>`）から自動推論するため、checker特殊形は不要（`case`/`do`/`while-let`と同列）。`Sexpr`は要素型固定なし（ジェネリックな`Iter<Item>`を実装すべきでない、というユーザー判断）のため対象外、`Vector<T>`（新規導入）の`vector-iter<T>`が動作確認の実装例 |
| `the` | Rust | **Rust**（変更なし） | 型注釈の検査自体が目的のため、構文展開だけのマクロでは実現不能（checker拡張が必須） |

### 1.2 新規追加候補

| 項目 | 分類 | 理由 |
|---|---|---|
| `labels`（ローカル再帰関数定義） | **Rust** | `lambda` は定義時点の環境を捕捉するクロージャのため、自分自身を参照する再帰ローカル関数が現状書けない（`let` で束縛した変数は対応する `lambda` 本体からまだ見えない）。`letrec` 相当の新しい束縛規則が必要で、新ASTノード＋checker拡張が必要 |
| `unreachable` / `todo` | **TypeLisp**（`defmacro`） | 既存の `panic` 特殊形を呼ぶだけで実現可能。例: `` (defmacro todo () `(panic "todo")) ``。新しい特殊形をRustに追加する必要はない |
| `exit` | **Rust** | プロセス終了はOS呼び出しが必要。戻り型 `!` を持つ**通常の組み込み関数**として実装すれば良く、特殊形にする必要はない |

## 2. 関数カタログ（拡張）

### 2.1 既存カタログ（language-design.md §4.1）の再分類

| 関数 | 旧分類 | 新分類 | 理由 |
|---|---|---|---|
| `consp` / `atom` / `null` | Rust | **TypeLisp** | `Sexpr` の `match` で `Cons`/`Nil` を判定するだけで書ける（`match` は既に `Sexpr` の構成子パターンに対応済み） |
| `eq` | Rust（`Sexpr`専用の単一関数） | **型ごとに `defmethod`**（§0.1 拡張後） | `i32`/`char`/`bool`/`Str` は Rust 側で個別定義、`Sexpr`/ユーザ `defstruct` は TypeLisp（`match`）または Rust。型を問わず `(eq a b)` の形で呼べるようになり、`case`（§1.1）が型を意識せず書ける |
| `equal`（新規） | — | **TypeLisp** | `eq` と `match` を使った構造的な再帰比較として実装可能 |

IO系（`print`/`println`/`princ`/`format`/`read`/`read-line`）・型変換・i32算術等、既存カタログのまま
変更がない項目は本書では再掲しない。

### 2.2 新規カテゴリ（CL同等のために追加が必要）

#### a. ハッシュテーブル（提案当時は既存ドキュメントに記載なしだったが【実装済み】、`src/check/registry.rs`の`hashtable_def`）

`HashTable<K,V>` を `Option`/`Result`/`Sexpr` と同じ仕組みの組み込み **nominal型** として登録する
（[src/check/registry.rs](../../crates/typelisp-front/src/check/registry.rs) の `option_def`/`result_def` と同型のパターン。
`Type::Named` なので §0.1 の拡張は不要、`defmethod` がそのまま使える）。CL流のハイフン付き自由関数
（`make-hash-table`/`gethash`等）ではなく、メソッドAPIとして設計する。

| 項目 | 呼び出し形 | 分類 | 備考 |
|---|---|---|---|
| 構築 | `(HashTable::new)` | **Rust** | 静的assoc関数。新規 `RtValue::HashTable`（GC統合が必要、Vector同様の規模の実装作業） |
| 取得 | `(get h k)` | **Rust** | インスタンスメソッド。戻り値 `Option<V>` |
| 設定 | `(set h k v)` | **Rust** | |
| 削除 | `(remove h k)` | **Rust** | |
| 件数 | `(count h)` | **Rust** | |
| 全削除 | `(clear h)` | **Rust** | |
| 一覧化 | `(to-list h)` | **Rust** | `(K . V)` の cons リスト（`Sexpr`）を返す最小限プリミティブ |
| キー/値一覧 | `(keys h)` / `(values h)` | **TypeLisp**（`defmethod`） | `(to-list h)` の結果に `map` を適用するだけで書ける |

#### b. ベクタ操作拡張【実装済み、`src/check/registry.rs`の`vector_def`】

`Vector<T>` も同様に組み込み nominal型として登録し、メソッドAPIとする。実装は`RtValue::Vector`
という専用バリアントではなく`RtValue::Struct`（`StructData.fields`を可変長コレクションとして
扱う）で行われた——詳細は[language-design.md](language-design.md) §8参照。

| 項目 | 呼び出し形 | 分類 | 備考 |
|---|---|---|---|
| 構築 | `(Vector::new n init)` | **Rust** | 新規 `RtValue::Vector`（GC統合が必要） |
| 取得 | `(get v i)` | **Rust** | 範囲外は panic |
| 設定 | `(set v i x)` | **Rust** | |
| 長さ | `(length v)` | **Rust** | |
| 末尾追加 | `(push v x)` | **Rust** | |
| 末尾削除 | `(pop v)` | **Rust** | 戻り値 `Option<T>` |
| 変換 | `(to-list v)` / `Vector::from-list` | **Rust** | |
| map/filter | `(map v f)` / `(filter v f)` | **TypeLisp**（`defmethod`） | 上記プリミティブの組合せで実装可能 |

> 名前（`get`/`set`/`length`/`count`等）はインスタンスメソッドとして**型ごとの assoc テーブル**に
> 登録されるため、`Vector` と `HashTable` で同名メソッドを使っても衝突しない
> （[src/check/checker.rs:1001-1003](../../crates/typelisp-front/src/check/checker.rs) で型ごとに別のテーブルを引く）。
> ただし**同名の自由関数（`defun`）が既に存在する場合は常にそちらが優先され、インスタンスメソッドへ
> フォールバックしない**（[src/check/checker.rs:875-886](../../crates/typelisp-front/src/check/checker.rs)）。既存の
> `length`/`map`/`filter` 等（Sexpr/リスト向け、§4.2想定）は自由関数のままとする方針なので、本書の
> 範囲では問題にならないが、実装時はこの優先順位を踏まえて命名する。

#### c. ソート

| 関数 | 分類 | 備考 |
|---|---|---|
| `sort`（リスト用） | **TypeLisp** | 比較関数を受け取るマージソート等を `cons`/`car`/`cdr`/`match` で実装可能 |
| `sort`（ベクタ用、破壊的） | **TypeLisp**（`defmethod`） | `(sort v cmp)`。`get`/`set` を使ったクイックソート等で実装可能（プリミティブはRust、アルゴリズムはTypeLisp）。リスト用の自由関数 `sort` と同名だが、自由関数優先のため `Vector` 側からは到達不能になる懸念がある — 実装時に名前を分けるか、リスト側も `defmethod` 化するか要決定（前項のメソッド名衝突の注記と同じ問題） |

#### d. 文字・文字列操作拡張

§0.1 の拡張（プリミティブ型への `defmethod` 対応）を前提に、CLの `(string-upcase s)` のような
ハイフン付き自由関数ではなく、`(upcase s)` のようなインスタンスメソッドとして定義する。

| 項目 | 呼び出し形 | 分類 | 備考 |
|---|---|---|---|
| 大文字化 | `(upcase s)`（`s: Str` または `s: char` で型ごとに別定義） | **Rust** | 文字コード操作はプリミティブ。`Str` と `Char` それぞれの assoc テーブルに同名 `upcase` を登録できる（§0.1拡張後） |
| 小文字化 | `(downcase s)` | **Rust** | 同上 |
| 長さ | `(length s)`（`s: Str`） | **Rust** | |
| 部分文字列 | `(substring s start end)` | **Rust** | |
| 1文字取得 | `(ref s i)` | **Rust** | |
| 連結 | `(append s1 s2)`（`s: Str`） | **Rust** | |
| 比較 | `(eq s1 s2)` | **Rust** | §0.1拡張で `Str` にも `eq` を定義（§1.1 `case` の前提と共通） |
| 大小比較 | `(lt s1 s2)` 等 | **Rust** | |
| trim/split | `(trim s)` / `(split s sep)` | **TypeLisp**（`defmethod`） | `ref`/`substring` の組合せで実装可能 |
| 文字判定 | `(alphap c)` / `(digitp c)`（`c: char`） | **Rust** | 文字コード操作はプリミティブ |
| 文字列⇔リスト変換 | `(to-list s)` / `Str::from-list` | **Rust** | 内部表現の変換は直接アクセスが要る |

#### e. シーケンス操作拡張（リスト、`Sexpr` 向けの自由関数のまま）

| 関数 | 分類 | 備考 |
|---|---|---|
| `remove` / `remove-if` / `remove-if-not` / `count` / `count-if` / `position` / `position-if` / `copy-list` / `nthcdr` / `butlast` / `elt` / `subseq` | **TypeLisp** | すべて既存の `cons`/`car`/`cdr`/`match`/`length`/`append` の組合せで実装可能 |
| `nconc` / `nreverse`（破壊的） | **TypeLisp** | `set-car`/`set-cdr` の組合せで実装可能（`set-car`/`set-cdr` 自体はRust）。**追記（2026-07-19）**: 提案通り一度実装されたが（[implementation-log.md](implementation-log.md)参照）、`symbol-sexpr-redesign.md` Phase 5 で cons チェーン専用APIとして削除され、非破壊 generic `reverse`（Phase 4a）に一本化された。意図的な設計変更であり未実装ではない |

#### f. 数値拡張

| 関数 | 分類 | 備考 |
|---|---|---|
| i64 / f64 版の算術・比較演算子一式 | **Rust** | 既存 i32 版と同様の追加実装 |
| `expt` / `sqrt` / `floor` / `ceiling` / `round` / `truncate` | **Rust** | 浮動小数演算はネイティブ命令が必要 |
| `gcd` / `lcm` | **TypeLisp** | `mod` を使ったユークリッドの互除法で実装可能 |
| `signum` | **TypeLisp** | 比較演算の組合せで実装可能 |
| `random` | **Rust**（実装済み、`random-state` 含む） | ビット演算を要する xorshift ステップ自体は Rust（`typelisp_rt::xorshift64_step`）、`random`/`make-random-state`/`random-state-p` は `&optional` を使った prelude の `defun`。`*random-state*` は動的束縛ではなく通常の再代入可能グローバル |

#### g. `apply` / `&rest` — 2026-07-15 実装完了

値レベルの `&rest`（`Type::Fn` の第2フィールド、`FnSig.rest`）と `(apply f a1..aN rest-list)` 特殊形を
再導入した。固定引数は静的型検査、rest-list は `Sexpr` 型として渡し、呼び出し側は各 rest 要素を
`wrap_rest_elem`/`cons_rest_list` で単一 `Sexpr` リストへ畳んで実引数化する。詳細は
[implementation-log.md](implementation-log.md) を参照。`compile`（LLVM）側は `&rest` 付き関数の
compile 自体は非対応のまま（`unsupported` として明示的にテスト固定）。

## 3. 対象外（今回はユーザの判断で記載のみ）

- 多値 (`values` / `multiple-value-bind`)
- `catch` / `throw`
- コンディションシステム (`handler-case` / `define-condition` / `restart`)

これらは [language-design.md](language-design.md) §7 の方針（`Result`+`panic` のみ、trait/CLOS 無し）
と衝突するため、本カタログでは対象外とする。

## 4. 実装順序の提案（参考）

1. **§0.1: プリミティブ型への `defmethod` 拡張**（i32/char/bool/Str対応）— char/string メソッドと
   型ごとの `eq` の両方の前提になるため最優先
2. ハッシュテーブル（新規nominal型、最も影響範囲が大きい）
3. ベクタ操作の基本プリミティブ（新規nominal型）
4. 文字・文字列操作の基本プリミティブ（1.の拡張を利用）
5. 数値拡張（i64/f64、四則・比較）
6. TypeLisp側ライブラリ関数一式（リスト操作拡張、Option/Result補助、ソート、文字列補助）を `defun`/`defmacro` で実装
7. `case`/`until`/`while-let`/`do` を `defmacro` で実装（`case` は型ごとの `eq` が前提）。`doiter` は
   trait機構（`deftrait`/`impl`）の導入後、2026-06-30に別途実装（§1.1参照）
8. `labels`（ローカル再帰関数）の特殊形追加
9. `apply`（`&rest` 設計と合わせて）

## 5. 訂正（2026-07-01）: `eq`/`eql`/`equal`/`equalp`の再設計

§2.1・§2.2dで「`eq`を型ごとに再定義し、`case`が型を問わず`(eq a b)`で書けるようにする」とした
当初の設計は、`Str`について実際にはCLの`eq`（同一性）ではなく**内容比較**を行っており、
CL仕様上は`equal`の役割だった。CLの`eq`/`eql`/`equal`/`equalp`の正確な違いは:

- `eq`: 同一性（同じオブジェクトか）。
- `eql`: `eq`に加え、同じ型・同じ値の数値/文字を真にする。**文字列の内容比較はしない**
  （`eq`と同じく同一性のまま）。
- `equal`: `eql`に加え、`cons`の再帰比較・文字列の内容比較（大小区別あり）。
- `equalp`: `equal`に加え、文字列は大小区別なし・数値は型を跨いで値比較・配列/構造体は要素ごと
  `equalp`。

この訂正に伴い、4つとも正しく再実装した（2026-07-01、`RtValue::Str`を`Rc<str>`化——後述):

| 型 | `eq`/`eql` | `equal` | `equalp` |
|---|---|---|---|
| `Str` | 真の同一性（`Rc::ptr_eq`） | 内容比較（大小区別あり） | 内容比較（大小区別なし） |
| `char` | 値比較（immediate） | `eq`と同じ | 大小区別なし |
| `i32`/`i64`/`f64`/`bool` | 値比較 | `eq`と同じ | `eq`と同じ（型跨ぎ数値比較は静的型の時点で到達不能） |
| `Sexpr` | 同一性（既存のまま） | `prelude.rs`の再帰構造比較（`Str`は内容比較を呼ぶ） | 同上+大小無視+`Cons`/`Char`再帰+`Int`⇔`Float`型跨ぎ比較（`int->float`変換プリミティブ経由、2026-07-01実装） |

**`RtValue::Str`を`String`から`Rc<str>`に変更**（`src/eval/value.rs`）: プレーンな`string`型の値は
変数を読むたびに`clone`されるため、`String`のままでは「同じオブジェクト」という概念自体が存在せず
（`(let ((s "hi")) (eq s s))`すら成立しない）、真の`eq`を実装できなかった。`Rc<str>`の`clone`は
ポインタ複製（refcountインクリメント）なので、同じ束縛を2回読んでも同一オブジェクトのまま——
`Rc::ptr_eq`で正しい同一性判定ができるようになった。

**`case`マクロは`eq`ではなく`equal`を使う**（`src/prelude.rs`）: ANSI CLの`case`は`eql`基準であり
文字列キーはほぼ一致しない（`eql`は文字列の内容比較をしないため）。本処理系では文字列キーが内容で
一致してほしいという当初の設計意図を保つため、意図的に`equal`基準にした——CLの`case`そのものとは
異なる、明示的な拡張。

**`compiler.rs`（自己ホスト型コンパイラ）の全面修正**: `compile-value`等のタグ/メソッド名/型名
ディスパッチが`(eq s "int")`のような**文字列内容比較としての`eq`**に大きく依存していたため、
`eq`の意味変更で48箇所が全て壊れた（`compile-value: unsupported tag int`等）。該当箇所は全て
`equal`に置き換えて修正済み。
