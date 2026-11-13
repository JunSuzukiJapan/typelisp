# Eq を Equal に、equals を equal にする計画

2026-11-13 にユーザーと決めたこと。同じ日に全段階を終えた(段階 5 の改名は段階 3 と一緒に入れた)。

計画と違えたところ:

- `Sexpr` の組み込みメソッド `equal` は `(Sexpr, Sexpr)` を取る。受け手が `Option<Sexpr>` のときは、残りの引数も `Option<Sexpr>` として検査する(`method::check_method_call`)。受け手が `Option<Sexpr>` のときは、`Option` 自身のメソッドより `Sexpr` のメソッドを先に探す。`Option` の `impl Equal` は prelude にあり、prelude に依存しない島のソースの `(equal x y)` がそれを選ぶとリンクできないため。
- `Sexpr` を `match` するときに、文字列や浮動小数点数のリテラルを断る制限を外した。`equal` は中身で比べるので、腕は一致できる。

## 決めたこと

- Lisp に無い名前 `equals` のメソッドは、すべて廃止する。
- トレイト `Eq` を `Equal` に改名し、「`equal` が使うトレイト」と定義する。メソッドは `equal`。
- 組み込みの型(数値、`bool`、`char`、`string`、`symbol` など)は、型ごとの組み込みメソッド `equal` をそのまま `Equal` の実装と見なす。
- `Equal` を実装しない型に `(equal a b)` を書いたら、「`Equal` を実装していない」という型エラーにする。どの型にも効く自由関数 `equal<T>` は無くなる。
- 型変数の上の `=` は `equal` に読み替える。
- `not-equals` は消す。
- `member`/`find`/`position`/`pushnew` などの既定の比較は CL どおり `eql` にし、`:test` で変えられるようにする。
- リストの `equal` は実行時のコードが値をたどって比べ、ユーザーの `impl Equal` は呼ばない。リストの中の構造体は、CL どおり同一性で比べる。これを文書に書く。

## 今の形

- `Eq` のメソッドは `equals` と `not-equals`(既定の本体)。`Ord` と `Hash` が `Eq` を親に持つ。
- prelude に `impl Eq` が 21 個、`(Eq A)` の境界が 32 個ある。
- `Sexpr` の `equals` は `eq` で比べる。
- `equal`/`equalp` には、型ごとの組み込みメソッド(`string`、`char`、`bool`、数値)と、どの型にも効く自由関数 `equal<T>`/`equalp<T>` がある。
- `eq`/`eql` は、数値、文字、シンボル、文字列、`bool`、`Sexpr` の組み込みメソッドだけ。構造体、タプル、`Option` などには無い。
- `match` の値パターンは `(equals 値 リテラル)` で比べる。
- 型変数の上の `=` は `equals` に、`/=` は `not-equals` に読み替える(`bounds::trait_operator_method`)。

## 段階

### 段階 1: eq と eql をすべての型に

既定の比較を `eql` にするには、どの型の要素にも `eql` が使えなければならない。

- `eq`/`eql` を、`equalp` と同じく、どの型にも効く自由関数として `std::cmp` に置く。型ごとの組み込みメソッドがある型では、そちらを先に選ぶ。
- 中身は `typelisp_rt::equality` の `eq_val`/`eql_val`。インタプリタはそれを直接呼び、コンパイルされたコードは `rt_sexpr_eq`(新設)と `rt_sexpr_eql` を呼ぶ。
- 語彙の表(`std::cmp`)に `eq`/`eql` を足す。externs の表が変わるので、島と prelude を作り直す。
- 構造体、タプル、`Option` などは同一性で比べる(CL の `eql`)。

### 段階 2: 列の関数の既定の比較を eql に

次の関数から `(Eq A)` の境界を外し、`:test` も `:test-not` も無いときの比較を `eql` にする。

`member`、`find`、`position`、`count`、`assoc`、`rassoc`、`search`、`mismatch`、`remove`、`remove-duplicates`、`substitute`、`adjoin`、`union`、`intersection`、`set-difference`、`set-exclusive-or`、`subsetp`、`tailp`、`ldiff`、`Vector` の `delete`/`delete-duplicates`/`nsubstitute`/`pushnew`。

文字列を探すときなどは `:test equal` が要るようになる。テストと文書の例を合わせる。

### 段階 3: Eq を Equal に

- `deftrait Eq` を `deftrait Equal` にし、メソッドを `equal` だけにする。`not-equals` を消す。
- `Ord` と `Hash` の親を `Equal` にする。`Hash` の約束は「`equal` なら `sxhash` も等しい」。
- 組み込みの型: 本体の無い `(impl Equal string)` を、型の組み込みメソッド `equal` が満たすようにする。トレイトのメソッドと同じ名前・同じ形の組み込みメソッドがあれば、それを実装とする。
- 自由関数 `equal<T>` を消す。`Sexpr` には、組み込みメソッド `equal` を足す(中身は今の `equal_val`)。`Option<Sexpr>` は `Option` の `impl Equal` から `Sexpr` の `equal` に届く。
- `Sexpr` の `impl Eq`(`eq` で比べるもの)は消す。`Sexpr` の `Equal` は、組み込みメソッド `equal`(CL の `equal`)が満たす。
- `Option`、`Result`、`cons-cell`、タプル(prelude.rs で作るもの)の `impl` を `Equal`/`equal` にする。
- `=` の読み替え先を `equal` にし、`/=` は `(not (equal a b))` にする。
- `match` の値パターンは `(equal 値 リテラル)` で比べる。`Sexpr` を `match` するときは、文字列や浮動小数点数のリテラルも中身で一致するようになる。
- `Equal` を実装しない型への `(equal a b)` は型エラーにし、`eq`/`eql`/`equalp` を案内する。
- `equalp` は今のまま、どの型にも効く自由関数として残す。

### 段階 4: HashTable に :test

CL の `make-hash-table` と同じく、表を作るときに比べ方を選べるようにする。

- `(HashTable::new)` の既定は `eql`。`(HashTable::new :test 'equal)` のように、`:test` には `'eq`、`'eql`、`'equal`、`'equalp` のどれかをリテラルで書く。それ以外は型エラー。
- 表は、作るときに決まったハッシュ関数と比較関数を、関数の値として持つ。`get`/`set`/`remove` はそれを呼ぶので、`(Hash K)` の境界は要らない。
- `:test` ごとのハッシュと比較:
  - `eq`: 値の語でハッシュし、`eq` で比べる。どの型でもキーにできる。
  - `eql`: 数値は値で、それ以外は値の語でハッシュし、`eql` で比べる。どの型でもキーにできる。
  - `equal`: キーの型の `Hash` の `sxhash` でハッシュし、`equal` で比べる。キーの型に `Hash`(親の `Equal` を含む)を求める。検査は `HashTable::new` を書いた所で、キーの型が決まってから行う。
  - `equalp`: 実行時のコードが `equalp` と食い違わないハッシュ(大文字と小文字を区別しない、など)を計算し、`equalp` で比べる。どの型でもキーにできる。
- 新しい組み込み: `eq`/`eql`/`equalp` 用のハッシュ関数(どの型にも効く自由関数、中身は `typelisp_rt`)。
- 表が持つ 2 つの関数の値は、GC が辿る。
- `HashSet::new` も同じ `:test` を受け、中の `HashTable` に渡す。
- 文字列をキーにする表は `:test 'equal` が要るようになる。テスト、文書、例を合わせる。

### 段階 5: 名前の変更

- `assert-eq` を `assert-equal` にする。比べ方は `equal`。
- `seq-equals` を `seq-equal` にする(`tailp` が使っている)。

### 段階 6: 文書とテスト

- 日本語の原本を直してコミットし、そのあと 20 言語の訳を合わせる。
- `Eq`、`equals`、`not-equals`、`assert-eq`、`seq-equals` を書いているテストを合わせる。

## そのまま残すもの

- `case` は `equal` に展開する。キーの型には `Equal` が要るようになる。
