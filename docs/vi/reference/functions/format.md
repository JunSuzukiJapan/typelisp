<!-- translated-from: docs/ja/reference/functions/format.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
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

Các **phần tử** của một đối số danh sách (`~{`, `~:{`, `~<...~:>`) là các `Sexpr`, và cả số lượng lẫn kiểu
của từng phần tử đều không thể biết từ các kiểu. Các yêu cầu đối với phần tử (một số nguyên cho `~d`, v.v.)
và phần tử bị thiếu được kiểm tra khi các giá trị đến, và là lỗi lúc chạy (nó không bao giờ chuyển sang một
biểu diễn khác để thay thế).

Các quy tắc dễ dãi của CL không được áp dụng. Truyền một giá trị không phải số nguyên cho `~d` và để nó
được in như `~a`, hay `~:[` coi mọi giá trị là boolean, đều không được diễn giải lại như vậy; chúng là lỗi
kiểu.

## 2. Đầu ra (tiêu thụ một đối số)

| Chỉ thị | Tham số / bộ điều chỉnh | Ý nghĩa |
|---|---|---|
| `~a` | `~mincol,colinc,minpad,padchar` / `@`=căn phải | Dạng thẩm mỹ (`princ` của CL; chuỗi không có dấu nháy). Đối số có thể thuộc kiểu bất kỳ |
| `~s` | Như trên | Dạng chuẩn (`prin1` của CL; dạng có thể đọc ngược lại). Đối số có thể thuộc kiểu bất kỳ |
| `~w` | — | `write` của CL. In đẹp nếu `*print-pretty*` là true, nếu không thì giống `~s` |
| `~d` `~b` `~o` `~x` | `~mincol,padchar,commachar,interval` / `:`=nhóm chữ số, `@`=luôn có dấu | Số nguyên thập phân/nhị phân/bát phân/thập lục phân. Đối số là một số nguyên |
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
| `~/name/` | Gọi phương thức (chương 5. Các cờ `:`/`@` được truyền cho phương thức. Không nhận tham số) |

Các chỉ thị CL sau không được hỗ trợ (chúng là lỗi lúc kiểm tra).

- `~?` và `~@?`: chúng nhận một chuỗi điều khiển làm đối số lúc chạy, nên các đối số mà chỉ thị của nó
  tiêu thụ không thể được kiểm tra. Hãy viết các chỉ thị đó trực tiếp trong chuỗi điều khiển.
- `~@[...~]`: nó kiểm tra xem một đối số có khác nil hay không, nhưng ngôn ngữ này không có nil. Hãy dùng
  `~:[false~;true~]`, rẽ nhánh theo một `bool`.
- `~{~}` với thân rỗng: nó lấy thân từ một đối số lúc chạy. Hãy viết các chỉ thị bên trong cặp ngoặc nhọn.

## 5. `~/name/`

**Một điểm khác với CL: tên được tra không phải như một hàm toàn cục mà như một phương thức của chính kiểu
của đối số.** Phương thức có dạng `((self Self) (colon bool) (at bool)) → string`, và các `:`/`@` của chỉ thị
được truyền nguyên trạng.

Cách tra như một hàm toàn cục của CL không thể được triển khai an toàn trong ngôn ngữ này. Ngay cả với một
chuỗi điều khiển literal, kiểu của các phần tử của một đối số danh sách (bên trong `~{`) không được biết lúc
kiểm tra, và việc tra hàm chỉ bằng tên có thể gọi một hàm dành cho kiểu khác. Chọn theo kiểu của giá trị có
nghĩa là phương thức được kiểm tra kiểu đúng cho kiểu đó, như vậy là an toàn (cùng cơ chế với
`print-object`). Nó cũng hoạt động với các giá trị như `string`/`bool`/`char`/`symbol`/danh sách. Chỉ với
số nguyên, vốn không thể biết độ rộng từ giá trị, thì là lỗi **khi có nhiều hơn một kiểu số nguyên định
nghĩa phương thức tên đó**.

Không biết nó áp dụng cho đối số nào, nhưng biết nó có thể gọi những phương thức nào. Bộ kiểm tra thu thập
mọi `~/name/` từ chuỗi điều khiển literal và ghi lại, trong số các kiểu của đối số tại vị trí gọi đó, những
kiểu có một phương thức dạng trên. Vì vậy **nếu không kiểu đối số nào có phương thức đó, đó là lỗi lúc kiểm
tra** (không phải lúc chạy), và nó cũng hoạt động trong các tệp thực thi AOT.

```lisp
(defstruct point (x i32) (y i32))
(defmethod brief ((self point) (colon bool) (at bool)) string
  (if colon (format false "<~a,~a>" self::x self::y) (format false "~a/~a" self::x self::y)))
(println "~a" (format false "~/brief/"  (point::new 3 4)))   ; => 3/4
(println "~a" (format false "~:/brief/" (point::new 3 4)))   ; => <3,4>
```
