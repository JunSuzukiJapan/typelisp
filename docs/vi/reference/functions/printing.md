<!-- translated-from: docs/ja/reference/functions/printing.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# In

`print`/`println`/`format`, các hàm in một đối số, pretty printer, `print-object`, và các biến điều khiển việc
in. Danh sách các chỉ thị định dạng nằm ở [format.md](format.md). Việc đọc từ và ghi vào stream nằm ở
[Stream và tệp](streams-files.md).

## 1. `print` / `println` / `format`

`print`/`println`/`format` đều là **các dạng đặc biệt thông dịch các chỉ thị định dạng (các chỉ thị `format`
của CL)**. Đối số đầu tiên (thứ hai với `format`) là **chuỗi điều khiển**, và mỗi chỉ thị lần lượt tiêu thụ các
đối số biến đổi theo sau.

| Tên | Dạng | Kiểu | Mô tả |
|---|---|---|---|
| `print` | `(print control args...)` | `(string, ...)→Unit` | Khai triển chuỗi điều khiển và ghi ra đầu ra chuẩn mà không có dòng mới |
| `println` | `(println control args...)` | `(string, ...)→Unit` | Giống vậy, với một dòng mới ở cuối |
| `format` | `(format dest control args...)` | `(bool, string, ...)→string` | `format` của CL. Trả về chuỗi đã khai triển. Nếu `dest` là `true` (`t` của CL), nó cũng được ghi ra đầu ra chuẩn; nếu là `false` (`nil` của CL), nó không được ghi và chỉ được trả về |
| `format` (tới một stream) | `(format stream control args...)` | `(S, string, ...)→()` với `CharOutput S` | Nếu `dest` không phải `bool`, đó là đích stream của CL. Chuỗi đã khai triển được ghi vào stream đó. Giá trị trả về là `()` (`nil` của CL), và không có chuỗi nào được trả về |

Kiểu của `dest` chia ý nghĩa làm hai (cái nào áp dụng được quyết định tĩnh). Dạng stream có thể được viết
giống nhau với một kiểu stream cụ thể, một `:dyn CharOutput`, hoặc một biến kiểu bị ràng buộc bởi
`(where (CharOutput S))`. Một `dest` không phải `bool` cũng không phải stream là lỗi kiểu.

**Chuỗi điều khiển phải là một literal** (cùng hạn chế như `format!` của Rust). Các chỉ thị trong đó quyết
định có bao nhiêu đối số được lấy và thuộc kiểu nào, nên một chuỗi dựng lúc chạy không thể đọc được lúc
kiểm tra. Vì nó phải là literal, **số lượng và kiểu của các đối số được kiểm tra lúc kiểm tra**:
`(println "~d" "x")` và `(println "~a ~a" 1)` là các lỗi lúc kiểm tra. Một chỉ thị viết sai chính tả, một
`~(` không đóng, và một `~/name/` mà không đối số nào đáp ứng được cũng là các lỗi lúc kiểm tra. Các quy tắc
kiểm tra nằm ở [format.md](format.md#1-cách-viết-chỉ-thị). Để in một chuỗi bạn dựng, hãy tạo nó bằng
`(format false ...)` và in bằng `(println "~a" s)`.

Các đối số biến đổi được bọc vào `Sexpr` cùng kiểu của chính chúng trước khi truyền: `i32`/`f64`/`int`/
`ratio`/`char`/`bool`/`string`/`Sexpr`, cũng như `defstruct`/`defenum`/`Vector<T>`/`HashTable<K,V>` do người
dùng định nghĩa và những kiểu tương tự, đều có thể truyền nguyên trạng (`(println "~a" my-struct)` chạy
được ngay).

Chạy một script bằng `typl file.typl` **không in giá trị của các biểu thức cấp cao nhất**, nên một chương
trình ghi ra đầu ra chuẩn bằng cách gọi các hàm này. `print`/`println`/`format` đẩy đầu ra của chúng đi ở mỗi
lần gọi (để một dấu nhắc hiện ra trước khi đọc đầu vào chuẩn, ngay cả qua một pipe).

**`Option<Sexpr>` được in trong suốt.** Kiểu của dữ liệu S-expression là `Option<Sexpr>`, nên lớp bọc
`(some x)` không xuất hiện trong đầu ra và nội dung được in nguyên trạng. Danh sách rỗng được in là `()`. Các
`Option<T>` khác được in là `(some ...)` / `none`. Điều tương tự áp dụng cho các trường `Option<T>` bên trong
struct, enum và `Vector`. Một `Result<Option<Sexpr>,…>` từ `(eval ...)` được in là `(ok 42)`, hoặc `(ok ())`
với `none`.

```lisp
(println "~a" (the Option<Sexpr> (Option::some 42)))   ; => 42
(println "~a" (the Option<Sexpr> ()))                  ; => ()
(println "~a" (the Option<i32>   (Option::some 42)))   ; => (some 42)
(defstruct p (x Option<int>))
(println "~a" (p::new (Option::some 1)))               ; => #<p x: (some 1)>
(println "~a" (p::new (Option::none)))                 ; => #<p x: none>
```

```lisp
(println "~a + ~a = ~d" 1 2 3)        ; => 1 + 2 = 3
(println "[~5,'0d]" 42)               ; => [00042]
(println "~:d" 1234567)               ; => 1,234,567
(println "~@r / ~r" 2024 42)          ; => MMXXIV / forty-two
(println "~{~a~^, ~}" '(a b c))       ; => a, b, c
(println "~[zero~;one~;two~]" 1)      ; => one
(let ((s (format false "id=~d" 42)))  ; chỉ lấy chuỗi, không in
  (println "~a" s))                   ; => id=42
```

## 2. Các hàm in một đối số

Các bộ in của CLHS 22.1.3. Thay vì khai triển một định dạng, chúng in một giá trị đơn lẻ nguyên trạng. Có thể
bỏ stream (mặc định là `*standard-output*`).

| Tên | Dạng | Mô tả |
|---|---|---|
| `prin1` | `(prin1 x [stream])` | Ghi ở dạng có thể đọc ngược lại (giống `~s`) và trả về `x` |
| `princ` | `(princ x [stream])` | Ghi ở dạng cho con người (giống `~a`) và trả về `x` |
| `write` | `(write x [stream])` | `prin1` nếu `*print-escape*` là true, `princ` nếu false. Trả về `x` |
| `prin1-to-string` | `(prin1-to-string x)` | Trả về một chuỗi thay vì ghi (`~s`) |
| `princ-to-string` | `(princ-to-string x)` | Giống vậy (`~a`). Giống `to-string` |
| `write-to-string` | `(write-to-string x)` | Giống vậy, theo `*print-escape*` |

`print`/`println` **không** nằm trong số này. Chúng là các cách viết tắt của `format` nhận một chuỗi điều
khiển, một việc khác với `print` của CL (dòng mới, rồi `prin1`, rồi một dấu cách), nên mỗi hàm giữ tên riêng
của nó. Kết quả là **`print` một đối số của CL không có cách viết nào trong ngôn ngữ này**: hãy viết `prin1`.

Đây là các macro, vì các đối số biến đổi của `format` không chấp nhận biến kiểu và kiểu phải được biết tại
chỗ gọi.

## 3. Đầu vào chuẩn và các stream chuẩn

**Đọc đầu vào chuẩn** được thực hiện không phải bằng các hàm chuyên dụng mà bằng các phương thức của
`CharInput` trên stream chuẩn `*standard-input*`: `(read-line *standard-input*)` /
`(read-char *standard-input*)` / `(read-all *standard-input*)`
([các phương thức stream](streams-files.md#2-phương-thức)). Đầu ra chuẩn và lỗi chuẩn cũng tương tự có
`*standard-output*` / `*error-output*`, và có thể ghi như `(write-line *standard-output* s)`
(`print`/`println`/`format` là các lối tắt khi bạn cần khai triển định dạng, và luôn ghi ra đầu ra chuẩn).

## 4. Pretty printer

Phần này tương ứng với Lisp Pretty Printer của CL (CLHS 22.2). **Nó ngắt đầu ra không vừa độ rộng dòng, theo
các khối logic và các dòng mới có điều kiện.**

### 4.1 Các biến điều khiển

Các biến toàn cục có thể gán. Sau khi `setf`, chúng ảnh hưởng đến mọi lần in sau đó. Để thay đổi tạm thời một
biến, hãy dùng `dlet` (6.3).

| Biến | Kiểu | Mặc định | Ý nghĩa |
|---|---|---|---|
| `*print-pretty*` | `bool` | `false` | Nếu true, `~a`/`~s`/`~w` và các chỉ thị in đẹp đi theo đường in đẹp |
| `*print-right-margin*` | `int` | `80` | Lề phải (tính bằng cột). 0 nghĩa là "không có lề, không bao giờ ngắt". Giá trị âm là lỗi in |
| `*print-miser-width*` | `int` | `0` | Độ rộng mà kiểu miser bắt đầu. 0 tương ứng với `nil` của CL (tắt kiểu miser). Giá trị âm là lỗi in |

Họ `pprint` và `pprint-logical-block` luôn in đẹp bất kể `*print-pretty*` (theo định nghĩa `pprint` của CL).

### 4.2 Các bố cục dựng sẵn (dạng đặc biệt)

Giống `print`, đây là các dạng đặc biệt, nên đối số có thể thuộc kiểu bất kỳ.

| Tên | Dạng | Mô tả |
|---|---|---|
| `pprint` | `(pprint x)` | In đẹp với bố cục mặc định. Như trong CL, nó **ghi một dòng mới trước** và không có dòng mới ở cuối |
| `pprint-fill` | `(pprint-fill x)` | Lấp đầy mỗi dòng với lượng nhiều nhất có thể vừa. Không ghi dòng mới |
| `pprint-linear` | `(pprint-linear x)` | Nếu không phải mọi phần tử đều vừa trên một dòng, **mỗi phần tử một dòng**. Không ghi dòng mới |
| `pprint-tabular` | `(pprint-tabular x [colinc])` | Một bảng với các cột rộng `colinc` (mặc định 16). Không ghi dòng mới. `colinc` âm là lỗi |

Bố cục mặc định (`pprint`, và `~a` dưới `*print-pretty*`) theo `*print-pprint-dispatch*` mặc định của CL: nó
viết tắt `(quote x)` thành `'x`, và định dạng các dạng mã như `defun`/`let`/`if`/`lambda` thành "phần đầu và
số đối số quy định ở dòng đầu tiên, và phần còn lại của thân thụt vào hai cột, mỗi dạng một dòng". Các danh
sách khác được lấp đầy.

```lisp
(setf *print-right-margin* 20)
(pprint '(1 2 3 4 5 6 7 8 9 10 11 12 13 14 15))
;; =>
;; (1 2 3 4 5 6 7 8 9
;;  10 11 12 13 14 15)
(pprint '(defun f (x) i32 (+ x 1) (* x 2)))
;; =>
;; (defun f (x) i32
;;   (+ x 1)
;;   (* x 2))
```

### 4.3 Tự dựng các khối logic

| Tên | Dạng | Mô tả |
|---|---|---|
| `pprint-logical-block` | `(pprint-logical-block (obj :prefix p :per-line-prefix p :suffix s) body...)` | Một dạng đặc biệt mở một khối logic. `obj` là danh sách mà `pprint-pop` duyệt (`()` nếu không duyệt gì). `:prefix` và `:per-line-prefix` loại trừ lẫn nhau (như trong CL) |
| `pprint-newline` | `(pprint-newline kind)` | Một dòng mới có điều kiện. `kind` là `:linear` / `:fill` / `:miser` / `:mandatory` |
| `pprint-indent` | `(pprint-indent kind n)` | Thụt lề. `kind` là `:block` (từ đầu khối) / `:current` (từ cột hiện tại) |
| `pprint-tab` | `(pprint-tab kind colnum colinc)` | Một tab. `kind` là `:line` / `:section` / `:line-relative` / `:section-relative`. `colnum` và `colinc` không âm (lỗi nếu âm) |
| `pprint-pop` | `(pprint-pop)` | Lấy phần tử tiếp theo từ danh sách của khối (`()` nếu đã hết) |
| `pprint-list-exhausted` | `(pprint-list-exhausted)` | Danh sách đã hết hay chưa |
| `pprint-exit-if-list-exhausted` | `(pprint-exit-if-list-exhausted)` | Nếu đã hết, `break` ra khỏi `loop` bao ngoài (một macro) |

Khối logic không nhận đối số stream: **một khối logic đang mở là trạng thái ngầm**. `pprint-logical-block`
ngoài cùng nhất bắt đầu nó, và khi nó đóng, toàn bộ được định dạng và ghi ra đầu ra chuẩn một lần. Khi nó
đang mở, đầu ra của `print`/`println`/`(format true ...)`/`pprint` đều đi vào khối đó, nên **bạn ghi nội dung
bằng `print` thông thường và chỉ đánh dấu những chỗ cần ngắt bằng `pprint-newline` và các hàm tương tự**, làm
cho mã trông gần như giống trong CL.

Trong CL, `pprint-exit-if-list-exhausted` là một lối thoát phi cục bộ khỏi `pprint-logical-block`; ở đây nó là
**một `break` từ `loop` bao ngoài** (`pprint-logical-block` không thiết lập một `block`). Thành ngữ của CL dù
sao cũng luôn đặt nó bên trong một `loop`, nên nó đọc giống nhau.

```lisp
(setf *print-right-margin* 24)
(pprint-logical-block ('(alpha beta gamma delta epsilon zeta) :prefix "(" :suffix ")")
  (loop (pprint-exit-if-list-exhausted)
        (print "~w" (pprint-pop))
        (if (pprint-list-exhausted) () (progn (print " ") (pprint-newline :fill)))))
;; =>
;; (alpha beta gamma delta
;;  epsilon zeta)
```

Các quy tắc cho dòng mới có điều kiện (CLHS `pprint-newline`):

- `:mandatory` luôn ngắt.
- `:linear` ngắt nếu khối logic bao ngoài không vừa trên một dòng. Quyết định theo từng khối, nên **mọi dòng
  mới `:linear` của một khối ngắt cùng nhau** (đây là "tất cả trên một dòng hoặc mỗi phần tử một dòng" của
  `pprint-linear`).
- `:fill` ngắt nếu (a) đoạn tiếp theo không vừa phần còn lại của dòng, (b) đoạn trước không vừa trên một
  dòng, hoặc (c) ở kiểu miser, khối không vừa trên một dòng.
- `:miser` hoạt động như `:linear` chỉ ở kiểu miser (khi khối bắt đầu trong phạm vi `*print-miser-width*` từ
  lề phải).

## 5. `print-object` (biểu diễn in theo từng kiểu)

Viết `impl print-object <type>` làm cho `print`/`println`/`format`/`pprint` in các giá trị của kiểu đó bằng
triển khai đó, **ngay cả khi chúng lồng bên trong các danh sách**. Nó tương ứng với hàm generic
`print-object` của CL (CLHS 22.1.4).

```lisp
(deftrait print-object ()
  (print-object ((self Self) (escape bool)) string))
```

| Đối số | Ý nghĩa |
|---|---|
| `self` | Giá trị cần in |
| `escape` | `*print-escape*` của CL. `true` với `~s`/`prin1`/`pprint` (dạng có thể đọc ngược lại), `false` với `~a`/`princ` (cho con người). Một triển khai không quan tâm có thể bỏ qua nó |

`string` được trả về đi thẳng vào đầu ra. Các kiểu không có `impl` được in theo biểu diễn dựng sẵn (có dạng
`#<point x: 1 y: 2>`).

```lisp
(defstruct point (x i32) (y i32))
(impl print-object point
  (print-object ((self Self) (escape bool)) string
    (if escape (format false "#S(point :x ~d :y ~d)" self::x self::y)
               (format false "(~d,~d)" self::x self::y))))

(println "~a" p)              ; => (1,2)
(println "~s" p)              ; => #S(point :x 1 :y 2)
(println "~a" (list p q))     ; => ((1,2) (3,4))   cũng hoạt động khi lồng nhau
```

Nó cũng kết hợp được với pretty printer (chương 4). Nếu `*print-pretty*` là true, một danh sách chứa các chuỗi
mà triển khai trả về sẽ bị ngắt ở lề phải.

Các biểu diễn in của các kiểu trong thư viện chuẩn. Các kiểu cũng tồn tại trong CL được in giống như trong
SBCL. Khi REPL hiển thị một kết quả, nó dùng cùng biểu diễn như `~s`.

| Kiểu | `~s` | `~a` |
|---|---|---|
| `Vector<T>` | `#<vector<int> 1 2 3>` | Giống vậy (phần tử với `~a`) |
| `HashTable<K,V>` | `#<hashtable<string,int> count=1>` | Giống vậy |
| `Chan<T>` / `Task<T>` / `Thread<T>` | `#<chan<int> 0>` (con số là một số thứ tự nội bộ) | Giống vậy |
| `pathname` | `#P"/tmp/a.txt"` | `/tmp/a.txt` |
| `universal-time` / `internal-time` | Một số nguyên (giá trị của `get-universal-time` / `get-internal-real-time` của CL) | Giống vậy |
| Các kiểu lỗi (`ParseIntError`, `SimpleError`, v.v.) | `#<simpleerror "boom">` | Chỉ thông điệp (`boom`) |
| `complex` | `#C(1.0 2.0)` | Giống vậy |
| `Array<T>` | `#2A((0 0) (0 0))` | Giống vậy |
| Stream | `#<file-stream for "file /tmp/a.txt" {7}>`, `#<string-output-stream {5}>`, `#<two-way-stream :input-stream … :output-stream …>` | Giống vậy |
| Socket | `#<socket-stream for "socket 127.0.0.1:5000, peer: 127.0.0.1:6000" {10}>`, `#<socket-listener 0.0.0.0:8080, fd: 6 {13}>` | Giống vậy |
| `decoded-time` | `#<decoded-time 2026-09-28 13:40:24 +09:00 Mon>` (`dst` ở cuối trong giờ mùa hè) | Giống vậy |
| `heap-info` | `#<heap-info 176246 of 262144 cells live (67%), 85898 free, 2058 symbols, 5928 strings, 199 boxes, 2 collections, growable>` | Giống vậy |
| Các kiểu `defstruct` | `#<point x: 1 y: 2>` (tên trường và giá trị) | Giống vậy (các trường với `~a`) |

Các quy tắc:

- **Việc đăng ký là tĩnh.** Một `impl` được kiểm tra kiểu như một định nghĩa phương thức thông thường, nên
  một tên kiểu viết sai chính tả hoặc một chữ ký sai là lỗi biên dịch.
- **Nó cũng hoạt động với các kiểu generic.** `(impl print-object box<T> (where (print-object T)) ...)` đi
  tới một thân riêng cho mỗi đối số kiểu: một giá trị nhớ kiểu của nó gồm cả các đối số kiểu
  (`box<i32>`). Các kiểu generic dựng sẵn như `Vector<T>` hoạt động tương tự.
- **Việc chọn được thực hiện lúc in.** Chỉ thị nào tiêu thụ đối số nào phụ thuộc vào nội dung lúc chạy của
  chuỗi điều khiển, nên sự phân biệt giữa `~a` và `~s` (tức là `escape`) chỉ được biết vào lúc in. Điều này
  giống CLOS, nơi các phương thức `print-object` "được định nghĩa theo từng lớp và được chọn lúc in".
- **Vào lại quay về biểu diễn dựng sẵn.** Nếu một triển khai tự in chính nó bằng `(format false "~a" self)`,
  nó sẽ đệ quy mãi, nên khi một giá trị đang được in xuất hiện lại, biểu diễn dựng sẵn được dùng. Điều này
  nhìn vào đồng nhất giá trị, không phải giới hạn độ sâu, nên nó không cản trở việc in hợp lệ các cấu trúc
  lồng nhau tự tham chiếu.
- **Mọi kiểu vô hướng đều triển khai trait này.** Điều này là **để có thể dùng nó làm ràng buộc**: các đối
  số biến đổi của `format` không thể nhận biến kiểu, nên ràng buộc này là cách duy nhất để mã generic nói
  "các giá trị của một kiểu chưa biết có thể được hiển thị" (cùng dạng với `T: Display` của Rust).
  `print-object` của `Array<T>` là một ví dụ.
- **Với các đối số kiểu không đáp ứng ràng buộc, biểu diễn dựng sẵn được dùng một cách im lặng.**
  `(impl print-object Array<T> (where (print-object T)))` áp dụng cho `Array<i32>`, nhưng không áp dụng cho
  một `Array` có các phần tử là `defstruct` không có `print-object`. Việc chỉ tạo một mảng mà bị lỗi là vô
  nghĩa, nên nó không phải lỗi.
- Cơ chế khác của CL, `set-pprint-dispatch` / `*print-pprint-dispatch*` (một registry lúc chạy khóa theo type
  specifier), **không được áp dụng**. Các đăng ký của nó không được kiểm tra, không hợp với một ngôn ngữ có
  kiểu tĩnh.

## 6. Điều khiển lượng được in

### 6.1 Độ sâu, độ dài và chia sẻ

Các biến điều khiển của CLHS 22.1.1 quyết định "một giá trị được in bao nhiêu". Giống ba biến ở 4.1, chúng là
các biến toàn cục có thể gán, và chúng áp dụng cho tất cả `print`/`println`/`format`/`pprint`, dù
`*print-pretty*` là true hay không.

| Biến | Kiểu | Mặc định | Ý nghĩa |
|---|---|---|---|
| `*print-level*` | `int` | `0` | Các đối tượng lồng ở độ sâu này hoặc sâu hơn được thay bằng `#`. Đối tượng đang được in ở độ sâu 0. 0 nghĩa là không giới hạn |
| `*print-length*` | `int` | `0` | In các phần tử danh sách (và các trường của giá trị `defstruct`/`defenum`) tối đa chừng này và thay phần còn lại bằng `...`. 0 nghĩa là không giới hạn |
| `*print-circle*` | `bool` | `false` | Nếu true, giá trị được quét trước khi in và **các đối tượng xuất hiện hai lần trở lên nhận nhãn**. Lần xuất hiện đầu tiên là `#n=…` và các lần sau là `#n#` |

CL dùng `nil` cho "không giới hạn", nhưng ngôn ngữ này không có `nil`, nên như với `*print-right-margin*`,
**0 nghĩa là không giới hạn**. Giá trị âm không có ý nghĩa và là lỗi in. Các giá trị mặc định đều là "không giới
hạn / không nhãn", khớp với giá trị khởi tạo của CL.

```lisp
(setf *print-level* 2)
(println "~a" '(1 (2 (3 (4)))))   ; => (1 (2 #))
(setf *print-level* 0)

(setf *print-length* 4)
(println "~a" '(1 2 3 4 5 6))     ; => (1 2 3 4 ...)
(setf *print-length* 0)
```

**Các cấu trúc vòng chỉ có thể được in khi `*print-circle*` là true.** Nếu bạn in một giá trị trỏ vào chính nó
trong khi nó là false (mặc định), bộ in cứ đi theo vòng và tiến trình bị crash. CL cũng vậy (CLHS để việc in
cấu trúc vòng là không xác định khi `*print-circle*` là false).

Một vòng chỉ có thể được tạo bằng cách "cho một trường `defstruct` trỏ vào chính nó bằng `setf`" (các ô
`Sexpr` không thể thay đổi sau khi tạo, nên một danh sách như `'(1 2 3)` không bao giờ có thể vòng):

```lisp
(defstruct node (val int) (next Option<node>))

(let ((a (node::new 1 (Option::none))))
  (setf a::next (Option::some a))   ; a trỏ vào chính a
  (setf *print-circle* true)
  (println "~a" a))                 ; => #1=#<node val: 1 next: (some #1#)>
```

Các nhãn **bắt đầu lại từ 1 cho mỗi thứ được in** (như trong CL). Ngay cả khi không có vòng, nếu cùng một đối
tượng xuất hiện hai lần thì nó nhận `#1=`/`#1#`, giữ lại trong đầu ra thông tin rằng "hai thứ này là cùng một
đối tượng", như CL quy định:

```lisp
(setf *print-circle* true)
(let ((x '(1 2)))
  (println "~a" (list x x)))        ; => (#1=(1 2) #1#)
```

Một giá trị không có chia sẻ **không hiển thị nhãn nào**, nên để biến này là true không làm thay đổi đầu ra
của mã hằng ngày.

### 6.2 Cơ số, hoa thường và khả năng đọc lại

| Biến | Kiểu | Mặc định | Ý nghĩa |
|---|---|---|---|
| `*print-base*` | `int` | `10` | Cơ số để in số nguyên (độ rộng cố định và `int`). Ngoài 2 đến 36 là **lỗi in** (CL cũng quy định phạm vi) |
| `*print-radix*` | `bool` | `false` | Nếu true, thêm một dấu cơ số: `#b`/`#o`/`#x`, `#NNr` cho các cơ số khác, và một dấu `.` ở cuối cho cơ số 10. Dấu đứng **trước** dấu âm/dương (`#x-ff`) |
| `*print-case*` | `symbol` | `:downcase` | Hoa thường của tên symbol: `:upcase` / `:downcase` / `:capitalize` (cùng cách viết như CL). Symbol khác là lỗi in |
| `*print-readably*` | `bool` | `false` | Nếu true, in ở dạng có thể đọc ngược lại. Nó ép escape và tắt các giới hạn cắt của `*print-level*`/`*print-length*` |
| `*print-lines*` | `int` | `0` | Số dòng mà pretty printer được dùng. Phần thừa bị cắt, với `..` ở cuối như trong CL. 0 nghĩa là không giới hạn. Giá trị âm là lỗi in |
| `*print-escape*` | `bool` | `true` | Việc `write`/`write-to-string` làm `prin1` hay `princ`. **Chỉ hai hàm đó đọc nó** |
| `*print-array*` | `bool` | `true` | Việc `Array<T>` có hiển thị nội dung hay không. Nếu true, cú pháp mảng của CL (`#(1 2 3)` / `#2A((1 2) (3 4))`); nếu false, chỉ hình dạng, `#<array 2x3>` |

```lisp
(dlet ((*print-base* 16)) (format false "~a" 255))                    ; => "ff"
(dlet ((*print-base* 16) (*print-radix* true)) (format false "~a" 255)) ; => "#xff"
(dlet ((*print-case* :upcase)) (format false "~a" 'hello))            ; => "HELLO"
```

Các dấu mà `*print-radix*` thêm vào có thể được reader đọc ngược lại (ký hiệu cơ số trong
[Tham chiếu cú pháp](../syntax.md#1-cú-pháp-từ-vựng)).

**Vì sao mặc định của `*print-case*` khác CL**: mặc định của CL là `:upcase` vì reader của CL lưu tên symbol
bằng chữ hoa, tức là nó có nghĩa "như đã lưu". Reader này lưu chúng bằng chữ thường, nên mặc định có cùng ý
nghĩa là `:downcase`.

**Nửa còn thiếu của `*print-readably*`**: CL báo `print-not-readable` cho các giá trị không thể đọc ngược
lại, nhưng ngôn ngữ này không có condition để báo, và không có cách quyết định khả năng đọc lại cho các kiểu
của người dùng, mà `print-object` có thể in theo bất kỳ cách nào. Chỉ có việc ép escape và việc ghi đè các
giới hạn cắt.

**Vì sao chỉ `write` đọc `*print-escape*`**: như CLHS quy định, `~s`/`prin1`/`pprint` ràng buộc nó là true, và
`~a`/`princ` là false, mỗi hàm chỉ trong thời gian lời gọi của chính nó. Vì vậy những bên đọc duy nhất thấy
nó không bị ràng buộc là `write`/`write-to-string`. Một triển khai `print-object` nên đọc đối số `escape` của
chính nó thay vì biến toàn cục này: đối số đó mang giá trị mà chỉ thị đã chọn.

**Những gì CL có mà ngôn ngữ này không có**: `*print-gensym*` (không có symbol không được intern).

### 6.3 Ghi đè tạm thời

CL ràng buộc các biến này bằng `let`, nhưng `let` trong ngôn ngữ này ràng buộc theo phạm vi từ vựng, nên hãy
dùng `dlet` ([Khác](system.md#10-khác)):

```lisp
(dlet ((*print-level* 2) (*print-length* 4))
  (println "~a" x))                 ; các giới hạn chỉ áp dụng cho lần in này
(with-standard-io-syntax (println "~a" x))   ; in với mọi thứ trở lại giá trị chuẩn
```

`with-standard-io-syntax` chạy thân của nó với mọi biến điều khiển bộ in ở giá trị chuẩn và `*read-eval*` đặt
là `true`.
