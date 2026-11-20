<!-- translated-from: docs/ja/reference/functions/traits.md @ 6271c6d8b68694e1d27dd2e349b08117d2ef2bfb -->
# स्टैंडर्ड trait

इटरेशन, तुलना, अंकगणित और तत्व जोड़ने व निकालने के trait। अन्य स्टैंडर्ड trait अपने-अपने अध्यायों में हैं: `Hash` ([HashTable](collections.md#4-hashtablekv)), `Error` ([त्रुटि टाइप](option-result.md#3-त्रुटि-टाइप-और-error-trait)), `print-object` ([प्रिंटिंग](printing.md#5-print-object-प्रति-टाइप-प्रिंट-किया-गया-प्रतिनिधित्व)), और स्ट्रीम trait तथा `Pathish` ([स्ट्रीम और फ़ाइलें](streams-files.md))। कौन-सा टाइप कौन-सा लागू करता है यह [टाइप](../types.md) में है। trait कैसे परिभाषित करें यह [सिंटैक्स संदर्भ](../syntax.md#39-deftrait--impl--trait) में है।

## 1. `Iter` trait और इटरेशन

```lisp
(deftrait Iter ()
  (:type Item)
  (defmethod next (self) Option<Item>))
```

`Vector<T>`/`HashTable<K,V>`/`Array<T>` क्रमशः `vector-iter<T>`/`hashtable-iter<K,V>`/`array-iter<T>` के ज़रिए `Iter` लागू करते हैं (इटरेटर `(iter collection)` से पाएँ)। `Chan<T>` स्वयं एक `Iter` है (`recv` `next` की भूमिका निभाता है; [चैनल](concurrency.md#2-chant--चैनल))। `Sexpr` सूचियाँ `Iter` लागू नहीं करतीं (उनके तत्वों के टाइप एकसमान नहीं हैं)। यदि आप अपने टाइप के लिए `Iter` लागू करते हैं, तो उस पर `doiter` से चला जा सकता है, और उसे [अनुक्रम फ़ंक्शन](sequences.md#4-iter-पर-अनुक्रम-फ़ंक्शन) को पास किया जा सकता है।

## 2. `Equal` / `Ord` (तुलना)

`Equal` वह trait है जिसे `equal` उपयोग करता है; यह Rust के `PartialEq` के समान है। `Ord` Rust के
`PartialOrd` के समान है। इनका उपयोग generic फ़ंक्शनों की `:where` सीमाओं में यह माँगने के लिए होता है कि
तत्वों के टाइप की तुलना की जा सके।

```lisp
(deftrait Equal ()
  (defmethod equal (self (other Self)) bool))                ; लागू करना अनिवार्य
(deftrait Ord (Equal)                                     ; Equal से विरासत
  (defmethod < (self (other Self)) bool)                     ; लागू करना अनिवार्य
  (defmethod <= (self (other Self)) bool (not (< other self)))
  (defmethod > (self (other Self)) bool (< other self))
  (defmethod >= (self (other Self)) bool (not (< self other))))
```

`Equal` लागू करने के लिए `equal` लिखें, और `Ord` के लिए केवल `<`। `Ord` का बाकी हिस्सा डिफ़ॉल्ट
कार्यान्वयन भरते हैं। `Ord` को `Equal` से विरासत मिलती है, इसलिए `impl Ord X` से पहले `impl Equal X` चाहिए।

`(equal a b)` `a` के टाइप का `Equal` कार्यान्वयन बुलाता है। जो टाइप `Equal` लागू नहीं करता उसके लिए इसे
लिखना टाइप त्रुटि है। पहचान से तुलना के लिए `eq`/`eql`, और संरचना से तुलना के लिए `equalp` का उपयोग करें।
ये तीनों किसी भी टाइप के मान पर उपयोग किए जा सकते हैं।

हर trait मेथड को सीधे फ़ंक्शन की तरह बुलाया जा सकता है (`where (Equal A)`/`(Ord A)` सीमा के भीतर, या उसे लागू
करने वाले ठोस टाइप पर):

| नाम | फ़ॉर्म | टाइप | विवरण |
|---|---|---|---|
| `equal` | `(equal a b)` | `(A,A)→bool` where `Equal A` | क्या वे बराबर हैं (Rust का `==`) |
| `<` | `(< a b)` | `(A,A)→bool` where `Ord A` | से छोटा |
| `<=` | `(<= a b)` | `(A,A)→bool` where `Ord A` | से छोटा या बराबर |
| `>` | `(> a b)` | `(A,A)→bool` where `Ord A` | से बड़ा |
| `>=` | `(>= a b)` | `(A,A)→bool` where `Ord A` | से बड़ा या बराबर |

`Equal` इनके लिए लागू है: सभी संख्यात्मक टाइप (`i8` से `u32` / `f32` / `f64` / `int` / `ratio`), `bool`
`char` `string` `symbol` `complex`, `Sexpr`, `cons-cell<A,B>` और tuple (तत्व-दर-तत्व, जब तत्व `Equal` हों),
तथा `Option<T>` और `Result<T,E>` (जब वे एक ही वेरिएंट हों और सामग्री `equal` हो)। बिल्ट-इन टाइपों के लिए
टाइप का अपना बिल्ट-इन `equal` मेथड जैसा है वैसा ही `Equal` का कार्यान्वयन है। संख्याएँ `=` से, स्ट्रिंग
सामग्री से (बड़े-छोटे अक्षरों में भेद के साथ) तुलना होती हैं। `Ord` इनके लिए लागू है: सभी संख्यात्मक टाइप,
`char` `string` और `cons-cell<A,B>` (शब्दकोशीय क्रम में, जब तत्व `Ord` हों)।

सूची (`Sexpr`) का `equal` CL का `equal` है: cons संरचना से, स्ट्रिंग सामग्री से, और बाकी हर atom `eql` से
तुलना होते हैं। यह तुलना रनटाइम पर मानों पर चलती है और कोई मेथड नहीं बुलाती। इसलिए सूची के भीतर का struct,
उसके टाइप का `impl Equal` कुछ भी हो, CL की तरह पहचान से तुलना होता है।

स्केलर तुलना ऑपरेटर receiver टाइप के अनुसार बिल्ट-इन मेथड हैं ([संख्याएँ](numbers.md), [स्ट्रिंग और अक्षर](collections.md)), और वही सीधे `Ord` का
कार्यान्वयन हैं। सीमा के भीतर `=` `equal` बनता है, और `/=` `(not (equal a b))` बनता है।

## 3. अंकगणितीय trait (`Add` / `Sub` / `Mul` / `Div` / `Rem` / `Bits` / `Number`)

जेनेरिक कोड के लिए "जोड़ा जा सकने वाला टाइप" माँगने की एक परत। **ठोस टाइप पर अंकगणित बिल्ट-इन ऑपरेटर का उपयोग करता है** ([संख्याएँ](numbers.md)) और इस परत से होकर नहीं जाता।

```lisp
(deftrait Add () (defmethod + (self (other Self)) Self))
(deftrait Sub () (defmethod - (self (other Self)) Self))
(deftrait Mul () (defmethod * (self (other Self)) Self))
(deftrait Div () (:type Output) (defmethod / (self (other Self)) Output))
(deftrait Rem () (defmethod rem (self (other Self)) Self))
(deftrait Bits ()
  (defmethod logand (self (other Self)) Self)
  (defmethod logior (self (other Self)) Self)
  (defmethod logxor (self (other Self)) Self)
  (defmethod lognot (self) Self)
  (defmethod ash (self (count int)) Self))                   ; दूरी हमेशा int है
(deftrait Number (Add Sub Mul Rem Ord))                   ; कोई मेथड नहीं; पाँच का संयोजन
```

**बाउंड के भीतर, आप ऑपरेटर लिख सकते हैं।** हर trait के मेथडों के नाम वही हैं जो ठोस टाइपों पर इस्तेमाल होने वाले ऑपरेटरों और बिल्ट-इन फ़ंक्शनों के हैं, इसलिए जब रिसीवर `:where` से बँधा टाइप वेरिएबल हो तब भी उन्हें वैसे ही लिखा जाता है:

```lisp
(defun sum3<T> ((a T) (b T) (c T)) T (:where (Number T))
  (+ a b c))                                              ; = (+ (+ a b) c)
```

`Neg` नहीं है: `(- x)` `(- (- x x) x)` में विस्तारित होता है, इसलिए `Sub` पर्याप्त है।

इनके लिए लागू: सभी संख्यात्मक टाइप (`complex` को छोड़कर) पर `Add`/`Sub`/`Mul`/`Div`/`Rem`/`Number`, और सभी पूर्णांक टाइप तथा `int` पर `Bits`।


**`Div` अपने परिणाम के प्रकार को संबद्ध प्रकार `Output` से नाम देता है।** CL की तरह दो पूर्णांकों पर `/`
`rational` है, इसलिए `int` का `Output` `rational` है, और हर दूसरे संख्यात्मक प्रकार का वह प्रकार स्वयं।
चूँकि `Output` प्रकार के अनुसार अलग है, `Number` में `Div` शामिल नहीं है (supertrait सूची को हर संबद्ध
प्रकार स्थिर करना होता है)। भाग करने वाला जेनेरिक फ़ंक्शन परिणाम का प्रकार `(:where (Div T (Output R)))` से
लेता है, या `(:where (Div T (Output T)))` से `T` के भीतर बंद भाग माँगता है (तब `int` नहीं दिया जा सकता)।

## 4. `Push` trait (तत्व जोड़ना)

```lisp
(deftrait Push ()
  (:type Item)
  (defmethod push (self (x Item)) ()))
```

ऐसा संग्रह जिसमें तत्व एक-एक करके जोड़े जा सकते हैं। जेनेरिक फ़ंक्शन `(:where (Push C (Item T)))` से "ऐसा टाइप जिसमें `T` जोड़ा जा सके" माँगता है, और ऐसे किसी भी संग्रह में उसी `(push c x)` से जोड़ता है।

| टाइप | `push` क्या करता है |
|---|---|
| `Vector<T>` | अंत में जोड़ता है |
| `Deque<T>` | अंत में जोड़ता है (`push-back`) |
| `Array<T>` | fill pointer की जगह पर जोड़ता है (`push-extend`)। fill pointer के बिना panic करता है |
| `HashSet<T>` | सेट में डालता है (`insert`) |

```lisp
(use std::collections::Deque)
(defun push-all<C,T> ((c C) (xs Vector<T>)) () (:where (Push C (Item T)))
  (doiter (x (iter xs)) (push c x)))

(let ((v (the Vector<int> (Vector::new)))
      (d (the Deque<int> (Deque::new))))
  (push-all v (the Vector<int> #(1 2)))
  (push-all d (the Vector<int> #(3 4)))
  (println "~a ~a" v d))                                  ; #(1 2) #<deque 3 4>
```

अपना टाइप जो `Push` लागू करता है, उन्हीं जेनेरिक फ़ंक्शनों को दिया जा सकता है। वह `:dyn Push<int>` की तरह trait object भी बन सकता है।

## 5. `Pop` trait (तत्व निकालना)

```lisp
(deftrait Pop ()
  (:type Item)
  (defmethod pop (self) Option<Item>))
```

ऐसा संग्रह जिससे `push` द्वारा सबसे बाद में जोड़ा गया तत्व वापस निकाला जा सके। खाली होने पर `None`। जेनेरिक फ़ंक्शन इसे `(:where (Pop C (Item T)))` से माँगता है।

| टाइप | `pop` क्या करता है |
|---|---|
| `Vector<T>` | अंतिम तत्व निकालता है |
| `Deque<T>` | अंतिम तत्व निकालता है (`pop-back`) |
| `Array<T>` | fill pointer से ठीक पहले का तत्व निकालता है। fill pointer न हो तो panic |

`HashSet<T>` में "अंतिम तत्व" नहीं होता, इसलिए वह `Push` है पर `Pop` नहीं।

```lisp
(use std::collections::Deque)
(defun pop-all<C,T> ((c C)) Vector<T> (:where (Pop C (Item T)))
  (let ((out (the Vector<T> (Vector::new))) (more true))
    (while more
      (match (pop c)
        ((Some x) (push out x))
        ((None) (progn (setf more false) ()))))
    out))

(let ((v (the Vector<int> #(1 2 3)))
      (d (the Deque<int> (Deque::new))))
  (push d 1) (push d 2)
  (println "~a ~a" (pop-all v) (pop-all d)))              ; #(3 2 1) #(2 1)
```

`Push` की तरह, आपका अपना टाइप इसे implement कर सकता है, और यह `:dyn Pop<int>` जैसा trait object भी हो सकता है।
