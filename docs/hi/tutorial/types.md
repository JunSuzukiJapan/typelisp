<!-- translated-from: docs/ja/tutorial/types.md @ fc3823e182015d6a1ef25d03ecdf8ca958af01f2 -->
# टाइप की बुनियादी बातें

typelisp एक स्टैटिक रूप से टाइप की गई भाषा है। यह अध्याय समझाता है कि टाइप चेकर आपके लिए क्या करता है, वे टाइप जिनका आप सबसे अधिक उपयोग करेंगे
(`Option`, `Result`, struct और enum), और जेनेरिक। यह मानकर चलता है कि आपने [शुरुआत करना](intro.md) पढ़ लिया है।

## 1. स्टैटिक टाइपिंग का अर्थ

typelisp में हर एक्सप्रेशन का टाइप प्रोग्राम चलने से पहले तय हो जाता है। जिस एक्सप्रेशन के टाइप मेल नहीं खाते, वह कुछ भी चलने से पहले ही त्रुटि है।

```lisp
(defun square ((n int)) int (* n n))

(defun main () ()
  (println "start")
  (println "~a" (square "3")))    ; टाइप त्रुटि

(main)
```

यह फ़ाइल चलाने पर `start` भी प्रिंट हुए बिना टाइप त्रुटि के साथ रुक जाती है।

```
error: main.typl:5:25: type error: type mismatch: expected `int`, found `string`
```

आपको फ़ंक्शन के आर्ग्युमेंट और रिटर्न मान, ग्लोबल वेरिएबल, और struct फ़ील्ड के टाइप लिखने होते हैं। `let` वेरिएबल का टाइप उसके प्रारंभिक मान से लिया जाता है।

मुख्य टाइप:

| टाइप | उदाहरण मान |
|---|---|
| `int` | `42`, `-7` (मनमानी सटीकता वाले पूर्णांक) |
| `i8` `i16` `i32` `u8` `u16` `u32` | निश्चित चौड़ाई वाले पूर्णांक |
| `f64` `f32` | `1.5` |
| `bool` | `true`, `false` |
| `char` | `#\a` |
| `string` | `"hello"` |
| `symbol` | `'foo`, `:key` |
| `()` | कोई मान न लौटाने वाले फ़ंक्शन का रिटर्न टाइप |

रन टाइम पर किसी मान का टाइप पूछने का कोई तरीका नहीं है (Common Lisp के `typep` या `type-of` जैसा कुछ नहीं), क्योंकि हर टाइप प्रोग्राम चलने से पहले ही ज्ञात होता है।

## 2. `Option<T>`: ऐसा मान जो अनुपस्थित हो सकता है

typelisp में `nil` नहीं है। "हो सकता है कोई मान न हो" को टाइप `Option<T>` से व्यक्त किया जाता है। `Option<T>` का मान या तो `some` होता है, जिसमें `T` का एक मान होता है, या `none`, जिसमें कुछ नहीं होता।

```lisp
(defun safe-div ((a int) (b int)) Option<int>
  (if (= b 0)
      (Option::none)
      (Option::some (/ a b))))
```

```
typl> (safe-div 10 2)
(some 5)
typl> (safe-div 1 0)
none
```

`Option<int>`, `int` नहीं है, इसलिए इसे जस का तस अंकगणित में इस्तेमाल नहीं किया जा सकता। `(+ (safe-div 10 2) 1)` एक टाइप त्रुटि है। अंदर के मान का उपयोग करने के लिए `match` से `some` और `none` को अलग करें।

```lisp
(defun show-div ((a int) (b int)) string
  (match (safe-div a b)
    ((some q) (format false "~a" q))
    ((none) "undefined")))
```

- `(some q)` शाखा में अंदर की सामग्री वेरिएबल `q` से बँध जाती है।
- `match` जाँचता है कि उसकी शाखाएँ **हर स्थिति को कवर करती हैं**। `(none)` शाखा भूल जाना टाइप त्रुटि है।

### nil क्यों नहीं है

कई भाषाओं में `nil` (`null`) किसी भी टाइप के मान की जगह ले सकता है। नतीजतन, "कोई मान नहीं" वाली स्थिति को संभालना भूल जाने का पता प्रोग्राम चलने तक नहीं चलता। typelisp में जहाँ मान अनुपस्थित हो सकता है, वहाँ टाइप `Option<T>` होता है, और जब तक `match` `none` स्थिति को संभाल न ले, कोड टाइप चेकर से पास नहीं होता। भूली हुई स्थिति प्रोग्राम चलने से पहले ही पकड़ ली जाती है।

शर्तें भी यही विचार अपनाती हैं: `if` की शर्त केवल `bool` हो सकती है। Common Lisp जैसा "`nil` के अलावा कुछ भी सत्य है" नियम यहाँ नहीं है।

### आम ऑपरेशन

| फ़ॉर्म | अर्थ |
|---|---|
| `(unwrap-or opt default)` | `some` के लिए सामग्री; `none` के लिए डिफ़ॉल्ट |
| `(unwrap opt)` | सामग्री बाहर निकालता है। `none` पर प्रोग्राम रोक देता है |
| `(is-some opt)` / `(is-none opt)` | जाँचता है कि कौन-सा है |

कई स्टैंडर्ड लाइब्रेरी फ़ंक्शन `Option` लौटाते हैं। उदाहरण के लिए, `position` तत्व मिलने पर `some` में स्थिति लौटाता है और न मिलने पर `none`।

```lisp
(defvar (fruits Vector<string>) (Vector::new))
(push fruits "apple")
(push fruits "banana")

(match (position "banana" (iter fruits))
  ((some i) (println "found at ~a" i))
  ((none) (println "not found")))
```

## 3. `Result<T,E>`: ऐसा ऑपरेशन जो विफल हो सकता है

जो ऑपरेशन विफल हो सकता है वह `Result<T,E>` लौटाता है: सफलता पर `T` का मान रखने वाला `ok`, या विफलता पर त्रुटि `E` रखने वाला `err`।

```lisp
(match (parse-int "42")
  ((ok n) (println "n = ~a" (+ n 1)))
  ((err e) (println "error: ~a" (message e))))
;; n = 43

(match (parse-int "4x2")
  ((ok n) (println "n = ~a" (+ n 1)))
  ((err e) (println "error: ~a" (message e))))
;; error: parse-int: invalid integer literal: "4x2"
```

आपके अपने फ़ंक्शन भी `Result` लौटा सकते हैं।

```lisp
(defun checked-age ((n int)) Result<int,string>
  (if (< n 0)
      (Result::err "age must not be negative")
      (Result::ok n)))
```

जब अनुपस्थित मान को किसी स्पष्टीकरण की ज़रूरत न हो तब `Option` का उपयोग करें, और जब आप बताना चाहें कि कुछ क्यों विफल हुआ तब `Result` का। त्रुटियों को विस्तार से संभालने के बारे में [त्रुटि प्रबंधन](errors.md) में बताया गया है।

## 4. `defstruct`: struct

नामित फ़ील्ड वाला टाइप `defstruct` से परिभाषित होता है।

```lisp
(defstruct point
  (x int)
  (y int))
```

यह परिभाषा आपको निम्नलिखित देती है:

```lisp
(let ((p (point::new 3 4)))     ; एक बनाएँ (आर्ग्युमेंट फ़ील्ड के क्रम में)
  (println "~a" p::x)           ; फ़ील्ड पढ़ें; (x p) भी काम करता है
  (setf p::x 10)                ; उसे बदलें
  (println "~a" p))             ; #<point x: 10 y: 4>
```

struct को अपने फ़ंक्शन देने के लिए `defmethod` का उपयोग करें। पहले आर्ग्युमेंट (`self`) का टाइप तय करता है कि मेथड किस टाइप का है।

```lisp
(defmethod norm2 ((self point)) int
  (+ (* self::x self::x) (* self::y self::y)))

(norm2 (point::new 3 4))        ; => 25
```

`self` आर्ग्युमेंट की जगह केवल टाइप का नाम लिखने से एक ऐसा फ़ंक्शन बनता है जो `point::origin` के रूप में कॉल होता है।

```lisp
(defmethod origin (point) point
  (point::new 0 0))

(point::origin)                 ; => #<point x: 0 y: 0>
```

## 5. `defenum`: कई आकारों में से एक

ऐसा मान जो कई आकारों में से एक हो, जैसे "वृत्त, आयत या बिंदु", `defenum` से परिभाषित होता है। हर आकार को **variant** कहते हैं। हर variant अलग संख्या और अलग टाइप के मान रख सकता है।

```lisp
(defenum shape
  (circle int)        ; त्रिज्या
  (rect int int)      ; चौड़ाई और ऊँचाई
  (dot))              ; कोई मान नहीं रखता
```

मान सामने टाइप का नाम लगाकर बनते हैं, जैसे `shape::circle`। `match` में उन्हें variant के नाम से खोला जाता है।

```lisp
(defun area ((s shape)) int
  (match s
    ((circle r) (* 3 r r))
    ((rect w h) (* w h))
    ((dot) 0)))

(area (shape::circle 2))        ; => 12
(area (shape::rect 3 4))        ; => 12
```

यहाँ भी `match` जाँचता है कि हर स्थिति कवर है। यदि आप बाद में `shape` में कोई variant जोड़ते हैं, तो उसे न संभालने वाला हर `match` टाइप त्रुटि बन जाता है, इसलिए सुधार की ज़रूरत वाली कोई जगह छूटती नहीं।

`(use shape)` के बाद आप टाइप के नाम के बिना `(rect 5 6)` लिख सकते हैं।

`Option` और `Result` इसी तंत्र से बने enum हैं।

## 6. जेनेरिक

जो फ़ंक्शन किसी भी टाइप के लिए काम करे, वह अपने नाम के बाद **टाइप पैरामीटर** `<T>` के साथ परिभाषित होता है।

```lisp
(defun first-or<T> ((v Vector<T>) (default T)) T
  (if (= (len v) 0)
      default
      (get v 0)))
```

कॉल करते समय आप टाइप नहीं देते। `T` आर्ग्युमेंट से निकाल लिया जाता है।

```lisp
(defvar (ints Vector<int>) (Vector::new))
(defvar (names Vector<string>) (Vector::new))
(push names "Ann")

(first-or ints 7)          ; T int है
(first-or names "none")    ; T string है
(first-or ints "none")     ; टाइप त्रुटि: ints एक Vector<int> है, इसलिए T int है
```

struct और enum भी जेनेरिक हो सकते हैं। `Vector<T>`, `Option<T>` और `Result<T,E>` इसी तरह के टाइप हैं।

```lisp
(defstruct pair<A,B>
  (left A)
  (right B))

(pair::new 1 "one")        ; pair<int,string>
```

जेनेरिक फ़ंक्शन के भीतर `T` के बारे में कुछ ज्ञात नहीं होता, इसलिए आप `T` के मानों की तुलना या जोड़ नहीं कर सकते। "कोई भी टाइप जिसकी तुलना हो सके" जैसी माँग के लिए trait का उपयोग करें ([Trait](traits.md))।

## 7. किसी टाइप को दूसरा नाम देना

`deftype` किसी टाइप को दूसरा नाम देता है।

```lisp
(deftype meters int)
(defun double ((m meters)) meters (* m 2))
```

`meters`, `int` की सिर्फ़ दूसरी वर्तनी है, कोई नया टाइप नहीं। जहाँ `meters` अपेक्षित हो वहाँ सादा `int` पास करना त्रुटि नहीं है। यदि आप उन्हें अलग रखना चाहते हैं, तो struct बनाएँ, जैसे `(defstruct meters (value int))`।

## 8. आगे क्या पढ़ें

- [Trait](traits.md): टाइप को साझा ऑपरेशन देना
- [टाइप](../reference/types.md): बिल्ट-इन टाइप और हर टाइप द्वारा लागू किए गए trait
