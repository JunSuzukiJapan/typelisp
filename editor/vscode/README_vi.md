<!-- translated-from: editor/vscode/README_JP.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# typelisp (VS Code)

Một phần mở rộng VS Code để soạn thảo mã nguồn typelisp (`.typl`).
Phiên bản Emacs nằm ở [../emacs/](../emacs/README_vi.md). Hai phiên bản dùng chung các bảng từ khóa và các
quy tắc thụt lề giống nhau, và `cargo test --test editor_keyword_sync_test` kiểm tra điều đó một cách máy móc
(xem bên dưới).

## Tính năng

- **Tô sáng cú pháp** (một ngữ pháp TextMate; không cần language server)
  - Các dạng đặc biệt và cấu trúc điều khiển (`defun` `let` `if` `match` `loop` `lambda` `setf` `as`
    `apply`, `print`/`println`/`format`, họ `pprint`)
  - Các tên được định nghĩa (`(defun NAME ...)` là hàm, `(defstruct NAME ...)` là kiểu,
    `(defvar (NAME ...))` là biến; tương tự với `pub`, như trong `(pub defun NAME ...)`) và cả hai tên của
    `(impl Trait Type)`
  - Các từ khóa không gian tên và khai báo (`pub` `module` `use` `load` `impl` `where`) và các dấu hiệu của
    danh sách lambda (`&rest` `&optional` `&key`)
  - Hàm dựng sẵn, kiểu nguyên thủy (gồm `bignum` / `ratio`), kiểu lỗi dựng sẵn, các kiểu người dùng viết
    hoa chữ đầu (`Capitalized`), và kiểu trait object `:dyn Trait` (cũng bên trong các đối số generic)
  - Literal số (thập phân / `0xff` / `1.5` / `3.0e10` / `1/3`), literal ký tự như `#\Space`, keyword như
    `:name`, và biến toàn cục có earmuff như `*print-pretty*`
  - **Các chỉ thị điều khiển của `format` bên trong chuỗi** (`~a` `~5,'0d` `~{...~}` `~^`, v.v.)
  - Chú thích dòng `;` và chú thích khối `#| ... |#` **có thể lồng nhau**
- **Các chỗ dùng kiểu do người dùng định nghĩa** (semantic token)
  - Tên của `defstruct` / `defenum` / `deftrait` thường viết thường (`rect` `todo-item` `board`), nên quy
    tắc `Capitalized` không bắt được chúng, và một ngữ pháp TextMate làm việc theo từng dòng và không nhìn
    được cả tệp. Semantic token thì nhìn được, nhờ đó khắc phục tình trạng một ngôn ngữ có kiểu tĩnh chỉ
    được tô màu ở các chú thích kiểu
  - Khi được kết nối với `typl-lsp`, phần mở rộng nhận **các vị trí mà bộ kiểm tra thực sự đã phân giải
    thành tên kiểu**. Vì vậy các kiểu đến từ tệp khác qua `use` được tô màu, còn các lời gọi một **hàm** cùng
    tên với một kiểu thì không (bộ kiểm tra đã phân giải chúng là hàm, nên ngay từ đầu không có token nào được
    ghi ở đó)
  - Khi server chưa được kết nối hoặc chưa được build, phần mở rộng quay về một lượt quét văn bản phân giải
    trong phạm vi tệp. Đó là một phép xấp xỉ: nó không tìm được các kiểu từ tệp khác, và không phân biệt
    được một hàm cùng tên với một kiểu
- **Thụt lề Lisp** (VS Code không có thụt lề Lisp dựng sẵn, nên phần mở rộng tự triển khai)
  - Format Document, Format Selection, và định dạng khi gõ (Enter và `)`, khi `editor.formatOnType` được bật)
- **Outline / breadcrumbs / `Ctrl+Shift+O`** (hàm, phương thức, macro, kiểu, trait, `impl`, biến, module)
- **Tích hợp `typl-lsp`** (chẩn đoán, hover, chuyển đến định nghĩa, gợi ý hoàn thành, semantic token)
- **Các lệnh CLI `typl`** (chạy, REPL)

Mọi thứ trừ language server đều hoạt động chỉ với phần mở rộng, nên ngay cả trong một bản checkout chưa build
`typl-lsp`, vẫn có tô sáng, thụt lề, Outline, và tô sáng kiểu (trong phạm vi tệp).

## Cài đặt

Phần mở rộng chưa có trên Marketplace, nên hãy build cục bộ và cài đặt.

```sh
cd editor/vscode
npm install
npm run compile
```

Sau đó chọn một trong hai:

- **Thử trong một development host**: mở `editor/vscode` trong VS Code và nhấn `F5`
- **Cài đặt lâu dài**: tạo một tệp `.vsix` bằng `npx @vscode/vsce package`, rồi dùng
  "..." → "Install from VSIX..." trong khung Extensions

Các tệp `.typl` tự động mở ở chế độ typelisp.

## Phím tắt

| Phím | Lệnh | Tác dụng |
|---|---|---|
| `Ctrl+Alt+R` | `typelisp.runFile` | Lưu và chạy `typl FILE` |
| `Ctrl+Alt+Z` | `typelisp.repl` | Khởi động REPL của `typl` |

Bảng lệnh cũng có `typelisp: Restart Language Server`.

## Cài đặt

| Cài đặt | Mặc định | Tác dụng |
|---|---|---|
| `typelisp.program` | `typl` | Đường dẫn của CLI `typl` |
| `typelisp.languageServer.enable` | `true` | Có kết nối với `typl-lsp` hay không |
| `typelisp.languageServer.path` | (trống) | Đường dẫn của `typl-lsp`. Khi trống, phần mở rộng tìm `target/release/typl-lsp` của workspace, rồi `target/debug/typl-lsp`, rồi `PATH` |
| `typelisp.trace.server` | `off` | Ghi log lưu lượng JSON-RPC của LSP |

Build language server bằng:

```sh
cargo build --release --bin typl-lsp
```

Các tham chiếu giữa các tệp qua `use` được phân giải bằng cách tìm ngược lên `typelisp.toml` của thư mục gốc dự
án (chi tiết xem
[Tham chiếu cú pháp 3.11](../../docs/vi/reference/syntax.md#311-tệp-và-module-dự-án-nhiều-tệp)).

## Problem matcher của task

Phần mở rộng cung cấp một problem matcher tên là `typelisp`. `typl` in chẩn đoán dưới dạng
`error: FILE:LINE:COL: message`, nên chúng có thể đi thẳng vào bảng Problems:

```jsonc
{
  "version": "2.0.0",
  "tasks": [
    {
      "label": "typl: run",
      "type": "shell",
      "command": "typl ${file}",
      "problemMatcher": "$typelisp"
    }
  ]
}
```

## Phát triển

```sh
npm run compile   # tsc
npm run watch     # build khi có thay đổi
npm test          # node --test (ngữ pháp, thụt lề, symbol, tham chiếu kiểu, manifest)
```

Các bài kiểm thử chỉ bao phủ những phần không cần module `vscode`. Vì mục đích đó, `src/indent.ts` và
`src/symbols.ts` được viết dưới dạng các hàm thuần, và chỉ `src/extension.ts` chạm vào API của trình soạn thảo.

- `src/test/grammar.test.ts` — tách token bằng ngữ pháp thật, dùng cùng engine như VS Code
  (`vscode-textmate` + `vscode-oniguruma`), và kiểm tra kết quả.
  Oniguruma khác biểu thức chính quy của Emacs ở các chi tiết (ví dụ, nó không coi một `]` ở đầu một lớp ký tự
  là nguyên văn), và những khác biệt như vậy chỉ được tìm ra bằng cách chạy engine thật.
- `src/test/indent.test.ts` — với mọi tệp `.typl` dưới `examples/`, yêu cầu rằng **làm phẳng toàn bộ thụt lề
  rồi khôi phục lại phải khớp với nội dung đã commit từng byte một**. Chế độ Emacs đáp ứng cùng tiêu chuẩn
  trên cùng các tệp, và đó là điều làm cho "hai trình soạn thảo thống nhất" trở thành một khẳng định đã được
  kiểm chứng.
  Ngoài ra, `src/test/fixtures/emacs-indent-reference.txt` là đầu ra tham chiếu thu thập bằng cách thực sự chạy
  `indent-region` trong một buffer `typelisp-mode` của Emacs. Phía kỳ vọng không phải là việc diễn đạt lại
  triển khai TS mà là **những gì trình soạn thảo kia thực sự tạo ra**, nên độ trung thực của bản chuyển được
  kiểm tra trực tiếp (nó gồm `let*` `do` `doiter` `labels` `impl` `pprint-logical-block`, các tiền tố quote và
  nhiều hơn nữa).
- `src/test/symbols.test.ts` — nội dung của Outline và việc phát hiện các tham chiếu kiểu của phương án dự
  phòng. Số lượng định nghĩa phải khớp chính xác với một phép đếm độc lập các dạng định nghĩa ở đầu dòng. Các
  quy tắc ranh giới cho tham chiếu kiểu được căn chỉnh có chủ ý với phương án dự phòng của phiên bản Emacs (VS
  Code dùng lookbehind; Emacs biểu diễn cùng tập bằng cách tiêu thụ một ký tự đứng trước).
- Các token do phân giải điều khiển của server (`crates/typelisp-front/src/check/semantic.rs`) được kiểm tra
  bằng `cargo test --test lsp_semantic_test` và `scripts/lsp-semantic-smoke.py` (chạy một tiến trình thật qua
  stdio). Client Emacs được kiểm tra bằng `scripts/emacs-semantic-smoke.el` qua một kết nối eglot thật.
- `src/test/manifest.test.ts` — `package.json` là phần duy nhất mà trình biên dịch không kiểm tra, nên bài này
  kiểm tra rằng các lệnh đã khai báo và các lời gọi `registerCommand` là cùng một tập, các phím tắt tham chiếu
  tới đâu, rằng các cài đặt mà mã đọc đã được khai báo, và rằng problem matcher có thể phân tích những gì `typl`
  thực sự in ra.

### Phát hiện sự lệch pha của các định nghĩa trình soạn thảo

Các bảng từ khóa được duy trì hai lần, ở phiên bản Emacs và phiên bản VS Code. Để ngăn các định nghĩa trình
soạn thảo tụt lại khi phần triển khai tiến lên, có một bài kiểm thử ở phía Rust:

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

## Ghi chú

- typelisp chuyển symbol thành chữ thường khi đọc, nhưng việc tô sáng phân biệt hoa thường để có thể phân biệt
  các tên kiểu bắt đầu bằng chữ hoa.
- Thụt lề được quyết định bởi `INDENT_SPECS` trong `src/indent.ts`. Nó là bản chuyển của
  `typelisp-indent-specs` của phiên bản Emacs, với cùng giá trị và quy tắc. Những chỗ một dạng khác hình dạng so
  với dạng Emacs Lisp cùng tên được giữ nguyên: phần đầu của `(defun NAME (PARAMS) RETTYPE ...)` có ba phần tử,
  `if` được cố định ba phần tử với `else` bắt buộc, v.v.
- Nội dung của `#| ... |#` được thụt lề lại khi định dạng. Điều này khớp với hành vi của `indent-region` của
  Emacs.
