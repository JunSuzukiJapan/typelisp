# トレイト

トレイトは「この操作ができる型」という約束です。複数の型に同じ名前の操作を持たせ、それを使う
関数を型ごとに書かずに済ませるために使います。Rust のトレイトとほぼ同じ仕組みです。
[型の基本](types.md) を読んでいることを前提にします。

## 1. トレイトを定義して実装する

図形の面積と名前を返す操作を、トレイト `Shape` として定義します。

```lisp
(deftrait Shape ()
  (area ((self Self)) int)
  (name ((self Self)) string))
```

- `deftrait` の名前の次の `()` は、継承するトレイトの一覧です（4 節）。無ければ空にします。
- 各行はメソッドの宣言です。`Self` は「このトレイトを実装する型」を指します。

型にトレイトを実装するには `impl` を書きます。

```lisp
(defstruct circle (r int))
(defstruct rect (w int) (h int))

(impl Shape circle
  (area ((self Self)) int (* 3 self::r self::r))
  (name ((self Self)) string "circle"))

(impl Shape rect
  (area ((self Self)) int (* self::w self::h))
  (name ((self Self)) string "rect"))
```

実装したメソッドは普通の関数と同じ形で呼べます。

```lisp
(area (circle::new 2))     ; => 12
(area (rect::new 3 4))     ; => 12
```

トレイトで宣言したメソッドを 1 つでも実装し忘れると、`impl` の位置で型エラーになります。

## 2. トレイト境界：「このトレイトを実装した型なら何でも」

ジェネリック関数の型パラメータに、`where` で条件を付けられます。これを**トレイト境界**と呼びます。

```lisp
(defun report<T> ((s T)) string
  (where (Shape T))
  (format false "~a: ~a" (name s) (area s)))

(report (circle::new 1))   ; => "circle: 3"
(report (rect::new 2 5))   ; => "rect: 10"
```

`(where (Shape T))` があるので、本体で `T` の値に `name` と `area` を使えます。境界が無ければ、
`T` について何も分からないので呼べません。

`Shape` を実装していない型を渡すと、呼び出した位置で型エラーになります。

```
(report 5)
error: ...: type error: report: type `int` does not implement trait `shape` required by `where` clause on type parameter `t`
```

ジェネリック関数は、呼ばれた型ごとに専用の関数が作られます。実行時に型を調べて分岐する処理は
入りません。

## 3. デフォルト実装

トレイトのメソッドに本体を書いておくと、`impl` でそのメソッドを省略したときに使われます。

```lisp
(deftrait Describe ()
  (label ((self Self)) string)
  (describe ((self Self)) string
    (format false "<~a>" (label self))))

(impl Describe circle
  (label ((self Self)) string "a circle"))            ; describe は既定のもの

(impl Describe rect
  (label ((self Self)) string "a rect")
  (describe ((self Self)) string "[rect]"))           ; 自分で書いたものが優先される

(describe (circle::new 1))     ; => "<a circle>"
(describe (rect::new 1 1))     ; => "[rect]"
```

## 4. 標準のトレイトを実装する

標準ライブラリにもトレイトがあり、実装すると標準の関数がその型に使えるようになります。

| トレイト | 実装するメソッド | 使えるようになるもの |
|---|---|---|
| `Eq` | `equals` | `member`、`find`、`position`、`match` の `(= 式)` パターンなど |
| `Ord` | `less` | `less-equal`、`greater` など。`Ord` は `Eq` を継承する |
| `print-object` | `print-object` | `println` などでの表示のしかた |
| `Iter` | `next` | `doiter`、`map`、`filter`、`sort` など |
| `Error` | `message`、`source` | エラー型として扱う（[エラー処理](errors.md)） |

金額を表す型に `Eq` と `Ord` を実装してみます。`Ord` は `Eq` を継承しているので、`Eq` の `impl` を
先に書きます。

```lisp
(defstruct money (yen int))

(impl Eq money
  (equals ((self Self) (other Self)) bool (= self::yen other::yen)))

(impl Ord money
  (less ((self Self) (other Self)) bool (< self::yen other::yen)))

(greater (money::new 5) (money::new 3))     ; => true（Ord のデフォルト実装）
```

`print-object` を実装すると、`println` での表示を決められます。引数 `escape` は、`~s` のように
読み戻せる形を求められたときに `true` になります。

```lisp
(impl print-object money
  (print-object ((self Self) (escape bool)) string
    (format false "~a yen" self::yen)))

(println "~a" (money::new 500))              ; 500 yen
```

トレイト境界と組み合わせると、`Ord` を実装した型なら何にでも使える関数が書けます。

```lisp
(defun largest<T> ((v Vector<T>)) Option<T>
  (where (Ord T))
  (foldl (iter v)
         (lambda ((best Option<T>) (x T)) Option<T>
           (match best
             ((some b) (if (less b x) (Option::some x) best))
             ((none) (Option::some x))))
         (Option::none)))
```

`money` を 300、900、100 の順に入れた `Vector` を渡すと、`(some 900 yen)` が返ります。

## 5. `:dyn`：型の違う値をまとめて扱う

`Vector<T>` の要素はすべて同じ型なので、`circle` と `rect` を 1 つの `Vector<circle>` には
入れられません。「`Shape` を実装した何か」をまとめて扱うには、型 `:dyn Shape` を使います。

```lisp
(defun total-area ((shapes Vector<:dyn Shape>)) int
  (let ((sum 0))
    (doiter (s (iter shapes))
      (setf sum (+ sum (area s))))
    sum))

(let ((shapes (the Vector<:dyn Shape> (Vector::new))))
  (push shapes (circle::new 1))
  (push shapes (rect::new 2 3))
  (doiter (s (iter shapes))
    (println "~a -> ~a" (name s) (area s)))
  (println "total ~a" (total-area shapes)))
```

```
circle -> 3
rect -> 6
total 9
```

- `circle` や `rect` の値は、`:dyn Shape` が求められている場所に置くと自動で変換されます。
- `(area s)` がどの型の `area` を呼ぶかは、実行時に `s` の中身の型で決まります。
- `Shape` を実装していない型の値を `:dyn Shape` の場所に置くと、型エラーになります。

2 節のトレイト境界と `:dyn` の使い分け:

| | トレイト境界（`where`） | `:dyn Trait` |
|---|---|---|
| 呼び出し先が決まるとき | 実行前 | 実行時 |
| 1 つの `Vector` に違う型を混ぜる | できない | できる |
| 使える型 | 制限なし | 構造体・列挙型・`int`・`string`・`f64` など（`bool`・`char`・`symbol`・`i32` などは不可） |

入れられる型の正確な一覧は [構文リファレンス 3.9](../reference/syntax.md#39-deftrait--impl--トレイト機構) にあります。

`:dyn` にできないトレイトもあります。メソッドが `self` 以外の引数や戻り値に `Self` を使う場合
（`Eq` の `equals` など）です。どの型か実行時まで分からないので、「同じ型の値」を用意できない
ためです。

## 6. 制約

- トレイトの定義と、それに対する `impl`、`:dyn` でそのトレイトを使うコードは、1 つのモジュール
  （ファイル）にまとめてください。トレイトを他のモジュールへ公開することはまだできません。
- 型とトレイトは同じ名前空間にあります。同じモジュールで、型とトレイトに同じ名前は付けられません。

## 7. 次に読むもの

- [マクロ](macros.md)：構文を自分で定義する
- [構文リファレンス 3.9](../reference/syntax.md#39-deftrait--impl--トレイト機構)：ブランケット実装、関連型など
- [トレイト](../reference/functions/traits.md)：標準ライブラリのトレイトの一覧
