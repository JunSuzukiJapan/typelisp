<!-- translated-from: docs/ja/reference/functions/collections.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# السلاسل النصية والمحارف والمجموعات

`string` و`char` و`Vector<T>` و`HashTable<K,V>` و`Array<T>` و`BitVector`.

## 1. السلاسل النصية `string`

السلاسل النصية غير قابلة للتعديل.

| الاسم | الصيغة | النوع | الوصف |
|---|---|---|---|
| `upcase` | `(upcase s)` | `string→string` | يحوّل إلى أحرف كبيرة (ASCII فقط). ومثل `string-upcase` في CL يعيد سلسلة جديدة. والسلاسل غير قابلة للتعديل، فلا يوجد `nstring-upcase` المُتلِف؛ وهذه تحل محله |
| `downcase` | `(downcase s)` | `string→string` | يحوّل إلى أحرف صغيرة (ASCII فقط). يحل محل `nstring-downcase` |
| `capitalize` | `(capitalize s)` | `string→string` | يكبّر أول حرف في كل كلمة ويصغّر الباقي (`string-capitalize` في CL). والكلمة أطول تسلسل من الحروف والأرقام |
| `length` | `(length s)` | `string→int` | عدد المحارف |
| `ref` | `(ref s i)` | `(string,int)→char` | المحرف `i`. ويحدث `panic` خارج النطاق |
| `substring` | `(substring s start end)` | `(string,int,int)→string` | السلسلة الفرعية `[start,end)` |
| `append` | `(append s1 s2 ...)` | `(string,string,...)→string` | الضم. ويمكن إعطاء ثلاث أو أكثر (مثل `(concatenate 'string ...)`) |
| `<` `<=` `>` `>=` | `(op s1 s2)` | `(string,string)→bool` | مقارنة معجمية |
| `lt` | `(lt s1 s2)` | `(string,string)→bool` | أصغر من بدقة معجميًا (مثل `<`) |
| `eq` `eql` | `(op s1 s2)` | `(string,string)→bool` | مقارنة الهوية (هل هما الكائن نفسه، لا المحتوى نفسه) |
| `equal` | `(equal s1 s2)` | `(string,string)→bool` | يقارن المحتوى (حساس لحالة الأحرف) |
| `equalp` | `(equalp s1 s2)` | `(string,string)→bool` | يقارن المحتوى (غير حساس لحالة الأحرف، ASCII فقط) |
| `/=` | `(/= s1 s2)` | `(string,string)→bool` | هل يختلف المحتوى (`string/=` في CL. وصيغة الوسائط المتعددة تقارن الأزواج المتجاورة) |
| `lessp` `greaterp` `not-lessp` `not-greaterp` | `(op s1 s2)` | `(string,string)→bool` | ترتيب غير حساس لحالة الأحرف (`string-lessp` وأخواتها في CL). ومع بادئة مشتركة تكون الأقصر هي الأصغر |
| `string::filled` | `(string::filled n c)` | `(int,char)→string` | سلسلة من `n` نسخة من `c` (`make-string` في CL) |
| `search` | `(search s sub)` | `(string,string)→Option<int>` | الموضع الذي تظهر فيه `sub` أول مرة. **وفي `search` في CL ترتيب الوسائط معكوس** (`(search pattern sequence)`). والسلسلة الفارغة تُوجَد عند 0. وللكلمات المفتاحية انظر [وسائط الكلمات المفتاحية للتسلسلات](sequences.md#6-وسائط-الكلمات-المفتاحية) |
| `mismatch` | `(mismatch a b)` | `(string,string)→Option<int>` | أول موضع يختلفان فيه. `none` فقط عندما تكونان `equal`. وإذا كانت إحداهما بادئة للأخرى فنهاية الأقصر. والكلمات المفتاحية كما سبق |
| `trim` `left-trim` `right-trim` | `(trim s)` / `(trim s bag)` | `(string &optional string)→string` | يزيل المحارف الموجودة في `bag` من الطرفين / اليسار / اليمين (`string-trim` وأخواتها في CL). وبدون `bag` المسافات البيضاء `" \t\n\r"` |
| `split` | `(split s sep)` | `(string,string)→Vector<string>` | يقسّم عند `sep`. ولا مقابل له في CL. والفواصل المتتالية تنتج عناصر فارغة. ويحدث `panic` إذا كانت `sep` فارغة |
| `to-string` | `(to-string x)` | `T→string` | يحوّل إلى سلسلة كما يفعل `~a`. منفَّذ لـ `int`/`i32`/`f64`/`bool`/`char`/`string` (`princ-to-string` في CL) |
| `string->utf8` | `(string->utf8 s)` | `string→Vector<int>` | يرمّز بـ UTF-8 (كل عنصر 0..255) |
| `utf8->string` | `(utf8->string bytes)` | `Vector<int>→Option<string>` | يفك الترميز. `none` إذا لم يكن UTF-8 صالحًا |

## 2. المحارف `char`

`char` قيمة عددية Unicode. ويتعامل تحويل الحالة والتصنيف مع نطاق ASCII فقط.

| الاسم | الصيغة | النوع | الوصف |
|---|---|---|---|
| `upcase` | `(upcase c)` | `char→char` | يحوّل إلى حرف كبير (ASCII فقط) |
| `downcase` | `(downcase c)` | `char→char` | يحوّل إلى حرف صغير (ASCII فقط) |
| `<` `<=` `>` `>=` | `(op c1 c2)` | `(char,char)→bool` | مقارنة بنقطة الشيفرة (code point) |
| `lt` | `(lt c1 c2)` | `(char,char)→bool` | أصغر من بدقة بنقطة الشيفرة (مثل `<`) |
| `alphap` | `(alphap c)` | `char→bool` | هل هو حرف ASCII |
| `digitp` | `(digitp c)` | `char→bool` | هل هو رقم ASCII |
| `eq` `eql` `equal` | `(op c1 c2)` | `(char,char)→bool` | يقارن القيم |
| `equalp` | `(equalp c1 c2)` | `(char,char)→bool` | يقارن القيم مع تجاهل حالة الأحرف (`char-equal` في CL) |
| `/=` | `(/= c1 c2)` | `(char,char)→bool` | هل تختلف القيم (`char/=` في CL. **وصيغة الوسائط المتعددة تقارن الأزواج المتجاورة**، بخلاف CL التي تسأل هل تختلف كل الأزواج) |
| `lessp` `greaterp` `not-lessp` `not-greaterp` | `(op c1 c2)` | `(char,char)→bool` | ترتيب غير حساس لحالة الأحرف (`char-lessp` وأخواتها في CL) |
| `upper-casep` `lower-casep` `both-casep` | `(op c)` | `char→bool` | كبير / صغير / له تمييز حالة أصلًا (`upper-case-p` وأخواتها في CL) |
| `alphanumericp` | `(alphanumericp c)` | `char→bool` | حرف أو رقم (الاسم نفسه كما في CL) |
| `graphicp` | `(graphicp c)` | `char→bool` | هل هو قابل للطباعة. يشمل المسافة ولا يشمل السطر الجديد ولا الجدولة (`graphic-char-p` في CL) |
| `standardp` | `(standardp c)` | `char→bool` | هل هو أحد محارف CL القياسية الـ 96، أي `graphicp` مع السطر الجديد (`standard-char-p` في CL) |
| `char->int` | `(char->int c)` | `char→int` | القيمة العددية Unicode (والعكس هو `int->char`/`try-int->char` في [الأعداد](numbers.md#1-الأعداد-الصحيحة-ذات-العرض-الثابت)). يقابل `char-code`/`char-int` في CL |
| `char->string` | `(char->string c)` | `char→string` | سلسلة من محرف واحد. تغطي دالة `string` في CL هذا بأخذ محدِّد (designator)، لكن هذه اللغة لا محدِّدات فيها، فالاتجاه في الاسم |
| `digit-weight` | `(digit-weight c)` / `(digit-weight c radix)` | `(char &optional int)→Option<int>` | **وزن** الرقم في ذلك الأساس (`digit-char-p` في CL). و`digitp` دالة منفصلة تعيد `bool` |
| `digit->char` | `(digit->char w)` / `(digit->char w radix)` | `(int &optional int)→Option<char>` | المحرف الموافق للوزن `w`. أحرف كبيرة من 10 فما فوق (`digit-char` في CL؛ والأساس 36 على الأكثر) |
| `char->name` | `(char->name c)` | `char→Option<string>` | اسم المحرف. فقط المحارف المسماة التي يستطيع القارئ قراءتها لها أسماء (`char-name` في CL) |
| `name->char` | `(name->char s)` | `string→Option<char>` | المحرف الموافق لاسم. غير حساس لحالة الأحرف، ويقبل أيضًا أسماء القارئ المستعارة (`linefeed`/`null`) (`name-char` في CL) |

لا يوجد ثابت يقابل `char-code-limit` (فالحد الأعلى لـ `char` تحدده Unicode لا اللغة).

## 3. `Vector<T>`

مصفوفة قابلة للنمو.

| الاسم | الصيغة | النوع | الوصف |
|---|---|---|---|
| `new` | `(Vector::new)` | `()→Vector<T>` | يصنع متجهًا فارغًا. ويأتي وسيط النوع من النوع المتوقع، لذا في `let` مجردة اكتب `(the Vector<i32> (Vector::new))` |
| `filled` | `(Vector::filled n x)` | `(int,T)→Vector<T>` | `n` نسخة من `x` |
| `push` | `(push v x)` | `(Vector<T>,T)→Unit` | يضيف في النهاية |
| `get` | `(get v i)` | `(Vector<T>,int)→T` | يقرأ العنصر `i`. ويحدث `panic` خارج النطاق |
| `set` | `(set v i x)` | `(Vector<T>,int,T)→Unit` | يغيّر العنصر `i`. ويحدث `panic` خارج النطاق. ويمكن كتابته أيضًا `(setf (get v i) x)` |
| `len` | `(len v)` | `Vector<T>→int` | عدد العناصر |
| `pop` | `(pop v)` | `Vector<T>→Option<T>` | يزيل العنصر الأخير ويعيده. `None` إذا كان فارغًا (وبخلاف `get`/`set` لا يحدث `panic`) |
| `iter` | `(iter v)` | `Vector<T>→vector-iter<T>` | يصنع مكرِّرًا ينفّذ `Iter` |
| `pushnew` | `(pushnew v x)` | `(Vector<T>,T)→Unit` حيث `Eq T` | يضيف `x` إذا لم يوجد عنصر مساوٍ (`pushnew` في CL. ولا يحتاج إلى إعادة كتابة موضع (place)، فهو تابع لا ماكرو) |

`map`/`filter` وأخواتهما [دوال تسلسلات](sequences.md#4-دوال-التسلسلات-على-iter): مرّر المتجه عبر `iter`
كما في `(map (iter v) f)`. والعمليات المُتلِفة (`nreverse` و`delete` وهكذا) في
[العمليات المُتلِفة](sequences.md#7-العمليات-المتلفة).

## 4. `HashTable<K,V>`

| الاسم | الصيغة | النوع | الوصف |
|---|---|---|---|
| `new` | `(HashTable::new)` | `()→HashTable<K,V>` | يصنع جدولًا فارغًا |
| `get` | `(get h k)` | `(HashTable<K,V>,K)→Option<V>` | بحث |
| `set` | `(set h k v)` | `(HashTable<K,V>,K,V)→Unit` | إدراج أو استبدال |
| `remove` | `(remove h k)` | `(HashTable<K,V>,K)→Option<V>` | يزيل الإدخال ويعيد القيمة القديمة إن وُجدت |
| `count` | `(count h)` | `HashTable<K,V>→int` | عدد الإدخالات |
| `clear` | `(clear h)` | `HashTable<K,V>→Unit` | يزيل كل شيء |
| `keys` | `(keys h)` | `HashTable<K,V>→Vector<K>` | لقطة من المفاتيح |
| `values` | `(values h)` | `HashTable<K,V>→Vector<V>` | لقطة من القيم |
| `entries` | `(entries h)` | `HashTable<K,V>→Vector<cons-cell<K,V>>` | لقطة من الأزواج `(k . v)` |
| `iter` | `(iter h)` | `HashTable<K,V>→hashtable-iter<K,V>` | مكرِّر ينفّذ `Iter`. والعناصر قيم `cons-cell` على الصورة `(k . v)`. يقابل `with-hash-table-iterator` في CL؛ وتعمل عليه `doiter`/`map`/`filter` وغيرها كما هو |
| `maphash` | `(maphash h f)` | `(HashTable<K,V>,(fn (K V) ()))→Unit` | `maphash` في CL |
| `size` | `(size h)` | `HashTable<K,V>→int` | `hash-table-size` في CL. وفي هذا الجدول هو عدد الإدخالات المشغولة (يساوي `count`) |

**يمكن لأي نوع ينفّذ `Hash` أن يكون مفتاحًا**، بما في ذلك أنواع `defstruct`/`defenum`. وتحمل `get`/`set`/
`remove` القيد `(where (Hash K))`، فالجدول المفتاحه نوع لا ينفّذه **خطأ في الأنواع** (ولا يوجد `Hash` لـ
`f64` بسبب `NaN`).

```lisp
(deftrait Hash (Eq)
  (sxhash ((self Self)) int))              ; تعيد قيمة غير سالبة تتسع في fixnum
```

منفَّذة لـ: `int` والأنواع الصحيحة الستة ذات العرض الثابت و`bool` و`char` و`string` و`symbol` (وليس لأعداد
الفاصلة العائمة). ولأنواعك الخاصة أبقِ النتيجة غير سالبة بتطبيق `logand` عليها مع `*sxhash-mask*` (2^30-1).
ولتجزئة سلسلة نصية يمكنك استدعاء `(sxhash-string s)` (‏FNV-1a بـ 32 بت)، وهي التي يستخدمها التنفيذ الخاص
بـ `string`.

```lisp
(defstruct point (x int) (y int))
(impl Eq point
  (equals ((self Self) (other Self)) bool
    (if (= self::x other::x) (= self::y other::y) false)))
(impl Hash point
  (sxhash ((self Self)) int (logand (+ (* 31 self::x) self::y) *sxhash-mask*)))

(let ((h (the HashTable<point,string> (HashTable::new))))
  (progn (set h (point::new 1 2) "a")
         (get h (point::new 1 2))))          ; => (some "a")
```

يتحدد ما إذا كان مفتاحان هما نفسه بـ **نوع المفتاح نفسه** (`sxhash`، و`equals` من `Eq` السمة الأعلى لـ
`Hash`)، لا بهوية الكائن. ولهذا يمكنك، كما في المثال، البحث بمفتاح "قيمة مختلفة لكنها مساوية".

ولا بأس أن تتصادم `sxhash` (فعقد `Hash` في اتجاه واحد فقط: القيم المتساوية يجب أن يكون لها التجزئة
نفسها). وتُميَّز المفاتيح المتصادمة بـ `equals`.

## 5. `Array<T>` (المصفوفات متعددة الأبعاد)

`defstruct` في المكتبة القياسية. وهو ليس نوعًا مضمنًا، فكل ما يمكنك فعله بـ `defstruct` يمكن فعله به.

| الاسم | الصيغة | النوع | الوصف |
|---|---|---|---|
| `make` | `(Array::make dims init &key fill-pointer)` | `(Vector<int>,T)→Array<T>` | `make-array` في CL. تُنسخ `dims`. و`init` القيمة الابتدائية لكل خلية (`:initial-element` في CL؛ ولا توجد في هذه اللغة "خلية غير مربوطة"، فهو مطلوب). و`:fill-pointer` لبعد واحد فقط |
| `get` / `set` | `(get a idx)` / `(set a idx x)` | `(Array<T>,Vector<int>)→T` | `aref` / `(setf (aref …))` في CL. ويحدث `panic` إذا كان فهرس خارج النطاق |
| `aref` | `(aref a i j …)` | — | كتابة CL بفهارس مجردة. تتوسع إلى `get`/`set` أعلاه. ويعمل `(setf (aref a i j) v)` أيضًا |
| `row-major-get` / `row-major-set` | `(row-major-get a i)` | `(Array<T>,int)→T` | `row-major-aref` في CL. فهرس مسطّح |
| `rank` | `(rank a)` | `Array<T>→int` | `array-rank` في CL |
| `dimension` | `(dimension a n)` | `(Array<T>,int)→int` | `array-dimension` في CL |
| `dimensions` | `(dimensions a)` | `Array<T>→Vector<int>` | `array-dimensions` في CL. يعيد **نسخة**، كما تعيد CL قائمة جديدة |
| `total-size` | `(total-size a)` | `Array<T>→int` | `array-total-size` في CL (عدد الخلايا المحجوزة، ولا علاقة له بمؤشر الملء) |
| `len` | `(len a)` | `Array<T>→int` | `length` في CL على المصفوفات. مؤشر الملء إن وُجد، وإلا `total-size` |
| `in-bounds` | `(in-bounds a idx)` | `(Array<T>,Vector<int>)→bool` | `array-in-bounds-p` في CL. false (لا خطأ) حتى عندما يكون **عدد** الفهارس خاطئًا |
| `row-major-index` | `(row-major-index a idx)` | `(Array<T>,Vector<int>)→int` | `array-row-major-index` في CL |
| `adjust` | `(adjust a dims init)` | `(Array<T>,Vector<int>,T)→Unit` | `adjust-array` في CL. لا يمكن أن تتغير الرتبة. وتبقى العناصر التي تظل داخل النطاق عند فهارسها، وتأخذ الخلايا الجديدة `init`. وبخلاف CL لا تعيد المصفوفة (فكل مصفوفة في هذه اللغة قابلة للتعديل، فلا توجد مصفوفة ثانية لإعادتها) |
| `push-extend` | `(push-extend a x)` | `(Array<T>,T)→Unit` | `vector-push-extend` في CL. ويحدث `panic` بدون مؤشر ملء |
| `pop` | `(pop a)` | `Array<T>→Option<T>` | `vector-pop` في CL. `none` إذا كانت فارغة |
| `fill-pointer` | `(fill-pointer a)` | `Array<T>→Option<int>` | مؤشر الملء (`none` إذا لم يوجد). ويمكن كتابته بـ `(setf a::fill-pointer …)` |
| `iter` | `(iter a)` | `Array<T>→array-iter<T>` | مكرِّر بترتيب الصفوف (row-major). ويتوقف عند مؤشر الملء إن وُجد |

- **الفهارس `Vector<int>`.** فلا يستطيع التابع أن يعلن "الوسيط من النوع نفسه مكررًا أي عدد من المرات في
  النهاية"، وتسدّ صياغة `aref` المختصرة هذه الفجوة.
- `array-element-type` / `simple-vector-p` / `adjustable-array-p` / `array-has-fill-pointer-p`
  **غير موجودة**. فالنوع الساكن للمستقبِل يجيب عن هذه الأسئلة أصلًا.
- `Array::new` هو المُنشِئ بترتيب الحقول الذي يولّده `defstruct` وليس مقصودًا لإنشاء المصفوفات. استخدم
  `Array::make`.
- **تُطبع المصفوفات بصياغة المصفوفات في CL.** الرتبة 1 هي `#(1 2 3)`؛ والرتب الأخرى `#nA` متبوعة بذلك
  العدد من مستويات الأقواس (`#2A((1 2 3) (4 5 6))`)؛ والرتبة 0 هي `#0A5`. وتتوقف الطباعة عند مؤشر الملء إن
  وُجد. وضبط `*print-array*` ([الطباعة](printing.md#6-التحكم-في-مقدار-ما-يطبع)) على false يطبع الشكل
  فقط، `#<array 2x3>`. ولا تُطبع بالصورة المضمنة `#<array<...> ...>` إلا مصفوفة عناصرها `defstruct` بلا
  `print-object` (وليس هذا خطأ).

## 6. `BitVector` (متجهات البتات)

تسلسل ثابت الطول من البتات. وهو `defstruct` في المكتبة القياسية.

| الاسم | الصيغة | النوع | الوصف |
|---|---|---|---|
| `make` | `(BitVector::make n)` | `int→BitVector` | الطول `n` وكل البتات 0 |
| `get` / `set` | `(get v i)` / `(set v i b)` | `(BitVector,int)→bool` | يحدث `panic` خارج النطاق |
| `bit` / `sbit` | `(bit v i)` | `(BitVector,int)→bool` | كتابتا CL. ويعمل `(setf (bit v i) b)` أيضًا. وتختلف `sbit` في CL عن `bit` فقط في اشتراط متجه بتات بسيط، لكن هذه اللغة فيها نوع واحد من متجهات البتات |
| `len` | `(len v)` | `BitVector→int` | عدد البتات |
| `bit-and` `bit-ior` `bit-xor` `bit-eqv` `bit-nand` `bit-nor` `bit-andc1` `bit-andc2` `bit-orc1` `bit-orc2` | `(op a b)` | `(BitVector,BitVector)→BitVector` | تعيد متجه بتات جديدًا. ويحدث `panic` إذا اختلف الطولان. ولا يوجد وسيط ثالث كما في CL (وجهة النتيجة) |
| `bit-not` | `(bit-not v)` | `BitVector→BitVector` | المتمم |

لا يوجد `bit-vector-p` (فالنوع الساكن يجيب عنه).
