<!-- translated-from: docs/ja/reference/errors.md @ b550ae4 -->
# رسائل الخطأ

معنى أهم رسائل الخطأ الصادرة من `typl` وكيفية إصلاحها.

## 1. قراءة خطأ

تُكتب الأخطاء إلى مجرى الخطأ القياسي بهذه الصورة:

```text
error: file:line:column: kind: message
```

يخبرك `kind` متى اكتُشف الخطأ.

| النوع | متى | المعنى |
|---|---|---|
| `type error` | قبل التشغيل (وقت الفحص) | خطأ في الأنواع أو الأسماء. ولا تُشغَّل تلك الصيغة |
| (بلا نوع) | عند القراءة أو الفحص | خطأ في الصياغة مثل أقواس غير متوازنة، أو اسم لا يمكن العثور عليه |
| `panic` | أثناء التشغيل | إخفاق غير قابل للتعافي. يتوقف البرنامج بعد تنفيذ تنظيف `unwind-protect` |

الأسطر التي تبدأ بـ `warning:` تحذيرات، وتستمر المعالجة.

يشير `file:line:column` إلى التعبير المخطئ. وبالنسبة إلى خطأ وقت تشغيل يحدث داخل دالة في المكتبة
القياسية فإنه يشير إلى الموضع الذي استدعى فيه البرنامج تلك الدالة. وبعض الأخطاء بلا موضع (مثل
`error: panic: ...`).

مثال:

```text
error: main.typl:1:24: type error: type mismatch: expected `i32`, found `string`
```

يعني هذا أن التعبير في السطر 1 والعمود 24 من `main.typl` كان `string` حيث كان يُتوقَّع `i32`.

## 2. أخطاء وقت الفحص

أخطاء تُكتشف قبل التشغيل. ولا تُشغَّل الصيغة حتى تُصلَح.

### 2.1 الأنواع

| الرسالة | المعنى والإصلاح |
|---|---|
| ``type mismatch: expected `T`, found `U` `` | تعبير من النوع `U` في موضع يلزم فيه النوع `T`. لا توجد تحويلات ضمنية؛ وللأعداد حوّل بـ `(as T x)`. و`int` و`i32` نوعان مختلفان أيضًا |
| ``integer literal 300 is out of range for u8 (0..=255)`` | لا يتسع الثابت في النوع. وإذا أردت قصّه فاكتب `(as u8 300)` |
| ``unknown type `foo`: no type of that name is visible here. ...`` | لا يوجد نوع بهذا الاسم. عرّف النوع قبل أول صيغة تستخدمه (لا يوجد إعلان مسبق للأنواع). وإذا كنت تقصد متغير نوع فاكتبه في موضع إعلان مثل `<foo>` بعد اسم الدالة ([مرجع الصياغة 3.6](syntax.md#36-defstruct--البنى-أنواع-يعرفها-المستخدم)) |
| ``cannot infer type argument `t` for `vector::new` `` | لا يمكن تحديد وسيط نوع. اكتب النوع بـ `the` كما في `(the Vector<int> (Vector::new))` |
| ``non-exhaustive match on `color`: 1/2 variants covered`` | لا يعالج `match` كل المتغايرات. أضف فروعًا للمتغايرات الناقصة أو فرع `_` |
| ``type `pt` does not implement trait `eq` required by `where` clause on type parameter `a` `` | تشترط الدالة سمة لا ينفّذها النوع الذي مرّرته. اكتب `(impl Eq pt ...)` ([السمات القياسية](functions/traits.md)) |
| ``` `sq` does not implement `shape`, so it cannot be used as `:dyn shape` ``` | مُرِّرت قيمة من نوع لا ينفّذ السمة حيث يُتوقَّع `:dyn`. اكتب `impl` |
| ``` `error` is a trait, not a type — write `:dyn error` for a trait object ``` | كُتب اسم سمة حيث يُوضع نوع. اكتب `:dyn Error` |
| ``if: (if cond then else)`` | شكل `if` خاطئ. يتطلب `if` فرع else. وعندما لا تحتاج إليه استخدم `when` |

### 2.2 الأسماء

| الرسالة | المعنى والإصلاح |
|---|---|
| `no such function: bar` | لا توجد دالة أو تابع بهذا الاسم. تحقق من الإملاء |
| ``no method `upcase` for type `int` (the type of the first argument, which selects the method); `upcase` is a method of `char`, `string` `` | تُختار التوابع بنوع الوسيط الأول. يوجد تابع بهذا الاسم لكن ليس لنوع الوسيط الأول (`int` هنا). ويسرد آخر الرسالة الأنواع التي لها التابع (وللتابع المكتوب لتجسيد واحد: التجسيد نفسه، مثل `box<int>`) |
| ``no method `twice` for type `box<string>` (the first parameter of the function this value is expected to be, which selects the method); `twice` is a method of `box<int>` `` | تُختار الطريقة الممرَّرة كقيمة بحسب نوع المعامل الأول لنوع الدالة الذي تُمرَّر بوصفه. ليس لذلك النوع (هنا `box<string>`) طريقة بهذا الاسم. تسرد نهاية الرسالة الأنواع التي تملكها |
| `unbound variable: y` | لا يوجد متغير بهذا الاسم. تحقق من الإملاء ومن نطاق الربط (هل يُستخدم خارج `let` الخاصة به؟) |
| ``cannot define variable `f`: a function of that name already exists here — a name has one meaning, whether a function, a variable or a macro`` | أُعطي اثنان من دالة ومتغير عام وماكرو الاسمَ نفسه في وحدة واحدة. غيّر اسم أحدهما ([مرجع الصياغة 3.10](syntax.md#310-module--use--مساحات-الأسماء)) |
| ``` `point` has no static method `new`; a struct's methods are written in its `impl`, ...``` | لا يُولَّد `new` للبنية. اصنع القيمة بـ `make-instance`، أو اكتب `new` في `impl` ([مرجع الصياغة 3.5](syntax.md#35-impl--توابع-النوع)) |
| ``defmethod is gone — write methods in the type's `impl`: ...`` | لا يوجد `defmethod`. تُكتب التوابع في `impl` النوع ([مرجع الصياغة 3.5](syntax.md#35-impl--توابع-النوع)) |
| ``defstruct: (:constructor ...) is gone — ...`` | لا يوجد خيار `:constructor`. يُكتب المُنشئ في `impl` تابعًا ساكنًا |
| ``method box::m: `box` has a `m` for every instantiation already, so an `impl` of `box<int>` cannot define its own ...`` | كُتب تابع بالاسم نفسه في `impl` معاملات نوعه متغيرات (`Box<T>`) وفي `impl` تجسيد (`Box<int>`) معًا، أيًّا كان الترتيب. أبقِ واحدًا منهما فقط ([مرجع الصياغة 3.5](syntax.md#35-impl--توابع-النوع)) |
| ``impl show box<int>: `box` implements `show` for every instantiation already, ...`` | نُفّذت السمة نفسها لـ`Box<T>` ولـ`Box<int>` معًا. أبقِ واحدًا منهما فقط |
| ``use: unresolved `nosuch` `` | تعذر العثور على الوحدة المسماة في `use`. وللاطلاع على كيفية ربط أسماء الملفات بمسارات الوحدات انظر [مرجع الصياغة 3.11](syntax.md#311-الملفات-والوحدات-مشاريع-متعددة-الملفات) |
| `unresolved path: c::hidden` | الوحدة موجودة لكن الاسم غير موجود، أو غير مرئي لعدم وجود `pub` |
| `circular module dependency: a -> b -> a` | تستورد الوحدات بعضها بعضًا بـ `use`. انقل الجزء المشترك إلى وحدة منفصلة |
| ``return-from: no enclosing block named `nope` `` | لا يحيط بـ `return-from` أي `block` بالاسم المعطى. ولا يمكن استخدام block الخاص بدالة إلا داخل تلك الدالة |

### 2.3 الاستدعاءات

| الرسالة | المعنى والإصلاح |
|---|---|
| `f: expected 1 argument(s), got 2` | عدد الوسائط غير مطابق |
| `f: unknown keyword argument :b` | مُرِّر وسيط بكلمة مفتاحية ليس للدالة |
| `circle: expected 1 field(s), got 2` | عدد القيم الممرَّرة إلى متغير تعداد لا يطابق عدد حقوله |
| ``make-instance point: slot `y` is not given and has no default`` | لم يُعطَ `make-instance` حقلًا لا قيمة افتراضية له |
| ``make-instance point: no slot `z` (its slots are `x`, `y`)`` | أُعطي حقل ليس في البنية |
| ``make-instance geo::pt: slot `secret` is not `pub`, so it can be given only inside module `geo` `` | أُعطي حقل ليس `pub` خارج الوحدة التي تعرّف النوع. اطلب من تلك الوحدة مُنشئًا `pub` |
| ``` `box::make` is written for `box<int>`, `box<string>`; the type the call is expected to return picks one, ...``` | لدى `impl` عدة تجسيدات تابع ساكن بهذا الاسم، ولا شيء يحدد أيها يُستدعى. اكتب النوع المتوقع، كما في `(the Box<int> (Box::make))` |
| ``compile: `box::twice` is written for more than one instantiation of `box`; name one: ...`` | مُرّر تابع مكتوب لكل تجسيد إلى `compile` أو `trace` أو `documentation` ونحوها باسم النوع وحده. اكتب التجسيد، كما في `box<int>::twice` |
| ``setf: cannot assign to constant `k` `` | أُسنِد إلى اسم معرَّف بـ `defconstant`. وإذا كان يحتاج إلى التغيير فاستخدم `defvar` |
| ``defsignature: `later` has no definition in this file — a declaration promises one`` | دالة معلَنة بـ `defsignature` غير معرَّفة |
| ``format: ~/nosuch/ calls what (nosuch arg colon at) with arg a `string` would: no such function: nosuch`` | لا يمكن حلّ الاستدعاء `(nosuch arg colon at)` المكتوب في ذلك الموضع. عرِّف تابعًا لنوع الوسيط أو دالة ([الفصل 5 من موجِّهات التنسيق](functions/format.md#5-name)) |
| ``format: ~/brief/ can be handed an element of a list argument, whose type is only `Sexpr` — ...`` | كُتب `~/name/` حيث يتلقى عناصر قائمة، كما داخل `~{`. ونوع العناصر لا يحدد ما يُستدعى |

## 3. أخطاء القراءة

| الرسالة | المعنى والإصلاح |
|---|---|
| `unexpected end of input while reading a list` | قوس إغلاق ناقص. يشير الموضع إلى المكان الذي انتهت عنده القراءة (مثل نهاية الملف)، فابحث عن قوس الفتح |

## 4. أخطاء وقت التشغيل (panic)

| الرسالة | المعنى والإصلاح |
|---|---|
| `panic: divide by zero` | قسمة على صفر في الأعداد الصحيحة أو الكسرية (ratio). أما القسمة على صفر في الفاصلة العائمة فلا تحدث `panic`؛ بل تعطي `inf`/`NaN` |
| `panic: unwrap: called on none` | طُبّقت `unwrap` على `none`. عالج حالة `none` بـ `match` أو `unwrap-or` |
| `panic: Vector: index 5 out of bounds` | فهرس خارج النطاق. افحص الطول بـ `len` أو استخدم دالة تعيد `none` عند الخروج عن النطاق (`nth` و`pop` وغيرهما) |
| `panic: an integer argument does not fit a fixnum` | مُرِّر `int` لا يتسع في 63 بتًا إلى وسيط يأخذ فهرسًا أو عددًا |
| `throw: no enclosing (catch 'oops) for this throw` | نُفِّذ `throw` دون `catch` محيطة بالوسم نفسه |
| `panic: <message>` | استدعى البرنامج `(panic "<message>")`. و`assert` الفاشلة تعطي `assertion failed: ...` |

يوقف `panic` العملية كلها حتى عندما يحدث داخل مهمة
([مرجع الصياغة 12.4](syntax.md#124-التفاعل-مع-الميزات-الأخرى)). عبّر عن الإخفاقات التي تريد التعافي
منها بـ `Result` ([الفصل 9 من مرجع الصياغة](syntax.md#9-سياسة-معالجة-الأخطاء)).

## 5. التحذيرات

| الرسالة | المعنى |
|---|---|
| ``warning: redefining function `f` `` | عُرِّفت دالة بالاسم نفسه مرة أخرى. ويسري التعريف الأخير. وتظهر عادةً عندما تصلح تعريفًا في الـ REPL |
