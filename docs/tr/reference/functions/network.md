<!-- translated-from: docs/ja/reference/functions/network.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Ağ (TCP / TLS / Unix Domain / UDP)

Soketler [akışların](streams-files.md) üyeleridir. Bir bağlantı, aynı bağlantının **iki görünümü**
olan `socket-stream` (karakterler) ve `socket-byte-stream` (baytlar) ile görülür ve
`read-line`/`write-line`/`read-byte`/`format` bunların üzerinde olduğu gibi çalışır. Bir dinleyici
`socket-listener`'dır. **TCP, TLS ve Unix domain soketleri tek bir türü paylaşır** (Go'nun
`net.Conn`'u gibi): bağlandıktan sonra okuma ve yazma aynıdır; yalnızca nasıl oluşturuldukları
farklıdır. UDP bir akış değil, datagramlardır (`udp-socket`).

**Bekleyen thread değil, task'tir.** `accept`, `read-line`, `write-string`, `tcp-connect` (ad çözümleme
dahil) ve `recv-from`, hazır değillerse *o task'i* durdurur (`sleep`/`recv` gibi) ve diğer task'ler
çalışmaya devam eder. Bu yüzden bir sunucu, Go'daki gibi her bağlantı için `(task (serve c))` ile aynı
biçimde yazılabilir ([Sözdizimi Başvurusu 12.5](../syntax.md#125-tasklerin-geçiş-yaptığı-yerler)).

## 1. Türler

| Tür | Gerçekleştirilen trait'ler | Nasıl elde edilir |
|---|---|---|
| `socket-stream` | `CharInput` `PeekInput` `CharOutput` | `tcp-connect` / `tls-connect` / `unix-connect` / `accept` / `char-stream-of` |
| `socket-byte-stream` | `ByteInput` `ByteOutput` | `byte-stream-of` |
| `socket-listener` | `Stream` (`close` / `open-stream-p`) | `tcp-listen` / `tls-listen` / `unix-listen` |
| `udp-socket` | `Stream` | `udp-bind` |
| `datagram` | — (`bytes`: `Vector<int>`, `from`: `string`) | `recv-from` |
| `NetError` | `Error` | Aşağıdaki fonksiyonların `Err`'i |

Bir akış türünün tek bir eleman türü vardır (`file-stream`/`binary-file-stream` ile aynı nedenle); bu
yüzden karakterler ve baytlar farklı türlerdir. `byte-stream-of`/`char-stream-of`, **aynı bağlantıyı**
gösteren ve alma tamponunu paylaşan değerler döndürür: başlıkların karakter, gövdenin bayt olarak
okunduğu HTTP gibi şeyleri bu şekilde yazarsınız. Bir karakterin `unread-char`'ından hemen sonra bir
bayt okumak hatadır (dosyalardaki kuralın aynısı).

## 2. TCP / TLS / Unix domain

| Ad | Kullanım | Tür | Anlamı |
|---|---|---|---|
| `tcp-connect` | `(tcp-connect host port &key timeout)` | `(string,int,&key f64)→Result<socket-stream,NetError>` | Bağlanır. `host` bir ad ya da adres olabilir. Bir adın birden çok adresi varsa sırayla denenirler (`localhost` için `::1` ve `127.0.0.1`). Başarısız bir ad araması, reddedilen bir bağlantı ya da `:timeout` saniyenin aşılması `Err` verir |
| `tls-connect` | `(tls-connect host port &key timeout ca-file cert-file key-file server-name)` | `(string,int,&key f64 string string string string)→Result<socket-stream,NetError>` | `tcp-connect`'ten sonra TLS. Sertifika, Mozilla'nın kök sertifikalarıyla `host` adına karşı doğrulanır (doğrulanacak ad bağlandığınız yerden farklıysa `:server-name`). `:ca-file` (PEM) verilirse **yalnızca içindeki sertifikalara** güvenir (özel bir CA ya da kendi `tls-listen`'inizin sunduğu sertifikanın kendisi). `:cert-file`/`:key-file` (ikisi birden ya da hiçbiri) bizim sertifikamızdır ve sunucu istediğinde sunulur (karşılıklı TLS). El sıkışma burada tamamlanır; bu yüzden sunucunun sertifikası geçmezse bu çağrı `Err` döndürür. Sonuç sıradan bir `socket-stream`'dir |
| `unix-connect` | `(unix-connect path)` | `(string)→Result<socket-stream,NetError>` | `path` Unix domain soketine bağlanır. Yereldir; bu yüzden el sıkışma beklemesi ve zaman aşımı yoktur |
| `tcp-listen` | `(tcp-listen host port)` | `(string,int)→Result<socket-listener,NetError>` | Dinler. `"127.0.0.1"` yalnızca bu makine, `"0.0.0.0"` her arayüzdür. `port` olarak `0` geçirmek seçimi işletim sistemine bırakır |
| `tls-listen` | `(tls-listen host port cert-file key-file &key client-ca)` | `(string,int,string,string,&key string)→Result<socket-listener,NetError>` | `tcp-listen`'in TLS sürümü. `cert-file` sertifika zinciridir (PEM, önce kendi sertifikası), `key-file` özel anahtardır. İkisi de burada okunur ve denetlenir; bu yüzden eşleşmeyen bir anahtar, ilk istemcide değil, bu çağrıdan `Err` verir. `accept` **el sıkışmadan önce** döner ve bağlantıyı işleyen task'in ilk okuması ya da yazması el sıkışmayı tamamlar (Go'nun `tls.Conn`'u gibi); bu yüzden el sıkışması yavaş bir istemci diğer `accept`'leri tutmaz. `:client-ca` (PEM) verilirse **her istemcinin** içindeki bir CA'nın verdiği bir sertifika sunmasını **ister** (karşılıklı TLS). Verilmezse hiçbiri istenmez |
| `unix-listen` | `(unix-listen path)` | `(string)→Result<socket-listener,NetError>` | `path`'te dinler. **Dosya zaten varsa `Err`** (çalışan başka bir sürece ait olabilir; bu yüzden sessizce değiştirilmez). `close` dosyayı kaldırır |
| `accept` | `(accept l &key timeout)` | `(socket-listener,&key f64)→Result<socket-stream,NetError>` | Sıradaki bağlantı. Biri gelene kadar task'i durdurur. `:timeout` saniye sonra `Err` ile vazgeçer |
| `wait-readable` | `(wait-readable s secs)` | `(socket-stream,f64)→bool` | Beklemeden okunabilene ya da `secs` saniyeye kadar. `true`, ilkini ifade eder (tamponda zaten bulunan veri dahil). Bir okumaya saat koymanın yolu: `(if (wait-readable c 5.0) (read-line c) ...)`. Verdiği söz, **bir sonraki okumanın durmayacağıdır**; `read-line` satırın geri kalanını bekleyebilir |
| `wait-writable` | `(wait-writable s secs)` | `(socket-stream,f64)→bool` | Yazılabilene ya da `secs` saniyeye kadar |
| `tls-add-certificate` | `(tls-add-certificate l name cert-file key-file)` | `(socket-listener,string,string,string)→Result<(),NetError>` | Bir `tls-listen` dinleyicisine, `name` isteyen istemcilere (SNI) sunulan bir sertifika daha ekler: tek dinleyicide birkaç site. Zincirin `name` için olup olmadığı burada denetlenir ve değilse bu çağrı `Err` döndürür. Kimsenin eklemediği bir ad isteyen ya da hiç ad istemeyen istemciler `tls-listen`'in sertifikasını alır |
| `peer-subject` | `(peer-subject s)` | `(socket-stream)→Option<string>` | Karşı tarafın sertifikasının konusu (`CN=client,O=Example,C=JP`, RFC 4514 biçimi, en özelden başlayarak). Karşılıklı TLS sunucusunun "kim bağlandı"yı öğrenme yolu (el sıkışmayı tamamlayan ilk okumadan sonra). İstemci tarafında sunucu sertifikasının adı. Düz bağlantılar, el sıkışmadan önce ya da karşı taraf sertifika sunmadıysa `none` |
| `requested-server-name` | `(requested-server-name s)` | `(socket-stream)→Option<string>` | Sunucu tarafı bir TLS bağlantısında, istemcinin istediği ad (SNI). `tls-add-certificate` ile eklenen birkaç siteli bir sunucunun "hangi site"yi öğrenme yolu. Düz bağlantılar, istemci tarafı ya da ad yoksa (adresle bağlanılmışsa) `none` |
| `set-nodelay` | `(set-nodelay s on)` | `(socket-stream,bool)→()` | Nagle'ı kapatır (`TCP_NODELAY`). `write-string` her seferinde soketin ta kendisine gider; bu yüzden Nagle açıkken, başlık ve gövde olmak üzere iki parça yazılan bir yanıt karşı tarafın gecikmeli ACK'ini bekler: istek/yanıt protokolleri için `true` kullanın. Unix domain soketlerinde Nagle yoktur ve çağrı yalnızca başarılı olur |
| `set-keepalive` | `(set-keepalive s on)` | `(socket-stream,bool)→()` | Boştayken karşı tarafı yoklar (`SO_KEEPALIVE`). Kapatmadan ortadan kaybolan bir karşı tarafı (çekilmiş bir kablo, durmuş bir host) algılar ve bağlantıyı sıfırlar. İşletim sisteminin varsayılan süresi uzundur (çoğunlukla 2 saat); bu yüzden aşağıdaki `set-keepalive-period` ile birlikte kullanın. Unix domain soketlerinde yoktur; bu yüzden panic olur |
| `set-keepalive-period` | `(set-keepalive-period s secs)` | `(socket-stream,int)→()` | İlk yoklamadan önceki boşta saniyeleri ve yoklamalar arasındaki aralık (tam saniye, en az 1). Go'nun `SetKeepAlivePeriod`'u gibi ikisi de aynı değere ayarlanır |
| `socket-error` | `(socket-error s)` | `(socket-stream \| socket-byte-stream)→Option<NetError>` | Bu bağlantıyı **karşı taraf** bozduysa ilk başarısızlığı (bir sıfırlama, bir TLS uyarısı, yazma sırasında bir kopma). Sağlıklıysa `none`: karşı tarafın temiz kapanmasından gelen bir EOF başarısızlık değildir. Aşağıdaki "Karşı taraf başarısızlıkları"na bakın |
| `local-address` | `(local-address s)` | `(socket-stream \| socket-listener \| udp-socket)→string` | Bizim tarafın `host:port`'u. `(tcp-listen h 0)`'dan sonra seçilen portu öğrenmenin yolu. Unix soketleri için yol (bağlanan uç `(unnamed)`'dır) |
| `peer-address` | `(peer-address s)` | `(socket-stream)→string` | Karşı tarafın `host:port`'u ya da Unix soketleri için yol |
| `shutdown-output` | `(shutdown-output s)` | `(socket-stream)→()` | Yalnızca gönderme tarafını kapatır (yarım kapatma). Karşı taraf EOF okur ve bu taraf hâlâ okuyabilir. "Tüm isteği gönderdim" işareti. TLS için ayrıca `close_notify` gönderir |
| `byte-stream-of` | `(byte-stream-of s)` | `(socket-stream)→socket-byte-stream` | Aynı bağlantının bayt sürümü |
| `char-stream-of` | `(char-stream-of b)` | `(socket-byte-stream)→socket-stream` | Aynı bağlantının karakter sürümü |
| `close` | `(close s)` | `Stream` | Tamponu gönderir, sonra kapatır. GC onu kapatmaz |
| `with-connection` | `(with-connection (var host port) body...)` | Makro | Bağlan, gövdeyi çalıştır, kapat. `Result<gövdenin değeri, NetError>` (`with-open-file` ile aynı biçim) |

`write-string`/`write-line`, **her şey yazıldıktan sonra döner** (Go'nun `net.Conn.Write`'ı gibi).
Küçük yazmaları bir araya getirmek için onları bir `string-output-stream` içinde biriktirip bir kerede
yazın. `listen`, yalnızca alma tamponunda bir şey olduğunda `true`'dur; bu yüzden `read-char-no-hang`
adının dediği gibi çalışır.

**Karşı taraf başarısızlıkları panic olmaz.** Diğer uç bağlantıyı sıfırlarsa, TLS el sıkışmasını
reddederse ya da yazma sırasında koparırsa bu bu programdaki bir hata değildir; bu yüzden sunucu diğer
istemcileriyle birlikte durmaz. İlk başarısızlık bağlantıya kaydedilir; sonraki okumalar `none`
döndürür (EOF gibi görünür) ve yazmalar hiçbir yere gitmez ve sessizce döner. Bunları ayırt etmek için
Go'nun `bufio.Scanner.Err`'iyle aynı biçimde olan `socket-error`'ı kullanın (`read-item` bir
`Option`'dır ve bildirim yapabileceği başka bir kanalı yoktur). Yalnızca programın kendi hataları panic
olur (kapatılmış bir tanıtıcı, `unread-char`'dan hemen sonra bir bayt okumak).

**Zaman aşımları açık bağımsız değişkenler ya da `wait-readable`'dır.** Go'nun `SetReadDeadline`'ı gibi,
bir akışın son tarih taşıdığı ve `read-line`'ın başarısız olduğu bir biçim yoktur: `read-item`,
`Option<Item>` döndürür ve hata döndürmenin bir yolu yoktur. Saat üç yerde gerekir: bağlanma, kabul
etme ve "sıradaki okuma"; her birinin bunun için bir bağımsız değişkeni vardır.

```lisp
;; server: one task per connection
(defun serve ((c socket-stream)) ()
  (loop (match (read-line c)
          ((some line) (write-line c line))
          ((none) (break))))
  (close c))

(let ((l (unwrap (tcp-listen "0.0.0.0" 7777))))
  (loop (match (accept l)
          ((ok c) (progn (task (serve c)) ()))
          ((err e) (println "accept: ~a" (message e))))))

;; client
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

;; TLS server (the certificate can be made, for example, with
;;   openssl req -x509 -newkey rsa:2048 -nodes -subj /CN=localhost \
;;           -addext subjectAltName=DNS:localhost -keyout key.pem -out cert.pem)
(let ((l (unwrap (tls-listen "0.0.0.0" 8443 "cert.pem" "key.pem"))))
  (loop (match (accept l)
          ((ok c) (progn (task (serve c)) ()))        ; the serve above as it is; the first read-line is the handshake
          ((err e) (println "accept: ~a" (message e))))))
;; its client: connect trusting its own certificate
(unwrap (tls-connect "localhost" 8443 :timeout 5.0 :ca-file "cert.pem"))

;; mutual TLS: the server requires a client certificate issued by ca.pem, and the client presents one
(tls-listen "0.0.0.0" 8443 "cert.pem" "key.pem" :client-ca "ca.pem")
(tls-connect "localhost" 8443 :ca-file "ca.pem" :cert-file "client.pem" :key-file "client-key.pem")
;; on the server side, after the first read-line: who connected
(println "~a" (unwrap-or (peer-subject c) "anonymous"))    ; CN=client,O=Example

;; two sites on one listener (SNI)
(let ((l (unwrap (tls-listen "0.0.0.0" 443 "a.example.pem" "a.key"))))
  (unwrap (tls-add-certificate l "b.example" "b.example.pem" "b.key"))
  ...)                                        ; on the connection, (requested-server-name c) says which
```

Karşılıklı TLS'de, sertifika sunmayan (ya da geçmeyen) bir istemci, TLS 1.3 altında sunucu karar
vermeden önce el sıkışmanın kendi tarafını bitirir; bu yüzden `tls-connect` `Ok` döndürür ve **ilk okuma
`none` döndürür** (uyarı `socket-error` içindedir). Sunucu tarafında aynı bağlantının ilk okuması da
`none` döndürür. Hiçbir taraf panic olmaz.

## 3. UDP

| Ad | Kullanım | Tür | Anlamı |
|---|---|---|---|
| `udp-bind` | `(udp-bind host port)` | `(string,int)→Result<udp-socket,NetError>` | Bir soket oluşturur. Yalnızca göndermek için bile gereklidir (`port` `0`'dır) |
| `send-to` | `(send-to s host port bytes)` | `(udp-socket,string,int,Vector<int>)→Result<(),NetError>` | Bir datagram gönderir. Adlar çözümlenir. Ulaşıp ulaşmadığı bilinmez (UDP) |
| `recv-from` | `(recv-from s &key timeout)` | `(udp-socket,&key f64)→Result<datagram,NetError>` | Sıradaki datagram. `from`, `ip:port`'tur ve `send-to`'nun `host`'u olarak olduğu gibi geçirilebilir |

String'ler ile bayt dizileri arasında `string->utf8` / `utf8->string` ile dönüşüm yapın
([String'ler](collections.md#1-stringler-string)).

```lisp
(let ((s (unwrap (udp-bind "127.0.0.1" 0))))
  (unwrap (send-to s "127.0.0.1" 9999 (string->utf8 "ping")))
  (match (recv-from s :timeout 1.0)
    ((ok d) (println "~a from ~a" (unwrap (utf8->string (bytes d))) (from d)))
    ((err e) (println "~a" (message e)))))
```

## 4. Bulunmayanlar

- Karşı tarafın sertifikasının **konu dışında** herhangi bir şeyini (SAN, geçerlilik süresi, veren)
  okuma yolu.
- **`select` ile bir soketi ve bir kanalı aynı anda bekleme.** Go'daki gibi "okuyan bir task başlat ve
  bir kanalı beslemesini sağla" olarak yazın.
- **HTTP/2**. Unix domain **datagramları** (`SOCK_DGRAM`).
- **Bir akışın taşıdığı son tarihler** (yukarıdaki nedenlerle saatler bağımsız değişkenlerdir).
