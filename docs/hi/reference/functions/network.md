<!-- translated-from: docs/ja/reference/functions/network.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# नेटवर्किंग (TCP / TLS / Unix डोमेन / UDP)

सॉकेट [स्ट्रीम](streams-files.md) के सदस्य हैं। कनेक्शन को **उसी कनेक्शन के दो दृश्यों** में देखा जाता है, `socket-stream` (कैरेक्टर) और `socket-byte-stream` (बाइट), और `read-line`/`write-line`/`read-byte`/`format` उन पर जस के तस काम करते हैं। लिसनर `socket-listener` है। **TCP, TLS और Unix डोमेन सॉकेट एक ही टाइप साझा करते हैं** (Go के `net.Conn` की तरह): कनेक्ट होने के बाद, पढ़ना और लिखना एक जैसा है, और केवल बनाने का तरीका भिन्न है। UDP स्ट्रीम नहीं बल्कि डेटाग्राम (`udp-socket`) है।

**जो प्रतीक्षा करता है वह टास्क है, थ्रेड नहीं।** `accept`, `read-line`, `write-string`, `tcp-connect` (नाम समाधान सहित) और `recv-from` सब, यदि तैयार नहीं हैं, तो *उसी टास्क* को रोकते हैं (`sleep`/`recv` की तरह), और दूसरे टास्क चलते रहते हैं। इसीलिए सर्वर को Go जैसे ही आकार में लिखा जा सकता है, हर कनेक्शन के लिए `(task (serve c))` ([सिंटैक्स संदर्भ 12.5](../syntax.md#125-टास्क-कहाँ-स्विच-होते-हैं))।

## 1. टाइप

| टाइप | लागू किए गए trait | कैसे पाएँ |
|---|---|---|
| `socket-stream` | `CharInput` `PeekInput` `CharOutput` | `tcp-connect` / `tls-connect` / `unix-connect` / `accept` / `char-stream-of` |
| `socket-byte-stream` | `ByteInput` `ByteOutput` | `byte-stream-of` |
| `socket-listener` | `Stream` (`close` / `open-stream-p`) | `tcp-listen` / `tls-listen` / `unix-listen` |
| `udp-socket` | `Stream` | `udp-bind` |
| `datagram` | — (`bytes`: `Vector<int>`, `from`: `string`) | `recv-from` |
| `NetError` | `Error` | नीचे के फ़ंक्शन का `Err` |

स्ट्रीम टाइप का एक तत्व टाइप होता है (उसी कारण से जैसे `file-stream`/`binary-file-stream`), इसलिए कैरेक्टर और बाइट अलग टाइप हैं। `byte-stream-of`/`char-stream-of` ऐसे मान लौटाते हैं जो **उसी कनेक्शन** की ओर इंगित करते हैं और प्राप्त बफ़र साझा करते हैं: HTTP जैसी चीज़ें इसी तरह लिखी जाती हैं, जहाँ हेडर कैरेक्टर के रूप में और बॉडी बाइट के रूप में पढ़ी जाती है। कैरेक्टर के `unread-char` के ठीक बाद बाइट पढ़ना त्रुटि है (फ़ाइलों जैसा ही नियम)।

## 2. TCP / TLS / Unix डोमेन

| नाम | उपयोग | टाइप | अर्थ |
|---|---|---|---|
| `tcp-connect` | `(tcp-connect host port &key timeout)` | `(string,int,&key f64)→Result<socket-stream,NetError>` | कनेक्ट करता है। `host` नाम या पता हो सकता है। यदि एक नाम के कई पते हैं, तो उन्हें बारी-बारी से आज़माया जाता है (`localhost` `::1` और `127.0.0.1` है)। नाम लुकअप विफल होना, कनेक्शन अस्वीकार होना, या `:timeout` सेकंड से अधिक होना `Err` देता है |
| `tls-connect` | `(tls-connect host port &key timeout ca-file cert-file key-file server-name)` | `(string,int,&key f64 string string string string)→Result<socket-stream,NetError>` | `tcp-connect` के बाद TLS। प्रमाणपत्र `host` नाम के विरुद्ध (यदि सत्यापित किया जाने वाला नाम आपके कनेक्ट करने की जगह से भिन्न हो तो `:server-name`) Mozilla के रूट प्रमाणपत्रों से सत्यापित होता है। `:ca-file` (PEM) दिए जाने पर, यह **केवल उसमें के प्रमाणपत्रों** पर भरोसा करता है (निजी CA, या वही प्रमाणपत्र जो आपका अपना `tls-listen` प्रस्तुत करता है)। `:cert-file`/`:key-file` (दोनों या कोई नहीं) हमारा प्रमाणपत्र हैं, जो सर्वर के माँगने पर प्रस्तुत किया जाता है (म्यूचुअल TLS)। हैंडशेक यहीं पूरा होता है, इसलिए यदि सर्वर का प्रमाणपत्र पास न हो, तो यह कॉल `Err` लौटाता है। परिणाम सामान्य `socket-stream` है |
| `unix-connect` | `(unix-connect path)` | `(string)→Result<socket-stream,NetError>` | Unix डोमेन सॉकेट `path` से कनेक्ट करता है। यह लोकल है, इसलिए हैंडशेक की प्रतीक्षा और टाइमआउट नहीं है |
| `tcp-listen` | `(tcp-listen host port)` | `(string,int)→Result<socket-listener,NetError>` | सुनता है। `"127.0.0.1"` केवल यही मशीन है, `"0.0.0.0"` हर इंटरफ़ेस है। `port` के रूप में `0` देने पर OS चुनता है |
| `tls-listen` | `(tls-listen host port cert-file key-file &key client-ca)` | `(string,int,string,string,&key string)→Result<socket-listener,NetError>` | `tcp-listen` का TLS संस्करण। `cert-file` प्रमाणपत्र शृंखला है (PEM, अपना प्रमाणपत्र पहले), और `key-file` निजी कुंजी। दोनों यहीं पढ़े और जाँचे जाते हैं, इसलिए जो कुंजी मेल नहीं खाती वह इसी कॉल से `Err` देती है, पहले क्लाइंट पर नहीं। `accept` **हैंडशेक से पहले** लौटता है, और कनेक्शन संभालने वाले टास्क का पहला पढ़ना या लिखना हैंडशेक पूरा करता है (Go के `tls.Conn` की तरह), इसलिए हैंडशेक में धीमा क्लाइंट दूसरे `accept` को नहीं रोकता। `:client-ca` (PEM) दिए जाने पर, यह **हर क्लाइंट से** उसमें के किसी CA द्वारा जारी प्रमाणपत्र प्रस्तुत करने की **माँग करता है** (म्यूचुअल TLS)। इसके बिना, कोई नहीं माँगा जाता |
| `unix-listen` | `(unix-listen path)` | `(string)→Result<socket-listener,NetError>` | `path` पर सुनता है। **यदि फ़ाइल पहले से मौजूद है तो `Err`** (वह किसी दूसरी चल रही प्रोसेस की हो सकती है, इसलिए उसे चुपचाप बदला नहीं जाता)। `close` फ़ाइल हटा देता है |
| `accept` | `(accept l &key timeout)` | `(socket-listener,&key f64)→Result<socket-stream,NetError>` | अगला कनेक्शन। एक आने तक टास्क को रोकता है। `:timeout` सेकंड बाद `Err` के साथ छोड़ देता है |
| `wait-readable` | `(wait-readable s secs)` | `(socket-stream,f64)→bool` | जब तक बिना प्रतीक्षा के पढ़ा न जा सके, या `secs` सेकंड। `true` का अर्थ पहला है (पहले से बफ़र किया डेटा सहित)। पढ़ने पर घड़ी लगाने का तरीका: `(if (wait-readable c 5.0) (read-line c) ...)`। यह जो वादा करता है वह है कि **अगला पढ़ना रुकेगा नहीं**; `read-line` पंक्ति के शेष भाग की प्रतीक्षा कर सकता है |
| `wait-writable` | `(wait-writable s secs)` | `(socket-stream,f64)→bool` | जब तक लिखा न जा सके, या `secs` सेकंड |
| `tls-add-certificate` | `(tls-add-certificate l name cert-file key-file)` | `(socket-listener,string,string,string)→Result<(),NetError>` | `tls-listen` लिसनर में एक और प्रमाणपत्र जोड़ता है, जो `name` (SNI) माँगने वाले क्लाइंट को प्रस्तुत किया जाता है: एक लिसनर पर कई साइटें। यहीं जाँचा जाता है कि शृंखला `name` के लिए है या नहीं, और यदि नहीं, तो यह कॉल `Err` लौटाता है। जो क्लाइंट ऐसा नाम माँगते हैं जिसे किसी ने नहीं जोड़ा, या कोई नाम नहीं माँगते, उन्हें `tls-listen` का प्रमाणपत्र मिलता है |
| `peer-subject` | `(peer-subject s)` | `(socket-stream)→Option<string>` | पीयर के प्रमाणपत्र का विषय (`CN=client,O=Example,C=JP`, RFC 4514 रूप, सबसे विशिष्ट पहले)। म्यूचुअल-TLS सर्वर कैसे जानता है कि "किसने कनेक्ट किया" (पहले पढ़ने के बाद, जो हैंडशेक पूरा करता है)। क्लाइंट की ओर, सर्वर प्रमाणपत्र का नाम। सादे कनेक्शन के लिए, हैंडशेक से पहले, या यदि पीयर ने कोई प्रमाणपत्र प्रस्तुत नहीं किया तो `none` |
| `requested-server-name` | `(requested-server-name s)` | `(socket-stream)→Option<string>` | सर्वर-पक्ष TLS कनेक्शन पर, वह नाम जो क्लाइंट ने माँगा (SNI)। `tls-add-certificate` से जोड़ी गई कई साइटों वाला सर्वर कैसे जानता है "कौन-सी साइट"। सादे कनेक्शन के लिए, क्लाइंट की ओर, या बिना नाम (पते से कनेक्ट) `none` |
| `set-nodelay` | `(set-nodelay s on)` | `(socket-stream,bool)→()` | Nagle बंद करता है (`TCP_NODELAY`)। `write-string` हर बार पूरा सॉकेट तक जाता है, इसलिए Nagle चालू होने पर, हेडर और बॉडी दो हिस्सों में लिखा गया उत्तर पीयर के विलंबित ACK की प्रतीक्षा करता है: अनुरोध/उत्तर प्रोटोकॉल के लिए `true` का उपयोग करें। Unix डोमेन सॉकेट में Nagle नहीं होता, और यह बस सफल होता है |
| `set-keepalive` | `(set-keepalive s on)` | `(socket-stream,bool)→()` | निष्क्रिय रहने पर पीयर की जाँच करता है (`SO_KEEPALIVE`)। ऐसे पीयर का पता लगाता है जो बंद किए बिना गायब हो गया (खींचा गया केबल, रुका हुआ होस्ट) और कनेक्शन रीसेट करता है। OS की डिफ़ॉल्ट अवधि लंबी है (अक्सर 2 घंटे), इसलिए इसे नीचे के `set-keepalive-period` के साथ जोड़ें। Unix डोमेन सॉकेट में यह नहीं होता, इसलिए यह panic करता है |
| `set-keepalive-period` | `(set-keepalive-period s secs)` | `(socket-stream,int)→()` | पहली जाँच से पहले के निष्क्रिय सेकंड और जाँचों के बीच का अंतराल (पूरे सेकंड, कम से कम 1)। Go के `SetKeepAlivePeriod` की तरह, दोनों समान मान पर सेट होते हैं |
| `socket-error` | `(socket-error s)` | `(socket-stream \| socket-byte-stream)→Option<NetError>` | यदि **पीयर** ने इस कनेक्शन को तोड़ा, तो उसकी पहली विफलता (रीसेट, TLS अलर्ट, लिखते समय डिस्कनेक्ट)। स्वस्थ होने पर `none`: पीयर के साफ़-सुथरे ढंग से बंद होने से EOF विफलता नहीं है। नीचे "पीयर की विफलताएँ" देखें |
| `local-address` | `(local-address s)` | `(socket-stream \| socket-listener \| udp-socket)→string` | हमारी ओर का `host:port`। `(tcp-listen h 0)` के बाद चुना गया पोर्ट जानने का तरीका। Unix सॉकेट के लिए, पाथ (कनेक्ट करने वाला सिरा `(unnamed)` है) |
| `peer-address` | `(peer-address s)` | `(socket-stream)→string` | पीयर का `host:port`, या Unix सॉकेट के लिए पाथ |
| `shutdown-output` | `(shutdown-output s)` | `(socket-stream)→()` | केवल भेजने वाला पक्ष बंद करता है (हाफ़-क्लोज़)। पीयर EOF पढ़ता है, और यह पक्ष अब भी पढ़ सकता है। "मैंने पूरा अनुरोध भेज दिया" का संकेत। TLS के लिए यह `close_notify` भी भेजता है |
| `byte-stream-of` | `(byte-stream-of s)` | `(socket-stream)→socket-byte-stream` | उसी कनेक्शन का बाइट संस्करण |
| `char-stream-of` | `(char-stream-of b)` | `(socket-byte-stream)→socket-stream` | उसी कनेक्शन का कैरेक्टर संस्करण |
| `close` | `(close s)` | `Stream` | बफ़र बाहर भेजता है, फिर बंद करता है। GC इसे बंद नहीं करता |
| `with-connection` | `(with-connection (var host port) body...)` | Macro | कनेक्ट करें, बॉडी चलाएँ, बंद करें। `Result<बॉडी का मान, NetError>` (`with-open-file` जैसा ही आकार) |

`write-string`/`write-line` **सब कुछ लिखे जाने के बाद लौटते हैं** (Go के `net.Conn.Write` की तरह)। छोटे लिखने को बंडल करने के लिए, उन्हें `string-output-stream` में इकट्ठा करें और एक बार लिखें। `listen` केवल तब `true` है जब प्राप्त बफ़र में कुछ हो, इसलिए `read-char-no-hang` वैसे ही काम करता है जैसा उसका नाम कहता है।

**पीयर की विफलताएँ panic नहीं करतीं।** यदि दूसरा सिरा कनेक्शन रीसेट कर दे, TLS हैंडशेक अस्वीकार कर दे, या लिखने के बीच में उसे छोड़ दे, तो यह इस प्रोग्राम की गलती नहीं है, इसलिए सर्वर अपने दूसरे क्लाइंट के साथ नहीं रुकता। पहली विफलता कनेक्शन पर दर्ज होती है; बाद के पढ़ने `none` लौटाते हैं (EOF जैसे दिखते हुए), और लिखना कहीं नहीं जाता और चुपचाप लौट आता है। उन्हें अलग पहचानने के लिए, `socket-error` का उपयोग करें, Go के `bufio.Scanner.Err` जैसा ही आकार (`read-item` `Option` है और रिपोर्ट करने के लिए उसके पास कोई और चैनल नहीं है)। केवल प्रोग्राम की अपनी गलतियाँ panic करती हैं (बंद हैंडल, `unread-char` के ठीक बाद बाइट पढ़ना)।

**टाइमआउट स्पष्ट आर्ग्युमेंट या `wait-readable` हैं।** Go के `SetReadDeadline` जैसा कोई रूप नहीं है जहाँ स्ट्रीम समय सीमा ले जाती है और `read-line` विफल होता है: `read-item` `Option<Item>` लौटाता है और त्रुटि लौटाने का उसके पास कोई तरीका नहीं है। घड़ी तीन जगह चाहिए, कनेक्ट करना, स्वीकार करना और "अगला पढ़ना", और हर एक के लिए उसका आर्ग्युमेंट है।

```lisp
;; सर्वर: हर कनेक्शन के लिए एक टास्क
(defun serve ((c socket-stream)) ()
  (loop (match (read-line c)
          ((some line) (write-line c line))
          ((none) (break))))
  (close c))

(let ((l (unwrap (tcp-listen "0.0.0.0" 7777))))
  (loop (match (accept l)
          ((ok c) (progn (task (serve c)) ()))
          ((err e) (println "accept: ~a" (message e))))))

;; क्लाइंट
(match (with-connection (c "127.0.0.1" 7777)
         (write-line c "hello")
         (unwrap (read-line c)))
  ((ok reply) (println "~a" reply))
  ((err e) (println "~a" (message e))))

;; HTTPS
(let ((c (unwrap (tls-connect "example.com" 443 :timeout 10.0))))
  (write-string c "GET / HTTP/1.1\r\nHost: example.com\r\nConnection: close\r\n\r\n")
  (println "~a" (unwrap (read-line c)))       ; HTTP/1.1 200 OK
  (close c))

;; TLS सर्वर (प्रमाणपत्र उदाहरण के लिए इससे बनाया जा सकता है
;;   openssl req -x509 -newkey rsa:2048 -nodes -subj /CN=localhost \
;;           -addext subjectAltName=DNS:localhost -keyout key.pem -out cert.pem)
(let ((l (unwrap (tls-listen "0.0.0.0" 8443 "cert.pem" "key.pem"))))
  (loop (match (accept l)
          ((ok c) (progn (task (serve c)) ()))        ; ऊपर का serve जस का तस; पहला read-line हैंडशेक है
          ((err e) (println "accept: ~a" (message e))))))
;; इसका क्लाइंट: अपने प्रमाणपत्र पर भरोसा करके कनेक्ट करें
(unwrap (tls-connect "localhost" 8443 :timeout 5.0 :ca-file "cert.pem"))

;; म्यूचुअल TLS: सर्वर ca.pem द्वारा जारी क्लाइंट प्रमाणपत्र माँगता है, और क्लाइंट एक प्रस्तुत करता है
(tls-listen "0.0.0.0" 8443 "cert.pem" "key.pem" :client-ca "ca.pem")
(tls-connect "localhost" 8443 :ca-file "ca.pem" :cert-file "client.pem" :key-file "client-key.pem")
;; सर्वर की ओर, पहले read-line के बाद: किसने कनेक्ट किया
(println "~a" (unwrap-or (peer-subject c) "anonymous"))    ; CN=client,O=Example

;; एक लिसनर पर दो साइटें (SNI)
(let ((l (unwrap (tls-listen "0.0.0.0" 443 "a.example.pem" "a.key"))))
  (unwrap (tls-add-certificate l "b.example" "b.example.pem" "b.key"))
  ...)                                        ; कनेक्शन पर, (requested-server-name c) बताता है कि कौन-सी
```

म्यूचुअल TLS के साथ, जो क्लाइंट कोई प्रमाणपत्र प्रस्तुत नहीं करता (या जो पास नहीं होता), TLS 1.3 के तहत सर्वर के निर्णय लेने से पहले हैंडशेक का अपना पक्ष पूरा कर लेता है, इसलिए `tls-connect` `Ok` लौटाता है और **पहला पढ़ना `none` लौटाता है** (अलर्ट `socket-error` में होता है)। सर्वर की ओर, उसी कनेक्शन का पहला पढ़ना भी `none` लौटाता है। कोई भी पक्ष panic नहीं करता।

## 3. UDP

| नाम | उपयोग | टाइप | अर्थ |
|---|---|---|---|
| `udp-bind` | `(udp-bind host port)` | `(string,int)→Result<udp-socket,NetError>` | सॉकेट बनाता है। केवल भेजने के लिए भी चाहिए (`port` `0` है) |
| `send-to` | `(send-to s host port bytes)` | `(udp-socket,string,int,Vector<int>)→Result<(),NetError>` | एक डेटाग्राम भेजता है। नाम हल होते हैं। यह पता नहीं चलता कि पहुँचा या नहीं (UDP) |
| `recv-from` | `(recv-from s &key timeout)` | `(udp-socket,&key f64)→Result<datagram,NetError>` | अगला डेटाग्राम। `from` `ip:port` है और `send-to` के `host` के रूप में जस का तस पास किया जा सकता है |

स्ट्रिंग और बाइट अनुक्रम के बीच `string->utf8` / `utf8->string` से बदलें ([स्ट्रिंग](collections.md#1-स्ट्रिंग-string))।

```lisp
(let ((s (unwrap (udp-bind "127.0.0.1" 0))))
  (unwrap (send-to s "127.0.0.1" 9999 (string->utf8 "ping")))
  (match (recv-from s :timeout 1.0)
    ((ok d) (println "~a from ~a" (unwrap (utf8->string (bytes d))) (from d)))
    ((err e) (println "~a" (message e)))))
```

## 4. जो नहीं है

- पीयर के प्रमाणपत्र की **विषय के अलावा** कुछ भी पढ़ने का तरीका (SAN, वैधता अवधि, जारीकर्ता)।
- **`select` से एक साथ सॉकेट और चैनल की प्रतीक्षा।** Go की तरह, इसे "पढ़ने वाला टास्क शुरू करें, और उससे चैनल को भरवाएँ" के रूप में लिखें।
- **HTTP/2**। Unix डोमेन **डेटाग्राम** (`SOCK_DGRAM`)।
- **स्ट्रीम द्वारा ले जाई जाने वाली समय सीमाएँ** (ऊपर के कारणों से, घड़ियाँ आर्ग्युमेंट हैं)।
