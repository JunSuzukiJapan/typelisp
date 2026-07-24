# typelisp 関数・メソッド一覧

呼び出し形式は3種類ある。

- 自由関数: `(name args...)`
- インスタンスメソッド: `(name receiver args...)`（第一引数の静的型から解決）
- static / 関連関数: `(Type::name args...)`

構文（特殊形・定義方法）は [syntax.md](syntax.md) を参照。

## 1. 算術・比較（`i32` / `i64`）

第一引数の型で `i32` 用と `i64` 用のどちらに解決されるかが決まる（両者は独立で、暗黙変換はない）。

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `+` `-` `*` `/` | `(op a b)` | `(T,T)→T` | 四則演算。`/` はゼロ方向切り捨て・ゼロ除算で panic |
| `mod` | `(mod a b)` | `(T,T)→T` | 剰余（CL の `mod`、**床除算**＝符号は除数側。`(mod -7 3)`→`2`）。ゼロ除算で panic |
| `rem` | `(rem a b)` | `(T,T)→T` | 剰余（CL の `rem`、**切り捨て除算**＝符号は被除数側。`(rem -7 3)`→`-1`）。ゼロ除算で panic |
| `abs` | `(abs x)` | `T→T` | 絶対値（`prelude.rs` のメソッド） |
| `signum` | `(signum x)` | `T→T` | 符号（`1`/`-1`/`0`） |
| `gcd` | `(gcd a b)` | `(T,T)→T` | 最大公約数 |
| `lcm` | `(lcm a b)` | `(T,T)→T` | 最小公倍数（どちらかが0なら0） |
| `<` `<=` `>` `>=` `=` `/=` | `(op a b)` | `(T,T)→bool` | 比較 |
| `eq` `eql` `equal` `equalp` | `(op a b)` | `(T,T)→bool` | いずれも `=` と同じ（同型の数値に差はない） |
| `int->float` | `(int->float x)` | `T→f64` | `f64` への拡大変換 |
| `int->bignum` | `(int->bignum x)` | `T→bignum` | `bignum` への拡大変換（常に正確） |
| `int->ratio` | `(int->ratio x)` | `T→ratio` | `ratio` への拡大変換（常に正確） |
| `int->char` | `(int->char x)` | `T→char` | Unicode スカラ値として解釈。不正な値は panic |
| `try-int->char` | `(try-int->char x)` | `T→Option<char>` | `int->char` の失敗を `None` で返す版 |

これらの変換は `(as Type x)`/`(try-as Type x)` 特殊形（[syntax.md](syntax.md) 参照）の実体でもある。

`i8` `i16` `isize` `u8` `u16` `u32` `u64` `usize` `f32` は `defmethod` の受け手として型登録は
されているが、現時点では算術・比較を含め一切のメソッドを持たない。

## 2. 算術・比較（`f64`）

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `+` `-` `*` `/` | `(op a b)` | `(f64,f64)→f64` | IEEE-754。ゼロ除算は panic せず `inf`/`NaN` |
| `mod` | `(mod a b)` | `(f64,f64)→f64` | 床除算の剰余（CL 準拠、符号は除数側。`a - b*floor(a/b)`） |
| `rem` | `(rem a b)` | `(f64,f64)→f64` | 切り捨て除算の剰余（CL 準拠、符号は被除数側。`a - b*truncate(a/b)`） |
| `<` `<=` `>` `>=` `=` `/=` | `(op a b)` | `(f64,f64)→bool` | 比較 |
| `eq` `eql` `equal` `equalp` | `(op a b)` | `(f64,f64)→bool` | いずれも `=` と同じ |
| `expt` | `(expt a b)` | `(f64,f64)→f64` | 冪乗 |
| `abs` | `(abs x)` | `f64→f64` | 絶対値 |
| `signum` | `(signum x)` | `f64→f64` | 符号（`1.0`/`-1.0`、`±0.0`/`NaN` はそのまま。CL 準拠で Rust の `signum` とは異なる） |
| `sqrt` `floor` `ceiling` `round` `truncate` | `(op x)` | `f64→f64` | 単項演算 |
| `float->int` | `(float->int x)` | `f64→i32` | ゼロ方向への切り捨てで `i32` へ変換 |
| `float->bignum` | `(float->bignum x)` | `f64→bignum` | ゼロ方向への切り捨てで `bignum` へ変換 |
| `float->ratio` | `(float->ratio x)` | `f64→ratio` | 正確な二進有理数として `ratio` へ変換（CL の `rational`） |

## 2.5 多倍長数値（`bignum` / `ratio`）

CL 準拠の任意精度数値型。`bignum` は多倍長整数、`ratio` は常に既約・正の分母で保たれる有理数。
どちらもヒープ確保され、`i32`/`i64`/`f64` との暗黙変換はない（明示的な変換メソッドまたは
`as`/`try-as` を使う）。整数/比リテラル構文は [syntax.md](syntax.md) を参照。

**`bignum`**（CL の整数と同じ演算集合。`/` はゼロ方向切り捨て、除算・剰余系はゼロ除算で panic）:

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `+` `-` `*` `/` | `(op a b)` | `(bignum,bignum)→bignum` | 四則（`/` は切り捨て） |
| `mod` | `(mod a b)` | `(bignum,bignum)→bignum` | 床除算の剰余（符号は除数側。§1 の `mod` と同じ） |
| `rem` | `(rem a b)` | `(bignum,bignum)→bignum` | 切り捨て除算の剰余（符号は被除数側。§1 の `rem` と同じ） |
| `abs` | `(abs x)` | `bignum→bignum` | 絶対値 |
| `signum` | `(signum x)` | `bignum→bignum` | 符号（`1`/`-1`/`0`） |
| `gcd` | `(gcd a b)` | `(bignum,bignum)→bignum` | 最大公約数 |
| `lcm` | `(lcm a b)` | `(bignum,bignum)→bignum` | 最小公倍数（どちらかが0なら0） |
| `expt` | `(expt a b)` | `(bignum,bignum)→bignum` | 冪乗。指数が負なら panic（結果が `ratio` になり `bignum` で表せないため） |
| `<` `<=` `>` `>=` `=` `/=` | `(op a b)` | `(bignum,bignum)→bool` | 比較 |
| `eq` `eql` `equal` `equalp` | `(op a b)` | `(bignum,bignum)→bool` | いずれも `=` と同じ |
| `bignum->int` | `(bignum->int x)` | `bignum→i32` | 縮小変換。`i64` に収まらなければ panic |
| `try-bignum->int` | `(try-bignum->int x)` | `bignum→Option<i32>` | 収まらなければ `None` |
| `bignum->float` | `(bignum->float x)` | `bignum→f64` | `f64` へ変換 |
| `bignum->ratio` | `(bignum->ratio x)` | `bignum→ratio` | `ratio` への拡大変換（正確） |
| `print` `println` | `(op x)` | `bignum→Unit` | 標準出力へ書く（§15） |

**`ratio`**（`/` はゼロ除算で panic）:

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `+` `-` `*` `/` | `(op a b)` | `(ratio,ratio)→ratio` | 四則（結果は常に既約） |
| `mod` | `(mod a b)` | `(ratio,ratio)→ratio` | 床除算の剰余（CL 準拠、符号は除数側） |
| `rem` | `(rem a b)` | `(ratio,ratio)→ratio` | 切り捨て除算の剰余（CL 準拠、符号は被除数側） |
| `abs` | `(abs x)` | `ratio→ratio` | 絶対値 |
| `signum` | `(signum x)` | `ratio→ratio` | 符号（`1`/`-1`/`0` を `ratio` で返す） |
| `expt` | `(expt a b)` | `(ratio,ratio)→ratio` | 冪乗。指数は整数値の `ratio` のみ（非整数なら panic）。負指数は逆数 |
| `<` `<=` `>` `>=` `=` `/=` | `(op a b)` | `(ratio,ratio)→bool` | 比較 |
| `eq` `eql` `equal` `equalp` | `(op a b)` | `(ratio,ratio)→bool` | いずれも `=` と同じ |
| `numerator` | `(numerator x)` | `ratio→bignum` | 既約分子（CL と同名） |
| `denominator` | `(denominator x)` | `ratio→bignum` | 既約分母（常に正） |
| `ratio->bignum` | `(ratio->bignum x)` | `ratio→bignum` | 整数部（ゼロ方向切り捨て） |
| `ratio->float` | `(ratio->float x)` | `ratio→f64` | `f64` へ変換 |
| `print` `println` | `(op x)` | `ratio→Unit` | 標準出力へ書く（§15） |

`i32`/`i64`/`f64` からの入口は `int->bignum`/`int->ratio`（§1）と `float->bignum`/`float->ratio`
（§2）。`bignum`/`ratio` は `i32` 等とは独立した別型で、混在した算術には明示変換が必要。

> **実装と compile 対応**: `abs`/`signum`/`rem`/`gcd`/`lcm`/`expt`（および `f64`/`ratio` の
> `mod`）は Rust ビルトインではなく `prelude.rs` の**typelisp メソッド**として各型のプリミティブ演算
> （`/`・`mod`・`floor`/`truncate`・`ratio->bignum` 等）から組み立てられている。したがって JIT/AOT
> でも通常の関数として**コンパイル可能**で、インタプリタと結果が一致する（専用の `rt_*` シムや
> compiler.rs 分岐は不要）。整数 `mod` と各型の四則・比較・変換はネイティブ命令へ直接ローワリングされる。

## 3. 論理・真偽値

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `not` | `(not b)` | `bool→bool` | 否定 |
| `eq` `eql` `equal` `equalp` | `(op a b)` | `(bool,bool)→bool` | すべて値の等価比較 |

`and`/`or` は短絡評価が必要なため特殊形（[syntax.md](syntax.md) 参照）。

## 4. 数値ヘルパー

`abs`/`signum`（全数値型）・`gcd`/`lcm`（整数型のみ）・`rem`（`f64` 含む全実数型）・`expt`
（`bignum`/`f64`/`ratio`）は、各数値型のメソッドとして `prelude.rs` に typelisp で定義されている
（レシーバ型で解決。`(abs x)` は `x` の型に応じたメソッド）。詳細と型は各型の節（§1・§2・§2.5）を
参照。整数の `expt` は無い（`bignum` 昇格が無くオーバーフローするため、`(int->bignum x)` 経由で
`bignum` の `expt` を使う）。

（`ratio-expt-int` も `prelude.rs` に `defun` として存在するが、これは `ratio` の `expt` が整数乗を
計算するための内部ヘルパーであり、通常は `(expt r n)` を使う。）

`random` のみレシーバを持たない自由関数として残る。

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `random` | `(random n)` | `i32→i32` | `0` 以上 `n` 未満の乱数（自由関数、レシーバなし） |

## 5. `cons`/`car`/`cdr`（ジェネリックなペア）と `Sexpr`

Symbol/Sexpr 再設計 Phase 4b 以降、`cons`/`car`/`cdr` は `Sexpr` 専用ではなく**ジェネリックな
ペア型 `cons-cell<A,B>`**（`defstruct`、`prelude.rs`）のコンストラクタ/フィールドアクセサに
付け替えられている。フィールドは `変数::car`/`変数::cdr`（[syntax.md](syntax.md) の `defstruct`
アクセサ構文）でも `(car 変数)`/`(cdr 変数)`（インスタンスメソッド呼び出し）でも読める。
**`set-car`/`set-cdr`（破壊的更新）は完全に撤去済み**——`cons-cell` のフィールドを書き換えるには
`(setf 変数::car v)`/`(setf 変数::cdr v)` を使う。

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `cons` | `(cons a b)` | `(A,B)→cons-cell<A,B>` | ペアを作る（自由関数、`prelude.rs`） |
| `car` | `(car p)` | `cons-cell<A,B>→A` | 先頭（`defstruct` フィールドアクセサ、インスタンスメソッドとして呼べる） |
| `cdr` | `(cdr p)` | `cons-cell<A,B>→B` | 残り（同上） |

`read` が返すデータ型 `Sexpr`（`Nil | Int | Float | Char | Bool | Sym | Str | Cons(Sexpr,Sexpr)`）
自体のセル操作は、上記の汎用 `cons`/`car`/`cdr` とは別の内部 island 層 `sexpr-*` が担う
（`read`/`eval`/`print`/`defmacro`/自己ホストコンパイラ `compiler.rs` の内部でのみ使われ、
ユーザー向けライブラリ関数からは `sexpr-*` を直接呼ぶ場面はほぼ無い）。

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `sexpr-cons` | `(sexpr-cons a b)` | `(Sexpr,Sexpr)→Sexpr` | `Sexpr` セルを作る |
| `sexpr-car` | `(sexpr-car s)` | `Sexpr→Sexpr` | 先頭。`Cons` でなければ panic |
| `sexpr-cdr` | `(sexpr-cdr s)` | `Sexpr→Sexpr` | 残り。`Cons` でなければ panic |
| `sexpr-consp` | `(sexpr-consp s)` | `Sexpr→bool` | `Cons` かどうか |
| `sexpr-null` | `(sexpr-null s)` | `Sexpr→bool` | `Nil` かどうか |
| `sexpr-atom` | `(sexpr-atom s)` | `Sexpr→bool` | `Cons` でないか |
| `sexpr-symp` | `(sexpr-symp s)` | `Sexpr→bool` | `Sym`（シンボル）かどうか |
| `sexpr-int` | `(sexpr-int s)` | `Sexpr→i64` | `Int` の中身を取り出す。`Int` でなければ panic |
| `sexpr-float` | `(sexpr-float s)` | `Sexpr→f64` | `Float` の中身。型違いは panic |
| `sexpr-char` | `(sexpr-char s)` | `Sexpr→char` | `Char` の中身。型違いは panic |
| `sexpr-bool` | `(sexpr-bool s)` | `Sexpr→bool` | `Bool` の中身。型違いは panic |
| `sexpr-str` | `(sexpr-str s)` | `Sexpr→string` | `Str` の中身。型違いは panic |
| `sexpr-sym-name` | `(sexpr-sym-name s)` | `Sexpr→string` | `Sym` の名前。型違いは panic |
| `eq` `eql` | `(op a b)` | `(Sexpr,Sexpr)→bool` | 同一性比較（`Cons`/`Str` はポインタ、それ以外は値） |

上の `sexpr-*` アクセサは Rust 組み込み。これらの上に、`Sexpr` リスト全体を扱う次の2つが
`prelude.rs` に typelisp の `defun` として定義されている（`defmacro` の本体で引数の `Sexpr`
フォーム列を組み立て・変換するマクロ作者向け。`,@`（unquote-splicing）は内部で `sexpr-append`
へ展開される）:

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `sexpr-append` | `(sexpr-append a b)` | `(Sexpr,Sexpr)→Sexpr` | 2つの `Sexpr` リストを連結（非破壊） |
| `sexpr-map` | `(sexpr-map f lst)` | `((fn (Sexpr) Sexpr),Sexpr)→Sexpr` | `Sexpr` リストの各要素へ `f` を適用した新しい `Sexpr` リスト（`map`（§6）は `Iter` 用でマクロ引数リストは回せないため、その代替） |

## 6. シーケンス操作（`Iter` 上のライブラリ関数）

Phase 6.5 の再設計で、旧来の `Sexpr` リスト用ライブラリは **`Iter` トレイト上のジェネリック
関数**へ作り直された。呼び出しはコレクションから `(iter coll)` でカーソルを得て渡す
（`Vector<T>` / `HashTable<K,V>` が `Iter` を実装。`Sexpr` のリストは `Iter` を実装しない
ので、これらの関数の対象にはならない）。旧 API との主な違いは、**コレクション/イテレータを
第一引数に取る**点と、**結果のコレクションは新しい `Vector` として返る**点。表中の `Iter<A>` は
「`Item` が `A` の任意の `Iter` 実装型」を表す。

`symbol->string` / `string->symbol` は独立したプリミティブ型 `Symbol`（`Sexpr` の `Sym` 構成子
とは別物——`Sexpr` が要求される文脈へは暗黙変換されるが逆方向の自動変換はない）と `string` の
橋渡し:

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `symbol->string` | `(symbol->string s)` | `Symbol→string` | シンボル名を取り出す |
| `string->symbol` | `(string->symbol s)` | `string→Symbol` | 文字列からシンボルを作る（intern） |
| `equal` | `(equal a b)` | `(Sexpr,Sexpr)→bool` | 構造的等価（`Cons` は再帰、`Str` は内容比較） |
| `equalp` | `(equalp a b)` | `(Sexpr,Sexpr)→bool` | `equal` に加え大文字小文字無視・数値の型跨ぎ比較 |

述語を取る関数（CL の `-if` 系に対応。すべて `where (Iter I (Item A))`）:

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `map` | `(map it f)` | `(Iter<A>,(fn (A) U))→Vector<U>` | 写像 |
| `filter` | `(filter it pred)` | `(Iter<A>,(fn (A) bool))→Vector<A>` | 条件を満たす要素のみ |
| `remove-if` | `(remove-if it pred)` | `(Iter<A>,(fn (A) bool))→Vector<A>` | 条件を満たす要素を除く |
| `find` | `(find it pred)` | `(Iter<A>,(fn (A) bool))→Option<A>` | 条件を満たす最初の要素 |
| `position` | `(position it pred)` | `(Iter<A>,(fn (A) bool))→Option<i32>` | 条件を満たす最初の位置 |
| `count` | `(count it pred)` | `(Iter<A>,(fn (A) bool))→i32` | 条件を満たす個数 |
| `every` | `(every it pred)` | `(Iter<A>,(fn (A) bool))→bool` | 全要素が条件を満たすか |
| `any` | `(any it pred)` | `(Iter<A>,(fn (A) bool))→bool` | いずれかが条件を満たすか（CL の `some` 相当、`Some` 構成子との衝突回避名） |
| `foldl` | `(foldl it f init)` | `(Iter<A>,(fn (B A) B),B)→B` | 左畳み込み |
| `foldr` | `(foldr it f init)` | `(Iter<A>,(fn (A B) B),B)→B` | 右畳み込み |

添字・長さ・スライス（すべて `where (Iter I (Item A))`）:

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `length` | `(length it)` | `Iter<A>→i32` | 要素数 |
| `append` | `(append a b)` | `(Iter<A>,Iter<A>)→Vector<A>` | 2つのイテレータを連結 |
| `reverse` | `(reverse it)` | `Iter<A>→Vector<A>` | 反転（非破壊） |
| `nth` | `(nth n it)` | `(i32,Iter<A>)→Option<A>` | `n` 番目の要素（範囲外は `None`） |
| `elt` | `(elt it n)` | `(Iter<A>,i32)→Option<A>` | `nth` の引数順違い版 |
| `take` | `(take it n)` | `(Iter<A>,i32)→Vector<A>` | 先頭 `n` 個 |
| `subseq` | `(subseq it start end)` | `(Iter<A>,i32,i32)→Vector<A>` | `[start,end)`（`end` は長さでクランプ） |
| `last` | `(last it)` | `Iter<A>→Option<A>` | 最後の**要素**（CL の「最後のセル」ではない） |
| `butlast` | `(butlast it)` | `Iter<A>→Vector<A>` | 最後の要素を除く |

`Eq` / `Ord` 境界を要求する関数（述語の代わりにトレイトで比較。§12.1 参照）:

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `member` | `(member x it)` | `(A,Iter<A>)→bool` where `Eq A` | `x` と等しい要素があるか（CL と違い残りリストではなく `bool`） |
| `sort` | `(sort it)` | `Iter<A>→Vector<A>` where `Ord A` | 昇順の挿入ソート（安定・非破壊） |
| `assoc` | `(assoc k it)` | `(K,Iter<cons-cell<K,V>>)→Option<cons-cell<K,V>>` where `Eq K` | `car` が `k` と等しい最初のペア。値は `(cdr p)` で取り出す |

`(list e1 e2 ... en)` は特殊形（`(cons e1 (cons e2 (... (Nil))))` へ展開、[syntax.md](syntax.md) 参照）。
`map` / `filter` 等が返す `Vector<T>` を再び回すには `(iter result)` を渡す。

> **旧 API から削除された関数**（`docs/dev/symbol-sexpr-redesign.md` Phase 5 / 6.5）:
> `consp` `null` `atom`（`Sexpr` 述語）、`nthcdr` `copy-list`（cons チェーン専用）、
> `nconc` `nreverse`（破壊的操作）、`find-if` `count-if` `position-if` `remove-if-not`
> （`find` / `count` / `position` / `filter` で代替）。`remove`（要素削除）は現在
> `HashTable<K,V>` のメソッドとしてのみ存在（§11）。

## 7. `Option<T>` / `Result<T,E>`

構成子: `Option<T>` は `Some(T)` / `None`。`Result<T,E>` は `Ok(T)` / `Err(E)`。
`Error` は組み込みの汎用エラー型（`Result` の既定の `E`）。

| 名前 | 形式 | Option | Result | 説明 |
|---|---|---|---|---|
| `unwrap` | `(unwrap self)` | `Option<T>→T` | `Result<T,E>→T` | 値を取り出す。`None`/`Err` なら panic |
| `unwrap-or` | `(unwrap-or self default)` | `(Option<T>,T)→T` | `(Result<T,E>,T)→T` | 値、または既定値 |
| `is-some` | `(is-some self)` | `Option<T>→bool` | — | `Some` か |
| `is-none` | `(is-none self)` | `Option<T>→bool` | — | `None` か |
| `is-ok` | `(is-ok self)` | — | `Result<T,E>→bool` | `Ok` か |
| `is-err` | `(is-err self)` | — | `Result<T,E>→bool` | `Err` か |

構成子は `Option::some`/`Option::none`/`Result::ok`/`Result::err`（または `(use option)`/
`(use result)` で裸名 `some`/`none`/`ok`/`err` も使用可能）。

## 8. 文字列 (`string`)

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `upcase` | `(upcase s)` | `string→string` | 大文字化（ASCII のみ） |
| `downcase` | `(downcase s)` | `string→string` | 小文字化（ASCII のみ） |
| `length` | `(length s)` | `string→i32` | 文字数 |
| `ref` | `(ref s i)` | `(string,i32)→char` | `i` 番目の文字。範囲外は panic |
| `substring` | `(substring s start end)` | `(string,i32,i32)→string` | 部分文字列 `[start,end)` |
| `append` | `(append s1 s2)` | `(string,string)→string` | 連結 |
| `<` `<=` `>` `>=` | `(op s1 s2)` | `(string,string)→bool` | 辞書順比較（`i32` 等と同じくレシーバ型で多重定義） |
| `lt` | `(lt s1 s2)` | `(string,string)→bool` | 辞書順の狭義小なり（`<` の旧 CL カタログ名） |
| `eq` `eql` | `(op s1 s2)` | `(string,string)→bool` | 同一性比較（内容ではなく参照） |
| `equal` | `(equal s1 s2)` | `(string,string)→bool` | 内容比較（大文字小文字を区別） |
| `equalp` | `(equalp s1 s2)` | `(string,string)→bool` | 内容比較（大文字小文字を無視、ASCII のみ） |

## 9. 文字 (`char`)

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `upcase` | `(upcase c)` | `char→char` | 大文字化（ASCII のみ） |
| `downcase` | `(downcase c)` | `char→char` | 小文字化（ASCII のみ） |
| `<` `<=` `>` `>=` | `(op c1 c2)` | `(char,char)→bool` | コードポイント順比較（レシーバ型で多重定義） |
| `lt` | `(lt c1 c2)` | `(char,char)→bool` | コードポイント順の狭義小なり（`<` の旧 CL カタログ名） |
| `alphap` | `(alphap c)` | `char→bool` | ASCII アルファベットか |
| `digitp` | `(digitp c)` | `char→bool` | ASCII 数字か |
| `eq` `eql` `equal` | `(op c1 c2)` | `(char,char)→bool` | 値の比較 |
| `equalp` | `(equalp c1 c2)` | `(char,char)→bool` | 大文字小文字を無視した値の比較 |
| `char->int` | `(char->int c)` | `char→i32` | Unicode スカラ値 |

## 10. `Vector<T>`

可変長配列。`RtValue::Struct` を流用した組み込み型。

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `new` | `(Vector::new)` | `()→Vector<T>` | 空のベクタを作る（static） |
| `push` | `(push v x)` | `(Vector<T>,T)→Unit` | 末尾に追加 |
| `get` | `(get v i)` | `(Vector<T>,i32)→T` | `i` 番目を読む。範囲外は panic |
| `set` | `(set v i x)` | `(Vector<T>,i32,T)→Unit` | `i` 番目を書き換える。範囲外は panic |
| `len` | `(len v)` | `Vector<T>→i32` | 要素数 |
| `pop` | `(pop v)` | `Vector<T>→Option<T>` | 末尾を取り除いて返す。空なら `None`（`get`/`set` と異なり範囲外でも panic しない） |
| `iter` | `(iter v)` | `Vector<T>→vector-iter<T>` | `Iter` を実装するカーソルを作る（ライブラリ定義） |

`map`/`filter` は§6（`Iter` トレイト上のジェネリック関数）で実装済み——`(map (iter v) f)` のように
`Vector<T>` を `iter` でカーソル化して渡す。

**`Vector<T>` ↔ `Sexpr` リストの相互変換は言語仕様上できない（意図的に対象外）**: `cons` は
`cons<T,U>` という異種ペア型であり、`(cons 1 "hello")` の型は `cons<i32, cons<str, null>>` になる
——1つめの要素の型は `i32`、2つめの要素の型は `cons<str, null>` で異なる。`Sexpr` のリストは
この異種の入れ子 `cons` 連鎖（各要素ごとに型が変わりうる）であり、`Vector<T>` のような単一の要素型
`T` だけからなるコレクションとは表現が根本的に異なるため、両者を汎用的に変換する
`to-list`/`from-list` のような関数は書けない（型パラメータ`T`だけでは`Sexpr`側の入れ子構造を
静的に表現できない）。

## 11. `HashTable<K,V>`

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `new` | `(HashTable::new)` | `()→HashTable<K,V>` | 空のテーブルを作る（static） |
| `get` | `(get h k)` | `(HashTable<K,V>,K)→Option<V>` | 検索 |
| `set` | `(set h k v)` | `(HashTable<K,V>,K,V)→Unit` | 挿入・上書き |
| `remove` | `(remove h k)` | `(HashTable<K,V>,K)→Option<V>` | 削除し、あれば旧値を返す |
| `count` | `(count h)` | `HashTable<K,V>→i32` | 要素数 |
| `clear` | `(clear h)` | `HashTable<K,V>→Unit` | 全削除 |
| `keys` | `(keys h)` | `HashTable<K,V>→Vector<K>` | キーのスナップショット |
| `values` | `(values h)` | `HashTable<K,V>→Vector<V>` | 値のスナップショット |
| `entries` | `(entries h)` | `HashTable<K,V>→Vector<cons-cell<K,V>>` | `(k . v)` ペアのスナップショット |
| `iter` | `(iter h)` | `HashTable<K,V>→hashtable-iter<K,V>` | `Iter` を実装するカーソル（ライブラリ定義） |

## 12. `Iter` トレイトと反復

```lisp
(deftrait Iter
  (type Item)
  (next ((self Self)) Option<Item>))
```

`Vector<T>`/`HashTable<K,V>` はそれぞれ `vector-iter<T>`/`hashtable-iter<K,V>` を介して `Iter` を
実装している（`(iter コレクション)` でカーソルを取得）。`Sexpr` のリストには意図的に `Iter` を
実装していない（要素型が一様でないため）。ユーザ定義の `deftrait`/`impl` で独自のコレクションに
`Iter` を実装すれば、そのまま `doiter` で回せる（[syntax.md](syntax.md) 参照）。

`cons-cell<A,B>` は `(car cell)`/`(cdr cell)` を持つ汎用の2要素組（タプル構文の代わり）。

## 12.1 `Eq` / `Ord` トレイト（比較）

Rust の `PartialEq`/`PartialOrd` に相当（名前は `Eq`/`Ord`）。ジェネリック関数の `where` 境界で
要素型の比較を要求するのに使う（`sort`/`member`/`assoc` 等）。

```lisp
(deftrait Eq   (equals ((self Self) (other Self)) bool)
               (not-equals ((self Self) (other Self)) bool))
(deftrait Ord  (less ((self Self) (other Self)) bool)
               (less-equal ((self Self) (other Self)) bool)
               (greater ((self Self) (other Self)) bool)
               (greater-equal ((self Self) (other Self)) bool))
```

各トレイトメソッドはそのまま関数として呼べる（`where (Eq A)`/`(Ord A)` 境界内、または実装済みの
具体型に対して）:

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `equals` | `(equals a b)` | `(A,A)→bool` where `Eq A` | 等しいか（Rust の `==`） |
| `not-equals` | `(not-equals a b)` | `(A,A)→bool` where `Eq A` | 等しくないか（`!=`） |
| `less` | `(less a b)` | `(A,A)→bool` where `Ord A` | `a < b` |
| `less-equal` | `(less-equal a b)` | `(A,A)→bool` where `Ord A` | `a <= b` |
| `greater` | `(greater a b)` | `(A,A)→bool` where `Ord A` | `a > b` |
| `greater-equal` | `(greater-equal a b)` | `(A,A)→bool` where `Ord A` | `a >= b` |

`Eq` 実装済み: `i32` `i64` `f64` `bool` `char` `string` `symbol` および `cons-cell<A,B>`（要素が
`Eq` なら再帰的に）。`Ord` 実装済み: `i32` `i64` `f64` `char` `string` および `cons-cell<A,B>`
（辞書順、要素が `Ord` なら）。メソッド名が組み込み演算子（`= /= < <= > >=`）・`eq`/`lt` と重複
しないのは、組み込みは再定義できず各実装がそれらへ委譲するため。スカラの比較演算子そのものは
レシーバ型で多重定義された組み込みメソッド（§1・§2・§8・§9）。

## 13. 高階関数

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `identity` | `(identity x)` | `T→T` | そのまま返す |
| `const` | `(const x y)` | `(A,B)→A` | 第一引数を返す |
| `compose` | `(compose f g)` | `((fn (B) C),(fn (A) B))→(fn (A) C)` | 関数合成 `f∘g` |
| `flip` | `(flip f)` | `(fn (A B) C)→(fn (B A) C)` | 2引数関数の引数順を入れ替える |

## 14. マクロ・システム

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `gensym` | `(gensym)` | `()→Symbol` | 衝突耐性のある新しいシンボルを返す（マクロ用） |
| `exit` | `(exit code)` | `i32→!` | プロセスを終了する |

`compile`/`compile-file` は [syntax.md](syntax.md) の「コンパイル」節を参照。

## 15. 標準入出力 (I/O)

`print`/`println`/`format` はいずれも**書式指定子（CL の `format` ディレクティブ)を解釈する特殊形**。
第1引数（`format` は第2引数）が**制御文字列**で、以降の可変長引数を各ディレクティブが順に消費する。
`(list ...)` と同じく、可変長引数は各自の型のまま `Sexpr` へ包まれてから渡る——`i32`/`i64`/`f64`/
`bignum`/`ratio`/`char`/`bool`/`string`/`Sexpr` はスカラ用の `Sexpr` コンストラクタでラップされ、
ユーザ定義 `defstruct`/`defenum`/`Vector<T>`/`HashTable<K,V>` 等ヒープ表現の ADT インスタンスは
無変換のまま `Sexpr` へ retype される（`(println "~a" my-struct)` はそのまま動く）。ネイティブ表現の
ジェネリック実体化（`Option<llvm-value>` 等）だけは `Sexpr` の表現を持たないため型エラーのまま。
`typl file.typl` によるスクリプト実行は [main.rs](../src/main.rs) の `run_file`
の通り**トップレベル式の戻り値を出力しない**ため、プログラム自身が標準出力へ書くにはこれらを呼ぶ必要がある。

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `print` | `(print control args...)` | `(string, ...)→Unit` | 制御文字列を書式展開し、改行なしで標準出力へ書く |
| `println` | `(println control args...)` | `(string, ...)→Unit` | 同上、末尾に改行を付ける |
| `format` | `(format dest control args...)` | `(bool, string, ...)→string` | CL の `format` 相当。展開した文字列を返す。`dest` が `true`（CL の `t`）なら加えて標準出力へも書く／`false`（CL の `nil`）なら書かず文字列を返すだけ |
| `read-line` | `(read-line)` | `()→Option<string>` | 標準入力から1行読む（末尾の改行/`\r`は除去）。EOFなら`None` |

### 書式ディレクティブ

CL の `format` ディレクティブをほぼ網羅する。各ディレクティブは `~`、任意の**プレフィックス
パラメータ**（カンマ区切り。整数 / `'c`（文字）/ `v`（次の引数から取る）/ `#`（残り引数数））、任意の
**修飾子** `:`・`@`、ディレクティブ文字、の順。ディレクティブ文字は大文字小文字を区別しない。
引数が余れば無視（CL と同じ）、足りなければ実行時エラー。

**出力（引数を1つ消費）**

| ディレクティブ | パラメータ / 修飾子 | 意味 |
|---|---|---|
| `~a` | `~mincol,colinc,minpad,padchar` / `@`=右詰め | aesthetic（CL `princ`。文字列クォートなし） |
| `~s` | 同上 | standard（CL `prin1`。reader 構文） |
| `~w` | — | `~s` と同義（pretty-print なしの `write`） |
| `~d` `~b` `~o` `~x` | `~mincol,padchar,commachar,interval` / `:`=桁区切り, `@`=符号必須 | 10/2/8/16進整数（非整数は `~a` 相当で表示） |
| `~r` | `~radix,...`（基数指定）または無指定 | 基数指定時はその進数。無指定で `~r`=英語基数、`~:r`=英語序数、`~@r`=ローマ数字、`~:@r`=旧ローマ |
| `~p` | `:`=1つ戻る, `@`=y/ies | 複数形（`~p`→"s"、`~@p`→"y"/"ies"） |
| `~c` | `:`=名前, `@`=`#\`構文 | 文字 |
| `~f` | `~w,d,k,overflowchar,padchar` / `@`=符号 | 固定小数点 |
| `~e` `~g` | float | 指数 / 汎用浮動小数点 |
| `~$` | `~d,n,w,padchar` / `:`,`@` | 金額表記 |

**出力（引数を消費しない）**

| ディレクティブ | 意味 |
|---|---|
| `~%` | 改行（`~n%` で n 個） |
| `~&` | fresh-line（行頭でなければ改行。`~n&`） |
| `~\|` | 改ページ（form feed） |
| `~~` | リテラルの `~`（`~n~` で n 個） |
| `~t` | タブ（`~col,incT`。`:`/`@`=相対） |
| `~<改行>` | 改行を無視（`:`=空白保持, `@`=改行保持） |

**制御構造**

| ディレクティブ | 意味 |
|---|---|
| `~(...~)` | 大文字小文字変換（`~(`小文字, `~:(`各語頭大文字, `~@(`先頭のみ大文字, `~:@(`全大文字） |
| `~[...~;...~]` | 条件選択（整数で分岐。`~:;`=デフォルト節, `~:[偽~;真~]`=真偽, `~@[...~]`=非false時のみ） |
| `~{...~}` | 反復（リスト引数を走査。`~:{`=部分リストごと, `~@{`=残り引数, `~^`=脱出, `~:}`=空でも1回） |
| `~<...~;...~>` | 桁揃え（セグメントを `~mincol` 幅に分散。`:`/`@`=端の詰め） |
| `~?` | 間接（次の引数=制御文字列、その次=引数リスト。`~@?`=以降の引数を流用） |
| `~*` | 引数スキップ（`~n*`=n個進む, `~:*`=戻る, `~@*`=絶対位置へ） |

**未対応**（実行時エラー or no-op）: `~/name/`（関数呼び出しディレクティブ。実行時の関数名解決機構が
`format` の呼出規約に合わない）、pretty-printer 系の `~i`/`~_`（pretty-print ストリームが無いため no-op）。
CL の `~:a`/`~@[` の nil 特有挙動は typelisp の `false` に読み替える（nil は無い）。

```lisp
(println "~a + ~a = ~d" 1 2 3)        ; => 1 + 2 = 3
(println "[~5,'0d]" 42)               ; => [00042]
(println "~:d" 1234567)               ; => 1,234,567
(println "~@r / ~r" 2024 42)          ; => MMXXIV / forty-two
(println "~{~a~^, ~}" '(a b c))       ; => a, b, c
(println "~[zero~;one~;two~]" 1)      ; => one
(let ((s (format false "id=~d" 42)))  ; 出力せず文字列だけ得る
  (println "~a" s))                   ; => id=42
```

`print`/`println`/`format` は呼び出しのたびに即座に `flush` する（パイプ経由でも `read-line` の前に
プロンプトが確実に見えるようにするため）。書式エンジンは [format.rs](../src/eval/format.rs)（制御文字列を
[`Node`] 木にパース→引数リストに対して解釈。`~a`/`~s` の値描画は GCヒープ走査＋enum 変種名解決が要る
Rust 専用処理）、`Interp::run_format` が enum 変種名表を渡して呼ぶ。可変長引数を `Sexpr` リストへまとめる
特殊形は [checker.rs](../src/check/checker.rs) の `check_format`/`check_print_like`。コンパイル
（`compile`）対象ではない（旧 `print`/`println` も未対応だった）。

## 16. 解析・評価 (`parse-int` / `parse-float` / `read` / `eval`)

いずれも実行時の（プログラム自身は制御できない）テキスト・データを扱うため、失敗時は panic では
なく `Result<_, Error>` の `Err` を返す。

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `parse-int` | `(parse-int s)` | `string→Result<i32,Error>` | 10進整数（`+`/`-`前置可）。Rust の `str::parse::<i32>` と同じ受理範囲 |
| `parse-float` | `(parse-float s)` | `string→Result<f64,Error>` | 浮動小数点数。Rust の `str::parse::<f64>` と同じ受理範囲（`inf`/`nan`含む） |
| `read` | `(read s)` | `string→Result<Sexpr,Error>` | `s` から `Sexpr` を1つ読む（`typl`/REPL がソーステキストを読むのと同じ reader を使う）。不完全な括弧・文字列などは `Err` |
| `eval` | `(eval form)` | `Sexpr→Result<Sexpr,Error>` | `form` を実行時に型チェックして評価する。CL の `eval` に準拠 |

### `eval` の意味論（Common Lisp 準拠）

CLHS の `eval` に準拠する: **現在の大域環境**（グローバルの関数・変数・型・マクロ。実行時に
追加された定義も含む）で、かつ **null 字句環境**（呼び出し元の `let`/`lambda` のローカル束縛は
見えない）で評価する。式でも定義（`defun`/`defvar`/`defstruct`/`defenum`/`defmacro`）でも評価
でき、定義は即座かつ永続的にグローバル環境へ登録される。

```lisp
(eval (unwrap (read "(+ 40 2)")))                 ; => (ok 42)
(defvar (x i32) 10)
(eval (unwrap (read "(+ x 5)")))                  ; => (ok 15)  ; グローバル x が見える
(eval (unwrap (read "(defun sq ((n i32)) i32 (* n n))")))  ; => (ok sq)  ; 定義名を返す
(eval (unwrap (read "(sq 9)")))                   ; => (ok 81)  ; 直前の定義が見える
```

- **戻り値**: 式なら評価結果を `Sexpr` として、定義なら定義名シンボルを返す（CL と同じ）。
  結果を `println` 等で表示するには `Sexpr` を `match`（`(int n)`/`(str s)`/…）で分解する。
- **静的型ゆえの差異（重要）**: CL は結果の実値（動的型）を返すが typelisp は戻り型を一律
  `Result<Sexpr,Error>` にするしかない。また **静的に書いたコードは、実行時に `eval` が定義する
  名前を前方参照できない**——チェッカーは全トップレベルフォームを実行前に検査するので、
  ファイル中に直接書いた `(sq 9)` は `sq` を定義する `eval` より前に検査され「未定義」になる。
  ただし **後続の `eval` からは見える**（その `eval` の型チェックは実行時、定義後に走るため）。
  REPL は1行ずつ検査・実行するので、`eval` で定義した名前を次の行から直接呼べる。
- **エラーの扱い**: チェッカーが静的に弾ける型エラー・構文エラーは `Err` を返す（パニックしない）。
  評価したコード内の**実行時パニック**（ゼロ除算等）は、直接書いたコードと同様にそのまま伝播する
  （CL の condition system は typelisp に無いため、これが最も近い挙動）。
- **名前空間**: `typl file.typl` 実行時、`eval` はそのスクリプトのファイル由来モジュール名前空間で
  評価される（スクリプト自身のグローバルが見える）。REPL はルート名前空間で評価する。
