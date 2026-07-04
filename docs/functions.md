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

## 6. リスト操作（`Sexpr` 上のライブラリ関数）

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `consp` | `(consp s)` | `Sexpr→bool` | `Cons` か |
| `null` | `(null s)` | `Sexpr→bool` | `Nil` か |
| `atom` | `(atom s)` | `Sexpr→bool` | `Cons` でないか |
| `symbol->string` | `(symbol->string s)` | `Sexpr→string` | `Sym` の名前を取り出す |
| `string->symbol` | `(string->symbol s)` | `string→Sexpr` | 文字列から `Sym` を作る |
| `equal` | `(equal a b)` | `(Sexpr,Sexpr)→bool` | 構造的等価（`Cons` は再帰、`Str` は内容比較） |
| `equalp` | `(equalp a b)` | `(Sexpr,Sexpr)→bool` | `equal` に加え大文字小文字無視・数値の型跨ぎ比較 |
| `length` | `(length lst)` | `Sexpr→i32` | 長さ（不正なリストは panic） |
| `append` | `(append a b)` | `(Sexpr,Sexpr)→Sexpr` | 非破壊的連結 |
| `reverse` | `(reverse lst)` | `Sexpr→Sexpr` | 非破壊的反転 |
| `nthcdr` | `(nthcdr n lst)` | `(i32,Sexpr)→Sexpr` | 先頭 `n` 個を落とした残り |
| `nth` | `(nth n lst)` | `(i32,Sexpr)→Sexpr` | `n` 番目の要素 |
| `elt` | `(elt lst n)` | `(Sexpr,i32)→Sexpr` | `nth` の引数順違い版 |
| `last` | `(last lst)` | `Sexpr→Sexpr` | 最後のセル |
| `butlast` | `(butlast lst)` | `Sexpr→Sexpr` | 最後を除いた要素 |
| `take` | `(take n lst)` | `(i32,Sexpr)→Sexpr` | 先頭 `n` 個 |
| `subseq` | `(subseq lst start end)` | `(Sexpr,i32,i32)→Sexpr` | `[start,end)` の部分リスト |
| `copy-list` | `(copy-list lst)` | `Sexpr→Sexpr` | 浅いコピー |
| `member` | `(member item lst)` | `(Sexpr,Sexpr)→Sexpr` | `eq` で一致する最初の要素からの残り、なければ `()` |
| `find-if` | `(find-if pred lst)` | `((fn (Sexpr) bool),Sexpr)→Sexpr` | 条件を満たす最初の要素、なければ `()` |
| `every` | `(every pred lst)` | `((fn (Sexpr) bool),Sexpr)→bool` | 全要素が条件を満たすか |
| `any` | `(any pred lst)` | `((fn (Sexpr) bool),Sexpr)→bool` | いずれかが条件を満たすか（CL の `some` 相当。`Some` 構成子との名前衝突を避けた名前） |
| `count-if` | `(count-if pred lst)` | `((fn (Sexpr) bool),Sexpr)→i32` | 条件を満たす個数 |
| `count` | `(count item lst)` | `(Sexpr,Sexpr)→i32` | `eq` で一致する個数 |
| `position-if` | `(position-if pred lst)` | `((fn (Sexpr) bool),Sexpr)→Option<i32>` | 条件を満たす最初の位置 |
| `position` | `(position item lst)` | `(Sexpr,Sexpr)→Option<i32>` | `eq` で一致する最初の位置 |
| `remove-if` | `(remove-if pred lst)` | `((fn (Sexpr) bool),Sexpr)→Sexpr` | 条件を満たす要素を除く |
| `remove-if-not` | `(remove-if-not pred lst)` | `((fn (Sexpr) bool),Sexpr)→Sexpr` | 条件を満たす要素のみ残す（filter と同じ） |
| `remove` | `(remove item lst)` | `(Sexpr,Sexpr)→Sexpr` | `eq` で一致する要素を除く |
| `map` | `(map f lst)` | `((fn (Sexpr) Sexpr),Sexpr)→Sexpr` | 写像 |
| `filter` | `(filter pred lst)` | `((fn (Sexpr) bool),Sexpr)→Sexpr` | 条件を満たす要素のみ |
| `foldl` | `(foldl f init lst)` | `((fn (Sexpr Sexpr) Sexpr),Sexpr,Sexpr)→Sexpr` | 左畳み込み |
| `foldr` | `(foldr f init lst)` | `((fn (Sexpr Sexpr) Sexpr),Sexpr,Sexpr)→Sexpr` | 右畳み込み |
| `nconc` | `(nconc a b)` | `(Sexpr,Sexpr)→Sexpr` | 破壊的連結（`a` の最後のセルを書き換える） |
| `nreverse` | `(nreverse lst)` | `Sexpr→Sexpr` | 破壊的反転 |
| `sort` | `(sort cmp lst)` | `((fn (Sexpr Sexpr) bool),Sexpr)→Sexpr` | 挿入ソート（非破壊的） |
| `assoc` | `(assoc key alist)` | `(Sexpr,Sexpr)→Sexpr` | 連想リストから `car` が `key` と `eq` なペアを探す |

`(list e1 e2 ... en)` は特殊形（`(cons e1 (cons e2 (... (Nil))))` へ展開、[syntax.md](syntax.md) 参照）。

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
| `lt` | `(lt s1 s2)` | `(string,string)→bool` | 辞書順比較 |
| `eq` `eql` | `(op s1 s2)` | `(string,string)→bool` | 同一性比較（内容ではなく参照） |
| `equal` | `(equal s1 s2)` | `(string,string)→bool` | 内容比較（大文字小文字を区別） |
| `equalp` | `(equalp s1 s2)` | `(string,string)→bool` | 内容比較（大文字小文字を無視、ASCII のみ） |

## 9. 文字 (`char`)

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `upcase` | `(upcase c)` | `char→char` | 大文字化（ASCII のみ） |
| `downcase` | `(downcase c)` | `char→char` | 小文字化（ASCII のみ） |
| `lt` | `(lt c1 c2)` | `(char,char)→bool` | 比較 |
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
