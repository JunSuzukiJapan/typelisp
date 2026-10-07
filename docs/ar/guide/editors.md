<!-- translated-from: docs/ja/guide/editors.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# التكامل مع المحررات (typl-lsp)

`typl-lsp` هو خادم اللغة (language server) الخاص بـ typelisp. عند ربطه بمحرر يدعم LSP
(بروتوكول خادم اللغة Language Server Protocol) يوفر الميزات التالية للملف الذي تحرره:

- التشخيصات: أخطاء القراءة وأخطاء الأنواع وتحذيرات إعادة التعريف
- التمرير بالمؤشر (hover): نوع التعبير المحاط بأقواس والسلسلة التوثيقية (docstring) للتعريف الذي
  يستدعيه (لا تظهر لأسماء المتغيرات المجردة)
- الانتقال إلى التعريف
- الإكمال التلقائي (تظهر الاقتراحات عند كتابة `:`)
- تلوين أسماء الأنواع (semantic tokens) بما في ذلك الأنواع المستوردة بـ `use` من ملفات أخرى

تُحَلّ المراجع بين الملفات عبر `use`. وتنعكس التعديلات غير المحفوظة في ملف آخر مفتوح فورًا في
تشخيصات الملفات التي تستورده بـ `use`.

## 1. البناء

```sh
cargo build --release --bin typl-lsp
```

ينتج هذا الملف `target/release/typl-lsp`. وإذا ثبّتّه بـ `cargo install` كما هو موضح في
[README.md](../../../README.md) فسيكون في `~/.cargo/bin/typl-lsp` بجانب `typl`.

## 2. VS Code

الامتداد موجود في `editor/vscode` داخل المستودع. وهو غير منشور على Marketplace، لذا ابنِه وثبّته بنفسك.

```sh
cd editor/vscode
npm install
npm run compile
npx @vscode/vsce package      # ينتج ملف .vsix
```

اختر "Install from VSIX..." من قائمة "..." في عرض الامتدادات (Extensions) ثم حدد ملف `.vsix` الذي
بنيته.

يبحث الامتداد عن `typl-lsp` في `target/release/typl-lsp` داخل مساحة العمل، ثم في
`target/debug/typl-lsp`، ثم في `PATH`. وإذا وضعته في مكان آخر فاكتب مساره في الإعداد
`typelisp.languageServer.path`.

| الإعداد | القيمة الافتراضية | المعنى |
|---|---|---|
| `typelisp.program` | `typl` | مسار `typl` |
| `typelisp.languageServer.enable` | `true` | هل يُتصل بـ `typl-lsp` |
| `typelisp.languageServer.path` | (فارغ) | مسار `typl-lsp` |

يحفظ `Ctrl+Alt+R` الملف الذي تحرره ويشغّله بـ `typl`، ويبدأ `Ctrl+Alt+Z` الـ REPL. لمزيد من التفاصيل
راجع [README الخاص بامتداد VS Code](../../../editor/vscode/README_ar.md).

## 3. Emacs

يوجد `typelisp-mode` في `editor/emacs` داخل المستودع.

```elisp
(add-to-list 'load-path "/path/to/typelisp/editor/emacs")
(require 'typelisp-mode)
```

إعدادات الاتصال بـ `typl-lsp` باستخدام `eglot` (المضمّن مع Emacs 29 وما بعده):

```elisp
(with-eval-after-load 'eglot
  (add-to-list 'eglot-server-programs
               '(typelisp-mode . ("/path/to/typelisp/target/release/typl-lsp"))))
(add-hook 'typelisp-mode-hook #'eglot-ensure)
```

وباستخدام `lsp-mode`:

```elisp
(with-eval-after-load 'lsp-mode
  (lsp-register-client
   (make-lsp-client
    :new-connection (lsp-stdio-connection "/path/to/typelisp/target/release/typl-lsp")
    :major-modes '(typelisp-mode)
    :server-id 'typl-lsp)))
(add-hook 'typelisp-mode-hook #'lsp)
```

يلوّن eglot في Emacs 31 وما بعده أسماء الأنواع (semantic tokens) بنفسه. أما eglot في Emacs 30 وما
قبله فلا يدعمها، لذا يلوّن `typelisp-mode` أسماء الأنواع بدلًا منه. ومع `lsp-mode` اضبط
`lsp-semantic-tokens-enable` على `t`.

يشغّل `C-c C-c` الملف الذي تحرره، ويبدأ `C-c C-z` الـ REPL. لمزيد من التفاصيل راجع
[README الخاص بـ typelisp-mode](../../../editor/emacs/README_ar.md).

## 4. محررات أخرى

يتحدث `typl-lsp` بروتوكول LSP عبر الإدخال والإخراج القياسيين ولا يأخذ أي وسائط من سطر الأوامر. اضبط
عميل LSP في محررك ليشغّل `typl-lsp` لملفات `.typl`.

## 5. كيف تُعرَف المشاريع

يبحث `typl-lsp` عن `typelisp.toml` بدءًا من مجلد الملف المفتوح ثم صعودًا في المجلدات، ويحلّ `use`
معتبرًا ذلك المكان جذرًا للمصدر. وهذه هي القواعد نفسها المتبعة عندما يشغّل `typl` ملفًا
([الوحدات وتنظيم الملفات](modules.md#2-إعداد-مشروع)). وفي المشروع المكوّن من عدة ملفات ضع
`typelisp.toml` في جذره.

## 6. خادم اللغة لا يشغّل برنامجك

ينتج `typl-lsp` التشخيصات بالقراءة وفحص الأنواع فقط، ولا يشغّل أبدًا البرنامج الذي تحرره. فالتشخيصات
تُجرى مع كل ضغطة مفتاح، لذا لا يمكنه تحمّل تشغيل شيفرة ذات آثار جانبية أو شيفرة لا تنتهي. والاستثناء
الوحيد هو تسجيل `defmacro`، وهو لازم لفحص استدعاءات الماكرو التي تليها.

ولهذا فإن الأخطاء التي لا تحدث إلا عندما يشغّل `typl` البرنامج (`panic` وغياب ملف وما إلى ذلك) لا
تظهر في تشخيصات خادم اللغة.
