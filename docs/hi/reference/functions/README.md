<!-- translated-from: docs/ja/reference/functions/README.md @ 2d61c22ad717f8ad6dfd178f397927e87e1d8902 -->
# बिल्ट-इन फ़ंक्शन

बिल्ट-इन फ़ंक्शन, मेथड और स्टैंडर्ड लाइब्रेरी की सूची। सिंटैक्स (विशेष फ़ॉर्म और चीज़ें कैसे परिभाषित करें) के लिए [सिंटैक्स संदर्भ](../syntax.md) देखें; टाइप की सूची के लिए [टाइप](../types.md) देखें।

## कॉल के रूप

कॉल के तीन रूप हैं।

- मुक्त फ़ंक्शन: `(name args...)`
- इंस्टेंस मेथड: `(name receiver args...)` (पहले आर्ग्युमेंट के स्टैटिक टाइप से हल होता है)
- स्टैटिक मेथड (असोसिएटेड फ़ंक्शन): `(Type::name args...)`

हर टाइप का समान नाम का अपना मेथड हो सकता है। `(+ a b)` `a` के टाइप का `+` कॉल करता है।

## `use` चाहने वाले नाम

मानक लाइब्रेरी के नामों में से ये हर मॉड्यूल में कुछ भी लिखे बिना काम आते हैं
([सिंटैक्स संदर्भ 3.10](../syntax.md#310-module--use--नेमस्पेस)):

- मूल प्रकार, `Option`/`Result`/`Vector`, पुनरावृत्ति, S-अभिव्यक्तियाँ और सूचियाँ, अनुक्रम फ़ंक्शन,
  नियंत्रण मैक्रो, तुलना और अंकगणितीय trait
- हर वह नाम जो Common Lisp के `COMMON-LISP` पैकेज का भी नाम है (`length`, `format`, `open` आदि)
- `Error`, `assert`, `print-object`, `HashTable`, `Array`, `BitVector`, स्ट्रीम trait `Stream`,
  `InputStream` और `OutputStream`, और `Hash`, जिसे `HashTable` की कुंजियाँ लागू करती हैं
- वे प्रकार जिनका इनके फ़ंक्शनों और चरों में उल्लेख है

कोई भी दूसरा नाम `(use std::net::tcp-connect)` की तरह `use` से लाया जाता है, या उसके पथ
`std::net::tcp-connect` से लिखा जाता है। तालिका ये नाम और हर एक का मॉड्यूल बताती है।

| मॉड्यूल | नाम |
|---|---|
| `std::array` | `array-iter` |
| `std::char` | `digit->char`, `digit-weight`, `name->char` |
| `std::collections` | `Deque`, `HashSet`, `Scope`, `SortedTable`, `hashtable-iter` |
| `std::env` | `command-line-args`, `getenv`, `home-directory` |
| `std::error` | `WrappedError`, `as-dyn-error`, `assert-eq`, `describe-error`, `wrap-error` |
| `std::fmt` | `pprint-list-exhausted` |
| `std::fs` | `direction-append`, `direction-input`, `direction-output`, `directory-p`, `open-binary`, `open-binary-input`, `open-binary-output`, `open-file`, `open-input`, `open-output`, `read-file-lines`, `read-file-string`, `write-file-string` |
| `std::hash` | `*sxhash-mask*`, `sxhash-string` |
| `std::io` | `ByteInput`, `ByteOutput`, `PeekInput`, `binary-file-stream`, `copy-stream`, `make-peek-stream`, `peek-stream`, `read-lines`, `write-lines` |
| `std::net` | `NetError`, `byte-stream-of`, `char-stream-of`, `datagram`, `socket-byte-stream`, `socket-listener`, `socket-stream`, `tcp-connect`, `tcp-listen`, `tls-connect`, `tls-listen`, `udp-bind`, `udp-socket`, `unix-connect`, `unix-listen`, `with-connection` |
| `std::num` | `ParseFloatError`, `ParseIntError`, `atan2`, `parse-float`, `parse-int` |
| `std::path` | `Pathish`, `pathname-absolute-p` |
| `std::process` | `exit` |
| `std::random` | `seed-random-state` |
| `std::reader` | `ReadOutcome`, `read-from-string-preserving-whitespace`, `read-sexpr`, `read-sexpr-preserving-whitespace` |
| `std::repl` | `heap-info` |
| `std::string` | `left-trim`, `right-trim`, `string->utf8`, `trim`, `utf8->string` |
| `std::symbol` | `symbol->string` |
| `std::sync` | `Chan`, `Context`, `Mutex`, `WaitGroup`, `after`, `with-lock` |
| `std::thread` | `Task`, `Thread`, `yield` |
| `std::time` | `internal-time-seconds`, `timezone-daylight-p`, `timezone-offset-seconds` |

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
| [traits.md](traits.md) | `Iter`, `Eq`/`Ord`, अंकगणितीय trait, `Push` |
| [printing.md](printing.md) | `print`/`println`/`format`, सुंदर प्रिंटर, `print-object`, प्रिंटर नियंत्रण वेरिएबल |
| [format.md](format.md) | फ़ॉर्मैट निर्देश |
| [streams-files.md](streams-files.md) | स्ट्रीम, फ़ाइल ऑपरेशन, पाथनेम, readtable |
| [concurrency.md](concurrency.md) | टास्क, चैनल, `WaitGroup`, `Mutex`, `Thread`, `Context` |
| [network.md](network.md) | TCP, TLS, Unix डोमेन सॉकेट, UDP |
| [system.md](system.md) | समय, रनटाइम पर्यावरण, इम्प्लीमेंटेशन उपकरण, `read`/`eval`, डॉकस्ट्रिंग, macro-संबंधित फ़ंक्शन |
