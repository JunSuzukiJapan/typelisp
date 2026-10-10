# 印字

`print`/`println`/`format`、1 引数プリンタ、pretty printer、`print-object`、印字を制御する変数。
書式ディレクティブの一覧は [format.md](format.md)。ストリームへの読み書きは
[ストリームとファイル](streams-files.md)。

## 1. `print` / `println` / `format`

`print`/`println`/`format` はいずれも**書式指定子（CL の `format` ディレクティブ）を解釈する特殊形**。
第1引数（`format` は第2引数）が**制御文字列**で、以降の可変長引数を各ディレクティブが順に消費する。

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `print` | `(print control args...)` | `(string, ...)→Unit` | 制御文字列を書式展開し、改行なしで標準出力へ書く |
| `println` | `(println control args...)` | `(string, ...)→Unit` | 同上、末尾に改行を付ける |
| `format` | `(format dest control args...)` | `(bool, string, ...)→string` | CL の `format`。展開した文字列を返す。`dest` が `true`（CL の `t`）なら加えて標準出力へも書く／`false`（CL の `nil`）なら書かず文字列を返すだけ |
| `format`（ストリーム宛） | `(format stream control args...)` | `(S, string, ...)→()` where `CharOutput S` | `dest` が `bool` でなければ CL のストリーム宛。展開した文字列をそのストリームへ書く。戻り値は `()`（CL の `nil` に当たる）で、文字列は返らない |

`dest` の型で2つの意味に分かれる（どちらになるかは静的に決まる）。ストリーム宛は
具象ストリーム型・`:dyn CharOutput`・`(where (CharOutput S))` の型変数のいずれでも同じように書ける。
`bool` でもストリームでもない `dest` は型エラー。

**制御文字列はリテラルでなければならない**（Rust の `format!` と同じ制約）。中のディレクティブが
引数をいくつ・どの型で取るかを決めるので、実行時に組み立てた文字列は検査時に読めない。
リテラルに限ることで、**引数の数と型がチェック時に検査される**——`(println "~d" "x")` や
`(println "~a ~a" 1)` はチェック時のエラーになる。綴りを間違えたディレクティブ・閉じていない `~(`・
どの引数も答えられない `~/name/` も同じくチェック時のエラー。検査の規則は
[format.md](format.md#1-書き方)。文字列を組み立てて出したいときは `(format false ...)` で
作って `(println "~a" s)` と印字する。

可変長引数は各自の型のまま `Sexpr` へ包まれてから渡る——`i32`/`f64`/`int`/`ratio`/`char`/`bool`/
`string`/`Sexpr` も、ユーザ定義の `defstruct`/`defenum`/`Vector<T>`/`HashTable<K,V>` なども
そのまま渡せる（`(println "~a" my-struct)` はそのまま動く）。

`typl file.typl` によるスクリプト実行は**トップレベル式の値を出力しない**ので、プログラム自身が
標準出力へ書くにはこれらを呼ぶ。`print`/`println`/`format` は呼び出しのたびに出力を送り出す
（パイプ経由でも、標準入力を読む前にプロンプトが見えるように）。

**`Option<Sexpr>` は透過的に印字される。** S 式データの型は `Option<Sexpr>` なので、
`(some x)` の包みは印字に現れず、中身がそのまま出る。空リストは `()` と出る。
他の `Option<T>` は `(some ...)` / `none` と印字する。構造体・列挙・`Vector` の中の
`Option<T>` フィールドも同じ。`(eval ...)` の `Result<Option<Sexpr>,…>` は
`(ok 42)`、`none` なら `(ok ())` と出る。

```lisp
(println "~a" (the Option<Sexpr> (Option::some 42)))   ; => 42
(println "~a" (the Option<Sexpr> ()))                  ; => ()
(println "~a" (the Option<i32>   (Option::some 42)))   ; => (some 42)
(defstruct p (x Option<int>))
(println "~a" (p::new (Option::some 1)))               ; => #<p x: (some 1)>
(println "~a" (p::new (Option::none)))                 ; => #<p x: none>
```

```lisp
(println "~a + ~a = ~d" 1 2 3)        ; => 1 + 2 = 3
(println "[~5,'0d]" 42)               ; => [00042]
(println "~:d" 1234567)               ; => 1,234,567
(println "~@r / ~r" 2024 42)          ; => MMXXIV / forty-two
(println "~{~a~^, ~}" '(a b c))       ; => a, b, c
(println "~[zero~;one~;two~]" 1)      ; => one
(let ((s (format false "id=~d" 42)))  ; 出力せず文字列だけ得る
  (println "~a" s))                   ; => id=42
```

## 2. 1 引数プリンタ

CLHS 22.1.3 のプリンタ。書式展開ではなく、値ひとつをそのまま印字する。
ストリームは省略可能（既定 `*standard-output*`）。

| 名前 | 形式 | 説明 |
|---|---|---|
| `prin1` | `(prin1 x [stream])` | 読み戻せる表現（`~s` と同じ）で書き、`x` を返す |
| `princ` | `(princ x [stream])` | 人向けの表現（`~a` と同じ）で書き、`x` を返す |
| `write` | `(write x [stream])` | `*print-escape*` が真なら `prin1`、偽なら `princ`。`x` を返す |
| `prin1-to-string` | `(prin1-to-string x)` | 書かずに文字列で返す（`~s`） |
| `princ-to-string` | `(princ-to-string x)` | 同上（`~a`）。`to-string` と同じ |
| `write-to-string` | `(write-to-string x)` | 同上、`*print-escape*` に従う |

`print`/`println` は**これらではない**。制御文字列を取る `format` の短縮形であり、
CL の `print`（改行 → `prin1` → 空白）とは別の仕事なので、両方をそれぞれの名前で残してある。
その結果 **CL の 1 引数 `print` にはこの言語での綴りが無い**——`prin1` を書く。

これらはマクロである。`format` の可変長引数は型変数を受け付けず、呼び出し地点で型が
決まっている必要があるため。

## 3. 標準入力と標準ストリーム

**標準入力を読む**のは専用関数ではなく、標準ストリーム `*standard-input*` に対する
`CharInput` のメソッド——`(read-line *standard-input*)` / `(read-char *standard-input*)` /
`(read-all *standard-input*)`（[ストリームのメソッド](streams-files.md#2-メソッド)）。
標準出力・標準エラーも同様に `*standard-output*` / `*error-output*` があり、
`(write-line *standard-output* s)` のように書ける（`print`/`println`/`format` は書式展開が
要るときの近道で、常に標準出力へ書く）。

## 4. pretty printer

CL の Lisp Pretty Printer（CLHS 22.2）に当たる。**行幅に収まらない出力を、論理ブロックと条件改行の
指定に従って折り返す**。

### 4.1 制御変数

代入可能なグローバル変数。`setf` した時点から以降のすべての印字に効く。一時的に変えるには
`dlet`（6.3）を使う。

| 変数 | 型 | 既定 | 意味 |
|---|---|---|---|
| `*print-pretty*` | `bool` | `false` | 真なら `~a`/`~s`/`~w` と pretty ディレクティブが整形経路に入る |
| `*print-right-margin*` | `int` | `80` | 右マージン（桁）。0 は「マージン無し＝折らない」。負の値は印字エラー |
| `*print-miser-width*` | `int` | `0` | miser スタイルに入る幅。0 は CL の `nil`（miser 無効）に当たる。負の値は印字エラー |

`pprint` 系と `pprint-logical-block` は `*print-pretty*` に関わらず常に整形する（CL の `pprint` の
定義どおり）。

### 4.2 既製レイアウト（特殊形）

`print` と同じく特殊形なので、引数はどんな型でもよい。

| 名前 | 形式 | 説明 |
|---|---|---|
| `pprint` | `(pprint x)` | 既定レイアウトで整形出力。CL 準拠で**先頭に改行**を出し、末尾には出さない |
| `pprint-fill` | `(pprint-fill x)` | 1行に入るだけ詰める（語詰め）。改行は出さない |
| `pprint-linear` | `(pprint-linear x)` | 全要素が1行に収まらなければ**1要素1行**。改行は出さない |
| `pprint-tabular` | `(pprint-tabular x [colinc])` | `colinc` 桁の表形式（既定 16）。改行は出さない。負の `colinc` はエラー |

既定レイアウト（`pprint` / `*print-pretty*` 下の `~a`）は、CL の既定 `*print-pprint-dispatch*` に倣って
`(quote x)` を `'x` と略記し、`defun`/`let`/`if`/`lambda` 等のコード形は「頭部＋規定個数の引数を1行目、
残りの本体を2桁字下げして1行ずつ」に整形する。それ以外のリストは語詰め。

```lisp
(setf *print-right-margin* 20)
(pprint '(1 2 3 4 5 6 7 8 9 10 11 12 13 14 15))
;; =>
;; (1 2 3 4 5 6 7 8 9
;;  10 11 12 13 14 15)
(pprint '(defun f (x) i32 (+ x 1) (* x 2)))
;; =>
;; (defun f (x) i32
;;   (+ x 1)
;;   (* x 2))
```

### 4.3 論理ブロックを自分で組む

| 名前 | 形式 | 説明 |
|---|---|---|
| `pprint-logical-block` | `(pprint-logical-block (obj :prefix p :per-line-prefix p :suffix s) body...)` | 論理ブロックを開く特殊形。`obj` は `pprint-pop` が辿るリスト（辿らないなら `()`）。`:prefix` と `:per-line-prefix` は排他（CL と同じ） |
| `pprint-newline` | `(pprint-newline kind)` | 条件改行。`kind` は `:linear` / `:fill` / `:miser` / `:mandatory` |
| `pprint-indent` | `(pprint-indent kind n)` | 字下げ。`kind` は `:block`（ブロック起点から）/ `:current`（現在桁から） |
| `pprint-tab` | `(pprint-tab kind colnum colinc)` | タブ。`kind` は `:line` / `:section` / `:line-relative` / `:section-relative`。`colnum` と `colinc` は非負（負ならエラー） |
| `pprint-pop` | `(pprint-pop)` | ブロックのリストから次の要素を取る（尽きていれば `()`） |
| `pprint-list-exhausted` | `(pprint-list-exhausted)` | リストが尽きたか |
| `pprint-exit-if-list-exhausted` | `(pprint-exit-if-list-exhausted)` | 尽きていれば囲む `loop` を `break`（マクロ） |

論理ブロックはストリームを引数に取らない——**開いている論理ブロックは暗黙の状態**。
最も外側の `pprint-logical-block` が開始し、それが閉じたときに一括で整形して標準出力へ書く。
開いている間は `print`/`println`/`(format true ...)`/`pprint` の出力もすべてそのブロックへ入るので、
**内容は普通の `print` で書き、改行位置だけ `pprint-newline` 等で指定する**——CL のコードとほぼ同じ形になる。

`pprint-exit-if-list-exhausted` は CL では `pprint-logical-block` からの非局所脱出だが、ここでは
**囲む `loop` からの `break`** である（`pprint-logical-block` は `block` を張らない）。
CL 側の定型もつねに `loop` の中に書くので、書き味は変わらない。

```lisp
(setf *print-right-margin* 24)
(pprint-logical-block ('(alpha beta gamma delta epsilon zeta) :prefix "(" :suffix ")")
  (loop (pprint-exit-if-list-exhausted)
        (print "~w" (pprint-pop))
        (if (pprint-list-exhausted) () (progn (print " ") (pprint-newline :fill)))))
;; =>
;; (alpha beta gamma delta
;;  epsilon zeta)
```

条件改行の判定規則（CLHS `pprint-newline`）:

- `:mandatory` — つねに折る。
- `:linear` — 囲む論理ブロックが1行に収まらなければ折る。ブロック単位の判定なので、**同一ブロックの
  `:linear` はすべて一緒に折れる**（`pprint-linear` の「全部1行か1要素1行か」はこれ）。
- `:fill` — (a) 次の区間が行の残りに収まらない、(b) 直前の区間が1行に収まらなかった、
  (c) miser スタイルでブロックが1行に収まらない、のいずれかで折る。
- `:miser` — miser スタイル（ブロックの開始桁が右マージンから `*print-miser-width*` 以内）のときだけ
  `:linear` として働く。

## 5. `print-object`（型ごとの印字表現）

`impl print-object <型>` を書くと、`print`/`println`/`format`/`pprint` がその型の値を——
**リストの中に入れ子で埋まっていても**——その実装で印字する。CL の総称関数
`print-object`（CLHS 22.1.4）に当たる。

```lisp
(deftrait print-object ()
  (print-object ((self Self) (escape bool)) string))
```

| 引数 | 意味 |
|---|---|
| `self` | 印字する値 |
| `escape` | CL の `*print-escape*`。`~s`/`prin1`/`pprint` で `true`（読み戻せる表現）、`~a`/`princ` で `false`（人間向け）。気にしない実装は無視してよい |

戻り値の `string` がそのまま出力に流れる。`impl` を書かない型は組み込みの表現
（`#<point x: 1 y: 2>` の形）で印字される。

```lisp
(defstruct point (x i32) (y i32))
(impl print-object point
  (print-object ((self Self) (escape bool)) string
    (if escape (format false "#S(point :x ~d :y ~d)" self::x self::y)
               (format false "(~d,~d)" self::x self::y))))

(println "~a" p)              ; => (1,2)
(println "~s" p)              ; => #S(point :x 1 :y 2)
(println "~a" (list p q))     ; => ((1,2) (3,4))   入れ子でも効く
```

pretty printer とも合成される（4 章）。`*print-pretty*` が真なら、実装が返した文字列を
含むリストが右マージンで折り返される。

標準ライブラリの型の印字表現。CL に同じものがある型は SBCL と同じ形にしてある。REPL が
結果を表示するときも `~s` と同じ表現になる。

| 型 | `~s` | `~a` |
|---|---|---|
| `Vector<T>` | `#(1 2 3)`、`#("a" "b")` | `#(1 2 3)`、`#(a b)` |
| タプル `#{..}` | `#{1 "a"}` | `#{1 a}` |
| `HashTable<K,V>` | `#<hashtable<string,int> count=1>` | 同左 |
| `Chan<T>` / `Task<T>` / `Thread<T>` | `#<chan<int> 0>`（数字は処理系内の番号） | 同左 |
| `pathname` | `#P"/tmp/a.txt"` | `/tmp/a.txt` |
| `universal-time` / `internal-time` | 整数（CL の `get-universal-time` / `get-internal-real-time` の値） | 同左 |
| エラー型（`ParseIntError`、`SimpleError` など） | `#<simpleerror "boom">` | メッセージだけ（`boom`） |
| `complex` | `#C(1.0 2.0)` | 同左 |
| `Array<T>` | `#2A((0 0) (0 0))` | 同左 |
| ストリーム | `#<file-stream for "file /tmp/a.txt" {7}>`、`#<string-output-stream {5}>`、`#<two-way-stream :input-stream … :output-stream …>` | 同左 |
| ソケット | `#<socket-stream for "socket 127.0.0.1:5000, peer: 127.0.0.1:6000" {10}>`、`#<socket-listener 0.0.0.0:8080, fd: 6 {13}>` | 同左 |
| `decoded-time` | `#<decoded-time 2026-09-28 13:40:24 +09:00 Mon>`（夏時間なら末尾に `dst`） | 同左 |
| `heap-info` | `#<heap-info 176246 of 262144 cells live (67%), 85898 free, 2058 symbols, 5928 strings, 199 boxes, 2 collections, growable>` | 同左 |
| `defstruct` の型 | `#<point x: 1 y: 2>`（フィールド名と値） | 同左（フィールドは `~a` で） |

規則:

- **登録は静的**。`impl` はふつうのメソッド定義として型検査されるので、型名の打ち間違いも
  シグネチャ違いもコンパイルエラーになる。
- **ジェネリック型にも効く**。`(impl print-object box<T> (where (print-object T)) ...)` は
  型引数ごとに別の本体へ飛ぶ——値は自分の型引数まで含めた型を覚えている（`box<i32>`）。
  `Vector<T>` のような組み込みのジェネリック型も同じ。
- **選択は印字時**。どのディレクティブがどの引数を消費するかは制御文字列の実行時の中身で
  決まるため、`~a` と `~s` の区別（＝`escape`）は印字の瞬間にしか分からない。CLOS が
  `print-object` メソッドを「クラスごとに定義し、印字時に選択する」のと同じ。
- **再入は組み込み表現へ戻る**。実装が `(format false "~a" self)` と自分自身を
  印字すると無限再帰になるので、印字中の値が再び現れたら組み込み表現に落とす。深さ制限では
  なく値の同一性で見るので、正当な自己参照構造の入れ子印字は妨げない。
- **スカラ型は全部この trait を実装している**。これは**境界として使うため**で、`format` の
  可変長引数は型変数を受け取れないので、「知らない型の値を描画してよい」とジェネリックなコードが
  言う手段はこの境界しかない（Rust の `T: Display` と同じ形）。`Array<T>` の `print-object` が
  その例。
- **境界を満たさない型引数では黙って組み込み表現になる**。`(impl print-object Array<T> (where
  (print-object T)))` は `Array<i32>` には効くが、`print-object` を書いていない `defstruct` を
  要素にした `Array` には効かない。配列を作っただけでエラーになるのは筋が通らないので、
  エラーにはしない。
- CL のもう一方の機構 `set-pprint-dispatch` / `*print-pprint-dispatch*`（型指定子をキーに
  した実行時の登録表）は**採用しない**。登録が無検査で、静的型付け言語には合わない。

## 6. 印字量の制御

### 6.1 深さ・長さ・共有

CLHS 22.1.1 の「値のどこまでを印字するか」を決める制御変数。4.1 の3つと同じく
代入可能なグローバルで、`print`/`println`/`format`/`pprint` のすべてに——`*print-pretty*` の
真偽にかかわらず——効く。

| 変数 | 型 | 既定 | 意味 |
|---|---|---|---|
| `*print-level*` | `int` | `0` | この深さ以上に入れ子になったオブジェクトを `#` で置き換える。印字対象そのものが深さ 0。0 は無制限 |
| `*print-length*` | `int` | `0` | リストの要素（`defstruct`/`defenum` 値のフィールドも）をこの個数まで印字し、残りを `...` にする。0 は無制限 |
| `*print-circle*` | `bool` | `false` | 真なら、印字前に値を走査して**2回以上現れるオブジェクトにラベルを振る**。最初の出現が `#n=…`、以降が `#n#` |

CL は「無制限」を `nil` で表すが、この言語に `nil` は無いので、`*print-right-margin*` 等と同じく
**0 を無制限**とする。負の値は意味を持たないので印字エラーになる。既定はすべて「制限なし／ラベルなし」で、CL の初期値と一致する。

```lisp
(setf *print-level* 2)
(println "~a" '(1 (2 (3 (4)))))   ; => (1 (2 #))
(setf *print-level* 0)

(setf *print-length* 4)
(println "~a" '(1 2 3 4 5 6))     ; => (1 2 3 4 ...)
(setf *print-length* 0)
```

**循環した構造を印字できるのは `*print-circle*` を真にしたときだけ**である。偽（既定）のまま
自分自身を指す値を印字すると、プリンタは循環を辿り続けてプロセスが落ちる——これは CL でも同じ
（CLHS は `*print-circle*` が偽のときの循環構造の印字を未定義としている）。

循環は「`defstruct` のフィールドを `setf` で自分自身に向ける」経路でのみ作れる（`Sexpr` の
セルは作成後に書き換えられないので、`'(1 2 3)` のようなリストが循環することはない）:

```lisp
(defstruct node (val int) (next Option<node>))

(let ((a (node::new 1 (Option::none))))
  (setf a::next (Option::some a))   ; a が a 自身を指す
  (setf *print-circle* true)
  (println "~a" a))                 ; => #1=#<node val: 1 next: (some #1#)>
```

ラベルは**1回の印字対象ごとに 1 から振り直す**（CL と同じ）。循環していなくても、同じ
オブジェクトが2回現れれば `#1=`/`#1#` が付く——「この2つは同一のオブジェクトだ」という情報を
出力に残す、CL の仕様どおりの挙動:

```lisp
(setf *print-circle* true)
(let ((x '(1 2)))
  (println "~a" (list x x)))        ; => (#1=(1 2) #1#)
```

共有が1つも無い値では**ラベルは一切現れない**ので、この変数を真にしたまま普段のコードを動かしても
出力は変わらない。

### 6.2 基数・大小・読み戻し

| 変数 | 型 | 既定 | 意味 |
|---|---|---|---|
| `*print-base*` | `int` | `10` | 整数（固定幅と `int`）を印字する基数。2〜36 の外は**印字エラー**（CL も範囲を規定している） |
| `*print-radix*` | `bool` | `false` | 真なら基数の印を付ける。`#b`/`#o`/`#x`、それ以外は `#NNr`、基数 10 は末尾の `.`。印は符号の**前**（`#x-ff`） |
| `*print-case*` | `symbol` | `:downcase` | シンボル名の大小。`:upcase` / `:downcase` / `:capitalize`（CL と同じ綴り）。それ以外のシンボルは印字エラー |
| `*print-readably*` | `bool` | `false` | 真なら読み戻せる形で印字する。エスケープを強制し、`*print-level*`/`*print-length*` の打ち切りを無効化する |
| `*print-lines*` | `int` | `0` | pretty printer が使ってよい行数。超えた分は切り、末尾に CL と同じ `..` を付ける。0 は無制限。負の値は印字エラー |
| `*print-escape*` | `bool` | `true` | `write`/`write-to-string` が `prin1` と `princ` のどちらをするか。**これを読むのはその 2 つだけ** |
| `*print-array*` | `bool` | `true` | `Vector<T>` と `Array<T>` が中身を見せるか。真なら CL の配列構文（`#(1 2 3)` / `#2A((1 2) (3 4))`）、偽なら型と形だけの `#<vector<int> 3>` / `#<array 2x3>` |

```lisp
(dlet ((*print-base* 16)) (format false "~a" 255))                    ; => "ff"
(dlet ((*print-base* 16) (*print-radix* true)) (format false "~a" 255)) ; => "#xff"
(dlet ((*print-case* :upcase)) (format false "~a" 'hello))            ; => "HELLO"
```

`*print-radix*` が付ける印はリーダが読み戻せる（[構文リファレンス](../syntax.md#1-字句要素)の radix 表記）。

**`*print-case*` の既定が CL と違う理由**: CL の既定は `:upcase` だが、それは CL のリーダが
シンボル名を大文字で格納するから——つまり「格納されているまま」の意味。このリーダは小文字で
格納するので、同じ意味になる既定は `:downcase`。

**`*print-readably*` に無い半分**: CL は読み戻せない値に `print-not-readable` を上げるが、
この言語には上げるコンディションが無く、`print-object` が何でも印字しうるユーザ型について
可否を決める手段も無い。エスケープと打ち切りの上書きだけがある。

**`*print-escape*` を読むのが `write` だけな理由**: CLHS どおり `~s`/`prin1`/`pprint` は
これを真に、`~a`/`princ` は偽に、それぞれ自分の呼び出しの間だけ束縛する。つまり誰も
束縛していない状態で読まれるのは `write`/`write-to-string` だけ。`print-object` の実装は
この大域変数ではなく自分の `escape` 引数を読むこと——そちらがディレクティブの選んだ値を運ぶ。

**CL にあって無いもの**: `*print-gensym*`（未 intern のシンボルが無い）。

### 6.3 一時的な差し替え

CL はこれらを `let` で束縛するが、この言語の `let` は字句束縛なので `dlet`
（[その他](system.md#10-その他)）を使う:

```lisp
(dlet ((*print-level* 2) (*print-length* 4))
  (println "~a" x))                 ; この 1 回だけ制限が効く
(with-standard-io-syntax (println "~a" x))   ; 全部を標準値に戻して印字
```

`with-standard-io-syntax` は印字制御変数を全部標準値に、`*read-eval*` を `true` にして本体を走らせる。
