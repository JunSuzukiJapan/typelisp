<!-- translated-from: docs/ja/reference/functions/network.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# الشبكات (TCP / TLS / نطاق Unix / UDP)

المقابس (sockets) من أفراد [التدفقات](streams-files.md). ويُرى الاتصال في **منظورين للاتصال نفسه**:
`socket-stream` (محارف) و`socket-byte-stream` (بايتات)، وتعمل عليهما `read-line`/`write-line`/
`read-byte`/`format` كما هي. والمستمِع (listener) هو `socket-listener`. **ويشترك TCP وTLS ومقابس نطاق
Unix في نوع واحد** (مثل `net.Conn` في Go): فبعد الاتصال تتماثل القراءة والكتابة، ولا يختلف إلا أسلوب
إنشائها. أما UDP فليس تدفقًا بل رسائل بيانات (datagrams) (`udp-socket`).

**الذي ينتظر هو المهمة لا الخيط.** فـ `accept` و`read-line` و`write-string` و`tcp-connect` (بما في ذلك حل
الأسماء) و`recv-from` كلها توقف *تلك المهمة* إذا لم تكن جاهزة (مثل `sleep`/`recv`)، وتواصل المهام الأخرى
العمل. ولهذا يمكن كتابة خادم بالشكل نفسه كما في Go، `(task (serve c))` لكل اتصال
([مرجع الصياغة 12.5](../syntax.md#125-أين-تتبدل-المهام)).

## 1. الأنواع

| النوع | السمات المنفَّذة | كيف تحصل عليه |
|---|---|---|
| `socket-stream` | `CharInput` `PeekInput` `CharOutput` | `tcp-connect` / `tls-connect` / `unix-connect` / `accept` / `char-stream-of` |
| `socket-byte-stream` | `ByteInput` `ByteOutput` | `byte-stream-of` |
| `socket-listener` | `Stream` (`close` / `open-stream-p`) | `tcp-listen` / `tls-listen` / `unix-listen` |
| `udp-socket` | `Stream` | `udp-bind` |
| `datagram` | — (`bytes`: `Vector<int>`، `from`: `string`) | `recv-from` |
| `NetError` | `Error` | قيمة `Err` الراجعة من الدوال أدناه |

لنوع التدفق نوع عنصر واحد (للسبب نفسه في `file-stream`/`binary-file-stream`)، فالمحارف والبايتات نوعان
مختلفان. وتعيد `byte-stream-of`/`char-stream-of` قيمًا تشير إلى **الاتصال نفسه** وتتشارك مخزن الاستقبال
المؤقت: وهكذا تكتب أمورًا مثل HTTP، حيث تُقرأ الترويسات محارفَ والجسم بايتات. وقراءة بايت مباشرة بعد
`unread-char` لمحرف خطأ (القاعدة نفسها كما في الملفات).

## 2. TCP / TLS / مقابس نطاق Unix

| الاسم | الاستخدام | النوع | المعنى |
|---|---|---|---|
| `tcp-connect` | `(tcp-connect host port &key timeout)` | `(string,int,&key f64)→Result<socket-stream,NetError>` | يتصل. ويمكن أن يكون `host` اسمًا أو عنوانًا. وإذا كان للاسم عدة عناوين جُرّبت بالتتابع (`localhost` هو `::1` و`127.0.0.1`). وفشل حل الاسم أو رفض الاتصال أو تجاوز `:timeout` ثانية يعطي `Err` |
| `tls-connect` | `(tls-connect host port &key timeout ca-file cert-file key-file server-name)` | `(string,int,&key f64 string string string string)→Result<socket-stream,NetError>` | TLS بعد `tcp-connect`. تُفحص الشهادة مقابل اسم `host` (أو `:server-name` إذا اختلف الاسم المراد التحقق منه عن مكان الاتصال) بشهادات الجذر من Mozilla. وعند إعطاء `:ca-file` (PEM) يثق **بالشهادات التي فيه فقط** (سلطة شهادات خاصة، أو الشهادة نفسها التي يقدمها `tls-listen` الخاص بك). و`:cert-file`/`:key-file` (كلاهما أو لا شيء) هما شهادتنا، وتُقدَّم عندما يطلب الخادم شهادة (TLS المتبادل). وتكتمل المصافحة هنا، فإذا لم تجتز شهادة الخادم أعاد هذا الاستدعاء `Err`. والنتيجة `socket-stream` عادي |
| `unix-connect` | `(unix-connect path)` | `(string)→Result<socket-stream,NetError>` | يتصل بمقبس نطاق Unix في `path`. وهو محلي، فلا انتظار للمصافحة ولا مهلة زمنية |
| `tcp-listen` | `(tcp-listen host port)` | `(string,int)→Result<socket-listener,NetError>` | يستمع. `"127.0.0.1"` لهذا الجهاز وحده، و`"0.0.0.0"` لكل واجهة. وتمرير `0` لـ `port` يترك لنظام التشغيل الاختيار |
| `tls-listen` | `(tls-listen host port cert-file key-file &key client-ca)` | `(string,int,string,string,&key string)→Result<socket-listener,NetError>` | نسخة TLS من `tcp-listen`. `cert-file` سلسلة الشهادات (PEM، وشهادتك أولًا) و`key-file` المفتاح الخاص. ويُقرآن ويُفحصان هنا، فمفتاح غير مطابق يعطي `Err` من هذا الاستدعاء لا عند أول عميل. وتعود `accept` **قبل المصافحة**، وتكمل أول قراءة أو كتابة من المهمة التي تعالج الاتصال المصافحة (مثل `tls.Conn` في Go)، فلا يعطّل عميل بطيء المصافحة بقية استدعاءات `accept`. وعند إعطاء `:client-ca` (PEM) **يشترط على كل عميل** تقديم شهادة صادرة عن سلطة فيه (TLS المتبادل). وبدونه لا تُطلب أي شهادة |
| `unix-listen` | `(unix-listen path)` | `(string)→Result<socket-listener,NetError>` | يستمع عند `path`. **`Err` إذا كان الملف موجودًا** (فقد يكون لعملية أخرى جارية، فلا يُستبدل بصمت). ويزيل `close` الملف |
| `accept` | `(accept l &key timeout)` | `(socket-listener,&key f64)→Result<socket-stream,NetError>` | الاتصال التالي. يوقف المهمة حتى يأتي واحد. ويستسلم بـ `Err` بعد `:timeout` ثانية |
| `wait-readable` | `(wait-readable s secs)` | `(socket-stream,f64)→bool` | حتى تمكن القراءة دون انتظار، أو `secs` ثانية. و`true` تعني الأول (بما في ذلك بيانات مخزنة مؤقتًا بالفعل). وهو طريقة وضع مؤقت على قراءة: `(if (wait-readable c 5.0) (read-line c) ...)`. وما يعد به هو أن **القراءة التالية لا تتوقف**؛ وقد تنتظر `read-line` بقية السطر |
| `wait-writable` | `(wait-writable s secs)` | `(socket-stream,f64)→bool` | حتى تمكن الكتابة، أو `secs` ثانية |
| `tls-add-certificate` | `(tls-add-certificate l name cert-file key-file)` | `(socket-listener,string,string,string)→Result<(),NetError>` | يضيف شهادة أخرى إلى مستمِع `tls-listen`، تُقدَّم للعملاء الذين يطلبون `name` (SNI): عدة مواقع على مستمِع واحد. ويُفحص هنا هل السلسلة مخصصة لـ `name`، وإلا أعاد هذا الاستدعاء `Err`. والعملاء الذين يطلبون اسمًا لم يضفه أحد، أو لا يطلبون اسمًا، يحصلون على شهادة `tls-listen` |
| `peer-subject` | `(peer-subject s)` | `(socket-stream)→Option<string>` | موضوع (subject) شهادة الطرف الآخر (`CN=client,O=Example,C=JP`، بصيغة RFC 4514، الأكثر تحديدًا أولًا). وهي الطريقة التي يعرف بها خادم TLS المتبادل "من الذي اتصل" (بعد أول قراءة، التي تكمل المصافحة). وفي جانب العميل اسم شهادة الخادم. و`none` للاتصالات العادية، أو قبل المصافحة، أو إذا لم يقدم الطرف الآخر شهادة |
| `requested-server-name` | `(requested-server-name s)` | `(socket-stream)→Option<string>` | في اتصال TLS من جانب الخادم، الاسم الذي طلبه العميل (SNI). وهي الطريقة التي يعرف بها خادم له عدة مواقع أُضيفت بـ `tls-add-certificate` "أي موقع". و`none` للاتصالات العادية، أو في جانب العميل، أو بلا اسم (اتصال بالعنوان) |
| `set-nodelay` | `(set-nodelay s on)` | `(socket-stream,bool)→()` | يوقف Nagle (`TCP_NODELAY`). وتذهب `write-string` إلى المقبس مباشرة كل مرة، فمع تفعيل Nagle تنتظر استجابة مكتوبة على جزأين، ترويسة وجسم، ACK المتأخر من الطرف الآخر: استخدم `true` لبروتوكولات الطلب/الاستجابة. ولا Nagle في مقابس نطاق Unix، فتنجح ببساطة |
| `set-keepalive` | `(set-keepalive s on)` | `(socket-stream,bool)→()` | يفحص الطرف الآخر أثناء الخمول (`SO_KEEPALIVE`). ويكتشف طرفًا اختفى دون إغلاق (كابل مسحوب، مضيف متوقف) ويعيد ضبط الاتصال. والمدة الافتراضية لنظام التشغيل طويلة (غالبًا ساعتان)، فاقرنه بـ `set-keepalive-period` أدناه. ومقابس نطاق Unix لا تدعمه، فيحدث `panic` |
| `set-keepalive-period` | `(set-keepalive-period s secs)` | `(socket-stream,int)→()` | ثواني الخمول قبل أول فحص والفاصل بين الفحوص (ثوانٍ كاملة، 1 على الأقل). ومثل `SetKeepAlivePeriod` في Go يُضبط الاثنان على القيمة نفسها |
| `socket-error` | `(socket-error s)` | `(socket-stream \| socket-byte-stream)→Option<NetError>` | إذا كسر **الطرف الآخر** هذا الاتصال، فأول إخفاق له (إعادة ضبط أو تنبيه TLS أو انقطاع أثناء كتابة). و`none` إذا كان سليمًا: فنهاية الملف EOF من الطرف الآخر إذ يغلق بسلام ليست إخفاقًا. انظر "إخفاقات الطرف الآخر" أدناه |
| `local-address` | `(local-address s)` | `(socket-stream \| socket-listener \| udp-socket)→string` | `host:port` من جانبنا. وهي طريقة معرفة المنفذ المختار بعد `(tcp-listen h 0)`. ولمقابس Unix المسار (والطرف المتصل `(unnamed)`) |
| `peer-address` | `(peer-address s)` | `(socket-stream)→string` | `host:port` للطرف الآخر، أو المسار لمقابس Unix |
| `shutdown-output` | `(shutdown-output s)` | `(socket-stream)→()` | يغلق جانب الإرسال فقط (إغلاق نصفي half-close). فيقرأ الطرف الآخر EOF، ويمكن لهذا الطرف أن يظل يقرأ. وهي إشارة "أرسلت الطلب كله". ومع TLS يرسل أيضًا `close_notify` |
| `byte-stream-of` | `(byte-stream-of s)` | `(socket-stream)→socket-byte-stream` | نسخة البايتات من الاتصال نفسه |
| `char-stream-of` | `(char-stream-of b)` | `(socket-byte-stream)→socket-stream` | نسخة المحارف من الاتصال نفسه |
| `close` | `(close s)` | `Stream` | يرسل المخزن المؤقت ثم يغلق. والـ GC لا يغلقه |
| `with-connection` | `(with-connection (var host port) body...)` | ماكرو | يتصل وينفّذ الجسم ويغلق. `Result<قيمة الجسم, NetError>` (الشكل نفسه كـ `with-open-file`) |

تعود `write-string`/`write-line` **بعد كتابة كل شيء** (مثل `net.Conn.Write` في Go). ولتجميع كتابات صغيرة
اجمعها في `string-output-stream` واكتب مرة واحدة. و`listen` تكون `true` فقط عندما يكون في مخزن الاستقبال
المؤقت شيء، فتعمل `read-char-no-hang` كما يقول اسمها.

**إخفاقات الطرف الآخر لا تحدث `panic`.** فإذا أعاد الطرف الآخر ضبط الاتصال أو رفض مصافحة TLS أو أسقطه في
منتصف كتابة، فليس هذا خطأً في هذا البرنامج، فلا يتوقف الخادم مع بقية عملائه. ويُسجَّل أول إخفاق في
الاتصال؛ وتعيد القراءات اللاحقة `none` (فتبدو مثل EOF)، وتذهب الكتابات إلى لا مكان وتعود بصمت. وللتمييز
بينها استخدم `socket-error`، بالشكل نفسه كـ `bufio.Scanner.Err` في Go (فـ `read-item` قيمة `Option` وليس
لها قناة أخرى للإبلاغ). ولا يحدث `panic` إلا لأخطاء البرنامج نفسه (مقبض مغلق، أو قراءة بايت مباشرة بعد
`unread-char`).

**المهل الزمنية وسائط صريحة أو `wait-readable`.** لا توجد صيغة، مثل `SetReadDeadline` في Go، يحمل فيها
تدفق موعدًا نهائيًا فتفشل `read-line`: فـ `read-item` تعيد `Option<Item>` وليس لها طريقة لإعادة خطأ.
ويلزم مؤقت في ثلاثة مواضع: الاتصال والقبول و"القراءة التالية"، ولكل منها وسيط لذلك.

```lisp
;; الخادم: مهمة لكل اتصال
(defun serve ((c socket-stream)) ()
  (loop (match (read-line c)
          ((some line) (write-line c line))
          ((none) (break))))
  (close c))

(let ((l (unwrap (tcp-listen "0.0.0.0" 7777))))
  (loop (match (accept l)
          ((ok c) (progn (task (serve c)) ()))
          ((err e) (println "accept: ~a" (message e))))))

;; العميل
(match (with-connection (c "127.0.0.1" 7777)
         (write-line c "hello")
         (unwrap (read-line c)))
  ((ok reply) (println "~a" reply))
  ((err e) (println "~a" (message e))))

;; HTTPS
(let ((c (unwrap (tls-connect "example.com" 443 :timeout 10.0))))
  (write-string c "GET / HTTP/1.1\r\nHost: example.com\r\nConnection: close\r\n\r\n")
  (println "~a" (unwrap (read-line c)))       ; HTTP/1.1 200 OK
  (close c))

;; خادم TLS (يمكن صنع الشهادة مثلًا بـ
;;   openssl req -x509 -newkey rsa:2048 -nodes -subj /CN=localhost \
;;           -addext subjectAltName=DNS:localhost -keyout key.pem -out cert.pem)
(let ((l (unwrap (tls-listen "0.0.0.0" 8443 "cert.pem" "key.pem"))))
  (loop (match (accept l)
          ((ok c) (progn (task (serve c)) ()))        ; serve أعلاه كما هي؛ وأول read-line هي المصافحة
          ((err e) (println "accept: ~a" (message e))))))
;; عميله: يتصل واثقًا بشهادته هو
(unwrap (tls-connect "localhost" 8443 :timeout 5.0 :ca-file "cert.pem"))

;; TLS المتبادل: يشترط الخادم شهادة عميل صادرة عن ca.pem، ويقدم العميل واحدة
(tls-listen "0.0.0.0" 8443 "cert.pem" "key.pem" :client-ca "ca.pem")
(tls-connect "localhost" 8443 :ca-file "ca.pem" :cert-file "client.pem" :key-file "client-key.pem")
;; في جانب الخادم، بعد أول read-line: من الذي اتصل
(println "~a" (unwrap-or (peer-subject c) "anonymous"))    ; CN=client,O=Example

;; موقعان على مستمِع واحد (SNI)
(let ((l (unwrap (tls-listen "0.0.0.0" 443 "a.example.pem" "a.key"))))
  (unwrap (tls-add-certificate l "b.example" "b.example.pem" "b.key"))
  ...)                                        ; على الاتصال تقول (requested-server-name c) أي موقع
```

في TLS المتبادل يُنهي العميل الذي لا يقدم شهادة (أو يقدم شهادة لا تجتاز) جانبه من المصافحة قبل أن يقرر
الخادم، في TLS 1.3، فتعيد `tls-connect` القيمة `Ok` و**تعيد أول قراءة `none`** (مع التنبيه في
`socket-error`). وفي جانب الخادم تعيد أول قراءة على الاتصال نفسه `none` أيضًا. ولا يحدث `panic` في أي من
الجانبين.

## 3. UDP

| الاسم | الاستخدام | النوع | المعنى |
|---|---|---|---|
| `udp-bind` | `(udp-bind host port)` | `(string,int)→Result<udp-socket,NetError>` | ينشئ مقبسًا. وهو لازم حتى للإرسال فقط (`port` هو `0`) |
| `send-to` | `(send-to s host port bytes)` | `(udp-socket,string,int,Vector<int>)→Result<(),NetError>` | يرسل رسالة بيانات واحدة. وتُحَلّ الأسماء. ولا يُعرف هل وصلت (UDP) |
| `recv-from` | `(recv-from s &key timeout)` | `(udp-socket,&key f64)→Result<datagram,NetError>` | رسالة البيانات التالية. و`from` هو `ip:port` ويمكن تمريره كما هو كـ `host` لـ `send-to` |

حوّل بين السلاسل النصية وتسلسلات البايتات بـ `string->utf8` / `utf8->string`
([السلاسل النصية](collections.md#1-السلاسل-النصية-string)).

```lisp
(let ((s (unwrap (udp-bind "127.0.0.1" 0))))
  (unwrap (send-to s "127.0.0.1" 9999 (string->utf8 "ping")))
  (match (recv-from s :timeout 1.0)
    ((ok d) (println "~a from ~a" (unwrap (utf8->string (bytes d))) (from d)))
    ((err e) (println "~a" (message e)))))
```

## 4. ما ليس موجودًا

- طريقة لقراءة أي شيء من شهادة الطرف الآخر **غير الموضوع (subject)** (SAN أو مدة الصلاحية أو الجهة
  المصدِرة).
- **الانتظار على مقبس وقناة معًا بـ `select`.** وكما في Go اكتبها على أنها "ابدأ مهمة تقرأ ودعها تغذّي
  قناة".
- **HTTP/2**. و**رسائل البيانات** (`SOCK_DGRAM`) في نطاق Unix.
- **المواعيد النهائية التي يحملها تدفق** (للأسباب المذكورة أعلاه، المؤقتات وسائط).
