<!-- translated-from: docs/ja/reference/functions/traits.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# السمات القياسية

سمات التكرار والمقارنة والحساب. أما السمات القياسية الأخرى ففي فصولها الخاصة: `Hash`
([HashTable](collections.md#4-hashtablekv)) و`Error`
([أنواع الأخطاء](option-result.md#3-أنواع-الأخطاء-والسمة-error)) و`print-object`
([الطباعة](printing.md#5-print-object-التمثيل-المطبوع-لكل-نوع)) وسمات التدفقات و`Pathish`
([التدفقات والملفات](streams-files.md)). وأي الأنواع تنفّذ أيًّا منها في [الأنواع](../types.md). وطريقة
تعريف السمات في [مرجع الصياغة](../syntax.md#39-deftrait--impl--السمات).

## 1. السمة `Iter` والتكرار

```lisp
(deftrait Iter ()
  (type Item)
  (next ((self Self)) Option<Item>))
```

تنفّذ `Vector<T>`/`HashTable<K,V>`/`Array<T>` السمة `Iter` عبر `vector-iter<T>`/
`hashtable-iter<K,V>`/`array-iter<T>` على الترتيب (احصل على المكرِّر بـ `(iter collection)`).
و`Chan<T>` هي نفسها `Iter` (وتؤدي `recv` دور `next`؛ [القنوات](concurrency.md#2-chant--القنوات)).
أما قوائم `Sexpr` فلا تنفّذ `Iter` (فأنواع عناصرها غير موحدة). وإذا نفّذت `Iter` لنوعك الخاص أمكن المرور
عليه بـ `doiter` كما هو، وتمريره إلى [دوال التسلسلات](sequences.md#4-دوال-التسلسلات-على-iter).

## 2. `Eq` / `Ord` (المقارنة)

تقابل `PartialEq`/`PartialOrd` في Rust (وتُسمّى `Eq`/`Ord`). وتُستخدم في قيود `where` للدوال العامة
لاشتراط أن تكون أنواع العناصر قابلة للمقارنة (`sort`/`member`/`assoc` وهكذا).

```lisp
(deftrait Eq ()
  (equals ((self Self) (other Self)) bool)                ; يجب تنفيذه
  (not-equals ((self Self) (other Self)) bool             ; تنفيذ افتراضي
    (not (equals self other))))
(deftrait Ord (Eq)                                        ; ترث من Eq
  (less ((self Self) (other Self)) bool)                  ; يجب تنفيذه
  (less-equal ((self Self) (other Self)) bool (not (less other self)))
  (greater ((self Self) (other Self)) bool (less other self))
  (greater-equal ((self Self) (other Self)) bool (not (less self other))))
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
(deftrait Add () (add ((self Self) (other Self)) Self))
(deftrait Sub () (sub ((self Self) (other Self)) Self))
(deftrait Mul () (mul ((self Self) (other Self)) Self))
(deftrait Div () (div ((self Self) (other Self)) Self))
(deftrait Rem () (remainder ((self Self) (other Self)) Self))
(deftrait Bits ()
  (bit-and ((self Self) (other Self)) Self)
  (bit-or  ((self Self) (other Self)) Self)
  (bit-xor ((self Self) (other Self)) Self)
  (bit-not ((self Self)) Self)
  (shift ((self Self) (count int)) Self))                 ; المسافة دائمًا int (كما في ash)
(deftrait Number (Add Sub Mul Div Rem Ord))               ; بلا توابع؛ تجميع للست
```

**داخل القيد يمكنك كتابة المعامِلات.** عندما يكون المستقبِل متغير نوع مقيّدًا بـ `where` تُقرأ المعامِلات
على أنها توابع السمة (`+`→`add` و`-`→`sub` و`*`→`mul` و`/`→`div` و`rem`→`remainder` و
`logand`→`bit-and` و`=`→`equals` و`<`→`less` ...):

```lisp
(defun sum3<T> ((a T) (b T) (c T)) T (where (Number T))
  (+ a b c))                                              ; = (add (add a b) c)
```

لا يُسمّى تابع السمة `+` لأن `+` اسم تابع مضمن و`impl` ترفض إعادة تعريفه
(`cannot redefine built-in method`). ولا توجد `Neg`: فـ `(- x)` تتوسع إلى `(- (- x x) x)`، فتكفي `Sub`.

تُنفَّذ: `Add`/`Sub`/`Mul`/`Div`/`Rem`/`Number` لكل الأنواع العددية (عدا `complex`)، و`Bits` لكل أنواع
الأعداد الصحيحة و`int`.
