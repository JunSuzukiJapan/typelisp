<!-- translated-from: docs/ja/reference/functions/collections.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# 字串、字元與集合

`string`、`char`、`Vector<T>`、`HashTable<K,V>`、`Array<T>`、`BitVector`。

## 1. 字串 `string`

字串是不可變的。

| 名稱 | 形式 | 型別 | 說明 |
|---|---|---|---|
| `upcase` | `(upcase s)` | `string→string` | 轉為大寫（僅 ASCII）。與 CL 的 `string-upcase` 一樣回傳新字串。字串不可變，所以沒有破壞性版本 `nstring-upcase`，由它取代 |
| `downcase` | `(downcase s)` | `string→string` | 轉為小寫（僅 ASCII）。取代 `nstring-downcase` |
| `capitalize` | `(capitalize s)` | `string→string` | 每個詞的第一個字母大寫、其餘小寫（CL 的 `string-capitalize`）。詞是英數字的極大連續序列 |
| `length` | `(length s)` | `string→int` | 字元數 |
| `ref` | `(ref s i)` | `(string,int)→char` | 第 `i` 個字元。超出範圍時 panic |
| `substring` | `(substring s start end)` | `(string,int,int)→string` | 子字串 `[start,end)` |
| `append` | `(append s1 s2 ...)` | `(string,string,...)→string` | 串接。也可以寫 3 個以上（與 `(concatenate 'string ...)` 相同） |
| `<` `<=` `>` `>=` | `(op s1 s2)` | `(string,string)→bool` | 字典序比較 |
| `lt` | `(lt s1 s2)` | `(string,string)→bool` | 字典序的嚴格小於（與 `<` 相同） |
| `eq` `eql` | `(op s1 s2)` | `(string,string)→bool` | 同一性比較（比較是否為同一物件，而不是內容） |
| `equal` | `(equal s1 s2)` | `(string,string)→bool` | 內容比較（區分大小寫） |
| `equalp` | `(equalp s1 s2)` | `(string,string)→bool` | 內容比較（忽略大小寫，僅 ASCII） |
| `/=` | `(/= s1 s2)` | `(string,string)→bool` | 內容是否不同（CL 的 `string/=`。可變引數形式比較相鄰的組） |
| `lessp` `greaterp` `not-lessp` `not-greaterp` | `(op s1 s2)` | `(string,string)→bool` | 忽略大小寫的順序比較（CL 的 `string-lessp` 等）。有共同前綴時較短者較小 |
| `string::filled` | `(string::filled n c)` | `(int,char)→string` | 由 `n` 個 `c` 組成的字串（CL 的 `make-string`） |
| `search` | `(search s sub)` | `(string,string)→Option<int>` | `sub` 第一次出現的位置。**CL 的 `search` 引數順序相反**（`(search pattern sequence)`）。空字串在 0 找到。關鍵字見[序列的關鍵字引數](sequences.md#6-關鍵字引數) |
| `mismatch` | `(mismatch a b)` | `(string,string)→Option<int>` | 第一個不一致的位置。只有 `equal` 時為 `none`。一方是另一方的前綴時為較短者的結尾。關鍵字同上 |
| `trim` `left-trim` `right-trim` | `(trim s)` / `(trim s bag)` | `(string &optional string)→string` | 從兩端／左端／右端去掉 `bag` 中包含的字元（CL 的 `string-trim` 等）。省略 `bag` 時為空白字元 `" \t\n\r"` |
| `split` | `(split s sep)` | `(string,string)→Vector<string>` | 以 `sep` 分割。CL 中沒有對應物。連續的分隔符號會產生空元素。`sep` 為空時 panic |
| `to-string` | `(to-string x)` | `T→string` | 相當於 `~a` 的字串化。實作於 `int`/`i32`/`f64`/`bool`/`char`/`string`（CL 的 `princ-to-string`） |
| `string->utf8` | `(string->utf8 s)` | `string→Vector<int>` | 編碼為 UTF-8（每個元素 0..255） |
| `utf8->string` | `(utf8->string bytes)` | `Vector<int>→Option<string>` | 解碼。不是合法的 UTF-8 時為 `none` |

## 2. 字元 `char`

`char` 是 Unicode 純量值。大小寫轉換與分類只處理 ASCII 範圍。

| 名稱 | 形式 | 型別 | 說明 |
|---|---|---|---|
| `upcase` | `(upcase c)` | `char→char` | 轉為大寫（僅 ASCII） |
| `downcase` | `(downcase c)` | `char→char` | 轉為小寫（僅 ASCII） |
| `<` `<=` `>` `>=` | `(op c1 c2)` | `(char,char)→bool` | 依碼位比較 |
| `lt` | `(lt c1 c2)` | `(char,char)→bool` | 依碼位的嚴格小於（與 `<` 相同） |
| `alphap` | `(alphap c)` | `char→bool` | 是否為 ASCII 字母 |
| `digitp` | `(digitp c)` | `char→bool` | 是否為 ASCII 數字 |
| `eq` `eql` `equal` | `(op c1 c2)` | `(char,char)→bool` | 值的比較 |
| `equalp` | `(equalp c1 c2)` | `(char,char)→bool` | 忽略大小寫的值比較（CL 的 `char-equal`） |
| `/=` | `(/= c1 c2)` | `(char,char)→bool` | 值是否不同（CL 的 `char/=`。**可變引數形式比較相鄰的組**，與詢問所有組是否互不相同的 CL 不同） |
| `lessp` `greaterp` `not-lessp` `not-greaterp` | `(op c1 c2)` | `(char,char)→bool` | 忽略大小寫的順序比較（CL 的 `char-lessp` 等） |
| `upper-casep` `lower-casep` `both-casep` | `(op c)` | `char→bool` | 是否大寫／是否小寫／是否有大小寫之分（CL 的 `upper-case-p` 等） |
| `alphanumericp` | `(alphanumericp c)` | `char→bool` | 是否為字母或數字（與 CL 同名） |
| `graphicp` | `(graphicp c)` | `char→bool` | 是否可列印。包括空格，不包括換行與定位字元（CL 的 `graphic-char-p`） |
| `standardp` | `(standardp c)` | `char→bool` | 是否為 CL 的 96 個標準字元之一，即 `graphicp` 再加上換行（CL 的 `standard-char-p`） |
| `char->int` | `(char->int c)` | `char→int` | Unicode 純量值（反方向是[數值](numbers.md#1-固定寬度整數)的 `int->char`/`try-int->char`）。相當於 CL 的 `char-code`/`char-int` |
| `char->string` | `(char->string c)` | `char→string` | 只含一個字元的字串。CL 的 `string` 函式透過接受指定子兼顧這一點，但這個語言沒有指定子，所以把方向寫進名稱 |
| `digit-weight` | `(digit-weight c)` / `(digit-weight c radix)` | `(char &optional int)→Option<int>` | 該基數下數字的**權值**（CL 的 `digit-char-p`）。`digitp` 是回傳 `bool` 的另一個函式 |
| `digit->char` | `(digit->char w)` / `(digit->char w radix)` | `(int &optional int)→Option<char>` | 表示權值 `w` 的字元。10 以上為大寫（CL 的 `digit-char`。基數最大 36） |
| `char->name` | `(char->name c)` | `char→Option<string>` | 字元名稱。只有讀取器能讀的具名字元才有名稱（CL 的 `char-name`） |
| `name->char` | `(name->char s)` | `string→Option<char>` | 由字元名稱得到字元。忽略大小寫，也接受讀取器的別名（`linefeed`/`null`）（CL 的 `name-char`） |

沒有相當於 `char-code-limit` 的常數（`char` 的上限由 Unicode 而不是語言決定）。

## 3. `Vector<T>`

可變長度陣列。

| 名稱 | 形式 | 型別 | 說明 |
|---|---|---|---|
| `new` | `(Vector::new)` | `()→Vector<T>` | 建立空向量。型別引數由期望型別決定，所以在裸 `let` 中寫成 `(the Vector<i32> (Vector::new))` |
| `filled` | `(Vector::filled n x)` | `(int,T)→Vector<T>` | `n` 個 `x` |
| `push` | `(push v x)` | `(Vector<T>,T)→Unit` | 加到尾端 |
| `get` | `(get v i)` | `(Vector<T>,int)→T` | 讀取第 `i` 個。超出範圍時 panic |
| `set` | `(set v i x)` | `(Vector<T>,int,T)→Unit` | 修改第 `i` 個。超出範圍時 panic。也可以寫成 `(setf (get v i) x)` |
| `len` | `(len v)` | `Vector<T>→int` | 元素個數 |
| `pop` | `(pop v)` | `Vector<T>→Option<T>` | 去掉尾端元素並回傳。為空時為 `None`（與 `get`/`set` 不同，不會 panic） |
| `iter` | `(iter v)` | `Vector<T>→vector-iter<T>` | 建立實作了 `Iter` 的迭代器 |
| `pushnew` | `(pushnew v x)` | `(Vector<T>,T)→Unit` where `Eq T` | 沒有與 `x` 相等的元素時加到尾端（CL 的 `pushnew`。不需要修改位置，所以是方法而不是巨集） |

`map`/`filter` 等是[序列函式](sequences.md#4-iter-上的序列函式)——像 `(map (iter v) f)` 這樣經 `iter` 傳入。破壞性操作（`nreverse`、
`delete` 等）見[破壞性操作](sequences.md#7-破壞性操作)。

## 4. `HashTable<K,V>`

| 名稱 | 形式 | 型別 | 說明 |
|---|---|---|---|
| `new` | `(HashTable::new)` | `()→HashTable<K,V>` | 建立空表 |
| `get` | `(get h k)` | `(HashTable<K,V>,K)→Option<V>` | 查詢 |
| `set` | `(set h k v)` | `(HashTable<K,V>,K,V)→Unit` | 插入或覆寫 |
| `remove` | `(remove h k)` | `(HashTable<K,V>,K)→Option<V>` | 刪除，如有舊值則回傳 |
| `count` | `(count h)` | `HashTable<K,V>→int` | 元素個數 |
| `clear` | `(clear h)` | `HashTable<K,V>→Unit` | 全部刪除 |
| `keys` | `(keys h)` | `HashTable<K,V>→Vector<K>` | 鍵的快照 |
| `values` | `(values h)` | `HashTable<K,V>→Vector<V>` | 值的快照 |
| `entries` | `(entries h)` | `HashTable<K,V>→Vector<cons-cell<K,V>>` | `(k . v)` 序對的快照 |
| `iter` | `(iter h)` | `HashTable<K,V>→hashtable-iter<K,V>` | 實作了 `Iter` 的迭代器。元素是 `(k . v)` 的 `cons-cell`。相當於 CL 的 `with-hash-table-iterator`，`doiter`/`map`/`filter` 等可以直接使用 |
| `maphash` | `(maphash h f)` | `(HashTable<K,V>,(fn (K V) ()))→Unit` | CL 的 `maphash` |
| `size` | `(size h)` | `HashTable<K,V>→int` | CL 的 `hash-table-size`。在這個表中是占用數（等於 `count`） |

**只要實作了 `Hash`，任何型別都可以當作鍵**——包括 `defstruct`/`defenum`。`get`/`set`/`remove` 帶有 `(where (Hash K))`，所以以沒有實作它的
型別為鍵的表是**型別錯誤**（`f64` 沒有 `Hash` 是因為 `NaN`）。

```lisp
(deftrait Hash (Eq)
  (sxhash ((self Self)) int))              ; 回傳非負且放得進 fixnum 的值
```

已實作：`int` 與 6 種固定寬度整數、`bool`、`char`、`string`、`symbol`（浮點數沒有）。在自己的型別中，把結果與 `*sxhash-mask*`（2^30-1）做
`logand` 以保持非負。想對字串求雜湊時，可以呼叫 `string` 的實作所使用的 `(sxhash-string s)`（FNV-1a 32 位元）。

```lisp
(defstruct point (x int) (y int))
(impl Eq point
  (equals ((self Self) (other Self)) bool
    (if (= self::x other::x) (= self::y other::y) false)))
(impl Hash point
  (sxhash ((self Self)) int (logand (+ (* 31 self::x) self::y) *sxhash-mask*)))

(let ((h (the HashTable<point,string> (HashTable::new))))
  (progn (set h (point::new 1 2) "a")
         (get h (point::new 1 2))))          ; => (some "a")
```

鍵是否相同由**鍵的型別本身**決定（`sxhash`，以及 `Hash` 的超 trait `Eq` 的 `equals`），而不是物件的同一性。所以像上面那樣，可以用「不同的
值但相等」的鍵查詢。

`sxhash` 衝突也無妨（`Hash` 的約定是反方向的——只規定 `equals` 相等則雜湊相同）。衝突的鍵由 `equals` 區分。

## 5. `Array<T>`（多維陣列）

標準函式庫中的 `defstruct`。不是內建型別，所以 `defstruct` 能做的事全都可以做。

| 名稱 | 形式 | 型別 | 說明 |
|---|---|---|---|
| `make` | `(Array::make dims init &key fill-pointer)` | `(Vector<int>,T)→Array<T>` | CL 的 `make-array`。`dims` 會被複製。`init` 是所有單元的初始值（CL 的 `:initial-element`。這個語言沒有「未繫結的單元」，所以是必要的）。`:fill-pointer` 只用於一維 |
| `get` / `set` | `(get a idx)` / `(set a idx x)` | `(Array<T>,Vector<int>)→T` | CL 的 `aref` / `(setf (aref …))`。索引超出範圍時 panic |
| `aref` | `(aref a i j …)` | — | 以裸索引書寫的 CL 寫法。展開為上面的 `get`/`set`。`(setf (aref a i j) v)` 也一樣 |
| `row-major-get` / `row-major-set` | `(row-major-get a i)` | `(Array<T>,int)→T` | CL 的 `row-major-aref`。平坦的索引 |
| `rank` | `(rank a)` | `Array<T>→int` | CL 的 `array-rank` |
| `dimension` | `(dimension a n)` | `(Array<T>,int)→int` | CL 的 `array-dimension` |
| `dimensions` | `(dimensions a)` | `Array<T>→Vector<int>` | CL 的 `array-dimensions`。與 CL 回傳新串列一樣，回傳**副本** |
| `total-size` | `(total-size a)` | `Array<T>→int` | CL 的 `array-total-size`（與填充指標無關的已配置單元數） |
| `len` | `(len a)` | `Array<T>→int` | CL 對陣列的 `length`。有填充指標時為其值，否則為 `total-size` |
| `in-bounds` | `(in-bounds a idx)` | `(Array<T>,Vector<int>)→bool` | CL 的 `array-in-bounds-p`。索引的**個數**不對時也為假（不是錯誤） |
| `row-major-index` | `(row-major-index a idx)` | `(Array<T>,Vector<int>)→int` | CL 的 `array-row-major-index` |
| `adjust` | `(adjust a dims init)` | `(Array<T>,Vector<int>,T)→Unit` | CL 的 `adjust-array`。不能改變秩。仍在範圍內的元素依索引保留，新增的單元為 `init`。與 CL 不同，不回傳陣列（這個語言的陣列都是可調整的，沒有要回傳的第二個陣列） |
| `push-extend` | `(push-extend a x)` | `(Array<T>,T)→Unit` | CL 的 `vector-push-extend`。沒有填充指標時 panic |
| `pop` | `(pop a)` | `Array<T>→Option<T>` | CL 的 `vector-pop`。為空時為 `none` |
| `fill-pointer` | `(fill-pointer a)` | `Array<T>→Option<int>` | 填充指標（沒有時為 `none`）。可以用 `(setf a::fill-pointer …)` 寫入 |
| `iter` | `(iter a)` | `Array<T>→array-iter<T>` | 依列優先順序的迭代器。有填充指標時在那裡停止 |

- **索引是 `Vector<int>`。** 方法無法宣告「尾端跟著若干個同型別引數」的形式，`aref` 語法糖彌補了這個落差。
- **沒有** `array-element-type` / `simple-vector-p` / `adjustable-array-p` / `array-has-fill-pointer-p`。接收者的靜態型別已經回答了這些問題。
- `Array::new` 是 `defstruct` 產生的依欄位順序的建構函式，不是用來建立陣列的。請使用 `Array::make`。
- **列印採用 CL 的陣列語法。** 秩 1 為 `#(1 2 3)`，其他為 `#nA` 加上與維數相同層數的括號（`#2A((1 2 3) (4 5 6))`），秩 0 為 `#0A5`。
  有填充指標時在那裡截斷。把 `*print-array*`（[列印](printing.md#6-控制列印量)）設為假，就只列印形狀 `#<array 2x3>`。只有元素是沒有寫
  `print-object` 的 `defstruct` 的陣列以內建形式 `#<array<...> ...>` 列印（不是錯誤）。

## 6. `BitVector`（位元向量）

固定長度的位元序列。標準函式庫中的 `defstruct`。

| 名稱 | 形式 | 型別 | 說明 |
|---|---|---|---|
| `make` | `(BitVector::make n)` | `int→BitVector` | 長度為 `n`，所有位元為 0 |
| `get` / `set` | `(get v i)` / `(set v i b)` | `(BitVector,int)→bool` | 超出範圍時 panic |
| `bit` / `sbit` | `(bit v i)` | `(BitVector,int)→bool` | CL 的寫法。也可以寫 `(setf (bit v i) b)`。CL 的 `sbit` 與 `bit` 的唯一差異是要求簡單位元向量，但這個語言的位元向量只有一種 |
| `len` | `(len v)` | `BitVector→int` | 位元數 |
| `bit-and` `bit-ior` `bit-xor` `bit-eqv` `bit-nand` `bit-nor` `bit-andc1` `bit-andc2` `bit-orc1` `bit-orc2` | `(op a b)` | `(BitVector,BitVector)→BitVector` | 回傳新的位元向量。長度不同時 panic。沒有 CL 的第 3 個引數（結果的寫入目的地） |
| `bit-not` | `(bit-not v)` | `BitVector→BitVector` | 補集 |

沒有 `bit-vector-p`（靜態型別已經回答了）。
