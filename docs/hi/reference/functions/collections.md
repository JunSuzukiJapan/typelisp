<!-- translated-from: docs/ja/reference/functions/collections.md @ 6a9ad7ad4f7c098262628978d36af26e7cdd0401 -->
# स्ट्रिंग, कैरेक्टर और कलेक्शन

`string`, `char`, `Vector<T>`, `HashTable<K,V>`, `Array<T>` और `BitVector`।

## 1. स्ट्रिंग `string`

स्ट्रिंग अपरिवर्तनीय हैं।

| नाम | फ़ॉर्म | टाइप | विवरण |
|---|---|---|---|
| `upcase` | `(upcase s)` | `string→string` | बड़े अक्षरों में बदलता है (केवल ASCII)। CL के `string-upcase` की तरह, नई स्ट्रिंग लौटाता है। स्ट्रिंग अपरिवर्तनीय हैं, इसलिए विनाशकारी `nstring-upcase` नहीं है; यह उसकी जगह लेता है |
| `downcase` | `(downcase s)` | `string→string` | छोटे अक्षरों में बदलता है (केवल ASCII)। `nstring-downcase` की जगह लेता है |
| `capitalize` | `(capitalize s)` | `string→string` | हर शब्द का पहला अक्षर बड़ा और शेष छोटे करता है (CL का `string-capitalize`)। शब्द अक्षरों और अंकों का अधिकतम क्रम है |
| `length` | `(length s)` | `string→int` | कैरेक्टर की संख्या |
| `ref` | `(ref s i)` | `(string,int)→char` | कैरेक्टर `i`। सीमा से बाहर panic करता है |
| `substring` | `(substring s start end)` | `(string,int,int)→string` | उपस्ट्रिंग `[start,end)` |
| `append` | `(append s1 s2 ...)` | `(string,string,...)→string` | जोड़ना। तीन या अधिक दिए जा सकते हैं (`(concatenate 'string ...)` जैसा ही) |
| `<` `<=` `>` `>=` | `(op s1 s2)` | `(string,string)→bool` | शब्दकोशीय तुलना |
| `lt` | `(lt s1 s2)` | `(string,string)→bool` | सख्त शब्दकोशीय less-than (`<` जैसा ही) |
| `eq` `eql` | `(op s1 s2)` | `(string,string)→bool` | पहचान की तुलना (क्या वे एक ही ऑब्जेक्ट हैं, न कि समान सामग्री) |
| `equal` | `(equal s1 s2)` | `(string,string)→bool` | सामग्री की तुलना (अक्षर-भेद के साथ) |
| `equalp` | `(equalp s1 s2)` | `(string,string)→bool` | सामग्री की तुलना (अक्षर-भेद के बिना, केवल ASCII) |
| `/=` | `(/= s1 s2)` | `(string,string)→bool` | क्या सामग्री भिन्न है (CL का `string/=`। वेरिएडिक रूप निकटवर्ती जोड़ियों की तुलना करता है) |
| `lessp` `greaterp` `not-lessp` `not-greaterp` | `(op s1 s2)` | `(string,string)→bool` | अक्षर-भेद के बिना क्रमण (CL का `string-lessp` आदि)। उभयनिष्ठ उपसर्ग के साथ, छोटा वाला छोटा है |
| `string::filled` | `(string::filled n c)` | `(int,char)→string` | `c` की `n` प्रतियों की स्ट्रिंग (CL का `make-string`) |
| `search` | `(search s sub)` | `(string,string)→Option<int>` | वह स्थिति जहाँ `sub` पहली बार आता है। **CL के `search` में आर्ग्युमेंट उलटे क्रम में हैं** (`(search pattern sequence)`)। खाली स्ट्रिंग 0 पर मिलती है। कीवर्ड के लिए [अनुक्रमों के कीवर्ड आर्ग्युमेंट](sequences.md#6-कीवर्ड-आर्ग्युमेंट) देखें |
| `mismatch` | `(mismatch a b)` | `(string,string)→Option<int>` | पहली स्थिति जहाँ वे भिन्न हैं। `none` केवल तब जब वे `equal` हों। यदि एक दूसरे का उपसर्ग है, तो छोटे वाले का अंत। कीवर्ड ऊपर जैसे |
| `trim` `left-trim` `right-trim` | `(trim s)` / `(trim s bag)` | `(string &optional string)→string` | `bag` में शामिल कैरेक्टर को दोनों सिरों से / बाएँ से / दाएँ से हटाता है (CL का `string-trim` आदि)। `bag` के बिना, रिक्त स्थान `" \t\n\r"` |
| `split` | `(split s sep)` | `(string,string)→Vector<string>` | `sep` पर बाँटता है। CL में कोई समकक्ष नहीं है। लगातार विभाजक खाली तत्व बनाते हैं। `sep` खाली हो तो panic करता है |
| `to-string` | `(to-string x)` | `T→string` | `~a` की तरह स्ट्रिंग में बदलता है। `int`/`i32`/`f64`/`bool`/`char`/`string` के लिए लागू है (CL का `princ-to-string`) |
| `string->utf8` | `(string->utf8 s)` | `string→Vector<int>` | UTF-8 के रूप में एन्कोड करता है (हर तत्व 0..255) |
| `utf8->string` | `(utf8->string bytes)` | `Vector<int>→Option<string>` | डीकोड करता है। यदि वह वैध UTF-8 नहीं है तो `none` |

## 2. कैरेक्टर `char`

`char` एक Unicode स्केलर मान है। केस रूपांतरण और वर्गीकरण केवल ASCII सीमा को संभालते हैं।

| नाम | फ़ॉर्म | टाइप | विवरण |
|---|---|---|---|
| `upcase` | `(upcase c)` | `char→char` | बड़े अक्षर में बदलता है (केवल ASCII) |
| `downcase` | `(downcase c)` | `char→char` | छोटे अक्षर में बदलता है (केवल ASCII) |
| `<` `<=` `>` `>=` | `(op c1 c2)` | `(char,char)→bool` | कोड पॉइंट से तुलना |
| `lt` | `(lt c1 c2)` | `(char,char)→bool` | कोड पॉइंट से सख्त less-than (`<` जैसा ही) |
| `alphap` | `(alphap c)` | `char→bool` | क्या यह ASCII अक्षर है |
| `digitp` | `(digitp c)` | `char→bool` | क्या यह ASCII अंक है |
| `eq` `eql` `equal` | `(op c1 c2)` | `(char,char)→bool` | मानों की तुलना करता है |
| `equalp` | `(equalp c1 c2)` | `(char,char)→bool` | अक्षर-भेद की अनदेखी करके मानों की तुलना करता है (CL का `char-equal`) |
| `/=` | `(/= c1 c2)` | `(char,char)→bool` | क्या मान भिन्न हैं (CL का `char/=`। **वेरिएडिक रूप निकटवर्ती जोड़ियों की तुलना करता है**, CL के विपरीत, जो पूछता है कि क्या सभी जोड़ियाँ भिन्न हैं) |
| `lessp` `greaterp` `not-lessp` `not-greaterp` | `(op c1 c2)` | `(char,char)→bool` | अक्षर-भेद के बिना क्रमण (CL का `char-lessp` आदि) |
| `upper-casep` `lower-casep` `both-casep` | `(op c)` | `char→bool` | बड़ा अक्षर / छोटा अक्षर / क्या उसमें केस भेद है भी (CL का `upper-case-p` आदि) |
| `alphanumericp` | `(alphanumericp c)` | `char→bool` | अक्षर या अंक (CL जैसा ही नाम) |
| `graphicp` | `(graphicp c)` | `char→bool` | क्या यह प्रिंट करने योग्य है। रिक्त स्थान शामिल है, नई पंक्ति या टैब नहीं (CL का `graphic-char-p`) |
| `standardp` | `(standardp c)` | `char→bool` | क्या यह CL के 96 मानक कैरेक्टर में से एक है, यानी `graphicp` और नई पंक्ति (CL का `standard-char-p`) |
| `char->int` | `(char->int c)` | `char→int` | Unicode स्केलर मान (उलटा [संख्याएँ](numbers.md#1-निश्चित-चौड़ाई-वाले-पूर्णांक) में `int->char`/`try-int->char` है)। CL के `char-code`/`char-int` के अनुरूप |
| `char->string` | `(char->string c)` | `char→string` | एक-कैरेक्टर की स्ट्रिंग। CL का `string` फ़ंक्शन डेज़िग्नेटर लेकर यह कवर करता है, लेकिन इस भाषा में डेज़िग्नेटर नहीं हैं, इसलिए दिशा नाम में है |
| `digit-weight` | `(digit-weight c)` / `(digit-weight c radix)` | `(char &optional int)→Option<int>` | उस आधार में अंक का **भार** (CL का `digit-char-p`)। `digitp` अलग फ़ंक्शन है जो `bool` लौटाता है |
| `digit->char` | `(digit->char w)` / `(digit->char w radix)` | `(int &optional int)→Option<char>` | भार `w` का कैरेक्टर। 10 और उससे ऊपर के लिए बड़ा अक्षर (CL का `digit-char`; आधार अधिकतम 36 है) |
| `char->name` | `(char->name c)` | `char→Option<string>` | कैरेक्टर का नाम। केवल वही नामित कैरेक्टर जिन्हें रीडर पढ़ सकता है उनके नाम होते हैं (CL का `char-name`) |
| `name->char` | `(name->char s)` | `string→Option<char>` | नाम का कैरेक्टर। अक्षर-भेद के बिना, और रीडर के उपनाम (`linefeed`/`null`) भी स्वीकार करता है (CL का `name-char`) |

`char-code-limit` के अनुरूप कोई स्थिरांक नहीं है (`char` की ऊपरी सीमा Unicode तय करता है, भाषा नहीं)।

## 3. `Vector<T>`

बढ़ने योग्य ऐरे।
मान `#(1 2 3)` लिखा जा सकता है ([सिंटैक्स संदर्भ](../syntax.md#1-लेक्सिकल-तत्व); एलिमेंट टाइप संदर्भ
या पहले एलिमेंट से आता है, और हर मूल्यांकन एक नया वेक्टर बनाता है)। यह `#(1 2 3)` के रूप में ही
प्रिंट भी होता है।

| नाम | फ़ॉर्म | टाइप | विवरण |
|---|---|---|---|
| `new` | `(Vector::new)` | `()→Vector<T>` | खाली वेक्टर बनाता है। टाइप आर्ग्युमेंट अपेक्षित टाइप से आता है, इसलिए नंगे `let` में `(the Vector<i32> (Vector::new))` लिखें |
| `filled` | `(Vector::filled n x)` | `(int,T)→Vector<T>` | `x` की `n` प्रतियाँ |
| `push` | `(push v x)` | `(Vector<T>,T)→Unit` | अंत में जोड़ता है |
| `get` | `(get v i)` | `(Vector<T>,int)→T` | तत्व `i` पढ़ता है। सीमा से बाहर panic करता है |
| `set` | `(set v i x)` | `(Vector<T>,int,T)→Unit` | तत्व `i` बदलता है। सीमा से बाहर panic करता है। `(setf (get v i) x)` भी लिखा जा सकता है |
| `len` | `(len v)` | `Vector<T>→int` | तत्वों की संख्या |
| `pop` | `(pop v)` | `Vector<T>→Option<T>` | अंतिम तत्व हटाकर लौटाता है। खाली हो तो `None` (`get`/`set` के विपरीत, यह panic नहीं करता) |
| `iter` | `(iter v)` | `Vector<T>→vector-iter<T>` | `Iter` लागू करने वाला इटरेटर बनाता है |
| `pushnew` | `(pushnew v x)` | `(Vector<T>,T)→Unit` where `Eq T` | यदि कोई बराबर तत्व मौजूद न हो तो `x` जोड़ता है (CL का `pushnew`। इसे place को दोबारा लिखने की ज़रूरत नहीं, इसलिए यह macro की बजाय मेथड है) |

`map`/`filter` और साथी [अनुक्रम फ़ंक्शन](sequences.md#4-iter-पर-अनुक्रम-फ़ंक्शन) हैं: वेक्टर को `iter` से पास करें, जैसे `(map (iter v) f)`। विनाशकारी ऑपरेशन (`nreverse`, `delete` आदि) [विनाशकारी ऑपरेशन](sequences.md#7-विनाशकारी-ऑपरेशन) में हैं।

## 4. `HashTable<K,V>`

| नाम | फ़ॉर्म | टाइप | विवरण |
|---|---|---|---|
| `new` | `(HashTable::new)` | `()→HashTable<K,V>` | खाली तालिका बनाता है |
| `get` | `(get h k)` | `(HashTable<K,V>,K)→Option<V>` | लुकअप |
| `set` | `(set h k v)` | `(HashTable<K,V>,K,V)→Unit` | जोड़ना या अधिलेखित करना |
| `remove` | `(remove h k)` | `(HashTable<K,V>,K)→Option<V>` | प्रविष्टि हटाता है और, यदि थी, पुराना मान लौटाता है |
| `count` | `(count h)` | `HashTable<K,V>→int` | प्रविष्टियों की संख्या |
| `clear` | `(clear h)` | `HashTable<K,V>→Unit` | सब कुछ हटाता है |
| `keys` | `(keys h)` | `HashTable<K,V>→Vector<K>` | कुंजियों का स्नैपशॉट |
| `values` | `(values h)` | `HashTable<K,V>→Vector<V>` | मानों का स्नैपशॉट |
| `entries` | `(entries h)` | `HashTable<K,V>→Vector<cons-cell<K,V>>` | `(k . v)` जोड़ियों का स्नैपशॉट |
| `iter` | `(iter h)` | `HashTable<K,V>→hashtable-iter<K,V>` | `Iter` लागू करने वाला इटरेटर। तत्व `(k . v)` `cons-cell` हैं। CL के `with-hash-table-iterator` के अनुरूप; `doiter`/`map`/`filter` और अन्य उस पर जस के तस काम करते हैं |
| `maphash` | `(maphash h f)` | `(HashTable<K,V>,(fn (K V) ()))→Unit` | CL का `maphash` |
| `size` | `(size h)` | `HashTable<K,V>→int` | CL का `hash-table-size`। इस तालिका में यह भरी हुई प्रविष्टियों की संख्या है (`count` के बराबर) |

**कोई भी टाइप जो `Hash` लागू करता है कुंजी हो सकता है**, `defstruct`/`defenum` टाइप सहित। `get`/`set`/`remove` में `(where (Hash K))` है, इसलिए ऐसे टाइप की कुंजी वाली तालिका जो इसे लागू नहीं करता **टाइप त्रुटि** है (`NaN` के कारण `f64` में `Hash` नहीं है)।

```lisp
(deftrait Hash (Eq)
  (sxhash ((self Self)) int))              ; fixnum में समाने वाला गैर-ऋणात्मक मान लौटाता है
```

इनके लिए लागू: `int` और छह निश्चित चौड़ाई वाले पूर्णांक, `bool`, `char`, `string` और `symbol` (फ़्लोटिंग-पॉइंट संख्याओं के लिए नहीं)। अपने टाइप के लिए, परिणाम को `*sxhash-mask*` (2^30-1) से `logand` करके गैर-ऋणात्मक रखें। स्ट्रिंग को हैश करने के लिए, आप `(sxhash-string s)` (32-बिट FNV-1a) कॉल कर सकते हैं, जिसे `string` का इम्प्लीमेंटेशन उपयोग करता है।

```lisp
(defstruct point (x int) (y int))
(impl Eq point
  (equals ((self Self) (other Self)) bool
    (if (= self::x other::x) (= self::y other::y) false)))
(impl Hash point
  (sxhash ((self Self)) int (logand (+ (* 31 self::x) self::y) *sxhash-mask*)))

(let ((h (the HashTable<point,string> (HashTable::new))))
  (progn (set h (point::new 1 2) "a")
         (get h (point::new 1 2))))          ; => (some "a")
```

दो कुंजियाँ समान हैं या नहीं, यह **कुंजी का टाइप स्वयं** तय करता है (`sxhash`, और `Hash` के supertrait `Eq` से `equals`), ऑब्जेक्ट पहचान नहीं। इसीलिए, ऊपर की तरह, आप ऐसी कुंजी से लुकअप कर सकते हैं जो "भिन्न मान है लेकिन बराबर है"।

`sxhash` का टकराना ठीक है (`Hash` का अनुबंध केवल एक दिशा में जाता है: बराबर मानों का हैश समान होना चाहिए)। टकराने वाली कुंजियाँ `equals` से अलग पहचानी जाती हैं।

## 5. `Array<T>` (बहुआयामी ऐरे)

स्टैंडर्ड लाइब्रेरी का एक `defstruct`। यह बिल्ट-इन टाइप नहीं है, इसलिए `defstruct` से जो कुछ किया जा सकता है वह इससे भी किया जा सकता है।
मान `#2A((1 2) (3 4))` लिखा जा सकता है ([सिंटैक्स संदर्भ](../syntax.md#1-लेक्सिकल-तत्व))।

| नाम | फ़ॉर्म | टाइप | विवरण |
|---|---|---|---|
| `make` | `(Array::make dims init &key fill-pointer)` | `(Vector<int>,T)→Array<T>` | CL का `make-array`। `dims` कॉपी होता है। `init` हर सेल का प्रारंभिक मान है (CL का `:initial-element`; इस भाषा में "unbound सेल" नहीं है, इसलिए यह आवश्यक है)। `:fill-pointer` केवल एक आयाम के लिए |
| `get` / `set` | `(get a idx)` / `(set a idx x)` | `(Array<T>,Vector<int>)→T` | CL का `aref` / `(setf (aref …))`। इंडेक्स सीमा से बाहर हो तो panic करता है |
| `aref` | `(aref a i j …)` | — | नंगे इंडेक्स के साथ CL की वर्तनी। ऊपर के `get`/`set` में विस्तारित होता है। `(setf (aref a i j) v)` भी काम करता है |
| `row-major-get` / `row-major-set` | `(row-major-get a i)` | `(Array<T>,int)→T` | CL का `row-major-aref`। समतल इंडेक्स |
| `rank` | `(rank a)` | `Array<T>→int` | CL का `array-rank` |
| `dimension` | `(dimension a n)` | `(Array<T>,int)→int` | CL का `array-dimension` |
| `dimensions` | `(dimensions a)` | `Array<T>→Vector<int>` | CL का `array-dimensions`। एक **कॉपी** लौटाता है, ठीक जैसे CL नई सूची लौटाता है |
| `total-size` | `(total-size a)` | `Array<T>→int` | CL का `array-total-size` (आवंटित सेल की संख्या, fill pointer से असंबंधित) |
| `len` | `(len a)` | `Array<T>→int` | ऐरे पर CL का `length`। यदि fill pointer है तो वह, अन्यथा `total-size` |
| `in-bounds` | `(in-bounds a idx)` | `(Array<T>,Vector<int>)→bool` | CL का `array-in-bounds-p`। इंडेक्स की **संख्या** गलत होने पर भी असत्य (त्रुटि नहीं) |
| `row-major-index` | `(row-major-index a idx)` | `(Array<T>,Vector<int>)→int` | CL का `array-row-major-index` |
| `adjust` | `(adjust a dims init)` | `(Array<T>,Vector<int>,T)→Unit` | CL का `adjust-array`। रैंक नहीं बदल सकता। जो तत्व सीमा में रहते हैं वे अपने इंडेक्स पर रखे जाते हैं, और नए सेल `init` पाते हैं। CL के विपरीत, यह ऐरे को नहीं लौटाता (इस भाषा का हर ऐरे समायोज्य है, इसलिए लौटाने को दूसरा ऐरे नहीं है) |
| `push-extend` | `(push-extend a x)` | `(Array<T>,T)→Unit` | CL का `vector-push-extend`। fill pointer के बिना panic करता है |
| `pop` | `(pop a)` | `Array<T>→Option<T>` | CL का `vector-pop`। खाली हो तो `none` |
| `fill-pointer` | `(fill-pointer a)` | `Array<T>→Option<int>` | fill pointer (यदि नहीं है तो `none`)। `(setf a::fill-pointer …)` से लिखा जा सकता है |
| `iter` | `(iter a)` | `Array<T>→array-iter<T>` | रो-मेजर क्रम में इटरेटर। यदि fill pointer है तो वहीं रुकता है |

- **इंडेक्स `Vector<int>` हैं।** मेथड "अंत में एक ही टाइप के आर्ग्युमेंट कितनी भी बार दोहराए जाएँ" घोषित नहीं कर सकता, और `aref` शॉर्टहैंड उस अंतर को पाटता है।
- `array-element-type` / `simple-vector-p` / `adjustable-array-p` / `array-has-fill-pointer-p` **मौजूद नहीं हैं**। रिसीवर का स्टैटिक टाइप पहले से इन प्रश्नों का उत्तर देता है।
- `Array::new` `defstruct` द्वारा बनाया गया फ़ील्ड-क्रम कंस्ट्रक्टर है और ऐरे बनाने के लिए नहीं है। `Array::make` का उपयोग करें।
- **ऐरे CL के ऐरे सिंटैक्स में प्रिंट होते हैं।** रैंक 1 `#(1 2 3)` है; अन्य रैंक `#nA` के बाद उतने स्तर के कोष्ठक हैं (`#2A((1 2 3) (4 5 6))`); रैंक 0 `#0A5` है। यदि fill pointer है तो प्रिंटिंग वहीं रुकती है। `*print-array*` ([प्रिंटिंग](printing.md#6-कितना-प्रिंट-हो-इसे-नियंत्रित-करना)) को असत्य करने पर केवल आकार प्रिंट होता है, `#<array 2x3>`। केवल वह ऐरे जिसके तत्व बिना `print-object` वाला `defstruct` हों बिल्ट-इन रूप `#<array<...> ...>` में प्रिंट होता है (यह त्रुटि नहीं है)।

## 6. `BitVector` (बिट वेक्टर)

बिट का निश्चित लंबाई वाला अनुक्रम। स्टैंडर्ड लाइब्रेरी का एक `defstruct`।

| नाम | फ़ॉर्म | टाइप | विवरण |
|---|---|---|---|
| `make` | `(BitVector::make n)` | `int→BitVector` | लंबाई `n`, सभी बिट 0 |
| `get` / `set` | `(get v i)` / `(set v i b)` | `(BitVector,int)→bool` | सीमा से बाहर panic करते हैं |
| `bit` / `sbit` | `(bit v i)` | `(BitVector,int)→bool` | CL की वर्तनियाँ। `(setf (bit v i) b)` भी काम करता है। CL का `sbit` `bit` से केवल सरल बिट वेक्टर की माँग में भिन्न है, लेकिन इस भाषा में केवल एक प्रकार का बिट वेक्टर है |
| `len` | `(len v)` | `BitVector→int` | बिट की संख्या |
| `bit-and` `bit-ior` `bit-xor` `bit-eqv` `bit-nand` `bit-nor` `bit-andc1` `bit-andc2` `bit-orc1` `bit-orc2` | `(op a b)` | `(BitVector,BitVector)→BitVector` | नया बिट वेक्टर लौटाते हैं। लंबाई भिन्न हो तो panic करते हैं। CL की तरह तीसरा आर्ग्युमेंट (परिणाम का गंतव्य) नहीं है |
| `bit-not` | `(bit-not v)` | `BitVector→BitVector` | पूरक |

`bit-vector-p` नहीं है (स्टैटिक टाइप उसका उत्तर देता है)।
