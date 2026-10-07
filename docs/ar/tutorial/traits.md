<!-- translated-from: docs/ja/tutorial/traits.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# السمات (Traits)

السمة (trait) وعد بأن "هذا النوع يدعم هذه العمليات". تتيح السمات لعدة أنواع أن تشترك في عمليات بالاسم
نفسه، فلا تضطر الدالة التي تستخدمها إلى أن تُكتب مرة لكل نوع. وهي تعمل تقريبًا كما تعمل السمات في Rust
تمامًا. يفترض هذا الفصل أنك قرأت [أساسيات الأنواع](types.md).

## 1. تعريف سمة وتنفيذها

عرّف العمليات التي تعيد مساحة شكل واسمه على أنها السمة `Shape`.

```lisp
(deftrait Shape ()
  (area ((self Self)) int)
  (name ((self Self)) string))
```

- القوسان `()` بعد اسم السمة هما قائمة السمات التي ترث منها (القسم 4). اتركهما فارغتين عندما لا توجد
  سمات موروثة.
- يعلن كل سطر عن تابع (method). و`Self` تعني "النوع الذي ينفّذ هذه السمة".

ولتنفيذ سمة لنوع اكتب `impl`.

```lisp
(defstruct circle (r int))
(defstruct rect (w int) (h int))

(impl Shape circle
  (area ((self Self)) int (* 3 self::r self::r))
  (name ((self Self)) string "circle"))

(impl Shape rect
  (area ((self Self)) int (* self::w self::h))
  (name ((self Self)) string "rect"))
```

تُستدعى التوابع المنفَّذة تمامًا كالدوال العادية.

```lisp
(area (circle::new 2))     ; => 12
(area (rect::new 3 4))     ; => 12
```

إهمال تابع واحد فقط مما أعلنته السمة خطأ في الأنواع عند `impl`.

## 2. قيود السمات: "أي نوع ينفّذ هذه السمة"

يمكنك وضع شرط على معامل نوع في دالة عامة بـ `where`. ويسمى هذا **قيد سمة (trait bound)**.

```lisp
(defun report<T> ((s T)) string
  (where (Shape T))
  (format false "~a: ~a" (name s) (area s)))

(report (circle::new 1))   ; => "circle: 3"
(report (rect::new 2 5))   ; => "rect: 10"
```

بفضل `(where (Shape T))` يمكن للجسم استخدام `name` و`area` على قيم `T`. وبدون القيد لا يُعرف شيء عن
`T` فلا يمكن استدعاؤهما.

وتمرير نوع لا ينفّذ `Shape` خطأ في الأنواع عند الاستدعاء.

```
(report 5)
error: ...: type error: report: type `int` does not implement trait `shape` required by `where` clause on type parameter `t`
```

تحصل الدالة العامة على نسخة خاصة بها لكل نوع تُستدعى به. ولا تدخل فيها اختبارات أنواع ولا تفرعات وقت
التشغيل.

## 3. التنفيذات الافتراضية

إذا كان لتابع في سمة جسم فإن ذلك الجسم يُستخدم عندما يهمل `impl` ذلك التابع.

```lisp
(deftrait Describe ()
  (label ((self Self)) string)
  (describe ((self Self)) string
    (format false "<~a>" (label self))))

(impl Describe circle
  (label ((self Self)) string "a circle"))            ; describe هي الافتراضية

(impl Describe rect
  (label ((self Self)) string "a rect")
  (describe ((self Self)) string "[rect]"))           ; المكتوبة هنا لها الأولوية

(describe (circle::new 1))     ; => "<a circle>"
(describe (rect::new 1 1))     ; => "[rect]"
```

## 4. تنفيذ السمات القياسية

في المكتبة القياسية سمات أيضًا. وتنفيذ إحداها يجعل الدوال القياسية التي تستخدمها متاحة لنوعك.

| السمة | التوابع الواجب تنفيذها | ما تتيحه |
|---|---|---|
| `Eq` | `equals` | `member` و`find` و`position` ونمط `(= expr)` في `match` وغيرها |
| `Ord` | `less` | `less-equal` و`greater` وغيرها. وترث `Ord` من `Eq` |
| `print-object` | `print-object` | كيف تعرض `println` وأخواتها القيم |
| `Iter` | `next` | `doiter` و`map` و`filter` و`sort` وغيرها |
| `Error` | `message` و`source` | الاستخدام كنوع خطأ ([معالجة الأخطاء](errors.md)) |

لننفّذ `Eq` و`Ord` لنوع يمثل مبلغًا من المال. ولأن `Ord` ترث من `Eq` يجب أن يأتي `impl` الخاص بـ `Eq`
أولًا.

```lisp
(defstruct money (yen int))

(impl Eq money
  (equals ((self Self) (other Self)) bool (= self::yen other::yen)))

(impl Ord money
  (less ((self Self) (other Self)) bool (< self::yen other::yen)))

(greater (money::new 5) (money::new 3))     ; => true (التنفيذ الافتراضي في Ord)
```

تنفيذ `print-object` يقرر كيف تعرض `println` القيمة. ويكون وسيط `escape` هو `true` عندما تُطلب صورة
يمكن قراءتها من جديد، كما مع `~s`.

```lisp
(impl print-object money
  (print-object ((self Self) (escape bool)) string
    (format false "~a yen" self::yen)))

(println "~a" (money::new 500))              ; 500 yen
```

وبالجمع مع قيد سمة يمكنك كتابة دالة تعمل لأي نوع ينفّذ `Ord`.

```lisp
(defun largest<T> ((v Vector<T>)) Option<T>
  (where (Ord T))
  (foldl (iter v)
         (lambda ((best Option<T>) (x T)) Option<T>
           (match best
             ((some b) (if (less b x) (Option::some x) best))
             ((none) (Option::some x))))
         (Option::none)))
```

وإذا أُعطيت `Vector` فيه قيم `money` هي 300 و900 و100 بهذا الترتيب أعادت `(some 900 yen)`.

## 5. `:dyn`: التعامل مع قيم من أنواع مختلفة معًا

كل عناصر `Vector<T>` من النوع نفسه، فلا يمكن وضع قيم `circle` و`rect` في `Vector<circle>` واحد. وللتعامل
مع "أي شيء ينفّذ `Shape`" معًا استخدم النوع `:dyn Shape`.

```lisp
(defun total-area ((shapes Vector<:dyn Shape>)) int
  (let ((sum 0))
    (doiter (s (iter shapes))
      (setf sum (+ sum (area s))))
    sum))

(let ((shapes (the Vector<:dyn Shape> (Vector::new))))
  (push shapes (circle::new 1))
  (push shapes (rect::new 2 3))
  (doiter (s (iter shapes))
    (println "~a -> ~a" (name s) (area s)))
  (println "total ~a" (total-area shapes)))
```

```
circle -> 3
rect -> 6
total 9
```

- تُحوَّل قيمة `circle` أو `rect` الموضوعة حيث يُتوقَّع `:dyn Shape` تلقائيًا.
- يتحدد وقت التشغيل أي نوع تُنفَّذ `area` الخاصة به في الاستدعاء `(area s)` بحسب نوع ما تحمله `s`.
- وضع قيمة لا ينفّذ نوعها `Shape` حيث يُتوقَّع `:dyn Shape` خطأ في الأنواع.

الاختيار بين قيود السمات في القسم 2 و`:dyn`:

| | قيد السمة (`where`) | `:dyn Trait` |
|---|---|---|
| متى يتحدد التابع المستدعى | قبل التشغيل | وقت التشغيل |
| خلط الأنواع في `Vector` واحد | غير ممكن | ممكن |
| الأنواع القابلة للاستخدام | بلا قيد | البنى والتعدادات و`int` و`string` و`f64` وغيرها (وليس `bool` ولا `char` ولا `symbol` ولا `i32` ونحوها) |

القائمة الدقيقة للأنواع التي يمكن استخدامها موجودة في
[مرجع الصياغة 3.9](../reference/syntax.md#39-deftrait--impl--السمات).

لا يمكن استخدام بعض السمات مع `:dyn`: تلك التي تستخدم توابعها `Self` لوسيط غير `self` أو للقيمة الراجعة
(مثل `equals` في `Eq`). فبما أن النوع غير معروف حتى وقت التشغيل لا توجد طريقة لإنتاج "قيمة من النوع
نفسه".

## 6. القيود

- أبقِ تعريف السمة وما يُكتب لها من `impl` والشيفرة التي تستخدمها عبر `:dyn` في وحدة (ملف) واحدة. فلا
  يمكن بعد جعل السمة مرئية للوحدات الأخرى.
- تشترك الأنواع والسمات في مساحة أسماء واحدة. ففي وحدة واحدة لا يمكن أن يكون لنوع وسمة الاسم نفسه.

## 7. ماذا تقرأ بعد ذلك

- [الماكرو](macros.md): تعريف صياغة خاصة بك
- [مرجع الصياغة 3.9](../reference/syntax.md#39-deftrait--impl--السمات): التنفيذات الشاملة (blanket)
  والأنواع المرتبطة وغيرها
- [السمات القياسية](../reference/functions/traits.md): قائمة السمات في المكتبة القياسية
