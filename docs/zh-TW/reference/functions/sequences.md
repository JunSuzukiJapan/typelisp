<!-- translated-from: docs/ja/reference/functions/sequences.md @ 8c7bff99b2cddddb57736d0567933bc024a5a467 -->
# 序對、S 運算式與序列

泛型序對 `cons-cell`、S 運算式資料 `Sexpr`、符號、建構在 `Iter` 之上的序列函式，以及高階函式。

## 1. 序對 `cons-cell<A,B>`

`cons`/`car`/`cdr` 是**泛型序對型別 `cons-cell<A,B>`**（標準函式庫中的 `defstruct`）的建構函式與欄位存取器。欄位既可以用
`變數::car`/`變數::cdr`（[語法參考](../syntax.md#36-defstruct--結構使用者定義型別)中 `defstruct` 的存取器語法）讀取，也可以用
`(car 變數)`/`(cdr 變數)` 讀取。修改時使用 `(setf 變數::car v)`/`(setf 變數::cdr v)`。

| 名稱 | 形式 | 型別 | 說明 |
|---|---|---|---|
| `cons` | `(cons a b)` | `(A,B)→cons-cell<A,B>` | 建立序對 |
| `car` | `(car p)` | `cons-cell<A,B>→A` | 第一個元素 |
| `cdr` | `(cdr p)` | `cons-cell<A,B>→B` | 其餘部分 |

`cons-cell` 也用來取代元組語法。回傳多值的 CL 函式（`floor` 的商與餘數、`read-from-string` 的值與位置等）在這個語言中回傳 `cons-cell`。

## 2. S 運算式資料 `Sexpr`

`read` 回傳的資料型別 `Sexpr` 有 19 種變體：
`int | i8 | i16 | i32 | u8 | u16 | u32 | f32 | f64 | char | bool | sym | str | cons | ratio | path | vector | array | tuple`。
`vector` 和 `array` 是用 `#(..)` 和 `#nA(..)` 寫的資料（[語法參考](../syntax.md#1-詞法元素)），內容分別是 `Vector<Option<Sexpr>>` 和 `Array<Option<Sexpr>>`——對 `(vector v)` 繫結的 `v` 可以直接使用 `len`、`get` 等。
`tuple` 是用 `#{..}` 寫的資料，`(tuple v)` 繫結的 `v` 是依序排列元素的新 `Vector<Option<Sexpr>>`（為了用同一個型別接收任意長度的元組）。
處理 S 運算式單元的不是第 1 章通用的 `cons`/`car`/`cdr`，而是 `sexpr-*` 函式。主要在 `defmacro` 的本體中用於組裝與拆解形式。

**S 運算式資料的型別是 `Option<Sexpr>`。** 空串列不是 `Sexpr` 的變體，而是 `Option` 的 `none`，`Sexpr` 本身表示「非空的 S 運算式」。
因此 `sexpr-*` 的引數與回傳值都是 `Option<Sexpr>`。

- 在期望 `Option<Sexpr>` 的位置，`()` 是空串列（也可以寫成 `(Option::none)`）
- 在期望 `Option<Sexpr>` 的位置，`Sexpr` 會隱式擴大（沒有執行期的轉換）。反方向——把 `Option<Sexpr>` 當作 `Sexpr` 使用——是在聲稱
  「不是空串列」，需要以 `match` 或 `unwrap` 明確表示
- 在 `match` 中，`Sexpr` 的 19 種變體與 `none` 可以**平鋪在同一組分支中**（[語法參考](../syntax.md#43-match--模式比對)）

| 名稱 | 形式 | 型別 | 說明 |
|---|---|---|---|
| `sexpr-cons` | `(sexpr-cons a b)` | `(Option<Sexpr>,Option<Sexpr>)→Option<Sexpr>` | 建立 `Sexpr` 單元 |
| `sexpr-car` | `(sexpr-car s)` | `Option<Sexpr>→Option<Sexpr>` | 第一個元素。**空串列時回傳空串列**（依 CL）。不是 `Cons` 的原子時 panic |
| `sexpr-cdr` | `(sexpr-cdr s)` | `Option<Sexpr>→Option<Sexpr>` | 其餘部分。**空串列時回傳空串列**（依 CL）。不是 `Cons` 的原子時 panic |
| `sexpr-consp` | `(sexpr-consp s)` | `Option<Sexpr>→bool` | 是否為 `Cons` |
| `sexpr-null` | `(sexpr-null s)` | `Option<Sexpr>→bool` | 是否為空串列 |
| `sexpr-atom` | `(sexpr-atom s)` | `Option<Sexpr>→bool` | 是否不是 `Cons` |
| `sexpr-symp` | `(sexpr-symp s)` | `Option<Sexpr>→bool` | 是否為 `Sym`（符號） |
| `sexpr-int` | `(sexpr-int s)` | `Option<Sexpr>→int` | `int` 變體的內容（fixnum 或多倍精度）。型別不符時 panic |
| `sexpr-i8` `sexpr-i16` `sexpr-i32` `sexpr-u8` `sexpr-u16` `sexpr-u32` | `(sexpr-i32 s)` | `Option<Sexpr>→W` | 該寬度變體的內容。型別不符時 panic |
| `sexpr-f32` `sexpr-f64` | `(sexpr-f64 s)` | `Option<Sexpr>→f32` / `→f64` | 浮點變體的內容。型別不符時 panic |
| `sexpr-char` | `(sexpr-char s)` | `Option<Sexpr>→char` | `Char` 的內容。型別不符時 panic |
| `sexpr-bool` | `(sexpr-bool s)` | `Option<Sexpr>→bool` | `Bool` 的內容。型別不符時 panic |
| `sexpr-str` | `(sexpr-str s)` | `Option<Sexpr>→string` | `Str` 的內容。型別不符時 panic |
| `sexpr-sym-name` | `(sexpr-sym-name s)` | `Option<Sexpr>→string` | `Sym` 的名稱。型別不符時 panic |
| `eq` `eql` | `(op a b)` | `(Option<Sexpr>,Option<Sexpr>)→bool` | 同一性比較（`Cons`/`Str` 比較物件的同一性，其他比較值） |
| `equal` | `(equal a b)` | `(Option<Sexpr>,Option<Sexpr>)→bool` | 結構相等（`Cons` 遞迴比較，`Str` 比較內容） |
| `equalp` | `(equalp a b)` | `(Option<Sexpr>,Option<Sexpr>)→bool` | 在 `equal` 的基礎上忽略大小寫，並跨型別比較數值 |
| `sexpr-append` | `(sexpr-append a b)` | `(Option<Sexpr>,Option<Sexpr>)→Option<Sexpr>` | 串接兩個 `Sexpr` 串列（非破壞性）。`,@` 展開為它 |
| `sexpr-map` | `(sexpr-map f lst)` | `((fn (Option<Sexpr>) Option<Sexpr>),Option<Sexpr>)→Option<Sexpr>` | 對 `Sexpr` 串列的每個元素套用 `f` 得到的新 `Sexpr` 串列（第 4 章的 `map` 用於 `Iter`，不能走訪 `Sexpr` 串列） |

數值存取器依型別分為 9 個，是因為 `Sexpr` 是「值的型別在其他任何地方都沒有寫出的唯一場所」。放入 `Sexpr` 的 `u8` 以 `u8` 變體放入，只能
用 `(sexpr-u8 s)` 取出。傳給 `(sexpr-int s)` 會 panic——不會默默擴大寬度來作答。讀入的資料（`'(1 2 3)`、巨集的引數）中的整數是 `int`
變體，以 `(sexpr-int s)` 讀取。

`Sexpr` 串列沒有 `rplaca`/`nconc` 之類的破壞性操作。`Sexpr` 單元建立後不能修改。

## 3. 符號

`symbol` 是符號本身的型別。在需要 `Sexpr` 的上下文中會隱式轉換，但反方向沒有自動轉換。

| 名稱 | 形式 | 型別 | 說明 |
|---|---|---|---|
| `symbol->string` | `(symbol->string s)` | `symbol→string` | 取出符號名稱 |
| `string->symbol` | `(string->symbol s)` | `string→symbol` | 從字串建立符號（intern） |
| `keywordp` | `(keywordp s)` | `symbol→bool` | 是否為關鍵字（`:name`）。冒號是名稱的一部分，所以依第一個字元判斷（[語法參考](../syntax.md#1-詞法元素)） |

`gensym` 見[巨集](system.md#8-巨集)。

## 4. `Iter` 上的序列函式

序列函式是 **`Iter` trait 上的泛型函式**。從集合以 `(iter coll)` 取得迭代器後傳入（`Vector<T>` / `HashTable<K,V>` / `Array<T>` 支援。
`Sexpr` 串列沒有實作 `Iter`，不是這些函式的對象）。**結果集合以新的 `Vector` 回傳。** 表中的 `Iter<A>` 表示「`Item` 為 `A` 的任意
`Iter` 實作型別」。要再次走訪回傳的 `Vector`，傳入 `(iter result)`。

接受述詞的函式（對應 CL 的 `-if` 系列）：

| 名稱 | 形式 | 型別 | 說明 |
|---|---|---|---|
| `map` | `(map it f)` | `(Iter<A>,(fn (A) U))→Vector<U>` | 映射 |
| `filter` | `(filter it pred)` | `(Iter<A>,(fn (A) bool))→Vector<A>` | 只保留符合條件的元素 |
| `remove-if` | `(remove-if it pred)` | `(Iter<A>,(fn (A) bool))→Vector<A>` | 去掉符合條件的元素 |
| `find-if` | `(find-if it pred)` | `(Iter<A>,(fn (A) bool))→Option<A>` | 第一個符合條件的元素 |
| `position-if` | `(position-if it pred)` | `(Iter<A>,(fn (A) bool))→Option<int>` | 第一個符合條件的位置 |
| `count-if` | `(count-if it pred)` | `(Iter<A>,(fn (A) bool))→int` | 符合條件的個數 |
| `every` | `(every it pred)` | `(Iter<A>,(fn (A) bool))→bool` | 是否所有元素都符合條件 |
| `any` | `(any it pred)` | `(Iter<A>,(fn (A) bool))→bool` | 是否有元素符合條件（相當於 CL 的 `some`。是不與 `Some` 建構函式衝突的名稱） |
| `foldl` | `(foldl it f init)` | `(Iter<A>,(fn (B A) B),B)→B` | 左摺疊 |
| `foldr` | `(foldr it f init)` | `(Iter<A>,(fn (A B) B),B)→B` | 右摺疊 |

索引、長度與切片：

| 名稱 | 形式 | 型別 | 說明 |
|---|---|---|---|
| `length` | `(length it)` | `Iter<A>→int` | 元素個數 |
| `append` | `(append a b ...)` | `(Iter<A>,Iter<A>,...)→Vector<A>` | 串接迭代器。也可以寫 3 個以上 |
| `concatenate` | `(concatenate 'vector it ...)` / `(concatenate 'string s ...)` | `→Vector<A>` / `→string` | CL 的 `concatenate`。結果型別寫成**引用符號的字面值**（CL 是執行期的型別指定子）。`'vector` 需要 1 個以上，`'string` 0 個以上（0 個時為 `""`）。`Sexpr` 串列不適用（用 `sexpr-append`） |
| `reverse` | `(reverse it)` | `Iter<A>→Vector<A>` | 反轉（非破壞性） |
| `nth` | `(nth n it)` | `(int,Iter<A>)→Option<A>` | 第 `n` 個元素（超出範圍時為 `None`） |
| `elt` | `(elt it n)` | `(Iter<A>,int)→Option<A>` | 引數順序不同的 `nth` |
| `take` | `(take it n)` | `(Iter<A>,int)→Vector<A>` | 前 `n` 個 |
| `subseq` | `(subseq it start end)` | `(Iter<A>,int,int)→Vector<A>` | `[start,end)`（`end` 依長度截斷） |
| `last` | `(last it)` | `Iter<A>→Option<A>` | 最後一個**元素**（不是 CL 的「最後一個單元」） |
| `butlast` | `(butlast it)` | `Iter<A>→Vector<A>` | 去掉最後一個元素 |

要求 `Eq` / `Ord` 約束的函式（以 trait 取代述詞進行比較。[標準 trait](traits.md#2-eq--ord比較)）：

| 名稱 | 形式 | 型別 | 說明 |
|---|---|---|---|
| `member` | `(member x it)` | `(A,Iter<A>)→bool` where `Eq A` | 是否有與 `x` 相等的元素（與 CL 不同，回傳 `bool` 而不是串列的其餘部分） |
| `find` | `(find x it)` | `(A,Iter<A>)→Option<A>` where `Eq A` | 第一個與 `x` 相等的元素 |
| `position` | `(position x it)` | `(A,Iter<A>)→Option<int>` where `Eq A` | 第一個與 `x` 相等的位置 |
| `count` | `(count x it)` | `(A,Iter<A>)→int` where `Eq A` | 與 `x` 相等的元素個數 |
| `sort` | `(sort it cmp)` | `(Iter<A>,(fn (A A) bool))→Vector<A>` | CL 的 `(sort sequence predicate)`。穩定的非破壞性排序。`cmp` 在「第 1 個引數嚴格排在第 2 個引數之前」時為 `true` |
| `assoc` | `(assoc k it)` | `(K,Iter<cons-cell<K,V>>)→Option<cons-cell<K,V>>` where `Eq K` | 第一個 `car` 與 `k` 相等的序對。值以 `(cdr p)` 取出 |

這些函式以及第 5 章的許多函式也接受 CL 的關鍵字引數 `:key` / `:test` / `:test-not` / `:start` / `:end` / `:from-end` / `:count`（第 6 章）。

## 5. CL 其餘的序列函式

都是與第 4 章相同的 `Iter` 上的泛型函式。結果集合以新的 `Vector` 回傳。

| 名稱 | 形式 | 型別 | 說明 |
|---|---|---|---|
| `first`…`tenth` | `(first it)` | `Iter<A>→Option<A>` | CL 的具名索引 |
| `rest` | `(rest it)` | `Iter<A>→Vector<A>` | 去掉第一個後的其餘部分（是新的 `Vector`，不是共享的尾部） |
| `copy-seq` | `(copy-seq it)` | `Iter<A>→Vector<A>` | 把迭代器實體化為 `Vector`（CL 的 `copy-seq`/`copy-list`） |
| `revappend` | `(revappend a b)` | `(Iter<A>,Iter<A>)→Vector<A>` | 反轉 `a` 後接上 `b` |
| `Vector::filled` | `(Vector::filled n x)` | `(int,T)→Vector<T>` | `n` 個 `x`（CL 的 `make-list`/`make-sequence`）。與 `Vector::new` 一樣型別引數來自期望型別，所以裸 `let` 需要 `the` |
| `member-if` `member-if-not` | `(member-if it pred)` | `(Iter<A>,(fn (A) bool))→bool` | 與 `member` 一樣回傳 **`bool`**（迭代器沒有可回傳的尾部） |
| `notany` `notevery` | `(notany it pred)` | `(Iter<A>,(fn (A) bool))→bool` | `any`/`every` 的否定 |
| `find-if-not` `position-if-not` `count-if-not` `remove-if-not` | `(op it pred)` | 與各肯定版同型別 | 述詞取反的版本 |
| `remove` | `(remove x it)` | `(A,Iter<A>)→Vector<A>` where `Eq A` | 依值刪除 |
| `remove-duplicates` | `(remove-duplicates it)` | `Iter<A>→Vector<A>` where `Eq A` | 去除重複。依 CL **保留最後一次出現**（要保留第一次出現用 `:from-end true`） |
| `substitute` `substitute-if` | `(substitute new old it)` | `(A,A,Iter<A>)→Vector<A>` | 依值／述詞取代 |
| `assoc-if` `rassoc` `rassoc-if` | `(rassoc v it)` | 作用於 `Iter<cons-cell<K,V>>` | `assoc` 的述詞版、值端版 |
| `acons` | `(acons k v it)` | `(K,V,Iter<cons-cell<K,V>>)→Vector<cons-cell<K,V>>` | 在開頭加上序對 |
| `pairlis` | `(pairlis ks vs)` | `(Iter<K>,Iter<V>)→Vector<cons-cell<K,V>>` | 把兩個序列配對。在較短者處停止 |
| `map2` | `(map2 a b f)` | `(Iter<A>,Iter<B>,(fn (A B) U))→Vector<U>` | CL 多序列的 `mapcar`。在較短者處停止 |
| `mapc` | `(mapc it f)` | `(Iter<A>,(fn (A) ()))→()` | 用於副作用的映射 |
| `mapcan` | `(mapcan it f)` | `(Iter<A>,(fn (A) Vector<U>))→Vector<U>` | 映射後串接 |
| `maplist` | `(maplist it f)` | `(Iter<A>,(fn (Vector<A>) U))→Vector<U>` | 對連續的**尾部**映射 |
| `mapl` | `(mapl it f)` | `(Iter<A>,(fn (Vector<A>) ()))→()` | 對尾部的副作用映射（`maplist` 版的 `mapc`） |
| `mapcon` | `(mapcon it f)` | `(Iter<A>,(fn (Vector<A>) Vector<U>))→Vector<U>` | 對尾部映射後串接（`maplist` 版的 `mapcan`） |
| `search` | `(search it sub)` | `(Iter<A>,Iter<A>)→Option<int>` where `Eq A` | `sub` 第一次出現的位置。接收者是 `string` 時選擇 `string` 的方法（[字串](collections.md#1-字串-string)） |
| `mismatch` | `(mismatch a b)` | `(Iter<A>,Iter<A>)→Option<int>` where `Eq A` | 第一個不一致的位置。相等時為 `none` |
| `merge` | `(merge a b less)` | `(Iter<A>,Iter<A>,(fn (A A) bool))→Vector<A>` | 合併。CL 要求已排序，這裡對串接結果排序 |
| `adjoin` | `(adjoin x it)` | `(A,Iter<A>)→Vector<A>` where `Eq A` | 沒有時加到**開頭** |
| `union` `intersection` `set-difference` `set-exclusive-or` | `(op a b)` | `(Iter<A>,Iter<A>)→Vector<A>` where `Eq A` | 集合運算。CL 不規定順序，這裡依**首次出現順序**保持穩定 |
| `subsetp` | `(subsetp a b)` | `(Iter<A>,Iter<A>)→bool` where `Eq A` | 包含關係 |
| `tailp` `ldiff` | `(tailp tail whole)` | `(Iter<A>,Iter<A>)→bool` / `→Vector<A>` | 是否為後綴／去掉後綴的前半部分。CL 詢問的是**結構共享**，但這裡沒有可共享的結構，所以詢問的是**依值**的後綴 |
| `seq-equals` | `(seq-equals a b)` | `(Vector<A>,Vector<A>)→bool` where `Eq A` | 逐元素相等。`Vector<T>` 本身沒有實作 `Eq` |
| `caar`…`cddddr` | `(cadr p)` | 作用於巢狀的序對 | CL 的 28 個函式。走訪的是**序對而不是串列**，`cadr` 接受 `cons-cell<A,cons-cell<B,C>>` |

CL 中有而這裡沒有的：`list*`（沒有替換結尾的不正規串列這個概念）、`copy-tree`/`copy-alist`/`sublis`/`subst`/`subst-if`（無法寫出走訪任意
深度異質樹的型別。作為 `Sexpr` 的樹，`equal` 相當於 `tree-equal`）、屬性串列一套 `getf`/`get-properties`/`symbol-plist`/`remprop`
（沒有鍵值交替排列的無型別串列這種表示。同樣的角色由 `assoc`（關聯串列）或 `HashTable` 擔任）、在 `Vector<T>` 與 `Sexpr` 串列之間互相
轉換的函式（`Sexpr` 串列的每個元素型別可能不同，無法以單一元素型別 `T` 寫出）。

## 6. 關鍵字引數

第 4 章與第 5 章的函式接受 CL 序列函式的關鍵字 `:key` / `:test` / `:test-not` / `:start` / `:end` / `:from-end` / `:count`。都是**可省略的**。

| 關鍵字 | 型別 | 意義 |
|---|---|---|
| `:key` | `(fn (A) A)` | 在比較或述詞之前套用於元素的投影 |
| `:test` | `(fn (A A) bool)` | 取代 `Eq` 約束的 `equals` 使用的相等判斷。第 1 個引數是**要找的項目**，第 2 個引數是（套用 `:key` 後的）元素——與 CL 順序相同 |
| `:test-not` | `(fn (A A) bool)` | `:test` 的否定 |
| `:start` `:end` | `int` | 掃描的視窗 `[start, end)`。索引相對於整個序列 |
| `:from-end` | `bool` | 搜尋回傳**最後一個**符合項。與 `:count` 一起使用時從尾端開始影響 |
| `:count` | `int` | `remove`／`substitute` 系列影響的最大個數 |

哪個函式接受哪些關鍵字依 CL：

| 函式 | 接受的關鍵字 |
|---|---|
| `find` `position` | `:key` `:test` `:test-not` `:start` `:end` `:from-end` |
| `count` | `:key` `:test` `:test-not` `:start` `:end` |
| `member` `adjoin` | `:key` `:test` `:test-not` |
| `remove` `substitute` | 上面全部（包括 `:count`） |
| `remove-duplicates` | `:key` `:test` `:test-not` `:start` `:end` `:from-end` |
| `find-if` `find-if-not` `position-if` `position-if-not` | `:key` `:start` `:end` `:from-end` |
| `count-if` `count-if-not` | `:key` `:start` `:end` |
| `member-if` `member-if-not` | `:key` |
| `remove-if` `remove-if-not` `substitute-if` | `:key` `:start` `:end` `:from-end` `:count` |
| `assoc` `rassoc` | `:key` `:test` `:test-not`（`assoc` 的 `:key` 作用於 `car`，`rassoc` 的作用於 `cdr`） |
| `assoc-if` `rassoc-if` | `:key` |
| `sort` `merge` | `:key` |
| `union` `intersection` `set-difference` `set-exclusive-or` `subsetp` | `:key` `:test` `:test-not` |
| `search` | `:key` `:test` `:test-not` `:from-end` `:start` `:end` `:sub-start` `:sub-end` |
| `mismatch` | `:key` `:test` `:test-not` `:from-end` `:start1` `:end1` `:start2` `:end2` |

```lisp
(find 2 (iter v) :key (lambda ((x int)) int (abs x)))   ; → (some -2)
(remove 2 (iter v) :count 1 :from-end true)             ; 只刪除尾端的 1 個
(position 3 (iter v) :start 1)                          ; 索引相對於整個序列
```

**與 CL 的不同之處**：

1. **`:key` 的投影封閉在元素型別之內**（`(fn (A) A)`）。不能像 CL 那樣投影到其他型別——增加型別變數的話，省略時就無法決定。需要投影到
   不同型別時，可以改為對 `-if` 系列傳 lambda（`(find-if it (lambda ((p ...)) bool (= (car p) 3)))`）。
2. **在依項目的搜尋中，`:key` 只作用於元素**（不作用於要找的項目）。與 CL 的 `find`/`position`/`count`/`member`/`remove`/`substitute`
   規則相同。集合運算中兩邊都是元素，所以兩邊都作用。
3. **只有 `search` 的關鍵字是名稱而不是編號。** CL 中 `:start1`/`:end1` 是**模式**，`:start2`/`:end2` 是被搜尋的序列，但這個語言中接收者
   在前，同樣的編號意義正好相反——而且是默默地。`:start`/`:end` 是接收者，`:sub-start`/`:sub-end` 是模式，所以順手寫下的 `:start1` 會得到
   「未知關鍵字」錯誤。`mismatch` 與 `replace` 的引數順序與 CL 一致，所以保留 CL 的編號。

## 7. 破壞性操作

`Vector<T>` 的方法。**修改接收者並回傳接收者本身**，所以 `(nreverse v)` 寫法與 `reverse` 相同，`v` 本身也會反轉。

| 名稱 | 形式 | 說明 |
|---|---|---|
| `nreverse` | `(nreverse v)` | 原地反轉 |
| `delete` `delete-if` `delete-if-not` `delete-duplicates` | `(delete v x)` | `remove`／`remove-if`／`filter`／`remove-duplicates` 的原地版本 |
| `nsubstitute` `nsubstitute-if` | `(nsubstitute v new old)` | `substitute` 系列的原地版本 |
| `nbutlast` | `(nbutlast v)` | 去掉尾端一個 |
| `fill` | `(fill v x)` | 所有元素設為 `x`。不改變長度 |
| `replace` | `(replace v src)` | 從開頭以 `src` 的元素覆寫。`(min (len v) (len src))` 個 |
| `map-into` | `(map-into v src f)` | `v[i] = (f src[i])`。個數同上 |
| `nconc` | `(nconc v w)` | 把 `w` 的元素加到 `v`。與 CL 不同，**不是修改共享結構**（`w` 不受影響） |
| `nreconc` | `(nreconc v w)` | `(nconc (nreverse v) w)` |
| `set-contents` | `(set-contents v src)` | 以 `src` 取代 `v` 的內容（長度也會改變） |
| `rplaca` `rplacd` | `(rplaca p x)` | 修改 `cons-cell` 的 `car`/`cdr` 並回傳該單元本身 |

接受的關鍵字：

| 破壞性版本 | 接受的關鍵字 |
|---|---|
| `delete` `nsubstitute` | `:key` `:test` `:test-not` `:start` `:end` `:from-end` `:count` |
| `delete-if` `delete-if-not` `nsubstitute-if` | `:key` `:start` `:end` `:from-end` `:count` |
| `delete-duplicates` | `:key` `:test` `:test-not` `:start` `:end` `:from-end` |
| `fill` | `:start` `:end` |
| `replace` | `:start1` `:end1` `:start2` `:end2`（接收者是 CL 的 `sequence-1`） |

`vector-push-extend`/`vector-pop` 就是 `Vector<T>` 的 `push`/`pop` 本身——`Vector<T>` 一律可以增長，所以沒有對應 CL 中「帶填充指標的向量」
與「簡單向量」之分的東西。

## 8. 高階函式

| 名稱 | 形式 | 型別 | 說明 |
|---|---|---|---|
| `identity` | `(identity x)` | `T→T` | 原樣回傳 |
| `const` | `(const x y)` | `(A,B)→A` | 回傳第一個引數 |
| `compose` | `(compose f g)` | `((fn (B) C),(fn (A) B))→(fn (A) C)` | 函式合成 `f∘g` |
| `flip` | `(flip f)` | `(fn (A B) C)→(fn (B A) C)` | 交換雙引數函式的引數順序 |
| `complement` | `(complement pred)` | `(fn (A) bool)→(fn (A) bool)` | 述詞的否定 |

沒有 CL 的 `constantly`（被忽略的引數的型別只出現在回傳型別中，無法決定）。請寫成 `(lambda ((x T)) A v)`。
