# ファイル I/O、ストリーム、ネットワーク

ファイルの読み書き、パス名、ソケット通信の基本的な書き方を説明します。関数の一覧は
[ストリームとファイル](../reference/functions/streams-files.md) と
[ネットワーク](../reference/functions/network.md) にあります。

## 1. 失敗は `Result` で返る

ファイルを開く・接続するといった、環境しだいで失敗する操作は `Result` を返します。ファイルが
無いことはプログラムの誤りではないので、`panic` にはなりません。結果は `match` で分けます。

```lisp
(match (read-file-string "config.txt")
  ((ok text) (println "~a" text))
  ((err e) (println "error: ~a" (message e))))
;; ファイルが無ければ:
;; error: config.txt: No such file or directory (os error 2)
```

失敗しないと分かっている場面や、失敗したら止まってよい小さなスクリプトでは `unwrap` で
取り出せます。`Err` だった場合は `panic` します。

## 2. ファイル全体を読み書きする

いちばん手軽なのは、ファイル全体を 1 回で扱う関数です。

```lisp
(unwrap (write-file-string "copy.txt" "abc\n"))
(read-file-string "copy.txt")      ; => (ok "abc\n")
(read-file-lines "copy.txt")       ; => (ok #<vector<string> abc>)
```

## 3. ストリームで読み書きする

少しずつ読み書きするときは `with-open-file` でストリームを開きます。本体を抜けると、どう抜けても
ストリームは閉じられます。値は `Result<本体の値, FileError>` です。

```lisp
;; 書く
(match (with-open-file (out "notes.txt" direction-output)
         (write-line out "first")
         (write-line out "second")
         (format out "total: ~a~%" 2))
  ((ok _) ())
  ((err e) (println "write failed: ~a" (message e))))

;; 1 行ずつ読む
(match (with-open-file (in "notes.txt" direction-input)
         (let ((n 0))
           (loop (match (read-line in)
                   ((some line) (progn (setf n (+ n 1)) (println "~a: ~a" n line)))
                   ((none) (break))))
           n))
  ((ok n) (println "~a lines" n))
  ((err e) (println "read failed: ~a" (message e))))
```

- 開く向きは `direction-input`（読む）・`direction-output`（書く。既存の内容は消える）・
  `direction-append`（末尾に追記）の 3 つです。
- `read-line` はファイルの終わりで `none` を返します。
- `with-open-file` を使わずに `open-file` で開いた場合は、`close` を必ず呼んでください。
  GC はストリームを閉じません。

バイト単位で読み書きするときは `open-binary-input` / `open-binary-output` で開き、
`read-byte` / `write-byte` を使います。文字のストリームとバイトのストリームは別の型なので、
文字ストリームからバイトを読もうとすると型エラーになります。

### 文字列をストリームとして使う

```lisp
(with-output-to-string (s)
  (write-string s "a")
  (write-string s "b"))                  ; => "ab"

(with-input-from-string (s "line1\nline2")
  (read-line s))                         ; => (some "line1")
```

### 標準入出力

`*standard-input*`・`*standard-output*`・`*error-output*` もストリームです。
`(read-line *standard-input*)` で 1 行読めます。

### 読み書きする関数を汎用にする

ストリームは型ごとに別ですが、共通の操作はトレイトにまとまっています。引数を
`(where (CharInput S))` で受ければ、ファイル・文字列・ソケットのどれからでも読める関数になります。

```lisp
(defun count-lines<S> ((s S)) int (where (CharInput S))
  (let ((n 0))
    (loop (match (read-line s)
            ((some _) (setf n (+ n 1)))
            ((none) (break))))
    n))
```

## 4. パス名

ファイル名を受け取る関数は、文字列でも `pathname` でも受け付けます。パスを部品に分けたり
組み立てたりするときに `pathname` を使います。

```lisp
(let ((p (parse-namestring "/var/log/app.tar.gz")))
  (pathname-directory p)   ; => ["var" "log"]
  (pathname-name p)        ; => (some "app.tar")
  (pathname-type p))       ; => (some "gz")

;; 拡張子だけ差し替える
(namestring (merge-pathnames (make-pathname :type "json") (to-pathname "data/in.csv")))
;; => "data/in.json"

;; ディレクトリの中にファイル名を置く
(namestring (merge-pathnames (make-pathname :name "today" :type "log") "/var/log/"))
;; => "/var/log/today.log"
```

ファイルシステムの操作:

```lisp
(unwrap (ensure-directories-exist "out/deep/"))   ; 親ごと作る
(probe-file "out/deep")                           ; => true（存在する）
(directory-p "out/deep")                          ; => true
(directory "out")                                 ; => 中身の一覧
(delete-file "copy.txt")
(rename-file "a.txt" "b.txt")
```

区切り文字は `/` 固定です。Common Lisp のパス名にあるホスト・デバイス・バージョンの成分や、
ワイルドカード・論理パス名はありません。

## 5. TCP

ソケットの接続もストリームなので、`read-line` や `write-line` がそのまま使えます。

### サーバ

接続ごとにタスクを 1 つ起動するのが基本の形です。`accept` や `read-line` はデータが来るまで
**そのタスクだけ**を止めるので、他の接続の処理は続きます。

```lisp
(defun serve ((c socket-stream)) ()
  (loop (match (read-line c)
          ((some line) (write-line c (upcase line)))
          ((none) (break))))                  ; 相手が閉じた
  (close c))

(let ((l (unwrap (tcp-listen "127.0.0.1" 7777))))
  (loop (match (accept l)
          ((ok c) (progn (task (serve c)) ()))
          ((err e) (println "accept: ~a" (message e))))))
```

`"127.0.0.1"` はこの機械からの接続だけを受け、`"0.0.0.0"` はすべてのインタフェースで受けます。
タスクについては [構文リファレンス 12 章](../reference/syntax.md#12-並行機構タスク) を参照してください。

### クライアント

`with-connection` は、接続して本体を実行し、最後に閉じます。値は `Result<本体の値, NetError>` です。

```lisp
(match (with-connection (c "localhost" 7777)
         (write-line c "hello")
         (unwrap (read-line c)))
  ((ok reply) (println "reply: ~a" reply))      ; reply: HELLO
  ((err e) (println "~a" (message e))))
```

時間の制限をかけるときは `:timeout`（秒）を渡します。

```lisp
(tcp-connect "example.com" 80 :timeout 5.0)
(accept l :timeout 30.0)
(if (wait-readable c 5.0) (read-line c) (option::none))   ; 次の読みに時計をかける
```

### 相手が切断したとき

接続の相手がリセットしたり途中で切ったりしても、`panic` にはなりません。以降の読みは
`none` を返し、書き込みは黙って捨てられます。きれいに閉じられたのか失敗したのかを区別したい
ときは `(socket-error c)` を調べます。

## 6. 名前解決（DNS）

名前解決のための専用の関数はありません。`tcp-connect`・`tls-connect`・`send-to` にホスト名を
渡せば、その中で解決されます。解決は別のスレッドで行われるので、待っている間も他のタスクは
走ります。名前が複数のアドレスを持つ場合は順に接続を試します。

解決できなかった場合は `Err` が返ります。

```lisp
(match (tcp-connect "no-such-host.invalid" 80 :timeout 3.0)
  ((ok c) (close c))
  ((err e) (println "~a" (message e))))
;; resolve: no-such-host.invalid:80: failed to lookup address information: ...
```

## 7. TLS

`tls-connect` は `tcp-connect` と同じ使い方で、接続の後に TLS の握手をします。サーバ証明書は
標準のルート証明書で検証されます。結果は普通の `socket-stream` なので、読み書きは TCP と同じです。

```lisp
(match (tls-connect "example.com" 443 :timeout 10.0)
  ((ok c)
   (progn
     (write-string c "GET / HTTP/1.1\r\nHost: example.com\r\nConnection: close\r\n\r\n")
     (println "~a" (unwrap (read-line c)))     ; HTTP/1.1 200 OK
     (close c)))
  ((err e) (println "tls failed: ~a" (message e))))
```

TLS のサーバは `tls-listen` に証明書と秘密鍵のファイル（PEM）を渡して作ります。

```lisp
(let ((l (unwrap (tls-listen "0.0.0.0" 8443 "cert.pem" "key.pem"))))
  (loop (match (accept l)
          ((ok c) (progn (task (serve c)) ()))   ; TCP の serve をそのまま使える
          ((err e) (println "accept: ~a" (message e))))))
```

自己署名の証明書で試すときは、クライアント側で `:ca-file "cert.pem"` を渡してその証明書を
信頼させます。相互 TLS や、1 つの待ち受けで複数のサイトを扱う方法は
[ネットワーク](../reference/functions/network.md#2-tcp--tls--unix-ドメイン) にあります。

## 8. UDP

UDP はストリームではなく、データグラムを 1 つずつ送受信します。データはバイト列
（`Vector<int>`）で、文字列との変換には `string->utf8` / `utf8->string` を使います。

```lisp
(let ((a (unwrap (udp-bind "127.0.0.1" 0)))          ; 送るだけでもソケットが要る。0 は OS に任せる
      (b (unwrap (udp-bind "127.0.0.1" 9999))))
  (unwrap (send-to a "localhost" 9999 (string->utf8 "ping")))
  (match (recv-from b :timeout 1.0)
    ((ok d) (println "~a from ~a" (unwrap (utf8->string (bytes d))) (from d)))
    ((err e) (println "~a" (message e)))))
;; ping from 127.0.0.1:59161
```

`(from d)` は送り主の `ip:port` で、そのまま `send-to` の宛先に使えます。

## 9. Unix ドメインソケット

`unix-listen` / `unix-connect` にソケットファイルのパスを渡します。得られる値は TCP と同じ
`socket-stream` です。`unix-listen` はファイルが既にあると `Err` を返します。待ち受けを
`close` するとファイルは消えます。
