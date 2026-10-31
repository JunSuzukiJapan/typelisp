<!-- translated-from: docs/ja/reference/functions/README.md @ 9a4e7d23b09eb3f3244bfb876df1ec57576d6874 -->
# الدوال المضمنة

قائمة الدوال المضمنة والتوابع (methods) والمكتبة القياسية. وللصياغة (الصيغ الخاصة وطريقة التعريف) انظر
[مرجع الصياغة](../syntax.md)؛ ولقائمة الأنواع انظر [الأنواع](../types.md).

## صيغ الاستدعاء

توجد ثلاث صيغ للاستدعاء.

- الدوال الحرة: `(name args...)`
- توابع النسخ: `(name receiver args...)` (تُحَلّ من النوع الساكن للوسيط الأول)
- التوابع الساكنة (الدوال المرتبطة): `(Type::name args...)`

يجوز أن يكون لكل نوع تابعه الخاص بالاسم نفسه. و`(+ a b)` تستدعي `+` الخاصة بنوع `a`.

## أسماء تحتاج إلى `use`

من أسماء المكتبة القياسية، هذه تُستعمل في كل وحدة دون كتابة شيء
([مرجع الصياغة 3.10](../syntax.md#310-module--use--مساحات-الأسماء)):

- الأنواع الأولية، و`Option`/`Result`/`Vector`، والتكرار، وتعابير S والقوائم، ودوال المتتاليات،
  ووحدات الماكرو الضابطة، والمقارنة، والسمات الحسابية
- كل اسم هو أيضًا اسم في حزمة `COMMON-LISP` الخاصة بـ Common Lisp (`length` و`format` و`open` وغيرها)
- `Error` و`assert` و`print-object`، و`HashTable` و`Array` و`BitVector`، وسمات التدفق `Stream`
  و`InputStream` و`OutputStream`، و`Hash` الذي تنفّذه مفاتيح `HashTable`
- الأنواع التي تذكرها دوال هذه ومتغيراتها

وأي اسم آخر يُدخَل بـ `use`، مثل `(use std::net::tcp-connect)`، أو يُكتب بمساره،
`std::net::tcp-connect`. يذكر الجدول هذه الأسماء والوحدة التي فيها كل منها.

| الوحدة | الأسماء |
|---|---|
| `std::array` | `array-iter` |
| `std::char` | `digit->char`، `digit-weight`، `name->char` |
| `std::collections` | `Deque`، `HashSet`، `Scope`، `SortedTable`، `hashtable-iter` |
| `std::env` | `command-line-args`، `getenv`، `home-directory` |
| `std::error` | `WrappedError`، `as-dyn-error`، `assert-eq`، `describe-error`، `wrap-error` |
| `std::fmt` | `pprint-list-exhausted` |
| `std::fs` | `direction-append`، `direction-input`، `direction-output`، `directory-p`، `open-binary`، `open-binary-input`، `open-binary-output`، `open-file`، `open-input`، `open-output`، `read-file-lines`، `read-file-string`، `write-file-string` |
| `std::hash` | `*sxhash-mask*`، `sxhash-string` |
| `std::io` | `ByteInput`، `ByteOutput`، `PeekInput`، `binary-file-stream`، `copy-stream`، `make-peek-stream`، `peek-stream`، `read-lines`، `write-lines` |
| `std::net` | `NetError`، `byte-stream-of`، `char-stream-of`، `datagram`، `socket-byte-stream`، `socket-listener`، `socket-stream`، `tcp-connect`، `tcp-listen`، `tls-connect`، `tls-listen`، `udp-bind`، `udp-socket`، `unix-connect`، `unix-listen`، `with-connection` |
| `std::num` | `ParseFloatError`، `ParseIntError`، `atan2`، `parse-float`، `parse-int` |
| `std::path` | `Pathish`، `pathname-absolute-p` |
| `std::process` | `exit` |
| `std::random` | `seed-random-state` |
| `std::reader` | `ReadOutcome`، `read-from-string-preserving-whitespace`، `read-sexpr`، `read-sexpr-preserving-whitespace` |
| `std::repl` | `heap-info` |
| `std::string` | `left-trim`، `right-trim`، `string->utf8`، `trim`، `utf8->string` |
| `std::symbol` | `symbol->string` |
| `std::sync` | `Chan`، `Context`، `Mutex`، `WaitGroup`، `after`، `with-lock` |
| `std::thread` | `Task`، `Thread`، `yield` |
| `std::time` | `internal-time-seconds`، `timezone-daylight-p`، `timezone-offset-seconds` |

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
| [traits.md](traits.md) | `Iter` و`Eq`/`Ord` وسمات الحساب و`Push` و`Pop` |
| [printing.md](printing.md) | `print`/`println`/`format` والطابعة المنسَّقة (pretty printer) و`print-object` ومتغيرات التحكم بالطباعة |
| [format.md](format.md) | موجِّهات التنسيق |
| [streams-files.md](streams-files.md) | التدفقات وعمليات الملفات والمسارات وreadtable |
| [concurrency.md](concurrency.md) | المهام والقنوات و`WaitGroup` و`Mutex` و`Thread` و`Context` |
| [network.md](network.md) | TCP وTLS ومقابس نطاق Unix وUDP |
| [system.md](system.md) | الزمن وبيئة التشغيل وأدوات التنفيذ و`read`/`eval` والسلاسل التوثيقية والدوال المتعلقة بالماكرو |
