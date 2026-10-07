<!-- translated-from: editor/vscode/README_JP.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# typelisp (VS Code)

ส่วนขยาย VS Code สำหรับแก้ไขซอร์ส typelisp (`.typl`)
เวอร์ชัน Emacs อยู่ใน [../emacs/](../emacs/README_th.md) ทั้งสองใช้ตารางคีย์เวิร์ดชุดเดียวกันและ
กฎการเยื้องชุดเดียวกัน และ `cargo test --test editor_keyword_sync_test` ตรวจสอบเรื่องนี้
โดยอัตโนมัติ (ดูด้านล่าง)

## ความสามารถ

- **การระบายสีไวยากรณ์** (ไวยากรณ์ TextMate; ไม่ต้องใช้ language server)
  - ฟอร์มพิเศษและโครงสร้างควบคุม (`defun` `let` `if` `match` `loop` `lambda` `setf` `as`
    `apply`, `print`/`println`/`format`, ตระกูล `pprint`)
  - ชื่อที่นิยาม (`(defun NAME ...)` เป็นฟังก์ชัน, `(defstruct NAME ...)` เป็นชนิด,
    `(defvar (NAME ...))` เป็นตัวแปร; เช่นเดียวกันเมื่อมี `pub` เช่น `(pub defun NAME ...)`) และ
    ทั้งสองชื่อของ `(impl Trait Type)`
  - คีย์เวิร์ดของเนมสเปซและการประกาศ (`pub` `module` `use` `load` `impl` `where`) และตัวบอกรายการพารามิเตอร์
    ของ lambda (`&rest` `&optional` `&key`)
  - ฟังก์ชันที่มีให้ในตัว ชนิดพื้นฐาน (รวม `bignum` / `ratio`) ชนิดข้อผิดพลาดที่มีให้ในตัว
    ชนิดที่ผู้ใช้นิยามซึ่งขึ้นต้นด้วยตัวพิมพ์ใหญ่ (`Capitalized`) และชนิด trait object `:dyn Trait` (ภายในอาร์กิวเมนต์
    generic ด้วย)
  - literal ตัวเลข (ฐานสิบ / `0xff` / `1.5` / `3.0e10` / `1/3`) literal อักขระอย่าง
    `#\Space` คีย์เวิร์ดอย่าง `:name` และตัวแปรโกลบอลที่มีเครื่องหมายดาวอย่าง `*print-pretty*`
  - **คำสั่งควบคุมของ `format` ภายในสตริง** (`~a` `~5,'0d` `~{...~}` `~^` และอื่น ๆ)
  - คอมเมนต์บรรทัด `;` และคอมเมนต์บล็อก `#| ... |#` ที่ **ซ้อนกันได้**
- **การใช้ชนิดที่ผู้ใช้นิยาม** (semantic token)
  - ชื่อของ `defstruct` / `defenum` / `deftrait` มักเป็นตัวพิมพ์เล็ก (`rect` `todo-item`
    `board`) ดังนั้นกฎ `Capitalized` จับไม่ได้ และไวยากรณ์ TextMate ทำงานทีละบรรทัดและ
    มองไม่เห็นทั้งไฟล์ semantic token มองเห็น ซึ่งแก้สภาพที่ภาษา
    ที่มีระบบชนิดแบบสถิตระบายสีเฉพาะคำอธิบายชนิดของมัน
  - เมื่อเชื่อมต่อกับ `typl-lsp` ส่วนขยายจะได้รับ **ตำแหน่งที่ตัวตรวจสอบแก้ไขเป็นชื่อชนิดจริง ๆ** ดังนั้น
    ชนิดที่มาจากไฟล์อื่นผ่าน `use` ก็ถูกระบายสี และการเรียก **ฟังก์ชัน** ที่ชื่อเหมือนชนิดจะไม่ถูกระบายสี
    (ตัวตรวจสอบแก้ไขมันเป็นฟังก์ชัน จึงไม่มีการบันทึก token ที่นั่นตั้งแต่แรก)
  - เมื่อไม่ได้เชื่อมต่อเซิร์ฟเวอร์หรือยังไม่ได้บิลด์ ส่วนขยายถอยไปใช้การสแกนข้อความที่
    แก้ไขภายในไฟล์ นั่นเป็นการประมาณ: หาชนิดจากไฟล์อื่นไม่ได้
    และแยกฟังก์ชันที่ชื่อเหมือนชนิดไม่ได้
- **การเยื้องแบบ Lisp** (VS Code ไม่มีการเยื้อง Lisp ในตัว ส่วนขยายจึง implement เอง)
  - Format Document, Format Selection และ format on type (Enter และ `)` เมื่อ
    เปิด `editor.formatOnType`)
- **Outline / breadcrumb / `Ctrl+Shift+O`** (ฟังก์ชัน เมทอด แมโคร ชนิด trait `impl`
  ตัวแปร โมดูล)
- **การทำงานร่วมกับ `typl-lsp`** (การวินิจฉัย hover ไปยังจุดนิยาม การเติมข้อความอัตโนมัติ semantic token)
- **คำสั่ง CLI ของ `typl`** (รัน, REPL)

ทุกอย่างยกเว้น language server ทำงานได้ด้วยส่วนขยายเพียงอย่างเดียว ดังนั้นแม้ใน checkout
ที่ยังไม่ได้บิลด์ `typl-lsp` การระบายสี การเยื้อง Outline และการระบายสีชนิด (ภายในไฟล์)
ก็ใช้ได้

## การติดตั้ง

ส่วนขยายไม่ได้อยู่บน Marketplace ดังนั้นให้บิลด์ในเครื่องแล้วติดตั้ง

```sh
cd editor/vscode
npm install
npm run compile
```

จากนั้นเลือกอย่างใดอย่างหนึ่ง:

- **ลองใน development host**: เปิด `editor/vscode` ใน VS Code แล้วกด `F5`
- **ติดตั้งถาวร**: สร้าง `.vsix` ด้วย `npx @vscode/vsce package` แล้วใช้
  "..." → "Install from VSIX..." ในมุมมอง Extensions

ไฟล์ `.typl` เปิดในโหมด typelisp โดยอัตโนมัติ

## ปุ่มลัด

| ปุ่ม | คำสั่ง | ทำอะไร |
|---|---|---|
| `Ctrl+Alt+R` | `typelisp.runFile` | บันทึกและรัน `typl FILE` |
| `Ctrl+Alt+Z` | `typelisp.repl` | เริ่ม REPL ของ `typl` |

command palette ยังมี `typelisp: Restart Language Server` ด้วย

## การตั้งค่า

| การตั้งค่า | ค่าเริ่มต้น | ทำอะไร |
|---|---|---|
| `typelisp.program` | `typl` | พาธของ CLI `typl` |
| `typelisp.languageServer.enable` | `true` | เชื่อมต่อกับ `typl-lsp` หรือไม่ |
| `typelisp.languageServer.path` | (ว่าง) | พาธของ `typl-lsp` เมื่อว่าง ส่วนขยายจะค้นหา `target/release/typl-lsp` ของ workspace ก่อน แล้ว `target/debug/typl-lsp` แล้ว `PATH` |
| `typelisp.trace.server` | `off` | บันทึกทราฟฟิก LSP JSON-RPC |

บิลด์ language server ด้วย:

```sh
cargo build --release --bin typl-lsp
```

การอ้างอิงข้ามไฟล์ผ่าน `use` ถูกแก้ไขโดยค้นหาขึ้นไปหา `typelisp.toml` ของรากโปรเจกต์ (รายละเอียดดู
[เอกสารอ้างอิงไวยากรณ์ 3.11](../../docs/th/reference/syntax.md#311-ไฟล์และโมดูล-โปรเจกต์หลายไฟล์))

## ตัวจับคู่ปัญหาของ task

ส่วนขยายจัดให้มีตัวจับคู่ปัญหา (problem matcher) ชื่อ `typelisp` `typl` พิมพ์การวินิจฉัยในรูปแบบ
`error: FILE:LINE:COL: message` ดังนั้นส่งไปยังแผง Problems ได้โดยตรง:

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

## การพัฒนา

```sh
npm run compile   # tsc
npm run watch     # build on change
npm test          # node --test (grammar, indentation, symbols, type references, manifest)
```

การทดสอบครอบคลุมเฉพาะส่วนที่ไม่ต้องใช้โมดูล `vscode` เพื่อสิ่งนั้น `src/indent.ts` และ
`src/symbols.ts` เขียนเป็นฟังก์ชันบริสุทธิ์ และมีเพียง `src/extension.ts` ที่แตะ API ของโปรแกรมแก้ไขข้อความ

- `src/test/grammar.test.ts` — แยกโทเค็นด้วยไวยากรณ์จริง โดยใช้เอนจินเดียวกับ VS
  Code (`vscode-textmate` + `vscode-oniguruma`) และตรวจสอบผลลัพธ์
  Oniguruma ต่างจากนิพจน์ปรกติของ Emacs ในรายละเอียด (ตัวอย่างเช่น มันไม่ถือ
  `]` ที่ต้นของคลาสอักขระเป็น literal) และความต่างเช่นนั้นพบได้เฉพาะโดย
  รันเอนจินจริง
- `src/test/indent.test.ts` — สำหรับทุกไฟล์ `.typl` ใต้ `examples/` กำหนดให้
  **การทำให้การเยื้องทั้งหมดแบนแล้วคืนค่า ตรงกับเนื้อหาที่ commit ไว้ทุกไบต์**
  โหมด Emacs ผ่านมาตรฐานเดียวกันบนไฟล์เดียวกัน และนั่นคือสิ่งที่ทำให้ "สองโปรแกรมแก้ไขข้อความ
  ตรงกัน" เป็นข้อกล่าวอ้างที่ผ่านการตรวจสอบ
  นอกจากนี้ `src/test/fixtures/emacs-indent-reference.txt` เป็นเอาต์พุตอ้างอิงที่เก็บโดย
  รัน `indent-region` จริงในบัฟเฟอร์ `typelisp-mode` ของ Emacs ด้านที่คาดหวังไม่ใช่
  การพูดซ้ำของ implementation TS แต่เป็น **สิ่งที่โปรแกรมแก้ไขข้อความอีกตัวสร้างจริง ๆ** ดังนั้น
  ความซื่อตรงของการพอร์ตจึงถูกตรวจสอบโดยตรง (รวม `let*` `do` `doiter` `labels` `impl`
  `pprint-logical-block` prefix ของ quote และอื่น ๆ)
- `src/test/symbols.test.ts` — เนื้อหาของ Outline และการตรวจจับการอ้างอิงชนิดของ
  fallback จำนวนการนิยามต้องตรงกับการนับอิสระของฟอร์มการนิยามที่ต้นบรรทัดพอดี
  กฎขอบเขตของการอ้างอิงชนิดถูกจัดให้ตรงกับ fallback ของเวอร์ชัน Emacs โดยเจตนา (VS Code ใช้ lookbehind;
  Emacs แสดงเซตเดียวกันด้วยการใช้อักขระนำหน้าหนึ่งตัว)
- token ที่ขับเคลื่อนด้วยการแก้ไขชื่อของเซิร์ฟเวอร์ (`crates/typelisp-front/src/check/semantic.rs`) ถูก
  ตรวจสอบโดย `cargo test --test lsp_semantic_test` และ `scripts/lsp-semantic-smoke.py` (ซึ่ง
  ขับโพรเซสจริงผ่าน stdio) ไคลเอนต์ Emacs ถูกตรวจสอบโดย
  `scripts/emacs-semantic-smoke.el` ผ่านการเชื่อมต่อ eglot จริง
- `src/test/manifest.test.ts` — `package.json` เป็นส่วนเดียวที่คอมไพเลอร์ไม่ตรวจสอบ จึงตรวจสอบ
  ว่าคำสั่งที่ประกาศกับการเรียก `registerCommand` เป็นเซตเดียวกัน ปุ่มลัด
  อ้างถึงอะไร การตั้งค่าที่โค้ดอ่านถูกประกาศ และ problem matcher
  อ่านสิ่งที่ `typl` พิมพ์จริงได้

### การตรวจจับความคลาดเคลื่อนของการนิยามในโปรแกรมแก้ไขข้อความ

ตารางคีย์เวิร์ดถูกดูแลสองที่ ในเวอร์ชัน Emacs และเวอร์ชัน VS Code เพื่อป้องกันไม่ให้
การนิยามในโปรแกรมแก้ไขข้อความล้าหลังขณะที่ implementation เดินหน้า มีการทดสอบฝั่ง Rust:

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

## หมายเหตุ

- typelisp ทำสัญลักษณ์เป็นตัวพิมพ์เล็กเมื่ออ่าน แต่การระบายสีแยกตัวพิมพ์ใหญ่เล็กเพื่อให้ชื่อชนิดที่
  ขึ้นต้นด้วยตัวพิมพ์ใหญ่แยกออกได้
- การเยื้องถูกตัดสินโดย `INDENT_SPECS` ใน `src/indent.ts` เป็นการพอร์ตของ
  `typelisp-indent-specs` ของเวอร์ชัน Emacs ด้วยค่าและกฎเดียวกัน จุดที่ฟอร์มมีรูปร่างต่างจาก
  ฟอร์ม Emacs Lisp ชื่อเดียวกันถูกส่งต่อตามที่เป็น: ส่วนหัวของ
  `(defun NAME (PARAMS) RETTYPE ...)` มีสามสมาชิก `if` กำหนดตายตัวที่สามสมาชิกโดยมี `else`
  บังคับ และอื่น ๆ
- เนื้อหาของ `#| ... |#` ถูกเยื้องใหม่เมื่อจัดรูปแบบ ตรงกับพฤติกรรมของ
  `indent-region` ของ Emacs
