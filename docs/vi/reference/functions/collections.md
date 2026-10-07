<!-- translated-from: docs/ja/reference/functions/collections.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# Chuỗi, ký tự và tập hợp

`string`, `char`, `Vector<T>`, `HashTable<K,V>`, `Array<T>` và `BitVector`.

## 1. Chuỗi `string`

Chuỗi là bất biến.

| Tên | Dạng | Kiểu | Mô tả |
|---|---|---|---|
| `upcase` | `(upcase s)` | `string→string` | Chuyển thành chữ hoa (chỉ ASCII). Như `string-upcase` của CL, trả về một chuỗi mới. Chuỗi là bất biến, nên không có `nstring-upcase` phá hủy; hàm này thế chỗ nó |
| `downcase` | `(downcase s)` | `string→string` | Chuyển thành chữ thường (chỉ ASCII). Thế chỗ `nstring-downcase` |
| `capitalize` | `(capitalize s)` | `string→string` | Viết hoa chữ cái đầu của mỗi từ và viết thường phần còn lại (`string-capitalize` của CL). Một từ là một dãy tối đa các chữ cái và chữ số |
| `length` | `(length s)` | `string→int` | Số ký tự |
| `ref` | `(ref s i)` | `(string,int)→char` | Ký tự thứ `i`. Panic khi ngoài phạm vi |
| `substring` | `(substring s start end)` | `(string,int,int)→string` | Chuỗi con `[start,end)` |
| `append` | `(append s1 s2 ...)` | `(string,string,...)→string` | Nối chuỗi. Có thể đưa ba trở lên (giống `(concatenate 'string ...)`) |
| `<` `<=` `>` `>=` | `(op s1 s2)` | `(string,string)→bool` | So sánh theo thứ tự từ điển |
| `lt` | `(lt s1 s2)` | `(string,string)→bool` | Nhỏ hơn chặt theo thứ tự từ điển (giống `<`) |
| `eq` `eql` | `(op s1 s2)` | `(string,string)→bool` | So sánh đồng nhất (có phải cùng một đối tượng hay không, không phải cùng nội dung) |
| `equal` | `(equal s1 s2)` | `(string,string)→bool` | So sánh nội dung (phân biệt hoa thường) |
| `equalp` | `(equalp s1 s2)` | `(string,string)→bool` | So sánh nội dung (không phân biệt hoa thường, chỉ ASCII) |
| `/=` | `(/= s1 s2)` | `(string,string)→bool` | Nội dung có khác nhau không (`string/=` của CL. Dạng nhiều đối số so sánh các cặp liền kề) |
| `lessp` `greaterp` `not-lessp` `not-greaterp` | `(op s1 s2)` | `(string,string)→bool` | Thứ tự không phân biệt hoa thường (`string-lessp` của CL, v.v.). Với tiền tố chung, chuỗi ngắn hơn nhỏ hơn |
| `string::filled` | `(string::filled n c)` | `(int,char)→string` | Một chuỗi gồm `n` bản sao của `c` (`make-string` của CL) |
| `search` | `(search s sub)` | `(string,string)→Option<int>` | Vị trí `sub` xuất hiện đầu tiên. **`search` của CL có thứ tự đối số ngược lại** (`(search pattern sequence)`). Chuỗi rỗng được tìm thấy ở 0. Về các keyword, xem [đối số keyword của dãy](sequences.md#6-đối-số-keyword) |
| `mismatch` | `(mismatch a b)` | `(string,string)→Option<int>` | Vị trí đầu tiên mà chúng khác nhau. `none` chỉ khi chúng `equal`. Nếu một chuỗi là tiền tố của chuỗi kia, là cuối của chuỗi ngắn hơn. Keyword như trên |
| `trim` `left-trim` `right-trim` | `(trim s)` / `(trim s bag)` | `(string &optional string)→string` | Bỏ các ký tự có trong `bag` ở cả hai đầu / bên trái / bên phải (`string-trim` của CL, v.v.). Không có `bag`, là khoảng trắng `" \t\n\r"` |
| `split` | `(split s sep)` | `(string,string)→Vector<string>` | Tách tại `sep`. CL không có thứ tương ứng. Các dấu phân cách liên tiếp tạo ra phần tử rỗng. Panic nếu `sep` rỗng |
| `to-string` | `(to-string x)` | `T→string` | Chuyển thành chuỗi như `~a`. Được triển khai cho `int`/`i32`/`f64`/`bool`/`char`/`string` (`princ-to-string` của CL) |
| `string->utf8` | `(string->utf8 s)` | `string→Vector<int>` | Mã hóa thành UTF-8 (mỗi phần tử 0..255) |
| `utf8->string` | `(utf8->string bytes)` | `Vector<int>→Option<string>` | Giải mã. `none` nếu không phải UTF-8 hợp lệ |

## 2. Ký tự `char`

Một `char` là một giá trị vô hướng Unicode. Chuyển đổi hoa thường và phân loại chỉ xử lý dải ASCII.

| Tên | Dạng | Kiểu | Mô tả |
|---|---|---|---|
| `upcase` | `(upcase c)` | `char→char` | Chuyển thành chữ hoa (chỉ ASCII) |
| `downcase` | `(downcase c)` | `char→char` | Chuyển thành chữ thường (chỉ ASCII) |
| `<` `<=` `>` `>=` | `(op c1 c2)` | `(char,char)→bool` | So sánh theo code point |
| `lt` | `(lt c1 c2)` | `(char,char)→bool` | Nhỏ hơn chặt theo code point (giống `<`) |
| `alphap` | `(alphap c)` | `char→bool` | Có phải chữ cái ASCII không |
| `digitp` | `(digitp c)` | `char→bool` | Có phải chữ số ASCII không |
| `eq` `eql` `equal` | `(op c1 c2)` | `(char,char)→bool` | So sánh giá trị |
| `equalp` | `(equalp c1 c2)` | `(char,char)→bool` | So sánh giá trị bỏ qua hoa thường (`char-equal` của CL) |
| `/=` | `(/= c1 c2)` | `(char,char)→bool` | Các giá trị có khác nhau không (`char/=` của CL. **Dạng nhiều đối số so sánh các cặp liền kề**, khác CL, vốn hỏi liệu mọi cặp có khác nhau không) |
| `lessp` `greaterp` `not-lessp` `not-greaterp` | `(op c1 c2)` | `(char,char)→bool` | Thứ tự không phân biệt hoa thường (`char-lessp` của CL, v.v.) |
| `upper-casep` `lower-casep` `both-casep` | `(op c)` | `char→bool` | Chữ hoa / chữ thường / có phân biệt hoa thường hay không (`upper-case-p` của CL, v.v.) |
| `alphanumericp` | `(alphanumericp c)` | `char→bool` | Một chữ cái hoặc một chữ số (cùng tên như trong CL) |
| `graphicp` | `(graphicp c)` | `char→bool` | Có in ra được không. Gồm dấu cách, không gồm xuống dòng hay tab (`graphic-char-p` của CL) |
| `standardp` | `(standardp c)` | `char→bool` | Có phải một trong 96 ký tự chuẩn của CL không, tức là `graphicp` cộng xuống dòng (`standard-char-p` của CL) |
| `char->int` | `(char->int c)` | `char→int` | Giá trị vô hướng Unicode (chiều ngược lại là `int->char`/`try-int->char` trong [Số](numbers.md#1-số-nguyên-độ-rộng-cố-định)). Tương ứng với `char-code`/`char-int` của CL |
| `char->string` | `(char->string c)` | `char→string` | Một chuỗi một ký tự. Hàm `string` của CL đảm nhận việc này bằng cách nhận một designator, nhưng ngôn ngữ này không có designator, nên chiều chuyển đổi nằm trong tên |
| `digit-weight` | `(digit-weight c)` / `(digit-weight c radix)` | `(char &optional int)→Option<int>` | **Trọng số** của chữ số trong cơ số đó (`digit-char-p` của CL). `digitp` là một hàm riêng trả về `bool` |
| `digit->char` | `(digit->char w)` / `(digit->char w radix)` | `(int &optional int)→Option<char>` | Ký tự cho trọng số `w`. Chữ hoa từ 10 trở lên (`digit-char` của CL; cơ số tối đa là 36) |
| `char->name` | `(char->name c)` | `char→Option<string>` | Tên của ký tự. Chỉ các ký tự có tên mà reader đọc được mới có tên (`char-name` của CL) |
| `name->char` | `(name->char s)` | `string→Option<char>` | Ký tự cho một tên. Không phân biệt hoa thường, và cũng chấp nhận các bí danh của reader (`linefeed`/`null`) (`name-char` của CL) |

Không có hằng số tương ứng với `char-code-limit` (giới hạn trên của `char` do Unicode đặt ra, không phải do
ngôn ngữ).

## 3. `Vector<T>`

Một mảng có thể tăng trưởng.

| Tên | Dạng | Kiểu | Mô tả |
|---|---|---|---|
| `new` | `(Vector::new)` | `()→Vector<T>` | Tạo một vector rỗng. Đối số kiểu đến từ kiểu mong đợi, nên trong một `let` trần hãy viết `(the Vector<i32> (Vector::new))` |
| `filled` | `(Vector::filled n x)` | `(int,T)→Vector<T>` | `n` bản sao của `x` |
| `push` | `(push v x)` | `(Vector<T>,T)→Unit` | Nối vào cuối |
| `get` | `(get v i)` | `(Vector<T>,int)→T` | Đọc phần tử `i`. Panic khi ngoài phạm vi |
| `set` | `(set v i x)` | `(Vector<T>,int,T)→Unit` | Thay đổi phần tử `i`. Panic khi ngoài phạm vi. Cũng có thể viết `(setf (get v i) x)` |
| `len` | `(len v)` | `Vector<T>→int` | Số phần tử |
| `pop` | `(pop v)` | `Vector<T>→Option<T>` | Bỏ phần tử cuối và trả về nó. `None` nếu rỗng (không như `get`/`set`, nó không panic) |
| `iter` | `(iter v)` | `Vector<T>→vector-iter<T>` | Tạo một iterator triển khai `Iter` |
| `pushnew` | `(pushnew v x)` | `(Vector<T>,T)→Unit` với `Eq T` | Nối `x` nếu chưa có phần tử bằng nó (`pushnew` của CL. Nó không cần ghi lại một place, nên là một phương thức chứ không phải macro) |

`map`/`filter` và các hàm tương tự là [các hàm trên dãy](sequences.md#4-các-hàm-trên-dãy-dựa-trên-iter): hãy
truyền vector qua `iter`, như `(map (iter v) f)`. Các thao tác phá hủy (`nreverse`, `delete`, v.v.) nằm ở
[Các thao tác phá hủy](sequences.md#7-các-thao-tác-phá-hủy).

## 4. `HashTable<K,V>`

| Tên | Dạng | Kiểu | Mô tả |
|---|---|---|---|
| `new` | `(HashTable::new)` | `()→HashTable<K,V>` | Tạo một bảng rỗng |
| `get` | `(get h k)` | `(HashTable<K,V>,K)→Option<V>` | Tra cứu |
| `set` | `(set h k v)` | `(HashTable<K,V>,K,V)→Unit` | Chèn hoặc ghi đè |
| `remove` | `(remove h k)` | `(HashTable<K,V>,K)→Option<V>` | Bỏ mục và trả về giá trị cũ, nếu có |
| `count` | `(count h)` | `HashTable<K,V>→int` | Số mục |
| `clear` | `(clear h)` | `HashTable<K,V>→Unit` | Bỏ tất cả |
| `keys` | `(keys h)` | `HashTable<K,V>→Vector<K>` | Một ảnh chụp các khóa |
| `values` | `(values h)` | `HashTable<K,V>→Vector<V>` | Một ảnh chụp các giá trị |
| `entries` | `(entries h)` | `HashTable<K,V>→Vector<cons-cell<K,V>>` | Một ảnh chụp các cặp `(k . v)` |
| `iter` | `(iter h)` | `HashTable<K,V>→hashtable-iter<K,V>` | Một iterator triển khai `Iter`. Các phần tử là các `cons-cell` `(k . v)`. Tương ứng với `with-hash-table-iterator` của CL; `doiter`/`map`/`filter` và các hàm khác hoạt động trên nó nguyên trạng |
| `maphash` | `(maphash h f)` | `(HashTable<K,V>,(fn (K V) ()))→Unit` | `maphash` của CL |
| `size` | `(size h)` | `HashTable<K,V>→int` | `hash-table-size` của CL. Trong bảng này nó là số mục đang chiếm (bằng `count`) |

**Mọi kiểu triển khai `Hash` đều có thể làm khóa**, gồm cả các kiểu `defstruct`/`defenum`.
`get`/`set`/`remove` mang `(where (Hash K))`, nên một bảng có khóa thuộc kiểu không triển khai nó là
**lỗi kiểu** (`f64` không có `Hash` vì `NaN`).

```lisp
(deftrait Hash (Eq)
  (sxhash ((self Self)) int))              ; trả về một giá trị không âm vừa với fixnum
```

Được triển khai cho: `int` và sáu số nguyên độ rộng cố định, `bool`, `char`, `string` và `symbol` (không
cho số dấu phẩy động). Với kiểu của riêng bạn, hãy giữ kết quả không âm bằng cách `logand` nó với
`*sxhash-mask*` (2^30-1). Để băm một chuỗi, bạn có thể gọi `(sxhash-string s)` (FNV-1a 32 bit), mà triển
khai cho `string` dùng.

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

Việc hai khóa có giống nhau hay không được quyết định bởi **chính kiểu khóa** (`sxhash`, và `equals` từ
`Eq`, supertrait của `Hash`), không phải bởi đồng nhất đối tượng. Đó là lý do, như trên, bạn có thể tra
cứu bằng một khóa là "một giá trị khác nhưng bằng nhau".

`sxhash` va chạm là chấp nhận được (hợp đồng của `Hash` chỉ đi một chiều: các giá trị bằng nhau phải có
cùng giá trị băm). Các khóa va chạm được phân biệt bằng `equals`.

## 5. `Array<T>` (mảng nhiều chiều)

Một `defstruct` trong thư viện chuẩn. Nó không phải kiểu dựng sẵn, nên mọi thứ bạn làm được với một
`defstruct` đều làm được với nó.

| Tên | Dạng | Kiểu | Mô tả |
|---|---|---|---|
| `make` | `(Array::make dims init &key fill-pointer)` | `(Vector<int>,T)→Array<T>` | `make-array` của CL. `dims` được sao chép. `init` là giá trị ban đầu của mọi ô (`:initial-element` của CL; ngôn ngữ này không có "ô chưa gán", nên nó là bắt buộc). `:fill-pointer` chỉ cho một chiều |
| `get` / `set` | `(get a idx)` / `(set a idx x)` | `(Array<T>,Vector<int>)→T` | `aref` / `(setf (aref …))` của CL. Panic nếu một chỉ số ngoài phạm vi |
| `aref` | `(aref a i j …)` | — | Cách viết của CL với các chỉ số trần. Khai triển thành `get`/`set` ở trên. `(setf (aref a i j) v)` cũng dùng được |
| `row-major-get` / `row-major-set` | `(row-major-get a i)` | `(Array<T>,int)→T` | `row-major-aref` của CL. Một chỉ số phẳng |
| `rank` | `(rank a)` | `Array<T>→int` | `array-rank` của CL |
| `dimension` | `(dimension a n)` | `(Array<T>,int)→int` | `array-dimension` của CL |
| `dimensions` | `(dimensions a)` | `Array<T>→Vector<int>` | `array-dimensions` của CL. Trả về một **bản sao**, giống như CL trả về một danh sách mới |
| `total-size` | `(total-size a)` | `Array<T>→int` | `array-total-size` của CL (số ô đã cấp phát, không liên quan đến fill pointer) |
| `len` | `(len a)` | `Array<T>→int` | `length` của CL trên mảng. Fill pointer nếu có, nếu không là `total-size` |
| `in-bounds` | `(in-bounds a idx)` | `(Array<T>,Vector<int>)→bool` | `array-in-bounds-p` của CL. False (không phải lỗi) ngay cả khi **số lượng** chỉ số sai |
| `row-major-index` | `(row-major-index a idx)` | `(Array<T>,Vector<int>)→int` | `array-row-major-index` của CL |
| `adjust` | `(adjust a dims init)` | `(Array<T>,Vector<int>,T)→Unit` | `adjust-array` của CL. Hạng không thể đổi. Các phần tử còn trong phạm vi được giữ ở chỉ số của chúng, và các ô mới nhận `init`. Không như CL, nó không trả về mảng (mọi mảng trong ngôn ngữ này đều điều chỉnh được, nên không có mảng thứ hai để trả về) |
| `push-extend` | `(push-extend a x)` | `(Array<T>,T)→Unit` | `vector-push-extend` của CL. Panic nếu không có fill pointer |
| `pop` | `(pop a)` | `Array<T>→Option<T>` | `vector-pop` của CL. `none` nếu rỗng |
| `fill-pointer` | `(fill-pointer a)` | `Array<T>→Option<int>` | Fill pointer (`none` nếu không có). Có thể ghi bằng `(setf a::fill-pointer …)` |
| `iter` | `(iter a)` | `Array<T>→array-iter<T>` | Một iterator theo thứ tự row-major. Dừng ở fill pointer nếu có |

- **Chỉ số là một `Vector<int>`.** Một phương thức không thể khai báo "cùng một kiểu đối số lặp lại bất kỳ
  số lần nào ở cuối", và đường cú pháp `aref` bắc cầu qua khoảng trống đó.
- `array-element-type` / `simple-vector-p` / `adjustable-array-p` / `array-has-fill-pointer-p`
  **không tồn tại**. Kiểu tĩnh của bên nhận đã trả lời những câu hỏi này.
- `Array::new` là hàm khởi tạo theo thứ tự trường do `defstruct` sinh ra và không nhằm để tạo mảng. Hãy
  dùng `Array::make`.
- **Mảng được in theo cú pháp mảng của CL.** Hạng 1 là `#(1 2 3)`; các hạng khác là `#nA` theo sau bởi
  từng đó mức ngoặc (`#2A((1 2 3) (4 5 6))`); hạng 0 là `#0A5`. Việc in dừng ở fill pointer nếu có. Đặt
  `*print-array*` ([In](printing.md#6-điều-khiển-lượng-được-in)) thành false thì chỉ in hình dạng,
  `#<array 2x3>`. Chỉ một mảng có các phần tử là `defstruct` không có `print-object` mới in ở dạng dựng
  sẵn `#<array<...> ...>` (đó không phải lỗi).

## 6. `BitVector` (vector bit)

Một dãy bit có độ dài cố định. Một `defstruct` trong thư viện chuẩn.

| Tên | Dạng | Kiểu | Mô tả |
|---|---|---|---|
| `make` | `(BitVector::make n)` | `int→BitVector` | Độ dài `n`, mọi bit bằng 0 |
| `get` / `set` | `(get v i)` / `(set v i b)` | `(BitVector,int)→bool` | Panic khi ngoài phạm vi |
| `bit` / `sbit` | `(bit v i)` | `(BitVector,int)→bool` | Cách viết của CL. `(setf (bit v i) b)` cũng dùng được. `sbit` của CL chỉ khác `bit` ở chỗ yêu cầu một bit vector đơn giản, nhưng ngôn ngữ này chỉ có một loại bit vector |
| `len` | `(len v)` | `BitVector→int` | Số bit |
| `bit-and` `bit-ior` `bit-xor` `bit-eqv` `bit-nand` `bit-nor` `bit-andc1` `bit-andc2` `bit-orc1` `bit-orc2` | `(op a b)` | `(BitVector,BitVector)→BitVector` | Trả về một bit vector mới. Panic nếu độ dài khác nhau. Không có đối số thứ ba như trong CL (đích của kết quả) |
| `bit-not` | `(bit-not v)` | `BitVector→BitVector` | Phần bù |

Không có `bit-vector-p` (kiểu tĩnh đã trả lời).
