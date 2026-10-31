<!-- translated-from: docs/ja/reference/functions/traits.md @ 9a4e7d23b09eb3f3244bfb876df1ec57576d6874 -->
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
  (next (self) Option<Item>))
```

تنفّذ `Vector<T>`/`HashTable<K,V>`/`Array<T>` السمة `Iter` عبر `vector-iter<T>`/
`hashtable-iter<K,V>`/`array-iter<T>` على الترتيب (احصل على المكرِّر بـ `(iter collection)`).
و`Chan<T>` هي نفسها `Iter` (وتؤدي `recv` دور `next`؛ [القنوات](concurrency.md#2-chant--القنوات)).
أما قوائم `Sexpr` فلا تنفّذ `Iter` (فأنواع عناصرها غير موحدة). وإذا نفّذت `Iter` لنوعك الخاص أمكن المرور
عليه بـ `doiter` كما هو، وتمريره إلى [دوال التسلسلات](sequences.md#4-دوال-التسلسلات-على-iter).

## 2. `Eq` / `Ord` (المقارنة)

تقابل `PartialEq`/`PartialOrd` في Rust (وتُسمّى `Eq`/`Ord`). وتُستخدم في قيود `:where` للدوال العامة
لاشتراط أن تكون أنواع العناصر قابلة للمقارنة (`sort`/`member`/`assoc` وهكذا).

```lisp
(deftrait Eq ()
  (equals (self (other Self)) bool)                ; يجب تنفيذه
  (not-equals (self (other Self)) bool             ; تنفيذ افتراضي
    (not (equals self other))))
(deftrait Ord (Eq)                                        ; ترث من Eq
  (less (self (other Self)) bool)                  ; يجب تنفيذه
  (less-equal (self (other Self)) bool (not (less other self)))
  (greater (self (other Self)) bool (less other self))
  (greater-equal (self (other Self)) bool (not (less self other))))
```

لتنفيذ `Eq` تكتب `equals` فقط، ولتنفيذ `Ord` تكتب `less` فقط. وتملأ التنفيذات الافتراضية الباقي. وترث
`Ord` من `Eq`، فيلزم `impl Eq X` قبل `impl Ord X`.

يمكن استدعاء كل تابع سمة كدالة كما هو (داخل قيد `where (Eq A)`/`(Ord A)`، أو على نوع ملموس ينفّذه):

| الاسم | الصيغة | النوع | الوصف |
|---|---|---|---|
| `equals` | `(equals a b)` | `(A,A)→bool` حيث `Eq A` | هل هما متساويان (`==` في Rust) |
| `not-equals` | `(not-equals a b)` | `(A,A)→bool` حيث `Eq A` | هل هما غير متساويين (`!=`) |
| `less` | `(less a b)` | `(A,A)→bool` حيث `Ord A` | `a < b` |
| `less-equal` | `(less-equal a b)` | `(A,A)→bool` حيث `Ord A` | `a <= b` |
| `greater` | `(greater a b)` | `(A,A)→bool` حيث `Ord A` | `a > b` |
| `greater-equal` | `(greater-equal a b)` | `(A,A)→bool` حيث `Ord A` | `a >= b` |

تُنفَّذ `Eq` لـ: كل الأنواع العددية (من `i8` إلى `u32` / `f32` / `f64` / `int` / `ratio`)، و`bool`
و`char` و`string` و`symbol` و`complex`، و`Sexpr` (`eq`، أي الهوية؛ وتُستخدم في أنماط القيم في `match`)،
و`cons-cell<A,B>` (تراجعيًا، عندما تكون العناصر `Eq`). وتُنفَّذ `Ord` لـ: كل الأنواع العددية، و`char`
و`string`، و`cons-cell<A,B>` (معجميًا، عندما تكون العناصر `Ord`).

لا تتداخل أسماء التوابع مع المعامِلات المضمنة (`= /= < <= > >=`) ولا مع `eq`/`lt` لأن المضمنة لا يمكن
إعادة تعريفها، ويفوّض كل تنفيذ إليها. ومعامِلات المقارنة العددية نفسها توابع مضمنة في كل نوع مستقبِل
([الأعداد](numbers.md)، [السلاسل النصية والمحارف](collections.md)). وداخل القيد تُقرأ كتابة المعامِلات على
أنها توابع السمة (الفصل 3).

## 3. سمات الحساب (`Add` / `Sub` / `Mul` / `Div` / `Rem` / `Bits` / `Number`)

طبقة تتيح للشيفرة العامة أن تشترط "نوعًا يمكن جمعه". **أما الحساب على الأنواع الملموسة فيستخدم
المعامِلات المضمنة** ([الأعداد](numbers.md)) ولا يمرّ بهذه الطبقة.

```lisp
(deftrait Add () (add (self (other Self)) Self))
(deftrait Sub () (sub (self (other Self)) Self))
(deftrait Mul () (mul (self (other Self)) Self))
(deftrait Div () (:type Output) (div (self (other Self)) Output))
(deftrait Rem () (remainder (self (other Self)) Self))
(deftrait Bits ()
  (bit-and (self (other Self)) Self)
  (bit-or  (self (other Self)) Self)
  (bit-xor (self (other Self)) Self)
  (bit-not (self) Self)
  (shift (self (count int)) Self))                 ; المسافة دائمًا int (كما في ash)
(deftrait Number (Add Sub Mul Rem Ord))                   ; بلا طرق؛ تركيب من خمس
```

**داخل القيد يمكنك كتابة المعامِلات.** عندما يكون المستقبِل متغير نوع مقيّدًا بـ `:where` تُقرأ المعامِلات
على أنها توابع السمة (`+`→`add` و`-`→`sub` و`*`→`mul` و`/`→`div` و`rem`→`remainder` و
`logand`→`bit-and` و`=`→`equals` و`<`→`less` ...):

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
  (push (self (x Item)) ()))
```

مجموعة يمكن إضافة العناصر إليها واحدًا تلو الآخر. تطلب الدالة العامة «نوعًا يمكن إضافة `T` إليه» بـ `(:where (Push C (Item T)))`، وتضيف إلى أي مجموعة من هذا القبيل بالاستدعاء نفسه `(push c x)`.

| النوع | ما يفعله `push` |
|---|---|
| `Vector<T>` | يضيف في النهاية |
| `Deque<T>` | يضيف في النهاية (`push-back`) |
| `Array<T>` | يضيف عند مؤشر الملء (`push-extend`). ويحدث `panic` بدون مؤشر ملء |
| `HashSet<T>` | يضعه في المجموعة (`insert`). ولا يكون `Push` إلا حين يطبّق نوع العناصر `Hash` |

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
  (pop (self) Option<Item>))
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
