# Rust/Go 由来の追加機能 — 実装計画

作成: 2026-10-09

Common Lisp との差はほぼ埋まった（[cl-missing-classes-and-methods.md](cl-missing-classes-and-methods.md)）。
残る差は Rust と Go の側にある。2026-10-09 にサンプルコードを見たうえで、下の 8 項目の採用が決まった。
この文書はその構文の確定事項と、実装の順序・手が入る場所・完了判定をまとめる。

## 1. 確定した構文

| # | 機能 | 構文 |
|---|---|---|
| 1 | Option/Result のコンビネータ | `(map opt f)` `(and-then r f)` `(or-else r f)` `(map-err r f)` `(ok-or opt e)` `(unwrap-or-else opt f)` `(expect opt msg)`。すべて `defmethod`。左から右へつなぐ `->` マクロ |
| 2 | 遅延評価のイテレータ | `lazy::map` `lazy::filter` `lazy::take` `lazy::take-while` `lazy::skip` `lazy::enumerate` `lazy::zip` `lazy::chain` `lazy::flat-map` `lazy::iterate` `lazy::repeat`、`(collect it)`。既存の `map`/`filter`（`Vector` を返す）はそのまま |
| 3 | テスト | `#[test]` を付けた `defun`。`typl test [名前の一部]` |
| 4 | match のガード・or パターン | `(pat :when cond body...)`、`(:or p1 p2 ...)` |
| 5 | コレクション | `HashSet<T>` `SortedTable<K,V>` `Deque<T>` |
| 6 | キャンセル | `Context`。タスクもスレッドも協調的（`done` チャネルと `is-cancelled`） |
| 7 | タプル | リテラル `#{"foo" 123 45.6}` |
| 8 | ベクタ・配列リテラル | CL どおり `#(1 2 3)`（`Vector<T>`）と `#2A((1 2) (3 4))`（`Array<T>`）。`[...]` は採らない |

リテラル（7・8）の規則:

- 中身はすべてリテラル。CL と同じく `#(a b)` の `a` は変数ではなく記号。
- 要素の型は整数リテラルと同じく文脈で決まる。文脈が無ければ `Vector<int>`。
- `Vector<T>`/`Array<T>` の要素型は 1 つにそろうので、ソースに書いた `#(1 "a")` は型エラー。
  `#2A` の行の長さがそろわなければ読み取りエラー。
- `read` で読むデータの中にも書ける。`Sexpr` に vector（と配列）の変種を足す。
- 印字は、できる限り再入力できる形にする（`#(..)` `#2A(..)` `#{..}`）。
  読み戻せない値には今までどおり `#<..>` を使う。

リーダーの前提: `]` と `}` は今は記号の一部として読まれる（区切り文字は `( ) " ' `` ` `` ;` だけ）。
`#[test]` と `#{..}` のために、この 2 つを区切り文字にする。記号の途中の `#` は、今までどおり書ける
（CL と同じ）。`[` `]` 単独の意味は、CL どおり利用者のために空けておく。

## 2. 2026-10-09 に決まった細部

| 項目 | 決定 |
|---|---|
| タプルの型の書き方 | `#{int string}`（値と同じ形） |
| `read` で読むデータの中の `#{..}` | `Sexpr` にタプルの変種を足す |
| データの中の `#(..)` の要素 | `Option<Sexpr>`（空リスト `()` を要素にできる）を想定 |
| `#[test]` を書ける場所 | `tests/` にもソースの同じファイルにも書ける |
| ガード付きの腕と網羅性 | 網羅したものとして数えない（Rust と同じ）を想定 |

タプルのパターンは `#{a b}`、要素の取り出しは `t::0` `t::1`（2026-10-09 決定）。

`Sexpr` の変種番号は `crates/typelisp-front/src/sexpr_variant.rs` の 1 か所にまとめた（2026-10-09）。
変種を足すときは、そこに末尾で足す。島の `case` は `#%sexpr-名前` で番号を参照する。

## 3. 実装の順序

影響の及ぶ範囲が大きいものから先に入れる。後の Phase は前の Phase の上に乗る。

### Phase 1: リーダーの土台と `Sexpr` の vector 変種（項目 8）

**2026-10-10 完了**（ブランチ `feat/vector-literals`）。計画から変わった点・計画に無かった点:

- 印字は `*print-array*` に従う。真なら `#(1 2 3)`、偽なら `#<vector<int> 3>`。印字器が読む
  ようになったので、その `defvar` を prelude の先頭（ほかの `*print-*` と同じ場所）へ移した。
- `read` で読んだ配列（`Array<Option<Sexpr>>`）は、どのプログラムの型にも現れないので prelude の
  `print-object` が作られない。Sexpr のデータとして組み込みの印字器が同じ書式で印字する。
- 配列の箱の組み立て（prelude の `defstruct Array` のフィールド順）はリーダーの
  `alloc_sexpr_array` の 1 か所。ソースのリテラルは `%internal::array-from-row-major` で作るので、
  要素 0 個の配列も文脈の型で作れる。
- コンパイル済みコードが quote されたベクタ・配列を組み立て直すランタイム関数を 2 つ足したので、
  ABI 版は 0.2.1 になった。
- Sexpr を `_` 無しで網羅する `match` は、`vector`/`array` の腕を足す必要がある（利用者に見える
  変化。リリースノートに書く）。

1. `]` と `}` を区切り文字にする（`crates/typelisp-read/src/reader.rs` の `is_delimiter`）。
2. `#(..)` と `#nA(..)` を読む（`read_hash`）。利用者が登録した `#(` は今までどおり組み込みより優先される。
3. `Sexpr` に vector と配列の変種を、`sexpr_variant.rs` の**末尾に**足す。既存の番号は動かさない。
   島を作り直す（`SOURCE` を変えたら 2 回）。
4. チェッカーが、ソースの中の vector/配列の datum を、文脈の型に合わせた
   `Vector<T>`/`Array<T>` の構築に下ろす。インタプリタとコンパイル済みの両方。
5. プリンタが `Vector`/`Array` を `#(..)`/`#2A(..)` で印字する。`printing.md` の表を直す。

完了判定: 下のことがインタプリタと `compile` の両方で確かめられること。
- `#(1 2 3)` が `(the Vector<i32> ..)` の中でも、型の指定が無くても型が付く
- `(read-from-string "#(1 a \"s\")")` で読んだデータを `match` で取り出せる
- 印字した結果を `read` で読み戻すと、同じ値になる

### Phase 2: `defmethod` の型パラメータ（項目 1 の前提）

`(defmethod map<U> ((self Option<T>) (f (fn (T) U))) Option<U>)` のように、受け手の型パラメータに加えて
メソッド自身の型パラメータを持てるようにする。型パラメータは `defun` と同じく引数から推論する。
単型化（再 check 方式）と、コンパイル経路のマングル名に型パラメータが入ることを確かめる。

完了判定: 受け手の型パラメータとメソッド自身の型パラメータを両方使うメソッドが、
インタプリタと `compile` の両方で動くこと。

**2026-10-10 完了**（ブランチ `feat/defmethod-type-params`）。決めたこと・計画に無かったこと:

- メソッド自身の型パラメータは `FnSig::type_params` に入れる。呼び出しでは、受け手が決める所有者の
  型パラメータと同じ置換の中で、引数から推論する。名前が重なると区別できないので、所有者の型
  パラメータ（受け手に書いた名前と、型の宣言の名前の両方）と同じ名前は断る。
- 単型化の引数は「所有者の型引数のあとにメソッド自身の型引数」。`MethodTemplate::Form` に
  `own_vars` を足し、ダンプの形式は 16 に、ABI 版は 0.2.2 になった。
- 所有者がジェネリックでなくても、メソッド自身の型パラメータがあればテンプレートにする。
- 受け手が所有者の型パラメータの一部だけを変数で書く形（`Pair<T,int>`）は、単型化で束縛する
  手段が無いので、メソッド自身の型パラメータを持つときは断る。
- `impl` の中のメソッドは型パラメータを足せない（トレイトのシグネチャに無い名前として断られる）。
  トレイトのメソッドに型パラメータを持たせることは今回の範囲外。

### Phase 3: match のガードと or パターン（項目 4）

- ガードは腕の束縛を読めなければならない。既存の `pat-guard` は値パターンの比較用で、
  スクルーティニーしか見えないので、使えない。腕の単位で「束縛してから条件を評価し、
  偽なら次の腕へ進む」という新しいノードを作る。インタプリタ、島、コンパイル済みコードのすべてに要る。
- `(:or p1 p2)` は、同じ本体を持つ複数の腕に展開できる。ただし、各選択肢が同じ名前・同じ型の
  変数を束縛することは検査する。
- 網羅性の検査では、ガード付きの腕を「網羅したもの」に数えない。

**2026-10-10 完了**（ブランチ `feat/match-guards-or`）。決めたこと・計画から変わった点:

- ガードは腕の新しいノードではなく、パターンを包む核のノード `(pat-when PAT TEST)` にした。
  インタプリタ（`match_core_pattern`）も島（`compile-pattern-test`）もパターンを調べる場所で
  そのまま調べられ、偽は「パターンが合わない」と同じ失敗先へ分岐する。well-known シンボルの
  表の末尾に `pat-when` を足したので、ABI 版は 0.2.3 になった。
- 束縛を本体のクロージャが捕まえる場合、本体はセルを作る `let` で包まれる。ガードは本体より前に
  走るので、`core_bridge` はガードの式も同じ `let` で包む。
- `(:or ...)` はパターンのどこに書いてもよく、チェッカーが組み合わせの数だけ腕を作る
  （`(= expr)` と `'datum` の中は展開しない）。`if-let` は `match` に展開されるので、そのまま使える。
- 自由変数の walk が `pat-typetest` の部分パターンを field 1（型のキー文字列）から読んでいた
  既存の不具合を直した。`(the T (= x))` の `x` を、コンパイル済みのクロージャが捕まえ損ねていた。

### Phase 4: タプル（項目 7）

新しい型（要素数ごとに別の型）を足すので、型 identity
（`src/type_key.rs`）・単型化・表現（`Repr`）・印字・`Eq`/`Hash`/`print-object` の実装に手が入る。
タプルが入れば、`lazy::enumerate`/`lazy::zip` の要素を `cons-cell` からタプルに変える。

### Phase 5: prelude で書けるもの（項目 1・2・5・6）

typelisp で書けるものは typelisp で書く（Rust 組み込みは Rust でしか書けないものだけ）。

1. Option/Result のコンビネータと `->` マクロ。
2. `lazy` モジュール。アダプタは `Iter` を実装する構造体として書く。impl の `where` に構造体の
   型パラメータ（`(where (Iter I (Item A)))`）を書けるかを最初に確かめる。
3. `HashSet<T>`（`HashTable<T,()>` の上に作る）、`SortedTable<K,V>`、`Deque<T>`。
   名前は `HashTable` に合わせる（`get` `set` `remove` `count` `clear`）。要素の追加は `insert`
   （`add` は `Add` トレイトが使っている）。どれも `Iter` を実装する。
4. `Context`（`background` `with-cancel` `with-timeout` `cancel` `done` `is-cancelled`）。
   `done` は「キャンセルされると閉じる `Chan<()>`」。親をキャンセルすると子もキャンセルされる。
   `cancel` は何度呼んでもよい（`Mutex` で 1 回だけ閉じる）。

### Phase 6: テスト（項目 3）

1. `#[...]` を読む。次のフォームに属性を付けた形を返す。
2. 普通に実行したときと `compile-file` では、`#[test]` の付いた `defun` を読み飛ばす。
3. `typl test` サブコマンド: テストを集めて 1 本ずつ実行し、panic と実行時エラーを捕まえて続ける。
   名前の一部で絞り込める。結果の行と集計を出す。
4. `assert-eq`: 失敗したら両辺を印字する（`Eq` と `print-object` を要求する）。

## 4. ドキュメント

利用者向けのリファレンス（`docs/ja` を原本にして、日英を先に、残り 8 言語をまとめて）を、
Phase ごとに更新する。`syntax.md`（リテラル、ガード、`#[test]`）、`types.md`、
`functions/option-result.md`、`functions/sequences.md`、`functions/collections.md`、
`functions/concurrency.md`、`functions/printing.md`、`guide/from-common-lisp.md`
（`#(` が CL と同じであること、データの中の vector が `Option<Sexpr>` の要素を持つこと）。
