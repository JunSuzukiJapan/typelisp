<!-- translated-from: docs/ja/reference/functions/sequences.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# الأزواج والتعبيرات الرمزية والتسلسلات

الزوج العام `cons-cell` وبيانات التعبيرات الرمزية `Sexpr` والرموز ودوال التسلسلات المكتوبة فوق `Iter`
والدوال عالية الرتبة.

## 1. الأزواج `cons-cell<A,B>`

`cons`/`car`/`cdr` هي المُنشِئ ودوال الوصول إلى الحقول في **نوع الزوج العام `cons-cell<A,B>`** (وهو
`defstruct` في المكتبة القياسية). ويمكن قراءة الحقول إما بـ `variable::car`/`variable::cdr` (صياغة الوصول
في `defstruct` الموصوفة في [مرجع الصياغة](../syntax.md#36-defstruct--البنى-أنواع-يعرفها-المستخدم)) أو
بـ `(car variable)`/`(cdr variable)`. ولتغييرها استخدم `(setf variable::car v)`/`(setf variable::cdr v)`.

| الاسم | الصيغة | النوع | الوصف |
|---|---|---|---|
| `cons` | `(cons a b)` | `(A,B)→cons-cell<A,B>` | يصنع زوجًا |
| `car` | `(car p)` | `cons-cell<A,B>→A` | العنصر الأول |
| `cdr` | `(cdr p)` | `cons-cell<A,B>→B` | الباقي |

ويقوم `cons-cell` أيضًا مقام صياغة الصف (tuple). فدوال CL التي تعيد قيمًا متعددة (الخارج والباقي في
`floor`، والقيمة والموضع في `read-from-string` وهكذا) تعيد في هذه اللغة `cons-cell`.

## 2. بيانات التعبيرات الرمزية `Sexpr`

لنوع البيانات `Sexpr` الذي تعيده `read` ستة عشر متغايرًا:
`int | i8 | i16 | i32 | u8 | u16 | u32 | f32 | f64 | char | bool | sym | str | cons | ratio | path`.
وتُعالَج خلايا التعبيرات الرمزية لا بـ `cons`/`car`/`cdr` العامة في الفصل 1 بل بدوال `sexpr-*`. وتُستخدم
أساسًا في أجسام `defmacro` لبناء الصيغ وتفكيكها.

**نوع بيانات التعبيرات الرمزية هو `Option<Sexpr>`.** فالقائمة الفارغة ليست متغايرًا من `Sexpr` بل هي `none`
من `Option`، و`Sexpr` نفسه يعني "تعبير رمزي غير فارغ". ولذلك تأخذ دوال `sexpr-*` وتعيد `Option<Sexpr>`.

- `()` هي القائمة الفارغة حيث يُتوقَّع `Option<Sexpr>` (ويمكن كتابتها أيضًا `(Option::none)`)
- يتّسع `Sexpr` ضمنيًا حيث يُتوقَّع `Option<Sexpr>` (دون تحويل وقت التشغيل). أما الاتجاه المعاكس، أي
  استخدام `Option<Sexpr>` على أنه `Sexpr`، فيدّعي "هذه ليست القائمة الفارغة"، فيجب التصريح به بـ `match`
  أو `unwrap`
- في `match` يمكن كتابة المتغايرات الستة عشر لـ `Sexpr` و`none` **مسطّحة في قائمة الفروع نفسها**
  ([مرجع الصياغة](../syntax.md#43-match--مطابقة-الأنماط))

| الاسم | الصيغة | النوع | الوصف |
|---|---|---|---|
| `sexpr-cons` | `(sexpr-cons a b)` | `(Option<Sexpr>,Option<Sexpr>)→Option<Sexpr>` | يصنع خلية `Sexpr` |
| `sexpr-car` | `(sexpr-car s)` | `Option<Sexpr>→Option<Sexpr>` | العنصر الأول. **والقائمة الفارغة للقائمة الفارغة** (كما في CL). ويحدث `panic` على ذرة ليست `Cons` |
| `sexpr-cdr` | `(sexpr-cdr s)` | `Option<Sexpr>→Option<Sexpr>` | الباقي. **والقائمة الفارغة للقائمة الفارغة** (كما في CL). ويحدث `panic` على ذرة ليست `Cons` |
| `sexpr-consp` | `(sexpr-consp s)` | `Option<Sexpr>→bool` | هل هي `Cons` |
| `sexpr-null` | `(sexpr-null s)` | `Option<Sexpr>→bool` | هل هي القائمة الفارغة |
| `sexpr-atom` | `(sexpr-atom s)` | `Option<Sexpr>→bool` | هل ليست `Cons` |
| `sexpr-symp` | `(sexpr-symp s)` | `Option<Sexpr>→bool` | هل هي `Sym` (رمز) |
| `sexpr-int` | `(sexpr-int s)` | `Option<Sexpr>→int` | محتوى المتغاير `int` (fixnum أو bignum). ويحدث `panic` على نوع آخر |
| `sexpr-i8` `sexpr-i16` `sexpr-i32` `sexpr-u8` `sexpr-u16` `sexpr-u32` | `(sexpr-i32 s)` | `Option<Sexpr>→W` | محتوى المتغاير بذلك العرض. ويحدث `panic` على نوع آخر |
| `sexpr-f32` `sexpr-f64` | `(sexpr-f64 s)` | `Option<Sexpr>→f32` / `→f64` | محتوى متغايرات الفاصلة العائمة. ويحدث `panic` على نوع آخر |
| `sexpr-char` | `(sexpr-char s)` | `Option<Sexpr>→char` | محتوى `Char`. ويحدث `panic` على نوع آخر |
| `sexpr-bool` | `(sexpr-bool s)` | `Option<Sexpr>→bool` | محتوى `Bool`. ويحدث `panic` على نوع آخر |
| `sexpr-str` | `(sexpr-str s)` | `Option<Sexpr>→string` | محتوى `Str`. ويحدث `panic` على نوع آخر |
| `sexpr-sym-name` | `(sexpr-sym-name s)` | `Option<Sexpr>→string` | اسم `Sym`. ويحدث `panic` على نوع آخر |
| `eq` `eql` | `(op a b)` | `(Option<Sexpr>,Option<Sexpr>)→bool` | مقارنة الهوية (`Cons`/`Str` تقارن هوية الكائن، والباقي يقارن القيم) |
| `equal` | `(equal a b)` | `(Option<Sexpr>,Option<Sexpr>)→bool` | مساواة بنيوية (`Cons` تراجعيًا، و`Str` بالمحتوى) |
| `equalp` | `(equalp a b)` | `(Option<Sexpr>,Option<Sexpr>)→bool` | مثل `equal`، مع مقارنة غير حساسة لحالة الأحرف ومقارنة الأعداد عبر الأنواع |
| `sexpr-append` | `(sexpr-append a b)` | `(Option<Sexpr>,Option<Sexpr>)→Option<Sexpr>` | يضم قائمتي `Sexpr` (دون إتلاف). وتتوسع `,@` إليها |
| `sexpr-map` | `(sexpr-map f lst)` | `((fn (Option<Sexpr>) Option<Sexpr>),Option<Sexpr>)→Option<Sexpr>` | قائمة `Sexpr` جديدة بتطبيق `f` على كل عنصر من قائمة `Sexpr` (فـ `map` في الفصل 4 لـ `Iter` ولا تستطيع المرور على قائمة `Sexpr`) |

توجد تسع دوال وصول عددية، واحدة لكل نوع، لأن `Sexpr` هو "الموضع الوحيد الذي لا يُكتب فيه نوع القيمة في أي
مكان آخر". فـ `u8` يوضع في `Sexpr` يدخل على صورة المتغاير `u8` ولا يخرج إلا بـ `(sexpr-u8 s)`. وتمريره إلى
`(sexpr-int s)` يحدث `panic`؛ فلا يوسّع الجواب بصمت أبدًا. والأعداد الصحيحة في البيانات المقروءة
(`'(1 2 3)` ووسائط الماكرو) من المتغاير `int` وتُقرأ بـ `(sexpr-int s)`.

وليس لقوائم `Sexpr` عمليات مُتلِفة مثل `rplaca`/`nconc`. فلا يمكن تغيير خلية `Sexpr` بعد صنعها.

## 3. الرموز

`symbol` هو نوع الرموز نفسها. ويتحول ضمنيًا حيث يلزم `Sexpr`، لكن لا تلقائيًا في الاتجاه الآخر.

| الاسم | الصيغة | النوع | الوصف |
|---|---|---|---|
| `symbol->string` | `(symbol->string s)` | `symbol→string` | يستخرج اسم الرمز |
| `string->symbol` | `(string->symbol s)` | `string→symbol` | يصنع رمزًا من سلسلة (يدرجه intern) |
| `keywordp` | `(keywordp s)` | `symbol→bool` | هل هو كلمة مفتاحية (`:name`). والنقطتان الرأسيتان جزء من الاسم، فيفحص الاختبار المحرف الأول ([مرجع الصياغة](../syntax.md#1-العناصر-المعجمية)) |

وبخصوص `gensym` انظر [الماكرو](system.md#8-الماكرو).

## 4. دوال التسلسلات على `Iter`

دوال التسلسلات **دوال عامة على السمة `Iter`**. فمن مجموعة احصل على مكرِّر بـ `(iter coll)` ومرّره
(تدعم هذا `Vector<T>` / `HashTable<K,V>` / `Array<T>`؛ أما قائمة `Sexpr` فلا تنفّذ `Iter`، فلا تنطبق عليها
هذه الدوال). **وتُعاد المجموعة الناتجة على صورة `Vector` جديد.** وتعني `Iter<A>` في الجداول "أي تنفيذ لـ
`Iter` يكون `Item` فيه هو `A`". وللمرور على `Vector` المُعاد من جديد مرّر `(iter result)`.

الدوال التي تأخذ محمولًا (تقابل عائلة `-if` في CL):

| الاسم | الصيغة | النوع | الوصف |
|---|---|---|---|
| `map` | `(map it f)` | `(Iter<A>,(fn (A) U))→Vector<U>` | التحويل (mapping) |
| `filter` | `(filter it pred)` | `(Iter<A>,(fn (A) bool))→Vector<A>` | العناصر التي تحقق المحمول فقط |
| `remove-if` | `(remove-if it pred)` | `(Iter<A>,(fn (A) bool))→Vector<A>` | يزيل العناصر التي تحقق المحمول |
| `find-if` | `(find-if it pred)` | `(Iter<A>,(fn (A) bool))→Option<A>` | أول عنصر يحقق المحمول |
| `position-if` | `(position-if it pred)` | `(Iter<A>,(fn (A) bool))→Option<int>` | أول موضع يحقق المحمول |
| `count-if` | `(count-if it pred)` | `(Iter<A>,(fn (A) bool))→int` | كم عنصرًا يحقق المحمول |
| `every` | `(every it pred)` | `(Iter<A>,(fn (A) bool))→bool` | هل يحقق كل عنصر المحمول |
| `any` | `(any it pred)` | `(Iter<A>,(fn (A) bool))→bool` | هل يحقق أي عنصر المحمول (تقابل `some` في CL؛ اسم لا يتعارض مع المُنشِئ `Some`) |
| `foldl` | `(foldl it f init)` | `(Iter<A>,(fn (B A) B),B)→B` | طي من اليسار |
| `foldr` | `(foldr it f init)` | `(Iter<A>,(fn (A B) B),B)→B` | طي من اليمين |

الفهرسة والطول والتقطيع:

| الاسم | الصيغة | النوع | الوصف |
|---|---|---|---|
| `length` | `(length it)` | `Iter<A>→int` | عدد العناصر |
| `append` | `(append a b ...)` | `(Iter<A>,Iter<A>,...)→Vector<A>` | يضم المكرِّرات. ويمكن إعطاء ثلاثة أو أكثر |
| `concatenate` | `(concatenate 'vector it ...)` / `(concatenate 'string s ...)` | `→Vector<A>` / `→string` | `concatenate` في CL. ويُكتب نوع النتيجة على صورة **ثابت رمز مقتبس** (تستخدم CL محدِّد نوع وقت التشغيل). و`'vector` تأخذ واحدًا أو أكثر، و`'string` صفرًا أو أكثر (`""` للصفر). ولا تغطي قوائم `Sexpr` (استخدم `sexpr-append`) |
| `reverse` | `(reverse it)` | `Iter<A>→Vector<A>` | العكس (دون إتلاف) |
| `nth` | `(nth n it)` | `(int,Iter<A>)→Option<A>` | العنصر `n` (`None` خارج النطاق) |
| `elt` | `(elt it n)` | `(Iter<A>,int)→Option<A>` | `nth` بترتيب وسائط معكوس |
| `take` | `(take it n)` | `(Iter<A>,int)→Vector<A>` | أول `n` عنصر |
| `subseq` | `(subseq it start end)` | `(Iter<A>,int,int)→Vector<A>` | `[start,end)` (تُقيَّد `end` بالطول) |
| `last` | `(last it)` | `Iter<A>→Option<A>` | **العنصر** الأخير (لا "الخلية الأخيرة" كما في CL) |
| `butlast` | `(butlast it)` | `Iter<A>→Vector<A>` | كل العناصر عدا الأخير |

الدوال التي تتطلب قيد `Eq` / `Ord` (تقارن عبر سمة بدلًا من محمول؛
[السمات القياسية](traits.md#2-eq--ord-المقارنة)):

| الاسم | الصيغة | النوع | الوصف |
|---|---|---|---|
| `member` | `(member x it)` | `(A,Iter<A>)→bool` حيث `Eq A` | هل يوجد عنصر مساوٍ لـ `x` (بخلاف CL، `bool` لا بقية القائمة) |
| `find` | `(find x it)` | `(A,Iter<A>)→Option<A>` حيث `Eq A` | أول عنصر مساوٍ لـ `x` |
| `position` | `(position x it)` | `(A,Iter<A>)→Option<int>` حيث `Eq A` | أول موضع مساوٍ لـ `x` |
| `count` | `(count x it)` | `(A,Iter<A>)→int` حيث `Eq A` | كم عنصرًا يساوي `x` |
| `sort` | `(sort it cmp)` | `(Iter<A>,(fn (A A) bool))→Vector<A>` | `(sort sequence predicate)` في CL. فرز مستقر دون إتلاف. و`cmp` هي `true` عندما "يسبق الوسيط الأول الثاني بدقة" |
| `assoc` | `(assoc k it)` | `(K,Iter<cons-cell<K,V>>)→Option<cons-cell<K,V>>` حيث `Eq K` | أول زوج يساوي `car` فيه `k`. واستخرج القيمة بـ `(cdr p)` |

وهذه وكثير من دوال الفصل 5 تأخذ أيضًا وسائط CL بالكلمات المفتاحية `:key` / `:test` / `:test-not` /
`:start` / `:end` / `:from-end` / `:count` (الفصل 6).

## 5. بقية دوال التسلسلات في CL

كلها دوال عامة على `Iter`، كما في الفصل 4. وتُعاد المجموعات الناتجة على صورة `Vector` جديدة.

| الاسم | الصيغة | النوع | الوصف |
|---|---|---|---|
| `first`…`tenth` | `(first it)` | `Iter<A>→Option<A>` | الفهارس المسماة في CL |
| `rest` | `(rest it)` | `Iter<A>→Vector<A>` | كل العناصر عدا الأول (`Vector` جديد لا ذيل مشترك) |
| `copy-seq` | `(copy-seq it)` | `Iter<A>→Vector<A>` | يحوّل مكرِّرًا إلى `Vector` (`copy-seq`/`copy-list` في CL) |
| `revappend` | `(revappend a b)` | `(Iter<A>,Iter<A>)→Vector<A>` | `a` معكوسة يتبعها `b` |
| `Vector::filled` | `(Vector::filled n x)` | `(int,T)→Vector<T>` | `n` نسخة من `x` (`make-list`/`make-sequence` في CL). وكما في `Vector::new` يأتي وسيط النوع من النوع المتوقع، فـ `let` المجردة تحتاج `the` |
| `member-if` `member-if-not` | `(member-if it pred)` | `(Iter<A>,(fn (A) bool))→bool` | مثل `member`، **`bool`** (فليس للمكرِّر ذيل يُعاد) |
| `notany` `notevery` | `(notany it pred)` | `(Iter<A>,(fn (A) bool))→bool` | نفي `any`/`every` |
| `find-if-not` `position-if-not` `count-if-not` `remove-if-not` | `(op it pred)` | الأنواع نفسها كالنسخ الموجبة | نسخ يُنفى فيها المحمول |
| `remove` | `(remove x it)` | `(A,Iter<A>)→Vector<A>` حيث `Eq A` | يزيل بالقيمة |
| `remove-duplicates` | `(remove-duplicates it)` | `Iter<A>→Vector<A>` حيث `Eq A` | يزيل المكررات. وكما في CL **يُبقى آخر ظهور** (و`:from-end true` تُبقي الأول) |
| `substitute` `substitute-if` | `(substitute new old it)` | `(A,A,Iter<A>)→Vector<A>` | يستبدل بالقيمة / بالمحمول |
| `assoc-if` `rassoc` `rassoc-if` | `(rassoc v it)` | على `Iter<cons-cell<K,V>>` | نسخ المحمول وجانب القيمة من `assoc` |
| `acons` | `(acons k v it)` | `(K,V,Iter<cons-cell<K,V>>)→Vector<cons-cell<K,V>>` | يضيف زوجًا في المقدمة |
| `pairlis` | `(pairlis ks vs)` | `(Iter<K>,Iter<V>)→Vector<cons-cell<K,V>>` | يزاوج بين تسلسلين. ويتوقف عند الأقصر |
| `map2` | `(map2 a b f)` | `(Iter<A>,Iter<B>,(fn (A B) U))→Vector<U>` | `mapcar` في CL على عدة تسلسلات. ويتوقف عند الأقصر |
| `mapc` | `(mapc it f)` | `(Iter<A>,(fn (A) ()))→()` | تحويل لأجل الآثار الجانبية |
| `mapcan` | `(mapcan it f)` | `(Iter<A>,(fn (A) Vector<U>))→Vector<U>` | يحوّل ويضم |
| `maplist` | `(maplist it f)` | `(Iter<A>,(fn (Vector<A>) U))→Vector<U>` | يحوّل على **الذيول** المتتابعة |
| `mapl` | `(mapl it f)` | `(Iter<A>,(fn (Vector<A>) ()))→()` | يحوّل على الذيول لأجل الآثار الجانبية (نظير `maplist` لـ `mapc`) |
| `mapcon` | `(mapcon it f)` | `(Iter<A>,(fn (Vector<A>) Vector<U>))→Vector<U>` | يحوّل على الذيول ويضم (نظير `maplist` لـ `mapcan`) |
| `search` | `(search it sub)` | `(Iter<A>,Iter<A>)→Option<int>` حيث `Eq A` | الموضع الذي تظهر فيه `sub` أول مرة. وإذا كان المستقبِل `string` اختير تابع `string` ([السلاسل النصية](collections.md#1-السلاسل-النصية-string)) |
| `mismatch` | `(mismatch a b)` | `(Iter<A>,Iter<A>)→Option<int>` حيث `Eq A` | أول موضع يختلفان فيه. `none` إذا تساويا |
| `merge` | `(merge a b less)` | `(Iter<A>,Iter<A>,(fn (A A) bool))→Vector<A>` | الدمج. وتشترط CL مدخلات مرتبة؛ أما هذه فتفرز المضموم |
| `adjoin` | `(adjoin x it)` | `(A,Iter<A>)→Vector<A>` حيث `Eq A` | يضيف `x` **في المقدمة** إذا لم يكن موجودًا |
| `union` `intersection` `set-difference` `set-exclusive-or` | `(op a b)` | `(Iter<A>,Iter<A>)→Vector<A>` حيث `Eq A` | عمليات المجموعات. ولا تحدد CL الترتيب؛ وهنا مستقر، **بترتيب أول ظهور** |
| `subsetp` | `(subsetp a b)` | `(Iter<A>,Iter<A>)→bool` حيث `Eq A` | الاحتواء |
| `tailp` `ldiff` | `(tailp tail whole)` | `(Iter<A>,Iter<A>)→bool` / `→Vector<A>` | هل هو لاحقة / الجزء الذي قبل اللاحقة. وتسأل CL عن **بنية مشتركة**، لكن لا توجد بنية تُشارَك، فهذه تسأل عن لاحقة **بالقيم** |
| `seq-equals` | `(seq-equals a b)` | `(Vector<A>,Vector<A>)→bool` حيث `Eq A` | مساواة عنصرًا بعنصر. و`Vector<T>` نفسه لا ينفّذ `Eq` |
| `caar`…`cddddr` | `(cadr p)` | على أزواج متداخلة | دوال CL الـ 28. وهي تمر على **الأزواج لا القوائم**: فـ `cadr` تأخذ `cons-cell<A,cons-cell<B,C>>` |

ما في CL وليس في هذه اللغة: `list*` (فلا مفهوم لقائمة غير سليمة يُستبدل ذيلها)،
`copy-tree`/`copy-alist`/`sublis`/`subst`/`subst-if` (فلا نوع يصف المرور على شجرة غير متجانسة بعمق
اعتباطي؛ ولشجرة من `Sexpr` تقابل `equal` الدالة `tree-equal`)، وعائلة قوائم الخصائص
`getf`/`get-properties`/`symbol-plist`/`remprop` (فلا تمثيل لقائمة غير منمّطة تتناوب فيها المفاتيح
والقيم؛ و`assoc` (قوائم الارتباط) أو `HashTable` تؤديان الدور نفسه)، والدوال التي تحوّل بين `Vector<T>`
وقوائم `Sexpr` (فعناصر قائمة `Sexpr` يمكن أن يكون لكل منها نوع مختلف، فلا يمكن كتابتها بنوع عنصر واحد `T`).

## 6. وسائط الكلمات المفتاحية

تأخذ دوال الفصلين 4 و5 كلمات التسلسلات المفتاحية في CL: `:key` / `:test` / `:test-not` / `:start` /
`:end` / `:from-end` / `:count`. وكلها **اختيارية**.

| الكلمة المفتاحية | النوع | المعنى |
|---|---|---|
| `:key` | `(fn (A) A)` | إسقاط (projection) يُطبَّق على كل عنصر قبل المقارنة أو الاختبار |
| `:test` | `(fn (A A) bool)` | اختبار مساواة يُستخدم بدلًا من `equals` من قيد `Eq`. والوسيط الأول هو **العنصر المبحوث عنه** والثاني هو العنصر (بعد `:key`)، بالترتيب نفسه كما في CL |
| `:test-not` | `(fn (A A) bool)` | نفي `:test` |
| `:start` `:end` | `int` | النافذة `[start, end)` المراد مسحها. والفهارس نسبة إلى التسلسل كله |
| `:from-end` | `bool` | يجيب البحث بـ**آخر** تطابق. ومع `:count` تؤخذ العناصر المتأثرة من النهاية |
| `:count` | `int` | أقصى عدد من العناصر تؤثر فيها عائلتا `remove` / `substitute` |

أي دالة تأخذ أيًّا منها يتبع CL:

| الدالة | الكلمات المفتاحية المأخوذة |
|---|---|
| `find` `position` | `:key` `:test` `:test-not` `:start` `:end` `:from-end` |
| `count` | `:key` `:test` `:test-not` `:start` `:end` |
| `member` `adjoin` | `:key` `:test` `:test-not` |
| `remove` `substitute` | كل ما سبق (بما في ذلك `:count`) |
| `remove-duplicates` | `:key` `:test` `:test-not` `:start` `:end` `:from-end` |
| `find-if` `find-if-not` `position-if` `position-if-not` | `:key` `:start` `:end` `:from-end` |
| `count-if` `count-if-not` | `:key` `:start` `:end` |
| `member-if` `member-if-not` | `:key` |
| `remove-if` `remove-if-not` `substitute-if` | `:key` `:start` `:end` `:from-end` `:count` |
| `assoc` `rassoc` | `:key` `:test` `:test-not` (ويُطبَّق `:key` في `assoc` على `car` وفي `rassoc` على `cdr`) |
| `assoc-if` `rassoc-if` | `:key` |
| `sort` `merge` | `:key` |
| `union` `intersection` `set-difference` `set-exclusive-or` `subsetp` | `:key` `:test` `:test-not` |
| `search` | `:key` `:test` `:test-not` `:from-end` `:start` `:end` `:sub-start` `:sub-end` |
| `mismatch` | `:key` `:test` `:test-not` `:from-end` `:start1` `:end1` `:start2` `:end2` |

```lisp
(find 2 (iter v) :key (lambda ((x int)) int (abs x)))   ; → (some -2)
(remove 2 (iter v) :count 1 :from-end true)             ; يزيل واحدًا فقط، من النهاية
(position 3 (iter v) :start 1)                          ; الفهرس نسبة إلى التسلسل كله
```

**الفروق عن CL**:

1. **يبقى إسقاط `:key` ضمن نوع العنصر** (`(fn (A) A)`). فلا يمكنه الإسقاط إلى نوع آخر كما في CL: فمتغير
   نوع إضافي لا يمكن تحديده عند حذف الوسيط. وحيثما يلزم إسقاط إلى نوع مختلف مرّر `lambda` إلى عائلة
   `-if` بدلًا من ذلك (`(find-if it (lambda ((p ...)) bool (= (car p) 3)))`).
2. **في عمليات البحث المعتمدة على عنصر مرجعي يُطبَّق `:key` على العناصر فقط** (لا على العنصر المبحوث عنه).
   وهذه القاعدة نفسها في `find`/`position`/`count`/`member`/`remove`/`substitute` في CL. وفي عمليات
   المجموعات يكون الجانبان عنصرين، فيُطبَّق عليهما.
3. **كلمات `search` المفتاحية وحدها مسماة لا مرقّمة.** ففي CL `:start1`/`:end1` لـ **النمط** و
   `:start2`/`:end2` للتسلسل المبحوث. أما في هذه اللغة فالمستقبِل يأتي أولًا، فالأرقام نفسها ستعني العكس،
   وبصمت. فـ `:start`/`:end` للمستقبِل و`:sub-start`/`:sub-end` للنمط، فـ `:start1` الشاردة تعطي خطأ
   "unknown keyword". أما `mismatch` و`replace` فلهما ترتيب الوسائط نفسه كما في CL، فتحتفظان بأرقام CL.

## 7. العمليات المتلفة

توابع في `Vector<T>`. **تعدّل المستقبِل وتعيد المستقبِل نفسه**، فتُكتب `(nreverse v)` بالطريقة نفسها كـ
`reverse` ويُعكس `v` نفسه أيضًا.

| الاسم | الصيغة | الوصف |
|---|---|---|
| `nreverse` | `(nreverse v)` | يعكس في مكانه |
| `delete` `delete-if` `delete-if-not` `delete-duplicates` | `(delete v x)` | نسخ في المكان من `remove` / `remove-if` / `filter` / `remove-duplicates` |
| `nsubstitute` `nsubstitute-if` | `(nsubstitute v new old)` | نسخ في المكان من عائلة `substitute` |
| `nbutlast` | `(nbutlast v)` | يُسقط العنصر الأخير |
| `fill` | `(fill v x)` | يضبط كل عنصر على `x`. ولا يتغير الطول |
| `replace` | `(replace v src)` | يكتب فوق البداية بعناصر `src`. `(min (len v) (len src))` عنصرًا |
| `map-into` | `(map-into v src f)` | `v[i] = (f src[i])`. العدد نفسه كما سبق |
| `nconc` | `(nconc v w)` | يلحق عناصر `w` بـ `v`. وبخلاف CL **لا يعيد كتابة بنية مشتركة** (فلا تتأثر `w`) |
| `nreconc` | `(nreconc v w)` | `(nconc (nreverse v) w)` |
| `set-contents` | `(set-contents v src)` | يستبدل محتويات `v` بـ `src` (ويتغير الطول أيضًا) |
| `rplaca` `rplacd` | `(rplaca p x)` | يعيد كتابة `car`/`cdr` في `cons-cell` ويعيد الخلية نفسها |

الكلمات المفتاحية المأخوذة:

| النسخة المُتلِفة | الكلمات المفتاحية المأخوذة |
|---|---|
| `delete` `nsubstitute` | `:key` `:test` `:test-not` `:start` `:end` `:from-end` `:count` |
| `delete-if` `delete-if-not` `nsubstitute-if` | `:key` `:start` `:end` `:from-end` `:count` |
| `delete-duplicates` | `:key` `:test` `:test-not` `:start` `:end` `:from-end` |
| `fill` | `:start` `:end` |
| `replace` | `:start1` `:end1` `:start2` `:end2` (المستقبِل هو `sequence-1` في CL) |

و`vector-push-extend`/`vector-pop` هما ببساطة `push`/`pop` في `Vector<T>`. و`Vector<T>` ينمو دائمًا،
فلا شيء يقابل تمييز CL بين "متجه بمؤشر ملء" و"متجه بسيط".

## 8. الدوال عالية الرتبة

| الاسم | الصيغة | النوع | الوصف |
|---|---|---|---|
| `identity` | `(identity x)` | `T→T` | تعيد وسيطها |
| `const` | `(const x y)` | `(A,B)→A` | تعيد الوسيط الأول |
| `compose` | `(compose f g)` | `((fn (B) C),(fn (A) B))→(fn (A) C)` | تركيب الدوال `f∘g` |
| `flip` | `(flip f)` | `(fn (A B) C)→(fn (B A) C)` | تبدّل وسيطي دالة ذات وسيطين |
| `complement` | `(complement pred)` | `(fn (A) bool)→(fn (A) bool)` | نفي محمول |

لا توجد `constantly` من CL (فنوع الوسيط المُهمَل سيظهر في النوع الراجع فقط ولا يمكن تحديده). اكتب
`(lambda ((x T)) A v)`.
