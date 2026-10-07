<!-- translated-from: docs/ja/guide/editors.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Tích hợp trình soạn thảo (typl-lsp)

`typl-lsp` là language server của typelisp. Khi được kết nối với một trình soạn thảo hỗ trợ LSP
(Language Server Protocol), nó cung cấp các tính năng sau cho tệp bạn đang chỉnh sửa:

- Chẩn đoán: lỗi đọc, lỗi kiểu và cảnh báo định nghĩa lại
- Hover: kiểu của một biểu thức trong ngoặc và docstring của định nghĩa mà nó gọi (không hiển thị
  cho tên biến trần)
- Chuyển đến định nghĩa
- Gợi ý hoàn thành (các ứng viên xuất hiện khi bạn gõ `:`)
- Tô màu tên kiểu (semantic token), kể cả các kiểu được `use` từ các tệp khác

Các tham chiếu giữa các tệp thông qua `use` đều được phân giải. Những chỉnh sửa chưa lưu ở một tệp
khác đang mở sẽ được phản ánh ngay vào chẩn đoán của các tệp `use` tệp đó.

## 1. Build

```sh
cargo build --release --bin typl-lsp
```

Lệnh này tạo ra `target/release/typl-lsp`. Nếu bạn đã cài bằng `cargo install` như mô tả trong
[README.md](../../../README.md), nó nằm ở `~/.cargo/bin/typl-lsp` cùng với `typl`.

## 2. VS Code

Phần mở rộng nằm ở `editor/vscode` trong repository. Nó chưa được đăng trên Marketplace, nên bạn
phải tự build và cài đặt.

```sh
cd editor/vscode
npm install
npm run compile
npx @vscode/vsce package      # tạo ra tệp .vsix
```

Chọn "Install from VSIX..." từ menu "..." của khung Extensions rồi chọn tệp `.vsix` bạn vừa build.

Phần mở rộng tìm `typl-lsp` trong `target/release/typl-lsp` của workspace, rồi
`target/debug/typl-lsp`, rồi đến `PATH`. Nếu bạn đặt nó ở nơi khác, hãy ghi đường dẫn của nó vào
cài đặt `typelisp.languageServer.path`.

| Cài đặt | Mặc định | Ý nghĩa |
|---|---|---|
| `typelisp.program` | `typl` | Đường dẫn của `typl` |
| `typelisp.languageServer.enable` | `true` | Có kết nối với `typl-lsp` hay không |
| `typelisp.languageServer.path` | (trống) | Đường dẫn của `typl-lsp` |

`Ctrl+Alt+R` lưu tệp đang chỉnh sửa và chạy nó bằng `typl`, còn `Ctrl+Alt+Z` khởi động REPL. Chi tiết
hơn, xem [README của phần mở rộng VS Code](../../../editor/vscode/README_vi.md).

## 3. Emacs

`typelisp-mode` nằm ở `editor/emacs` trong repository.

```elisp
(add-to-list 'load-path "/path/to/typelisp/editor/emacs")
(require 'typelisp-mode)
```

Cấu hình để kết nối với `typl-lsp` bằng `eglot` (đi kèm Emacs 29 trở lên):

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

Eglot của Emacs 31 trở lên tự tô màu tên kiểu (semantic token). Eglot của Emacs 30 trở xuống không
hỗ trợ tính năng này, nên `typelisp-mode` sẽ tô màu tên kiểu thay. Với `lsp-mode`, hãy đặt
`lsp-semantic-tokens-enable` thành `t`.

`C-c C-c` chạy tệp đang chỉnh sửa, và `C-c C-z` khởi động REPL. Chi tiết hơn, xem
[README của typelisp-mode](../../../editor/emacs/README_vi.md).

## 4. Các trình soạn thảo khác

`typl-lsp` giao tiếp LSP qua đầu vào và đầu ra chuẩn và không nhận đối số dòng lệnh nào. Hãy cấu hình
LSP client của trình soạn thảo để nó khởi động `typl-lsp` cho các tệp `.typl`.

## 5. Cách nhận diện dự án

`typl-lsp` tìm `typelisp.toml` bắt đầu từ thư mục của tệp đang mở rồi đi ngược lên các thư mục cha, và
phân giải `use` với vị trí đó làm gốc mã nguồn. Đây là cùng các quy tắc như khi `typl` chạy một tệp
([Module và cách bố trí tệp](modules.md#2-thiết-lập-dự-án)). Với dự án gồm nhiều tệp, hãy đặt
`typelisp.toml` ở gốc của nó.

## 6. Language server không chạy chương trình của bạn

`typl-lsp` tạo chẩn đoán chỉ bằng cách đọc và kiểm tra kiểu. Nó không bao giờ chạy chương trình bạn
đang chỉnh sửa. Chẩn đoán chạy sau mỗi lần gõ phím, nên không thể chạy mã có tác dụng phụ hoặc mã
không bao giờ kết thúc ở đó. Ngoại lệ duy nhất là việc đăng ký các `defmacro`, vốn cần thiết để kiểm
tra các lời gọi macro đứng sau chúng.

Vì vậy, những lỗi chỉ xảy ra khi `typl` chạy chương trình (`panic`, thiếu tệp, v.v.) sẽ không xuất
hiện trong chẩn đoán của language server.
