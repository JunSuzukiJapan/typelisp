# 文字列・文字・コレクション

`string`、`char`、`Vector<T>`、`HashTable<K,V>`、`Array<T>`、`BitVector`。

## 1. 文字列 `string`

文字列は不変。

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `upcase` | `(upcase s)` | `string→string` | 大文字化（ASCII のみ）。CL の `string-upcase` と同じく新しい文字列を返す。文字列は不変なので破壊版 `nstring-upcase` は無く、これが代わりになる |
| `downcase` | `(downcase s)` | `string→string` | 小文字化（ASCII のみ）。`nstring-downcase` の代わり |
| `capitalize` | `(capitalize s)` | `string→string` | 各語の先頭を大文字・残りを小文字（CL `string-capitalize`）。語＝英数字の極大連続 |
| `length` | `(length s)` | `string→int` | 文字数 |
| `ref` | `(ref s i)` | `(string,int)→char` | `i` 番目の文字。範囲外は panic |
| `substring` | `(substring s start end)` | `(string,int,int)→string` | 部分文字列 `[start,end)` |
| `append` | `(append s1 s2 ...)` | `(string,string,...)→string` | 連結。3 個以上も書ける（`(concatenate 'string ...)` と同じ） |
| `<` `<=` `>` `>=` | `(op s1 s2)` | `(string,string)→bool` | 辞書順比較 |
| `lt` | `(lt s1 s2)` | `(string,string)→bool` | 辞書順の狭義小なり（`<` と同じ） |
| `eq` `eql` | `(op s1 s2)` | `(string,string)→bool` | 同一性比較（内容ではなく同じオブジェクトか） |
| `equal` | `(equal s1 s2)` | `(string,string)→bool` | 内容比較（大文字小文字を区別） |
| `equalp` | `(equalp s1 s2)` | `(string,string)→bool` | 内容比較（大文字小文字を無視、ASCII のみ） |
| `/=` | `(/= s1 s2)` | `(string,string)→bool` | 内容が異なるか（CL `string/=`。可変長形は隣接ペア比較） |
| `lessp` `greaterp` `not-lessp` `not-greaterp` | `(op s1 s2)` | `(string,string)→bool` | 大文字小文字を無視した順序比較（CL `string-lessp` 等）。共通接頭辞なら短い方が小 |
| `string::filled` | `(string::filled n c)` | `(int,char)→string` | `c` を `n` 個並べた文字列（CL `make-string`） |
| `search` | `(search s sub)` | `(string,string)→Option<int>` | `sub` が最初に現れる位置。**CL の `search` は引数順が逆**（`(search pattern sequence)`）。空文字列は 0。キーワードは [シーケンスのキーワード引数](sequences.md#6-キーワード引数) |
| `mismatch` | `(mismatch a b)` | `(string,string)→Option<int>` | 最初に食い違う位置。`equal` なときだけ `none`。片方が接頭辞なら短い方の末尾。キーワードは同上 |
| `trim` `left-trim` `right-trim` | `(trim s)` / `(trim s bag)` | `(string &optional string)→string` | 両端/左/右から `bag` に含まれる文字を除く（CL `string-trim` 等）。`bag` 省略時は空白類 `" \t\n\r"` |
| `split` | `(split s sep)` | `(string,string)→Vector<string>` | `sep` で分割。CL に対応物は無い。連続する区切りは空要素を生む。`sep` が空なら panic |
| `to-string` | `(to-string x)` | `T→string` | `~a` 相当の文字列化。`int`/`i32`/`f64`/`bool`/`char`/`string` に実装（CL `princ-to-string`） |
| `string->utf8` | `(string->utf8 s)` | `string→Vector<int>` | UTF-8 に符号化（各要素 0..255） |
| `utf8->string` | `(utf8->string bytes)` | `Vector<int>→Option<string>` | 復号。正しい UTF-8 でなければ `none` |

## 2. 文字 `char`

`char` は Unicode スカラ値。大文字小文字の変換と分類は ASCII の範囲だけを扱う。

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `upcase` | `(upcase c)` | `char→char` | 大文字化（ASCII のみ） |
| `downcase` | `(downcase c)` | `char→char` | 小文字化（ASCII のみ） |
| `<` `<=` `>` `>=` | `(op c1 c2)` | `(char,char)→bool` | コードポイント順比較 |
| `lt` | `(lt c1 c2)` | `(char,char)→bool` | コードポイント順の狭義小なり（`<` と同じ） |
| `alphap` | `(alphap c)` | `char→bool` | ASCII アルファベットか |
| `digitp` | `(digitp c)` | `char→bool` | ASCII 数字か |
| `eq` `eql` `equal` | `(op c1 c2)` | `(char,char)→bool` | 値の比較 |
| `equalp` | `(equalp c1 c2)` | `(char,char)→bool` | 大文字小文字を無視した値の比較（CL `char-equal`） |
| `/=` | `(/= c1 c2)` | `(char,char)→bool` | 値が異なるか（CL `char/=`。**可変長形は隣接ペア比較**で、全ペア相異を問う CL とは異なる） |
| `lessp` `greaterp` `not-lessp` `not-greaterp` | `(op c1 c2)` | `(char,char)→bool` | 大文字小文字を無視した順序比較（CL `char-lessp` 等） |
| `upper-casep` `lower-casep` `both-casep` | `(op c)` | `char→bool` | 大文字か/小文字か/そもそも大小の別を持つか（CL `upper-case-p` 等） |
| `alphanumericp` | `(alphanumericp c)` | `char→bool` | 英字または数字か（CL 同名） |
| `graphicp` | `(graphicp c)` | `char→bool` | 印字可能か。空白は含み、改行・タブは含まない（CL `graphic-char-p`） |
| `standardp` | `(standardp c)` | `char→bool` | CL の標準文字 96 個か＝`graphicp` に改行を足したもの（CL `standard-char-p`） |
| `char->int` | `(char->int c)` | `char→int` | Unicode スカラ値（逆方向は [数値](numbers.md#1-固定幅整数)の `int->char`/`try-int->char`）。CL の `char-code`/`char-int` に当たる |
| `char->string` | `(char->string c)` | `char→string` | 1文字だけの文字列。CL は `string` 関数が指定子を取って兼ねるが、この言語には指定子が無いので向きを名前に出している |
| `digit-weight` | `(digit-weight c)` / `(digit-weight c radix)` | `(char &optional int)→Option<int>` | その基数での数字の**重み**（CL `digit-char-p`）。`digitp` は `bool` を返す別の関数 |
| `digit->char` | `(digit->char w)` / `(digit->char w radix)` | `(int &optional int)→Option<char>` | 重み `w` を表す文字。10 以上は大文字（CL `digit-char`。基数は最大 36） |
| `char->name` | `(char->name c)` | `char→Option<string>` | 文字名。名前を持つのはリーダが読める名前付き文字だけ（CL `char-name`） |
| `name->char` | `(name->char s)` | `string→Option<char>` | 文字名から文字。大文字小文字を無視し、リーダの別名（`linefeed`/`null`）も受ける（CL `name-char`） |

`char-code-limit` に当たる定数は無い（`char` の上限は言語ではなく Unicode が決める）。

## 3. `Vector<T>`

可変長配列。

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `new` | `(Vector::new)` | `()→Vector<T>` | 空のベクタを作る。型引数は期待型から決まるので、裸の `let` では `(the Vector<i32> (Vector::new))` と書く |
| `filled` | `(Vector::filled n x)` | `(int,T)→Vector<T>` | `x` を `n` 個 |
| `push` | `(push v x)` | `(Vector<T>,T)→Unit` | 末尾に追加 |
| `get` | `(get v i)` | `(Vector<T>,int)→T` | `i` 番目を読む。範囲外は panic |
| `set` | `(set v i x)` | `(Vector<T>,int,T)→Unit` | `i` 番目を書き換える。範囲外は panic。`(setf (get v i) x)` とも書ける |
| `len` | `(len v)` | `Vector<T>→int` | 要素数 |
| `pop` | `(pop v)` | `Vector<T>→Option<T>` | 末尾を取り除いて返す。空なら `None`（`get`/`set` と異なり panic しない） |
| `iter` | `(iter v)` | `Vector<T>→vector-iter<T>` | `Iter` を実装するイテレータを作る |
| `pushnew` | `(pushnew v x)` | `(Vector<T>,T)→Unit` where `Eq T` | `x` と等しい要素が無ければ末尾に追加（CL `pushnew`。場所を書き換える必要が無いのでマクロではなくメソッド） |

`map`/`filter` などは [シーケンス関数](sequences.md#4-iter-上のシーケンス関数)——`(map (iter v) f)` の
ように `iter` で渡す。破壊的な操作（`nreverse`、`delete` など）は [破壊的操作](sequences.md#7-破壊的操作)。

## 4. `HashTable<K,V>`

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `new` | `(HashTable::new)` | `()→HashTable<K,V>` | 空のテーブルを作る |
| `get` | `(get h k)` | `(HashTable<K,V>,K)→Option<V>` | 検索 |
| `set` | `(set h k v)` | `(HashTable<K,V>,K,V)→Unit` | 挿入・上書き |
| `remove` | `(remove h k)` | `(HashTable<K,V>,K)→Option<V>` | 削除し、あれば旧値を返す |
| `count` | `(count h)` | `HashTable<K,V>→int` | 要素数 |
| `clear` | `(clear h)` | `HashTable<K,V>→Unit` | 全削除 |
| `keys` | `(keys h)` | `HashTable<K,V>→Vector<K>` | キーのスナップショット |
| `values` | `(values h)` | `HashTable<K,V>→Vector<V>` | 値のスナップショット |
| `entries` | `(entries h)` | `HashTable<K,V>→Vector<cons-cell<K,V>>` | `(k . v)` ペアのスナップショット |
| `iter` | `(iter h)` | `HashTable<K,V>→hashtable-iter<K,V>` | `Iter` を実装するイテレータ。要素は `(k . v)` の `cons-cell`。CL の `with-hash-table-iterator` に当たり、`doiter`/`map`/`filter` などがそのまま使える |
| `maphash` | `(maphash h f)` | `(HashTable<K,V>,(fn (K V) ()))→Unit` | CL `maphash` |
| `size` | `(size h)` | `HashTable<K,V>→int` | CL `hash-table-size`。この表では占有数（＝`count`） |

**キーの型は `Hash` を実装していれば何でもよい**——`defstruct`/`defenum` も含めて。
`get`/`set`/`remove` は `(where (Hash K))` を持つので、実装していない型をキーにした表は
**型エラー**（`f64` に `Hash` が無いのは `NaN` のため）。

```lisp
(deftrait Hash (Eq)
  (sxhash ((self Self)) int))              ; 非負で fixnum に入る値を返す
```

実装済み: `int` と 6 つの固定幅整数、`bool`、`char`、`string`、`symbol`（浮動小数点数には無い）。
自前の型では、結果を `*sxhash-mask*`（2^30-1）で `logand` して非負に保つ。文字列を
ハッシュしたいときは `string` の実装が使っている `(sxhash-string s)`（FNV-1a 32bit）を呼べる。

```lisp
(defstruct point (x int) (y int))
(impl Eq point
  (equals ((self Self) (other Self)) bool
    (if (= self::x other::x) (= self::y other::y) false)))
(impl Hash point
  (sxhash ((self Self)) int (logand (+ (* 31 self::x) self::y) *sxhash-mask*)))

(let ((h (the HashTable<point,string> (HashTable::new))))
  (progn (set h (point::new 1 2) "a")
         (get h (point::new 1 2))))          ; => (some a)
```

キーが同じかどうかを決めるのは**キーの型自身**（`sxhash` と、`Hash` のスーパトレイト
`Eq` の `equals`）で、オブジェクトの同一性ではない。だから上のように「別の値だが等しい」
キーで引ける。

`sxhash` が衝突しても構わない（`Hash` の契約は逆向き——`equals` なら同じハッシュ、としか
言っていない）。衝突したキーは `equals` で区別される。

## 5. `Array<T>`（多次元配列）

標準ライブラリの `defstruct`。組み込み型ではないので、`defstruct` にできることは全部できる。

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `make` | `(Array::make dims init &key fill-pointer)` | `(Vector<int>,T)→Array<T>` | CL `make-array`。`dims` は複製される。`init` が全セルの初期値（CL の `:initial-element`。この言語に「未束縛のセル」は無いので必須）。`:fill-pointer` は 1 次元のときだけ |
| `get` / `set` | `(get a idx)` / `(set a idx x)` | `(Array<T>,Vector<int>)→T` | CL `aref` / `(setf (aref …))`。添字が範囲外なら panic |
| `aref` | `(aref a i j …)` | — | 裸の添字で書く CL の綴り。上の `get`/`set` に展開される。`(setf (aref a i j) v)` も同じ |
| `row-major-get` / `row-major-set` | `(row-major-get a i)` | `(Array<T>,int)→T` | CL `row-major-aref`。平坦な添字 |
| `rank` | `(rank a)` | `Array<T>→int` | CL `array-rank` |
| `dimension` | `(dimension a n)` | `(Array<T>,int)→int` | CL `array-dimension` |
| `dimensions` | `(dimensions a)` | `Array<T>→Vector<int>` | CL `array-dimensions`。CL が新しいリストを返すのと同じく**複製**を返す |
| `total-size` | `(total-size a)` | `Array<T>→int` | CL `array-total-size`（fill pointer とは無関係の確保済みセル数） |
| `len` | `(len a)` | `Array<T>→int` | CL の配列に対する `length`。fill pointer があればその値、無ければ `total-size` |
| `in-bounds` | `(in-bounds a idx)` | `(Array<T>,Vector<int>)→bool` | CL `array-in-bounds-p`。添字の**個数**が違っても偽（エラーではない） |
| `row-major-index` | `(row-major-index a idx)` | `(Array<T>,Vector<int>)→int` | CL `array-row-major-index` |
| `adjust` | `(adjust a dims init)` | `(Array<T>,Vector<int>,T)→Unit` | CL `adjust-array`。ランクは変えられない。範囲に残る要素は添字ごと保存、増えたセルは `init`。CL と違い配列を返さない（この言語の配列は全部 adjustable なので、返す第 2 の配列が無い） |
| `push-extend` | `(push-extend a x)` | `(Array<T>,T)→Unit` | CL `vector-push-extend`。fill pointer が無ければ panic |
| `pop` | `(pop a)` | `Array<T>→Option<T>` | CL `vector-pop`。空なら `none` |
| `fill-pointer` | `(fill-pointer a)` | `Array<T>→Option<int>` | fill pointer（無ければ `none`）。`(setf a::fill-pointer …)` で書ける |
| `iter` | `(iter a)` | `Array<T>→array-iter<T>` | row-major 順のイテレータ。fill pointer があればそこで止まる |

- **添字は `Vector<int>`**。メソッドは「末尾に同じ型の引数が何個か続く」形を宣言できないので、
  `aref` の糖衣がその差を埋めている。
- `array-element-type` / `simple-vector-p` / `adjustable-array-p` /
  `array-has-fill-pointer-p` は**無い**。受け手の静的型が既に答えている問い。
- `Array::new` は `defstruct` が生成するフィールド順のコンストラクタで、作るときに使うものでは
  ない。`Array::make` を使う。
- **印字は CL の配列構文**。ランク 1 は `#(1 2 3)`、それ以外は `#nA` と次元ぶんの括弧
  （`#2A((1 2 3) (4 5 6))`）、ランク 0 は `#0A5`。fill pointer があればそこで切る。
  `*print-array*`（[印字](printing.md#6-印字量の制御)）を偽にすると形だけの `#<array 2x3>` になる。
  `print-object` を書いていない `defstruct` を要素にした配列だけは組み込みの
  `#<array<...> ...>` で出る（エラーにはならない）。

## 6. `BitVector`（ビットベクタ）

固定長のビット列。標準ライブラリの `defstruct`。

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `make` | `(BitVector::make n)` | `int→BitVector` | 長さ `n`、全ビット 0 |
| `get` / `set` | `(get v i)` / `(set v i b)` | `(BitVector,int)→bool` | 範囲外は panic |
| `bit` / `sbit` | `(bit v i)` | `(BitVector,int)→bool` | CL の綴り。`(setf (bit v i) b)` も書ける。CL の `sbit` は simple なビットベクタを要求する点だけが `bit` と違うが、この言語のビットベクタは 1 種類しかない |
| `len` | `(len v)` | `BitVector→int` | ビット数 |
| `bit-and` `bit-ior` `bit-xor` `bit-eqv` `bit-nand` `bit-nor` `bit-andc1` `bit-andc2` `bit-orc1` `bit-orc2` | `(op a b)` | `(BitVector,BitVector)→BitVector` | 新しいビットベクタを返す。長さが違えば panic。CL の第 3 引数（結果の書き込み先）は無い |
| `bit-not` | `(bit-not v)` | `BitVector→BitVector` | 補集合 |

`bit-vector-p` は無い（静的型が答えている）。
