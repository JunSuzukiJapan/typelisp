<!-- translated-from: docs/ja/reference/types.md @ 1a01065673fd9d568c138bb444c88c5345552937 -->
# الأنواع

الأنواع الموجودة في typelisp، والسمات القياسية التي ينفّذها كل نوع. وطريقة كتابة الأنواع في
[الفصل 2 من مرجع الصياغة](syntax.md#2-كتابة-الأنواع)؛ ودوال وتوابع كل نوع في
[الدوال المضمنة](functions/README.md).

## 1. الأنواع الأولية

| النوع | المحتوى | التفاصيل |
|---|---|---|
| `int` | عدد صحيح اعتباطي الدقة. يُحفظ كقيمة فورية ما دام يتسع في 63 بتًا، ويصير bignum تلقائيًا بعد ذلك. وهو النوع الافتراضي للثوابت الصحيحة غير الموصَّفة | [الفصل 3 من الأعداد](functions/numbers.md#3-الأعداد-الصحيحة-اعتباطية-الدقة-int) |
| `i8` `i16` `i32` | أعداد صحيحة ذات إشارة بعرض ثابت | [الفصل 1 من الأعداد](functions/numbers.md#1-الأعداد-الصحيحة-ذات-العرض-الثابت) |
| `u8` `u16` `u32` | أعداد صحيحة بلا إشارة بعرض ثابت | مثل ما سبق |
| `f32` `f64` | أعداد IEEE-754 ذات الفاصلة العائمة. والثوابت العشرية افتراضيًا `f64` | [الفصل 4 من الأعداد](functions/numbers.md#4-الأعداد-ذات-الفاصلة-العائمة-f64--f32) |
| `ratio` | عدد كسري (نسبي) في أبسط صورة | [الفصل 5 من الأعداد](functions/numbers.md#5-الأعداد-الكسرية-ratio) |
| `bool` | `true` / `false` | [الفصل 7 من الأعداد](functions/numbers.md#7-القيم-المنطقية) |
| `char` | قيمة عددية Unicode (scalar value) | [المحارف](functions/collections.md#2-المحارف-char) |
| `string` | سلسلة نصية غير قابلة للتعديل | [السلاسل النصية](functions/collections.md#1-السلاسل-النصية-string) |
| `symbol` | رمز. والكلمات المفتاحية (`:name`) لها هذا النوع أيضًا | [الرموز](functions/sequences.md#3-الرموز) |
| `()` | النوع Unit. وقيمته `()` أيضًا | |
| `!` | النوع Never. نوع التعابير التي لا تعود، مثل `panic`. ويمكن وضعه حيث يُتوقَّع أي نوع | |
| `ptr` `c-long` `c-ulong` | كلمات تُستخدم فقط لتمرير القيم من C وإليها. ولا يمكن أن تكون قيمًا إلا داخل `unsafe`، والمواضع التي يمكن أن تظهر فيها محدودة | [الفصل 2 من الأعداد](functions/numbers.md#2-الكلمات-الخام-عند-حدود-c-ptr--c-long--c-ulong) |
| `random-state` | حالة مولّد أعداد عشوائية | [الفصل 12 من الأعداد](functions/numbers.md#12-الأعداد-العشوائية) |

لا يوجد نوع عدد صحيح بعرض 64 بت. وللأعداد الصحيحة التي لا يهم عرضها استخدم `int`.

## 2. الأنواع العامة المضمنة

| النوع | المحتوى | التفاصيل |
|---|---|---|
| `Option<T>` | قيمة موجودة أو غير موجودة. `some` / `none` | [Option وResult](functions/option-result.md) |
| `Result<T,E>` | نجاح أو فشل. `ok` / `err` | مثل ما سبق |
| `Vector<T>` | مصفوفة قابلة للنمو | [Vector](functions/collections.md#3-vectort) |
| `HashTable<K,V>` | جدول تجزئة. ويجب أن ينفّذ نوع المفتاح `Hash` | [HashTable](functions/collections.md#4-hashtablekv) |
| `#{T0 T1 ...}` | صف (من 1 إلى 12 عنصرًا). تُقرأ العناصر بـ `t::0` | [الصياغة، الفصل 2](syntax.md#2-كتابة-الأنواع) |
| `Task<T>` | مقبض إلى مهمة | [المهام](functions/concurrency.md#1-taskt--مقابض-المهام) |
| `Thread<T>` | مقبض إلى مهمة تعمل على خيط OS مخصص | [Thread](functions/concurrency.md#7-threadt--خيوط-نظام-التشغيل-المخصصة) |
| `Chan<T>` | قناة | [القنوات](functions/concurrency.md#2-chant--القنوات) |

وتُكتب أنواع الدوال `(fn (أنواع-الوسائط...) النوع-الراجع)`، وكائنات السمات `:dyn Trait`
([الفصل 2 من مرجع الصياغة](syntax.md#2-كتابة-الأنواع)).

## 3. بيانات التعبيرات الرمزية (S-expression)

| النوع | المحتوى | التفاصيل |
|---|---|---|
| `Sexpr` | تعبير رمزي غير فارغ. 19 متغايرًا: `int` و`i8` إلى `u32` و`f32` و`f64` و`char` و`bool` و`sym` و`str` و`cons` و`ratio` و`path` و`vector` و`array` و`tuple` | [بيانات التعبيرات الرمزية](functions/sequences.md#2-بيانات-التعبيرات-الرمزية-sexpr) |
| `Option<Sexpr>` | بيانات تعبيرات رمزية عمومًا. والقائمة الفارغة `()` هي `none` | مثل ما سبق |

## 4. الأنواع في المكتبة القياسية

أنواع تعرّفها المكتبة القياسية (prelude) بـ `defstruct` / `defenum`. وتُعامَل كالأنواع التي تكتبها بنفسك،
وكل ما يمكنك فعله بـ `defstruct` يمكن فعله بها.

| النوع | المحتوى | التفاصيل |
|---|---|---|
| `cons-cell<A,B>` | زوج. `cons`/`car`/`cdr` | [الأزواج](functions/sequences.md#1-الأزواج-cons-cellab) |
| `complex` | عدد مركب (مكوناته `f64`) | [الفصل 6 من الأعداد](functions/numbers.md#6-الأعداد-المركبة-complex) |
| `Array<T>` | مصفوفة متعددة الأبعاد | [Array](functions/collections.md#5-arrayt-المصفوفات-متعددة-الأبعاد) |
| `BitVector` | تسلسل ثابت الطول من البتات | [BitVector](functions/collections.md#6-bitvector-متجهات-البتات) |
| `HashSet<T>` | مجموعة عناصر بلا تكرار | [HashSet](functions/collections.md#7-hashsett) |
| `SortedTable<K,V>` | جدول مرتّب بحسب المفتاح | [SortedTable](functions/collections.md#8-sortedtablekv) |
| `Deque<T>` | تسلسل يُضاف إليه ويُؤخذ منه من الطرفين | [Deque](functions/collections.md#9-dequet) |
| `vector-iter<T>` `hashtable-iter<K,V>` `array-iter<T>` | المكرِّرات التي تعيدها `iter` لكل مجموعة | [Iter](functions/traits.md#1-السمة-iter-والتكرار) |
| `lazy::map-iter<I,A,U>` وغيرها | المكرِّرات التي تُرجعها دوال الوحدة `lazy` | [المكرِّرات الكسولة](functions/sequences.md#المكرِّرات-الكسولة-الوحدة-lazy) |
| `WaitGroup` | انتظار اكتمال N عملية | [WaitGroup](functions/concurrency.md#4-waitgroup--انتظار-اكتمال-n-عملية) |
| `Mutex<T>` | استبعاد متبادل للبيانات المشتركة | [Mutex](functions/concurrency.md#6-mutext--الاستبعاد-المتبادل-للبيانات-المشتركة) |
| `Context` | الإلغاء التعاوني | [Context](functions/concurrency.md#8-context--الإلغاء-التعاوني) |
| `pathname` | اسم ملف مقسَّم إلى أجزاء | [المسارات](functions/streams-files.md#9-المسارات-pathname) |
| `file-stream` `binary-file-stream` `string-input-stream` `string-output-stream` `standard-stream` | تدفقات | [التدفقات](functions/streams-files.md#3-أنواع-التدفقات-الملموسة) |
| `broadcast-stream` `two-way-stream` `echo-stream` `concatenated-stream` `peek-stream` | تدفقات مركّبة | [التدفقات المركّبة](functions/streams-files.md#4-التدفقات-المركبة) |
| `socket-stream` `socket-byte-stream` `socket-listener` `udp-socket` `datagram` | الشبكات | [الشبكات](functions/network.md#1-الأنواع) |
| `ReadOutcome` | نتيجة `read-sexpr`. `datum` / `eof` | [التدفقات](functions/streams-files.md#6-الدوال-العامة-وعمليات-الملفات) |
| `universal-time` `internal-time` `decoded-time` | الزمن | [الزمن](functions/system.md#1-الزمن) |
| `heap-info` | الحالة الراهنة للـ heap | [أدوات التنفيذ](functions/system.md#51-حقول-heap-info) |

## 5. أنواع الأخطاء

`Error` ليست نوعًا بل سمة، والأنواع التالية تنفّذها. ولمعالجة أخطاء من أي نوع اكتب `:dyn Error`.

| النوع | يُنتَج بواسطة |
|---|---|
| `ParseIntError` | `parse-int` |
| `ParseFloatError` | `parse-float` |
| `ReadError` | `read` / `read-from-string` / `read-sexpr` |
| `EvalError` | `eval` / `macroexpand` |
| `FileError` | عمليات الملفات والتدفقات |
| `NetError` | عمليات الشبكة |
| `SimpleError` | `simple-error` |
| `WrappedError` | `wrap-error` |

التفاصيل في [أنواع الأخطاء والسمة Error](functions/option-result.md#3-أنواع-الأخطاء-والسمة-error).

## 6. تنفيذات السمات القياسية

أي الأنواع تنفّذ أي السمات. وتوابع كل سمة في [السمات القياسية](functions/traits.md) وفي الفصول المذكورة
في العمود الأخير.

### 6.1 المقارنة والتجزئة والطباعة

| السمة | الأنواع المنفِّذة | التفاصيل |
|---|---|---|
| `Eq` | `int` `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `ratio` `complex` `bool` `char` `string` `symbol` `Sexpr` `cons-cell<A,B>` `#{..}` | [Eq / Ord](functions/traits.md#2-eq--ord-المقارنة) |
| `Ord` | `int` `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `ratio` `char` `string` `cons-cell<A,B>` `#{..}` | مثل ما سبق |
| `Hash` | `int` `i8` `i16` `i32` `u8` `u16` `u32` `bool` `char` `string` `symbol` `#{..}` | [HashTable](functions/collections.md#4-hashtablekv) |
| `print-object` | `int` `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `ratio` `complex` `bool` `char` `string` `symbol` `#{..}` `Array<T>` `HashSet<T>` `SortedTable<K,V>` `Deque<T>` `Context` `pathname` `universal-time` `internal-time` وكل أنواع الأخطاء المضمنة | [print-object](functions/printing.md#5-print-object-التمثيل-المطبوع-لكل-نوع) |

سمات `cons-cell<A,B>` والصفوف `#{..}`، و`print-object` الخاص بالمجموعات، متاحة حين تنفّذ أنواع
العناصر تلك السمة.

### 6.2 الحساب

| السمة | الأنواع المنفِّذة |
|---|---|
| `Add` `Sub` `Mul` `Div` `Rem` `Number` | `int` `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `ratio` |
| `Bits` | `int` `i8` `i16` `i32` `u8` `u16` `u32` |

التفاصيل في [سمات الحساب](functions/traits.md#3-سمات-الحساب-add--sub--mul--div--rem--bits--number).

### 6.3 التكرار

| السمة | الأنواع المنفِّذة |
|---|---|
| `Iter` | `vector-iter<T>` `hashtable-iter<K,V>` `array-iter<T>` `Chan<T>` أنواع الوحدة `lazy` (`lazy::map-iter<I,A,U>` وغيرها) |

### 6.4 التدفقات

| النوع | السمات المنفَّذة |
|---|---|
| `file-stream` | `CharInput` `PeekInput` `CharOutput` |
| `binary-file-stream` | `ByteInput` `ByteOutput` |
| `string-input-stream` | `CharInput` `PeekInput` |
| `string-output-stream` | `CharOutput` |
| `standard-stream` | `CharInput` `PeekInput` `CharOutput` |
| `socket-stream` | `CharInput` `PeekInput` `CharOutput` |
| `socket-byte-stream` | `ByteInput` `ByteOutput` |
| `broadcast-stream` | `CharOutput` |
| `two-way-stream` | `CharInput` `CharOutput` |
| `echo-stream` | `CharInput` |
| `concatenated-stream` | `CharInput` |
| `peek-stream` | `CharInput` `PeekInput` |

كل تدفق ينفّذ `Stream`؛ وتدفقات الدخل تنفّذ أيضًا `InputStream`، وتدفقات الخرج `OutputStream`. أما
`socket-listener` و`udp-socket` فتنفّذان `Stream` فقط (`close` / `open-stream-p`). والتفاصيل في
[التدفقات](functions/streams-files.md#1-تسلسل-السمات-الهرمي).

### 6.5 أخرى

| السمة | الأنواع المنفِّذة | التفاصيل |
|---|---|---|
| `Error` | كل أنواع الأخطاء في الفصل 5 | [أنواع الأخطاء](functions/option-result.md#3-أنواع-الأخطاء-والسمة-error) |
| `Pathish` | `string` `pathname` | [المسارات](functions/streams-files.md#91-سمة-محدد-المسار-pathish) |
