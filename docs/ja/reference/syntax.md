# typelisp 構文リファレンス

typelisp は静的型付きの Lisp。文法は S 式。組み込み関数・メソッドの一覧は
[組み込み関数](functions/README.md)、型の一覧は [types.md](types.md)、エラーメッセージの読み方は
[errors.md](errors.md) を参照。

## 1. 字句要素

- **大文字小文字は区別しない**。シンボルは読み取り時にすべて小文字へ正規化される。
- **コメント**: `;` で始まり行末まで（行コメント）。`#| ... |#`（ネスト可能なブロックコメント）。
- **読み込み時評価**: `#.(式)` は続くフォームを**読みながら実行**し、その値を読んだことにする。
  リーダがテキストだけの関数でなくなる唯一の場所。届く範囲は読み込み経路で変わり、これは
  CL と同じ:
  - `(load ...)` と REPL は 1 フォームずつ評価するので、**同じテキストの手前で定義した関数**を
    呼べる（CL の `load`）。
  - モジュールファイルは単位として検査され実行は `use` した側なので、`#.` から届くのは
    標準ライブラリと、そのセッションが既に実行したものだけ。ファイル自身の定義も、`use` した
    モジュールの定義も**まだ走っていない**（CL の `compile-file` で `eval-when` が要るのと同じ）。
  - プログラムの中の `read` / `read-from-string` も `#.` を評価する（CL と同じ）。
  - `*read-eval*`（既定 `true`）を `false` にすると、`#.` はどこでも読み取りエラーになる——
    データとして読むテキストに実行させないためのスイッチ（CL と同じ）。`#.` のたびに読むので、
    `setf` は次に読むフォームから効く。`with-standard-io-syntax` の中では `true`。
- **真偽値**: `true` / `false`。
- **整数**: 10進（`42`, `-7`）。符号 `+`/`-` を前置可能。10進以外は CL の radix マクロ
  `#b`/`#o`/`#x`/`#NNr` で書く（符号は印の後ろ、`#x-ff`）。`0x` 接頭辞は CL に無いので
  採らない——`0xff` はシンボルとして読まれる。
  型注釈のない整数リテラルは既定で `int`（任意精度、[数値](functions/numbers.md#3-任意精度整数-int)）——大きさに
  上限は無い。**期待される型が固定幅の整数型ならその型になり、その型が持てる値かどうかが
  検査される**——`(the u8 300)` は型エラー（切り詰めが欲しければ `(as u8 300)` と書く）。
  `(the u32 4294967295)` や `(the u32 #xFFFFFFFF)` はこの規則で書ける。`int` の値が 63bit の
  即値に入るか多倍長になるかは値の大きさで決まり、専用構文はない（CL と同じ）。
- **浮動小数点数**: 小数点または指数表記（`e`/`E`）を含むもの（`1.5`, `3.0e10`）。
  既定で `f64`（期待される型が `f32` ならその型になる）。
- **比 (ratio)**: `分子/分母`（10進のみ、例 `1/3`）。読み取り時に CL 仕様どおり既約化される
  （`2/4` は `1/2`）。整数値になるもの（`4/2` など）は `ratio` ではなく `int` として
  読まれる。分母が `0`（`1/0`）は読み取りエラー。
- **文字**: `#\` に続けて1文字、または名前付き文字。例: `#\a` `#\Space` `#\Newline`
  `#\Tab` `#\Return` `#\Page` `#\Nul`（`#\Null` も可）`#\Backspace`。名前は大文字小文字を区別しない。
- **文字列**: `"..."`。エスケープは `\n` `\t` `\r` `\0` `\\` `\"`（それ以外の `\x` はそのまま `x`）。
- **シンボル**: 英数字・記号を含む任意のトークン（`+` `<=` `my-func` など）。
- **キーワード**: `:name` のようにコロンで始まるシンボル（CL 準拠）。自己評価する——束縛を探さず
  それ自身の値になり、静的型は `symbol`。同名なら常に同一オブジェクト（`(eq :foo :FOO)` は真。
  他のシンボル同様に小文字化される）。コロン自体は名前の一部で、`(symbol->string :foo)` は
  `":foo"`（typelisp にはパッケージ機構が無いため、CL の `symbol-name` とは異なる）。
  `:` 単独や `:a:b` のように追加のコロンを含むものは読み取りエラー。判定は `keywordp`。
  先頭が `::` のものはキーワードではなく絶対パス（下記）。
  なお `:dyn` は型位置専用の予約キーワードで、それ以外の場所に書くとエラーになる（[2 章](#2-型の書き方)参照）。
- **リスト**: `(a b c)`。ドット対 `(a . b)` も読み取り可能。
- **空リスト `()`**: 文脈によって `Unit` 型の値、または `Option<Sexpr>` の `none` になる。
  **`Sexpr` に空リストの変種は無い**——`Sexpr` は「空でない S 式」を表し、S 式データの型は
  `Option<Sexpr>` である（[4.3 match](#43-match--パターンマッチ) の「`Option<Sexpr>` のパターン」参照）。
- **quote/quasiquote/unquote**:
  - `'x` → `(quote x)`
  - `` `x `` → `(quasiquote x)`
  - `,x` → `(unquote x)`（quasiquote の中でのみ意味を持つ）
  - `,@x` → `(unquote-splicing x)`（リスト要素として展開時に結合される）
- **パス `::`**: `foo::bar` はモジュール・型・メンバをたどるパスとして読まれる（1 つの
  シンボル名にはならない）。`::foo` のように先頭が `::` の場合はルートからの絶対パス。
  ジェネリック引数の内側の `::`（`Vec<a::b>` など）はパス区切りとして扱われない。

## 2. 型の書き方

型はソース上では通常のシンボルまたはリストとして書く。

- **プリミティブ型**: `int` `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `bool` `char` `string` `symbol`。
  `int` が整数（CL の integer——63bit 即値と多倍長のあいだを自動で行き来する、
  [数値](functions/numbers.md#3-任意精度整数-int)）、6 つの固定幅は幅と符号を名乗る型
  （64bit 幅の整数型は無い——[数値](functions/numbers.md#1-固定幅整数)参照）。
- **有理数型**: `ratio`（既約な有理数）。CL 準拠でヒープ確保され、`int`/`f64` 等との暗黙変換は
  ない（`as`/`try-as` または変換メソッドで明示する。[数値](functions/numbers.md#5-有理数-ratio)参照）。
- **C 境界の生の語**: `ptr`（不透明ポインタ）、`c-long` / `c-ulong`。FFI 専用で、値にするには
  `(unsafe ...)` が要り、置ける場所も限られる（[3.3 defffi](#ptr--c-long--c-ulong--生の機械語)）。
  64bit 整数が欲しい場面でこれを使ってはいけない——算術は付いていない。
- **不透明な可変型**: `random-state`（乱数生成器の状態）。`Vector<T>`/`HashTable<K,V>`/`Sexpr` には
  入れられない（`Option<T>`/`Result<T,E>` には入る）。
- **Unit 型**: `()`
- **Never 型**: `!`（`panic`/`unreachable`/`todo`/`return`しないループ 等、発散する式の型。
  任意の期待型に適合する）
- **関数型**: `(fn (引数型...) 戻り値型)`。可変長引数を持つ関数型は
  `(fn (引数型... &rest 要素型) 戻り値型)`。
- **ジェネリック型**: `Name<T1,T2,...>`（空白なしの1トークンとして読み取られる）。
  例: `Option<i32>` `Result<i32,ParseIntError>` `HashTable<string,i32>` `Vector<T>`。
  型引数には unit 型 `()` も書ける（`Result<(), FileError>`）。`(`/`)` は本来トークンを
  切るデリミタだが、山括弧が開いている間に限りこの2文字の組だけが通る。`()` は
  フィールド型・引数型としても使える。
- **ジェネリック型の適用形**: `(Name T1 T2 ...)` — `Name<T1,T2,...>` と同じ型を指す
  リスト形式の綴り。例: `(vector char)` は `Vector<char>` と同一。
  名前形が普通の書き方で、こちらは**型引数が名前で綴れない場合のためにある**——
  型引数はそれ自体が型式だが、1トークンの名前の中に書けるのは名前・`()`・`:dyn` だけで、
  関数型は書けない（`Vector<(fn (i32) i32)>` という綴りは存在しない）。
  トレイトの関連型を署名に代入した結果など、処理系が型を表示するときにもこの形で出ることがある。
- **修飾型名**: `module::Type` のように `::` で修飾できる。
- **trait オブジェクト型**: `:dyn Trait`（空白区切りの2語で1つの型）。実行時に具象型が決まる値を
  表し、trait のメソッド呼び出しは vtable 経由の動的ディスパッチになる。関連型を持つ trait は
  宣言順に位置指定で固定する（`:dyn Iter<i32>` は `Item` を `i32` に固定）。ジェネリック引数の
  内側にも書ける: `Vector<:dyn Drawable>` `HashTable<string, :dyn Drawable>`。
  具象値は期待位置で自動的に箱詰めされ、明示形は `(as :dyn Trait 式)`。
  `:dyn Sub` の値はスーパトレイト（推移的に継承しているものすべて）の `:dyn Super` を要求する
  位置にもそのまま渡せる（アップキャスト）。継承関係の無いトレイトへは渡せない。
  `:dyn` にできるトレイトの条件は [3.9 deftrait / impl](#39-deftrait--impl--トレイト機構) を参照。
  `:dyn` を型位置以外に書くとエラー。
- 組み込みジェネリック型: `Option<T>`（`Some(T)` / `None`）、`Result<T,E>`（`Ok(T)` / `Err(E)`）、
  `HashTable<K,V>`、`Vector<T>`、並行機構の `Task<T>` / `Thread<T>` / `Chan<T>`（[12 章](#12-並行機構タスク)）。
  S 式データの型 `Sexpr` もある。組み込みの具象エラー型は `ParseIntError` /
  `ParseFloatError` / `ReadError` / `EvalError` / `FileError` / `NetError`、標準ライブラリの構造体として
  `SimpleError` / `WrappedError`（`Error` は型ではなくトレイト——`:dyn Error` として使う）。
  一覧は [types.md](types.md)。
- **型とトレイトは同じ名前空間**（Rust と同じ）: 同一モジュール内で型（`defstruct`/`defenum`）と
  トレイト（`deftrait`）に同じ名前は付けられない。

## 3. トップレベル定義

### 3.1 defun — 関数定義

```lisp
(defun name ((arg1 Type1) (arg2 Type2) ...) RetType
  body...)
```

- 引数の型・戻り値の型は必須。
- ジェネリック関数は名前に山括弧で型パラメータを書く: `(defun name<T1,T2...> (params) Ret body...)`
  （型位置の `Vector<T>` と同じ山括弧構文）。
- `defun`/`lambda`/`defmethod` は末尾に `&rest (name Type)` を書くと可変長引数を受け取れる:
  `(defun name ((a Type1) &rest (xs Type2)) Ret body...)`（本体内では `xs` は常に `Option<Sexpr>`
  ——S 式のリスト——として束縛される。呼び出し側の各実引数は `Type2` として個別に型検査されてから
  `Sexpr` へ包まれる）。
  `defmacro` にも独自の `&rest` があるが、常に無型の `Sexpr` である点が異なる（`defun`/`lambda`
  は要素型を明示する）。`fn` 型でも `(fn (T1... &rest Te) Ret)` の形で可変長関数の型を書ける。
- **`&optional` / `&key`**（`defun` と `defmethod`。`lambda`/`labels` は後述の理由で対象外、
  `defmacro` は後述の別実装）。順序は CL 流に `必須 &optional &rest &key`。各パラメータは
  `(name Type)` か `(name Type デフォルト式)` と書く:

  ```lisp
  (defun greet ((name string) &optional (suffix string)) string      ; デフォルト無し
    (match suffix ((some s) (append name s)) ((none) name)))         ; 本体では Option<string>

  (defun pow ((b i32) &optional (n i32 2)) i32 ...)                  ; デフォルト有り
  (pow 3)      ; n = 2
  (pow 3 5)    ; n = 5

  (defun mk (&key (a i32 0) (b string "z")) string ...)
  (mk :b "q")  ; 呼び出し側は `:name 値`、順不同。省略した分はデフォルト
  ```

  - **デフォルト式を書かなかったパラメータの型は `Option<Type>` になる**。省略すれば `none`、
    渡せば呼び出し側が書いた裸の値が自動で `some` に包まれる。CL の「省略されたかどうかを
    supplied-p 変数で知る」に当たるものが、静的型の側に出る形。
  - デフォルト式を書いた場合は宣言どおりの `Type` のまま。省略時はその**検査済みの式**が
    呼び出し側へそのまま埋め込まれる（呼び出しごとに評価される）。
  - **`&key` は `&optional`/`&rest` と同じ引数リストに混ぜられない**。CL 自身が抱える曖昧さ
    （末尾の実引数を、位置で埋まる `&optional` が取るのかラベルで照合する `&key` が取るのかが
    *値*に依存する）を、組み合わせを禁じることで回避している。`&optional` と `&rest` の併用は可。
  - ジェネリック関数でも使えるが、**省略された引数にしか現れない型パラメータは推論できず
    エラー**になる（そこには突き合わせる値が無いため）。
  - **`defmethod` でも同じ 3 区画が書ける**（インスタンスメソッド・静的関数の両方）。受け手の
    次から `&optional`/`&rest`/`&key` を並べる:

    ```lisp
    (defstruct box (w i32) (h i32))
    (defmethod grow ((self box) &key (dw i32 0) (dh i32 0)) i32 ...)
    (grow (box::new 1 2) :dh 10)

    (defmethod origin (point &key (x i32 0) (y i32 0)) point (point::new x y))   ; 静的関数
    (point::origin :y 7)
    ```

    ジェネリック型のメソッドでも使えるが、**デフォルト式を書いたパラメータの型に所有者の型
    パラメータを書くことはできない**（`defun` が自分の型パラメータについて負うのと同じ制限。
    省略時に埋め込まれるのは*検査済み*の式なので、その型が抽象変数のままでは困る）。
  - **トレイトのメソッドでは使えない**。`deftrait` 側に構文が無く、`impl` 側だけが区画を宣言
    できてしまうと、`:dyn` 受け手の呼び出し（トレイトの宣言から引数を埋める）と具象受け手の
    呼び出し（`impl` の宣言から埋める）が別物になる。vtable スロットのアリティは固定。
  - **`lambda` / `labels` では使えない**（`&rest` は使える）。省略された引数を埋めるには
    呼び出し側が**呼ばれる側の検査済みデフォルト式**を読む必要があり、それは名前で解決した
    シグネチャからしか手に入らない。`lambda` は値として渡され、その値を説明するのは関数型
    `(fn ...)` だけ——そこに式を置く場所は無いし、置けば「同じシグネチャでデフォルトだけ違う 2 つの
    ラムダ」が別の型になってしまう。`&rest` は型の話に閉じているので関数型に書ける。
- **前方参照は `defsignature` で宣言する**（下記）。宣言していない名前は、定義より前では
  呼べない——トップレベルは 1 フォームずつ、ソース順に検査・実行されるため。
- トレイト境界を要求する場合は本体の直前に `where` 節を書く:
  `(defun name<T> (params) Ret (where (Trait T (AssocName ConcreteType)...)) body...)`
  （`(AssocName ConcreteType)` による関連型の固定は省略可能）。
- **docstring**: `where` 節（あれば）の直後、本体の先頭に文字列リテラルを置くと docstring になる
  （CL 準拠）。ただし後ろに本体フォームが最低1つ続く場合のみ——単独の文字列は戻り値のままで
  docstring とは区別されない: `(defun f () string "doc" "value")` は docstring 付きで `"value"` を
  返すが、`(defun f () string "value")` は docstring なしで `"value"` を返す。
  `(documentation name)` で取り出せる（[docstring](functions/system.md#7-docstring--documentation)）。

### 3.2 defsignature — 前方宣言

```lisp
(defsignature name (引数型...) 戻り型)
(pub defsignature name (引数型...) 戻り型)
```

自分より**後**に定義される `defun` を呼ぶには、先にこう宣言する。相互再帰はこれでしか書けない:

```lisp
(defsignature odd2 (i32) bool)
(defun even2 ((n i32)) bool (if (= n 0) true  (odd2 (- n 1))))
(defun odd2  ((n i32)) bool (if (= n 0) false (even2 (- n 1))))
```

引数は**型だけ**を並べる。本体が無いので名前を付ける対象が無い。`&rest` は最後に
`&rest 要素型` と書ける。

宣言は**検査される**:

- 続く定義は宣言と一致しなければならない（引数の個数・型、戻り型、`&rest`、`pub` の有無）。
  食い違いは定義地点でエラーになる。
- 宣言したまま定義しないのはエラー（ファイル／モジュールの読み込み完了時に報告）。REPL は
  1 入力ごとには報告しない——宣言と定義を別の行に打てるべきなので。
- 定義**より後**に置いた宣言はエラー。何もできない宣言だから。

宣言できないものが 3 つある:

- **ジェネリック関数**。型ごとの実体を作るには本体が要り、宣言には本体が無い。前方呼び出しは
  解決できても実体化に失敗するので、宣言の時点で断る。
- **`&optional`/`&key`**。そのシグネチャは各デフォルト値の**検査済み**式を含み（引数省略時に
  呼び出し側へそのまま埋め込まれる）、宣言にはそれを置く場所が無い。
- **`defun` 以外**。`defmacro` は展開にマクロ本体が**実行済み**である必要があり、シグネチャ
  登録では代替できない。型（`defstruct`/`defenum`/`deftrait`）は、その登録が「型を登録する
  コード自身が必要とするもの」でシグネチャのように自己完結しない。`defmethod` は所有する型に
  登録されるので型に従う。

CL の対応物は `(declaim (ftype (function (i32) bool) even2))` だが、あちらは宣言システム
一式を伴い、かつ**助言**でしかない。こちらは静的型付けなので宣言は検査される。

### 3.3 defffi — C 関数の宣言（FFI）

```lisp
(defffi (名前 "c_symbol") (引数型...) 戻り型)
(defffi (名前 "c_symbol") (引数型...) 戻り型 :library "名前")
(defffi 名前 (引数型...) 戻り型)              ; 名前 = C のシンボル名
(pub defffi ...)
```

C の関数を宣言して呼べるようにする。形は `defsignature` と同じ——名前・引数型・戻り型・本体なし
——だが、本体が無いことの意味が違う。`defsignature` は「後で自分が定義する」約束で、`defffi` は
「本体はもう他人が書いてコンパイル済みだ」という宣言。

```lisp
(defffi (c-abs "abs") (i32) i32)
(defffi (c-sqrt "sqrt") (f64) f64)
(defffi (c-getpid "getpid") () i32)

(unsafe (c-abs -5))                          ; => 5
```

typelisp 側の名前と C のシンボル名を分けて書けるのは、typelisp の識別子は普通 `-` を含み、
C の識別子は含めないため。C 名を省くと名前がそのまま C のシンボル名になる。

**呼び出しには `(unsafe ...)` が要る**（スカラだけの関数でも）。宣言した C シグネチャが本物と
一致しているかはコンパイラに確かめようがなく、宣言を信じるほかない——`unsafe` はその責任を
引き受けたという印。一度だけ包んで安全なラッパを作るのが想定された書き方:

```lisp
(defun abs-i32 ((n i32)) i32 (unsafe (c-abs n)))
(abs-i32 -3)                                 ; 以降 unsafe は要らない
```

書ける型は `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `bool` `()`（void）`string`
`ptr` `c-long` `c-ulong`、それに型付きポインタ `(ptr T)`
（[後述](#def-c-struct-と型付きポインタ--c-の構造体を確保する)）。

`string` は `const char *`。typelisp の文字列は NUL 終端されておらず自身が NUL を含みうるので、
**渡すときは C 文字列へ複製**し、呼び出しが終わったら解放する。文字列の中に NUL があれば
エラーになる——C はその手前までしか見ないので、黙って別の文字列を渡すことになる。

**返すときも複製**する。解放はしない——C が返したものは C のもので、`getenv` のように静的な
表を指していることがある。呼び出し側が解放すべきメモリを返す関数（`strdup` など）は `ptr` で
受けて自分で解放する形にする。

結果が引数の内部を指す関数（`strchr`、`strstr`）も正しく動く。複製してから引数を解放する順に
なっている。

`string` を返すと宣言した関数が NULL を返したらエラーになる。`string` には「無かった」を表す値が
無いため。NULL がありうるなら `ptr` で受ける。

```lisp
(defffi (c-strlen "strlen") (string) i32)
(defffi (c-getenv "getenv") (string) string)

(unsafe (c-strlen "hello"))                  ; => 5
(unsafe (c-getenv "PATH"))                   ; => "/usr/bin:..."
```

`:library` を書くとその共有ライブラリを開いてシンボルを探す。省くと**プロセス自身**（既に
リンクされているもの全部——libc を含む）から探す。名前は `sqlite3` のような短い名前なら
`libsqlite3.dylib` / `libsqlite3.so` の順に、`/` を含むならパスとして扱う。開いたライブラリは
閉じない——中の関数を指したコードが走り続けるので、正しい寿命はプロセスの寿命だけ。

#### ptr / c-long / c-ulong —— 生の機械語

`ptr` は不透明なポインタ（`void *`、`FILE *`、何であれ宣言が意味したもの）。`c-long` /
`c-ulong` は C の `long` / `unsigned long`（`size_t`、`int64_t`、`intptr_t` も同じ）。

```lisp
(defffi (c-malloc "malloc") (c-ulong) ptr)
(defffi (c-free "free") (ptr) ())
(defffi (c-strlen "strlen") (string) c-ulong)

(unsafe (let ((p (c-malloc 16))) (c-free p) ()))
```

**`i64` / `u64` と呼ばないのは意図的。** この言語には 64bit 整数型が無い——タグ付きの即値は
63bit しかないため（[2 章](#2-型の書き方)）。`c-long` という名前は「これは C との境界を渡る語で
あって、この言語の整数ではない」と言っている。

**算術は付いていない。** `(+ x 1)` は書けない。付けられるのに付けていないのは、どこにも保存
できない値の上で、他のあらゆる数と幅の違う計算をさせないため——64bit 整数型を消したのと同じ
理由。あるのは**変換だけ**:

```lisp
(as i32 (unsafe (c-strlen "hello")))         ; 返ってきたものを読む
(as int (unsafe (c-strlen s)))               ; 正確に読むならこちら（int は 64bit を落とさない）
(try-as i32 (unsafe (c-strlen s)))           ; 入るかどうかを問う
(as c-ulong n)                               ; 他の整数から作る
```

整数**リテラル**は期待された型を取るので、渡すだけなら `as` は要らない:

```lisp
(unsafe (c-malloc 16))                       ; 16 は c-ulong として読まれる
```

範囲外のリテラルは他の幅と同じく拒否される（`(c-malloc -1)` は `c-ulong` に入らない）。

**置ける場所が限られる。** 引数型・戻り型・局所変数だけ。次はいずれもエラーになる:

```lisp
(defstruct handle (p ptr))          ; 構造体のフィールド
(defenum maybe (none) (some ptr))   ; 列挙型のフィールド
(defvar (block ptr) ...)            ; グローバル
(defffi f ((vector ptr)) i32)       ; 型引数の内側
```

理由は1つで、どれも**スロットが中身にタグを付ける**から。タグを付ければポインタの最上位
ビットが落ちる——64bit 整数型を消したのと同じ理由なので、`unsafe` でも許さない。これは許可の
問題ではなく、その表現が存在しないという話。

同じ理由で、入れ子の関数に**捕捉される**局所変数にもできない（捕捉された束縛はセルに入り、
セルは中身にタグを付ける）。これはコンパイル時に分かるので `(compile f)` で報告される。

GC は `ptr` を追跡しない。ヒープの外を指しているので、それが正しい。

宣言できないものが 4 つある:

- **可変長引数**（`printf`）。可変長部分は固定引数と別の規則で渡される（AArch64 Darwin では
  スタック）ので、固定シグネチャからは正しく呼べない。`&rest` は拒否される。
- **構造体の値渡し・値返し**。同じ理由（プラットフォームごとの受け渡し規則に依存する）。
  書ける型を上の一覧に閉じることで、綴れないようにしてある。
- **ジェネリック**。C に対応物が無い。
- **組み込みと同じ名前**。コンパイル済みの呼び出しはその名前で組み込みに解決されて
  しまうので、静かに間違うより断る。

#### コールバック —— C から呼び返してもらう

引数型に関数型 `(fn (型...) 戻り型)` を書くと、その引数は C が呼び返す関数（コールバック）になる。

```lisp
(defffi (c-qsort "qsort") (ptr c-ulong c-ulong (fn (ptr ptr) i32)) ())

(defun desc ((a ptr) (b ptr)) i32 ...)

(unsafe
  (c-qsort buf n 8 desc)                                 ; トップレベル関数
  (c-qsort buf n 8 (lambda ((a ptr) (b ptr)) i32 ...))   ; lambda
  (labels ((cmp ((a ptr) (b ptr)) i32 ...))
    (c-qsort buf n 8 cmp)))                              ; 局所関数
```

C の関数ポインタはコードのアドレスでしかなく、C は宣言どおりの引数だけを渡して呼ぶ。捕捉した
変数を渡す場所が無いので、**渡せるのは自由変数の無い関数だけ**で、これは型検査のときに調べる。

- 実引数には関数の名前か `lambda` 式を**直接**書く。関数を入れた変数は渡せない——どの関数が
  入っているか、したがって自由変数があるかどうかは、実行するまで分からないため。
- `lambda` は、その外側の局所変数を参照していればエラーになる。グローバル変数と
  トップレベル関数は参照してよい。
- 局所関数（`labels`）は、呼んでいる兄弟関数まで含めて自由変数が無いこと。兄弟関数は捕捉した
  変数の置き場を共有しているので、呼んでいる兄弟関数の捕捉はその関数の捕捉でもある。
- ジェネリック関数は、宣言した関数型から型が決まる。
- 関数型の中に書ける型は、上の一覧と同じ。ただしコールバックの戻り型に `string` は書けない
  （誰も解放しないメモリを C に渡すことになるため）。`string` の引数は、C が渡した文字列を
  typelisp の文字列へ複製する。

C 関数の呼び出しは `unsafe` の中でしか書けないので、コールバックを渡せるのも `unsafe` の中だけ。

**呼び返せるのは、typelisp が呼んだ C 関数が走っている間だけ。** それ以外の場所——typelisp を
走らせていないスレッド、シグナルハンドラ、`atexit` で登録した関数——から呼ばれると、理由を
表示してプロセスを止める。

**失敗は C を越えて伝わらない。** コールバックの中の `panic` や `throw` は C のフレームを越えて
巻き戻せない（未定義動作になる）ので、C には 0 を返し、C 関数が戻った時点で呼び出し元へ
投げ直す。失敗してから C 関数が戻るまでの間にもう一度呼ばれた場合は、実行せずに 0 を返す。

コールバックの中で待つことになる操作（空のチャネルからの `recv` など）はエラーになる
（[12.6](#126-コンパイル済みコードとタスク)）。

関数を定義し直すと、次に C へ渡したときから新しい定義が呼ばれる。

AOT（`compile-file`）でも同じように動く。C が呼ぶ入口は実行ファイルに組み込まれる。

**値として渡せない**。`(map f xs)` の `f` に FFI 宣言をそのまま書くことはできない——関数値は
定義の本体を包んだクロージャで、この宣言には包む本体が無いため。`lambda` で包む:

```lisp
(run-it (unsafe (lambda ((n i32)) i32 (c-abs n))))
```

`(disassemble c-abs)` も断られる。表示できるのは C 側の機械語で、それはこのコンパイラが
作ったものではない。`(compile c-abs)` は成功する（何もしない——既にコンパイル済みなので）。

**AOT（`compile-file`）でも動く。** C 関数そのものはリンカが解決する。`:library` を書いた宣言があれば、そのライブラリが `-l` としてリンク行に
足される（重複は1つにまとめられる）——`compile-file` に引数を足す必要は無い。ソースを読んで
いるのは compile-file 自身なので、宣言から集められる。

ビルド時にもシンボルを引く。存在しない関数を宣言していれば、リンクエラーより先に、名前を
名指ししたエラーになる。

標準ライブラリ（prelude）は `defffi` を使わない。標準ライブラリはどの実行ファイルにも丸ごと
入るので、そこに `:library` の付いた宣言があると、FFI を使わないプログラムまでそのライブラリを
リンクすることになるため。

#### def-c-struct と型付きポインタ —— C の構造体を確保する

```lisp
(unsafe
  (def-c-struct 名前 (フィールド 型)...)
  ...)
(unsafe (pub def-c-struct ...))
```

C と同じ配置の構造体を宣言する。トップレベルの `unsafe` の中にだけ書ける（その `unsafe` には
`def-c-struct` 以外を書けない）。名前の直後に docstring を置ける。

フィールドに書ける型は `i8` `i16` `i32` `u8` `u16` `u32` `c-long` `c-ulong` `f32` `f64` `bool`
`ptr`、型付きポインタ `(ptr T)`、それに別の `def-c-struct`（値として埋め込む）。配置（各
フィールドのオフセット、構造体のサイズと整列）は C の規則で計算する（LP64 を前提にする）。
自分自身を指すフィールドは書けるが、自分自身を埋め込むことはできない。

```lisp
(unsafe
  (def-c-struct point (x i32) (y f64))              ; x は 0、y は 8、サイズ 16
  (def-c-struct seg (a point) (b point) (next (ptr seg))))
```

`def-c-struct` の名前は型の名前空間に入る（同じモジュールに同名の `defstruct` などは置けない）
が、**値の型ではない**。`(defun f ((p point)) ...)` とは書けず、現れるのは型付きポインタの
指す先としてだけ。

**型付きポインタ `(ptr T)`** は、`T` を指すアドレス。`T` は上のフィールドに書ける型のどれか。
`ptr` と同じ生の機械語で、置ける場所の規則も同じ（引数・戻り型・局所変数だけ、`unsafe` の
中でだけ値にできる）。

確保と読み書きは次の形で書く。どれも `unsafe` の中でだけ使える。

| 形 | 意味 |
|---|---|
| `(c-alloc T)` / `(c-alloc T n)` | `T` を `n` 個（省略時 1 個）確保する。中身は 0 で埋まる。`(ptr T)` を返す |
| `(c-ref p i)` | `p` から `i` 個目の要素へのポインタ。確保した範囲の外ならエラー |
| `(c-deref p)` / `(setf (c-deref p) v)` | `p` の指すスカラを読む・書く |
| `p::field` / `(setf p::field v)` | 構造体のフィールドを読む・書く。埋め込んだ構造体のフィールドは、読むとそのアドレス（`(ptr 内側の型)`）になる |
| `(as ptr p)` | 型を忘れて `ptr` にする（`qsort` の `void *` などへ渡すため）。逆向きの変換は無い |

```lisp
(defun sum-x ((n int)) int
  (unsafe
    (let ((ps (c-alloc point n)))
      (dotimes (i n)
        (let ((p (c-ref ps i)))
          (setf p::x (as i32 i))))
      (let ((total 0))
        (dotimes (i n)
          (let ((p (c-ref ps i)))
            (setf total (+ total (as int p::x)))))
        total))))
```

**確保したメモリは、それを確保した `unsafe` を出ると解放される。** 持ち主になるのは、同じ関数の
中で字句的に一番外側の `unsafe`。正常に終わっても、`panic` や `throw`、`return-from` で抜けても
解放する。`lambda` と `labels` の関数は別の関数なので、`c-alloc` にはその中に自分の `unsafe` が
要る。

そのため、型付きポインタは確保した `unsafe` の外へ出せない。次はどれも型検査でエラーになる。

- `unsafe` 式の値にする（したがって関数から返すこともできない）
- クロージャ（`lambda`、`labels`）で捕捉する
- `task` / `thread` に渡す
- `throw` で投げる

`unsafe` の外で値を使いたいときは、`unsafe` の中で `defstruct` や数値へコピーしてから返す。

**C 側で確保したメモリは扱わない。** C から型付きポインタとして入ってくる値——`defffi` の
戻り値、コールバックの引数、ポインタ型フィールドを読んだ値——は、実行時に、生きている
`c-alloc` の確保の中の、その型の値の位置を指しているかを確かめ、違えばエラーにする。NULL も
エラー。C が確保したメモリや NULL を受けたいときは、型の無い `ptr` で受ける（中身は読めない）。

```lisp
(unsafe (def-c-struct item (key i32) (tag u8)))
(defffi (c-qsort "qsort") (ptr c-ulong c-ulong (fn ((ptr item) (ptr item)) i32)) ())

(unsafe
  (let ((xs (c-alloc item 4)))
    ...
    (c-qsort (as ptr xs) 4 8 (lambda ((a (ptr item)) (b (ptr item))) i32 (- a::key b::key)))
    ...))
```

コールバックの引数が検査で断られたときは、コールバックの中の失敗と同じく、C 関数が戻った
時点で呼び出し元へ伝わる。

### 3.4 defvar / defparameter / defconstant — グローバル変数

```lisp
(defvar (name Type) init-expr)        ; まだ束縛されていないときだけ初期化する
(defparameter (name Type) init-expr)  ; 毎回代入する
(defconstant (name Type) init-expr)

; docstring 付き（CL の defvar/defparameter/defconstant と同じ順序: 値の後ろ）
(defvar (name Type) init-expr "doc")
(defconstant (name Type) init-expr "doc")
```

**`defvar` と `defparameter` の違いは再ロードのとき**に出る（CL と同じ）。`defvar` は
そのグローバルが**すでに束縛されていれば初期化式を評価すらしない**ので、設定ファイルを
編集して読み直しても、セッションが変更した値はそのまま残る。`defparameter` は毎回
代入するので、読み直せば書かれたとおりの値に戻る。

型注釈は必須（初期化式から推論しない）。`defvar` は書き換え可能、`defconstant` は不可（`setf` でエラー）。

### 3.5 defmethod — メソッド定義

```lisp
; インスタンスメソッド: (m obj args...) の形で呼べる
(defmethod name ((self Type) (arg Type2) ...) RetType body...)

; static / 関連関数: (Type::name args...) の形で呼べる
(defmethod name (Type (arg Type2) ...) RetType body...)
```

呼び出し側は `obj` の静的型からメソッドを解決する（単一・静的ディスパッチ）。`defun` と同じ位置・
同じ規則で docstring を置ける（`where` 節の直後、本体の先頭、後ろに本体フォームが続く場合のみ）。
`impl` 内のメソッドも同様——`(documentation Type::method)` で取り出す。

### 3.6 defstruct — 構造体（ユーザ定義型）

```lisp
(defstruct Name
  (field1 Type1)
  (pub field2 Type2)
  ...)

; ジェネリック（山括弧で型パラメータ）
(defstruct Name<T1,T2...>
  (field TypeUsingT1)
  ...)
```

- 各フィールドは `(name type)` または `(pub name type)`（フィールド単位で公開設定、構造体自体の
  `pub` とは独立）。末尾にもう 1 つ式を書くとそのスロットの**デフォルト値**になる
  （`(x i32 0)`）——後述のオプションリスト参照。
- 自動的に次が生成される:
  - コンストラクタ `Name::new`（フィールド順に引数を渡す）
  - ゲッター `(field-name instance)`、糖衣構文 `instance::field-name`
  - セッター `(set-field-name instance value)`、糖衣構文 `(setf instance::field-name value)`
- 構造体自体を `pub` にするには `(pub defstruct ...)` のように先頭に `pub` を付ける。
- **型は名指すより前に定義する**。フィールドの型に自分自身は書ける（`(next Option<node>)`）が、
  後で定義する型は書けない——型には `defsignature` に当たる前方宣言が無い。まだ定義していない
  名前は、`defun` の引数型でも `the` でも同じく `unknown type` のエラーになる。したがって互いを
  参照し合う 2 つの型は書けない。
- **型変数は宣言部に書いたものだけ**。`defun`/`defstruct`/`defenum`/`deftype` は名前の `<T>`、
  `defmethod` は受け手の型（`(self box<T>)`、静的メソッドなら `box<T>`）、`impl` は対象の型と
  `impl<T>`、`deftrait` は `Self` と `(type Item)` の関連型。それ以外の場所——引数・戻り値・本体の
  `the`/`lambda`——に初めて現れる名前は型変数にはならず、`unknown type` になる。
- **docstring**: 名前の直後、フィールド列の前に文字列リテラルを置くと docstring になる
  （`(defstruct Name "doc" (field Type)...)` — CL の `defstruct` と同じ位置）。フィールドは常に
  `(name Type ...)` の形で裸の文字列にはなり得ないため曖昧性は無い。`(documentation Name)` で
  取り出す。

#### オプションリスト

名前の位置に `(Name option...)` とリストを書くとオプションを指定できる（CL と同じ位置）。

```lisp
(defstruct (point (:constructor make-point)          ; キーワードコンストラクタ
                  (:constructor at (x &optional y))  ; BOA コンストラクタ
                  (:copier copy-point))
  (x i32 0)          ; 第3要素はそのスロットのデフォルト値
  (y i32 0))

(point::make-point :y 7)   ; x は 0
(point::at 1)              ; y は 0
(point::at 1 2)
(copy-point p)             ; 浅いコピー（CL の copier と同じ）
```

- **`:constructor`** — 生成されるのは型の**静的関数**（`point::make-point`）で、本体は必ず
  `(point::new ...)`。`new` は構造上の唯一のコンストラクタのままで、ここで作るのはその
  *呼び方*。複数宣言できる。
  - `(:constructor name)` — 全スロットを `&key` で取る。**全スロットにデフォルトが要る**
    （CL の「未束縛スロット」に当たるものがこの言語には無いため）。
  - `(:constructor name (slot...))` — 名指したスロットを位置引数で取る（順序は自由）。
    名指さなかったスロットはそのスロットのデフォルトで埋まるので、**デフォルトが要る**。
    `&optional` を挟むとそれ以降は省略可（同じくデフォルトが要る）。
- **`:copier`** — 同じスロット値を持つ新しい値を返す**インスタンスメソッド**を生成する。
  CL の copier と同じく浅い。
- **`:include Parent`** — 親のスロット列を先頭に連結する（デフォルトも引き継ぐ。別ファイルの
  親でもよい）。**型の関係は作らない**——子は親の部分型ではなく、親のメソッドは子に適用
  されず、両者を結ぶ実行時テストも無い。この言語に部分型は無く、共通のインタフェースは
  `deftrait` が受け持つ。連結されるのはスロットの*一覧*だけ。
- **スロットのデフォルトは生成されるコンストラクタだけが読む**。`:constructor` を 1 つも
  宣言していないのにデフォルトを書くと、使われようが無いのでエラーになる。
- 入れないオプションと、その理由:
  - **`:conc-name`** — CL ではアクセサに接頭辞を付けて、1 つの平坦な関数名前空間での衝突を
    避けるためのもの。ここではアクセサは受け手の型でディスパッチするメソッドなので衝突が
    起きないうえ、接頭辞を付けると `instance::field`（スロット名しか知らない）が壊れる。
  - **`:predicate`** — 「この値は `point` か」を実行時に答えるもの。ここでは型は実行時の
    witness を持たないコンパイル時の分類で、「point かもしれない未知の型の値」が存在する
    位置も無い（`Sexpr` に対する `match` は封じてあり、`:dyn` はダウンキャストできない）
    ので、生成される述語は常に `true` しか返せない。
  - **`:type` / `:initial-offset` / `:named`** — 値の表現をリストやベクタに置き換える指定。
    表現はコンパイラのもので、言語からは観測できない。

### 3.7 defenum — 列挙型（直和型）

```lisp
(defenum Name
  (Variant1 Type1 Type2...)   ; ペイロード付きバリアント（位置フィールド）
  (Variant2)                  ; ペイロードなしバリアント
  ...)

; ジェネリック
(defenum Option<T>
  (Some T)
  (None))
```

- 各バリアントは `(VariantName FieldType...)` の形。フィールドは位置指定のみ（名前は持たない）。
  バリアントは1つ以上必要で、名前の重複は不可。
- 値の構築は組み込み `Option`/`Result` と同じく修飾または `use` 経由:
  `(Name::Variant1 a b)`、または `(use Name)` の後は `(Variant1 a b)`。
- `match` / `if-let` で分解できる。`match` は網羅性を検査する（全バリアントを尽くすか `_` が必要）:
  ```lisp
  (match opt
    ((Some v) v)
    ((None) 0))
  ```
- メソッド/関連関数は `defstruct` と同様に `defmethod`/`impl` で後付けする。
- 列挙型自体を `pub` にするには `(pub defenum ...)` と書く。
- **docstring**: `defstruct` と同じ位置・同じ規則——名前の直後、バリアント列の前
  （`(defenum Name "doc" (Variant ...)...)`）。`(documentation Name)` で取り出す。

### 3.8 deftype — 型別名

```lisp
(deftype meters i32)
(deftype fallible<T> Result<T,string>)
(deftype pred (fn (i32) bool))

(defun double ((m meters)) meters (* m 2))
(defun parse ((s string)) fallible<i32> ...)
```

CL の `deftype` を、静的型付けの言語で意味の通る範囲に絞ったもの——**型の綴りであって、
型ではない**。

- 名前の位置は `defun` と同じで、ジェネリック引数は `Name<T,U>` と書く。使用位置では
  宣言した個数どおりの型引数が要る（過不足はその場でエラー）。
- 展開は**型パーサの中**で起きる。したがって下流は誰も別名の存在を知らない——単型化の
  キーもダンプもコンパイル経路も、そして**エラーメッセージ**も、すべて展開後を見せる。
  `(f "x")` が `meters` を要求する関数で失敗すれば、メッセージには `i32` と出る。
- **新しい型ではない**。`(deftype meters i32)` は `meters` と `i32` を同じ型にするので、
  取り違えは何も捕まえない。区別したいなら `defstruct`。
- **述語にはならない**。CL の `(deftype small () '(integer 0 9))` は*値の集合*を表し
  `typep` が実行時に判定するが、ここでは型は実行時の witness を持たないコンパイル時の
  分類なので、値を制限する別名には制限する相手がいない。
- **自分自身を含められない**。別名は書かれた場所で展開されるので、再帰する先が無い。
  再帰的なデータ型は `defstruct`/`defenum` で書く。
- 名前空間は型・トレイトと共有する（同じモジュール内で `defstruct`/`defenum`/`deftrait`
  と同名にはできない）。`(pub deftype ...)` で公開、`(use m::meters)` で取り込める。
- **docstring**: 名前の直後、型の前（`(deftype Name "doc" Type)`）。

### 3.9 deftrait / impl — トレイト機構

```lisp
(deftrait TraitName (SuperTrait...)      ; 継承リストは必須。無ければ ()
  (type AssocName)                       ; 関連型（複数可、省略可）
  (method-name ((self Self) params...) RetType)          ; 本体なし＝実装必須
  (method-name ((self Self) params...) RetType body...)) ; 本体あり＝デフォルト実装

(impl TraitName TargetType
  (where (Trait A)...)                   ; impl 全体に効く境界（省略可）
  (type AssocName ConcreteType)          ; 関連型を具体化
  (method-name (recv params...) RetType body...))
```

`impl` によって各メソッドは `TargetType` の通常の `defmethod` として登録される。ジェネリック関数の
`where` 節でトレイト境界として参照する（[3.1 defun](#31-defun--関数定義) 参照）。トレイト名には `m::Trait` のような
`::` パスも書ける。

**継承リスト（必須）**: トレイト名の直後に必ず書く。要素は素のトレイト名か、そのトレイトが
関連型を持つ場合は `(Trait (Assoc Type))` の形で**全ての関連型をピン留めした**もの。

```lisp
(deftrait Eq () ...)                       ; 継承なし
(deftrait Ord (Eq) ...)                    ; Rust の trait Ord: Eq
(deftrait CharSource ((Iter (Item char)))  ; 関連型のピン留め
  (rewind ((self Self)) ()))
```

継承の効果は3つ。(1) `impl Ord X` は `impl Eq X` を**先に**書くことを要求する（記述順の規則。
REPL でも逐次 `load` でも決定的に判定できる唯一の形で、Rust より制限が強い）。
(2) `(where (Ord T))` だけで `Eq` のメソッドも呼べる。(3) `:dyn Ord` から `Eq` のメソッドを呼べ、
`:dyn Ord` の値をそのまま `:dyn Eq` を要求する場所に渡せる（アップキャスト）。
サブトレイトが親と同名のメソッドを再宣言することと、2つの親から同名のメソッドを継承することは
どちらもエラー（vtable のスロットは名前ごとに1つ）。ダイヤモンド継承は合流して1スロットになる。

**デフォルト実装**: シグネチャの後ろに本体を書くと、その `impl` が省略したときに使われる。
本体はトレイトを書いた**モジュールの名前空間**で解決されるので、そのモジュール内の非公開関数も
呼べる。本体を持つメソッドは `where` 節と docstring も書ける。
本体の型検査は**宣言の時点で1回**、`Self` を型変数のまま（`Self: そのトレイト` を境界として）
行う（Rust と同じ）——どの `impl` も省略しないデフォルトでも、どの実装型でも通らない誤りは
そこで弾かれる。`self` に対するそのトレイト自身・継承元のメソッド呼び出しはこの境界で通り、
関連型は自分自身に固定されるので `Item` を返すシグネチャと本体は具体型を知らないまま照合される。

**ブランケット実装**: 対象を型変数にすると、境界を満たす全ての型に一括で実装できる。

```lisp
(deftrait Clamp (Ord)
  (clamp ((self Self) (lo Self) (hi Self)) Self
    (if (less self lo) lo (if (less hi self) hi self))))
(impl<T> Clamp T (where (Ord T)))          ; 本体ゼロ — 全部デフォルト
```

コードは**具体型が実際に使うまで生成されない**（型ごとに1回、通常の単型化と同じ仕組み）。
1つのトレイトにブランケット実装は1つまで。同じ型に明示 `impl` があればそちらが優先される。
本体の型検査は生成とは別で、宣言の時点で1回**対象を型変数のまま**行う（Rust と同じ）——
一度も使われない実装でも、宣言した境界でどの対象にも通らない誤りはそこで弾かれる。
境界が正当化する呼び出し（`(where (Ord T))` 下の `(less self other)` 等）は、ジェネリック
`defun` の本体と同じ扱いで通る。

**docstring**: `deftrait` は継承リストの直後、アイテム列の前に文字列リテラルを置くとトレイト全体に1つ
docstring を持てる（`(deftrait Name () "doc" (type ...) (method ...)...)`）。本体を持たないシグネチャに
docstring は書けない——末尾の文字列はそれ自体がデフォルト実装の戻り値になるので、両者を区別できない。

標準ライブラリが提供するトレイト: **`Iter`**（`next`／関連型 `Item`。`doiter`／シーケンス関数の
基盤）・**`Eq`**（`equals`。`not-equals` はデフォルト実装）・**`Ord`**（`Eq` を継承。`less` のみ
実装必須で `less-equal`／`greater`／`greater-equal` はデフォルト実装）・**`Error`**（`message`／
`source`。エラー型を一様に扱うための `:dyn Error`）・**`print-object`**（型ごとの印字表現）・
**`Pathish`**（パス名指定子＝文字列 or `pathname`）・ストリーム階層 **`Stream`** →
**`InputStream`**／**`OutputStream`** → **`CharInput`**／**`CharOutput`** → **`PeekInput`**。
どの型がどのトレイトを実装しているかは [types.md](types.md)、各トレイトのメソッドは
[トレイト](functions/traits.md)・[エラー型](functions/option-result.md#3-エラー型と-error-トレイト)・
[print-object](functions/printing.md#5-print-object型ごとの印字表現)・[ストリーム](functions/streams-files.md)。
自前のコレクション型に `Iter` を `impl` すれば `doiter`（5 章）や `map`／`filter`／`sort` 等がそのまま使える。

トレイトの呼び出しは既定で**静的**（レシーバの静的型で解決）。実行時に具象型が決まる値を扱いたい
場合は trait オブジェクト型 `:dyn Trait`（2 章）を使うと vtable 経由の動的ディスパッチになる:

```lisp
(deftrait Drawable () (draw ((self Self)) string))
(defstruct circle (r i32))
(defstruct square (side i32))
(impl Drawable circle (draw ((self Self)) string "circle"))
(impl Drawable square (draw ((self Self)) string "square"))

(defun render-all ((xs Vector<:dyn Drawable>)) ()
  (doiter (d (iter xs)) (println "~a" (draw d))))   ; 1つの呼び出し地点、実装ごとの答え
```

`:dyn Trait` にできるのは「全メソッドが `self` レシーバを持ち、`Self` をレシーバ以外に使わず、
メソッド自身がジェネリックでも可変長でもない」トレイトだけ（継承したメソッドも同じ条件を満たす
必要がある）。

`:dyn` の箱に入れられるのは、値がヒープ上の表現を持つ型だけ:

| 入れられる | 入れられない |
|---|---|
| `defstruct` / `defenum` の型（`Vector<T>`、`cons-cell<A,B>`、`Result<T,E>`、標準ライブラリの構造体を含む）、`HashTable<K,V>`、`Sexpr`、`int`、`ratio`、`f64`、`string`、`random-state` | 固定幅の整数（`i8`〜`u32`）、`f32`、`bool`、`char`、`symbol`、`()`、関数型、箱を持たない `Option<T>`（[Option の実行時表現](functions/option-result.md#2-optiont-の実行時表現)） |

入れられない型の値を `:dyn` の位置に置くと型エラーになる。そうした値を `:dyn` で扱いたいときは、
`(defstruct flag (v bool))` のように構造体で包む。

### 3.10 module / use — 名前空間

```lisp
(module path body...)      ; path は foo または foo::bar のようなセグメント列
(in-module path)           ; 以降このユニットの末尾まで path の中（module の平たい形）
(use path...)              ; 関数・型・モジュールをカレント名前空間へエイリアス導入
(import path...)           ; use と同じ（CL 互換の綴り）
(shadowing-import path...) ; 既に埋まっている裸名を承知の上で取る use
```

- `module` は名前空間を作る。**型は名前空間ではない**（Rust と同様、型は関連関数/メソッドを持つのみ）。
- `use` で型を導入すると、その型のコンストラクタと公開 static メソッドも裸名で使えるようになる
  （例: `(use option)` の後は `some`/`none` を `option::some`/`option::none` なしで呼べる）。
- 裸名（修飾なしの識別子）の解決順序: 特殊形 → コンストラクタ → 自由関数（現在の名前空間 → ルート）
  → インスタンスメソッド（第一引数の静的型から解決）。中間の親モジュールへは遡らない。
- 修飾パス `a::b` は `a` を上記の順序で解決し、モジュールなら内部を辿り、型なら最終セグメントを
  関連項目として解決する。
- **`use` はそれより後のフォームに効く。** ファイルは 1 フォームずつ読まれ、依存も
  そのフォームを検査する直前に解決されるので、`(use m)` より**上**で `m::f` と書くと
  `unresolved path` になる。`use` はファイルの先頭に置く。
- **`use` は複数のパスを取れる**（`(use a::f b::g)`）。`import` は同じ動作の CL 互換の綴り。
- **裸名がすでに埋まっている `use` は報告される。** 裸名の解決はそのモジュール自身の定義を
  エイリアスより先に見るので、`(defun twice ...)` の後の `(use m::twice)` は**何もしない**。
  承知の上でやるなら `shadowing-import` と書く（ただし定義には勝てない——定義を取り消す
  手段は無い。勝てるのは先行するエイリアスに対してだけ）。
- **`in-module` は `(module path body...)` の平たい形**。`(in-module geometry)` と書くと
  以降そのユニット（ファイル、または囲む `module` の本体）の末尾まで `geometry` の中になる。
  ファイル自身のモジュールの**内側**に入る（`main.typl` なら `main::geometry`）。
  2 つ並べれば順に入れ子になる。CL の `in-package` とは別物で、名前も別にしてある——
  このシステムではファイルが既にモジュールなので「選ぶ」対象が無く、フォームにできるのは
  入れ子にすることだけだから。

### 3.11 ファイルとモジュールの対応（複数ファイルのプロジェクト）

ソースルートからの相対ファイルパスがそのままモジュールパスになる:
`<root>/geo/point.typl` の内容は暗黙にモジュール `geo::point` に包まれる
（ディレクトリも1セグメント、Rust/Python 方式）。ファイル内の明示 `(module bar ...)` は
その**内側**にネストする（`geo::point::bar`）——導出パスと明示宣言が衝突することはない。

- **ソースルート**: プロジェクトルートにマニフェストファイル `typelisp.toml` を置く
  （空でよい。任意で `src = "src"` の1行でソースディレクトリを指定）。対象ファイルの
  ディレクトリから上へ辿って発見される。マニフェストが無ければエントリファイルの
  ディレクトリ（REPL はカレントディレクトリ）がルート。
- **オンデマンドロード**: `(use geo::point)` がまだ読み込まれていないモジュールを参照すると、
  対応するファイル（`geo/point.typl`）が自動で読み込まれ、型チェックされて登録される。
  `use a::b::c` は `a/b/c.typl` → `a/b.typl` → `a.typl` の最長プレフィックス順で探す
  （`c` がモジュール内アイテムの可能性があるため）。他モジュールから見える定義には
  `pub` が必要（[3.13 pub](#313-pub--公開指定)）。
- **循環参照はエラー**: `circular module dependency: a -> b -> a` の形で連鎖が報告される。
- **実行**: `typl <file.typl>` でファイルを実行できる（引数なしなら REPL）。REPL の `use` も
  同じ規約でファイルを解決する。
- **cons アリーナ容量**: `typl --heap-cells N` で cons セルのアリーナ**初期容量**を指定できる
  （既定 65536。`--heap-cells=N` 形も可、ファイル実行/REPL 共通）。アリーナは足りなくなれば
  **追加して伸びる**。伸びる上限は初期容量の 256 倍で、そこを超えた確保が `heap exhausted` に
  なる——つまり初期容量は「最初にこれだけ確保する」、上限は「ここを越えたらリークとみなす」という
  意味。

### 3.12 load — フラットロード

```lisp
(load "path")   ; トップレベル専用。path は文字列リテラル
```

- CL 流の**フラットロード**: 対象ファイルのフォームを**カレント名前空間**にそのまま読み込む
  （`use` のようにモジュールで包まない）。トップレベル専用（関数本体内は型エラー）。
- `path` は読み込み元ファイルのディレクトリからの相対（REPL からならプロセスの cwd）。
  拡張子が無ければ `.typl` を補う。
- 読み込んだファイル自身の `(load ...)`/`(use ...)` も再帰的に処理される。
- **1 フォームずつ読んで、その場で実行する**（CL の `load` と同じ）。フォーム *k* は
  *k+1* が読まれる前に走り終わっている——途中に構文エラーや型エラーがあっても、
  その手前のフォームは実行済みになる。`use` で読み込むモジュールファイルはこれと違い、
  1 単位として検査され、実行は `use` した側に任される（CL の `compile-file` に相当）。

### 3.13 pub — 公開指定

```lisp
(pub defun ...)
(pub defsignature ...)
(pub defffi ...)
(pub defvar ...)
(pub defparameter ...)
(pub defconstant ...)
(pub defmacro ...)
(pub defmethod ...)
(pub defstruct ...)
(pub defenum ...)
(pub deftype ...)
```

`pub` が付けられるのは上記11種類のみ（`module`/`use`/`deftrait`/`impl` には付けられない）。
定義形を括弧で包む `(pub (defun ...))` 形式ではなく、`pub` の直後に定義キーワードを続ける。
1つの `pub` が公開指定できる定義は1つだけ（複数の定義の一括指定はできない）。

### 3.14 defmacro — マクロ定義

```lisp
(defmacro name (必須... &optional opt... &rest rest-name &key key...) body...)
```

- 全パラメータ・戻り値は常に `Sexpr` 固定なので型注釈は書かない。
- CL 流の非衛生的マクロ（`gensym` で衝突を避けるのはマクロ作者の責任）。
- ラムダリストは CL 流に `必須 &optional &rest &key` の順（各マーカーは高々1回、この順序でのみ）。
  - `&optional` … 省略可能引数。`name` または `(name デフォルト式)`。デフォルト式は展開時に評価され
    （先に束縛済みのパラメータを参照できる）、省略時に束縛される（デフォルトを書かなければ空リスト `()`）。
  - `&rest name` … 残りの位置引数を1つの `Sexpr` リストとしてまとめて受け取る。
  - `&key` … キーワード引数。`name` または `(name デフォルト式)`。呼び出し側は `:name 値` で渡す
    （順不同）。省略時はデフォルト式（無ければ空リスト `()`）。未知のキーワードや奇数個の `:key` 列はエラー。
- 例: `(defmacro pair (x &optional (y 1)) ...)` / `(defmacro make (&key (a 0) (b 9)) ...)`。

### 3.15 macrolet / symbol-macrolet — 局所的なマクロ束縛

```lisp
(macrolet ((name (ラムダリスト) body...) ...) body...)   ; 字句スコープのマクロ
(symbol-macrolet ((name 展開形) ...) body...)            ; 名前が形を表す
```

どちらも**式**の特殊形で、実行時には何も残らない（本体がコンパイルされるのは展開後の形）。
ラムダリストは `defmacro` と同じ。詳しい規則と例は
[局所的なマクロ束縛](functions/system.md#9-局所的なマクロ束縛macrolet--symbol-macrolet)。

## 4. 束縛・条件分岐

```lisp
(let ((name val) ...) body...)      ; 並行束縛
(let* ((name val) ...) body...)     ; 逐次束縛（先の束縛を後の初期化式で使える）

(if cond then else)                 ; else は必須（3要素固定）
(when cond body...)                 ; else なしの if（Unit 型）。defmacro
(unless cond body...)               ; when の否定版。defmacro
(cond (test1 body...) (test2 body...) ... (else body...))   ; defmacro
(case expr
  (key1 body...)
  ((key2 key3) body...)             ; キー列: どれかに当たれば
  (else body...))                   ; expr は一度だけ評価。key は equal で比較。
                                     ; key は「リテラル」で、評価されない（CL と同じ）。
                                     ; 裸のシンボル a はシンボル 'a を意味する。
                                     ; 'a と書くとエラー（裸の a を使う）。defmacro
(ecase expr (key body...) ...)      ; 網羅を要求する case。どれにも当たらなければ panic。defmacro
(ccase expr (key body...) ...)      ; CL の ccase。差し出せる restart が無いので ecase と同一。defmacro
(and expr...)                       ; 短絡評価。0引数なら true。defmacro
(or expr...)                        ; 短絡評価。0引数なら false。defmacro
(progn body...)                     ; 順次実行、最後の値を返す
(unsafe body...)                    ; progn と同じ。加えて FFI 呼び出しと生の語を
                                     ; 書く許可を与える。3.3 defffi を参照
(prog1 form more...)                ; 全部評価し、値は form のもの。defmacro
(prog2 a b more...)                 ; 全部評価し、値は b のもの。defmacro
(the Type expr)                     ; 型注釈（実行時の効果なし）
```

### 4.1 unsafe — 検査できない前提を引き受ける

```lisp
(unsafe body...)
```

`progn` と同じ——本体を順に評価し、最後の値を返す。スコープも作らず、関数の境界でもない
（`break` / `return-from` は素通りして外へ抜ける）。違うのは、この中でだけ書けるものがある点。

いま `unsafe` を要求するのは 3 つ。[defffi](#33-defffi--c-関数の宣言ffi) で宣言した C 関数の
呼び出し、生の機械語（`ptr` / `c-long` / `c-ulong` / `(ptr T)`）を値にすること、それに
[`def-c-struct` と `c-alloc`](#def-c-struct-と型付きポインタ--c-の構造体を確保する)。

`c-alloc` で確保したメモリは、同じ関数の中で一番外側の `unsafe` を出るときに解放される。
その `unsafe` だけは `progn` と違い、出るときに解放する処理を持つ。

`unsafe` が引き受けるのは、コンパイラが確かめられない次の前提:

- **型の一致**。宣言した C シグネチャが本物と合っていること。合っていなければ、引数は間違った
  レジスタに載り、戻り値は間違った幅で読まれる。
- **メモリ安全**。C 側が渡されたものをどう扱うか。
- **プロセス大域の状態**。環境変数・シグナルハンドラ・`errno`。たとえば `setenv` を FFI で
  呼ぶと、この処理系の `decode-universal-time` が地方時を求めるときの前提が崩れる。
- **スレッド安全**。

型検査からの逃げ道ではない。`(unsafe (+ 1 "two"))` は通らない。許されるのは特定の**操作**を
書くことであって、でたらめを書くことではない。

字句的に働く。`unsafe` の中に書いた `lambda` の本体はその許可を継承する（Rust の `unsafe`
ブロック内のクロージャと同じ）——その値が後で `unsafe` の外から呼ばれることはありうるが、
そこに書いたこと自体が責任を引き受ける行為だとみなす。

### 4.2 destructuring-bind — リストを形で分解する

```lisp
(destructuring-bind ラムダリスト form body...)
```

`form` が作るリストを**形**で分解して束縛する。ラムダリストは `defmacro` のもの
（必須 → `&optional` → `&rest`/`&body` → `&key`、それぞれデフォルト式つき）で、CL が両者で
1 つを共有しているのと同じ理由——同じものを分解する 2 つの形だから。

```lisp
(destructuring-bind (op a b) (quote (+ 1 2)) (format false "~a ~a ~a" op a b))  ; "+ 1 2"
(destructuring-bind (head &rest tail) xs (format false "~a | ~a" head tail))
(destructuring-bind (a &optional (b 9)) (quote (1)) b)                          ; 9
(destructuring-bind (&key (x 0) y) (quote (:y 7)) (format false "~a ~a" x y))   ; "0 7"
```

- **束縛される変数はすべて `Option<Sexpr>`**。実装の制約ではなく、束縛する対象の性質:
  S 式リストがこの言語で唯一のリストなので、要素に与えられる他の型が無い。スカラが要る
  ところで `match` に落とすのは `defmacro` の本体と同じ。
- **形が合わなければ panic**（CL のエラーに当たる）。要素が足りない・多い、`&key` の並びが
  奇数個、知らないキーワード、のいずれも。`sexpr-car` は `()` に対して `()` を返す寛容な
  関数なので、チェックを書かなければ短いリストが黙って空の並びに束縛される。
- **入れ子のラムダリストは非対応**。`defmacro` も取らないので、規則は 1 つに保つ。
  `(a (b c))` は黙って `b` にサブリストを束縛したりせず、その旨のエラーになる。
- `&optional` / `&key` のデフォルト式は**使うときだけ評価**される（CL と同じ）。
- CL の `&allow-other-keys` に当たるものは無い（`defmacro` にも無い）。

### 4.3 match — パターンマッチ

```lisp
(match expr
  (pattern body...)
  ...)
```

パターンの種類:
- `_` — ワイルドカード
- 変数名 — 束縛パターン（常にマッチ）。ただしスクルーティニーの型がその名前の変種を持つ場合は
  **下の裸変種名パターン**として解決される
- 裸の変種名 — 引数を取らない変種にマッチ（`(match c (red 1) (blue 2))`）。フィールドを持つ変種を
  裸名で書くとアリティエラーになるので、`(circle r)` のように括弧で書く
- **即値リテラル**: 整数 / `true`/`false` / 文字 — 語の比較
- **値リテラル**: 文字列 / 浮動小数点 / シンボル（`'foo`）/ 多倍長の整数 / ratio — その型の
  `Eq::equals`（[トレイト](functions/traits.md#2-eq--ord比較)）による値比較。文字列は内容比較であって同一性比較ではない
- `(= expr)` — 任意の式を評価し、`Eq::equals` で比較する。リテラル構文を持たない型
  （`defstruct` インスタンス、グローバル、計算結果）を比較する唯一の書き方であり、
  ユーザ定義の `Eq` 実装がそのまま比較規則になる。`expr` はその腕の位置から見えるものを何でも
  参照できる（引数、外側の束縛、グローバル）
- `(Ctor sub-pattern...)` — コンストラクタパターン（`Some x` `None` `Cons a d` `Ok v` など）

`Eq` を実装しない型を値リテラル／`(= expr)` で比較しようとすると型エラーになる
（マッチしない腕が黙って残るより、比較できないと言うほうを選んでいる）。

**`Sexpr` スクルーティニーに対する値リテラル**: `sexpr` の `Eq` は `eq`（CL の同一性）なので、
即値——`'foo`（intern 済み）/ 整数 / 文字 / `true`/`false`——はそのまま書けて内容どおりにマッチする:

```lisp
(match s ('add 1) (42 2) (#\a 3) (_ 0))
```

即値でないリテラル（文字列 / 浮動小数点 / 多倍長の整数 / ratio）は `Sexpr` に対して**書けない**。
それらの `eq` はオブジェクトの同一性を比べるので「型は通るが決してマッチしない腕」になるため、
変種パターンを名指すエラーにしてある——`(str "hi")` と書けば `string` に分解されて内容比較になる。
`(= expr)` は明示的に `equals` を求めているので、この制限は掛からない。

**スクルーティニーは ADT でなくてよい**。`string`/`symbol`/`i32`/`f64` などをそのまま `match`
できる（それが文字列リテラルパターンの置き場所になる）。ただし変種を持たない型は列挙で網羅できない
ので、`_`（またはワイルドカードとして働く束縛パターン）が必須:

```lisp
(defun kind ((s string)) i32
  (match s
    ("add" 1)
    ("sub" 2)
    (_     0)))          ; 変種の無い型なので `_` が要る
```

`Sexpr` スクルーティニーに対しては、上記の組み込み16変種パターンに加えて **downcast パターン**
（ユーザ定義 ADT インスタンスの取り出し）が書ける — `(list p 42)` のように `Sexpr` へ暗黙変換された
`defstruct`/`defenum`（3 章）のインスタンスを `match` で取り戻す構文:

- `(TypeName sub-pattern...)` — **型名**を先頭に置くフィールド分解（struct 専用、`defstruct` は変種が
  常に1つなので変種名でなく型名で書く）。例: `(defstruct point (x f64) (y f64))` に対し `(point x y)`。
- 裸の変種名 `(VariantName sub-pattern...)` — `defenum` の変種抽出。`(use EnumType)` 済みで可視な
  裸の名前として解決される（構成子を呼ぶときと同じ可視性規則）。例: `(defenum color (red) (blue))` の
  `(use color)` 後に `(red)` `(blue)`。可視な複数 enum で変種名が衝突する場合は曖昧エラーになるため、
  修飾形 `(EnumType::VariantName ...)` でも書ける（`use` 不要）。
- `(the Type pattern)` — 型全体でのdowncast（丸ごと束縛）。フィールド分解せず、値をそのまま
  `pattern` へ渡す。可変な struct の同一性を保ったまま取り出せる唯一の書き方であり、`Vector<T>`/
  `HashTable<K,V>` を `Sexpr` から取り出す唯一の手段でもある（両者はフィールド分解形を持たない）。
  例: `(the point p)` の後で `(setf p::x 9)` すればリスト内の元インスタンスにも反映される。

**`Option<Sexpr>` のパターン**: S 式データの型は `Sexpr` ではなく `Option<Sexpr>` で、空リストは
`Sexpr` の変種ではなく `Option` の `none` である。そのため `Option<Sexpr>` を `match` するときは、
`Sexpr` の 16 変種と `none` を**同じ腕の並びに平らに**書ける（`Option` を剥がす外側の `match` は
要らない）:

```lisp
(defun tag ((s Option<Sexpr>)) i32
  (match s
    ((int _)    1)
    ((cons _ _) 2)
    ((str _)    3)
    ((none)     0)          ; 空リスト
    (_          9)))
```

網羅性も同じ平らな宇宙——`Sexpr` の 16 変種 ＋ `none` の 17 個——で検査する。`(none)` を
書き忘れれば `_` が無いかぎりエラーになる。`(some x)` も書けて「空でない何か」を束縛する。

この糖衣は `Option<Sexpr>` **ちょうど**にしか掛からない。`Option<Option<Sexpr>>` では
`(int n)` がどちらの層を剥がしたのか決まらないので、通常どおり 2 段の `match` を書く。

**trait オブジェクト（`:dyn Trait`、2 章）のスクルーティニー**にも同じ downcast パターンがそのまま
使える——`match` は箱を外してから上の `Sexpr` パターン機構に渡すので、追加の構文はない。実装型の
集合は開いているので網羅にはならず、`_` が必須:

```lisp
(defun area ((d :dyn Drawable)) i32
  (match d
    ((circle r) (* (* r r) 3))     ; 型名先頭のフィールド分解
    ((the square s) (* s::side s::side))
    (_ 0)))
```

**腕どうしの型推論**: 全ての腕は同じ型でなければならない（`panic` 等で発散する腕は除く）。
期待型が無い位置に書かれた `match` では、腕が**互いに**足りない型引数を埋め合う——
`(result::ok v)` は `T` だけ、`(result::err e)` は `E` だけを決めるが、両方を並べれば
`Result<T,E>` が決まる。片方の腕だけでは決まらない型引数が最後まで残った場合は、
その腕自身のエラー（`cannot infer type argument ...`）になる。`match` の外では、
決まらない型引数はその場でエラー。

downcast パターンを使う `match` の網羅性チェックは、`Sexpr` 本来の変種のカバレッジには数えない
（downcast パターンだけを並べた `match` は `_` で閉じる必要がある）。ジェネリックな ADT
（`defstruct point<T> ...` など）は downcast パターンの型引数を推論できないため、フィールド分解形
（`(point ...)`)/裸変種形は使えず、`(the point<i32> p)` のように `the` で明示する。

**downcast は実体化まで見る。** 明示した型引数は照合に使われる——`(the point<i32> p)` は
`point<i32>` の値だけを通し、`point<string>` は素通りして次の腕へ行く。値が自分の型引数まで
含めた型を覚えているため（`print-object` の選択と同じ仕組み）。

```lisp
(if-let (pattern val) then els)     ; val が pattern にマッチすれば then（束縛あり）、失敗なら els。defmacro
(while-let (pattern val) body...)   ; val（毎回再評価される）が pattern にマッチする間ループ。defmacro
```

## 5. 反復

```lisp
(loop body...)                      ; 無限ループ。break/return で脱出
(while test body...)                ; test が真の間ループ。defmacro
(until test body...)                ; test が偽の間ループ（while の否定版）。defmacro
(dotimes (var count-expr) body...)  ; count-expr を一度評価し、var を 0..count-1 で回す。defmacro
(do ((var init step) ...)
    (test result...)
  body...)                          ; CL 流の並行ステップ反復。defmacro
(do* ((var init step) ...)
     (test result...)
  body...)                          ; do の逐次版（let* 束縛・順に代入）。defmacro
(doiter (var coll-expr) body...)    ; Iter トレイトを実装する値を反復。defmacro

(break)                             ; 直近のループのみを抜ける。値は常に Unit
(return)                            ; 直近のループのみを抜ける
(return value)                      ; 値を伴って直近のループを抜ける
```

`break`/`return` はどちらも **直近の囲むループのみ** を脱出する（関数の早期リターンではない。
`lambda` の境界は越えられない）。`loop` の型は内部で見つかった `break`/`return` の値型の合流型
（一度も脱出しなければ `!`）。関数から抜けたいときは次の `return-from` を使う。

### 5.1 `block` / `return-from` — 名前付きの脱出

```lisp
(block name body...)                ; 名前付きの脱出先。値は最後のフォーム、
                                    ; または return-from が渡した値
(return-from name)                  ; その block を Unit で抜ける
(return-from name value)            ; 値を伴って抜ける
```

**`defun` / `defmethod` / `labels` の各関数は、自分の名前の block を暗黙に張る**（CL と同じ）。
だから `(return-from f v)` が関数の早期リターンになる:

```lisp
(defun first-even ((a i32) (b i32)) i32
  (if (= (mod a 2) 0) (return-from first-even a) ())
  (if (= (mod b 2) 0) (return-from first-even b) ())
  -1)
```

`block` は**字句的**な脱出で、名前は**書かれた場所で解決される**——チェッカーが
`return-from` を囲む `block` に対応づけ、その値の型をブロックの脱出型に合流させる。したがって:

- 対応する `block` が無い `return-from` は**型エラー**（実行時エラーではない）。
- 値の型が他の脱出や本体の型と合わなければ**型エラー**（`match` の腕と同じ規則）。
- 同名の `block` が入れ子なら**内側が勝つ**（CL の遮蔽規則）。
- **関数の境界は越えられない**。`lambda` の中から外の `block` へは抜けられない
  （`lambda` はブロックを張らない——CL の暗黙ブロックは*名前*を要求し、無名関数には無い）。
  越える必要があるものは `catch`/`throw`（8 章、こちらは**動的**）。

`break`/`return`（5 章）と同じ**静的**な脱出なので、コンパイル済みコードでは
コンパイル時に決まっている基本ブロックへの分岐になる。途中に `unwind-protect` があれば
その `cleanup` は走る（8 章）。

`return-from` を一度も書かなければ、暗黙の block は何のコストも持たない。

### 5.2 拡張 `loop`（CL の LOOP 構文）

`loop` の**第 1 要素がキーワードなら**節の並びとして読む。そうでなければ上の単純ループの
ままで、既に書かれている `loop` の意味は変わらない（CL 自身の simple loop 規則と同じ）。

CL は節の語を裸のシンボルで書くが（`(loop for i from 1 to 3 collect i)`）、ここでは
**すべてキーワード**にする——裸の `for` はただの変数参照になってしまうし、キーワードで
あることが単純ループとの分かれ目でもある。例外は変数と値を区切る `=` で、位置が
一意なので裸でもキーワード（`:=`）でも読む。

```lisp
(loop :for i :from 1 :to 3 :collect i)              ; #<vector<int> 1 2 3>
(loop :for x :in (iter v) :when (evenp x) :sum x)
(loop :repeat 4 :for x = 1 :then (* x 2) :collect x) ; #<vector<int> 1 2 4 8>
(loop :for i :from 1 :to 4 :sum i :into s :finally (return (* s 2))) ; 20
```

**変数節**（本体節より前に書く。CL の規則で、後ろに書くと「そこから先だけ回る」と
読めてしまうためエラーにする）:

| 節 | 意味 |
|---|---|
| `:with v = e` | 一度だけ束縛する。前の節の変数を読んでよい |
| `:for v :in s` / `:for v :across s` | `Iter` の要素を順に。CL のリスト/ベクタの区別はここには無いので同じ節の別綴り |
| `:for v :on s` | 以降の**接尾辞**を順に。CL は共有される tail cons を渡すが、`Iter` に共有すべき tail は無いので新しい `Vector` |
| `:for v :from a [:to b \| :below b \| :downto b \| :above b] [:by s]` | 数え上げ。`:downfrom`/`:upfrom` も可 |
| `:for v = e [:then f]` | `e` で始め、2 回目以降は `f`（`:then` 無しなら毎回 `e`） |
| `:repeat n` | 回数だけ回す |

`:for` を複数書くと**並行に**進み、どれか 1 つが尽きた時点で終わる。

**本体節**（書いた順に毎回実行）:

| 節 | 意味 |
|---|---|
| `:do form...` | 副作用のため |
| `:collect e [:into v]` | `Vector<T>` に集める |
| `:append e [:into v]` | `Iter` の中身を継ぎ足す |
| `:sum e` / `:count e` | 合計 / 真だった回数 |
| `:maximize e` / `:minimize e` | 最大 / 最小。**`Option<T>`**（CL が空列に nil を返すのと同じ。任意の `Ord` 型に最小元は無い） |
| `:always e` / `:never e` | 全部満たせば `true`、破れたら即 `false` |
| `:thereis e` | `e` は **`Option<T>`**。最初の `some` を返し、無ければ `none`（CL の「最初の非 nil 値」に当たるのがこれ。`bool` を試すなら `:always`/`:never`） |
| `:while e` / `:until e` | ここで**正常終了**する（`:finally` は走り、集めたものが答え） |
| `:when e clause` / `:unless e clause` / `:if e clause [:else clause]` | 節 1 つを条件付きにする |
| `:return e` | 即座にその値で脱出（`:finally` は走らない。CL と同じ） |
| `:initially form...` / `:finally form...` | ループの前 / 正常終了時 |

**`:named name`**（他のどの節よりも先に、1 つだけ）はループ全体を `(block name …)` で囲む。
`(return-from name e)` が入れ子のループの中からでも一気に脱出でき、`:return` と同じく
`:finally` は走らない。名前を付けなければ block も張らない——CL の無名 `loop` は `block nil`
を張るが、ここに `nil` は無く、`break`/`return`（5 章）が既に「直近のループを抜ける」を持って
いる。

```lisp
(loop :named outer :for i :from 1 :to 3
  :do (loop :for j :from 1 :to 3 :do (if (= (* i j) 4) (return-from outer (* 100 i)) ()))
  :finally (return 0))                                  ; 200
```

`:finally (return 0)` を省くと**型エラー**になる。`block` の規則がそのまま効くだけで
（5.1）、脱出の型 `int` と、尽きたときにループが残す `()` が合わない。

**ループの値**は、集約節があればその蓄積（複数あれば最初のもの）、`:always`/`:never` なら
`true`、`:thereis` なら `none`、どれも無ければ `()`。`:finally` の最後が `(return e)` なら
それが値になる——CL の `finally (return …)` の慣用で、集約しないループが自分の答えを
名乗る唯一の方法。

**CL との違い / 入っていないもの**:

- **節の語はキーワード**（上記）。
- `:maximize`/`:minimize`/`:thereis` は `Option<T>` を返す（nil が無いため）。
- **`:return` だけ書いて集約も `:finally` も無いのはエラー**。CL は尽きたとき nil を返すが、
  ここにはそれが無いので「尽きたときの値」をループが言う必要がある。
- `:and` による並行節の連結、`:being`/ハッシュ表の専用反復、`:it`、`:nconc` は入っていない。
- `:collect` の要素型は集約式の型から決まる。関数型のように**型名として書き表せない型**を
  集めようとするとその旨のエラーになる。

## 6. 関数値・呼び出し

```lisp
(lambda (params) RetType body...)   ; 第一級関数値（クロージャ）を作る
(labels ((name (params) RetType body...) ...) body...)   ; 相互再帰可能なローカル関数定義
(apply f arg1 ... argN rest-list)   ; f（&rest を持つ可変長関数）を rest-list を展開して呼ぶ
```

名前付き関数もそのまま値として渡せる（高階関数への引数など）。

## 7. その他の特殊形

```lisp
(setq var value ...)                ; CL の変数代入。(setf var value) を順に並べるだけ。defmacro
(psetq var value ...)               ; 並行代入。全ての値を先に評価してから代入する。defmacro
(psetf place value ...)             ; psetq を place へ一般化したもの（同じ展開）。defmacro
(setf place value)                  ; place への代入。place は 変数名 / var::field /
                                     ; (accessor recv key...) 形の呼び出し形。recv の静的な
                                     ; 型が set-{accessor} というインスタンスメソッドを
                                     ; 持てば成立（Vector<T>・HashTable<K,V> の get は
                                     ; 例外的に set が対応、それ以外は set-アクセサ名）。
                                     ; 値は代入した値（CL と同じ）。したがって
                                     ; (if c (setf x 1) ()) は then と else の型が合わない
(incf place)  (incf place delta)    ; place += delta（省略時 delta=1）。結果は setf 同様
(decf place)  (decf place delta)    ; place -= delta（省略時 delta=1）
(rotatef place1 place2 ... placeN)  ; N個の place を巡回シフト（新place1=旧place2, ...,
                                     ; 新placeN=旧place1）。各 place の部分式は1回だけ評価
(shiftf place1 ... placeN newvalue) ; place2..N の値を左へシフトし、newvalue を placeN へ。
                                     ; 戻り値は旧 place1 の値
(list e1 e2 ... en)                 ; (cons e1 (cons e2 (... ()))) への展開。0引数なら ()
                                     ; 各要素は Sexpr へ暗黙変換される（CL の cons 同様、任意の値を
                                     ; 保持できる）。スカラ（int/i32/f64/ratio/char/bool/string/
                                     ; symbol）は対応する Sexpr の変種に包まれ、defstruct/defenum/
                                     ; Vector<T>/HashTable<K,V> などはそのまま入る（変換の
                                     ; コストは無い）。&rest/format の引数も同様。
(source-file)                       ; このフォームが読まれたファイル名（string）。チェック時に
                                     ; 定数として決まる。CL の *load-pathname* に当たるが変数では
                                     ; ない——モジュールの本体は検査の後に実行されるので「いま
                                     ; ロード中」は当てにならず、検査の時点なら常に分かっている。
                                     ; ファイルでないソースはリーダの呼び名（<stdin>/<input>）
(quote datum)                       ; 'datum と同義。評価せず Sexpr データとして返す
(quasiquote template)               ; `template と同義。,/,@ でテンプレート内に式を埋め込む
(documentation name)                ; name（裸名または Type::method）の docstring を Option<string> で返す
(panic message)                     ; message: string。回復不能なエラーで異常終了。型は !
(unreachable)                       ; (panic "unreachable") に展開。defmacro
(todo)                              ; (panic "todo") に展開。defmacro
(as Type expr)                      ; 数値/文字の型変換。失敗しうる変換は失敗時に panic
(try-as Type expr)                  ; as と同じだが結果を Option<Type> で返す（失敗は None）
(print control args...)             ; 書式展開して標準出力へ（改行なし）
(println control args...)           ; 同上（末尾に改行）
(format dest control args...)       ; CL の format。展開結果の string を返す
(pprint x)                          ; pretty printer で整形出力。CL 準拠で先頭に改行を出す
(pprint-fill x)                     ; 語詰めレイアウト
(pprint-linear x)                   ; 全部1行か1要素1行か
(pprint-tabular x [colinc])         ; 表形式レイアウト（既定 16 桁）
(pprint-logical-block (obj :prefix p :suffix s) body...)  ; 論理ブロックを自分で組む
```

`print`/`println`/`format`/`pprint` 系は特殊形なので、可変長引数（`pprint` 系は1つの対象）は
各自の型のまま `Sexpr` へ包まれて渡る——`(println "~a" my-struct)` がそのまま動くのはこのため。
書式ディレクティブと pretty printer の詳細は [書式ディレクティブ](functions/format.md) と [印字](functions/printing.md#4-pretty-printer)。

`as`/`try-as` が扱えるのは数値・文字カタログのみ（`int`・固定幅整数型・`f32`/`f64`/`ratio`/`char`
間）。同一型は無変換。**整数の幅どうし（`int` を含む）・`f32`↔`f64` は本物の変換**——`as` は
切り詰め／丸め、`try-as` はその幅（精度）に入るかどうかを答える。`(as int x)` は固定幅からの正確な
拡大、`(as i32 n)` は `int` からの切り詰め。整数→`char` は範囲外で失敗しうるので `as` は panic・
`try-as` は `None`。それ以外（拡大変換や `float->int`/`ratio->int` の切り捨て）は常に成功する。
`float->int`/`ratio->int`/`char->int` は `int` に着地し、より狭い幅を頼まれれば `int->W` を
続けて呼ぶ。対応する変換メソッド（[数値](functions/numbers.md)の `int->char`/`int->int`/`int->W` 等）へ
展開される糖衣構文。

`documentation` は `quote`/`compile` と同様、`name` を評価せず未評価の裸シンボル/`::`パスとして
読む特殊形。CL の `(documentation 'name 'function)` と違い型引数は取らない——`name` を変数→関数→型
→トレイト→マクロの順（裸識別子を式として評価するときと同じ優先順位）で解決し、見つかった定義の
docstring を返す（`(documentation Type::method)` はメソッド専用）。解決自体に失敗する（そんな名前の
定義が無い）のはチェック時のエラー、定義はあるが docstring が無い場合は `Option::none`。すべて
チェック時に定数として決まる——実行時のルックアップは発生しない。モジュール修飾された自由名
（`mod::name`、`Type::method` を除く）は非対応。

## 8. 非局所脱出（catch / throw / unwind-protect）

```lisp
(catch 'tag body)                   ; body を走らせる。body が届く範囲のどこかで
                                    ; (throw 'tag v) が起きたら、その v を値にする
(throw 'tag value)                  ; 直近の動的に囲む (catch 'tag ...) へ脱出する
(unwind-protect protected cleanup)  ; protected をどう抜けても cleanup を走らせる
```

`break`/`return`（5 章）と違い、これは**動的**な脱出——`throw` は自分を囲む `catch` を字句的に
見ておらず、関数を何段跨いでも同じタグの `catch` に届く。

```lisp
(defun find-first ((xs Option<Sexpr>)) int
  (catch 'found
    (progn
      (dolist (x xs)
        (match x ((int n) (if (> n 10) (throw 'found n) ())) (_ ())))
      -1)))                         ; 見つからなければ通常どおり末尾の値
```

- **タグはリテラルシンボルのみ**（`'done`）。CL と違い評価されない。
- **タグが型を運ぶ。** `'tag` が最初に使われたときに型が決まり、以降の同じシンボルの
  `throw`/`catch` は全部それに突き合わされる。別の型で使うと型エラー。
- `throw` の型は `!`（発散）。`(catch 'tag expr)` の型は `expr` の型とタグの型の合流型。
- `unwind-protect` の値は `protected` の値。`cleanup` の値は捨てられる。
  `cleanup` は `protected` をどう抜けても走る——正常終了・`throw`・`panic` に加えて、
  `break`/`return`/`return-from` で抜けた場合も走る。`cleanup` 自身の非局所脱出は、
  飛行中の脱出に勝つ。
- 入れ子の `unwind-protect` は内側から順に走る。`protected` の**内側**のループを抜ける
  `break` は `protected` から出ていないので、その `cleanup` は走らない。

CL のコンディション（`define-condition`/`handler-bind`/`invoke-restart`）は採用していない。
静的型付けと合わないため、回復できる失敗は `Result` で表す（9 章）。

## 9. エラー処理の方針

- 回復可能な失敗は `Result<T,E>` + `match`。回復不能な失敗（バグ・不変条件違反）は `panic`。
- `?`/try に相当する構文はない。分岐は `match` で明示する。
- 関数・特殊形の名前に `!`（破壊的操作）や `?`（述語）を接尾辞として使わない。述語は
  `-p`/`p` 接尾辞（`zerop` `consp` など）または `is-` 前置（`is-some` `is-ok` など）で命名する。

## 10. コンパイル

```lisp
(compile name)                      ; 定義済みの defun/メソッドをネイティブコードへ JIT コンパイル
(compile-file src-path out-path)    ; ソースファイルをネイティブ実行ファイルへ AOT コンパイル（末尾の `(main)` は読み飛ばす）
(dump path)                         ; いまの環境（型情報 + コンパイル済み本体）を1ファイルへ
(disassemble name)                  ; その定義が何になるかを印字（既定はホストの機械語、第2引数 true で LLVM IR）
```

`compile` は特殊形で、`name` は評価されず未評価の裸シンボル/`::`パスとして読む（文字列は型エラー）。
ジェネリックな関数は対象にできない——型ごとの実体は使用箇所ごとに作られるので、単一のコンパイル済み本体が
存在しない。**解決できない名前はチェック時のエラー**であり、実行時まで持ち越されない
（型は在るがそのメソッドが無い場合／型も関数も無い場合／裸の未定義名、で別々のメッセージになる）。
ここでの可視性は他の参照と同じ扱いで、「在るがここからは見えない」は「解決しない」と同じく
チェック時に落ちる。

呼び先も推移的にコンパイルされるので、**コンパイルできないものを（間接的にでも）呼ぶ関数は
コンパイルできない**。プロセスが落ちるのではなく、その旨を述べるエラーで断られる。
組み込み関数はすべてコンパイルできるので、この形で断られるのは次のインタプリタ専用の操作を
呼ぶ関数だけ:

```lisp
(defun g () int 1)
(defun f () () (progn (trace g) ()))
(compile f)
; => trace: `(trace ...)` is an interpreter-only action and cannot itself be compiled
```

インタプリタ専用なのは `compile`/`compile-file`/`dump` と、`trace`/`untrace`/`step`/`disassemble`
（[処理系の道具](functions/system.md#5-処理系の道具clhs-252)）。これらはコンパイルできないのではなく、
コンパイルする側の操作である（`dump` が書き出すのはインタプリタの環境そのもので、AOT 実行ファイルには
その環境が無い。`trace` が見ているのも `step` が止まるのも走っているインタプリタの呼び出し経路で、
`disassemble` はコンパイラそのものを使う）。`room`/`dribble`/`ed` はこの仲間ではなく、普通に
コンパイルできる。

コンパイル**できる**もの: ストリーム・ファイル I/O、`random`、`gensym`、
`symbol->string`/`string->symbol`、`parse-int`/`parse-float`、`get-universal-time`/
`get-internal-real-time`、`exit`、超越関数、ビット演算、`catch`/`throw`/`unwind-protect`、
`eq`/`eql`/`equal`/`equalp` の4つ全部（`case` もこれで全型でコンパイルできる）、
`print`/`println`/`format`/`pprint` と `pprint-logical-block` を含む印字一式、`read`、
そして `eval`。標準ライブラリはコンパイル済みの状態で同梱されている。

AOT 実行ファイルには、プログラムが使う機能のぶんだけが入る。印字しないプログラムに書式エンジンは
入らず、`read` を呼ばないプログラムにリーダは入らず、`eval` を呼ばないプログラムにチェッカーと
インタプリタは入らない。

コマンドラインからは `typl -c src-path [-o out-path]`（`-c` は `--compile` とも書ける）で `compile-file` と同じことができる。
`-o` を省くと、`src-path` から拡張子 `.typl` を除いたものが出力になる。実行ファイルにリンクする
静的ライブラリ `libtypelisp_front.a` は、既定では、リリースビルドの `typl` なら `typl` が
中に持っているものを初回のリンク時に `$TYPELISP_HOME/lib/<ビルドID>/`（`TYPELISP_HOME` が
無ければ `~/.typelisp/lib/<ビルドID>/`）へ書き出して使い、デバッグビルドなら `typl` をビルドした
場所のものを使う。`typl --remove-lib` は、その `typl` が書き出したものを削除する。
`--others` を付けると他のビルドIDのものを、`--all` を付けるとすべてのビルドIDのものを削除する。
`typl --lib-dir DIR` を指定すると `DIR` にあるものを使い（`-c` にも `compile-file` にも効く）、
そこに無ければ起動時にエラーになる。

### 10.1 ダンプ

```lisp
(dump "session.typld")     ; 書き出す
```
```sh
typl --image session.typld prog.typl   # そこから起動する
typl --image session.typld             # REPL も同じ
```

ダンプは、型情報とコンパイル済みの本体を 1 ファイルに収めたもの。`(dump path)` が書くのは、
いまのセッションが読み込んだもの（標準ライブラリ、あるいは `--image` で渡されたダンプ）に、
**セッション自身が定義したもの**を足したもの。だから出力は自己完結していて、`typl --image` で
同じ環境が立ち上がる。セッション中に `(compile f)` したものは、コンパイル済みの形で書き出される。

保存されるのは**定義であって履歴ではない**:

- セッションのトップレベル式（`(println ...)` など）は入らない。ロードで再実行されたら困る。
- グローバル変数は**初期化式を走らせ直した値**で戻る。ダンプ時点の値ではない。
  これは SBCL の `save-lisp-and-die`（ヒープをそのまま書き出す）との意図的な違いで、
  この選択のおかげで「保存できない値」——開いているストリーム、クロージャの関数ポインタ、
  外部メモリ——という問題群がまるごと消える。
- `save-lisp-and-die` と違い、**プロセスは死なない**。書き出しはイメージを壊さないため。

ダンプは、それを書いた処理系の標準ライブラリとコンパイラの版を記録している。版の違う `typl` で
読み込もうとするとエラーになり、黙って受け入れることはない。

### 10.2 AOT 実行ファイルの中の `eval`

`eval` は「現在の大域環境」に対して型検査してから評価する（[解析・評価](functions/system.md#6-解析評価)）。
その環境——チェッカーが引く署名・型・マクロの表と、インタプリタが実行できる本体——は
**機械語には入っていない**。コンパイル済みの関数はアドレスに置かれたシンボルでしかなく、
引数の型も、名前から本体を引く表も持っていないからである。

そこで `compile-file` は `eval` を呼ぶプログラムに限って、**その環境をコンパイル時に組み立てて
実行ファイルに書き込む**。形式はダンプと同じで、標準ライブラリの分とプログラム自身の分が入る。
起動時にやるのは復元だけで、ソースを読み直すことも型検査し直すことも無い。`eval` を呼ばない
プログラムには何も足さない。

帰結:

- **起動に時間がかかり、実行ファイルが大きくなる**。チェッカーとインタプリタのコードと、
  環境のスナップショットが入るため。ヒープも大きめに取る。
- **eval したフォームは解釈実行される**。プログラム自身の関数を呼ぶ形を eval しても、走るのは
  スナップショットが持っている解釈実行用の本体のほう。結果は同じで、速度だけが違う。

グローバル変数の記憶域はコンパイル済みコードと**共有される**（同じスロット）。`defvar` の
初期化子はコンパイル済みの初期化が1回だけ走らせ、復元のほうはスキップする——副作用のある
初期化子が二度走らないため。

`compile-file` は標準ライブラリも読む（その本体を実行ファイルへ埋め込む）ので、`abs`/`gcd` のような
標準ライブラリの関数も、`(impl print-object ...)` も、`(defmethod print-object ...)` と同じく
AOT で使える。

`compile-file` は `use`（および `import`/`shadowing-import`）も受理する。エントリファイルの
`(use m)` は `typl file.typl` と同じ規則でファイルを探し、見つかった依存ファイルも
コンパイルして実行ファイルへリンクする——`main.typl` が `(use http)` で `http.typl` を
読む構成もそのまま AOT 化できる。エントリファイル自身の定義も `typl file.typl` と同じく
ファイル名のモジュールに入る（`p.typl` の `point` は `p::point`）。そのため値の表示
（`#<p::point x: 1 y: 2>`）もどちらで実行しても同じになる。

## 11. リーダマクロ（readtable）

リーダが**ある文字に出会ったとき何をするか**を、プログラムから差し替えられる（CLHS 23.1）。

```lisp
(set-macro-character c f)             ; 文字 c を f が読む
(get-macro-character c)               ; Option<f>
(set-dispatch-macro-character d s f)  ; 2文字並び d s を f が読む
(get-dispatch-macro-character d s)    ; Option<f>
```

`f` の型は `(fn (string-input-stream char) Option<Sexpr>)`。第 1 引数は**読み残しのテキストを
張ったストリーム**、第 2 引数は**発火した文字**（ディスパッチなら 2 文字目）。返り値がそこに
読まれたデータになる。ストリームが `:dyn PeekInput` でなく具体型なのは、リーダが渡すものが
常にこれ 1 種類だから——`read-sexpr` / `read-char` / `peek-char` / `unread-char` /
`read-delimited-list` はどれも `(where (PeekInput S))` なので、具体型のまま全部使える。

```lisp
(set-macro-character #\!
  (lambda ((s string-input-stream) (c char)) Option<Sexpr>
    (match (read-sexpr s)
      ((ok o) (match o
                ((datum d) (sexpr-cons (quote not) (sexpr-cons d (quote ()))))
                ((eof) (quote ()))))
      ((err e) (quote ())))))

!(equal 1 2)   ; => (not (equal 1 2)) と読まれる、つまり true
```

リーダは**マクロ文字を組み込み構文より先に見る**ので、`(` や `'` も奪える。`#` のサブ文字は
組み込みの `#b`/`#x`/`#.` より登録が優先される。`#` 以外の文字も
`set-dispatch-macro-character` に渡せばその場でディスパッチ文字になる——CL の
`make-dispatch-macro-character` に当たるものは**無い**。登録がその役をしてしまうので、
別の段として残しても何もすることが無い。

**いつ効くか**は `#.`（1 章）と同じで、読み込み経路によって変わる:

- REPL と `(load ...)` は 1 フォームずつ実行するので、**手前のフォームで定義した関数**を
  そのまま登録できる。
- モジュールファイルは単位として検査され実行は後——なので `set-macro-character` /
  `set-dispatch-macro-character` の**呼び出しだけが即時実行される**
  （CL の `(eval-when (:compile-toplevel) ...)` の役）。
  即時に走る以上、**渡す関数はその時点で在らねばならない**。同じファイルの `defun` は
  まだ走っていないので、`lambda` で書くか、標準ライブラリか既に走ったものを使う。
  トップレベルの呼び出しだけが対象で、`progn` や `let` の中は見ない。

組み込みの `read` / `read-from-string` も readtable を見る（CL と同じ）。

**無いもの**: `*readtable*` と `copy-readtable`、および `readtable-case`。前の 2 つは
readtable が**値でない**ため——値なら「リーダに手渡せるもの」でなければならないが、
ソースを読むリーダはプログラムの外側にあり、渡す先が無い。`readtable-case` は、この言語のリーダが
常に小文字化する（CL の `:downcase`）と 1 章で決めているため。


## 12. 並行機構（タスク）

**タスクは軽量スレッド**（Go で言えば `go` 文で起こすもの）で、協調的に走る（プリエンプションは
無い）。切り替えはカーネルを通らず、実行状態は機械スタックではなくヒープ上にあるので、
タスクは安く大量に作れる。

**タスクは複数の OS スレッドで同時に走る**（マルチコア並列）。スレッド数は環境変数
`TYPELISP_THREADS`（`main` を走らせるスレッドを含む総数、既定はマシンの並列度）。
`typl` では**コンパイル済みのタスクだけ**が他のスレッドで走り、解釈実行されるタスクは
インタプリタのスレッドで走る（12.7）。共有データは `Mutex<T>` か `Chan<T>` を通す
——通さない同時の読み書きは Go と同じく未定義（12.7）。

語彙のうち**特殊形は `task` / `thread` / `select` の 3 つだけ**で、残りはふつうの関数・メソッド・マクロ
（[タスクとチャネル](functions/concurrency.md)）。

### 12.1 `task` — タスクを起動する

```lisp
(task (f arg...))                   ; Task<T> を返す。T は f の戻り型
```

**呼び出し形だけを取る。** `f` も各 `arg` も `task` を書いた場所で、書いた順に評価され、
新しいタスクで起きるのは**呼び出しだけ**。Go の `go f(x)` と同じ規則で、これが
サンクでなく呼び出し形を取る理由でもある——サンクは引数を評価せずに捕捉してしまう。

```lisp
(dotimes (i 10)
  (task (worker i ch)))             ; i は毎回その場で評価される。捕捉の罠は無い

(task ((lambda () ()                ; 任意の本体を走らせたいときは lambda を呼ぶ
         (println "start")
         (send ch 1))))
```

特殊形（`if` / `let` / `progn` …）は `task` の直下に書けない。

**関数にできない理由**: `(spawn (lambda () T body...))` と書くなら `T` を綴る必要があるが、
`lambda` は戻り型注釈が必須で、マクロは `(f a b)` の戻り型を知らない。知っているのは
チェッカーだけ。

### 12.2 `thread` — 専用の OS スレッドでタスクを起動する

```lisp
(thread (f arg...))                 ; Thread<T> を返す。T は f の戻り型
(join th)                           ; 完了を待ってその値を返す（何度でも可）
```

形と評価規則は `task` と同じ（呼び出し形だけを取り、`f` も `arg` も書いた場所で評価される）。
違いは走る場所で、**そのタスク専用の OS スレッドを 1 本起こし、その上だけで走らせる**。
他のタスクと多重化されないので、中でブロックする C 関数（`defffi`）を呼んでも止まるのは
そのスレッドだけで、他のタスクは進む。中では `task`・`send`・`recv` などがそのまま使える。

- `Thread<T>` は `Task<T>` の対。`join` は `wait` と同じく**呼んだタスクを**止め、値は
  キャッシュされる。タスクが終わるとスレッドも終わる。
- panic の規則は `task` と同じ（プロセス全体が落ちる）。`main` が返ればプロセスが終わる。
- 関数として書きたいときは `(Thread::spawn (lambda () T body...))`（Rust の
  `std::thread::spawn`）。名前付きの関数を渡してもよい。
- **専用スレッドで走るのはコンパイル済みのコードだけ。** `typl` で解釈実行中に
  `(thread (f ...))` や `Thread::spawn` を評価すると、走らせる関数（とそこから呼ばれるもの）を
  その場でコンパイルしてから走らせる。コンパイルできないもの——外側のローカル変数を参照する
  `lambda`、構造体の構築など——は、スレッドを起こす前に、`(panic ...)` と同じ扱いの panic に
  なる。ローカル変数を参照する `lambda` は、コンパイル済みの関数の中で作れば渡せる。

### 12.3 `select` — 複数のチャネル操作を同時に待つ

```lisp
(select
  ((v (recv ch1)) body...)          ; 受信腕。v は Option<T> に束縛される
  ((send ch2 x) body...)            ; 送信腕
  (else body...))                   ; 省略可。**書くなら最後**
```

- **`else` があればブロックしない**（Go の `default`）。無ければどれかが可能になるまで待つ。
- **同時に複数可能なら 1 つを無作為に選ぶ**（記述順にすると後ろの腕が飢える）。
- 受信腕の `v` は **`Option<T>`**。閉じたチャネルは「答え」であって腕が飛ばされるのではない
  ので、腕の中で `match` する。
- 型は**全腕の本体の型の合流型**（`match` の腕と同じ規則）。
- 腕が 0 個の `(select)` は型エラー（Go の `select{}` ＝永久ブロックは非採用）。`else` だけの
  `select` も同様——本体をそのまま書いたのと同じだから。

**チャネル式と送る値は、どの腕が選ばれるかに関わらず左から 1 度だけ評価される**
（`case` がキーに対して持つのと同じ規律）。

```lisp
(select                             ; タイムアウト付き受信
  ((v (recv ch))          (println "~a" (unwrap v)))
  ((z (recv (after 0.5))) (println "timeout")))
```

`after`（[時間で届くチャネル](functions/concurrency.md#5-after--時間で届くチャネル)）は
「`sec` 秒後に 1 つ届くチャネル」で、Go の `time.After` に当たる。

### 12.4 ほかの機能との関係

| 機能 | タスクとの関係 |
|---|---|
| `catch` / `throw` | **タスク境界を越えない**。タスク本体を抜けようとする `throw` は panic |
| `unwind-protect` | タスクが自然に終わるときは cleanup が走る。**メインタスク終了によるプロセス終了では走らない** |
| `block` / `return-from` | 字句的なので `lambda` 境界を越えない |
| `panic` | Go と同じくプロセス全体が落ちる。`wait` は panic を値として観測しない |
| `dlet` | **タスクごとの束縛にはならない**。「グローバルを借りて返す」ままなので、タスク間で干渉する |
| 標準出力 | 全タスクが共有する。`println` 1 回の出力が他と行の途中で混ざることは無い |
| `compile` / `eval` | 制限は無い。タスクの中の `(compile f)` は通る |

### 12.5 切り替わる場所

協調的スケジューリングなので、**切り替わるのは書いた場所だけ**：`(yield)`、`(sleep ...)`、
`(wait ...)`、**待つことになったチャネル操作**（`send`/`recv`/`select`）、そして
**待つことになったソケット操作**（`accept`／`tcp-connect`（名前解決を含む）／ソケットへの
読み書き／`recv-from`、[ネットワーク](functions/network.md)）。ソケットは全部 non-blocking で、
用意できていなければそのタスクだけが止まり、OS が準備できたと答えたときに再開する——Go の
netpoller と同じ形。走れるタスクが無いときだけ、処理系は最寄りの `sleep` 期限まで OS を待つ。

その場で答えが出るチャネル操作——バッファに空きのある `send`、値のある `recv`、
`(len ch)`/`(cap ch)`/`(close ch)`/`(Chan::new n)`——は**番を消費しない**。読み取りで
勝手に割り込まれないということで、`(sleep 0.0)` が CL 流の「譲る 0 秒」であるのとは
別の扱いになっている。

**プリエンプションは無い。** 何も呼ばないタイトループは他のタスクを飢えさせる
——ただしコンパイル済みのループは定期的にスケジューラへ手を渡すので、
コンパイル済みのタイトループは飢えさせない。

### 12.6 コンパイル済みコードとタスク

コンパイル済みのコードもタスクを中断できる。`compile-file` で作った実行ファイルも
同じで、`main` はスケジューラのメインタスクとして走る——`task`・`sleep`・`wait`・チャネル・
ソケット待ちのどれも `typl` と同じ意味で動き、`main` が返ればプロセスが終わって残りの
タスクは打ち切られる（Go と同じ）。スケジューラのためにインタプリタが実行ファイルへ
入ることは無い。

例外は「C の FFI コールバックの中」だけで、そこでは**待つことになった**操作が
エラーになる（黙ってデッドロックするより親切なので）——`defffi` で渡した関数が C から
呼ばれている間は C のスタックが積まれており、タスクを中断して後で再開する手段が無い。

次の場所も、タスクの途中で呼ばれる関数でありながら中断はできない：`print-object` メソッド、
`format` の `~/name/`、リーダマクロ、`eval` の中、AOT 実行ファイルの `defvar` 初期化子。
ここでは**待たずに答えが出る操作は通り**（バッファに値のある `(recv ch)`、受信済みデータのある
ソケットの `read-line`、`(task ...)`、`(yield)` など）、**本当に待つことになる操作は
エラーになる**（その場でプロセスを止めるのではなく、`` `recv` cannot block: ... `` のような
`(panic ...)` と同じ扱いの panic として）。

### 12.7 Go との違い

- **`typl` で他のスレッドへ出るのはコンパイル済みのタスクだけ。** インタプリタの状態は
  スレッド間で共有できないので、解釈実行される `task` のタスクはインタプリタのスレッドで
  走る。コンパイル済みのタスクも、解釈実行される関数値を呼ぶ・誰もコンパイルしていない
  `:dyn` メソッドを呼ぶ・`eval`/`macroexpand`/`read` を呼ぶ時点で**インタプリタの
  スレッドへ移り、以後そこに留まる**（戻らない）。長い処理の途中で一度でも解釈実行される
  コードに触れると、残りはインタプリタのスレッドで走る。
- **`typl` のワーカーはトップレベルの評価 1 回ぶんだけ生きる。** REPL の入力待ちの間や
  トップレベルの形の合間には、他のスレッドはタスクを進めない（残ったタスクは次の評価で
  続きから走る）。評価の終わりには各スレッドが今の一歩を終えるのを待つので、`thread` の中で
  ブロックし続ける C 関数（`defffi`）があると、それが返るまで評価が終わらない。
- **ワーカー上の印字**: 解釈実行される `print-object`／`~/name/` メソッドは他のスレッドでは
  走らせられないので、そういう値を他のスレッドで印字すると `(panic ...)` と同じ扱いの panic になる
  （`(compile T::print-object)` するか、メインタスクから印字する）。
- **データ競合は未定義**（Go と同じ立場）。`Mutex<T>`・`Chan<T>` を通さずに複数のタスクから
  同じ値を書き換えた結果は保証されない。
- **`task` は値を返す。** Go の `go` 文と違い `Task<T>` が返り、`(wait t)` で結果を取れる。
- **nil チャネルが無い。** Go の fan-in の定石（閉じたチャネルを `nil` にして `select` の腕から
  外す）は書けないので、入力ごとに 1 タスク立てて `WaitGroup` で合流する
  （[WaitGroup](functions/concurrency.md#4-waitgroup--n-個の完了待ち)）。Go でもこちらが
  推奨される書き方だが、**Go から移る人が最初に困る差**。
