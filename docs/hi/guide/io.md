<!-- translated-from: docs/ja/guide/io.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# फ़ाइल I/O, स्ट्रीम और नेटवर्किंग

यह गाइड फ़ाइलें पढ़ने-लिखने, पाथनेम और सॉकेट संचार की बुनियादी बातें दिखाती है। फ़ंक्शनों की सूचियाँ [स्ट्रीम और फ़ाइलें](../reference/functions/streams-files.md) और [नेटवर्किंग](../reference/functions/network.md) में हैं।

## 1. विफलताएँ `Result` के रूप में लौटती हैं

जो ऑपरेशन पर्यावरण के आधार पर विफल हो सकते हैं, जैसे फ़ाइल खोलना या कनेक्ट करना, वे `Result` लौटाते हैं। फ़ाइल का गायब होना प्रोग्राम की गलती नहीं है, इसलिए वह `panic` नहीं करता। परिणामों को `match` से अलग करें।

```lisp
(match (read-file-string "config.txt")
  ((ok text) (println "~a" text))
  ((err e) (println "error: ~a" (message e))))
;; यदि फ़ाइल गायब है:
;; error: config.txt: No such file or directory (os error 2)
```

जब आप जानते हों कि कोई ऑपरेशन विफल नहीं हो सकता, या किसी छोटी स्क्रिप्ट में जहाँ विफलता पर रुक जाना ठीक है, `unwrap` मान बाहर निकाल लेता है। यदि वह `Err` है, तो वह panic करता है।

## 2. पूरी फ़ाइल को पढ़ना और लिखना

सबसे आसान फ़ंक्शन पूरी फ़ाइल को एक साथ संभालते हैं।

```lisp
(unwrap (write-file-string "copy.txt" "abc\n"))
(read-file-string "copy.txt")      ; => (ok "abc\n")
(read-file-lines "copy.txt")       ; => (ok #<vector<string> "abc">)
```

## 3. स्ट्रीम से पढ़ना और लिखना

थोड़ा-थोड़ा पढ़ने या लिखने के लिए `with-open-file` से स्ट्रीम खोलें। बॉडी जिस भी तरह छोड़ी जाए, स्ट्रीम बंद हो जाती है। इसका मान `Result<बॉडी का मान, FileError>` है।

```lisp
;; लिखना
(match (with-open-file (out "notes.txt" direction-output)
         (write-line out "first")
         (write-line out "second")
         (format out "total: ~a~%" 2))
  ((ok _) ())
  ((err e) (println "write failed: ~a" (message e))))

;; एक बार में एक पंक्ति पढ़ना
(match (with-open-file (in "notes.txt" direction-input)
         (let ((n 0))
           (loop (match (read-line in)
                   ((some line) (progn (setf n (+ n 1)) (println "~a: ~a" n line)))
                   ((none) (break))))
           n))
  ((ok n) (println "~a lines" n))
  ((err e) (println "read failed: ~a" (message e))))
```

- फ़ाइल खोलने की तीन दिशाएँ हैं: `direction-input` (पढ़ना), `direction-output` (लिखना; मौजूदा सामग्री हटा दी जाती है) और `direction-append` (अंत में जोड़ना)।
- फ़ाइल के अंत पर `read-line` `none` लौटाता है।
- यदि आप `with-open-file` की जगह `open-file` से फ़ाइल खोलते हैं, तो हमेशा `close` कॉल करें। GC स्ट्रीम बंद नहीं करता।

बाइट पढ़ने और लिखने के लिए, फ़ाइल को `open-binary-input` / `open-binary-output` से खोलें और `read-byte` / `write-byte` का उपयोग करें। कैरेक्टर स्ट्रीम और बाइट स्ट्रीम अलग टाइप हैं, इसलिए कैरेक्टर स्ट्रीम से बाइट पढ़ने की कोशिश टाइप त्रुटि है।

### स्ट्रिंग को स्ट्रीम की तरह उपयोग करना

```lisp
(with-output-to-string (s)
  (write-string s "a")
  (write-string s "b"))                  ; => "ab"

(with-input-from-string (s "line1\nline2")
  (read-line s))                         ; => (some "line1")
```

### स्टैंडर्ड इनपुट और आउटपुट

`*standard-input*`, `*standard-output*` और `*error-output*` भी स्ट्रीम हैं। `(read-line *standard-input*)` एक पंक्ति पढ़ता है।

### पढ़ने और लिखने वाले फ़ंक्शनों को जेनेरिक बनाना

हर प्रकार की स्ट्रीम अपना अलग टाइप है, लेकिन साझा ऑपरेशन trait में इकट्ठा किए गए हैं। जो फ़ंक्शन अपना आर्ग्युमेंट `(where (CharInput S))` के साथ लेता है, वह फ़ाइल, स्ट्रिंग या सॉकेट से पढ़ सकता है।

```lisp
(defun count-lines<S> ((s S)) int (where (CharInput S))
  (let ((n 0))
    (loop (match (read-line s)
            ((some _) (setf n (+ n 1)))
            ((none) (break))))
    n))
```

## 4. पाथनेम

फ़ाइल का नाम लेने वाले फ़ंक्शन स्ट्रिंग या `pathname` दोनों स्वीकार करते हैं। पाथ को हिस्सों में बाँटने या बनाने के लिए `pathname` का उपयोग करें।

```lisp
(let ((p (parse-namestring "/var/log/app.tar.gz")))
  (pathname-directory p)   ; => #<vector<string> "var" "log">
  (pathname-name p)        ; => (some "app.tar")
  (pathname-type p))       ; => (some "gz")

;; केवल एक्सटेंशन बदलें
(namestring (merge-pathnames (make-pathname :type "json") (to-pathname "data/in.csv")))
;; => "data/in.json"

;; फ़ाइल का नाम किसी डायरेक्टरी के भीतर रखें
(namestring (merge-pathnames (make-pathname :name "today" :type "log") "/var/log/"))
;; => "/var/log/today.log"
```

फ़ाइल सिस्टम ऑपरेशन:

```lisp
(unwrap (ensure-directories-exist "out/deep/"))   ; पैरेंट सहित उसे बनाएँ
(probe-file "out/deep")                           ; => true (वह मौजूद है)
(directory-p "out/deep")                          ; => true
(directory "out")                                 ; => सामग्री की सूची
(delete-file "copy.txt")
(rename-file "a.txt" "b.txt")
```

विभाजक हमेशा `/` है। Common Lisp पाथनेम की तरह होस्ट, डिवाइस या संस्करण घटक नहीं हैं, और वाइल्डकार्ड या लॉजिकल पाथनेम भी नहीं हैं।

## 5. TCP

सॉकेट कनेक्शन भी एक स्ट्रीम है, इसलिए `read-line` और `write-line` उस पर जस के तस काम करते हैं।

### सर्वर

बुनियादी पैटर्न है हर कनेक्शन के लिए एक टास्क शुरू करना। `accept` और `read-line` डेटा आने तक **केवल उसी टास्क** को रोकते हैं, इसलिए दूसरे कनेक्शन का संचालन जारी रहता है।

```lisp
(defun serve ((c socket-stream)) ()
  (loop (match (read-line c)
          ((some line) (write-line c (upcase line)))
          ((none) (break))))                  ; दूसरे पक्ष ने बंद कर दिया
  (close c))

(let ((l (unwrap (tcp-listen "127.0.0.1" 7777))))
  (loop (match (accept l)
          ((ok c) (progn (task (serve c)) ()))
          ((err e) (println "accept: ~a" (message e))))))
```

`"127.0.0.1"` केवल इसी मशीन से कनेक्शन स्वीकार करता है, और `"0.0.0.0"` हर इंटरफ़ेस पर स्वीकार करता है। टास्क के लिए [सिंटैक्स संदर्भ अध्याय 12](../reference/syntax.md#12-कंकरेंसी-टास्क) देखें।

### क्लाइंट

`with-connection` कनेक्ट करता है, अपनी बॉडी चलाता है और अंत में कनेक्शन बंद करता है। इसका मान `Result<बॉडी का मान, NetError>` है।

```lisp
(match (with-connection (c "localhost" 7777)
         (write-line c "hello")
         (unwrap (read-line c)))
  ((ok reply) (println "reply: ~a" reply))      ; reply: HELLO
  ((err e) (println "~a" (message e))))
```

किसी ऑपरेशन पर समय सीमा लगाने के लिए `:timeout` (सेकंड में) पास करें।

```lisp
(tcp-connect "example.com" 80 :timeout 5.0)
(accept l :timeout 30.0)
(if (wait-readable c 5.0) (read-line c) (option::none))   ; अगले पढ़ने पर घड़ी लगाएँ
```

### जब दूसरा पक्ष डिस्कनेक्ट हो जाए

यदि दूसरा पक्ष कनेक्शन रीसेट कर दे या उसे बीच में छोड़ दे, तो वह `panic` नहीं करता। बाद के पढ़ने `none` लौटाते हैं, और लिखना चुपचाप छोड़ दिया जाता है। यह बताने के लिए कि कनेक्शन साफ़-सुथरे ढंग से बंद हुआ या विफल हुआ, `(socket-error c)` जाँचें।

## 6. नाम समाधान (DNS)

नाम समाधान के लिए कोई समर्पित फ़ंक्शन नहीं है। `tcp-connect`, `tls-connect` या `send-to` को होस्ट नाम देने पर वह उनके भीतर हल हो जाता है। समाधान दूसरे थ्रेड पर होता है, इसलिए प्रतीक्षा के दौरान दूसरे टास्क चलते रहते हैं। यदि एक नाम के कई पते हैं, तो उन्हें बारी-बारी से आज़माया जाता है।

यदि नाम हल नहीं हो सकता, तो `Err` लौटाया जाता है।

```lisp
(match (tcp-connect "no-such-host.invalid" 80 :timeout 3.0)
  ((ok c) (close c))
  ((err e) (println "~a" (message e))))
;; resolve: no-such-host.invalid:80: failed to lookup address information: ...
```

## 7. TLS

`tls-connect` का उपयोग `tcp-connect` की तरह होता है और कनेक्ट करने के बाद TLS हैंडशेक करता है। सर्वर प्रमाणपत्र को स्टैंडर्ड रूट प्रमाणपत्रों के विरुद्ध सत्यापित किया जाता है। परिणाम सामान्य `socket-stream` है, इसलिए पढ़ना और लिखना TCP की तरह काम करता है।

```lisp
(match (tls-connect "example.com" 443 :timeout 10.0)
  ((ok c)
   (progn
     (write-string c "GET / HTTP/1.1\r\nHost: example.com\r\nConnection: close\r\n\r\n")
     (println "~a" (unwrap (read-line c)))     ; HTTP/1.1 200 OK
     (close c)))
  ((err e) (println "tls failed: ~a" (message e))))
```

TLS सर्वर `tls-listen` को प्रमाणपत्र और निजी कुंजी फ़ाइलें (PEM) देकर बनाया जाता है।

```lisp
(let ((l (unwrap (tls-listen "0.0.0.0" 8443 "cert.pem" "key.pem"))))
  (loop (match (accept l)
          ((ok c) (progn (task (serve c)) ()))   ; TCP वाला serve जस का तस काम करता है
          ((err e) (println "accept: ~a" (message e))))))
```

स्व-हस्ताक्षरित प्रमाणपत्र के साथ आज़माते समय, क्लाइंट की ओर `:ca-file "cert.pem"` पास करें ताकि वह उस प्रमाणपत्र पर भरोसा करे। म्यूचुअल TLS, और एक लिसनर से कई साइटें परोसना, [नेटवर्किंग](../reference/functions/network.md#2-tcp--tls--unix-डोमेन) में दिए हैं।

## 8. UDP

UDP स्ट्रीम नहीं है; यह एक बार में एक डेटाग्राम भेजता और पाता है। डेटा बाइट अनुक्रम (`Vector<int>`) है; स्ट्रिंग से और स्ट्रिंग में बदलने के लिए `string->utf8` / `utf8->string` का उपयोग करें।

```lisp
(let ((a (unwrap (udp-bind "127.0.0.1" 0)))          ; भेजने के लिए भी सॉकेट चाहिए; 0 पोर्ट OS पर छोड़ देता है
      (b (unwrap (udp-bind "127.0.0.1" 9999))))
  (unwrap (send-to a "localhost" 9999 (string->utf8 "ping")))
  (match (recv-from b :timeout 1.0)
    ((ok d) (println "~a from ~a" (unwrap (utf8->string (bytes d))) (from d)))
    ((err e) (println "~a" (message e)))))
;; ping from 127.0.0.1:59161
```

`(from d)` भेजने वाले का `ip:port` है, जिसे `send-to` के गंतव्य के रूप में जस का तस इस्तेमाल किया जा सकता है।

## 9. Unix डोमेन सॉकेट

`unix-listen` / `unix-connect` को सॉकेट फ़ाइल का पाथ दें। जो मान मिलता है वह TCP वाला ही `socket-stream` है। यदि फ़ाइल पहले से मौजूद हो तो `unix-listen` `Err` लौटाता है। लिसनर को बंद करने से फ़ाइल हट जाती है।
