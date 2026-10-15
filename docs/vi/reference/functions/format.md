<!-- translated-from: docs/ja/reference/functions/format.md @ 7aae962c61e6f36448edc8bab6b37ff177aefdb3 -->
# Các chỉ thị định dạng

Các chỉ thị được viết trong chuỗi điều khiển của `print`/`println`/`format`. Chúng bao phủ gần hết các chỉ
thị `format` của CL. Bản thân các hàm được mô tả ở [In](printing.md#1-print--println--format).

## 1. Cách viết chỉ thị

Mỗi chỉ thị là `~`, rồi các **tham số tiền tố** tùy chọn (phân tách bằng dấu phẩy: một số nguyên / `'c`
(một ký tự) / `v` (lấy từ đối số tiếp theo) / `#` (số đối số còn lại)), rồi các **bộ điều chỉnh** tùy chọn
`:` và `@`, rồi ký tự chỉ thị, theo thứ tự đó. Ký tự chỉ thị không phân biệt hoa thường.

Chuỗi điều khiển phải là một literal ([In](printing.md#1-print--println--format)). Ngoài ra, những điều
sau được kiểm tra lúc kiểm tra.

- **Số lượng và kiểu của đối số.** Với mỗi chỉ thị tiêu thụ một đối số: còn đối số hay không, và kiểu của
  nó có được chấp nhận hay không (các ghi chú "đối số" trong các bảng bên dưới). Ở những chỗ đường đi phụ
  thuộc vào giá trị lúc chạy, như di chuyển bằng `~*`, mệnh đề nào của `~[` được chọn, `~^` có kích hoạt
  hay không, hoặc `~@{` lặp bao nhiêu lần, **mọi đường đi** đều được kiểm tra. Đối số thừa thì không sao
  (như trong CL).
- **Tham số và bộ điều chỉnh.** Một bộ điều chỉnh không được chấp nhận, quá nhiều tham số, và các giá trị
  ngoài phạm vi (độ rộng âm, cơ số khác 2 đến 36, một số nguyên ở chỗ cần ký tự, v.v.) là lỗi. Chúng không
  bao giờ bị bỏ qua hay làm tròn một cách im lặng.

Các **phần tử** của đối số danh sách (`~{`, `~:{`, `~<...~:>`) là `Sexpr`; không thể biết số lượng lẫn kiểu
của từng phần tử từ các kiểu. Yêu cầu đối với phần tử (ký tự cho `~c`, v.v.) và việc thiếu phần tử được
kiểm tra khi giá trị đến và là lỗi thời gian chạy (không bao giờ chuyển sang cách biểu diễn khác thay
thế).

`~d` `~b` `~o` `~x` và `~r` có cơ số in giá trị không nguyên như `~a` sẽ in, ở hệ thập phân, giống CL
(`(format false "~5d" 1/2)` là `"  1/2"`). Các quy tắc dễ dãi khác của CL không được áp dụng: việc
`~:[` coi mọi giá trị là giá trị logic không được diễn giải lại như vậy; đó là lỗi kiểu.

## 2. Đầu ra (tiêu thụ một đối số)

| Chỉ thị | Tham số / bộ điều chỉnh | Ý nghĩa |
|---|---|---|
| `~a` | `~mincol,colinc,minpad,padchar` / `@`=căn phải | Dạng thẩm mỹ (`princ` của CL; chuỗi không có dấu nháy). Đối số có thể thuộc kiểu bất kỳ |
| `~s` | Như trên | Dạng chuẩn (`prin1` của CL; dạng có thể đọc ngược lại). Đối số có thể thuộc kiểu bất kỳ |
| `~w` | — | `write` của CL. In đẹp nếu `*print-pretty*` là true, nếu không thì giống `~s` |
| `~d` `~b` `~o` `~x` | `~mincol,padchar,commachar,interval` / `:`=nhóm chữ số, `@`=luôn có dấu | Số nguyên thập phân/nhị phân/bát phân/thập lục phân. Đối số có thể là bất kỳ (giá trị không nguyên được in như với `~a`, ở hệ thập phân) |
| `~r` | `~radix,mincol,padchar,commachar,interval` (với một cơ số) hoặc không có | Với cơ số, dùng cơ số đó (2 đến 36). Không có cơ số: `~r`=số đếm tiếng Anh, `~:r`=số thứ tự tiếng Anh, `~@r`=chữ số La Mã, `~:@r`=chữ số La Mã cổ. Đối số là một số nguyên |
| `~p` | `:`=lùi lại một, `@`=y/ies | Số nhiều (`~p`→"s", `~@p`→"y"/"ies"). Đối số là một số nguyên |
| `~c` | `:`=tên, `@`=cú pháp `#\` | Một ký tự. Đối số là một `char` |
| `~f` | `~w,d,k,overflowchar,padchar` / `@`=dấu | Dấu phẩy tĩnh. Đối số là một số |
| `~e` | `~w,d,,,,padchar,exptchar` / `@`=dấu | Ký hiệu mũ. Đối số là một số. Các tham số exponent-digits, scale và overflowchar của CL không được hỗ trợ (đưa chúng ra là lỗi) |
| `~g` | `@`=dấu | Dấu phẩy động tổng quát. Đối số là một số. Không nhận tham số |
| `~$` | `~d,n,w,padchar` / `:`,`@` | Ký hiệu tiền tệ. Đối số là một số |

## 3. Đầu ra (không tiêu thụ đối số)

| Chỉ thị | Ý nghĩa |
|---|---|
| `~%` | Xuống dòng (`~n%` cho n lần) |
| `~&` | fresh-line (xuống dòng trừ khi đang ở đầu dòng; `~n&`) |
| `~\|` | Ngắt trang (form feed) |
| `~~` | Một `~` nguyên văn (`~n~` cho n lần) |
| `~t` | Tab (`~colnum,colincT`. Nếu đã ở cột colnum hoặc quá, di chuyển tiếp theo bội số của colinc; không di chuyển nếu colinc là 0. `@`=tương đối. `:`=tab tương đối so với đầu khối logic, chỉ hoạt động khi in đẹp) |
| `~_` | Xuống dòng có điều kiện (in đẹp; trơn=`:linear` / `~:_`=`:fill` / `~@_`=`:miser` / `~:@_`=`:mandatory`) |
| `~i` | Thụt lề (in đẹp; `~ni`=đầu khối + n / `~n:i`=cột hiện tại + n) |
| `~<newline>` | Bỏ qua dòng mới (`:`=giữ khoảng trắng, `@`=giữ dòng mới) |

Như trong CL, các chỉ thị in đẹp (`~_` `~i` `~:t` `~<...~:>`, và đường đi in đẹp của `~a`/`~s`/`~w`) đều không
làm gì khi `*print-pretty*` là false. Mặc định nó là false.

## 4. Các cấu trúc điều khiển

| Chỉ thị | Ý nghĩa |
|---|---|
| `~(...~)` | Chuyển đổi chữ hoa/thường (`~(` chữ thường, `~:(` viết hoa chữ đầu mỗi từ, `~@(` chỉ viết hoa từ đầu tiên, `~:@(` toàn bộ chữ hoa) |
| `~[...~;...~]` | Chọn có điều kiện (rẽ nhánh theo một đối số số nguyên. Với `~n[`, `~v[` hoặc `~#[`, nó rẽ nhánh theo giá trị đó và không nhận đối số. `~:;`=mệnh đề mặc định, chỉ ở vị trí mệnh đề cuối). `~:[false~;true~]` rẽ nhánh theo một đối số `bool` và có đúng hai mệnh đề |
| `~{...~}` | Lặp (duyệt một đối số danh sách. `~:{`=theo từng danh sách con, `~@{`=trên các đối số còn lại, `~:@{`=trên từng danh sách trong các đối số còn lại, `~^`=thoát, `~:}`=chạy một lần ngay cả khi rỗng). Một thân không tiêu thụ đối số nào trong một lần lặp là lỗi (nó sẽ không bao giờ kết thúc) |
| `~<...~;...~>` | Căn chỉnh (trải các đoạn trên `~mincol` cột. `:`/`@`=phần đệm ở hai đầu) |
| `~<...~;...~:>` | **Khối logic** (đóng bằng `~:>`; khác với căn chỉnh ở trên). Đoạn đầu là tiền tố và đoạn cuối là hậu tố (cả hai chỉ là chuỗi nguyên văn). Với dấu phân cách `~@;`, tiền tố là **tiền tố theo từng dòng**. `~:<` mặc định tiền tố/hậu tố là `(`/`)`. Đối số là một danh sách (`~@<` dùng các đối số còn lại tại chỗ) |
| `~*` | Bỏ qua đối số (`~n*`=tiến n, `~:*`=lùi, `~@*`=đến một vị trí tuyệt đối) |
| `~/name/` | Gọi hàm hoặc phương thức (chương 5; cờ `:`/`@` được truyền cho đích gọi; không nhận tham số) |

Các chỉ thị CL sau không được hỗ trợ (chúng là lỗi lúc kiểm tra).

- `~?` và `~@?`: chúng nhận một chuỗi điều khiển làm đối số lúc chạy, nên các đối số mà chỉ thị của nó
  tiêu thụ không thể được kiểm tra. Hãy viết các chỉ thị đó trực tiếp trong chuỗi điều khiển.
- `~@[...~]`: nó kiểm tra xem một đối số có khác nil hay không, nhưng ngôn ngữ này không có nil. Hãy dùng
  `~:[false~;true~]`, rẽ nhánh theo một `bool`.
- `~{~}` với thân rỗng: nó lấy thân từ một đối số lúc chạy. Hãy viết các chỉ thị bên trong cặp ngoặc nhọn.

## 5. `~/name/`

`~/name/` phân giải `name` theo cùng quy tắc như một lời gọi `(name arg colon at)` viết tại vị trí đó, và xuất
chuỗi mà lời gọi trả về. Nó tiêu thụ một đối số. `colon` và `at` là các giá trị `bool` cho biết chỉ thị có `:`
và `@` hay không.

```lisp
(defstruct point (x i32) (y i32))
(impl point
  (brief (self (colon bool) (at bool)) string
    (if colon (format false "<~a,~a>" self::x self::y) (format false "~a/~a" self::x self::y))))
(defun yen ((n int) (colon bool) (at bool)) string
  (format false "~a yen" n))

(println "~a" (format false "~/brief/"  (make-instance 'point :x 3 :y 4)))   ; => 3/4
(println "~a" (format false "~:/brief/" (make-instance 'point :x 3 :y 4)))   ; => <3,4>
(println "~a" (format false "~/yen/" 500))                                    ; => 500 yen
```

- Việc phân giải tên giống như với lời gọi ([Tham chiếu cú pháp 3.10](../syntax.md#310-module--use--không-gian-tên)). Nếu kiểu của đối số có
  phương thức `name` thì gọi nó; nếu không, gọi hàm `name` nhìn thấy được. Hàm trong module có thể được
  viết bằng đường dẫn (`~/m::shout/`).
- Thứ có thể gọi là hàm hoặc phương thức viết bằng typelisp, nhận nguyên đối số, `colon` và `at`, và trả về
  `string`. Không gọi được hàm dựng sẵn.
- **Gọi gì được xác định lúc kiểm tra.** Bộ kiểm tra tìm mọi đối số có thể đến được `~/name/` đó, tính cả
  các bước nhảy của `~*` và `~[`, rồi phân giải tên theo kiểu của từng đối số. Nếu có một cái không phân
  giải được thì là lỗi lúc kiểm tra, và trường hợp các đối số có thể đến được sẽ gọi hai định nghĩa khác
  nhau cũng vậy. Lúc chạy không tìm tên nào, nên nó chạy giống hệt trong tệp thực thi AOT.
- **Không dùng được bên trong `~{`, `~:{`, `~:@{` hay `~<...~:>`** (lỗi lúc kiểm tra). Kiểu của phần tử
  trong đối số danh sách là `Sexpr`, không xác định được cần gọi gì. `~@{` duyệt các đối số còn lại của lời
  gọi, có kiểu đã biết, nên dùng được ở đó.

Khác biệt với CL: CL tìm tên như một hàm toàn cục. Ở đây việc phân giải giống như với lời gọi, nên phương thức
của kiểu đối số được chọn trước hàm cùng tên.
