<!-- translated-from: docs/ja/tutorial/errors.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# معالجة الأخطاء

تقسم معالجة الأخطاء في typelisp الإخفاقات إلى نوعين.

| نوع الإخفاق | أمثلة | كيف يُعبَّر عنه |
|---|---|---|
| إخفاقات يمكن أن تحدث (قابلة للتعافي) | ملف غير موجود، مُدخَل ليس عددًا | إعادة `Result<T,E>` |
| أخطاء في البرنامج (غير قابلة للتعافي) | فهرس خارج النطاق، `unwrap` على `none`، قسمة على صفر | التوقف بـ `panic` |

وفوق هذين توجد `catch` / `throw` اللتان تغادران استدعاءات دوال كثيرة دفعة واحدة، و`unwind-protect` التي
تنفّذ التنظيف مهما كانت طريقة مغادرة جسمها. يفترض هذا الفصل أنك قرأت قسم `Result` في
[أساسيات الأنواع](types.md).

## 1. أعد `Result` واستلمها بـ `match`

هذه دالة تقرأ رقم منفذ من سلسلة نصية. ويمكن أن تفشل بطريقتين: المُدخَل ليس عددًا، أو هو خارج النطاق.

```lisp
(defun parse-port ((s string)) Result<int,string>
  (match (parse-int s)
    ((ok n) (if (and (>= n 1) (<= n 65535))
                (Result::ok n)
                (Result::err (format false "port out of range: ~a" n))))
    ((err e) (Result::err (message e)))))
```

يفصل المستدعي بين النجاح والفشل بـ `match`.

```lisp
(match (parse-port "99999")
  ((ok p) (println "port ~a" p))
  ((err m) (println "error: ~a" m)))
;; error: port out of range: 99999
```

- لا يمكن استخدام قيمة دالة تعيد `Result` ما لم يعالج `match` حالة `err`. ونسيان معالجة الفشل خطأ في
  الأنواع.
- الخطأ الصادر من `parse-int` قيمة من النوع `ParseIntError`. و`(message e)` تعطي سلسلة رسالته.

## 2. تمرير الإخفاق إلى المستدعي

لا يوجد اختصار مثل `?` في Rust. وعند استدعاء عدة دوال تعيد `Result` واحدة بعد الأخرى اكتب جزء
"أعد الإخفاق كما هو" بـ `match`.

```lisp
(defun add-ports ((a string) (b string)) Result<int,string>
  (match (parse-port a)
    ((err m) (Result::err m))
    ((ok x) (match (parse-port b)
              ((err m) (Result::err m))
              ((ok y) (Result::ok (+ x y)))))))

(add-ports "1" "2")       ; => (ok 3)
(add-ports "1" "x")       ; => (err "parse-int: invalid integer literal: \"x\"")
```

وعندما تعلم أن عملية لا يمكن أن تفشل، أو في سكربت صغير لا بأس فيه بالتوقف عند الفشل، يستخرج `unwrap`
المحتوى. وإذا كانت القيمة `err` فإنه يحدث `panic`. وإذا كانت قيمة افتراضية تكفي فاستخدم `unwrap-or`.

## 3. صنع نوع خطأ خاص بك

التعبير عن الأخطاء بنوع بدلًا من سلسلة نصية يتيح للمستدعي أن يتفرع بحسب نوع الخطأ. ونوع الخطأ هو
`defenum` أو `defstruct` عادي ينفّذ السمة `Error`.

```lisp
(defenum config-error
  (missing string)          ; إعداد مفقود
  (invalid string int))     ; قيمة خاطئة

(impl Error config-error
  (message ((self Self)) string
    (match self
      ((missing key) (format false "missing key: ~a" key))
      ((invalid key v) (format false "invalid value for ~a: ~a" key v))))
  (source ((self Self)) Option<:dyn Error> (Option::none)))

(defun check-workers ((n int)) Result<int,config-error>
  (if (> n 0)
      (Result::ok n)
      (Result::err (config-error::invalid "workers" n))))
```

- تعيد `message` وصفًا للخطأ.
- تعيد `source` خطأً آخر تسبب في هذا الخطأ. ومع عدم وجود سبب تكون `none`.

## 4. الجمع بين أنواع مختلفة من الأخطاء

إذا استدعت دالة واحدة كلًا من `parse-int` (`ParseIntError`) و`check-workers` (`config-error`) فهناك
نوعا خطأ، ولا يمكن أن يكونا معًا `E` في `Result<T,E>` واحدة. وفي تلك الحالة اجعل `E` هو `:dyn Error`
(خطأ من أي نوع ينفّذ `Error`). وحوّل كل خطأ بـ `as-dyn-error`.

```lisp
(defun load-count ((s string)) Result<int, :dyn Error>
  (match (as-dyn-error (parse-int s))
    ((ok n) (as-dyn-error (check-workers n)))
    ((err e) (Result::err e))))
```

وعند إعطاء `"4"` و`"-1"` و`"abc"` تكون النتائج:

```
ok 4
err invalid value for workers: -1
err parse-int: invalid integer literal: "abc"
```

وبخصوص `:dyn` انظر القسم 5 من [السمات](traits.md).

## 5. `panic`: أخطاء في البرنامج

عندما يصل البرنامج إلى حالة يجب ألا تحدث أبدًا أوقفه بـ `panic`.

```lisp
(defun safe-get ((v Vector<int>) (i int)) int
  (if (< i (len v))
      (get v i)
      (panic (format false "index ~a out of range" i))))
```

```
error: main.typl:4:7: panic: index 3 out of range
```

- نوع `panic` هو `!` (لا تعود)، فيمكن كتابتها حيثما يُتوقَّع أي نوع. ولهذا يتلاءم فرعا `if` أعلاه.
- تُحدث هذه العمليات `panic` أيضًا: `unwrap` على `none` أو `err`، و`get` بفهرس خارج النطاق، والقسمة
  الصحيحة على صفر.
- يوقف `panic` البرنامج. وحتى عندما يحدث داخل مهمة (task) يتوقف البرنامج كله.
- في الـ REPL لا ينهي `panic` الـ REPL؛ بل ينتظر المُدخَل التالي.
- يمكنك كتابة `(todo)` بمعنى "لم يُكتب بعد" و`(unreachable)` بمعنى "لا ينبغي بلوغ هذه النقطة أبدًا".
  وكلتاهما تحدث `panic`.

لا يحل `panic` محل `Result`. ففي الإخفاقات التي يمكن أن تحدث، مثل مُدخَلات المستخدم أو وجود ملف، استخدم
`Result`.

## 6. `catch` / `throw`: القفز عبر الدوال

يقفز `throw` مباشرة إلى `catch` المحيطة التي لها الوسم نفسه، مهما كان عدد استدعاءات الدوال بينهما.

```lisp
(defun check-all ((v Vector<int>)) ()
  (doiter (x (iter v))
    (if (< x 0)
        (throw 'bad-input (format false "negative: ~a" x))
        ())))

(defun validate ((v Vector<int>)) string
  (catch 'bad-input
    (progn
      (check-all v)
      "all fine")))
```

إذا لم يكن في `v` عدد سالب أعادت `validate` القيمة `"all fine"`؛ وإذا احتوت `-7` قفز التحكم من داخل
`check-all` إلى `catch` التي تعيد `"negative: -7"`.

- اكتب الوسم رمزًا عاديًا مثل `'bad-input`.
- **يحمل كل وسم قيمًا من نوع واحد فقط.** في المثال أعلاه يحمل `'bad-input` قيمة `string`، فرمي `int`
  بالوسم نفسه خطأ في الأنواع. ويجب أن يطابق نوع جسم `catch` نوع الوسم أيضًا.

  ```
  error: ...: type error: type mismatch: expected `string`, found `int`
  ```

- `throw` الذي لا توجد `catch` بالوسم نفسه يمكن بلوغها خطأ.

وإذا أردت فقط العودة المبكرة من داخل دالة فاستخدم `return-from` بدلًا من `catch` / `throw`. لا تستطيع
`return-from` عبور الدوال، لكن مقابل ذلك يمكنك معرفة الموضع الذي تعود إليه بقراءة المصدر.

```lisp
(defun first-negative ((v Vector<int>)) Option<int>
  (doiter (x (iter v))
    (if (< x 0) (return-from first-negative (Option::some x)) ()))
  (Option::none))
```

## 7. `unwind-protect`: نظّف دائمًا

يشغّل `(unwind-protect body cleanup)` التنظيف مهما كانت طريقة مغادرة الجسم: عند الانتهاء عاديًا، وعند
المغادرة بـ `throw`، وعند حدوث `panic`.

```lisp
(defun with-cleanup ((n int)) int
  (unwind-protect
    (progn
      (println "working on ~a" n)
      (if (< n 0) (throw 'bad-input "negative") ())
      (* n 2))
    (println "cleanup for ~a" n)))
```

```lisp
(println "~a" (with-cleanup 5))
;; working on 5
;; cleanup for 5
;; 10

(println "~a" (catch 'bad-input (progn (with-cleanup -1) "unreached")))
;; working on -1
;; cleanup for -1
;; negative
```

استخدمها في أمور مثل "أغلق دائمًا ملفًا فتحته" أو "حرّر دائمًا قفلًا أخذته". وتستخدم `with-open-file`
و`with-lock` في المكتبة القياسية `unwind-protect` داخليًا.

## 8. حول نظام الشروط في Common Lisp

لا تعتمد typelisp نظام الشروط في Common Lisp (`handler-case` و`restart-case` وغيرهما). فهو لا يُظهر في
الأنواع أي إخفاقات قد تسببها الدالة، وهذا لا يتلاءم جيدًا مع الأنواع الساكنة. فالإخفاقات التي يمكن أن
تحدث تُكتب في الأنواع بـ `Result`، وتتم عمليات نقل التحكم بـ `catch` / `throw`.

## 9. ماذا تقرأ بعد ذلك

- [التزامن](concurrency.md): المهام والقنوات
- [Option وResult وأنواع الأخطاء](../reference/functions/option-result.md): قائمة الدوال
- [رسائل الخطأ](../reference/errors.md): معنى الأخطاء الشائعة وكيفية إصلاحها
