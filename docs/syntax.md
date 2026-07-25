# typelisp 構文リファレンス

typelisp は静的型付きの Lisp。文法は S 式。組み込み関数・メソッドの一覧は
[functions.md](functions.md) を参照。

## 1. 字句要素

- **大文字小文字は区別しない**。シンボルは読み取り時にすべて小文字へ正規化される。
- **コメント**: `;` で始まり行末まで（行コメント）。`#| ... |#`（ネスト可能なブロックコメント）。
- **真偽値**: `true` / `false`。
- **整数**: 10進（`42`, `-7`）と `0x` 接頭辞の16進（`0xff`）。符号 `+`/`-` を前置可能。
  型注釈のない整数リテラルは既定で `i32`（期待される型が他の整数型ならその型になる）。
  `i64` の範囲を超える整数リテラルは（10進・16進とも）自動的に `bignum` になる
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
- **多倍長数値型**: `bignum`（任意精度整数）、`ratio`（既約な有理数）。CL 準拠でヒープ確保され、
  `i32`/`f64` 等との暗黙変換はない（`as`/`try-as` または変換メソッドで明示。functions.md 参照）。
- **Unit 型**: `()`
- **Never 型**: `!`（`panic`/`unreachable`/`todo`/`return`しないループ 等、発散する式の型。
  任意の期待型に適合する）
- **関数型**: `(fn (引数型...) 戻り値型)`。可変長引数を持つ関数型は
  `(fn (引数型... &rest 要素型) 戻り値型)`。
- **ジェネリック型**: `Name<T1,T2,...>`（空白なしの1トークンとして読み取られ、内部で分解される）。
  例: `Option<i32>` `Result<i32,ParseIntError>` `HashTable<string,i32>` `Vector<T>`。
- **修飾型名**: `module::Type` のように `::` で修飾できる。
- **trait オブジェクト型**: `:dyn Trait`（空白区切りの2語で1つの型）。実行時に具象型が決まる値を
  表し、trait のメソッド呼び出しは vtable 経由の動的ディスパッチになる。関連型を持つ trait は
  宣言順に位置指定で固定する（`:dyn Iter<i32>` は `Item` を `i32` に固定）。ジェネリック引数の
  内側にも書ける: `Vector<:dyn Drawable>` `HashTable<string, :dyn Drawable>`。
  具象値は期待位置で自動的に箱詰めされ、明示形は `(as :dyn Trait 式)`。
  `:dyn` を型位置以外に書くとエラー。詳細は [dev/language-design.md](dev/language-design.md) §5.2。
- 組み込みジェネリック型: `Option<T>`（`Some(T)` / `None`）、`Result<T,E>`（`Ok(T)` / `Err(E)`）、
  `Sexpr`、`HashTable<K,V>`、`Vector<T>`。組み込みの具象エラー型は `ParseIntError` /
  `ParseFloatError` / `ReadError` / `EvalError`（`Error` は型ではなく prelude のトレイト
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
- `defun`/`lambda` は末尾に `&rest (name Type)` を書くと可変長引数を受け取れる:
  `(defun name ((a Type1) &rest (xs Type2)) Ret body...)`（本体内では `xs` は常に `Sexpr` の
  リストとして束縛される。呼び出し側の各実引数は `Type2` として個別に型検査される）。
  `defmacro` にも独自の `&rest` があるが、常に無型の `Sexpr` である点が異なる（`defun`/`lambda`
  は要素型を明示する）。`fn` 型でも `(fn (T1... &rest Te) Ret)` の形で可変長関数の型を書ける。
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

`prelude.rs` は標準トレイト **`Iter`**（`next`／関連型 `Item`。`doiter`／シーケンス関数の基盤）・
**`Eq`**（`equals`／`not-equals`）・**`Ord`**（`less`／`less-equal`／`greater`／`greater-equal`）を
提供し、主要なスカラ型と `cons-cell<A,B>` に実装済み（詳細は [functions.md](functions.md) §12・§12.1）。
自前のコレクション型に `Iter` を `impl` すれば `doiter`（§5）や `map`／`filter`／`sort` 等がそのまま使える。

トレイトの呼び出しは既定で**静的**（レシーバの静的型で解決）。実行時に具象型が決まる値を扱いたい
場合は trait オブジェクト型 `:dyn Trait`（§2）を使うと vtable 経由の動的ディスパッチになる:

```lisp
(deftrait Drawable (draw ((self Self)) string))
(defstruct circle (r i32))
(defstruct square (side i32))
(impl Drawable circle (draw ((self Self)) string "circle"))
(impl Drawable square (draw ((self Self)) string "square"))

(defun render-all ((xs Vector<:dyn Drawable>)) ()
  (doiter (d (iter xs)) (println "~a" (draw d))))   ; 1つの呼び出し地点、実装ごとの答え
```

`:dyn Trait` にできるのは「全メソッドが `self` レシーバを持ち、`Self` をレシーバ以外に使わず、
メソッド自身がジェネリックでも可変長でもない」トレイトだけ。箱に入れられるのはヒープ表現を持つ型
（`defstruct`/`defenum` 等）で、プリミティブ型は入れられない。詳細と設計理由は
[dev/language-design.md](dev/language-design.md) §5.2。

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
- **cons アリーナ容量**: `typl --heap-cells N` で cons セルの固定アリーナ容量を指定できる（既定
  65536。`--heap-cells=N` 形も可、`run`/REPL/`compile-module` 共通）。アリーナは起動時確保・
  再成長しないため、大量のリスト処理で `heap exhausted` になる場合はここで増やす。

### load — コンパイル済み優先ロード（fasl）

```lisp
(load "path")   ; トップレベル専用。path は文字列リテラル
```

- CL 流の**フラットロード**: 対象ファイルのフォームを**カレント名前空間**にそのまま読み込む
  （`use` のようにモジュールで包まない）。トップレベル専用（関数本体内は型エラー）。
- **コンパイル済み（fastl）優先**: `path.fastl` があり、その `source_hash` が `path.typl` の
  現在の内容と一致すれば（または `.typl` が無ければ）、fasl を直接ロードする——read・マクロ展開・
  型チェックをすべてスキップ。無い/古い場合は `.typl` ソースを読む（**自動コンパイルはしない**）。
- **fasl の生成**: `typl compile-module <file.typl> [-o <out.fastl>]` でチェック済み状態を
  fastl（`.fastl` 拡張子）に書き出す。fasl はネイティブコードではなく「チェック済み定義のシリアライズ」
  （LLVM の `(compile ...)`/`compile-file` とは無関係の別機構）。モジュールは定義のみで、
  トップレベル式を含むとエラー。
- ロード時、ヒープ上の値（マクロ本体の quote 等）はランタイムの確保 API（`heap.cons` 等）で
  作り直されるので、生ポインタのコピーは発生しない。
- prelude 自身もこの機構で起動時ロードされ（専用ディレクトリ `$TYPL_CACHE_DIR` または
  `~/.typl/cache/` にキャッシュ——共有の `~/.cache` は使わない）、LSP は
  prelude fasl を1度だけ構築して各診断パスで再利用する（キー入力毎の再チェックが消える）。

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

`Sexpr` スクルーティニーに対しては、上記の組み込み11変種パターンに加えて **downcast パターン**
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

downcast パターンを使う `match` の網羅性チェックは、`Sexpr` 本来の11変種のカバレッジには数えない
（downcast パターンだけを並べた `match` は `_` で閉じる必要がある）。ジェネリックな ADT
（`defstruct point<T> ...` など）は downcast パターンの型引数を推論できないため、フィールド分解形
（`(point ...)`)/裸変種形は使えず、`(the point<i32> p)` のように `the` で明示する。

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
(apply f arg1 ... argN rest-list)   ; f（&rest を持つ可変長関数）を rest-list を展開して呼ぶ
```

名前付き関数もそのまま値として渡せる（高階関数への引数など）。

## 7. その他の特殊形

```lisp
(setf place value)                  ; 変数への代入。place は変数名または var::field
(list e1 e2 ... en)                 ; (cons e1 (cons e2 (... (Nil)))) への展開。0引数なら Nil
                                     ; 各要素は Sexpr へ暗黙変換される（CL のcons同様、任意の値を
                                     ; 保持できる）: スカラ(i32/f64/bignum/ratio/char/bool/string/
                                     ; symbol)は対応する Sexpr コンストラクタでラップ、defstruct/
                                     ; defenum/Vector<T>/HashTable<K,V> 等ヒープ表現ADTは無変換の
                                     ; まま retype（実行時コストなし）。&rest/format引数も同様。
(quote datum)                       ; 'datum と同義。評価せず Sexpr データとして返す
(quasiquote template)               ; `template と同義。,/,@ でテンプレート内に式を埋め込む
(panic message)                     ; message: string。回復不能なエラーで異常終了。型は !
(unreachable)                       ; (panic "unreachable") に展開。defmacro
(todo)                              ; (panic "todo") に展開。defmacro
(as Type expr)                      ; 数値/文字の型変換。失敗しうる変換は失敗時に panic
(try-as Type expr)                  ; as と同じだが結果を Option<Type> で返す（失敗は None）
```

`as`/`try-as` が扱えるのは数値・文字カタログのみ（`i32`/`i64`/`f64`/`bignum`/`ratio`/`char` 間）。
同一型・`i32`↔`i64` は無変換。`i32`/`i64`→`char` と `bignum`→`i32`/`i64` は範囲外で失敗しうるため
`as` は panic・`try-as` は `None`、それ以外（拡大変換や `float->int` 等の切り捨て）は常に成功する。
内部的には対応する変換メソッド（functions.md の `int->char`/`int->bignum`/`bignum->int` 等）へ
展開される糖衣構文。

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
