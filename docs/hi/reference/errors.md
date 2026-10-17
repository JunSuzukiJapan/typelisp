<!-- translated-from: docs/ja/reference/errors.md @ 919605e85abc17d3889701183c762718842ab3b4 -->
# त्रुटि संदेश

`typl` के मुख्य त्रुटि संदेशों का अर्थ और उन्हें कैसे ठीक करें।

## 1. त्रुटि पढ़ना

त्रुटियाँ स्टैंडर्ड एरर पर इस रूप में लिखी जाती हैं:

```text
error: file:line:column: kind: message
```

`kind` बताता है कि त्रुटि कब पकड़ी गई।

| Kind | कब | अर्थ |
|---|---|---|
| `type error` | चलने से पहले (जाँच के समय) | टाइप या नामों में गलती। वह फ़ॉर्म नहीं चलाया जाता |
| (कोई kind नहीं) | पढ़ते या जाँचते समय | कोष्ठकों के असंतुलन जैसी सिंटैक्स की गलती, या ऐसा नाम जो मिल नहीं सकता |
| `panic` | चलते समय | पुनर्प्राप्त न की जा सकने वाली विफलता। `unwind-protect` का क्लीनअप चलाने के बाद प्रोग्राम रुक जाता है |

`warning:` से शुरू होने वाली पंक्तियाँ चेतावनियाँ हैं, और प्रोसेसिंग जारी रहती है।

`file:line:column` गलती वाले एक्सप्रेशन की ओर इशारा करता है। जो रन-टाइम त्रुटि स्टैंडर्ड लाइब्रेरी फ़ंक्शन के भीतर होती है, उसके लिए यह उस जगह की ओर इशारा करता है जहाँ प्रोग्राम ने उस फ़ंक्शन को कॉल किया। कुछ त्रुटियों की कोई स्थिति नहीं होती (जैसे `error: panic: ...`)।

उदाहरण:

```text
error: main.typl:1:24: type error: type mismatch: expected `i32`, found `string`
```

इसका अर्थ है कि `main.typl` की पंक्ति 1, स्तंभ 24 का एक्सप्रेशन `string` था जहाँ `i32` अपेक्षित था।

## 2. जाँच के समय की त्रुटियाँ

चलने से पहले पकड़ी गई गलतियाँ। जब तक उन्हें ठीक नहीं किया जाता, फ़ॉर्म नहीं चलाया जाता।

### 2.1 टाइप

| संदेश | अर्थ और समाधान |
|---|---|
| ``type mismatch: expected `T`, found `U` `` | जहाँ टाइप `T` चाहिए वहाँ टाइप `U` का एक्सप्रेशन है। कोई अंतर्निहित रूपांतरण नहीं हैं; संख्याओं के लिए `(as T x)` से बदलें। `int` और `i32` भी अलग टाइप हैं |
| ``integer literal 300 is out of range for u8 (0..=255)`` | लिटरल टाइप में नहीं समाता। यदि आप चाहते हैं कि उसे काट दिया जाए, तो `(as u8 300)` लिखें |
| ``unknown type `foo`: no type of that name is visible here. ...`` | उस नाम का कोई टाइप नहीं है। टाइप को उसका उपयोग करने वाले पहले फ़ॉर्म से पहले परिभाषित करें (टाइप की फ़ॉरवर्ड घोषणा नहीं होती)। यदि आपका आशय टाइप वेरिएबल था, तो उसे फ़ंक्शन के नाम के बाद `<foo>` जैसी घोषित करने वाली स्थिति में लिखें ([सिंटैक्स संदर्भ 3.6](syntax.md#36-defstruct--struct-उपयोगकर्ता-परिभाषित-टाइप)) |
| ``cannot infer type argument `t` for `vector::new` `` | टाइप आर्ग्युमेंट निर्धारित नहीं किया जा सकता। टाइप को `the` से लिखें, जैसे `(the Vector<int> (Vector::new))` |
| ``non-exhaustive match on `color`: 1/2 variants covered`` | `match` हर variant को नहीं संभालता। छूटे हुए variant के लिए शाखाएँ, या `_` शाखा जोड़ें |
| ``type `pt` does not implement trait `eq` required by `:where` clause on type parameter `a` `` | फ़ंक्शन को ऐसा trait चाहिए जिसे आपका पास किया हुआ टाइप लागू नहीं करता। `(impl Eq pt ...)` लिखें ([स्टैंडर्ड trait](functions/traits.md)) |
| ``` `sq` does not implement `shape`, so it cannot be used as `:dyn shape` ``` | जहाँ `:dyn` अपेक्षित था वहाँ ऐसे टाइप का मान पास किया गया जो trait को लागू नहीं करता। `impl` लिखें |
| ``` `error` is a trait, not a type — write `:dyn error` for a trait object ``` | जहाँ टाइप आता है वहाँ trait का नाम लिखा गया। `:dyn Error` लिखें |
| ``if: (if cond then else)`` | `if` का आकार गलत है। `if` को else शाखा चाहिए। जब ज़रूरत न हो, तो `when` का उपयोग करें |

### 2.2 नाम

| संदेश | अर्थ और समाधान |
|---|---|
| `no such function: bar` | उस नाम का कोई फ़ंक्शन या मेथड नहीं है। वर्तनी जाँचें |
| ``no method `upcase` for type `int` (the type of the first argument, which selects the method); `upcase` is a method of `char`, `string` `` | मेथड पहले आर्ग्युमेंट के टाइप से चुने जाते हैं। उस नाम का मेथड मौजूद है, लेकिन पहले आर्ग्युमेंट के टाइप (यहाँ `int`) के लिए नहीं। संदेश का अंत उन टाइप को सूचीबद्ध करता है जिनके पास वह मेथड है (विशिष्ट टाइप के `impl` में लिखे मेथड के लिए `box<int>` जैसा विशिष्ट टाइप) |
| ``no method `twice` for type `box<string>` (the first parameter of the function this value is expected to be, which selects the method); `twice` is a method of `box<int>` `` | मान के रूप में दी गई मेथड उस फ़ंक्शन प्रकार के पहले पैरामीटर के प्रकार से चुनी जाती है जिसके रूप में उसे दिया जाता है। उस प्रकार (यहाँ `box<string>`) में उस नाम की कोई मेथड नहीं है। संदेश के अंत में वे प्रकार सूचीबद्ध हैं जिनमें वह है |
| `unbound variable: y` | उस नाम का कोई वेरिएबल नहीं है। वर्तनी और बाइंडिंग का स्कोप जाँचें (क्या इसका उपयोग इसके `let` के बाहर हो रहा है?) |
| ``cannot define variable `f`: a function of that name already exists here — a name has one meaning, whether a function, a variable or a macro`` | एक मॉड्यूल में फ़ंक्शन, ग्लोबल वेरिएबल और मैक्रो में से दो को एक ही नाम दिया गया। उनमें से एक का नाम बदलें ([सिंटैक्स संदर्भ 3.10](syntax.md#310-module--use--नेमस्पेस)) |
| ``` `point` has no static method `new`; a struct's methods are written in its `impl`, ...``` | स्ट्रक्ट के लिए कोई `new` उत्पन्न नहीं होता। मान `make-instance` से बनाएँ, या `impl` में एक `new` लिखें ([सिंटैक्स संदर्भ 3.5](syntax.md#35-impl--टाइप-के-मेथड)) |
| ``defmethod is gone — write methods in the type's `impl`: ...`` | `defmethod` नहीं है। मेथड टाइप के `impl` में लिखे जाते हैं ([सिंटैक्स संदर्भ 3.5](syntax.md#35-impl--टाइप-के-मेथड)) |
| ``defstruct: (:constructor ...) is gone — ...`` | `:constructor` विकल्प नहीं है। कंस्ट्रक्टर `impl` में स्टैटिक मेथड के रूप में लिखा जाता है |
| ``method box::m: `box` has a `m` for every instantiation already, so an `impl` of `box<int>` cannot define its own ...`` | एक ही नाम का मेथड टाइप पैरामीटर को वेरिएबल लिखने वाले `impl` (`Box<T>`) और विशिष्ट टाइप के `impl` (`Box<int>`) दोनों में लिखा गया (क्रम कोई भी हो)। केवल एक रखें ([सिंटैक्स संदर्भ 3.5](syntax.md#35-impl--टाइप-के-मेथड)) |
| ``impl show box<int>: `box` implements `show` for every instantiation already, ...`` | एक ही ट्रेट `Box<T>` और `Box<int>` दोनों के लिए `impl` किया गया। केवल एक रखें |
| ``use: unresolved `nosuch` `` | `use` में नामित मॉड्यूल नहीं मिल सकता। फ़ाइल के नाम मॉड्यूल पाथ से कैसे मेल खाते हैं, इसके लिए [सिंटैक्स संदर्भ 3.11](syntax.md#311-फ़ाइलें-और-मॉड्यूल-बहु-फ़ाइल-प्रोजेक्ट) देखें |
| `unresolved path: c::hidden` | मॉड्यूल मौजूद है, लेकिन नाम नहीं है, या `pub` न होने के कारण दिखाई नहीं देता |
| `circular module dependency: a -> b -> a` | मॉड्यूल एक-दूसरे को `use` करते हैं। साझा हिस्से को अलग मॉड्यूल में ले जाएँ |
| ``return-from: no enclosing block named `nope` `` | `return-from` को दिए गए नाम वाला कोई `block` उसे नहीं घेरता। फ़ंक्शन का block केवल उसी फ़ंक्शन के भीतर उपयोग किया जा सकता है |

### 2.3 कॉल

| संदेश | अर्थ और समाधान |
|---|---|
| `f: expected 1 argument(s), got 2` | आर्ग्युमेंट की संख्या मेल नहीं खाती |
| `f: unknown keyword argument :b` | ऐसा कीवर्ड आर्ग्युमेंट पास किया गया जो फ़ंक्शन के पास नहीं है |
| `circle: expected 1 field(s), got 2` | enum वेरिएंट को दिए गए मानों की संख्या उस वेरिएंट के फ़ील्ड की संख्या से मेल नहीं खाती |
| ``make-instance point: slot `y` is not given and has no default`` | डिफ़ॉल्ट मान रहित फ़ील्ड `make-instance` को नहीं दिया गया |
| ``make-instance point: no slot `z` (its slots are `x`, `y`)`` | ऐसा फ़ील्ड दिया गया जो स्ट्रक्ट में नहीं है |
| ``make-instance geo::pt: slot `secret` is not `pub`, so it can be given only inside module `geo` `` | टाइप परिभाषित करने वाले मॉड्यूल के बाहर ऐसा फ़ील्ड दिया गया जो `pub` नहीं है। उस मॉड्यूल से एक `pub` कंस्ट्रक्टर उपलब्ध कराने को कहें |
| ``` `box::make` is written for `box<int>`, `box<string>`; the type the call is expected to return picks one, ...``` | कई विशिष्ट टाइपों के `impl` में उस नाम का स्टैटिक मेथड है, और कौन-सा कॉल हो यह तय नहीं होता। `(the Box<int> (Box::make))` की तरह अपेक्षित टाइप लिखें |
| ``compile: `box::twice` is written for more than one instantiation of `box`; name one: ...`` | विशिष्ट टाइप के लिए लिखा मेथड केवल टाइप के नाम से `compile`, `trace`, `documentation` आदि को दिया गया। `box<int>::twice` की तरह विशिष्ट टाइप लिखें |
| ``setf: cannot assign to constant `k` `` | `defconstant` से परिभाषित नाम को असाइन किया गया। यदि उसे बदलना आवश्यक है, तो `defvar` का उपयोग करें |
| ``defsignature: `later` has no definition in this file — a declaration promises one`` | `defsignature` से घोषित फ़ंक्शन परिभाषित नहीं है |
| ``format: ~/nosuch/ calls what (nosuch arg colon at) with arg a `string` would: no such function: nosuch`` | उस स्थान पर लिखा कॉल `(nosuch arg colon at)` हल नहीं होता। आर्ग्युमेंट के टाइप का मेथड या कोई फ़ंक्शन परिभाषित करें ([फ़ॉर्मैट निर्देश अध्याय 5](functions/format.md#5-name)) |
| ``format: ~/brief/ can be handed an element of a list argument, whose type is only `Sexpr` — ...`` | `~/name/` ऐसी जगह लिखा गया जहाँ उसे सूची के तत्व मिलते हैं, जैसे `~{` के भीतर। तत्वों का टाइप तय नहीं कर सकता कि क्या कॉल करना है |

## 3. पढ़ने की त्रुटियाँ

| संदेश | अर्थ और समाधान |
|---|---|
| `unexpected end of input while reading a list` | बंद करने वाला कोष्ठक गायब है। स्थिति वहाँ इशारा करती है जहाँ पढ़ना समाप्त हुआ (जैसे फ़ाइल का अंत), इसलिए खोलने वाले कोष्ठक को खोजें |

## 4. रन टाइम की त्रुटियाँ (panic)

| संदेश | अर्थ और समाधान |
|---|---|
| `panic: divide by zero` | पूर्णांकों या ratio के साथ शून्य से भाग। फ़्लोटिंग-पॉइंट का शून्य से भाग panic नहीं करता; वह `inf`/`NaN` देता है |
| `panic: unwrap: called on none` | `unwrap` को `none` पर लगाया गया। `none` स्थिति को `match` या `unwrap-or` से संभालें |
| `panic: Vector: index 5 out of bounds` | सीमा से बाहर इंडेक्स। `len` से लंबाई जाँचें, या ऐसा फ़ंक्शन उपयोग करें जो सीमा से बाहर होने पर `none` लौटाता है (`nth`, `pop` आदि) |
| `panic: an integer argument does not fit a fixnum` | इंडेक्स या गिनती लेने वाले आर्ग्युमेंट को ऐसा `int` पास किया गया जो 63 बिट में नहीं समाता |
| `throw: no enclosing (catch 'oops) for this throw` | `throw` चला जबकि उसी टैग का कोई घेरने वाला `catch` नहीं था |
| `panic: <message>` | प्रोग्राम ने `(panic "<message>")` कॉल किया। विफल `assert` `assertion failed: ...` देता है |

`panic` पूरी प्रोसेस को रोक देता है, चाहे वह किसी टास्क के भीतर हो ([सिंटैक्स संदर्भ 12.4](syntax.md#124-अन्य-सुविधाओं-के-साथ-अंतःक्रिया))। जिन विफलताओं से आप उबरना चाहते हैं उन्हें `Result` से व्यक्त करें ([सिंटैक्स संदर्भ अध्याय 9](syntax.md#9-त्रुटि-प्रबंधन-नीति))।

## 5. चेतावनियाँ

| संदेश | अर्थ |
|---|---|
| ``warning: redefining function `f` `` | उसी नाम का फ़ंक्शन दोबारा परिभाषित किया गया। बाद वाली परिभाषा प्रभावी होती है। यह सामान्यतः तब दिखता है जब आप REPL में परिभाषा ठीक करते हैं |
