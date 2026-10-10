<!-- translated-from: docs/ja/reference/functions/option-result.md @ 7bc227fccd76e389ecb46e23b24421514247c86c -->
# Option وResult وأنواع الأخطاء

## 1. `Option<T>` / `Result<T,E>`

المُنشِئات: لـ `Option<T>` الصيغتان `Some(T)` / `None`. ولـ `Result<T,E>` الصيغتان `Ok(T)` / `Err(E)`. ويمكن
أن يكون `E` أي نوع: أنواع الأخطاء الملموسة المضمنة، والأنواع التي تكتبها بنفسك بـ
`defstruct`/`defenum`، كلها تصلح هناك على حد سواء (الفصل 3).

| الاسم | الصيغة | Option | Result | الوصف |
|---|---|---|---|---|
| `unwrap` | `(unwrap self)` | `Option<T>→T` | `Result<T,E>→T` | يستخرج القيمة. ويحدث `panic` عند `None`/`Err` |
| `unwrap-or` | `(unwrap-or self default)` | `(Option<T>,T)→T` | `(Result<T,E>,T)→T` | القيمة أو القيمة الافتراضية |
| `is-some` | `(is-some self)` | `Option<T>→bool` | — | هل هي `Some` |
| `is-none` | `(is-none self)` | `Option<T>→bool` | — | هل هي `None` |
| `is-ok` | `(is-ok self)` | — | `Result<T,E>→bool` | هل هي `Ok` |
| `is-err` | `(is-err self)` | — | `Result<T,E>→bool` | هل هي `Err` |
| `expect` | `(expect self msg)` | `(Option<T>,string)→T` | `(Result<T,E>,string)→T` | تُخرج القيمة. عند `None`/`Err` تُطلق panic بالرسالة `msg` |
| `unwrap-or-else` | `(unwrap-or-else self f)` | `(Option<T>,fn()→T)→T` | `(Result<T,E>,fn(E)→T)→T` | القيمة، أو نتيجة `f`. لا تُستدعى `f` إلا عند `None`/`Err` |
| `map` | `(map self f)` | `(Option<T>,fn(T)→U)→Option<U>` | `(Result<T,E>,fn(T)→U)→Result<U,E>` | تطبّق `f` على محتوى `Some`/`Ok` |
| `map-err` | `(map-err self f)` | — | `(Result<T,E>,fn(E)→F)→Result<T,F>` | تطبّق `f` على محتوى `Err` |
| `and-then` | `(and-then self f)` | `(Option<T>,fn(T)→Option<U>)→Option<U>` | `(Result<T,E>,fn(T)→Result<U,E>)→Result<U,E>` | عند `Some`/`Ok` تمرّر المحتوى إلى `f` وتُرجع نتيجتها |
| `or-else` | `(or-else self f)` | `(Option<T>,fn()→Option<T>)→Option<T>` | `(Result<T,E>,fn(E)→Result<T,F>)→Result<T,F>` | عند `None`/`Err` تُرجع نتيجة `f` |
| `ok-or` | `(ok-or self e)` | `(Option<T>,E)→Result<T,E>` | — | تحوّل `Some(v)` إلى `Ok(v)` و`None` إلى `Err(e)` |

المُنشِئات هي `Option::some`/`Option::none`/`Result::ok`/`Result::err` (أو الأسماء المجردة
`some`/`none`/`ok`/`err` بعد `(use option)`/`(use result)`).

يُكتب التفرع صراحة بـ `match`، أو يُسلسَل بـ `map`/`and-then` وغيرهما مما سبق. ولا توجد صياغة تقابل
`?` في Rust.

`map` الخاصة بـ `Option`/`Result` تابع (method)، وهي غير `map` الخاصة بـ[التسلسلات](sequences.md).
وهي التي تُستدعى حين يكون نوع الوسيط الأول `Option`/`Result`.

الماكرو `->` يمرّر قيمة بالترتيب بوصفها الوسيط الأول لكل صيغة تالية (مثل `->` في Clojure).
`(-> x (f a) (g b))` تصبح `(g (f x a) b)`. والاسم بلا أقواس، `h`، يُعامَل على أنه `(h x)`. الوسيط
الأول للتابع هو مستقبِله، لذا تتسلسل المُركِّبات كما هي:

```lisp
(defun half ((n int)) Option<int>
  (if (= 0 (mod n 2)) (option::some (/ n 2)) (option::none)))

(-> (option::some 8)
    (and-then half)                          ; (some 4)
    (and-then half)                          ; (some 2)
    (map (lambda ((x int)) int (* x 10)))    ; (some 20)
    (unwrap-or 0))                           ; => 20

(-> (parse-int "x")
    (map-err (lambda ((e ParseIntError)) string (message e)))
    (unwrap-or-else (lambda ((m string)) int (length m))))
```

## 2. التمثيل وقت التشغيل لنوع `Option<T>`

كما في Rust، **لا يصنع `Option<T>` صندوقًا عادةً**. فـ `some v` هي `v` نفسها و`none` هي قيمة القائمة
الفارغة، دون حجز ودون مستوى غير مباشر. وكل من `Option<Sexpr>` (حيث القائمة الفارغة هي `none`) و
`Option<int>` و`Option<string>` و`Option<my-struct>` و`Option<f64>` و`Option<(fn ...)>` يتخذ هذه الصورة.

ولا يُستخدم صندوق إلا عندما لا يمكن تمييز قيمة من `T` عن قيمة القائمة الفارغة:

| `T` | التمثيل | السبب |
|---|---|---|
| `Option<U>` (متداخل) | صندوق | ستكون `none` الداخلية القيمة نفسها كـ `none` الخارجية |
| `()` | صندوق | قيمة `()` هي قيمة القائمة الفارغة نفسها |
| `ptr` / `c-long` / `c-ulong` | صندوق | كل البتات الـ 64 قيمة، فلا يبقى مجال للتمييز |
| أي شيء آخر | بلا صندوق | — |

يتحدد التمثيل بالنوع وحده ولا يمكن قراءته من قيمة. وعند الطباعة تُعاد بناء `(some ...)`/`none` من النوع
الساكن، فتطبع `(format false "~a" opt)` القيمة `(some 1)`. وثمة قيدان:

- **لا يمكن وضعه في `:dyn Trait`** (فتمرير قيمة من `Option<int>` كتبتَ لها
  `(impl Speak Option<int> ...)` إلى `:dyn Speak` خطأ).
- التحويل الهابط `(the Option<T> ...)` من `Sexpr` **يسمّي مُنشِئًا**:
  `(the Option<int> (some x))` / `(the Option<int> (none))`. أما الصيغة التي تربط القيمة كلها،
  `(the Option<int> o)`، فخطأ.

## 3. أنواع الأخطاء والسمة `Error`

اتباعًا لـ `std::error::Error` في Rust، **`Error` ليست نوعًا بل سمة**. والأنواع الملموسة التي تمثل
الأخطاء منفصلة لكل غرض، وكل منها ينفّذ `Error`.

| النوع | يُنتَج بواسطة |
|---|---|
| `ParseIntError` | `parse-int` |
| `ParseFloatError` | `parse-float` |
| `ReadError` | `read` / `read-from-string` / `read-sexpr` |
| `EvalError` | `eval` / `macroexpand` |
| `FileError` | عمليات الملفات والتدفقات ([التدفقات والملفات](streams-files.md)) |
| `NetError` | عمليات الشبكة ([الشبكات](network.md)) |
| `SimpleError` | `(SimpleError::new msg)` / `(simple-error msg)`. وهو `simple-error` في CL: الخيار الافتراضي عندما تريد فقط أن تقول ما حدث |
| `WrappedError` | `(wrap-error msg cause)`. نوع يحمل رسالتك الخاصة والسبب معًا؛ وهو سبب وجود `source` في السمة `Error` |

كل من `ParseIntError` إلى `NetError` هو "تعداد بمتغاير واحد يحمل سلسلة رسالة واحدة"، واسم النوع واسم
المتغاير واحد (`(match e ((ParseIntError m) m))`، ويُبنى بـ `(ParseIntError::ParseIntError "...")`). وليس
فيها ما هو خاص: فهي تُعامَل تمامًا كأنواع الأخطاء الخاصة بك المكتوبة بـ `(defstruct my-err (...))` /
`(defenum my-err ...)`.

| الاسم | الصيغة | النوع | الوصف |
|---|---|---|---|
| `message` | `(message e)` | `Self→string` | رسالة الخطأ (تابع من السمة `Error`) |
| `source` | `(source e)` | `Self→Option<:dyn Error>` | السبب الذي يغلّفه هذا الخطأ، أو `None` إن لم يوجد (`Error::source` في Rust) |
| `as-dyn-error` | `(as-dyn-error r)` | `Result<T,E>→Result<T,:dyn Error>` (`E` ينفّذ `Error`) | يوسّع نوع خطأ ملموسًا إلى كائن السمة |
| `describe-error` | `(describe-error e)` | `E→string` (`E` ينفّذ `Error`) | الرسالة وسلسلة الأسباب التي يُعثر عليها باتباع `source`، سببًا واحدًا في كل سطر. ولا مقابل له في CL ("caused by" في Rust) |

إذا نفّذت `Error` لنوع الخطأ الخاص بك أمكن التعامل معه **بالطريقة نفسها** كالأخطاء المضمنة:

```lisp
(defstruct io-err (path string))
(impl Error io-err
  (message ((self Self)) string (append "io failed: " self::path))
  (source ((self Self)) Option<:dyn Error> (option::none)))

(defun open-it ((p string)) Result<i32, io-err>          ; يدخل النوع الملموس في E كما هو
  (if (equal p "") (result::err (io-err::new "<empty>")) (result::ok 3)))

(defun describe ((e :dyn Error)) string (message e))     ; تعامل مع أي نوع بالتساوي
(describe (io-err::new "/etc/app.conf"))
(describe (ParseIntError::ParseIntError "boom"))
```

ولجمع عدة أنواع أخطاء في `Result` واحدة استخدم `Result<T, :dyn Error>` (وهو يقابل `Box<dyn Error>` في
Rust)، ووسّع الأخطاء الملموسة بـ `as-dyn-error`. ولأنه لا يوجد `?` فإن هذا التحويل يُكتب صراحة:

```lisp
(defun run ((s string)) Result<i32, :dyn Error>
  (match (as-dyn-error (parse-int s))            ; ParseIntError -> :dyn Error
    ((ok n) (as-dyn-error (open-it (if (= n 0) "" "f"))))   ; io-err -> :dyn Error
    ((err e) (result::err e))))
```

**تشترك الأنواع والسمات في مساحة أسماء واحدة** (كما في Rust). ففي وحدة واحدة لا يمكن أن يكون لـ
`defstruct`/`defenum` وسمة الاسم نفسه، ويُبلَّغ عن كتابة اسم سمة في موضع نوع بالرسالة
"`error` is a trait, not a type — write `:dyn error`".

وتُعبَّر الإخفاقات غير القابلة للتعافي بـ `panic`. وبخصوص `panic` و`catch`/`throw` انظر
[مرجع الصياغة](../syntax.md#8-الخروج-غير-المحلي-catch--throw--unwind-protect)؛ وبخصوص سياسة معالجة
الأخطاء انظر [الفصل 9 منه](../syntax.md#9-سياسة-معالجة-الأخطاء).
