# ネットワーク（TCP / TLS / Unix ドメイン / UDP）

ソケットは [ストリーム](streams-files.md) の一員。接続は `socket-stream`（文字）／
`socket-byte-stream`（バイト）という**同じ接続の 2 つの見え方**で、`read-line`/`write-line`/
`read-byte`/`format` がそのまま効く。待ち受けは `socket-listener`。**TCP・TLS・Unix ドメインソケットで
型は 1 つ**（Go の `net.Conn` と同じ）——繋がった後の読み書きは同じで、違うのは作り方だけ。
UDP はストリームでなくデータグラム（`udp-socket`）。

**待つのはタスクであってスレッドではない。** `accept`・`read-line`・`write-string`・
`tcp-connect`（名前解決を含む）・`recv-from` のどれも、用意できていなければ*そのタスク*を
止め（`sleep`/`recv` と同じ）、他のタスクは走り続ける。だから Go と同じ形——接続ごとに
`(task (serve c))`——でサーバが書ける（[構文リファレンス 12.5](../syntax.md#125-切り替わる場所)）。

## 1. 型

| 型 | 実装するトレイト | 得る方法 |
|---|---|---|
| `socket-stream` | `CharInput` `PeekInput` `CharOutput` | `tcp-connect` / `tls-connect` / `unix-connect` / `accept` / `char-stream-of` |
| `socket-byte-stream` | `ByteInput` `ByteOutput` | `byte-stream-of` |
| `socket-listener` | `Stream`（`close` / `open-stream-p`） | `tcp-listen` / `tls-listen` / `unix-listen` |
| `udp-socket` | `Stream` | `udp-bind` |
| `datagram` | ——（`bytes`: `Vector<int>`、`from`: `string`） | `recv-from` |
| `NetError` | `Error` | 下の関数の `Err` |

ストリームの要素型は型ごとに 1 つなので（`file-stream`/`binary-file-stream` と同じ理由）
文字とバイトは別の型。`byte-stream-of`/`char-stream-of` は**同じ接続**を指す値を返し、
受信バッファも共有する——HTTP のようにヘッダを文字で・本体をバイトで読む用途はこれで書く。
文字を `unread-char` した直後のバイト読みはエラー（ファイルと同じ規則）。

## 2. TCP / TLS / Unix ドメイン

| 名前 | 使い方 | 型 | 意味 |
|---|---|---|---|
| `tcp-connect` | `(tcp-connect host port &key timeout)` | `(string,int,&key f64)→Result<socket-stream,NetError>` | 接続する。`host` は名前でもアドレスでも。名前が複数のアドレスを持てば順に試す（`localhost` は `::1` と `127.0.0.1`）。名前解決失敗・接続拒否・`:timeout` 秒の超過は `Err` |
| `tls-connect` | `(tls-connect host port &key timeout ca-file cert-file key-file server-name)` | `(string,int,&key f64 string string string string)→Result<socket-stream,NetError>` | `tcp-connect` の後に TLS。証明書は `host` 名（繋ぐ先と検証する名前が違うなら `:server-name`）に対して Mozilla のルート証明書で検証——`:ca-file`（PEM）を渡せば**その中の証明書だけ**を信頼する（私設 CA、または自分の `tls-listen` が出す証明書そのもの）。`:cert-file`/`:key-file`（両方か無しか）はサーバに求められたとき出すこちらの証明書（相互 TLS）。握手はここで済ませるので、サーバの証明書が通らなければこの呼び出しの `Err`。結果は普通の `socket-stream` |
| `unix-connect` | `(unix-connect path)` | `(string)→Result<socket-stream,NetError>` | Unix ドメインソケット `path` に接続。ローカルなので握手待ちは無く、タイムアウトも無い |
| `tcp-listen` | `(tcp-listen host port)` | `(string,int)→Result<socket-listener,NetError>` | 待ち受ける。`"127.0.0.1"` はこの機械だけ、`"0.0.0.0"` は全インタフェース。`port` に `0` を渡すと OS が選ぶ |
| `tls-listen` | `(tls-listen host port cert-file key-file &key client-ca)` | `(string,int,string,string,&key string)→Result<socket-listener,NetError>` | `tcp-listen` の TLS 版。`cert-file` は証明書チェーン（PEM、自分のものが先頭）、`key-file` は秘密鍵。両方ここで読んで検査するので、鍵が合わなければ最初のクライアントでなくこの呼び出しの `Err`。`accept` は**握手の前に**返り、接続を担当するタスクの最初の読み書きが握手を済ませる（Go の `tls.Conn` と同じ）——握手の遅いクライアントが他の `accept` を止めない。`:client-ca`（PEM）を渡すと、その中の CA が発行した証明書を**全クライアントに要求**する（相互 TLS）。無ければ求めない |
| `unix-listen` | `(unix-listen path)` | `(string)→Result<socket-listener,NetError>` | `path` で待ち受ける。**ファイルが既にあれば `Err`**（走っている別プロセスのものかもしれないので黙って置き換えない）。`close` がファイルを消す |
| `accept` | `(accept l &key timeout)` | `(socket-listener,&key f64)→Result<socket-stream,NetError>` | 次の接続。来るまでタスクを止める。`:timeout` 秒で諦めると `Err` |
| `wait-readable` | `(wait-readable s secs)` | `(socket-stream,f64)→bool` | 待たずに読めるようになるまで、または `secs` 秒。`true` なら前者（バッファ済みも含む）。読みに時計を付ける手段：`(if (wait-readable c 5.0) (read-line c) ...)`。約束するのは**次の読みが止まらない**ことで、`read-line` は行の残りを待ちうる |
| `wait-writable` | `(wait-writable s secs)` | `(socket-stream,f64)→bool` | 書けるようになるまで、または `secs` 秒 |
| `tls-add-certificate` | `(tls-add-certificate l name cert-file key-file)` | `(socket-listener,string,string,string)→Result<(),NetError>` | `tls-listen` の listener にもう 1 枚、`name` を求めた（SNI）クライアントに出す証明書を足す——1 つの listener で複数サイト。チェーンが `name` 用かはここで検査し、違えばこの呼び出しの `Err`。誰も足していない名前・名前無しのクライアントには `tls-listen` の証明書を出す |
| `peer-subject` | `(peer-subject s)` | `(socket-stream)→Option<string>` | 相手の証明書の subject（`CN=client,O=Example,C=JP`、RFC 4514 形式・具体的なものが先）。相互 TLS のサーバが「誰が繋いだか」を知る手段（握手を済ませる最初の読みの後）。クライアント側ならサーバ証明書の名前。平文・握手前・相手が証明書を出していなければ `none` |
| `requested-server-name` | `(requested-server-name s)` | `(socket-stream)→Option<string>` | サーバ側 TLS 接続で、クライアントが求めた名前（SNI）。`tls-add-certificate` で複数サイトを持つサーバが「どのサイトか」を知る。平文・クライアント側・名前無し（アドレスで繋いだ）なら `none` |
| `set-nodelay` | `(set-nodelay s on)` | `(socket-stream,bool)→()` | Nagle を切る（`TCP_NODELAY`）。`write-string` は毎回ソケットまで届くので、Nagle が効いているとヘッダと本体を 2 回に分けて書いた応答が相手の遅延 ACK を待つ——要求/応答型のプロトコルは `true` に。Unix ドメインには Nagle が無く、そのまま成功 |
| `set-keepalive` | `(set-keepalive s on)` | `(socket-stream,bool)→()` | アイドル中に相手を探る（`SO_KEEPALIVE`）。閉じずに消えた相手（ケーブル抜け、ホスト停止）を検出して接続をリセットする。OS 既定の周期は長い（多くは 2 時間）ので下の `set-keepalive-period` と組で。Unix ドメインには無いので panic |
| `set-keepalive-period` | `(set-keepalive-period s secs)` | `(socket-stream,int)→()` | 最初の探りまでのアイドル秒数と探りの間隔（整数秒、1 以上）。Go の `SetKeepAlivePeriod` と同じく両方を同じ値に |
| `socket-error` | `(socket-error s)` | `(socket-stream \| socket-byte-stream)→Option<NetError>` | **相手が**この接続を壊していればその最初の失敗（リセット、TLS のアラート、書き込み中の切断）。健全なら `none`——相手がきれいに閉じた EOF は失敗ではない。下記「相手の失敗」 |
| `local-address` | `(local-address s)` | `(socket-stream \| socket-listener \| udp-socket)→string` | こちら側の `host:port`。`(tcp-listen h 0)` の後で選ばれたポートを知る手段。Unix ならパス（接続した側の端は `(unnamed)`） |
| `peer-address` | `(peer-address s)` | `(socket-stream)→string` | 相手側の `host:port`、Unix ならパス |
| `shutdown-output` | `(shutdown-output s)` | `(socket-stream)→()` | 送信側だけ閉じる（半クローズ）。相手は EOF を読み、こちらはまだ読める。「要求は全部送った」の合図。TLS なら `close_notify` も送る |
| `byte-stream-of` | `(byte-stream-of s)` | `(socket-stream)→socket-byte-stream` | 同じ接続のバイト版 |
| `char-stream-of` | `(char-stream-of b)` | `(socket-byte-stream)→socket-stream` | 同じ接続の文字版 |
| `close` | `(close s)` | `Stream` | バッファを送り切ってから閉じる。GC では閉じない |
| `with-connection` | `(with-connection (var host port) body...)` | マクロ | 接続→本体→閉じる。`Result<本体の値, NetError>`（`with-open-file` と同形） |

`write-string`/`write-line` は**書き切ってから返る**（Go の `net.Conn.Write` と同じ）。
細かい書き込みを束ねたければ `string-output-stream` に溜めてから 1 回で書く。
`listen` は受信バッファに何かあるときだけ `true`——`read-char-no-hang` が名前どおりに動く。

**相手の失敗は panic にならない。** 接続の向こうがリセットしても、TLS の握手を拒んでも、
書いている最中に切っても、それはこのプログラムの誤りではないので、サーバが他のクライアント
ごと止まることはない。最初の失敗は接続に記録され、以後の読みは `none`（EOF と同じ顔）、
書きはどこにも届かず黙って戻る。区別したければ `socket-error`——Go の `bufio.Scanner.Err`
と同じ形（`read-item` が `Option` で、他に伝える口が無い）。panic するのはプログラム自身の
誤り（閉じたハンドル、`unread-char` 直後のバイト読み）だけ。

**タイムアウトは明示の引数か `wait-readable`。** Go の `SetReadDeadline` のように「ストリームに
期限を持たせて `read-line` が失敗する」形は無い——`read-item` は `Option<Item>` を返し、
エラーを返す口が無いから。時計が要るのは接続・受理・「次の読み」の 3 箇所で、それぞれに
引数がある。

```lisp
;; サーバ: 接続ごとに 1 タスク
(defun serve ((c socket-stream)) ()
  (loop (match (read-line c)
          ((some line) (write-line c line))
          ((none) (break))))
  (close c))

(let ((l (unwrap (tcp-listen "0.0.0.0" 7777))))
  (loop (match (accept l)
          ((ok c) (progn (task (serve c)) ()))
          ((err e) (println "accept: ~a" (message e))))))

;; クライアント
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

;; TLS サーバ（証明書は例えば
;;   openssl req -x509 -newkey rsa:2048 -nodes -subj /CN=localhost \
;;           -addext subjectAltName=DNS:localhost -keyout key.pem -out cert.pem）
(let ((l (unwrap (tls-listen "0.0.0.0" 8443 "cert.pem" "key.pem"))))
  (loop (match (accept l)
          ((ok c) (progn (task (serve c)) ()))        ; 上の serve のまま。最初の read-line が握手
          ((err e) (println "accept: ~a" (message e))))))
;; そのクライアント: 自分の証明書を信頼して繋ぐ
(unwrap (tls-connect "localhost" 8443 :timeout 5.0 :ca-file "cert.pem"))

;; 相互 TLS: サーバは ca.pem が発行したクライアント証明書を要求し、クライアントはそれを出す
(tls-listen "0.0.0.0" 8443 "cert.pem" "key.pem" :client-ca "ca.pem")
(tls-connect "localhost" 8443 :ca-file "ca.pem" :cert-file "client.pem" :key-file "client-key.pem")
;; サーバ側、最初の read-line の後: 誰が繋いだか
(println "~a" (unwrap-or (peer-subject c) "anonymous"))    ; CN=client,O=Example

;; 1 つの listener で 2 つのサイト（SNI）
(let ((l (unwrap (tls-listen "0.0.0.0" 443 "a.example.pem" "a.key"))))
  (unwrap (tls-add-certificate l "b.example" "b.example.pem" "b.key"))
  ...)                                        ; 接続側で (requested-server-name c) がどちらか言う
```

相互 TLS で証明書を出さない（または通らない）クライアントは、TLS 1.3 ではクライアント側の握手が
サーバの判定より先に終わるため、`tls-connect` は `Ok` で返り**最初の読みが `none`**（`socket-error`
にアラートが載る）。サーバ側の同じ接続も最初の読みが `none`。どちらの側も panic しない。

## 3. UDP

| 名前 | 使い方 | 型 | 意味 |
|---|---|---|---|
| `udp-bind` | `(udp-bind host port)` | `(string,int)→Result<udp-socket,NetError>` | ソケットを作る。送るだけでも要る（`port` は `0`） |
| `send-to` | `(send-to s host port bytes)` | `(udp-socket,string,int,Vector<int>)→Result<(),NetError>` | 1 データグラムを送る。名前は解決する。届いたかは分からない（UDP） |
| `recv-from` | `(recv-from s &key timeout)` | `(udp-socket,&key f64)→Result<datagram,NetError>` | 次のデータグラム。`from` は `ip:port` で、そのまま `send-to` の `host` に渡せる |

文字列とバイト列の変換は `string->utf8` / `utf8->string`（[文字列](collections.md#1-文字列-string)）。

```lisp
(let ((s (unwrap (udp-bind "127.0.0.1" 0))))
  (unwrap (send-to s "127.0.0.1" 9999 (string->utf8 "ping")))
  (match (recv-from s :timeout 1.0)
    ((ok d) (println "~a from ~a" (unwrap (utf8->string (bytes d))) (from d)))
    ((err e) (println "~a" (message e)))))
```

## 4. 無いもの

- 相手の証明書のうち **subject 以外**（SAN、有効期限、発行者）を読む手段。
- **`select` でソケットと channel を同時に待つ**形。Go と同じく「読むタスクを立てて channel に流す」で書く。
- **HTTP/2**。Unix ドメインの**データグラム**（`SOCK_DGRAM`）。
- **ストリームに持たせる期限**（上記の理由で、時計は引数）。
