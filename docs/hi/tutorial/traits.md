<!-- translated-from: docs/ja/tutorial/traits.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Trait

Trait एक वादा है कि "यह टाइप ये ऑपरेशन सपोर्ट करता है"। Trait कई टाइप को एक ही नाम के ऑपरेशन साझा करने देते हैं, ताकि उन्हें इस्तेमाल करने वाले फ़ंक्शन को हर टाइप के लिए अलग-अलग न लिखना पड़े। वे लगभग ठीक Rust के trait की तरह काम करते हैं। यह अध्याय मानकर चलता है कि आपने [टाइप की बुनियादी बातें](types.md) पढ़ ली हैं।

## 1. Trait को परिभाषित और लागू करना

किसी आकृति का क्षेत्रफल और नाम लौटाने वाले ऑपरेशनों को trait `Shape` के रूप में परिभाषित करें।

```lisp
(deftrait Shape ()
  (area ((self Self)) int)
  (name ((self Self)) string))
```

- trait के नाम के बाद का `()` उन trait की सूची है जिनसे यह विरासत लेता है (खंड 4)। कोई न हो तो उसे खाली छोड़ें।
- हर पंक्ति एक मेथड घोषित करती है। `Self` का अर्थ है "इस trait को लागू करने वाला टाइप"।

किसी टाइप के लिए trait लागू करने के लिए `impl` लिखें।

```lisp
(defstruct circle (r int))
(defstruct rect (w int) (h int))

(impl Shape circle
  (area ((self Self)) int (* 3 self::r self::r))
  (name ((self Self)) string "circle"))

(impl Shape rect
  (area ((self Self)) int (* self::w self::h))
  (name ((self Self)) string "rect"))
```

लागू किए गए मेथड सामान्य फ़ंक्शन की तरह ही कॉल होते हैं।

```lisp
(area (circle::new 2))     ; => 12
(area (rect::new 3 4))     ; => 12
```

Trait द्वारा घोषित मेथडों में से एक भी छोड़ देना `impl` पर टाइप त्रुटि है।

## 2. Trait बाउंड: "कोई भी टाइप जो इस trait को लागू करता हो"

आप `where` से जेनेरिक फ़ंक्शन के टाइप पैरामीटर पर शर्त लगा सकते हैं। इसे **trait बाउंड** कहते हैं।

```lisp
(defun report<T> ((s T)) string
  (where (Shape T))
  (format false "~a: ~a" (name s) (area s)))

(report (circle::new 1))   ; => "circle: 3"
(report (rect::new 2 5))   ; => "rect: 10"
```

`(where (Shape T))` के कारण बॉडी `T` के मानों पर `name` और `area` का उपयोग कर सकती है। बाउंड के बिना `T` के बारे में कुछ ज्ञात नहीं होता, इसलिए उन्हें कॉल नहीं किया जा सकता।

ऐसा टाइप पास करना जो `Shape` को लागू नहीं करता, कॉल पर टाइप त्रुटि है।

```
(report 5)
error: ...: type error: report: type `int` does not implement trait `shape` required by `where` clause on type parameter `t`
```

जेनेरिक फ़ंक्शन को हर उस टाइप के लिए अपनी अलग प्रति मिलती है जिसके साथ उसे कॉल किया जाता है। इसमें कोई रन-टाइम टाइप जाँच या शाखा शामिल नहीं होती।

## 3. डिफ़ॉल्ट इम्प्लीमेंटेशन

यदि trait के मेथड की बॉडी है, तो जब कोई `impl` उस मेथड को छोड़ देता है तब वही बॉडी इस्तेमाल होती है।

```lisp
(deftrait Describe ()
  (label ((self Self)) string)
  (describe ((self Self)) string
    (format false "<~a>" (label self))))

(impl Describe circle
  (label ((self Self)) string "a circle"))            ; describe डिफ़ॉल्ट वाला है

(impl Describe rect
  (label ((self Self)) string "a rect")
  (describe ((self Self)) string "[rect]"))           ; यहाँ लिखे गए को प्राथमिकता मिलती है

(describe (circle::new 1))     ; => "<a circle>"
(describe (rect::new 1 1))     ; => "[rect]"
```

## 4. स्टैंडर्ड trait लागू करना

स्टैंडर्ड लाइब्रेरी में भी trait हैं। एक को लागू करने से उसे इस्तेमाल करने वाले स्टैंडर्ड फ़ंक्शन आपके टाइप के लिए उपलब्ध हो जाते हैं।

| Trait | लागू करने वाले मेथड | यह क्या सक्षम करता है |
|---|---|---|
| `Eq` | `equals` | `member`, `find`, `position`, `match` का `(= expr)` पैटर्न, आदि |
| `Ord` | `less` | `less-equal`, `greater` आदि। `Ord`, `Eq` से विरासत लेता है |
| `print-object` | `print-object` | `println` और उसके साथियों द्वारा मान कैसे दिखाए जाते हैं |
| `Iter` | `next` | `doiter`, `map`, `filter`, `sort` आदि |
| `Error` | `message`, `source` | त्रुटि टाइप के रूप में उपयोग ([त्रुटि प्रबंधन](errors.md)) |

आइए पैसे की राशि दर्शाने वाले टाइप के लिए `Eq` और `Ord` लागू करें। चूँकि `Ord`, `Eq` से विरासत लेता है, `Eq` का `impl` पहले आना चाहिए।

```lisp
(defstruct money (yen int))

(impl Eq money
  (equals ((self Self) (other Self)) bool (= self::yen other::yen)))

(impl Ord money
  (less ((self Self) (other Self)) bool (< self::yen other::yen)))

(greater (money::new 5) (money::new 3))     ; => true (Ord का डिफ़ॉल्ट इम्प्लीमेंटेशन)
```

`print-object` लागू करने से तय होता है कि `println` मान को कैसे दिखाता है। `escape` आर्ग्युमेंट तब `true` होता है जब `~s` की तरह वापस पढ़ा जा सकने वाला रूप माँगा गया हो।

```lisp
(impl print-object money
  (print-object ((self Self) (escape bool)) string
    (format false "~a yen" self::yen)))

(println "~a" (money::new 500))              ; 500 yen
```

trait बाउंड के साथ मिलाकर आप ऐसा फ़ंक्शन लिख सकते हैं जो `Ord` को लागू करने वाले किसी भी टाइप के लिए काम करे।

```lisp
(defun largest<T> ((v Vector<T>)) Option<T>
  (where (Ord T))
  (foldl (iter v)
         (lambda ((best Option<T>) (x T)) Option<T>
           (match best
             ((some b) (if (less b x) (Option::some x) best))
             ((none) (Option::some x))))
         (Option::none)))
```

300, 900 और 100 के `money` मानों वाले `Vector` को इसी क्रम में देने पर यह `(some 900 yen)` लौटाता है।

## 5. `:dyn`: अलग-अलग टाइप के मानों को साथ संभालना

`Vector<T>` के सभी तत्वों का टाइप समान होता है, इसलिए `circle` और `rect` के मान एक `Vector<circle>` में नहीं जा सकते। "`Shape` को लागू करने वाली कोई भी चीज़" को साथ संभालने के लिए टाइप `:dyn Shape` का उपयोग करें।

```lisp
(defun total-area ((shapes Vector<:dyn Shape>)) int
  (let ((sum 0))
    (doiter (s (iter shapes))
      (setf sum (+ sum (area s))))
    sum))

(let ((shapes (the Vector<:dyn Shape> (Vector::new))))
  (push shapes (circle::new 1))
  (push shapes (rect::new 2 3))
  (doiter (s (iter shapes))
    (println "~a -> ~a" (name s) (area s)))
  (println "total ~a" (total-area shapes)))
```

```
circle -> 3
rect -> 6
total 9
```

- जहाँ `:dyn Shape` अपेक्षित हो वहाँ रखा गया `circle` या `rect` का मान अपने-आप परिवर्तित हो जाता है।
- कॉल `(area s)` किस टाइप का `area` चलाएगा, यह रन टाइम पर इस बात से तय होता है कि `s` किस टाइप को रखे हुए है।
- जहाँ `:dyn Shape` अपेक्षित हो वहाँ ऐसे टाइप का मान रखना जो `Shape` को लागू नहीं करता, टाइप त्रुटि है।

खंड 2 के trait बाउंड और `:dyn` में से चुनना:

| | Trait बाउंड (`where`) | `:dyn Trait` |
|---|---|---|
| कॉल किया जाने वाला मेथड कब तय होता है | चलने से पहले | रन टाइम पर |
| एक `Vector` में टाइप मिलाना | संभव नहीं | संभव |
| उपयोग योग्य टाइप | कोई प्रतिबंध नहीं | struct, enum, `int`, `string`, `f64` और अन्य (`bool`, `char`, `symbol`, `i32` और इसी तरह के नहीं) |

उपयोग किए जा सकने वाले टाइप की सटीक सूची
[सिंटैक्स संदर्भ 3.9](../reference/syntax.md#39-deftrait--impl--trait) में है।

कुछ trait `:dyn` के साथ उपयोग नहीं किए जा सकते: वे जिनके मेथड `self` के अलावा किसी आर्ग्युमेंट के लिए या रिटर्न मान के लिए `Self` का उपयोग करते हैं (जैसे `Eq` का `equals`)। चूँकि टाइप रन टाइम तक ज्ञात नहीं होता, "उसी टाइप का मान" बनाने का कोई तरीका नहीं है।

## 6. प्रतिबंध

- trait की परिभाषा, उसके `impl` और उसे `:dyn` के ज़रिए इस्तेमाल करने वाला कोड एक ही मॉड्यूल (फ़ाइल) में रखें। trait को अभी दूसरे मॉड्यूल के लिए दृश्य नहीं बनाया जा सकता।
- टाइप और trait एक ही नेमस्पेस साझा करते हैं। एक मॉड्यूल के भीतर टाइप और trait का नाम समान नहीं हो सकता।

## 7. आगे क्या पढ़ें

- [Macro](macros.md): अपना खुद का सिंटैक्स परिभाषित करना
- [सिंटैक्स संदर्भ 3.9](../reference/syntax.md#39-deftrait--impl--trait): ब्लैंकेट इम्प्लीमेंटेशन, असोसिएटेड टाइप और बहुत कुछ
- [स्टैंडर्ड trait](../reference/functions/traits.md): स्टैंडर्ड लाइब्रेरी के trait की सूची
