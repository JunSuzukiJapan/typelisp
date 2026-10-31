<!-- translated-from: docs/ja/README.md @ 25173243b2eff6e1cad20c77d2fc8be7e35e0124 -->
# توثيق typelisp (العربية)

typelisp هي لغة Lisp ذات أنواع ساكنة. للاطلاع على طريقة التثبيت والبناء راجع
[README.md](../../README.md) (بالإنجليزية) في جذر المستودع.

## الميزات

### المهام والقنوات

تُشغَّل المهام (خيوط خفيفة) وتتبادل القيم فيما بينها عبر القنوات. للتفاصيل راجع [التزامن](tutorial/concurrency.md).

```lisp
(use std::sync::Chan)
(defun worker ((name string) (jobs Chan<int>) (results Chan<string>)) ()
  (doiter (n jobs)
    (sleep 0.1)
    (send results (format false "~a: ~d * ~d = ~d" name n n (* n n)))))

(defun main () ()
  (let ((jobs (the Chan<int> (Chan::new 0)))
        (results (the Chan<string> (Chan::new 10))))
    (task (worker "A" jobs results))
    (task (worker "B" jobs results))
    (dotimes (i 4)
      (send jobs (+ i 1)))
    (close jobs)
    (dotimes (i 4)
      (println "~a" (unwrap (recv results))))))

(main)
```

### الأنواع العامة

تُكتب معاملات الأنواع بين قوسين زاويين بعد اسم النوع أو الدالة، وتُكتب قيود السمات في عبارة `:where`. للتفاصيل راجع [أساسيات الأنواع](tutorial/types.md) و[السمات (Traits)](tutorial/traits.md).

```lisp
(defstruct Pair<A,B> (first A) (second B))

(impl Pair<A,B>
  (swap (self) Pair<B,A>
    (make-instance 'Pair :first self::second :second self::first)))

(defun largest<T> ((xs Vector<T>)) Option<T> (:where (Ord T))
  (let ((best (the Option<T> (Option::none))))
    (doiter (x (iter xs))
      (setf best (Option::some (match best
                                 ((some b) (if (> x b) x b))
                                 ((none) x)))))
    best))

(let ((p (swap (make-instance 'Pair :first 1 :second "one"))))
  (println "~a ~a" p::first p::second))                       ; one 1

(println "~a" (largest (the Vector<int> #(3 9 4))))           ; (some 9)
(println "~a" (largest (the Vector<string> #("pear" "fig")))) ; (some pear)
```

### Option و Result بدلًا من nil

لا يوجد `nil`. الدالة التي قد لا تكون لها قيمة تُعيد `Option<T>`، والدالة التي قد تفشل تُعيد `Result<T,E>`، ويُستخرج المحتوى باستخدام `match`. استخدام `Option<string>` على أنه `string`، أو إغفال حالة `none` في `match`، خطأ أنواع يظهر قبل تشغيل البرنامج. للتفاصيل راجع [أساسيات الأنواع](tutorial/types.md) و[معالجة الأخطاء](tutorial/errors.md).

```lisp
(use std::num::parse-int)
(defun find-user ((id int)) Option<string>
  (case id
    (1 (Option::some "alice"))
    (2 (Option::some "bob"))
    (:else (Option::none))))

(defun parse-port ((s string)) Result<int,string>
  (match (parse-int s)
    ((ok n) (if (<= 1 n 65535)
                (Result::ok n)
                (Result::err (format false "port out of range: ~a" n))))
    ((err e) (Result::err (message e)))))

(match (find-user 3)
  ((some name) (println "user: ~a" name))
  ((none) (println "not found")))        ; not found

(match (parse-port "99999")
  ((ok p) (println "port ~a" p))
  ((err m) (println "error: ~a" m)))     ; error: port out of range: 99999
```

### ربط الخطوات باستخدام `->`

يمرّر الماكرو `->` قيمةً، بالتتابع، بوصفها الوسيط الأول لكل صيغة تليها. فـ `(-> x (f a) (g b))` يساوي `(g (f x a) b)`. وبذلك تُكتب الخطوات التي تُعيد `Option` أو `Result` من الأعلى إلى الأسفل. للتفاصيل راجع [Option وResult وأنواع الأخطاء](reference/functions/option-result.md).

```lisp
(use std::num::parse-int)
(defun half ((n int)) Option<int>
  (if (= 0 (mod n 2))
      (Option::some (truncate n 2))
      (Option::none)))

(println "~a"
  (-> (Option::some 40)
      (and-then half)                          ; (some 20)
      (and-then half)                          ; (some 10)
      (map (lambda ((x int)) int (+ x 1)))     ; (some 11)
      (unwrap-or 0)))                          ; 11

(println "~a"
  (-> (parse-int "abc")
      (map (lambda ((n int)) int (* n 2)))
      (unwrap-or -1)))                         ; -1
```

### ماكرو يخضع لفحص الأنواع

تُكتب الماكرو باستخدام `defmacro` والاقتباس الجزئي (quasiquote) و`gensym` كما في Common Lisp. ويُفحص الكود الناتج عن التوسيع كما يُفحص الكود المكتوب يدويًا، وإذا لم يتوافق مع الأنواع ظهر الخطأ في موضع استدعاء الماكرو. للتفاصيل راجع [الماكرو (Macros)](tutorial/macros.md).

```lisp
(defmacro swap (a b)
  (let ((tmp (gensym "tmp")))
    `(let ((,tmp ,a))
       (setf ,a ,b)
       (setf ,b ,tmp))))

(let ((x 1) (y 2))
  (swap x y)
  (println "x=~a y=~a" x y))             ; x=2 y=1
```

```lisp
(defmacro add-a (x) `(+ ,x "a"))
(add-a 1)
;; error: type error: type mismatch: expected `int`, found `string`
```

## الدروس التعليمية

إذا كنت جديدًا على typelisp فاقرأ هذه الدروس بالترتيب.

- [البدء السريع](tutorial/intro.md): الـ REPL والدوال والمتغيرات والشروط والحلقات والقوائم و`Vector`
- [أساسيات الأنواع](tutorial/types.md): الأنواع الساكنة و`Option` و`Result` والبنى والتعدادات والأنواع العامة
- [السمات (Traits)](tutorial/traits.md): `deftrait` / `impl` وقيود السمات و`:dyn`
- [الماكرو (Macros)](tutorial/macros.md): `defmacro` والاقتباس الجزئي (quasiquote) و`gensym` و`macrolet`
- [معالجة الأخطاء](tutorial/errors.md): `Result` و`panic` و`catch` / `throw` و`unwind-protect`
- [التزامن](tutorial/concurrency.md): المهام والقنوات و`select` و`Mutex` و`thread`

## الأدلة

- [الوحدات وتنظيم الملفات](guide/modules.md): `use` و`pub` وكيفية ربط الملفات بالوحدات
- [الترجمة](guide/compile.md): الـ JIT وبناء الملفات التنفيذية بالترجمة المسبقة AOT وملفات الـ dump
- [إدخال وإخراج الملفات والتدفقات والشبكات](guide/io.md): الملفات والمسارات وTCP / TLS / UDP وحل الأسماء
- [واجهة C الخارجية (FFI)](guide/ffi.md): استدعاء دوال C باستخدام `defffi` (بما في ذلك دوال الاستدعاء الراجع وبنى C عبر `def-c-struct`)
- [التكامل مع المحررات](guide/editors.md): `typl-lsp` وإعداد VS Code / Emacs
- [لمبرمجي Common Lisp](guide/from-common-lisp.md): الفروق بين typelisp وCL وكيفية إعادة كتابة شيفرة CL

## المرجع

- [مرجع الصياغة](reference/syntax.md): الصياغة المعجمية وكتابة الأنواع والتعريفات وصيغ التحكم والترجمة والتزامن
- [الدوال المضمنة](reference/functions/README.md): الدوال المضمنة والتوابع (methods) والمكتبة القياسية
- [الأنواع](reference/types.md): الأنواع والسمات التي ينفذها كل نوع
- [رسائل الخطأ](reference/errors.md): معنى الأخطاء الشائعة وكيفية إصلاحها

## التكامل مع المحررات

تعليمات الإعداد موجودة في [دليل التكامل مع المحررات](guide/editors.md). أما اختصارات المفاتيح
وإعدادات كل محرر فهي مدرجة في هذه المستندات:

- [Emacs (typelisp-mode)](../../editor/emacs/README_ar.md)
- [VS Code](../../editor/vscode/README_ar.md)
