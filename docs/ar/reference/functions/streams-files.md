<!-- translated-from: docs/ja/reference/functions/streams-files.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# التدفقات والملفات

سمات التدفقات وتوابعها وأنواع التدفقات الملموسة وعمليات الملفات والمسارات. ومقابس الشبكة تدفقات أيضًا،
وتُعالَج في [الشبكات](network.md).

## 1. تسلسل السمات الهرمي

ما تعبّر عنه CL بتسلسل هرمي للأصناف يُعبَّر عنه هنا بـ **تسلسل هرمي للسمات**. فكل من الاتجاه (دخل / خرج)
ونوع العنصر يتحدد **ساكنًا**، فلا حاجة إلى السؤال وقت التشغيل "هل يمكن قراءة هذا التدفق؟".

```lisp
(deftrait Stream ()             (open-stream-p ...) (close ...))
(deftrait InputStream  (Stream) (type Item) (read-item ...))
(deftrait OutputStream (Stream) (type Item) (write-item ...))
(deftrait CharInput  ((InputStream  (Item char))) ...)   ; دخل محارف
(deftrait CharOutput ((OutputStream (Item char))) ...)   ; خرج محارف
(deftrait PeekInput  (CharInput) (unread-char ...) (peek-char ...))  ; دخل يمكنه إرجاع محرف واحد
(deftrait ByteInput  ((InputStream  (Item int))) ...)    ; دخل بايتات
(deftrait ByteOutput ((OutputStream (Item int))) ...)    ; خرج بايتات
```

الدالة التي تقرأ محارف تقبل أي نوع تدفق، مضمنًا كان أو معرَّفًا من المستخدم، إذا أخذت
`(where (CharInput S))` أو `:dyn CharInput`.

## 2. التوابع

لكل توابع `CharInput` تنفيذ افتراضي. ولا يكتب التنفيذ إلا `read-item`.

| الاسم | الصيغة | النوع | الوصف |
|---|---|---|---|
| `read-item` | `(read-item s)` | `(S)→Option<Item>` | العنصر التالي. `none` عند النهاية. **التابع الوحيد الذي يجب تنفيذه** |
| `read-char` | `(read-char s)` | `(S)→Option<char>` | المحرف التالي |
| `read-line` | `(read-line s)` | `(S)→Option<string>` | حتى السطر الجديد التالي (يُستهلك السطر الجديد ويُزال). ويُعاد أيضًا آخر سطر لا ينتهي بسطر جديد |
| `read-all` | `(read-all s)` | `(S)→string` | كل ما تبقى |
| `read-char-no-hang` | `(read-char-no-hang s)` | `(S)→Option<char>` | محرف موجود في المتناول فقط. `none` بدلًا من الانتظار |
| `read-sequence` | `(read-sequence s v n)` | `(S,Vector<char>,int)→int` | يدفع حتى `n` محرفًا إلى `v` ويعيد كم قُرئ فعلًا. أقل من `n` عند النهاية فقط |

`listen` في `InputStream` (أب `CharInput`):

| الاسم | الصيغة | النوع | الوصف |
|---|---|---|---|
| `listen` | `(listen s)` | `(S)→bool` | هل يمكن الإجابة عن القراءة التالية دون انتظار. والافتراضي `false`، **الجانب الذي لا يكذب أبدًا**: فـ `true` تخمين، وتخمين خاطئ سيجعل `read-char-no-hang` تحجب. وكل التدفقات المضمنة تتجاوزه. **وفي التدفقات المعرَّفة من المستخدم التي لا تتجاوزه تعيد `read-char-no-hang` دائمًا `none`** |

تضيف `PeekInput` (التي ترث من `CharInput`) **إرجاع محرف واحد**. فالتدفق نفسه وحده له مكان يحفظ فيه المحرف
المرجَع، فلا يمكن أن يكون لهذا تنفيذ افتراضي ولذلك هو سمة منفصلة. وتنفّذها `file-stream`/
`string-input-stream`/`standard-stream`، وأي تدفق آخر يحصل عليها عند تغليفه بـ `make-peek-stream`
(الفصل 4).

| الاسم | الصيغة | النوع | الوصف |
|---|---|---|---|
| `unread-char` | `(unread-char s c)` | `(S,char)→()` | تجعل القراءة التالية تعيد `c`. **التابع الوحيد الذي يجب تنفيذه**. وكما في CL يُضمن محرف واحد فقط |
| `peek-char` | `(peek-char s)` | `(S)→Option<char>` | تنظر إلى المحرف التالي دون استهلاكه |

وبالمثل لـ `CharOutput` لا يكتب التنفيذ إلا `write-item`.

| الاسم | الصيغة | النوع | الوصف |
|---|---|---|---|
| `write-item` | `(write-item s x)` | `(S,Item)→()` | يكتب عنصرًا واحدًا. **التابع الوحيد الذي يجب تنفيذه** |
| `write-char` | `(write-char s c)` | `(S,char)→()` | يكتب محرفًا واحدًا |
| `write-string` | `(write-string s str)` | `(S,string)→()` | يكتب سلسلة نصية |
| `write-line` | `(write-line s str)` | `(S,string)→()` | سلسلة وسطر جديد |
| `terpri` | `(terpri s)` | `(S)→()` | سطر جديد واحد (اسم CL) |
| `fresh-line` | `(fresh-line s)` | `(S)→()` | سطر جديد واحد ما لم نكن في بداية سطر |
| `at-line-start` | `(at-line-start s)` | `(S)→bool` | هل سيبدأ المحرف التالي المكتوب سطرًا. والافتراضي `false` (فتكتب `fresh-line` السطر الجديد: وعند الشك فالكتابة هي الجانب الآمن). وكل التدفقات المضمنة تتجاوزه |
| `finish-output` | `(finish-output s)` | `(S)→()` | يفرّغ المخزن المؤقت |
| `write-sequence` | `(write-sequence s v)` | `(S,Vector<char>)→()` | يكتب كل محارف `v` بالترتيب |

يتذكر `at-line-start` **ما كُتب عبر ذلك التدفق فقط**. و`print`/`println`/`(format true ...)` تكتب إلى الخرج
القياسي دون المرور بـ `*standard-output*`، فإذا خلطتَ بينهما فإن `(fresh-line *standard-output*)` لا تعرف
أسطر `println` الجديدة. فالتزم بأحدهما.

`Stream` مشتركة بين كل التدفقات:

| الاسم | الصيغة | النوع | الوصف |
|---|---|---|---|
| `open-stream-p` | `(open-stream-p s)` | `(S)→bool` | هل ما زال مفتوحًا |
| `close` | `(close s)` | `(S)→()` | يغلقه. **والـ GC لا يغلق التدفقات**، فافعل ذلك صراحة (أو بـ `with-open-file`) |

## 3. أنواع التدفقات الملموسة

| النوع | كيف تصنعه | السمات المنفَّذة |
|---|---|---|
| `file-stream` | `(open-file name direction)` / `open-input` / `open-output` | `CharInput` `PeekInput` `CharOutput` |
| `string-input-stream` | `(make-string-input-stream s)` | `CharInput` `PeekInput` |
| `string-output-stream` | `(make-string-output-stream)` | `CharOutput` |
| `standard-stream` | `*standard-input*` `*standard-output*` `*error-output*` | `CharInput` `PeekInput` `CharOutput` |
| `binary-file-stream` | `(open-binary name direction)` / `open-binary-input` / `open-binary-output` | `ByteInput` `ByteOutput` |

`direction` أحد الثوابت الثلاثة `direction-input` / `direction-output` / `direction-append`. وتعيد
`open-file` القيمة `Err(FileError)` إذا تعذّر فتح الملف (فغياب ملف نتيجة عادية لا `panic`). ويمكن أن يكون
اسم الملف سلسلة نصية أو `pathname` (`Pathish` في الفصل 9).

تعيد `(get-output-stream-string s)` ما كُتب إلى `string-output-stream` وتفرّغه. وكما في CL يمكن استخراجه
حتى بعد `close`.

**إدخال وإخراج البايتات** يستخدم `ByteInput`/`ByteOutput`. وهما تثبّتان `Item` في
`InputStream`/`OutputStream` على `int`، بالطريقة نفسها التي تثبّتها `CharInput`/`CharOutput` على `char`.

| الاسم | الصيغة | النوع | الوصف |
|---|---|---|---|
| `read-byte` | `(read-byte s)` | `(S)→Option<int>` حيث `ByteInput S` | البايت التالي. `none` عند نهاية الملف |
| `write-byte` | `(write-byte s b)` | `(S,int)→()` حيث `ByteOutput S` | يكتب بايتًا واحدًا. خطأ خارج 0..255 |
| `read-sequence` | `(read-sequence s v n)` | `(S,Vector<int>,int)→int` حيث `ByteInput S` | نسخة المحارف، بالبايتات |
| `write-sequence` | `(write-sequence s v)` | `(S,Vector<int>)→()` حيث `ByteOutput S` | مثل ما سبق |

تحدد CL نوع العنصر في **الاستدعاء**، كما في `(open name :element-type '(unsigned-byte 8))`، لكن نوع
العنصر هنا هو **نوع** التدفق، فما يختلف هو الدالة التي تفتحه. وقراءة بايتات من تدفق محارف خطأ في الأنواع
(فـ `string-input-stream` لا تنفّذ `ByteInput`). وقراءة بايت مباشرة بعد إرجاع محرف بـ `unread-char` خطأ
أيضًا.

## 4. التدفقات المركبة

كلها `defstruct` في المكتبة القياسية ويمكن تداخلها.

| الاسم | الصيغة | الوصف |
|---|---|---|
| `make-broadcast-stream` | `(make-broadcast-stream v)` | يكتب إلى كل عناصر `Vector<:dyn CharOutput>` |
| `make-two-way-stream` | `(make-two-way-stream in out)` | يقرأ من `in` ويكتب إلى `out` |
| `make-echo-stream` | `(make-echo-stream in out)` | يقرأ من `in` ويكتب المحارف المقروءة أيضًا إلى `out` |
| `make-concatenated-stream` | `(make-concatenated-stream v)` | يقرأ من `Vector<:dyn CharInput>` واحدًا بعد الآخر |
| `make-peek-stream` | `(make-peek-stream in)` | يضيف إرجاع محرف واحد إلى أي `:dyn CharInput` فيجعله `PeekInput` (لـ `read-sexpr`) |

## 5. الماكرو

| الاسم | الصيغة | الوصف |
|---|---|---|
| `with-open-file` | `(with-open-file (var name direction) body...)` | يفتح وينفّذ الجسم ويغلق. `Result<قيمة الجسم, FileError>` |
| `with-input-from-string` | `(with-input-from-string (var s) body...)` | يقرأ من سلسلة نصية |
| `with-output-to-string` | `(with-output-to-string (var) body...)` | يعيد ما كُتب |

## 6. الدوال العامة وعمليات الملفات

| الاسم | الصيغة | النوع | الوصف |
|---|---|---|---|
| `copy-stream` | `(copy-stream from to)` | `(I,O)→()` حيث `CharInput I`،`CharOutput O` | ينقل كل شيء |
| `read-lines` | `(read-lines s)` | `(S)→Vector<string>` حيث `CharInput S` | كل الأسطر المتبقية |
| `read-sexpr` | `(read-sexpr s)` | `(S)→Result<ReadOutcome,ReadError>` حيث `PeekInput S` | يقرأ `Sexpr` واحدًا (`read` في CL). `Ok(eof)` عند نهاية الدخل، و`Ok(datum d)` عند قراءة واحد، و`Err` إذا لم تكن بيانات. وهو **يستهلك المحرف الأبيض الواحد** الذي أنهى البيان (كما في CL). و`ReadOutcome` ليست `Option<Sexpr>` حتى لا تكون قراءة القائمة الفارغة `()` ونهاية الدخل القيمة نفسها |
| `read-sexpr-preserving-whitespace` | مثل ما سبق | مثل ما سبق | المثل، لكنه يترك المحرف الأبيض (`read-preserving-whitespace` في CL) |
| `read-delimited-list` | `(read-delimited-list ch s)` | `(char,S)→Result<Option<Sexpr>,ReadError>` حيث `PeekInput S` | يقرأ حتى `ch` ويصنع قائمة. ويُستهلك `ch`. و`Err` إذا نفد الدخل |
| `write-lines` | `(write-lines s lines)` | `(S,I)→()` حيث `CharOutput S`،`Iter I (Item string)` | يكتب سطرًا في كل مرة |
| `read-file-string` | `(read-file-string name)` | `(P)→Result<string,FileError>` حيث `Pathish P` | المحتويات كلها |
| `read-file-lines` | `(read-file-lines name)` | `(P)→Result<Vector<string>,FileError>` حيث `Pathish P` | كل الأسطر |
| `write-file-string` | `(write-file-string name text)` | `(P,string)→Result<(),FileError>` حيث `Pathish P` | يكتبه |
| `probe-file` | `(probe-file name)` | `(P)→bool` حيث `Pathish P` | هل هو موجود |
| `delete-file` / `rename-file` | | `→Result<(),FileError>` | حذف وإعادة تسمية (الوسائط `Pathish`) |
| `truename` | `(truename name)` | `(P)→Result<string,FileError>` حيث `Pathish P` | المسار المطلق مع حل الروابط الرمزية و`.`/`..`. و`Err` إذا لم يكن موجودًا |
| `file-write-date` | `(file-write-date name)` | `(P)→Result<universal-time,FileError>` حيث `Pathish P` | وقت آخر تعديل. وهو **زمن عالمي (universal time)**، فيمكن لـ `decode-universal-time` ([الزمن](system.md#2-فك-ترميز-التواريخ-وترميزها)) قراءته |
| `file-author` | `(file-author name)` | `(P)→Result<Option<string>,FileError>` حيث `Pathish P` | اسم دخول المالك. و`Err` إذا لم يكن الملف موجودًا، و`Ok(none)` إذا لم يكن لمعرّف المستخدم (uid) الخاص بالمالك إدخال في قاعدة بيانات كلمات المرور: فالحالتان اللتان تميز بينهما CL تبقيان منفصلتين |
| `directory-p` | `(directory-p name)` | `(P)→bool` حيث `Pathish P` | هل هو مجلد. **وهي `false` أيضًا إذا لم يكن موجودًا**؛ استخدم `probe-file` للتمييز بين الحالتين |
| `directory` | `(directory name)` | `(P)→Result<Vector<string>,FileError>` حيث `Pathish P` | يسرد المحتويات بـ truename (المسار المطلق مع حل الروابط الرمزية، كما في `truename`). وتُستبعد الروابط الرمزية التي هدفها مفقود. وتُستبعد `.`/`..`. والترتيب ما يعطيه نظام التشغيل |
| `ensure-directories-exist` | `(ensure-directories-exist name)` | `(P)→Result<(),FileError>` حيث `Pathish P` | ينشئه مع المجلدات الأم. وينجح إذا كان موجودًا |

كل وسيط يسمّي ملفًا **يمكن أن يكون سلسلة نصية أو `pathname`**. وهذه المعاملة نفسها لمحدِّدات المسار
(pathname designators) في CL، وتُحَلّ عبر السمة `Pathish` لا باختبار نوع وقت التشغيل (الفصل 9).

والحرف المنهي في `read-delimited-list` **ينهي الرموز (tokens) أيضًا**. ولا يسري إلا عند العمق 0: ففي
`(1 2]` يُقرأ `]` على أنه جزء من نص القائمة نفسها ويُبلَّغ عن قائمة مكسورة. ولا يوجد مقابل للوسيط الثالث
`recursive-p` في CL.

## 7. جعل نوعك الخاص تدفقًا

اكتب `write-item` واحدة وستجلب التنفيذات الافتراضية الباقي. ويمكنه أيضًا الدخول في التدفقات المركبة.

```lisp
(defstruct counter (n i32))
(impl Stream counter
  (open-stream-p ((self Self)) bool true)
  (close ((self Self)) () ()))
(impl OutputStream counter
  (type Item char)
  (write-item ((self Self) (c char)) () (setf self::n (+ self::n 1))))
(impl CharOutput counter)              ; كل التوابع المتبقية افتراضية

(write-line (counter::new 0) "four")   ; تعمل write-line وterpri وfresh-line كلها
```

والدخل يعمل بالطريقة نفسها: تكتب `read-item` فقط. وحتى النوع الذي ليس له إرجاع خاص به يمكن `read`
منه بعد تغليفه، كما في `(read-sexpr (make-peek-stream my-stream))`.

## 8. readtable

| الاسم | الاستدعاء | النوع | الوصف |
|---|---|---|---|
| `set-macro-character` | `(set-macro-character c f)` | `(char, F)→()` | تقرأ `f` المحرف `c` |
| `get-macro-character` | `(get-macro-character c)` | `(char)→Option<F>` | تعيد ما سُجِّل |
| `set-dispatch-macro-character` | `(set-dispatch-macro-character d s f)` | `(char,char,F)→()` | تقرأ `f` تسلسل المحرفين `d s` |
| `get-dispatch-macro-character` | `(get-dispatch-macro-character d s)` | `(char,char)→Option<F>` | مثل ما سبق |

`F` هي `(fn (string-input-stream char) Option<Sexpr>)`. وطريقة استخدامها ومتى تسري وكيف تختلف عن CL في
[مرجع الصياغة](../syntax.md#11-ماكرو-القارئ-readtable).

## 9. المسارات `pathname`

اسم ملف مقسَّم إلى أجزاء. ويحمل مكونات المجلد المفصولة بـ `/` والاسم والنوع (الامتداد) وهل يبدأ من الجذر.

```lisp
(let ((p (parse-namestring "/var/log/app.tar.gz")))
  (pathname-directory p)   ; => #<vector<string> "var" "log">
  (pathname-name p)        ; => (some "app.tar")   يُقسَّم عند آخر نقطة
  (pathname-type p)        ; => (some "gz")
  (namestring p))          ; => "/var/log/app.tar.gz"

(namestring (merge-pathnames (make-pathname :name "today" :type "log")
                             "/var/log/"))        ; => "/var/log/today.log"
```

### 9.1 سمة محدد المسار `Pathish`

حيثما تقبل CL محدِّد مسار (سلسلة نصية أو مسارًا) تقبل هذه اللغة `Pathish`. وكل من `string` و`pathname`
ينفّذها، و**كل عمليات الملفات تأخذها بصورة عامة**، فـ `(open-input "a.txt")` و`(open-input p)` كلتاهما
استدعاء عادي (ولا اختبار نوع وقت التشغيل). و`namestring` لسلسلة نصية تعيدها كما هي، فما دمتَ تمرّر سلسلة
نصية فلا يحدث أي تحليل.

| الاسم | الصيغة | النوع | الوصف |
|---|---|---|---|
| `namestring` | `(namestring p)` | `(P)→string` | الصورة النصية. يجب تنفيذه |
| `to-pathname` | `(to-pathname p)` | `(P)→pathname` | يحوّل إلى `pathname` (دالة `pathname` في CL، أُعيدت تسميتها لأنها ستتعارض مع اسم النوع). يجب تنفيذه |

### 9.2 الدوال

| الاسم | الصيغة | النوع | الوصف |
|---|---|---|---|
| `parse-namestring` | `(parse-namestring s)` | `string→pathname` | يقسّم سلسلة نصية إلى أجزاء. وانتهاء بـ `/` (أو اسم فارغ) يعني "لا اسم"، أي مجلد |
| `make-pathname` | `(make-pathname :directory v :name s :type s :absolute b)` | `→pathname` | يبني واحدًا من المكونات المعطاة فقط (كلها `&key`). والاسم أو النوع المحذوف يبقى "غائبًا" وهو ما تملؤه `merge-pathnames` |
| `pathname-directory` | `(pathname-directory p)` | `(P)→Vector<string>` | مكونات المجلد، الأبعد أولًا |
| `pathname-name` | `(pathname-name p)` | `(P)→Option<string>` | الاسم بدون النوع. `none` لمجلد |
| `pathname-type` | `(pathname-type p)` | `(P)→Option<string>` | ما بعد آخر نقطة. والنقطة في البداية لا تُحتسب (فكل `.gitignore` هو الاسم) |
| `pathname-absolute-p` | `(pathname-absolute-p p)` | `(P)→bool` | هل يبدأ من الجذر |
| `user-homedir-pathname` | `(user-homedir-pathname)` | `()→Option<pathname>` | مجلد المنزل. `none` إذا لم يوجد `$HOME` (وCL تسمح بـ `NIL` أيضًا) |
| `directory-namestring` | `(directory-namestring p)` | `(P)→string` | الجزء حتى آخر `/` |
| `file-namestring` | `(file-namestring p)` | `(P)→string` | جزء `name.type` فقط |
| `merge-pathnames` | `(merge-pathnames p default)` | `(P,D)→pathname` | يملأ المكونات الناقصة من `p` من `default`. و`p` النسبي يوضع تحت مجلد `default`؛ و`p` المطلق يحتفظ بمجلده |
| `enough-namestring` | `(enough-namestring p default)` | `(P,D)→string` | الصورة النسبية إلى `default`. وكل `p` إذا لم يكن تحت الأساس |

وكل وسائط الأنواع تحمل `(where (Pathish P))`.

## 10. الفروق عن CL

- **تسلسل هرمي للسمات لا للأصناف.** لا يوجد `input-stream-p` / `output-stream-p`: فالنوع يحمل الاتجاه،
  فليس سؤالًا يُطرح وقت التشغيل.
- **لـ `read` أسماء مختلفة لنسختي السلسلة النصية والتدفق.** `(read "...")` (تقابل القيمة الأولى من
  `read-from-string` في CL؛ وإذا احتجت أيضًا إلى الموضع الذي انتهت عنده القراءة فاستخدم `read-from-string`)
  و`(read-sexpr s)` (`read` في CL). فالاستدعاء يُحَلّ إلى نوع مستقبِل واحد، فلا يمكن تحميل الاسم نفسه أكثر
  من معنى.
- **الإرجاع سمة منفصلة** (`PeekInput`)، فلا تُجبر الأنواع التي لا تحتاج إلا إلى `read-char` على تنفيذ
  `unread-char`.
- **الإغلاق صريح.** فالـ GC لا يغلق التدفقات (والـ GC يعمل في أوقات لا يمكن التنبؤ بها، فتركه للـ GC
  سيجعل لحظة الإغلاق غير متوقعة أيضًا). واستخدام `with-open-file` هو الطريقة الآمنة.
- **المسارات بلا مكونات المضيف والجهاز والإصدار.** ولا مسارات بأحرف بدل ولا مسارات منطقية
  (`logical-pathname`). والفاصل `/` دائمًا.
- **دالة `pathname` هي `to-pathname`**، لأن الأنواع والسمات والدوال تشترك في مساحة أسماء واحدة.
- **لا توجد مطابقة بأحرف البدل**، فـ `directory` دالة "تسرد محتويات ذلك المجلد" ولا شيء أكثر. أما
  `directory` في CL فتطابق نمط مسار.
