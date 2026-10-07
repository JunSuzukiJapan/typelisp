<!-- translated-from: docs/ja/README.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Tài liệu typelisp (Tiếng Việt)

typelisp là một ngôn ngữ Lisp có kiểu tĩnh. Về cách cài đặt và build, xem [README.md](../../README.md)
(bằng tiếng Anh) ở thư mục gốc của repository.

## Hướng dẫn nhập môn

Nếu mới làm quen với typelisp, hãy đọc theo thứ tự sau.

- [Bắt đầu](tutorial/intro.md): REPL, hàm, biến, rẽ nhánh, vòng lặp, danh sách và `Vector`
- [Kiến thức cơ bản về kiểu](tutorial/types.md): kiểu tĩnh, `Option`, `Result`, struct, enum, generics
- [Trait](tutorial/traits.md): `deftrait` / `impl`, ràng buộc trait, `:dyn`
- [Macro](tutorial/macros.md): `defmacro`, quasiquote, `gensym`, `macrolet`
- [Xử lý lỗi](tutorial/errors.md): `Result`, `panic`, `catch` / `throw`, `unwind-protect`
- [Lập trình đồng thời](tutorial/concurrency.md): task, kênh, `select`, `Mutex`, `thread`

## Hướng dẫn chuyên đề

- [Module và cách bố trí tệp](guide/modules.md): `use`, `pub`, cách tệp tương ứng với module
- [Biên dịch](guide/compile.md): JIT, tạo tệp thực thi bằng biên dịch AOT, dump
- [Vào/ra tệp, stream và mạng](guide/io.md): tệp, pathname, TCP / TLS / UDP, phân giải tên
- [C FFI](guide/ffi.md): gọi hàm C bằng `defffi` (gồm cả callback và struct C với `def-c-struct`)
- [Tích hợp trình soạn thảo](guide/editors.md): `typl-lsp` và cách thiết lập VS Code / Emacs
- [Dành cho lập trình viên Common Lisp](guide/from-common-lisp.md): typelisp khác CL ở đâu và cách viết lại mã CL

## Tham chiếu

- [Tham chiếu cú pháp](reference/syntax.md): cú pháp từ vựng, cách viết kiểu, định nghĩa, các dạng điều khiển, biên dịch, lập trình đồng thời
- [Hàm dựng sẵn](reference/functions/README.md): hàm dựng sẵn, phương thức và thư viện chuẩn
- [Kiểu](reference/types.md): các kiểu và những trait mà mỗi kiểu triển khai
- [Thông báo lỗi](reference/errors.md): ý nghĩa của các lỗi thường gặp và cách sửa

## Tích hợp trình soạn thảo

Các bước thiết lập có trong [hướng dẫn Tích hợp trình soạn thảo](guide/editors.md). Phím tắt và
cài đặt của từng trình soạn thảo được liệt kê trong các tài liệu sau:

- [Emacs (typelisp-mode)](../../editor/emacs/README_vi.md)
- [VS Code](../../editor/vscode/README_vi.md)
