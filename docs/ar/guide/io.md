<!-- translated-from: docs/ja/guide/io.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# إدخال وإخراج الملفات والتدفقات والشبكات

يعرض هذا الدليل أساسيات قراءة الملفات وكتابتها والمسارات (pathnames) والاتصال عبر المقابس (sockets).
وقوائم الدوال موجودة في [التدفقات والملفات](../reference/functions/streams-files.md) و
[الشبكات](../reference/functions/network.md).

## 1. تعود الإخفاقات على صورة `Result`

العمليات التي قد تفشل بحسب البيئة، مثل فتح ملف أو الاتصال، تعيد `Result`. فغياب ملف ليس خطأ في البرنامج،
ولذلك لا يحدث `panic`. افصل بين النتائج بـ `match`.

```lisp
(match (read-file-string "config.txt")
  ((ok text) (println "~a" text))
  ((err e) (println "error: ~a" (message e))))
;; إذا كان الملف غير موجود:
;; error: config.txt: No such file or directory (os error 2)
```

وعندما تعلم أن عملية لا يمكن أن تفشل، أو في سكربت صغير لا بأس فيه بالتوقف عند الفشل، يستخرج `unwrap`
القيمة. وإذا كانت `Err` فإنه يحدث `panic`.

## 2. قراءة ملف كامل وكتابته

أسهل الدوال تتعامل مع الملف كله دفعة واحدة.

```lisp
(unwrap (write-file-string "copy.txt" "abc\n"))
(read-file-string "copy.txt")      ; => (ok "abc\n")
(read-file-lines "copy.txt")       ; => (ok #<vector<string> "abc">)
```

## 3. القراءة والكتابة بالتدفقات

لقراءة أو كتابة قليل في كل مرة افتح تدفقًا (stream) بـ `with-open-file`. ومهما كانت طريقة مغادرة
الجسم يُغلق التدفق. والقيمة هي `Result<قيمة الجسم, FileError>`.

```lisp
;; الكتابة
(match (with-open-file (out "notes.txt" direction-output)
         (write-line out "first")
         (write-line out "second")
         (format out "total: ~a~%" 2))
  ((ok _) ())
  ((err e) (println "write failed: ~a" (message e))))

;; قراءة سطر واحد في كل مرة
(match (with-open-file (in "notes.txt" direction-input)
         (let ((n 0))
           (loop (match (read-line in)
                   ((some line) (progn (setf n (+ n 1)) (println "~a: ~a" n line)))
                   ((none) (break))))
           n))
  ((ok n) (println "~a lines" n))
  ((err e) (println "read failed: ~a" (message e))))
```

- لفتح ملف ثلاثة اتجاهات: `direction-input` (قراءة) و`direction-output` (كتابة؛ يُتخلَّص من المحتويات
  الموجودة) و`direction-append` (الإلحاق في النهاية).
- تعيد `read-line` القيمة `none` عند نهاية الملف.
- إذا فتحت ملفًا بـ `open-file` بدلًا من `with-open-file` فاستدعِ `close` دائمًا. فالـ GC لا يغلق
  التدفقات.

ولقراءة البايتات وكتابتها افتح الملف بـ `open-binary-input` / `open-binary-output` واستخدم
`read-byte` / `write-byte`. تدفقات المحارف وتدفقات البايتات نوعان مختلفان، فمحاولة قراءة بايتات من تدفق
محارف خطأ في الأنواع.

### استخدام سلسلة نصية كتدفق

```lisp
(with-output-to-string (s)
  (write-string s "a")
  (write-string s "b"))                  ; => "ab"

(with-input-from-string (s "line1\nline2")
  (read-line s))                         ; => (some "line1")
```

### الدخل والخرج القياسيان

`*standard-input*` و`*standard-output*` و`*error-output*` تدفقات أيضًا.
`(read-line *standard-input*)` تقرأ سطرًا واحدًا.

### جعل دوال القراءة والكتابة عامة

كل نوع من التدفقات نوع قائم بذاته، لكن العمليات المشتركة مجمعة في سمات (traits). والدالة التي تأخذ
وسيطها بـ `(where (CharInput S))` يمكنها القراءة من ملف أو سلسلة نصية أو مقبس.

```lisp
(defun count-lines<S> ((s S)) int (where (CharInput S))
  (let ((n 0))
    (loop (match (read-line s)
            ((some _) (setf n (+ n 1)))
            ((none) (break))))
    n))
```

## 4. المسارات (Pathnames)

الدوال التي تأخذ اسم ملف تقبل سلسلة نصية أو `pathname`. استخدم `pathname` لتقسيم مسار إلى أجزاء أو لبنائه.

```lisp
(let ((p (parse-namestring "/var/log/app.tar.gz")))
  (pathname-directory p)   ; => #<vector<string> "var" "log">
  (pathname-name p)        ; => (some "app.tar")
  (pathname-type p))       ; => (some "gz")

;; استبدال الامتداد فقط
(namestring (merge-pathnames (make-pathname :type "json") (to-pathname "data/in.csv")))
;; => "data/in.json"

;; وضع اسم ملف داخل مجلد
(namestring (merge-pathnames (make-pathname :name "today" :type "log") "/var/log/"))
;; => "/var/log/today.log"
```

عمليات نظام الملفات:

```lisp
(unwrap (ensure-directories-exist "out/deep/"))   ; تُنشأ مع المجلدات الأم
(probe-file "out/deep")                           ; => true (موجود)
(directory-p "out/deep")                          ; => true
(directory "out")                                 ; => قائمة المحتويات
(delete-file "copy.txt")
(rename-file "a.txt" "b.txt")
```

الفاصل هو `/` دائمًا. ولا توجد مكونات المضيف والجهاز والإصدار كما في مسارات Common Lisp، ولا توجد أحرف
بدل (wildcards) ولا مسارات منطقية.

## 5. TCP

اتصال المقبس تدفق أيضًا، فتعمل عليه `read-line` و`write-line` كما هما.

### الخادم

النمط الأساسي هو بدء مهمة (task) واحدة لكل اتصال. وتوقف `accept` و`read-line` **تلك المهمة وحدها** حتى
تصل بيانات، فيستمر التعامل مع الاتصالات الأخرى.

```lisp
(defun serve ((c socket-stream)) ()
  (loop (match (read-line c)
          ((some line) (write-line c (upcase line)))
          ((none) (break))))                  ; أغلق الطرف الآخر
  (close c))

(let ((l (unwrap (tcp-listen "127.0.0.1" 7777))))
  (loop (match (accept l)
          ((ok c) (progn (task (serve c)) ()))
          ((err e) (println "accept: ~a" (message e))))))
```

`"127.0.0.1"` تقبل الاتصالات من هذا الجهاز وحده، و`"0.0.0.0"` تقبلها على كل واجهة. وللمهام انظر
[الفصل 12 من مرجع الصياغة](../reference/syntax.md#12-التزامن-المهام).

### العميل

يتصل `with-connection` ثم ينفّذ جسمه ويغلق الاتصال في النهاية. والقيمة هي
`Result<قيمة الجسم, NetError>`.

```lisp
(match (with-connection (c "localhost" 7777)
         (write-line c "hello")
         (unwrap (read-line c)))
  ((ok reply) (println "reply: ~a" reply))      ; reply: HELLO
  ((err e) (println "~a" (message e))))
```

ولوضع حد زمني لعملية مرّر `:timeout` (بالثواني).

```lisp
(tcp-connect "example.com" 80 :timeout 5.0)
(accept l :timeout 30.0)
(if (wait-readable c 5.0) (read-line c) (option::none))   ; ضع مؤقتًا على القراءة التالية
```

### عندما ينقطع الطرف الآخر

إذا أعاد الطرف الآخر ضبط الاتصال أو أسقطه في منتصفه فلا يحدث `panic`. فالقراءات اللاحقة تعيد `none`،
وتُهمَل الكتابات بصمت. وللتمييز بين اتصال أُغلق بسلام واتصال فشل افحص `(socket-error c)`.

## 6. حل الأسماء (DNS)

لا توجد دالة مخصصة لحل الأسماء. فتمرير اسم مضيف إلى `tcp-connect` أو `tls-connect` أو `send-to` يحلّه
في داخلها. ويجري الحل على خيط آخر، فتستمر المهام الأخرى في العمل أثناء الانتظار. وإذا كان للاسم عدة
عناوين جُرّبت بالتتابع.

وإذا تعذّر حل الاسم أُعيدت `Err`.

```lisp
(match (tcp-connect "no-such-host.invalid" 80 :timeout 3.0)
  ((ok c) (close c))
  ((err e) (println "~a" (message e))))
;; resolve: no-such-host.invalid:80: failed to lookup address information: ...
```

## 7. TLS

تُستخدم `tls-connect` مثل `tcp-connect` وتجري مصافحة TLS بعد الاتصال. ويُتحقَّق من شهادة الخادم مقابل
شهادات الجذر القياسية. والنتيجة `socket-stream` عادي، فتعمل القراءة والكتابة كما مع TCP.

```lisp
(match (tls-connect "example.com" 443 :timeout 10.0)
  ((ok c)
   (progn
     (write-string c "GET / HTTP/1.1\r\nHost: example.com\r\nConnection: close\r\n\r\n")
     (println "~a" (unwrap (read-line c)))     ; HTTP/1.1 200 OK
     (close c)))
  ((err e) (println "tls failed: ~a" (message e))))
```

يُنشأ خادم TLS بتمرير ملفي الشهادة والمفتاح الخاص (PEM) إلى `tls-listen`.

```lisp
(let ((l (unwrap (tls-listen "0.0.0.0" 8443 "cert.pem" "key.pem"))))
  (loop (match (accept l)
          ((ok c) (progn (task (serve c)) ()))   ; تعمل serve الخاصة بـ TCP كما هي
          ((err e) (println "accept: ~a" (message e))))))
```

وعند التجربة بشهادة موقَّعة ذاتيًا مرّر `:ca-file "cert.pem"` من جانب العميل ليثق بتلك الشهادة. أما
TLS المتبادل (mutual TLS) وخدمة عدة مواقع من مستمِع واحد فمشروحان في
[الشبكات](../reference/functions/network.md#2-tcp--tls--مقابس-نطاق-unix).

## 8. UDP

UDP ليس تدفقًا؛ فهو يرسل ويستقبل رسالة بيانات (datagram) واحدة في كل مرة. والبيانات تسلسل بايتات
(`Vector<int>`)؛ وتُحوَّل من السلاسل النصية وإليها بـ `string->utf8` / `utf8->string`.

```lisp
(let ((a (unwrap (udp-bind "127.0.0.1" 0)))          ; حتى الإرسال يحتاج مقبسًا؛ و0 تترك المنفذ لنظام التشغيل
      (b (unwrap (udp-bind "127.0.0.1" 9999))))
  (unwrap (send-to a "localhost" 9999 (string->utf8 "ping")))
  (match (recv-from b :timeout 1.0)
    ((ok d) (println "~a from ~a" (unwrap (utf8->string (bytes d))) (from d)))
    ((err e) (println "~a" (message e)))))
;; ping from 127.0.0.1:59161
```

`(from d)` هو `ip:port` الخاص بالمرسل، ويمكن استخدامه كما هو وجهةً لـ `send-to`.

## 9. مقابس نطاق Unix

مرّر مسار ملف المقبس إلى `unix-listen` / `unix-connect`. والقيمة التي تحصل عليها هي نفس
`socket-stream` الخاص بـ TCP. وتعيد `unix-listen` قيمة `Err` إذا كان الملف موجودًا بالفعل. وإغلاق
المستمِع يزيل الملف.
