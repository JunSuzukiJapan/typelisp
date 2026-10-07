<!-- translated-from: docs/ja/reference/functions/network.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Jaringan (TCP / TLS / Domain Unix / UDP)

Socket adalah anggota [stream](streams-files.md). Sebuah koneksi dilihat dalam **dua pandangan atas
koneksi yang sama**, `socket-stream` (karakter) dan `socket-byte-stream` (byte), dan
`read-line`/`write-line`/`read-byte`/`format` bekerja padanya apa adanya. Listener adalah
`socket-listener`. **TCP, TLS, dan socket domain Unix berbagi satu tipe** (seperti `net.Conn` pada
Go): setelah tersambung, membaca dan menulis sama saja, dan hanya cara pembuatannya yang berbeda.
UDP bukan stream melainkan datagram (`udp-socket`).

**Yang menunggu adalah task, bukan thread.** `accept`, `read-line`, `write-string`, `tcp-connect`
(termasuk resolusi nama), dan `recv-from` semuanya menghentikan *task itu* jika belum siap (seperti
`sleep`/`recv`), dan task lain terus berjalan. Itulah sebabnya server dapat ditulis dalam bentuk yang
sama seperti di Go, `(task (serve c))` per koneksi
([Referensi Sintaks 12.5](../syntax.md#125-tempat-task-berpindah)).

## 1. Tipe

| Tipe | Trait yang diimplementasikan | Cara memperolehnya |
|---|---|---|
| `socket-stream` | `CharInput` `PeekInput` `CharOutput` | `tcp-connect` / `tls-connect` / `unix-connect` / `accept` / `char-stream-of` |
| `socket-byte-stream` | `ByteInput` `ByteOutput` | `byte-stream-of` |
| `socket-listener` | `Stream` (`close` / `open-stream-p`) | `tcp-listen` / `tls-listen` / `unix-listen` |
| `udp-socket` | `Stream` | `udp-bind` |
| `datagram` | — (`bytes`: `Vector<int>`, `from`: `string`) | `recv-from` |
| `NetError` | `Error` | `Err` dari fungsi-fungsi di bawah |

Tipe stream memiliki satu tipe elemen (karena alasan yang sama seperti `file-stream`/
`binary-file-stream`), sehingga karakter dan byte adalah tipe yang berbeda. `byte-stream-of`/
`char-stream-of` mengembalikan nilai yang menunjuk **koneksi yang sama** dan berbagi buffer
penerimaan: beginilah Anda menulis hal seperti HTTP, tempat header dibaca sebagai karakter dan badan
sebagai byte. Membaca byte tepat setelah `unread-char` pada karakter adalah kesalahan (aturan yang
sama seperti pada berkas).

## 2. TCP / TLS / Unix domain

| Nama | Penggunaan | Tipe | Arti |
|---|---|---|---|
| `tcp-connect` | `(tcp-connect host port &key timeout)` | `(string,int,&key f64)→Result<socket-stream,NetError>` | Menyambung. `host` dapat berupa nama atau alamat. Jika sebuah nama memiliki beberapa alamat, alamat-alamat itu dicoba bergiliran (`localhost` adalah `::1` dan `127.0.0.1`). Pencarian nama yang gagal, koneksi yang ditolak, atau melebihi `:timeout` detik menghasilkan `Err` |
| `tls-connect` | `(tls-connect host port &key timeout ca-file cert-file key-file server-name)` | `(string,int,&key f64 string string string string)→Result<socket-stream,NetError>` | TLS setelah `tcp-connect`. Sertifikat diverifikasi terhadap nama `host` (`:server-name` jika nama yang diverifikasi berbeda dari tempat Anda menyambung) dengan sertifikat akar Mozilla. Jika `:ca-file` (PEM) diberikan, ia memercayai **hanya sertifikat di dalamnya** (CA privat, atau sertifikat yang sama dengan yang disajikan `tls-listen` Anda sendiri). `:cert-file`/`:key-file` (keduanya atau tidak sama sekali) adalah sertifikat kita, yang disajikan ketika server memintanya (TLS timbal balik). Jabat tangan selesai di sini, sehingga jika sertifikat server tidak lolos, pemanggilan ini mengembalikan `Err`. Hasilnya adalah `socket-stream` biasa |
| `unix-connect` | `(unix-connect path)` | `(string)→Result<socket-stream,NetError>` | Menyambung ke socket domain Unix `path`. Ia lokal, sehingga tidak ada penantian jabat tangan dan tidak ada batas waktu |
| `tcp-listen` | `(tcp-listen host port)` | `(string,int)→Result<socket-listener,NetError>` | Mendengarkan. `"127.0.0.1"` hanya mesin ini, `"0.0.0.0"` setiap antarmuka. Memberikan `0` sebagai `port` membiarkan OS memilih |
| `tls-listen` | `(tls-listen host port cert-file key-file &key client-ca)` | `(string,int,string,string,&key string)→Result<socket-listener,NetError>` | Versi TLS dari `tcp-listen`. `cert-file` adalah rantai sertifikat (PEM, sertifikat sendiri lebih dulu), dan `key-file` kunci privat. Keduanya dibaca dan diperiksa di sini, sehingga kunci yang tidak cocok menghasilkan `Err` dari pemanggilan ini, bukan pada klien pertama. `accept` kembali **sebelum jabat tangan**, dan pembacaan atau penulisan pertama oleh task yang menangani koneksi menyelesaikan jabat tangan (seperti `tls.Conn` pada Go), sehingga klien yang lambat berjabat tangan tidak menahan `accept` lain. Jika `:client-ca` (PEM) diberikan, ia **mewajibkan setiap klien** menyajikan sertifikat yang diterbitkan oleh sebuah CA di dalamnya (TLS timbal balik). Tanpanya, tidak ada yang diminta |
| `unix-listen` | `(unix-listen path)` | `(string)→Result<socket-listener,NetError>` | Mendengarkan di `path`. **`Err` jika berkasnya sudah ada** (mungkin milik proses lain yang sedang berjalan, sehingga tidak diganti diam-diam). `close` menghapus berkasnya |
| `accept` | `(accept l &key timeout)` | `(socket-listener,&key f64)→Result<socket-stream,NetError>` | Koneksi berikutnya. Menghentikan task sampai ada yang datang. Menyerah dengan `Err` setelah `:timeout` detik |
| `wait-readable` | `(wait-readable s secs)` | `(socket-stream,f64)→bool` | Sampai dapat dibaca tanpa menunggu, atau `secs` detik. `true` berarti yang pertama (termasuk data yang sudah di-buffer). Cara memasang jam pada pembacaan: `(if (wait-readable c 5.0) (read-line c) ...)`. Yang dijanjikannya adalah bahwa **pembacaan berikutnya tidak berhenti**; `read-line` mungkin menunggu sisa barisnya |
| `wait-writable` | `(wait-writable s secs)` | `(socket-stream,f64)→bool` | Sampai dapat ditulis, atau `secs` detik |
| `tls-add-certificate` | `(tls-add-certificate l name cert-file key-file)` | `(socket-listener,string,string,string)→Result<(),NetError>` | Menambahkan satu sertifikat lagi ke listener `tls-listen`, yang disajikan kepada klien yang meminta `name` (SNI): beberapa situs pada satu listener. Apakah rantainya untuk `name` diperiksa di sini, dan jika tidak, pemanggilan ini mengembalikan `Err`. Klien yang meminta nama yang tidak ditambahkan siapa pun, atau tanpa nama, mendapat sertifikat dari `tls-listen` |
| `peer-subject` | `(peer-subject s)` | `(socket-stream)→Option<string>` | Subjek sertifikat pihak seberang (`CN=client,O=Example,C=JP`, bentuk RFC 4514, paling spesifik lebih dulu). Cara server TLS timbal balik mengetahui "siapa yang tersambung" (setelah pembacaan pertama, yang menyelesaikan jabat tangan). Di sisi klien, nama sertifikat server. `none` untuk koneksi polos, sebelum jabat tangan, atau jika pihak seberang tidak menyajikan sertifikat |
| `requested-server-name` | `(requested-server-name s)` | `(socket-stream)→Option<string>` | Pada koneksi TLS sisi server, nama yang diminta klien (SNI). Cara server dengan beberapa situs yang ditambahkan oleh `tls-add-certificate` mengetahui "situs yang mana". `none` untuk koneksi polos, di sisi klien, atau tanpa nama (tersambung dengan alamat) |
| `set-nodelay` | `(set-nodelay s on)` | `(socket-stream,bool)→()` | Mematikan Nagle (`TCP_NODELAY`). `write-string` selalu langsung sampai ke socket, sehingga dengan Nagle menyala, respons yang ditulis dalam dua bagian, header dan badan, menunggu ACK tertunda dari pihak seberang: gunakan `true` untuk protokol permintaan/respons. Socket domain Unix tidak memiliki Nagle, dan ia sekadar berhasil |
| `set-keepalive` | `(set-keepalive s on)` | `(socket-stream,bool)→()` | Menyelidiki pihak seberang saat menganggur (`SO_KEEPALIVE`). Mendeteksi pihak seberang yang lenyap tanpa menutup (kabel dicabut, host berhenti) dan mereset koneksi. Periode bawaan OS panjang (sering 2 jam), jadi pasangkan dengan `set-keepalive-period` di bawah. Socket domain Unix tidak memilikinya, sehingga ia melakukan panic |
| `set-keepalive-period` | `(set-keepalive-period s secs)` | `(socket-stream,int)→()` | Detik menganggur sebelum penyelidikan pertama dan interval antarpenyelidikan (detik bulat, minimal 1). Seperti `SetKeepAlivePeriod` pada Go, keduanya disetel ke nilai yang sama |
| `socket-error` | `(socket-error s)` | `(socket-stream \| socket-byte-stream)→Option<NetError>` | Jika **pihak seberang** memutus koneksi ini, kegagalan pertamanya (reset, peringatan TLS, pemutusan saat penulisan). `none` jika sehat: EOF dari pihak seberang yang menutup dengan baik bukan kegagalan. Lihat "Kegagalan pihak seberang" di bawah |
| `local-address` | `(local-address s)` | `(socket-stream \| socket-listener \| udp-socket)→string` | `host:port` sisi kita. Cara mengetahui port yang dipilih setelah `(tcp-listen h 0)`. Untuk socket Unix, path (ujung yang menyambung adalah `(unnamed)`) |
| `peer-address` | `(peer-address s)` | `(socket-stream)→string` | `host:port` pihak seberang, atau path untuk socket Unix |
| `shutdown-output` | `(shutdown-output s)` | `(socket-stream)→()` | Menutup hanya sisi pengiriman (half-close). Pihak seberang membaca EOF, dan sisi ini masih dapat membaca. Sinyal untuk "saya sudah mengirim seluruh permintaan". Untuk TLS ia juga mengirim `close_notify` |
| `byte-stream-of` | `(byte-stream-of s)` | `(socket-stream)→socket-byte-stream` | Versi byte dari koneksi yang sama |
| `char-stream-of` | `(char-stream-of b)` | `(socket-byte-stream)→socket-stream` | Versi karakter dari koneksi yang sama |
| `close` | `(close s)` | `Stream` | Mengirim keluar buffer, lalu menutup. GC tidak menutupnya |
| `with-connection` | `(with-connection (var host port) body...)` | Makro | Menyambung, menjalankan badan, menutup. `Result<nilai dari badan, NetError>` (bentuk yang sama seperti `with-open-file`) |

`write-string`/`write-line` **kembali setelah semuanya tertulis** (seperti `net.Conn.Write` pada
Go). Untuk menggabungkan penulisan kecil, kumpulkan dalam `string-output-stream` dan tulis sekali.
`listen` bernilai `true` hanya ketika ada sesuatu di buffer penerimaan, sehingga
`read-char-no-hang` bekerja seperti namanya.

**Kegagalan pihak seberang tidak menyebabkan panic.** Jika ujung lain mereset koneksi, menolak jabat
tangan TLS, atau memutusnya di tengah penulisan, itu bukan kesalahan program ini, sehingga server
tidak berhenti bersama klien-kliennya yang lain. Kegagalan pertama dicatat pada koneksi; pembacaan
berikutnya mengembalikan `none` (tampak sama seperti EOF), dan penulisan tidak ke mana-mana dan
kembali tanpa suara. Untuk membedakannya, gunakan `socket-error`, bentuk yang sama seperti
`bufio.Scanner.Err` pada Go (`read-item` adalah `Option` dan tidak memiliki saluran lain untuk
melapor). Hanya kesalahan program itu sendiri yang melakukan panic (handle yang ditutup, membaca byte
tepat setelah `unread-char`).

**Batas waktu adalah argumen eksplisit atau `wait-readable`.** Tidak ada bentuk, seperti
`SetReadDeadline` pada Go, tempat stream membawa tenggat dan `read-line` gagal: `read-item`
mengembalikan `Option<Item>` dan tidak punya cara mengembalikan kesalahan. Jam diperlukan di tiga
tempat, menyambung, menerima, dan "pembacaan berikutnya", dan masing-masing memiliki argumennya.

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

Pada TLS timbal balik, klien yang tidak menyajikan sertifikat (atau sertifikatnya tidak lolos)
menyelesaikan sisi jabat tangannya sendiri sebelum server memutuskan, pada TLS 1.3, sehingga
`tls-connect` mengembalikan `Ok` dan **pembacaan pertama mengembalikan `none`** (dengan peringatan
pada `socket-error`). Di sisi server, pembacaan pertama pada koneksi yang sama juga mengembalikan
`none`. Tidak ada sisi yang melakukan panic.

## 3. UDP

| Nama | Penggunaan | Tipe | Arti |
|---|---|---|---|
| `udp-bind` | `(udp-bind host port)` | `(string,int)→Result<udp-socket,NetError>` | Membuat socket. Diperlukan bahkan hanya untuk mengirim (`port` adalah `0`) |
| `send-to` | `(send-to s host port bytes)` | `(udp-socket,string,int,Vector<int>)→Result<(),NetError>` | Mengirim satu datagram. Nama diselesaikan. Apakah sampai tidak diketahui (UDP) |
| `recv-from` | `(recv-from s &key timeout)` | `(udp-socket,&key f64)→Result<datagram,NetError>` | Datagram berikutnya. `from` adalah `ip:port` dan dapat diserahkan apa adanya sebagai `host` pada `send-to` |

Konversikan antara string dan urutan byte dengan `string->utf8` / `utf8->string`
([String](collections.md#1-string-string)).

```lisp
(let ((s (unwrap (udp-bind "127.0.0.1" 0))))
  (unwrap (send-to s "127.0.0.1" 9999 (string->utf8 "ping")))
  (match (recv-from s :timeout 1.0)
    ((ok d) (println "~a from ~a" (unwrap (utf8->string (bytes d))) (from d)))
    ((err e) (println "~a" (message e)))))
```

## 4. Yang tidak ada

- Cara membaca apa pun dari sertifikat pihak seberang **selain subjek** (SAN, masa berlaku,
  penerbit).
- **Menunggu socket dan kanal sekaligus dengan `select`.** Seperti di Go, tulis sebagai "mulai task
  yang membaca, dan biarkan ia mengisi kanal".
- **HTTP/2**. **Datagram** domain Unix (`SOCK_DGRAM`).
- **Tenggat yang dibawa oleh stream** (karena alasan di atas, jam adalah argumen).
