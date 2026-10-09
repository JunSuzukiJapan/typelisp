<!-- translated-from: docs/ja/reference/functions/streams-files.md @ 6a9ad7ad4f7c098262628978d36af26e7cdd0401 -->
# 串流與檔案

串流的 trait 與方法、具體串流型別、檔案操作、路徑名稱。網路 socket 也是串流的一員，見[網路](network.md)。

## 1. trait 階層

CL 以類別階層表示的東西，在這裡以 **trait 階層**表示。方向（輸入／輸出）與元素型別都是**靜態**決定的，所以不需要在執行期詢問「這個串流能讀嗎」。

```lisp
(deftrait Stream ()             (open-stream-p ...) (close ...))
(deftrait InputStream  (Stream) (type Item) (read-item ...))
(deftrait OutputStream (Stream) (type Item) (write-item ...))
(deftrait CharInput  ((InputStream  (Item char))) ...)   ; 字元輸入
(deftrait CharOutput ((OutputStream (Item char))) ...)   ; 字元輸出
(deftrait PeekInput  (CharInput) (unread-char ...) (peek-char ...))  ; 可以推回一個字元的輸入
(deftrait ByteInput  ((InputStream  (Item int))) ...)    ; 位元組輸入
(deftrait ByteOutput ((OutputStream (Item int))) ...)    ; 位元組輸出
```

讀取字元的函式只要接受 `(where (CharInput S))` 或 `:dyn CharInput`，就能接受任何串流型別，無論是內建的還是使用者定義的。

## 2. 方法

`CharInput` 的所有方法都有預設實作。實作端只需寫 `read-item`。

| 名稱 | 形式 | 型別 | 說明 |
|---|---|---|---|
| `read-item` | `(read-item s)` | `(S)→Option<Item>` | 下一個元素。到結尾時為 `none`。**唯一必須實作的方法** |
| `read-char` | `(read-char s)` | `(S)→Option<char>` | 下一個字元 |
| `read-line` | `(read-line s)` | `(S)→Option<string>` | 到下一個換行為止（換行被消耗並去掉）。不以換行結尾的最後一行也會回傳 |
| `read-all` | `(read-all s)` | `(S)→string` | 剩下的全部 |
| `read-char-no-hang` | `(read-char-no-hang s)` | `(S)→Option<char>` | 只取已經在手邊的一個字元。與其等待不如回傳 `none` |
| `read-sequence` | `(read-sequence s v n)` | `(S,Vector<char>,int)→int` | 最多把 `n` 個字元 push 到 `v`，回傳實際讀到的數量。只有到結尾時才會少於 `n` |

`listen` 在 `InputStream`（`CharInput` 的上層）中：

| 名稱 | 形式 | 型別 | 說明 |
|---|---|---|---|
| `listen` | `(listen s)` | `(S)→bool` | 下一次讀取能否不等待就得到回答。預設為 `false`——**絕不會成為謊言的一方**。`true` 是猜測，猜錯的話 `read-char-no-hang` 會阻塞。所有內建串流都已覆寫它。**對於沒有覆寫它的使用者定義串流，`read-char-no-hang` 一律回傳 `none`** |

`PeekInput`（繼承 `CharInput`）增加了**推回一個字元**。放置推回字元的地方只有串流本身才有，所以無法有預設實作，作為另一個 trait。
`file-stream`/`string-input-stream`/`standard-stream` 已實作，其他的以 `make-peek-stream` 包住就能得到（第 4 章）。

| 名稱 | 形式 | 型別 | 說明 |
|---|---|---|---|
| `unread-char` | `(unread-char s c)` | `(S,char)→()` | 讓下一次讀取回傳 `c`。**唯一必須實作的方法**。與 CL 一樣只保證一個字元 |
| `peek-char` | `(peek-char s)` | `(S)→Option<char>` | 不消耗地查看下一個字元 |

`CharOutput` 也一樣，實作端只需寫 `write-item`。

| 名稱 | 形式 | 型別 | 說明 |
|---|---|---|---|
| `write-item` | `(write-item s x)` | `(S,Item)→()` | 寫一個元素。**唯一必須實作的方法** |
| `write-char` | `(write-char s c)` | `(S,char)→()` | 寫一個字元 |
| `write-string` | `(write-string s str)` | `(S,string)→()` | 寫字串 |
| `write-line` | `(write-line s str)` | `(S,string)→()` | 字串加換行 |
| `terpri` | `(terpri s)` | `(S)→()` | 一個換行（CL 的名稱） |
| `fresh-line` | `(fresh-line s)` | `(S)→()` | 不在行首時輸出一個換行 |
| `at-line-start` | `(at-line-start s)` | `(S)→bool` | 下一個寫入的字元是否位於行首。預設為 `false`（即 `fresh-line` 會寫換行。不確定時寫比較安全）。所有內建串流都已覆寫它 |
| `finish-output` | `(finish-output s)` | `(S)→()` | 送出緩衝區 |
| `write-sequence` | `(write-sequence s v)` | `(S,Vector<char>)→()` | 依序寫出 `v` 中的所有字元 |

`at-line-start` 記得的**只是經由該串流寫入的內容**。`print`/`println`/`(format true ...)` 不經過 `*standard-output*` 就寫到標準輸出，
所以兩者混用時，`(fresh-line *standard-output*)` 的判斷不知道 `println` 寫過的換行。請統一使用其中一種。

`Stream` 是所有串流共同的：

| 名稱 | 形式 | 型別 | 說明 |
|---|---|---|---|
| `open-stream-p` | `(open-stream-p s)` | `(S)→bool` | 是否仍然開啟 |
| `close` | `(close s)` | `(S)→()` | 關閉。**GC 不會關閉**，請明確關閉（或使用 `with-open-file`） |

## 3. 具體串流型別

| 型別 | 建立方式 | 實作的 trait |
|---|---|---|
| `file-stream` | `(open-file name direction)` / `open-input` / `open-output` | `CharInput` `PeekInput` `CharOutput` |
| `string-input-stream` | `(make-string-input-stream s)` | `CharInput` `PeekInput` |
| `string-output-stream` | `(make-string-output-stream)` | `CharOutput` |
| `standard-stream` | `*standard-input*` `*standard-output*` `*error-output*` | `CharInput` `PeekInput` `CharOutput` |
| `binary-file-stream` | `(open-binary name direction)` / `open-binary-input` / `open-binary-output` | `ByteInput` `ByteOutput` |

`direction` 是 `direction-input` / `direction-output` / `direction-append` 三個常數。`open-file` 無法開啟時回傳 `Err(FileError)`（不存在的
檔案是一般的結果，不是 panic）。檔名可以是字串也可以是 `pathname`（第 9 章的 `Pathish`）。

`(get-output-stream-string s)` 回傳寫入 `string-output-stream` 的內容並清空。與 CL 一樣，`close` 之後也能取出。

**位元組 I/O** 使用 `ByteInput`/`ByteOutput`。它們把 `InputStream`/`OutputStream` 的 `Item` 固定為 `int`，與 `CharInput`/`CharOutput`
固定為 `char` 的形式相同。

| 名稱 | 形式 | 型別 | 說明 |
|---|---|---|---|
| `read-byte` | `(read-byte s)` | `(S)→Option<int>` where `ByteInput S` | 下一個位元組。檔案結尾時為 `none` |
| `write-byte` | `(write-byte s b)` | `(S,int)→()` where `ByteOutput S` | 寫一個位元組。0..255 以外是錯誤 |
| `read-sequence` | `(read-sequence s v n)` | `(S,Vector<int>,int)→int` where `ByteInput S` | 字元版本的位元組版 |
| `write-sequence` | `(write-sequence s v)` | `(S,Vector<int>)→()` where `ByteOutput S` | 同上 |

CL 在**呼叫**中決定元素型別，如 `(open name :element-type '(unsigned-byte 8))`，而這裡元素型別是串流的**型別**，所以不同的是開啟它的函式。
從字元串流讀取位元組是型別錯誤（`string-input-stream` 沒有實作 `ByteInput`）。以 `unread-char` 推回字元後緊接著讀取位元組也是錯誤。

## 4. 組合串流

都是標準函式庫中的 `defstruct`，也可以巢狀。

| 名稱 | 形式 | 說明 |
|---|---|---|
| `make-broadcast-stream` | `(make-broadcast-stream v)` | 寫到 `Vector<:dyn CharOutput>` 的全部 |
| `make-two-way-stream` | `(make-two-way-stream in out)` | 從 `in` 讀，寫到 `out` |
| `make-echo-stream` | `(make-echo-stream in out)` | 從 `in` 讀，讀到的字元也寫到 `out` |
| `make-concatenated-stream` | `(make-concatenated-stream v)` | 依序接續讀取 `Vector<:dyn CharInput>` |
| `make-peek-stream` | `(make-peek-stream in)` | 為任意 `:dyn CharInput` 加上一個字元的推回，使之成為 `PeekInput`（給 `read-sexpr` 使用） |

## 5. 巨集

| 名稱 | 形式 | 說明 |
|---|---|---|
| `with-open-file` | `(with-open-file (var name direction) body...)` | 開啟→本體→關閉。`Result<本體的值, FileError>` |
| `with-input-from-string` | `(with-input-from-string (var s) body...)` | 從字串讀取 |
| `with-output-to-string` | `(with-output-to-string (var) body...)` | 回傳寫入的內容 |

## 6. 泛型函式與檔案操作

| 名稱 | 形式 | 型別 | 說明 |
|---|---|---|---|
| `copy-stream` | `(copy-stream from to)` | `(I,O)→()` where `CharInput I`,`CharOutput O` | 全部轉送 |
| `read-lines` | `(read-lines s)` | `(S)→Vector<string>` where `CharInput S` | 剩下的所有行 |
| `read-sexpr` | `(read-sexpr s)` | `(S)→Result<ReadOutcome,ReadError>` where `PeekInput S` | 讀取一個 `Sexpr`（CL 的 `read`）。輸入結尾為 `Ok(eof)`，讀到時為 `Ok(datum d)`，不是資料時為 `Err`。**消耗結束該 datum 的一個空白字元**（與 CL 相同）。`ReadOutcome` 不是 `Option<Sexpr>`，是為了不讓「讀到空串列 `()`」與「輸入結尾」以同一個值表示 |
| `read-sexpr-preserving-whitespace` | 同上 | 同上 | 同上，但保留空白（CL 的 `read-preserving-whitespace`） |
| `read-delimited-list` | `(read-delimited-list ch s)` | `(char,S)→Result<Option<Sexpr>,ReadError>` where `PeekInput S` | 讀到 `ch` 為止並組成串列。`ch` 被消耗。輸入用盡時為 `Err` |
| `write-lines` | `(write-lines s lines)` | `(S,I)→()` where `CharOutput S`,`Iter I (Item string)` | 逐行寫出 |
| `read-file-string` | `(read-file-string name)` | `(P)→Result<string,FileError>` where `Pathish P` | 全部內容 |
| `read-file-lines` | `(read-file-lines name)` | `(P)→Result<Vector<string>,FileError>` where `Pathish P` | 所有行 |
| `write-file-string` | `(write-file-string name text)` | `(P,string)→Result<(),FileError>` where `Pathish P` | 寫出 |
| `probe-file` | `(probe-file name)` | `(P)→bool` where `Pathish P` | 是否存在 |
| `delete-file` / `rename-file` | | `→Result<(),FileError>` | 刪除、重新命名（引數是 `Pathish`） |
| `truename` | `(truename name)` | `(P)→Result<string,FileError>` where `Pathish P` | 解析了符號連結與 `.`/`..` 的絕對路徑。不存在時為 `Err` |
| `file-write-date` | `(file-write-date name)` | `(P)→Result<universal-time,FileError>` where `Pathish P` | 最後修改時間。是**世界時**，所以 `decode-universal-time`（[時間](system.md#2-日期的分解與合成)）可以讀取 |
| `file-author` | `(file-author name)` | `(P)→Result<Option<string>,FileError>` where `Pathish P` | 擁有者的登入名稱。檔案不存在時為 `Err`，擁有者的 uid 在密碼資料庫中沒有項目時為 `Ok(none)`——把 CL 區分的兩種情況照樣區分 |
| `directory-p` | `(directory-p name)` | `(P)→bool` where `Pathish P` | 是否為目錄。**不存在時也為 `false`**——區分兩者用 `probe-file` |
| `directory` | `(directory name)` | `(P)→Result<Vector<string>,FileError>` where `Pathish P` | 以 truename（與 `truename` 一樣是解析了符號連結的絕對路徑）列出內容。沒有目標的符號連結不包括在內。`.`/`..` 不包括在內。順序依 OS 而定 |
| `ensure-directories-exist` | `(ensure-directories-exist name)` | `(P)→Result<(),FileError>` where `Pathish P` | 連同上層目錄一起建立。已存在時成功 |

指定檔案的引數**都可以是字串或 `pathname`**——與 CL 的路徑名稱指定子同樣處理，不是透過執行期的型別檢查，而是透過 `Pathish` trait 解析（第 9 章）。

`read-delimited-list` 的結束字元**也會結束記號**。只在深度 0 生效，`(1 2]` 中的 `]` 被當作串列本身文字的一部分讀取，回報為損壞的串列。
沒有 CL 的第 3 個引數 `recursive-p` 的對應物。

## 7. 讓自己的型別成為串流

只寫一個 `write-item`，其餘由預設實作提供。也可以放進組合串流。

```lisp
(defstruct counter (n i32))
(impl Stream counter
  (open-stream-p ((self Self)) bool true)
  (close ((self Self)) () ()))
(impl OutputStream counter
  (type Item char)
  (write-item ((self Self) (c char)) () (setf self::n (+ self::n 1))))
(impl CharOutput counter)              ; 其餘方法全部使用預設實作

(write-line (counter::new 0) "four")   ; write-line、terpri、fresh-line 都能運作
```

輸入端也一樣，只寫 `read-item`。即使是本身沒有推回功能的型別，以 `(read-sexpr (make-peek-stream my-stream))` 包住後也能 `read`。

## 8. readtable

| 名稱 | 呼叫方式 | 型別 | 說明 |
|---|---|---|---|
| `set-macro-character` | `(set-macro-character c f)` | `(char, F)→()` | 由 `f` 讀取字元 `c` |
| `get-macro-character` | `(get-macro-character c)` | `(char)→Option<F>` | 回傳已登記的內容 |
| `set-dispatch-macro-character` | `(set-dispatch-macro-character d s f)` | `(char,char,F)→()` | 由 `f` 讀取兩個字元的序列 `d s` |
| `get-dispatch-macro-character` | `(get-dispatch-macro-character d s)` | `(char,char)→Option<F>` | 同上 |

`F` 是 `(fn (string-input-stream char) Option<Sexpr>)`。用法、何時生效以及與 CL 的差異見[語法參考](../syntax.md#11-讀取巨集readtable)。

## 9. 路徑名稱 `pathname`

拆解後的檔名。持有以 `/` 分隔的目錄成分、名稱、型別（副檔名），以及是否從根開始。

```lisp
(let ((p (parse-namestring "/var/log/app.tar.gz")))
  (pathname-directory p)   ; => #("var" "log")
  (pathname-name p)        ; => (some "app.tar")   在最後一個點處切分
  (pathname-type p)        ; => (some "gz")
  (namestring p))          ; => "/var/log/app.tar.gz"

(namestring (merge-pathnames (make-pathname :name "today" :type "log")
                             "/var/log/"))        ; => "/var/log/today.log"
```

### 9.1 路徑名稱指定子 trait `Pathish`

在 CL 接受路徑名稱指定子（字串或路徑名稱）的地方，這裡接受 `Pathish`。`string` 與 `pathname` 都實作了它，**所有檔案操作都以泛型方式接受它**，
所以 `(open-input "a.txt")` 與 `(open-input p)` 都是一般的呼叫（沒有執行期的型別檢查）。字串端的 `namestring` 只是回傳自己，所以只要傳入字串，
就不會進行解析。

| 名稱 | 形式 | 型別 | 說明 |
|---|---|---|---|
| `namestring` | `(namestring p)` | `(P)→string` | 字串表示。必須實作 |
| `to-pathname` | `(to-pathname p)` | `(P)→pathname` | 轉換為 `pathname`（CL 的 `pathname` 函式。與型別名稱衝突，所以改了名稱）。必須實作 |

### 9.2 函式

| 名稱 | 形式 | 型別 | 說明 |
|---|---|---|---|
| `parse-namestring` | `(parse-namestring s)` | `string→pathname` | 拆解字串。結尾的 `/`（或空名稱）表示「沒有名稱」＝目錄 |
| `make-pathname` | `(make-pathname :directory v :name s :type s :absolute b)` | `→pathname` | 只以給出的成分組裝（全部是 `&key`）。省略的名稱、型別保持「沒有」，是 `merge-pathnames` 要補上的對象 |
| `pathname-directory` | `(pathname-directory p)` | `(P)→Vector<string>` | 由外而內的目錄成分 |
| `pathname-name` | `(pathname-name p)` | `(P)→Option<string>` | 去掉型別的名稱。目錄時為 `none` |
| `pathname-type` | `(pathname-type p)` | `(P)→Option<string>` | 最後一個點之後的部分。開頭的點不算（`.gitignore` 整個是名稱） |
| `pathname-absolute-p` | `(pathname-absolute-p p)` | `(P)→bool` | 是否從根開始 |
| `user-homedir-pathname` | `(user-homedir-pathname)` | `()→Option<pathname>` | 家目錄。沒有 `$HOME` 時為 `none`（CL 也允許 `NIL`） |
| `directory-namestring` | `(directory-namestring p)` | `(P)→string` | 到最後一個 `/` 為止的部分 |
| `file-namestring` | `(file-namestring p)` | `(P)→string` | 只有 `name.type` 部分 |
| `merge-pathnames` | `(merge-pathnames p default)` | `(P,D)→pathname` | 以 `default` 補上 `p` 中沒有的成分。相對的 `p` 放在 `default` 的目錄下，絕對的 `p` 保持自己的目錄 |
| `enough-namestring` | `(enough-namestring p default)` | `(P,D)→string` | 以 `default` 為基準的相對表示。不在基準之下時為 `p` 的全部 |

型別引數都帶有 `(where (Pathish P))`。

## 10. 與 CL 的差異

- **是 trait 階層而不是類別階層。** 沒有 `input-stream-p` / `output-stream-p`——方向由型別持有，不是在執行期詢問的問題。
- **`read` 的字串版與串流版名稱不同。** `(read "...")`（相當於 CL 的 `read-from-string` 的第 1 個值。還需要讀取結束位置時用
  `read-from-string`）與 `(read-sexpr s)`（CL 的 `read`）。呼叫會解析到唯一的接收者型別，所以不能同名多載。
- **推回是另一個 trait**（`PeekInput`）。為了不強迫只需要 `read-char` 的型別實作 `unread-char`。
- **關閉是明確的。** GC 不會關閉串流（GC 何時執行無法預測，交給它的話關閉的時機也無法預測）。使用 `with-open-file` 是安全的。
- **路徑名稱沒有主機、裝置、版本成分。** 沒有萬用字元路徑名稱，也沒有邏輯路徑名稱（`logical-pathname`）。分隔字元固定為 `/`。
- **`pathname` 函式是 `to-pathname`。** 因為型別、trait 與函式共享同一命名空間。
- **沒有萬用字元比對**，所以 `directory` 只是「列出該目錄的內容」的函式。CL 的 `directory` 會與路徑名稱模式進行比對。
