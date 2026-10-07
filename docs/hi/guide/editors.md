<!-- translated-from: docs/ja/guide/editors.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# एडिटर इंटीग्रेशन (typl-lsp)

`typl-lsp` typelisp का भाषा सर्वर है। LSP (Language Server Protocol) समर्थित एडिटर से जुड़ने पर, यह आपके संपादित फ़ाइल के लिए ये सुविधाएँ देता है:

- डायग्नोस्टिक: पढ़ने की त्रुटियाँ, टाइप त्रुटियाँ और पुनर्परिभाषा चेतावनियाँ
- Hover: कोष्ठक वाले एक्सप्रेशन का टाइप और जिस परिभाषा को वह कॉल करता है उसकी डॉकस्ट्रिंग (नंगे वेरिएबल नामों के लिए नहीं दिखाया जाता)
- परिभाषा पर जाना
- कम्प्लीशन (`:` टाइप करने पर उम्मीदवार दिखते हैं)
- टाइप नामों की रंगाई (सिमैंटिक टोकन), दूसरी फ़ाइलों से `use` किए गए टाइप सहित

`use` के ज़रिए फ़ाइलों के पार संदर्भ हल होते हैं। किसी दूसरी खुली फ़ाइल के बिना सहेजे बदलाव उसे `use` करने वाली फ़ाइलों के डायग्नोस्टिक में तुरंत दिखते हैं।

## 1. बिल्ड करना

```sh
cargo build --release --bin typl-lsp
```

इससे `target/release/typl-lsp` बनता है। यदि आपने [README.md](../../../README.md) में बताए अनुसार `cargo install` से इंस्टॉल किया है, तो वह `typl` के साथ `~/.cargo/bin/typl-lsp` में है।

## 2. VS Code

एक्सटेंशन रिपॉज़िटरी के `editor/vscode` में है। यह Marketplace पर प्रकाशित नहीं है, इसलिए इसे स्वयं बिल्ड और इंस्टॉल करें।

```sh
cd editor/vscode
npm install
npm run compile
npx @vscode/vsce package      # .vsix बनाता है
```

Extensions व्यू के "..." मेनू से "Install from VSIX..." चुनें और अपना बनाया `.vsix` चुनें।

एक्सटेंशन `typl-lsp` को वर्कस्पेस के `target/release/typl-lsp` में, फिर `target/debug/typl-lsp` में, फिर `PATH` पर खोजता है। यदि आपने उसे कहीं और रखा है, तो उसका पाथ सेटिंग `typelisp.languageServer.path` में लिखें।

| सेटिंग | डिफ़ॉल्ट | अर्थ |
|---|---|---|
| `typelisp.program` | `typl` | `typl` का पाथ |
| `typelisp.languageServer.enable` | `true` | क्या `typl-lsp` से जुड़ना है |
| `typelisp.languageServer.path` | (खाली) | `typl-lsp` का पाथ |

`Ctrl+Alt+R` संपादित फ़ाइल को सहेजता है और उसे `typl` से चलाता है, और `Ctrl+Alt+Z` REPL शुरू करता है। अधिक के लिए [VS Code एक्सटेंशन का README](../../../editor/vscode/README_hi.md) देखें।

## 3. Emacs

`typelisp-mode` रिपॉज़िटरी के `editor/emacs` में है।

```elisp
(add-to-list 'load-path "/path/to/typelisp/editor/emacs")
(require 'typelisp-mode)
```

`eglot` (Emacs 29 और बाद के साथ बंडल) से `typl-lsp` से जुड़ने की सेटिंग:

```elisp
(with-eval-after-load 'eglot
  (add-to-list 'eglot-server-programs
               '(typelisp-mode . ("/path/to/typelisp/target/release/typl-lsp"))))
(add-hook 'typelisp-mode-hook #'eglot-ensure)
```

`lsp-mode` के साथ:

```elisp
(with-eval-after-load 'lsp-mode
  (lsp-register-client
   (make-lsp-client
    :new-connection (lsp-stdio-connection "/path/to/typelisp/target/release/typl-lsp")
    :major-modes '(typelisp-mode)
    :server-id 'typl-lsp)))
(add-hook 'typelisp-mode-hook #'lsp)
```

Emacs 31 और बाद का eglot टाइप नामों को (सिमैंटिक टोकन) स्वयं रंगता है। Emacs 30 और पहले का eglot उन्हें सपोर्ट नहीं करता, इसलिए इसकी जगह `typelisp-mode` टाइप नामों को रंगता है। `lsp-mode` के साथ, `lsp-semantic-tokens-enable` को `t` पर सेट करें।

`C-c C-c` संपादित फ़ाइल को चलाता है, और `C-c C-z` REPL शुरू करता है। अधिक के लिए [typelisp-mode का README](../../../editor/emacs/README_hi.md) देखें।

## 4. अन्य एडिटर

`typl-lsp` स्टैंडर्ड इनपुट और आउटपुट पर LSP बोलता है और कोई कमांड-लाइन आर्ग्युमेंट नहीं लेता। अपने एडिटर के LSP क्लाइंट को `.typl` फ़ाइलों के लिए `typl-lsp` शुरू करने के लिए कॉन्फ़िगर करें।

## 5. प्रोजेक्ट कैसे पहचाने जाते हैं

`typl-lsp` खोली गई फ़ाइल की डायरेक्टरी से शुरू करके ऊपर की ओर `typelisp.toml` खोजता है, और उस जगह को सोर्स रूट मानकर `use` हल करता है। ये वही नियम हैं जो `typl` के फ़ाइल चलाने पर होते हैं ([मॉड्यूल और फ़ाइल संरचना](modules.md#2-प्रोजेक्ट-सेट-करना))। कई फ़ाइलों से बने प्रोजेक्ट के लिए, `typelisp.toml` को उसके रूट पर रखें।

## 6. भाषा सर्वर आपका प्रोग्राम नहीं चलाता

`typl-lsp` केवल पढ़कर और टाइप जाँचकर डायग्नोस्टिक बनाता है। यह आपके संपादित प्रोग्राम को कभी नहीं चलाता। डायग्नोस्टिक हर कीस्ट्रोक पर चलते हैं, इसलिए वहाँ साइड इफ़ेक्ट वाला कोड या कभी समाप्त न होने वाला कोड चलाना संभव नहीं। एकमात्र अपवाद `defmacro` का पंजीकरण है, जो उनके बाद आने वाले macro कॉल की जाँच के लिए ज़रूरी है।

इसी कारण, जो त्रुटियाँ केवल तब होती हैं जब `typl` प्रोग्राम चलाता है (`panic`, गायब फ़ाइल आदि) वे भाषा सर्वर के डायग्नोस्टिक में नहीं दिखतीं।
