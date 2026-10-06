# ストリームとファイル

ストリームのトレイトとメソッド、具象ストリーム型、ファイル操作、パス名。
ネットワークのソケットもストリームの一員で、[ネットワーク](network.md) にある。

## 1. トレイト階層

CL がクラス階層で表すものを、ここでは**トレイト階層**で表す。方向（入力／出力）も要素型も
**静的**に決まるので、「このストリームは読めるか」を実行時に尋ねる必要がない。

```lisp
(deftrait Stream ()             (open-stream-p ...) (close ...))
(deftrait InputStream  (Stream) (type Item) (read-item ...))
(deftrait OutputStream (Stream) (type Item) (write-item ...))
(deftrait CharInput  ((InputStream  (Item char))) ...)   ; 文字入力
(deftrait CharOutput ((OutputStream (Item char))) ...)   ; 文字出力
(deftrait PeekInput  (CharInput) (unread-char ...) (peek-char ...))  ; 1文字押し戻せる入力
(deftrait ByteInput  ((InputStream  (Item int))) ...)    ; バイト入力
(deftrait ByteOutput ((OutputStream (Item int))) ...)    ; バイト出力
```

文字を読む関数は `(where (CharInput S))` か `:dyn CharInput` を取れば、組み込み・ユーザ定義を
問わずあらゆるストリーム型を受け付ける。

## 2. メソッド

`CharInput` の全メソッドはデフォルト実装を持つ。実装側が書くのは `read-item` だけ。

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `read-item` | `(read-item s)` | `(S)→Option<Item>` | 次の1要素。末尾なら `none`。**唯一の実装必須メソッド** |
| `read-char` | `(read-char s)` | `(S)→Option<char>` | 次の1文字 |
| `read-line` | `(read-line s)` | `(S)→Option<string>` | 次の改行まで（改行は消費して除去）。改行で終わらない最終行も返る |
| `read-all` | `(read-all s)` | `(S)→string` | 残り全部 |
| `read-char-no-hang` | `(read-char-no-hang s)` | `(S)→Option<char>` | すでに手元にある1文字だけ。待たされるくらいなら `none` |
| `read-sequence` | `(read-sequence s v n)` | `(S,Vector<char>,int)→int` | 最大 `n` 文字を `v` へ push し、実際に読めた数を返す。`n` に満たないのは末尾のときだけ |

`listen` は `InputStream`（`CharInput` の親）にある:

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `listen` | `(listen s)` | `(S)→bool` | 次の読みが待たされずに答えられるか。デフォルトは `false`——**決して嘘にならない側**。`true` は推測になり、外すと `read-char-no-hang` がブロックする。組み込みストリームは全て上書き済み。**上書きしないユーザ定義ストリームでは `read-char-no-hang` が常に `none` を返す** |

`PeekInput`（`CharInput` を継承）は**1文字の押し戻し**を足す。押し戻した文字を置く場所は
ストリーム自身しか持たないので、デフォルト実装を持てず、別のトレイトにしてある。
`file-stream`/`string-input-stream`/`standard-stream` は実装済みで、それ以外は `make-peek-stream`
で包めば得られる（4 章）。

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `unread-char` | `(unread-char s c)` | `(S,char)→()` | 次の読みが `c` を返すようにする。**唯一の実装必須メソッド**。CL 同様、保証は1文字だけ |
| `peek-char` | `(peek-char s)` | `(S)→Option<char>` | 消費せずに次の1文字を見る |

`CharOutput` も同様に、実装側が書くのは `write-item` だけ。

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `write-item` | `(write-item s x)` | `(S,Item)→()` | 1要素を書く。**唯一の実装必須メソッド** |
| `write-char` | `(write-char s c)` | `(S,char)→()` | 1文字書く |
| `write-string` | `(write-string s str)` | `(S,string)→()` | 文字列を書く |
| `write-line` | `(write-line s str)` | `(S,string)→()` | 文字列＋改行 |
| `terpri` | `(terpri s)` | `(S)→()` | 改行を1つ（CL の名前） |
| `fresh-line` | `(fresh-line s)` | `(S)→()` | 行頭でなければ改行を1つ |
| `at-line-start` | `(at-line-start s)` | `(S)→bool` | 次に書く文字が行頭になるか。デフォルトは `false`（＝`fresh-line` は改行を書く。分からないなら書くほうが安全）。組み込みストリームは全て上書き済み |
| `finish-output` | `(finish-output s)` | `(S)→()` | バッファを送り出す |
| `write-sequence` | `(write-sequence s v)` | `(S,Vector<char>)→()` | `v` の全文字を順に書く |

`at-line-start` が覚えているのは**そのストリーム経由で書かれた分だけ**。`print`/`println`/
`(format true ...)` は `*standard-output*` を通らずに標準出力へ書くので、両者を混ぜると
`(fresh-line *standard-output*)` の判断は `println` が書いた改行を知らない。片方に寄せること。

`Stream` は全ストリーム共通:

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `open-stream-p` | `(open-stream-p s)` | `(S)→bool` | まだ開いているか |
| `close` | `(close s)` | `(S)→()` | 閉じる。**GC では閉じられない**ので明示的に（または `with-open-file` で） |

## 3. 具象ストリーム型

| 型 | 作り方 | 実装するトレイト |
|---|---|---|
| `file-stream` | `(open-file name direction)` / `open-input` / `open-output` | `CharInput` `PeekInput` `CharOutput` |
| `string-input-stream` | `(make-string-input-stream s)` | `CharInput` `PeekInput` |
| `string-output-stream` | `(make-string-output-stream)` | `CharOutput` |
| `standard-stream` | `*standard-input*` `*standard-output*` `*error-output*` | `CharInput` `PeekInput` `CharOutput` |
| `binary-file-stream` | `(open-binary name direction)` / `open-binary-input` / `open-binary-output` | `ByteInput` `ByteOutput` |

`direction` は `direction-input` / `direction-output` / `direction-append` の3定数。
`open-file` は開けなければ `Err(FileError)` を返す（存在しないファイルは普通の結果であって
panic ではない）。ファイル名は文字列でも `pathname` でもよい（9 章の `Pathish`）。

`(get-output-stream-string s)` は `string-output-stream` に書かれた内容を返して空にする。
CL 同様、`close` 後でも取り出せる。

**バイト I/O** は `ByteInput`/`ByteOutput`。`InputStream`/`OutputStream` の `Item` を
`int` に固定したもので、`CharInput`/`CharOutput` が `char` に固定しているのと同じ形。

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `read-byte` | `(read-byte s)` | `(S)→Option<int>` where `ByteInput S` | 次の1バイト。ファイル終端で `none` |
| `write-byte` | `(write-byte s b)` | `(S,int)→()` where `ByteOutput S` | 1バイト書く。0..255 の外はエラー |
| `read-sequence` | `(read-sequence s v n)` | `(S,Vector<int>,int)→int` where `ByteInput S` | 文字版と同じものをバイトで |
| `write-sequence` | `(write-sequence s v)` | `(S,Vector<int>)→()` where `ByteOutput S` | 同上 |

CL は `(open name :element-type '(unsigned-byte 8))` と要素型を**呼び出し**で決めるが、
ここでは要素型はストリームの**型**なので、違うのは開く関数の側になる。文字ストリームから
バイトを読むことは型エラー（`string-input-stream` は `ByteInput` を実装しない）。
`unread-char` で文字を押し戻した直後のバイト読みもエラーになる。

## 4. 合成ストリーム

どれも標準ライブラリの `defstruct` で、入れ子にもできる。

| 名前 | 形式 | 説明 |
|---|---|---|
| `make-broadcast-stream` | `(make-broadcast-stream v)` | `Vector<:dyn CharOutput>` の全てへ書く |
| `make-two-way-stream` | `(make-two-way-stream in out)` | `in` から読み `out` へ書く |
| `make-echo-stream` | `(make-echo-stream in out)` | `in` から読み、読んだ文字を `out` にも書く |
| `make-concatenated-stream` | `(make-concatenated-stream v)` | `Vector<:dyn CharInput>` を順に読み継ぐ |
| `make-peek-stream` | `(make-peek-stream in)` | 任意の `:dyn CharInput` に1文字の押し戻しを足して `PeekInput` にする（`read-sexpr` 用） |

## 5. マクロ

| 名前 | 形式 | 説明 |
|---|---|---|
| `with-open-file` | `(with-open-file (var name direction) body...)` | 開く→本体→閉じる。`Result<本体の値, FileError>` |
| `with-input-from-string` | `(with-input-from-string (var s) body...)` | 文字列から読む |
| `with-output-to-string` | `(with-output-to-string (var) body...)` | 書かれた内容を返す |

## 6. ジェネリック関数とファイル操作

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `copy-stream` | `(copy-stream from to)` | `(I,O)→()` where `CharInput I`,`CharOutput O` | 全部転送 |
| `read-lines` | `(read-lines s)` | `(S)→Vector<string>` where `CharInput S` | 残り全行 |
| `read-sexpr` | `(read-sexpr s)` | `(S)→Result<ReadOutcome,ReadError>` where `PeekInput S` | `Sexpr` を1つ読む（CL の `read`）。入力末尾は `Ok(eof)`、読めたときは `Ok(datum d)`、データでなければ `Err`。datum を終わらせた**空白1文字を消費する**（CL と同じ）。`ReadOutcome` が `Option<Sexpr>` でないのは、空リスト `()` を読んだことと入力末尾とを同じ値で表さないため |
| `read-sexpr-preserving-whitespace` | 同上 | 同上 | 同上だが空白を残す（CL の `read-preserving-whitespace`） |
| `read-delimited-list` | `(read-delimited-list ch s)` | `(char,S)→Result<Option<Sexpr>,ReadError>` where `PeekInput S` | `ch` まで読んでリストにする。`ch` は消費。入力が尽きたら `Err` |
| `write-lines` | `(write-lines s lines)` | `(S,I)→()` where `CharOutput S`,`Iter I (Item string)` | 1行ずつ書く |
| `read-file-string` | `(read-file-string name)` | `(P)→Result<string,FileError>` where `Pathish P` | 全内容 |
| `read-file-lines` | `(read-file-lines name)` | `(P)→Result<Vector<string>,FileError>` where `Pathish P` | 全行 |
| `write-file-string` | `(write-file-string name text)` | `(P,string)→Result<(),FileError>` where `Pathish P` | 書き出す |
| `probe-file` | `(probe-file name)` | `(P)→bool` where `Pathish P` | 存在するか |
| `delete-file` / `rename-file` | | `→Result<(),FileError>` | 削除・改名（引数は `Pathish`） |
| `truename` | `(truename name)` | `(P)→Result<string,FileError>` where `Pathish P` | シンボリックリンクと `.`/`..` を解いた絶対パス。存在しなければ `Err` |
| `file-write-date` | `(file-write-date name)` | `(P)→Result<universal-time,FileError>` where `Pathish P` | 最終更新時刻。**万国時**なので `decode-universal-time`（[時間](system.md#2-日時への分解合成)）が読める |
| `file-author` | `(file-author name)` | `(P)→Result<Option<string>,FileError>` where `Pathish P` | 所有者のログイン名。ファイルが無ければ `Err`、所有者の uid にパスワードデータベースの項目が無ければ `Ok(none)`——CL が分けている2つをそのまま分けている |
| `directory-p` | `(directory-p name)` | `(P)→bool` where `Pathish P` | ディレクトリか。**無い場合も `false`** ——両者を分けるのは `probe-file` |
| `directory` | `(directory name)` | `(P)→Result<Vector<string>,FileError>` where `Pathish P` | 中身を truename（`truename` と同じく、シンボリックリンクを解いた絶対パス）で並べる。リンク先の無いシンボリックリンクは入らない。`.`/`..` は入らない。順序は OS のまま |
| `ensure-directories-exist` | `(ensure-directories-exist name)` | `(P)→Result<(),FileError>` where `Pathish P` | 親ごと作る。既にあれば成功 |

ファイルを名指しする引数は全て**文字列でも `pathname` でもよい**——CL のパス名指定子と同じ扱いで、
実行時の型テストではなく `Pathish` トレイトで解決している（9 章）。

`read-delimited-list` の終端文字は**トークンも終わらせる**。効くのは深さ 0 だけで、`(1 2]` の
`]` はリスト自身のテキストの一部として読まれ、壊れたリストとして報告される。CL の
第3引数 `recursive-p` に対応物は無い。

## 7. 自分の型をストリームにする

`write-item` を1つ書けば、残りはデフォルト実装が付いてくる。合成ストリームにも入れられる。

```lisp
(defstruct counter (n i32))
(impl Stream counter
  (open-stream-p ((self Self)) bool true)
  (close ((self Self)) () ()))
(impl OutputStream counter
  (type Item char)
  (write-item ((self Self) (c char)) () (setf self::n (+ self::n 1))))
(impl CharOutput counter)              ; 残りのメソッドは全部デフォルト

(write-line (counter::new 0) "四文字")  ; write-line も terpri も fresh-line も動く
```

入力側も同じで、書くのは `read-item` だけ。押し戻しを自前で持たない型でも、
`(read-sexpr (make-peek-stream my-stream))` と包めば `read` できる。

## 8. readtable

| 名前 | 呼び方 | 型 | 説明 |
|---|---|---|---|
| `set-macro-character` | `(set-macro-character c f)` | `(char, F)→()` | 文字 `c` を `f` が読む |
| `get-macro-character` | `(get-macro-character c)` | `(char)→Option<F>` | 登録されているものを返す |
| `set-dispatch-macro-character` | `(set-dispatch-macro-character d s f)` | `(char,char,F)→()` | 2文字並び `d s` を `f` が読む |
| `get-dispatch-macro-character` | `(get-dispatch-macro-character d s)` | `(char,char)→Option<F>` | 同上 |

`F` は `(fn (string-input-stream char) Option<Sexpr>)`。使い方・いつ効くか・CL との違いは
[構文リファレンス](../syntax.md#11-リーダマクロreadtable)。

## 9. パス名 `pathname`

ファイル名を分解した値。`/` 区切りのディレクトリ成分・名前・型（拡張子）と、ルート始まりかどうか
を持つ。

```lisp
(let ((p (parse-namestring "/var/log/app.tar.gz")))
  (pathname-directory p)   ; => #<vector<string> "var" "log">
  (pathname-name p)        ; => (some "app.tar")   最後のドットで切る
  (pathname-type p)        ; => (some "gz")
  (namestring p))          ; => "/var/log/app.tar.gz"

(namestring (merge-pathnames (make-pathname :name "today" :type "log")
                             "/var/log/"))        ; => "/var/log/today.log"
```

### 9.1 パス名指定子トレイト `Pathish`

CL がパス名指定子（文字列かパス名）を受ける場所で、こちらは `Pathish` を受ける。`string` と
`pathname` の両方が実装しており、**ファイル操作は全てこれをジェネリックに取る**ので、
`(open-input "a.txt")` と `(open-input p)` はどちらも普通の呼び出し（実行時の型テストは無い）。
文字列側の `namestring` は自分自身を返すだけなので、文字列を渡す限りパースは走らない。

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `namestring` | `(namestring p)` | `(P)→string` | 文字列表現。実装必須 |
| `to-pathname` | `(to-pathname p)` | `(P)→pathname` | `pathname` に直す（CL の `pathname` 関数。型名と衝突するので改名）。実装必須 |

### 9.2 関数

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `parse-namestring` | `(parse-namestring s)` | `string→pathname` | 文字列を分解する。末尾 `/`（や空名）は「名前無し」＝ディレクトリ |
| `make-pathname` | `(make-pathname :directory v :name s :type s :absolute b)` | `→pathname` | 持っている成分だけで組み立てる（全て `&key`）。省略した名前・型は「無い」ままで、`merge-pathnames` が埋める対象になる |
| `pathname-directory` | `(pathname-directory p)` | `(P)→Vector<string>` | 外側から順のディレクトリ成分 |
| `pathname-name` | `(pathname-name p)` | `(P)→Option<string>` | 型を除いた名前。ディレクトリなら `none` |
| `pathname-type` | `(pathname-type p)` | `(P)→Option<string>` | 最後のドット以降。先頭のドットは対象外（`.gitignore` は全部が名前） |
| `pathname-absolute-p` | `(pathname-absolute-p p)` | `(P)→bool` | ルート始まりか |
| `user-homedir-pathname` | `(user-homedir-pathname)` | `()→Option<pathname>` | ホームディレクトリ。`$HOME` が無ければ `none`（CL も `NIL` を許す） |
| `directory-namestring` | `(directory-namestring p)` | `(P)→string` | 最後の `/` までの部分 |
| `file-namestring` | `(file-namestring p)` | `(P)→string` | `name.type` の部分だけ |
| `merge-pathnames` | `(merge-pathnames p default)` | `(P,D)→pathname` | `p` に無い成分を `default` から補う。相対の `p` は `default` のディレクトリの下に置かれ、絶対の `p` は自分のディレクトリを保つ |
| `enough-namestring` | `(enough-namestring p default)` | `(P,D)→string` | `default` を基準にした相対表記。基準の下に無ければ `p` の全体 |

型引数はいずれも `(where (Pathish P))`。

## 10. CL との違い

- **クラス階層ではなくトレイト階層**。`input-stream-p` / `output-stream-p` は無い——方向は型が
  持つので、実行時に尋ねる問いではない。
- **`read` は文字列版とストリーム版で名前が違う**。`(read "...")`（CL の `read-from-string` の
  1 つ目の値に当たる。読み終わり位置も要るなら `read-from-string`）と `(read-sexpr s)`（CL の `read`）。受け手の型が 1 つに決まる呼び出しなので、同名の多重定義が
  できない。
- **押し戻しは別トレイト**（`PeekInput`）。`read-char` しか要らない型に `unread-char` の実装を
  強いないため。
- **閉じるのは明示的**。GC はストリームを閉じない（GC はいつ走るか予測できないので、
  それに任せると閉じる時点も予測できない）。`with-open-file` を使うのが安全。
- **パス名にホスト・デバイス・バージョン成分は無い**。ワイルドカードパス名も、論理パス名
  （`logical-pathname`）も無い。区切りは `/` 固定。
- **`pathname` 関数は `to-pathname`**。型とトレイト・関数が同じ名前空間を共有するため。
- **ワイルドカードによる照合は無い**ので、`directory` は「そのディレクトリの中身を並べる」だけの
  関数。CL の `directory` はパス名のパターンと照合する。
