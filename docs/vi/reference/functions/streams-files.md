<!-- translated-from: docs/ja/reference/functions/streams-files.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# Stream và tệp

Các trait và phương thức của stream, các kiểu stream cụ thể, thao tác tệp và pathname. Socket mạng cũng là
stream, và được trình bày ở [Mạng](network.md).

## 1. Hệ phân cấp trait

Điều mà CL biểu diễn bằng một hệ phân cấp lớp thì ở đây được biểu diễn bằng một **hệ phân cấp trait**. Cả
hướng (đầu vào / đầu ra) lẫn kiểu phần tử đều được quyết định **tĩnh**, nên không cần hỏi lúc chạy "stream
này có đọc được không?".

```lisp
(deftrait Stream ()             (open-stream-p ...) (close ...))
(deftrait InputStream  (Stream) (type Item) (read-item ...))
(deftrait OutputStream (Stream) (type Item) (write-item ...))
(deftrait CharInput  ((InputStream  (Item char))) ...)   ; đầu vào ký tự
(deftrait CharOutput ((OutputStream (Item char))) ...)   ; đầu ra ký tự
(deftrait PeekInput  (CharInput) (unread-char ...) (peek-char ...))  ; đầu vào có thể đẩy lại một ký tự
(deftrait ByteInput  ((InputStream  (Item int))) ...)    ; đầu vào byte
(deftrait ByteOutput ((OutputStream (Item int))) ...)    ; đầu ra byte
```

Một hàm đọc ký tự chấp nhận mọi kiểu stream, dựng sẵn hay do người dùng định nghĩa, nếu nó nhận
`(where (CharInput S))` hoặc `:dyn CharInput`.

## 2. Phương thức

Mọi phương thức của `CharInput` đều có triển khai mặc định. Một triển khai chỉ viết `read-item`.

| Tên | Dạng | Kiểu | Mô tả |
|---|---|---|---|
| `read-item` | `(read-item s)` | `(S)→Option<Item>` | Phần tử tiếp theo. `none` ở cuối. **Phương thức duy nhất phải được triển khai** |
| `read-char` | `(read-char s)` | `(S)→Option<char>` | Ký tự tiếp theo |
| `read-line` | `(read-line s)` | `(S)→Option<string>` | Đến dòng mới tiếp theo (dòng mới được tiêu thụ và bỏ đi). Một dòng cuối không kết thúc bằng dòng mới cũng được trả về |
| `read-all` | `(read-all s)` | `(S)→string` | Mọi thứ còn lại |
| `read-char-no-hang` | `(read-char-no-hang s)` | `(S)→Option<char>` | Chỉ một ký tự đã có sẵn. `none` thay vì chờ |
| `read-sequence` | `(read-sequence s v n)` | `(S,Vector<char>,int)→int` | Đẩy tối đa `n` ký tự vào `v` và trả về số ký tự thực sự đã đọc. Ít hơn `n` chỉ ở cuối |

`listen` nằm trong `InputStream` (cha của `CharInput`):

| Tên | Dạng | Kiểu | Mô tả |
|---|---|---|---|
| `listen` | `(listen s)` | `(S)→bool` | Lần đọc tiếp theo có thể được đáp ứng mà không phải chờ hay không. Mặc định là `false`, **phía không bao giờ nói dối**: `true` sẽ là một phỏng đoán, và một phỏng đoán sai sẽ làm `read-char-no-hang` bị chặn. Mọi stream dựng sẵn đều ghi đè nó. **Với các stream do người dùng định nghĩa mà không ghi đè, `read-char-no-hang` luôn trả về `none`** |

`PeekInput` (kế thừa từ `CharInput`) thêm **việc đẩy lại một ký tự**. Chỉ bản thân stream mới có chỗ để giữ
ký tự đã đẩy lại, nên điều này không thể có triển khai mặc định và là một trait riêng.
`file-stream`/`string-input-stream`/`standard-stream` triển khai nó, và mọi stream khác có được nó khi được
bọc bằng `make-peek-stream` (chương 4).

| Tên | Dạng | Kiểu | Mô tả |
|---|---|---|---|
| `unread-char` | `(unread-char s c)` | `(S,char)→()` | Làm cho lần đọc tiếp theo trả về `c`. **Phương thức duy nhất phải được triển khai**. Như trong CL, chỉ đảm bảo một ký tự |
| `peek-char` | `(peek-char s)` | `(S)→Option<char>` | Xem ký tự tiếp theo mà không tiêu thụ nó |

Tương tự, với `CharOutput` một triển khai chỉ viết `write-item`.

| Tên | Dạng | Kiểu | Mô tả |
|---|---|---|---|
| `write-item` | `(write-item s x)` | `(S,Item)→()` | Ghi một phần tử. **Phương thức duy nhất phải được triển khai** |
| `write-char` | `(write-char s c)` | `(S,char)→()` | Ghi một ký tự |
| `write-string` | `(write-string s str)` | `(S,string)→()` | Ghi một chuỗi |
| `write-line` | `(write-line s str)` | `(S,string)→()` | Một chuỗi và một dòng mới |
| `terpri` | `(terpri s)` | `(S)→()` | Một dòng mới (tên trong CL) |
| `fresh-line` | `(fresh-line s)` | `(S)→()` | Một dòng mới trừ khi đang ở đầu dòng |
| `at-line-start` | `(at-line-start s)` | `(S)→bool` | Ký tự tiếp theo được ghi có bắt đầu một dòng hay không. Mặc định là `false` (nên `fresh-line` ghi dòng mới: khi nghi ngờ, ghi là phía an toàn). Mọi stream dựng sẵn đều ghi đè nó |
| `finish-output` | `(finish-output s)` | `(S)→()` | Đẩy bộ đệm ra |
| `write-sequence` | `(write-sequence s v)` | `(S,Vector<char>)→()` | Ghi tất cả ký tự của `v` theo thứ tự |

`at-line-start` chỉ nhớ **những gì đã được ghi qua stream đó**. `print`/`println`/`(format true ...)` ghi ra
đầu ra chuẩn mà không đi qua `*standard-output*`, nên nếu bạn trộn hai cách, `(fresh-line *standard-output*)`
không biết về các dòng mới mà `println` đã ghi. Hãy chỉ dùng một trong hai.

`Stream` chung cho mọi stream:

| Tên | Dạng | Kiểu | Mô tả |
|---|---|---|---|
| `open-stream-p` | `(open-stream-p s)` | `(S)→bool` | Nó còn mở hay không |
| `close` | `(close s)` | `(S)→()` | Đóng nó. **GC không đóng stream**, nên hãy làm điều đó tường minh (hoặc bằng `with-open-file`) |

## 3. Các kiểu stream cụ thể

| Kiểu | Cách tạo | Các trait được triển khai |
|---|---|---|
| `file-stream` | `(open-file name direction)` / `open-input` / `open-output` | `CharInput` `PeekInput` `CharOutput` |
| `string-input-stream` | `(make-string-input-stream s)` | `CharInput` `PeekInput` |
| `string-output-stream` | `(make-string-output-stream)` | `CharOutput` |
| `standard-stream` | `*standard-input*` `*standard-output*` `*error-output*` | `CharInput` `PeekInput` `CharOutput` |
| `binary-file-stream` | `(open-binary name direction)` / `open-binary-input` / `open-binary-output` | `ByteInput` `ByteOutput` |

`direction` là một trong ba hằng số `direction-input` / `direction-output` / `direction-append`. `open-file`
trả về `Err(FileError)` nếu không mở được tệp (một tệp bị thiếu là một kết quả bình thường, không phải
panic). Tên tệp có thể là một chuỗi hoặc một `pathname` (`Pathish` ở chương 9).

`(get-output-stream-string s)` trả về những gì đã được ghi vào một `string-output-stream` và làm rỗng nó.
Như trong CL, nó có thể được lấy ra ngay cả sau `close`.

**Vào/ra byte** dùng `ByteInput`/`ByteOutput`. Chúng cố định `Item` của `InputStream`/`OutputStream` là
`int`, giống cách `CharInput`/`CharOutput` cố định nó là `char`.

| Tên | Dạng | Kiểu | Mô tả |
|---|---|---|---|
| `read-byte` | `(read-byte s)` | `(S)→Option<int>` với `ByteInput S` | Byte tiếp theo. `none` ở cuối tệp |
| `write-byte` | `(write-byte s b)` | `(S,int)→()` với `ByteOutput S` | Ghi một byte. Lỗi nếu ngoài 0..255 |
| `read-sequence` | `(read-sequence s v n)` | `(S,Vector<int>,int)→int` với `ByteInput S` | Phiên bản ký tự, tính bằng byte |
| `write-sequence` | `(write-sequence s v)` | `(S,Vector<int>)→()` với `ByteOutput S` | Như trên |

CL quyết định kiểu phần tử trong **lời gọi**, như `(open name :element-type '(unsigned-byte 8))`, nhưng ở
đây kiểu phần tử là **kiểu** của stream, nên cái khác nhau là hàm mở nó. Đọc byte từ một stream ký tự là
lỗi kiểu (`string-input-stream` không triển khai `ByteInput`). Đọc một byte ngay sau khi đẩy lại một ký tự
bằng `unread-char` cũng là lỗi.

## 4. Các stream tổ hợp

Tất cả là các `defstruct` trong thư viện chuẩn và có thể lồng nhau.

| Tên | Dạng | Mô tả |
|---|---|---|
| `make-broadcast-stream` | `(make-broadcast-stream v)` | Ghi vào tất cả trong một `Vector<:dyn CharOutput>` |
| `make-two-way-stream` | `(make-two-way-stream in out)` | Đọc từ `in` và ghi vào `out` |
| `make-echo-stream` | `(make-echo-stream in out)` | Đọc từ `in` và cũng ghi các ký tự đã đọc vào `out` |
| `make-concatenated-stream` | `(make-concatenated-stream v)` | Đọc một `Vector<:dyn CharInput>` lần lượt từng cái |
| `make-peek-stream` | `(make-peek-stream in)` | Thêm việc đẩy lại một ký tự cho một `:dyn CharInput` bất kỳ, biến nó thành `PeekInput` (cho `read-sexpr`) |

## 5. Macro

| Tên | Dạng | Mô tả |
|---|---|---|
| `with-open-file` | `(with-open-file (var name direction) body...)` | Mở, chạy thân, đóng. `Result<giá trị của thân, FileError>` |
| `with-input-from-string` | `(with-input-from-string (var s) body...)` | Đọc từ một chuỗi |
| `with-output-to-string` | `(with-output-to-string (var) body...)` | Trả về những gì đã được ghi |

## 6. Hàm generic và thao tác tệp

| Tên | Dạng | Kiểu | Mô tả |
|---|---|---|---|
| `copy-stream` | `(copy-stream from to)` | `(I,O)→()` với `CharInput I`,`CharOutput O` | Chuyển mọi thứ |
| `read-lines` | `(read-lines s)` | `(S)→Vector<string>` với `CharInput S` | Tất cả các dòng còn lại |
| `read-sexpr` | `(read-sexpr s)` | `(S)→Result<ReadOutcome,ReadError>` với `PeekInput S` | Đọc một `Sexpr` (`read` của CL). `Ok(eof)` ở cuối đầu vào, `Ok(datum d)` khi đọc được một cái, `Err` nếu không phải dữ liệu. Nó **tiêu thụ một ký tự khoảng trắng** đã kết thúc datum (như trong CL). `ReadOutcome` không phải là `Option<Sexpr>` để việc đọc danh sách rỗng `()` và cuối đầu vào không phải cùng một giá trị |
| `read-sexpr-preserving-whitespace` | Như trên | Như trên | Giống vậy, nhưng để lại khoảng trắng (`read-preserving-whitespace` của CL) |
| `read-delimited-list` | `(read-delimited-list ch s)` | `(char,S)→Result<Option<Sexpr>,ReadError>` với `PeekInput S` | Đọc đến `ch` và tạo một danh sách. `ch` được tiêu thụ. `Err` nếu đầu vào hết |
| `write-lines` | `(write-lines s lines)` | `(S,I)→()` với `CharOutput S`,`Iter I (Item string)` | Ghi từng dòng một |
| `read-file-string` | `(read-file-string name)` | `(P)→Result<string,FileError>` với `Pathish P` | Toàn bộ nội dung |
| `read-file-lines` | `(read-file-lines name)` | `(P)→Result<Vector<string>,FileError>` với `Pathish P` | Tất cả các dòng |
| `write-file-string` | `(write-file-string name text)` | `(P,string)→Result<(),FileError>` với `Pathish P` | Ghi ra |
| `probe-file` | `(probe-file name)` | `(P)→bool` với `Pathish P` | Nó có tồn tại không |
| `delete-file` / `rename-file` | | `→Result<(),FileError>` | Xóa, đổi tên (các đối số là `Pathish`) |
| `truename` | `(truename name)` | `(P)→Result<string,FileError>` với `Pathish P` | Đường dẫn tuyệt đối với liên kết tượng trưng và `.`/`..` đã được phân giải. `Err` nếu nó không tồn tại |
| `file-write-date` | `(file-write-date name)` | `(P)→Result<universal-time,FileError>` với `Pathish P` | Thời điểm sửa đổi cuối cùng. Đó là **universal time**, nên `decode-universal-time` ([Thời gian](system.md#2-giải-mã-và-mã-hóa-ngày-tháng)) đọc được nó |
| `file-author` | `(file-author name)` | `(P)→Result<Option<string>,FileError>` với `Pathish P` | Tên đăng nhập của chủ sở hữu. `Err` nếu tệp không tồn tại, `Ok(none)` nếu uid của chủ sở hữu không có mục trong cơ sở dữ liệu mật khẩu: hai trường hợp mà CL phân biệt được giữ tách bạch |
| `directory-p` | `(directory-p name)` | `(P)→bool` với `Pathish P` | Có phải thư mục không. **Cũng là `false` nếu nó không tồn tại**; hãy dùng `probe-file` để phân biệt hai trường hợp |
| `directory` | `(directory name)` | `(P)→Result<Vector<string>,FileError>` với `Pathish P` | Liệt kê nội dung theo truename (đường dẫn tuyệt đối với liên kết tượng trưng đã phân giải, như `truename`). Các liên kết tượng trưng có đích bị thiếu bị bỏ qua. `.`/`..` bị bỏ qua. Thứ tự là bất cứ thứ gì HĐH đưa ra |
| `ensure-directories-exist` | `(ensure-directories-exist name)` | `(P)→Result<(),FileError>` với `Pathish P` | Tạo nó cùng với các thư mục cha. Thành công nếu nó đã tồn tại |

Mọi đối số nêu tên một tệp **có thể là một chuỗi hoặc một `pathname`**. Đây là cùng cách đối xử như các
pathname designator của CL, được phân giải qua trait `Pathish` thay vì kiểm tra kiểu lúc chạy (chương 9).

Ký tự kết thúc của `read-delimited-list` **cũng kết thúc các token**. Nó chỉ có hiệu lực ở độ sâu 0: trong
`(1 2]` dấu `]` được đọc như một phần văn bản của chính danh sách và được báo là danh sách bị hỏng. Không
có thứ tương ứng với đối số thứ ba `recursive-p` của CL.

## 7. Biến kiểu của riêng bạn thành một stream

Hãy viết một `write-item` và các triển khai mặc định sẽ mang theo phần còn lại. Nó cũng có thể đi vào các
stream tổ hợp.

```lisp
(defstruct counter (n i32))
(impl Stream counter
  (open-stream-p ((self Self)) bool true)
  (close ((self Self)) () ()))
(impl OutputStream counter
  (type Item char)
  (write-item ((self Self) (c char)) () (setf self::n (+ self::n 1))))
(impl CharOutput counter)              ; mọi phương thức còn lại là mặc định

(write-line (counter::new 0) "four")   ; write-line, terpri và fresh-line đều hoạt động
```

Đầu vào hoạt động tương tự: bạn chỉ viết `read-item`. Ngay cả một kiểu không có đẩy lại của riêng nó cũng có
thể được `read` sau khi bọc, như `(read-sexpr (make-peek-stream my-stream))`.

## 8. readtable

| Tên | Lời gọi | Kiểu | Mô tả |
|---|---|---|---|
| `set-macro-character` | `(set-macro-character c f)` | `(char, F)→()` | `f` đọc ký tự `c` |
| `get-macro-character` | `(get-macro-character c)` | `(char)→Option<F>` | Trả về thứ đã đăng ký |
| `set-dispatch-macro-character` | `(set-dispatch-macro-character d s f)` | `(char,char,F)→()` | `f` đọc chuỗi hai ký tự `d s` |
| `get-dispatch-macro-character` | `(get-dispatch-macro-character d s)` | `(char,char)→Option<F>` | Như trên |

`F` là `(fn (string-input-stream char) Option<Sexpr>)`. Cách dùng, khi nào chúng có hiệu lực và chúng khác
CL thế nào nằm ở [Tham chiếu cú pháp](../syntax.md#11-reader-macro-readtable).

## 9. Pathname `pathname`

Một tên tệp được tách thành các phần. Nó giữ các thành phần thư mục phân tách bằng `/`, tên, kiểu (phần mở
rộng), và việc nó có bắt đầu từ gốc hay không.

```lisp
(let ((p (parse-namestring "/var/log/app.tar.gz")))
  (pathname-directory p)   ; => #<vector<string> "var" "log">
  (pathname-name p)        ; => (some "app.tar")   tách ở dấu chấm cuối cùng
  (pathname-type p)        ; => (some "gz")
  (namestring p))          ; => "/var/log/app.tar.gz"

(namestring (merge-pathnames (make-pathname :name "today" :type "log")
                             "/var/log/"))        ; => "/var/log/today.log"
```

### 9.1 Trait pathname designator `Pathish`

Ở nơi CL chấp nhận một pathname designator (một chuỗi hoặc một pathname), ngôn ngữ này chấp nhận một
`Pathish`. Cả `string` lẫn `pathname` đều triển khai nó, và **mọi thao tác tệp nhận nó một cách generic**, nên
`(open-input "a.txt")` và `(open-input p)` đều là các lời gọi thông thường (không có kiểm tra kiểu lúc
chạy). `namestring` của một chuỗi chỉ trả về chính nó, nên miễn là bạn truyền một chuỗi, không có việc phân
tích nào xảy ra.

| Tên | Dạng | Kiểu | Mô tả |
|---|---|---|---|
| `namestring` | `(namestring p)` | `(P)→string` | Dạng chuỗi. Phải được triển khai |
| `to-pathname` | `(to-pathname p)` | `(P)→pathname` | Chuyển thành một `pathname` (hàm `pathname` của CL, được đổi tên vì nó sẽ trùng với tên kiểu). Phải được triển khai |

### 9.2 Hàm

| Tên | Dạng | Kiểu | Mô tả |
|---|---|---|---|
| `parse-namestring` | `(parse-namestring s)` | `string→pathname` | Tách một chuỗi thành các phần. Một `/` ở cuối (hoặc một tên rỗng) nghĩa là "không có tên", tức là một thư mục |
| `make-pathname` | `(make-pathname :directory v :name s :type s :absolute b)` | `→pathname` | Dựng một pathname chỉ từ các thành phần được đưa ra (tất cả là `&key`). Một tên hoặc kiểu bị bỏ qua vẫn "vắng mặt" và là thứ `merge-pathnames` điền vào |
| `pathname-directory` | `(pathname-directory p)` | `(P)→Vector<string>` | Các thành phần thư mục, ngoài cùng đứng đầu |
| `pathname-name` | `(pathname-name p)` | `(P)→Option<string>` | Tên không có kiểu. `none` với một thư mục |
| `pathname-type` | `(pathname-type p)` | `(P)→Option<string>` | Sau dấu chấm cuối cùng. Dấu chấm ở đầu không tính (toàn bộ `.gitignore` là tên) |
| `pathname-absolute-p` | `(pathname-absolute-p p)` | `(P)→bool` | Nó có bắt đầu từ gốc không |
| `user-homedir-pathname` | `(user-homedir-pathname)` | `()→Option<pathname>` | Thư mục home. `none` nếu không có `$HOME` (CL cũng cho phép `NIL`) |
| `directory-namestring` | `(directory-namestring p)` | `(P)→string` | Phần đến `/` cuối cùng |
| `file-namestring` | `(file-namestring p)` | `(P)→string` | Chỉ phần `name.type` |
| `merge-pathnames` | `(merge-pathnames p default)` | `(P,D)→pathname` | Điền các thành phần còn thiếu của `p` từ `default`. Một `p` tương đối được đặt dưới thư mục của `default`; một `p` tuyệt đối giữ thư mục của chính nó |
| `enough-namestring` | `(enough-namestring p default)` | `(P,D)→string` | Dạng tương đối so với `default`. Toàn bộ `p` nếu nó không nằm dưới gốc cơ sở |

Các đối số kiểu đều mang `(where (Pathish P))`.

## 10. Khác biệt so với CL

- **Một hệ phân cấp trait, không phải hệ phân cấp lớp.** Không có `input-stream-p` / `output-stream-p`:
  kiểu mang hướng, nên đó không phải câu hỏi để hỏi lúc chạy.
- **`read` có các tên khác nhau cho phiên bản chuỗi và stream.** `(read "...")` (tương ứng với giá trị đầu
  tiên của `read-from-string` của CL; nếu bạn cũng cần vị trí nơi việc đọc kết thúc, hãy dùng
  `read-from-string`) và `(read-sexpr s)` (`read` của CL). Một lời gọi được phân giải theo một kiểu bên
  nhận, nên cùng một tên không thể bị nạp chồng.
- **Đẩy lại là một trait riêng** (`PeekInput`), nên các kiểu chỉ cần `read-char` không bị buộc phải triển
  khai `unread-char`.
- **Đóng là tường minh.** GC không đóng stream (GC chạy vào những lúc không đoán trước, nên giao cho GC sẽ
  khiến thời điểm đóng cũng không đoán trước). Dùng `with-open-file` là cách an toàn.
- **Pathname không có các thành phần host, device hay version.** Không có pathname ký tự đại diện và không
  có pathname logic (`logical-pathname`). Dấu phân cách luôn là `/`.
- **Hàm `pathname` là `to-pathname`**, vì kiểu, trait và hàm dùng chung một không gian tên.
- **Không có so khớp bằng ký tự đại diện**, nên `directory` là một hàm "liệt kê nội dung của thư mục đó" và
  không hơn. `directory` của CL so khớp với một mẫu pathname.
