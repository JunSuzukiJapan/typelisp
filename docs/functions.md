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
| `floor-div` `ceiling-div` `round-div` `truncate-div` | `(op a b)` | `(T,T)→cons-cell<T,T>` | CL の2引数 `floor`/`ceiling`/`round`/`truncate`（`(floor 7 2)`→商3・剰余1）に相当。多値の代わりに商・剰余を`cons-cell`（`car`=商、`cdr`=剰余）で返す（§5、§3.4）。`round-div` は同点をCL準拠で偶数側に丸める |
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
| `floor-div` `ceiling-div` `round-div` `truncate-div` | `(op a b)` | `(f64,f64)→cons-cell<f64,f64>` | CL の2引数版（`(floor 7.0 2.0)`→商2・剰余1）に相当。§1 の同名メソッドと同じ設計（`car`=商、`cdr`=剰余） |
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
| `keywordp` | `(keywordp s)` | `Symbol→bool` | キーワード（`:name`）か。コロンは名前の一部なので判定は先頭文字（[syntax.md](syntax.md) §1） |
| `equal` | `(equal a b)` | `(Sexpr,Sexpr)→bool` | 構造的等価（`Cons` は再帰、`Str` は内容比較） |
| `equalp` | `(equalp a b)` | `(Sexpr,Sexpr)→bool` | `equal` に加え大文字小文字無視・数値の型跨ぎ比較 |

述語を取る関数（CL の `-if` 系に対応。すべて `where (Iter I (Item A))`）:

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `map` | `(map it f)` | `(Iter<A>,(fn (A) U))→Vector<U>` | 写像 |
| `filter` | `(filter it pred)` | `(Iter<A>,(fn (A) bool))→Vector<A>` | 条件を満たす要素のみ |
| `remove-if` | `(remove-if it pred)` | `(Iter<A>,(fn (A) bool))→Vector<A>` | 条件を満たす要素を除く |
| `find-if` | `(find-if it pred)` | `(Iter<A>,(fn (A) bool))→Option<A>` | 条件を満たす最初の要素 |
| `position-if` | `(position-if it pred)` | `(Iter<A>,(fn (A) bool))→Option<i32>` | 条件を満たす最初の位置 |
| `count-if` | `(count-if it pred)` | `(Iter<A>,(fn (A) bool))→i32` | 条件を満たす個数 |
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
| `find` | `(find x it)` | `(A,Iter<A>)→Option<A>` where `Eq A` | `x` と等しい最初の要素（CL 本来の `find`。デフォルト `:test` の `eql` に相当） |
| `position` | `(position x it)` | `(A,Iter<A>)→Option<i32>` where `Eq A` | `x` と等しい最初の位置 |
| `count` | `(count x it)` | `(A,Iter<A>)→i32` where `Eq A` | `x` と等しい要素の個数 |
| `sort` | `(sort it cmp)` | `(Iter<A>,(fn (A A) bool))→Vector<A>` | CL 本来の `(sort sequence predicate)`。安定な非破壊挿入ソート。`cmp` は「第1引数が第2引数より真に前」で `true` |
| `assoc` | `(assoc k it)` | `(K,Iter<cons-cell<K,V>>)→Option<cons-cell<K,V>>` where `Eq K` | `car` が `k` と等しい最初のペア。値は `(cdr p)` で取り出す |

`(list e1 e2 ... en)` は特殊形（`(cons e1 (cons e2 (... (Nil))))` へ展開、[syntax.md](syntax.md) 参照）。
`map` / `filter` 等が返す `Vector<T>` を再び回すには `(iter result)` を渡す。

> **旧 API から削除された関数**（`docs/dev/symbol-sexpr-redesign.md` Phase 5 / 6.5）:
> `consp` `null` `atom`（`Sexpr` 述語）、`nthcdr` `copy-list`（cons チェーン専用）、
> `nconc` `nreverse`（破壊的操作）、`remove-if-not`。`find-if`/`count-if`/`position-if`
> は一時 `find`/`count`/`position` に統合されていたが、CL 本来の項目ベース版
> `find`/`count`/`position`（上表）を別途追加したのに伴い述語版の名前として復活した。
> `remove`（要素削除）は現在 `HashTable<K,V>` のメソッドとしてのみ存在（§11）。

## 7. `Option<T>` / `Result<T,E>`

構成子: `Option<T>` は `Some(T)` / `None`。`Result<T,E>` は `Ok(T)` / `Err(E)`。
`E` は任意の型でよい——組み込みの具象エラー型も、`defstruct`/`defenum` で書いた自前の型も
そのまま載る（§7.1）。

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

### 7.1 エラー型と `Error` トレイト

Rust の `std::error::Error` に倣い、**`Error` は型ではなくトレイト**。エラーを表す具象型は
用途ごとに分かれていて、いずれも `Error` を実装する。

| 型 | 生成元 |
|---|---|
| `ParseIntError` | `parse-int` |
| `ParseFloatError` | `parse-float` |
| `ReadError` | `read` |
| `EvalError` | `eval` |

いずれも「メッセージ文字列を1つ持つ単一変種の直和型」で、型名と変種名が同じ
（`(match e ((ParseIntError m) m))`、構成は `(ParseIntError::ParseIntError "...")`）。
特別扱いは一切なく、自前のエラー型を `(defstruct my-err (...))` / `(defenum my-err ...)`
で書いたときとまったく同じ扱いになる。

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `message` | `(message e)` | `Self→string` | エラーメッセージ（`Error` トレイトのメソッド） |
| `source` | `(source e)` | `Self→Option<:dyn Error>` | このエラーが包んでいる原因、無ければ `None`（Rust の `Error::source`） |
| `as-dyn-error` | `(as-dyn-error r)` | `Result<T,E>→Result<T,:dyn Error>`（`E` は `Error` 実装） | 具象エラー型を trait オブジェクトへ広げる |

自前のエラー型に `Error` を実装すれば、組み込みエラーと**同じ形で**扱える:

```lisp
(defstruct io-err (path string))
(impl Error io-err
  (message ((self Self)) string (append "io failed: " self::path))
  (source ((self Self)) Option<:dyn Error> (option::none)))

(defun open-it ((p string)) Result<i32, io-err>          ; 具象型をそのまま E に載せる
  (if (equal p "") (result::err (io-err::new "<empty>")) (result::ok 3)))

(defun describe ((e :dyn Error)) string (message e))     ; 種類を問わず一様に扱う
(describe (io-err::new "/etc/app.conf"))
(describe (ParseIntError::ParseIntError "boom"))
```

複数のエラー型を1つの `Result` に集める場合は `Result<T, :dyn Error>`（Rust の
`Box<dyn Error>` 相当）を使い、具象エラーは `as-dyn-error` で広げる。typelisp には `?` が
無い（[language-design.md](dev/language-design.md) §7.3）ので、この変換は明示的に書く:

```lisp
(defun run ((s string)) Result<i32, :dyn Error>
  (match (as-dyn-error (parse-int s))            ; ParseIntError -> :dyn Error
    ((ok n) (as-dyn-error (open-it (if (= n 0) "" "f"))))   ; io-err -> :dyn Error
    ((err e) (result::err e))))
```

なお**型とトレイトは1つの名前空間を共有する**（Rust と同じ）。同じモジュール内で
`defstruct`/`defenum` とトレイトに同じ名前は付けられず、型位置にトレイト名を書くと
「`error` is a trait, not a type — write `:dyn error`」と報告される。

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
(deftrait Iter ()
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
(deftrait Eq ()
  (equals ((self Self) (other Self)) bool)                ; 実装必須
  (not-equals ((self Self) (other Self)) bool             ; デフォルト実装
    (not (equals self other))))
(deftrait Ord (Eq)                                        ; Eq を継承
  (less ((self Self) (other Self)) bool)                  ; 実装必須
  (less-equal ((self Self) (other Self)) bool (not (less other self)))
  (greater ((self Self) (other Self)) bool (less other self))
  (greater-equal ((self Self) (other Self)) bool (not (less self other))))
```

`Eq` を実装するには `equals` だけ、`Ord` を実装するには `less` だけ書けばよい。残りはデフォルト
実装が埋める。`Ord` は `Eq` を継承するので、`impl Ord X` の前に `impl Eq X` が必要。

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

`Eq` 実装済み: `i32` `i64` `f64` `bignum` `ratio` `bool` `char` `string` `symbol` および
`cons-cell<A,B>`（要素が `Eq` なら再帰的に）。`Ord` 実装済み: `i32` `i64` `f64` `bignum` `ratio`
`char` `string` および `cons-cell<A,B>`（辞書順、要素が `Ord` なら）。メソッド名が組み込み演算子
（`= /= < <= > >=`）・`eq`/`lt` と重複
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
| `format`（ストリーム宛） | `(format stream control args...)` | `(S, string, ...)→()` where `CharOutput S` | `dest` が `bool` でなければ CL のストリーム宛。展開した文字列をそのストリームへ書く。戻り値は `()`（CL の `nil` 相当）で、文字列は返らない |

`dest` の型で2つの意味に分かれる（どちらになるかは静的に決まる）。ストリーム宛は
`(write-string dest (format false control args...))` へ展開されるだけなので、具象ストリーム型・
`:dyn CharOutput`・`(where (CharOutput S))` の型変数のいずれでも同じように書ける。
`bool` でもストリームでもない `dest` は「destination は `true`/`false` か `CharOutput` を実装した
ストリーム」という型エラーになる。

**標準入力を読む**のは専用関数ではなく、標準ストリーム `*standard-input*` に対する
`CharInput` のメソッド（§18.1）——`(read-line *standard-input*)` / `(read-char *standard-input*)` /
`(read-all *standard-input*)`。標準出力・標準エラーも同様に `*standard-output*` /
`*error-output*` があり、`(write-line *standard-output* s)` のように書ける（上の
`print`/`println`/`format` は書式展開が要るときの近道で、常に標準出力へ書く）。

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
| `~w` | — | CL の `write`。`*print-pretty*` が真なら整形出力、偽なら `~s` と同義 |
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
| `~t` | タブ（`~col,incT`。`:`/`@`=相対。pretty 時は `:`=論理ブロック起点の相対タブ） |
| `~_` | 条件改行（pretty。素=`:linear` / `~:_`=`:fill` / `~@_`=`:miser` / `~:@_`=`:mandatory`） |
| `~i` | インデント（pretty。`~ni`=ブロック起点+n / `~n:i`=現在桁+n） |
| `~<改行>` | 改行を無視（`:`=空白保持, `@`=改行保持） |

**制御構造**

| ディレクティブ | 意味 |
|---|---|
| `~(...~)` | 大文字小文字変換（`~(`小文字, `~:(`各語頭大文字, `~@(`先頭のみ大文字, `~:@(`全大文字） |
| `~[...~;...~]` | 条件選択（整数で分岐。`~:;`=デフォルト節, `~:[偽~;真~]`=真偽, `~@[...~]`=非false時のみ） |
| `~{...~}` | 反復（リスト引数を走査。`~:{`=部分リストごと, `~@{`=残り引数, `~^`=脱出, `~:}`=空でも1回） |
| `~<...~;...~>` | 桁揃え（セグメントを `~mincol` 幅に分散。`:`/`@`=端の詰め） |
| `~<...~;...~:>` | **論理ブロック**（閉じが `~:>`。上の桁揃えとは別物）。先頭セグメント=prefix、末尾=suffix（いずれもリテラル文字列のみ）。`~@;` 区切りなら prefix は**行頭 prefix**。`~:<` は prefix/suffix を `(`/`)` に既定。引数はリスト1つ（`~@<` は残り引数をその場で使う） |
| `~?` | 間接（次の引数=制御文字列、その次=引数リスト。`~@?`=以降の引数を流用） |
| `~*` | 引数スキップ（`~n*`=n個進む, `~:*`=戻る, `~@*`=絶対位置へ） |

**未対応**（実行時エラー）: `~/name/`（関数呼び出しディレクティブ。実行時の関数名解決機構が
`format` の呼出規約に合わない）。CL の `~:a`/`~@[` の nil 特有挙動は typelisp の `false` に読み替える
（nil は無い）。pretty-printer 系ディレクティブ（`~_` `~i` `~:t` `~<...~:>`、および `~a`/`~s`/`~w` の
整形経路）は `*print-pretty*` が偽のとき CL 同様すべて no-op——既定は偽なので、既存の出力は一切変わらない。
詳細は下の §15.1。

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

`print`/`println`/`format` は呼び出しのたびに即座に `flush` する（パイプ経由でも標準入力を読む前に
プロンプトが確実に見えるようにするため）。書式エンジンは [format.rs](../src/eval/format.rs)（制御文字列を
[`Node`] 木にパース→引数リストに対して解釈。`~a`/`~s` の値描画は GCヒープ走査＋enum 変種名解決が要る
Rust 専用処理）、`Interp::run_format` が enum 変種名表を渡して呼ぶ。可変長引数を `Sexpr` リストへまとめる
特殊形は [checker.rs](../src/check/checker.rs) の `check_format`/`check_print_like`。コンパイル
（`compile`）対象ではない（旧 `print`/`println` も未対応だった）。

### 15.1 pretty printer（CLHS 22.2）

CL の Lisp Pretty Printer 相当。**行幅に収まらない出力を、論理ブロックと条件改行の指定に従って
折り返す**。実体は [pprint.rs](../src/eval/pprint.rs)。

#### 制御変数

CL では特殊変数（`let` で動的に束縛する）だが、typelisp に動的束縛は無いので**通常の代入可能な
グローバル**（prelude の `defvar`）。`setf` した時点から以降のすべての印字に効く。

| 変数 | 型 | 既定 | 意味 |
|---|---|---|---|
| `*print-pretty*` | `bool` | `false` | 真なら `~a`/`~s`/`~w` と pretty ディレクティブが整形経路に入る |
| `*print-right-margin*` | `i64` | `80` | 右マージン（桁）。0 以下は「マージン無し＝折らない」 |
| `*print-miser-width*` | `i64` | `0` | miser スタイルに入る幅。0 以下は CL の `nil`（miser 無効）に相当 |

既定が `false` なのは、既存プログラムの出力を一切変えないため（CL でも初期値は処理系定義）。
`pprint` 系と `pprint-logical-block` は `*print-pretty*` に関わらず常に整形する（CL の `pprint` の定義通り）。

上の3つが「どう並べるか」を決めるのに対し、**「どこまで印字するか」**を決める CLHS 22.1.1 の
制御変数が別に3つある。こちらは整形の有無に関係なく、`print`/`println`/`format`/`pprint` の
すべてに効く（§15.3）。

#### 既製レイアウト（特殊形）

`print` と同じく特殊形なので、引数はどんな型でもよい（各自の型のまま `Sexpr` へ包まれる）。

| 名前 | 形式 | 説明 |
|---|---|---|
| `pprint` | `(pprint x)` | 既定レイアウトで整形出力。CL 準拠で**先頭に改行**を出し、末尾には出さない |
| `pprint-fill` | `(pprint-fill x)` | 1行に入るだけ詰める（語詰め）。改行は出さない |
| `pprint-linear` | `(pprint-linear x)` | 全要素が1行に収まらなければ**1要素1行**。改行は出さない |
| `pprint-tabular` | `(pprint-tabular x [colinc])` | `colinc` 桁の表形式（既定 16）。改行は出さない |

既定レイアウト（`pprint` / `*print-pretty*` 下の `~a`）は、CL の既定 `*print-pprint-dispatch*` に倣って
`(quote x)` を `'x` と略記し、`defun`/`let`/`if`/`lambda` 等のコード形は「頭部＋規定個数の引数を1行目、
残りの本体を2桁字下げして1行ずつ」に整形する。それ以外のリストは fill（語詰め）。

```lisp
(setf *print-right-margin* 20)
(pprint '(1 2 3 4 5 6 7 8 9 10 11 12 13 14 15))
;; =>
;; (1 2 3 4 5 6 7 8 9
;;  10 11 12 13 14 15)
(pprint '(defun f (x) i64 (+ x 1) (* x 2)))
;; =>
;; (defun f (x) i64
;;   (+ x 1)
;;   (* x 2))
```

#### 論理ブロックを自分で組む

| 名前 | 形式 | 説明 |
|---|---|---|
| `pprint-logical-block` | `(pprint-logical-block (obj :prefix p :per-line-prefix p :suffix s) body...)` | 論理ブロックを開く特殊形。`obj` は `pprint-pop` が辿るリスト（辿らないなら `()`）。`:prefix` と `:per-line-prefix` は排他（CL と同じ） |
| `pprint-newline` | `(pprint-newline kind)` | 条件改行。`kind` は `:linear` / `:fill` / `:miser` / `:mandatory` |
| `pprint-indent` | `(pprint-indent kind n)` | 字下げ。`kind` は `:block`（ブロック起点から）/ `:current`（現在桁から） |
| `pprint-tab` | `(pprint-tab kind colnum colinc)` | タブ。`kind` は `:line` / `:section` / `:line-relative` / `:section-relative` |
| `pprint-pop` | `(pprint-pop)` | ブロックのリストから次の要素を取る（尽きていれば `()`） |
| `pprint-list-exhausted` | `(pprint-list-exhausted)` | リストが尽きたか |
| `pprint-exit-if-list-exhausted` | `(pprint-exit-if-list-exhausted)` | 尽きていれば囲む `loop` を `break`（マクロ） |

typelisp には第一級ストリームが無いので、**開いている論理ブロックは暗黙のインタプリタ状態**（GC ヒープと
同じ扱い）。最も外側の `pprint-logical-block` が開始し、それが閉じたときに一括で整形して標準出力へ書く。
開いている間は `print`/`println`/`(format true ...)`/`pprint` の出力もすべてそのブロックへ入るので、
**内容は普通の `print` で書き、改行位置だけ `pprint-newline` 等で指定する**——CL のコードとほぼ同じ形になる。

`pprint-exit-if-list-exhausted` は CL ではブロックからの非局所脱出だが、typelisp に汎用の脱出機構は
無いので**囲む `loop` からの `break`** として実装している。CL 側の定型もつねに `loop` の中に書くので、
実用上の書き味は変わらない。

```lisp
(setf *print-right-margin* 24)
(pprint-logical-block ('(alpha beta gamma delta epsilon zeta) :prefix "(" :suffix ")")
  (loop (pprint-exit-if-list-exhausted)
        (print "~w" (pprint-pop))
        (if (pprint-list-exhausted) () (progn (print " ") (pprint-newline :fill)))))
;; =>
;; (alpha beta gamma delta
;;  epsilon zeta)
```

条件改行の判定規則（CLHS `pprint-newline`）:

- `:mandatory` — つねに折る。
- `:linear` — 囲む論理ブロックが1行に収まらなければ折る。ブロック単位の判定なので、**同一ブロックの
  `:linear` はすべて一緒に折れる**（`pprint-linear` の「全部1行か1要素1行か」はこれ）。
- `:fill` — (a) 次の区間が行の残りに収まらない、(b) 直前の区間が1行に収まらなかった、
  (c) miser スタイルでブロックが1行に収まらない、のいずれかで折る。
- `:miser` — miser スタイル（ブロックの開始桁が右マージンから `*print-miser-width*` 以内）のときだけ
  `:linear` として働く。

### 15.2 `print-object`（型ごとの印字表現）

`impl print-object <型>` を書くと、`print`/`println`/`format`/`pprint` がその型の値を——
**リストの中に入れ子で埋まっていても**——その実装で印字する。CL の CLOS 総称関数
`print-object`（CLHS 22.1.4）に対応する。

```lisp
(deftrait print-object
  (print-object ((self Self) (escape bool)) string))
```

| 引数 | 意味 |
|---|---|
| `self` | 印字する値 |
| `escape` | CL の `*print-escape*`。`~s`/`prin1`/`pprint` で `true`（reader 構文）、`~a`/`princ` で `false`（人間向け）。気にしない実装は無視してよい |

戻り値の `string` がそのまま出力に流れる。組み込み型には**一切実装を入れていない**ので、
`impl` を書かない限り既存の出力（`#<point 1 2>` 形式）は1バイトも変わらない。

```lisp
(defstruct point (x i64) (y i64))
(impl print-object point
  (print-object ((self Self) (escape bool)) string
    (if escape (format false "#S(point :x ~d :y ~d)" self::x self::y)
               (format false "(~d,~d)" self::x self::y))))

(println "~a" p)              ; => (1,2)
(println "~s" p)              ; => #S(point :x 1 :y 2)
(println "~a" (list p q))     ; => ((1,2) (3,4))   ← 入れ子でも効く
```

pretty printer とも合成される（§15.1）。`*print-pretty*` が真なら、実装が返した文字列を
含むリストが右マージンで折り返される。

#### 設計上の約束ごと

- **登録は静的**。`impl` はふつうのメソッド定義として型検査されるので、型名の打ち間違いも
  シグネチャ違いもコンパイルエラーになる。別建ての登録表は無い。
- **選択は印字時**。どのディレクティブがどの引数を消費するかは制御文字列の実行時の中身で
  決まるため、`~a` と `~s` の区別（＝`escape`）は印字の瞬間にしか分からない。CLOS が
  `print-object` メソッドを「クラスごとに定義し、印字時に選択する」のと同じ。
- **再入は組み込み表現へフォールバック**。実装が `(format false "~a" self)` と自分自身を
  印字すると無限再帰になるので、印字中の値が再び現れたら組み込み表現に落とす。深さ制限では
  なく値の同一性で見るので、正当な自己参照構造の入れ子印字は妨げない。
- **ジェネリック型の実行時選択は不可**。`Vector<point>` の箱は実行時には要素型を持たない
  （`vector` としか名乗らない）ので、`impl print-object Vector<T>` を実行時に選ぶことは
  できない。ただし組み込みの `Vector` 描画が要素へ再帰し、各要素がそこでディスパッチされる
  ので、見え方は揃う。
- CL のもう一方の機構 `set-pprint-dispatch` / `*print-pprint-dispatch*`（型指定子をキーに
  した実行時の登録表）は**採用しない**。文字列キーもプリンタのシグネチャも無検査で、
  「登録時点で分かっていた型を捨ててから `match` で復元する」形になり、静的型付け言語には
  合わない。非採用の確定事項としては
  [dev/language-design.md](dev/language-design.md) §9、経緯は
  [dev/implementation-log.md](dev/implementation-log.md)（旧 TODO T5-b の節）。

### 15.3 印字量の制御（`*print-level*` / `*print-length*` / `*print-circle*`）

CLHS 22.1.1 の「値のどこまでを印字するか」を決める制御変数。§15.1 の3つと同じく prelude の
代入可能なグローバルで、`print`/`println`/`format`/`pprint` のすべてに——`*print-pretty*` の
真偽にかかわらず——効く。

| 変数 | 型 | 既定 | 意味 |
|---|---|---|---|
| `*print-level*` | `i64` | `0` | この深さ以上に入れ子になったオブジェクトを `#` で置き換える。印字対象そのものが深さ 0。0 以下は無制限 |
| `*print-length*` | `i64` | `0` | リストの要素（`defstruct`/`defenum` 値のフィールドも）をこの個数まで印字し、残りを `...` にする。0 以下は無制限 |
| `*print-circle*` | `bool` | `false` | 真なら、印字前に値を走査して**2回以上現れるオブジェクトにラベルを振る**。最初の出現が `#n=…`、以降が `#n#` |

CL は「無制限」を `nil` で表すが typelisp に `nil` は無いので、`*print-right-margin*` 等と同じく
**0 以下を無制限**とする。既定はすべて「制限なし／ラベルなし」で、CL の初期値とも既存の出力とも
一致する。

```lisp
(setf *print-level* 2)
(println "~a" '(1 (2 (3 (4)))))   ; => (1 (2 #))
(setf *print-level* 0)

(setf *print-length* 4)
(println "~a" '(1 2 3 4 5 6))     ; => (1 2 3 4 ...)
(setf *print-length* 0)
```

#### `*print-circle*` と循環構造

**循環した構造を印字できるのはこの変数を真にしたときだけ**である。偽（既定）のまま自分自身を
指す値を印字すると、プリンタは循環を辿り続けてプロセスが落ちる——これは CL でも同じ挙動
（CLHS は `*print-circle*` が偽のときの循環構造の印字を未定義としている）。

循環は「`defstruct` のフィールドを `setf` で自分自身に向ける」経路でのみ作れる（`Sexpr` の
cons セルは作成後に書き換えられないので、`'(1 2 3)` のようなリストが循環することはない）:

```lisp
(defenum link (no-link) (to node))
(defstruct node (val i64) (next link))

(let ((a (node::new 1 (link::no-link))))
  (setf a::next (link::to a))       ; a が a 自身を指す
  (setf *print-circle* true)
  (println "~a" a))                 ; => #1=#<node 1 (to #1#)>
```

ラベルは**1回の印字対象ごとに 1 から振り直す**（CL と同じ）。循環していなくても、同じ
オブジェクトが2回現れれば `#1=`/`#1#` が付く——「この2つは同一のオブジェクトだ」という情報を
出力に残すための CL の仕様どおりの挙動:

```lisp
(setf *print-circle* true)
(let ((x '(1 2)))
  (println "~a" (list x x)))        ; => (#1=(1 2) #1#)
```

共有が1つも無い値では**ラベルは一切現れない**ので、この変数を真にしたまま普段のコードを動かしても
出力は変わらない。

## 16. 解析・評価 (`parse-int` / `parse-float` / `read` / `eval`)

いずれも実行時の（プログラム自身は制御できない）テキスト・データを扱うため、失敗時は panic では
なく `Result` の `Err` を返す。エラー型は Rust の std に倣い**操作ごとの具象型**（§7.1）。

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `parse-int` | `(parse-int s)` | `string→Result<i32,ParseIntError>` | 10進整数（`+`/`-`前置可）。Rust の `str::parse::<i32>` と同じ受理範囲 |
| `parse-float` | `(parse-float s)` | `string→Result<f64,ParseFloatError>` | 浮動小数点数。Rust の `str::parse::<f64>` と同じ受理範囲（`inf`/`nan`含む） |
| `read` | `(read s)` | `string→Result<Sexpr,ReadError>` | `s` から `Sexpr` を1つ読む（`typl`/REPL がソーステキストを読むのと同じ reader を使う）。不完全な括弧・文字列などは `Err`。CL の `read-from-string` に当たる——ストリームから読むのは `read-sexpr`（§18.5） |
| `eval` | `(eval form)` | `Sexpr→Result<Sexpr,EvalError>` | `form` を実行時に型チェックして評価する。CL の `eval` に準拠 |

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
  `Result<Sexpr,EvalError>` にするしかない。また **静的に書いたコードは、実行時に `eval` が定義する
  名前を前方参照できない**——チェッカーは全トップレベルフォームを実行前に検査するので、
  ファイル中に直接書いた `(sq 9)` は `sq` を定義する `eval` より前に検査され「未定義」になる。
  ただし **後続の `eval` からは見える**（その `eval` の型チェックは実行時、定義後に走るため）。
  REPL は1行ずつ検査・実行するので、`eval` で定義した名前を次の行から直接呼べる。
- **エラーの扱い**: チェッカーが静的に弾ける型エラー・構文エラーは `Err` を返す（パニックしない）。
  評価したコード内の**実行時パニック**（ゼロ除算等）は、直接書いたコードと同様にそのまま伝播する
  （CL の condition system は typelisp に無いため、これが最も近い挙動）。
- **名前空間**: `typl file.typl` 実行時、`eval` はそのスクリプトのファイル由来モジュール名前空間で
  評価される（スクリプト自身のグローバルが見える）。REPL はルート名前空間で評価する。

## 17. docstring / `documentation`（Common Lisp 準拠）

`defun`/`defmethod`（`impl` 内も含む）/`defmacro`/`defvar`/`defconstant`/`defstruct`/`defenum`/
`deftrait` は docstring を持てる。位置は CL のそれぞれの規則にそのまま従う:

| フォーム | docstring の位置 |
|---|---|
| `defun` / `defmethod` / `defmacro` | 本体の先頭（戻り値型・`where` 節の後）。ただし後ろに本体フォームが最低1つ続く場合のみ——単独の文字列は戻り値のまま |
| `defvar` / `defconstant` | 初期値の**後ろ**: `(defvar (name Type) value "doc")` |
| `defstruct` / `defenum` | 名前の**直後**、フィールド/バリアント列の前 |
| `deftrait` | 継承リストの直後、アイテム列の前。トレイト全体に1つ。**デフォルト実装を持つメソッド**は、その本体の直前に自分の docstring を置ける |

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `documentation` | `(documentation name)` | （特殊形。`name` は裸シンボルまたは `Type::method`）→`Option<string>` | `name` の docstring を返す |

`documentation` は `quote`/`compile` と同様の特殊形（`name` を評価せず、未評価の名前として読む）。
CL の `(documentation 'name 'function)` と異なり型引数を取らない代わりに、裸名を**変数→関数→型→
トレイト→マクロ**の順（式として評価するときの裸識別子の優先順位と同じ）で解決する。`Type::method`
の形なら関連メソッド/静的メソッドの docstring を引く。

```lisp
(defun square ((n i32)) i32
  "Returns n squared."
  (* n n))

(unwrap-or (documentation square) "no docs")   ; => "Returns n squared."

(defstruct point
  "A 2D point."
  (x i32)
  (y i32))

(unwrap-or (documentation point) "no docs")    ; => "A 2D point."
```

**check 時に定数へ畳み込まれる**: `documentation` はランタイムのルックアップを一切行わない
（`Checker` は常にどの定義を指しているか静的に分かるため）。名前が何の定義にも解決できない場合は
check 時のエラー（未定義変数参照などと同様）。解決はできたが docstring が無い場合のみ `Option::none`。

**対象外**:
- `(setf documentation)`（docstring の実行時書き換え）は無い。
- モジュール修飾された自由名（`mod::name`。`Type::method` は対応）は非対応。
- `deftrait` 内の**本体を持たない**メソッド宣言は docstring を持てない。末尾の文字列リテラルは
  それ自体がデフォルト実装の本体（＝戻り値）になるので、両者を区別する手段が無い。
  デフォルト実装を持つメソッドは、`where` 節の後・本体の前に docstring を置ける
  （通常どおり「後ろに本体が続くときだけ docstring」の規則が効く）。

LSP のホバーにも統合されている: 定義済みの名前にカーソルを合わせると、型の下に docstring が
表示される（`src/check/locate.rs` の `doc_for`/`hover_text`）。

## 18. ストリームとファイル I/O

CL がクラス階層で表すものを、ここでは**トレイト階層**で表す。方向（入力／出力）も要素型も
**静的**に決まるので、「このストリームは読めるか」を実行時に尋ねる必要がない。

```lisp
(deftrait Stream ()             (open-stream-p ...) (close ...))
(deftrait InputStream  (Stream) (type Item) (read-item ...))
(deftrait OutputStream (Stream) (type Item) (write-item ...))
(deftrait CharInput  ((InputStream  (Item char))) ...)   ; 文字入力
(deftrait CharOutput ((OutputStream (Item char))) ...)   ; 文字出力
(deftrait PeekInput  (CharInput) (unread-char ...) (peek-char ...))  ; 1文字押し戻せる入力
```

文字を読む関数は `(where (CharInput S))` か `:dyn CharInput` を取れば、組み込み・ユーザ定義を
問わずあらゆるストリーム型を受け付ける。

### 18.1 メソッド（すべてトレイト経由）

`CharInput` の全メソッドはデフォルト実装を持つ。実装側が書くのは `read-item` だけ。

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `read-item` | `(read-item s)` | `(S)→Option<Item>` | 次の1要素。末尾なら `none`。**唯一の実装必須メソッド** |
| `read-char` | `(read-char s)` | `(S)→Option<char>` | 次の1文字 |
| `read-line` | `(read-line s)` | `(S)→Option<string>` | 次の改行まで（改行は消費して除去）。改行で終わらない最終行も返る |
| `read-all` | `(read-all s)` | `(S)→string` | 残り全部 |

`PeekInput`（`CharInput` を継承）は**1文字の押し戻し**を足す。デフォルト本体を持てない唯一の
入力操作なので別トレイトにしてある——押し戻した文字を置く場所はストリーム自身しか持たない。
組み込みのリーフストリーム（`file-stream`/`string-input-stream`/`standard-stream`）は実装済み、
それ以外は `make-peek-stream` で包めば得られる（§18.3）。

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `unread-char` | `(unread-char s c)` | `(S,char)→()` | 次の読みが `c` を返すようにする。**唯一の実装必須メソッド**。CL 同様、保証は1文字だけ |
| `peek-char` | `(peek-char s)` | `(S)→Option<char>` | 消費せずに次の1文字を見る |

`CharOutput` も同様に、実装側が書くのは `write-item` だけ。

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `write-item` | `(write-item s x)` | `(S,Item)→()` | 1要素を書く。**唯一の実装必須メソッド** |
| `write-char` | `(write-char s c)` | `(S,char)→()` | 1文字書く |
| `write-string` | `(write-string s str)` | `(S,string)→()` | 文字列を書く |
| `write-line` | `(write-line s str)` | `(S,string)→()` | 文字列＋改行 |
| `terpri` | `(terpri s)` | `(S)→()` | 改行を1つ（CL の名前） |
| `fresh-line` | `(fresh-line s)` | `(S)→()` | 行頭でなければ改行を1つ |
| `at-line-start` | `(at-line-start s)` | `(S)→bool` | 次に書く文字が行頭になるか。デフォルトは `false`（＝`fresh-line` は改行を書く。分からないなら書くほうが安全）。組み込みストリームは全て上書き済み |
| `finish-output` | `(finish-output s)` | `(S)→()` | バッファを送り出す |

`at-line-start` が覚えているのは**そのストリーム経由で書かれた分だけ**。`print`/`println`/
`(format true ...)` は標準出力へ直接書く（`*standard-output*` のハンドルを通らない）ので、
両者を混ぜると `(fresh-line *standard-output*)` の判断は `println` が書いた改行を知らない。
片方に寄せること。

`Stream` は全ストリーム共通:

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `open-stream-p` | `(open-stream-p s)` | `(S)→bool` | まだ開いているか |
| `close` | `(close s)` | `(S)→()` | 閉じる。**GC では閉じられない**ので明示的に（または `with-open-file` で） |

### 18.2 具象ストリーム型

| 型 | 作り方 | 実装するトレイト |
|---|---|---|
| `file-stream` | `(open-file name direction)` / `open-input` / `open-output` | `CharInput` `PeekInput` `CharOutput` |
| `string-input-stream` | `(make-string-input-stream s)` | `CharInput` `PeekInput` |
| `string-output-stream` | `(make-string-output-stream)` | `CharOutput` |
| `standard-stream` | `*standard-input*` `*standard-output*` `*error-output*` | `CharInput` `PeekInput` `CharOutput` |

`direction` は `direction-input` / `direction-output` / `direction-append` の3定数。
`open-file` は開けなければ `Err(FileError)` を返す（存在しないファイルは普通の結果であって
panic ではない）。ファイル名は文字列でも `pathname` でもよい（§19 の `Pathish`）。

`(get-output-stream-string s)` は `string-output-stream` に書かれた内容を返して空にする。
CL 同様、`close` 後でも取り出せる。

### 18.3 合成ストリーム

いずれも**ただの `defstruct`** で、ネイティブ層の支援を必要としない。入れ子にもできる。

| 名前 | 形式 | 説明 |
|---|---|---|
| `make-broadcast-stream` | `(make-broadcast-stream v)` | `Vector<:dyn CharOutput>` の全てへ書く |
| `make-two-way-stream` | `(make-two-way-stream in out)` | `in` から読み `out` へ書く |
| `make-echo-stream` | `(make-echo-stream in out)` | `in` から読み、読んだ文字を `out` にも書く |
| `make-concatenated-stream` | `(make-concatenated-stream v)` | `Vector<:dyn CharInput>` を順に読み継ぐ |
| `make-peek-stream` | `(make-peek-stream in)` | 任意の `:dyn CharInput` に1文字の押し戻しを足して `PeekInput` にする（`read-sexpr` 用） |

### 18.4 マクロ

| 名前 | 形式 | 説明 |
|---|---|---|
| `with-open-file` | `(with-open-file (var name direction) body...)` | 開く→本体→閉じる。`Result<本体の値, FileError>` |
| `with-input-from-string` | `(with-input-from-string (var s) body...)` | 文字列から読む |
| `with-output-to-string` | `(with-output-to-string (var) body...)` | 書かれた内容を返す |

### 18.5 ジェネリック関数とファイル操作

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `copy-stream` | `(copy-stream from to)` | `(I,O)→()` where `CharInput I`,`CharOutput O` | 全部転送 |
| `read-lines` | `(read-lines s)` | `(S)→Vector<string>` where `CharInput S` | 残り全行 |
| `read-sexpr` | `(read-sexpr s)` | `(S)→Result<Option<Sexpr>,ReadError>` where `PeekInput S` | `Sexpr` を1つ読む（CL の `read`）。入力末尾は `Ok(none)`、データでなければ `Err`。ちょうど1個だけ消費する |
| `write-lines` | `(write-lines s lines)` | `(S,I)→()` where `CharOutput S`,`Iter I (Item string)` | 1行ずつ書く |
| `read-file-string` | `(read-file-string name)` | `(P)→Result<string,FileError>` where `Pathish P` | 全内容 |
| `read-file-lines` | `(read-file-lines name)` | `(P)→Result<Vector<string>,FileError>` where `Pathish P` | 全行 |
| `write-file-string` | `(write-file-string name text)` | `(P,string)→Result<(),FileError>` where `Pathish P` | 書き出す |
| `probe-file` | `(probe-file name)` | `(P)→bool` where `Pathish P` | 存在するか |
| `delete-file` / `rename-file` | | `→Result<(),FileError>` | 削除・改名（引数は `Pathish`） |

ファイルを名指しする引数は全て**文字列でも `pathname` でもよい**——CL のパス名指定子と同じ扱いで、
実行時の型テストではなく `Pathish` トレイトで解決している（§19）。

### 18.6 自分の型をストリームにする

`write-item` を1つ書けば、残りはデフォルト実装が付いてくる。合成ストリームにも入れられる。

```lisp
(defstruct counter (n i32))
(impl Stream counter
  (open-stream-p ((self Self)) bool true)
  (close ((self Self)) () ()))
(impl OutputStream counter
  (type Item char)
  (write-item ((self Self) (c char)) () (setf self::n (+ self::n 1))))
(impl CharOutput counter)              ; 残りのメソッドは全部デフォルト

(write-line (counter::new 0) "四文字")  ; write-line も terpri も fresh-line も動く
```

入力側も同じで、書くのは `read-item` だけ。押し戻しを自前で持たない型でも、
`(read-sexpr (make-peek-stream my-stream))` と包めば `read` できる。

### 18.7 CL との違い

- **クラス階層ではなくトレイト階層**。`input-stream-p` / `output-stream-p` は無い——方向は型が
  持つので、実行時に尋ねる問いではない。
- **`read` は文字列版とストリーム版で名前が違う**。`(read "...")`（CL の `read-from-string`）と
  `(read-sexpr s)`（CL の `read`）。単一・静的ディスパッチなので同名の多重定義ができない。
- **押し戻しは別トレイト**（`PeekInput`）。`read-char` しか要らない型に `unread-char` の実装を
  強いないため。
- **閉じるのは明示的**。GC はクローズしない（コレクタは cons アリーナ枯渇時にしか走らないので、
  ファイナライザは予測できない時点で動くか一度も動かない）。`with-open-file` を使うのが安全。
- ストリーム操作は `format`/`random` と同じく**インタプリタ専用**で、これらを呼ぶ関数は
  JIT/AOT コンパイルされない。

## 19. パス名 (`pathname`)

ファイル名を分解した値。`/` 区切りのディレクトリ成分・名前・型（拡張子）と、ルート始まりかどうか
を持つ。分解も再構成も純粋な文字列処理なので**この層は全て typelisp で書かれている**——ネイティブ
側が見るのは `namestring` が描いた文字列だけ。

```lisp
(let ((p (parse-namestring "/var/log/app.tar.gz")))
  (pathname-directory p)   ; => ["var" "log"]（Vector<string>）
  (pathname-name p)        ; => (some "app.tar")   最後のドットで切る
  (pathname-type p)        ; => (some "gz")
  (namestring p))          ; => "/var/log/app.tar.gz"

(namestring (merge-pathnames (make-pathname :name "today" :type "log")
                             "/var/log/"))        ; => "/var/log/today.log"
```

### 19.1 パス名指定子トレイト `Pathish`

CL がパス名指定子（文字列 or パス名）を受ける場所で、こちらは `Pathish` を受ける。`string` と
`pathname` の両方が実装しており、**ファイル操作は全てこれをジェネリックに取る**ので、
`(open-input "a.txt")` と `(open-input p)` はどちらも普通の呼び出し（実行時の型テストは無い）。
文字列側の `namestring` は自分自身を返すだけなので、文字列を渡す限りパースは走らない。

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `namestring` | `(namestring p)` | `(P)→string` | 文字列表現。**唯一の実装必須メソッド**（`to-pathname` と2つ） |
| `to-pathname` | `(to-pathname p)` | `(P)→pathname` | `pathname` に直す（CL の `pathname` 関数。型名と衝突するので改名） |

### 19.2 関数

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `parse-namestring` | `(parse-namestring s)` | `string→pathname` | 文字列を分解する。末尾 `/`（や空名）は「名前無し」＝ディレクトリ |
| `make-pathname` | `(make-pathname :directory v :name s :type s :absolute b)` | `→pathname` | 持っている成分だけで組み立てる（全て `&key`）。省略した名前・型は「無い」ままで、`merge-pathnames` が埋める対象になる |
| `pathname-directory` | `(pathname-directory p)` | `(P)→Vector<string>` | 外側から順のディレクトリ成分 |
| `pathname-name` | `(pathname-name p)` | `(P)→Option<string>` | 型を除いた名前。ディレクトリなら `none` |
| `pathname-type` | `(pathname-type p)` | `(P)→Option<string>` | 最後のドット以降。先頭のドットは対象外（`.gitignore` は全部が名前） |
| `pathname-absolute-p` | `(pathname-absolute-p p)` | `(P)→bool` | ルート始まりか |
| `directory-namestring` | `(directory-namestring p)` | `(P)→string` | 最後の `/` までの部分 |
| `file-namestring` | `(file-namestring p)` | `(P)→string` | `name.type` の部分だけ |
| `merge-pathnames` | `(merge-pathnames p default)` | `(P,D)→pathname` | `p` に無い成分を `default` から補う。相対の `p` は `default` のディレクトリの下に置かれ、絶対の `p` は自分のディレクトリを保つ |
| `enough-namestring` | `(enough-namestring p default)` | `(P,D)→string` | `default` を基準にした相対表記。基準の下に無ければ `p` の全体 |

型引数はいずれも `(where (Pathish P))`。

### 19.3 CL との違い

- **ホスト・デバイス・バージョン成分は無い**。ワイルドカードパス名も `directory` による照合も、
  論理パス名（`logical-pathname`）も無い。CLHS 19 のそれらの部分は、この処理系が走らない
  ファイルシステムのためにある。区切りは `/` 固定。
- **`pathname` 関数は `to-pathname`**。型とトレイト・関数が同じ名前空間を共有するため。
- **`truename` / `file-write-date` / `directory` は無い**（ファイルシステムへの問い合わせ層は
  `probe-file` だけ）。
