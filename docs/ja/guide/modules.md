# モジュールとファイル構成

複数のファイルからなるプログラムの組み立て方を説明します。構文の細かい規則は
[構文リファレンス](../reference/syntax.md#310-module--use--名前空間) の 3.10〜3.13 節にあります。

## 1. 1 ファイルが 1 モジュール

typelisp では、**ファイルがそのままモジュールになります**。ソースルートから見たファイルの
相対パスが、そのままモジュールのパスです。

| ファイル | モジュール |
|---|---|
| `<ルート>/geometry.typl` | `geometry` |
| `<ルート>/geo/shapes.typl` | `geo::shapes` |
| `<ルート>/net/http/client.typl` | `net::http::client` |

ファイルの先頭にモジュール宣言を書く必要はありません。

## 2. プロジェクトの作り方

プロジェクトのルートに `typelisp.toml` という名前のファイルを置きます。中身は空でかまいません。

```
myapp/
├── typelisp.toml
├── main.typl
└── geometry.typl
```

ソースを `src/` の下にまとめたい場合は、`typelisp.toml` に次の 1 行を書きます。

```toml
src = "src"
```

`typl` は実行するファイルのディレクトリから上へ向かって `typelisp.toml` を探し、見つかった
場所をソースルートにします。見つからなければ、実行するファイルのあるディレクトリがルートに
なります（REPL ではカレントディレクトリ）。

## 3. 定義を公開して使う

`geometry.typl`:

```lisp
(pub defstruct point
  (pub x i32)
  (pub y i32)
  (tag string))                        ; pub の無いフィールドは外から読めない

(defun square ((n i32)) i32 (* n n))   ; pub の無い関数も外から呼べない

(pub defun dist2 ((a point) (b point)) i32
  (+ (square (- a::x b::x)) (square (- a::y b::y))))

(pub defmethod origin (point) point (point::new 0 0 "o"))
```

`main.typl`:

```lisp
(use geometry)

(let ((a (geometry::point::origin))
      (b (geometry::point::new 3 4 "b")))
  (println "~a" (geometry::dist2 a b)))
```

```sh
typl main.typl        # => 25
```

`(use geometry)` と書いた時点で `geometry.typl` が読み込まれます。前もって読み込んでおく
必要はありません。

### 公開の単位

- 関数・構造体・列挙型・グローバル変数・マクロ・メソッドは、それぞれ `pub` を付けたものだけが
  他のモジュールから見えます。書き方は `(pub defun ...)` のように、定義の直前に `pub` を置きます。
- 構造体は**型の公開とフィールドの公開が別**です。`(pub defstruct point ...)` で型が見えるように
  なり、各フィールドは `(pub x i32)` と書いたものだけが外から読み書きできます。
- 公開されていない名前を外から使うと、`unresolved path: geometry::square` のように
  「解決できない」というエラーになります。名前を打ち間違えたときと同じメッセージなので、
  綴りが合っているのに解決しない場合は `pub` の付け忘れを疑ってください。

`pub` を付けられる定義の一覧は [構文リファレンス 3.13](../reference/syntax.md#313-pub--公開指定) にあります。

## 4. `use` の書き方

```lisp
(use geometry)              ; モジュールを取り込む。geometry::dist2 と書いて使う
(use geometry::dist2)       ; 関数を取り込む。dist2 と裸の名前で使える
(use geometry::point)       ; 型を取り込む。point::new、point::origin、型注釈の point と書ける
(use a::f b::g)             ; 複数をまとめて書ける
```

- **`use` はそれより後のフォームにだけ効きます。** ファイルの先頭に置いてください。`use` より
  上で `geometry::dist2` と書くと、`unresolved path` になります。
- モジュールを `use` せずに `geometry::dist2` とフルパスで書いても解決しません。ファイルを読み込む
  きっかけは `use` だけです。
- 裸の名前が既に埋まっているところへ同じ名前を `use` すると警告が出ます。承知のうえで
  取り込むときは `shadowing-import` を使います。
- ディレクトリの中のモジュールは `(use geo::shapes)` と書き、取り込んだ後は最後の部分
  （`shapes::...`）で参照します。

### トレイトのメソッドの呼び方

`impl` で実装したメソッドは、モジュールの関数ではなく**型に属する**ので、モジュール名を付けずに
呼びます。

```lisp
(use shapes::core)
(println "~a" (area (core::square::new 4)))    ; core::area ではなく area
```

`impl` 内のメソッドは `pub` を書かなくても常に公開されます。

トレイトそのものは他のモジュールへ公開できません。トレイトの定義と、それに対する `impl` と、
`:dyn` でそのトレイトを使うコードは、1 つのモジュールにまとめてください。

## 5. ファイルの中で名前空間を分ける

1 つのファイルの中でさらに名前空間を分けたいときは `module` を使います。ファイル自身の
モジュールの内側に入れ子になります。

```lisp
;; main.typl の中
(module util
  (pub defun clamp ((x i32) (lo i32) (hi i32)) i32
    (if (< x lo) lo (if (> x hi) hi x))))

(println "~a" (util::clamp 15 0 10))    ; main::util::clamp
```

ファイルの残り全部を 1 つの名前空間に入れるなら、括弧で包まずに `(in-module util)` と書くことも
できます。

## 6. 依存関係の制約

- **循環は作れません。** `a.typl` が `(use b)` し、`b.typl` が `(use a)` すると、
  `circular module dependency: a -> b -> a` というエラーになります。両方が必要とする定義を
  3 つ目のモジュールに移してください。
- **型にも関数にも前方参照はありません。** 同じファイルの中でも、定義より前で使うことはできません。
  相互再帰する関数は `defsignature` で片方を先に宣言します
  （[構文リファレンス 3.2](../reference/syntax.md#32-defsignature--前方宣言)）。

## 7. 実行される順序

`typl main.typl` で実行すると、次の順で進みます。

1. `main.typl` と、そこから `use` されるすべてのファイルを読み、型を検査します。**どこか 1 か所でも
   型エラーがあれば、何も実行されません。**
2. `use` されたモジュールのトップレベルの式が、使う側より先に実行されます。
3. `main.typl` のトップレベルの式が、上から順に実行されます。

プログラムの入口を `main` 関数にまとめ、ファイルの末尾で `(main)` を呼ぶ形にしておくと、
そのまま [AOT コンパイル](compile.md#3-aot-コンパイルで実行ファイルを作る) にも使えます。

## 8. `load` との違い

`(load "path")` は Common Lisp の `load` と同じく、ファイルの中身を**いまの名前空間へそのまま**
読み込みます。モジュールで包まず、`pub` も関係ありません。設定ファイルを読み込む、REPL で
手元のファイルを読み直す、といった用途に使います。プログラムを部品に分けるときは `use` を
使ってください。
