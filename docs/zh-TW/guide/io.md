<!-- translated-from: docs/ja/guide/io.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# 檔案 I/O、串流與網路

本指南說明檔案讀寫、路徑名稱以及 socket 通訊的基本寫法。函式一覽見[串流與檔案](../reference/functions/streams-files.md)與
[網路](../reference/functions/network.md)。

## 1. 失敗以 `Result` 回傳

開啟檔案、建立連線等取決於環境而可能失敗的操作會回傳 `Result`。檔案不存在不是程式的錯誤，所以不會 panic。用 `match` 區分結果。

```lisp
(match (read-file-string "config.txt")
  ((ok text) (println "~a" text))
  ((err e) (println "error: ~a" (message e))))
;; 檔案不存在時：
;; error: config.txt: No such file or directory (os error 2)
```

知道不會失敗時，或者在失敗了停止也無妨的小腳本中，可以用 `unwrap` 取出值。如果是 `Err` 就會 panic。

## 2. 一次讀寫整個檔案

最簡便的是一次處理整個檔案的函式。

```lisp
(unwrap (write-file-string "copy.txt" "abc\n"))
(read-file-string "copy.txt")      ; => (ok "abc\n")
(read-file-lines "copy.txt")       ; => (ok #<vector<string> "abc">)
```

## 3. 用串流讀寫

一點一點讀寫時，用 `with-open-file` 開啟串流。無論以何種方式離開本體，串流都會被關閉。值是 `Result<本體的值, FileError>`。

```lisp
;; 寫入
(match (with-open-file (out "notes.txt" direction-output)
         (write-line out "first")
         (write-line out "second")
         (format out "total: ~a~%" 2))
  ((ok _) ())
  ((err e) (println "write failed: ~a" (message e))))

;; 逐行讀取
(match (with-open-file (in "notes.txt" direction-input)
         (let ((n 0))
           (loop (match (read-line in)
                   ((some line) (progn (setf n (+ n 1)) (println "~a: ~a" n line)))
                   ((none) (break))))
           n))
  ((ok n) (println "~a lines" n))
  ((err e) (println "read failed: ~a" (message e))))
```

- 開啟方向有三種：`direction-input`（讀取）、`direction-output`（寫入，原有內容會被清除）、`direction-append`（加到尾端）。
- `read-line` 在檔案結尾回傳 `none`。
- 不用 `with-open-file` 而用 `open-file` 開啟時，請務必呼叫 `close`。GC 不會關閉串流。

以位元組讀寫時，用 `open-binary-input` / `open-binary-output` 開啟，並使用 `read-byte` / `write-byte`。字元串流與位元組串流是不同的
型別，從字元串流讀取位元組是型別錯誤。

### 把字串當作串流使用

```lisp
(with-output-to-string (s)
  (write-string s "a")
  (write-string s "b"))                  ; => "ab"

(with-input-from-string (s "line1\nline2")
  (read-line s))                         ; => (some "line1")
```

### 標準輸入輸出

`*standard-input*`、`*standard-output*`、`*error-output*` 也是串流。用 `(read-line *standard-input*)` 讀取一行。

### 讓讀寫函式通用

串流的型別各不相同，但共同的操作整理在 trait 中。以 `(where (CharInput S))` 接收引數，函式就能從檔案、字串、socket 中的任何一種讀取。

```lisp
(defun count-lines<S> ((s S)) int (where (CharInput S))
  (let ((n 0))
    (loop (match (read-line s)
            ((some _) (setf n (+ n 1)))
            ((none) (break))))
    n))
```

## 4. 路徑名稱

接受檔名的函式既接受字串也接受 `pathname`。要把路徑拆成部分或組裝路徑時使用 `pathname`。

```lisp
(let ((p (parse-namestring "/var/log/app.tar.gz")))
  (pathname-directory p)   ; => #<vector<string> "var" "log">
  (pathname-name p)        ; => (some "app.tar")
  (pathname-type p))       ; => (some "gz")

;; 只替換副檔名
(namestring (merge-pathnames (make-pathname :type "json") (to-pathname "data/in.csv")))
;; => "data/in.json"

;; 把檔名放進目錄中
(namestring (merge-pathnames (make-pathname :name "today" :type "log") "/var/log/"))
;; => "/var/log/today.log"
```

檔案系統操作：

```lisp
(unwrap (ensure-directories-exist "out/deep/"))   ; 連同上層目錄一起建立
(probe-file "out/deep")                           ; => true（存在）
(directory-p "out/deep")                          ; => true
(directory "out")                                 ; => 內容一覽
(delete-file "copy.txt")
(rename-file "a.txt" "b.txt")
```

分隔字元固定為 `/`。沒有 Common Lisp 路徑名稱中的主機、裝置、版本成分，也沒有萬用字元與邏輯路徑名稱。

## 5. TCP

socket 連線也是串流，所以 `read-line` 與 `write-line` 可以直接使用。

### 伺服器

基本形式是每個連線啟動一個任務。`accept` 與 `read-line` 在資料抵達之前**只讓該任務**停下，其他連線的處理會繼續。

```lisp
(defun serve ((c socket-stream)) ()
  (loop (match (read-line c)
          ((some line) (write-line c (upcase line)))
          ((none) (break))))                  ; 對方關閉了
  (close c))

(let ((l (unwrap (tcp-listen "127.0.0.1" 7777))))
  (loop (match (accept l)
          ((ok c) (progn (task (serve c)) ()))
          ((err e) (println "accept: ~a" (message e))))))
```

`"127.0.0.1"` 只接受來自本機的連線，`"0.0.0.0"` 在所有網路介面上接受連線。關於任務，請參閱
[語法參考第 12 章](../reference/syntax.md#12-並行任務)。

### 用戶端

`with-connection` 建立連線、執行本體，最後關閉。值是 `Result<本體的值, NetError>`。

```lisp
(match (with-connection (c "localhost" 7777)
         (write-line c "hello")
         (unwrap (read-line c)))
  ((ok reply) (println "reply: ~a" reply))      ; reply: HELLO
  ((err e) (println "~a" (message e))))
```

要限制時間時，傳入 `:timeout`（秒）。

```lisp
(tcp-connect "example.com" 80 :timeout 5.0)
(accept l :timeout 30.0)
(if (wait-readable c 5.0) (read-line c) (option::none))   ; 為下一次讀取加上計時
```

### 對方中斷連線時

即使連線的對方重設或中途中斷，也不會 panic。之後的讀取回傳 `none`，寫入會被默默丟棄。想區分是正常關閉還是失敗時，檢查
`(socket-error c)`。

## 6. 名稱解析（DNS）

沒有專門用於名稱解析的函式。把主機名稱傳給 `tcp-connect`、`tls-connect`、`send-to`，就會在其中解析。解析在另一個執行緒中進行，所以等待
期間其他任務照常執行。一個名稱有多個位址時會依序嘗試連線。

無法解析時回傳 `Err`。

```lisp
(match (tcp-connect "no-such-host.invalid" 80 :timeout 3.0)
  ((ok c) (close c))
  ((err e) (println "~a" (message e))))
;; resolve: no-such-host.invalid:80: failed to lookup address information: ...
```

## 7. TLS

`tls-connect` 的用法與 `tcp-connect` 相同，在連線之後進行 TLS 交握。伺服器憑證以標準根憑證驗證。結果是一般的 `socket-stream`，讀寫與
TCP 相同。

```lisp
(match (tls-connect "example.com" 443 :timeout 10.0)
  ((ok c)
   (progn
     (write-string c "GET / HTTP/1.1\r\nHost: example.com\r\nConnection: close\r\n\r\n")
     (println "~a" (unwrap (read-line c)))     ; HTTP/1.1 200 OK
     (close c)))
  ((err e) (println "tls failed: ~a" (message e))))
```

TLS 伺服器透過把憑證與私鑰檔案（PEM）傳給 `tls-listen` 來建立。

```lisp
(let ((l (unwrap (tls-listen "0.0.0.0" 8443 "cert.pem" "key.pem"))))
  (loop (match (accept l)
          ((ok c) (progn (task (serve c)) ()))   ; 可以直接使用 TCP 的 serve
          ((err e) (println "accept: ~a" (message e))))))
```

以自簽憑證試驗時，在用戶端傳入 `:ca-file "cert.pem"` 以信任該憑證。雙向 TLS 以及以一個監聽器服務多個網站的方法見
[網路](../reference/functions/network.md#2-tcp--tls--unix-網域)。

## 8. UDP

UDP 不是串流，而是一次收發一個資料包。資料是位元組序列（`Vector<int>`），與字串之間以 `string->utf8` / `utf8->string` 轉換。

```lisp
(let ((a (unwrap (udp-bind "127.0.0.1" 0)))          ; 只傳送也需要 socket。0 表示交給 OS 選擇
      (b (unwrap (udp-bind "127.0.0.1" 9999))))
  (unwrap (send-to a "localhost" 9999 (string->utf8 "ping")))
  (match (recv-from b :timeout 1.0)
    ((ok d) (println "~a from ~a" (unwrap (utf8->string (bytes d))) (from d)))
    ((err e) (println "~a" (message e)))))
;; ping from 127.0.0.1:59161
```

`(from d)` 是傳送方的 `ip:port`，可以直接作為 `send-to` 的目的地。

## 9. Unix 網域 socket

把 socket 檔案的路徑傳給 `unix-listen` / `unix-connect`。得到的值與 TCP 一樣是 `socket-stream`。檔案已存在時 `unix-listen` 回傳
`Err`。關閉監聽器時檔案會被刪除。
