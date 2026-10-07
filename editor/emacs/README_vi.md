<!-- translated-from: editor/emacs/README_JP.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# typelisp-mode (Emacs)

Một major mode của Emacs để soạn thảo mã nguồn typelisp (`.typl`).
Phiên bản VS Code nằm ở [../vscode/](../vscode/README_vi.md). Hai phiên bản dùng chung các bảng từ khóa và các
quy tắc thụt lề giống nhau, và `cargo test --test editor_keyword_sync_test` kiểm tra điều đó một cách máy móc
(xem cuối tài liệu này).

## Tính năng

- Tô sáng cú pháp
  - Các dạng đặc biệt và cấu trúc điều khiển (`defun` `let` `if` `match` `loop` `lambda` `setf` `as`
    `apply`, `print`/`println`/`format`, họ `pprint`, v.v.)
  - Các tên được định nghĩa (`NAME` của `(defun NAME ...)` là tên hàm, của `(defstruct NAME ...)` là tên kiểu,
    và của `(defvar (NAME ...))` là tên biến; tương tự với `pub`, như trong `(pub defun NAME ...)`)
  - Các từ khóa không gian tên và khai báo (`pub` `module` `use` `load` `impl` `where`) và các dấu hiệu của
    danh sách lambda (`&rest` `&optional` `&key`)
  - Các hàm dựng sẵn (`car` `map` `foldl` `unwrap` `parse-int` `message` `sexpr-car`, v.v.)
  - Kiểu nguyên thủy (gồm `bignum` / `ratio`), kiểu dựng sẵn, kiểu lỗi dựng sẵn (`ParseIntError`, v.v.), các
    kiểu người dùng viết hoa chữ đầu (`Capitalized`), và kiểu trait object `:dyn Trait`
  - **Các chỗ dùng kiểu do người dùng định nghĩa** (tên của `defstruct`/`defenum`/`deftrait` thường viết
    thường (`rect` `todo-item` `board`), nên quy tắc `Capitalized` không bắt được chúng). Khi được kết nối với
    `typl-lsp`, chúng được tô màu từ các semantic token của server (cũng hoạt động với `eglot`; xem bên dưới).
    Khi không kết nối, chế độ quay về việc thu thập các tên kiểu được định nghĩa trong buffer
  - Literal (`true` `false`, literal số (thập phân / `0xff` / `1.5` / `1/3`), literal ký tự như `#\Space`,
    chuỗi, keyword như `:name`)
  - Các chỉ thị điều khiển của `format` bên trong chuỗi (`~a` `~5,'0d` `~{...~}`, v.v.)
  - Biến toàn cục có earmuff kiểu CL (`*print-pretty*`, v.v.)
- Chú thích
  - Chú thích dòng `;`
  - Chú thích khối `#| ... |#` **có thể lồng nhau**
- Điều hướng S-expression và thụt lề kiểu Lisp
- Một chỉ mục các định nghĩa qua `imenu` (hàm / phương thức / macro / kiểu / trait / `impl` / biến / module)
- Các lệnh chạy CLI `typl` (bên dưới)

## Phím tắt

| Phím | Lệnh | Tác dụng |
|---|---|---|
| `C-c C-c` | `typelisp-run-buffer` | Lưu và chạy `typl FILE` (qua `compile`, nên bạn có thể nhảy tới các dòng lỗi) |
| `C-c C-z` | `typelisp-repl` | Khởi động REPL của `typl` trong một buffer comint |

Đặt vị trí của `typl` bằng `typelisp-program` (mặc định `"typl"`).
Chẩn đoán có dạng `error: FILE:LINE:COL: ...`, mà `compilation-mode` phân tích được, nên `next-error` /
``C-x ` `` nhảy thẳng tới đúng chỗ.

## Cài đặt

```elisp
(add-to-list 'load-path "/path/to/typelisp/editor/emacs")
(require 'typelisp-mode)
```

Các tệp `.typl` tự động mở trong `typelisp-mode` (chế độ được đăng ký trong `auto-mode-alist`).

Với `use-package`:

```elisp
(use-package typelisp-mode
  :load-path "/path/to/typelisp/editor/emacs"
  :mode "\\.typl\\'")
```

## Language Server (`typl-lsp`)

Khi `typl-lsp` đã được build, nó có thể dùng từ `eglot` (có sẵn trong Emacs 29+) hoặc `lsp-mode`.

```sh
cargo build --release --bin typl-lsp
```

Với `eglot`:

```elisp
(with-eval-after-load 'eglot
  (add-to-list 'eglot-server-programs
               '(typelisp-mode . ("/path/to/typelisp/target/release/typl-lsp"))))
(add-hook 'typelisp-mode-hook #'eglot-ensure)
```

Với `lsp-mode`:

```elisp
(with-eval-after-load 'lsp-mode
  (lsp-register-client
   (make-lsp-client
    :new-connection (lsp-stdio-connection "/path/to/typelisp/target/release/typl-lsp")
    :major-modes '(typelisp-mode)
    :server-id 'typl-lsp)))
(add-hook 'typelisp-mode-hook #'lsp)
```

Được hỗ trợ: chẩn đoán (lỗi cú pháp/kiểu và cảnh báo định nghĩa lại, được gửi qua
`textDocument/publishDiagnostics`), hover, chuyển đến định nghĩa, gợi ý hoàn thành (`:` được đăng ký làm ký tự
kích hoạt), và semantic token. Các tham chiếu giữa các tệp qua `use` được phân giải (server tìm ngược lên
`typelisp.toml` của thư mục gốc dự án; chi tiết xem
[Tham chiếu cú pháp 3.11](../../docs/vi/reference/syntax.md#311-tệp-và-module-dự-án-nhiều-tệp)). Những
chỉnh sửa chưa lưu trong các buffer trình soạn thảo đang mở được phản ánh ngay vào chẩn đoán của cả các tệp mà
chúng phụ thuộc vào lẫn các tệp phụ thuộc vào chúng.

### Tô sáng tên kiểu (semantic token)

Qua `textDocument/semanticTokens`, server báo cáo **các vị trí mà bộ kiểm tra thực sự đã phân giải thành tên
kiểu**. Vì đây không phải so khớp văn bản:

- Các kiểu đến từ tệp khác qua `use` cũng được tô màu (một phạm vi mà việc phân giải bên trong buffer không thể
  với tới về nguyên tắc)
- Các lời gọi một **hàm** cùng tên với một kiểu không được tô màu (bộ kiểm tra đã phân giải chúng là hàm, nên
  ngay từ đầu không có token nào được ghi ở đó)

Ở phía client:

- **`eglot` (Emacs 31 trở lên)**: eglot tự vẽ các token (`eglot-semantic-tokens-mode`). `typelisp-mode`
  đứng ngoài
- **`eglot` (Emacs 30 trở xuống)**: phiên bản eglot này không xử lý semanticTokens. Vì vậy
  **`typelisp-mode` tự gửi yêu cầu và vẽ kết quả bằng overlay** (`typelisp-semantic-tokens-mode`, được bật tự
  động khi eglot kết nối)
- **`lsp-mode`**: hỗ trợ sẵn (đặt `lsp-semantic-tokens-enable` thành `t`). Trong trường hợp đó
  `typelisp-mode` đứng ngoài

`scripts/emacs-semantic-smoke.el` kết nối qua eglot thật và kiểm tra phía thực hiện việc vẽ trong Emacs đang
dùng. Với bất kỳ client nào, phương án dự phòng trong buffer lùi lại khi server đang trả lời (để hai bộ quy
tắc không cùng tô một buffer).

| Cài đặt | Mặc định | Tác dụng |
|---|---|---|
| `typelisp-semantic-tokens` | `t` | Với eglot của Emacs 30 trở xuống, có tô màu từ các semantic token của server hay không |
| `typelisp-semantic-tokens-idle-delay` | `0.6` | Số giây nhàn rỗi sau một chỉnh sửa trước khi yêu cầu lại (giữ nó lớn hơn `eglot-send-changes-idle-time`) |

## Ghi chú

- typelisp chuyển symbol thành chữ thường khi đọc, nhưng việc tô sáng phân biệt hoa thường để có thể phân biệt
  các tên kiểu bắt đầu bằng chữ hoa.
- Thụt lề được quyết định bởi `typelisp-indent-function` chuyên dụng, hàm này tra `typelisp-indent-specs` (một
  alist). Chế độ giữ các mục riêng của nó ngay cả với các dạng có tên trùng với Emacs Lisp (`defun` `let` `if`
  ...) vì các thuộc tính symbol là **toàn cục**, và các thiết lập typelisp ở đó sẽ làm thay đổi thụt lề của các
  buffer Lisp khác trong cùng phiên. Và các dạng của typelisp khác hình dạng ngay cả khi trùng tên với Emacs
  Lisp: `(defun NAME (PARAMS) RETTYPE ...)` có ba phần tử đầu, và `if` được cố định ba phần tử với `else` bắt
  buộc. Vì vậy các giá trị cũng không thể dùng chung.
  Mọi tệp `.typl` dưới `examples/` đã được kiểm tra: `indent-region` không thay đổi một byte nào, và làm phẳng
  toàn bộ thụt lề rồi thụt lề lại sẽ khôi phục bản gốc (phiên bản VS Code đáp ứng cùng tiêu chuẩn trên cùng các
  tệp).

## Phát hiện sự lệch pha của các định nghĩa trình soạn thảo

Các bảng từ khóa được duy trì hai lần, một lần ở đây và một lần ở phiên bản VS Code. Để ngăn các định nghĩa
trình soạn thảo tụt lại khi phần triển khai tiến lên, có một bài kiểm thử ở phía Rust:

```sh
cargo test --test editor_keyword_sync_test
```

Nó thực sự nạp prelude, duyệt registry, và báo cáo **các tên mà một trong hai trình soạn thảo không biết**. Các
dạng đặc biệt không có biểu diễn lúc chạy, nên chúng được đọc từ giữa `// SPECIAL-FORM DISPATCH BEGIN` / `END`
trong `crates/typelisp-front/src/check/checker.rs` (đừng xóa các chú thích này). Nếu nó thất bại, hãy thêm các
tên được báo vào **cả hai** định nghĩa trình soạn thảo.

Cùng bài kiểm thử đó cũng so sánh chú giải semantic token (`SEMANTIC_TOKEN_TYPES` trong `src/bin/lsp.rs` và các
bảng mà cả hai trình soạn thảo giữ phải khớp nhau về tên và thứ tự). Một sự không khớp không gây lỗi lúc chạy;
nó chỉ làm đổi chỗ màu của mọi token, nên nó được chốt lại một cách máy móc.
