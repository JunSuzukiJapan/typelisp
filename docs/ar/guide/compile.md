<!-- translated-from: docs/ja/guide/compile.md @ be56bb8a477929497064b8f807acfd1755f2b35b -->
# الترجمة

إذا لم تفعل شيئًا آخر فإن برامج typelisp تعمل في المفسِّر (interpreter). وإضافة إلى ذلك توجد طريقتان
للترجمة إلى شيفرة أصلية (native code) وطريقة واحدة لحفظ بيئة. والتفاصيل الكاملة للمواصفة موجودة في
[الفصل 10 من مرجع الصياغة](../reference/syntax.md#10-الترجمة).

| الطريقة | كيف | النتيجة |
|---|---|---|
| ترجمة JIT | `(compile name)` | تُستبدل دالة في الجلسة الجارية بشيفرة أصلية |
| ترجمة AOT | `typl -c src.typl` أو `(compile-file "src.typl" "out")` | ملف تنفيذي مستقل |
| الـ dump | `(dump "file.typld")` | يحفظ التعريفات؛ ويبدأ `typl --image` من جديد من البيئة نفسها |

## 1. التحضير

تستخدم الترجمة LLVM 22. وإذا بنيت `typl` باتباع [README.md](../../../README.md) فلا حاجة إلى أي تحضير
إضافي.

تُربَط الملفات التنفيذية الناتجة عن ترجمة AOT بالمكتبة الساكنة `libtypelisp_front.a`. والإصدار الذي
بُني بوضع release من `typl` (بما في ذلك المثبَّت بـ `cargo install`) يحمل هذه المكتبة في داخله، فلا
حاجة إلى تحضير. وفي أول ترجمة يكتب المكتبة إلى `~/.typelisp/lib/<معرّف البناء>/` (أو إلى
`$TYPELISP_HOME/lib/<معرّف البناء>/` إذا كان المتغير البيئي `TYPELISP_HOME` مضبوطًا) ويستخدم تلك
النسخة بعد ذلك. ويحذفها الأمر `typl --remove-lib` (ومع `--others` تُحذف النسخ التي كتبتها إصدارات
أخرى من `typl`، ومع `--all` تُحذف كلها). أما الإصدار الذي بُني بوضع debug من `typl` فيستخدم المكتبة
الموجودة في `target/debug/` من المستودع الذي بُني فيه. ولاستخدام مكتبة موضوعة في مكان آخر مرّر
مجلدها بـ `--lib-dir` عند تشغيل `typl` (القسم 3.2). وعلى macOS يستخدم الربط أدوات Xcode Command Line
Tools.

## 2. ترجمة JIT

تحوّل هذه الترجمة دالة معرَّفة بالفعل إلى شيفرة أصلية في الحال.

```lisp
(defun fib ((n int)) int
  (if (< n 2) n (+ (fib (- n 1)) (fib (- n 2)))))

(compile fib)
(fib 30)           ; من هنا فصاعدًا تنفّذ الاستدعاءات الشيفرة المترجمة
```

- لا يُقيَّم `name`. اكتب اسم الدالة كما هو (وليس كسلسلة نصية). وللتابع (method) اكتبه مع اسم النوع كما
  في `(compile point::norm)`.
- تُترجم الدوال التي تستدعيها معها.
- **لا يمكن ترجمة الدوال العامة (generic).** إذ تُنشأ نسخة لكل نوع في كل موضع استخدام. ترجم بدلًا من
  ذلك الدالة التي تستدعيها بأنواع محددة.
- `trace` و`step` و`disassemble` و`compile` و`compile-file` و`dump` عمليات خاصة بالمفسِّر، فلا يمكن
  ترجمة دالة تستدعيها. ومحاولة ترجمتها تعطي خطأ يبيّن السبب.

لرؤية نتيجة الترجمة استخدم `disassemble`.

```lisp
(disassemble fib)          ; شيفرة الآلة المضيفة
(disassemble fib true)     ; LLVM IR
```

## 3. بناء ملف تنفيذي بالترجمة المسبقة AOT

### 3.1 كتابة البرنامج

عرّف كنقطة دخول **دالة `main` لا تأخذ وسائط**.

```lisp
(use std::env::command-line-args)
;; hello.typl
(defun fib ((n int)) int
  (if (< n 2) n (+ (fib (- n 1)) (fib (- n 2)))))

(defun main () ()
  (println "args: ~a" (command-line-args))
  (println "fib(25) = ~a" (fib 25)))

(main)
```

وجود `(main)` في آخر الملف هو لكي تُستدعى `main` عند تشغيل `typl hello.typl`. ويتخطى
`compile-file` هذا `(main)` الأخير، لذا يصلح الملف نفسه للمفسِّر ولترجمة AOT معًا.

### 3.2 الترجمة

من سطر الأوامر استخدم `typl -c` (و`typl --compile` مثله).

```sh
$ typl -c hello.typl            # ينتج hello
$ typl -c hello.typl -o fib     # يسمّي الملف التنفيذي fib
$ ./hello a b
args: #(./hello a b)
fib(25) = 75025
```

بدون `-o` يُسمّى الملف التنفيذي باسم ملف المصدر بعد حذف `.typl` ويوضع في المجلد نفسه الذي فيه ملف
المصدر. وإذا لم ينتهِ اسم ملف المصدر بـ `.typl` فإن `-o` مطلوب. ومع `-c` (`--compile`) لا يمكن
تمرير `--image` و`--feature`.

مع `--release` يصنع `typl` **بناء إصدار**، يزيل `eprint`/`eprintln` (مخرجات الأخطاء القياسية للتحقيق في المشكلات؛ [الطباعة](../reference/functions/printing.md)) مع وسائطها. وما يُزال يخضع مع ذلك لفحص الأنواع، فإن وُجد فيه خطأ في الأنواع تفشل الترجمة. `--release` خيار لـ `typl` ككل: يعمل بالطريقة نفسها لا مع `-c` فحسب، بل أيضًا عند تشغيل ملف، وفي REPL، ومع `typl test`. و`compile-file` الذي يُستدعى داخل `typl` شُغّل بـ `--release` يصنع هو أيضًا بناء إصدار، وتُزال `eprint`/`eprintln` كذلك من الشيفرة التي تُنفَّذ بـ `eval` داخل الملف التنفيذي المصنوع بهذه الطريقة.

```sh
$ typl --release -c hello.typl
$ typl -c hello.typl --release     # الأمر نفسه إذا كُتب بعد -c
```

تحدد `-O0` و`-O1` و`-O2` و`-O3` **مستوى التحسين**. الافتراضي هو `-O0`، وهو لا يحسّن. ابتداءً من `-O1` يجري المصرّف أولًا تحسينات بسيطة، مثل استبدال متغير مربوط بقيمة حرفية فقط بقيمته، أو حساب العمليات الحسابية الصحيحة على الثوابت مسبقًا، ثم يترك الباقي لتحسين LLVM في المستوى الذي يحمل الاسم نفسه. نتائج الحساب واحدة في كل المستويات. ومثل `--release`، هو خيار لـ`typl` ككل: فإلى جانب الملف التنفيذي الذي ينشئه `-c`، يسري على الشيفرة التي تُصرَّف أثناء تشغيل ملف وفي REPL وفي `typl test`، وعلى `compile-file` المستدعى هناك. أما الشيفرة التي ينفذها المفسّر فلا تُحسَّن.

```sh
$ typl -O2 -c hello.typl
$ typl -c hello.typl -O2     # الأمر نفسه إذا كُتب بعد -c
```

ويمكنك فعل الشيء نفسه باستدعاء `compile-file` من الـ REPL أو من برنامج.

```sh
$ typl
typl> (compile-file "hello.typl" "hello")
typl> :quit
$ ./hello a b
args: #(./hello a b)
fib(25) = 75025
```

وإذا كنت تبني مرارًا فيمكنك وضع هذا السطر الواحد في ملف وتشغيله بـ `typl build.typl`.

```lisp
;; build.typl
(compile-file "hello.typl" "hello")
```

تُحَلّ أسماء الملفات انطلاقًا من **المجلد الحالي الذي بدأ فيه `typl`**، لا من موقع `build.typl`.

ولربط `libtypelisp_front.a` موضوعة في مكان غير الذي يبحث فيه `typl` مرّر مجلدها بـ `--lib-dir`.
وينطبق هذا على `typl -c` وعلى `compile-file` معًا.

```sh
$ typl --lib-dir ~/lib/typelisp -c hello.typl
$ typl --lib-dir ~/lib/typelisp build.typl
```

وإذا لم يكن في المجلد المعطى `libtypelisp_front.a` توقف `typl` بخطأ. والملف لا يعمل إلا مع `typl` الذي
بُني معه. وبعد إعادة بناء `typl` انسخه من جديد.

### 3.3 ما يجوز أن يحتويه ملف مترجم بـ AOT

- لا يجوز أن يحتوي المستوى الأعلى من ملف الدخول إلا على تعريفات (`defun` و`defvar`
  و`defparameter` و`defconstant` و`defmacro` و`defsignature` و`defstruct` و`defenum` و`deftype`
  و`deftrait` و`impl` و`defffi` و`(unsafe (def-c-struct ...))`) وعلى `use` و`module`. أما تعبيرات
  المستوى الأعلى مثل `(println ...)` فغير مسموحة، عدا `(main)` الأخيرة. ضع العمل داخل `main`.
- بدون `main` لا تأخذ وسائط تفشل الترجمة بخطأ.
- تُترجم أيضًا ملفات الوحدات المستورَدة بـ `use` وتُدمج في ملف تنفيذي واحد.
- تُربط تلقائيًا المكتبات المسمّاة بـ `:library` في `defffi` ([واجهة C الخارجية](ffi.md)).
- يمكن استخدام كل دالة في المكتبة القياسية مع ترجمة AOT. ويمكن استخدام `eval` أيضًا، لكن عندئذ يدخل
  فاحص الأنواع والمفسِّر في الملف التنفيذي، فيكبر حجمه ويبطؤ بدؤه. أما البرامج التي لا تستدعي `eval`
  فلا تتضمنهما.

### 3.4 سلوك الملف التنفيذي

- يعيد `(command-line-args)` قيمة `Vector<string>` بالشكل نفسه سواء شُغّل بـ `typl hello.typl a b` أو
  بـ `./hello a b`. والعنصر الأول هو اسم البرنامج.
- اضبط رمز الخروج بـ `(exit n)`. وإذا عادت `main` عودة عادية فهو 0.
- عند `panic` يطبع البرنامج الرسالة ويخرج برمز غير صفري.

### 3.5 الموارد التي يستخدمها الملف التنفيذي

لـ `typl -c` خيارات تحدد الموارد التي يجوز للملف التنفيذي استخدامها أثناء تشغيله.

```sh
$ typl --heap-cells 100000 -c hello.typl    # تثبيت ساحة cons على 100000 خلية
$ typl -c hello.typl --max-memory 64M       # حجز 64 MiB على الأكثر
$ typl -c hello.typl --no-cons              # لا تستخدم خلايا cons
```

- `--heap-cells N` يثبّت سعة ساحة خلايا cons على `N` خلية. لا تكبر الساحة بعدها: إذا لم يستطع جمع القمامة تحرير أي خلية، يتوقف البرنامج بخطأ وقت التشغيل. ومن دون الخيار تبدأ الساحة بـ 65536 خلية وتكبر عند الحاجة.
- `--max-memory SIZE` يحدّ مجموع الذاكرة التي يحجزها الملف التنفيذي. يُحسب كل شيء: ساحة cons والسلاسل النصية والقيم مثل المتجهات وجداول التجزئة والذاكرة التي يستخدمها وقت التشغيل نفسه. `SIZE` عدد من البايتات ويجوز أن يليه `K` أو `M` أو `G` (1024 و1024² و1024³ ضعفًا). يُبلَّغ عن أي حجز يتجاوز الحد، وينتهي البرنامج برمز الخروج 1. لا يمكن التقاط هذا الخروج بـ `catch`. ومن دون الخيار لا يوجد حد.
- `--no-cons` ينشئ ملفًا تنفيذيًا لا يستخدم أي خلية cons. ليست له ساحة cons، لذا فالبرنامج الذي يفعل شيئًا يحتاج إلى خلايا cons يُعدّ **خطأ ترجمة** يعرض تلك العملية وسلسلة الدوال من `main` التي تصل إليها. تُحتاج خلايا cons لبناء قائمة `Sexpr` (بما فيها القائمة المقتبسة والقائمة المبنية بشبه الاقتباس) أو وسيط `&rest`، وكذلك لـ `read` و`eval` و`macroexpand`. أما العمليات على الأعداد والسلاسل النصية والمتجهات وجداول التجزئة والبنى والقنوات ونحوها، و`print`/`println`/`format`، فلا تستخدم أيًا منها. لا يمكن تمريره مع `--heap-cells`، ويُكتب بعد `-c`.

يُكتب `--max-memory` بعد `-c`. ويجوز كتابة `--heap-cells` قبل `-c` أو بعده، ويكون حينئذ سعة ساحة cons للملف التنفيذي. تُحسب ساحة cons ضمن حد الذاكرة، فإذا كان الحد لا يسع الساحة توقف البرنامج قبل أن يبدأ.

### 3.6 الترجمة إلى WebAssembly

مع `--target wasm32-wasip1` يُنشئ `typl -c` وحدة WebAssembly (أمر WASI preview 1) بدلًا من ملف تنفيذي. وبدون `-o` يكون اسمها اسم المصدر مع استبدال `.typl` بـ `.wasm`.

```sh
$ typl -c hello.typl --target wasm32-wasip1      # يُنشئ hello.wasm
$ wasmtime hello.wasm a b                        # التشغيل ببيئة تدعم WASI
args: #(hello.wasm a b)
fib(25) = 75025
```

- يمكن استخدام `typl` المثبّت بـ `install.sh` كما هو. أما المثبّت بـ `cargo install` فيحتاج إلى تحضير (يصلح أيضًا للقسم 3.7). نفّذ أولًا `rustup target add wasm32-wasip1 wasm32-unknown-unknown` عبر rustup. ثم ثبّته مع `--features wasm` (`cargo install --locked --path . --features wasm`).
  لا يملك `typl` المثبَّت بدونه بيئة تشغيل WebAssembly، ويكون `--target` خطأً. ولاستخدام نسخة تصحيح من `typl` في المستودع، نفّذ بدلًا من ذلك `scripts/build-wasm-runtime.sh` في المستودع.
- يستخدم الربط رابط سلسلة أدوات rustup (`rust-lld`)، لذا يلزم rustup عند الترجمة أيضًا.
- تتصرف الوحدة كما يتصرف الملف التنفيذي: المخرج القياسي والخطأ القياسي ورمز الخروج، و`(command-line-args)`،
  وقراءة الملفات وكتابتها، و`throw`/`catch`/`panic`، والمهام والقنوات و`sleep`، و`eval` كلها تعمل.
  تعمل المهام واحدة بعد الأخرى في خيط واحد. ولا يمكن قراءة الملفات وكتابتها إلا داخل المجلدات
  التي تعرضها البيئة على الوحدة (في wasmtime تُحدَّد بـ `--dir`).
- البرنامج الذي يستخدم شيئًا لا يملكه WebAssembly هو **خطأ ترجمة**. يعرض الخطأ ما استُخدم
  وسلسلة الدوال من `main` التي تصل إليه. لا يمكن استخدام: المقابس (TCP وUDP ونطاق Unix
  وTLS وتحليل الأسماء)، وخيوط نظام التشغيل (`Thread`)، ودوال C (`defffi`)، و`get-internal-run-time`, `machine-instance`, `software-version`, `file-author`،
  و`ed`.
- لا تملك WASI وسيلة لمعرفة المنطقة الزمنية، لذا فالتوقيت المحلي مجهول. وتُحدث `decode-universal-time` و`encode-universal-time` بدون `zone`، وكذلك `get-decoded-time`، panic وقت التشغيل. وتعمل عند إعطاء `zone`.
- تعمل `--heap-cells` و`--max-memory` و`--no-cons` و`-O<N>` و`--release` كما تعمل مع الملف التنفيذي.

### 3.7 التشغيل في متصفح أو في Node.js

مع `--target wasm32-unknown-unknown` يُنشئ `typl -c` وحدة WebAssembly تُحمَّل وتُشغَّل من JavaScript. وبجانب الوحدة (`hello.wasm`) في المجلد نفسه، يُنشئ ملف JavaScript الذي يحمّلها ويشغّلها (`hello.mjs`). والتحضير هو نفسه كما في القسم 3.6.

```sh
$ typl -c hello.typl --target wasm32-unknown-unknown   # يُنشئ hello.wasm وhello.mjs
$ node hello.mjs                                       # التشغيل بـ Node.js
args: #()
fib(25) = 75025
```

من شيفرة JavaScript أخرى، استدعِ `run` من `hello.mjs`. تتلقى `write` المخرجات نصًّا (`fd` هو 1 للمخرج القياسي و2 للخطأ القياسي). وقيمة `run` هي رمز الخروج.

```js
import { run } from './hello.mjs';
const code = await run({ write: (fd, text) => { /* اعرضه في الصفحة مثلًا */ } });
```

- بدون `write` يكتب Node.js إلى المخرج القياسي والخطأ القياسي، ويكتب المتصفح إلى وحدة التحكم سطرًا سطرًا.
- في المتصفح، قدّم `hello.wasm` عبر HTTP من المكان نفسه الذي فيه `hello.mjs` (لا يمكن تحميله من `file://`).
- ينتظر `sleep` بواسطة `Atomics.wait`. ولا يسمح المتصفح بذلك إلا داخل Worker في صفحة معزولة عبر الأصول، لذا شغّل البرنامج الذي يستخدم `sleep` في Worker.
- يُحسب التوقيت المحلي (`decode-universal-time` بدون `zone` ونحوها) بالمنطقة الزمنية التي يُبلغ عنها `Date` في JavaScript.
- بالإضافة إلى ما لا يمكن استخدامه في القسم 3.6، تكون عمليات الملفات والمجلدات ومتغيرات البيئة و`(command-line-args)` والمجلد الرئيسي و`dribble` **خطأ ترجمة**. يُقرأ الإدخال القياسي فارغًا.

### 3.8 الترجمة لمتحكم دقيق (Cortex-M، RISC-V)

مع `--target thumbv7em-none-eabihf` ينشئ `typl -c` ملفًا تنفيذيًا (ELF) للوحة Cortex-M4F أو M7 بلا نظام تشغيل. بدون `-o` يُسمّى باسم المصدر مع استبدال `.typl` بـ`.elf`. ومع `--target thumbv8m.main-none-eabihf` يكون للوحة Cortex-M33. ومع `--target riscv32imac-unknown-none-elf` يكون للوحة بنواة RISC-V بعرض 32 بت (الامتدادات M وA وC).

```sh
$ typl -c hello.typl --target thumbv7em-none-eabihf          # ينشئ hello.elf
$ qemu-system-arm -M mps2-an386 -nographic \
    -semihosting-config enable=on,target=native -kernel hello.elf   # شغّله على Cortex-M4 في QEMU
$ typl -c hello.typl --target thumbv8m.main-none-eabihf
$ qemu-system-arm -M mps2-an505 -nographic \
    -semihosting-config enable=on,target=native -kernel hello.elf   # شغّله على Cortex-M33 في QEMU
$ typl -c hello.typl --target riscv32imac-unknown-none-elf
$ qemu-system-riscv32 -M virt -bios none -nographic \
    -semihosting-config enable=on,target=native -kernel hello.elf   # شغّله على RISC-V في QEMU
```

- يمكن استخدام `typl` المثبّت بـ`install.sh` كما هو. للتثبيت بـ`cargo install` شغّل `rustup target add thumbv7em-none-eabihf thumbv8m.main-none-eabihf riscv32imac-unknown-none-elf` ثم ثبّت مع `--features embedded` (و`--features wasm,embedded` للاثنين). مع نسخة التصحيح من `typl` في المستودع شغّل `scripts/build-embedded-runtime.sh`. يستخدم الربط رابط سلسلة أدوات rustup.
- مرّر ذاكرة اللوحة بـ`--linker-script FILE`. يُكتب الملف مثل `memory.x` في `cortex-m-rt`: عناوين FLASH وRAM وأحجامها. سطر مثل `_typelisp_stack_size = 32K;` يحدد ما يأخذه المكدس من RAM (64KiB إن لم يُذكر)؛ وبقية RAM هي الكومة. بدون الخيار يُستخدم تخطيط `mps2-an386` (و`mps2-an505` لـ Cortex-M33) في QEMU.

  ```
  MEMORY
  {
    FLASH : ORIGIN = 0x08000000, LENGTH = 512K
    RAM : ORIGIN = 0x20000000, LENGTH = 128K
  }
  _typelisp_stack_size = 16K;
  ```

  على RISC-V يكون الافتراضي تخطيط `virt` في QEMU. والملف الممرَّر يسمّي أيضًا بـ`REGION_ALIAS`، كما في `memory.x` الخاص بـ`riscv-rt`، المنطقة التي يذهب إليها كل نوع من الأقسام.

  ```
  MEMORY
  {
    FLASH : ORIGIN = 0x20010000, LENGTH = 4M - 64K
    RAM : ORIGIN = 0x80000000, LENGTH = 16K
  }
  REGION_ALIAS("REGION_TEXT", FLASH);
  REGION_ALIAS("REGION_RODATA", FLASH);
  REGION_ALIAS("REGION_DATA", RAM);
  REGION_ALIAS("REGION_BSS", RAM);
  REGION_ALIAS("REGION_HEAP", RAM);
  REGION_ALIAS("REGION_STACK", RAM);
  _typelisp_stack_size = 4K;
  ```

- يختار `--board-functions` من يوفّر الإخراج والإدخال والإنهاء والساعة (دوال اللوحة).
  - `semihosting` (الافتراضي): عبر semihosting، للتشغيل تحت QEMU (`-semihosting-config enable=on`) أو مصحّح.
  - `c`: توفّرها اللوحة كدوال C. مرّر بـ`--link FILE` ملف كائن أو مكتبة ثابتة تعرّف الدوال أدناه (يمكن تكرار `--link`؛ وتُمرَّر مكتبة المصنّع التي يستدعيها `defffi` بالطريقة نفسها).

  ```c
  void     typelisp_host_write(int32_t fd, const uint8_t *p, size_t len); // fd 1 الإخراج القياسي، 2 الخطأ القياسي
  size_t   typelisp_host_read(int32_t fd, uint8_t *p, size_t len);        // fd 0؛ عدد البايتات المقروءة، 0 عند النهاية
  void     typelisp_host_exit(int32_t code);                              // لا تعود
  uint64_t typelisp_host_now_us(void);                                    // ميكروثوانٍ على ساعة لا ترجع أبدًا
  int64_t  typelisp_host_unix_us(void);                                   // ميكروثوانٍ منذ 1970-01-01
  void     typelisp_host_sleep_us(uint64_t us);                           // تنتظر us ميكروثانية
  ```

- تُقرأ سجلات اللوحة (الأجهزة الطرفية المعيَّنة في الذاكرة) بـ `volatile-read-u8` و`volatile-read-u16` و`volatile-read-u32`، وتُكتب بـ `volatile-write-u8` و`volatile-write-u16` و`volatile-write-u32`. يُمرَّر العنوان بوصفه `int` أو عددًا صحيحًا ثابت العرض مثل `u16`، وتكون القيمة المقروءة من نوع عرضها (`u8` وما إلى ذلك). كلٌّ منها يقرأ أو يكتب مرة واحدة بالضبط في الموضع الذي كُتب فيه، ولا يحذفه المترجم ولا ينقله. لا تُستخدم جميعها إلا داخل `unsafe`. العنوان السالب، أو الذي ليس من مضاعفات العرض، خطأ وقت التشغيل قبل قراءة أي شيء أو كتابته.

  ```lisp
  ;; UART 0 في لوحة mps2-an386 في QEMU: البيانات 0x40004000، الحالة 0x40004004، التحكم 0x40004008
  (defun put-byte ((c u32)) ()
    (unsafe
      (loop (when (= (logand (volatile-read-u32 #x40004004) 1) 0) (break)))   ; انتظر أثناء الإرسال
      (volatile-write-u32 #x40004000 c)))

  (defun main () ()
    (unsafe (volatile-write-u32 #x40004008 1))                               ; فعِّل الإرسال
    (put-byte 104) (put-byte 105) (put-byte 10))                             ; "hi" وسطر جديد
  ```

- اكتب الدالة التي تُستدعى عند وصول مقاطعة على هيئة `defun` بالسمة `#[interrupt(N)]` (مقاطعة الطرفية `N`) أو سمة مثل `#[exception(SysTick)]`. فعّل المقاطعة بـ `(enable-interrupt N)`. يعمل المعالج فورًا، ولا يعمل بعد انتهاء العمل إلا إذا وصل في منتصف عمل بيئة التشغيل. لا يستطيع المعالج الانتظار (`sleep` خطأ ترجمة). التفاصيل في القسم 3.17 من [مرجع الصياغة](../reference/syntax.md). على RISC-V يكون `N` رقم المقاطعة في `mcause` (من 0 إلى 31: 3 مقاطعة البرمجيات، و7 المؤقت، و11 المقاطعات الخارجية)، ولا يمكن استخدام `#[exception(NAME)]`.

  ```lisp
  (use std::board::enable-interrupt)

  #[interrupt(5)]
  (defun on-five () ()
    (println "interrupt 5"))

  (defun main () ()
    (enable-interrupt 5)
    (unsafe (volatile-write-u32 #xE000E200 (as u32 32))))   ; أحدث المقاطعة 5 بنفسك (سجل المقاطعات المعلقة في NVIC)
  ```

- تبدأ بيئة التشغيل البرنامج (جدول المتجهات وتهيئة RAM) وتخصّص الذاكرة. HardFault (استثناء على RISC-V) أو مقاطعة بلا معالج ينهي البرنامج برمز خروج panic (1).
- يعمل `throw`/`catch`/`panic` والأعداد الكبيرة والكسور والأعداد العشرية والمهام والقنوات و`sleep` كما في الملف التنفيذي. تعمل المهام بالتناوب على النواة الوحيدة. يمكن استخدام دوال C (`defffi`).
- البرنامج الذي يستخدم ما ليس في اللوحة **خطأ ترجمة**: بالإضافة إلى ما لا يمكن استخدامه في القسم 3.7، `eval` و`macroexpand`. الوقت المحلي غير معروف (كما في القسم 3.6).
- تبدأ ساحة cons بـ1024 خلية وتكبر عند الحاجة. تعمل `--heap-cells` و`--max-memory` و`--no-cons` و`-O<N>` و`--release` كما هي.

### 3.9 الترجمة لـ Arduino Uno ‏(AVR)

مع `--target avr-atmega328p` يُنشأ ملف تنفيذي (ELF) للمتحكم الدقيق في Arduino Uno ‏(ATmega328P). البرنامج التالي يجعل مصباح LED على اللوحة يومض.

```lisp
(defconstant (PINB int) #x23)
(defconstant (DDRB int) #x24)
(defconstant (LED u8) 32)                         ; PB5

(defun wait-a-bit () ()
  (let ((i (the u32 0)))
    (while (< i (the u32 400000))
      (unsafe (volatile-read-u8 PINB))
      (setf i (+ i 1)))
    ()))

(defun main () ()
  (unsafe (volatile-write-u8 DDRB LED))           ; اجعله خرجًا
  (while true
    (unsafe (volatile-write-u8 PINB LED))         ; اعكسه
    (wait-a-bit)))
```

```sh
$ typl -c blink.typl --target avr-atmega328p -O2   # ينشئ blink.elf
$ qemu-system-avr -M uno -display none -serial stdio -bios blink.elf   # شغّله في QEMU
```

- لا توجد بيئة تشغيل، لذا فإن البرنامج الذي يستخدمها **خطأ ترجمة**. يذكر الخطأ ما المطلوب وعبر أي دوال يُوصل إليه. ما يمكن استخدامه: حسابات الأعداد الصحيحة ثابتة العرض (من `i8` إلى `u32`) و`f32` و`f64` و`bool` و`char`، واستدعاء الدوال (ومنه الاستدعاء الذاتي)، والشروط، والحلقات، و`defvar` و`defconstant` الحاملان لهذه القيم، و`volatile-read-u8` وما شابهها، واستدعاء دوال C.
- ما لا يمكن استخدامه: القيم الموجودة في الكومة مثل السلاسل والقوائم والبنى؛ `catch` واستدعاء قيم الدوال؛ استدعاء دوال C التي تأخذ سلسلة أو دالة؛ الدوال الرياضية للأعداد العشرية مثل `sin` و`expt` (ويمكن استخدام `sqrt` والتقريب مثل `floor`).
- يُكتب معالج المقاطعة بالصيغة `#[interrupt(N)]` ([الصياغة](../reference/syntax.md)، القسم 3.17). و`N` هو رقم المقاطعة في ATmega328P، من 1 إلى 25 باعتبار إعادة الضبط 0 (فيضان المؤقت 1 هو 13). انظر المثال أدناه.
- القسمة على 0 توقف المتحكم في مكانها (ولا تأتي مقاطعات بعدها). وكذلك `/` لأصغر قيمة ذات إشارة على `-1`.
- لا يمكن استخدام `int` (خطأ ترجمة) لأنه بلا عرض ثابت. استخدم نوعًا ثابت العرض مثل `i16` أو `u16`. ويمكن حساب العناوين أيضًا بأعداد صحيحة ثابتة العرض. وما تتحدد قيمته وقت الترجمة يمكن استخدامه، مثل العنوان المكتوب كرقم و`int` في `defconstant`.
- إذا نفد المكدس في استدعاء ذاتي توقف المتحكم.
- صرّح عن دالة C بـ `defffi`، ومرّر ملف كائن مترجمًا لـ AVR بـ `--link FILE`.
- الحلقة التي لا تفعل شيئًا تُحذف ابتداءً من `-O1`. في حلقة الانتظار اقرأ سجلًا مثلًا.
- عند انتهاء `main` يتوقف المتحكم الدقيق.
- يُغيَّر توزيع الذاكرة باستخدام `--linker-script FILE`. عند حذفه تكون الذاكرة 32 KiB من الفلاش ناقصًا آخر 512 بايت الخاصة بمحمّل الإقلاع، و2 KiB من RAM. تُكتب عناوين RAM مع إضافة `0x800000`، مثل `0x800100`.
- حسابات `f32` و`f64` تجريها دوال مساعدة. ويدخل المستخدَم منها فقط في الفلاش (ودوال `f64` أكبر).
- يستخدم الربط رابط سلسلة أدوات rustup كما في القسم 3.8.

مثال يجعل مصباح LED يومض بمقاطعة مؤقت. تُفعَّل المقاطعة في سجل الجهاز نفسه.

```lisp
(defconstant (PINB int) #x23)
(defconstant (DDRB int) #x24)
(defconstant (LED u8) 32)
(defconstant (TIMSK1 int) #x6F)
(defconstant (TCCR1B int) #x81)
(defvar (ticks u16) 0)

#[interrupt(13)]                                  ; فيضان المؤقت 1
(defun on-overflow () ()
  (unsafe (volatile-write-u8 PINB LED))           ; اعكسه
  (setf ticks (+ ticks 1))
  ())

(defun main () ()
  (unsafe (volatile-write-u8 DDRB LED)
          (volatile-write-u8 TIMSK1 1)            ; فعّل مقاطعة الفيضان
          (volatile-write-u8 TCCR1B 3))           ; عُدّ بمعدل 1/64 من الساعة
  (while true))
```

- البرنامج الذي فيه معالج يقبل المقاطعات فور تهيئة متغيراته العامة. اضبط الأجهزة في بداية `main`.
- لا يمكن استخدام `enable-interrupt` و`#[exception(NAME)]` (خطأ تصريف).
- أثناء تنفيذ معالج تنتظر المقاطعات الأخرى. ولا يستطيع المعالج استخدام بيئة التشغيل، كأي دالة أخرى.
- القراءة الواحدة أو الكتابة الواحدة لمتغير عام لا يقطعها معالج، حتى لقيمة من بايتين أو أكثر. وتُعطَّل المقاطعات لذلك عند قراءة متغير يكتبه معالج وعند كتابة متغير يقرؤه معالج أو يكتبه، ويُذكر هذا المتغير في تحذير وقت التصريف. ويمكن أن تأتي مقاطعة بين القراءة والكتابة في التحديث (`(setf n (+ n 1))`). لتحديث متغير يكتبه معالج أيضًا، اقرأ سجل الحالة (العنوان `#x5F`) واحتفظ به، واكتب فيه 0 لإيقاف المقاطعات، ثم أعد كتابة القيمة المحفوظة.
- المقاطعة التي لا معالج لها توقف المتحكم.

## 4. ملفات الـ dump

يمكنك حفظ تعريفات الجلسة الحالية في ملف واحد والبدء منه في المرة التالية.

```sh
$ typl
typl> (defun sq ((n i32)) i32 (* n n))
typl> (compile sq)
true
typl> (dump "session.typld")
true
typl> :quit
$ typl --image session.typld
typl> (sq 9)
81
```

ويعمل أيضًا لتشغيل ملف، كما في `typl --image session.typld prog.typl`.

- ما يُحفظ هو **التعريفات**. أما التعبيرات التي قُيّمت في الجلسة فلا تُحفظ.
- الدوال التي ترجمتها بـ `compile` تُحفظ في صورتها المترجمة.
- تُستعاد المتغيرات العامة **بتشغيل مهيِّئاتها من جديد**، لا بالقيم التي كانت لها عند كتابة الـ dump.
- لا يمكن تحميل ملف dump بواسطة `typl` من إصدار مختلف عن الذي كتبه (وهذا خطأ).

وإذا شغّلت ملفًا ونفّذت منه `(dump ...)` فإن تعريفات ذلك الملف تكون في وحدة باسم الملف. فالدالة
المعرَّفة في `dp.typl` اسمها `dp::sq`، واستدعاؤها من ملف آخر يتطلب `pub`
([الوحدات وتنظيم الملفات](modules.md)).

## 5. حول ملفات الوحدات المترجمة

لا توجد صيغة، مثل `.fasl` في Common Lisp، لكتابة النتيجة المترجمة لكل وحدة إلى ملف. فـ `compile-file`
تبني الملف التنفيذي مباشرة من المصادر. ولا تُترك ملفات وسيطة.
