# ペア・S 式・シーケンス

ジェネリックなペア `cons-cell`、S 式データ `Sexpr`、シンボル、`Iter` の上に書かれたシーケンス
関数、高階関数。

## 1. ペア `cons-cell<A,B>`

`cons`/`car`/`cdr` は**ジェネリックなペア型 `cons-cell<A,B>`**（標準ライブラリの `defstruct`）の
コンストラクタとフィールドアクセサ。フィールドは `変数::car`/`変数::cdr`
（[構文リファレンス](../syntax.md#36-defstruct--構造体ユーザ定義型)の `defstruct` アクセサ構文）でも
`(car 変数)`/`(cdr 変数)` でも読める。書き換えるには `(setf 変数::car v)`/`(setf 変数::cdr v)` を使う。

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `cons` | `(cons a b)` | `(A,B)→cons-cell<A,B>` | ペアを作る |
| `car` | `(car p)` | `cons-cell<A,B>→A` | 先頭 |
| `cdr` | `(cdr p)` | `cons-cell<A,B>→B` | 残り |

`cons-cell` はタプル構文の代わりにも使われる。多値を返す CL の関数（`floor` の商と剰余、
`read-from-string` の値と位置など）は、この言語では `cons-cell` を返す。

## 2. S 式データ `Sexpr`

`read` が返すデータ型 `Sexpr` は 19 の変種を持つ:
`int | i8 | i16 | i32 | u8 | u16 | u32 | f32 | f64 | char | bool | sym | str | cons | ratio | path | vector | array | tuple`。
`vector` と `array` は `#(..)` と `#nA(..)` で書いたデータ（[構文リファレンス](../syntax.md#1-字句要素)）で、
中身はそれぞれ `Vector<Option<Sexpr>>` と `Array<Option<Sexpr>>`——`(vector v)` で束縛した `v` には
`len` や `get` がそのまま使える。`tuple` は `#{..}` で書いたデータで、`(tuple v)` で束縛した `v` は
要素を並べた新しい `Vector<Option<Sexpr>>` になる（要素数がいくつでも同じ型で受け取るため）。
S 式のセルを扱うのは、1 章の汎用 `cons`/`car`/`cdr` ではなく `sexpr-*` 関数である。主に
`defmacro` の本体でフォームを組み立て・分解するときに使う。

**S 式データの型は `Option<Sexpr>` である。** 空リストは `Sexpr` の変種ではなく
`Option` の `none` であり、`Sexpr` そのものは「空でない S 式」を意味する。
したがって `sexpr-*` は引数も戻り値も `Option<Sexpr>` を取る。

- `()` は `Option<Sexpr>` が期待される位置で空リストになる（`(Option::none)` とも書ける）
- `Sexpr` は `Option<Sexpr>` が期待される位置へ暗黙に広がる（実行時の変換は無い）。
  逆向き——`Option<Sexpr>` を `Sexpr` として使う——は「空リストではない」の主張なので、
  `match` か `unwrap` で明示的に示す必要がある
- `match` では `Sexpr` の 19 変種と `none` を**同じ腕の並びに平らに**書ける
  （[構文リファレンス](../syntax.md#43-match--パターンマッチ)）

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `sexpr-cons` | `(sexpr-cons a b)` | `(Option<Sexpr>,Option<Sexpr>)→Option<Sexpr>` | `Sexpr` セルを作る |
| `sexpr-car` | `(sexpr-car s)` | `Option<Sexpr>→Option<Sexpr>` | 先頭。**空リストなら空リスト**（CL 準拠）。`Cons` でない原子は panic |
| `sexpr-cdr` | `(sexpr-cdr s)` | `Option<Sexpr>→Option<Sexpr>` | 残り。**空リストなら空リスト**（CL 準拠）。`Cons` でない原子は panic |
| `sexpr-consp` | `(sexpr-consp s)` | `Option<Sexpr>→bool` | `Cons` かどうか |
| `sexpr-null` | `(sexpr-null s)` | `Option<Sexpr>→bool` | 空リストかどうか |
| `sexpr-atom` | `(sexpr-atom s)` | `Option<Sexpr>→bool` | `Cons` でないか |
| `sexpr-symp` | `(sexpr-symp s)` | `Option<Sexpr>→bool` | `Sym`（シンボル）かどうか |
| `sexpr-int` | `(sexpr-int s)` | `Option<Sexpr>→int` | `int` 変種の中身（fixnum でも多倍長でも）。型違いは panic |
| `sexpr-i8` `sexpr-i16` `sexpr-i32` `sexpr-u8` `sexpr-u16` `sexpr-u32` | `(sexpr-i32 s)` | `Option<Sexpr>→W` | その幅の変種の中身。型違いは panic |
| `sexpr-f32` `sexpr-f64` | `(sexpr-f64 s)` | `Option<Sexpr>→f32` / `→f64` | 浮動小数点の変種の中身。型違いは panic |
| `sexpr-char` | `(sexpr-char s)` | `Option<Sexpr>→char` | `Char` の中身。型違いは panic |
| `sexpr-bool` | `(sexpr-bool s)` | `Option<Sexpr>→bool` | `Bool` の中身。型違いは panic |
| `sexpr-str` | `(sexpr-str s)` | `Option<Sexpr>→string` | `Str` の中身。型違いは panic |
| `sexpr-sym-name` | `(sexpr-sym-name s)` | `Option<Sexpr>→string` | `Sym` の名前。型違いは panic |
| `eq` `eql` | `(op a b)` | `(Option<Sexpr>,Option<Sexpr>)→bool` | 同一性比較（`Cons`/`Str` はオブジェクトの同一性、それ以外は値） |
| `equal` | `(equal a b)` | `(Option<Sexpr>,Option<Sexpr>)→bool` | 構造的等価（`Cons` は再帰、`Str` は内容比較） |
| `equalp` | `(equalp a b)` | `(Option<Sexpr>,Option<Sexpr>)→bool` | `equal` に加え大文字小文字無視・数値の型跨ぎ比較 |
| `sexpr-append` | `(sexpr-append a b)` | `(Option<Sexpr>,Option<Sexpr>)→Option<Sexpr>` | 2つの `Sexpr` リストを連結（非破壊）。`,@` はこれへ展開される |
| `sexpr-map` | `(sexpr-map f lst)` | `((fn (Option<Sexpr>) Option<Sexpr>),Option<Sexpr>)→Option<Sexpr>` | `Sexpr` リストの各要素へ `f` を適用した新しい `Sexpr` リスト（4 章の `map` は `Iter` 用で、`Sexpr` のリストは回せない） |

数値のアクセサが型ごとに 9 本あるのは、`Sexpr` が「値の型がほかのどこにも書かれていない
唯一の場所」だから。`Sexpr` に入れた `u8` は `u8` の変種として入り、`(sexpr-u8 s)` でしか
出てこない。`(sexpr-int s)` に渡せば panic する——黙って幅を広げて答えることはしない。
読んだデータ（`'(1 2 3)`、マクロの引数）の整数は `int` 変種で、`(sexpr-int s)` で読む。

`Sexpr` のリストには `rplaca`/`nconc` のような破壊的操作が無い。`Sexpr` のセルは作った後に
書き換えられない。

## 3. シンボル

`symbol` はシンボルそのものの型。`Sexpr` が要求される文脈へは暗黙に変換されるが、逆向きの
自動変換はない。

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `symbol->string` | `(symbol->string s)` | `symbol→string` | シンボル名を取り出す |
| `string->symbol` | `(string->symbol s)` | `string→symbol` | 文字列からシンボルを作る（intern） |
| `keywordp` | `(keywordp s)` | `symbol→bool` | キーワード（`:name`）か。コロンは名前の一部なので判定は先頭文字（[構文リファレンス](../syntax.md#1-字句要素)） |

`gensym` は [マクロ](system.md#8-マクロ) を参照。

## 4. `Iter` 上のシーケンス関数

シーケンス関数は **`Iter` トレイト上のジェネリック関数**である。コレクションからは
`(iter coll)` でイテレータを得て渡す（`Vector<T>` / `HashTable<K,V>` / `Array<T>` が対応。
`Sexpr` のリストは `Iter` を実装しないので、これらの関数の対象にはならない）。**結果のコレクションは
新しい `Vector` として返る**。表中の `Iter<A>` は「`Item` が `A` の任意の `Iter` 実装型」を表す。
返ってきた `Vector` を再び回すには `(iter result)` を渡す。

述語を取る関数（CL の `-if` 系に対応）:

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `map` | `(map it f)` | `(Iter<A>,(fn (A) U))→Vector<U>` | 写像 |
| `filter` | `(filter it pred)` | `(Iter<A>,(fn (A) bool))→Vector<A>` | 条件を満たす要素のみ |
| `remove-if` | `(remove-if it pred)` | `(Iter<A>,(fn (A) bool))→Vector<A>` | 条件を満たす要素を除く |
| `find-if` | `(find-if it pred)` | `(Iter<A>,(fn (A) bool))→Option<A>` | 条件を満たす最初の要素 |
| `position-if` | `(position-if it pred)` | `(Iter<A>,(fn (A) bool))→Option<int>` | 条件を満たす最初の位置 |
| `count-if` | `(count-if it pred)` | `(Iter<A>,(fn (A) bool))→int` | 条件を満たす個数 |
| `every` | `(every it pred)` | `(Iter<A>,(fn (A) bool))→bool` | 全要素が条件を満たすか |
| `any` | `(any it pred)` | `(Iter<A>,(fn (A) bool))→bool` | いずれかが条件を満たすか（CL の `some` に当たる。`Some` 構成子と衝突しない名前） |
| `foldl` | `(foldl it f init)` | `(Iter<A>,(fn (B A) B),B)→B` | 左畳み込み |
| `foldr` | `(foldr it f init)` | `(Iter<A>,(fn (A B) B),B)→B` | 右畳み込み |

添字・長さ・スライス:

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `length` | `(length it)` | `Iter<A>→int` | 要素数 |
| `append` | `(append a b ...)` | `(Iter<A>,Iter<A>,...)→Vector<A>` | イテレータを連結。3 個以上も書ける |
| `concatenate` | `(concatenate 'vector it ...)` / `(concatenate 'string s ...)` | `→Vector<A>` / `→string` | CL の `concatenate`。結果の型は**引用したシンボルのリテラル**で書く（CL は実行時の型指定子）。`'vector` は 1 個以上、`'string` は 0 個以上（0 個なら `""`）。`Sexpr` のリストは対象外（`sexpr-append`） |
| `reverse` | `(reverse it)` | `Iter<A>→Vector<A>` | 反転（非破壊） |
| `nth` | `(nth n it)` | `(int,Iter<A>)→Option<A>` | `n` 番目の要素（範囲外は `None`） |
| `elt` | `(elt it n)` | `(Iter<A>,int)→Option<A>` | `nth` の引数順違い版 |
| `take` | `(take it n)` | `(Iter<A>,int)→Vector<A>` | 先頭 `n` 個 |
| `subseq` | `(subseq it start end)` | `(Iter<A>,int,int)→Vector<A>` | `[start,end)`（`end` は長さでクランプ） |
| `last` | `(last it)` | `Iter<A>→Option<A>` | 最後の**要素**（CL の「最後のセル」ではない） |
| `butlast` | `(butlast it)` | `Iter<A>→Vector<A>` | 最後の要素を除く |

`Eq` / `Ord` 境界を要求する関数（述語の代わりにトレイトで比較する。[トレイト](traits.md#2-eq--ord比較)）:

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `member` | `(member x it)` | `(A,Iter<A>)→bool` where `Eq A` | `x` と等しい要素があるか（CL と違い残りリストではなく `bool`） |
| `find` | `(find x it)` | `(A,Iter<A>)→Option<A>` where `Eq A` | `x` と等しい最初の要素 |
| `position` | `(position x it)` | `(A,Iter<A>)→Option<int>` where `Eq A` | `x` と等しい最初の位置 |
| `count` | `(count x it)` | `(A,Iter<A>)→int` where `Eq A` | `x` と等しい要素の個数 |
| `sort` | `(sort it cmp)` | `(Iter<A>,(fn (A A) bool))→Vector<A>` | CL の `(sort sequence predicate)`。安定な非破壊ソート。`cmp` は「第1引数が第2引数より真に前」で `true` |
| `assoc` | `(assoc k it)` | `(K,Iter<cons-cell<K,V>>)→Option<cons-cell<K,V>>` where `Eq K` | `car` が `k` と等しい最初のペア。値は `(cdr p)` で取り出す |

これらと 5 章の関数の多くは、CL のキーワード引数 `:key` / `:test` / `:test-not` /
`:start` / `:end` / `:from-end` / `:count` も取る（6 章）。

## 5. CL のシーケンス関数の残り

すべて 4 章と同じ `Iter` 上のジェネリック関数。結果のコレクションは新しい `Vector` として返る。

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `first`…`tenth` | `(first it)` | `Iter<A>→Option<A>` | CL の名前付き添字 |
| `rest` | `(rest it)` | `Iter<A>→Vector<A>` | 先頭を除いた残り（共有される tail ではなく新しい `Vector`） |
| `copy-seq` | `(copy-seq it)` | `Iter<A>→Vector<A>` | イテレータを `Vector` に実体化（CL `copy-seq`/`copy-list`） |
| `revappend` | `(revappend a b)` | `(Iter<A>,Iter<A>)→Vector<A>` | `a` を反転して `b` を続ける |
| `Vector::filled` | `(Vector::filled n x)` | `(int,T)→Vector<T>` | `x` を `n` 個（CL `make-list`/`make-sequence`）。`Vector::new` と同じく型引数は期待型から来るので、裸の `let` には `the` が要る |
| `member-if` `member-if-not` | `(member-if it pred)` | `(Iter<A>,(fn (A) bool))→bool` | `member` と同じく **`bool`**（イテレータに返すべき tail が無い） |
| `notany` `notevery` | `(notany it pred)` | `(Iter<A>,(fn (A) bool))→bool` | `any`/`every` の否定 |
| `find-if-not` `position-if-not` `count-if-not` `remove-if-not` | `(op it pred)` | 各正版と同型 | 述語を否定した版 |
| `remove` | `(remove x it)` | `(A,Iter<A>)→Vector<A>` where `Eq A` | 値で削除 |
| `remove-duplicates` | `(remove-duplicates it)` | `Iter<A>→Vector<A>` where `Eq A` | 重複除去。CL どおり**最後の出現を残す**（最初の出現を残すには `:from-end true`） |
| `substitute` `substitute-if` | `(substitute new old it)` | `(A,A,Iter<A>)→Vector<A>` | 値／述語で置換 |
| `assoc-if` `rassoc` `rassoc-if` | `(rassoc v it)` | `Iter<cons-cell<K,V>>` 上 | `assoc` の述語版・値側版 |
| `acons` | `(acons k v it)` | `(K,V,Iter<cons-cell<K,V>>)→Vector<cons-cell<K,V>>` | 先頭にペアを足す |
| `pairlis` | `(pairlis ks vs)` | `(Iter<K>,Iter<V>)→Vector<cons-cell<K,V>>` | 2列を組にする。短い方で止まる |
| `map2` | `(map2 a b f)` | `(Iter<A>,Iter<B>,(fn (A B) U))→Vector<U>` | CL の複数シーケンス `mapcar`。短い方で止まる |
| `mapc` | `(mapc it f)` | `(Iter<A>,(fn (A) ()))→()` | 副作用のための写像 |
| `mapcan` | `(mapcan it f)` | `(Iter<A>,(fn (A) Vector<U>))→Vector<U>` | 写像して連結 |
| `maplist` | `(maplist it f)` | `(Iter<A>,(fn (Vector<A>) U))→Vector<U>` | 連続する**末尾**への写像 |
| `mapl` | `(mapl it f)` | `(Iter<A>,(fn (Vector<A>) ()))→()` | 末尾への副作用のための写像（`maplist` 版の `mapc`） |
| `mapcon` | `(mapcon it f)` | `(Iter<A>,(fn (Vector<A>) Vector<U>))→Vector<U>` | 末尾へ写像して連結（`maplist` 版の `mapcan`） |
| `search` | `(search it sub)` | `(Iter<A>,Iter<A>)→Option<int>` where `Eq A` | `sub` が最初に現れる位置。受け手が `string` なら `string` のメソッドが選ばれる（[文字列](collections.md#1-文字列-string)） |
| `mismatch` | `(mismatch a b)` | `(Iter<A>,Iter<A>)→Option<int>` where `Eq A` | 最初に食い違う位置。等しければ `none` |
| `merge` | `(merge a b less)` | `(Iter<A>,Iter<A>,(fn (A A) bool))→Vector<A>` | 併合。CL は整列済みを要求するが、これは連結を整列する |
| `adjoin` | `(adjoin x it)` | `(A,Iter<A>)→Vector<A>` where `Eq A` | 無ければ**先頭に**足す |
| `union` `intersection` `set-difference` `set-exclusive-or` | `(op a b)` | `(Iter<A>,Iter<A>)→Vector<A>` where `Eq A` | 集合演算。CL は順序を規定しないが、ここは**初出順**で安定 |
| `subsetp` | `(subsetp a b)` | `(Iter<A>,Iter<A>)→bool` where `Eq A` | 包含 |
| `tailp` `ldiff` | `(tailp tail whole)` | `(Iter<A>,Iter<A>)→bool` / `→Vector<A>` | 接尾辞か／接尾辞を除いた前半。CL は**構造の共有**を問うが、共有すべき構造が無いので**値として**の接尾辞を問う |
| `seq-equals` | `(seq-equals a b)` | `(Vector<A>,Vector<A>)→bool` where `Eq A` | 要素ごとの等価。`Vector<T>` 自身に `Eq` の実装は無い |
| `caar`…`cddddr` | `(cadr p)` | ネストしたペア上 | CL の 28 個。**リストではなくペア**の走査で、`cadr` は `cons-cell<A,cons-cell<B,C>>` を取る |

CL にあってここに無いもの: `list*`（末尾を差し替えた不完全リストという概念が無い）、
`copy-tree`/`copy-alist`/`sublis`/`subst`/`subst-if`（任意深さの異種の木を走査する型が書けない。
`Sexpr` の木としてなら `equal` が `tree-equal` に当たる）、
プロパティリスト一式 `getf`/`get-properties`/`symbol-plist`/`remprop`（キーと値が交互に並ぶ
無型のリストという表現が無い。同じ役割は `assoc`（連想リスト）か `HashTable` が担う）、
`Vector<T>` と `Sexpr` のリストを相互変換する関数（`Sexpr` のリストは要素ごとに型が違いうるので、
単一の要素型 `T` からは書けない）。

## 6. キーワード引数

CL のシーケンス関数が取るキーワード `:key` / `:test` / `:test-not` / `:start` / `:end` /
`:from-end` / `:count` を、4 章と 5 章の関数が取る。すべて**省略可能**。

| キーワード | 型 | 意味 |
|---|---|---|
| `:key` | `(fn (A) A)` | 比較・述語にかける前に要素へ適用する射影 |
| `:test` | `(fn (A A) bool)` | `Eq` 境界の `equals` の代わりに使う等価判定。第1引数が**探している項目**、第2引数が（`:key` 適用後の）要素——CL と同じ順 |
| `:test-not` | `(fn (A A) bool)` | `:test` の否定 |
| `:start` `:end` | `int` | 走査する窓 `[start, end)`。添字は列全体に対するもの |
| `:from-end` | `bool` | 探索は**最後の**一致を答える。`:count` と併せると影響を受けるのは末尾側から |
| `:count` | `int` | `remove`／`substitute` 系が影響を与える最大個数 |

どの関数がどれを取るかは CL に従う:

| 関数 | 取るキーワード |
|---|---|
| `find` `position` | `:key` `:test` `:test-not` `:start` `:end` `:from-end` |
| `count` | `:key` `:test` `:test-not` `:start` `:end` |
| `member` `adjoin` | `:key` `:test` `:test-not` |
| `remove` `substitute` | 上の全部（`:count` を含む） |
| `remove-duplicates` | `:key` `:test` `:test-not` `:start` `:end` `:from-end` |
| `find-if` `find-if-not` `position-if` `position-if-not` | `:key` `:start` `:end` `:from-end` |
| `count-if` `count-if-not` | `:key` `:start` `:end` |
| `member-if` `member-if-not` | `:key` |
| `remove-if` `remove-if-not` `substitute-if` | `:key` `:start` `:end` `:from-end` `:count` |
| `assoc` `rassoc` | `:key` `:test` `:test-not`（`assoc` の `:key` は `car`、`rassoc` は `cdr` に掛かる） |
| `assoc-if` `rassoc-if` | `:key` |
| `sort` `merge` | `:key` |
| `union` `intersection` `set-difference` `set-exclusive-or` `subsetp` | `:key` `:test` `:test-not` |
| `search` | `:key` `:test` `:test-not` `:from-end` `:start` `:end` `:sub-start` `:sub-end` |
| `mismatch` | `:key` `:test` `:test-not` `:from-end` `:start1` `:end1` `:start2` `:end2` |

```lisp
(find 2 (iter v) :key (lambda ((x int)) int (abs x)))   ; → (some -2)
(remove 2 (iter v) :count 1 :from-end true)             ; 末尾側の 1 個だけ消す
(position 3 (iter v) :start 1)                          ; 添字は列全体に対するもの
```

**CL と違うところ**:

1. **`:key` の射影は要素型の中に閉じる**（`(fn (A) A)`）。CL のように別の型へ射影する
   ことはできない——型変数を増やすと省略時に決まらなくなるため。異なる型への射影が要る
   場面は `-if` 系にラムダを渡すほうで書ける（`(find-if it (lambda ((p ...)) bool (= (car p) 3)))`）。
2. **項目ベースの探索では `:key` は要素にだけ掛かる**（探している項目には掛からない）。
   CL の `find`/`position`/`count`/`member`/`remove`/`substitute` と同じ規則。集合演算では
   両辺とも要素なので両方に掛かる。
3. **`search` のキーワードだけ番号でなく名前**。CL は `:start1`/`:end1` が**パターン**、
   `:start2`/`:end2` が探される列だが、この言語は受け手が先なので同じ番号が逆の意味になる——
   しかも黙って。`:start`/`:end` が受け手、`:sub-start`/`:sub-end` がパターンなので、
   つい書いた `:start1` は「未知のキーワード」エラーになる。`mismatch` と `replace` は
   引数の順序が CL と一致するので CL の番号のまま。

## 7. 破壊的操作

`Vector<T>` のメソッド。**受け手を書き換えたうえで受け手自身を返す**ので、`(nreverse v)` は
`reverse` と同じ形で書けて `v` 自身も反転する。

| 名前 | 形式 | 説明 |
|---|---|---|
| `nreverse` | `(nreverse v)` | その場で反転 |
| `delete` `delete-if` `delete-if-not` `delete-duplicates` | `(delete v x)` | `remove`／`remove-if`／`filter`／`remove-duplicates` のその場版 |
| `nsubstitute` `nsubstitute-if` | `(nsubstitute v new old)` | `substitute` 系のその場版 |
| `nbutlast` | `(nbutlast v)` | 末尾を1つ落とす |
| `fill` | `(fill v x)` | 全要素を `x` に。長さは変えない |
| `replace` | `(replace v src)` | `src` の要素を先頭から上書き。`(min (len v) (len src))` 個 |
| `map-into` | `(map-into v src f)` | `v[i] = (f src[i])`。同上 |
| `nconc` | `(nconc v w)` | `w` の要素を `v` に追加。CL と違い**共有構造の書き換えではない**（`w` は影響を受けない） |
| `nreconc` | `(nreconc v w)` | `(nconc (nreverse v) w)` |
| `set-contents` | `(set-contents v src)` | `v` の中身を `src` で置き換える（長さも変わる） |
| `rplaca` `rplacd` | `(rplaca p x)` | `cons-cell` の `car`/`cdr` を書き換え、セル自身を返す |

取るキーワード:

| 破壊的な版 | 取るキーワード |
|---|---|
| `delete` `nsubstitute` | `:key` `:test` `:test-not` `:start` `:end` `:from-end` `:count` |
| `delete-if` `delete-if-not` `nsubstitute-if` | `:key` `:start` `:end` `:from-end` `:count` |
| `delete-duplicates` | `:key` `:test` `:test-not` `:start` `:end` `:from-end` |
| `fill` | `:start` `:end` |
| `replace` | `:start1` `:end1` `:start2` `:end2`（受け手が CL の `sequence-1`） |

`vector-push-extend`/`vector-pop` は `Vector<T>` の `push`/`pop` そのもの——`Vector<T>` は常に
伸びるので、CL の「fill pointer を持つベクタ」と「simple なベクタ」の区別に対応するものが無い。

## 8. 高階関数

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `identity` | `(identity x)` | `T→T` | そのまま返す |
| `const` | `(const x y)` | `(A,B)→A` | 第一引数を返す |
| `compose` | `(compose f g)` | `((fn (B) C),(fn (A) B))→(fn (A) C)` | 関数合成 `f∘g` |
| `flip` | `(flip f)` | `(fn (A B) C)→(fn (B A) C)` | 2引数関数の引数順を入れ替える |
| `complement` | `(complement pred)` | `(fn (A) bool)→(fn (A) bool)` | 述語の否定 |

CL の `constantly` は無い（無視する引数の型が戻り型にしか現れず決まらない）。
`(lambda ((x T)) A v)` と書く。
