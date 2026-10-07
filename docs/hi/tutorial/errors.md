<!-- translated-from: docs/ja/tutorial/errors.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# त्रुटि प्रबंधन

typelisp में त्रुटि प्रबंधन विफलताओं को दो प्रकारों में बाँटता है।

| विफलता का प्रकार | उदाहरण | इसे कैसे व्यक्त किया जाता है |
|---|---|---|
| ऐसी विफलताएँ जो हो सकती हैं (पुनर्प्राप्त की जा सकने वाली) | फ़ाइल गायब है, इनपुट संख्या नहीं है | `Result<T,E>` लौटाएँ |
| प्रोग्राम में गलतियाँ (पुनर्प्राप्त नहीं की जा सकने वाली) | सीमा से बाहर इंडेक्स, `none` पर `unwrap`, शून्य से भाग | `panic` से रुकें |

इनके ऊपर `catch` / `throw` हैं, जो कई फ़ंक्शन कॉल से एक साथ बाहर निकलते हैं, और `unwind-protect`, जो बॉडी जिस भी तरह छोड़ी जाए, क्लीनअप चलाता है। यह अध्याय मानकर चलता है कि आपने [टाइप की बुनियादी बातें](types.md) का `Result` खंड पढ़ लिया है।

## 1. `Result` लौटाएँ और `match` से प्राप्त करें

यहाँ एक फ़ंक्शन है जो स्ट्रिंग से पोर्ट नंबर पढ़ता है। यह दो तरह से विफल हो सकता है: इनपुट संख्या नहीं है, या वह सीमा से बाहर है।

```lisp
(defun parse-port ((s string)) Result<int,string>
  (match (parse-int s)
    ((ok n) (if (and (>= n 1) (<= n 65535))
                (Result::ok n)
                (Result::err (format false "port out of range: ~a" n))))
    ((err e) (Result::err (message e)))))
```

कॉल करने वाला `match` से सफलता और विफलता को अलग करता है।

```lisp
(match (parse-port "99999")
  ((ok p) (println "port ~a" p))
  ((err m) (println "error: ~a" m)))
;; error: port out of range: 99999
```

- `Result` लौटाने वाले फ़ंक्शन का मान तब तक उपयोग नहीं किया जा सकता जब तक `match` `err` स्थिति को न संभाले। विफलता को संभालना भूल जाना टाइप त्रुटि है।
- `parse-int` की त्रुटि `ParseIntError` टाइप का मान है। `(message e)` उसकी संदेश स्ट्रिंग देता है।

## 2. विफलता को कॉल करने वाले तक ऊपर पहुँचाना

Rust के `?` जैसा कोई शॉर्टहैंड नहीं है। `Result` लौटाने वाले कई फ़ंक्शन को बारी-बारी से कॉल करते समय, "विफलता को जस का तस लौटा दो" वाला हिस्सा `match` से लिखें।

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

जब आप जानते हों कि कोई ऑपरेशन विफल नहीं हो सकता, या किसी छोटी स्क्रिप्ट में जहाँ विफलता पर रुक जाना ठीक है, `unwrap` सामग्री बाहर निकाल लेता है। यदि मान `err` है, तो वह panic करता है। यदि डिफ़ॉल्ट मान से काम चल जाए, तो `unwrap-or` का उपयोग करें।

## 3. अपना त्रुटि टाइप बनाना

त्रुटियों को स्ट्रिंग की बजाय टाइप के रूप में व्यक्त करने से कॉल करने वाला त्रुटि के प्रकार के आधार पर शाखा बना सकता है। त्रुटि टाइप एक सामान्य `defenum` या `defstruct` है जो `Error` trait को लागू करता है।

```lisp
(defenum config-error
  (missing string)          ; कोई सेटिंग गायब है
  (invalid string int))     ; कोई मान गलत है

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

- `message` त्रुटि का विवरण लौटाता है।
- `source` वह दूसरी त्रुटि लौटाता है जिसके कारण यह त्रुटि हुई। कोई कारण न हो तो वह `none` है।

## 4. अलग-अलग प्रकार की त्रुटियों को मिलाना

यदि एक फ़ंक्शन `parse-int` (`ParseIntError`) और `check-workers` (`config-error`) दोनों को कॉल करता है, तो दो त्रुटि टाइप होते हैं, और वे दोनों एक `Result<T,E>` का `E` नहीं हो सकते। ऐसे में `E` को `:dyn Error` बनाएँ (`Error` को लागू करने वाले किसी भी टाइप की त्रुटि)। हर त्रुटि को `as-dyn-error` से बदलें।

```lisp
(defun load-count ((s string)) Result<int, :dyn Error>
  (match (as-dyn-error (parse-int s))
    ((ok n) (as-dyn-error (check-workers n)))
    ((err e) (Result::err e))))
```

`"4"`, `"-1"` और `"abc"` देने पर परिणाम ये होते हैं:

```
ok 4
err invalid value for workers: -1
err parse-int: invalid integer literal: "abc"
```

`:dyn` के लिए [Trait](traits.md) का खंड 5 देखें।

## 5. `panic`: प्रोग्राम में गलतियाँ

जब प्रोग्राम ऐसी स्थिति में पहुँच जाए जो कभी नहीं होनी चाहिए, तो उसे `panic` से रोकें।

```lisp
(defun safe-get ((v Vector<int>) (i int)) int
  (if (< i (len v))
      (get v i)
      (panic (format false "index ~a out of range" i))))
```

```
error: main.typl:4:7: panic: index 3 out of range
```

- `panic` का टाइप `!` है (यह लौटता नहीं), इसलिए इसे वहाँ लिखा जा सकता है जहाँ कोई भी टाइप अपेक्षित हो। इसीलिए ऊपर `if` की दोनों शाखाएँ मेल खाती हैं।
- ये ऑपरेशन भी panic करते हैं: `none` या `err` पर `unwrap`, सीमा से बाहर इंडेक्स वाला `get`, और शून्य से पूर्णांक भाग।
- `panic` प्रोग्राम को रोक देता है। यह किसी टास्क के भीतर हो तब भी पूरा प्रोग्राम रुक जाता है।
- REPL में `panic` REPL को समाप्त नहीं करता; वह अगले इनपुट की प्रतीक्षा करता है।
- आप "अभी लिखा नहीं गया" के लिए `(todo)` और "यहाँ कभी नहीं पहुँचना चाहिए" के लिए `(unreachable)` लिख सकते हैं। दोनों panic करते हैं।

`panic`, `Result` का विकल्प नहीं है। जो विफलताएँ हो सकती हैं, जैसे उपयोगकर्ता का इनपुट या फ़ाइल का मौजूद होना, उनके लिए `Result` का उपयोग करें।

## 6. `catch` / `throw`: फ़ंक्शनों के पार कूदना

`throw` बीच में कितने भी फ़ंक्शन कॉल हों, सीधे उसी टैग वाले घेरने वाले `catch` तक कूद जाता है।

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

यदि `v` में कोई ऋणात्मक संख्या नहीं है, तो `validate` `"all fine"` लौटाता है; यदि उसमें `-7` है, तो नियंत्रण `check-all` के भीतर से `catch` तक कूद जाता है, जो `"negative: -7"` लौटाता है।

- टैग को सादे सिंबल के रूप में लिखें, जैसे `'bad-input`।
- **हर टैग ठीक एक टाइप के मान ले जाता है।** ऊपर के उदाहरण में `'bad-input` एक `string` ले जाता है, इसलिए उसी टैग से `int` को throw करना टाइप त्रुटि है। `catch` की बॉडी का टाइप भी टैग के टाइप से मेल खाना चाहिए।

  ```
  error: ...: type error: type mismatch: expected `string`, found `int`
  ```

- ऐसा `throw` जिसके पहुँचने के लिए उसी टैग का कोई `catch` न हो, त्रुटि है।

यदि आप केवल किसी फ़ंक्शन के भीतर से जल्दी लौटना चाहते हैं, तो `catch` / `throw` की जगह `return-from` का उपयोग करें। `return-from` फ़ंक्शनों को पार नहीं कर सकता, लेकिन बदले में आप सोर्स पढ़कर बता सकते हैं कि वह कहाँ लौटता है।

```lisp
(defun first-negative ((v Vector<int>)) Option<int>
  (doiter (x (iter v))
    (if (< x 0) (return-from first-negative (Option::some x)) ()))
  (Option::none))
```

## 7. `unwind-protect`: हमेशा क्लीनअप करें

`(unwind-protect body cleanup)` बॉडी जिस भी तरह छोड़ी जाए क्लीनअप चलाता है: जब वह सामान्य रूप से समाप्त हो, जब उसे `throw` से छोड़ा जाए, और जब वह panic करे।

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

इसका उपयोग "खोली गई फ़ाइल को हमेशा बंद करो" या "लिए गए लॉक को हमेशा छोड़ो" जैसी चीज़ों के लिए करें। स्टैंडर्ड लाइब्रेरी के `with-open-file` और `with-lock` आंतरिक रूप से `unwind-protect` का उपयोग करते हैं।

## 8. Common Lisp की कंडीशन प्रणाली के बारे में

typelisp Common Lisp की कंडीशन प्रणाली (`handler-case`, `restart-case` आदि) को नहीं अपनाता। यह टाइप में नहीं दिखाती कि कोई फ़ंक्शन कौन-सी विफलताएँ पैदा कर सकता है, जो स्टैटिक टाइपिंग से मेल नहीं खाता। जो विफलताएँ हो सकती हैं उन्हें `Result` से टाइप में लिखा जाता है, और नियंत्रण का स्थानांतरण `catch` / `throw` से किया जाता है।

## 9. आगे क्या पढ़ें

- [कंकरेंसी](concurrency.md): टास्क और चैनल
- [Option, Result और त्रुटि टाइप](../reference/functions/option-result.md): फ़ंक्शनों की सूची
- [त्रुटि संदेश](../reference/errors.md): आम त्रुटियों का अर्थ और उन्हें कैसे ठीक करें
