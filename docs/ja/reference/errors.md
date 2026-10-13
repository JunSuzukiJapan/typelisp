# エラーメッセージ

`typl` が出す主なエラーメッセージの意味と直し方。

## 1. 読み方

エラーは次の形で標準エラーに出る。

```text
error: ファイル:行:桁: 種類: メッセージ
```

`種類` によって、いつ見つかったエラーかが分かる。

| 種類 | いつ | 意味 |
|---|---|---|
| `type error` | 実行する前（チェック時） | 型や名前の誤り。そのフォームは実行されない |
| （種類なし） | 読み取り時・チェック時 | 括弧の対応などの構文の誤りや、名前が見つからない誤り |
| `panic` | 実行中 | 回復できない失敗。`unwind-protect` の cleanup を走らせてから止まる |

`warning:` で始まる行は警告で、処理は続く。

`ファイル:行:桁` は誤りのある式の位置を指す。標準ライブラリの関数の中で起きた実行時のエラーは、
プログラムがその関数を呼んだ位置を指す。位置の無いエラー（`error: panic: ...` など）もある。

例:

```text
error: main.typl:1:24: type error: type mismatch: expected `i32`, found `string`
```

`main.typl` の 1 行目 24 桁目の式が、`i32` を期待される位置で `string` だった、という意味。

## 2. チェック時のエラー

実行の前に見つかる誤り。直すまでそのフォームは実行されない。

### 2.1 型

| メッセージ | 意味と直し方 |
|---|---|
| ``type mismatch: expected `T`, found `U` `` | `T` 型が要る位置に `U` 型の式がある。暗黙の型変換は無いので、数値なら `(as T x)` で変換する。`int` と `i32` も別の型 |
| ``integer literal 300 is out of range for u8 (0..=255)`` | リテラルがその型に入らない。切り詰めたいなら `(as u8 300)` と書く |
| ``unknown type `foo`: no type of that name is visible here. ...`` | その名前の型が無い。型は、それを使う最初のフォームより前に定義する（型には前方宣言が無い）。型変数のつもりなら、関数名の `<foo>` などの宣言部に書く（[構文リファレンス 3.6](syntax.md#36-defstruct--構造体ユーザ定義型)） |
| ``cannot infer type argument `t` for `vector::new` `` | 型引数が決まらない。`(the Vector<int> (Vector::new))` のように `the` で型を書く |
| ``non-exhaustive match on `color`: 1/2 variants covered`` | `match` がすべての変種を扱っていない。足りない変種の腕か、`_` の腕を足す |
| ``type `pt` does not implement trait `eq` required by `where` clause on type parameter `a` `` | 関数がトレイトを要求しているのに、渡した型が実装していない。`(impl Eq pt ...)` を書く（[標準トレイト](functions/traits.md)） |
| ``` `sq` does not implement `shape`, so it cannot be used as `:dyn shape` ``` | `:dyn` の位置に、そのトレイトを実装していない型の値を渡した。`impl` を書く |
| ``` `error` is a trait, not a type — write `:dyn error` for a trait object ``` | 型の位置にトレイト名を書いた。`:dyn Error` と書く |
| ``if: (if cond then else)`` | `if` の形が違う。`if` は else が必須。else の要らない場合は `when` を使う |

### 2.2 名前

| メッセージ | 意味と直し方 |
|---|---|
| `no such function: bar` | その名前の関数もメソッドも無い。綴りを確かめる |
| ``no method `upcase` for type `int` (the type of the first argument, which selects the method); `upcase` is a method of `char`, `string` `` | メソッドは第一引数の型で選ばれる。その名前のメソッドはあるが、第一引数の型（ここでは `int`）には無い。メッセージの末尾が、そのメソッドを持つ型の一覧（具体型ごとの `impl` に書いたメソッドなら `box<int>` のような具体型） |
| ``no method `twice` for type `box<string>` (the first parameter of the function this value is expected to be, which selects the method); `twice` is a method of `box<int>` `` | メソッドを値として渡すと、渡す先の関数型の第一引数の型でメソッドが選ばれる。その型（ここでは `box<string>`）には、その名前のメソッドが無い。メッセージの末尾は、そのメソッドを持つ型の一覧 |
| `unbound variable: y` | その名前の変数が無い。綴りと、束縛の範囲（`let` の外で使っていないか）を確かめる |
| ``cannot define variable `f`: a function of that name already exists here — a name has one meaning, whether a function, a variable or a macro`` | 同じモジュールで、関数・グローバル変数・マクロのうち 2 つに同じ名前を付けた。どちらかの名前を変える（[構文リファレンス 3.10](syntax.md#310-module--use--名前空間)） |
| ``` `point` has no static method `new`; a struct's methods are written in its `impl`, ...``` | 構造体に `new` は自動では作られない。値は `make-instance` で作るか、`impl` に `new` を書く（[構文リファレンス 3.5](syntax.md#35-impl--型のメソッド)） |
| ``defmethod is gone — write methods in the type's `impl`: ...`` | `defmethod` は無い。メソッドは型の `impl` に書く（[構文リファレンス 3.5](syntax.md#35-impl--型のメソッド)） |
| ``defstruct: (:constructor ...) is gone — ...`` | `:constructor` オプションは無い。コンストラクタは `impl` に静的メソッドとして書く |
| ``method box::m: `box` has a `m` for every instantiation already, so an `impl` of `box<int>` cannot define its own ...`` | 同じ名前のメソッドを、型パラメータを変数で書いた `impl`（`Box<T>`）と具体型の `impl`（`Box<int>`）の両方に書いた（書いた順によらない）。どちらか一方にまとめる（[構文リファレンス 3.5](syntax.md#35-impl--型のメソッド)） |
| ``impl show box<int>: `box` implements `show` for every instantiation already, ...`` | 同じトレイトを、`Box<T>` と `Box<int>` の両方に `impl` した。どちらか一方にまとめる |
| ``use: unresolved `nosuch` `` | `use` したモジュールが見つからない。ファイル名とモジュールパスの対応は [構文リファレンス 3.11](syntax.md#311-ファイルとモジュールの対応複数ファイルのプロジェクト) |
| `unresolved path: c::hidden` | モジュールはあるが、その名前が無いか、`pub` が付いていないので見えない |
| `circular module dependency: a -> b -> a` | モジュールが互いを `use` している。共通部分を別のモジュールに分ける |
| ``return-from: no enclosing block named `nope` `` | `return-from` の名前に合う `block` が囲んでいない。関数名の block は、その関数の中でしか使えない |

### 2.3 呼び出し

| メッセージ | 意味と直し方 |
|---|---|
| `f: expected 1 argument(s), got 2` | 引数の個数が合わない |
| `f: unknown keyword argument :b` | その関数に無いキーワード引数を渡した |
| `circle: expected 1 field(s), got 2` | 列挙型の変種に渡した値の個数が、その変種のフィールドの数と合わない |
| ``make-instance point: slot `y` is not given and has no default`` | デフォルト値の無いフィールドを `make-instance` に書いていない |
| ``make-instance point: no slot `z` (its slots are `x`, `y`)`` | 構造体に無いフィールドを書いた |
| ``make-instance geo::pt: slot `secret` is not `pub`, so it can be given only inside module `geo` `` | 型を定義したモジュールの外で、`pub` でないフィールドを書いた。そのモジュールに `pub` のコンストラクタを書いてもらう |
| ``` `box::make` is written for `box<int>`, `box<string>`; the type the call is expected to return picks one, ...``` | 具体型ごとの `impl` に同じ名前の静的メソッドがあり、どれを呼ぶか決まらない。`(the Box<int> (Box::make))` のように期待型を書く |
| ``compile: `box::twice` is written for more than one instantiation of `box`; name one: ...`` | `compile`・`trace`・`documentation` などに、具体型ごとに書いたメソッドを型の名前だけで渡した。`box<int>::twice` のように具体型を書く |
| ``setf: cannot assign to constant `k` `` | `defconstant` で定義した名前に代入した。書き換えるなら `defvar` にする |
| ``defsignature: `later` has no definition in this file — a declaration promises one`` | `defsignature` で宣言した関数を定義していない |
| ``format: ~/nosuch/ calls what (nosuch arg colon at) with arg a `string` would: no such function: nosuch`` | その場所で `(nosuch 引数 colon at)` と書いた呼び出しが解決できない。引数の型のメソッドか関数を定義する（[書式ディレクティブ 5 章](functions/format.md#5-name)） |
| ``format: ~/brief/ can be handed an element of a list argument, whose type is only `Sexpr` — ...`` | `~{` などの中、リストの要素に当たる位置に `~/name/` を書いた。要素の型では呼ぶ先を決められない |

## 3. 読み取りのエラー

| メッセージ | 意味と直し方 |
|---|---|
| `unexpected end of input while reading a list` | 閉じ括弧が足りない。位置は読み終えたところ（ファイルの末尾など）を指すので、開き括弧のほうを探す |

## 4. 実行時のエラー（panic）

| メッセージ | 意味と直し方 |
|---|---|
| `panic: divide by zero` | 整数・有理数のゼロ除算。浮動小数点数のゼロ除算は panic せず `inf`/`NaN` になる |
| `panic: unwrap: called on none` | `none` に `unwrap` した。`match` か `unwrap-or` で `none` の場合を扱う |
| `panic: Vector: index 5 out of bounds` | 範囲外の添字。`len` で長さを確かめるか、範囲外で `none` を返す関数（`nth`、`pop` など）を使う |
| `panic: an integer argument does not fit a fixnum` | 添字や個数を取る引数に、63bit に入らない `int` を渡した |
| `throw: no enclosing (catch 'oops) for this throw` | 同じタグの `catch` が囲んでいない `throw` を実行した |
| `panic: <メッセージ>` | プログラムが `(panic "<メッセージ>")` を呼んだ。`assert` の失敗は `assertion failed: ...` |

`panic` はタスクの中で起きてもプロセス全体を止める（[構文リファレンス 12.4](syntax.md#124-ほかの機能との関係)）。
回復したい失敗は `Result` で表す（[構文リファレンス 9 章](syntax.md#9-エラー処理の方針)）。

## 5. 警告

| メッセージ | 意味 |
|---|---|
| ``warning: redefining function `f` `` | 同じ名前の関数を定義し直した。後の定義が有効になる。REPL で定義を直すときには普通に出る |
