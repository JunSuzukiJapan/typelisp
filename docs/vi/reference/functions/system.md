<!-- translated-from: docs/ja/reference/functions/system.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# Thời gian, môi trường và triển khai

Các hàm về thời gian, truy vấn môi trường chạy, công cụ triển khai, phân tích và đánh giá văn bản, docstring
và macro.

## 1. Thời gian

| Tên | Dạng | Kiểu | Mô tả |
|---|---|---|---|
| `universal-time` | — | `defstruct` | Hai trường: `day` (số ngày kể từ 1900-01-01) và `second` (giây trong ngày đó, 0..86399) |
| `internal-time` | — | `defstruct` | Hai trường: `second` và `microsecond` (trong giây đó, 0..999999) |
| `get-universal-time` | `(get-universal-time)` | `()→universal-time` | Thời gian kể từ epoch của CL (1900-01-01 UTC) |
| `get-internal-real-time` | `(get-internal-real-time)` | `()→internal-time` | Thời gian đã trôi qua tương đối so với tiến trình |
| `get-internal-run-time` | `(get-internal-run-time)` | `()→internal-time` | **Thời gian CPU** mà tiến trình này đã dùng (người dùng cộng hệ thống) |
| `internal-time-seconds` | `(internal-time-seconds it)` | `internal-time→f64` | Dưới dạng số giây. Dạng để báo cáo hiệu giữa hai lần đọc |
| `internal-time-units-per-second` | — | `int` | `1000000` (micro giây), đơn vị của trường `microsecond`. Như trong CL, giá trị là lựa chọn của triển khai |
| `time` | `(time form)` | Macro | Chạy `form`, in thời gian thực và thời gian CPU, mỗi loại một dòng, và trả về giá trị của `form` nguyên trạng |

Thời gian thực và thời gian CPU cho bạn biết những điều khác nhau. Với công việc chủ yếu chờ I/O, hai giá trị
khác nhau nhiều, và chính sự khác biệt đó là điều bạn muốn biết, nên `time` hiển thị cả hai.

`sleep`, vốn dừng một task, nằm ở [Task và kênh](concurrency.md#3-yield--sleep--nhường-lượt).

## 2. Giải mã và mã hóa ngày tháng

| Tên | Dạng | Kiểu | Mô tả |
|---|---|---|---|
| `decoded-time` | — | `defstruct` | **Chín trường**: `second` / `minute` / `hour` / `date` / `month` / `year` / `day-of-week` / `daylight-p` / `zone`. Chín giá trị trả về của CL dưới dạng một struct (không có đa giá trị) |
| `decode-universal-time` | `(decode-universal-time ut &optional zone)` | `(universal-time,int)→decoded-time` | Universal time thành các thành phần lịch. `zone` là số giờ về phía tây Greenwich (cùng chiều như CL). **Nếu bỏ qua, là giờ địa phương** (như trong CL) |
| `encode-universal-time` | `(encode-universal-time sec min hour date month year &optional zone)` | `(int×6,int)→universal-time` | Chiều ngược lại. Không có `zone`, các đối số được đọc là **giờ địa phương** |
| `get-decoded-time` | `(get-decoded-time)` | `()→decoded-time` | Bây giờ, được giải mã theo giờ địa phương |
| `timezone-offset-seconds` | `(timezone-offset-seconds day second)` | `(int,int)→Option<int>` | Độ lệch của giờ địa phương về phía tây Greenwich, tính bằng **giây**, tại universal time đó |
| `timezone-daylight-p` | `(timezone-daylight-p day second)` | `(int,int)→Option<bool>` | Giờ mùa hè có hiệu lực tại universal time đó hay không |

Như trong CL, với `day-of-week` **0 là thứ Hai và 6 là Chủ nhật**.

**Không có `zone`, giờ địa phương được dùng**, như trong CL. Độ lệch địa phương được hỏi từ HĐH, nên kết quả
phụ thuộc vào nơi đặt máy. **Đưa ra một zone tường minh làm cho nó có tính xác định**, và `0` là UTC.

Đơn vị của `zone` là, như trong CL, "số giờ về phía tây Greenwich", nên UTC+9 được đọc là `-9`. Tuy nhiên,
**đối số là một số nguyên còn trường `zone` của kết quả là một `f64`**. Các độ lệch thật không phải lúc nào
cũng là số giờ nguyên (Ấn Độ là +5:30, Nepal +5:45), và làm tròn giá trị được báo cáo sẽ âm thầm nói dối. Một
zone bạn tự viết là số giờ nguyên, nên đối số là `int`.

Khi có `zone`, `daylight-p` là `false` và `zone` đúng bằng giá trị đã đưa, như CL quy định (*If a time-zone is
supplied, daylight saving time information is ignored*).

Một giờ địa phương rơi vào bên trong một lần chuyển giờ mùa hè vốn đã không duy nhất, và CL không nói phải
chọn cái nào. `encode-universal-time` trả về một trong hai đáp án cho một thời điểm như vậy.

## 3. Môi trường chạy

| Tên | Dạng | Kiểu | Mô tả |
|---|---|---|---|
| `command-line-args` | `(command-line-args)` | `()→Vector<string>` | Dòng lệnh. **Phần tử 0 là tên chương trình** |
| `getenv` | `(getenv name)` | `string→Option<string>` | Một biến môi trường. `none` nếu chưa đặt hoặc không phải UTF-8 |
| `home-directory` | `(home-directory)` | `()→Option<string>` | `$HOME`. Cơ sở của `user-homedir-pathname` ([Pathname](streams-files.md#92-hàm)) |
| `lisp-implementation-type` | `(lisp-implementation-type)` | `()→string` | `"typelisp"` |
| `lisp-implementation-version` | `(lisp-implementation-version)` | `()→string` | Phiên bản của triển khai |
| `machine-type` | `(machine-type)` | `()→string` | Kiến trúc CPU (`x86_64` / `aarch64` …). Giá trị của **đích build** |
| `machine-instance` | `(machine-instance)` | `()→Option<string>` | Tên host |
| `machine-version` | `(machine-version)` | `()→Option<string>` | Tên phần cứng **đang chạy** (`Apple M1` / `Intel(R) Xeon(R) …`). `none` ở nơi không xác định được |
| `software-type` | `(software-type)` | `()→string` | HĐH (`macos` / `linux` …) |
| `software-version` | `(software-version)` | `()→Option<string>` | Bản phát hành của HĐH (`uname -r`, ví dụ `24.6.0`) |
| `short-site-name` | `(short-site-name)` | `()→Option<string>` | Một tên ngắn cho địa điểm cài đặt. **Luôn là `none`** |
| `long-site-name` | `(long-site-name)` | `()→Option<string>` | Tương tự, một tên dài. **Luôn là `none`** |

Những hàm trả về `Option` là các mục mà CL cho phép `NIL` (*or nil if no such name can be determined*). POSIX
không có chỗ ghi tên địa điểm, nên chúng luôn là `none`; SBCL trả về giống vậy. Hãy lưu ý sự khác nhau giữa
`machine-type` và `machine-version`: cái trước là kiến trúc mà tệp nhị phân này được **build** cho, cái sau là
con chip đang **chạy** nó.

Phần tử 0 của `command-line-args` là đường dẫn của script với `typl script.typl a b`, và là chính tệp thực
thi với một tệp thực thi AOT chạy dưới dạng `./prog a b`. **Dù chạy theo cách nào, các đối số đọc ra giống nhau
ở cùng các chỉ số** (`typl` bỏ tên của chính nó và các tùy chọn như `--heap-cells` trước khi chuyển tiếp).

## 4. Hỏi người dùng

| Tên | Dạng | Kiểu | Mô tả |
|---|---|---|---|
| `y-or-n-p` | `(y-or-n-p question)` | `string→bool` | Nhận một ký tự `y` / `n`. Hỏi lại cho đến khi nhận được |
| `yes-or-no-p` | `(yes-or-no-p question)` | `string→bool` | Bắt người dùng đánh vần `yes` / `no`. Dành cho các câu hỏi mà một sai sót có giá đắt |

Cả hai đọc từ `*standard-input*`. Chỉ hết đầu vào mới dừng việc hỏi lại, và khi đó kết quả là `false`.

## 5. Công cụ triển khai (CLHS 25.2)

Tầng mà triển khai trả lời các câu hỏi về chính nó. `heap-info` / `room` / `dribble` là các hàm thông thường;
`trace` / `untrace` / `step` / `disassemble` / `ed` là **các dạng đặc biệt** (`trace` / `untrace` /
`disassemble` / `ed` nhận *tên* của một định nghĩa, và `step` một *dạng*, tất cả đều không được đánh giá).

| Tên | Dạng | Kiểu | Mô tả |
|---|---|---|---|
| `heap-info` | `(heap-info)` | `()→heap-info` | Trạng thái hiện tại của heap dưới dạng một struct. Cùng các con số mà `room` in |
| `room` | `(room &optional verbose)` | `(bool)→()` | Báo cáo `heap-info` ra `*standard-output*`. `(room true)` cho chi tiết hơn |
| `dribble` | `(dribble &optional path)` | `(string)→Result<(),FileError>` | Bắt đầu ghi đầu ra của phiên vào `path` / dừng ghi khi được gọi không có đối số |
| `trace` | `(trace name...)` | `Sexpr` | Báo cáo các lời gọi của các định nghĩa được nêu tên ra `*trace-output*`. Trả về danh sách các tên đang được theo dõi |
| `untrace` | `(untrace name...)` | `Sexpr` | Dừng báo cáo. **Không có đối số, bỏ tất cả** |
| `step` | `(step form)` | Kiểu của `form` | Đánh giá `form`, dừng ở mỗi lời gọi để hỏi |
| `disassemble` | `(disassemble name [llvm])` | `()` | In ra thứ mà định nghĩa đó trở thành. Mặc định là mã máy của máy chủ, với `true` là LLVM IR |
| `ed` | `(ed)` / `(ed name)` / `(ed "path")` | `Result<(),FileError>` | Khởi động `$VISUAL` / `$EDITOR`. Khi đưa một tên, nó mở dòng nơi định nghĩa đó được viết |

`trace`/`untrace`/`step`/`disassemble` chỉ dành cho trình thông dịch, và các hàm gọi chúng không thể biên dịch
([Tham chiếu cú pháp chương 10](../syntax.md#10-biên-dịch)).

### 5.1 Các trường của `heap-info`

| Trường | Kiểu | Nội dung |
|---|---|---|
| `capacity` / `live` / `free` | `int` | Toàn bộ arena cons và phân chia của nó. Luôn có `live + free = capacity` |
| `symbols` / `strings` / `boxes` | `int` | Số lượng hiện tại của ba loại đối tượng khác của heap |
| `gc-count` | `int` | Số lần thu gom kể từ khi triển khai bắt đầu |
| `growable` | `bool` | Arena còn có thể tăng trưởng hay không |

Các trường đều là `int` (trừ `growable`). Giới hạn tăng trưởng (xem mô tả của `typl --heap-cells`) không được
báo cáo, vì điều người đọc muốn biết là nó còn tăng trưởng được hay không (`growable`).

### 5.2 `trace` / `step` thấy được và không thấy được gì

- **Các định nghĩa có thân đã biên dịch cũng thấy được, từ các vị trí gọi đang được thông dịch.**
- **Các vị trí gọi *bên trong* mã đã biên dịch thì không thấy được.** Theo dõi một tên có thân đã biên dịch
  sẽ thêm một ghi chú một dòng nói điều đó. Cùng hạn chế mà SBCL mô tả cho các lời gọi cục bộ.
- **Các lời gọi qua giá trị closure (`funcall`/`apply`) không thấy được.** Closure không có tên.
- **Các định nghĩa generic không được bao phủ.** Một bản sao cho từng kiểu được tạo ở mỗi nơi dùng, nên không
  có thân đơn lẻ nào để đặt tên (cùng lý do, và cùng cách diễn đạt, như khi `compile` từ chối).

Các lệnh của `step` là `s` (bước vào lời gọi này; một dòng trống cũng vậy), `n` (bỏ qua lời gọi này), `c`
(ngừng hỏi từ đây trở đi) và `q` (hủy bỏ). **Nếu đầu vào chuẩn không phải một terminal, `step` chỉ đánh giá
`form`**: một hành vi suy biến mà CLHS cho phép tường minh, để các script và kiểm thử không bị treo ở một dấu
nhắc mà không ai trả lời được.

`$VISUAL` / `$EDITOR` của `ed` được tách tại khoảng trắng, nên `EDITOR="code -w"` hoạt động. Nếu cả hai đều
chưa đặt, kết quả là `Err`: nó không đoán `vi`. Số dòng được truyền đầu tiên, dưới dạng `+N`.

`dribble` ghi lại cả ba cách mà đầu ra của phiên rời khỏi tiến trình: những gì `print`/`println`/`format` ghi,
những gì được ghi vào các stream nối với đầu ra chuẩn, và các dòng gõ vào REPL cùng với các giá trị mà REPL in
trả lại.

## 6. Phân tích và đánh giá

Tất cả các hàm này xử lý văn bản và dữ liệu từ lúc chạy (mà chính chương trình không kiểm soát), nên khi thất
bại chúng trả về `Err` của một `Result` thay vì panic. Các kiểu lỗi là các kiểu cụ thể theo từng phép toán
([Các kiểu lỗi](option-result.md#3-các-kiểu-lỗi-và-trait-error)).

| Tên | Dạng | Kiểu | Mô tả |
|---|---|---|---|
| `parse-int` | `(parse-int s &key radix junk-allowed)` | `string→Result<int,ParseIntError>` | `parse-integer` của CL. Bỏ qua khoảng trắng ở đầu và cuối (cùng tập như `trim`), đọc tối đa một dấu `+`/`-`, rồi các chữ số theo cơ số `radix` (mặc định 10, 2 đến 36; các chữ số trên 10 ở dạng hoa hoặc thường đều được). Không có giới hạn về số chữ số (`int`). Bất kỳ ký tự nào khác còn lại cho `Err`. Với `:junk-allowed true`, nó dừng ở ký tự không phải chữ số đầu tiên và bỏ qua phần còn lại, nhưng cho `Err` nếu không có một chữ số nào (tương ứng với `nil` của CL). Nó không trả về giá trị thứ hai của CL (vị trí nơi việc đọc kết thúc). Một `radix` ngoài phạm vi sẽ panic (sai sót của bên gọi, không phải của văn bản) |
| `parse-float` | `(parse-float s)` | `string→Result<f64,ParseFloatError>` | Một số dấu phẩy động. Cũng chấp nhận `inf`/`nan` |
| `read` | `(read s)` | `string→Result<Option<Sexpr>,ReadError>` | Đọc một `Sexpr` từ `s` (bằng cùng reader đọc mã nguồn). Ngoặc không cân bằng, chuỗi không kết thúc và những thứ tương tự cho `Err`. Đọc từ một stream là `read-sexpr` ([Stream](streams-files.md#6-hàm-generic-và-thao-tác-tệp)) |
| `read-from-string` | `(read-from-string s [start])` | `(string,int)→Result<cons-cell<Option<Sexpr>,int>,ReadError>` | `read` cộng **vị trí nơi việc đọc kết thúc**. `(car r)` là giá trị và `(cdr r)` là vị trí của ký tự tiếp theo cần đọc. `start` mặc định là 0 |
| `read-from-string-preserving-whitespace` | Như trên | Như trên | Giống vậy, nhưng không tiêu thụ khoảng trắng đã kết thúc datum. Sự khác biệt thể hiện ở vị trí được trả về |
| `eval` | `(eval form)` | `Option<Sexpr>→Result<Option<Sexpr>,EvalError>` | Kiểm tra kiểu `form` lúc chạy và đánh giá nó. Theo `eval` của CL |

CL trả về **hai giá trị** (giá trị và vị trí) từ `read-from-string`, nhưng ngôn ngữ này không có đa giá trị,
nên nó trả về một `cons-cell`. Có vị trí làm cho việc đọc một chuỗi từng datum một thành một vòng lặp thay vì
quét lại:

```lisp
(let ((s "1 2 3") (i 0) (going true))
  (while going
    (match (read-from-string s i)
      ((ok p) (progn (println "~s" (car p)) (setf i (cdr p))
                     (if (>= i (length s)) (progn (setf going false) ()) ()) ()))
      ((err e) (progn (setf going false) ())))))
```

Sự khác biệt do `preserving-whitespace` tạo ra là **một ký tự khoảng trắng**: `read` của CL tiêu thụ khoảng
trắng đã kết thúc datum, còn `read-preserving-whitespace` để lại nó. `(read-from-string "12 34")` trả về vị
trí 3, và phiên bản preserving trả về 2.

Cú pháp số mà reader chấp nhận nằm ở [Tham chiếu cú pháp chương 1](../syntax.md#1-cú-pháp-từ-vựng). Những gì
`*print-radix*` ([In](printing.md#62-cơ-số-hoa-thường-và-khả-năng-đọc-lại)) in ra có thể đọc ngược lại nguyên trạng. Không
có `*read-base*` của CL.

### 6.1 `eval` có nghĩa là gì

Nó theo `eval` của CLHS: đánh giá trong **môi trường toàn cục hiện tại** (hàm toàn cục, biến, kiểu và macro,
gồm cả các định nghĩa được thêm lúc chạy) và trong **môi trường từ vựng rỗng** (các ràng buộc cục bộ của
`let`/`lambda` của bên gọi không nhìn thấy được). Cả biểu thức lẫn định nghĩa
(`defun`/`defvar`/`defstruct`/`defenum`/`defmacro`) đều có thể được đánh giá, và các định nghĩa được đăng ký
ngay lập tức và vĩnh viễn vào môi trường toàn cục.

```lisp
(eval (unwrap (read "(+ 40 2)")))                 ; => (ok 42)
(defvar (x i32) 10)
(eval (unwrap (read "(+ x 5)")))                  ; => (ok 15)  ; x toàn cục nhìn thấy được
(eval (unwrap (read "(defun sq ((n i32)) i32 (* n n))")))  ; => (ok sq)  ; trả về tên đã định nghĩa
(eval (unwrap (read "(sq 9)")))                   ; => (ok 81)  ; định nghĩa vừa tạo nhìn thấy được
```

- **Giá trị trả về**: với một biểu thức, kết quả dưới dạng một `Option<Sexpr>`; với một định nghĩa, symbol của
  tên đã định nghĩa (như trong CL). Để dùng kết quả, hãy tách `Sexpr` ra bằng `match`
  (`(int n)`/`(str s)`/…).
- **Khác biệt do kiểu tĩnh (quan trọng)**: CL trả về giá trị thực của kết quả, nhưng trong ngôn ngữ này kiểu
  trả về chỉ có thể là `Result<Option<Sexpr>,EvalError>` đồng nhất. Ngoài ra, **mã viết tĩnh không thể tham
  chiếu tiến tới các tên mà `eval` định nghĩa lúc chạy**: một `(sq 9)` viết trực tiếp trong tệp được kiểm
  tra trước khi `eval` định nghĩa `sq` chạy, và là "chưa định nghĩa". Tuy nhiên, **các `eval` sau đó thì thấy
  được** (việc kiểm tra kiểu của chúng chạy lúc chạy, sau định nghĩa). REPL kiểm tra và chạy từng dòng một,
  nên một tên được định nghĩa bằng `eval` có thể được gọi trực tiếp từ dòng tiếp theo.
- **Lỗi**: lỗi kiểu và lỗi cú pháp trả về `Err` (chúng không panic). **Panic lúc chạy** trong mã được đánh
  giá (chia cho không, v.v.) lan truyền như từ mã viết trực tiếp. Phần dọn dẹp của mọi `unwind-protect` ở giữa
  đều chạy ([Tham chiếu cú pháp chương 8](../syntax.md#8-lối-thoát-phi-cục-bộ-catch--throw--unwind-protect)).
- **Không gian tên**: khi chạy bằng `typl file.typl` và bên trong một tệp thực thi AOT, `eval` đánh giá trong
  không gian tên của module của script (các biến toàn cục của chính script nhìn thấy được). REPL đánh giá
  trong không gian tên gốc.
- **Biên dịch**: cả `read` lẫn `eval` đều có thể biên dịch. Cách chúng được xử lý trong các tệp thực thi AOT,
  và hệ quả (các dạng truyền cho eval được thông dịch), nằm ở
  [Tham chiếu cú pháp 10.2](../syntax.md#102-eval-trong-các-tệp-thực-thi-aot).

## 7. Docstring / `documentation`

`defun`/`defmethod` (kể cả bên trong `impl`)/`defmacro`/`defvar`/`defconstant`/`defstruct`/`defenum`/
`deftype`/`deftrait` có thể mang docstring. Vị trí theo quy tắc của CL cho từng loại:

| Dạng | Vị trí của docstring |
|---|---|
| `defun` / `defmethod` / `defmacro` | Ở đầu thân (sau kiểu trả về và mệnh đề `where`). Chỉ khi có ít nhất một dạng của thân theo sau; một chuỗi đứng một mình vẫn là giá trị trả về |
| `defvar` / `defconstant` | **Sau** giá trị ban đầu: `(defvar (name Type) value "doc")` |
| `defstruct` / `defenum` | **Ngay sau** tên, trước các trường/variant |
| `deftype` | **Ngay sau** tên, trước kiểu: `(deftype meters "doc" i32)` |
| `deftrait` | Ngay sau danh sách supertrait, trước các mục. Một cái cho cả trait. **Các phương thức có triển khai mặc định** có thể đặt docstring riêng ngay trước thân của chúng |

| Tên | Dạng | Kiểu | Mô tả |
|---|---|---|---|
| `documentation` | `(documentation name)` | (dạng đặc biệt; `name` là một symbol trần hoặc `Type::method`)→`Option<string>` | Trả về docstring của `name` |

Như `quote`/`compile`, `documentation` là một dạng đặc biệt (nó đọc `name` như một tên không được đánh giá).
Khác `(documentation 'name 'function)` của CL, nó không nhận đối số kiểu; thay vào đó nó phân giải một tên trần
theo thứ tự **biến → hàm → kiểu → trait → macro** (cùng độ ưu tiên như với một định danh trần được đánh giá như
một biểu thức). Dạng `Type::method` tra docstring của một phương thức liên kết hoặc tĩnh.

```lisp
(defun square ((n i32)) i32
  "Returns n squared."
  (* n n))

(unwrap-or (documentation square) "no docs")   ; => "Returns n squared."

(defstruct point
  "A 2D point."
  (x i32)
  (y i32))

(unwrap-or (documentation point) "no docs")    ; => "A 2D point."
```

**Giá trị được quyết định lúc kiểm tra**: nếu tên không phân giải được thành định nghĩa nào, đó là lỗi lúc kiểm
tra (như tham chiếu một biến chưa định nghĩa). Nếu nó phân giải được nhưng không có docstring, kết quả là
`Option::none`.

**Không được bao phủ**:

- `(setf documentation)` (thay đổi một docstring lúc chạy) không tồn tại.
- Các tên tự do có tiền tố module (`mod::name`; `Type::method` được hỗ trợ) không được hỗ trợ.
- Một khai báo phương thức trong `deftrait` **không có thân** không thể có docstring. Một literal chuỗi ở cuối
  sẽ tự nó là thân (giá trị trả về) của một triển khai mặc định, nên không có cách nào phân biệt hai trường
  hợp.

Hover của language server (`typl-lsp`) cũng hiển thị docstring.

## 8. Macro

Cách định nghĩa macro nằm ở [Tham chiếu cú pháp 3.14](../syntax.md#314-defmacro--định-nghĩa-macro).

| Tên | Dạng | Kiểu | Mô tả |
|---|---|---|---|
| `gensym` | `(gensym)` / `(gensym prefix)` | `(&optional string)→symbol` | Một symbol mới. Tên của nó là `" <prefix><n>"`, trong đó `n` là `*gensym-counter*`. Một dấu cách ở đầu không thể viết trong mã nguồn, nên các ràng buộc được sinh ra không bao giờ xung đột với các tên đã viết |
| `*gensym-counter*` | Biến | `int` | Số mà `gensym` dùng tiếp theo. Như trong CL, nó có thể đọc và đặt |
| `macroexpand-1` | `(macroexpand-1 form)` | `Option<Sexpr>→Result<Option<Sexpr>,EvalError>` | Khai triển một lời gọi macro một bước. `none` nghĩa là "không phải một lời gọi macro" |
| `macroexpand` | `(macroexpand form)` | `Option<Sexpr>→Result<Option<Sexpr>,EvalError>` | Lặp lại cho đến khi không còn là macro |

`macroexpand-1` trả về một `Option`. CL báo "có khai triển hay không" dưới dạng giá trị trả về thứ hai, nhưng
không có đa giá trị, nên `none` đóng vai trò đó. **Một macro khai triển thành một lời gọi chính nó không bao giờ
bị nhầm với một thứ không phải macro.** Một bước khai triển là cùng bước mà bộ kiểm tra kiểu dùng, nên những gì
chương trình thấy và những gì bộ kiểm tra thấy không bao giờ khác nhau.

```lisp
(defmacro twice (x) `(+ ,x ,x))
(macroexpand-1 '(twice 5))   ; => (ok (+ 5 5))
(macroexpand-1 '(+ 1 2))     ; => (ok ())      ; none hiển thị là danh sách rỗng (Option<Sexpr> là trong suốt)
(macroexpand '(when true 1)) ; => (ok (if true (progn 1 ()) ()))
```

Những gì CL có mà ngôn ngữ này không có: `eval-when` (`:compile-toplevel`/`:load-toplevel`/`:execute` luôn
trùng nhau, nên không có sự phân biệt nào để chọn), `define-compiler-macro`, `load-time-value`,
`make-symbol`/`copy-symbol`/`gentemp` (các symbol không được intern; các ràng buộc được tra theo tên, nên sẽ
không được lợi gì).

## 9. Ràng buộc macro cục bộ (`macrolet` / `symbol-macrolet`)

Cả hai là các dạng đặc biệt ràng buộc **các tên không phải giá trị** theo phạm vi từ vựng. Không có gì còn lại
lúc chạy: thứ được biên dịch là dạng đã khai triển của thân.

```lisp
(macrolet ((twice (x) `(+ ,x ,x)))
  (twice 21))                       ; => 42

(let ((v (the Vector<i32> (Vector::new))))
  (progn (push v 7)
    (symbol-macrolet ((head (get v 0)))
      (progn (setf head 42) head))))  ; => 42
```

- Một ràng buộc `macrolet` che một macro toàn cục cùng tên **chỉ trong thân**. Danh sách lambda giống như của
  `defmacro` (`&optional`/`&rest`/`&key`).
- **Các anh em của cùng một `macrolet` không thấy nhau từ *thân* của chúng** (như trong CL; đây là điểm khác
  với `labels`). Các khai triển được kiểm tra tại nơi dùng, nên `earlier` khai triển thành `(later ...)` hoạt
  động: cả hai đều nhìn thấy được tại nơi đó.
- Một tên `symbol-macrolet` đi vào môi trường như một ràng buộc thông thường. Vì vậy một `let` bên trong che
  cùng tên đó, và một biến bên ngoài bị che: các quy tắc của CL cho ra đúng như vậy.
- **`setf` ghi vào khai triển.** `(setf head 42)` là `(setf (get v 0) 42)`.
- Các khai triển được kiểm tra trong **môi trường của nơi dùng** (không phải nơi ràng buộc).

## 10. Khác

| Tên | Dạng | Kiểu | Mô tả |
|---|---|---|---|
| `assert` | `(assert test)` / `(assert test msg)` | `(bool[,string])→()` | Panic nếu false. Không có thông điệp, là `assertion failed: <phép kiểm tra như đã viết>` (nó là một macro, nên có thể nêu chính biểu thức). Các restart của CL không tồn tại trong ngôn ngữ này |
| `warn` | `(warn control args...)` | `(string,...)→()` | Ghi một dòng có tiền tố `WARNING: ` ra `*error-output*` và **tiếp tục**. Một cách báo cáo điều gì đó mà không trả về một `Result` và không kết thúc chương trình |
| `dlet` | `(dlet ((*var* val)...) body...)` | — | Thay các biến toàn cục chỉ trong `body` và khôi phục chúng khi ra. CL viết điều này là `let`, nhưng `let` trong ngôn ngữ này luôn ràng buộc theo phạm vi từ vựng, vì vậy có tên riêng (cùng vai trò như macro cùng tên của Emacs Lisp). Khôi phục chúng dù thân được rời đi bằng cách nào: hoàn tất bình thường, `throw`, `panic`, `break`/`return`. **Không phải ràng buộc theo từng task** |
| `with-standard-io-syntax` | `(with-standard-io-syntax body...)` | — | Chạy `body` với mọi biến điều khiển bộ in ở giá trị chuẩn và `*read-eval*` đặt là `true` ([In](printing.md#6-điều-khiển-lượng-được-in)) |
| `exit` | `(exit code)` | `int→!` | Kết thúc tiến trình |
| `dump` | `(dump path)` | `string→bool` | Ghi môi trường hiện tại (thông tin kiểu cộng các thân đã biên dịch) vào một tệp. `typl --image <path>` khởi động lại từ đó. Chỉ dành cho trình thông dịch ([Tham chiếu cú pháp 10.1](../syntax.md#101-dump)) |
