<!-- translated-from: docs/ja/README.md @ 25173243b2eff6e1cad20c77d2fc8be7e35e0124 -->
# typelisp दस्तावेज़ (हिन्दी)

typelisp एक स्टैटिक रूप से टाइप किया गया Lisp है। इसे इंस्टॉल और बिल्ड करने के लिए रिपॉज़िटरी के शीर्ष स्तर पर मौजूद
[README.md](../../README.md) (अंग्रेज़ी में) देखें।

## विशेषताएँ

### टास्क और चैनल

टास्क (हल्के थ्रेड) शुरू किए जाते हैं, और वे चैनलों के ज़रिए एक-दूसरे को मान भेजते हैं। विस्तार के लिए [कंकरेंसी](tutorial/concurrency.md) देखें।

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

### जेनेरिक

टाइप पैरामीटर किसी टाइप या फ़ंक्शन के नाम के बाद कोण कोष्ठकों में लिखे जाते हैं। trait बाउंड `:where` खंड में लिखे जाते हैं। विस्तार के लिए [टाइप की बुनियादी बातें](tutorial/types.md) और [Trait](tutorial/traits.md) देखें।

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

### nil की जगह Option और Result

`nil` नहीं है। जिस फ़ंक्शन का मान न हो सकता है वह `Option<T>` लौटाता है, और जो विफल हो सकता है वह `Result<T,E>` लौटाता है; अंदर का मान `match` से निकाला जाता है। `Option<string>` को `string` की तरह इस्तेमाल करना, या `match` में `none` वाला मामला छोड़ देना, प्रोग्राम चलने से पहले ही टाइप त्रुटि है। विस्तार के लिए [टाइप की बुनियादी बातें](tutorial/types.md) और [त्रुटि प्रबंधन](tutorial/errors.md) देखें।

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

### `->` से चरणों को जोड़ना

`->` मैक्रो किसी मान को क्रम से हर अगले फ़ॉर्म के पहले आर्ग्युमेंट के रूप में देता है। `(-> x (f a) (g b))` और `(g (f x a) b)` एक ही हैं। इससे `Option` या `Result` लौटाने वाले चरण ऊपर से नीचे क्रम में लिखे जा सकते हैं। विस्तार के लिए [Option, Result और त्रुटि टाइप](reference/functions/option-result.md) देखें।

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

### टाइप-जाँच वाले मैक्रो

मैक्रो Common Lisp की तरह `defmacro`, quasiquote और `gensym` से लिखे जाते हैं। विस्तार के बाद बना कोड भी हाथ से लिखे कोड की तरह टाइप-जाँच से गुज़रता है, और टाइप न मिलने पर त्रुटि उसी जगह बताई जाती है जहाँ मैक्रो बुलाया गया था। विस्तार के लिए [Macro](tutorial/macros.md) देखें।

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

## ट्यूटोरियल

यदि आप typelisp से पहली बार परिचित हो रहे हैं, तो इन्हें इसी क्रम में पढ़ें।

- [शुरुआत करना](tutorial/intro.md): REPL, फ़ंक्शन, वेरिएबल, कंडीशनल, लूप, सूचियाँ और `Vector`
- [टाइप की बुनियादी बातें](tutorial/types.md): स्टैटिक टाइप, `Option`, `Result`, struct, enum, जेनेरिक
- [Trait](tutorial/traits.md): `deftrait` / `impl`, trait बाउंड, `:dyn`
- [Macro](tutorial/macros.md): `defmacro`, क्वासिक्वोट, `gensym`, `macrolet`
- [त्रुटि प्रबंधन](tutorial/errors.md): `Result`, `panic`, `catch` / `throw`, `unwind-protect`
- [कंकरेंसी](tutorial/concurrency.md): टास्क, चैनल, `select`, `Mutex`, `thread`

## गाइड

- [मॉड्यूल और फ़ाइल संरचना](guide/modules.md): `use`, `pub`, फ़ाइलें मॉड्यूल से कैसे मेल खाती हैं
- [कंपाइल करना](guide/compile.md): JIT, AOT कंपाइलेशन से एक्ज़ीक्यूटेबल बनाना, डंप
- [फ़ाइल I/O, स्ट्रीम और नेटवर्किंग](guide/io.md): फ़ाइलें, पाथनेम, TCP / TLS / UDP, नाम समाधान
- [C FFI](guide/ffi.md): `defffi` से C फ़ंक्शन कॉल करना (कॉलबैक और `def-c-struct` वाले C struct सहित)
- [एडिटर इंटीग्रेशन](guide/editors.md): `typl-lsp` और VS Code / Emacs की सेटिंग
- [Common Lisp प्रोग्रामरों के लिए](guide/from-common-lisp.md): typelisp CL से कैसे भिन्न है और CL कोड को कैसे दोबारा लिखें

## संदर्भ

- [सिंटैक्स संदर्भ](reference/syntax.md): लेक्सिकल सिंटैक्स, टाइप लिखना, परिभाषाएँ, नियंत्रण फ़ॉर्म, कंपाइलेशन, कंकरेंसी
- [बिल्ट-इन फ़ंक्शन](reference/functions/README.md): बिल्ट-इन फ़ंक्शन, मेथड और स्टैंडर्ड लाइब्रेरी
- [टाइप](reference/types.md): टाइप और हर टाइप द्वारा लागू किए गए trait
- [त्रुटि संदेश](reference/errors.md): आम त्रुटियों का अर्थ और उन्हें कैसे ठीक करें

## एडिटर इंटीग्रेशन

सेटअप के निर्देश [एडिटर इंटीग्रेशन गाइड](guide/editors.md) में हैं। हर एडिटर के की बाइंडिंग और
सेटिंग इन दस्तावेज़ों में सूचीबद्ध हैं:

- [Emacs (typelisp-mode)](../../editor/emacs/README_hi.md)
- [VS Code](../../editor/vscode/README_hi.md)
