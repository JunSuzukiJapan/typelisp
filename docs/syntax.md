# typelisp 構文リファレンス

typelisp は静的型付きの Lisp。文法は S 式。組み込み関数・メソッドの一覧は
[functions.md](functions.md) を参照。

## 1. 字句要素

- **大文字小文字は区別しない**。シンボルは読み取り時にすべて小文字へ正規化される。
- **コメント**: `;` で始まり行末まで（行コメント）。`#| ... |#`（ネスト可能なブロックコメント）。
- **真偽値**: `true` / `false`。
- **整数**: 10進（`42`, `-7`）と `0x` 接頭辞の16進（`0xff`）。符号 `+`/`-` を前置可能。
  型注釈のない整数リテラルは既定で `i32`（期待される型が他の整数型ならその型になる）。
- **浮動小数点数**: 小数点または指数表記（`e`/`E`）を含むもの（`1.5`, `3.0e10`）。
  既定で `f64`（期待される型が `f32` ならその型になる）。
- **文字**: `#\` に続けて1文字、または名前付き文字。例: `#\a` `#\Space` `#\Newline`
  `#\Tab` `#\Return` `#\Page` `#\Nul`（`#\Null` も可）`#\Backspace`。名前は大文字小文字を区別しない。
- **文字列**: `"..."`。エスケープは `\n` `\t` `\r` `\0` `\\` `\"`（それ以外の `\x` はそのまま `x`）。
- **シンボル**: 英数字・記号を含む任意のトークン（`+` `<=` `my-func` など）。
- **リスト**: `(a b c)`。ドット対 `(a . b)` も読み取り可能。
- **空リスト `()`**: 文脈によって `Unit` 型の値、または `Sexpr` 型の `Nil` になる。
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

- **プリミティブ型**: `i8` `i16` `i32` `i64` `isize` `u8` `u16` `u32` `u64` `usize`
  `f32` `f64` `bool` `char` `string`
- **Unit 型**: `()`
- **Never 型**: `!`（`panic`/`unreachable`/`todo`/`return`しないループ 等、発散する式の型。
  任意の期待型に適合する）
- **関数型**: `(fn (引数型...) 戻り値型)`。関数型は常に固定アリティ
  （値レベルの可変長関数は無い。`&rest` は `defmacro` 専用）。
- **ジェネリック型**: `Name<T1,T2,...>`（空白なしの1トークンとして読み取られ、内部で分解される）。
  例: `Option<i32>` `Result<i32,Error>` `HashTable<string,i32>` `Vector<T>`。
- **修飾型名**: `module::Type` のように `::` で修飾できる。
- 組み込みジェネリック型: `Option<T>`（`Some(T)` / `None`）、`Result<T,E>`（`Ok(T)` / `Err(E)`）、
  `Error`、`Sexpr`、`HashTable<K,V>`、`Vector<T>`。詳細は functions.md を参照。

## 3. トップレベル定義

### defun — 関数定義

```lisp
(defun name ((arg1 Type1) (arg2 Type2) ...) RetType
  body...)
```

- 引数の型・戻り値の型は必須。
- ジェネリック関数は名前に山括弧で型パラメータを書く: `(defun name<T1,T2...> (params) Ret body...)`
  （型位置の `Vector<T>` と同じ山括弧構文。旧来の `(name T1 T2...)` リスト形式は廃止）。
- `defun`/`lambda` は固定アリティのみ。可変長引数（`&rest`）は `defmacro` 専用で、
  `defun`/`lambda`/`fn` 型では使えない。
- トレイト境界を要求する場合は本体の直前に `where` 節を書く:
  `(defun name<T> (params) Ret (where (Trait T (AssocName ConcreteType)...)) body...)`
  （`(AssocName ConcreteType)` による関連型の固定は省略可能）。

### defvar / defconstant — グローバル変数

```lisp
(defvar (name Type) init-expr)
(defconstant (name Type) init-expr)
```

型注釈は必須（初期化式から推論しない）。`defvar` は書き換え可能、`defconstant` は不可（`setf` でエラー）。

### defmethod — メソッド定義

```lisp
; インスタンスメソッド: (m obj args...) の形で呼べる
(defmethod name ((self Type) (arg Type2) ...) RetType body...)

; static / 関連関数: (Type::name args...) の形で呼べる
(defmethod name (Type (arg Type2) ...) RetType body...)
```

呼び出し側は `obj` の静的型からメソッドを解決する（単一・静的ディスパッチ）。

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
  `pub` とは独立）。
- 自動的に次が生成される:
  - コンストラクタ `Name::new`（フィールド順に引数を渡す）
  - ゲッター `(field-name instance)`、糖衣構文 `instance::field-name`
  - セッター `(set-field-name instance value)`、糖衣構文 `(setf instance::field-name value)`
- 構造体自体を `pub` にするには `(pub defstruct ...)` のように先頭に `pub` を付ける。

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

### deftrait / impl — トレイト機構

```lisp
(deftrait TraitName
  (type AssocName)                       ; 関連型（複数可、省略可）
  (method-name ((self Self) params...) RetType))  ; メソッドシグネチャ（本体なし）

(impl TraitName TargetType
  (type AssocName ConcreteType)          ; 関連型を具体化
  (method-name (recv params...) RetType body...))
```

`impl` によって各メソッドは `TargetType` の通常の `defmethod` として登録される。ジェネリック関数の
`where` 節でトレイト境界として参照する（§ defun 参照）。

### module / use — 名前空間

```lisp
(module path body...)   ; path は foo または foo::bar のようなセグメント列
(use path)              ; 関数・型・モジュールをカレント名前空間へエイリアス導入
```

- `module` は名前空間を作る。**型は名前空間ではない**（Rust と同様、型は関連関数/メソッドを持つのみ）。
- `use` で型を導入すると、その型のコンストラクタと公開 static メソッドも裸名で使えるようになる
  （例: `(use option)` の後は `some`/`none` を `option::some`/`option::none` なしで呼べる）。
- 裸名（修飾なしの識別子）の解決順序: 特殊形 → コンストラクタ → 自由関数（現在の名前空間 → ルート）
  → インスタンスメソッド（第一引数の静的型から解決）。中間の親モジュールへは遡らない。
- 修飾パス `a::b` は `a` を上記の順序で解決し、モジュールなら内部を辿り、型なら最終セグメントを
  関連項目として解決する。

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
(defmacro name (p1 p2 ... &rest rest-name) body...)
```

- 全パラメータ・戻り値は常に `Sexpr` 固定なので型注釈は書かない。
- CL 流の非衛生的マクロ（`gensym` で衝突を避けるのはマクロ作者の責任）。
- 末尾 `&rest name` で可変長引数（複数フォームをまとめて1つの `Sexpr` リストとして受け取る）に対応。

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
  (key2 body...)
  (else body...))                   ; expr は一度だけ評価。key は equal で比較。
                                     ; シンボルを key にするときは 'sym と quote する。defmacro
(and expr...)                       ; 短絡評価。0引数なら true。defmacro
(or expr...)                        ; 短絡評価。0引数なら false。defmacro
(progn body...)                     ; 順次実行、最後の値を返す
(the Type expr)                     ; 型注釈（実行時の効果なし）
```

### match — パターンマッチ

```lisp
(match expr
  (pattern body...)
  ...)
```

パターンの種類:
- `_` — ワイルドカード
- 変数名 — 束縛パターン（常にマッチ）
- 整数リテラル / `true`/`false` / 文字リテラル — リテラルパターン
- `(Ctor sub-pattern...)` — コンストラクタパターン（`Some x` `None` `Cons a d` `Ok v` など）

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
(doiter (var coll-expr) body...)    ; Iter トレイトを実装する値を反復。defmacro

(break)                             ; 直近のループのみを抜ける。値は常に Unit
(return)                            ; 直近のループのみを抜ける
(return value)                      ; 値を伴って直近のループを抜ける
```

`break`/`return` はどちらも **直近の囲むループのみ** を脱出する（関数の早期リターンではない。
`lambda` の境界は越えられない）。`loop` の型は内部で見つかった `break`/`return` の値型の合流型
（一度も脱出しなければ `!`）。

## 6. 関数値・呼び出し

```lisp
(lambda (params) RetType body...)   ; 第一級関数値（クロージャ）を作る
(labels ((name (params) RetType body...) ...) body...)   ; 相互再帰可能なローカル関数定義
```

名前付き関数もそのまま値として渡せる（高階関数への引数など）。

## 7. その他の特殊形

```lisp
(setf place value)                  ; 変数への代入。place は変数名または var::field
(list e1 e2 ... en)                 ; (cons e1 (cons e2 (... (Nil)))) への展開。0引数なら Nil
(quote datum)                       ; 'datum と同義。評価せず Sexpr データとして返す
(quasiquote template)               ; `template と同義。,/,@ でテンプレート内に式を埋め込む
(panic message)                     ; message: string。回復不能なエラーで異常終了。型は !
(unreachable)                       ; (panic "unreachable") に展開。defmacro
(todo)                              ; (panic "todo") に展開。defmacro
```

## 8. エラー処理の方針

- 回復可能な失敗は `Result<T,E>` + `match`。回復不能な失敗（バグ・不変条件違反）は `panic`。
- `?`/try に相当する構文はない。分岐は `match` で明示する。
- 関数・特殊形の名前に `!`（破壊的操作）や `?`（述語）を接尾辞として使わない。述語は
  `-p`/`p` 接尾辞（`zerop` `consp` など）または `is-` 前置（`is-some` `is-ok` など）で命名する。

## 9. コンパイル（実験的機能）

```lisp
(compile name)                      ; 定義済みの defun/メソッドをネイティブコードへ JIT コンパイル
(compile-file src-path out-path)    ; ソースファイルをネイティブ実行ファイルへ AOT コンパイル
```

ジェネリックな関数は対象にできない。内部実装（LLVM バックエンド）の詳細は開発用ドキュメント
（[docs/dev/](dev/)）を参照。
