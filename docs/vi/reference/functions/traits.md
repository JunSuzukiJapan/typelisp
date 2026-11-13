<!-- translated-from: docs/ja/reference/functions/traits.md @ cedb434055399f2f83e17dcdaa55007fe4fc9dad -->
# Các trait chuẩn

Các trait cho việc lặp, so sánh, số học và thêm, lấy ra phần tử. Các trait chuẩn khác nằm ở các chương riêng của chúng: `Hash`
([HashTable](collections.md#4-hashtablekv)), `Error`
([Các kiểu lỗi](option-result.md#3-các-kiểu-lỗi-và-trait-error)), `print-object`
([In](printing.md#5-print-object-biểu-diễn-in-theo-từng-kiểu)), và các trait của stream cùng
`Pathish` ([Stream và tệp](streams-files.md)). Kiểu nào triển khai trait nào nằm ở
[Kiểu](../types.md). Cách định nghĩa trait nằm ở
[Tham chiếu cú pháp](../syntax.md#39-deftrait--impl--trait).

## 1. Trait `Iter` và việc lặp

```lisp
(deftrait Iter ()
  (:type Item)
  (defmethod next (self) Option<Item>))
```

`Vector<T>`/`HashTable<K,V>`/`Array<T>` triển khai `Iter` lần lượt qua `vector-iter<T>`/
`hashtable-iter<K,V>`/`array-iter<T>` (lấy iterator bằng `(iter collection)`).
`Chan<T>` tự nó là một `Iter` (`recv` đóng vai trò của `next`; [Kênh](concurrency.md#2-chant--kênh)).
Các danh sách `Sexpr` không triển khai `Iter` (kiểu phần tử của chúng không đồng nhất). Nếu bạn triển
khai `Iter` cho kiểu của riêng mình, nó có thể được duyệt bằng `doiter` nguyên trạng, và được truyền cho
[các hàm trên dãy](sequences.md#4-các-hàm-trên-dãy-dựa-trên-iter).

## 2. `Equal` / `Ord` (so sánh)

`Equal` là trait mà `equal` dùng, tương ứng với `PartialEq` của Rust. `Ord` tương ứng với `PartialOrd` của
Rust. Chúng được dùng trong ràng buộc `:where` của hàm generic để yêu cầu kiểu phần tử so sánh được.

```lisp
(deftrait Equal ()
  (defmethod equal (self (other Self)) bool))                ; bắt buộc cài đặt
(deftrait Ord (Equal)                                     ; kế thừa Equal
  (defmethod less (self (other Self)) bool)                  ; bắt buộc cài đặt
  (defmethod less-equal (self (other Self)) bool (not (less other self)))
  (defmethod greater (self (other Self)) bool (less other self))
  (defmethod greater-equal (self (other Self)) bool (not (less self other))))
```

Để cài đặt `Equal` thì viết `equal`, còn `Ord` chỉ cần `less`. Các cài đặt mặc định điền phần còn lại của
`Ord`. `Ord` kế thừa `Equal`, nên cần `impl Equal X` trước `impl Ord X`.

`(equal a b)` gọi cài đặt `Equal` của kiểu của `a`. Viết nó cho một kiểu không cài đặt `Equal` là lỗi kiểu.
Để so sánh theo định danh dùng `eq`/`eql`, theo cấu trúc dùng `equalp`. Ba hàm này dùng được với giá trị
thuộc bất kỳ kiểu nào.

Mỗi phương thức của trait có thể gọi trực tiếp như hàm (bên trong ràng buộc `where (Equal A)`/`(Ord A)`,
hoặc trên một kiểu cụ thể đã cài đặt nó):

| Tên | Dạng | Kiểu | Mô tả |
|---|---|---|---|
| `equal` | `(equal a b)` | `(A,A)→bool` với `Equal A` | Có bằng nhau không (`==` của Rust) |
| `less` | `(less a b)` | `(A,A)→bool` với `Ord A` | `a < b` |
| `less-equal` | `(less-equal a b)` | `(A,A)→bool` với `Ord A` | `a <= b` |
| `greater` | `(greater a b)` | `(A,A)→bool` với `Ord A` | `a > b` |
| `greater-equal` | `(greater-equal a b)` | `(A,A)→bool` với `Ord A` | `a >= b` |

`Equal` được cài đặt cho: mọi kiểu số (`i8` đến `u32` / `f32` / `f64` / `int` / `ratio`), `bool` `char`
`string` `symbol` `complex`, `Sexpr`, `cons-cell<A,B>` và các tuple (từng phần tử, khi các phần tử là
`Equal`), cùng `Option<T>` và `Result<T,E>` (khi cùng biến thể và nội dung `equal`). Với các kiểu dựng sẵn,
phương thức dựng sẵn `equal` của chính kiểu đó là cài đặt `Equal`. Số so sánh bằng `=`, chuỗi so sánh theo
nội dung, phân biệt hoa thường. `Ord` được cài đặt cho: mọi kiểu số, `char` `string` và `cons-cell<A,B>`
(theo thứ tự từ điển, khi các phần tử là `Ord`).

`equal` của một danh sách (`Sexpr`) là `equal` của CL: cons so sánh theo cấu trúc, chuỗi theo nội dung, và
mọi atom khác bằng `eql`. Phép so sánh này duyệt các giá trị lúc chạy và không gọi phương thức nào. Vì thế
một struct nằm trong danh sách được so sánh theo định danh như trong CL, bất kể kiểu của nó có
`impl Equal` nào.

Tên các phương thức của `Ord` không trùng với các toán tử dựng sẵn (`< <= > >=`) hay `lt` vì không thể định
nghĩa lại cái dựng sẵn và mỗi cài đặt ủy quyền cho chúng. Bản thân các toán tử so sánh vô hướng là phương
thức dựng sẵn theo kiểu đối tượng nhận ([Số](numbers.md), [Chuỗi và ký tự](collections.md)). Bên trong ràng
buộc, toán tử được viết sẽ được đọc là phương thức của trait (chương 3). `=` trở thành `equal`, còn `/=` trở
thành `(not (equal a b))`.

## 3. Các trait số học (`Add` / `Sub` / `Mul` / `Div` / `Rem` / `Bits` / `Number`)

Một tầng để mã generic yêu cầu "một kiểu có thể cộng được". **Số học trên các kiểu cụ thể dùng các toán
tử dựng sẵn** ([Số](numbers.md)) và không đi qua tầng này.

```lisp
(deftrait Add () (defmethod add (self (other Self)) Self))
(deftrait Sub () (defmethod sub (self (other Self)) Self))
(deftrait Mul () (defmethod mul (self (other Self)) Self))
(deftrait Div () (:type Output) (defmethod div (self (other Self)) Output))
(deftrait Rem () (defmethod remainder (self (other Self)) Self))
(deftrait Bits ()
  (defmethod bit-and (self (other Self)) Self)
  (defmethod bit-or  (self (other Self)) Self)
  (defmethod bit-xor (self (other Self)) Self)
  (defmethod bit-not (self) Self)
  (defmethod shift (self (count int)) Self))                 ; khoảng dịch luôn là int (như với ash)
(deftrait Number (Add Sub Mul Rem Ord))                   ; không có phương thức; tổ hợp của năm trait
```

**Bên trong một ràng buộc, bạn có thể viết toán tử.** Khi bên nhận là một biến kiểu bị ràng buộc bởi
`:where`, các toán tử được hiểu là phương thức của trait (`+`→`add`, `-`→`sub`, `*`→`mul`, `/`→`div`,
`rem`→`remainder`, `logand`→`bit-and`, `=`→`equal`, `<`→`less` …):

```lisp
(defun sum3<T> ((a T) (b T) (c T)) T (:where (Number T))
  (+ a b c))                                              ; = (add (add a b) c)
```

Phương thức của trait không được đặt tên là `+` vì `+` là tên của một phương thức dựng sẵn và `impl` từ
chối định nghĩa lại nó (`cannot redefine built-in method`). Không có `Neg`: `(- x)` khai triển thành
`(- (- x x) x)`, nên `Sub` là đủ.

Được triển khai cho: `Add`/`Sub`/`Mul`/`Div`/`Rem`/`Number` trên mọi kiểu số (trừ `complex`), và `Bits`
trên mọi kiểu số nguyên và `int`.


**`Div` đặt tên kiểu kết quả bằng kiểu liên kết `Output`.** Như CL, `/` trên hai số nguyên là
`rational`, nên `Output` của `int` là `rational`, còn của mọi kiểu số khác là chính kiểu đó. Vì `Output`
khác nhau giữa các kiểu, `Number` không chứa `Div` (danh sách supertrait phải cố định mọi kiểu liên
kết). Hàm tổng quát có phép chia nhận kiểu kết quả bằng `(:where (Div T (Output R)))`, hoặc yêu cầu phép
chia khép kín trong `T` bằng `(:where (Div T (Output T)))` (khi đó không truyền được `int`).

## 4. Trait `Push` (thêm phần tử)

```lisp
(deftrait Push ()
  (:type Item)
  (defmethod push (self (x Item)) ()))
```

Một tập hợp có thể thêm phần tử vào từng cái một. Hàm generic yêu cầu "một kiểu có thể thêm `T` vào"
bằng `(:where (Push C (Item T)))` và thêm vào bất kỳ tập hợp nào như vậy bằng cùng một `(push c x)`.

| Kiểu | `push` làm gì |
|---|---|
| `Vector<T>` | Thêm vào cuối |
| `Deque<T>` | Thêm vào cuối (`push-back`) |
| `Array<T>` | Thêm tại vị trí fill pointer (`push-extend`). Panic nếu không có fill pointer |
| `HashSet<T>` | Đưa vào tập hợp (`insert`) |

```lisp
(use std::collections::Deque)
(defun push-all<C,T> ((c C) (xs Vector<T>)) () (:where (Push C (Item T)))
  (doiter (x (iter xs)) (push c x)))

(let ((v (the Vector<int> (Vector::new)))
      (d (the Deque<int> (Deque::new))))
  (push-all v (the Vector<int> #(1 2)))
  (push-all d (the Vector<int> #(3 4)))
  (println "~a ~a" v d))                                  ; #(1 2) #<deque 3 4>
```

Một kiểu tự định nghĩa có triển khai `Push` có thể được truyền cho cùng các hàm generic đó. Nó cũng
có thể là một trait object, như `:dyn Push<int>`.

## 5. Trait `Pop` (lấy phần tử ra)

```lisp
(deftrait Pop ()
  (:type Item)
  (defmethod pop (self) Option<Item>))
```

Tập hợp có thể lấy lại phần tử mà `push` thêm vào sau cùng. Trả về `none` khi rỗng. Hàm generic yêu
cầu nó bằng `(:where (Pop C (Item T)))`.

| Kiểu | `pop` làm gì |
|---|---|
| `Vector<T>` | Lấy ra phần tử cuối |
| `Deque<T>` | Lấy ra phần tử cuối (`pop-back`) |
| `Array<T>` | Lấy ra phần tử ngay trước fill pointer. Panic nếu không có fill pointer |

`HashSet<T>` không có "phần tử cuối", nên nó là `Push` nhưng không phải `Pop`.

```lisp
(use std::collections::Deque)
(defun pop-all<C,T> ((c C)) Vector<T> (:where (Pop C (Item T)))
  (let ((out (the Vector<T> (Vector::new))) (more true))
    (while more
      (match (pop c)
        ((some x) (push out x))
        ((none) (progn (setf more false) ()))))
    out))

(let ((v (the Vector<int> #(1 2 3)))
      (d (the Deque<int> (Deque::new))))
  (push d 1) (push d 2)
  (println "~a ~a" (pop-all v) (pop-all d)))              ; #(3 2 1) #(2 1)
```

Giống `Push`, kiểu của bạn có thể cài đặt nó, và nó có thể là trait object, như `:dyn Pop<int>`.
