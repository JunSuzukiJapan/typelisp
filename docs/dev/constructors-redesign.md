# コンストラクタの再設計 — 実装計画

作成: 2026-10-10

## 1. 決まったこと（2026-10-10、ユーザー決定）

- `defstruct` はフィールド順の `Name::new` を**自動生成しない**。フィールド順に固定された
  コンストラクタは要らない。コンストラクタは明示的に書く。
- フィールドは今までどおり `defstruct` で宣言する。メソッドは Rust の固有 impl にあたる
  `(impl Point 項目...)` に書く。`self` を取らないメソッドは型の静的メソッド（クラスメソッド）になる。
  コンストラクタの名前は利用者が決める（推奨は `new`）。
- 構造体の値は CLOS にならい `(make-instance 'Point :x x :y y)` で作る。
  - 省いたスロットは、スロットのデフォルト（`(x i32 0)`）があればそれ、無ければ型エラー。
  - ジェネリックな構造体の型引数は、引数と期待型から推論する。
  - 型を定義したモジュールの外では、`pub` でないスロットを指定できない。
- `defstruct` の `(:constructor ...)` オプションと、静的メソッドを書く `(defmethod name (Type ...))`
  の形は廃止し、`impl` に一本化する。
- `self` を取る普通の `defmethod` も廃止し、メソッドはすべて `impl` に書く。
- 構造体型の値への `match` はキーワードで書く。`((Point :x :y) ...)` はスロット `x`/`y` を
  同じ名前の変数に束縛し、`((Point :x a :y b) ...)` は `a`/`b` に束縛する。
- 固有 impl は、Rust と同じく型と同じクレートの中に限る（§2.1 の解釈）。
- `Array`/`BitVector`/`Mutex`/`WaitGroup`/`HashSet`/`SortedTable`/`Deque` は `make` をやめて `new` で作る。

経緯: Phase 5（[rust-go-features-plan.md](rust-go-features-plan.md)）で `HashSet` を作ろうと
したとき、自動の `HashSet::new` が型の作者の `new` とぶつかり、`make` にするしかなかった。
しかも自動の `new` は内部表現を外に見せていた（`HashSet::new` が `HashTable<T,()>` を取る）。

## 2. 構文

### 2.1 固有 impl

```lisp
(defstruct Point (pub x f64) (pub y f64))

(impl Point
  (new ((x f64) (y f64)) Point
    (make-instance 'Point :x x :y y))
  (origin () Point
    (make-instance 'Point :x 0.0 :y 0.0))
  (area (self) f64
    (* self::x self::y)))

(Point::new 1.0 2.0)        ; 静的メソッド
(area p)                    ; インスタンスメソッド
```

- 項目の形は trait の `impl` と同じ `(名前 (引数...) 戻り値型 本体...)`。この言語は戻り値型を
  どこでも書くので、ここでも書く。
- 第 1 引数が `self` なら**インスタンスメソッド**、そうでなければ**静的メソッド**。`self` の型は
  書かない（`(self Type)` とは書けない）。
- 公開は項目ごとに `(pub new ...)`。`defstruct` のフィールドの `(pub x f64)` と同じ書き方。
  付けなければ定義したモジュールの中だけで使える。
- ジェネリックな型は `(impl Box<T> ...)`、境界は `(impl Box<T> (where (print-object T)) ...)`。
  メソッド自身の型パラメータは名前に書く（`(map<U> ((self) (f (fn (T) U))) Option<U> ...)`）。
- 固有 impl を書けるのは型と同じクレートの中だけ。この言語にはクレートが無いので、
  **標準ライブラリ（組み込みの型と prelude）を 1 つのクレート、利用者のプログラムを別の
  クレート**とみなす。利用者は自分のプログラムの型にはどのモジュールからでも固有 impl を書けるが、
  `Vector` や `Option` のような標準の型には書けない（足したい振る舞いはトレイトで書く）。
- trait の impl `(impl Trait Type ...)` とは、2 番目の要素で見分ける。名前なら trait の impl、
  項目（リスト）か `where` か何も無ければ固有 impl。

### 2.2 `make-instance`

```lisp
(make-instance 'Point :x 1.0 :y 2.0)
(make-instance 'Box :v 1)              ; Box<int> と推論
(the Box<f64> (make-instance 'Box :v 1.0))
```

- 型名は引用したシンボルのリテラル（`concatenate` の `'vector` と同じく、実行時の型指定ではない）。
- 未知のスロット、同じスロットの 2 度書きは型エラー。
- 引数は書いた順に 1 度ずつ評価する。
- チェッカーが今の構築ノードに落とすので、インタプリタもコンパイル済みコードも新しい実行時の
  仕組みは要らない。

### 2.3 `match` の構造体パターン

今の構造体型の値への `match` は `((new a b) ...)` と書き、変種名 `new` が見えている。これを
型名とキーワードで書く形に変える。

```lisp
(match p
  ((Point :x :y) (+ x y)))            ; x と y に束縛
(match p
  ((Point :x a :y (some b)) ...))     ; a に束縛し、y は入れ子のパターンで照合
```

- キーワードの次がキーワードか終わりなら、スロットを同じ名前の変数に束縛する。
  そうでなければ次の要素がそのスロットのパターン。
- 書かなかったスロットは照合しない（フィールドの数や順序に依存しない）。
- `Sexpr` からの downcast パターン（`((point a b) ...)`、位置で書く）も同じキーワード形にそろえる。

## 3. 内部の扱い

構造体は「変種が 1 つの ADT で、その変種の名前が `new`」として登録されている
（`Checker::check_defstruct_form`）。`(Name::new ...)` は構成子呼び出しの一般経路
（`check_path_call` の `Type::ctor` 分岐）で解決されていた。

- 変種は内部に残すが、パスとして書ける名前を持たせない。`Name::new` は、固有 impl に
  静的メソッド `new` があればそれを指す。
- `make-instance` はその変種の構築ノードを作る。スロットの並べ替えとデフォルトの補完はチェッカーで行う。
- `:copier` は残し、生成する本体を `Name::new` から `make-instance` に変える。

## 4. 実装の順序

各 Phase の終わりに全テストを通す。追加 → 移行 → 削除の順にして、途中でも壊れないようにする。

### Phase A: 固有 impl（追加）

`(impl Type 項目...)` を読み、インスタンスメソッドと静的メソッドを登録する。今の `defmethod` と
静的 `defmethod` の登録の仕組みに載せる。メソッド自身の型パラメータ、単型化・コンパイル・`pub`、
標準の型に利用者が書いたときのエラーを確かめるテストを書く。

### Phase B: `make-instance`（追加）

チェッカーの特殊形として入れる。推論、デフォルト、エラー（未知・重複・不足・非 pub）、
インタプリタとコンパイル済みの両方をテストする。

### Phase C: prelude の移行

- prelude の `defmethod`（308 個、うち静的 13 個）を `impl` に移す。
- `make` を `new` に変える。
- prelude の中の構造体の `Name::new` を `make-instance` に書き換える。
- 自分の `new` が要る型（`SimpleError` など、利用者が `X::new` で作る型）には `impl` で `new` を書く。
- 成果物を作り直す。

### Phase D: 古い形の削除と移行

- 自動の `Name::new`、`(:constructor ...)`、`defmethod` を削除する。
- 構造体パターンをキーワード形にする（§2.3）。
- テスト（構造体の `Name::new` が約 1400 箇所、`defmethod` が 84 箇所）と examples（22）を
  移行する。移行はスクリプトで行い、同じソース文字列の中の `defstruct` からスロット名を読んで
  `(make-instance 'Name :slot v ...)` に書き換える。
- 廃止した形を書いたときのエラーは、何に書き換えればよいかを言う。

### Phase E: 文書とエディタ

- 日本語の文書（構造体の `Name::new` が 46 箇所、`defmethod` が 11 箇所。`syntax.md` の
  `defstruct` の節とオプションリストの節）。
- 20 言語の訳。
- エディタの定義に `make-instance` を足す。

## 5. 未決の問い

なし（2026-10-10 の問い 3 つは §1 に取り込んだ）。
