<!-- translated-from: docs/ja/reference/functions/sequences.md @ 8c7bff99b2cddddb57736d0567933bc024a5a467 -->
# Cặp, S-expression và dãy

Cặp generic `cons-cell`, dữ liệu S-expression `Sexpr`, symbol, các hàm trên dãy được viết dựa trên `Iter`,
và các hàm bậc cao.

## 1. Cặp `cons-cell<A,B>`

`cons`/`car`/`cdr` là hàm khởi tạo và các hàm truy cập trường của **kiểu cặp generic `cons-cell<A,B>`**
(một `defstruct` trong thư viện chuẩn). Các trường có thể được đọc bằng `variable::car`/`variable::cdr`
(cú pháp truy cập `defstruct` của [Tham chiếu cú pháp](../syntax.md#36-defstruct--struct-kiểu-do-người-dùng-định-nghĩa))
hoặc bằng `(car variable)`/`(cdr variable)`. Để thay đổi chúng, hãy dùng
`(setf variable::car v)`/`(setf variable::cdr v)`.

| Tên | Dạng | Kiểu | Mô tả |
|---|---|---|---|
| `cons` | `(cons a b)` | `(A,B)→cons-cell<A,B>` | Tạo một cặp |
| `car` | `(car p)` | `cons-cell<A,B>→A` | Phần tử đầu tiên |
| `cdr` | `(cdr p)` | `cons-cell<A,B>→B` | Phần còn lại |

`cons-cell` cũng đóng vai trò thay cho cú pháp tuple. Các hàm CL trả về nhiều giá trị (thương và số dư của
`floor`, giá trị và vị trí của `read-from-string`, v.v.) trả về một `cons-cell` trong ngôn ngữ này.

## 2. Dữ liệu S-expression `Sexpr`

Kiểu dữ liệu `Sexpr` do `read` trả về có 19 variant:
`int | i8 | i16 | i32 | u8 | u16 | u32 | f32 | f64 | char | bool | sym | str | cons | ratio | path | vector | array | tuple`.
`vector` và `array` là dữ liệu viết dưới dạng `#(..)` và `#nA(..)` ([Tham chiếu cú
pháp](../syntax.md#1-cú-pháp-từ-vựng)), lần lượt chứa một `Vector<Option<Sexpr>>` và một
`Array<Option<Sexpr>>`: `len`, `get` và các hàm khác dùng trực tiếp được trên `v` mà `(vector v)`
`tuple` là dữ liệu viết bằng `#{..}`, và `v` mà `(tuple v)` gắn là một `Vector<Option<Sexpr>>` mới
gồm các phần tử (để nhận tuple ở mọi độ dài bằng một kiểu).
gắn.
Các ô S-expression được xử lý không phải bằng `cons`/`car`/`cdr` tổng quát của chương 1 mà bằng các hàm
`sexpr-*`. Chúng được dùng chủ yếu trong thân `defmacro` để dựng và tách các dạng.

**Kiểu của dữ liệu S-expression là `Option<Sexpr>`.** Danh sách rỗng không phải một variant của `Sexpr` mà là
`none` của `Option`, và bản thân `Sexpr` nghĩa là "một S-expression không rỗng". Vì vậy các hàm `sexpr-*`
nhận và trả về `Option<Sexpr>`.

- `()` là danh sách rỗng ở chỗ mong đợi một `Option<Sexpr>` (cũng có thể viết `(Option::none)`)
- `Sexpr` được mở rộng ngầm ở chỗ mong đợi một `Option<Sexpr>` (không có chuyển đổi lúc chạy). Chiều ngược
  lại, dùng một `Option<Sexpr>` như một `Sexpr`, khẳng định "đây không phải danh sách rỗng", nên phải nêu
  tường minh bằng `match` hoặc `unwrap`
- Trong `match`, 19 variant của `Sexpr` và `none` có thể được viết **phẳng trong cùng một danh sách nhánh**
  ([Tham chiếu cú pháp](../syntax.md#43-match--so-khớp-mẫu))

| Tên | Dạng | Kiểu | Mô tả |
|---|---|---|---|
| `sexpr-cons` | `(sexpr-cons a b)` | `(Option<Sexpr>,Option<Sexpr>)→Option<Sexpr>` | Tạo một ô `Sexpr` |
| `sexpr-car` | `(sexpr-car s)` | `Option<Sexpr>→Option<Sexpr>` | Phần tử đầu tiên. **Danh sách rỗng cho danh sách rỗng** (như trong CL). Panic với một atom không phải `Cons` |
| `sexpr-cdr` | `(sexpr-cdr s)` | `Option<Sexpr>→Option<Sexpr>` | Phần còn lại. **Danh sách rỗng cho danh sách rỗng** (như trong CL). Panic với một atom không phải `Cons` |
| `sexpr-consp` | `(sexpr-consp s)` | `Option<Sexpr>→bool` | Có phải là `Cons` không |
| `sexpr-null` | `(sexpr-null s)` | `Option<Sexpr>→bool` | Có phải danh sách rỗng không |
| `sexpr-atom` | `(sexpr-atom s)` | `Option<Sexpr>→bool` | Có phải không phải `Cons` không |
| `sexpr-symp` | `(sexpr-symp s)` | `Option<Sexpr>→bool` | Có phải là `Sym` (symbol) không |
| `sexpr-int` | `(sexpr-int s)` | `Option<Sexpr>→int` | Nội dung của variant `int` (fixnum hoặc bignum). Panic với kiểu khác |
| `sexpr-i8` `sexpr-i16` `sexpr-i32` `sexpr-u8` `sexpr-u16` `sexpr-u32` | `(sexpr-i32 s)` | `Option<Sexpr>→W` | Nội dung của variant có độ rộng đó. Panic với kiểu khác |
| `sexpr-f32` `sexpr-f64` | `(sexpr-f64 s)` | `Option<Sexpr>→f32` / `→f64` | Nội dung của các variant dấu phẩy động. Panic với kiểu khác |
| `sexpr-char` | `(sexpr-char s)` | `Option<Sexpr>→char` | Nội dung của một `Char`. Panic với kiểu khác |
| `sexpr-bool` | `(sexpr-bool s)` | `Option<Sexpr>→bool` | Nội dung của một `Bool`. Panic với kiểu khác |
| `sexpr-str` | `(sexpr-str s)` | `Option<Sexpr>→string` | Nội dung của một `Str`. Panic với kiểu khác |
| `sexpr-sym-name` | `(sexpr-sym-name s)` | `Option<Sexpr>→string` | Tên của một `Sym`. Panic với kiểu khác |
| `eq` `eql` | `(op a b)` | `(Option<Sexpr>,Option<Sexpr>)→bool` | So sánh đồng nhất (`Cons`/`Str` so sánh đồng nhất đối tượng, phần còn lại so sánh giá trị) |
| `equal` | `(equal a b)` | `(Option<Sexpr>,Option<Sexpr>)→bool` | Bằng nhau về cấu trúc (`Cons` đệ quy, `Str` theo nội dung) |
| `equalp` | `(equalp a b)` | `(Option<Sexpr>,Option<Sexpr>)→bool` | Như `equal`, cộng thêm so sánh không phân biệt hoa thường và so sánh số giữa các kiểu |
| `sexpr-append` | `(sexpr-append a b)` | `(Option<Sexpr>,Option<Sexpr>)→Option<Sexpr>` | Nối hai danh sách `Sexpr` (không phá hủy). `,@` khai triển thành hàm này |
| `sexpr-map` | `(sexpr-map f lst)` | `((fn (Option<Sexpr>) Option<Sexpr>),Option<Sexpr>)→Option<Sexpr>` | Một danh sách `Sexpr` mới với `f` áp dụng cho từng phần tử của một danh sách `Sexpr` (`map` của chương 4 dành cho `Iter` và không duyệt được danh sách `Sexpr`) |

Có chín hàm truy cập số, mỗi kiểu một hàm, vì một `Sexpr` là "nơi duy nhất mà kiểu của một giá trị không
được viết ở chỗ nào khác". Một `u8` đưa vào một `Sexpr` đi vào dưới dạng variant `u8` và chỉ ra được bằng
`(sexpr-u8 s)`. Truyền nó cho `(sexpr-int s)` sẽ panic; nó không bao giờ mở rộng đáp án một cách im lặng.
Các số nguyên trong dữ liệu đã đọc (`'(1 2 3)`, đối số macro) thuộc variant `int` và được đọc bằng
`(sexpr-int s)`.

Các danh sách `Sexpr` không có thao tác phá hủy như `rplaca`/`nconc`. Một ô `Sexpr` không thể thay đổi sau
khi được tạo.

## 3. Symbol

`symbol` là kiểu của chính các symbol. Nó được chuyển đổi ngầm ở chỗ cần một `Sexpr`, nhưng không tự động
theo chiều ngược lại.

| Tên | Dạng | Kiểu | Mô tả |
|---|---|---|---|
| `symbol->string` | `(symbol->string s)` | `symbol→string` | Lấy tên của symbol |
| `string->symbol` | `(string->symbol s)` | `string→symbol` | Tạo một symbol từ một chuỗi (intern nó) |
| `keywordp` | `(keywordp s)` | `symbol→bool` | Có phải là keyword (`:name`) không. Dấu hai chấm là một phần của tên, nên phép kiểm tra nhìn vào ký tự đầu ([Tham chiếu cú pháp](../syntax.md#1-cú-pháp-từ-vựng)) |

Về `gensym`, xem [Macro](system.md#8-macro).

## 4. Các hàm trên dãy dựa trên `Iter`

Các hàm trên dãy là **các hàm generic trên trait `Iter`**. Từ một tập hợp, lấy một iterator bằng
`(iter coll)` và truyền nó (`Vector<T>` / `HashTable<K,V>` / `Array<T>` hỗ trợ điều này; một danh sách
`Sexpr` không triển khai `Iter`, nên các hàm này không áp dụng cho nó). **Một tập hợp kết quả được trả về
dưới dạng một `Vector` mới.** `Iter<A>` trong các bảng nghĩa là "mọi triển khai của `Iter` có `Item` là
`A`". Để duyệt lại `Vector` được trả về, hãy truyền `(iter result)`.

Các hàm nhận một vị từ (tương ứng với họ `-if` của CL):

| Tên | Dạng | Kiểu | Mô tả |
|---|---|---|---|
| `map` | `(map it f)` | `(Iter<A>,(fn (A) U))→Vector<U>` | Ánh xạ |
| `filter` | `(filter it pred)` | `(Iter<A>,(fn (A) bool))→Vector<A>` | Chỉ các phần tử thỏa vị từ |
| `remove-if` | `(remove-if it pred)` | `(Iter<A>,(fn (A) bool))→Vector<A>` | Bỏ các phần tử thỏa vị từ |
| `find-if` | `(find-if it pred)` | `(Iter<A>,(fn (A) bool))→Option<A>` | Phần tử đầu tiên thỏa vị từ |
| `position-if` | `(position-if it pred)` | `(Iter<A>,(fn (A) bool))→Option<int>` | Vị trí đầu tiên thỏa vị từ |
| `count-if` | `(count-if it pred)` | `(Iter<A>,(fn (A) bool))→int` | Có bao nhiêu phần tử thỏa vị từ |
| `every` | `(every it pred)` | `(Iter<A>,(fn (A) bool))→bool` | Mọi phần tử có thỏa vị từ không |
| `any` | `(any it pred)` | `(Iter<A>,(fn (A) bool))→bool` | Có phần tử nào thỏa vị từ không (tương ứng với `some` của CL; một cái tên không trùng với hàm khởi tạo `Some`) |
| `foldl` | `(foldl it f init)` | `(Iter<A>,(fn (B A) B),B)→B` | Gấp trái |
| `foldr` | `(foldr it f init)` | `(Iter<A>,(fn (A B) B),B)→B` | Gấp phải |

Đánh chỉ số, độ dài và cắt lát:

| Tên | Dạng | Kiểu | Mô tả |
|---|---|---|---|
| `length` | `(length it)` | `Iter<A>→int` | Số phần tử |
| `append` | `(append a b ...)` | `(Iter<A>,Iter<A>,...)→Vector<A>` | Nối các iterator. Có thể đưa ba trở lên |
| `concatenate` | `(concatenate 'vector it ...)` / `(concatenate 'string s ...)` | `→Vector<A>` / `→string` | `concatenate` của CL. Kiểu kết quả được viết là **một literal symbol có dấu nháy** (CL dùng một type specifier lúc chạy). `'vector` nhận một trở lên, `'string` nhận không hoặc nhiều hơn (`""` cho không). Danh sách `Sexpr` không được bao phủ (dùng `sexpr-append`) |
| `reverse` | `(reverse it)` | `Iter<A>→Vector<A>` | Đảo ngược (không phá hủy) |
| `nth` | `(nth n it)` | `(int,Iter<A>)→Option<A>` | Phần tử `n` (`None` khi ngoài phạm vi) |
| `elt` | `(elt it n)` | `(Iter<A>,int)→Option<A>` | `nth` với thứ tự đối số ngược lại |
| `take` | `(take it n)` | `(Iter<A>,int)→Vector<A>` | `n` phần tử đầu tiên |
| `subseq` | `(subseq it start end)` | `(Iter<A>,int,int)→Vector<A>` | `[start,end)` (`end` bị kẹp về độ dài) |
| `last` | `(last it)` | `Iter<A>→Option<A>` | **Phần tử** cuối cùng (không phải "ô cuối cùng" như trong CL) |
| `butlast` | `(butlast it)` | `Iter<A>→Vector<A>` | Tất cả trừ phần tử cuối |

Các hàm yêu cầu ràng buộc `Eq` / `Ord` (chúng so sánh qua một trait thay vì một vị từ;
[Các trait chuẩn](traits.md#2-eq--ord-so-sánh)):

| Tên | Dạng | Kiểu | Mô tả |
|---|---|---|---|
| `member` | `(member x it)` | `(A,Iter<A>)→bool` với `Eq A` | Có phần tử nào bằng `x` không (khác CL, là một `bool`, không phải phần còn lại của danh sách) |
| `find` | `(find x it)` | `(A,Iter<A>)→Option<A>` với `Eq A` | Phần tử đầu tiên bằng `x` |
| `position` | `(position x it)` | `(A,Iter<A>)→Option<int>` với `Eq A` | Vị trí đầu tiên bằng `x` |
| `count` | `(count x it)` | `(A,Iter<A>)→int` với `Eq A` | Có bao nhiêu phần tử bằng `x` |
| `sort` | `(sort it cmp)` | `(Iter<A>,(fn (A A) bool))→Vector<A>` | `(sort sequence predicate)` của CL. Sắp xếp ổn định, không phá hủy. `cmp` là `true` khi "đối số đầu đứng trước đối số thứ hai một cách chặt chẽ" |
| `assoc` | `(assoc k it)` | `(K,Iter<cons-cell<K,V>>)→Option<cons-cell<K,V>>` với `Eq K` | Cặp đầu tiên có `car` bằng `k`. Lấy giá trị ra bằng `(cdr p)` |

Các hàm này và nhiều hàm của chương 5 cũng nhận các đối số keyword của CL `:key` / `:test` /
`:test-not` / `:start` / `:end` / `:from-end` / `:count` (chương 6).

## 5. Phần còn lại của các hàm trên dãy của CL

Tất cả là các hàm generic trên `Iter`, như ở chương 4. Các tập hợp kết quả được trả về dưới dạng các
`Vector` mới.

| Tên | Dạng | Kiểu | Mô tả |
|---|---|---|---|
| `first`…`tenth` | `(first it)` | `Iter<A>→Option<A>` | Các chỉ số có tên của CL |
| `rest` | `(rest it)` | `Iter<A>→Vector<A>` | Tất cả trừ phần tử đầu (một `Vector` mới, không phải đuôi dùng chung) |
| `copy-seq` | `(copy-seq it)` | `Iter<A>→Vector<A>` | Hiện thực hóa một iterator thành một `Vector` (`copy-seq`/`copy-list` của CL) |
| `revappend` | `(revappend a b)` | `(Iter<A>,Iter<A>)→Vector<A>` | `a` đảo ngược, theo sau là `b` |
| `Vector::filled` | `(Vector::filled n x)` | `(int,T)→Vector<T>` | `n` bản sao của `x` (`make-list`/`make-sequence` của CL). Như với `Vector::new`, đối số kiểu đến từ kiểu mong đợi, nên một `let` trần cần `the` |
| `member-if` `member-if-not` | `(member-if it pred)` | `(Iter<A>,(fn (A) bool))→bool` | Như `member`, là một **`bool`** (iterator không có đuôi để trả về) |
| `notany` `notevery` | `(notany it pred)` | `(Iter<A>,(fn (A) bool))→bool` | Phủ định của `any`/`every` |
| `find-if-not` `position-if-not` `count-if-not` `remove-if-not` | `(op it pred)` | Cùng kiểu với các phiên bản khẳng định | Các phiên bản với vị từ bị phủ định |
| `remove` | `(remove x it)` | `(A,Iter<A>)→Vector<A>` với `Eq A` | Bỏ theo giá trị |
| `remove-duplicates` | `(remove-duplicates it)` | `Iter<A>→Vector<A>` với `Eq A` | Bỏ các phần tử trùng. Như trong CL, **lần xuất hiện cuối cùng được giữ** (`:from-end true` giữ lần đầu) |
| `substitute` `substitute-if` | `(substitute new old it)` | `(A,A,Iter<A>)→Vector<A>` | Thay thế theo giá trị / vị từ |
| `assoc-if` `rassoc` `rassoc-if` | `(rassoc v it)` | trên `Iter<cons-cell<K,V>>` | Các phiên bản vị từ và phía giá trị của `assoc` |
| `acons` | `(acons k v it)` | `(K,V,Iter<cons-cell<K,V>>)→Vector<cons-cell<K,V>>` | Thêm một cặp vào đầu |
| `pairlis` | `(pairlis ks vs)` | `(Iter<K>,Iter<V>)→Vector<cons-cell<K,V>>` | Ghép cặp hai dãy. Dừng ở dãy ngắn hơn |
| `map2` | `(map2 a b f)` | `(Iter<A>,Iter<B>,(fn (A B) U))→Vector<U>` | `mapcar` của CL trên nhiều dãy. Dừng ở dãy ngắn hơn |
| `mapc` | `(mapc it f)` | `(Iter<A>,(fn (A) ()))→()` | Ánh xạ vì tác dụng phụ |
| `mapcan` | `(mapcan it f)` | `(Iter<A>,(fn (A) Vector<U>))→Vector<U>` | Ánh xạ và nối |
| `maplist` | `(maplist it f)` | `(Iter<A>,(fn (Vector<A>) U))→Vector<U>` | Ánh xạ trên các **đuôi** liên tiếp |
| `mapl` | `(mapl it f)` | `(Iter<A>,(fn (Vector<A>) ()))→()` | Ánh xạ trên các đuôi vì tác dụng phụ (đối ứng `maplist` của `mapc`) |
| `mapcon` | `(mapcon it f)` | `(Iter<A>,(fn (Vector<A>) Vector<U>))→Vector<U>` | Ánh xạ trên các đuôi và nối (đối ứng `maplist` của `mapcan`) |
| `search` | `(search it sub)` | `(Iter<A>,Iter<A>)→Option<int>` với `Eq A` | Vị trí `sub` xuất hiện đầu tiên. Nếu bên nhận là một `string`, phương thức của `string` được chọn ([Chuỗi](collections.md#1-chuỗi-string)) |
| `mismatch` | `(mismatch a b)` | `(Iter<A>,Iter<A>)→Option<int>` với `Eq A` | Vị trí đầu tiên mà chúng khác nhau. `none` nếu chúng bằng nhau |
| `merge` | `(merge a b less)` | `(Iter<A>,Iter<A>,(fn (A A) bool))→Vector<A>` | Trộn. CL yêu cầu đầu vào đã sắp xếp; hàm này sắp xếp phần nối |
| `adjoin` | `(adjoin x it)` | `(A,Iter<A>)→Vector<A>` với `Eq A` | Thêm `x` **vào đầu** nếu nó chưa có |
| `union` `intersection` `set-difference` `set-exclusive-or` | `(op a b)` | `(Iter<A>,Iter<A>)→Vector<A>` với `Eq A` | Các phép toán tập hợp. CL không quy định thứ tự; ở đây nó ổn định, **theo thứ tự xuất hiện đầu tiên** |
| `subsetp` | `(subsetp a b)` | `(Iter<A>,Iter<A>)→bool` với `Eq A` | Phép bao hàm |
| `tailp` `ldiff` | `(tailp tail whole)` | `(Iter<A>,Iter<A>)→bool` / `→Vector<A>` | Có phải hậu tố không / phần đứng trước hậu tố. CL hỏi về **cấu trúc dùng chung**, nhưng ở đây không có cấu trúc nào để dùng chung, nên hàm này hỏi về một hậu tố **theo giá trị** |
| `seq-equals` | `(seq-equals a b)` | `(Vector<A>,Vector<A>)→bool` với `Eq A` | Bằng nhau theo từng phần tử. Bản thân `Vector<T>` không triển khai `Eq` |
| `caar`…`cddddr` | `(cadr p)` | trên các cặp lồng nhau | 28 hàm của CL. Chúng duyệt **các cặp, không phải danh sách**: `cadr` nhận một `cons-cell<A,cons-cell<B,C>>` |

Những gì CL có mà ngôn ngữ này không có: `list*` (không có khái niệm danh sách không chuẩn mà đuôi bị thay
thế), `copy-tree`/`copy-alist`/`sublis`/`subst`/`subst-if` (không kiểu nào có thể mô tả việc duyệt một cây
không đồng nhất có độ sâu bất kỳ; với một cây `Sexpr`, `equal` tương ứng với `tree-equal`), họ property list
`getf`/`get-properties`/`symbol-plist`/`remprop` (không có biểu diễn dưới dạng danh sách không kiểu xen kẽ
khóa và giá trị; `assoc` (danh sách kết hợp) hoặc `HashTable` đảm nhận cùng vai trò), và các hàm chuyển đổi
giữa `Vector<T>` và danh sách `Sexpr` (các phần tử của một danh sách `Sexpr` có thể có kiểu khác nhau, nên
không thể viết bằng một kiểu phần tử `T` duy nhất).

## 6. Đối số keyword

Các hàm của chương 4 và 5 nhận các keyword dãy của CL `:key` / `:test` / `:test-not` / `:start` / `:end` /
`:from-end` / `:count`. Tất cả đều **tùy chọn**.

| Keyword | Kiểu | Ý nghĩa |
|---|---|---|
| `:key` | `(fn (A) A)` | Một phép chiếu áp dụng cho từng phần tử trước khi so sánh hoặc kiểm tra |
| `:test` | `(fn (A A) bool)` | Một phép kiểm tra bằng nhau dùng thay cho `equals` từ ràng buộc `Eq`. Đối số đầu là **mục đang tìm**, đối số thứ hai là phần tử (sau `:key`), cùng thứ tự như CL |
| `:test-not` | `(fn (A A) bool)` | Phủ định của `:test` |
| `:start` `:end` | `int` | Cửa sổ `[start, end)` để quét. Chỉ số tính so với toàn dãy |
| `:from-end` | `bool` | Một phép tìm kiếm trả lời bằng lần khớp **cuối cùng**. Kết hợp với `:count`, các phần tử bị tác động được lấy từ cuối |
| `:count` | `int` | Số phần tử tối đa mà các họ `remove` / `substitute` tác động |

Hàm nào nhận keyword nào theo CL:

| Hàm | Các keyword nhận |
|---|---|
| `find` `position` | `:key` `:test` `:test-not` `:start` `:end` `:from-end` |
| `count` | `:key` `:test` `:test-not` `:start` `:end` |
| `member` `adjoin` | `:key` `:test` `:test-not` |
| `remove` `substitute` | Tất cả những keyword trên (gồm `:count`) |
| `remove-duplicates` | `:key` `:test` `:test-not` `:start` `:end` `:from-end` |
| `find-if` `find-if-not` `position-if` `position-if-not` | `:key` `:start` `:end` `:from-end` |
| `count-if` `count-if-not` | `:key` `:start` `:end` |
| `member-if` `member-if-not` | `:key` |
| `remove-if` `remove-if-not` `substitute-if` | `:key` `:start` `:end` `:from-end` `:count` |
| `assoc` `rassoc` | `:key` `:test` `:test-not` (`:key` của `assoc` áp dụng cho `car`, của `rassoc` cho `cdr`) |
| `assoc-if` `rassoc-if` | `:key` |
| `sort` `merge` | `:key` |
| `union` `intersection` `set-difference` `set-exclusive-or` `subsetp` | `:key` `:test` `:test-not` |
| `search` | `:key` `:test` `:test-not` `:from-end` `:start` `:end` `:sub-start` `:sub-end` |
| `mismatch` | `:key` `:test` `:test-not` `:from-end` `:start1` `:end1` `:start2` `:end2` |

```lisp
(find 2 (iter v) :key (lambda ((x int)) int (abs x)))   ; → (some -2)
(remove 2 (iter v) :count 1 :from-end true)             ; chỉ bỏ một phần tử, từ cuối
(position 3 (iter v) :start 1)                          ; chỉ số tính so với toàn dãy
```

**Khác biệt so với CL**:

1. **Phép chiếu của `:key` giữ nguyên trong kiểu phần tử** (`(fn (A) A)`). Nó không thể chiếu sang kiểu
   khác như trong CL: một biến kiểu thừa không thể được xác định khi đối số bị bỏ qua. Ở nơi cần phép chiếu
   sang kiểu khác, hãy truyền một lambda cho họ `-if` thay vào đó
   (`(find-if it (lambda ((p ...)) bool (= (car p) 3)))`).
2. **Trong các phép tìm kiếm theo mục, `:key` chỉ áp dụng cho các phần tử** (không áp dụng cho mục đang
   tìm). Đây là cùng quy tắc như `find`/`position`/`count`/`member`/`remove`/`substitute` của CL. Trong các
   phép toán tập hợp cả hai phía đều là phần tử, nên nó áp dụng cho cả hai.
3. **Chỉ các keyword của `search` được đặt tên thay vì đánh số.** Trong CL, `:start1`/`:end1` dành cho
   **mẫu** và `:start2`/`:end2` cho dãy được tìm. Trong ngôn ngữ này bên nhận đứng đầu, nên cùng các con số
   sẽ có nghĩa ngược lại, và là ngược một cách im lặng. `:start`/`:end` dành cho bên nhận và
   `:sub-start`/`:sub-end` cho mẫu, nên một `:start1` bất cẩn cho lỗi "unknown keyword". `mismatch` và
   `replace` có cùng thứ tự đối số như CL, nên chúng giữ các con số của CL.

## 7. Các thao tác phá hủy

Các phương thức của `Vector<T>`. **Chúng sửa đổi bên nhận và trả về chính bên nhận**, nên `(nreverse v)`
được viết giống `reverse` và chính `v` cũng bị đảo ngược.

| Tên | Dạng | Mô tả |
|---|---|---|
| `nreverse` | `(nreverse v)` | Đảo ngược tại chỗ |
| `delete` `delete-if` `delete-if-not` `delete-duplicates` | `(delete v x)` | Các phiên bản tại chỗ của `remove` / `remove-if` / `filter` / `remove-duplicates` |
| `nsubstitute` `nsubstitute-if` | `(nsubstitute v new old)` | Các phiên bản tại chỗ của họ `substitute` |
| `nbutlast` | `(nbutlast v)` | Bỏ phần tử cuối |
| `fill` | `(fill v x)` | Đặt mọi phần tử thành `x`. Độ dài không đổi |
| `replace` | `(replace v src)` | Ghi đè từ đầu bằng các phần tử của `src`. `(min (len v) (len src))` phần tử |
| `map-into` | `(map-into v src f)` | `v[i] = (f src[i])`. Cùng số lượng như trên |
| `nconc` | `(nconc v w)` | Nối các phần tử của `w` vào `v`. Khác CL, **nó không ghi lại cấu trúc dùng chung** (`w` không bị ảnh hưởng) |
| `nreconc` | `(nreconc v w)` | `(nconc (nreverse v) w)` |
| `set-contents` | `(set-contents v src)` | Thay nội dung của `v` bằng `src` (độ dài cũng thay đổi) |
| `rplaca` `rplacd` | `(rplaca p x)` | Ghi lại `car`/`cdr` của một `cons-cell` và trả về chính ô đó |

Các keyword nhận:

| Phiên bản phá hủy | Các keyword nhận |
|---|---|
| `delete` `nsubstitute` | `:key` `:test` `:test-not` `:start` `:end` `:from-end` `:count` |
| `delete-if` `delete-if-not` `nsubstitute-if` | `:key` `:start` `:end` `:from-end` `:count` |
| `delete-duplicates` | `:key` `:test` `:test-not` `:start` `:end` `:from-end` |
| `fill` | `:start` `:end` |
| `replace` | `:start1` `:end1` `:start2` `:end2` (bên nhận là `sequence-1` của CL) |

`vector-push-extend`/`vector-pop` đơn giản là `push`/`pop` của `Vector<T>`. Một `Vector<T>` luôn tăng trưởng
được, nên không có gì tương ứng với sự phân biệt của CL giữa "một vector có fill pointer" và "một vector
đơn giản".

## 8. Hàm bậc cao

| Tên | Dạng | Kiểu | Mô tả |
|---|---|---|---|
| `identity` | `(identity x)` | `T→T` | Trả về đối số của nó |
| `const` | `(const x y)` | `(A,B)→A` | Trả về đối số đầu tiên |
| `compose` | `(compose f g)` | `((fn (B) C),(fn (A) B))→(fn (A) C)` | Hợp hàm `f∘g` |
| `flip` | `(flip f)` | `(fn (A B) C)→(fn (B A) C)` | Hoán đổi các đối số của một hàm hai đối số |
| `complement` | `(complement pred)` | `(fn (A) bool)→(fn (A) bool)` | Phủ định của một vị từ |

Không có `constantly` của CL (kiểu của đối số bị bỏ qua chỉ xuất hiện trong kiểu trả về và không thể được
xác định). Hãy viết `(lambda ((x T)) A v)`.
