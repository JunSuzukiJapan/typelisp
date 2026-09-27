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
| ``no method `upcase` for type `int` (the type of the first argument, which selects the method); `upcase` is a method of `char`, `string` `` | メソッドは第一引数の型で選ばれる。その名前のメソッドはあるが、第一引数の型（ここでは `int`）には無い。メッセージの末尾が、そのメソッドを持つ型の一覧 |
| `unbound variable: y` | その名前の変数が無い。綴りと、束縛の範囲（`let` の外で使っていないか）を確かめる |
| ``use: unresolved `nosuch` `` | `use` したモジュールが見つからない。ファイル名とモジュールパスの対応は [構文リファレンス 3.11](syntax.md#311-ファイルとモジュールの対応複数ファイルのプロジェクト) |
| `unresolved path: c::hidden` | モジュールはあるが、その名前が無いか、`pub` が付いていないので見えない |
| `circular module dependency: a -> b -> a` | モジュールが互いを `use` している。共通部分を別のモジュールに分ける |
| ``return-from: no enclosing block named `nope` `` | `return-from` の名前に合う `block` が囲んでいない。関数名の block は、その関数の中でしか使えない |

### 2.3 呼び出し

| メッセージ | 意味と直し方 |
|---|---|
| `f: expected 1 argument(s), got 2` | 引数の個数が合わない |
| `f: unknown keyword argument :b` | その関数に無いキーワード引数を渡した |
| `new: expected 1 field(s), got 2` | 構造体のコンストラクタに渡した値の個数がフィールドの数と合わない |
| ``setf: cannot assign to constant `k` `` | `defconstant` で定義した名前に代入した。書き換えるなら `defvar` にする |
| ``defsignature: `later` has no definition in this file — a declaration promises one`` | `defsignature` で宣言した関数を定義していない |
| ``format: ~/nosuch/ — no argument here has a method `nosuch` of the shape ...`` | `~/name/` で呼ぶメソッドを、どの引数の型も持っていない（[書式ディレクティブ 5 章](functions/format.md#5-name)） |

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
