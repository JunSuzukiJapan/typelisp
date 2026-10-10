<!-- translated-from: docs/ja/reference/functions/README.md @ 0f8a35599b7916036b773d6cb777b7de28762078 -->
# الدوال المضمنة

قائمة الدوال المضمنة والتوابع (methods) والمكتبة القياسية. وللصياغة (الصيغ الخاصة وطريقة التعريف) انظر
[مرجع الصياغة](../syntax.md)؛ ولقائمة الأنواع انظر [الأنواع](../types.md).

## صيغ الاستدعاء

توجد ثلاث صيغ للاستدعاء.

- الدوال الحرة: `(name args...)`
- توابع النسخ: `(name receiver args...)` (تُحَلّ من النوع الساكن للوسيط الأول)
- التوابع الساكنة (الدوال المرتبطة): `(Type::name args...)`

يجوز أن يكون لكل نوع تابعه الخاص بالاسم نفسه. و`(+ a b)` تستدعي `+` الخاصة بنوع `a`.

## قراءة الجداول

لجداول كل فصل الأعمدة "الاسم والصيغة والنوع والوصف". ويُكتب عمود النوع على الصورة
`(argument-type,...)→return-type`.

- الحرف الكبير المفرد مثل `T` أو `A` أو `B` متغير نوع.
- ملاحظة مثل `where Eq A` قيد سمة يجب أن يستوفيه متغير النوع.
- تعني `Iter<A>` "أي تنفيذ لـ `Iter` يكون `Item` فيه هو `A`".
- يمكن حذف الوسائط الموسومة بـ `&optional` / `&key`.

## الفصول

| الملف | المحتويات |
|---|---|
| [numbers.md](numbers.md) | الأعداد الصحيحة والأعداد ذات الفاصلة العائمة والأعداد الكسرية والمركبة والقيم المنطقية وعمليات البتات والأعداد العشوائية |
| [sequences.md](sequences.md) | الزوج `cons-cell` وبيانات التعبيرات الرمزية `Sexpr` والرموز ودوال التسلسلات والمكرِّرات الكسولة `lazy` والدوال عالية الرتبة |
| [collections.md](collections.md) | السلاسل النصية والمحارف و`Vector` و`HashTable` و`Array` و`BitVector` و`HashSet` و`SortedTable` و`Deque` |
| [option-result.md](option-result.md) | `Option` و`Result` وأنواع الأخطاء والسمة `Error` |
| [traits.md](traits.md) | `Iter` و`Eq`/`Ord` وسمات الحساب |
| [printing.md](printing.md) | `print`/`println`/`format` والطابعة المنسَّقة (pretty printer) و`print-object` ومتغيرات التحكم بالطباعة |
| [format.md](format.md) | موجِّهات التنسيق |
| [streams-files.md](streams-files.md) | التدفقات وعمليات الملفات والمسارات وreadtable |
| [concurrency.md](concurrency.md) | المهام والقنوات و`WaitGroup` و`Mutex` و`Thread` |
| [network.md](network.md) | TCP وTLS ومقابس نطاق Unix وUDP |
| [system.md](system.md) | الزمن وبيئة التشغيل وأدوات التنفيذ و`read`/`eval` والسلاسل التوثيقية والدوال المتعلقة بالماكرو |
