<!-- translated-from: editor/emacs/README_JP.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# typelisp-mode (Emacs)

وضع Emacs رئيسي (major mode) لتحرير مصدر typelisp (`.typl`).
نسخة VS Code في [../vscode/](../vscode/README_ar.md). وتتشارك النسختان جداول الكلمات المفتاحية نفسها
وقواعد الإزاحة نفسها، ويتحقق `cargo test --test editor_keyword_sync_test` من ذلك آليًا (انظر نهاية هذا
المستند).

## الميزات

- تلوين الصياغة
  - الصيغ الخاصة وبنى التحكم (`defun` `let` `if` `match` `loop` `lambda` `setf` `as`
    `apply` و`print`/`println`/`format` وعائلة `pprint` وهكذا)
  - الأسماء المعرَّفة (`NAME` في `(defun NAME ...)` على أنه اسم دالة، وفي `(defstruct NAME ...)`
    على أنه اسم نوع، وفي `(defvar (NAME ...))` على أنه اسم متغير؛ والأمر نفسه مع `pub` كما في
    `(pub defun NAME ...)`)
  - كلمات مفتاحية لمساحات الأسماء والإعلانات (`pub` `module` `use` `load` `impl` `where`) وعلامات قائمة
    الوسائط (`&rest` `&optional` `&key`)
  - الدوال المضمنة (`car` `map` `foldl` `unwrap` `parse-int` `message` `sexpr-car` وهكذا)
  - الأنواع الأولية (بما في ذلك `bignum` / `ratio`) والأنواع المضمنة وأنواع الأخطاء المضمنة
    (`ParseIntError` وهكذا) وأنواع المستخدم ذات الحرف الأول الكبير `Capitalized`، ونوع كائن السمة
    `:dyn Trait`
  - **استخدامات الأنواع المعرَّفة من المستخدم** (فأسماء `defstruct`/`defenum`/`deftrait` عادةً بأحرف
    صغيرة (`rect` `todo-item` `board`)، فلا تلتقطها قاعدة `Capitalized`).
    وعند الاتصال بـ `typl-lsp` تُلوَّن من رموز الخادم الدلالية (semantic tokens) (وهذا يعمل مع `eglot`
    أيضًا؛ انظر أدناه). وعند عدم الاتصال يرجع الوضع إلى جمع أسماء الأنواع المعرَّفة في المخزن المؤقت
    (buffer)
  - الثوابت الحرفية (`true` `false` وثوابت الأعداد (عشري / `0xff` / `1.5` / `1/3`) وثوابت المحارف مثل
    `#\Space` والسلاسل النصية والكلمات المفتاحية مثل `:name`)
  - موجِّهات تحكم `format` داخل السلاسل النصية (`~a` `~5,'0d` `~{...~}` وهكذا)
  - المتغيرات العامة بنمط CL ذات علامات النجمة (`*print-pretty*` وهكذا)
- التعليقات
  - التعليقات السطرية `;`
  - تعليقات كتلية **قابلة للتداخل** `#| ... |#`
- التنقل بين التعبيرات الرمزية والإزاحة بنمط Lisp
- فهرس للتعريفات عبر `imenu` (دوال / توابع / ماكرو / أنواع / سمات / `impl` / متغيرات / وحدات)
- أوامر تشغّل واجهة `typl` (أدناه)

## اختصارات المفاتيح

| المفتاح | الأمر | ما يفعله |
|---|---|---|
| `C-c C-c` | `typelisp-run-buffer` | يحفظ ويشغّل `typl FILE` (عبر `compile`، فيمكنك القفز إلى أسطر الأخطاء) |
| `C-c C-z` | `typelisp-repl` | يبدأ الـ REPL الخاص بـ `typl` في مخزن comint المؤقت |

اضبط موقع `typl` بـ `typelisp-program` (الافتراضي `"typl"`).
وللتشخيصات الصورة `error: FILE:LINE:COL: ...` التي يستطيع `compilation-mode` تحليلها، فيقفز
`next-error` / `C-x \`` مباشرة إلى الموضع.

## التثبيت

```elisp
(add-to-list 'load-path "/path/to/typelisp/editor/emacs")
(require 'typelisp-mode)
```

تُفتح ملفات `.typl` في `typelisp-mode` تلقائيًا (يُسجَّل الوضع في `auto-mode-alist`).

مع `use-package`:

```elisp
(use-package typelisp-mode
  :load-path "/path/to/typelisp/editor/emacs"
  :mode "\\.typl\\'")
```

## خادم اللغة (`typl-lsp`)

بعد بناء `typl-lsp` يمكن استخدامه من `eglot` (مضمَّن في Emacs 29 وما بعده) أو `lsp-mode`.

```sh
cargo build --release --bin typl-lsp
```

مع `eglot`:

```elisp
(with-eval-after-load 'eglot
  (add-to-list 'eglot-server-programs
               '(typelisp-mode . ("/path/to/typelisp/target/release/typl-lsp"))))
(add-hook 'typelisp-mode-hook #'eglot-ensure)
```

ومع `lsp-mode`:

```elisp
(with-eval-after-load 'lsp-mode
  (lsp-register-client
   (make-lsp-client
    :new-connection (lsp-stdio-connection "/path/to/typelisp/target/release/typl-lsp")
    :major-modes '(typelisp-mode)
    :server-id 'typl-lsp)))
(add-hook 'typelisp-mode-hook #'lsp)
```

المدعوم: التشخيصات (أخطاء الصياغة والأنواع وتحذيرات إعادة التعريف، وتُرسل عبر
`textDocument/publishDiagnostics`) والتمرير بالمؤشر (hover) والانتقال إلى التعريف والإكمال التلقائي (`:`
مسجَّلة محرف تفعيل) والرموز الدلالية. وتُحَلّ المراجع بين الملفات عبر `use` (يبحث الخادم صعودًا عن
`typelisp.toml` في جذر المشروع؛ وللتفاصيل انظر
[مرجع الصياغة 3.11](../../docs/ar/reference/syntax.md#311-الملفات-والوحدات-مشاريع-متعددة-الملفات)).
وتنعكس التعديلات غير المحفوظة في مخازن المحرر المفتوحة فورًا في تشخيصات الملفات التي تعتمد عليها وفي
الملفات التي تعتمد عليها.

### تلوين أسماء الأنواع (الرموز الدلالية)

عبر `textDocument/semanticTokens` يبلّغ الخادم عن **المواضع التي حلّها الفاحص فعلًا على أنها أسماء
أنواع**. ولأن هذا ليس مطابقة نصوص:

- تُلوَّن أيضًا الأنواع القادمة من ملفات أخرى عبر `use` (نطاق لا يبلغه الحل داخل المخزن المؤقت من حيث
  المبدأ)
- ولا تُلوَّن استدعاءات **دالة** بالاسم نفسه كنوع (فقد حلّها الفاحص على أنها دوال، فلا يُسجَّل رمز هناك
  أصلًا)

في جانب العميل:

- **`eglot` (Emacs 31 وما بعده)**: يرسم eglot الرموز بنفسه (`eglot-semantic-tokens-mode`).
  ويبتعد `typelisp-mode` عن الطريق
- **`eglot` (Emacs 30 وما قبله)**: لا يتعامل هذا الإصدار من eglot مع semanticTokens. لذلك **يرسل
  `typelisp-mode` الطلب بنفسه ويرسم النتيجة بطبقات overlays**
  (`typelisp-semantic-tokens-mode`، ويُفعَّل تلقائيًا عند اتصال eglot)
- **`lsp-mode`**: دعم أصلي (اضبط `lsp-semantic-tokens-enable` على `t`). وفي تلك الحالة يبتعد
  `typelisp-mode` عن الطريق

يتصل `scripts/emacs-semantic-smoke.el` عبر eglot فعلًا ويفحص الجانب الذي يقوم بالرسم في Emacs المستخدم.
ومع أي عميل يتراجع الرجوع الاحتياطي داخل المخزن المؤقت ما دام الخادم يجيب (حتى لا تطلي مجموعتا قواعد
المخزن المؤقت نفسه).

| الإعداد | الافتراضي | ما يفعله |
|---|---|---|
| `typelisp-semantic-tokens` | `t` | مع eglot في Emacs 30 وما قبله، هل يُلوَّن من الرموز الدلالية للخادم |
| `typelisp-semantic-tokens-idle-delay` | `0.6` | ثواني الخمول بعد تعديل قبل الطلب من جديد (أبقِها أكبر من `eglot-send-changes-idle-time`) |

## ملاحظات

- تحوّل typelisp الرموز إلى أحرف صغيرة عند القراءة، لكن التلوين حساس لحالة الأحرف ليمكن تمييز أسماء
  الأنواع التي تبدأ بحرف كبير.
- تتحدد الإزاحة بالدالة المخصصة `typelisp-indent-function` التي تبحث في `typelisp-indent-specs` (قائمة
  ارتباط alist). ويحتفظ الوضع بإدخالاته الخاصة حتى للصيغ التي تشاركه أسماءها مع Emacs Lisp (`defun` `let`
  `if` ...) لأن خصائص الرموز **عامة**، ووضع إعدادات typelisp هناك سيغيّر إزاحة مخازن Lisp الأخرى في الجلسة
  نفسها. وتختلف صيغ typelisp في الشكل حتى عندما تشارك اسمًا مع Emacs Lisp:
  فـ `(defun NAME (PARAMS) RETTYPE ...)` لها ثلاثة عناصر في الترويسة، و`if` ثابتة على ثلاثة عناصر مع
  `else` إلزامية. فلا يمكن مشاركة القيم أيضًا.
  وقد فُحص كل ملف `.typl` تحت `examples/`: فـ `indent-region` لا تغيّر بايتًا واحدًا، وتسوية كل الإزاحة ثم
  إعادة الإزاحة تعيد الأصل (وتستوفي نسخة VS Code المعيار نفسه على الملفات نفسها).

## اكتشاف الانجراف في تعريفات المحررات

تُصان جداول الكلمات المفتاحية مرتين، مرة هنا ومرة في نسخة VS Code. ولمنع تخلف تعريفات المحررين بينما
يمضي التنفيذ قدمًا يوجد اختبار في جانب Rust:

```sh
cargo test --test editor_keyword_sync_test
```

يحمّل المكتبة القياسية فعلًا، ويمر على السجل، ويبلّغ عن **الأسماء التي لا يعرفها أي من المحررين**. وليس
للصيغ الخاصة تمثيل وقت التشغيل، فتُقرأ من بين `// SPECIAL-FORM DISPATCH BEGIN` / `END` في
`crates/typelisp-front/src/check/checker.rs` (لا تحذف هذين التعليقين). وإذا فشل فأضف الأسماء المبلَّغ عنها
إلى تعريفات **كلا** المحررين.

ويقارن الاختبار نفسه أيضًا أسطورة الرموز الدلالية (`SEMANTIC_TOKEN_TYPES` في
`src/bin/lsp.rs` والجداول التي يحملها المحرران يجب أن تتفق في الأسماء والترتيب). وعدم التطابق لا يسبب خطأ
وقت التشغيل؛ بل يبدّل ألوان كل رمز فقط، لذلك يُثبَّت آليًا.
