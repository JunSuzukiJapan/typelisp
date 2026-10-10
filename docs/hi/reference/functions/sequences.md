<!-- translated-from: docs/ja/reference/functions/sequences.md @ 8c7bff99b2cddddb57736d0567933bc024a5a467 -->
# जोड़ियाँ, S-एक्सप्रेशन और अनुक्रम

जेनेरिक जोड़ी `cons-cell`, S-एक्सप्रेशन डेटा `Sexpr`, सिंबल, `Iter` के ऊपर लिखे अनुक्रम फ़ंक्शन, और उच्च-क्रम फ़ंक्शन।

## 1. जोड़ियाँ `cons-cell<A,B>`

`cons`/`car`/`cdr` **जेनेरिक जोड़ी टाइप `cons-cell<A,B>`** (स्टैंडर्ड लाइब्रेरी का एक `defstruct`) के कंस्ट्रक्टर और फ़ील्ड एक्सेसर हैं। फ़ील्ड को `variable::car`/`variable::cdr` ([सिंटैक्स संदर्भ](../syntax.md#36-defstruct--struct-उपयोगकर्ता-परिभाषित-टाइप) का `defstruct` एक्सेसर सिंटैक्स) या `(car variable)`/`(cdr variable)` के रूप में पढ़ा जा सकता है। उन्हें बदलने के लिए `(setf variable::car v)`/`(setf variable::cdr v)` का उपयोग करें।

| नाम | फ़ॉर्म | टाइप | विवरण |
|---|---|---|---|
| `cons` | `(cons a b)` | `(A,B)→cons-cell<A,B>` | जोड़ी बनाता है |
| `car` | `(car p)` | `cons-cell<A,B>→A` | पहला तत्व |
| `cdr` | `(cdr p)` | `cons-cell<A,B>→B` | शेष |

`cons-cell` ट्यूपल सिंटैक्स की जगह भी काम करता है। CL के जो फ़ंक्शन कई मान लौटाते हैं (`floor` का भागफल और शेषफल, `read-from-string` का मान और स्थिति आदि) वे इस भाषा में `cons-cell` लौटाते हैं।

## 2. S-एक्सप्रेशन डेटा `Sexpr`

`read` द्वारा लौटाए जाने वाले डेटा टाइप `Sexpr` के 19 variant हैं: `int | i8 | i16 | i32 | u8 | u16 | u32 | f32 | f64 | char | bool | sym | str | cons | ratio | path | vector | array | tuple`। S-एक्सप्रेशन सेल को अध्याय 1 के सामान्य `cons`/`car`/`cdr` से नहीं बल्कि `sexpr-*` फ़ंक्शन से संभाला जाता है। इनका उपयोग मुख्य रूप से `defmacro` बॉडी में फ़ॉर्म बनाने और खोलने के लिए होता है।
`vector` और `array` `#(..)` और `#nA(..)` के रूप में लिखा गया डेटा हैं ([सिंटैक्स
संदर्भ](../syntax.md#1-लेक्सिकल-तत्व)), जिनमें क्रमशः एक `Vector<Option<Sexpr>>` और एक
`Array<Option<Sexpr>>` होता है: `(vector v)` से बँधे `v` पर `len`, `get` आदि सीधे काम करते हैं।
`tuple` `#{..}` से लिखा गया डेटा है, और `(tuple v)` द्वारा बाँधा गया `v` तत्वों का नया `Vector<Option<Sexpr>>` है (ताकि किसी भी लंबाई का टपल एक ही टाइप से लिया जा सके)।

**S-एक्सप्रेशन डेटा का टाइप `Option<Sexpr>` है।** खाली सूची `Sexpr` का variant नहीं बल्कि `Option` का `none` है, और `Sexpr` का अर्थ स्वयं "एक गैर-खाली S-एक्सप्रेशन" है। इसलिए `sexpr-*` फ़ंक्शन `Option<Sexpr>` लेते और लौटाते हैं।

- जहाँ `Option<Sexpr>` अपेक्षित हो वहाँ `()` खाली सूची है (इसे `(Option::none)` भी लिखा जा सकता है)
- जहाँ `Option<Sexpr>` अपेक्षित हो वहाँ `Sexpr` अपने-आप चौड़ा हो जाता है (बिना रन-टाइम रूपांतरण के)। उलटी दिशा, `Option<Sexpr>` को `Sexpr` की तरह उपयोग करना, यह दावा करता है कि "यह खाली सूची नहीं है", इसलिए इसे `match` या `unwrap` से स्पष्ट रूप से कहना पड़ता है
- `match` में, `Sexpr` के 19 variant और `none` **शाखाओं की एक ही सूची में समतल रूप से** लिखे जा सकते हैं ([सिंटैक्स संदर्भ](../syntax.md#43-match--पैटर्न-मिलान))

| नाम | फ़ॉर्म | टाइप | विवरण |
|---|---|---|---|
| `sexpr-cons` | `(sexpr-cons a b)` | `(Option<Sexpr>,Option<Sexpr>)→Option<Sexpr>` | `Sexpr` सेल बनाता है |
| `sexpr-car` | `(sexpr-car s)` | `Option<Sexpr>→Option<Sexpr>` | पहला तत्व। **खाली सूची के लिए खाली सूची** (CL की तरह)। ऐसे एटम पर panic करता है जो `Cons` नहीं है |
| `sexpr-cdr` | `(sexpr-cdr s)` | `Option<Sexpr>→Option<Sexpr>` | शेष। **खाली सूची के लिए खाली सूची** (CL की तरह)। ऐसे एटम पर panic करता है जो `Cons` नहीं है |
| `sexpr-consp` | `(sexpr-consp s)` | `Option<Sexpr>→bool` | क्या यह `Cons` है |
| `sexpr-null` | `(sexpr-null s)` | `Option<Sexpr>→bool` | क्या यह खाली सूची है |
| `sexpr-atom` | `(sexpr-atom s)` | `Option<Sexpr>→bool` | क्या यह `Cons` नहीं है |
| `sexpr-symp` | `(sexpr-symp s)` | `Option<Sexpr>→bool` | क्या यह `Sym` (सिंबल) है |
| `sexpr-int` | `(sexpr-int s)` | `Option<Sexpr>→int` | `int` variant की सामग्री (fixnum या bignum)। दूसरे टाइप पर panic करता है |
| `sexpr-i8` `sexpr-i16` `sexpr-i32` `sexpr-u8` `sexpr-u16` `sexpr-u32` | `(sexpr-i32 s)` | `Option<Sexpr>→W` | उस चौड़ाई के variant की सामग्री। दूसरे टाइप पर panic करता है |
| `sexpr-f32` `sexpr-f64` | `(sexpr-f64 s)` | `Option<Sexpr>→f32` / `→f64` | फ़्लोटिंग-पॉइंट variant की सामग्री। दूसरे टाइप पर panic करता है |
| `sexpr-char` | `(sexpr-char s)` | `Option<Sexpr>→char` | `Char` की सामग्री। दूसरे टाइप पर panic करता है |
| `sexpr-bool` | `(sexpr-bool s)` | `Option<Sexpr>→bool` | `Bool` की सामग्री। दूसरे टाइप पर panic करता है |
| `sexpr-str` | `(sexpr-str s)` | `Option<Sexpr>→string` | `Str` की सामग्री। दूसरे टाइप पर panic करता है |
| `sexpr-sym-name` | `(sexpr-sym-name s)` | `Option<Sexpr>→string` | `Sym` का नाम। दूसरे टाइप पर panic करता है |
| `eq` `eql` | `(op a b)` | `(Option<Sexpr>,Option<Sexpr>)→bool` | पहचान की तुलना (`Cons`/`Str` ऑब्जेक्ट पहचान की तुलना करते हैं, बाकी मानों की) |
| `equal` | `(equal a b)` | `(Option<Sexpr>,Option<Sexpr>)→bool` | संरचनात्मक समानता (`Cons` पुनरावर्ती रूप से, `Str` सामग्री से) |
| `equalp` | `(equalp a b)` | `(Option<Sexpr>,Option<Sexpr>)→bool` | `equal` जैसा, साथ में अक्षर-भेद के बिना तुलना और टाइपों के पार संख्याओं की तुलना |
| `sexpr-append` | `(sexpr-append a b)` | `(Option<Sexpr>,Option<Sexpr>)→Option<Sexpr>` | दो `Sexpr` सूचियों को जोड़ता है (अविनाशकारी रूप से)। `,@` इसी में विस्तारित होता है |
| `sexpr-map` | `(sexpr-map f lst)` | `((fn (Option<Sexpr>) Option<Sexpr>),Option<Sexpr>)→Option<Sexpr>` | `Sexpr` सूची के हर तत्व पर `f` लगाकर नई `Sexpr` सूची (अध्याय 4 का `map` `Iter` के लिए है और `Sexpr` सूची पर नहीं चल सकता) |

नौ संख्यात्मक एक्सेसर हैं, हर टाइप के लिए एक, क्योंकि `Sexpr` "वह एकमात्र जगह है जहाँ मान का टाइप और कहीं नहीं लिखा जाता"। `Sexpr` में रखा `u8` `u8` variant के रूप में जाता है और केवल `(sexpr-u8 s)` से बाहर आता है। उसे `(sexpr-int s)` को देने पर panic होता है; वह कभी चुपचाप उत्तर को चौड़ा नहीं करता। पढ़े गए डेटा (`'(1 2 3)`, macro आर्ग्युमेंट) के पूर्णांक `int` variant के होते हैं और `(sexpr-int s)` से पढ़े जाते हैं।

`Sexpr` सूचियों में `rplaca`/`nconc` जैसे विनाशकारी ऑपरेशन नहीं हैं। `Sexpr` सेल बनने के बाद बदला नहीं जा सकता।

## 3. सिंबल

`symbol` स्वयं सिंबल का टाइप है। जहाँ `Sexpr` चाहिए वहाँ यह अपने-आप बदल जाता है, लेकिन दूसरी दिशा में अपने-आप नहीं।

| नाम | फ़ॉर्म | टाइप | विवरण |
|---|---|---|---|
| `symbol->string` | `(symbol->string s)` | `symbol→string` | सिंबल का नाम निकालता है |
| `string->symbol` | `(string->symbol s)` | `string→symbol` | स्ट्रिंग से सिंबल बनाता है (इंटर्न करता है) |
| `keywordp` | `(keywordp s)` | `symbol→bool` | क्या यह कीवर्ड (`:name`) है। कोलन नाम का हिस्सा है, इसलिए जाँच पहला कैरेक्टर देखती है ([सिंटैक्स संदर्भ](../syntax.md#1-लेक्सिकल-तत्व)) |

`gensym` के लिए [Macro](system.md#8-macro) देखें।

## 4. `Iter` पर अनुक्रम फ़ंक्शन

अनुक्रम फ़ंक्शन **`Iter` trait पर जेनेरिक फ़ंक्शन** हैं। कलेक्शन से `(iter coll)` द्वारा इटरेटर पाएँ और उसे पास करें (`Vector<T>` / `HashTable<K,V>` / `Array<T>` यह सपोर्ट करते हैं; `Sexpr` सूची `Iter` लागू नहीं करती, इसलिए ये फ़ंक्शन उस पर लागू नहीं होते)। **परिणामी कलेक्शन नए `Vector` के रूप में लौटाया जाता है।** तालिकाओं में `Iter<A>` का अर्थ है "`Iter` का कोई भी इम्प्लीमेंटेशन जिसका `Item` `A` है"। लौटाए गए `Vector` पर फिर चलने के लिए `(iter result)` पास करें।

प्रेडिकेट लेने वाले फ़ंक्शन (CL के `-if` परिवार के अनुरूप):

| नाम | फ़ॉर्म | टाइप | विवरण |
|---|---|---|---|
| `map` | `(map it f)` | `(Iter<A>,(fn (A) U))→Vector<U>` | मैपिंग |
| `filter` | `(filter it pred)` | `(Iter<A>,(fn (A) bool))→Vector<A>` | केवल वे तत्व जो प्रेडिकेट को संतुष्ट करते हैं |
| `remove-if` | `(remove-if it pred)` | `(Iter<A>,(fn (A) bool))→Vector<A>` | प्रेडिकेट को संतुष्ट करने वाले तत्व हटाता है |
| `find-if` | `(find-if it pred)` | `(Iter<A>,(fn (A) bool))→Option<A>` | प्रेडिकेट को संतुष्ट करने वाला पहला तत्व |
| `position-if` | `(position-if it pred)` | `(Iter<A>,(fn (A) bool))→Option<int>` | प्रेडिकेट को संतुष्ट करने वाली पहली स्थिति |
| `count-if` | `(count-if it pred)` | `(Iter<A>,(fn (A) bool))→int` | कितने प्रेडिकेट को संतुष्ट करते हैं |
| `every` | `(every it pred)` | `(Iter<A>,(fn (A) bool))→bool` | क्या हर तत्व प्रेडिकेट को संतुष्ट करता है |
| `any` | `(any it pred)` | `(Iter<A>,(fn (A) bool))→bool` | क्या कोई तत्व प्रेडिकेट को संतुष्ट करता है (CL के `some` के अनुरूप; ऐसा नाम जो `Some` कंस्ट्रक्टर से नहीं टकराता) |
| `foldl` | `(foldl it f init)` | `(Iter<A>,(fn (B A) B),B)→B` | बायाँ फ़ोल्ड |
| `foldr` | `(foldr it f init)` | `(Iter<A>,(fn (A B) B),B)→B` | दायाँ फ़ोल्ड |

इंडेक्सिंग, लंबाई और स्लाइसिंग:

| नाम | फ़ॉर्म | टाइप | विवरण |
|---|---|---|---|
| `length` | `(length it)` | `Iter<A>→int` | तत्वों की संख्या |
| `append` | `(append a b ...)` | `(Iter<A>,Iter<A>,...)→Vector<A>` | इटरेटर जोड़ता है। तीन या अधिक दिए जा सकते हैं |
| `concatenate` | `(concatenate 'vector it ...)` / `(concatenate 'string s ...)` | `→Vector<A>` / `→string` | CL का `concatenate`। परिणाम का टाइप **उद्धृत सिंबल लिटरल** के रूप में लिखा जाता है (CL रन-टाइम टाइप स्पेसिफ़ायर उपयोग करता है)। `'vector` एक या अधिक लेता है, `'string` शून्य या अधिक (शून्य के लिए `""`)। `Sexpr` सूचियाँ कवर नहीं हैं (`sexpr-append` उपयोग करें) |
| `reverse` | `(reverse it)` | `Iter<A>→Vector<A>` | उलटना (अविनाशकारी) |
| `nth` | `(nth n it)` | `(int,Iter<A>)→Option<A>` | तत्व `n` (सीमा से बाहर `None`) |
| `elt` | `(elt it n)` | `(Iter<A>,int)→Option<A>` | उलटे क्रम के आर्ग्युमेंट वाला `nth` |
| `take` | `(take it n)` | `(Iter<A>,int)→Vector<A>` | पहले `n` तत्व |
| `subseq` | `(subseq it start end)` | `(Iter<A>,int,int)→Vector<A>` | `[start,end)` (`end` को लंबाई तक सीमित किया जाता है) |
| `last` | `(last it)` | `Iter<A>→Option<A>` | अंतिम **तत्व** (CL की तरह "अंतिम सेल" नहीं) |
| `butlast` | `(butlast it)` | `Iter<A>→Vector<A>` | अंतिम को छोड़कर सब |

वे फ़ंक्शन जिन्हें `Eq` / `Ord` बाउंड चाहिए (वे प्रेडिकेट की जगह trait से तुलना करते हैं; [स्टैंडर्ड trait](traits.md#2-eq--ord-तुलना)):

| नाम | फ़ॉर्म | टाइप | विवरण |
|---|---|---|---|
| `member` | `(member x it)` | `(A,Iter<A>)→bool` where `Eq A` | क्या `x` के बराबर तत्व मौजूद है (CL के विपरीत, `bool`, सूची का शेष नहीं) |
| `find` | `(find x it)` | `(A,Iter<A>)→Option<A>` where `Eq A` | `x` के बराबर पहला तत्व |
| `position` | `(position x it)` | `(A,Iter<A>)→Option<int>` where `Eq A` | `x` के बराबर पहली स्थिति |
| `count` | `(count x it)` | `(A,Iter<A>)→int` where `Eq A` | कितने तत्व `x` के बराबर हैं |
| `sort` | `(sort it cmp)` | `(Iter<A>,(fn (A A) bool))→Vector<A>` | CL का `(sort sequence predicate)`। स्थिर, अविनाशकारी सॉर्ट। `cmp` तब `true` है जब "पहला आर्ग्युमेंट सख्ती से दूसरे से पहले आता है" |
| `assoc` | `(assoc k it)` | `(K,Iter<cons-cell<K,V>>)→Option<cons-cell<K,V>>` where `Eq K` | पहली जोड़ी जिसका `car` `k` के बराबर है। मान `(cdr p)` से निकालें |

ये और अध्याय 5 के कई फ़ंक्शन CL के कीवर्ड आर्ग्युमेंट `:key` / `:test` / `:test-not` / `:start` / `:end` / `:from-end` / `:count` भी लेते हैं (अध्याय 6)।

## 5. CL के शेष अनुक्रम फ़ंक्शन

सभी अध्याय 4 की तरह `Iter` पर जेनेरिक फ़ंक्शन हैं। परिणामी कलेक्शन नए `Vector` के रूप में लौटाए जाते हैं।

| नाम | फ़ॉर्म | टाइप | विवरण |
|---|---|---|---|
| `first`…`tenth` | `(first it)` | `Iter<A>→Option<A>` | CL के नामित इंडेक्स |
| `rest` | `(rest it)` | `Iter<A>→Vector<A>` | पहले को छोड़कर सब (नया `Vector`, साझा टेल नहीं) |
| `copy-seq` | `(copy-seq it)` | `Iter<A>→Vector<A>` | इटरेटर को `Vector` में मूर्त करता है (CL का `copy-seq`/`copy-list`) |
| `revappend` | `(revappend a b)` | `(Iter<A>,Iter<A>)→Vector<A>` | `a` उलटा, उसके बाद `b` |
| `Vector::filled` | `(Vector::filled n x)` | `(int,T)→Vector<T>` | `x` की `n` प्रतियाँ (CL का `make-list`/`make-sequence`)। `Vector::new` की तरह, टाइप आर्ग्युमेंट अपेक्षित टाइप से आता है, इसलिए नंगे `let` को `the` चाहिए |
| `member-if` `member-if-not` | `(member-if it pred)` | `(Iter<A>,(fn (A) bool))→bool` | `member` की तरह, **`bool`** (इटरेटर के पास लौटाने को टेल नहीं है) |
| `notany` `notevery` | `(notany it pred)` | `(Iter<A>,(fn (A) bool))→bool` | `any`/`every` के निषेध |
| `find-if-not` `position-if-not` `count-if-not` `remove-if-not` | `(op it pred)` | सकारात्मक संस्करणों जैसे टाइप | प्रेडिकेट के निषेध वाले संस्करण |
| `remove` | `(remove x it)` | `(A,Iter<A>)→Vector<A>` where `Eq A` | मान से हटाता है |
| `remove-duplicates` | `(remove-duplicates it)` | `Iter<A>→Vector<A>` where `Eq A` | दोहराव हटाता है। CL की तरह, **अंतिम आवृत्ति रखी जाती है** (`:from-end true` पहली रखता है) |
| `substitute` `substitute-if` | `(substitute new old it)` | `(A,A,Iter<A>)→Vector<A>` | मान / प्रेडिकेट से बदलता है |
| `assoc-if` `rassoc` `rassoc-if` | `(rassoc v it)` | `Iter<cons-cell<K,V>>` पर | `assoc` के प्रेडिकेट और मान-पक्ष संस्करण |
| `acons` | `(acons k v it)` | `(K,V,Iter<cons-cell<K,V>>)→Vector<cons-cell<K,V>>` | सामने जोड़ी जोड़ता है |
| `pairlis` | `(pairlis ks vs)` | `(Iter<K>,Iter<V>)→Vector<cons-cell<K,V>>` | दो अनुक्रमों को जोड़ी बनाता है। छोटे पर रुकता है |
| `map2` | `(map2 a b f)` | `(Iter<A>,Iter<B>,(fn (A B) U))→Vector<U>` | कई अनुक्रमों पर CL का `mapcar`। छोटे पर रुकता है |
| `mapc` | `(mapc it f)` | `(Iter<A>,(fn (A) ()))→()` | साइड इफ़ेक्ट के लिए मैपिंग |
| `mapcan` | `(mapcan it f)` | `(Iter<A>,(fn (A) Vector<U>))→Vector<U>` | मैप करता है और जोड़ता है |
| `maplist` | `(maplist it f)` | `(Iter<A>,(fn (Vector<A>) U))→Vector<U>` | क्रमिक **टेल** पर मैप करता है |
| `mapl` | `(mapl it f)` | `(Iter<A>,(fn (Vector<A>) ()))→()` | साइड इफ़ेक्ट के लिए टेल पर मैप करता है (`mapc` का `maplist` समकक्ष) |
| `mapcon` | `(mapcon it f)` | `(Iter<A>,(fn (Vector<A>) Vector<U>))→Vector<U>` | टेल पर मैप करता है और जोड़ता है (`mapcan` का `maplist` समकक्ष) |
| `search` | `(search it sub)` | `(Iter<A>,Iter<A>)→Option<int>` where `Eq A` | वह स्थिति जहाँ `sub` पहली बार आता है। यदि रिसीवर `string` है, तो `string` मेथड चुना जाता है ([स्ट्रिंग](collections.md#1-स्ट्रिंग-string)) |
| `mismatch` | `(mismatch a b)` | `(Iter<A>,Iter<A>)→Option<int>` where `Eq A` | पहली स्थिति जहाँ वे भिन्न हैं। बराबर हों तो `none` |
| `merge` | `(merge a b less)` | `(Iter<A>,Iter<A>,(fn (A A) bool))→Vector<A>` | मर्ज करना। CL को क्रमबद्ध इनपुट चाहिए; यह जोड़े हुए को सॉर्ट करता है |
| `adjoin` | `(adjoin x it)` | `(A,Iter<A>)→Vector<A>` where `Eq A` | यदि `x` नहीं है तो उसे **सामने** जोड़ता है |
| `union` `intersection` `set-difference` `set-exclusive-or` | `(op a b)` | `(Iter<A>,Iter<A>)→Vector<A>` where `Eq A` | समुच्चय ऑपरेशन। CL क्रम निर्दिष्ट नहीं करता; यहाँ यह स्थिर है, **पहली आवृत्ति के क्रम में** |
| `subsetp` | `(subsetp a b)` | `(Iter<A>,Iter<A>)→bool` where `Eq A` | समावेश |
| `tailp` `ldiff` | `(tailp tail whole)` | `(Iter<A>,Iter<A>)→bool` / `→Vector<A>` | क्या यह प्रत्यय है / प्रत्यय से पहले का भाग। CL **साझा संरचना** के बारे में पूछता है, लेकिन साझा करने को कोई संरचना नहीं है, इसलिए यह प्रत्यय के बारे में **मानों के रूप में** पूछता है |
| `seq-equals` | `(seq-equals a b)` | `(Vector<A>,Vector<A>)→bool` where `Eq A` | तत्व-दर-तत्व समानता। `Vector<T>` स्वयं `Eq` लागू नहीं करता |
| `caar`…`cddddr` | `(cadr p)` | नेस्टेड जोड़ियों पर | CL के 28 फ़ंक्शन। वे **जोड़ियों पर चलते हैं, सूचियों पर नहीं**: `cadr` `cons-cell<A,cons-cell<B,C>>` लेता है |

जो CL में है और इस भाषा में नहीं: `list*` (बदली हुई टेल वाली अनुचित सूची की धारणा नहीं है), `copy-tree`/`copy-alist`/`sublis`/`subst`/`subst-if` (कोई टाइप मनमानी गहराई के विषमजातीय वृक्ष पर चलने का वर्णन नहीं कर सकता; `Sexpr` के वृक्ष के लिए, `equal` `tree-equal` के अनुरूप है), प्रॉपर्टी सूची परिवार `getf`/`get-properties`/`symbol-plist`/`remprop` (कुंजियों और मानों को बारी-बारी रखने वाली बिना टाइप की सूची का प्रतिनिधित्व नहीं है; `assoc` (असोसिएशन सूचियाँ) या `HashTable` वही भूमिका निभाते हैं), और `Vector<T>` तथा `Sexpr` सूचियों के बीच बदलने वाले फ़ंक्शन (`Sexpr` सूची के तत्वों के टाइप हर एक अलग हो सकते हैं, इसलिए उन्हें एकल तत्व टाइप `T` से नहीं लिखा जा सकता)।

## 6. कीवर्ड आर्ग्युमेंट

अध्याय 4 और 5 के फ़ंक्शन CL के अनुक्रम कीवर्ड `:key` / `:test` / `:test-not` / `:start` / `:end` / `:from-end` / `:count` लेते हैं। सभी **वैकल्पिक** हैं।

| कीवर्ड | टाइप | अर्थ |
|---|---|---|
| `:key` | `(fn (A) A)` | तुलना या जाँच से पहले हर तत्व पर लगाया जाने वाला प्रक्षेपण |
| `:test` | `(fn (A A) bool)` | `Eq` बाउंड के `equals` की जगह उपयोग होने वाला समानता परीक्षण। पहला आर्ग्युमेंट **खोजा जा रहा आइटम** है, दूसरा तत्व है (`:key` के बाद), CL के समान क्रम में |
| `:test-not` | `(fn (A A) bool)` | `:test` का निषेध |
| `:start` `:end` | `int` | स्कैन करने की विंडो `[start, end)`। इंडेक्स पूरे अनुक्रम के सापेक्ष हैं |
| `:from-end` | `bool` | खोज **अंतिम** मेल से उत्तर देती है। `:count` के साथ मिलाने पर, प्रभावित तत्व अंत से लिए जाते हैं |
| `:count` | `int` | `remove` / `substitute` परिवार जितने तत्वों को प्रभावित करते हैं उसकी अधिकतम संख्या |

कौन-सा फ़ंक्शन कौन-सा लेता है यह CL का अनुसरण करता है:

| फ़ंक्शन | लिए जाने वाले कीवर्ड |
|---|---|
| `find` `position` | `:key` `:test` `:test-not` `:start` `:end` `:from-end` |
| `count` | `:key` `:test` `:test-not` `:start` `:end` |
| `member` `adjoin` | `:key` `:test` `:test-not` |
| `remove` `substitute` | ऊपर के सभी (`:count` सहित) |
| `remove-duplicates` | `:key` `:test` `:test-not` `:start` `:end` `:from-end` |
| `find-if` `find-if-not` `position-if` `position-if-not` | `:key` `:start` `:end` `:from-end` |
| `count-if` `count-if-not` | `:key` `:start` `:end` |
| `member-if` `member-if-not` | `:key` |
| `remove-if` `remove-if-not` `substitute-if` | `:key` `:start` `:end` `:from-end` `:count` |
| `assoc` `rassoc` | `:key` `:test` `:test-not` (`assoc` का `:key` `car` पर लागू होता है, `rassoc` का `cdr` पर) |
| `assoc-if` `rassoc-if` | `:key` |
| `sort` `merge` | `:key` |
| `union` `intersection` `set-difference` `set-exclusive-or` `subsetp` | `:key` `:test` `:test-not` |
| `search` | `:key` `:test` `:test-not` `:from-end` `:start` `:end` `:sub-start` `:sub-end` |
| `mismatch` | `:key` `:test` `:test-not` `:from-end` `:start1` `:end1` `:start2` `:end2` |

```lisp
(find 2 (iter v) :key (lambda ((x int)) int (abs x)))   ; → (some -2)
(remove 2 (iter v) :count 1 :from-end true)             ; केवल एक हटाता है, अंत से
(position 3 (iter v) :start 1)                          ; इंडेक्स पूरे अनुक्रम के सापेक्ष है
```

**CL से अंतर**:

1. **`:key` का प्रक्षेपण तत्व टाइप के भीतर रहता है** (`(fn (A) A)`)। यह CL की तरह दूसरे टाइप में प्रक्षेपित नहीं कर सकता: आर्ग्युमेंट छोड़े जाने पर अतिरिक्त टाइप वेरिएबल निर्धारित नहीं हो सकता था। जहाँ भिन्न टाइप में प्रक्षेपण चाहिए, वहाँ इसकी बजाय `-if` परिवार को lambda पास करें (`(find-if it (lambda ((p ...)) bool (= (car p) 3)))`)।
2. **आइटम-आधारित खोजों में, `:key` केवल तत्वों पर लागू होता है** (खोजे जा रहे आइटम पर नहीं)। यह CL के `find`/`position`/`count`/`member`/`remove`/`substitute` जैसा ही नियम है। समुच्चय ऑपरेशन में दोनों पक्ष तत्व हैं, इसलिए वह दोनों पर लागू होता है।
3. **केवल `search` के कीवर्ड क्रमांकित नहीं बल्कि नामित हैं।** CL में, `:start1`/`:end1` **पैटर्न** के लिए और `:start2`/`:end2` खोजे जा रहे अनुक्रम के लिए हैं। इस भाषा में रिसीवर पहले आता है, इसलिए वही क्रमांक उलटा अर्थ रखते, और चुपचाप। `:start`/`:end` रिसीवर के लिए और `:sub-start`/`:sub-end` पैटर्न के लिए हैं, इसलिए अनजाने में लिखा `:start1` "unknown keyword" त्रुटि देता है। `mismatch` और `replace` का आर्ग्युमेंट क्रम CL जैसा ही है, इसलिए वे CL के क्रमांक रखते हैं।

## 7. विनाशकारी ऑपरेशन

`Vector<T>` के मेथड। **वे रिसीवर को बदलते हैं और स्वयं रिसीवर को लौटाते हैं**, इसलिए `(nreverse v)` `reverse` जैसे ही लिखा जाता है और `v` स्वयं भी उलट जाता है।

| नाम | फ़ॉर्म | विवरण |
|---|---|---|
| `nreverse` | `(nreverse v)` | जगह पर उलटता है |
| `delete` `delete-if` `delete-if-not` `delete-duplicates` | `(delete v x)` | `remove` / `remove-if` / `filter` / `remove-duplicates` के जगह-पर संस्करण |
| `nsubstitute` `nsubstitute-if` | `(nsubstitute v new old)` | `substitute` परिवार के जगह-पर संस्करण |
| `nbutlast` | `(nbutlast v)` | अंतिम तत्व हटाता है |
| `fill` | `(fill v x)` | हर तत्व को `x` पर सेट करता है। लंबाई नहीं बदलती |
| `replace` | `(replace v src)` | `src` के तत्वों से आगे से अधिलेखित करता है। `(min (len v) (len src))` तत्व |
| `map-into` | `(map-into v src f)` | `v[i] = (f src[i])`। ऊपर जितनी ही संख्या |
| `nconc` | `(nconc v w)` | `w` के तत्व `v` में जोड़ता है। CL के विपरीत, **यह साझा संरचना को नहीं बदलता** (`w` प्रभावित नहीं होता) |
| `nreconc` | `(nreconc v w)` | `(nconc (nreverse v) w)` |
| `set-contents` | `(set-contents v src)` | `v` की सामग्री को `src` से बदलता है (लंबाई भी बदलती है) |
| `rplaca` `rplacd` | `(rplaca p x)` | `cons-cell` का `car`/`cdr` बदलता है और सेल को ही लौटाता है |

लिए जाने वाले कीवर्ड:

| विनाशकारी संस्करण | लिए जाने वाले कीवर्ड |
|---|---|
| `delete` `nsubstitute` | `:key` `:test` `:test-not` `:start` `:end` `:from-end` `:count` |
| `delete-if` `delete-if-not` `nsubstitute-if` | `:key` `:start` `:end` `:from-end` `:count` |
| `delete-duplicates` | `:key` `:test` `:test-not` `:start` `:end` `:from-end` |
| `fill` | `:start` `:end` |
| `replace` | `:start1` `:end1` `:start2` `:end2` (रिसीवर CL का `sequence-1` है) |

`vector-push-extend`/`vector-pop` केवल `Vector<T>` के `push`/`pop` हैं। `Vector<T>` हमेशा बढ़ता है, इसलिए CL के "fill pointer वाले वेक्टर" और "सरल वेक्टर" के भेद का कोई समकक्ष नहीं है।

## 8. उच्च-क्रम फ़ंक्शन

| नाम | फ़ॉर्म | टाइप | विवरण |
|---|---|---|---|
| `identity` | `(identity x)` | `T→T` | अपना आर्ग्युमेंट लौटाता है |
| `const` | `(const x y)` | `(A,B)→A` | पहला आर्ग्युमेंट लौटाता है |
| `compose` | `(compose f g)` | `((fn (B) C),(fn (A) B))→(fn (A) C)` | फ़ंक्शन संयोजन `f∘g` |
| `flip` | `(flip f)` | `(fn (A B) C)→(fn (B A) C)` | दो-आर्ग्युमेंट फ़ंक्शन के आर्ग्युमेंट की अदला-बदली करता है |
| `complement` | `(complement pred)` | `(fn (A) bool)→(fn (A) bool)` | प्रेडिकेट का निषेध |

CL का `constantly` नहीं है (अनदेखे आर्ग्युमेंट का टाइप केवल रिटर्न टाइप में आता और निर्धारित नहीं हो सकता)। `(lambda ((x T)) A v)` लिखें।
