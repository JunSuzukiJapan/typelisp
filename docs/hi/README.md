<!-- translated-from: docs/ja/README.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# typelisp दस्तावेज़ (हिन्दी)

typelisp एक स्टैटिक रूप से टाइप किया गया Lisp है। इसे इंस्टॉल और बिल्ड करने के लिए रिपॉज़िटरी के शीर्ष स्तर पर मौजूद
[README.md](../../README.md) (अंग्रेज़ी में) देखें।

## ट्यूटोरियल

यदि आप typelisp से पहली बार परिचित हो रहे हैं, तो इन्हें इसी क्रम में पढ़ें।

- [शुरुआत करना](tutorial/intro.md): REPL, फ़ंक्शन, वेरिएबल, कंडीशनल, लूप, सूचियाँ और `Vector`
- [टाइप की बुनियादी बातें](tutorial/types.md): स्टैटिक टाइप, `Option`, `Result`, struct, enum, जेनेरिक
- [Trait](tutorial/traits.md): `deftrait` / `impl`, trait बाउंड, `:dyn`
- [Macro](tutorial/macros.md): `defmacro`, क्वासिक्वोट, `gensym`, `macrolet`
- [त्रुटि प्रबंधन](tutorial/errors.md): `Result`, `panic`, `catch` / `throw`, `unwind-protect`
- [कंकरेंसी](tutorial/concurrency.md): टास्क, चैनल, `select`, `Mutex`, `thread`

## गाइड

- [मॉड्यूल और फ़ाइल संरचना](guide/modules.md): `use`, `pub`, फ़ाइलें मॉड्यूल से कैसे मेल खाती हैं
- [कंपाइल करना](guide/compile.md): JIT, AOT कंपाइलेशन से एक्ज़ीक्यूटेबल बनाना, डंप
- [फ़ाइल I/O, स्ट्रीम और नेटवर्किंग](guide/io.md): फ़ाइलें, पाथनेम, TCP / TLS / UDP, नाम समाधान
- [C FFI](guide/ffi.md): `defffi` से C फ़ंक्शन कॉल करना (कॉलबैक और `def-c-struct` वाले C struct सहित)
- [एडिटर इंटीग्रेशन](guide/editors.md): `typl-lsp` और VS Code / Emacs की सेटिंग
- [Common Lisp प्रोग्रामरों के लिए](guide/from-common-lisp.md): typelisp CL से कैसे भिन्न है और CL कोड को कैसे दोबारा लिखें

## संदर्भ

- [सिंटैक्स संदर्भ](reference/syntax.md): लेक्सिकल सिंटैक्स, टाइप लिखना, परिभाषाएँ, नियंत्रण फ़ॉर्म, कंपाइलेशन, कंकरेंसी
- [बिल्ट-इन फ़ंक्शन](reference/functions/README.md): बिल्ट-इन फ़ंक्शन, मेथड और स्टैंडर्ड लाइब्रेरी
- [टाइप](reference/types.md): टाइप और हर टाइप द्वारा लागू किए गए trait
- [त्रुटि संदेश](reference/errors.md): आम त्रुटियों का अर्थ और उन्हें कैसे ठीक करें

## एडिटर इंटीग्रेशन

सेटअप के निर्देश [एडिटर इंटीग्रेशन गाइड](guide/editors.md) में हैं। हर एडिटर के की बाइंडिंग और
सेटिंग इन दस्तावेज़ों में सूचीबद्ध हैं:

- [Emacs (typelisp-mode)](../../editor/emacs/README_hi.md)
- [VS Code](../../editor/vscode/README_hi.md)
