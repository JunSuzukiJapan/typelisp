<!-- translated-from: docs/ja/reference/syntax.md @ 37af68009626057caa98d1dc23e3879b42e983c7 -->
# Tham chiếu cú pháp typelisp

typelisp là một ngôn ngữ Lisp có kiểu tĩnh, được viết bằng S-expression. Về danh sách các hàm và phương thức
dựng sẵn, xem [Hàm dựng sẵn](functions/README.md); về danh sách các kiểu, xem [types.md](types.md); và về cách
đọc thông báo lỗi, xem [errors.md](errors.md).

## 1. Cú pháp từ vựng

- **Không phân biệt hoa thường.** Mọi symbol đều được chuẩn hóa thành chữ thường khi đọc.
- **Chú thích**: từ `;` đến cuối dòng (chú thích dòng). `#| ... |#` (chú thích khối, có thể lồng nhau).
- **Đánh giá lúc đọc**: `#.(expr)` **chạy dạng theo sau trong khi đọc** và coi giá trị của nó là thứ đã đọc.
  Đây là nơi duy nhất mà reader không chỉ là một hàm của văn bản. Phạm vi nó với tới phụ thuộc vào đường đọc,
  như trong CL:
  - `(load ...)` và REPL đánh giá từng dạng một, nên nó có thể gọi **các hàm đã định nghĩa trước đó trong
    cùng văn bản** (`load` của CL).
  - Một tệp module được kiểm tra như một đơn vị và được chạy bởi bên `use` nó, nên `#.` chỉ với tới thư viện
    chuẩn và những gì phiên đã chạy. Cả các định nghĩa của chính tệp lẫn của các module mà nó `use` **đều
    chưa chạy** (giống như `compile-file` của CL cần `eval-when`).
  - `read` / `read-from-string` bên trong một chương trình cũng đánh giá `#.` (như trong CL).
  - Đặt `*read-eval*` (mặc định `true`) thành `false` làm `#.` thành lỗi đọc ở mọi nơi: một công tắc để giữ
    văn bản được đọc như dữ liệu không chạy mã (như trong CL). Nó được tham khảo ở mỗi `#.`, nên một `setf`
    có hiệu lực từ dạng được đọc tiếp theo. Bên trong `with-standard-io-syntax` nó là `true`.
- **Boolean**: `true` / `false`.
- **Số nguyên**: thập phân (`42`, `-7`). Một dấu `+`/`-` có thể đứng đầu. Các cơ số khác được viết bằng cú
  pháp cơ số của CL `#b`/`#o`/`#x`/`#NNr` (dấu đứng sau ký hiệu: `#x-ff`). Tiền tố `0x` không có trong CL
  và không được áp dụng: `0xff` được đọc là một symbol.
  Một literal số nguyên không có chú thích kiểu mặc định là `int` (độ chính xác tùy ý,
  [Số](functions/numbers.md#3-số-nguyên-độ-chính-xác-tùy-ý-int)), không có giới hạn trên về độ lớn.
  **Nếu kiểu mong đợi là một kiểu số nguyên độ rộng cố định, literal nhận kiểu đó, và được kiểm tra rằng
  kiểu đó chứa được giá trị**: `(the u8 300)` là lỗi kiểu (nếu bạn muốn cắt bớt, hãy viết `(as u8 300)`).
  `(the u32 4294967295)` và `(the u32 #xFFFFFFFF)` viết được nhờ quy tắc này. Việc một giá trị `int` vừa
  trong một giá trị tức thời 63 bit hay trở thành bignum được quyết định bởi độ lớn của nó, không có cú pháp
  đặc biệt (như trong CL).
- **Số dấu phẩy động**: những số chứa dấu thập phân hoặc số mũ (`e`/`E`) (`1.5`, `3.0e10`). Mặc định là
  `f64` (`f32` nếu đó là kiểu mong đợi).
- **Số hữu tỉ (ratio)**: `tử/mẫu` (chỉ thập phân, ví dụ `1/3`). Được rút gọn khi đọc, như CL quy định (`2/4`
  là `1/2`). Những số có giá trị nguyên (`4/2`, v.v.) được đọc là `int`, không phải `ratio`. Mẫu số bằng
  không (`1/0`) là lỗi đọc.
- **Ký tự**: `#\` theo sau bởi một ký tự hoặc một tên ký tự. Ví dụ `#\a` `#\Space` `#\Newline` `#\Tab`
  `#\Return` `#\Page` `#\Nul` (cũng là `#\Null`) `#\Backspace`. Tên không phân biệt hoa thường.
- **Chuỗi**: `"..."`. Các escape là `\n` `\t` `\r` `\0` `\\` `\"` (mọi `\x` khác chỉ là `x`).
- **Symbol**: mọi token chứa chữ cái, chữ số và ký hiệu (`+` `<=` `my-func`, v.v.).
  `]` và `}` kết thúc một token, nên không thể xuất hiện bên trong ký hiệu, và gặp một trong hai ở
  đầu một datum là lỗi đọc. `[` và `{` thì có thể xuất hiện trong ký hiệu: giống CL, chúng được để
  trống để lập trình viên dùng trong [macro đọc](#11-reader-macro-readtable).
- **Keyword**: các symbol bắt đầu bằng dấu hai chấm, như `:name` (như trong CL). Chúng tự đánh giá: chúng
  không tra ràng buộc nào và giá trị của chúng là chính chúng, với kiểu tĩnh `symbol`. Các keyword cùng tên
  luôn là cùng một đối tượng (`(eq :foo :FOO)` là true; như các symbol khác chúng được chuyển thành chữ
  thường). Bản thân dấu hai chấm là một phần của tên, nên `(symbol->string :foo)` là `":foo"` (typelisp không
  có hệ thống package, nên điều này khác `symbol-name` của CL). Một dấu `:` đơn lẻ hoặc một keyword có thêm
  dấu hai chấm như `:a:b` là lỗi đọc. Kiểm tra bằng `keywordp`. Những cái bắt đầu bằng `::` không phải
  keyword mà là đường dẫn tuyệt đối (bên dưới).
  Lưu ý rằng `:dyn` là một keyword dành riêng chỉ cho vị trí kiểu; viết nó ở bất kỳ chỗ nào khác là lỗi
  (xem [chương 2](#2-cách-viết-kiểu)).
- **Danh sách**: `(a b c)`. Cặp có dấu chấm `(a . b)` cũng đọc được.
- **Vector**: `#(1 2 3)` (giống CL). Nội dung chỉ gồm literal và không được đánh giá: `a` trong
  `#(a b)` là ký hiệu, không phải biến. Kiểu phần tử lấy từ ngữ cảnh (`(the Vector<i32> #(1 2))`),
  hoặc từ phần tử đầu tiên khi không có ngữ cảnh (`#(1 2 3)` là `Vector<int>`). Mọi phần tử phải
  cùng kiểu: `#(1 "a")` là lỗi kiểu, `#()` không có phần tử lẫn ngữ cảnh cũng vậy. Mỗi lần đánh giá
  tạo ra một vector mới. Ở chỗ mong đợi dữ liệu S-expression (`(the Option<Sexpr> #(1 x))`,
  `'#(..)`, thứ `read` trả về), nó là `Vector<Option<Sexpr>>` mà mọi phần tử đều là dữ liệu: variant
  `vector` của `Sexpr`.
- **Mảng**: `#2A((1 2) (3 4))` (giống CL). Số giữa `#` và `A` là hạng, và chừng ấy tầng lồng nhau
  đầu tiên của các danh sách trong nội dung là các chiều. `#0A x` là mảng không chiều chứa một phần
  tử. Các danh sách cùng tầng có độ dài khác nhau là lỗi đọc. Kiểu được quyết định như với vector và
  là `Array<T>` (khi không có phần tử, ngữ cảnh phải cho kiểu, như `(the Array<f64> #2A(()))`). Dưới
  dạng dữ liệu S-expression nó là `Array<Option<Sexpr>>`: variant `array` của `Sexpr`.
- **Danh sách rỗng `()`**: tùy ngữ cảnh, là giá trị của kiểu `Unit` hoặc là `none` của `Option<Sexpr>`.
  **`Sexpr` không có variant danh sách rỗng**: `Sexpr` nghĩa là "một S-expression không rỗng", và kiểu của dữ
  liệu S-expression là `Option<Sexpr>` (xem "Mẫu cho `Option<Sexpr>`" ở
  [4.3 match](#43-match--so-khớp-mẫu)).
- **quote/quasiquote/unquote**:
  - `'x` → `(quote x)`
  - `` `x `` → `(quasiquote x)`
  - `,x` → `(unquote x)` (chỉ có nghĩa bên trong một quasiquote)
  - `,@x` → `(unquote-splicing x)` (được nối vào như các phần tử danh sách khi khai triển)
- **Đường dẫn `::`**: `foo::bar` được đọc là một đường dẫn qua các module, kiểu và thành viên (không phải
  một tên symbol đơn). Một đường dẫn bắt đầu bằng `::`, như `::foo`, là đường dẫn tuyệt đối từ gốc. Một `::`
  bên trong các đối số generic (`Vec<a::b>` và tương tự) không được coi là dấu phân cách đường dẫn.

## 2. Cách viết kiểu

Trong mã nguồn, kiểu được viết là các symbol hoặc danh sách thông thường.

- **Kiểu nguyên thủy**: `int` `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `bool` `char` `string` `symbol`.
  `int` là kiểu số nguyên (integer của CL, tự động chuyển giữa giá trị tức thời 63 bit và bignum;
  [Số](functions/numbers.md#3-số-nguyên-độ-chính-xác-tùy-ý-int)), và sáu kiểu độ rộng cố định được đặt tên
  theo độ rộng và tính có dấu của chúng (không có kiểu số nguyên 64 bit; xem
  [Số](functions/numbers.md#1-số-nguyên-độ-rộng-cố-định)).
- **Kiểu số hữu tỉ**: `ratio` (số hữu tỉ ở dạng tối giản). Được cấp phát trên heap như trong CL, không có
  chuyển đổi ngầm với `int`/`f64` và tương tự (hãy chuyển đổi tường minh bằng `as`/`try-as` hoặc một phương
  thức chuyển đổi; xem [Số](functions/numbers.md#5-số-hữu-tỉ-ratio)).
- **Các từ thô ở ranh giới C**: `ptr` (một con trỏ mờ), `c-long` / `c-ulong`. Chỉ cho FFI: để biến một thứ
  thành giá trị cần `(unsafe ...)`, và các chỗ chúng có thể xuất hiện bị giới hạn
  ([3.3 defffi](#ptr--c-long--c-ulong--các-từ-máy-thô)). Đừng dùng chúng ở nơi bạn muốn một số nguyên 64
  bit: chúng không có số học.
- **Các kiểu khả biến mờ**: `random-state` (trạng thái của một bộ sinh số ngẫu nhiên). Nó không thể đưa vào
  `Vector<T>`/`HashTable<K,V>`/`Sexpr` (nó có thể đưa vào `Option<T>`/`Result<T,E>`).
- **Kiểu Unit**: `()`
- **Kiểu Never**: `!` (kiểu của các biểu thức không hội tụ như `panic`/`unreachable`/`todo`/một vòng lặp
  không bao giờ trả về. Nó vừa với mọi kiểu mong đợi)
- **Kiểu hàm**: `(fn (các-kiểu-đối-số...) kiểu-trả-về)`. Kiểu của một hàm có đối số biến đổi là
  `(fn (các-kiểu-đối-số... &rest kiểu-phần-tử) kiểu-trả-về)`.
- **Kiểu generic**: `Name<T1,T2,...>` (được đọc như một token đơn không có dấu cách).
  Ví dụ `Option<i32>` `Result<i32,ParseIntError>` `HashTable<string,i32>` `Vector<T>`.
  Kiểu unit `()` cũng có thể được viết làm đối số kiểu (`Result<(), FileError>`). `(`/`)` thường là các dấu
  phân cách kết thúc một token, nhưng khi một ngoặc nhọn đang mở, cặp ký tự này được cho qua. `()` cũng có
  thể dùng làm kiểu trường hoặc kiểu đối số.
- **Dạng áp dụng của kiểu generic**: `(Name T1 T2 ...)`, một cách viết danh sách đặt tên cùng kiểu với
  `Name<T1,T2,...>`. Ví dụ `(vector char)` giống `Vector<char>`.
  Dạng tên là cách viết thông thường; dạng này **tồn tại cho khi một đối số kiểu không thể được viết bên
  trong một tên**: một đối số kiểu tự nó là một biểu thức kiểu, nhưng bên trong một tên token đơn chỉ viết
  được các tên, `()` và `:dyn`, không viết được kiểu hàm (không có cách viết như
  `Vector<(fn (i32) i32)>`). Nó cũng có thể xuất hiện ở dạng này khi triển khai hiển thị một kiểu, như kết
  quả của việc thay kiểu liên kết của một trait vào một chữ ký.
- **Tên kiểu có tiền tố**: có thể được qualify bằng `::`, như `module::Type`.
- **Kiểu trait object**: `:dyn Trait` (hai từ cách nhau bằng dấu cách tạo thành một kiểu). Biểu diễn một giá
  trị có kiểu cụ thể được quyết định lúc chạy; các lời gọi phương thức của trait đi qua một vtable (dispatch
  động). Với một trait có kiểu liên kết, chúng được cố định theo vị trí theo thứ tự khai báo (`:dyn Iter<i32>`
  cố định `Item` là `i32`). Nó cũng có thể được viết bên trong các đối số generic: `Vector<:dyn Drawable>`
  `HashTable<string, :dyn Drawable>`. Các giá trị cụ thể được đóng hộp tự động ở các vị trí mong đợi; dạng
  tường minh là `(as :dyn Trait expr)`.
  Một giá trị `:dyn Sub` có thể được truyền nguyên trạng ở nơi cần một `:dyn Super` của bất kỳ supertrait nào
  của nó (mọi thứ nó kế thừa, bắc cầu) (upcasting). Nó không thể được truyền cho một trait không liên quan.
  Về các điều kiện một trait phải đáp ứng để dùng với `:dyn`, xem
  [3.9 deftrait / impl](#39-deftrait--impl--trait). Viết `:dyn` ngoài vị trí kiểu là lỗi.
- Các kiểu generic dựng sẵn: `Option<T>` (`Some(T)` / `None`), `Result<T,E>` (`Ok(T)` / `Err(E)`),
  `HashTable<K,V>`, `Vector<T>`, và các kiểu đồng thời `Task<T>` / `Thread<T>` / `Chan<T>`
  ([chương 12](#12-lập-trình-đồng-thời-task)). Cũng có `Sexpr`, kiểu của dữ liệu S-expression. Các kiểu lỗi cụ thể
  dựng sẵn là `ParseIntError` / `ParseFloatError` / `ReadError` / `EvalError` / `FileError` / `NetError`, và
  thư viện chuẩn có các struct `SimpleError` / `WrappedError` (`Error` không phải một kiểu mà là một trait:
  hãy dùng nó dưới dạng `:dyn Error`). Danh sách nằm ở [types.md](types.md).
- **Kiểu và trait dùng chung một không gian tên** (như trong Rust): trong một module, một kiểu
  (`defstruct`/`defenum`) và một trait (`deftrait`) không thể trùng tên.

## 3. Các định nghĩa cấp cao nhất

### 3.1 defun — định nghĩa hàm

```lisp
(defun name ((arg1 Type1) (arg2 Type2) ...) RetType
  body...)
```

- Kiểu của các đối số và kiểu trả về là bắt buộc.
- Một hàm generic viết các tham số kiểu của nó trong ngoặc nhọn sau tên:
  `(defun name<T1,T2...> (params) Ret body...)` (cùng cú pháp ngoặc nhọn như `Vector<T>` ở các vị trí kiểu).
- `defun`/`lambda`/`defmethod` chấp nhận các đối số biến đổi khi `&rest (name Type)` được viết ở cuối:
  `(defun name ((a Type1) &rest (xs Type2)) Ret body...)` (trong thân, `xs` luôn được gán là một
  `Option<Sexpr>`, một danh sách S-expression. Mỗi đối số thực tế ở lời gọi được kiểm tra kiểu là `Type2` riêng
  lẻ rồi được bọc vào một `Sexpr`).
  `defmacro` cũng có `&rest` riêng, nhưng khác ở chỗ nó luôn là một `Sexpr` không kiểu (`defun`/`lambda` nêu
  kiểu phần tử). Một kiểu hàm cũng có thể mô tả một hàm có đối số biến đổi, dưới dạng
  `(fn (T1... &rest Te) Ret)`.
- **`&optional` / `&key`** (cho `defun` và `defmethod`; không cho `lambda`/`labels`, vì lý do bên dưới, và
  `defmacro` có một cách triển khai riêng, cũng ở bên dưới). Thứ tự là của CL:
  `required &optional &rest &key`. Mỗi tham số được viết `(name Type)` hoặc `(name Type default-expr)`:

  ```lisp
  (defun greet ((name string) &optional (suffix string)) string      ; không có mặc định
    (match suffix ((some s) (append name s)) ((none) name)))         ; Option<string> trong thân

  (defun pow ((b i32) &optional (n i32 2)) i32 ...)                  ; có mặc định
  (pow 3)      ; n = 2
  (pow 3 5)    ; n = 5

  (defun mk (&key (a i32 0) (b string "z")) string ...)
  (mk :b "q")  ; bên gọi viết `:name value`, theo thứ tự bất kỳ; các tham số bị bỏ qua nhận mặc định
  ```

  - **Một tham số không có biểu thức mặc định có kiểu `Option<Type>`.** Khi bị bỏ qua, nó là `none`; khi được
    truyền, giá trị trần mà bên gọi viết được bọc tự động vào `some`. Những gì CL làm với một biến supplied-p
    ("có được cung cấp không?") thì ở đây hiện ra ở phía kiểu tĩnh.
  - Với một biểu thức mặc định, kiểu vẫn là `Type` như đã khai báo. Khi bị bỏ qua, **biểu thức đã được kiểm
    tra** đó được nhúng vào lời gọi nguyên trạng (được đánh giá ở mỗi lần gọi).
  - **`&key` không thể trộn với `&optional`/`&rest` trong một danh sách đối số.** Điều này tránh một sự mơ hồ
    mà chính CL có (một đối số thực tế ở cuối được `&optional` theo vị trí nhận hay được khớp theo nhãn như
    một `&key` phụ thuộc vào các *giá trị*) bằng cách cấm tổ hợp này. `&optional` và `&rest` có thể dùng cùng
    nhau.
  - Chúng có thể dùng trong các hàm generic, nhưng **một tham số kiểu chỉ xuất hiện trong các đối số bị bỏ
    qua không thể được suy ra và là lỗi** (không có giá trị nào để khớp).
  - **`defmethod` có thể có cùng ba phần** (cho cả phương thức thể hiện lẫn hàm tĩnh). Hãy liệt kê
    `&optional`/`&rest`/`&key` sau bên nhận:

    ```lisp
    (defstruct box (w i32) (h i32))
    (defmethod grow ((self box) &key (dw i32 0) (dh i32 0)) i32 ...)
    (grow (box::new 1 2) :dh 10)

    (defmethod origin (point &key (x i32 0) (y i32 0)) point (point::new x y))   ; hàm tĩnh
    (point::origin :y 7)
    ```

    Chúng cũng có thể dùng trong các phương thức của kiểu generic, nhưng **kiểu của một tham số có biểu thức
    mặc định không thể nhắc đến các tham số kiểu của chủ sở hữu** (cùng hạn chế mà `defun` có với các tham số
    kiểu của chính nó: thứ được nhúng khi đối số bị bỏ qua là một biểu thức *đã kiểm tra*, nên kiểu của nó
    không thể để lại là một biến trừu tượng).
  - **Chúng không thể dùng trong các phương thức của trait.** `deftrait` không có cú pháp cho chúng, và nếu
    chỉ phía `impl` có thể khai báo các phần, các lời gọi với bên nhận `:dyn` (điền các đối số từ khai báo của
    trait) và các lời gọi với bên nhận cụ thể (điền từ khai báo của `impl`) sẽ trở thành những thứ khác nhau.
    Số ngôi của một vị trí vtable là cố định.
  - **Chúng không thể dùng trong `lambda` / `labels`** (`&rest` thì được). Để điền một đối số bị bỏ qua, bên
    gọi phải đọc **biểu thức mặc định đã kiểm tra của bên được gọi**, thứ chỉ có từ một chữ ký được phân giải
    theo tên. Một `lambda` được truyền như một giá trị, và thứ duy nhất mô tả giá trị đó là kiểu hàm của nó
    `(fn ...)`: trong đó không có chỗ cho một biểu thức, và nếu có, "hai lambda có cùng chữ ký nhưng mặc định
    khác nhau" sẽ trở thành các kiểu khác nhau. `&rest` vẫn nằm trong phạm vi kiểu, nên nó có thể được viết
    trong một kiểu hàm.
- **Tham chiếu tiến được khai báo bằng `defsignature`** (bên dưới). Một tên chưa được khai báo không thể được
  gọi trước định nghĩa của nó, vì cấp cao nhất được kiểm tra và chạy từng dạng một, theo thứ tự mã nguồn.
- Để yêu cầu các ràng buộc trait, hãy viết một mệnh đề `where` ngay trước thân:
  `(defun name<T> (params) Ret (where (Trait T (AssocName ConcreteType)...)) body...)`
  (cố định một kiểu liên kết bằng `(AssocName ConcreteType)` là tùy chọn).
- **Docstring**: một literal chuỗi ở đầu thân, ngay sau mệnh đề `where` (nếu có), trở thành docstring (như
  trong CL). Tuy nhiên chỉ khi có ít nhất một dạng của thân theo sau nó: một chuỗi đứng một mình vẫn là giá trị
  trả về và không được coi là docstring: `(defun f () string "doc" "value")` có một docstring và trả về
  `"value"`, còn `(defun f () string "value")` không có docstring và trả về `"value"`. Có thể lấy nó bằng
  `(documentation name)` ([docstring](functions/system.md#7-docstring--documentation)).

### 3.2 defsignature — khai báo trước

```lisp
(defsignature name (argument-types...) return-type)
(pub defsignature name (argument-types...) return-type)
```

Để gọi một `defun` được định nghĩa **sau** chính mình, hãy khai báo nó trước như thế này. Đệ quy lẫn nhau chỉ
có thể viết theo cách này:

```lisp
(defsignature odd2 (i32) bool)
(defun even2 ((n i32)) bool (if (= n 0) true  (odd2 (- n 1))))
(defun odd2  ((n i32)) bool (if (= n 0) false (even2 (- n 1))))
```

Các đối số được liệt kê **chỉ bằng kiểu**; không có thân, nên không có gì để đặt tên.
`&rest` có thể được viết ở cuối, dưới dạng `&rest kiểu-phần-tử`.

Các khai báo **được kiểm tra**:

- Định nghĩa theo sau phải khớp với khai báo (số lượng và kiểu của đối số, kiểu trả về, `&rest`, và việc nó có
  `pub` hay không). Không khớp là lỗi tại định nghĩa.
- Khai báo mà không định nghĩa là lỗi (được báo khi tệp / module nạp xong). REPL không báo sau mỗi lần nhập,
  vì một khai báo và định nghĩa của nó phải có thể được gõ ở các dòng riêng.
- Một khai báo đặt **sau** định nghĩa là lỗi, vì một khai báo như vậy không thể làm gì.

Ba thứ không thể khai báo:

- **Hàm generic.** Việc tạo một bản sao cho từng kiểu cần thân, và một khai báo không có thân. Một lời gọi
  tiến có thể được phân giải nhưng việc khởi tạo sẽ thất bại, nên khai báo bị từ chối ngay từ đầu.
- **`&optional`/`&key`.** Chữ ký của chúng gồm biểu thức **đã kiểm tra** của mỗi giá trị mặc định (được nhúng
  vào lời gọi khi đối số bị bỏ qua), và một khai báo không có chỗ cho nó.
- **Bất cứ thứ gì ngoài `defun`.** Một `defmacro` cần thân macro **đã chạy** để khai triển, điều mà việc đăng
  ký một chữ ký không thể thay thế. Với các kiểu (`defstruct`/`defenum`/`deftrait`), việc đăng ký chúng là
  "thứ mà chính mã đăng ký kiểu cần", không tự đầy đủ như một chữ ký. Một `defmethod` được đăng ký trên kiểu sở
  hữu nó, nên nó đi theo kiểu.

Đối ứng của CL là `(declaim (ftype (function (i32) bool) even2))`, nhưng nó đi kèm cả một hệ thống khai báo
và chỉ mang tính **tham khảo**. Ở đây, với kiểu tĩnh, các khai báo được kiểm tra.

### 3.3 defffi — khai báo hàm C (FFI)

```lisp
(defffi (name "c_symbol") (argument-types...) return-type)
(defffi (name "c_symbol") (argument-types...) return-type :library "name")
(defffi name (argument-types...) return-type)              ; name = tên ký hiệu C
(pub defffi ...)
```

Khai báo một hàm C để nó có thể được gọi. Dạng giống `defsignature` (một tên, các kiểu đối số, một kiểu trả
về và không có thân), nhưng việc không có thân mang một ý nghĩa khác. `defsignature` là một lời hứa rằng "tôi
sẽ định nghĩa nó sau", còn `defffi` khai báo rằng "người khác đã viết và biên dịch thân rồi".

```lisp
(defffi (c-abs "abs") (i32) i32)
(defffi (c-sqrt "sqrt") (f64) f64)
(defffi (c-getpid "getpid") () i32)

(unsafe (c-abs -5))                          ; => 5
```

Tên typelisp và tên ký hiệu C có thể được viết riêng vì định danh typelisp thường chứa `-` còn định danh C thì
không. Nếu bỏ tên C, tên được dùng làm tên ký hiệu C nguyên trạng.

**Các lời gọi yêu cầu `(unsafe ...)`** (ngay cả với các hàm chỉ trên kiểu vô hướng). Trình biên dịch không có
cách nào xác nhận rằng chữ ký C đã khai báo khớp với chữ ký thật và chỉ có thể tin khai báo; `unsafe` là dấu
hiệu rằng bạn nhận trách nhiệm đó. Cách dự kiến là bọc nó một lần và tạo một wrapper an toàn:

```lisp
(defun abs-i32 ((n i32)) i32 (unsafe (c-abs n)))
(abs-i32 -3)                                 ; từ đây không cần unsafe nữa
```

Các kiểu có thể viết là `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `bool` `()` (void) `string` `ptr`
`c-long` `c-ulong`, và con trỏ có kiểu `(ptr T)`
([bên dưới](#def-c-struct-and-typed-pointers--cấp-phát-struct-c)).

`string` là `const char *`. Chuỗi typelisp không kết thúc bằng NUL và bản thân chúng có thể chứa NUL, nên
**chúng được sao chép thành chuỗi C khi truyền**, và được giải phóng sau lời gọi. Một NUL trong chuỗi là lỗi:
C chỉ nhìn đến đó, nên một chuỗi khác sẽ bị truyền đi một cách im lặng.

**Các chuỗi trả về cũng được sao chép**, và không được giải phóng: thứ C trả về thuộc về C, và nó có thể trỏ
vào một bảng tĩnh, như với `getenv`. Các hàm trả về bộ nhớ mà bên gọi phải giải phóng (`strdup`, v.v.) nên
được nhận dưới dạng `ptr` và tự giải phóng.

Các hàm có kết quả trỏ vào bên trong một đối số (`strchr`, `strstr`) cũng hoạt động đúng: kết quả được sao
chép trước khi đối số được giải phóng.

Nếu một hàm được khai báo trả về `string` mà trả về NULL, đó là lỗi, vì `string` không có giá trị nào nghĩa là
"không có". Nếu NULL có thể xảy ra, hãy nhận kết quả dưới dạng `ptr`.

```lisp
(defffi (c-strlen "strlen") (string) i32)
(defffi (c-getenv "getenv") (string) string)

(unsafe (c-strlen "hello"))                  ; => 5
(unsafe (c-getenv "PATH"))                   ; => "/usr/bin:..."
```

Với `:library`, thư viện dùng chung đó được mở và ký hiệu được tra trong đó. Không có nó, ký hiệu được tra
trong **chính tiến trình** (mọi thứ đã được liên kết, gồm cả libc). Một tên ngắn như `sqlite3` được tra thành
`libsqlite3.dylib` / `libsqlite3.so` theo thứ tự đó, và một tên chứa `/` được coi là đường dẫn. Các thư viện
đã mở không bao giờ bị đóng: mã trỏ vào các hàm của chúng tiếp tục chạy, nên vòng đời đúng duy nhất là vòng
đời của tiến trình.

#### ptr / c-long / c-ulong — các từ máy thô

`ptr` là một con trỏ mờ (`void *`, `FILE *`, bất cứ thứ gì khai báo muốn nói). `c-long` / `c-ulong` là `long` /
`unsigned long` của C (cũng là `size_t`, `int64_t` và `intptr_t`).

```lisp
(defffi (c-malloc "malloc") (c-ulong) ptr)
(defffi (c-free "free") (ptr) ())
(defffi (c-strlen "strlen") (string) c-ulong)

(unsafe (let ((p (c-malloc 16))) (c-free p) ()))
```

**Không gọi chúng là `i64` / `u64` là có chủ ý.** Ngôn ngữ này không có kiểu số nguyên 64 bit, vì một giá trị
tức thời có tag chỉ có 63 bit ([chương 2](#2-cách-viết-kiểu)). Cái tên `c-long` nói rằng "đây là một từ đi qua
ranh giới với C, không phải một số nguyên của ngôn ngữ này".

**Chúng không có số học.** `(+ x 1)` không thể viết. Nó có thể được cung cấp nhưng không được cung cấp, để
không có phép tính nào chạy trên một giá trị không thể được lưu ở đâu cả và có độ rộng khác mọi số khác, vì
cùng lý do kiểu số nguyên 64 bit bị bỏ. Chỉ có **các phép chuyển đổi**:

```lisp
(as i32 (unsafe (c-strlen "hello")))         ; đọc thứ trả về
(as int (unsafe (c-strlen s)))               ; cái này để đọc chính xác (int không mất 64 bit)
(try-as i32 (unsafe (c-strlen s)))           ; hỏi xem nó có vừa không
(as c-ulong n)                               ; tạo một giá trị từ một số nguyên khác
```

**Literal** số nguyên nhận kiểu mong đợi, nên không cần `as` chỉ để truyền một literal:

```lisp
(unsafe (c-malloc 16))                       ; 16 được đọc là một c-ulong
```

Các literal ngoài phạm vi bị từ chối như với các độ rộng khác (`(c-malloc -1)` không vừa một `c-ulong`).

**Các chỗ chúng có thể xuất hiện bị giới hạn**: chỉ kiểu đối số, kiểu trả về và biến cục bộ. Mỗi trường hợp
sau là lỗi:

```lisp
(defstruct handle (p ptr))          ; một trường struct
(defenum maybe (none) (some ptr))   ; một trường enum
(defvar (block ptr) ...)            ; một biến toàn cục
(defffi f ((vector ptr)) i32)       ; bên trong một đối số kiểu
```

Tất cả có chung một lý do: **ô nhớ gắn tag cho thứ nó giữ**. Việc gắn tag sẽ làm rơi các bit cao của con trỏ,
cùng lý do kiểu số nguyên 64 bit bị bỏ, nên nó không được phép ngay cả trong `unsafe`. Đây không phải vấn đề
cho phép: biểu diễn đó không tồn tại.

Vì cùng lý do, chúng không thể là các biến cục bộ **được bắt giữ** bởi các hàm lồng nhau (một ràng buộc bị bắt
giữ đi vào một cell, và một cell gắn tag cho thứ nó giữ). Điều này được biết lúc biên dịch và được báo bởi
`(compile f)`.

GC không theo dõi `ptr`. Nó trỏ ra ngoài heap, nên như vậy là đúng.

Bốn thứ không thể khai báo:

- **Đối số biến đổi** (`printf`). Phần biến đổi được truyền theo quy tắc khác với các đối số cố định (trên
  stack ở AArch64 Darwin), nên không thể gọi đúng từ một chữ ký cố định. `&rest` bị từ chối.
- **Truyền hoặc trả về struct theo giá trị.** Vì cùng lý do (nó phụ thuộc vào quy ước gọi của từng nền tảng).
  Các kiểu viết được bị giới hạn trong danh sách ở trên, nên không thể viết ra.
- **Generic.** C không có thứ tương ứng.
- **Trùng tên với một hàm dựng sẵn.** Một lời gọi đã biên dịch sẽ phân giải tên đó thành hàm dựng sẵn, nên nó
  bị từ chối thay vì lặng lẽ sai.

#### Callback — để C gọi ngược lại

Viết một kiểu hàm `(fn (types...) return-type)` làm một kiểu đối số làm cho đối số đó là một hàm mà C gọi ngược
lại (một callback).

```lisp
(defffi (c-qsort "qsort") (ptr c-ulong c-ulong (fn (ptr ptr) i32)) ())

(defun desc ((a ptr) (b ptr)) i32 ...)

(unsafe
  (c-qsort buf n 8 desc)                                 ; một hàm cấp cao nhất
  (c-qsort buf n 8 (lambda ((a ptr) (b ptr)) i32 ...))   ; một lambda
  (labels ((cmp ((a ptr) (b ptr)) i32 ...))
    (c-qsort buf n 8 cmp)))                              ; một hàm cục bộ
```

Một con trỏ hàm C không là gì ngoài một địa chỉ mã, và C gọi nó chỉ truyền các đối số đã khai báo. Không có
chỗ để truyền các biến bị bắt giữ, nên **chỉ có thể truyền các hàm không có biến tự do**, và điều này được
kiểm tra ở thời điểm kiểm tra kiểu.

- Hãy viết một tên hàm hoặc một biểu thức `lambda` **trực tiếp** làm đối số thực tế. Một biến đang giữ một hàm
  không thể được truyền: nó giữ hàm nào, và do đó có biến tự do hay không, không được biết cho đến lúc chạy.
- Một `lambda` là lỗi nếu nó tham chiếu tới các biến cục bộ bên ngoài nó. Biến toàn cục và hàm cấp cao nhất
  có thể được tham chiếu.
- Một hàm cục bộ (`labels`) không được có biến tự do, kể cả những biến của các hàm anh em mà nó gọi. Các hàm
  anh em dùng chung nơi lưu các biến bị bắt giữ, nên thứ mà một hàm anh em được gọi bắt giữ cũng bị hàm này
  bắt giữ.
- Một hàm generic lấy kiểu của nó từ kiểu hàm đã khai báo.
- Các kiểu có thể viết trong kiểu hàm giống như danh sách ở trên. Tuy nhiên, `string` không thể là kiểu trả
  về của một callback (nó sẽ đưa cho C bộ nhớ mà không ai giải phóng). Một đối số `string` sao chép chuỗi mà C
  truyền vào thành một chuỗi typelisp.

Các lời gọi hàm C chỉ có thể được viết bên trong `unsafe`, nên callback chỉ có thể được truyền bên trong
`unsafe`.

**Callback chỉ có thể được gọi khi hàm C mà typelisp đã gọi đang chạy.** Nếu nó được gọi từ bất kỳ nơi nào
khác (một thread không chạy typelisp, một trình xử lý tín hiệu, một hàm đăng ký bằng `atexit`), nó in lý do
và dừng tiến trình.

**Thất bại không lan truyền qua C.** Một `panic` hoặc `throw` bên trong callback không thể tháo qua các frame
C (đó sẽ là hành vi không xác định), nên 0 được trả về cho C, và thất bại được ném lại cho bên gọi khi hàm C
trả về. Nếu callback được gọi lại giữa lúc thất bại và lúc hàm C trả về, nó không chạy và 0 được trả về.

Một thao tác phải chờ bên trong một callback (một `recv` trên kênh rỗng, v.v.) là lỗi
([12.6](#126-mã-đã-biên-dịch-và-task)).

Khi một hàm được định nghĩa lại, định nghĩa mới được gọi từ lần tiếp theo nó được truyền cho C.

Nó hoạt động giống vậy với AOT (`compile-file`). Các điểm vào mà C gọi được dựng sẵn vào tệp thực thi.

**Chúng không thể được truyền như giá trị.** Một khai báo FFI không thể được viết nguyên trạng cho `f` của
`(map f xs)`: một giá trị hàm là một closure bọc thân của một định nghĩa, và khai báo này không có thân nào để
bọc. Hãy bọc nó trong một `lambda`:

```lisp
(run-it (unsafe (lambda ((n i32)) i32 (c-abs n))))
```

`(disassemble c-abs)` cũng bị từ chối: thứ có thể hiển thị là mã máy của C, mà trình biên dịch này không tạo
ra. `(compile c-abs)` thành công (và không làm gì, vì nó đã được biên dịch).

**Nó cũng hoạt động với AOT (`compile-file`).** Bộ liên kết tự phân giải các hàm C. Nếu một khai báo có
`:library`, thư viện đó được thêm vào dòng liên kết dưới dạng `-l` (trùng lặp được gộp thành một), nên
`compile-file` không cần đối số bổ sung. Bản thân `compile-file` đọc mã nguồn, nên nó có thể thu thập chúng từ
các khai báo.

Ký hiệu cũng được tra vào lúc build. Nếu một hàm đã khai báo không tồn tại, lỗi nêu tên nó trước mọi lỗi liên
kết.

Thư viện chuẩn (prelude) không dùng `defffi`. Thư viện chuẩn đi vào mọi tệp thực thi nguyên cả khối, nên một
khai báo có `:library` ở đó sẽ liên kết thư viện đó ngay cả vào các chương trình không dùng FFI.

#### def-c-struct and typed pointers — cấp phát struct C

```lisp
(unsafe
  (def-c-struct name (field type)...)
  ...)
(unsafe (pub def-c-struct ...))
```

Khai báo một struct có cùng bố cục như trong C. Nó chỉ có thể được viết bên trong một `unsafe` cấp cao nhất
(mà không thể chứa gì ngoài các `def-c-struct`). Một docstring có thể đặt ngay sau tên.

Các kiểu có thể viết cho trường là `i8` `i16` `i32` `u8` `u16` `u32` `c-long` `c-ulong` `f32` `f64` `bool`
`ptr`, con trỏ có kiểu `(ptr T)`, và các `def-c-struct` khác (nhúng theo giá trị). Bố cục (offset của từng
trường, và kích thước cùng căn chỉnh của struct) được tính theo quy tắc của C (giả định LP64). Một trường
trỏ tới chính struct đó có thể được viết, nhưng struct không thể nhúng chính nó.

```lisp
(unsafe
  (def-c-struct point (x i32) (y f64))              ; x ở 0, y ở 8, kích thước 16
  (def-c-struct seg (a point) (b point) (next (ptr seg))))
```

Tên của một `def-c-struct` đi vào không gian tên kiểu (không có `defstruct` hay thứ tương tự cùng tên trong
cùng module), nhưng **nó không phải kiểu của một giá trị**. Bạn không thể viết `(defun f ((p point)) ...)`; nó
chỉ xuất hiện như thứ mà một con trỏ có kiểu trỏ tới.

**Một con trỏ có kiểu `(ptr T)`** là một địa chỉ trỏ tới một `T`. `T` là một trong các kiểu có thể viết cho
trường ở trên. Nó là một từ máy thô như `ptr`, với cùng các quy tắc về nơi nó có thể xuất hiện (chỉ đối số,
kiểu trả về và biến cục bộ; nó chỉ có thể là giá trị bên trong `unsafe`).

Việc cấp phát, đọc và ghi được viết theo các dạng sau. Tất cả chỉ có thể dùng bên trong `unsafe`.

| Dạng | Ý nghĩa |
|---|---|
| `(c-alloc T)` / `(c-alloc T n)` | Cấp phát `n` giá trị kiểu `T` (1 nếu bỏ qua). Nội dung được điền bằng 0. Trả về một `(ptr T)` |
| `(c-ref p i)` | Một con trỏ tới phần tử `i` kể từ `p`. Lỗi nếu ngoài phạm vi đã cấp phát |
| `(c-deref p)` / `(setf (c-deref p) v)` | Đọc / ghi giá trị vô hướng mà `p` trỏ tới |
| `p::field` / `(setf p::field v)` | Đọc / ghi một trường của struct. Đọc một trường là struct nhúng cho địa chỉ của nó (`(ptr kiểu-bên-trong)`) |
| `(as ptr p)` | Quên kiểu, tạo một `ptr` (để truyền cho thứ như `void *` của `qsort`). Không có chuyển đổi ngược lại |

```lisp
(defun sum-x ((n int)) int
  (unsafe
    (let ((ps (c-alloc point n)))
      (dotimes (i n)
        (let ((p (c-ref ps i)))
          (setf p::x (as i32 i))))
      (let ((total 0))
        (dotimes (i n)
          (let ((p (c-ref ps i)))
            (setf total (+ total (as int p::x)))))
        total))))
```

**Bộ nhớ đã cấp phát được giải phóng khi luồng điều khiển rời khỏi `unsafe` đã cấp phát nó.** Chủ sở hữu là
`unsafe` ngoài cùng nhất về mặt từ vựng trong cùng hàm. Nó được giải phóng dù mã kết thúc bình thường hay rời
đi bằng `panic`, `throw` hoặc `return-from`. Các hàm `lambda` và `labels` là các hàm riêng biệt, nên một
`c-alloc` trong chúng cần một `unsafe` riêng bên trong chúng.

Vì vậy, một con trỏ có kiểu không thể rời khỏi `unsafe` đã cấp phát nó. Mỗi trường hợp sau là lỗi kiểu:

- Làm nó thành giá trị của biểu thức `unsafe` (nên nó cũng không thể được trả về từ một hàm)
- Bắt giữ nó trong một closure (`lambda`, `labels`)
- Truyền nó cho `task` / `thread`
- Ném nó bằng `throw`

Để dùng các giá trị bên ngoài `unsafe`, hãy sao chép chúng vào một `defstruct` hoặc các số bên trong `unsafe`
và trả về những thứ đó.

**Bộ nhớ do phía C cấp phát không được xử lý.** Các giá trị đi vào từ C dưới dạng con trỏ có kiểu (giá trị trả
về của `defffi`, đối số của callback, giá trị đọc từ các trường kiểu con trỏ) được kiểm tra lúc chạy xem chúng
có trỏ vào một giá trị kiểu đó trong một vùng cấp phát `c-alloc` còn sống hay không, và là lỗi nếu không. NULL
cũng là lỗi. Để nhận bộ nhớ do C cấp phát, hoặc NULL, hãy dùng `ptr` không kiểu (nội dung của nó không thể
đọc).

```lisp
(unsafe (def-c-struct item (key i32) (tag u8)))
(defffi (c-qsort "qsort") (ptr c-ulong c-ulong (fn ((ptr item) (ptr item)) i32)) ())

(unsafe
  (let ((xs (c-alloc item 4)))
    ...
    (c-qsort (as ptr xs) 4 8 (lambda ((a (ptr item)) (b (ptr item))) i32 (- a::key b::key)))
    ...))
```

Khi đối số của một callback bị phép kiểm tra từ chối, nó được báo cho bên gọi khi hàm C trả về, y như một thất
bại bên trong một callback.

### 3.4 defvar / defparameter / defconstant — biến toàn cục

```lisp
(defvar (name Type) init-expr)        ; chỉ khởi tạo nếu chưa được gán
(defparameter (name Type) init-expr)  ; gán mỗi lần
(defconstant (name Type) init-expr)

; với docstring (cùng thứ tự như defvar/defparameter/defconstant của CL: sau giá trị)
(defvar (name Type) init-expr "doc")
(defconstant (name Type) init-expr "doc")
```

**Sự khác nhau giữa `defvar` và `defparameter` thể hiện khi nạp lại** (như trong CL). Nếu biến toàn cục
**đã được gán, `defvar` thậm chí không đánh giá biểu thức khởi tạo**, nên khi bạn sửa một tệp cấu hình và đọc
lại, các giá trị mà phiên đã thay đổi vẫn giữ nguyên. `defparameter` gán mỗi lần, nên đọc lại sẽ đưa các giá
trị về những gì đã được viết.

Chú thích kiểu là bắt buộc (nó không được suy ra từ biểu thức khởi tạo). `defvar` có thể thay đổi;
`defconstant` thì không (`setf` là lỗi).

### 3.5 defmethod — định nghĩa phương thức

```lisp
; phương thức thể hiện: có thể gọi là (m obj args...)
(defmethod name ((self Type) (arg Type2) ...) RetType body...)

; hàm tĩnh / liên kết: có thể gọi là (Type::name args...)
(defmethod name (Type (arg Type2) ...) RetType body...)
```

Bên gọi phân giải phương thức từ kiểu tĩnh của `obj` (dispatch đơn, tĩnh). Một docstring có thể đặt ở cùng vị
trí và theo cùng quy tắc như với `defun` (ngay sau mệnh đề `where`, ở đầu thân, chỉ khi có các dạng của thân
theo sau). Điều tương tự áp dụng cho các phương thức bên trong `impl`; chúng được lấy bằng
`(documentation Type::method)`.

Tham số kiểu riêng của một phương thức được viết trong tên của nó bằng `<...>`, giống như `defun`.
Tham số kiểu của kiểu bên nhận (`T` bên dưới) do bên nhận quyết định; tham số riêng của phương thức
(`U`) được suy ra từ các đối số của mỗi lần gọi.

```lisp
(defstruct Box<T> (v T))

(defmethod fmap<U> ((self Box<T>) (f (fn (T) U))) Box<U>
  (Box::new (f self::v)))

(fmap (Box::new 3) (lambda ((x int)) string (format false "~a" x)))   ; Box<string>
```

- Đặt cho tham số kiểu riêng của phương thức những tên khác với các tham số kiểu mà kiểu bên nhận
  khai báo (`T` của `(defstruct Box<T> ...)`) và khác với các tên viết trong bên nhận.
- Nếu kiểu bên nhận là generic, trong bên nhận hãy viết tất cả tham số kiểu của nó dưới dạng biến
  (`Box<T>`) hoặc tất cả dưới dạng kiểu cụ thể (`Box<int>`).
- Phương thức bên trong `impl` không thể thêm tham số kiểu: chữ ký của nó theo chữ ký mà trait khai
  báo.

### 3.6 defstruct — struct (kiểu do người dùng định nghĩa)

```lisp
(defstruct Name
  (field1 Type1)
  (pub field2 Type2)
  ...)

; generic (các tham số kiểu trong ngoặc nhọn)
(defstruct Name<T1,T2...>
  (field TypeUsingT1)
  ...)
```

- Mỗi trường là `(name type)` hoặc `(pub name type)` (khả năng hiển thị theo từng trường, độc lập với `pub`
  của chính struct). Thêm một biểu thức ở cuối trở thành **giá trị mặc định** của slot (`(x i32 0)`); xem danh
  sách tùy chọn bên dưới.
- Những thứ sau được sinh tự động:
  - Hàm khởi tạo `Name::new` (đối số theo thứ tự trường)
  - Các getter `(field-name instance)`, với đường cú pháp `instance::field-name`
  - Các setter `(set-field-name instance value)`, với đường cú pháp `(setf instance::field-name value)`
- Để làm cho chính struct là `pub`, hãy đặt `pub` ở phía trước, như `(pub defstruct ...)`.
- **Hãy định nghĩa một kiểu trước khi nêu tên nó.** Kiểu của một trường có thể là chính struct đó
  (`(next Option<node>)`), nhưng không thể là một kiểu được định nghĩa sau: kiểu không có khai báo trước tương
  ứng với `defsignature`. Một tên chưa được định nghĩa cho cùng lỗi `unknown type` trong một kiểu đối số của
  `defun` hoặc trong `the`. Vì vậy hai kiểu tham chiếu lẫn nhau không thể viết được.
- **Biến kiểu chỉ là những biến được viết ở các vị trí khai báo.** Với
  `defun`/`defstruct`/`defenum`/`deftype`, là `<T>` của tên; với `defmethod`, là kiểu của bên nhận
  (`(self box<T>)`, hoặc `box<T>` với một hàm tĩnh) và `<U>` của tên phương thức; với `impl`, là
  kiểu đích và `impl<T>`; với `deftrait`, là `Self` và các kiểu liên kết của `(type Item)`. Một tên
  xuất hiện lần đầu ở bất kỳ nơi nào khác (đối số, giá trị trả về, `the`/`lambda` trong thân) không
  trở thành biến kiểu; nó là `unknown type`.
- **Docstring**: một literal chuỗi ngay sau tên, trước các trường, trở thành docstring
  (`(defstruct Name "doc" (field Type)...)`, cùng vị trí như `defstruct` của CL). Một trường luôn có dạng
  `(name Type ...)` và không bao giờ có thể là một chuỗi trần, nên không có sự mơ hồ. Lấy nó bằng
  `(documentation Name)`.

#### Danh sách tùy chọn

Viết một danh sách `(Name option...)` ở vị trí tên chỉ định các tùy chọn (cùng vị trí như CL).

```lisp
(defstruct (point (:constructor make-point)          ; hàm khởi tạo keyword
                  (:constructor at (x &optional y))  ; hàm khởi tạo BOA
                  (:copier copy-point))
  (x i32 0)          ; phần tử thứ ba là giá trị mặc định của slot đó
  (y i32 0))

(point::make-point :y 7)   ; x là 0
(point::at 1)              ; y là 0
(point::at 1 2)
(copy-point p)             ; một bản sao nông (giống copier của CL)
```

- **`:constructor`**: thứ được sinh ra là một **hàm tĩnh** của kiểu (`point::make-point`), có thân luôn là
  `(point::new ...)`. `new` vẫn là hàm khởi tạo cấu trúc duy nhất; thứ được tạo ở đây là một *cách gọi* nó.
  Có thể khai báo nhiều cái.
  - `(:constructor name)` nhận mọi slot dưới dạng `&key`. **Mọi slot cần có mặc định** (ngôn ngữ này không có
    gì tương ứng với "slot chưa gán" của CL).
  - `(:constructor name (slot...))` nhận các slot được nêu tên làm đối số theo vị trí (theo thứ tự bất kỳ). Các
    slot không được nêu tên được điền bằng mặc định của chúng, nên **chúng cần có mặc định**. Sau
    `&optional`, phần còn lại có thể bỏ qua (và tương tự cần có mặc định).
- **`:copier`**: sinh ra một **phương thức thể hiện** trả về một giá trị mới có cùng giá trị slot. Nông, như
  copier của CL.
- **`:include Parent`**: đặt các slot của cha lên trước (mặc định cũng được kế thừa; cha có thể ở tệp khác).
  **Nó không tạo quan hệ kiểu nào**: con không phải kiểu con của cha, các phương thức của cha không áp dụng
  cho con, và không có phép kiểm tra lúc chạy nào nối hai kiểu. Ngôn ngữ này không có subtyping; các giao diện
  chung là việc của `deftrait`. Chỉ có *danh sách* các slot được nối.
- **Mặc định của slot chỉ được đọc bởi các hàm khởi tạo được sinh ra.** Viết một mặc định mà không khai báo
  `:constructor` nào là lỗi, vì nó không bao giờ có thể được dùng.
- Các tùy chọn bị bỏ, và lý do:
  - **`:conc-name`**: trong CL nó thêm tiền tố cho các hàm truy cập để tránh xung đột trong một không gian tên
    hàm phẳng. Ở đây, các hàm truy cập là phương thức dispatch theo kiểu của bên nhận, nên xung đột không xảy
    ra, và một tiền tố sẽ làm hỏng `instance::field` (vốn chỉ biết tên slot).
  - **`:predicate`**: trả lời lúc chạy "giá trị này có phải một `point` không?". Ở đây kiểu là một phân loại
    lúc biên dịch không có bằng chứng lúc chạy, và không có vị trí nào tồn tại "một giá trị có kiểu chưa biết
    có thể là một point" (`match` trên `Sexpr` là kín, và `:dyn` không thể downcast), nên một vị từ được sinh
    ra chỉ có thể trả về `true`.
  - **`:type` / `:initial-offset` / `:named`**: các tùy chọn này thay biểu diễn của giá trị bằng một danh sách
    hoặc vector. Biểu diễn thuộc về trình biên dịch và không thể quan sát từ ngôn ngữ.

### 3.7 defenum — enum (sum type)

```lisp
(defenum Name
  (Variant1 Type1 Type2...)   ; một variant có payload (các trường theo vị trí)
  (Variant2)                  ; một variant không có payload
  ...)

; generic
(defenum Option<T>
  (Some T)
  (None))
```

- Mỗi variant có dạng `(VariantName FieldType...)`. Các trường chỉ theo vị trí (chúng không có tên). Cần ít
  nhất một variant, và tên không được lặp lại.
- Giá trị được tạo, như với `Option`/`Result` dựng sẵn, có tiền tố hoặc qua `use`: `(Name::Variant1 a b)`,
  hoặc `(Variant1 a b)` sau `(use Name)`.
- Chúng có thể được tách bằng `match` / `if-let`. `match` kiểm tra tính đầy đủ (nó phải bao phủ mọi variant
  hoặc có một `_`):
  ```lisp
  (match opt
    ((Some v) v)
    ((None) 0))
  ```
- Phương thức và hàm liên kết được thêm sau bằng `defmethod`/`impl`, như với `defstruct`.
- Để làm cho chính enum là `pub`, hãy viết `(pub defenum ...)`.
- **Docstring**: cùng vị trí và quy tắc như `defstruct`, ngay sau tên, trước các variant
  (`(defenum Name "doc" (Variant ...)...)`). Lấy nó bằng `(documentation Name)`.

### 3.8 deftype — bí danh kiểu

```lisp
(deftype meters i32)
(deftype fallible<T> Result<T,string>)
(deftype pred (fn (i32) bool))

(defun double ((m meters)) meters (* m 2))
(defun parse ((s string)) fallible<i32> ...)
```

`deftype` của CL, được thu hẹp vào những gì có nghĩa trong một ngôn ngữ có kiểu tĩnh: **một cách viết của một
kiểu, không phải một kiểu**.

- Vị trí tên giống `defun`, và các đối số generic được viết `Name<T,U>`. Tại nơi dùng, cần đúng số đối số kiểu
  đã khai báo (thừa hay thiếu đều là lỗi ngay tại chỗ).
- Việc khai triển xảy ra **bên trong bộ phân tích kiểu**. Vì vậy không có gì ở phía sau biết bí danh tồn tại:
  các khóa đơn hình hóa, dump, đường biên dịch và **thông báo lỗi** đều hiển thị dạng đã khai triển. Nếu
  `(f "x")` thất bại với một hàm yêu cầu `meters`, thông báo nói `i32`.
- **Nó không phải một kiểu mới.** `(deftype meters i32)` làm cho `meters` và `i32` là cùng một kiểu, nên nhầm
  lẫn chúng không bị bắt. Nếu bạn muốn tách chúng ra, hãy dùng `defstruct`.
- **Nó không phải một vị từ.** `(deftype small () '(integer 0 9))` của CL mô tả một *tập giá trị* mà `typep`
  kiểm tra lúc chạy, nhưng ở đây kiểu là một phân loại lúc biên dịch không có bằng chứng lúc chạy, nên một bí
  danh hạn chế giá trị sẽ không có gì để hạn chế.
- **Nó không thể chứa chính nó.** Một bí danh được khai triển tại nơi nó được viết, nên không có chỗ nào để nó
  đệ quy tới. Các kiểu dữ liệu đệ quy được viết bằng `defstruct`/`defenum`.
- Nó dùng chung không gian tên với kiểu và trait (trong một module nó không thể trùng tên với một
  `defstruct`/`defenum`/`deftrait`). Làm cho nó công khai bằng `(pub deftype ...)` và đưa vào bằng
  `(use m::meters)`.
- **Docstring**: ngay sau tên, trước kiểu (`(deftype Name "doc" Type)`).

### 3.9 deftrait / impl — trait

```lisp
(deftrait TraitName (SuperTrait...)      ; danh sách supertrait là bắt buộc; () nếu không có
  (type AssocName)                       ; các kiểu liên kết (số lượng bất kỳ, tùy chọn)
  (method-name ((self Self) params...) RetType)          ; không có thân = phải được triển khai
  (method-name ((self Self) params...) RetType body...)) ; có thân = triển khai mặc định

(impl TraitName TargetType
  (where (Trait A)...)                   ; các ràng buộc áp dụng cho cả impl (tùy chọn)
  (type AssocName ConcreteType)          ; làm cho một kiểu liên kết trở nên cụ thể
  (method-name (recv params...) RetType body...))
```

Thông qua `impl`, mỗi phương thức được đăng ký như một `defmethod` thông thường của `TargetType`. Trait được
tham chiếu như các ràng buộc trait trong các mệnh đề `where` của hàm generic (xem
[3.1 defun](#31-defun--định-nghĩa-hàm)). Tên trait cũng có thể là một đường dẫn `::` như `m::Trait`.

**Danh sách supertrait (bắt buộc)**: luôn được viết ngay sau tên trait. Mỗi phần tử là một tên trait trần,
hoặc, nếu trait đó có kiểu liên kết, là `(Trait (Assoc Type))` với **mọi kiểu liên kết của nó được cố định**.

```lisp
(deftrait Eq () ...)                       ; không có supertrait
(deftrait Ord (Eq) ...)                    ; trait Ord: Eq của Rust
(deftrait CharSource ((Iter (Item char)))  ; cố định một kiểu liên kết
  (rewind ((self Self)) ()))
```

Kế thừa có ba tác dụng. (1) `impl Ord X` yêu cầu `impl Eq X` phải được viết **trước** (một quy tắc về thứ tự
viết: dạng duy nhất có thể được quyết định một cách xác định trong REPL và với `load` từng bước, và nghiêm
ngặt hơn Rust). (2) Chỉ riêng `(where (Ord T))` đã cho phép bạn gọi cả các phương thức của `Eq`. (3) Các phương
thức của `Eq` có thể được gọi qua một `:dyn Ord`, và một giá trị `:dyn Ord` có thể được truyền nguyên trạng ở
nơi cần một `:dyn Eq` (upcasting). Một subtrait khai báo lại một phương thức cùng tên với cha của nó, và kế
thừa các phương thức cùng tên từ hai cha, đều là lỗi (một vtable có một ô cho mỗi tên). Kế thừa kim cương gộp
thành một ô.

**Triển khai mặc định**: một thân sau chữ ký được dùng khi một `impl` bỏ phương thức ra. Thân được phân giải
trong **không gian tên của module** nơi trait được viết, nên nó có thể gọi các hàm không công khai của module
đó. Các phương thức có thân cũng có thể có mệnh đề `where` và docstring. Thân được kiểm tra kiểu **một lần,
tại điểm khai báo**, với `Self` để lại là một biến kiểu (bị ràng buộc bởi `Self: chính trait đó`), như trong
Rust: các sai sót sẽ thất bại với mọi `impl` và mọi kiểu triển khai, ngay cả trong các mặc định mà không `impl`
nào bỏ ra, đều bị bắt ở đó. Các lời gọi trên `self` tới các phương thức của chính trait hoặc các supertrait của
nó đi qua ràng buộc này, và các kiểu liên kết được cố định bằng chính chúng, nên một chữ ký trả về `Item` được
khớp với thân mà không cần biết kiểu cụ thể.

**Blanket implementation**: làm cho đích là một biến kiểu thì triển khai trait cùng lúc cho mọi kiểu đáp ứng
các ràng buộc.

```lisp
(deftrait Clamp (Ord)
  (clamp ((self Self) (lo Self) (hi Self)) Self
    (if (less self lo) lo (if (less hi self) hi self))))
(impl<T> Clamp T (where (Ord T)))          ; hoàn toàn không có thân; mọi thứ là mặc định
```

**Không có mã nào được sinh ra cho đến khi một kiểu cụ thể thực sự dùng nó** (một lần cho mỗi kiểu, bằng cùng
cơ chế như đơn hình hóa thông thường). Một trait có nhiều nhất một blanket implementation. Nếu một kiểu có một
`impl` tường minh, cái đó được ưu tiên. Việc kiểm tra kiểu thân tách biệt với việc sinh mã: nó được thực hiện
một lần tại điểm khai báo, **với đích để lại là một biến kiểu** (như trong Rust), nên ngay cả một triển khai
không bao giờ được dùng cũng có sai sót bị bắt ở đó nếu chúng sẽ thất bại với mọi đích dưới các ràng buộc đã
khai báo. Các lời gọi được biện minh bởi các ràng buộc (`(less self other)` dưới `(where (Ord T))`, v.v.) qua
được, như trong thân của một `defun` generic.

**Docstring**: một `deftrait` có thể có một docstring cho cả trait, dưới dạng một literal chuỗi ngay sau danh
sách supertrait, trước các mục (`(deftrait Name () "doc" (type ...) (method ...)...)`). Một chữ ký không có
thân không thể có docstring: một chuỗi ở cuối sẽ tự nó là giá trị trả về của một triển khai mặc định, nên không
thể phân biệt hai trường hợp.

Các trait mà thư viện chuẩn cung cấp: **`Iter`** (`next` / kiểu liên kết `Item`; nền tảng của `doiter` và các
hàm trên dãy), **`Eq`** (`equals`; `not-equals` là triển khai mặc định), **`Ord`** (kế thừa `Eq`; chỉ `less` phải
được triển khai, còn `less-equal` / `greater` / `greater-equal` là các triển khai mặc định), **`Error`**
(`message` / `source`; `:dyn Error` để xử lý các kiểu lỗi một cách đồng nhất), **`print-object`** (biểu diễn in
theo từng kiểu), **`Pathish`** (pathname designator: một chuỗi hoặc một `pathname`), và hệ phân cấp stream
**`Stream`** → **`InputStream`** / **`OutputStream`** → **`CharInput`** / **`CharOutput`** → **`PeekInput`**.
Kiểu nào triển khai trait nào nằm ở [types.md](types.md); các phương thức của từng trait nằm ở
[Các trait chuẩn](functions/traits.md), [Các kiểu lỗi](functions/option-result.md#3-các-kiểu-lỗi-và-trait-error),
[print-object](functions/printing.md#5-print-object-biểu-diễn-in-theo-từng-kiểu) và
[Stream](functions/streams-files.md). Nếu bạn `impl` `Iter` cho kiểu tập hợp của riêng mình, `doiter`
(chương 5) và `map` / `filter` / `sort` và các hàm tương tự hoạt động trên nó nguyên trạng.

Các lời gọi trait mặc định là **tĩnh** (được phân giải theo kiểu tĩnh của bên nhận). Để xử lý các giá trị có
kiểu cụ thể được quyết định lúc chạy, kiểu trait object `:dyn Trait` (chương 2) cho dispatch động qua một
vtable:

```lisp
(deftrait Drawable () (draw ((self Self)) string))
(defstruct circle (r i32))
(defstruct square (side i32))
(impl Drawable circle (draw ((self Self)) string "circle"))
(impl Drawable square (draw ((self Self)) string "square"))

(defun render-all ((xs Vector<:dyn Drawable>)) ()
  (doiter (d (iter xs)) (println "~a" (draw d))))   ; một vị trí gọi, một đáp án cho mỗi triển khai
```

Chỉ những trait mà "mọi phương thức đều có bên nhận `self`, không dùng `Self` ở đâu khác ngoài bên nhận, và tự
nó không phải generic cũng không có đối số biến đổi" mới có thể làm `:dyn` (các phương thức kế thừa phải đáp
ứng cùng điều kiện).

Chỉ những kiểu có giá trị có biểu diễn trên heap mới có thể đi vào một hộp `:dyn`:

| Có thể đi vào | Không thể đi vào |
|---|---|
| Các kiểu `defstruct` / `defenum` (gồm `Vector<T>`, `cons-cell<A,B>`, `Result<T,E>` và các struct của thư viện chuẩn), `HashTable<K,V>`, `Sexpr`, `int`, `ratio`, `f64`, `string`, `random-state` | Số nguyên độ rộng cố định (`i8` đến `u32`), `f32`, `bool`, `char`, `symbol`, `()`, kiểu hàm, và `Option<T>` không có hộp ([biểu diễn lúc chạy của Option](functions/option-result.md#2-biểu-diễn-lúc-chạy-của-optiont)) |

Đặt một giá trị của kiểu không thể đi vào vào chỗ mong đợi một `:dyn` là lỗi kiểu. Để xử lý các giá trị như vậy
qua `:dyn`, hãy bọc chúng trong một struct, như `(defstruct flag (v bool))`.

### 3.10 module / use — không gian tên

```lisp
(module path body...)      ; path là một dãy các đoạn như foo hoặc foo::bar
(in-module path)           ; từ đây đến cuối đơn vị này, nằm bên trong path (dạng phẳng của module)
(use path...)              ; đặt bí danh cho hàm, kiểu và module vào không gian tên hiện tại
(import path...)           ; giống use (một cách viết tương thích CL)
(shadowing-import path...) ; một use cố ý lấy một tên trần đã được dùng
```

- `module` tạo một không gian tên. **Kiểu không phải là không gian tên** (như trong Rust, một kiểu chỉ có các
  hàm liên kết và phương thức).
- `use` một kiểu cũng làm cho các hàm khởi tạo và các phương thức tĩnh công khai của nó khả dụng bằng tên trần
  (ví dụ, sau `(use option)`, `some`/`none` có thể được gọi mà không cần `option::some`/`option::none`).
- Thứ tự phân giải của các tên trần (định danh không có tiền tố): dạng đặc biệt → hàm khởi tạo → hàm tự do
  (không gian tên hiện tại → gốc) → phương thức thể hiện (được phân giải theo kiểu tĩnh của đối số đầu tiên).
  Nó không đi ngược lên qua các module cha trung gian.
- Một đường dẫn có tiền tố `a::b` phân giải `a` theo thứ tự trên; nếu nó là một module thì đi vào bên trong,
  và nếu nó là một kiểu, đoạn cuối được phân giải như một mục liên kết.
- **`use` ảnh hưởng đến các dạng sau nó.** Một tệp được đọc từng dạng một, và các phụ thuộc được phân giải ngay
  trước khi dạng được kiểm tra, nên viết `m::f` **phía trên** `(use m)` cho `unresolved path`. Hãy đặt `use` ở
  đầu tệp.
- **`use` có thể nhận nhiều đường dẫn** (`(use a::f b::g)`). `import` là một cách viết tương thích CL với cùng
  hành vi.
- **Một `use` mà tên trần đã bị chiếm sẽ được báo.** Việc phân giải một tên trần nhìn vào các định nghĩa của
  chính module trước các bí danh, nên `(use m::twice)` sau `(defun twice ...)` **không làm gì cả**. Nếu bạn chủ
  ý, hãy viết `shadowing-import` (nó vẫn không thể thắng một định nghĩa, vì không có cách nào gỡ một định
  nghĩa; nó chỉ thắng các bí danh trước đó).
- **`in-module` là dạng phẳng của `(module path body...)`.** Viết `(in-module geometry)` đặt mọi thứ từ đó đến
  cuối đơn vị (tệp, hoặc thân của `module` bao ngoài) vào bên trong `geometry`. Nó nằm **bên trong** module
  riêng của tệp (`main::geometry` cho `main.typl`). Hai cái liên tiếp lồng theo thứ tự. Nó khác `in-package` của
  CL, và được đặt tên khác: trong hệ thống này tệp đã là một module, nên không có gì để "chọn", và tất cả những
  gì một dạng có thể làm là lồng.

### 3.11 Tệp và module (dự án nhiều tệp)

Đường dẫn tệp so với thư mục gốc mã nguồn là đường dẫn module: nội dung của `<root>/geo/point.typl` được bọc
ngầm trong module `geo::point` (một thư mục cũng là một đoạn, theo kiểu Rust / Python). Một `(module bar ...)`
tường minh trong tệp lồng **bên trong** nó (`geo::point::bar`), nên đường dẫn suy ra và khai báo tường minh
không bao giờ va chạm.

- **Thư mục gốc mã nguồn**: đặt một tệp manifest `typelisp.toml` ở gốc dự án (nó có thể rỗng; tùy chọn một dòng
  `src = "src"` đặt tên thư mục mã nguồn). Nó được tìm bằng cách đi ngược lên từ thư mục của tệp đích. Không có
  manifest, thư mục của tệp đầu vào (thư mục hiện tại với REPL) là thư mục gốc.
- **Nạp theo yêu cầu**: khi `(use geo::point)` tham chiếu tới một module chưa nạp, tệp tương ứng
  (`geo/point.typl`) được nạp, kiểm tra kiểu và đăng ký tự động. `use a::b::c` tìm tiền tố dài nhất trước:
  `a/b/c.typl` → `a/b.typl` → `a.typl` (vì `c` có thể là một mục bên trong một module). Các định nghĩa nhìn thấy
  được từ các module khác cần `pub` ([3.13 pub](#313-pub--khả-năng-hiển-thị)).
- **Tham chiếu vòng là lỗi**: chuỗi được báo dưới dạng `circular module dependency: a -> b -> a`.
- **Chạy**: `typl <file.typl>` chạy một tệp (không có đối số, là REPL). `use` trong REPL phân giải tệp theo
  cùng quy tắc.
- **Dung lượng arena cons**: `typl --heap-cells N` đặt **dung lượng ban đầu** của arena ô cons (mặc định 65536;
  dạng `--heap-cells=N` cũng dùng được, cho cả chạy tệp lẫn REPL). Arena **tăng trưởng bằng cách thêm** khi thiếu.
  Giới hạn tăng trưởng là 256 lần dung lượng ban đầu, và một lần cấp phát vượt quá nó cho `heap exhausted`: dung
  lượng ban đầu nghĩa là "cấp phát chừng này lúc đầu", và giới hạn nghĩa là "vượt quá đây, coi là rò rỉ".

### 3.12 load — nạp phẳng

```lisp
(load "path")   ; chỉ cấp cao nhất; path là một literal chuỗi
```

- **Nạp phẳng** theo kiểu CL: đọc các dạng của tệp đích **vào không gian tên hiện tại** nguyên trạng (không bọc
  chúng trong một module, khác với `use`). Chỉ cấp cao nhất (bên trong thân hàm là lỗi kiểu).
- `path` tương đối với thư mục của tệp đang nạp (từ REPL, với cwd của tiến trình). Nếu nó không có phần mở
  rộng, `.typl` được thêm vào.
- `(load ...)`/`(use ...)` trong tệp được nạp cũng được xử lý đệ quy.
- **Nó đọc từng dạng một và chạy ngay tại chỗ** (như `load` của CL). Dạng *k* đã chạy xong trước khi *k+1* được
  đọc: ngay cả khi có lỗi cú pháp hay kiểu giữa chừng, các dạng trước nó đã chạy rồi. Các tệp module được nạp bởi
  `use` thì khác: chúng được kiểm tra như một đơn vị và việc chạy chúng được giao cho bên đã `use` chúng (tương
  ứng với `compile-file` của CL).

### 3.13 pub — khả năng hiển thị

```lisp
(pub defun ...)
(pub defsignature ...)
(pub defffi ...)
(pub defvar ...)
(pub defparameter ...)
(pub defconstant ...)
(pub defmacro ...)
(pub defmethod ...)
(pub defstruct ...)
(pub defenum ...)
(pub deftype ...)
```

`pub` chỉ có thể đặt trên mười một loại ở trên (không phải `module`/`use`/`deftrait`/`impl`). Nó được viết với
từ khóa định nghĩa ngay sau `pub`, không phải dạng `(pub (defun ...))` bọc định nghĩa trong ngoặc. Một `pub`
chỉ công khai đúng một định nghĩa (không thể đánh dấu nhiều định nghĩa cùng lúc).

### 3.14 defmacro — định nghĩa macro

```lisp
(defmacro name (required... &optional opt... &rest rest-name &key key...) body...)
```

- Mọi tham số và giá trị trả về luôn là `Sexpr`, nên không viết chú thích kiểu.
- Macro không vệ sinh kiểu CL (tránh xung đột với `gensym` là trách nhiệm của tác giả macro).
- Danh sách lambda theo thứ tự của CL `required &optional &rest &key` (mỗi dấu hiệu tối đa một lần, và chỉ theo
  thứ tự này).
  - `&optional` … các đối số tùy chọn. `name` hoặc `(name default-expr)`. Biểu thức mặc định được đánh giá lúc
    khai triển (nó có thể tham chiếu các tham số đã gán trước) và được gán khi đối số bị bỏ qua (nếu không có
    mặc định, là danh sách rỗng `()`).
  - `&rest name` … nhận các đối số theo vị trí còn lại gộp thành một danh sách `Sexpr`.
  - `&key` … các đối số keyword. `name` hoặc `(name default-expr)`. Bên gọi truyền chúng dưới dạng `:name value`
    (theo thứ tự bất kỳ). Khi bị bỏ qua, là biểu thức mặc định (danh sách rỗng `()` nếu không có). Keyword lạ
    hoặc một dãy `:key` có độ dài lẻ là lỗi.
- Ví dụ: `(defmacro pair (x &optional (y 1)) ...)` / `(defmacro make (&key (a 0) (b 9)) ...)`.

### 3.15 macrolet / symbol-macrolet — ràng buộc macro cục bộ

```lisp
(macrolet ((name (lambda-list) body...) ...) body...)   ; macro có phạm vi từ vựng
(symbol-macrolet ((name expansion) ...) body...)         ; một tên đại diện cho một dạng
```

Cả hai là các dạng đặc biệt **biểu thức**, và không có gì còn lại lúc chạy (thứ được biên dịch là dạng đã khai
triển của thân). Danh sách lambda giống như của `defmacro`. Các quy tắc chi tiết và ví dụ nằm ở
[Ràng buộc macro cục bộ](functions/system.md#9-ràng-buộc-macro-cục-bộ-macrolet--symbol-macrolet).

## 4. Ràng buộc và rẽ nhánh

```lisp
(let ((name val) ...) body...)      ; ràng buộc song song
(let* ((name val) ...) body...)     ; ràng buộc tuần tự (các ràng buộc trước dùng được trong các biểu thức khởi tạo sau)

(if cond then else)                 ; else là bắt buộc (luôn có ba phần tử)
(when cond body...)                 ; một if không có else (kiểu Unit). defmacro
(unless cond body...)               ; phủ định của when. defmacro
(cond (test1 body...) (test2 body...) ... (else body...))   ; defmacro
(case expr
  (key1 body...)
  ((key2 key3) body...)             ; một danh sách khóa: khớp nếu bất kỳ khóa nào khớp
  (else body...))                   ; expr được đánh giá một lần. các khóa được so sánh bằng equal.
                                     ; các khóa là "literal" và không được đánh giá (như trong CL).
                                     ; một symbol trần a nghĩa là symbol 'a.
                                     ; viết 'a là lỗi (hãy dùng a trần). defmacro
(ecase expr (key body...) ...)      ; một case yêu cầu phải khớp. panic nếu không có gì khớp. defmacro
(ccase expr (key body...) ...)      ; ccase của CL. không có restart nào để đưa ra, nên giống ecase. defmacro
(and expr...)                       ; đánh giá đoản mạch. true khi không có đối số. defmacro
(or expr...)                        ; đánh giá đoản mạch. false khi không có đối số. defmacro
(progn body...)                     ; chạy theo thứ tự và trả về giá trị cuối
(unsafe body...)                    ; giống progn, cộng thêm quyền viết các lời gọi FFI
                                     ; và các từ thô. xem 3.3 defffi
(prog1 form more...)                ; đánh giá tất cả; giá trị là của form. defmacro
(prog2 a b more...)                 ; đánh giá tất cả; giá trị là của b. defmacro
(the Type expr)                     ; một chú thích kiểu (không có tác dụng lúc chạy)
```

### 4.1 unsafe — nhận các giả định không thể kiểm tra

```lisp
(unsafe body...)
```

Giống `progn`: đánh giá thân theo thứ tự và trả về giá trị cuối. Nó không tạo phạm vi và không phải ranh giới
hàm (`break` / `return-from` đi thẳng qua ra bên ngoài). Khác biệt là một số thứ chỉ có thể viết bên trong nó.

Hiện có ba thứ yêu cầu `unsafe`: gọi các hàm C được khai báo bằng
[defffi](#33-defffi--khai-báo-hàm-c-ffi), biến các từ máy thô (`ptr` / `c-long` / `c-ulong` /
`(ptr T)`) thành giá trị, và [`def-c-struct` và `c-alloc`](#def-c-struct-and-typed-pointers--cấp-phát-struct-c).

Bộ nhớ cấp phát bằng `c-alloc` được giải phóng khi rời `unsafe` ngoài cùng nhất trong cùng hàm. Chỉ `unsafe`
đó, khác `progn`, có việc phải làm khi ra: giải phóng.

Những gì `unsafe` nhận là các giả định sau mà trình biên dịch không thể xác minh:

- **Rằng các kiểu khớp nhau.** Rằng chữ ký C đã khai báo khớp với chữ ký thật. Nếu không, các đối số đi vào sai
  thanh ghi và giá trị trả về được đọc sai độ rộng.
- **An toàn bộ nhớ.** Phía C làm gì với những gì nó được đưa.
- **Trạng thái toàn tiến trình.** Biến môi trường, trình xử lý tín hiệu, `errno`. Ví dụ, gọi `setenv` qua FFI
  phá vỡ các giả định mà `decode-universal-time` của triển khai này đưa ra khi tính giờ địa phương.
- **An toàn thread.**

Nó không phải lối thoát khỏi việc kiểm tra kiểu. `(unsafe (+ 1 "two"))` không qua được. Thứ được cho phép là
viết một số **thao tác** nhất định, không phải viết điều vô nghĩa.

Nó hoạt động theo phạm vi từ vựng. Thân của một `lambda` viết bên trong `unsafe` kế thừa quyền (như với
closure bên trong khối `unsafe` của Rust). Giá trị sau đó có thể được gọi từ bên ngoài `unsafe`, nhưng việc
viết nó ở đó tự nó được coi là chấp nhận trách nhiệm.

### 4.2 destructuring-bind — tách danh sách theo hình dạng

```lisp
(destructuring-bind lambda-list form body...)
```

Tách danh sách mà `form` tạo ra **theo hình dạng của nó** và gán nó. Danh sách lambda là của `defmacro`
(required → `&optional` → `&rest`/`&body` → `&key`, mỗi cái có biểu thức mặc định), vì cùng lý do CL dùng chung
một danh sách cho cả hai: chúng là hai dạng tách cùng một thứ.

```lisp
(destructuring-bind (op a b) (quote (+ 1 2)) (format false "~a ~a ~a" op a b))  ; "+ 1 2"
(destructuring-bind (head &rest tail) xs (format false "~a | ~a" head tail))
(destructuring-bind (a &optional (b 9)) (quote (1)) b)                          ; 9
(destructuring-bind (&key (x 0) y) (quote (:y 7)) (format false "~a ~a" x y))   ; "0 7"
```

- **Mọi biến được gán là một `Option<Sexpr>`.** Đây không phải hạn chế của triển khai mà là bản chất của thứ
  được gán: danh sách S-expression là danh sách duy nhất trong ngôn ngữ này, nên không có kiểu nào khác để đưa
  cho các phần tử. Quay về `match` ở nơi cần một giá trị vô hướng thì giống như trong thân `defmacro`.
- **Một hình dạng không khớp sẽ panic** (tương ứng với lỗi của CL): quá ít hoặc quá nhiều phần tử, một dãy
  `&key` có độ dài lẻ, hoặc một keyword lạ. `sexpr-car` là một hàm dễ dãi trả về `()` cho `()`, nên không có
  phép kiểm tra, một danh sách ngắn sẽ lặng lẽ được gán vào một dãy rỗng.
- **Danh sách lambda lồng nhau không được hỗ trợ.** `defmacro` cũng không nhận chúng, nên chỉ có một quy tắc.
  `(a (b c))` không âm thầm gán một danh sách con cho `b`; nó là một lỗi nói rõ như vậy.
- Các biểu thức mặc định của `&optional` / `&key` **chỉ được đánh giá khi được dùng** (như trong CL).
- Không có gì tương ứng với `&allow-other-keys` của CL (`defmacro` cũng không có).

### 4.3 match — so khớp mẫu

```lisp
(match expr
  (pattern body...)
  ...)
```

Các loại mẫu:
- `_` — ký tự đại diện
- Một tên biến — một mẫu gán (luôn khớp). Tuy nhiên, nếu kiểu của đối tượng được so khớp có một variant mang tên
  đó, nó được phân giải là **mẫu tên variant trần bên dưới**
- Một tên variant trần — khớp một variant không nhận đối số (`(match c (red 1) (blue 2))`). Viết một variant có
  trường bằng tên trần là lỗi số ngôi, nên hãy viết nó trong ngoặc, như `(circle r)`
- **Literal tức thời**: số nguyên / `true`/`false` / ký tự — được so sánh như các từ
- **Literal giá trị**: chuỗi / số dấu phẩy động / symbol (`'foo`) / số nguyên bignum / ratio — được so sánh theo
  giá trị bằng `Eq::equals` của kiểu đó ([Các trait chuẩn](functions/traits.md#2-eq--ord-so-sánh)). Chuỗi so
  sánh theo nội dung, không theo đồng nhất
- `(= expr)` — đánh giá một biểu thức bất kỳ và so sánh bằng `Eq::equals`. Cách duy nhất để so sánh các kiểu
  không có cú pháp literal (các thể hiện `defstruct`, biến toàn cục, kết quả được tính), và một triển khai `Eq`
  do người dùng định nghĩa trở thành quy tắc so sánh nguyên trạng. `expr` có thể tham chiếu mọi thứ nhìn thấy
  được từ vị trí của nhánh (đối số, ràng buộc bên ngoài, biến toàn cục)
- `(Ctor sub-pattern...)` — các mẫu hàm khởi tạo (`Some x` `None` `Cons a d` `Ok v`, v.v.)

So sánh một kiểu không triển khai `Eq` với một literal giá trị / `(= expr)` là lỗi kiểu (ngôn ngữ này chọn nói
"những thứ này không thể so sánh" thay vì để lại một nhánh âm thầm không bao giờ khớp).

**Literal giá trị với đối tượng `Sexpr`**: `Eq` của `sexpr` là `eq` (đồng nhất của CL), nên các giá trị tức
thời (`'foo` (đã intern) / số nguyên / ký tự / `true`/`false`) có thể được viết nguyên trạng và khớp theo nội
dung:

```lisp
(match s ('add 1) (42 2) (#\a 3) (_ 0))
```

Các literal không tức thời (chuỗi / số dấu phẩy động / số nguyên bignum / ratio) **không thể được viết** với một
`Sexpr`. `eq` của chúng so sánh đồng nhất đối tượng, điều này sẽ tạo ra một "nhánh qua được kiểm tra kiểu nhưng
không bao giờ khớp", nên nó là một lỗi nêu tên mẫu variant: hãy viết `(str "hi")` và nó được tách thành một
`string` và so sánh theo nội dung. `(= expr)` yêu cầu `equals` tường minh, nên hạn chế này không áp dụng cho nó.

**Đối tượng được so khớp không nhất thiết phải là một ADT.** `string`/`symbol`/`i32`/`f64` và các kiểu tương tự
có thể được so khớp trực tiếp (đó là nơi các mẫu literal chuỗi đi vào). Tuy nhiên, một kiểu không có variant
không thể được bao phủ bằng liệt kê, nên cần `_` (hoặc một mẫu gán đóng vai ký tự đại diện):

```lisp
(defun kind ((s string)) i32
  (match s
    ("add" 1)
    ("sub" 2)
    (_     0)))          ; một kiểu không có variant cần `_`
```

Với một đối tượng `Sexpr`, ngoài 18 mẫu variant dựng sẵn ở trên, có thể viết **các mẫu downcast** (lấy ra các
thể hiện của ADT do người dùng định nghĩa): cú pháp để lấy lại, bằng `match`, một thể hiện của
`defstruct`/`defenum` (chương 3) đã được chuyển đổi ngầm thành `Sexpr`, như trong `(list p 42)`:

- `(TypeName sub-pattern...)` — phân rã trường với **tên kiểu** đứng đầu (chỉ struct: một `defstruct` luôn có
  một variant, nên nó được viết bằng tên kiểu thay vì tên variant). Ví dụ, với
  `(defstruct point (x f64) (y f64))`, là `(point x y)`.
- Một tên variant trần `(VariantName sub-pattern...)` — lấy ra một variant của một `defenum`. Được phân giải
  như một tên trần nhìn thấy được sau `(use EnumType)` (cùng quy tắc hiển thị như khi gọi hàm khởi tạo). Ví dụ,
  với `(defenum color (red) (blue))`, là `(red)` `(blue)` sau `(use color)`. Nếu tên variant của nhiều enum nhìn
  thấy được va chạm, đó là lỗi mơ hồ, nên cũng có thể viết dạng có tiền tố `(EnumType::VariantName ...)`
  (không cần `use`).
- `(the Type pattern)` — một downcast của toàn bộ kiểu (gán nó như một tổng thể). Nó không phân rã các trường;
  nó truyền giá trị cho `pattern` nguyên trạng. Cách duy nhất để lấy ra một struct khả biến mà vẫn giữ đồng nhất
  của nó, và cũng là cách duy nhất để lấy một `Vector<T>`/`HashTable<K,V>` ra khỏi một `Sexpr` (chúng không có
  dạng phân rã trường). Ví dụ, sau `(the point p)`, `(setf p::x 9)` được phản ánh vào thể hiện gốc trong danh
  sách.

**Các mẫu cho `Option<Sexpr>`**: kiểu của dữ liệu S-expression không phải `Sexpr` mà là `Option<Sexpr>`, và
danh sách rỗng không phải một variant của `Sexpr` mà là `none` của `Option`. Vì vậy khi so khớp một
`Option<Sexpr>`, 18 variant của `Sexpr` và `none` có thể được viết **phẳng trong cùng một danh sách nhánh**
(không cần một `match` ngoài để bóc `Option`):

```lisp
(defun tag ((s Option<Sexpr>)) i32
  (match s
    ((int _)    1)
    ((cons _ _) 2)
    ((str _)    3)
    ((none)     0)          ; danh sách rỗng
    (_          9)))
```

Tính đầy đủ được kiểm tra trong cùng vũ trụ phẳng đó: 18 variant của `Sexpr` cộng `none`, tổng cộng 19. Quên
`(none)` là lỗi trừ khi có một `_`. `(some x)` cũng có thể viết và gán "một thứ không rỗng".

Đường cú pháp này áp dụng **chính xác** chỉ cho `Option<Sexpr>`. Với `Option<Option<Sexpr>>`, sẽ không rõ
`(int n)` đã bóc lớp nào, nên hãy viết hai tầng `match` như thường.

Các mẫu downcast tương tự có thể dùng nguyên trạng trên **một đối tượng trait object (`:dyn Trait`, chương
2)**: `match` mở hộp nó rồi đưa cho bộ máy mẫu `Sexpr` ở trên, nên không có cú pháp bổ sung. Tập các kiểu triển
khai là mở, nên nó không bao giờ có thể đầy đủ, và cần `_`:

```lisp
(defun area ((d :dyn Drawable)) i32
  (match d
    ((circle r) (* (* r r) 3))     ; phân rã trường với tên kiểu đứng đầu
    ((the square s) (* s::side s::side))
    (_ 0)))
```

**Suy luận kiểu giữa các nhánh**: mọi nhánh phải có cùng kiểu (trừ các nhánh không hội tụ, như với `panic`).
Trong một `match` viết ở nơi không có kiểu mong đợi, các nhánh điền các đối số kiểu còn thiếu cho nhau:
`(result::ok v)` chỉ cố định `T`, và `(result::err e)` chỉ cố định `E`, nhưng cùng nhau chúng cố định
`Result<T,E>`. Một đối số kiểu mà không nhánh nào cố định được đến cuối là lỗi của nhánh đó
(`cannot infer type argument ...`). Ngoài `match`, một đối số kiểu không thể cố định là lỗi ngay tại chỗ.

Việc kiểm tra tính đầy đủ của một `match` dùng mẫu downcast không tính chúng vào phần bao phủ các variant của
chính `Sexpr` (một `match` chỉ liệt kê các mẫu downcast phải được đóng bằng `_`). Với các ADT generic
(`defstruct point<T> ...` và tương tự), các đối số kiểu của một mẫu downcast không thể được suy ra, nên dạng
phân rã trường (`(point ...)`) và dạng tên variant trần không dùng được; hãy nêu chúng bằng `the`, như
`(the point<i32> p)`.

**Downcast cũng nhìn vào việc khởi tạo.** Các đối số kiểu tường minh được dùng để so khớp:
`(the point<i32> p)` chỉ cho qua các giá trị `point<i32>`, và một `point<string>` đi qua để tới nhánh tiếp
theo. Điều này là vì một giá trị nhớ kiểu của nó gồm cả các đối số kiểu (cùng cơ chế chọn `print-object`).

```lisp
(if-let (pattern val) then els)     ; then (có ràng buộc) nếu val khớp pattern, nếu không là els. defmacro
(while-let (pattern val) body...)   ; lặp khi val (đánh giá lại mỗi lần) khớp pattern. defmacro
```

## 5. Vòng lặp

```lisp
(loop body...)                      ; một vòng lặp vô hạn. thoát bằng break/return
(while test body...)                ; lặp khi test là true. defmacro
(until test body...)                ; lặp khi test là false (phủ định của while). defmacro
(dotimes (var count-expr) body...)  ; đánh giá count-expr một lần và chạy var qua 0..count-1. defmacro
(do ((var init step) ...)
    (test result...)
  body...)                          ; vòng lặp kiểu CL với bước nhảy song song. defmacro
(do* ((var init step) ...)
     (test result...)
  body...)                          ; phiên bản tuần tự của do (ràng buộc kiểu let*, gán theo thứ tự). defmacro
(doiter (var coll-expr) body...)    ; lặp qua một giá trị triển khai trait Iter. defmacro

(break)                             ; chỉ thoát vòng lặp trong cùng nhất. giá trị luôn là Unit
(return)                            ; chỉ thoát vòng lặp trong cùng nhất
(return value)                      ; thoát vòng lặp trong cùng nhất với một giá trị
```

Cả `break`/`return` đều thoát **chỉ vòng lặp bao ngoài trong cùng nhất** (chúng không phải thoát sớm khỏi hàm,
và không thể vượt qua ranh giới `lambda`). Kiểu của một `loop` là phép hợp của các kiểu giá trị của các
`break`/`return` tìm thấy bên trong nó (`!` nếu nó không bao giờ được rời đi). Để thoát khỏi một hàm, hãy dùng
`return-from`, bên dưới.

### 5.1 `block` / `return-from` — lối thoát có tên

```lisp
(block name body...)                ; một đích thoát có tên. giá trị là dạng cuối,
                                    ; hoặc giá trị được return-from truyền
(return-from name)                  ; thoát khỏi block đó với Unit
(return-from name value)            ; thoát với một giá trị
```

**Mỗi hàm của `defun` / `defmethod` / `labels` ngầm thiết lập một block mang tên của chính nó** (như trong CL).
Vì vậy `(return-from f v)` là một lần thoát sớm khỏi hàm:

```lisp
(defun first-even ((a i32) (b i32)) i32
  (if (= (mod a 2) 0) (return-from first-even a) ())
  (if (= (mod b 2) 0) (return-from first-even b) ())
  -1)
```

`block` là một lối thoát **từ vựng**, và tên được **phân giải tại nơi nó được viết**: bộ kiểm tra liên kết một
`return-from` với `block` bao ngoài và hợp kiểu của giá trị của nó vào kiểu thoát của block. Do đó:

- Một `return-from` không có `block` khớp là một **lỗi kiểu** (không phải lỗi lúc chạy).
- Một giá trị có kiểu không vừa với các lối thoát khác hoặc kiểu của thân là một **lỗi kiểu** (cùng quy tắc như
  với các nhánh `match`).
- Nếu các block cùng tên lồng nhau, **block bên trong thắng** (quy tắc che khuất của CL).
- **Nó không thể vượt qua ranh giới hàm.** Từ bên trong một `lambda`, bạn không thể thoát ra một `block` bên ngoài
  (`lambda` không thiết lập block: các block ngầm của CL cần một *tên*, và hàm ẩn danh không có). Thứ cần vượt qua
  là `catch`/`throw` (chương 8, vốn là **động**).

Như `break`/`return` (chương 5), nó là một lối thoát **tĩnh**, nên trong mã đã biên dịch nó là một bước nhảy tới
một basic block cố định lúc biên dịch. Nếu ở giữa có một `unwind-protect`, `cleanup` của nó chạy (chương 8).

Nếu bạn không bao giờ viết `return-from`, block ngầm không tốn gì.

### 5.2 `loop` mở rộng (LOOP của CL)

**Nếu phần tử đầu của `loop` là một keyword**, nó được đọc là một dãy các mệnh đề. Nếu không, nó vẫn là vòng lặp
đơn giản ở trên, và ý nghĩa của các `loop` hiện có không đổi (cùng quy tắc vòng lặp đơn giản của chính CL).

CL viết các từ mệnh đề là các symbol trần (`(loop for i from 1 to 3 collect i)`), nhưng ở đây **tất cả chúng là
keyword**: một `for` trần chỉ là một tham chiếu biến, và việc là keyword cũng là điều phân biệt nó với một vòng
lặp đơn giản. Ngoại lệ là `=`, vốn phân tách một biến với một giá trị: vị trí của nó không mơ hồ, nên nó được
đọc hoặc trần hoặc dưới dạng keyword (`:=`).

```lisp
(loop :for i :from 1 :to 3 :collect i)              ; #(1 2 3)
(loop :for x :in (iter v) :when (evenp x) :sum x)
(loop :repeat 4 :for x = 1 :then (* x 2) :collect x) ; #(1 2 4 8)
(loop :for i :from 1 :to 4 :sum i :into s :finally (return (* s 2))) ; 20
```

**Các mệnh đề biến** (viết trước các mệnh đề thân. Đây là quy tắc của CL: viết sau, chúng có thể được đọc là
"chỉ lặp từ đó trở đi", nên đó là lỗi):

| Mệnh đề | Ý nghĩa |
|---|---|
| `:with v = e` | Gán một lần. Có thể đọc các biến của mệnh đề trước |
| `:for v :in s` / `:for v :across s` | Các phần tử của một `Iter` theo thứ tự. Sự phân biệt list/vector của CL không tồn tại ở đây, nên đây là hai cách viết của cùng một mệnh đề |
| `:for v :on s` | Các **hậu tố** liên tiếp. CL truyền đuôi cons dùng chung, nhưng một `Iter` không có đuôi để chia sẻ, nên mỗi cái là một `Vector` mới |
| `:for v :from a [:to b \| :below b \| :downto b \| :above b] [:by s]` | Đếm. `:downfrom`/`:upfrom` cũng dùng được |
| `:for v = e [:then f]` | Bắt đầu với `e`, và từ lần thứ hai dùng `f` (không có `:then`, là `e` mỗi lần) |
| `:repeat n` | Lặp chừng đó lần |

Với nhiều `:for`, chúng tiến **song song**, và vòng lặp kết thúc ngay khi bất kỳ cái nào cạn.

**Các mệnh đề thân** (chạy mỗi lần, theo thứ tự đã viết):

| Mệnh đề | Ý nghĩa |
|---|---|
| `:do form...` | Vì tác dụng phụ |
| `:collect e [:into v]` | Thu thập vào một `Vector<T>` |
| `:append e [:into v]` | Nối nội dung của một `Iter` |
| `:sum e` / `:count e` | Tổng / số lần nó là true |
| `:maximize e` / `:minimize e` | Lớn nhất / nhỏ nhất. **`Option<T>`** (như CL trả về nil cho một dãy rỗng; một kiểu `Ord` bất kỳ không có phần tử nhỏ nhất) |
| `:always e` / `:never e` | `true` nếu tất cả đều đúng; `false` ngay khi một cái thất bại |
| `:thereis e` | `e` là một **`Option<T>`**. Trả về `some` đầu tiên, hoặc `none` nếu không có (đây là thứ tương ứng với "giá trị khác nil đầu tiên" của CL; để kiểm tra một `bool`, hãy dùng `:always`/`:never`) |
| `:while e` / `:until e` | **Kết thúc bình thường** ở đây (`:finally` chạy, và thứ đã thu thập là đáp án) |
| `:when e clause` / `:unless e clause` / `:if e clause [:else clause]` | Làm cho một mệnh đề có điều kiện |
| `:return e` | Thoát ngay với giá trị đó (`:finally` không chạy, như trong CL) |
| `:initially form...` / `:finally form...` | Trước vòng lặp / khi hoàn tất bình thường |

**`:named name`** (trước mọi mệnh đề khác, chỉ một lần) bọc cả vòng lặp trong `(block name …)`.
`(return-from name e)` có thể thoát ngay ngay cả từ bên trong các vòng lặp lồng nhau, và như `:return`,
`:finally` không chạy. Không có tên, không có block nào được thiết lập: `loop` không tên của CL thiết lập
`block nil`, nhưng ở đây không có `nil`, và `break`/`return` (chương 5) đã cung cấp "thoát vòng lặp trong cùng
nhất".

```lisp
(loop :named outer :for i :from 1 :to 3
  :do (loop :for j :from 1 :to 3 :do (if (= (* i j) 4) (return-from outer (* 100 i)) ()))
  :finally (return 0))                                  ; 200
```

Bỏ `:finally (return 0)` là một **lỗi kiểu**. Đó chỉ là các quy tắc của `block` đang tác động (5.1): kiểu của lối
thoát `int` không vừa với `()` mà vòng lặp để lại khi nó cạn.

**Giá trị của vòng lặp** là sự tích lũy của mệnh đề tích lũy nếu có (cái đầu tiên, nếu có nhiều), `true` với
`:always`/`:never`, `none` với `:thereis`, và `()` nếu không có. Nếu thứ cuối trong `:finally` là `(return e)`,
đó là giá trị: thành ngữ `finally (return …)` của CL, cách duy nhất để một vòng lặp không tích lũy nêu tên đáp
án của chính nó.

**Khác biệt so với CL / những gì không có**:

- **Các từ mệnh đề là keyword** (ở trên).
- `:maximize`/`:minimize`/`:thereis` trả về `Option<T>` (không có nil).
- **Chỉ viết `:return`, mà không có tích lũy cũng không có `:finally`, là lỗi.** CL trả về nil khi cạn, nhưng ở
  đây không có thứ đó, nên vòng lặp phải nói giá trị của nó là gì khi cạn.
- Việc nối các mệnh đề song song bằng `:and`, `:being`/lặp chuyên dụng trên bảng băm, `:it` và `:nconc` không
  được bao gồm.
- Kiểu phần tử của `:collect` đến từ kiểu của biểu thức được tích lũy. Cố thu thập một kiểu **không thể viết
  thành tên kiểu**, như kiểu hàm, là một lỗi nói rõ như vậy.

## 6. Giá trị hàm và lời gọi

```lisp
(lambda (params) RetType body...)   ; tạo một giá trị hàm hạng nhất (một closure)
(labels ((name (params) RetType body...) ...) body...)   ; các định nghĩa hàm cục bộ có thể đệ quy lẫn nhau
(apply f arg1 ... argN rest-list)   ; gọi f (một hàm có đối số biến đổi với &rest), trải rest-list ra
```

Các hàm có tên cũng có thể được truyền như giá trị nguyên trạng (làm đối số cho các hàm bậc cao, v.v.).

## 7. Các dạng đặc biệt khác

```lisp
(setq var value ...)                ; phép gán biến của CL. chỉ là một dãy (setf var value). defmacro
(psetq var value ...)               ; gán song song. đánh giá mọi giá trị trước, rồi gán. defmacro
(psetf place value ...)             ; psetq được khái quát hóa cho các place (cùng khai triển). defmacro
(setf place value)                  ; gán cho một place. một place là một tên biến / var::field /
                                     ; một lời gọi dạng (accessor recv key...). hợp lệ nếu
                                     ; kiểu tĩnh của recv có một phương thức thể hiện tên là
                                     ; set-{accessor} (với get của Vector<T> và HashTable<K,V>,
                                     ; set tương ứng là ngoại lệ; nếu không là set-accessor-name).
                                     ; giá trị là giá trị đã gán (như trong CL). vì vậy
                                     ; trong (if c (setf x 1) ()), then và else không có kiểu khớp nhau
(incf place)  (incf place delta)    ; place += delta (delta=1 nếu bỏ qua). kết quả như với setf
(decf place)  (decf place delta)    ; place -= delta (delta=1 nếu bỏ qua)
(rotatef place1 place2 ... placeN)  ; xoay N place (place1 mới=place2 cũ, ...,
                                     ; placeN mới=place1 cũ). các dạng con của mỗi place được đánh giá một lần
(shiftf place1 ... placeN newvalue) ; dịch trái các giá trị của place2..N và đặt newvalue vào placeN.
                                     ; giá trị trả về là giá trị cũ của place1
(list e1 e2 ... en)                 ; khai triển thành (cons e1 (cons e2 (... ()))). () khi không có đối số.
                                     ; mỗi phần tử được chuyển ngầm thành Sexpr (như cons của CL, nó
                                     ; có thể giữ mọi giá trị). các giá trị vô hướng (int/i32/f64/ratio/char/bool/string/
                                     ; symbol) được bọc vào variant Sexpr tương ứng, còn defstruct/
                                     ; defenum/Vector<T>/HashTable<K,V> và tương tự đi vào nguyên trạng
                                     ; (không tốn chi phí chuyển đổi). tương tự với các đối số &rest/format.
(source-file)                       ; tên tệp mà dạng này được đọc từ đó (chuỗi). được cố định như một
                                     ; hằng số lúc kiểm tra. tương ứng với *load-pathname* của CL, nhưng không
                                     ; phải một biến: thân module chạy sau khi kiểm tra, nên "đang nạp"
                                     ; không thể dựa vào, còn lúc kiểm tra thì luôn biết.
                                     ; với các nguồn không phải tệp, là tên mà reader đặt cho chúng (<stdin>/<input>)
(quote datum)                       ; giống 'datum. trả về nó như dữ liệu Sexpr mà không đánh giá
(quasiquote template)               ; giống `template. nhúng các biểu thức vào mẫu bằng ,/,@
(documentation name)                ; trả về docstring của name (một tên trần hoặc Type::method) dưới dạng Option<string>
(panic message)                     ; message: string. kết thúc bất thường với một lỗi không thể khôi phục. kiểu !
(unreachable)                       ; khai triển thành (panic "unreachable"). defmacro
(todo)                              ; khai triển thành (panic "todo"). defmacro
(as Type expr)                      ; chuyển đổi kiểu số/ký tự. các chuyển đổi có thể thất bại thì panic khi thất bại
(try-as Type expr)                  ; như as, nhưng trả về kết quả dưới dạng Option<Type> (None khi thất bại)
(print control args...)             ; khai triển định dạng và ghi ra đầu ra chuẩn (không có dòng mới)
(println control args...)           ; giống vậy (với một dòng mới ở cuối)
(format dest control args...)       ; format của CL. trả về chuỗi đã khai triển
(pprint x)                          ; in đẹp. ghi một dòng mới trước, như trong CL
(pprint-fill x)                     ; bố cục lấp đầy
(pprint-linear x)                   ; tất cả trên một dòng hoặc mỗi phần tử một dòng
(pprint-tabular x [colinc])         ; bố cục dạng bảng (mặc định 16 cột)
(pprint-logical-block (obj :prefix p :suffix s) body...)  ; tự dựng một khối logic
```

Họ `print`/`println`/`format`/`pprint` là các dạng đặc biệt, nên các đối số biến đổi của chúng (một đối tượng
đơn với họ `pprint`) được bọc vào `Sexpr` cùng kiểu của chính chúng trước khi truyền: đó là lý do
`(println "~a" my-struct)` chạy được ngay. Chi tiết về các chỉ thị định dạng và pretty printer nằm ở
[Các chỉ thị định dạng](functions/format.md) và [In](functions/printing.md#4-pretty-printer).

`as`/`try-as` chỉ xử lý danh mục số và ký tự (giữa `int`, các kiểu số nguyên độ rộng cố định,
`f32`/`f64`/`ratio`/`char`). Cùng một kiểu thì không phải chuyển đổi. **Các chuyển đổi giữa các độ rộng số
nguyên (gồm `int`) và giữa `f32`↔`f64` là các chuyển đổi thật**: `as` cắt cụt / làm tròn, và `try-as` trả lời
liệu nó có vừa độ rộng (độ chính xác) đó không. `(as int x)` là phép mở rộng chính xác từ một độ rộng cố định,
và `(as i32 n)` là phép cắt cụt từ `int`. Số nguyên → `char` có thể thất bại khi ngoài phạm vi, nên `as` panic và
`try-as` cho `None`. Mọi thứ khác (mở rộng, và phép cắt cụt của `float->int`/`ratio->int`) luôn thành công.
`float->int`/`ratio->int`/`char->int` hạ xuống `int`, và nếu cần độ rộng hẹp hơn, `int->W` được gọi sau chúng.
Đây là đường cú pháp khai triển thành các phương thức chuyển đổi tương ứng (`int->char`/`int->int`/`int->W`, v.v.
trong [Số](functions/numbers.md)).

`documentation`, như `quote`/`compile`, là một dạng đặc biệt đọc `name` mà không đánh giá, như một symbol trần /
đường dẫn `::` không được đánh giá. Khác `(documentation 'name 'function)` của CL, nó không nhận đối số kiểu: nó
phân giải `name` theo thứ tự biến → hàm → kiểu → trait → macro (cùng độ ưu tiên như khi một định danh trần được
đánh giá như một biểu thức) và trả về docstring của định nghĩa tìm được (`(documentation Type::method)` dành cho
phương thức). Không phân giải được (không có định nghĩa nào mang tên đó) là lỗi lúc kiểm tra; một định nghĩa
tồn tại nhưng không có docstring cho `Option::none`. Mọi thứ được quyết định như một hằng số lúc kiểm tra:
không có tra cứu lúc chạy. Các tên tự do có tiền tố module (`mod::name`, trừ `Type::method`) không được hỗ trợ.

## 8. Lối thoát phi cục bộ (catch / throw / unwind-protect)

```lisp
(catch 'tag body)                   ; chạy body. nếu (throw 'tag v) xảy ra ở bất kỳ đâu
                                    ; body với tới, v đó trở thành giá trị
(throw 'tag value)                  ; thoát tới (catch 'tag ...) bao ngoài gần nhất theo nghĩa động
(unwind-protect protected cleanup)  ; chạy cleanup dù protected được rời đi bằng cách nào
```

Khác `break`/`return` (chương 5), đây là một lối thoát **động**: `throw` không tìm `catch` bao quanh nó theo
nghĩa từ vựng, và với tới một `catch` cùng tag xuyên qua bất kỳ số lời gọi hàm nào.

```lisp
(defun find-first ((xs Option<Sexpr>)) int
  (catch 'found
    (progn
      (dolist (x xs)
        (match x ((int n) (if (> n 10) (throw 'found n) ())) (_ ())))
      -1)))                         ; nếu không tìm thấy, giá trị ở cuối như thường lệ
```

- **Tag chỉ là các symbol literal** (`'done`). Khác CL, chúng không được đánh giá.
- **Một tag mang một kiểu.** Kiểu được quyết định lần đầu `'tag` được dùng, và mọi `throw`/`catch` sau đó của
  cùng symbol đều được kiểm tra theo nó. Dùng nó với kiểu khác là lỗi kiểu.
- Kiểu của `throw` là `!` (nó không hội tụ). Kiểu của `(catch 'tag expr)` là phép hợp của kiểu của `expr` và kiểu
  của tag.
- Giá trị của `unwind-protect` là giá trị của `protected`. Giá trị của `cleanup` bị bỏ đi. `cleanup` chạy dù
  `protected` được rời đi bằng cách nào: ngoài hoàn tất bình thường, `throw` và `panic`, nó cũng chạy khi được
  rời đi bằng `break`/`return`/`return-from`. Một lối thoát phi cục bộ do chính `cleanup` thực hiện thắng lối
  thoát đang diễn ra.
- Các `unwind-protect` lồng nhau chạy từ trong ra ngoài. Một `break` rời một vòng lặp **bên trong** `protected`
  chưa rời `protected`, nên `cleanup` của nó không chạy.

Các condition của CL (`define-condition`/`handler-bind`/`invoke-restart`) không được áp dụng. Chúng không hợp
với kiểu tĩnh, nên các thất bại có thể khôi phục được biểu diễn bằng `Result` (chương 9).

## 9. Chính sách xử lý lỗi

- Các thất bại có thể khôi phục: `Result<T,E>` + `match`. Các thất bại không thể khôi phục (bug, bất biến bị
  phá vỡ): `panic`.
- Không có cú pháp tương ứng với `?`/try. Các nhánh được viết tường minh bằng `match`.
- Tên hàm và dạng đặc biệt không dùng `!` (thao tác phá hủy) hay `?` (vị từ) làm hậu tố. Vị từ được đặt tên bằng
  hậu tố `-p`/`p` (`zerop`, `consp`, v.v.) hoặc tiền tố `is-` (`is-some`, `is-ok`, v.v.).

## 10. Biên dịch

```lisp
(compile name)                      ; biên dịch JIT một defun/phương thức đã định nghĩa thành mã native
(compile-file src-path out-path)    ; biên dịch AOT một tệp nguồn thành một tệp thực thi native (bỏ qua `(main)` cuối cùng)
(dump path)                         ; ghi môi trường hiện tại (thông tin kiểu + các thân đã biên dịch) vào một tệp
(disassemble name)                  ; in ra thứ mà định nghĩa đó trở thành (mặc định là mã máy của máy chủ, LLVM IR với true làm đối số thứ hai)
```

`compile` là một dạng đặc biệt; `name` không được đánh giá và được đọc như một symbol trần / đường dẫn `::` không
được đánh giá (một chuỗi là lỗi kiểu). Các hàm generic không thể là đích: một bản sao cho từng kiểu được tạo ở
mỗi nơi dùng, nên không có thân đã biên dịch đơn lẻ nào. **Một tên không phân giải được là lỗi lúc kiểm tra**
và không bao giờ được đưa sang lúc chạy (có các thông báo riêng cho: kiểu tồn tại nhưng không có phương thức đó /
cả kiểu lẫn hàm đều không tồn tại / một tên trần chưa định nghĩa). Khả năng hiển thị ở đây được đối xử như với mọi
tham chiếu khác: "tồn tại nhưng không nhìn thấy được từ đây" thất bại lúc kiểm tra, y như "không phân giải được".

Các hàm được gọi cũng được biên dịch theo bắc cầu, nên **một hàm (dù gián tiếp) gọi thứ không thể biên dịch thì
không thể biên dịch**. Tiến trình không crash; nó bị từ chối với một lỗi nói rõ như vậy. Mọi hàm dựng sẵn đều có
thể biên dịch, nên các hàm duy nhất bị từ chối theo cách này là những hàm gọi các thao tác chỉ dành cho trình
thông dịch sau:

```lisp
(defun g () int 1)
(defun f () () (progn (trace g) ()))
(compile f)
; => trace: `(trace ...)` is an interpreter-only action and cannot itself be compiled
```

Những thao tác chỉ dành cho trình thông dịch là `compile`/`compile-file`/`dump` và
`trace`/`untrace`/`step`/`disassemble`
([Công cụ triển khai](functions/system.md#5-công-cụ-triển-khai-clhs-252)). Thay vì là những thứ không thể biên
dịch, đây là các thao tác của phía biên dịch (thứ `dump` ghi ra là chính môi trường của trình thông dịch, thứ mà
một tệp thực thi AOT không có; thứ `trace` theo dõi và nơi `step` dừng là các đường gọi của trình thông dịch đang
chạy; và `disassemble` dùng chính trình biên dịch). `room`/`dribble`/`ed` không nằm trong số đó và có thể biên
dịch bình thường.

Những gì **có thể** biên dịch: vào/ra stream và tệp, `random`, `gensym`, `symbol->string`/`string->symbol`,
`parse-int`/`parse-float`, `get-universal-time`/`get-internal-real-time`, `exit`, các hàm siêu việt, phép toán
bit, `catch`/`throw`/`unwind-protect`, cả bốn `eq`/`eql`/`equal`/`equalp` (cho phép `case` biên dịch được với mọi
kiểu), toàn bộ họ in gồm `print`/`println`/`format`/`pprint` và `pprint-logical-block`, `read`, và `eval`. Thư
viện chuẩn được phát hành ở dạng đã biên dịch sẵn.

Một tệp thực thi AOT chỉ chứa các tính năng mà chương trình dùng. Một chương trình không in thì không có engine
định dạng, một chương trình không gọi `read` thì không có reader, và một chương trình không gọi `eval` thì không
có bộ kiểm tra cũng như trình thông dịch.

Từ dòng lệnh, `typl -c src-path [-o out-path]` (`-c` cũng có thể viết là `--compile`) làm giống `compile-file`.
Không có `-o`, đầu ra là `src-path` bỏ phần mở rộng `.typl`. Theo mặc định, thư viện tĩnh
`libtypelisp_front.a` được liên kết vào các tệp thực thi là, với bản build release của `typl`, thư viện mà `typl`
mang bên trong nó, được ghi ra ở lần liên kết đầu tiên vào `$TYPELISP_HOME/lib/<build ID>/` (hoặc
`~/.typelisp/lib/<build ID>/` khi không có `TYPELISP_HOME`) và được dùng từ đó; với bản build debug, là thư viện
nơi `typl` được build. `typl --remove-lib` xóa những gì `typl` đó đã ghi ra. Với `--others`, nó xóa các thư viện
của các build ID khác; với `--all`, của mọi build ID. Với `typl --lib-dir DIR`, thư viện trong `DIR` được dùng
(cho cả `-c` lẫn `compile-file`), và nếu nó không có ở đó, là một lỗi lúc khởi động.

### 10.1 Dump

```lisp
(dump "session.typld")     ; ghi một dump ra
```
```sh
typl --image session.typld prog.typl   # khởi động từ nó
typl --image session.typld             # REPL cũng vậy
```

Một dump giữ thông tin kiểu và các thân đã biên dịch trong một tệp. Thứ `(dump path)` ghi ra là những gì phiên
hiện tại đã nạp (thư viện chuẩn, hoặc một dump truyền bằng `--image`) cộng với **những gì chính phiên đã định
nghĩa**. Vì vậy đầu ra là tự đầy đủ, và `typl --image` dựng lên cùng một môi trường. Những gì phiên đã
`(compile f)` được ghi ở dạng đã biên dịch.

Những gì được lưu là **các định nghĩa, không phải lịch sử**:

- Các biểu thức cấp cao nhất của phiên (`(println ...)`, v.v.) không được bao gồm. Sẽ là vấn đề nếu việc nạp chạy
  lại chúng.
- Biến toàn cục quay lại với **giá trị của biểu thức khởi tạo được chạy lại**, không phải giá trị lúc dump. Đây
  là một khác biệt có chủ ý so với `save-lisp-and-die` của SBCL (vốn ghi heap ra nguyên trạng), và lựa chọn này
  làm biến mất cả một họ vấn đề: "các giá trị không thể lưu", như stream đang mở, con trỏ hàm của closure và bộ
  nhớ ngoài.
- Khác `save-lisp-and-die`, **tiến trình không chết**, vì việc ghi không làm hỏng image.

Một dump ghi lại phiên bản của thư viện chuẩn và trình biên dịch của triển khai đã ghi ra nó. Nạp nó bằng một
`typl` phiên bản khác là một lỗi; nó không bao giờ được chấp nhận một cách im lặng.

### 10.2 `eval` trong các tệp thực thi AOT

`eval` kiểm tra kiểu theo "môi trường toàn cục hiện tại" rồi đánh giá
([Phân tích và đánh giá](functions/system.md#6-phân-tích-và-đánh-giá)). Môi trường đó (các bảng chữ ký, kiểu và
macro mà bộ kiểm tra tham khảo, và các thân mà trình thông dịch chạy được) **không nằm trong mã máy**. Một hàm đã
biên dịch không là gì ngoài một ký hiệu đặt tại một địa chỉ; nó không có kiểu đối số của mình cũng như bảng để
tra thân theo tên.

Vì vậy, chỉ với các chương trình gọi `eval`, `compile-file` **dựng môi trường đó lúc biên dịch và ghi nó vào tệp
thực thi**. Định dạng giống một dump, chứa phần của thư viện chuẩn và phần của chính chương trình. Tất cả những
gì xảy ra lúc khởi động là khôi phục nó: mã nguồn không được đọc lại, và không có gì được kiểm tra kiểu lại. Không
có gì được thêm vào các chương trình không gọi `eval`.

Hệ quả:

- **Khởi động lâu hơn và tệp thực thi lớn hơn**, vì mã của bộ kiểm tra và trình thông dịch cùng một ảnh chụp
  của môi trường được đưa vào. Heap cũng được làm lớn hơn đôi chút.
- **Các dạng truyền cho eval được thông dịch.** Ngay cả khi dạng được truyền cho eval gọi các hàm của chính
  chương trình, thứ chạy là thân có thể thông dịch mà ảnh chụp giữ. Kết quả như nhau; chỉ tốc độ khác.

Việc lưu trữ biến toàn cục được **dùng chung** với mã đã biên dịch (cùng các ô). Một biểu thức khởi tạo
`defvar` được chạy một lần bởi phần khởi tạo đã biên dịch, và việc khôi phục bỏ qua nó, nên một biểu thức
khởi tạo có tác dụng phụ không chạy hai lần.

`compile-file` cũng đọc thư viện chuẩn (và nhúng các thân của nó vào tệp thực thi), nên các hàm của thư viện
chuẩn như `abs`/`gcd`, và `(impl print-object ...)` cũng như `(defmethod print-object ...)`, có thể dùng với
AOT.

`compile-file` cũng chấp nhận `use` (và `import`/`shadowing-import`). `(use m)` của tệp đầu vào tìm tệp theo
cùng quy tắc như `typl file.typl`, và các tệp phụ thuộc tìm được cũng được biên dịch và liên kết vào tệp thực
thi: một bố cục trong đó `main.typl` đọc `http.typl` qua `(use http)` có thể được biên dịch AOT nguyên trạng.
Các định nghĩa của chính tệp đầu vào cũng đi vào module mang tên của tệp, như với `typl file.typl` (`point`
trong `p.typl` là `p::point`). Vì vậy biểu diễn in của các giá trị (`#<p::point x: 1 y: 2>`) giống nhau dù chạy
theo cách nào.

## 11. Reader macro (readtable)

Điều mà reader **làm khi gặp một ký tự nhất định** có thể được thay thế từ chương trình (CLHS 23.1).

```lisp
(set-macro-character c f)             ; f đọc ký tự c
(get-macro-character c)               ; Option<f>
(set-dispatch-macro-character d s f)  ; f đọc chuỗi hai ký tự d s
(get-dispatch-macro-character d s)    ; Option<f>
```

Kiểu của `f` là `(fn (string-input-stream char) Option<Sexpr>)`. Đối số đầu tiên là **một stream trên phần văn
bản chưa đọc**, và đối số thứ hai là **ký tự đã kích hoạt nó** (ký tự thứ hai với một dispatch). Giá trị trả về
trở thành dữ liệu được đọc tại điểm đó. Stream là một kiểu cụ thể thay vì `:dyn PeekInput` vì reader luôn truyền
đúng loại này: `read-sexpr` / `read-char` / `peek-char` / `unread-char` / `read-delimited-list` đều nhận
`(where (PeekInput S))`, nên tất cả đều hoạt động trên kiểu cụ thể nguyên trạng.

```lisp
(set-macro-character #\!
  (lambda ((s string-input-stream) (c char)) Option<Sexpr>
    (match (read-sexpr s)
      ((ok o) (match o
                ((datum d) (sexpr-cons (quote not) (sexpr-cons d (quote ()))))
                ((eof) (quote ()))))
      ((err e) (quote ())))))

!(equal 1 2)   ; => được đọc là (not (equal 1 2)), tức là true
```

Reader **nhìn các ký tự macro trước cú pháp dựng sẵn**, nên nó cũng có thể chiếm `(` và `'`. Các ký tự con của `#`
đăng ký theo cách này được ưu tiên hơn `#b`/`#x`/`#.` dựng sẵn. Một ký tự khác `#` trở thành ký tự dispatch ngay
tại chỗ khi được truyền cho `set-dispatch-macro-character`: **không có** thứ tương ứng với
`make-dispatch-macro-character` của CL. Việc đăng ký đã làm đúng việc của nó, nên một bước riêng sẽ không có gì
để làm.

**Khi nào chúng có hiệu lực** phụ thuộc vào đường đọc, giống như với `#.` (chương 1):

- REPL và `(load ...)` chạy từng dạng một, nên **các hàm được định nghĩa ở các dạng trước** có thể được đăng ký
  nguyên trạng.
- Các tệp module được kiểm tra như một đơn vị và chạy sau, nên **chỉ các lời gọi
  `set-macro-character` / `set-dispatch-macro-character` được chạy ngay** (vai trò của
  `(eval-when (:compile-toplevel) ...)` của CL). Vì chúng chạy ngay, **hàm được truyền phải đã tồn tại tại điểm
  đó**. Một `defun` trong cùng tệp chưa chạy, nên hãy viết một `lambda`, hoặc dùng thư viện chuẩn hoặc thứ gì đó
  đã chạy. Chỉ các lời gọi cấp cao nhất được bao phủ; nó không nhìn vào bên trong `progn` hay `let`.

`read` / `read-from-string` dựng sẵn cũng tham khảo readtable (như trong CL).

**Những gì không có**: `*readtable*` và `copy-readtable`, và `readtable-case`. Hai cái đầu vì một readtable
**không phải một giá trị**: một giá trị phải là "thứ có thể đưa cho một reader", nhưng reader đọc mã nguồn nằm
ngoài chương trình, không có nơi nào để đưa. `readtable-case` vì chương 1 quyết định rằng reader của ngôn ngữ này
luôn chuyển thành chữ thường (`:downcase` của CL).


## 12. Lập trình đồng thời (task)

**Một task là một thread nhẹ** (theo thuật ngữ Go, là thứ một câu lệnh `go` khởi động) và chạy theo kiểu hợp
tác (không có preemption). Việc chuyển đổi không đi qua kernel, và trạng thái thực thi nằm trên heap thay vì
trên một stack máy, nên task rẻ để tạo với số lượng lớn.

**Các task chạy đồng thời trên nhiều thread HĐH** (song song đa lõi). Số thread là biến môi trường
`TYPELISP_THREADS` (tổng số, gồm cả thread chạy `main`; mặc định là mức song song của máy). Trong `typl`,
**chỉ các task đã biên dịch** chạy trên các thread khác, và các task được thông dịch chạy trên thread của trình
thông dịch (12.7). Dữ liệu dùng chung đi qua `Mutex<T>` hoặc `Chan<T>`; việc đọc và ghi đồng thời không qua
chúng là không xác định, như trong Go (12.7).

Trong số từ vựng, **chỉ `task` / `thread` / `select` là các dạng đặc biệt**; phần còn lại là các hàm, phương
thức và macro thông thường ([Task và kênh](functions/concurrency.md)).

### 12.1 `task` — khởi động một task

```lisp
(task (f arg...))                   ; trả về Task<T>, trong đó T là kiểu trả về của f
```

**Nó chỉ nhận dạng của một lời gọi.** `f` và từng `arg` được đánh giá tại nơi viết `task`, theo thứ tự đã viết,
và chỉ **lời gọi** xảy ra trong task mới. Đây là cùng quy tắc như `go f(x)` của Go, và cũng là lý do nó nhận một
dạng lời gọi thay vì một thunk: một thunk sẽ bắt giữ các đối số của nó mà không đánh giá chúng.

```lisp
(dotimes (i 10)
  (task (worker i ch)))             ; i được đánh giá ngay tại chỗ mỗi lần; không có bẫy bắt giữ

(task ((lambda () ()                ; để chạy một thân bất kỳ, hãy gọi một lambda
         (println "start")
         (send ch 1))))
```

Các dạng đặc biệt (`if` / `let` / `progn` …) không thể viết trực tiếp dưới `task`.

**Vì sao nó không thể là một hàm**: viết `(spawn (lambda () T body...))` sẽ buộc phải viết ra `T`, vì `lambda`
yêu cầu một chú thích kiểu trả về, và một macro không biết kiểu trả về của `(f a b)`. Chỉ bộ kiểm tra biết nó.

### 12.2 `thread` — khởi động một task trên một thread HĐH riêng

```lisp
(thread (f arg...))                 ; trả về Thread<T>, trong đó T là kiểu trả về của f
(join th)                           ; chờ hoàn tất và trả về giá trị của nó (bao nhiêu lần cũng được)
```

Dạng và quy tắc đánh giá giống `task` (nó chỉ nhận một dạng lời gọi, và `f` và `arg` được đánh giá tại nơi viết
nó). Khác biệt là nơi nó chạy: **nó khởi động một thread HĐH dành riêng cho task đó và chỉ chạy trên đó**. Nó
không được ghép kênh với các task khác, nên gọi một hàm C chặn (`defffi`) bên trong chỉ dừng thread đó, và các
task khác tiếp tục tiến triển. Bên trong nó, `task`, `send`, `recv` và các hàm còn lại có thể dùng nguyên trạng.

- `Thread<T>` là đối ứng của `Task<T>`. Giống `wait`, `join` dừng **task đang gọi**, và giá trị được lưu đệm.
  Khi task kết thúc, thread cũng kết thúc.
- Quy tắc panic giống như với `task` (cả tiến trình sụp). Khi `main` trả về, tiến trình kết thúc.
- Để viết nó dưới dạng một hàm, hãy dùng `(Thread::spawn (lambda () T body...))` (`std::thread::spawn` của
  Rust). Một hàm có tên cũng có thể được truyền.
- **Chỉ mã đã biên dịch mới chạy trên một thread riêng.** Khi `typl` đánh giá `(thread (f ...))` hoặc
  `Thread::spawn` trong lúc thông dịch, nó biên dịch hàm cần chạy (và những gì nó gọi) ngay tại chỗ trước khi
  chạy. Những gì không thể biên dịch (một `lambda` tham chiếu tới biến cục bộ bên ngoài nó, việc dựng một struct,
  v.v.) là, trước khi thread được khởi động, một panic được đối xử giống như một `(panic ...)`. Một `lambda`
  tham chiếu tới biến cục bộ có thể được truyền nếu nó được tạo bên trong một hàm đã biên dịch.

### 12.3 `select` — chờ nhiều thao tác kênh cùng lúc

```lisp
(select
  ((v (recv ch1)) body...)          ; một nhánh nhận. v được gán là một Option<T>
  ((send ch2 x) body...)            ; một nhánh gửi
  (else body...))                   ; tùy chọn. **nếu viết, nó đứng cuối**
```

- **Với `else`, nó không chặn** (`default` của Go). Không có nó, nó chờ cho đến khi một nhánh trở nên khả thi.
- **Nếu nhiều nhánh khả thi cùng lúc, một nhánh được chọn ngẫu nhiên** (theo thứ tự đã viết, các nhánh sau sẽ
  bị bỏ đói).
- `v` của một nhánh nhận là một **`Option<T>`**. Một kênh đã đóng là "một đáp án", không phải lý do để bỏ qua
  nhánh, nên hãy `match` nó bên trong nhánh.
- Kiểu là **phép hợp của các kiểu của thân mọi nhánh** (cùng quy tắc như các nhánh `match`).
- `(select)` với không nhánh nào là lỗi kiểu (`select{}` của Go, chặn mãi mãi, không được áp dụng). Một
  `select` chỉ có `else` cũng vậy, vì nó giống việc viết thân trực tiếp.

**Các biểu thức kênh và các giá trị cần gửi được đánh giá mỗi cái một lần, từ trái sang phải, dù nhánh nào được
chọn** (cùng kỷ luật mà `case` có cho các khóa của nó).

```lisp
(select                             ; nhận với một timeout
  ((v (recv ch))          (println "~a" (unwrap v)))
  ((z (recv (after 0.5))) (println "timeout")))
```

`after` ([một kênh phát giá trị sau một khoảng thời gian](functions/concurrency.md#5-after--kênh-phát-giá-trị-sau-một-khoảng-thời-gian))
là "một kênh phát một giá trị sau `sec` giây", tương ứng với `time.After` của Go.

### 12.4 Tương tác với các tính năng khác

| Tính năng | Quan hệ với task |
|---|---|
| `catch` / `throw` | **Không vượt qua ranh giới task.** Một `throw` cố rời thân của một task là một panic |
| `unwind-protect` | Phần dọn dẹp chạy khi một task kết thúc tự nhiên. **Nó không chạy khi tiến trình kết thúc vì task chính kết thúc** |
| `block` / `return-from` | Từ vựng, nên chúng không vượt qua ranh giới `lambda` |
| `panic` | Như trong Go, cả tiến trình sụp. `wait` không quan sát một panic như một giá trị |
| `dlet` | **Không phải ràng buộc theo từng task.** Nó vẫn "mượn và trả một biến toàn cục", nên các task can thiệp lẫn nhau |
| Đầu ra chuẩn | Dùng chung bởi mọi task. Đầu ra của một `println` không bao giờ bị trộn với đầu ra khác ở giữa một dòng |
| `compile` / `eval` | Không có hạn chế. `(compile f)` bên trong một task hoạt động |

### 12.5 Nơi các task chuyển đổi

Việc lập lịch là hợp tác, nên **các task chỉ chuyển đổi ở nơi bạn viết ra**: `(yield)`, `(sleep ...)`,
`(wait ...)`, **các thao tác kênh phải chờ** (`send`/`recv`/`select`), và **các thao tác socket phải chờ**
(`accept` / `tcp-connect` (gồm cả phân giải tên) / đọc và ghi socket / `recv-from`;
[Mạng](functions/network.md)). Mọi socket đều không chặn: nếu một socket chưa sẵn sàng, chỉ task đó dừng, và nó
tiếp tục khi HĐH báo sẵn sàng, cùng dạng với netpoller của Go. Chỉ khi không có task nào chạy được, triển khai
mới chờ HĐH đến hạn `sleep` gần nhất.

Các thao tác kênh có thể trả lời ngay tại chỗ (một `send` còn chỗ trong bộ đệm, một `recv` có giá trị đang chờ,
`(len ch)`/`(cap ch)`/`(close ch)`/`(Chan::new n)`) **không tiêu hao lượt**. Điều này nghĩa là bạn không bị ngắt
đột ngột bởi một lần đọc, và nó được đối xử khác với `(sleep 0.0)`, vốn là "nhường trong 0 giây" của CL.

**Không có preemption.** Một vòng lặp chặt không gọi gì cả sẽ bỏ đói các task khác. Tuy nhiên, các vòng lặp đã
biên dịch định kỳ trao quyền điều khiển cho bộ lập lịch, nên một vòng lặp chặt đã biên dịch không bỏ đói chúng.

### 12.6 Mã đã biên dịch và task

Mã đã biên dịch cũng có thể tạm dừng các task. Điều tương tự áp dụng cho các tệp thực thi tạo bằng
`compile-file`: `main` chạy như task chính của bộ lập lịch, và `task`, `sleep`, `wait`, kênh và các lần chờ
socket đều hoạt động với cùng ý nghĩa như trong `typl`. Khi `main` trả về, tiến trình kết thúc và các task còn
lại bị cắt ngang (như trong Go). Trình thông dịch không bao giờ được đưa vào tệp thực thi vì lợi ích của bộ lập
lịch.

Ngoại lệ duy nhất là "bên trong một callback C FFI", nơi các thao tác **phải chờ** là lỗi (thân thiện hơn việc
deadlock một cách im lặng): trong khi một hàm được truyền bằng `defffi` đang được C gọi, stack của C nằm ở trên
cùng, và không có cách nào tạm dừng task và tiếp tục nó sau.

Những nơi sau cũng là các hàm được gọi giữa chừng một task, nhưng không thể tạm dừng: các phương thức
`print-object`, `~/name/` trong `format`, reader macro, bên trong `eval`, và các biểu thức khởi tạo `defvar` trong
các tệp thực thi AOT. Ở đây, **các thao tác trả lời mà không phải chờ được cho qua** (`(recv ch)` với một giá trị
trong bộ đệm, `read-line` trên một socket đã nhận sẵn dữ liệu, `(task ...)`, `(yield)`, v.v.), và **các thao tác
thực sự phải chờ là lỗi** (không dừng tiến trình ngay tại chỗ, mà là một panic như
`` `recv` cannot block: ... ``, được đối xử giống một `(panic ...)`).

### 12.7 Khác biệt so với Go

- **Trong `typl`, chỉ các task đã biên dịch mới ra các thread khác.** Trạng thái của trình thông dịch không thể
  chia sẻ giữa các thread, nên các task từ một `task` được thông dịch chạy trên thread của trình thông dịch. Một
  task đã biên dịch cũng **chuyển sang thread của trình thông dịch và ở lại đó** (nó không quay lại) tại điểm
  nó gọi một giá trị hàm được thông dịch, gọi một phương thức `:dyn` chưa ai biên dịch, hoặc gọi
  `eval`/`macroexpand`/`read`. Nếu một phép tính dài chạm vào mã được thông dịch dù chỉ một lần giữa chừng, phần
  còn lại chạy trên thread của trình thông dịch.
- **Trong `typl`, các worker chỉ sống trong một lần đánh giá cấp cao nhất.** Trong lúc REPL chờ đầu vào, và giữa
  các dạng cấp cao nhất, các thread khác không đẩy các task tiến lên (các task còn lại tiếp tục từ chỗ chúng dừng
  ở lần đánh giá kế tiếp). Cuối một lần đánh giá, nó chờ từng thread xong bước hiện tại của nó, nên nếu một hàm C
  (`defffi`) cứ chặn bên trong một `thread`, lần đánh giá không kết thúc cho đến khi nó trả về.
- **In trên các worker**: các phương thức `print-object` / `~/name/` được thông dịch không thể chạy trên các
  thread khác, nên in những giá trị như vậy trên một thread khác là một panic được đối xử giống một `(panic ...)`
  (`(compile T::print-object)`, hoặc in từ task chính).
- **Tranh chấp dữ liệu là không xác định** (cùng vị thế như Go). Kết quả của việc nhiều task thay đổi cùng một
  giá trị mà không qua `Mutex<T>` / `Chan<T>` không được đảm bảo.
- **`task` trả về một giá trị.** Khác câu lệnh `go` của Go, nó trả về một `Task<T>`, và `(wait t)` lấy kết quả.
- **Không có kênh nil.** Thành ngữ fan-in của Go (đặt một kênh đã đóng thành `nil` để bỏ nó khỏi các nhánh của
  `select`) không thể viết, nên hãy khởi động một task cho mỗi đầu vào và gộp chúng bằng một `WaitGroup`
  ([WaitGroup](functions/concurrency.md#4-waitgroup--chờ-n-lần-hoàn-tất)). Đó cũng là cách được khuyến
  nghị trong Go, nhưng nó là **khác biệt đầu tiên mà người đến từ Go gặp phải**.
