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

まだ決めていないのは、タプルのパターンと要素の取り出し方（案: パターン `#{a b}`、取り出し `t::0`）。

`Sexpr` の変種番号は `crates/typelisp-front/src/sexpr_variant.rs` の 1 か所にまとめた（2026-10-09）。
変種を足すときは、そこに末尾で足す。島の `case` は `#%sexpr-名前` で番号を参照する。

## 3. 実装の順序

影響の及ぶ範囲が大きいものから先に入れる。後の Phase は前の Phase の上に乗る。

### Phase 1: リーダーの土台と `Sexpr` の vector 変種（項目 8）

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

### Phase 3: match のガードと or パターン（項目 4）

- ガードは腕の束縛を読めなければならない。既存の `pat-guard` は値パターンの比較用で、
  スクルーティニーしか見えないので、使えない。腕の単位で「束縛してから条件を評価し、
  偽なら次の腕へ進む」という新しいノードを作る。インタプリタ、島、コンパイル済みコードのすべてに要る。
- `(:or p1 p2)` は、同じ本体を持つ複数の腕に展開できる。ただし、各選択肢が同じ名前・同じ型の
  変数を束縛することは検査する。
- 網羅性の検査では、ガード付きの腕を「網羅したもの」に数えない。

### Phase 4: タプル（項目 7）

タプルのパターンと取り出し方（§2）を決めてから始める。新しい型（要素数ごとに別の型）を足すので、型 identity
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
