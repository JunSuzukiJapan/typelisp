<!-- translated-from: docs/ja/reference/functions/traits.md @ 9a4e7d23b09eb3f3244bfb876df1ec57576d6874 -->
# स्टैंडर्ड trait

इटरेशन, तुलना, अंकगणित और तत्व जोड़ने व निकालने के trait। अन्य स्टैंडर्ड trait अपने-अपने अध्यायों में हैं: `Hash` ([HashTable](collections.md#4-hashtablekv)), `Error` ([त्रुटि टाइप](option-result.md#3-त्रुटि-टाइप-और-error-trait)), `print-object` ([प्रिंटिंग](printing.md#5-print-object-प्रति-टाइप-प्रिंट-किया-गया-प्रतिनिधित्व)), और स्ट्रीम trait तथा `Pathish` ([स्ट्रीम और फ़ाइलें](streams-files.md))। कौन-सा टाइप कौन-सा लागू करता है यह [टाइप](../types.md) में है। trait कैसे परिभाषित करें यह [सिंटैक्स संदर्भ](../syntax.md#39-deftrait--impl--trait) में है।

## 1. `Iter` trait और इटरेशन

```lisp
(deftrait Iter ()
  (:type Item)
  (next (self) Option<Item>))
```

`Vector<T>`/`HashTable<K,V>`/`Array<T>` क्रमशः `vector-iter<T>`/`hashtable-iter<K,V>`/`array-iter<T>` के ज़रिए `Iter` लागू करते हैं (इटरेटर `(iter collection)` से पाएँ)। `Chan<T>` स्वयं एक `Iter` है (`recv` `next` की भूमिका निभाता है; [चैनल](concurrency.md#2-chant--चैनल))। `Sexpr` सूचियाँ `Iter` लागू नहीं करतीं (उनके तत्वों के टाइप एकसमान नहीं हैं)। यदि आप अपने टाइप के लिए `Iter` लागू करते हैं, तो उस पर `doiter` से चला जा सकता है, और उसे [अनुक्रम फ़ंक्शन](sequences.md#4-iter-पर-अनुक्रम-फ़ंक्शन) को पास किया जा सकता है।

## 2. `Eq` / `Ord` (तुलना)

ये Rust के `PartialEq`/`PartialOrd` के अनुरूप हैं (नाम `Eq`/`Ord`)। इनका उपयोग जेनेरिक फ़ंक्शन के `:where` बाउंड में यह माँगने के लिए होता है कि तत्व टाइप की तुलना हो सके (`sort`/`member`/`assoc` आदि)।

```lisp
(deftrait Eq ()
  (equals (self (other Self)) bool)                ; लागू करना ज़रूरी
  (not-equals (self (other Self)) bool             ; डिफ़ॉल्ट इम्प्लीमेंटेशन
    (not (equals self other))))
(deftrait Ord (Eq)                                        ; Eq से विरासत लेता है
  (less (self (other Self)) bool)                  ; लागू करना ज़रूरी
  (less-equal (self (other Self)) bool (not (less other self)))
  (greater (self (other Self)) bool (less other self))
  (greater-equal (self (other Self)) bool (not (less self other))))
```

`Eq` लागू करने के लिए आप केवल `equals` लिखते हैं, और `Ord` के लिए केवल `less`। डिफ़ॉल्ट इम्प्लीमेंटेशन बाकी भर देते हैं। `Ord`, `Eq` से विरासत लेता है, इसलिए `impl Ord X` से पहले `impl Eq X` चाहिए।

हर trait मेथड को जस का तस फ़ंक्शन की तरह कॉल किया जा सकता है (`where (Eq A)`/`(Ord A)` बाउंड के भीतर, या उसे लागू करने वाले ठोस टाइप पर):

| नाम | फ़ॉर्म | टाइप | विवरण |
|---|---|---|---|
| `equals` | `(equals a b)` | `(A,A)→bool` where `Eq A` | क्या वे बराबर हैं (Rust का `==`) |
| `not-equals` | `(not-equals a b)` | `(A,A)→bool` where `Eq A` | क्या वे बराबर नहीं हैं (`!=`) |
| `less` | `(less a b)` | `(A,A)→bool` where `Ord A` | `a < b` |
| `less-equal` | `(less-equal a b)` | `(A,A)→bool` where `Ord A` | `a <= b` |
| `greater` | `(greater a b)` | `(A,A)→bool` where `Ord A` | `a > b` |
| `greater-equal` | `(greater-equal a b)` | `(A,A)→bool` where `Ord A` | `a >= b` |

`Eq` इनके लिए लागू है: सभी संख्यात्मक टाइप (`i8` से `u32` / `f32` / `f64` / `int` / `ratio`), `bool` `char` `string` `symbol` `complex`, `Sexpr` (`eq`, यानी पहचान; `match` के मान पैटर्न द्वारा उपयोग), और `cons-cell<A,B>` (पुनरावर्ती रूप से, जब तत्व `Eq` हों)। `Ord` इनके लिए लागू है: सभी संख्यात्मक टाइप, `char` `string`, और `cons-cell<A,B>` (शब्दकोशीय क्रम में, जब तत्व `Ord` हों)।

मेथड के नाम बिल्ट-इन ऑपरेटर (`= /= < <= > >=`) या `eq`/`lt` से नहीं टकराते क्योंकि बिल्ट-इन को दोबारा परिभाषित नहीं किया जा सकता, और हर इम्प्लीमेंटेशन उन्हें सौंप देता है। स्केलर तुलना ऑपरेटर स्वयं हर रिसीवर टाइप के बिल्ट-इन मेथड हैं ([संख्याएँ](numbers.md), [स्ट्रिंग और कैरेक्टर](collections.md))। बाउंड के भीतर, ऑपरेटर लिखने से वे trait मेथड के रूप में पढ़े जाते हैं (अध्याय 3)।

## 3. अंकगणितीय trait (`Add` / `Sub` / `Mul` / `Div` / `Rem` / `Bits` / `Number`)

जेनेरिक कोड के लिए "जोड़ा जा सकने वाला टाइप" माँगने की एक परत। **ठोस टाइप पर अंकगणित बिल्ट-इन ऑपरेटर का उपयोग करता है** ([संख्याएँ](numbers.md)) और इस परत से होकर नहीं जाता।

```lisp
(deftrait Add () (add (self (other Self)) Self))
(deftrait Sub () (sub (self (other Self)) Self))
(deftrait Mul () (mul (self (other Self)) Self))
(deftrait Div () (:type Output) (div (self (other Self)) Output))
(deftrait Rem () (remainder (self (other Self)) Self))
(deftrait Bits ()
  (bit-and (self (other Self)) Self)
  (bit-or  (self (other Self)) Self)
  (bit-xor (self (other Self)) Self)
  (bit-not (self) Self)
  (shift (self (count int)) Self))                 ; दूरी हमेशा int है (ash की तरह)
(deftrait Number (Add Sub Mul Rem Ord))                   ; कोई मेथड नहीं; पाँच का संयोजन
```

**बाउंड के भीतर, आप ऑपरेटर लिख सकते हैं।** जब रिसीवर `:where` से बँधा टाइप वेरिएबल हो, तो ऑपरेटर trait मेथड के रूप में पढ़े जाते हैं (`+`→`add`, `-`→`sub`, `*`→`mul`, `/`→`div`, `rem`→`remainder`, `logand`→`bit-and`, `=`→`equals`, `<`→`less` …):

```lisp
(defun sum3<T> ((a T) (b T) (c T)) T (:where (Number T))
  (+ a b c))                                              ; = (add (add a b) c)
```

trait मेथड का नाम `+` नहीं है क्योंकि `+` बिल्ट-इन मेथड का नाम है और `impl` उसे दोबारा परिभाषित करने से इनकार करता है (`cannot redefine built-in method`)। `Neg` नहीं है: `(- x)` `(- (- x x) x)` में विस्तारित होता है, इसलिए `Sub` पर्याप्त है।

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
  (push (self (x Item)) ()))
```

ऐसा संग्रह जिसमें तत्व एक-एक करके जोड़े जा सकते हैं। जेनेरिक फ़ंक्शन `(:where (Push C (Item T)))` से "ऐसा टाइप जिसमें `T` जोड़ा जा सके" माँगता है, और ऐसे किसी भी संग्रह में उसी `(push c x)` से जोड़ता है।

| टाइप | `push` क्या करता है |
|---|---|
| `Vector<T>` | अंत में जोड़ता है |
| `Deque<T>` | अंत में जोड़ता है (`push-back`) |
| `Array<T>` | fill pointer की जगह पर जोड़ता है (`push-extend`)। fill pointer के बिना panic करता है |
| `HashSet<T>` | सेट में डालता है (`insert`)। तभी `Push` है जब तत्व का टाइप `Hash` लागू करता हो |

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
  (pop (self) Option<Item>))
```

ऐसा संग्रह जिससे `push` द्वारा सबसे बाद में जोड़ा गया तत्व वापस निकाला जा सके। खाली होने पर `none`। जेनेरिक फ़ंक्शन इसे `(:where (Pop C (Item T)))` से माँगता है।

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
        ((some x) (push out x))
        ((none) (progn (setf more false) ()))))
    out))

(let ((v (the Vector<int> #(1 2 3)))
      (d (the Deque<int> (Deque::new))))
  (push d 1) (push d 2)
  (println "~a ~a" (pop-all v) (pop-all d)))              ; #(3 2 1) #(2 1)
```

`Push` की तरह, आपका अपना टाइप इसे implement कर सकता है, और यह `:dyn Pop<int>` जैसा trait object भी हो सकता है।
