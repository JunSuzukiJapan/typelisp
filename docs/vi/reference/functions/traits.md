<!-- translated-from: docs/ja/reference/functions/traits.md @ 9a4e7d23b09eb3f3244bfb876df1ec57576d6874 -->
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
  (next (self) Option<Item>))
```

`Vector<T>`/`HashTable<K,V>`/`Array<T>` triển khai `Iter` lần lượt qua `vector-iter<T>`/
`hashtable-iter<K,V>`/`array-iter<T>` (lấy iterator bằng `(iter collection)`).
`Chan<T>` tự nó là một `Iter` (`recv` đóng vai trò của `next`; [Kênh](concurrency.md#2-chant--kênh)).
Các danh sách `Sexpr` không triển khai `Iter` (kiểu phần tử của chúng không đồng nhất). Nếu bạn triển
khai `Iter` cho kiểu của riêng mình, nó có thể được duyệt bằng `doiter` nguyên trạng, và được truyền cho
[các hàm trên dãy](sequences.md#4-các-hàm-trên-dãy-dựa-trên-iter).

## 2. `Eq` / `Ord` (so sánh)

Chúng tương ứng với `PartialEq`/`PartialOrd` của Rust (được đặt tên `Eq`/`Ord`). Chúng được dùng trong
các ràng buộc `:where` của hàm generic để yêu cầu các kiểu phần tử có thể so sánh được
(`sort`/`member`/`assoc` v.v.).

```lisp
(deftrait Eq ()
  (equals (self (other Self)) bool)                ; phải được triển khai
  (not-equals (self (other Self)) bool             ; triển khai mặc định
    (not (equals self other))))
(deftrait Ord (Eq)                                        ; kế thừa từ Eq
  (less (self (other Self)) bool)                  ; phải được triển khai
  (less-equal (self (other Self)) bool (not (less other self)))
  (greater (self (other Self)) bool (less other self))
  (greater-equal (self (other Self)) bool (not (less self other))))
```

Để triển khai `Eq` bạn chỉ viết `equals`, và với `Ord` chỉ viết `less`. Các triển khai mặc định điền phần
còn lại. `Ord` kế thừa từ `Eq`, nên cần `impl Eq X` trước `impl Ord X`.

Mỗi phương thức của trait có thể được gọi như một hàm nguyên trạng (bên trong một ràng buộc
`where (Eq A)`/`(Ord A)`, hoặc trên một kiểu cụ thể triển khai nó):

| Tên | Dạng | Kiểu | Mô tả |
|---|---|---|---|
| `equals` | `(equals a b)` | `(A,A)→bool` với `Eq A` | Có bằng nhau không (`==` của Rust) |
| `not-equals` | `(not-equals a b)` | `(A,A)→bool` với `Eq A` | Có khác nhau không (`!=`) |
| `less` | `(less a b)` | `(A,A)→bool` với `Ord A` | `a < b` |
| `less-equal` | `(less-equal a b)` | `(A,A)→bool` với `Ord A` | `a <= b` |
| `greater` | `(greater a b)` | `(A,A)→bool` với `Ord A` | `a > b` |
| `greater-equal` | `(greater-equal a b)` | `(A,A)→bool` với `Ord A` | `a >= b` |

`Eq` được triển khai cho: mọi kiểu số (`i8` đến `u32` / `f32` / `f64` / `int` / `ratio`), `bool` `char`
`string` `symbol` `complex`, `Sexpr` (`eq`, tức là đồng nhất; được dùng bởi các mẫu giá trị của `match`),
và `cons-cell<A,B>` (đệ quy, khi các phần tử là `Eq`). `Ord` được triển khai cho: mọi kiểu số, `char`
`string`, và `cons-cell<A,B>` (theo thứ tự từ điển, khi các phần tử là `Ord`).

Tên các phương thức không trùng với các toán tử dựng sẵn (`= /= < <= > >=`) hay `eq`/`lt` vì các thứ dựng
sẵn không thể định nghĩa lại, và mỗi triển khai ủy quyền cho chúng. Bản thân các toán tử so sánh vô
hướng là các phương thức dựng sẵn của từng kiểu bên nhận ([Số](numbers.md),
[Chuỗi và ký tự](collections.md)). Bên trong một ràng buộc, việc viết các toán tử được hiểu là các
phương thức của trait (chương 3).

## 3. Các trait số học (`Add` / `Sub` / `Mul` / `Div` / `Rem` / `Bits` / `Number`)

Một tầng để mã generic yêu cầu "một kiểu có thể cộng được". **Số học trên các kiểu cụ thể dùng các toán
tử dựng sẵn** ([Số](numbers.md)) và không đi qua tầng này.

```lisp
(deftrait Add () (add (self (other Self)) Self))
(deftrait Sub () (sub (self (other Self)) Self))
(deftrait Mul () (mul (self (other Self)) Self))
(deftrait Div () (:type Output) (div (self (other Self)) Output))
(deftrait Rem () (remainder (self (other Self)) Self))
(deftrait Bits ()
  (bit-and (self (other Self)) Self)
  (bit-or  (self (other Self)) Self)
  (bit-xor (self (other Self)) Self)
  (bit-not (self) Self)
  (shift (self (count int)) Self))                 ; khoảng dịch luôn là int (như với ash)
(deftrait Number (Add Sub Mul Rem Ord))                   ; không có phương thức; tổ hợp của năm trait
```

**Bên trong một ràng buộc, bạn có thể viết toán tử.** Khi bên nhận là một biến kiểu bị ràng buộc bởi
`:where`, các toán tử được hiểu là phương thức của trait (`+`→`add`, `-`→`sub`, `*`→`mul`, `/`→`div`,
`rem`→`remainder`, `logand`→`bit-and`, `=`→`equals`, `<`→`less` …):

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
  (push (self (x Item)) ()))
```

Một tập hợp có thể thêm phần tử vào từng cái một. Hàm generic yêu cầu "một kiểu có thể thêm `T` vào"
bằng `(:where (Push C (Item T)))` và thêm vào bất kỳ tập hợp nào như vậy bằng cùng một `(push c x)`.

| Kiểu | `push` làm gì |
|---|---|
| `Vector<T>` | Thêm vào cuối |
| `Deque<T>` | Thêm vào cuối (`push-back`) |
| `Array<T>` | Thêm tại vị trí fill pointer (`push-extend`). Panic nếu không có fill pointer |
| `HashSet<T>` | Đưa vào tập (`insert`). Chỉ là `Push` khi kiểu phần tử triển khai `Hash` |

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
  (pop (self) Option<Item>))
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
