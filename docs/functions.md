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
| `+` `-` `*` `/` `mod` | `(op a b)` | `(T,T)→T` | 四則演算。`/`/`mod` はゼロ除算で panic |
| `<` `<=` `>` `>=` `=` `/=` | `(op a b)` | `(T,T)→bool` | 比較 |
| `eq` `eql` `equal` `equalp` | `(op a b)` | `(T,T)→bool` | いずれも `=` と同じ（同型の数値に差はない） |
| `int->float` | `(int->float x)` | `T→f64` | `f64` への拡大変換 |
| `int->char` | `(int->char x)` | `T→char` | Unicode スカラ値として解釈。不正な値は panic |

`i8` `i16` `isize` `u8` `u16` `u32` `u64` `usize` `f32` は `defmethod` の受け手として型登録は
されているが、現時点では算術・比較を含め一切のメソッドを持たない。

## 2. 算術・比較（`f64`）

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `+` `-` `*` `/` `mod` | `(op a b)` | `(f64,f64)→f64` | IEEE-754。ゼロ除算は panic せず `inf`/`NaN` |
| `<` `<=` `>` `>=` `=` `/=` | `(op a b)` | `(f64,f64)→bool` | 比較 |
| `eq` `eql` `equal` `equalp` | `(op a b)` | `(f64,f64)→bool` | いずれも `=` と同じ |
| `expt` | `(expt a b)` | `(f64,f64)→f64` | 冪乗 |
| `sqrt` `floor` `ceiling` `round` `truncate` | `(op x)` | `f64→f64` | 単項演算 |
| `float->int` | `(float->int x)` | `f64→i32` | ゼロ方向への切り捨てで `i32` へ変換 |

## 3. 論理・真偽値

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `not` | `(not b)` | `bool→bool` | 否定 |
| `eq` `eql` `equal` `equalp` | `(op a b)` | `(bool,bool)→bool` | すべて値の等価比較 |

`and`/`or` は短絡評価が必要なため特殊形（[syntax.md](syntax.md) 参照）。

## 4. 数値ヘルパー（`i32` 専用のライブラリ関数）

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `abs` | `(abs x)` | `i32→i32` | 絶対値 |
| `gcd` | `(gcd a b)` | `(i32,i32)→i32` | 最大公約数 |
| `lcm` | `(lcm a b)` | `(i32,i32)→i32` | 最小公倍数（どちらかが0なら0） |
| `signum` | `(signum x)` | `i32→i32` | 符号（`1`/`-1`/`0`） |
| `random` | `(random n)` | `i32→i32` | `0` 以上 `n` 未満の乱数（自由関数） |

`min`/`max`/`evenp`/`oddp`/`zerop` などは未実装。

## 5. `Sexpr` / cons セル

`read` が返すデータ型 `Sexpr`（`Nil | Int | Float | Char | Bool | Sym | Str | Cons(Sexpr,Sexpr)`）
を直接操作する組み込み自由関数。

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `cons` | `(cons a b)` | `(Sexpr,Sexpr)→Sexpr` | セルを作る |
| `car` | `(car s)` | `Sexpr→Sexpr` | 先頭。`Cons` でなければ panic |
| `cdr` | `(cdr s)` | `Sexpr→Sexpr` | 残り。`Cons` でなければ panic |
| `set-car` | `(set-car s v)` | `(Sexpr,Sexpr)→Unit` | 破壊的更新（`Cons` でなければ panic） |
| `set-cdr` | `(set-cdr s v)` | `(Sexpr,Sexpr)→Unit` | 破壊的更新（`Cons` でなければ panic） |
| `eq` `eql` | `(op a b)` | `(Sexpr,Sexpr)→bool` | 同一性比較（`Cons`/`Str` はポインタ、それ以外は値） |

## 6. シーケンス操作（`Iter` 上のライブラリ関数）

Phase 6.5 の再設計で、旧来の `Sexpr` リスト用ライブラリは **`Iter` トレイト上のジェネリック
関数**へ作り直された。呼び出しはコレクションから `(iter coll)` でカーソルを得て渡す
（`Vector<T>` / `HashTable<K,V>` が `Iter` を実装。`Sexpr` のリストは `Iter` を実装しない
ので、これらの関数の対象にはならない）。旧 API との主な違いは、**コレクション/イテレータを
第一引数に取る**点と、**結果のコレクションは新しい `Vector` として返る**点。表中の `Iter<A>` は
「`Item` が `A` の任意の `Iter` 実装型」を表す。

`symbol->string` / `string->symbol` は `Sexpr::Sym` と `string` の橋渡し（`Sym` は内部の
`Symbol` 型を包む）:

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `symbol->string` | `(symbol->string s)` | `Sexpr→string` | `Sym` の名前を取り出す |
| `string->symbol` | `(string->symbol s)` | `string→Sexpr` | 文字列から `Sym` を作る |
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
| `iter` | `(iter v)` | `Vector<T>→vector-iter<T>` | `Iter` を実装するカーソルを作る（ライブラリ定義） |

`pop`/`map`/`filter`/リスト変換などは未実装。

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
| `gensym` | `(gensym)` | `()→Sexpr` | 衝突耐性のある新しいシンボルを返す（マクロ用） |
| `exit` | `(exit code)` | `i32→!` | プロセスを終了する |

`compile`/`compile-file` は [syntax.md](syntax.md) の「コンパイル」節を参照。
