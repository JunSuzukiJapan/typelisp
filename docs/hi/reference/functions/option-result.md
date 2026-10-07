<!-- translated-from: docs/ja/reference/functions/option-result.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Option, Result और त्रुटि टाइप

## 1. `Option<T>` / `Result<T,E>`

कंस्ट्रक्टर: `Option<T>` में `Some(T)` / `None` हैं। `Result<T,E>` में `Ok(T)` / `Err(E)` हैं। `E` कोई भी टाइप हो सकता है: बिल्ट-इन ठोस त्रुटि टाइप, और जो टाइप आप स्वयं `defstruct`/`defenum` से लिखते हैं, वे भी वहाँ ठीक उसी तरह फ़िट होते हैं (अध्याय 3)।

| नाम | फ़ॉर्म | Option | Result | विवरण |
|---|---|---|---|---|
| `unwrap` | `(unwrap self)` | `Option<T>→T` | `Result<T,E>→T` | मान बाहर निकालता है। `None`/`Err` पर panic करता है |
| `unwrap-or` | `(unwrap-or self default)` | `(Option<T>,T)→T` | `(Result<T,E>,T)→T` | मान, या डिफ़ॉल्ट |
| `is-some` | `(is-some self)` | `Option<T>→bool` | — | क्या यह `Some` है |
| `is-none` | `(is-none self)` | `Option<T>→bool` | — | क्या यह `None` है |
| `is-ok` | `(is-ok self)` | — | `Result<T,E>→bool` | क्या यह `Ok` है |
| `is-err` | `(is-err self)` | — | `Result<T,E>→bool` | क्या यह `Err` है |

कंस्ट्रक्टर `Option::some`/`Option::none`/`Result::ok`/`Result::err` हैं (या `(use option)`/`(use result)` के बाद, नंगे नाम `some`/`none`/`ok`/`err`)।

शाखाएँ `match` से स्पष्ट रूप से लिखी जाती हैं। Rust के `?` के अनुरूप कोई सिंटैक्स नहीं है।

## 2. `Option<T>` का रन-टाइम प्रतिनिधित्व

Rust की तरह, **`Option<T>` सामान्यतः कोई बॉक्स नहीं बनाता**। `some v` स्वयं `v` है और `none` खाली-सूची मान है, बिना आवंटन और बिना अप्रत्यक्षता के। `Option<Sexpr>` (जहाँ खाली सूची `none` है), `Option<int>`, `Option<string>`, `Option<my-struct>`, `Option<f64>` और `Option<(fn ...)>` सभी यही रूप लेते हैं।

बॉक्स केवल तब उपयोग होता है जब `T` के मान को खाली-सूची मान से अलग न पहचाना जा सके:

| `T` | प्रतिनिधित्व | कारण |
|---|---|---|
| `Option<U>` (नेस्टेड) | बॉक्स | भीतर का `none` बाहर के `none` जैसा ही मान होता |
| `()` | बॉक्स | `()` का मान स्वयं खाली-सूची मान है |
| `ptr` / `c-long` / `c-ulong` | बॉक्स | सभी 64 बिट मान हैं, अलग पहचानने के लिए कोई जगह नहीं बचती |
| बाकी कुछ भी | बॉक्स नहीं | — |

प्रतिनिधित्व केवल टाइप से तय होता है और मान से पढ़ा नहीं जा सकता। प्रिंट करते समय, `(some ...)`/`none` स्टैटिक टाइप से पुनर्निर्मित होता है, इसलिए `(format false "~a" opt)` `(some 1)` प्रिंट करता है। दो प्रतिबंध हैं:

- **इसे `:dyn Trait` में नहीं डाला जा सकता** (`Option<int>` के उस मान को `:dyn Speak` को पास करना जिसके लिए आपने `(impl Speak Option<int> ...)` लिखा है, त्रुटि है)।
- `Sexpr` से `(the Option<T> ...)` डाउनकास्ट **कंस्ट्रक्टर का नाम देता है**: `(the Option<int> (some x))` / `(the Option<int> (none))`। पूरे मान को बाँधने वाला रूप, `(the Option<int> o)`, त्रुटि है।

## 3. त्रुटि टाइप और `Error` trait

Rust के `std::error::Error` का अनुसरण करते हुए, **`Error` टाइप नहीं बल्कि trait है**। त्रुटियों को दर्शाने वाले ठोस टाइप हर उद्देश्य के लिए अलग हैं, और हर एक `Error` लागू करता है।

| टाइप | किससे उत्पन्न होता है |
|---|---|
| `ParseIntError` | `parse-int` |
| `ParseFloatError` | `parse-float` |
| `ReadError` | `read` / `read-from-string` / `read-sexpr` |
| `EvalError` | `eval` / `macroexpand` |
| `FileError` | फ़ाइल और स्ट्रीम ऑपरेशन ([स्ट्रीम और फ़ाइलें](streams-files.md)) |
| `NetError` | नेटवर्क ऑपरेशन ([नेटवर्किंग](network.md)) |
| `SimpleError` | `(SimpleError::new msg)` / `(simple-error msg)`। CL का `simple-error`: जब आप बस यह बताना चाहें कि क्या हुआ तब डिफ़ॉल्ट चुनाव |
| `WrappedError` | `(wrap-error msg cause)`। ऐसा टाइप जो आपका अपना संदेश और कारण दोनों रखता है; इसीलिए `Error` trait में `source` है |

`ParseIntError` से `NetError` तक हर एक "एक संदेश स्ट्रिंग रखने वाले एकल variant का enum" है, और टाइप का नाम और variant का नाम एक ही हैं (`(match e ((ParseIntError m) m))`, `(ParseIntError::ParseIntError "...")` से बनाया जाता है)। इनमें कुछ विशेष नहीं है: इन्हें ठीक आपके अपने `(defstruct my-err (...))` / `(defenum my-err ...)` से लिखे त्रुटि टाइप की तरह माना जाता है।

| नाम | फ़ॉर्म | टाइप | विवरण |
|---|---|---|---|
| `message` | `(message e)` | `Self→string` | त्रुटि संदेश (`Error` trait का मेथड) |
| `source` | `(source e)` | `Self→Option<:dyn Error>` | वह कारण जिसे यह त्रुटि लपेटती है, या यदि कोई नहीं तो `None` (Rust का `Error::source`) |
| `as-dyn-error` | `(as-dyn-error r)` | `Result<T,E>→Result<T,:dyn Error>` (`E` `Error` लागू करता है) | ठोस त्रुटि टाइप को trait ऑब्जेक्ट तक चौड़ा करता है |
| `describe-error` | `(describe-error e)` | `E→string` (`E` `Error` लागू करता है) | संदेश और `source` का अनुसरण करके मिली कारणों की शृंखला, हर पंक्ति में एक कारण। CL में कोई समकक्ष नहीं है (Rust का "caused by") |

यदि आप अपने त्रुटि टाइप के लिए `Error` लागू करते हैं, तो उसे बिल्ट-इन त्रुटियों जैसे **ठीक उसी तरह** संभाला जा सकता है:

```lisp
(defstruct io-err (path string))
(impl Error io-err
  (message ((self Self)) string (append "io failed: " self::path))
  (source ((self Self)) Option<:dyn Error> (option::none)))

(defun open-it ((p string)) Result<i32, io-err>          ; ठोस टाइप जस का तस E में जाता है
  (if (equal p "") (result::err (io-err::new "<empty>")) (result::ok 3)))

(defun describe ((e :dyn Error)) string (message e))     ; किसी भी प्रकार को एकसमान रूप से संभालें
(describe (io-err::new "/etc/app.conf"))
(describe (ParseIntError::ParseIntError "boom"))
```

कई त्रुटि टाइप को एक `Result` में इकट्ठा करने के लिए, `Result<T, :dyn Error>` का उपयोग करें (Rust के `Box<dyn Error>` के अनुरूप), और ठोस त्रुटियों को `as-dyn-error` से चौड़ा करें। चूँकि `?` नहीं है, यह रूपांतरण स्पष्ट रूप से लिखा जाता है:

```lisp
(defun run ((s string)) Result<i32, :dyn Error>
  (match (as-dyn-error (parse-int s))            ; ParseIntError -> :dyn Error
    ((ok n) (as-dyn-error (open-it (if (= n 0) "" "f"))))   ; io-err -> :dyn Error
    ((err e) (result::err e))))
```

**टाइप और trait एक नेमस्पेस साझा करते हैं** (Rust की तरह)। एक मॉड्यूल के भीतर, `defstruct`/`defenum` और trait का नाम समान नहीं हो सकता, और टाइप के स्थान पर trait का नाम लिखना "`error` is a trait, not a type — write `:dyn error`" के रूप में रिपोर्ट होता है।

पुनर्प्राप्त न की जा सकने वाली विफलताएँ `panic` से व्यक्त होती हैं। `panic` और `catch`/`throw` के लिए [सिंटैक्स संदर्भ](../syntax.md#8-नॉन-लोकल-निकास-catch--throw--unwind-protect) देखें; त्रुटि प्रबंधन नीति के लिए, [उसी का अध्याय 9](../syntax.md#9-त्रुटि-प्रबंधन-नीति) देखें।
