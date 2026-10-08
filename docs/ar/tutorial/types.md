<!-- translated-from: docs/ja/tutorial/types.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# أساسيات الأنواع

typelisp لغة ذات أنواع ساكنة. يشرح هذا الفصل ما يفعله فاحص الأنواع من أجلك، والأنواع التي ستستخدمها
أكثر من غيرها (`Option` و`Result` والبنى والتعدادات)، والأنواع العامة (generics). ويفترض أنك قرأت
[البدء السريع](intro.md).

## 1. معنى الأنواع الساكنة

في typelisp يُحسم نوع كل تعبير قبل تشغيل البرنامج. والتعبير الذي لا تتلاءم أنواعه خطأ قبل أن يعمل أي
شيء.

```lisp
(defun square ((n int)) int (* n n))

(defun main () ()
  (println "start")
  (println "~a" (square "3")))    ; خطأ في الأنواع

(main)
```

يتوقف تشغيل هذا الملف بخطأ في الأنواع دون أن يطبع حتى `start`.

```
error: main.typl:5:25: type error: type mismatch: expected `int`, found `string`
```

عليك أن تكتب الأنواع لوسائط الدوال وقيمها الراجعة وللمتغيرات العامة ولحقول البنى. أما نوع متغير `let`
فيؤخذ من قيمته الابتدائية.

الأنواع الرئيسية:

| النوع | أمثلة على القيم |
|---|---|
| `int` | `42` و`-7` (أعداد صحيحة بدقة اعتباطية) |
| `i8` `i16` `i32` `u8` `u16` `u32` | أعداد صحيحة بعرض ثابت |
| `f64` `f32` | `1.5` |
| `bool` | `true` و`false` |
| `char` | `#\a` |
| `string` | `"hello"` |
| `symbol` | `'foo` و`:key` |
| `()` | النوع الراجع من دالة لا تعيد قيمة |

لا توجد طريقة للاستعلام عن نوع قيمة وقت التشغيل (لا `typep` ولا `type-of` من Common Lisp)، لأن كل
نوع معروف مسبقًا قبل تشغيل البرنامج.

## 2. `Option<T>`: قيمة قد تكون غائبة

لا يوجد `nil` في typelisp. ويُعبَّر عن "قد لا توجد قيمة" بالنوع `Option<T>`. وقيمة `Option<T>` إما
`some` وتحمل قيمة واحدة من `T`، وإما `none` ولا تحمل شيئًا.

```lisp
(defun safe-div ((a int) (b int)) Option<int>
  (if (= b 0)
      (Option::none)
      (Option::some (/ a b))))
```

```
typl> (safe-div 10 2)
(some 5)
typl> (safe-div 1 0)
none
```

`Option<int>` ليس `int`، فلا يمكن استخدامه في العمليات الحسابية كما هو. و`(+ (safe-div 10 2) 1)` خطأ في
الأنواع. ولاستخدام ما بداخله افصل `some` عن `none` بـ `match`.

```lisp
(defun show-div ((a int) (b int)) string
  (match (safe-div a b)
    ((some q) (format false "~a" q))
    ((none) "undefined")))
```

- في فرع `(some q)` يُربط المحتوى بالمتغير `q`.
- يتحقق `match` من أن فروعه **تغطي كل الحالات**. ونسيان فرع `(none)` خطأ في الأنواع.

### لماذا لا يوجد nil

في لغات كثيرة يمكن لـ `nil` (`null`) أن يحل محل قيمة من أي نوع. والنتيجة أن نسيان معالجة حالة "لا قيمة"
يمرّ دون أن يلاحظه أحد حتى يعمل البرنامج. أما في typelisp فالموضع الذي قد تغيب فيه القيمة نوعه
`Option<T>`، ولا تجتاز الشيفرة فاحص الأنواع ما لم يعالج `match` حالة `none`. فالحالة المنسية تُكتشف
قبل تشغيل البرنامج.

وتسير الشروط على الفكرة نفسها: لا يصلح شرطًا لـ `if` إلا `bool`. ولا توجد قاعدة مثل قاعدة Common Lisp
"كل ما عدا `nil` صحيح".

### العمليات الشائعة

| الصيغة | المعنى |
|---|---|
| `(unwrap-or opt default)` | المحتوى إن كانت `some`؛ والقيمة الافتراضية إن كانت `none` |
| `(unwrap opt)` | يستخرج المحتوى. ويوقف البرنامج عند `none` |
| `(is-some opt)` / `(is-none opt)` | يختبر أيهما هي |

كثير من دوال المكتبة القياسية تعيد `Option`. فمثلًا تعيد `position` الموضع داخل `some` إذا عُثر على
العنصر، و`none` إذا لم يُعثر عليه.

```lisp
(defvar (fruits Vector<string>) (Vector::new))
(push fruits "apple")
(push fruits "banana")

(match (position "banana" (iter fruits))
  ((some i) (println "found at ~a" i))
  ((none) (println "not found")))
```

## 3. `Result<T,E>`: عملية قد تفشل

العملية التي قد تفشل تعيد `Result<T,E>`: `ok` تحمل قيمة من `T` عند النجاح، أو `err` تحمل خطأ `E`
عند الفشل.

```lisp
(match (parse-int "42")
  ((ok n) (println "n = ~a" (+ n 1)))
  ((err e) (println "error: ~a" (message e))))
;; n = 43

(match (parse-int "4x2")
  ((ok n) (println "n = ~a" (+ n 1)))
  ((err e) (println "error: ~a" (message e))))
;; error: parse-int: invalid integer literal: "4x2"
```

ويمكن لدوالك أنت أن تعيد `Result` أيضًا.

```lisp
(defun checked-age ((n int)) Result<int,string>
  (if (< n 0)
      (Result::err "age must not be negative")
      (Result::ok n)))
```

استخدم `Option` عندما لا يحتاج غياب القيمة إلى تفسير، و`Result` عندما تريد أن تقول لماذا فشل شيء. ويتناول
[معالجة الأخطاء](errors.md) معالجتها بالتفصيل.

## 4. `defstruct`: البنى

يُعرَّف النوع ذو الحقول المسماة بـ `defstruct`.

```lisp
(defstruct point
  (x int)
  (y int))
```

يمنحك التعريف ما يلي:

```lisp
(let ((p (point::new 3 4)))     ; إنشاء واحدة (الوسائط بترتيب الحقول)
  (println "~a" p::x)           ; قراءة حقل؛ وتصلح أيضًا (x p)
  (setf p::x 10)                ; تغييره
  (println "~a" p))             ; #<point x: 10 y: 4>
```

ولإعطاء بنية دوالًا خاصة بها استخدم `defmethod`. ونوع الوسيط الأول (`self`) هو الذي يقرر النوع الذي
ينتمي إليه التابع.

```lisp
(defmethod norm2 ((self point)) int
  (+ (* self::x self::x) (* self::y self::y)))

(norm2 (point::new 3 4))        ; => 25
```

وكتابة اسم النوع وحده بدلًا من وسيط `self` تصنع دالة تُستدعى على الصورة `point::origin`.

```lisp
(defmethod origin (point) point
  (point::new 0 0))

(point::origin)                 ; => #<point x: 0 y: 0>
```

## 5. `defenum`: واحد من عدة أشكال

القيمة التي هي واحدة من عدة أشكال، مثل "دائرة أو مستطيل أو نقطة"، تُعرَّف بـ `defenum`. ويسمى كل شكل
**متغايرًا (variant)**. ويمكن لكل متغاير أن يحمل عددًا مختلفًا من القيم وأنواعًا مختلفة.

```lisp
(defenum shape
  (circle int)        ; نصف القطر
  (rect int int)      ; العرض والارتفاع
  (dot))              ; لا يحمل قيمة
```

تُصنع القيم بوضع اسم النوع أمامها كما في `shape::circle`. وفي `match` تُفكَّك باسم المتغاير.

```lisp
(defun area ((s shape)) int
  (match s
    ((circle r) (* 3 r r))
    ((rect w h) (* w h))
    ((dot) 0)))

(area (shape::circle 2))        ; => 12
(area (shape::rect 3 4))        ; => 12
```

وهنا أيضًا يتحقق `match` من تغطية كل الحالات. فإذا أضفت لاحقًا متغايرًا إلى `shape` صار كل `match` لا
يعالجه خطأ في الأنواع، فلا يفوتك أي موضع يحتاج إلى إصلاح.

وبعد `(use shape)` يمكنك كتابة `(rect 5 6)` بدون اسم النوع.

`Option` و`Result` تعدادان مبنيان بهذه الآلية نفسها.

## 6. الأنواع العامة (Generics)

تُعرَّف الدالة التي تعمل لأي نوع بـ **معامِل نوع** `<T>` بعد اسمها.

```lisp
(defun first-or<T> ((v Vector<T>) (default T)) T
  (if (= (len v) 0)
      default
      (get v 0)))
```

لا تعطي النوع عند الاستدعاء. فيُستنتج `T` من الوسائط.

```lisp
(defvar (ints Vector<int>) (Vector::new))
(defvar (names Vector<string>) (Vector::new))
(push names "Ann")

(first-or ints 7)          ; T هو int
(first-or names "none")    ; T هو string
(first-or ints "none")     ; خطأ في الأنواع: ints هي Vector<int> فـ T هو int
```

ويمكن أن تكون البنى والتعدادات عامة أيضًا. و`Vector<T>` و`Option<T>` و`Result<T,E>` أنواع من هذا القبيل.

```lisp
(defstruct pair<A,B>
  (left A)
  (right B))

(pair::new 1 "one")        ; pair<int,string>
```

داخل دالة عامة لا يُعرف شيء عن `T`، فلا يمكنك مقارنة قيم `T` ولا جمعها. ولاشتراط شيء مثل "أي نوع
يمكن مقارنته" استخدم السمات ([السمات](traits.md)).

## 7. إعطاء نوع اسمًا آخر

يعطي `deftype` نوعًا اسمًا آخر.

```lisp
(deftype meters int)
(defun double ((m meters)) meters (* m 2))
```

`meters` مجرد كتابة أخرى لـ `int` وليس نوعًا جديدًا. وتمرير `int` عادي حيث يُتوقَّع `meters` ليس خطأ.
وإذا أردت إبقاءهما منفصلين فاصنع بنية كما في `(defstruct meters (value int))`.

## 8. ماذا تقرأ بعد ذلك

- [السمات](traits.md): إعطاء الأنواع عمليات مشتركة
- [الأنواع](../reference/types.md): الأنواع المضمنة والسمات التي ينفذها كل نوع
