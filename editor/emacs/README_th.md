<!-- translated-from: editor/emacs/README_JP.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# typelisp-mode (Emacs)

major mode ของ Emacs สำหรับแก้ไขซอร์ส typelisp (`.typl`)
เวอร์ชัน VS Code อยู่ใน [../vscode/](../vscode/README_th.md) ทั้งสองใช้ตารางคีย์เวิร์ดชุดเดียวกัน
และกฎการเยื้องชุดเดียวกัน และ `cargo test --test editor_keyword_sync_test` ตรวจสอบเรื่องนี้
โดยอัตโนมัติ (ดูท้ายเอกสารนี้)

## ความสามารถ

- การระบายสีไวยากรณ์
  - ฟอร์มพิเศษและโครงสร้างควบคุม (`defun` `let` `if` `match` `loop` `lambda` `setf` `as`
    `apply`, `print`/`println`/`format`, ตระกูล `pprint` และอื่น ๆ)
  - ชื่อที่นิยาม (`NAME` ของ `(defun NAME ...)` เป็นชื่อฟังก์ชัน ของ `(defstruct NAME ...)`
    เป็นชื่อชนิด และของ `(defvar (NAME ...))` เป็นชื่อตัวแปร; เช่นเดียวกันเมื่อมี `pub` เช่น
    `(pub defun NAME ...)`)
  - คีย์เวิร์ดของเนมสเปซและการประกาศ (`pub` `module` `use` `load` `impl` `where`) และตัวบอกรายการพารามิเตอร์
    ของ lambda (`&rest` `&optional` `&key`)
  - ฟังก์ชันที่มีให้ในตัว (`car` `map` `foldl` `unwrap` `parse-int` `message` `sexpr-car` และอื่น ๆ)
  - ชนิดพื้นฐาน (รวม `bignum` / `ratio`) ชนิดที่มีให้ในตัว ชนิดข้อผิดพลาดที่มีให้ในตัว
    (`ParseIntError` และอื่น ๆ) ชนิดที่ผู้ใช้นิยามซึ่งขึ้นต้นด้วยตัวพิมพ์ใหญ่ (`Capitalized`) และชนิด trait object `:dyn Trait`
  - **การใช้ชนิดที่ผู้ใช้นิยาม** (ชื่อของ `defstruct`/`defenum`/`deftrait` มักเป็น
    ตัวพิมพ์เล็ก (`rect` `todo-item` `board`) ดังนั้นกฎ `Capitalized` จับไม่ได้)
    เมื่อเชื่อมต่อกับ `typl-lsp` จะระบายสีจาก semantic token ของเซิร์ฟเวอร์ (ใช้กับ `eglot` ได้ด้วย
    ดูด้านล่าง) เมื่อไม่ได้เชื่อมต่อ โหมดจะถอยไปรวบรวมชื่อชนิดที่นิยามใน
    บัฟเฟอร์
  - literal (`true` `false` literal ตัวเลข (ฐานสิบ / `0xff` / `1.5` / `1/3`) literal
    อักขระอย่าง `#\Space` สตริง คีย์เวิร์ดอย่าง `:name`)
  - คำสั่งควบคุมของ `format` ภายในสตริง (`~a` `~5,'0d` `~{...~}` และอื่น ๆ)
  - ตัวแปรโกลบอลสไตล์ CL ที่มีเครื่องหมายดาว (`*print-pretty*` และอื่น ๆ)
- คอมเมนต์
  - คอมเมนต์บรรทัด `;`
  - คอมเมนต์บล็อก `#| ... |#` ที่ **ซ้อนกันได้**
- การนำทาง S-expression และการเยื้องแบบ Lisp
- ดัชนีการนิยามผ่าน `imenu` (ฟังก์ชัน / เมทอด / แมโคร / ชนิด / trait / `impl` /
  ตัวแปร / โมดูล)
- คำสั่งที่รัน CLI `typl` (ด้านล่าง)

## ปุ่มลัด

| ปุ่ม | คำสั่ง | ทำอะไร |
|---|---|---|
| `C-c C-c` | `typelisp-run-buffer` | บันทึกและรัน `typl FILE` (ผ่าน `compile` จึงกระโดดไปบรรทัดที่ผิดพลาดได้) |
| `C-c C-z` | `typelisp-repl` | เริ่ม REPL ของ `typl` ในบัฟเฟอร์ comint |

ตั้งตำแหน่งของ `typl` ด้วย `typelisp-program` (ค่าเริ่มต้น `"typl"`)
การวินิจฉัยมีรูปแบบ `error: FILE:LINE:COL: ...` ซึ่ง `compilation-mode` อ่านได้ ดังนั้น
`next-error` / ``C-x ` `` กระโดดไปยังตำแหน่งนั้นได้ทันที

## การติดตั้ง

```elisp
(add-to-list 'load-path "/path/to/typelisp/editor/emacs")
(require 'typelisp-mode)
```

ไฟล์ `.typl` เปิดใน `typelisp-mode` โดยอัตโนมัติ (โหมดถูกลงทะเบียนใน `auto-mode-alist`)

เมื่อใช้ `use-package`:

```elisp
(use-package typelisp-mode
  :load-path "/path/to/typelisp/editor/emacs"
  :mode "\\.typl\\'")
```

## Language Server (`typl-lsp`)

เมื่อบิลด์ `typl-lsp` แล้ว ใช้จาก `eglot` (มีในตัว Emacs 29 ขึ้นไป) หรือ `lsp-mode` ได้

```sh
cargo build --release --bin typl-lsp
```

เมื่อใช้ `eglot`:

```elisp
(with-eval-after-load 'eglot
  (add-to-list 'eglot-server-programs
               '(typelisp-mode . ("/path/to/typelisp/target/release/typl-lsp"))))
(add-hook 'typelisp-mode-hook #'eglot-ensure)
```

เมื่อใช้ `lsp-mode`:

```elisp
(with-eval-after-load 'lsp-mode
  (lsp-register-client
   (make-lsp-client
    :new-connection (lsp-stdio-connection "/path/to/typelisp/target/release/typl-lsp")
    :major-modes '(typelisp-mode)
    :server-id 'typl-lsp)))
(add-hook 'typelisp-mode-hook #'lsp)
```

รองรับ: การวินิจฉัย (ข้อผิดพลาดของไวยากรณ์/ชนิดและคำเตือนการนิยามซ้ำ ส่งผ่าน
`textDocument/publishDiagnostics`) hover ไปยังจุดนิยาม การเติมข้อความอัตโนมัติ (`:` ถูกลงทะเบียนเป็น
อักขระกระตุ้น) และ semantic token การอ้างอิงข้ามไฟล์ผ่าน `use` ถูกแก้ไข (เซิร์ฟเวอร์
ค้นหาขึ้นไปหา `typelisp.toml` ของรากโปรเจกต์ รายละเอียดดู
[เอกสารอ้างอิงไวยากรณ์ 3.11](../../docs/th/reference/syntax.md#311-ไฟล์และโมดูล-โปรเจกต์หลายไฟล์))
การแก้ไขที่ยังไม่ได้บันทึกในบัฟเฟอร์ของโปรแกรมแก้ไขข้อความที่เปิดอยู่ถูกสะท้อนทันทีในการวินิจฉัยของทั้งไฟล์ที่
มันพึ่งพาและไฟล์ที่พึ่งพามัน

### การระบายสีชื่อชนิด (semantic token)

ผ่าน `textDocument/semanticTokens` เซิร์ฟเวอร์รายงาน **ตำแหน่งที่ตัวตรวจสอบแก้ไขเป็นชื่อชนิดจริง ๆ**
เพราะนี่ไม่ใช่การจับคู่ข้อความ:

- ชนิดที่มาจากไฟล์อื่นผ่าน `use` ก็ถูกระบายสี (ช่วงที่การแก้ไขภายในบัฟเฟอร์
  เข้าถึงไม่ได้ในหลักการ)
- การเรียก **ฟังก์ชัน** ที่ชื่อเหมือนชนิดไม่ถูกระบายสี (ตัวตรวจสอบแก้ไข
  มันเป็นฟังก์ชัน จึงไม่มีการบันทึก token ที่นั่นตั้งแต่แรก)

ฝั่งไคลเอนต์:

- **`eglot` (Emacs 31 ขึ้นไป)**: eglot วาด token เอง (`eglot-semantic-tokens-mode`)
  `typelisp-mode` ไม่ยุ่ง
- **`eglot` (Emacs 30 และรุ่นก่อน)**: eglot รุ่นนี้ไม่จัดการ semanticTokens ดังนั้น
  **`typelisp-mode` ส่งคำขอเองและวาดผลลัพธ์ด้วย overlay**
  (`typelisp-semantic-tokens-mode` เปิดโดยอัตโนมัติเมื่อ eglot เชื่อมต่อ)
- **`lsp-mode`**: รองรับในตัว (ตั้ง `lsp-semantic-tokens-enable` เป็น `t`) ในกรณีนั้น
  `typelisp-mode` ไม่ยุ่ง

`scripts/emacs-semantic-smoke.el` เชื่อมต่อผ่าน eglot จริงและตรวจสอบฝั่งที่วาดใน Emacs ที่ใช้อยู่
กับไคลเอนต์ใดก็ตาม fallback ในบัฟเฟอร์จะถอยออกขณะที่เซิร์ฟเวอร์
ตอบอยู่ (เพื่อไม่ให้กฎสองชุดระบายสีบัฟเฟอร์เดียวกัน)

| การตั้งค่า | ค่าเริ่มต้น | ทำอะไร |
|---|---|---|
| `typelisp-semantic-tokens` | `t` | กับ eglot ของ Emacs 30 และรุ่นก่อน ระบายสีจาก semantic token ของเซิร์ฟเวอร์หรือไม่ |
| `typelisp-semantic-tokens-idle-delay` | `0.6` | วินาทีที่ว่างหลังการแก้ไขก่อนขอใหม่ (ให้มากกว่า `eglot-send-changes-idle-time`) |

## หมายเหตุ

- typelisp ทำสัญลักษณ์เป็นตัวพิมพ์เล็กเมื่ออ่าน แต่การระบายสีแยกตัวพิมพ์ใหญ่เล็กเพื่อให้ชื่อชนิดที่
  ขึ้นต้นด้วยตัวพิมพ์ใหญ่แยกออกได้
- การเยื้องถูกตัดสินโดย `typelisp-indent-function` โดยเฉพาะ ซึ่งค้นหา
  `typelisp-indent-specs` (alist) โหมดเก็บรายการของตัวเองไว้แม้สำหรับฟอร์มที่ชื่อ
  ตรงกับ Emacs Lisp (`defun` `let` `if` ...) เพราะ property ของสัญลักษณ์เป็น **โกลบอล** และ
  การตั้งค่า typelisp ที่นั่นจะเปลี่ยนการเยื้องของบัฟเฟอร์ Lisp อื่นในเซสชันเดียวกัน
  และฟอร์มของ typelisp มีรูปร่างต่างกันแม้ชื่อเหมือน Emacs Lisp:
  `(defun NAME (PARAMS) RETTYPE ...)` มีสามสมาชิกในส่วนหัว และ `if` กำหนดตายตัวที่สาม
  สมาชิกโดยมี `else` บังคับ ดังนั้นแบ่งปันค่าก็ไม่ได้เช่นกัน
  ทุกไฟล์ `.typl` ใต้ `examples/` ถูกตรวจสอบแล้ว: `indent-region` ไม่เปลี่ยนแม้แต่ไบต์เดียว
  และการทำให้การเยื้องทั้งหมดแบนแล้วเยื้องใหม่คืนค่าเดิม (เวอร์ชัน VS Code ผ่านมาตรฐานเดียวกัน
  บนไฟล์เดียวกัน)

## การตรวจจับความคลาดเคลื่อนของการนิยามในโปรแกรมแก้ไขข้อความ

ตารางคีย์เวิร์ดถูกดูแลสองที่ ที่นี่และในเวอร์ชัน VS Code เพื่อป้องกันไม่ให้
การนิยามในโปรแกรมแก้ไขข้อความล้าหลังขณะที่ implementation เดินหน้า มีการทดสอบฝั่ง
Rust:

```sh
cargo test --test editor_keyword_sync_test
```

มันโหลด prelude จริง เดินผ่านทะเบียน และรายงาน **ชื่อที่โปรแกรมแก้ไขข้อความตัวใดตัวหนึ่งไม่
รู้จัก** ฟอร์มพิเศษไม่มีการแสดงผลตอนรัน จึงถูกอ่านจากระหว่าง
`// SPECIAL-FORM DISPATCH BEGIN` / `END` ใน `crates/typelisp-front/src/check/checker.rs` (อย่า
ลบคอมเมนต์เหล่านี้) หากล้มเหลว ให้เพิ่มชื่อที่รายงานลงในการนิยามของ **ทั้งสอง** โปรแกรมแก้ไขข้อความ

การทดสอบเดียวกันนี้ยังเปรียบเทียบ legend ของ semantic token ด้วย (`SEMANTIC_TOKEN_TYPES` ใน
`src/bin/lsp.rs` และตารางที่ทั้งสองโปรแกรมแก้ไขข้อความมีต้องตรงกันทั้งชื่อและลำดับ) ความไม่ตรงกันไม่ก่อให้เกิด
ข้อผิดพลาดตอนรัน มันเพียงสลับสีของทุก token ดังนั้นจึงตรึงไว้โดยอัตโนมัติ
