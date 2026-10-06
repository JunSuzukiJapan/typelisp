<!-- translated-from: docs/ja/reference/functions/network.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# 網路（TCP / TLS / Unix 網域 / UDP）

socket 是[串流](streams-files.md)的一員。連線有 `socket-stream`（字元）／`socket-byte-stream`（位元組）這**同一連線的兩種視圖**，
`read-line`/`write-line`/`read-byte`/`format` 可以直接使用。監聽器是 `socket-listener`。**TCP、TLS、Unix 網域 socket 共用一個型別**（與 Go
的 `net.Conn` 相同）——連線之後的讀寫都一樣，不同的只是建立方式。UDP 不是串流，而是資料包（`udp-socket`）。

**等待的是任務而不是執行緒。** `accept`・`read-line`・`write-string`・`tcp-connect`（包括名稱解析）・`recv-from`，如果尚未就緒，都只讓
*該任務*停下（與 `sleep`/`recv` 相同），其他任務繼續執行。所以可以寫出與 Go 同樣形式的伺服器——每個連線一個 `(task (serve c))`
（[語法參考 12.5](../syntax.md#125-切換發生的位置)）。

## 1. 型別

| 型別 | 實作的 trait | 取得方式 |
|---|---|---|
| `socket-stream` | `CharInput` `PeekInput` `CharOutput` | `tcp-connect` / `tls-connect` / `unix-connect` / `accept` / `char-stream-of` |
| `socket-byte-stream` | `ByteInput` `ByteOutput` | `byte-stream-of` |
| `socket-listener` | `Stream`（`close` / `open-stream-p`） | `tcp-listen` / `tls-listen` / `unix-listen` |
| `udp-socket` | `Stream` | `udp-bind` |
| `datagram` | ——（`bytes`：`Vector<int>`、`from`：`string`） | `recv-from` |
| `NetError` | `Error` | 下列函式的 `Err` |

串流的元素型別每個型別只有一種（與 `file-stream`/`binary-file-stream` 理由相同），所以字元與位元組是不同的型別。`byte-stream-of`/
`char-stream-of` 回傳指向**同一連線**的值，接收緩衝區也是共享的——像 HTTP 那樣以字元讀標頭、以位元組讀本體的用途就這樣寫。`unread-char`
字元之後緊接著讀位元組是錯誤（與檔案相同的規則）。

## 2. TCP / TLS / Unix 網域

| 名稱 | 用法 | 型別 | 意義 |
|---|---|---|---|
| `tcp-connect` | `(tcp-connect host port &key timeout)` | `(string,int,&key f64)→Result<socket-stream,NetError>` | 連線。`host` 可以是名稱也可以是位址。名稱有多個位址時依序嘗試（`localhost` 是 `::1` 與 `127.0.0.1`）。名稱解析失敗、連線被拒、超過 `:timeout` 秒都是 `Err` |
| `tls-connect` | `(tls-connect host port &key timeout ca-file cert-file key-file server-name)` | `(string,int,&key f64 string string string string)→Result<socket-stream,NetError>` | `tcp-connect` 之後進行 TLS。憑證針對 `host` 名稱（連線目標與要驗證的名稱不同時用 `:server-name`）以 Mozilla 的根憑證驗證——傳入 `:ca-file`（PEM）時**只信任其中的憑證**（私有 CA，或是自己的 `tls-listen` 出示的那張憑證本身）。`:cert-file`/`:key-file`（兩者都給或都不給）是伺服器要求時出示的我方憑證（雙向 TLS）。交握在這裡完成，所以伺服器的憑證通不過時這次呼叫回傳 `Err`。結果是一般的 `socket-stream` |
| `unix-connect` | `(unix-connect path)` | `(string)→Result<socket-stream,NetError>` | 連線到 Unix 網域 socket `path`。是本機的，所以沒有交握等待，也沒有逾時 |
| `tcp-listen` | `(tcp-listen host port)` | `(string,int)→Result<socket-listener,NetError>` | 監聽。`"127.0.0.1"` 只限本機，`"0.0.0.0"` 是所有介面。`port` 傳入 `0` 則由 OS 選擇 |
| `tls-listen` | `(tls-listen host port cert-file key-file &key client-ca)` | `(string,int,string,string,&key string)→Result<socket-listener,NetError>` | `tcp-listen` 的 TLS 版本。`cert-file` 是憑證鏈（PEM，自己的在最前面），`key-file` 是私鑰。兩者都在這裡讀取並檢查，所以金鑰不符時不是在第一個用戶端，而是這次呼叫回傳 `Err`。`accept` 在**交握之前**回傳，負責該連線的任務的第一次讀寫完成交握（與 Go 的 `tls.Conn` 相同）——交握慢的用戶端不會妨礙其他 `accept`。傳入 `:client-ca`（PEM）時，**要求所有用戶端**出示由其中的 CA 簽發的憑證（雙向 TLS）。沒有時不要求 |
| `unix-listen` | `(unix-listen path)` | `(string)→Result<socket-listener,NetError>` | 在 `path` 上監聽。**檔案已存在時為 `Err`**（可能屬於正在執行的其他行程，所以不會默默替換）。`close` 會刪除檔案 |
| `accept` | `(accept l &key timeout)` | `(socket-listener,&key f64)→Result<socket-stream,NetError>` | 下一個連線。在連線到來之前讓任務停下。`:timeout` 秒後放棄並回傳 `Err` |
| `wait-readable` | `(wait-readable s secs)` | `(socket-stream,f64)→bool` | 等到可以不等待地讀取，或者 `secs` 秒。`true` 表示前者（包括已緩衝的情況）。為讀取加上計時的手段：`(if (wait-readable c 5.0) (read-line c) ...)`。它保證的是**下一次讀取不會停下**，`read-line` 可能會等待該行的剩餘部分 |
| `wait-writable` | `(wait-writable s secs)` | `(socket-stream,f64)→bool` | 等到可以寫，或者 `secs` 秒 |
| `tls-add-certificate` | `(tls-add-certificate l name cert-file key-file)` | `(socket-listener,string,string,string)→Result<(),NetError>` | 為 `tls-listen` 的監聽器再加一張憑證，出示給要求 `name`（SNI）的用戶端——一個監聽器服務多個網站。憑證鏈是否適用於 `name` 在這裡檢查，不適用時這次呼叫回傳 `Err`。要求沒有人加過的名稱的用戶端、不帶名稱的用戶端得到 `tls-listen` 的憑證 |
| `peer-subject` | `(peer-subject s)` | `(socket-stream)→Option<string>` | 對方憑證的 subject（`CN=client,O=Example,C=JP`，RFC 4514 形式，具體的在前）。雙向 TLS 伺服器得知「誰連線了」的手段（在完成交握的第一次讀取之後）。在用戶端是伺服器憑證的名稱。明文、交握之前、對方沒有出示憑證時為 `none` |
| `requested-server-name` | `(requested-server-name s)` | `(socket-stream)→Option<string>` | 伺服器端的 TLS 連線中用戶端要求的名稱（SNI）。以 `tls-add-certificate` 擁有多個網站的伺服器得知「是哪個網站」。明文、用戶端、沒有名稱（以位址連線）時為 `none` |
| `set-nodelay` | `(set-nodelay s on)` | `(socket-stream,bool)→()` | 關閉 Nagle（`TCP_NODELAY`）。`write-string` 每次都寫到 socket，所以 Nagle 生效時，分兩次寫出標頭與本體的回應會等待對方的延遲 ACK——請求／回應型的協定請設為 `true`。Unix 網域沒有 Nagle，直接成功 |
| `set-keepalive` | `(set-keepalive s on)` | `(socket-stream,bool)→()` | 在閒置時探測對方（`SO_KEEPALIVE`）。偵測沒有關閉就消失的對方（拔掉網路線、主機停機）並重設連線。OS 預設週期很長（多為 2 小時），請與下面的 `set-keepalive-period` 一起使用。Unix 網域沒有它，所以會 panic |
| `set-keepalive-period` | `(set-keepalive-period s secs)` | `(socket-stream,int)→()` | 第一次探測之前的閒置秒數以及探測間隔（整數秒，1 以上）。與 Go 的 `SetKeepAlivePeriod` 一樣，兩者設為同一個值 |
| `socket-error` | `(socket-error s)` | `(socket-stream \| socket-byte-stream)→Option<NetError>` | 如果**對方**破壞了這個連線，回傳其最初的失敗（重設、TLS 警示、寫入中斷線）。健全時為 `none`——對方正常關閉的 EOF 不是失敗。見下文「對方的失敗」 |
| `local-address` | `(local-address s)` | `(socket-stream \| socket-listener \| udp-socket)→string` | 我方的 `host:port`。在 `(tcp-listen h 0)` 之後得知所選埠的手段。Unix 時為路徑（連線端的端點是 `(unnamed)`） |
| `peer-address` | `(peer-address s)` | `(socket-stream)→string` | 對方的 `host:port`，Unix 時為路徑 |
| `shutdown-output` | `(shutdown-output s)` | `(socket-stream)→()` | 只關閉傳送端（半關閉）。對方讀到 EOF，我方還能讀。「請求已全部送出」的訊號。TLS 時也會送出 `close_notify` |
| `byte-stream-of` | `(byte-stream-of s)` | `(socket-stream)→socket-byte-stream` | 同一連線的位元組版 |
| `char-stream-of` | `(char-stream-of b)` | `(socket-byte-stream)→socket-stream` | 同一連線的字元版 |
| `close` | `(close s)` | `Stream` | 把緩衝區全部送出後關閉。GC 不會關閉 |
| `with-connection` | `(with-connection (var host port) body...)` | 巨集 | 連線→本體→關閉。`Result<本體的值, NetError>`（與 `with-open-file` 同形） |

`write-string`/`write-line` **寫完之後才回傳**（與 Go 的 `net.Conn.Write` 相同）。想把零碎的寫入集中起來時，先累積在 `string-output-stream`
中再一次寫出。`listen` 只在接收緩衝區中有東西時為 `true`——`read-char-no-hang` 名副其實地運作。

**對方的失敗不會 panic。** 連線的另一端重設、拒絕 TLS 交握、在寫入途中斷線，這些都不是本程式的錯誤，所以伺服器不會連同其他用戶端一起停止。
最初的失敗記錄在連線上，之後的讀取回傳 `none`（與 EOF 一樣的樣子），寫入無處可達、默默回傳。想區分時用 `socket-error`——與 Go 的
`bufio.Scanner.Err` 同形（`read-item` 回傳 `Option`，沒有別的管道可以傳達）。會 panic 的只有程式本身的錯誤（已關閉的控制代碼、`unread-char`
之後緊接著讀位元組）。

**逾時以明確的引數或 `wait-readable` 處理。** 沒有像 Go 的 `SetReadDeadline` 那樣「讓串流帶有期限、使 `read-line` 失敗」的形式——`read-item`
回傳 `Option<Item>`，沒有回傳錯誤的管道。需要計時的地方有連線、接受、「下一次讀取」這 3 處，各自都有引數。

```lisp
;; 伺服器：每個連線一個任務
(defun serve ((c socket-stream)) ()
  (loop (match (read-line c)
          ((some line) (write-line c line))
          ((none) (break))))
  (close c))

(let ((l (unwrap (tcp-listen "0.0.0.0" 7777))))
  (loop (match (accept l)
          ((ok c) (progn (task (serve c)) ()))
          ((err e) (println "accept: ~a" (message e))))))

;; 用戶端
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

;; TLS 伺服器（憑證例如可以這樣建立：
;;   openssl req -x509 -newkey rsa:2048 -nodes -subj /CN=localhost \
;;           -addext subjectAltName=DNS:localhost -keyout key.pem -out cert.pem）
(let ((l (unwrap (tls-listen "0.0.0.0" 8443 "cert.pem" "key.pem"))))
  (loop (match (accept l)
          ((ok c) (progn (task (serve c)) ()))        ; 直接使用上面的 serve。第一次 read-line 就是交握
          ((err e) (println "accept: ~a" (message e))))))
;; 它的用戶端：信任自己的憑證並連線
(unwrap (tls-connect "localhost" 8443 :timeout 5.0 :ca-file "cert.pem"))

;; 雙向 TLS：伺服器要求由 ca.pem 簽發的用戶端憑證，用戶端出示它
(tls-listen "0.0.0.0" 8443 "cert.pem" "key.pem" :client-ca "ca.pem")
(tls-connect "localhost" 8443 :ca-file "ca.pem" :cert-file "client.pem" :key-file "client-key.pem")
;; 伺服器端，第一次 read-line 之後：誰連線了
(println "~a" (unwrap-or (peer-subject c) "anonymous"))    ; CN=client,O=Example

;; 一個監聽器服務兩個網站（SNI）
(let ((l (unwrap (tls-listen "0.0.0.0" 443 "a.example.pem" "a.key"))))
  (unwrap (tls-add-certificate l "b.example" "b.example.pem" "b.key"))
  ...)                                        ; 在連線端 (requested-server-name c) 說明是哪一個
```

在雙向 TLS 中不出示憑證（或憑證通不過）的用戶端，在 TLS 1.3 下用戶端的交握先於伺服器的判定結束，所以 `tls-connect` 回傳 `Ok`，**第一次讀取
得到 `none`**（警示記錄在 `socket-error` 中）。伺服器端同一連線的第一次讀取也是 `none`。雙方都不會 panic。

## 3. UDP

| 名稱 | 用法 | 型別 | 意義 |
|---|---|---|---|
| `udp-bind` | `(udp-bind host port)` | `(string,int)→Result<udp-socket,NetError>` | 建立 socket。只傳送也需要（`port` 為 `0`） |
| `send-to` | `(send-to s host port bytes)` | `(udp-socket,string,int,Vector<int>)→Result<(),NetError>` | 傳送一個資料包。名稱會被解析。是否送達無從得知（UDP） |
| `recv-from` | `(recv-from s &key timeout)` | `(udp-socket,&key f64)→Result<datagram,NetError>` | 下一個資料包。`from` 是 `ip:port`，可以直接作為 `send-to` 的 `host` |

字串與位元組序列之間的轉換用 `string->utf8` / `utf8->string`（[字串](collections.md#1-字串-string)）。

```lisp
(let ((s (unwrap (udp-bind "127.0.0.1" 0))))
  (unwrap (send-to s "127.0.0.1" 9999 (string->utf8 "ping")))
  (match (recv-from s :timeout 1.0)
    ((ok d) (println "~a from ~a" (unwrap (utf8->string (bytes d))) (from d)))
    ((err e) (println "~a" (message e)))))
```

## 4. 沒有的東西

- 讀取對方憑證中 **subject 以外**內容（SAN、有效期間、簽發者）的手段。
- **以 `select` 同時等待 socket 與 channel** 的形式。與 Go 一樣寫成「啟動一個讀取的任務，讓它把資料送進 channel」。
- **HTTP/2**。Unix 網域的**資料包**（`SOCK_DGRAM`）。
- **讓串流帶有的期限**（由於上述原因，計時以引數處理）。
