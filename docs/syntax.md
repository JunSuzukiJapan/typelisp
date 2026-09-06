# typelisp 構文リファレンス

typelisp は静的型付きの Lisp。文法は S 式。組み込み関数・メソッドの一覧は
[functions.md](functions.md) を参照。

## 1. 字句要素

- **大文字小文字は区別しない**。シンボルは読み取り時にすべて小文字へ正規化される。
- **コメント**: `;` で始まり行末まで（行コメント）。`#| ... |#`（ネスト可能なブロックコメント）。
- **読み込み時評価**: `#.(式)` は続くフォームを**読みながら実行**し、その値を読んだことにする。
  リーダがテキストだけの関数でなくなる唯一の場所。届く範囲は読み込み経路で変わり、これは
  CL と同じ:
  - `(load ...)` と REPL は 1 フォームずつ評価するので、**同じテキストの手前で定義した関数**を
    呼べる（CL の `load`）。
  - モジュールファイルは単位として検査され実行は `use` した側なので、`#.` から届くのは
    prelude と、そのセッションが既に実行したものだけ。ファイル自身の定義も、`use` した
    モジュールの定義も**まだ走っていない**（CL の `compile-file` で `eval-when` が要るのと同じ）。
  - `read-from-string` のような純粋な読みには評価器が無いので、`#.` はその旨のエラーになる。
- **真偽値**: `true` / `false`。
- **整数**: 10進（`42`, `-7`）。符号 `+`/`-` を前置可能。10進以外は CL の radix マクロ
  `#b`/`#o`/`#x`/`#NNr` で書く（符号は印の後ろ、`#x-ff`）。`0x` 接頭辞は CL に無いので
  採らない——`0xff` はシンボルとして読まれる。
  型注釈のない整数リテラルは既定で `i32`。**期待される型が整数型ならその型になり、
  その型が持てる値かどうかが検査される**——`(the u8 300)` は型エラー（切り詰めが欲しければ
  `(as u8 300)` と書く）。`(the u32 4294967295)` や `(the u32 #xFFFFFFFF)` はこの規則で書ける。
  型を名指す文脈が無いとき、`i32` の範囲を超える整数リテラルは（基数を問わず）`bignum` になる
  （CL 同様、固定長か多倍長かは値の大きさで決まり、専用構文はない）。
- **浮動小数点数**: 小数点または指数表記（`e`/`E`）を含むもの（`1.5`, `3.0e10`）。
  既定で `f64`（期待される型が `f32` ならその型になる）。
- **比 (ratio)**: `分子/分母`（10進のみ、例 `1/3`）。読み取り時に CL 仕様どおり既約化される
  （`2/4` は `1/2`）。整数値になるもの（`4/2` など）は `ratio` ではなく `Int`/`bignum` として
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
  なお `:dyn` は型位置専用の予約キーワードで、それ以外の場所に書くとエラーになる（§2 参照）。
- **リスト**: `(a b c)`。ドット対 `(a . b)` も読み取り可能。
- **空リスト `()`**: 文脈によって `Unit` 型の値、または `Option<Sexpr>` の `none` になる。
  **`Sexpr` に空リストの変種は無い**——`Sexpr` は「空でない S 式」を表し、S 式データの型は
  `Option<Sexpr>` である（§match の「`Option<Sexpr>` のパターン」参照）。
- **quote/quasiquote/unquote**:
  - `'x` → `(quote x)`
  - `` `x `` → `(quasiquote x)`
  - `,x` → `(unquote x)`（quasiquote の中でのみ意味を持つ）
  - `,@x` → `(unquote-splicing x)`（リスト要素として展開時に結合される）
- **パス `::`**: `foo::bar` はソース上でセグメント列に分割され、専用の `Value::Path` になる
  （文字列としては保持されない）。`::foo` のように先頭が `::` の場合はルートからの絶対パス。
  ジェネリック引数の内側の `::`（`Vec<a::b>` など）はパス区切りとして扱われない。

## 2. 型の書き方

型はソース上では通常のシンボルまたはリストとして書く。

- **プリミティブ型**: `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `bool` `char` `string`
  （64bit 幅の整数型は無い——[functions.md](functions.md) §1 参照）
- **多倍長数値型**: `bignum`（任意精度整数）、`ratio`（既約な有理数）。CL 準拠でヒープ確保され、
  `i32`/`f64` 等との暗黙変換はない（`as`/`try-as` または変換メソッドで明示。functions.md 参照）。
- **不透明な可変型**: `random-state`（PRNG の状態）。ネイティブ表現なので
  `Vector<T>`/`HashTable<K,V>`/`Sexpr` には入れられない（`Option<T>`/`Result<T,E>` には入る）。
- **Unit 型**: `()`
- **Never 型**: `!`（`panic`/`unreachable`/`todo`/`return`しないループ 等、発散する式の型。
  任意の期待型に適合する）
- **関数型**: `(fn (引数型...) 戻り値型)`。可変長引数を持つ関数型は
  `(fn (引数型... &rest 要素型) 戻り値型)`。
- **ジェネリック型**: `Name<T1,T2,...>`（空白なしの1トークンとして読み取られ、内部で分解される）。
  例: `Option<i32>` `Result<i32,ParseIntError>` `HashTable<string,i32>` `Vector<T>`。
  型引数には unit 型 `()` も書ける（`Result<(), FileError>`）。`(`/`)` は本来トークンを
  切るデリミタだが、山括弧が開いている間に限りこの2文字の組だけが通る。`()` は
  フィールド型・引数型としても使え、`compile`（JIT/AOT）にも対応している。
- **ジェネリック型の適用形**: `(Name T1 T2 ...)` — `Name<T1,T2,...>` と同じ型を指す
  リスト形式の綴り。例: `(vector char)` は `Vector<char>` と同一。
  名前形が普通の書き方で、こちらは**型引数が名前で綴れない場合のためにある**——
  型引数はそれ自体が型式だが、1トークンの名前の中に書けるのは名前・`()`・`:dyn` だけで、
  関数型は書けない（`Vector<(fn (i32) i32)>` という綴りは存在しない）。
  トレイトの関連型を署名に代入すると `impl` が束縛した任意の型が現れうるので、
  代入結果はこの形で出る（[dev/implementation-log.md](dev/implementation-log.md) の
  「関連型が総称名の内側にある場合」参照）。
- **修飾型名**: `module::Type` のように `::` で修飾できる。
- **trait オブジェクト型**: `:dyn Trait`（空白区切りの2語で1つの型）。実行時に具象型が決まる値を
  表し、trait のメソッド呼び出しは vtable 経由の動的ディスパッチになる。関連型を持つ trait は
  宣言順に位置指定で固定する（`:dyn Iter<i32>` は `Item` を `i32` に固定）。ジェネリック引数の
  内側にも書ける: `Vector<:dyn Drawable>` `HashTable<string, :dyn Drawable>`。
  具象値は期待位置で自動的に箱詰めされ、明示形は `(as :dyn Trait 式)`。
  `:dyn Sub` の値はスーパトレイト（推移的に継承しているものすべて）の `:dyn Super` を要求する
  位置にもそのまま渡せる（アップキャスト）。継承関係の無いトレイトへは渡せない — 制限は §5.2。
  `:dyn` を型位置以外に書くとエラー。詳細は [dev/language-design.md](dev/language-design.md) §5.2。
- 組み込みジェネリック型: `Option<T>`（`Some(T)` / `None`）、`Result<T,E>`（`Ok(T)` / `Err(E)`）、
  `Sexpr`、`HashTable<K,V>`、`Vector<T>`。組み込みの具象エラー型は `ParseIntError` /
  `ParseFloatError` / `ReadError` / `EvalError` / `FileError`（`Error` は型ではなく prelude のトレイト
  ——`:dyn Error` として使う）。詳細は functions.md を参照。
- **型とトレイトは同じ名前空間**（Rust と同じ）: 同一モジュール内で型（`defstruct`/`defenum`）と
  トレイト（`deftrait`）に同じ名前は付けられない。

## 3. トップレベル定義

### defun — 関数定義

```lisp
(defun name ((arg1 Type1) (arg2 Type2) ...) RetType
  body...)
```

- 引数の型・戻り値の型は必須。
- ジェネリック関数は名前に山括弧で型パラメータを書く: `(defun name<T1,T2...> (params) Ret body...)`
  （型位置の `Vector<T>` と同じ山括弧構文。旧来の `(name T1 T2...)` リスト形式は廃止）。
- `defun`/`lambda`/`defmethod` は末尾に `&rest (name Type)` を書くと可変長引数を受け取れる:
  `(defun name ((a Type1) &rest (xs Type2)) Ret body...)`（本体内では `xs` は常に `Sexpr` の
  リストとして束縛される。呼び出し側の各実引数は `Type2` として個別に型検査される）。
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
    シグネチャからしか手に入らない。`lambda` は値として渡され、その値を説明するのは `Type::Fn`
    だけ——そこに式を置く場所は無いし、置けば「同じシグネチャでデフォルトだけ違う 2 つの
    ラムダ」が別の型になってしまう。`&rest` は型の話に閉じているので `Type::Fn` に枠がある。
- **前方参照は `defsignature` で宣言する**（下記）。宣言していない名前は、定義より前では
  呼べない——トップレベルは 1 フォームずつ、ソース順に検査・実行されるため。
- トレイト境界を要求する場合は本体の直前に `where` 節を書く:
  `(defun name<T> (params) Ret (where (Trait T (AssocName ConcreteType)...)) body...)`
  （`(AssocName ConcreteType)` による関連型の固定は省略可能）。
- **docstring**: `where` 節（あれば）の直後、本体の先頭に文字列リテラルを置くと docstring になる
  （CL 準拠）。ただし後ろに本体フォームが最低1つ続く場合のみ——単独の文字列は戻り値のままで
  docstring とは区別されない: `(defun f () string "doc" "value")` は docstring 付きで `"value"` を
  返すが、`(defun f () string "value")` は docstring なしで `"value"` を返す。
  `(documentation name)` で取り出せる（§ documentation）。

### defsignature — 前方宣言

```lisp
(defsignature name (引数型...) 戻り型)
(pub defsignature name (引数型...) 戻り型)
```

自分より**後**に定義される `defun` を呼ぶには、先にこう宣言する。相互再帰はこれでしか書けない:

```lisp
(defsignature odd2? (i32) bool)
(defun even2? ((n i32)) bool (if (= n 0) true  (odd2? (- n 1))))
(defun odd2?  ((n i32)) bool (if (= n 0) false (even2? (- n 1))))
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

- **ジェネリック関数**。実体化には保持した本体が要り（`request_fn_specialization`）、宣言には
  本体が無い。前方呼び出しは解決してから実体化に失敗するので、宣言の時点で断る。
- **`&optional`/`&key`**。そのシグネチャは各デフォルト値の**検査済み**式を含み（引数省略時に
  呼び出し側へそのまま埋め込まれる）、宣言にはそれを置く場所が無い。
- **`defun` 以外**。`defmacro` は展開にマクロ本体が**実行済み**である必要があり、シグネチャ
  登録では代替できない。型（`defstruct`/`defenum`/`deftrait`）は、その登録が「型を登録する
  コード自身が必要とするもの」でシグネチャのように自己完結しない。`defmethod` は所有型の
  `TypeDef` に登録するので型に従う。

CL の対応物は `(declaim (ftype (function (i32) bool) even2?))` だが、あちらは宣言システム
一式を伴い、かつ**助言**でしかない。こちらは静的型付けなので宣言は検査される。

### defffi — C 関数の宣言（FFI）

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

書ける型は `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `bool` `()`（void）。

`:library` を書くとその共有ライブラリを開いてシンボルを探す。省くと**プロセス自身**（既に
リンクされているもの全部——libc を含む）から探す。名前は `sqlite3` のような短い名前なら
`libsqlite3.dylib` / `libsqlite3.so` の順に、`/` を含むならパスとして扱う。開いたライブラリは
閉じない——中の関数を指したコードが走り続けるので、正しい寿命はプロセスの寿命だけ。

宣言できないものが 4 つある:

- **可変長引数**（`printf`）。可変長部分は固定引数と別の規則で渡される（AArch64 Darwin では
  スタック）ので、固定シグネチャから組んだ thunk では正しくならない。`&rest` は拒否される。
- **構造体の値渡し・値返し**。同じ理由（プラットフォームの分類規則を thunk が再実装することに
  なる）。書ける型を上の一覧に閉じることで、綴れないようにしてある。
- **ジェネリック**。C に対応物が無い。
- **組み込みと同じ名前**。コンパイル済みの呼び出しはその名前でランタイムのシムに解決されて
  しまうので、静かに間違うより断る。

**値として渡せない**。`(map f xs)` の `f` に FFI 宣言をそのまま書くことはできない——関数値は
定義の本体を包んだクロージャで、この宣言には包む本体が無いため。`lambda` で包む:

```lisp
(run-it (unsafe (lambda ((n i32)) i32 (c-abs n))))
```

`(disassemble c-abs)` も断られる。表示できるのは C 側の機械語で、それはこのコンパイラが
作ったものではない。`(compile c-abs)` は成功する（何もしない——既にコンパイル済みなので）。

**実装**: 宣言ごとに thunk を1つ LLVM で生成する。共有 ABI（`i64 f(const i64*, u32)`）を持ち、
引数の語を宣言された C の型へ変換し、本物のシグネチャで呼び、戻り値を語へ戻す関数。thunk の
シンボル名は `defun` の本体と同じ規則で付くので、コンパイル済みの呼び出し側は普通の呼び出しを
出すだけでよく、自己ホストのコンパイラ（`src/compiler.rs`）は FFI の存在を知らない。

### defvar / defparameter / defconstant — グローバル変数

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

### defmethod — メソッド定義

```lisp
; インスタンスメソッド: (m obj args...) の形で呼べる
(defmethod name ((self Type) (arg Type2) ...) RetType body...)

; static / 関連関数: (Type::name args...) の形で呼べる
(defmethod name (Type (arg Type2) ...) RetType body...)
```

呼び出し側は `obj` の静的型からメソッドを解決する（単一・静的ディスパッチ）。`defun` と同じ位置・
同じ規則で docstring を置ける（`where` 節の直後、本体の先頭、後ろに本体フォームが続く場合のみ）。
`impl` 内のメソッドも同様——`(documentation Type::method)` で取り出す。

### defstruct — 構造体（ユーザ定義型）

```lisp
(defstruct Name
  (field1 Type1)
  (pub field2 Type2)
  ...)

; ジェネリック（山括弧で型パラメータ。旧来の (Name T1 T2...) リスト形式は廃止）
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

### defenum — 列挙型（直和型・ユーザ定義タグ付き共用体）

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

### deftype — 型別名

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

### deftrait / impl — トレイト機構

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
`where` 節でトレイト境界として参照する（§ defun 参照）。トレイト名には `m::Trait` のような
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

`prelude.rs` が提供する標準トレイト: **`Iter`**（`next`／関連型 `Item`。`doiter`／シーケンス関数の
基盤）・**`Eq`**（`equals`。`not-equals` はデフォルト実装）・**`Ord`**（`Eq` を継承。`less` のみ
実装必須で `less-equal`／`greater`／`greater-equal` はデフォルト実装）・**`Error`**（`message`／
`source`。エラー型を一様に扱うための `:dyn Error`）・**`print-object`**（型ごとの印字表現）・
**`Pathish`**（パス名指定子＝文字列 or `pathname`）・ストリーム階層 **`Stream`** →
**`InputStream`**／**`OutputStream`** → **`CharInput`**／**`CharOutput`** → **`PeekInput`**。
`Iter`/`Eq`/`Ord` は主要なスカラ型と `cons-cell<A,B>` に実装済み
（詳細は [functions.md](functions.md) §12・§12.1・§7.1・§15.2・§18・§19）。
自前のコレクション型に `Iter` を `impl` すれば `doiter`（§5）や `map`／`filter`／`sort` 等がそのまま使える。

トレイトの呼び出しは既定で**静的**（レシーバの静的型で解決）。実行時に具象型が決まる値を扱いたい
場合は trait オブジェクト型 `:dyn Trait`（§2）を使うと vtable 経由の動的ディスパッチになる:

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
必要がある）。箱に入れられるのはヒープ表現を持つ型
（`defstruct`/`defenum` 等）で、プリミティブ型は入れられない。詳細と設計理由は
[dev/language-design.md](dev/language-design.md) §5.2。

### module / use — 名前空間

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

### ファイル↔モジュール対応（マルチファイルプロジェクト）

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
  `pub` が必要（§ pub）。
- **循環参照はエラー**: `circular module dependency: a -> b -> a` の形で連鎖が報告される。
- **実行**: `typl <file.typl>` でファイルを実行できる（引数なしなら REPL）。REPL の `use` も
  同じ規約でファイルを解決する。
- **cons アリーナ容量**: `typl --heap-cells N` で cons セルのアリーナ**初期容量**を指定できる
  （既定 65536。`--heap-cells=N` 形も可、ファイル実行/REPL 共通）。チェッカーがコード自体を
  cons セルへ落とすようになって以降、必要量はプログラムを読むまで分からないので、アリーナは
  足りなくなればチャンクを**追加して伸びる**（既存セルは動かないのでポインタは有効なまま）。
  伸びる上限は初期容量の 256 倍で、そこを超えた確保が `heap exhausted` になる——つまり初期容量は
  「最初にこれだけ確保する」、上限は「ここを越えたらリークとみなす」という意味。

### load — フラットロード

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

### pub — 公開指定

```lisp
(pub defun ...)
(pub defvar ...)
(pub defconstant ...)
(pub defmacro ...)
(pub defmethod ...)
(pub defstruct ...)
(pub defenum ...)
```

`pub` が付けられるのは上記7種類のみ（`module`/`use`/`deftrait`/`impl` には付けられない）。
定義形を括弧で包む `(pub (defun ...))` 形式ではなく、`pub` の直後に定義キーワードを続ける。
1つの `pub` が公開指定できる定義は1つだけ（複数の定義の一括指定はできない）。

### defmacro — マクロ定義

```lisp
(defmacro name (必須... &optional opt... &rest rest-name &key key...) body...)
```

- 全パラメータ・戻り値は常に `Sexpr` 固定なので型注釈は書かない。
- CL 流の非衛生的マクロ（`gensym` で衝突を避けるのはマクロ作者の責任）。
- ラムダリストは CL 流に `必須 &optional &rest &key` の順（各マーカーは高々1回、この順序でのみ）。
  - `&optional` … 省略可能引数。`name` または `(name デフォルト式)`。デフォルト式は展開時に評価され
    （先に束縛済みのパラメータを参照できる）、省略時に束縛される（デフォルトを書かなければ `()` = `nil`）。
  - `&rest name` … 残りの位置引数を1つの `Sexpr` リストとしてまとめて受け取る。
  - `&key` … キーワード引数。`name` または `(name デフォルト式)`。呼び出し側は `:name 値` で渡す
    （順不同）。省略時はデフォルト式（無ければ `nil`）。未知のキーワードや奇数個の `:key` 列はエラー。
- 例: `(defmacro pair (x &optional (y 1)) ...)` / `(defmacro make (&key (a 0) (b 9)) ...)`。

### macrolet / symbol-macrolet — 局所的なマクロ束縛

```lisp
(macrolet ((name (ラムダリスト) body...) ...) body...)   ; 字句スコープのマクロ
(symbol-macrolet ((name 展開形) ...) body...)            ; 名前が形を表す
```

どちらも**式**の特殊形で、実行時には何も残らない（本体がコンパイルされるのは展開後の形）。
ラムダリストは `defmacro` と同じ。詳しい規則と例は
[functions.md](functions.md) §14.1。

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
                                     ; 書く許可を与える。§3 defffi を参照
(prog1 form more...)                ; 全部評価し、値は form のもの。defmacro
(prog2 a b more...)                 ; 全部評価し、値は b のもの。defmacro
(the Type expr)                     ; 型注釈（実行時の効果なし）
```

### unsafe — 検査できない前提を引き受ける

```lisp
(unsafe body...)
```

`progn` と同じ——本体を順に評価し、最後の値を返す。スコープも作らず、関数の境界でもない
（`break` / `return-from` は素通りして外へ抜ける）。違うのは、この中でだけ書けるものがある点。

いま `unsafe` を要求するのは、[defffi](#defffi--c-関数の宣言ffi) で宣言した C 関数の呼び出し。

`unsafe` が引き受けるのは、コンパイラが確かめられない次の前提:

- **型の一致**。宣言した C シグネチャが本物と合っていること。合っていなければ、引数は間違った
  レジスタに載り、戻り値は間違った幅で読まれる。
- **メモリ安全**。C 側が渡されたものをどう扱うか。
- **プロセス大域の状態**。環境変数・シグナルハンドラ・`errno`。たとえば `setenv` を FFI で
  呼ぶと、この処理系の `decode-universal-time` が使う `localtime_r` の健全性の前提が崩れる
  （`crates/typelisp-rt/src/os.rs` の該当コメントを参照）。
- **スレッド安全**。

型検査からの逃げ道ではない。`(unsafe (+ 1 "two"))` は通らない。許されるのは特定の**操作**を
書くことであって、でたらめを書くことではない。

字句的に働く。`unsafe` の中に書いた `lambda` の本体はその許可を継承する（Rust の `unsafe`
ブロック内のクロージャと同じ）——その値が後で `unsafe` の外から呼ばれることはありうるが、
そこに書いたこと自体が責任を引き受ける行為だとみなす。

### destructuring-bind — リストを形で分解する

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

### match — パターンマッチ

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
- **値リテラル**: 文字列 / 浮動小数点 / シンボル（`'foo`）/ bignum / ratio — その型の
  `Eq::equals`（§2 のトレイト）による値比較。文字列は内容比較であって同一性比較ではない
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

即値でないリテラル（文字列 / 浮動小数点 / bignum / ratio）は `Sexpr` に対して**書けない**。
それらの `eq` は `Str`・箱・cons セルの同一性なので「型は通るが決してマッチしない腕」になるため、
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

`Sexpr` スクルーティニーに対しては、上記の組み込み10変種パターンに加えて **downcast パターン**
（ユーザ定義 ADT インスタンスの取り出し）が書ける — `(list p 42)` のように `Sexpr` へ暗黙変換された
`defstruct`（§3）/`defenum`（§3）インスタンスを `match` で取り戻す構文:

- `(TypeName sub-pattern...)` — **型名**を先頭に置くフィールド分解（struct 専用、`defstruct` は変種が
  常に1つなので変種名でなく型名で書く）。例: `(defstruct point (x f64) (y f64))` に対し `(point x y)`。
- 裸の変種名 `(VariantName sub-pattern...)` — `defenum` の変種抽出。`(use EnumType)` 済みで可視な
  bare 名として解決される（`resolve_ctor` と同じ可視性規則）。例: `(defenum color (red) (blue))` の
  `(use color)` 後に `(red)` `(blue)`。可視な複数 enum で変種名が衝突する場合は曖昧エラーになるため、
  修飾形 `(EnumType::VariantName ...)` でも書ける（`use` 不要）。
- `(the Type pattern)` — 型全体でのdowncast（丸ごと束縛）。フィールド分解せず、値をそのまま
  `pattern` へ渡す。可変な struct の同一性を保ったまま取り出せる唯一の書き方であり、`Vector<T>`/
  `HashTable<K,V>` を `Sexpr` から取り出す唯一の手段でもある（両者はフィールド分解形を持たない）。
  例: `(the point p)` の後で `(setf p::x 9)` すればリスト内の元インスタンスにも反映される。

**`Option<Sexpr>` のパターン**: S 式データの型は `Sexpr` ではなく `Option<Sexpr>` で、空リストは
`Sexpr` の変種ではなく `Option` の `none` である。そのため `Option<Sexpr>` を `match` するときは、
`Sexpr` の 10 変種と `none` を**同じ腕の並びに平らに**書ける（`Option` を剥がす外側の `match` は
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

網羅性も同じ平らな宇宙——`Sexpr` の 10 変種 ＋ `none` の 11 個——で検査する。`(none)` を
書き忘れれば `_` が無いかぎりエラーになる。`(some x)` も従来どおり書けて「空でない何か」を束縛する。

この糖衣は `Option<Sexpr>` **ちょうど**にしか掛からない。`Option<Option<Sexpr>>` では
`(int n)` がどちらの層を剥がしたのか決まらないので、通常どおり 2 段の `match` を書く。

**trait オブジェクト（`:dyn Trait`、§2）のスクルーティニー**にも同じ downcast パターンがそのまま
使える——`match` は箱を外してから上の `Sexpr` パターン機構に渡すので、追加の構文はない。実装型の
集合は開いているので網羅にはならず、`_` が必須:

```lisp
(defun area ((d :dyn Drawable)) i32
  (match d
    ((circle r) (* 3 (* r r)))     ; 型名先頭のフィールド分解
    ((the square s) (* s::side s::side))
    (_ 0)))
```

**腕どうしの型推論**: 全ての腕は同じ型でなければならない（`panic` 等で発散する腕は除く）。
期待型が無い位置に書かれた `match` では、腕が**互いに**足りない型引数を埋め合う——
`(result::ok v)` は `T` だけ、`(result::err e)` は `E` だけを決めるが、両方を並べれば
`Result<T,E>` が決まる。片方の腕だけでは決まらない型引数が最後まで残った場合は、
その腕自身のエラー（`cannot infer type argument ...`）になる。`match` の外では従来どおり、
決まらない型引数はその場でエラー。

downcast パターンを使う `match` の網羅性チェックは、`Sexpr` 本来の変種のカバレッジには数えない
（downcast パターンだけを並べた `match` は `_` で閉じる必要がある）。ジェネリックな ADT
（`defstruct point<T> ...` など）は downcast パターンの型引数を推論できないため、フィールド分解形
（`(point ...)`)/裸変種形は使えず、`(the point<i32> p)` のように `the` で明示する。

**downcast は実体化まで見る。** 明示した型引数は照合に使われる——`(the point<i32> p)` は
`point<i32>` の値だけを通し、`point<string>` は素通りして次の腕へ行く。値が自分の実体化を
型キーとして持っている（`print-object` のディスパッチと同じ仕組み、
[functions.md](functions.md) §15.2）ためで、型引数を捨てて基底名だけで比べていたときは
`point<string>` が `(the point<i32> ...)` に通り、フィールドを `i32` として読んでいた。

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

### 5.0 `block` / `return-from` — 名前付きの脱出（cl-parity-plan.md Phase 4a）

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
  越える必要があるものは `catch`/`throw`（§8、こちらは**動的**）。

`break`/`return`(§5) と同じ**静的**な脱出なので、コンパイル済みコードでは
コンパイル時に決まっている基本ブロックへの分岐になる。途中に `unwind-protect` があれば
その `cleanup` は走る（§8）。

`return-from` を一度も書かなければ、その関数のコードは block が無かったときとまったく同じ
（チェッカーは名前が実際に使われたときだけ `block` ノードを出す）。

### 5.1 拡張 `loop`（CL の LOOP DSL、cl-parity-plan.md Phase 4b）

`loop` の**第 1 要素がキーワードなら**節の並びとして読む。そうでなければ上の単純ループの
ままで、既に書かれている `loop` の意味は変わらない（CL 自身の simple loop 規則と同じ）。

CL は節の語を裸のシンボルで書くが（`(loop for i from 1 to 3 collect i)`）、ここでは
**すべてキーワード**にする——裸の `for` はただの変数参照になってしまうし、キーワードで
あることが単純ループとの分かれ目でもある。例外は変数と値を区切る `=` で、位置が
一意なので裸でもキーワード（`:=`）でも読む。

```lisp
(loop :for i :from 1 :to 3 :collect i)              ; #<vector<i32> 1 2 3>
(loop :for x :in (iter v) :when (evenp x) :sum x)
(loop :repeat 4 :for x = 1 :then (* x 2) :collect x) ; #<vector<i32> 1 2 4 8>
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
を張るが、ここに `nil` は無く、`break`/`return`（§5）が既に「直近のループを抜ける」を持って
いる。

```lisp
(loop :named outer :for i :from 1 :to 3
  :do (loop :for j :from 1 :to 3 :do (if (= (* i j) 4) (return-from outer (* 100 i)) ()))
  :finally (return 0))                                  ; 200
```

`:finally (return 0)` を省くと**型エラー**になる。`block` の規則がそのまま効くだけで
（§5.0）、脱出の型 `i32` と、尽きたときにループが残す `()` が合わない。

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
- `:collect` の要素型はチェッカーが集約式を先に検査して決め、`(the Vector<T> …)` として
  書き込む。`Vector::new` の型引数は期待型から前向きに来るので、後ろの `push` からは
  決まらない（計画のこの前提は誤りだった）。関数型のように**書き表せない型**を集めようと
  するとその旨のエラーになる。

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
                                     ; 例外的に set が対応、それ以外は set-アクセサ名）
(incf place)  (incf place delta)    ; place += delta（省略時 delta=1）。結果は setf 同様
(decf place)  (decf place delta)    ; place -= delta（省略時 delta=1）
(rotatef place1 place2 ... placeN)  ; N個の place を巡回シフト（新place1=旧place2, ...,
                                     ; 新placeN=旧place1）。各 place の部分式は1回だけ評価
(shiftf place1 ... placeN newvalue) ; place2..N の値を左へシフトし、newvalue を placeN へ。
                                     ; 戻り値は旧 place1 の値
(list e1 e2 ... en)                 ; (cons e1 (cons e2 (... ()))) への展開。0引数なら ()
                                     ; 各要素は Sexpr へ暗黙変換される（CL のcons同様、任意の値を
                                     ; 保持できる）: スカラ(i32/f64/bignum/ratio/char/bool/string/
                                     ; symbol)は対応する Sexpr コンストラクタでラップ、defstruct/
                                     ; defenum/Vector<T>/HashTable<K,V> 等ヒープ表現ADTは無変換の
                                     ; まま retype（実行時コストなし）。&rest/format引数も同様。
(source-file)                       ; このフォームが読まれたファイル名（string）。チェック時に
                                     ; 定数畳み込みされる。CL の *load-pathname* に当たるが変数では
                                     ; ない——モジュールの本体は検査の後に実行されるので「いま
                                     ; ロード中」は当てにならず、チェッカーのほうは常に知っている。
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
書式ディレクティブと pretty printer の詳細は [functions.md](functions.md) §15 / §15.1。

`as`/`try-as` が扱えるのは数値・文字カタログのみ（整数型・`f32`/`f64`/`bignum`/`ratio`/`char` 間）。
同一型は無変換。**整数の幅どうし・`f32`↔`f64` は本物の変換**——`as` は切り詰め／丸め、`try-as` は
その幅（精度）に入るかどうかを答える。`i32`→`char` と `bignum`→`i32` も範囲外で失敗しうるので
`as` は panic・`try-as` は `None`。それ以外（拡大変換や `float->int` 等の切り捨て）は常に成功する。
他の族から狭い整数への `try-as`（例 `(try-as u8 some-bignum)`）は拒否される——1つの `Option` に
「変換できたか」と「その幅に入るか」の2つの問いを詰め込むことになるため、分けて書く。
内部的には対応する変換メソッド（functions.md の `int->char`/`int->bignum`/`bignum->int` 等）へ
展開される糖衣構文。

`documentation` は `quote`/`compile` と同様、`name` を評価せず未評価の裸シンボル/`::`パスとして
読む特殊形。CL の `(documentation 'name 'function)` と違い型引数は取らない——`name` を変数→関数→型
→トレイト→マクロの順（裸識別子を式として評価するときと同じ優先順位）で解決し、見つかった定義の
docstring を返す（`(documentation Type::method)` はメソッド専用）。解決自体に失敗する（そんな名前の
定義が無い）のは check 時のエラー、定義はあるが docstring が無い場合は `Option::none`。すべて check
時に定数として畳み込まれる——実行時のルックアップは発生しない（checker は常にどこへ解決するか知って
いるため）。モジュール修飾された自由名（`mod::name`、`Type::method` を除く）は現状非対応。

## 8. 非局所脱出（catch / throw / unwind-protect）

```lisp
(catch 'tag body)                   ; body を走らせる。body が届く範囲のどこかで
                                    ; (throw 'tag v) が起きたら、その v を値にする
(throw 'tag value)                  ; 直近の動的に囲む (catch 'tag ...) へ脱出する
(unwind-protect protected cleanup)  ; protected をどう抜けても cleanup を走らせる
```

`break`/`return`（§5）と違い、これは**動的**な脱出——`throw` は自分を囲む `catch` を字句的に
見ておらず、関数を何段跨いでも同じタグの `catch` に届く。

```lisp
(defun find-first ((xs Sexpr)) i32
  (catch 'found
    (dolist (x xs)
      (match x ((i32 n) (if (> n 10) (throw 'found n) ())) (_ ())))
    -1))                            ; 見つからなければ通常どおり末尾の値
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

CL のコンディション（`define-condition`/`handler-bind`/`invoke-restart`）は採用していない
（[language-design.md](dev/language-design.md) §9）。

## 9. エラー処理の方針

- 回復可能な失敗は `Result<T,E>` + `match`。回復不能な失敗（バグ・不変条件違反）は `panic`。
- `?`/try に相当する構文はない。分岐は `match` で明示する。
- 関数・特殊形の名前に `!`（破壊的操作）や `?`（述語）を接尾辞として使わない。述語は
  `-p`/`p` 接尾辞（`zerop` `consp` など）または `is-` 前置（`is-some` `is-ok` など）で命名する。

## 10. コンパイル

```lisp
(compile name)                      ; 定義済みの defun/メソッドをネイティブコードへ JIT コンパイル
(compile-file src-path out-path)    ; ソースファイルをネイティブ実行ファイルへ AOT コンパイル
(dump path)                         ; いまの環境（型情報 + コンパイル済み本体）を1ファイルへ
(disassemble name)                  ; その定義が何になるかを印字（既定はホストの機械語、第2引数 true で LLVM IR）
```

`compile` は特殊形で、`name` は評価されず未評価の裸シンボル/`::`パスとして読む（文字列は型エラー）。
ジェネリックな関数は対象にできない——単型化は使用箇所ごとに走るので、単一のコンパイル済み本体が
存在しない。**解決できない名前はチェック時のエラー**であり、実行時まで持ち越されない
（型は在るがそのメソッドが無い場合／型も関数も無い場合／裸の未定義名、で別々のメッセージになる）。
ここでの可視性は他の参照と同じ扱いで、「在るがここからは見えない」は「解決しない」と同じく
チェック時に落ちる。

呼び先も推移的にコンパイルされるので、**コンパイルできない組み込みを（間接的にでも）呼ぶ関数は
コンパイルできない**。プロセスが落ちるのではなく、その旨を述べるエラーで断られる:

```lisp
(defun f ((s string)) string (upcase s))
(compile f)
; => compile: "f" calls "string::upcase", a builtin method with no compiled implementation
```

2026-08-14 に prelude 側の穴を、2026-08-18 にシステム組み込み・等価述語・印字・リーダの
穴を、2026-08-19 に `eval` を塞ぎ、**2026-09-03 に `char` の `upcase` / `downcase` /
`alphap` / `digitp` と整数の `int->char`**（cl-parity-plan.md Phase 2 の残タスク）を
塞いだ。5 つはそれぞれ `rt_char_upcase` / `rt_char_downcase` / `rt_char_alphap` /
`rt_char_digitp` / `rt_int_to_char` を呼ぶ（`externs::native_lowered_primitive_methods`
の `char` 行と整数行）。ASCII 限定という規則は**シムが持っている**ので、コンパイル済みの
答えがインタプリタの答えから離れようがない。`int->char` だけが失敗しうる（Unicode
スカラ値でないコードポイント）ので、これだけが raise する側の呼び出し規約で宣言されている。

**この表は 2026-09-03 に空になった。** 残っていたのは 3 群で、順に:
`string::upcase`/`downcase`（`rt_str_upcase`/`rt_str_downcase`）、`Option` を返す変換
（`try-int->char`・`try-int->W`・`try-float->f32|f64`——判定だけを行う
`rt_int_fits`/`rt_int_fits_char`/`rt_f64_fits_f32` と、島の `build-try-option` が
`some`/`none` の箱を組む）、`bignum` の `ash`/`logbitp`/`logtest`/`logcount`/
`integer-length`。

どれも prelude からは到達しないので `PRELUDE_COMPILE_UNSUPPORTED` には現れなかった
——ユーザーが自分で呼び出しを書いたときだけ出る穴で、`compile_test.rs` の
`the_builtins_that_used_to_block_compilation_now_lower` がその書き方で全部を踏んでいる。

`native_lowered_primitive_methods` と島の `*-native-method?` が一致することは
`the_rust_and_island_native_method_lists_agree` が、**この 2 つがレジストリの組み込み
メソッドを網羅していること**は `every_registered_builtin_method_on_a_native_receiver_lowers`
が保証する。後者は「新しい組み込みを足したら、それを呼ぶ `defun` をコンパイルしてみるまで
穴が空いたか分からない」という状態を無くすために足した——この表が手作業で埋められていた
理由がそれだった。

唯一の例外は `print`/`println` で、これは穴ではない。レジストリは受け手ごとに登録している
（インタプリタの `eval_builtin_method` が受け手で分岐するため）が、チェッカーがメソッド解決の
前に特殊形として横取りする（`Checker::check_print_like`——制御文字列がリテラルでなければ
ならない）ので `Expr::Assoc` になることが無く、lowering を要求されることもない。

`compile`/`compile-file`/`dump`、および `trace`/`untrace`/`step`/`disassemble`
（[functions.md](functions.md) §4.9）はこの表に入らない——定義上インタプリタ専用の操作で、
コンパイルできないのではなくコンパイルする側だから（`dump` が書き出すのはインタプリタの環境
そのもので、AOT 実行ファイルにはその環境が無い。`trace` が見ているのも `step` が止まるのも
走っているインタプリタの呼び出し経路で、`disassemble` は*コンパイラそのもの*）。
これらを呼ぶ `defun` をコンパイルしようとすると、穴の報告ではなく
「`(trace ...)` is an interpreter-only action and cannot itself be compiled」と断られる。
`room`/`dribble`/`ed` は**この族ではない**——ヒープ統計も dribble の sink も実行時のもので、
エディタを起動するのはプロセス呼び出しなので、普通にコンパイルできる。
ジェネリックな関数がコンパイルできないのは
上記のとおり単型化の帰結であって、組み込みの穴ではない。

コンパイル**できる**もの: ストリーム・ファイル I/O、`random`、`gensym`、
`symbol->string`/`string->symbol`、`parse-int`/`parse-float`、`get-universal-time`/
`get-internal-real-time`、`exit`、超越関数、ビット演算、`catch`/`throw`/`unwind-protect`、
`eq`/`eql`/`equal`/`equalp` の4つ全部（`case` もこれで全型でコンパイルできる）、
`print`/`println`/`format`/`pprint` と `pprint-logical-block` を含む印字一式、`read`、
そして `eval`。prelude は事前コンパイル済みで出荷される。

印字・リーダ・フロントエンド（チェッカーとインタプリタ）は、必要な実行ファイルだけが払うように
**独立したクレート**に分けてある（`typelisp-print` / `typelisp-read` / `typelisp-front`）。
リンカはアーカイブのメンバ単位で引くので、印字しないプログラムに書式エンジンは入らない——
`(defun main () i32 42)` の AOT 出力で実測 3,530,224 バイト（印字シンボル 0 個・リーダシンボル
0 個・フロントエンドシンボル 0 個）、同じ出力に `println` を1つ足すと 3,877,648 バイト
（印字 124 個）、`read` を1つ足すと 3,647,960 バイト（リーダ 24 個・印字は 0 個のまま）。

### ダンプ

```lisp
(dump "session.typld")     ; 書き出す
```
```sh
typl --image session.typld prog.typl   # そこから起動する
typl --image session.typld             # REPL も同じ
```

**コンパイラがネイティブ本体を作る場所では、型情報も一緒に作る。** その対を 1 ファイルに
収めたものがダンプで、prelude も、コンパイラ島も、`compile-file` が実行ファイルへ埋め込む
`eval` の環境も、`(dump ...)` の出力も同じ形式である。ファイルは**単位（unit）の並び**で、
各単位が「そのコンパイルが足した検査済み状態」と「その本体のビットコード」を持つ。
ロードは先頭から順に適用するだけ。

`(dump path)` が書くのは、いまのセッションが読み込んだ単位（prelude と島、あるいは
`--image` で渡されたダンプの単位）をそのまま並べたものに、**セッション自身が定義したもの**の
単位を1つ足したもの。だから出力は自己完結していて、`typl --image` で同じ環境が立ち上がる。
セッション中に `(compile f)` したものは、ビットコードとして書き出される。

保存されるのは**定義であって履歴ではない**:

- セッションのトップレベル式（`(println ...)` など）は入らない。ロードで再実行されたら困る。
- グローバル変数は**初期化式を走らせ直した値**で戻る。ダンプ時点の値ではない。
  これは SBCL の `save-lisp-and-die`（ヒープをそのまま書き出す）との意図的な違いで、
  この選択のおかげで「保存できない値」——開いているストリーム、クロージャの関数ポインタ、
  外部メモリ——という問題群がまるごと消える。
- `save-lisp-and-die` と違い、**プロセスは死なない**。書き出しはイメージを壊さないため。

prelude と島の単位は、それを作ったときの `SOURCE` のダイジェストを持っている。読み込む
実行ファイルの `SOURCE` と食い違えばエラーになる（再生成スクリプト名つき）。異なるビルドが
書いたイメージを黙って受け入れることはない。

### AOT 実行ファイルの中の `eval`

`eval` は「現在の大域環境」に対して型検査してから評価する（[functions.md](functions.md) §16）。
その環境——チェッカーが引く署名・型・マクロの表と、インタプリタが実行できる本体——は
**機械語には入っていない**。コンパイル済みの関数はアドレスに置かれたシンボルでしかなく、
引数の型も、名前から本体を引く表も持っていないからである。

そこで `compile-file` は `eval` を呼ぶプログラムに限って、**その環境をコンパイル時に組み立てて
実行ファイルに書き込む**——上のダンプと同じ形式で、prelude の単位（コミット済み成果物から
そのまま）とプログラム自身の単位の2つ。起動時にやるのは復元だけで、ソースを読み直すことも
型検査し直すことも無い。`eval` を呼ばないプログラムには何も足さない。

保存されるのは「型情報」ではなく**検査済み状態**である。署名だけでは `(eval '(f 1))` は型検査を
通ったあと実行するものが無い。だから検査済みトップレベルフォームも一緒に入っていて、復元は
それを実行してインタプリタ側の表を埋める。

帰結:

- **起動に時間がかかる**。`(defun main () i32 0)` に `eval` を1つ足した実行ファイルで実測
  **0.05 秒**（front を最適化ビルドしたとき。参考として `typl` 自身の起動は 1.45 秒）。
  ヒープも大きめ（`1 << 18` セル）に取る。
- **サイズが増える**。同じ比較で 3.53MB → 8.4MB。チェッカーとインタプリタのコード、および
  環境スナップショットのぶん。
- **eval したフォームは解釈実行される**。プログラム自身の関数を呼ぶ形を eval しても、走るのは
  スナップショットが持っている解釈実行用の本体のほう。結果は同じで、速度だけが違う。

グローバル変数の記憶域はコンパイル済みコードと**共有される**（同じスロット）。`defvar` の
初期化子はコンパイル済みの初期化列が1回だけ走らせ、復元のほうはスキップする——副作用のある
初期化子が二度走らないため。

AOT 実行ファイルの `print-object` は `(defmethod print-object ...)` の形でのみ書ける。
`(impl print-object ...)` はトレイト本体が prelude にあり、`compile-file` は prelude を
読まない（コンパイラ島だけを読む）ため——これは以前からの制限で、印字対応で変わっていない。

内部実装（LLVM バックエンド）の詳細は開発用ドキュメント（[docs/dev/](dev/)）を参照。

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

**いつ効くか**は `#.`（§1）と同じで、読み込み経路によって変わる:

- REPL と `(load ...)` は 1 フォームずつ実行するので、**手前のフォームで定義した関数**を
  そのまま登録できる。
- モジュールファイルは単位として検査され実行は後——なので `set-macro-character` /
  `set-dispatch-macro-character` の**呼び出しだけが即時実行される**
  （`project::needs_immediate_exec`、CL の `(eval-when (:compile-toplevel) ...)` の役）。
  即時に走る以上、**渡す関数はその時点で在らねばならない**。同じファイルの `defun` は
  まだ走っていないので、`lambda` で書くか、prelude / 既に走ったものを使う。
  トップレベルの呼び出しだけが対象で、`progn` や `let` の中は見ない。
- 純粋な読み（評価器を渡されていない `Reader`）では、マクロ文字はその旨のエラーになる。

組み込みの `read` / `read-from-string` も readtable を見る（CL と同じ）。コンパイル済みの
実行ファイルでも動くが、そのぶん**チェッカーとインタプリタが実行ファイルに入る**——
`eval` と同じ値段で、理由も同じ（登録した関数を呼ぶのは評価器の仕事）。

**無いもの**: `*readtable*` と `copy-readtable`、および `readtable-case`。前の 2 つは
readtable が**値でない**ため——値なら「リーダに手渡せるもの」でなければならないが、
リーダを呼ぶのは Rust 側のドライバで、渡す先が無い。`readtable-case` は、この言語のリーダが
常に小文字化する（CL の `:downcase`）と§1 で決めているため。
