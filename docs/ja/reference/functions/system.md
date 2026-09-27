# 時間・実行環境・処理系

時間、実行環境の問い合わせ、処理系の道具、テキストの解析と評価、docstring、マクロまわりの関数。

## 1. 時間

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `universal-time` | — | `defstruct` | `day`（1900-01-01 からの日数）と `second`（その日の中の秒、0..86399）の2フィールド |
| `internal-time` | — | `defstruct` | `second` と `microsecond`（その秒の中、0..999999）の2フィールド |
| `get-universal-time` | `(get-universal-time)` | `()→universal-time` | CL の紀元（1900-01-01 UTC）からの時刻 |
| `get-internal-real-time` | `(get-internal-real-time)` | `()→internal-time` | プロセス基準の経過時間 |
| `get-internal-run-time` | `(get-internal-run-time)` | `()→internal-time` | このプロセスが使った **CPU 時間**（ユーザ＋システム） |
| `internal-time-seconds` | `(internal-time-seconds it)` | `internal-time→f64` | 秒数として。2つの読みの差を報告するときの形 |
| `internal-time-units-per-second` | — | `int` | `1000000`（マイクロ秒）＝`microsecond` フィールドの単位。CL 同様、値は処理系の選択 |
| `time` | `(time form)` | マクロ | `form` を実行し、実時間と CPU 時間を1行ずつ印字して `form` の値をそのまま返す |

実時間と CPU 時間は別のことを言う。I/O 待ちが主な処理は両者が大きく開き、その差こそが
知りたい情報なので、`time` は両方を出す。

タスクを止める `sleep` は [タスクとチャネル](concurrency.md#3-yield--sleep--譲る)。

## 2. 日時への分解・合成

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `decoded-time` | — | `defstruct` | `second` / `minute` / `hour` / `date` / `month` / `year` / `day-of-week` / `daylight-p` / `zone` の**9フィールド**。CL の9個の返り値を1つの構造体にしたもの（多値が無いため） |
| `decode-universal-time` | `(decode-universal-time ut &optional zone)` | `(universal-time,int)→decoded-time` | 万国時を暦の成分へ。`zone` はグリニッジ以西の時間数（CL と同じ向き）。**省略すると地方時**（CL と同じ） |
| `encode-universal-time` | `(encode-universal-time sec min hour date month year &optional zone)` | `(int×6,int)→universal-time` | 逆向き。`zone` を省くと引数は**地方時**として読まれる |
| `get-decoded-time` | `(get-decoded-time)` | `()→decoded-time` | いまを地方時で分解したもの |
| `timezone-offset-seconds` | `(timezone-offset-seconds day second)` | `(int,int)→Option<int>` | その万国時における地方時のグリニッジ以西**秒**数 |
| `timezone-daylight-p` | `(timezone-daylight-p day second)` | `(int,int)→Option<bool>` | その万国時に夏時間が施行されていたか |

`day-of-week` は CL と同じく **0 が月曜、6 が日曜**。

**zone を省くと地方時**——CL と同じ。地方時のずれは OS に訊くので、機械がどの地域にあるかで
結果が変わる。**明示的な zone を渡せば決定的**になり、`0` は UTC。

`zone` の単位は CL と同じ「グリニッジ以西の**時間**数」で、UTC+9 は `-9` と読む。ただし
**引数は整数、結果の `zone` フィールドは `f64`**。実在するずれは時間の整数倍とは限らず
（インドは +5:30、ネパールは +5:45）、報告値を丸めると黙って嘘になるため。手で書く zone は
整数時間なので引数側は `int` にしてある。

`zone` を明示したときは CL の規定どおり `daylight-p` は `false`、`zone` は渡した値そのもの
（*If a time-zone is supplied, daylight saving time information is ignored*）。

夏時間の切り替わりの中にある地方時はそもそも一意でなく、CL もどちらを取るとは言っていない。
`encode-universal-time` はそのような時刻にもどちらか一方の答えを返す。

## 3. 実行環境

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `command-line-args` | `(command-line-args)` | `()→Vector<string>` | コマンドライン。**要素0はプログラム名** |
| `getenv` | `(getenv name)` | `string→Option<string>` | 環境変数。未設定でも非UTF-8でも `none` |
| `home-directory` | `(home-directory)` | `()→Option<string>` | `$HOME`。`user-homedir-pathname`（[パス名](streams-files.md#92-関数)）の土台 |
| `lisp-implementation-type` | `(lisp-implementation-type)` | `()→string` | `"typelisp"` |
| `lisp-implementation-version` | `(lisp-implementation-version)` | `()→string` | 処理系の版数 |
| `machine-type` | `(machine-type)` | `()→string` | CPU アーキテクチャ（`x86_64` / `aarch64` …）。**ビルド先**の値 |
| `machine-instance` | `(machine-instance)` | `()→Option<string>` | ホスト名 |
| `machine-version` | `(machine-version)` | `()→Option<string>` | **実行中**のハードウェア名（`Apple M1` / `Intel(R) Xeon(R) …`）。分からない環境では `none` |
| `software-type` | `(software-type)` | `()→string` | OS（`macos` / `linux` …） |
| `software-version` | `(software-version)` | `()→Option<string>` | OS のリリース（`uname -r`、例 `24.6.0`） |
| `short-site-name` | `(short-site-name)` | `()→Option<string>` | 設置場所の短い名前。**常に `none`** |
| `long-site-name` | `(long-site-name)` | `()→Option<string>` | 同じく長い名前。**常に `none`** |

`Option` を返すものは CL が `NIL` を許している項目（*or nil if no such name can be
determined*）。site 名は POSIX に記録場所が無いので常に `none`——SBCL も同じものを返す。
`machine-type` と `machine-version` の違いに注意: 前者はこのバイナリが**ビルドされた**
アーキテクチャ、後者はいま**動いている**チップ。

`command-line-args` の要素0は、`typl script.typl a b` ならスクリプトのパス、AOT 実行ファイル
`./prog a b` なら実行ファイル自身。**どちらの走らせ方でも同じ添字で同じ引数が読める**
（`typl` は自分の名前と `--heap-cells` 等のオプションを取り除いてから渡す）。

## 4. ユーザへの問いかけ

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `y-or-n-p` | `(y-or-n-p question)` | `string→bool` | `y` / `n` を1文字で受ける。受け付けるまで訊き直す |
| `yes-or-no-p` | `(yes-or-no-p question)` | `string→bool` | `yes` / `no` を綴らせる。間違えると高くつく問い用 |

どちらも `*standard-input*` から読む。入力の終端だけが問い直しを止め、そのときは `false`。

## 5. 処理系の道具（CLHS 25.2）

処理系が自分自身について答える層。`heap-info` / `room` / `dribble` は普通の関数、
`trace` / `untrace` / `step` / `disassemble` / `ed` は**特殊形**（`trace` / `untrace` /
`disassemble` / `ed` は定義の*名前*を、`step` は*フォーム*を、いずれも未評価で受ける）。

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `heap-info` | `(heap-info)` | `()→heap-info` | ヒープの現況を構造体で。`room` が印字するのと同じ数 |
| `room` | `(room &optional verbose)` | `(bool)→()` | `heap-info` を `*standard-output*` へ報告する。`(room true)` で詳しく |
| `dribble` | `(dribble &optional path)` | `(string)→Result<(),FileError>` | セッションの出力を `path` へ記録し始める／引数なしで記録を終える |
| `trace` | `(trace name...)` | `Sexpr` | 名前を挙げた定義の呼び出しを `*trace-output*` へ報告する。いま trace 中の名前の一覧を返す |
| `untrace` | `(untrace name...)` | `Sexpr` | 報告をやめる。**引数なしで全解除** |
| `step` | `(step form)` | `form` の型 | `form` を評価しながら、呼び出しごとに止まって訊く |
| `disassemble` | `(disassemble name [llvm])` | `()` | その定義が何になるかを印字する。既定はホストの機械語、`true` で LLVM IR |
| `ed` | `(ed)` / `(ed name)` / `(ed "path")` | `Result<(),FileError>` | `$VISUAL`／`$EDITOR` を起動する。名前を渡すとその定義が書いてある行を開く |

`trace`/`untrace`/`step`/`disassemble` はインタプリタ専用で、これらを呼ぶ関数はコンパイルできない
（[構文リファレンス 10 章](../syntax.md#10-コンパイル)）。

### 5.1 `heap-info` の欄

| 欄 | 型 | 中身 |
|---|---|---|
| `capacity` / `live` / `free` | `int` | cons アリーナ全体と、その内訳。3 つは必ず `live + free = capacity` |
| `symbols` / `strings` / `boxes` | `int` | ヒープが持つ他の 3 種の現在数 |
| `gc-count` | `int` | この処理系が始めてからの収集回数 |
| `growable` | `bool` | アリーナがまだ伸びうるか |

欄はすべて `int`。成長の上限（`typl --heap-cells` の説明を参照）は報告しない。読み手が知りたいのは
伸びられるかどうか（`growable`）のほうだから。

### 5.2 `trace` / `step` が見えるもの・見えないもの

- **コンパイル済みの本体を持つ定義も、解釈実行中の呼び出し地点からは見える。**
- **コンパイル済みのコードの*中*の呼び出し地点は見えない**。コンパイル済みの本体を持つ名前を
  `trace` すると、その旨を 1 行注記する。SBCL が local call について言っているのと同じ制限。
- **クロージャ値越しの呼び出し（`funcall`/`apply`）は見えない**。クロージャは名前を持たない。
- **ジェネリックな定義は対象外**。型ごとの実体は使用箇所ごとに作られるので、名指しできる単一の
  本体が無い（`compile` が断るのと同じ理由・同じ文言）。

`step` のコマンドは `s`（この呼び出しへ入る／空行も同じ）・`n`（この呼び出しは飛ばす）・
`c`（以後訊かない）・`q`（中止）。**標準入力が端末でなければ `step` はただ `form` を評価する**
——CLHS が明示的に許している退化で、スクリプトやテストが答えようのないプロンプトで
固まらないため。

`ed` の `$VISUAL`／`$EDITOR` は空白で分割されるので `EDITOR="code -w"` も書ける。
どちらも未設定なら `Err`——`vi` を推測しない。行番号は `+N` の形で先頭に渡す。

`dribble` が記録するのは、セッションの出力がプロセスを出る 3 つの経路すべて:
`print`/`println`/`format` が書くもの、標準出力へつながるストリームへ書いたもの、
そして REPL で打った行と REPL が印字し返した値。

## 6. 解析・評価

いずれも実行時の（プログラム自身は制御できない）テキスト・データを扱うため、失敗時は panic では
なく `Result` の `Err` を返す。エラー型は操作ごとの具象型（[エラー型](option-result.md#3-エラー型と-error-トレイト)）。

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `parse-int` | `(parse-int s &key radix junk-allowed)` | `string→Result<int,ParseIntError>` | CL の `parse-integer`。前後の空白（`trim` と同じ集合）を読み飛ばし、符号 `+`/`-` を 1 つ、続けて `radix` 進（既定 10、2〜36。10 より上の桁は大文字小文字どちらでも）の数字を読む。桁数に上限は無い（`int`）。それ以外の文字が残れば `Err`。`:junk-allowed true` なら最初の非数字で読むのをやめて残りを無視する——ただし数字が 1 つも無ければ `Err`（CL の `nil` に当たる）。CL の第 2 値（読み終わり位置）は返さない。範囲外の `radix` は panic（テキストではなく呼び出し側の誤り） |
| `parse-float` | `(parse-float s)` | `string→Result<f64,ParseFloatError>` | 浮動小数点数。`inf`/`nan` も受ける |
| `read` | `(read s)` | `string→Result<Sexpr,ReadError>` | `s` から `Sexpr` を1つ読む（ソースを読むのと同じリーダ）。不完全な括弧・文字列などは `Err`。ストリームから読むのは `read-sexpr`（[ストリーム](streams-files.md#6-ジェネリック関数とファイル操作)） |
| `read-from-string` | `(read-from-string s [start])` | `(string,int)→Result<cons-cell<Option<Sexpr>,int>,ReadError>` | `read` に**読み終わり位置**を添えたもの。`(car r)` が値、`(cdr r)` が次に読む文字位置。`start` 省略時は 0 |
| `read-from-string-preserving-whitespace` | 同上 | 同上 | 同上だが datum を終わらせた空白を消費しない。違いは返る位置に出る |
| `eval` | `(eval form)` | `Sexpr→Result<Sexpr,EvalError>` | `form` を実行時に型チェックして評価する。CL の `eval` に準拠 |

CL は `read-from-string` から**2 値**（値と位置）を返すが、この言語に多値は無いので
`cons-cell` 1 つで返す。位置があると、文字列を 1 データずつ読むのが再スキャンではなく
ループになる:

```lisp
(let ((s "1 2 3") (i 0) (going true))
  (while going
    (match (read-from-string s i)
      ((ok p) (progn (println "~s" (car p)) (setf i (cdr p))
                     (if (>= i (length s)) (progn (setf going false) ()) ()) ()))
      ((err e) (progn (setf going false) ())))))
```

`preserving-whitespace` の違いは**空白 1 文字**だけ——CL の `read` は datum を終わらせた空白を
消費し、`read-preserving-whitespace` は残す。`(read-from-string "12 34")` は位置 3 を返し、
preserving 版は 2 を返す。

リーダが読む数値表記は [構文リファレンス 1 章](../syntax.md#1-字句要素)。`*print-radix*`
（[印字](printing.md#62-基数大小読み戻し)）が印字する表記はそのまま読み戻せる。CL の `*read-base*` は無い。

### 6.1 `eval` の意味

CLHS の `eval` に準拠する: **現在の大域環境**（グローバルの関数・変数・型・マクロ。実行時に
追加された定義も含む）で、かつ **null 字句環境**（呼び出し元の `let`/`lambda` のローカル束縛は
見えない）で評価する。式でも定義（`defun`/`defvar`/`defstruct`/`defenum`/`defmacro`）でも評価
でき、定義は即座かつ永続的にグローバル環境へ登録される。

```lisp
(eval (unwrap (read "(+ 40 2)")))                 ; => (ok 42)
(defvar (x i32) 10)
(eval (unwrap (read "(+ x 5)")))                  ; => (ok 15)  ; グローバル x が見える
(eval (unwrap (read "(defun sq ((n i32)) i32 (* n n))")))  ; => (ok sq)  ; 定義名を返す
(eval (unwrap (read "(sq 9)")))                   ; => (ok 81)  ; 直前の定義が見える
```

- **戻り値**: 式なら評価結果を `Sexpr` として、定義なら定義名シンボルを返す（CL と同じ）。
  結果を使うには `Sexpr` を `match`（`(int n)`/`(str s)`/…）で分解する。
- **静的型ゆえの違い（重要）**: CL は結果の実値を返すが、この言語では戻り型を一律
  `Result<Sexpr,EvalError>` にするしかない。また **静的に書いたコードは、実行時に `eval` が定義する
  名前を前方参照できない**——ファイル中に直接書いた `(sq 9)` は、`sq` を定義する `eval` が
  走る前に検査され「未定義」になる。ただし **後続の `eval` からは見える**（その `eval` の型チェックは
  実行時、定義後に走るため）。REPL は1行ずつ検査・実行するので、`eval` で定義した名前を次の行から
  直接呼べる。
- **エラーの扱い**: 型エラー・構文エラーは `Err` を返す（panic しない）。評価したコード内の
  **実行時の panic**（ゼロ除算など）は、直接書いたコードと同様にそのまま伝わる。途中の
  `unwind-protect` の cleanup は走る（[構文リファレンス 8 章](../syntax.md#8-非局所脱出catch--throw--unwind-protect)）。
- **名前空間**: `typl file.typl` の実行時、`eval` はそのスクリプトのモジュールの名前空間で
  評価される（スクリプト自身のグローバルが見える）。REPL はルート名前空間で評価する。
- **コンパイル**: `read` も `eval` もコンパイルできる。AOT 実行ファイルでの扱いと帰結（eval した
  フォームは解釈実行される）は [構文リファレンス 10.2](../syntax.md#102-aot-実行ファイルの中の-eval)。

## 7. docstring / `documentation`

`defun`/`defmethod`（`impl` 内も含む）/`defmacro`/`defvar`/`defconstant`/`defstruct`/`defenum`/
`deftype`/`deftrait` は docstring を持てる。位置は CL のそれぞれの規則に従う:

| フォーム | docstring の位置 |
|---|---|
| `defun` / `defmethod` / `defmacro` | 本体の先頭（戻り値型・`where` 節の後）。ただし後ろに本体フォームが最低1つ続く場合のみ——単独の文字列は戻り値のまま |
| `defvar` / `defconstant` | 初期値の**後ろ**: `(defvar (name Type) value "doc")` |
| `defstruct` / `defenum` | 名前の**直後**、フィールド/バリアント列の前 |
| `deftype` | 名前の**直後**、型の前: `(deftype meters "doc" i32)` |
| `deftrait` | 継承リストの直後、アイテム列の前。トレイト全体に1つ。**デフォルト実装を持つメソッド**は、その本体の直前に自分の docstring を置ける |

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `documentation` | `(documentation name)` | （特殊形。`name` は裸シンボルまたは `Type::method`）→`Option<string>` | `name` の docstring を返す |

`documentation` は `quote`/`compile` と同様の特殊形（`name` を評価せず、未評価の名前として読む）。
CL の `(documentation 'name 'function)` と異なり型引数を取らない代わりに、裸名を**変数→関数→型→
トレイト→マクロ**の順（式として評価するときの裸識別子の優先順位と同じ）で解決する。`Type::method`
の形なら関連メソッド/静的メソッドの docstring を引く。

```lisp
(defun square ((n i32)) i32
  "Returns n squared."
  (* n n))

(unwrap-or (documentation square) "no docs")   ; => "Returns n squared."

(defstruct point
  "A 2D point."
  (x i32)
  (y i32))

(unwrap-or (documentation point) "no docs")    ; => "A 2D point."
```

**チェック時に値が決まる**: 名前が何の定義にも解決できない場合はチェック時のエラー（未定義変数の
参照などと同様）。解決はできたが docstring が無い場合は `Option::none`。

**対象外**:

- `(setf documentation)`（docstring の実行時書き換え）は無い。
- モジュール修飾された自由名（`mod::name`。`Type::method` は対応）は非対応。
- `deftrait` 内の**本体を持たない**メソッド宣言は docstring を持てない。末尾の文字列リテラルは
  それ自体がデフォルト実装の本体（＝戻り値）になるので、両者を区別する手段が無い。

言語サーバ（`typl-lsp`）のホバーにも docstring が表示される。

## 8. マクロ

マクロの定義方法は [構文リファレンス 3.14](../syntax.md#314-defmacro--マクロ定義)。

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `gensym` | `(gensym)` / `(gensym prefix)` | `(&optional string)→symbol` | 新しいシンボル。名前は `" <prefix><n>"` で `n` は `*gensym-counter*`。先頭の空白はソースに書けないので、生成した束縛が書かれた名前と衝突しない |
| `*gensym-counter*` | 変数 | `int` | `gensym` が次に使う番号。CL 同様、読んでも設定してもよい |
| `macroexpand-1` | `(macroexpand-1 form)` | `Sexpr→Result<Option<Sexpr>,EvalError>` | マクロ呼び出しを 1 段展開。`none` は「マクロ呼び出しではない」 |
| `macroexpand` | `(macroexpand form)` | `Sexpr→Result<Sexpr,EvalError>` | マクロでなくなるまで繰り返す |

`macroexpand-1` が返すのは `Option`——CL は「展開したか」を第 2 返り値で伝えるが、多値が
無いので `none` がそれに当たる。**自分自身の呼び出しへ展開するマクロと非マクロを取り違えようが
ない**。展開の 1 段は型検査が使うのと同じもので、プログラムが見るものと検査が見たものはずれない。

```lisp
(defmacro twice (x) `(+ ,x ,x))
(macroexpand-1 '(twice 5))   ; => (ok (+ 5 5))
(macroexpand-1 '(+ 1 2))     ; => (ok ())      ; none は空リストとして出る（Option<Sexpr> は透過）
(macroexpand '(when true 1)) ; => (ok (if true (progn 1 ()) ()))
```

CL にあってここに無いもの: `eval-when`（`:compile-toplevel`/`:load-toplevel`/`:execute` が
常に一致するので選ぶ区別が無い）、`define-compiler-macro`、`load-time-value`、
`make-symbol`/`copy-symbol`/`gentemp`（未 intern のシンボル。束縛は名前で引かれるので得るものが無い）。

## 9. 局所的なマクロ束縛（`macrolet` / `symbol-macrolet`）

どちらも**値でない名前**を字句的に束縛する特殊形。実行時には何も残らない——本体が
コンパイルされるのは展開後の形。

```lisp
(macrolet ((twice (x) `(+ ,x ,x)))
  (twice 21))                       ; => 42

(let ((v (the Vector<i32> (Vector::new))))
  (progn (push v 7)
    (symbol-macrolet ((head (get v 0)))
      (progn (setf head 42) head))))  ; => 42
```

- `macrolet` の束縛は同名の大域マクロを**本体の間だけ**隠す。ラムダリストは `defmacro`
  と同じ（`&optional`/`&rest`/`&key`）。
- **同じ `macrolet` の兄弟どうしは、互いの*本体*からは見えない**（CL と同じ。`labels`
  との違い）。展開結果は使用位置で検査されるので、`earlier` が `(later ...)` へ展開する
  のは通る——その位置では両方が見えている。
- `symbol-macrolet` の名前は環境にふつうの束縛として入る。だから内側の `let` が同名を
  隠し、外側の変数は隠される——CL の規則がそのまま出てくる。
- **`setf` は展開先へ書く**。`(setf head 42)` は `(setf (get v 0) 42)`。
- 展開は**使用位置の環境**で検査される（束縛位置ではない）。

## 10. その他

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `assert` | `(assert test)` / `(assert test msg)` | `(bool[,string])→()` | 偽なら panic。メッセージ省略時は `assertion failed: <テストを書かれたまま>`（マクロなので式そのものを名指せる）。CL の restart はこの言語に無い |
| `warn` | `(warn control args...)` | `(string,...)→()` | `*error-output*` へ `WARNING: ` 付きで 1 行書いて**続行**する。`Result` を返しもせずプログラムを終わらせもせずに報告する手段 |
| `dlet` | `(dlet ((*var* val)...) body...)` | — | グローバルを `body` の間だけ差し替え、抜けるときに戻す。CL はこれを `let` と書くが、この言語の `let` は常に字句束縛なので別名（Emacs Lisp の同名マクロと同じ役目）。正常終了・`throw`・`panic`・`break`/`return` のどれで抜けても戻す。**タスクごとの束縛ではない** |
| `with-standard-io-syntax` | `(with-standard-io-syntax body...)` | — | 印字制御変数を全部標準値に、`*read-eval*` を `true` にして `body` を走らせる（[印字](printing.md#6-印字量の制御)） |
| `exit` | `(exit code)` | `int→!` | プロセスを終了する |
| `dump` | `(dump path)` | `string→bool` | いまの環境（型情報 + コンパイル済み本体）を1ファイルへ書き出す。`typl --image <path>` で立ち上げ直せる。インタプリタ専用（[構文リファレンス 10.1](../syntax.md#101-ダンプ)） |
