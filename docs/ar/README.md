<!-- translated-from: docs/ja/README.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# توثيق typelisp (العربية)

typelisp هي لغة Lisp ذات أنواع ساكنة. للاطلاع على طريقة التثبيت والبناء راجع
[README.md](../../README.md) (بالإنجليزية) في جذر المستودع.

## الدروس التعليمية

إذا كنت جديدًا على typelisp فاقرأ هذه الدروس بالترتيب.

- [البدء السريع](tutorial/intro.md): الـ REPL والدوال والمتغيرات والشروط والحلقات والقوائم و`Vector`
- [أساسيات الأنواع](tutorial/types.md): الأنواع الساكنة و`Option` و`Result` والبنى والتعدادات والأنواع العامة
- [السمات (Traits)](tutorial/traits.md): `deftrait` / `impl` وقيود السمات و`:dyn`
- [الماكرو (Macros)](tutorial/macros.md): `defmacro` والاقتباس الجزئي (quasiquote) و`gensym` و`macrolet`
- [معالجة الأخطاء](tutorial/errors.md): `Result` و`panic` و`catch` / `throw` و`unwind-protect`
- [التزامن](tutorial/concurrency.md): المهام والقنوات و`select` و`Mutex` و`thread`

## الأدلة

- [الوحدات وتنظيم الملفات](guide/modules.md): `use` و`pub` وكيفية ربط الملفات بالوحدات
- [الترجمة](guide/compile.md): الـ JIT وبناء الملفات التنفيذية بالترجمة المسبقة AOT وملفات الـ dump
- [إدخال وإخراج الملفات والتدفقات والشبكات](guide/io.md): الملفات والمسارات وTCP / TLS / UDP وحل الأسماء
- [واجهة C الخارجية (FFI)](guide/ffi.md): استدعاء دوال C باستخدام `defffi` (بما في ذلك دوال الاستدعاء الراجع وبنى C عبر `def-c-struct`)
- [التكامل مع المحررات](guide/editors.md): `typl-lsp` وإعداد VS Code / Emacs
- [لمبرمجي Common Lisp](guide/from-common-lisp.md): الفروق بين typelisp وCL وكيفية إعادة كتابة شيفرة CL

## المرجع

- [مرجع الصياغة](reference/syntax.md): الصياغة المعجمية وكتابة الأنواع والتعريفات وصيغ التحكم والترجمة والتزامن
- [الدوال المضمنة](reference/functions/README.md): الدوال المضمنة والتوابع (methods) والمكتبة القياسية
- [الأنواع](reference/types.md): الأنواع والسمات التي ينفذها كل نوع
- [رسائل الخطأ](reference/errors.md): معنى الأخطاء الشائعة وكيفية إصلاحها

## التكامل مع المحررات

تعليمات الإعداد موجودة في [دليل التكامل مع المحررات](guide/editors.md). أما اختصارات المفاتيح
وإعدادات كل محرر فهي مدرجة في هذه المستندات:

- [Emacs (typelisp-mode)](../../editor/emacs/README_ar.md)
- [VS Code](../../editor/vscode/README_ar.md)
