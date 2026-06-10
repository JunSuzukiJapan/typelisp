# typelisp 文法仕様書

静的型付け Lisp **typelisp** の言語文法を EBNF で厳密に定義する。

---

## 1. 概要

typelisp は以下の性格を持つ言語である。

- **静的型付け**: すべての関数引数・構造体フィールド・メソッドのレシーバは静的型を持つ。
- **Common Lisp 風**: S 式・前置記法・`defun`/`let`/`cond`/`progn` などの特殊形式を備える。
- **全引数型必須**: `defun`/`defmethod`/`lambda` の引数と `defstruct` のフィールドは型注釈が必須。ローカル束縛（`let`/`defvar`/`defconstant`）は型を省略でき、その場合は推論される。
- **GC 前提・参照型なし**: 所有権・借用・ライフタイムは持たない。`&str`/`&T`/`&mut` は文法に存在せず、文字列は単一の `String` 型。
- **型名は Rust 流**: `i32`/`f64`/`bool`/`String`/`Vec<T>` などを直接用いる（参照プロジェクト [macro-lisp](https://github.com/JunSuzukiJapan/macro-lisp) の型構文に準拠）。
- **CLOS 相当は単一ディスパッチ**: `defmethod` によるレシーバ型のみのディスパッチ。CLOS の全引数多重ディスパッチは採用しない。

### EBNF 表記規約

| 記法 | 意味 |
|---|---|
| `::=` | 規則定義 |
| `\|` | 選択 |
| `{ X }` | 0 回以上の繰り返し |
| `[ X ]` | 省略可能（0 または 1 回） |
| `( X )` | グループ化 |
| `"x"` | 終端記号（リテラル） |
| `/regex/` | 文字クラスの略記 |

非終端記号は `lower-kebab` で表記する。空白・コメントはトークン間で読み飛ばされ、トークンとしては現れない。

### 未決定事項（v1 既定値）

以下は本仕様で既定値を採用するが、実装時に変更しうる論点である。

- 整数リテラル既定型 `i64`、浮動小数点既定型 `f64`。サフィックスや文脈で上書きされうる。
- リテラルが制約確定前まで多相かは採らず、既定型に即固定する。
- `match` の網羅性検査の要否は未定（`_` 節を推奨）。
- 多値返却 `(values ...)` は v1 では対象外。

---

## 2. 字句文法

```ebnf
(* --- 字句層トップ --- *)
token         ::= l-paren | r-paren | quote | literal | symbol
l-paren       ::= "("
r-paren       ::= ")"
quote         ::= "'"

(* 空白・コメントはトークン間で読み飛ばす *)
ws            ::= { " " | "\t" | "\r" | "\n" }
comment       ::= line-comment | block-comment
line-comment  ::= ";" { any-char-except-newline } newline
block-comment ::= "#|" { any-char | block-comment } "|#"   (* 入れ子可 *)

(* ---------- リテラル ---------- *)
literal       ::= float-lit | int-lit | char-lit | string-lit | bool-lit | nil-lit

(* 整数: 符号・基数プレフィックス・型サフィックス・桁区切り '_' *)
int-lit       ::= [ sign ] ( dec-int | hex-int | oct-int | bin-int ) [ int-suffix ]
sign          ::= "+" | "-"
dec-int       ::= digit { digit | "_" }
hex-int       ::= "0x" hex-digit { hex-digit | "_" }
oct-int       ::= "0o" oct-digit { oct-digit | "_" }
bin-int       ::= "0b" ( "0" | "1" ) { "0" | "1" | "_" }
int-suffix    ::= "i8"|"i16"|"i32"|"i64"|"isize"|"u8"|"u16"|"u32"|"u64"|"usize"
digit         ::= /[0-9]/
hex-digit     ::= /[0-9a-fA-F]/
oct-digit     ::= /[0-7]/

(* 浮動小数点: '.' か指数を含み整数と区別する *)
float-lit     ::= [ sign ] float-mantissa [ float-exp ] [ float-suffix ]
float-mantissa ::= dec-int "." [ dec-int ] | "." dec-int | dec-int float-exp
float-exp     ::= ("e"|"E") [ sign ] dec-int
float-suffix  ::= "f32" | "f64"

(* 文字: CL 流リーダー構文 #\x （ ' は quote と衝突するため不採用 ） *)
char-lit      ::= "#\" ( named-char | any-single-char )
named-char    ::= "Space" | "Newline" | "Tab" | "Return" | "Nul" | "Backspace"

(* 文字列: 値は単一の String 型 *)
string-lit    ::= "\"" { string-elem } "\""
string-elem   ::= /[^"\\]/ | escape-seq
escape-seq    ::= "\\" ( "n" | "t" | "r" | "\\" | "\"" | "0" | "u{" hex-digit{1,6} "}" )

bool-lit      ::= "true" | "false"
nil-lit       ::= "null" | "nil" | "()"        (* 3 つは同義。() は unit 型・空引数列も兼ねる *)

(* ---------- シンボル / 識別子 ---------- *)
(* シンボルは数値で始まらない構成文字の最長一致 *)
symbol        ::= sym-start { sym-const }
sym-start     ::= /[A-Za-z_]/ | sym-op-start
sym-const     ::= /[A-Za-z0-9_\-]/ | "::" | "." | sym-op-start
sym-op-start  ::= "+"|"-"|"*"|"/"|"%"|"<"|">"|"="|"!"

(* 演算子トークンはリーダーでは通常シンボルとして読み、意味は構文層で与える *)
operator-sym  ::= "==" | "!=" | "<" | ">" | "<=" | ">=" | "="
                | "+" | "-" | "*" | "/" | "%" | "1+" | "1-" | "incf" | "decf" | "!"

ident         ::= /[A-Za-z_][A-Za-z0-9_]*/
```

> **`<` `>` の多義性**: `<`/`>` は比較演算子であると同時にジェネリック型の括弧でもある。**型位置**（パラメータ型・フィールド型・戻り型・明示束縛の型・ジェネリック引数）では `<...>` をジェネリック括弧として解釈し、**式位置**では `<`/`>` を比較演算子として解釈する。`Vec<T>` のように空白を含まない型は 1 トークンとして読まれるため曖昧にならず、`(< a b)` は式位置の比較として保たれる。

---

## 3. 型文法

参照型・借用は存在しない。文字列は単一の `String` 型のみ。

```ebnf
type          ::= prim-type | generic-type | fn-type | tuple-type | unit-type | path-type

unit-type     ::= "()"                          (* unit / 値なしを表す型 *)

prim-type     ::= int-type | float-type | "bool" | "char" | "String"
int-type      ::= "i8"|"i16"|"i32"|"i64"|"isize"|"u8"|"u16"|"u32"|"u64"|"usize"
float-type    ::= "f32" | "f64"

path-type     ::= ident { "::" ident }          (* MyStruct, std::string::String, 型変数 T *)
generic-type  ::= path-type "<" type { "," type } ">"   (* Vec<T>, Option<i32>, HashMap<K,V> *)

(* 関数型シグネチャ: S 式 (fn (引数型...) 戻り型) *)
fn-type       ::= "(" "fn" "(" { type } ")" ret-type ")"

(* タプル型: S 式 (tuple 型...) *)
tuple-type    ::= "(" "tuple" type { type } ")"

ret-type      ::= type                          (* "()" は値を返さないことを表す *)
```

**プリミティブ型の閉集合:**

```
i8 i16 i32 i64 isize  u8 u16 u32 u64 usize  f32 f64  bool char String
```

標準ジェネリック容器（`generic-type` により拡張可能）: `Vec<T>` `Option<T>` `Result<T, E>` `Box<T>` `HashMap<K, V>`。unit 型は `()`。

---

## 4. 構文文法

### 4.1 プログラム / トップレベル

```ebnf
program       ::= { toplevel }

toplevel      ::= defun | defstruct | defmethod
                | defvar | defconstant
                | use-form | module-form
                | expr
use-form      ::= "(" "use" path-type ")"
module-form   ::= "(" [ "pub" ] "module" ident { toplevel } ")"
```

### 4.2 関数定義 `defun`

戻り型は引数リストの直後に置く。引数なしは `()`、値を返さない場合の戻り型は `()`。

```ebnf
defun         ::= "(" [ "pub" ] "defun" ident param-list ret-type { expr } ")"
param-list    ::= "(" { typed-param } ")"
typed-param   ::= "(" symbol type ")"           (* 型は必須 *)
```

### 4.3 構造体定義 `defstruct`

```ebnf
defstruct     ::= "(" [ "pub" ] "defstruct" struct-name
                    [ pub-field-group ] [ priv-field-group ] ")"
struct-name   ::= ident [ generic-params ]
generic-params ::= "<" ident { "," ident } ">"  (* Point<T>, Pair<K, V> *)
pub-field-group  ::= "(" "pub" "(" { field } ")" ")"
priv-field-group ::= "(" "(" { field } ")" ")"
field         ::= "(" symbol type ")"           (* フィールド型は必須 *)
```

両グループとも省略可。フィールドなしのユニット構造体は `(defstruct Name)`。

### 4.4 メソッド定義 `defmethod`（CLOS 相当・単一ディスパッチ）

第一引数の形でインスタンス／スタティックを区別する。**この 2 形式以外の第一引数はエラー**。

```ebnf
defmethod     ::= "(" [ "pub" ] "defmethod" ident method-params ret-type { expr } ")"
method-params ::= "(" receiver-param { typed-param } ")"
receiver-param ::= "(" "self" type ")"          (* インスタンスメソッド: self 固定 *)
                | "(" type ")"                   (* スタティックメソッド: 型のみ *)
```

- **インスタンスメソッド** `(self type)`: レシーバ名は `self` 固定。`self` 以外の名前を持つ 2 要素の第一引数はエラー。
- **スタティックメソッド** `(type)`: 型のみの 1 要素。`(type)` はレシーバ型を宣言するだけで実引数にはならない。
- **レシーバ型はプリミティブ型も許可**（`(self i32)` 等）。
- **呼び出しは通常関数呼び出しと同形**（[4.14](#414-関数呼び出し-function-call) 参照）。インスタンスは第一実引数のレシーバ型で、スタティックは名前＋残り引数型で静的解決する。
- 同名メソッドを複数のレシーバ型に定義可（単一ディスパッチ）。スタティックの同名がレシーバ型違いで衝突し、残り引数型でも一意に解決できない場合はエラー。

### 4.5 変数定義 `defvar` / `defconstant`（型省略可）

```ebnf
defvar        ::= "(" "defvar" var-binding ")"          (* 可変 *)
defconstant   ::= "(" "defconstant" var-binding ")"     (* 不変 *)
var-binding   ::= "(" symbol type ")" expr              (* 明示 *)
                | symbol expr                            (* 推論 *)
```

### 4.6 ラムダ `lambda`

引数は `defun` と同じく型必須。戻り型は書かず推論される。`move` 変種あり。

```ebnf
lambda        ::= "(" "lambda" [ "move" ] param-list { expr } ")"
```

### 4.7 局所束縛 `let`（型省略可）

```ebnf
let-form      ::= "(" "let" "(" { binding } ")" { expr } ")"
binding       ::= "(" symbol expr ")"                   (* 推論 *)
                | "(" "(" symbol type ")" expr ")"      (* 明示 *)
```

### 4.8 条件分岐

```ebnf
if-form       ::= "(" "if" expr expr [ expr ] ")"       (* then [ else ] *)
when-form     ::= "(" "when" expr { expr } ")"
unless-form   ::= "(" "unless" expr { expr } ")"
cond-form     ::= "(" "cond" { cond-clause } ")"
cond-clause   ::= "(" expr { expr } ")"                 (* (test body...)。test に true で既定節 *)
```

### 4.9 パターンマッチ `match`

```ebnf
match-form    ::= "(" "match" expr { match-arm } ")"
match-arm     ::= "(" pattern { "|" pattern } "=>" "(" { expr } ")" ")"
pattern       ::= literal | symbol | "_" | range-pat | struct-pat | tuple-pat
range-pat     ::= int-lit ".." [ "=" ] int-lit
struct-pat    ::= path-type "(" { pattern } ")"
tuple-pat     ::= "(" "tuple" { pattern } ")"
```

### 4.10 反復

```ebnf
loop-form     ::= "(" "loop" { expr } ")"               (* 無限ループ。break で脱出 *)
while-form    ::= "(" "while" expr { expr } ")"
dotimes-form  ::= "(" "dotimes" "(" symbol expr ")" { expr } ")"   (* (dotimes (i n) body) *)
```

### 4.11 逐次実行 `progn`

```ebnf
progn-form    ::= "(" "progn" { expr } ")"
```

### 4.12 代入 `setf`

```ebnf
setf-form     ::= "(" "setf" place expr ")"
place         ::= symbol | field-access | index-access
field-access  ::= "(" "." object symbol ")"             (* フィールド参照 *)
index-access  ::= "(" "aref" object expr ")"            (* 添字参照 *)
```

### 4.13 クォート `quote`

```ebnf
quote-form    ::= "(" "quote" datum ")" | "'" datum
datum         ::= literal | symbol | "(" { datum } ")"
```

### 4.14 演算子と関数呼び出し

```ebnf
arith-form    ::= "(" arith-op expr expr ")"
                | "(" ( "1+" | "1-" ) expr ")"
                | "(" ( "incf" | "decf" ) place ")"
arith-op      ::= "+" | "-" | "*" | "/" | "%"
compare-form  ::= "(" compare-op expr expr ")"
compare-op    ::= "==" | "!=" | "<" | ">" | "<=" | ">="
not-form      ::= "(" "!" expr ")"

function-call ::= "(" call-target { expr } ")"
call-target   ::= symbol                                (* (factorial n) *)
                | path-type                              (* (std::process::exit 0) *)
                | lambda                                 (* 即時適用ラムダ *)
method-call   ::= "(" "." object symbol { expr } ")"    (* (.method obj args...) *)
```

### 4.15 式 `expr`（統合非終端）

```ebnf
expr          ::= literal | symbol
                | let-form | if-form | when-form | unless-form | cond-form
                | match-form | loop-form | while-form | dotimes-form
                | progn-form | setf-form | quote-form | lambda
                | defvar | defconstant
                | arith-form | compare-form | not-form
                | method-call | function-call
```

---

## 5. 型必須 vs 型省略可の区別（形式的対比）

```ebnf
(* 型が必須の位置: defun/lambda の引数、構造体フィールド、defmethod の非レシーバ引数 *)
typed-param   ::= "(" symbol type ")"

(* 型が省略可の位置: let の局所束縛、defvar/defconstant *)
binding       ::= "(" symbol expr ")"                   (* 推論 *)
                | "(" "(" symbol type ")" expr ")"      (* 明示 *)
```

両者は構文的に交わらない。

- `typed-param` `(sym type)` は第 2 要素が**型**。
- `let` 推論束縛 `(sym expr)` は第 2 要素が**式**。
- `let` 明示束縛 `((sym type) expr)` は型付きターゲットを入れ子にする。

パーサは「アリティ（要素数）」と「第 2 要素が型として読めるか式として読めるか」で曖昧性を解消する。`defmethod` の `receiver-param` も同様に、`(self type)`（2 要素）・`(type)`（1 要素）のアリティで判別する。

---

## 6. 各形式の実例

### defun / factorial

```lisp
(defun factorial ((n i32)) i32
  (if (<= n 1)
      1
      (* n (factorial (- n 1)))))

(defun main () ()                         ; 引数なし・戻り値なし(unit)
  (defconstant num (factorial 10))
  (println "10! = {}" num))
```

### defstruct（pub / private / ジェネリクス）

```lisp
(defstruct Point
  (pub ((x f64) (y f64)))                 ; 公開フィールド群
  (((label String))))                     ; 非公開フィールド群

(defstruct Pair<K, V>
  (pub ((key K) (value V))))
```

### defmethod（インスタンス・スタティック・エラー例）

```lisp
;; インスタンスメソッド: 第一引数 (self Circle)
(defmethod area ((self Circle)) f64
  (* 3.14159 (* (.radius self) (.radius self))))

;; スタティックメソッド: 第一引数 (Circle) — 型のみ
(defmethod make ((Circle) (r f64)) Circle
  (Circle r))

;; プリミティブ型のレシーバも可
(defmethod double ((self i32)) i32
  (* self 2))

;; 呼び出しは通常関数と同形
(area c)        ; インスタンス: c のレシーバ型 Circle でディスパッチ
(make 1.0)      ; スタティック: 名前 make + 引数型 f64 で解決
(double 21)     ; => 42

;; ▼ エラーになる第一引数の例
;; (defmethod bad ((x Circle)) f64 ...)  ; self 以外の 2 要素 → エラー
;; (defmethod bad ((self)) f64 ...)      ; self は型でない 1 要素 → エラー
```

### lambda（通常・move・即時適用）

```lisp
(defvar add1  (lambda ((x i32)) (+ x 1)))
(defvar adder (lambda move ((x i32)) (+ x captured)))
((lambda ((x i32)) (* x x)) 5)            ; 即時適用 => 25
```

### let（型あり・なし混在）

```lisp
(let ((x 5)                                ; 推論（既定 i64）
      ((y i64) 10)                         ; 明示
      (name "lisp"))                       ; 推論（String）
  (+ x y))
```

### cond

```lisp
(cond
  ((< n 0)  "negative")
  ((== n 0) "zero")
  (true     "positive"))                   ; true 節 = 既定
```

### loop / while / dotimes

```lisp
(dotimes (i 100)
  (println "{}" (1+ i)))

(while (< n 10)
  (incf n))

(loop
  (when (done) (break))
  (step))
```

### match

```lisp
(match b
  (0x20 | 0x09 | 0x0a => (true))
  (_                  => (false)))
```

### 関数型シグネチャ（高階関数）

```lisp
(defun apply-twice ((f (fn (i32) i32)) (x i32)) i32
  (f (f x)))
```

---

## 7. 静的型付け規則

1. **引数・フィールド・レシーバは型必須**。`defun`/`defmethod`/`lambda` のすべての引数、`defstruct` のすべてのフィールドは `(name type)` で型注釈する。引数なしは `()`。
2. **戻り型は必須**で引数リストの直後に書く。値を返さない場合は `()`（unit）。`defun`/`defmethod` の戻り型は推論しない（宣言の一部）。`lambda` の戻り型のみ推論。
3. **`let`/`defvar`/`defconstant` は型省略時に推論**。明示形 `((name type) value)` は初期化子が指定型に代入可能かを検査する。
4. **リテラル既定型**: サフィックスなし整数は `i64`、サフィックス付き（`5u8`）はその型、サフィックスなし浮動小数点は `f64`、文字列は `String`、文字は `char`、`true`/`false` は `bool`、`null`/`nil`/`()` は unit 型。文脈の期待型が既定型を上書きする（例: `(defvar (x u8) 5)` で `5 : u8`）。
5. **推論は限定的に双方向**: 未注釈 `let` 束縛の型は初期化子の型、関数呼び出しの実引数は呼び先の宣言引数型で検査、リテラル既定型は文脈の期待型で上書き。
6. **可変性**: `defvar` は可変束縛、`defconstant` は不変束縛を導入。`setf`/`incf`/`decf` は可変な place を要求する。
7. **分岐型の単一化**: `if`/`cond`/`match` を値として用いる場合、全分岐の型が共通型に単一化されねばならない。片腕 `if`・`when`・`unless` は unit 型。`match` は網羅的である（または `_` を含む）べき。
8. **メソッドディスパッチ**: `defmethod` はレシーバ型のみの単一ディスパッチ。インスタンスメソッドは第一実引数の静的型、スタティックメソッドは名前＋残り引数型で静的に解決する。
9. **ジェネリック構造体**: `defstruct Name<T>` の型変数 `T` はフィールド型に束縛され、インスタンス化時に解決される。

---

## 8. 実装対応表（付録）

現状のリーダーは `Object`（`src/read/object.rs`: `Null, True, False, Int(i64), Symbol(String), String(String), List(Cons<Object>)`）を生成し、AST `Expr`（`src/compile/ast/expr.rs`）は最小限、`Function.params: Vec<String>` は型なし。各構文の対応を以下に示す。

| 構文 | リーダー `Object` の変更 | AST `Expr` / `Function` の変更 |
|---|---|---|
| `float-lit` / `char-lit` | `Object::Float(f64)` `Object::Char(char)` を追加。`read_number` を `.`/`e`/サフィックス/基数 `0x 0o 0b`/`_` に対応拡張。`#\` 分岐を追加 | `Expr::Float(f64)` `Expr::Char(char)` |
| 整数サフィックス・基数 | `read_number` を拡張し幅情報を保持 | リテラルに `Type` を付随 |
| `bool` / `nil` | 既存の `true/false/null` 処理に `nil`・`()` 別名を追加 | 既存 `Expr::True/False/Null` |
| 型表現（§3） | 新規 `Type` enum（`I8..U64, Isize, Usize, F32, F64, Bool, Char, String, Generic(String, Vec<Type>), Fn(Vec<Type>, Box<Type>), Tuple(Vec<Type>), Unit, Path(Vec<String>), Var(String)`）を `Object` のリスト/シンボルから構築 | params/fields が `Type` を参照 |
| `typed-param` `(name type)` | 2 要素 `Object::List` として読む | `Function.params: Vec<String>` → `Vec<(String, Type)>`、`ret: Type` を追加 |
| `defun` | シンボル先頭リスト | `Expr::Defun { name, params, ret, body }` |
| `defstruct` | リスト形式 | `Expr::DefStruct { name, generics, pub_fields, priv_fields }` ＋ 構造体定義レジストリ |
| `defmethod` | シンボル先頭リスト。第一引数を `receiver-param` として特別扱い | `Expr::DefMethod { name, recv: Receiver, params: Vec<(String, Type)>, ret: Type, body }`。`Receiver::Instance(Type)` / `Receiver::Static(Type)`。`(self type)`/`(type)` 以外はパース時エラー。メソッド表 `(Type, name) → 実装` を保持し、呼び出し時にレシーバ型で静的解決 |
| `defvar` / `defconstant` | リスト形式 | `Expr::DefVar/DefConstant { target: (String, Option<Type>), init, mutable }` |
| `lambda` / `move` | リスト形式 | `CallLambdaFunction` を拡張し `Expr::Lambda { move_, params, body }` |
| `let`（型省略可） | リスト形式 | `Expr::Let { bindings: Vec<(String, Option<Type>, Expr)>, body }` |
| `if/when/unless/cond/match` | リスト形式 | 各 `Expr` バリアントを新設 |
| `loop/while/dotimes/progn` | リスト形式 | 各 `Expr` バリアントを新設 |
| `setf/incf/decf` | リスト形式 | `Expr::Setf { place, val }` 等 |
| `quote` | 既存の `(quote …)` 構築（`src/read/reader.rs`） | `Expr::Quote(Datum)` |
| 算術・比較 | 既存（`+` のみ） | `Expr::Plus` を `Expr::BinOp(Op, Box<Expr>, Box<Expr>)` / `Expr::Unary` に一般化 |
| `function-call` | シンボル/path 先頭リスト | 既存 `CallFunction` を path ターゲット・メソッド呼び出しに拡張 |

このほか、空の `src/type_inference/` を実装し、リーダー周辺の型構文解析・AST・inkwell コード生成で共有する `Type` 表現が必要（現状は `+` のみが LLVM へ降りる）。

対象ファイル: `src/read/reader.rs`, `src/read/object.rs`, `src/compile/ast/expr.rs`, `src/compile/ast/make_ast.rs`。型構文の出典は `/Users/suzukijun/Program/Rust/macro-lisp/src/lib.rs`。
