<!-- translated-from: docs/ja/reference/types.md @ 0f8a35599b7916036b773d6cb777b7de28762078 -->
# टाइप

typelisp में मौजूद टाइप, और हर टाइप द्वारा लागू किए गए स्टैंडर्ड trait। टाइप कैसे लिखें यह [सिंटैक्स संदर्भ अध्याय 2](syntax.md#2-टाइप-लिखना) में है; हर टाइप के फ़ंक्शन और मेथड [बिल्ट-इन फ़ंक्शन](functions/README.md) में हैं।

## 1. आदिम टाइप

| टाइप | सामग्री | विवरण |
|---|---|---|
| `int` | मनमानी सटीकता वाला पूर्णांक। जब तक 63 बिट में समाता है तब तक तत्काल मान के रूप में रखा जाता है, और उससे आगे अपने-आप bignum बन जाता है। बिना एनोटेशन वाले पूर्णांक लिटरल का डिफ़ॉल्ट टाइप | [संख्याएँ अध्याय 3](functions/numbers.md#3-मनमानी-सटीकता-वाले-पूर्णांक-int) |
| `i8` `i16` `i32` | चिह्नित निश्चित चौड़ाई वाले पूर्णांक | [संख्याएँ अध्याय 1](functions/numbers.md#1-निश्चित-चौड़ाई-वाले-पूर्णांक) |
| `u8` `u16` `u32` | अचिह्नित निश्चित चौड़ाई वाले पूर्णांक | ऊपर जैसा |
| `f32` `f64` | IEEE-754 फ़्लोटिंग-पॉइंट संख्याएँ। दशमलव लिटरल का डिफ़ॉल्ट `f64` है | [संख्याएँ अध्याय 4](functions/numbers.md#4-फ़्लोटिंग-पॉइंट-संख्याएँ-f64--f32) |
| `ratio` | न्यूनतम रूप में एक परिमेय संख्या | [संख्याएँ अध्याय 5](functions/numbers.md#5-परिमेय-संख्याएँ-ratio) |
| `bool` | `true` / `false` | [संख्याएँ अध्याय 7](functions/numbers.md#7-बूलियन) |
| `char` | एक Unicode स्केलर मान | [कैरेक्टर](functions/collections.md#2-कैरेक्टर-char) |
| `string` | एक अपरिवर्तनीय स्ट्रिंग | [स्ट्रिंग](functions/collections.md#1-स्ट्रिंग-string) |
| `symbol` | एक सिंबल। कीवर्ड (`:name`) का भी यही टाइप है | [सिंबल](functions/sequences.md#3-सिंबल) |
| `()` | Unit टाइप। इसका मान भी `()` है | |
| `!` | Never टाइप। `panic` जैसे न लौटने वाले एक्सप्रेशन का टाइप। जहाँ कोई भी टाइप अपेक्षित हो वहाँ रखा जा सकता है | |
| `ptr` `c-long` `c-ulong` | केवल C को और C से मान पास करने के लिए उपयोग होने वाले शब्द। वे केवल `unsafe` के भीतर मान हो सकते हैं, और जहाँ वे आ सकते हैं वह सीमित है | [संख्याएँ अध्याय 2](functions/numbers.md#2-c-सीमा-पर-कच्चे-शब्द-ptr--c-long--c-ulong) |
| `random-state` | यादृच्छिक संख्या जनरेटर की अवस्था | [संख्याएँ अध्याय 12](functions/numbers.md#12-यादृच्छिक-संख्याएँ) |

64-बिट पूर्णांक टाइप नहीं है। जिन पूर्णांकों की चौड़ाई मायने नहीं रखती उनके लिए `int` का उपयोग करें।

## 2. बिल्ट-इन जेनेरिक टाइप

| टाइप | सामग्री | विवरण |
|---|---|---|
| `Option<T>` | ऐसा मान जो है या नहीं है। `some` / `none` | [Option और Result](functions/option-result.md) |
| `Result<T,E>` | सफलता या विफलता। `ok` / `err` | ऊपर जैसा |
| `Vector<T>` | बढ़ने योग्य ऐरे | [Vector](functions/collections.md#3-vectort) |
| `HashTable<K,V>` | हैश टेबल। कुंजी टाइप को `Hash` लागू करना चाहिए | [HashTable](functions/collections.md#4-hashtablekv) |
| `#{T0 T1 ...}` | टपल (1 से 12 तत्व)। तत्व `t::0` से पढ़े जाते हैं | [सिंटैक्स अध्याय 2](syntax.md#2-टाइप-लिखना) |
| `Task<T>` | टास्क का हैंडल | [टास्क](functions/concurrency.md#1-taskt--टास्क-के-हैंडल) |
| `Thread<T>` | समर्पित OS थ्रेड पर चल रहे टास्क का हैंडल | [Thread](functions/concurrency.md#7-threadt--समर्पित-os-थ्रेड) |
| `Chan<T>` | चैनल | [चैनल](functions/concurrency.md#2-chant--चैनल) |

फ़ंक्शन टाइप `(fn (argument-types...) return-type)` के रूप में लिखे जाते हैं, और trait ऑब्जेक्ट `:dyn Trait` के रूप में ([सिंटैक्स संदर्भ अध्याय 2](syntax.md#2-टाइप-लिखना))।

## 3. S-एक्सप्रेशन डेटा

| टाइप | सामग्री | विवरण |
|---|---|---|
| `Sexpr` | एक गैर-खाली S-एक्सप्रेशन। 19 variant: `int`, `i8` से `u32`, `f32`, `f64`, `char`, `bool`, `sym`, `str`, `cons`, `ratio`, `path`, `vector`, `array`, `tuple` | [S-एक्सप्रेशन डेटा](functions/sequences.md#2-s-एक्सप्रेशन-डेटा-sexpr) |
| `Option<Sexpr>` | सामान्य रूप से S-एक्सप्रेशन डेटा। खाली सूची `()` `none` है | ऊपर जैसा |

## 4. स्टैंडर्ड लाइब्रेरी के टाइप

वे टाइप जो स्टैंडर्ड लाइब्रेरी (prelude) `defstruct` / `defenum` से परिभाषित करती है। उन्हें आपके अपने लिखे टाइप जैसा ही माना जाता है, और `defstruct` के साथ जो कुछ किया जा सकता है वह उनके साथ भी किया जा सकता है।

| टाइप | सामग्री | विवरण |
|---|---|---|
| `cons-cell<A,B>` | एक जोड़ी। `cons`/`car`/`cdr` | [जोड़ियाँ](functions/sequences.md#1-जोड़ियाँ-cons-cellab) |
| `complex` | एक कॉम्प्लेक्स संख्या (`f64` घटक) | [संख्याएँ अध्याय 6](functions/numbers.md#6-कॉम्प्लेक्स-संख्याएँ-complex) |
| `Array<T>` | बहुआयामी ऐरे | [Array](functions/collections.md#5-arrayt-बहुआयामी-ऐरे) |
| `BitVector` | बिट का निश्चित लंबाई वाला अनुक्रम | [BitVector](functions/collections.md#6-bitvector-बिट-वेक्टर) |
| `HashSet<T>` | बिना दोहराव वाले तत्वों का संग्रह | [HashSet](functions/collections.md#7-hashsett) |
| `SortedTable<K,V>` | कुंजी के क्रम में रखी तालिका | [SortedTable](functions/collections.md#8-sortedtablekv) |
| `Deque<T>` | दोनों सिरों से डालने और निकालने वाला अनुक्रम | [Deque](functions/collections.md#9-dequet) |
| `vector-iter<T>` `hashtable-iter<K,V>` `array-iter<T>` | हर कलेक्शन के `iter` द्वारा लौटाए जाने वाले इटरेटर | [Iter](functions/traits.md#1-iter-trait-और-इटरेशन) |
| `lazy::map-iter<I,A,U>` आदि | `lazy` मॉड्यूल के फ़ंक्शन जो इटरेटर लौटाते हैं | [आलसी इटरेटर](functions/sequences.md#आलसी-इटरेटर-lazy-मॉड्यूल) |
| `WaitGroup` | N चीज़ों के समाप्त होने की प्रतीक्षा | [WaitGroup](functions/concurrency.md#4-waitgroup--n-के-पूर्ण-होने-की-प्रतीक्षा) |
| `Mutex<T>` | साझा डेटा के लिए पारस्परिक अपवर्जन | [Mutex](functions/concurrency.md#6-mutext--साझा-डेटा-के-लिए-पारस्परिक-अपवर्जन) |
| `pathname` | हिस्सों में बँटा फ़ाइल का नाम | [पाथनेम](functions/streams-files.md#9-पाथनेम-pathname) |
| `file-stream` `binary-file-stream` `string-input-stream` `string-output-stream` `standard-stream` | स्ट्रीम | [स्ट्रीम](functions/streams-files.md#3-ठोस-स्ट्रीम-टाइप) |
| `broadcast-stream` `two-way-stream` `echo-stream` `concatenated-stream` `peek-stream` | संयुक्त स्ट्रीम | [संयुक्त स्ट्रीम](functions/streams-files.md#4-संयुक्त-स्ट्रीम) |
| `socket-stream` `socket-byte-stream` `socket-listener` `udp-socket` `datagram` | नेटवर्किंग | [नेटवर्किंग](functions/network.md#1-टाइप) |
| `ReadOutcome` | `read-sexpr` का परिणाम। `datum` / `eof` | [स्ट्रीम](functions/streams-files.md#6-जेनेरिक-फ़ंक्शन-और-फ़ाइल-ऑपरेशन) |
| `universal-time` `internal-time` `decoded-time` | समय | [समय](functions/system.md#1-समय) |
| `heap-info` | हीप की वर्तमान अवस्था | [इम्प्लीमेंटेशन उपकरण](functions/system.md#51-heap-info-के-फ़ील्ड) |

## 5. त्रुटि टाइप

`Error` टाइप नहीं बल्कि trait है, और निम्नलिखित टाइप उसे लागू करते हैं। किसी भी प्रकार की त्रुटियों को संभालने के लिए `:dyn Error` लिखें।

| टाइप | किससे उत्पन्न होता है |
|---|---|
| `ParseIntError` | `parse-int` |
| `ParseFloatError` | `parse-float` |
| `ReadError` | `read` / `read-from-string` / `read-sexpr` |
| `EvalError` | `eval` / `macroexpand` |
| `FileError` | फ़ाइल और स्ट्रीम ऑपरेशन |
| `NetError` | नेटवर्क ऑपरेशन |
| `SimpleError` | `simple-error` |
| `WrappedError` | `wrap-error` |

विवरण [त्रुटि टाइप और Error trait](functions/option-result.md#3-त्रुटि-टाइप-और-error-trait) में हैं।

## 6. स्टैंडर्ड trait के इम्प्लीमेंटेशन

कौन-से टाइप कौन-से trait को लागू करते हैं। हर trait के मेथड [स्टैंडर्ड trait](functions/traits.md) में और सबसे दाएँ कॉलम में सूचीबद्ध अध्यायों में हैं।

### 6.1 तुलना, हैशिंग और प्रिंटिंग

| Trait | लागू करने वाले टाइप | विवरण |
|---|---|---|
| `Eq` | `int` `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `ratio` `complex` `bool` `char` `string` `symbol` `Sexpr` `cons-cell<A,B>` `#{..}` | [Eq / Ord](functions/traits.md#2-eq--ord-तुलना) |
| `Ord` | `int` `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `ratio` `char` `string` `cons-cell<A,B>` `#{..}` | ऊपर जैसा |
| `Hash` | `int` `i8` `i16` `i32` `u8` `u16` `u32` `bool` `char` `string` `symbol` `#{..}` | [HashTable](functions/collections.md#4-hashtablekv) |
| `print-object` | `int` `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `ratio` `complex` `bool` `char` `string` `symbol` `#{..}` `Array<T>` `HashSet<T>` `SortedTable<K,V>` `Deque<T>` `pathname` `universal-time` `internal-time` और सभी बिल्ट-इन त्रुटि टाइप | [print-object](functions/printing.md#5-print-object-प्रति-टाइप-प्रिंट-किया-गया-प्रतिनिधित्व) |

`cons-cell<A,B>` और टपल `#{..}` के trait, और संग्रहों का `print-object`, तब उपलब्ध हैं जब तत्वों के टाइप वह trait लागू करते हों।

### 6.2 अंकगणित

| Trait | लागू करने वाले टाइप |
|---|---|
| `Add` `Sub` `Mul` `Div` `Rem` `Number` | `int` `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `ratio` |
| `Bits` | `int` `i8` `i16` `i32` `u8` `u16` `u32` |

विवरण [अंकगणितीय trait](functions/traits.md#3-अंकगणितीय-trait-add--sub--mul--div--rem--bits--number) में हैं।

### 6.3 इटरेशन

| Trait | लागू करने वाले टाइप |
|---|---|
| `Iter` | `vector-iter<T>` `hashtable-iter<K,V>` `array-iter<T>` `Chan<T>` `lazy` मॉड्यूल के टाइप (`lazy::map-iter<I,A,U>` आदि) |

### 6.4 स्ट्रीम

| टाइप | लागू किए गए trait |
|---|---|
| `file-stream` | `CharInput` `PeekInput` `CharOutput` |
| `binary-file-stream` | `ByteInput` `ByteOutput` |
| `string-input-stream` | `CharInput` `PeekInput` |
| `string-output-stream` | `CharOutput` |
| `standard-stream` | `CharInput` `PeekInput` `CharOutput` |
| `socket-stream` | `CharInput` `PeekInput` `CharOutput` |
| `socket-byte-stream` | `ByteInput` `ByteOutput` |
| `broadcast-stream` | `CharOutput` |
| `two-way-stream` | `CharInput` `CharOutput` |
| `echo-stream` | `CharInput` |
| `concatenated-stream` | `CharInput` |
| `peek-stream` | `CharInput` `PeekInput` |

हर स्ट्रीम `Stream` को लागू करती है; इनपुट स्ट्रीम `InputStream` भी लागू करती हैं, और आउटपुट स्ट्रीम `OutputStream`। `socket-listener` और `udp-socket` केवल `Stream` (`close` / `open-stream-p`) लागू करते हैं। विवरण [स्ट्रीम](functions/streams-files.md#1-trait-पदानुक्रम) में हैं।

### 6.5 अन्य

| Trait | लागू करने वाले टाइप | विवरण |
|---|---|---|
| `Error` | अध्याय 5 के सभी त्रुटि टाइप | [त्रुटि टाइप](functions/option-result.md#3-त्रुटि-टाइप-और-error-trait) |
| `Pathish` | `string` `pathname` | [पाथनेम](functions/streams-files.md#91-पाथनेम-डेज़िग्नेटर-trait-pathish) |
