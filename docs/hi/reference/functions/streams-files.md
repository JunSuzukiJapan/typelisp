<!-- translated-from: docs/ja/reference/functions/streams-files.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# स्ट्रीम और फ़ाइलें

स्ट्रीम trait और मेथड, ठोस स्ट्रीम टाइप, फ़ाइल ऑपरेशन और पाथनेम। नेटवर्क सॉकेट भी स्ट्रीम हैं, और [नेटवर्किंग](network.md) में कवर हैं।

## 1. Trait पदानुक्रम

CL जो क्लास पदानुक्रम से व्यक्त करता है वह यहाँ **trait पदानुक्रम** से व्यक्त होता है। दिशा (इनपुट / आउटपुट) और तत्व टाइप दोनों **स्टैटिक रूप से** तय होते हैं, इसलिए रन टाइम पर पूछने की ज़रूरत नहीं कि "क्या यह स्ट्रीम पढ़ी जा सकती है?"।

```lisp
(deftrait Stream ()             (open-stream-p ...) (close ...))
(deftrait InputStream  (Stream) (type Item) (read-item ...))
(deftrait OutputStream (Stream) (type Item) (write-item ...))
(deftrait CharInput  ((InputStream  (Item char))) ...)   ; कैरेक्टर इनपुट
(deftrait CharOutput ((OutputStream (Item char))) ...)   ; कैरेक्टर आउटपुट
(deftrait PeekInput  (CharInput) (unread-char ...) (peek-char ...))  ; ऐसा इनपुट जो एक कैरेक्टर वापस धकेल सकता है
(deftrait ByteInput  ((InputStream  (Item int))) ...)    ; बाइट इनपुट
(deftrait ByteOutput ((OutputStream (Item int))) ...)    ; बाइट आउटपुट
```

कैरेक्टर पढ़ने वाला फ़ंक्शन, यदि वह `(where (CharInput S))` या `:dyn CharInput` लेता है, तो किसी भी स्ट्रीम टाइप को स्वीकार करता है, बिल्ट-इन हो या उपयोगकर्ता-परिभाषित।

## 2. मेथड

`CharInput` के हर मेथड का डिफ़ॉल्ट इम्प्लीमेंटेशन है। इम्प्लीमेंटेशन केवल `read-item` लिखता है।

| नाम | फ़ॉर्म | टाइप | विवरण |
|---|---|---|---|
| `read-item` | `(read-item s)` | `(S)→Option<Item>` | अगला तत्व। अंत पर `none`। **एकमात्र मेथड जिसे लागू करना ज़रूरी है** |
| `read-char` | `(read-char s)` | `(S)→Option<char>` | अगला कैरेक्टर |
| `read-line` | `(read-line s)` | `(S)→Option<string>` | अगली नई पंक्ति तक (नई पंक्ति खर्च होकर हटा दी जाती है)। नई पंक्ति पर समाप्त न होने वाली अंतिम पंक्ति भी लौटाई जाती है |
| `read-all` | `(read-all s)` | `(S)→string` | जो कुछ बचा है वह सब |
| `read-char-no-hang` | `(read-char-no-hang s)` | `(S)→Option<char>` | केवल वह कैरेक्टर जो पहले से हाथ में है। प्रतीक्षा करने की बजाय `none` |
| `read-sequence` | `(read-sequence s v n)` | `(S,Vector<char>,int)→int` | `v` पर अधिकतम `n` कैरेक्टर धकेलता है और लौटाता है कि वास्तव में कितने पढ़े गए। `n` से कम केवल अंत पर |

`listen` `InputStream` (`CharInput` का पैरेंट) में है:

| नाम | फ़ॉर्म | टाइप | विवरण |
|---|---|---|---|
| `listen` | `(listen s)` | `(S)→bool` | क्या अगले पढ़ने का उत्तर बिना प्रतीक्षा के दिया जा सकता है। डिफ़ॉल्ट `false` है, **वह पक्ष जो कभी झूठ नहीं होता**: `true` अनुमान होता, और गलत अनुमान `read-char-no-hang` को ब्लॉक करवा देता। सभी बिल्ट-इन स्ट्रीम इसे ओवरराइड करती हैं। **जो उपयोगकर्ता-परिभाषित स्ट्रीम इसे ओवरराइड नहीं करतीं उनके लिए `read-char-no-hang` हमेशा `none` लौटाता है** |

`PeekInput` (जो `CharInput` से विरासत लेता है) **एक कैरेक्टर वापस धकेलना** जोड़ता है। केवल स्ट्रीम के पास ही वापस धकेले गए कैरेक्टर को रखने की जगह है, इसलिए इसका डिफ़ॉल्ट इम्प्लीमेंटेशन नहीं हो सकता और यह अलग trait है। `file-stream`/`string-input-stream`/`standard-stream` इसे लागू करते हैं, और कोई भी अन्य स्ट्रीम इसे तब पाती है जब उसे `make-peek-stream` (अध्याय 4) से लपेटा जाए।

| नाम | फ़ॉर्म | टाइप | विवरण |
|---|---|---|---|
| `unread-char` | `(unread-char s c)` | `(S,char)→()` | अगले पढ़ने से `c` लौटवाता है। **एकमात्र मेथड जिसे लागू करना ज़रूरी है**। CL की तरह, केवल एक कैरेक्टर की गारंटी है |
| `peek-char` | `(peek-char s)` | `(S)→Option<char>` | अगले कैरेक्टर को खर्च किए बिना देखता है |

इसी तरह, `CharOutput` के लिए इम्प्लीमेंटेशन केवल `write-item` लिखता है।

| नाम | फ़ॉर्म | टाइप | विवरण |
|---|---|---|---|
| `write-item` | `(write-item s x)` | `(S,Item)→()` | एक तत्व लिखता है। **एकमात्र मेथड जिसे लागू करना ज़रूरी है** |
| `write-char` | `(write-char s c)` | `(S,char)→()` | एक कैरेक्टर लिखता है |
| `write-string` | `(write-string s str)` | `(S,string)→()` | स्ट्रिंग लिखता है |
| `write-line` | `(write-line s str)` | `(S,string)→()` | स्ट्रिंग और नई पंक्ति |
| `terpri` | `(terpri s)` | `(S)→()` | एक नई पंक्ति (CL का नाम) |
| `fresh-line` | `(fresh-line s)` | `(S)→()` | एक नई पंक्ति, जब तक पंक्ति के आरंभ पर न हों |
| `at-line-start` | `(at-line-start s)` | `(S)→bool` | क्या अगला लिखा जाने वाला कैरेक्टर पंक्ति शुरू करेगा। डिफ़ॉल्ट `false` है (इसलिए `fresh-line` नई पंक्ति लिखता है: संदेह होने पर लिखना सुरक्षित पक्ष है)। सभी बिल्ट-इन स्ट्रीम इसे ओवरराइड करती हैं |
| `finish-output` | `(finish-output s)` | `(S)→()` | बफ़र फ़्लश करता है |
| `write-sequence` | `(write-sequence s v)` | `(S,Vector<char>)→()` | `v` के सभी कैरेक्टर क्रम से लिखता है |

`at-line-start` **केवल वही याद रखता है जो उस स्ट्रीम से होकर लिखा गया**। `print`/`println`/`(format true ...)` `*standard-output*` से होकर गए बिना स्टैंडर्ड आउटपुट पर लिखते हैं, इसलिए यदि आप दोनों को मिलाते हैं, तो `(fresh-line *standard-output*)` को उन नई पंक्तियों का पता नहीं होता जो `println` ने लिखीं। उनमें से एक पर टिके रहें।

`Stream` सभी स्ट्रीम के लिए साझा है:

| नाम | फ़ॉर्म | टाइप | विवरण |
|---|---|---|---|
| `open-stream-p` | `(open-stream-p s)` | `(S)→bool` | क्या यह अब भी खुली है |
| `close` | `(close s)` | `(S)→()` | इसे बंद करता है। **GC स्ट्रीम बंद नहीं करता**, इसलिए इसे स्पष्ट रूप से करें (या `with-open-file` से) |

## 3. ठोस स्ट्रीम टाइप

| टाइप | कैसे बनाएँ | लागू किए गए trait |
|---|---|---|
| `file-stream` | `(open-file name direction)` / `open-input` / `open-output` | `CharInput` `PeekInput` `CharOutput` |
| `string-input-stream` | `(make-string-input-stream s)` | `CharInput` `PeekInput` |
| `string-output-stream` | `(make-string-output-stream)` | `CharOutput` |
| `standard-stream` | `*standard-input*` `*standard-output*` `*error-output*` | `CharInput` `PeekInput` `CharOutput` |
| `binary-file-stream` | `(open-binary name direction)` / `open-binary-input` / `open-binary-output` | `ByteInput` `ByteOutput` |

`direction` तीन स्थिरांकों `direction-input` / `direction-output` / `direction-append` में से एक है। यदि फ़ाइल खोली न जा सके तो `open-file` `Err(FileError)` लौटाता है (गायब फ़ाइल सामान्य परिणाम है, panic नहीं)। फ़ाइल का नाम स्ट्रिंग या `pathname` हो सकता है (अध्याय 9 का `Pathish`)।

`(get-output-stream-string s)` वह लौटाता है जो `string-output-stream` में लिखा गया है और उसे खाली कर देता है। CL की तरह, इसे `close` के बाद भी निकाला जा सकता है।

**बाइट I/O** `ByteInput`/`ByteOutput` का उपयोग करता है। ये `InputStream`/`OutputStream` के `Item` को `int` पर तय करते हैं, उसी तरह जैसे `CharInput`/`CharOutput` उसे `char` पर तय करते हैं।

| नाम | फ़ॉर्म | टाइप | विवरण |
|---|---|---|---|
| `read-byte` | `(read-byte s)` | `(S)→Option<int>` where `ByteInput S` | अगला बाइट। फ़ाइल के अंत पर `none` |
| `write-byte` | `(write-byte s b)` | `(S,int)→()` where `ByteOutput S` | एक बाइट लिखता है। 0..255 के बाहर त्रुटि |
| `read-sequence` | `(read-sequence s v n)` | `(S,Vector<int>,int)→int` where `ByteInput S` | कैरेक्टर संस्करण, बाइट में |
| `write-sequence` | `(write-sequence s v)` | `(S,Vector<int>)→()` where `ByteOutput S` | ऊपर जैसा |

CL तत्व टाइप **कॉल में** तय करता है, जैसे `(open name :element-type '(unsigned-byte 8))`, लेकिन यहाँ तत्व टाइप स्ट्रीम का **टाइप** है, इसलिए जो भिन्न होता है वह उसे खोलने वाला फ़ंक्शन है। कैरेक्टर स्ट्रीम से बाइट पढ़ना टाइप त्रुटि है (`string-input-stream` `ByteInput` लागू नहीं करता)। `unread-char` से कैरेक्टर वापस धकेलने के ठीक बाद बाइट पढ़ना भी त्रुटि है।

## 4. संयुक्त स्ट्रीम

सभी स्टैंडर्ड लाइब्रेरी के `defstruct` हैं और नेस्ट हो सकती हैं।

| नाम | फ़ॉर्म | विवरण |
|---|---|---|
| `make-broadcast-stream` | `(make-broadcast-stream v)` | `Vector<:dyn CharOutput>` के सभी पर लिखता है |
| `make-two-way-stream` | `(make-two-way-stream in out)` | `in` से पढ़ता है और `out` पर लिखता है |
| `make-echo-stream` | `(make-echo-stream in out)` | `in` से पढ़ता है और पढ़े गए कैरेक्टर `out` पर भी लिखता है |
| `make-concatenated-stream` | `(make-concatenated-stream v)` | `Vector<:dyn CharInput>` को एक के बाद एक पढ़ता है |
| `make-peek-stream` | `(make-peek-stream in)` | किसी भी `:dyn CharInput` में एक-कैरेक्टर पुशबैक जोड़ता है, उसे `PeekInput` बनाता है (`read-sexpr` के लिए) |

## 5. Macro

| नाम | फ़ॉर्म | विवरण |
|---|---|---|
| `with-open-file` | `(with-open-file (var name direction) body...)` | खोलें, बॉडी चलाएँ, बंद करें। `Result<बॉडी का मान, FileError>` |
| `with-input-from-string` | `(with-input-from-string (var s) body...)` | स्ट्रिंग से पढ़ता है |
| `with-output-to-string` | `(with-output-to-string (var) body...)` | जो लिखा गया वह लौटाता है |

## 6. जेनेरिक फ़ंक्शन और फ़ाइल ऑपरेशन

| नाम | फ़ॉर्म | टाइप | विवरण |
|---|---|---|---|
| `copy-stream` | `(copy-stream from to)` | `(I,O)→()` where `CharInput I`,`CharOutput O` | सब कुछ स्थानांतरित करता है |
| `read-lines` | `(read-lines s)` | `(S)→Vector<string>` where `CharInput S` | शेष सभी पंक्तियाँ |
| `read-sexpr` | `(read-sexpr s)` | `(S)→Result<ReadOutcome,ReadError>` where `PeekInput S` | एक `Sexpr` पढ़ता है (CL का `read`)। इनपुट के अंत पर `Ok(eof)`, एक पढ़ा जाने पर `Ok(datum d)`, यदि वह डेटा नहीं है तो `Err`। यह उस **एक रिक्त-स्थान कैरेक्टर को खर्च करता है** जिसने डेटाम को समाप्त किया (CL की तरह)। `ReadOutcome` `Option<Sexpr>` नहीं है ताकि खाली सूची `()` पढ़ना और इनपुट का अंत एक ही मान न हों |
| `read-sexpr-preserving-whitespace` | ऊपर जैसा | ऊपर जैसा | वही, लेकिन रिक्त स्थान छोड़ देता है (CL का `read-preserving-whitespace`) |
| `read-delimited-list` | `(read-delimited-list ch s)` | `(char,S)→Result<Option<Sexpr>,ReadError>` where `PeekInput S` | `ch` तक पढ़ता है और सूची बनाता है। `ch` खर्च होता है। इनपुट समाप्त हो जाए तो `Err` |
| `write-lines` | `(write-lines s lines)` | `(S,I)→()` where `CharOutput S`,`Iter I (Item string)` | एक बार में एक पंक्ति लिखता है |
| `read-file-string` | `(read-file-string name)` | `(P)→Result<string,FileError>` where `Pathish P` | पूरी सामग्री |
| `read-file-lines` | `(read-file-lines name)` | `(P)→Result<Vector<string>,FileError>` where `Pathish P` | सभी पंक्तियाँ |
| `write-file-string` | `(write-file-string name text)` | `(P,string)→Result<(),FileError>` where `Pathish P` | इसे लिख देता है |
| `probe-file` | `(probe-file name)` | `(P)→bool` where `Pathish P` | क्या यह मौजूद है |
| `delete-file` / `rename-file` | | `→Result<(),FileError>` | हटाना, नाम बदलना (आर्ग्युमेंट `Pathish` हैं) |
| `truename` | `(truename name)` | `(P)→Result<string,FileError>` where `Pathish P` | सिंबॉलिक लिंक और `.`/`..` हल करके निरपेक्ष पाथ। मौजूद न हो तो `Err` |
| `file-write-date` | `(file-write-date name)` | `(P)→Result<universal-time,FileError>` where `Pathish P` | अंतिम संशोधन का समय। यह **यूनिवर्सल टाइम** है, इसलिए `decode-universal-time` ([समय](system.md#2-तारीख़ों-को-डीकोड-और-एन्कोड-करना)) इसे पढ़ सकता है |
| `file-author` | `(file-author name)` | `(P)→Result<Option<string>,FileError>` where `Pathish P` | स्वामी का लॉगिन नाम। फ़ाइल मौजूद न हो तो `Err`, यदि स्वामी के uid की पासवर्ड डेटाबेस में प्रविष्टि नहीं है तो `Ok(none)`: जिन दो स्थितियों में CL भेद करता है वे अलग रखी गई हैं |
| `directory-p` | `(directory-p name)` | `(P)→bool` where `Pathish P` | क्या यह डायरेक्टरी है। **मौजूद न हो तो भी `false`**; दोनों में भेद करने के लिए `probe-file` का उपयोग करें |
| `directory` | `(directory name)` | `(P)→Result<Vector<string>,FileError>` where `Pathish P` | सामग्री को truename (सिंबॉलिक लिंक हल किया निरपेक्ष पाथ, `truename` की तरह) से सूचीबद्ध करता है। जिन सिंबॉलिक लिंक का लक्ष्य गायब है वे छोड़ दिए जाते हैं। `.`/`..` छोड़ दिए जाते हैं। क्रम वही है जो OS देता है |
| `ensure-directories-exist` | `(ensure-directories-exist name)` | `(P)→Result<(),FileError>` where `Pathish P` | पैरेंट सहित बनाता है। यदि पहले से मौजूद है तो सफल होता है |

फ़ाइल का नाम देने वाला हर आर्ग्युमेंट **स्ट्रिंग या `pathname` हो सकता है**। यह CL के पाथनेम डेज़िग्नेटर जैसा ही व्यवहार है, जो रन-टाइम टाइप परीक्षण की बजाय `Pathish` trait से हल होता है (अध्याय 9)।

`read-delimited-list` का समापन कैरेक्टर **टोकन को भी समाप्त करता है**। यह केवल गहराई 0 पर प्रभावी होता है: `(1 2]` में `]` सूची के अपने पाठ का हिस्सा पढ़ा जाता है और टूटी सूची के रूप में रिपोर्ट होता है। CL के तीसरे आर्ग्युमेंट `recursive-p` का कोई समकक्ष नहीं है।

## 7. अपने टाइप को स्ट्रीम बनाना

एक `write-item` लिखें और डिफ़ॉल्ट इम्प्लीमेंटेशन बाकी ले आते हैं। इसे संयुक्त स्ट्रीम में भी डाला जा सकता है।

```lisp
(defstruct counter (n i32))
(impl Stream counter
  (open-stream-p ((self Self)) bool true)
  (close ((self Self)) () ()))
(impl OutputStream counter
  (type Item char)
  (write-item ((self Self) (c char)) () (setf self::n (+ self::n 1))))
(impl CharOutput counter)              ; शेष हर मेथड डिफ़ॉल्ट है

(write-line (counter::new 0) "four")   ; write-line, terpri और fresh-line सब काम करते हैं
```

इनपुट भी इसी तरह काम करता है: आप केवल `read-item` लिखते हैं। अपने पुशबैक के बिना टाइप को भी लपेटने के बाद `read` किया जा सकता है, जैसे `(read-sexpr (make-peek-stream my-stream))`।

## 8. readtable

| नाम | कॉल | टाइप | विवरण |
|---|---|---|---|
| `set-macro-character` | `(set-macro-character c f)` | `(char, F)→()` | `f` कैरेक्टर `c` को पढ़ता है |
| `get-macro-character` | `(get-macro-character c)` | `(char)→Option<F>` | जो पंजीकृत है उसे लौटाता है |
| `set-dispatch-macro-character` | `(set-dispatch-macro-character d s f)` | `(char,char,F)→()` | `f` दो-कैरेक्टर अनुक्रम `d s` को पढ़ता है |
| `get-dispatch-macro-character` | `(get-dispatch-macro-character d s)` | `(char,char)→Option<F>` | ऊपर जैसा |

`F` `(fn (string-input-stream char) Option<Sexpr>)` है। उनका उपयोग कैसे करें, वे कब प्रभावी होते हैं और CL से कैसे भिन्न हैं, यह [सिंटैक्स संदर्भ](../syntax.md#11-रीडर-macro-readtable) में है।

## 9. पाथनेम `pathname`

हिस्सों में बँटा फ़ाइल का नाम। यह `/`-से अलग डायरेक्टरी घटक, नाम, टाइप (एक्सटेंशन), और यह रखता है कि वह रूट से शुरू होता है या नहीं।

```lisp
(let ((p (parse-namestring "/var/log/app.tar.gz")))
  (pathname-directory p)   ; => #<vector<string> "var" "log">
  (pathname-name p)        ; => (some "app.tar")   अंतिम बिंदु पर बाँटा गया
  (pathname-type p)        ; => (some "gz")
  (namestring p))          ; => "/var/log/app.tar.gz"

(namestring (merge-pathnames (make-pathname :name "today" :type "log")
                             "/var/log/"))        ; => "/var/log/today.log"
```

### 9.1 पाथनेम डेज़िग्नेटर trait `Pathish`

जहाँ CL पाथनेम डेज़िग्नेटर (स्ट्रिंग या पाथनेम) स्वीकार करता है, वहाँ यह भाषा `Pathish` स्वीकार करती है। `string` और `pathname` दोनों इसे लागू करते हैं, और **हर फ़ाइल ऑपरेशन इसे जेनेरिक रूप से लेता है**, इसलिए `(open-input "a.txt")` और `(open-input p)` दोनों सामान्य कॉल हैं (कोई रन-टाइम टाइप परीक्षण नहीं)। स्ट्रिंग का `namestring` बस स्वयं को लौटाता है, इसलिए जब तक आप स्ट्रिंग पास करते हैं, कोई पार्सिंग नहीं होती।

| नाम | फ़ॉर्म | टाइप | विवरण |
|---|---|---|---|
| `namestring` | `(namestring p)` | `(P)→string` | स्ट्रिंग रूप। लागू करना ज़रूरी |
| `to-pathname` | `(to-pathname p)` | `(P)→pathname` | `pathname` में बदलता है (CL का `pathname` फ़ंक्शन, जिसका नाम बदला गया क्योंकि वह टाइप के नाम से टकराता)। लागू करना ज़रूरी |

### 9.2 फ़ंक्शन

| नाम | फ़ॉर्म | टाइप | विवरण |
|---|---|---|---|
| `parse-namestring` | `(parse-namestring s)` | `string→pathname` | स्ट्रिंग को हिस्सों में बाँटता है। अंत का `/` (या खाली नाम) का अर्थ है "कोई नाम नहीं", यानी डायरेक्टरी |
| `make-pathname` | `(make-pathname :directory v :name s :type s :absolute b)` | `→pathname` | केवल दिए गए घटकों से एक बनाता है (सब `&key`)। छोड़ा गया नाम या टाइप "अनुपस्थित" रहता है और ऐसी चीज़ है जिसे `merge-pathnames` भरता है |
| `pathname-directory` | `(pathname-directory p)` | `(P)→Vector<string>` | डायरेक्टरी घटक, सबसे बाहरी पहले |
| `pathname-name` | `(pathname-name p)` | `(P)→Option<string>` | टाइप के बिना नाम। डायरेक्टरी के लिए `none` |
| `pathname-type` | `(pathname-type p)` | `(P)→Option<string>` | अंतिम बिंदु के बाद। शुरू का बिंदु नहीं गिना जाता (`.gitignore` पूरा नाम है) |
| `pathname-absolute-p` | `(pathname-absolute-p p)` | `(P)→bool` | क्या यह रूट से शुरू होता है |
| `user-homedir-pathname` | `(user-homedir-pathname)` | `()→Option<pathname>` | होम डायरेक्टरी। यदि `$HOME` नहीं है तो `none` (CL `NIL` की भी अनुमति देता है) |
| `directory-namestring` | `(directory-namestring p)` | `(P)→string` | अंतिम `/` तक का भाग |
| `file-namestring` | `(file-namestring p)` | `(P)→string` | केवल `name.type` भाग |
| `merge-pathnames` | `(merge-pathnames p default)` | `(P,D)→pathname` | `p` में छूटे घटकों को `default` से भरता है। सापेक्ष `p` `default` की डायरेक्टरी के नीचे जाता है; निरपेक्ष `p` अपनी डायरेक्टरी रखता है |
| `enough-namestring` | `(enough-namestring p default)` | `(P,D)→string` | `default` के सापेक्ष रूप। यदि `p` आधार के नीचे नहीं है तो पूरा `p` |

टाइप आर्ग्युमेंट सभी `(where (Pathish P))` लिए होते हैं।

## 10. CL से अंतर

- **क्लास पदानुक्रम नहीं, trait पदानुक्रम।** `input-stream-p` / `output-stream-p` नहीं हैं: टाइप दिशा ले जाता है, इसलिए यह रन टाइम पर पूछने का प्रश्न नहीं है।
- **`read` के स्ट्रिंग और स्ट्रीम संस्करणों के नाम भिन्न हैं।** `(read "...")` (CL के `read-from-string` के पहले मान के अनुरूप; यदि आपको वह स्थिति भी चाहिए जहाँ पढ़ना समाप्त हुआ, तो `read-from-string` का उपयोग करें) और `(read-sexpr s)` (CL का `read`)। कॉल एक रिसीवर टाइप पर हल होता है, इसलिए एक ही नाम ओवरलोड नहीं किया जा सकता।
- **पुशबैक अलग trait है** (`PeekInput`), इसलिए जिन टाइप को केवल `read-char` चाहिए उन्हें `unread-char` लागू करने को बाध्य नहीं किया जाता।
- **बंद करना स्पष्ट है।** GC स्ट्रीम बंद नहीं करता (GC अप्रत्याशित समय पर चलता है, इसलिए GC पर छोड़ने से बंद होने का क्षण भी अप्रत्याशित हो जाता)। `with-open-file` का उपयोग सुरक्षित तरीका है।
- **पाथनेम में होस्ट, डिवाइस या संस्करण घटक नहीं हैं।** वाइल्डकार्ड पाथनेम और लॉजिकल पाथनेम (`logical-pathname`) नहीं हैं। विभाजक हमेशा `/` है।
- **`pathname` फ़ंक्शन `to-pathname` है**, क्योंकि टाइप, trait और फ़ंक्शन एक नेमस्पेस साझा करते हैं।
- **वाइल्डकार्ड से मिलान नहीं है**, इसलिए `directory` ऐसा फ़ंक्शन है जो "उस डायरेक्टरी की सामग्री सूचीबद्ध करता है" और इससे अधिक कुछ नहीं। CL का `directory` पाथनेम पैटर्न से मिलान करता है।
