<!-- translated-from: docs/ja/guide/editors.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# การใช้งานร่วมกับโปรแกรมแก้ไขข้อความ (typl-lsp)

`typl-lsp` คือ language server ของ typelisp เมื่อเชื่อมต่อกับโปรแกรมแก้ไขข้อความที่รองรับ LSP (Language
Server Protocol) จะให้ความสามารถเหล่านี้กับไฟล์ที่คุณกำลังแก้ไข

- การวินิจฉัย (diagnostics): ข้อผิดพลาดในการอ่าน, ข้อผิดพลาดของชนิด และคำเตือนการนิยามซ้ำ
- Hover: ชนิดของนิพจน์ที่อยู่ในวงเล็บ และ docstring ของการนิยามที่นิพจน์นั้นเรียกใช้ (ไม่แสดงสำหรับ
  ชื่อตัวแปรเดี่ยว ๆ)
- ไปยังจุดนิยาม (go to definition)
- การเติมข้อความอัตโนมัติ (ตัวเลือกจะปรากฏเมื่อพิมพ์ `:`)
- การระบายสีชื่อชนิด (semantic tokens) รวมถึงชนิดที่ `use` มาจากไฟล์อื่น

การอ้างอิงข้ามไฟล์ผ่าน `use` จะถูกแก้ไขให้ชี้ถูกต้อง การแก้ไขที่ยังไม่ได้บันทึกในไฟล์อื่นที่เปิดอยู่
จะสะท้อนในการวินิจฉัยของไฟล์ที่ `use` ไฟล์นั้นทันที

## 1. การบิลด์

```sh
cargo build --release --bin typl-lsp
```

คำสั่งนี้สร้าง `target/release/typl-lsp` หากคุณติดตั้งด้วย `cargo install` ตามที่อธิบายใน
[README.md](../../../README.md) ไฟล์นี้จะอยู่ที่ `~/.cargo/bin/typl-lsp` พร้อมกับ `typl`

## 2. VS Code

ส่วนขยายอยู่ใน `editor/vscode` ของรีโพซิทอรี ส่วนขยายนี้ไม่ได้เผยแพร่บน Marketplace ดังนั้น
ให้บิลด์และติดตั้งด้วยตนเอง

```sh
cd editor/vscode
npm install
npm run compile
npx @vscode/vsce package      # produces a .vsix
```

เลือก "Install from VSIX..." จากเมนู "..." ของมุมมอง Extensions แล้วเลือกไฟล์ `.vsix` ที่คุณ
บิลด์ไว้

ส่วนขยายจะค้นหา `typl-lsp` ใน `target/release/typl-lsp` ของ workspace ก่อน จากนั้นจึงเป็น
`target/debug/typl-lsp` แล้วจึงค้นใน `PATH` หากวางไว้ที่อื่น ให้เขียนพาธของไฟล์ในการตั้งค่า
`typelisp.languageServer.path`

| การตั้งค่า | ค่าเริ่มต้น | ความหมาย |
|---|---|---|
| `typelisp.program` | `typl` | พาธของ `typl` |
| `typelisp.languageServer.enable` | `true` | เชื่อมต่อกับ `typl-lsp` หรือไม่ |
| `typelisp.languageServer.path` | (ว่าง) | พาธของ `typl-lsp` |

`Ctrl+Alt+R` บันทึกไฟล์ที่กำลังแก้ไขแล้วรันด้วย `typl` และ `Ctrl+Alt+Z` เริ่ม REPL
รายละเอียดเพิ่มเติมดู[README ของส่วนขยาย VS Code](../../../editor/vscode/README_th.md)

## 3. Emacs

`typelisp-mode` อยู่ใน `editor/emacs` ของรีโพซิทอรี

```elisp
(add-to-list 'load-path "/path/to/typelisp/editor/emacs")
(require 'typelisp-mode)
```

การตั้งค่าเพื่อเชื่อมต่อกับ `typl-lsp` ด้วย `eglot` (มาพร้อมกับ Emacs 29 ขึ้นไป):

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

eglot ของ Emacs 31 ขึ้นไประบายสีชื่อชนิด (semantic tokens) ได้เอง ส่วน eglot ของ Emacs 30 และ
รุ่นก่อนหน้าไม่รองรับ `typelisp-mode` จึงระบายสีชื่อชนิดแทน เมื่อใช้ `lsp-mode` ให้ตั้ง
`lsp-semantic-tokens-enable` เป็น `t`

`C-c C-c` รันไฟล์ที่กำลังแก้ไข และ `C-c C-z` เริ่ม REPL รายละเอียดเพิ่มเติมดู
[README ของ typelisp-mode](../../../editor/emacs/README_th.md)

## 4. โปรแกรมแก้ไขข้อความอื่น

`typl-lsp` สื่อสาร LSP ผ่านอินพุตและเอาต์พุตมาตรฐาน และไม่รับอาร์กิวเมนต์บรรทัดคำสั่ง ให้ตั้งค่า
ไคลเอนต์ LSP ของโปรแกรมแก้ไขข้อความให้เริ่ม `typl-lsp` สำหรับไฟล์ `.typl`

## 5. วิธีที่โปรเจกต์ถูกระบุ

`typl-lsp` ค้นหา `typelisp.toml` โดยเริ่มจากไดเรกทอรีของไฟล์ที่เปิดและไล่ขึ้นไปทางไดเรกทอรีแม่
แล้วแก้ไข `use` โดยถือตำแหน่งนั้นเป็นรากของซอร์ส (source root) กฎนี้เหมือนกับตอนที่ `typl` รัน
ไฟล์ ([โมดูลและการจัดวางไฟล์](modules.md#2-การตั้งค่าโปรเจกต์)) สำหรับโปรเจกต์ที่ประกอบด้วยหลายไฟล์
ให้วาง `typelisp.toml` ไว้ที่รากของโปรเจกต์

## 6. language server ไม่รันโปรแกรมของคุณ

`typl-lsp` สร้างการวินิจฉัยด้วยการอ่านและตรวจสอบชนิดเท่านั้น และไม่เคยรันโปรแกรมที่คุณกำลัง
แก้ไข การวินิจฉัยทำงานทุกครั้งที่กดแป้นพิมพ์ จึงไม่อาจรันโค้ดที่มีผลข้างเคียงหรือโค้ดที่ไม่มีวัน
จบได้ ข้อยกเว้นอย่างเดียวคือการลงทะเบียน `defmacro` ซึ่งจำเป็นต่อการตรวจสอบการเรียกแมโครที่ตามมา

ด้วยเหตุนี้ ข้อผิดพลาดที่เกิดขึ้นเฉพาะตอนที่ `typl` รันโปรแกรม (`panic`, ไม่พบไฟล์ และอื่น ๆ)
จะไม่ปรากฏในการวินิจฉัยของ language server
