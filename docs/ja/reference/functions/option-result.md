# Option と Result、エラー型

## 1. `Option<T>` / `Result<T,E>`

構成子: `Option<T>` は `Some(T)` / `None`。`Result<T,E>` は `Ok(T)` / `Err(E)`。
`E` は任意の型でよい——組み込みの具象エラー型も、`defstruct`/`defenum` で書いた自前の型も
そのまま載る（3 章）。

| 名前 | 形式 | Option | Result | 説明 |
|---|---|---|---|---|
| `unwrap` | `(unwrap self)` | `Option<T>→T` | `Result<T,E>→T` | 値を取り出す。`None`/`Err` なら panic |
| `unwrap-or` | `(unwrap-or self default)` | `(Option<T>,T)→T` | `(Result<T,E>,T)→T` | 値、または既定値 |
| `is-some` | `(is-some self)` | `Option<T>→bool` | — | `Some` か |
| `is-none` | `(is-none self)` | `Option<T>→bool` | — | `None` か |
| `is-ok` | `(is-ok self)` | — | `Result<T,E>→bool` | `Ok` か |
| `is-err` | `(is-err self)` | — | `Result<T,E>→bool` | `Err` か |
| `expect` | `(expect self msg)` | `(Option<T>,string)→T` | `(Result<T,E>,string)→T` | 値を取り出す。`None`/`Err` なら `msg` で panic |
| `unwrap-or-else` | `(unwrap-or-else self f)` | `(Option<T>,fn()→T)→T` | `(Result<T,E>,fn(E)→T)→T` | 値、または `f` の結果。`f` は `None`/`Err` のときだけ呼ばれる |
| `map` | `(map self f)` | `(Option<T>,fn(T)→U)→Option<U>` | `(Result<T,E>,fn(T)→U)→Result<U,E>` | `Some`/`Ok` の中身に `f` を適用する |
| `map-err` | `(map-err self f)` | — | `(Result<T,E>,fn(E)→F)→Result<T,F>` | `Err` の中身に `f` を適用する |
| `and-then` | `(and-then self f)` | `(Option<T>,fn(T)→Option<U>)→Option<U>` | `(Result<T,E>,fn(T)→Result<U,E>)→Result<U,E>` | `Some`/`Ok` なら中身を `f` に渡し、その結果を返す |
| `or-else` | `(or-else self f)` | `(Option<T>,fn()→Option<T>)→Option<T>` | `(Result<T,E>,fn(E)→Result<T,F>)→Result<T,F>` | `None`/`Err` なら `f` の結果を返す |
| `ok-or` | `(ok-or self e)` | `(Option<T>,E)→Result<T,E>` | — | `Some(v)` を `Ok(v)` に、`None` を `Err(e)` にする |

構成子は `Option::some`/`Option::none`/`Result::ok`/`Result::err`（または `(use option)`/
`(use result)` で裸名 `some`/`none`/`ok`/`err` も使える）。

分岐は `match` で明示するか、上の `map`/`and-then` などでつなぐ。Rust の `?` に当たる構文は無い。

`Option`/`Result` の `map` はメソッドで、シーケンスの `map`（[シーケンス](sequences.md)）とは別のもの。
第 1 引数の型が `Option`/`Result` ならこちらが呼ばれる。

`->` マクロは、値を次の式の第 1 引数として順に渡す（Clojure の `->` と同じ）。
`(-> x (f a) (g b))` は `(g (f x a) b)` になる。括弧の無い名前 `h` は `(h x)` として扱う。
メソッドは第 1 引数が受け手なので、コンビネータをそのままつなげる:

```lisp
(defun half ((n int)) Option<int>
  (if (= 0 (mod n 2)) (option::some (/ n 2)) (option::none)))

(-> (option::some 8)
    (and-then half)                          ; (some 4)
    (and-then half)                          ; (some 2)
    (map (lambda ((x int)) int (* x 10)))    ; (some 20)
    (unwrap-or 0))                           ; => 20

(-> (parse-int "x")
    (map-err (lambda ((e ParseIntError)) string (message e)))
    (unwrap-or-else (lambda ((m string)) int (length m))))
```

## 2. `Option<T>` の実行時表現

Rust と同じく、**`Option<T>` は多くの場合、箱を作らない**。`some v` は `v` そのもの、`none` は
空リストの値で、確保も間接参照も無い。`Option<Sexpr>`（空リストが `none`）、`Option<int>`、
`Option<string>`、`Option<my-struct>`、`Option<f64>`、`Option<(fn ...)>` はすべてこの形。

箱に入るのは、`T` の値が空リストの値と区別できない場合だけ:

| `T` | 表現 | 理由 |
|---|---|---|
| `Option<U>`（入れ子） | 箱 | 内側の `none` が外側の `none` と同じ値になる |
| `()` | 箱 | `()` の値が空リストの値そのもの |
| `ptr` / `c-long` / `c-ulong` | 箱 | 64bit 全部が値で、区別に使える余地が無い |
| それ以外 | 箱なし | — |

表現は型だけで決まり、値からは読めない。印字では静的な型から `(some ...)`/`none` を
復元して見せるので、`(format false "~a" opt)` は `(some 1)` と出る。制約が 2 つある:

- **`:dyn Trait` には入れられない**（`(impl Speak Option<int> ...)` した値を `:dyn Speak` に
  渡すのはエラー）。
- `Sexpr` からの `(the Option<T> ...)` ダウンキャストは**構成子を名指す**——
  `(the Option<int> (some x))` / `(the Option<int> (none))`。`(the Option<int> o)` と全体を束縛する
  形はエラー。

## 3. エラー型と `Error` トレイト

Rust の `std::error::Error` に倣い、**`Error` は型ではなくトレイト**。エラーを表す具象型は
用途ごとに分かれていて、いずれも `Error` を実装する。

| 型 | 生成元 |
|---|---|
| `ParseIntError` | `parse-int` |
| `ParseFloatError` | `parse-float` |
| `ReadError` | `read` / `read-from-string` / `read-sexpr` |
| `EvalError` | `eval` / `macroexpand` |
| `FileError` | ファイル/ストリーム操作（[ストリームとファイル](streams-files.md)） |
| `NetError` | ネットワーク操作（[ネットワーク](network.md)） |
| `SimpleError` | `(SimpleError::new msg)` / `(simple-error msg)`。CL の `simple-error`——「何が起きたか言いたいだけ」のときの既定の選択肢 |
| `WrappedError` | `(wrap-error msg cause)`。自分のメッセージと原因の両方を運ぶ型で、`Error` トレイトに `source` がある理由 |

`ParseIntError` から `NetError` までは、いずれも「メッセージ文字列を1つ持つ単一変種の列挙型」で、
型名と変種名が同じ（`(match e ((ParseIntError m) m))`、構成は
`(ParseIntError::ParseIntError "...")`）。特別扱いは無く、自前のエラー型を
`(defstruct my-err (...))` / `(defenum my-err ...)` で書いたときとまったく同じ扱いになる。

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `message` | `(message e)` | `Self→string` | エラーメッセージ（`Error` トレイトのメソッド） |
| `source` | `(source e)` | `Self→Option<:dyn Error>` | このエラーが包んでいる原因、無ければ `None`（Rust の `Error::source`） |
| `as-dyn-error` | `(as-dyn-error r)` | `Result<T,E>→Result<T,:dyn Error>`（`E` は `Error` 実装） | 具象エラー型を trait オブジェクトへ広げる |
| `describe-error` | `(describe-error e)` | `E→string`（`E` は `Error` 実装） | メッセージと、`source` を辿った原因の連鎖を 1 行 1 原因で。CL に対応物は無い（Rust の "caused by"） |

自前のエラー型に `Error` を実装すれば、組み込みエラーと**同じ形で**扱える:

```lisp
(defstruct io-err (path string))
(impl Error io-err
  (message ((self Self)) string (append "io failed: " self::path))
  (source ((self Self)) Option<:dyn Error> (option::none)))

(defun open-it ((p string)) Result<i32, io-err>          ; 具象型をそのまま E に載せる
  (if (equal p "") (result::err (io-err::new "<empty>")) (result::ok 3)))

(defun describe ((e :dyn Error)) string (message e))     ; 種類を問わず一様に扱う
(describe (io-err::new "/etc/app.conf"))
(describe (ParseIntError::ParseIntError "boom"))
```

複数のエラー型を1つの `Result` に集める場合は `Result<T, :dyn Error>`（Rust の
`Box<dyn Error>` に当たる）を使い、具象エラーは `as-dyn-error` で広げる。`?` が無いので、
この変換は明示的に書く:

```lisp
(defun run ((s string)) Result<i32, :dyn Error>
  (match (as-dyn-error (parse-int s))            ; ParseIntError -> :dyn Error
    ((ok n) (as-dyn-error (open-it (if (= n 0) "" "f"))))   ; io-err -> :dyn Error
    ((err e) (result::err e))))
```

**型とトレイトは1つの名前空間を共有する**（Rust と同じ）。同じモジュール内で
`defstruct`/`defenum` とトレイトに同じ名前は付けられず、型位置にトレイト名を書くと
「`error` is a trait, not a type — write `:dyn error`」と報告される。

回復できない失敗は `panic` で表す。`panic` の扱いと `catch`/`throw` は
[構文リファレンス](../syntax.md#8-非局所脱出catch--throw--unwind-protect)、エラー処理の方針は
[同 9 章](../syntax.md#9-エラー処理の方針)。
