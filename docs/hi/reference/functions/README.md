<!-- translated-from: docs/ja/reference/functions/README.md @ 1a01065673fd9d568c138bb444c88c5345552937 -->
# बिल्ट-इन फ़ंक्शन

बिल्ट-इन फ़ंक्शन, मेथड और स्टैंडर्ड लाइब्रेरी की सूची। सिंटैक्स (विशेष फ़ॉर्म और चीज़ें कैसे परिभाषित करें) के लिए [सिंटैक्स संदर्भ](../syntax.md) देखें; टाइप की सूची के लिए [टाइप](../types.md) देखें।

## कॉल के रूप

कॉल के तीन रूप हैं।

- मुक्त फ़ंक्शन: `(name args...)`
- इंस्टेंस मेथड: `(name receiver args...)` (पहले आर्ग्युमेंट के स्टैटिक टाइप से हल होता है)
- स्टैटिक मेथड (असोसिएटेड फ़ंक्शन): `(Type::name args...)`

हर टाइप का समान नाम का अपना मेथड हो सकता है। `(+ a b)` `a` के टाइप का `+` कॉल करता है।

## तालिकाएँ पढ़ना

हर अध्याय की तालिकाओं में स्तंभ "नाम, फ़ॉर्म, टाइप, विवरण" हैं। टाइप स्तंभ `(argument-type,...)→return-type` के रूप में लिखा जाता है।

- `T`, `A` या `B` जैसा एकल बड़ा अक्षर टाइप वेरिएबल है।
- `where Eq A` जैसा नोट trait बाउंड है जिसे टाइप वेरिएबल को पूरा करना होगा।
- `Iter<A>` का अर्थ है "`Iter` का कोई भी इम्प्लीमेंटेशन जिसका `Item` `A` है"।
- `&optional` / `&key` से चिह्नित आर्ग्युमेंट छोड़े जा सकते हैं।

## अध्याय

| फ़ाइल | सामग्री |
|---|---|
| [numbers.md](numbers.md) | पूर्णांक, फ़्लोटिंग-पॉइंट संख्याएँ, परिमेय संख्याएँ, कॉम्प्लेक्स संख्याएँ, बूलियन, बिट ऑपरेशन, यादृच्छिक संख्याएँ |
| [sequences.md](sequences.md) | जोड़ी `cons-cell`, S-एक्सप्रेशन डेटा `Sexpr`, सिंबल, अनुक्रम फ़ंक्शन, आलसी इटरेटर `lazy`, उच्च-क्रम फ़ंक्शन |
| [collections.md](collections.md) | स्ट्रिंग, कैरेक्टर, `Vector`, `HashTable`, `Array`, `BitVector`, `HashSet`, `SortedTable`, `Deque` |
| [option-result.md](option-result.md) | `Option`, `Result`, त्रुटि टाइप और `Error` trait |
| [traits.md](traits.md) | `Iter`, `Eq`/`Ord`, अंकगणितीय trait |
| [printing.md](printing.md) | `print`/`println`/`format`, सुंदर प्रिंटर, `print-object`, प्रिंटर नियंत्रण वेरिएबल |
| [format.md](format.md) | फ़ॉर्मैट निर्देश |
| [streams-files.md](streams-files.md) | स्ट्रीम, फ़ाइल ऑपरेशन, पाथनेम, readtable |
| [concurrency.md](concurrency.md) | टास्क, चैनल, `WaitGroup`, `Mutex`, `Thread`, `Context` |
| [network.md](network.md) | TCP, TLS, Unix डोमेन सॉकेट, UDP |
| [system.md](system.md) | समय, रनटाइम पर्यावरण, इम्प्लीमेंटेशन उपकरण, `read`/`eval`, डॉकस्ट्रिंग, macro-संबंधित फ़ंक्शन |
