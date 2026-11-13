<!-- translated-from: docs/ja/reference/functions/traits.md @ cedb434055399f2f83e17dcdaa55007fe4fc9dad -->
# السمات القياسية

سمات التكرار والمقارنة والحساب وإضافة العناصر وإخراجها. أما السمات القياسية الأخرى ففي فصولها الخاصة: `Hash`
([HashTable](collections.md#4-hashtablekv)) و`Error`
([أنواع الأخطاء](option-result.md#3-أنواع-الأخطاء-والسمة-error)) و`print-object`
([الطباعة](printing.md#5-print-object-التمثيل-المطبوع-لكل-نوع)) وسمات التدفقات و`Pathish`
([التدفقات والملفات](streams-files.md)). وأي الأنواع تنفّذ أيًّا منها في [الأنواع](../types.md). وطريقة
تعريف السمات في [مرجع الصياغة](../syntax.md#39-deftrait--impl--السمات).

## 1. السمة `Iter` والتكرار

```lisp
(deftrait Iter ()
  (:type Item)
  (defmethod next (self) Option<Item>))
```

تنفّذ `Vector<T>`/`HashTable<K,V>`/`Array<T>` السمة `Iter` عبر `vector-iter<T>`/
`hashtable-iter<K,V>`/`array-iter<T>` على الترتيب (احصل على المكرِّر بـ `(iter collection)`).
و`Chan<T>` هي نفسها `Iter` (وتؤدي `recv` دور `next`؛ [القنوات](concurrency.md#2-chant--القنوات)).
أما قوائم `Sexpr` فلا تنفّذ `Iter` (فأنواع عناصرها غير موحدة). وإذا نفّذت `Iter` لنوعك الخاص أمكن المرور
عليه بـ `doiter` كما هو، وتمريره إلى [دوال التسلسلات](sequences.md#4-دوال-التسلسلات-على-iter).

## 2. `Equal` / `Ord` (المقارنة)

`Equal` هي السمة التي يستعملها `equal`، وتقابل `PartialEq` في Rust. وتقابل `Ord` السمة `PartialOrd` في
Rust. تُستعملان في قيود `:where` للدوال العامة لاشتراط أن يكون نوع العناصر قابلًا للمقارنة.

```lisp
(deftrait Equal ()
  (defmethod equal (self (other Self)) bool))                ; يجب تنفيذه
(deftrait Ord (Equal)                                     ; يرث Equal
  (defmethod less (self (other Self)) bool)                  ; يجب تنفيذه
  (defmethod less-equal (self (other Self)) bool (not (less other self)))
  (defmethod greater (self (other Self)) bool (less other self))
  (defmethod greater-equal (self (other Self)) bool (not (less self other))))
```

لتنفيذ `Equal` اكتب `equal`، ولتنفيذ `Ord` اكتب `less` وحده. وتملأ التنفيذات الافتراضية بقية `Ord`.
ترث `Ord` السمة `Equal`، فيلزم `impl Equal X` قبل `impl Ord X`.

يستدعي `(equal a b)` تنفيذ `Equal` لنوع `a`. وكتابته لنوع لا ينفّذ `Equal` خطأ نوع. للمقارنة بالهوية
استعمل `eq`/`eql`، وللمقارنة بالبنية استعمل `equalp`. وهذه الثلاث تصلح لقيمة من أي نوع.

يمكن استدعاء كل دالة من دوال السمة دالةً كما هي (داخل قيد `where (Equal A)`/`(Ord A)`، أو على نوع محدد
ينفّذها):

| الاسم | الصيغة | النوع | الوصف |
|---|---|---|---|
| `equal` | `(equal a b)` | `(A,A)→bool` حيث `Equal A` | هل هما متساويان (`==` في Rust) |
| `less` | `(less a b)` | `(A,A)→bool` حيث `Ord A` | `a < b` |
| `less-equal` | `(less-equal a b)` | `(A,A)→bool` حيث `Ord A` | `a <= b` |
| `greater` | `(greater a b)` | `(A,A)→bool` حيث `Ord A` | `a > b` |
| `greater-equal` | `(greater-equal a b)` | `(A,A)→bool` حيث `Ord A` | `a >= b` |

تُنفَّذ `Equal` لـ: كل الأنواع العددية (`i8` إلى `u32` / `f32` / `f64` / `int` / `ratio`)، و`bool` و`char`
و`string` و`symbol` و`complex`، و`Sexpr`، و`cons-cell<A,B>` والصفوف (عنصرًا عنصرًا، عندما تكون العناصر
`Equal`)، و`Option<T>` و`Result<T,E>` (عندما تكونان من المتغير نفسه ومحتواهما `equal`). وفي الأنواع المدمجة
تكون الدالة المدمجة `equal` الخاصة بالنوع هي تنفيذ `Equal` كما هي. تُقارَن الأعداد بـ `=`، والسلاسل
بالمحتوى مع التمييز بين الأحرف الكبيرة والصغيرة. وتُنفَّذ `Ord` لـ: كل الأنواع العددية، و`char` و`string`،
و`cons-cell<A,B>` (معجميًا، عندما تكون العناصر `Ord`).

و`equal` القائمة (`Sexpr`) هو `equal` في CL: تُقارَن خلايا cons بالبنية، والسلاسل بالمحتوى، وكل ذرة أخرى
بـ `eql`. وهذه المقارنة تمرّ على القيم وقت التشغيل ولا تستدعي أي دالة. لذا تُقارَن البنية داخل القائمة
بالهوية كما في CL، أيًّا كان `impl Equal` لنوعها.

لا تتداخل أسماء دوال `Ord` مع العوامل المدمجة (`< <= > >=`) أو `lt`، لأن المدمج لا يُعاد تعريفه وكل تنفيذ
يفوّض إليها. وعوامل المقارنة العددية نفسها دوال مدمجة بحسب نوع المستقبِل ([الأعداد](numbers.md)،
[السلاسل والمحارف](collections.md)). وداخل القيد يُقرأ العامل المكتوب على أنه دالة السمة (الفصل 3). يصير
`=` هو `equal`، ويصير `/=` هو `(not (equal a b))`.

## 3. سمات الحساب (`Add` / `Sub` / `Mul` / `Div` / `Rem` / `Bits` / `Number`)

طبقة تتيح للشيفرة العامة أن تشترط "نوعًا يمكن جمعه". **أما الحساب على الأنواع الملموسة فيستخدم
المعامِلات المضمنة** ([الأعداد](numbers.md)) ولا يمرّ بهذه الطبقة.

```lisp
(deftrait Add () (defmethod add (self (other Self)) Self))
(deftrait Sub () (defmethod sub (self (other Self)) Self))
(deftrait Mul () (defmethod mul (self (other Self)) Self))
(deftrait Div () (:type Output) (defmethod div (self (other Self)) Output))
(deftrait Rem () (defmethod remainder (self (other Self)) Self))
(deftrait Bits ()
  (defmethod bit-and (self (other Self)) Self)
  (defmethod bit-or  (self (other Self)) Self)
  (defmethod bit-xor (self (other Self)) Self)
  (defmethod bit-not (self) Self)
  (defmethod shift (self (count int)) Self))                 ; المسافة دائمًا int (كما في ash)
(deftrait Number (Add Sub Mul Rem Ord))                   ; بلا طرق؛ تركيب من خمس
```

**داخل القيد يمكنك كتابة المعامِلات.** عندما يكون المستقبِل متغير نوع مقيّدًا بـ `:where` تُقرأ المعامِلات
على أنها توابع السمة (`+`→`add` و`-`→`sub` و`*`→`mul` و`/`→`div` و`rem`→`remainder` و
`logand`→`bit-and` و`=`→`equal` و`<`→`less` ...):

```lisp
(defun sum3<T> ((a T) (b T) (c T)) T (:where (Number T))
  (+ a b c))                                              ; = (add (add a b) c)
```

لا يُسمّى تابع السمة `+` لأن `+` اسم تابع مضمن و`impl` ترفض إعادة تعريفه
(`cannot redefine built-in method`). ولا توجد `Neg`: فـ `(- x)` تتوسع إلى `(- (- x x) x)`، فتكفي `Sub`.

تُنفَّذ: `Add`/`Sub`/`Mul`/`Div`/`Rem`/`Number` لكل الأنواع العددية (عدا `complex`)، و`Bits` لكل أنواع
الأعداد الصحيحة و`int`.


**`Div` يسمّي نوع نتيجته بالنوع المرتبط `Output`.** كما في CL، `/` على عددين صحيحين هو `rational`،
لذا فإن `Output` لـ`int` هو `rational`، ولكل نوع عددي آخر هو النوع نفسه. ولأن `Output` يختلف من نوع
لآخر، لا يتضمن `Number` السمة `Div` (يجب أن تثبّت قائمة السمات الأعلى كل نوع مرتبط). الدالة العامة
التي تقسم تستقبل نوع النتيجة بـ`(:where (Div T (Output R)))`، أو تطلب قسمة مغلقة في `T`
بـ`(:where (Div T (Output T)))` (وحينها لا يمكن تمرير `int`).

## 4. السمة `Push` (إضافة العناصر)

```lisp
(deftrait Push ()
  (:type Item)
  (defmethod push (self (x Item)) ()))
```

مجموعة يمكن إضافة العناصر إليها واحدًا تلو الآخر. تطلب الدالة العامة «نوعًا يمكن إضافة `T` إليه» بـ `(:where (Push C (Item T)))`، وتضيف إلى أي مجموعة من هذا القبيل بالاستدعاء نفسه `(push c x)`.

| النوع | ما يفعله `push` |
|---|---|
| `Vector<T>` | يضيف في النهاية |
| `Deque<T>` | يضيف في النهاية (`push-back`) |
| `Array<T>` | يضيف عند مؤشر الملء (`push-extend`). ويحدث `panic` بدون مؤشر ملء |
| `HashSet<T>` | يضعه في المجموعة (`insert`) |

```lisp
(use std::collections::Deque)
(defun push-all<C,T> ((c C) (xs Vector<T>)) () (:where (Push C (Item T)))
  (doiter (x (iter xs)) (push c x)))

(let ((v (the Vector<int> (Vector::new)))
      (d (the Deque<int> (Deque::new))))
  (push-all v (the Vector<int> #(1 2)))
  (push-all d (the Vector<int> #(3 4)))
  (println "~a ~a" v d))                                  ; #(1 2) #<deque 3 4>
```

يمكن تمرير نوع خاص بك يطبّق `Push` إلى الدوال العامة نفسها. ويمكن أيضًا أن يكون كائن سمة، كما في `:dyn Push<int>`.

## 5. السمة `Pop` (إخراج العناصر)

```lisp
(deftrait Pop ()
  (:type Item)
  (defmethod pop (self) Option<Item>))
```

مجموعة يمكن أن يُخرَج منها العنصر الذي أضافه `push` أخيرًا. تعطي `none` إذا كانت فارغة. تطلبها الدالة العامة بـ `(:where (Pop C (Item T)))`.

| النوع | ما يفعله `pop` |
|---|---|
| `Vector<T>` | يُخرج العنصر الأخير |
| `Deque<T>` | يُخرج العنصر الأخير (`pop-back`) |
| `Array<T>` | يُخرج العنصر الذي يسبق مؤشر الملء مباشرة. يحدث panic إذا لم يكن هناك مؤشر ملء |

ليس لـ `HashSet<T>` «عنصر أخير»، فهو `Push` وليس `Pop`.

```lisp
(use std::collections::Deque)
(defun pop-all<C,T> ((c C)) Vector<T> (:where (Pop C (Item T)))
  (let ((out (the Vector<T> (Vector::new))) (more true))
    (while more
      (match (pop c)
        ((some x) (push out x))
        ((none) (progn (setf more false) ()))))
    out))

(let ((v (the Vector<int> #(1 2 3)))
      (d (the Deque<int> (Deque::new))))
  (push d 1) (push d 2)
  (println "~a ~a" (pop-all v) (pop-all d)))              ; #(3 2 1) #(2 1)
```

كما في `Push`، يمكن لنوعك الخاص أن ينفّذها، ويمكن أن تكون كائن سمة، كما في `:dyn Pop<int>`.
