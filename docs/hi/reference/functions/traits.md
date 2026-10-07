<!-- translated-from: docs/ja/reference/functions/traits.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# स्टैंडर्ड trait

इटरेशन, तुलना और अंकगणित के trait। अन्य स्टैंडर्ड trait अपने-अपने अध्यायों में हैं: `Hash` ([HashTable](collections.md#4-hashtablekv)), `Error` ([त्रुटि टाइप](option-result.md#3-त्रुटि-टाइप-और-error-trait)), `print-object` ([प्रिंटिंग](printing.md#5-print-object-प्रति-टाइप-प्रिंट-किया-गया-प्रतिनिधित्व)), और स्ट्रीम trait तथा `Pathish` ([स्ट्रीम और फ़ाइलें](streams-files.md))। कौन-सा टाइप कौन-सा लागू करता है यह [टाइप](../types.md) में है। trait कैसे परिभाषित करें यह [सिंटैक्स संदर्भ](../syntax.md#39-deftrait--impl--trait) में है।

## 1. `Iter` trait और इटरेशन

```lisp
(deftrait Iter ()
  (type Item)
  (next ((self Self)) Option<Item>))
```

`Vector<T>`/`HashTable<K,V>`/`Array<T>` क्रमशः `vector-iter<T>`/`hashtable-iter<K,V>`/`array-iter<T>` के ज़रिए `Iter` लागू करते हैं (इटरेटर `(iter collection)` से पाएँ)। `Chan<T>` स्वयं एक `Iter` है (`recv` `next` की भूमिका निभाता है; [चैनल](concurrency.md#2-chant--चैनल))। `Sexpr` सूचियाँ `Iter` लागू नहीं करतीं (उनके तत्वों के टाइप एकसमान नहीं हैं)। यदि आप अपने टाइप के लिए `Iter` लागू करते हैं, तो उस पर `doiter` से चला जा सकता है, और उसे [अनुक्रम फ़ंक्शन](sequences.md#4-iter-पर-अनुक्रम-फ़ंक्शन) को पास किया जा सकता है।

## 2. `Eq` / `Ord` (तुलना)

ये Rust के `PartialEq`/`PartialOrd` के अनुरूप हैं (नाम `Eq`/`Ord`)। इनका उपयोग जेनेरिक फ़ंक्शन के `where` बाउंड में यह माँगने के लिए होता है कि तत्व टाइप की तुलना हो सके (`sort`/`member`/`assoc` आदि)।

```lisp
(deftrait Eq ()
  (equals ((self Self) (other Self)) bool)                ; लागू करना ज़रूरी
  (not-equals ((self Self) (other Self)) bool             ; डिफ़ॉल्ट इम्प्लीमेंटेशन
    (not (equals self other))))
(deftrait Ord (Eq)                                        ; Eq से विरासत लेता है
  (less ((self Self) (other Self)) bool)                  ; लागू करना ज़रूरी
  (less-equal ((self Self) (other Self)) bool (not (less other self)))
  (greater ((self Self) (other Self)) bool (less other self))
  (greater-equal ((self Self) (other Self)) bool (not (less self other))))
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
(deftrait Add () (add ((self Self) (other Self)) Self))
(deftrait Sub () (sub ((self Self) (other Self)) Self))
(deftrait Mul () (mul ((self Self) (other Self)) Self))
(deftrait Div () (div ((self Self) (other Self)) Self))
(deftrait Rem () (remainder ((self Self) (other Self)) Self))
(deftrait Bits ()
  (bit-and ((self Self) (other Self)) Self)
  (bit-or  ((self Self) (other Self)) Self)
  (bit-xor ((self Self) (other Self)) Self)
  (bit-not ((self Self)) Self)
  (shift ((self Self) (count int)) Self))                 ; दूरी हमेशा int है (ash की तरह)
(deftrait Number (Add Sub Mul Div Rem Ord))               ; कोई मेथड नहीं; छह का संयोजन
```

**बाउंड के भीतर, आप ऑपरेटर लिख सकते हैं।** जब रिसीवर `where` से बँधा टाइप वेरिएबल हो, तो ऑपरेटर trait मेथड के रूप में पढ़े जाते हैं (`+`→`add`, `-`→`sub`, `*`→`mul`, `/`→`div`, `rem`→`remainder`, `logand`→`bit-and`, `=`→`equals`, `<`→`less` …):

```lisp
(defun sum3<T> ((a T) (b T) (c T)) T (where (Number T))
  (+ a b c))                                              ; = (add (add a b) c)
```

trait मेथड का नाम `+` नहीं है क्योंकि `+` बिल्ट-इन मेथड का नाम है और `impl` उसे दोबारा परिभाषित करने से इनकार करता है (`cannot redefine built-in method`)। `Neg` नहीं है: `(- x)` `(- (- x x) x)` में विस्तारित होता है, इसलिए `Sub` पर्याप्त है।

इनके लिए लागू: सभी संख्यात्मक टाइप (`complex` को छोड़कर) पर `Add`/`Sub`/`Mul`/`Div`/`Rem`/`Number`, और सभी पूर्णांक टाइप तथा `int` पर `Bits`।
