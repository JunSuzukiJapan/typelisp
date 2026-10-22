<!-- translated-from: docs/ja/guide/modules.md @ d07d7036fc5a553e51ac4b2fe6beddc181daaa2a -->
# Module và cách bố trí tệp

Tài liệu này giải thích cách ghép một chương trình gồm nhiều tệp. Các quy tắc chi tiết nằm ở các mục
3.10 đến 3.13 của [Tham chiếu cú pháp](../reference/syntax.md#310-module--use--không-gian-tên).

## 1. Một tệp là một module

Trong typelisp, **mỗi tệp tự nó là một module**. Đường dẫn của tệp so với thư mục gốc mã nguồn chính là
đường dẫn của module.

| Tệp | Module |
|---|---|
| `<root>/geometry.typl` | `geometry` |
| `<root>/geo/shapes.typl` | `geo::shapes` |
| `<root>/net/http/client.typl` | `net::http::client` |

Không cần viết khai báo module ở đầu tệp.

## 2. Thiết lập dự án

Đặt một tệp tên `typelisp.toml` ở thư mục gốc của dự án. Tệp này có thể để trống.

```
myapp/
├── typelisp.toml
├── main.typl
└── geometry.typl
```

Để giữ mã nguồn trong `src/`, hãy viết một dòng này trong `typelisp.toml`:

```toml
src = "src"
```

`typl` tìm `typelisp.toml` bắt đầu từ thư mục của tệp nó chạy rồi đi ngược lên các thư mục cha, và dùng
nơi tìm thấy làm thư mục gốc mã nguồn. Nếu không tìm thấy, thư mục của tệp đang chạy là thư mục gốc
(trong REPL là thư mục hiện tại).

## 3. Công khai các định nghĩa và sử dụng chúng

`geometry.typl`:

```lisp
(pub defstruct point
  (pub x i32)
  (pub y i32)
  (tag string))                        ; trường không có pub không đọc cũng không ghi được từ bên ngoài

(defun square ((n i32)) i32 (* n n))   ; hàm không có pub cũng không gọi được từ bên ngoài

(pub defun dist2 ((a point) (b point)) i32
  (+ (square (- a::x b::x)) (square (- a::y b::y))))

(impl point
  (pub new ((x i32) (y i32) (tag string)) point
    (make-instance 'point :x x :y y :tag tag))
  (pub origin () point (point::new 0 0 "o")))
```

`main.typl`:

```lisp
(use geometry)

(let ((a (geometry::point::origin))
      (b (geometry::point::new 3 4 "b")))
  (println "~a" (geometry::dist2 a b)))
```

```sh
typl main.typl        # => 25
```

`geometry.typl` được nạp tại vị trí viết `(use geometry)`. Không cần nạp nó trước.

### Những gì được công khai

- Hàm, struct, enum, trait, biến toàn cục và macro chỉ nhìn thấy được từ các module khác khi có `pub`. Đặt `pub`
  ngay trước định nghĩa, như `(pub defun ...)`.
- Trong các phương thức viết trong `impl`, chỉ những mục viết `(pub new ...)` mới dùng được từ bên ngoài.
- Với struct, **công khai kiểu và công khai trường là hai việc riêng**. `(pub defstruct point ...)` làm kiểu
  nhìn thấy được, và chỉ các trường viết `(pub x i32)` mới đọc ghi được từ bên ngoài.
- Tương tự, ở module khác chỉ đưa được các trường `pub` cho `make-instance`. `point` ở trên có `tag` không
  phải `pub`, nên module khác không tạo được nó bằng `make-instance` mà dùng `point::new`.
- Dùng từ bên ngoài một tên không công khai sẽ cho lỗi "không phân giải được" như
  `unresolved path: geometry::square`. Đây là cùng thông báo như khi viết sai tên, nên nếu chính tả
  đã đúng mà tên vẫn không phân giải được, hãy nghi ngờ là thiếu `pub`.

Danh sách các định nghĩa có thể nhận `pub` nằm ở
[Tham chiếu cú pháp 3.13](../reference/syntax.md#313-pub--khả-năng-hiển-thị).

## 4. Cách viết `use`

```lisp
(use geometry)              ; đưa một module vào; viết geometry::dist2 để dùng
(use geometry::dist2)       ; đưa một hàm vào; dùng bằng tên trần dist2
(use geometry::point)       ; đưa một kiểu vào; viết point::new, point::origin, và point trong chú thích kiểu
(use a::f b::g)             ; có thể viết nhiều mục cùng lúc
```

- **`use` chỉ có hiệu lực với các dạng đứng sau nó.** Hãy đặt nó ở đầu tệp. Viết `geometry::dist2` phía
  trên `use` sẽ cho `unresolved path`.
- Viết đường dẫn đầy đủ `geometry::dist2` mà không `use` module cũng không phân giải được. Chỉ có
  `use` mới khiến một tệp được nạp.
- `use` một tên mà dạng trần của nó đã bị chiếm sẽ cho cảnh báo. Khi bạn chủ ý muốn đưa nó vào, hãy
  dùng `shadowing-import`.
- Module nằm trong một thư mục được viết là `(use geo::shapes)`, và sau đó được gọi bằng phần cuối
  của nó (`shapes::...`).

### Gọi phương thức của trait

Các phương thức được triển khai trong `impl` **thuộc về kiểu**, chứ không thuộc các hàm của module, nên
chúng được gọi mà không cần tên module.

```lisp
(use shapes::core)
(println "~a" (area (make-instance 'core::square :side 4)))    ; area, không phải core::area
```

Các phương thức trong `impl` của trait luôn công khai, kể cả khi không có `pub`. Trong các phương thức của
`impl` cố hữu (`(impl point ...)`), chỉ những phương thức có `pub` mới công khai.

Trait cũng chỉ nhìn thấy được từ các module khác khi được viết là `(pub deftrait ...)`. Ở module khác, hãy
viết trait bằng đường dẫn như `shapes::Shape` hoặc đưa vào bằng `use`, rồi viết trong `impl`.

## 5. Chia không gian tên bên trong một tệp

Để chia không gian tên nhỏ hơn bên trong một tệp, dùng `module`. Nó được lồng bên trong module của
chính tệp đó.

```lisp
;; bên trong main.typl
(module util
  (pub defun clamp ((x i32) (lo i32) (hi i32)) i32
    (if (< x lo) lo (if (> x hi) hi x))))

(println "~a" (util::clamp 15 0 10))    ; main::util::clamp
```

Để đưa toàn bộ phần còn lại của tệp vào một không gian tên, bạn có thể viết `(in-module util)` thay vì
bọc nó trong ngoặc.

## 6. Hạn chế về phụ thuộc

- **Không cho phép vòng lặp phụ thuộc.** Nếu `a.typl` có `(use b)` và `b.typl` có `(use a)`, kết quả
  là lỗi `circular module dependency: a -> b -> a`. Hãy chuyển những định nghĩa mà cả hai cần sang
  một module thứ ba.
- **Cả kiểu lẫn hàm đều không thể được tham chiếu trước khi được định nghĩa**, ngay cả trong cùng một
  tệp. Với các hàm đệ quy lẫn nhau, hãy khai báo trước một trong số chúng bằng `defsignature`
  ([Tham chiếu cú pháp 3.2](../reference/syntax.md#32-defsignature--khai-báo-trước)).

## 7. Thứ tự thực thi

Chạy `typl main.typl` diễn ra theo thứ tự sau:

1. `main.typl` và mọi tệp được `use` từ nó được đọc và kiểm tra kiểu. **Nếu có lỗi kiểu ở bất kỳ đâu,
   sẽ không có gì được chạy.**
2. Các biểu thức cấp cao nhất của các module được `use` chạy trước các biểu thức của những module
   dùng chúng.
3. Các biểu thức cấp cao nhất của `main.typl` chạy từ trên xuống dưới.

Nếu bạn gom điểm vào của chương trình vào một hàm `main` và gọi `(main)` ở cuối tệp, thì cùng tệp đó
cũng có thể dùng cho
[biên dịch AOT](compile.md#3-tạo-tệp-thực-thi-bằng-biên-dịch-aot).

## 8. Khác với `load` ở đâu

`(load "path")`, giống `load` của Common Lisp, đọc nội dung của một tệp **vào không gian tên hiện tại
nguyên trạng**. Nó không bọc nội dung trong một module, và `pub` không có vai trò gì. Hãy dùng nó cho
những việc như đọc tệp cấu hình hoặc nạp lại một tệp cục bộ trong REPL. Để chia chương trình thành
nhiều phần, hãy dùng `use`.
