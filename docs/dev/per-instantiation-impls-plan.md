# 総称型の具体型ごとの impl — 実装計画

作成: 2026-10-13。実装済み（2026-10-13、ブランチ `feat/per-instantiation-impls`）。経緯は
[implementation-log.md](implementation-log.md) の「総称型の具体型ごとの impl」の節。

## 1. 決まったこと（2026-10-13、ユーザー決定）

- 1 つの総称型の別々の具体型に、別々の impl を書けるようにする。固有 impl
  （`(impl Box<int> ...)` と `(impl Box<string> ...)`）もトレイト impl
  （`(impl Show Box<int> ...)` と `(impl Show Box<string> ...)`）も、両方が生きる。
- 具体型やメソッドの内部の名前は、他の名前とぶつからないようにする。

これまでは 2 つ目の impl が 1 つ目を置き換えていた（再定義の警告が出る）。置き換えられた側の値は
型エラーになるので型の安全は保たれていたが（fd9694c）、1 つ目の具体型は impl を失っていた。

## 2. 今の仕組み

- メソッドは型ごとに名前 1 つで登録される（`AdtDef::assoc`、キーはメソッド名）。
  具体型の impl のメソッドも、型の名前だけで登録される。
- 実装したトレイトは型ごとの集合（`AdtDef::impls`）に、トレイトの名前だけで記録される。
  関連型の束縛（`AdtDef::trait_assoc`）も型ごとに 1 つ。
- 総称 impl（`(impl Box<T> ...)`）のメソッドは型の雛形として残り
  （`generic_method_templates`、キーは (型, メソッド名)）、呼ばれた具体型ごとに特殊化される。
  特殊化の名前は `m <型の型引数..., メソッド自身の型引数...>`（`mangled_method_name`）で、
  実行時の表には (型パス, その名前) で入る。リンク名は `型パス''m <...>`。
- 実行時の `print-object` は、値の型キー（`box<int>`）から `print-object <int>` を引く
  （`print_object_target`、AOT は `print_object_key`）。今は、具体型の impl に書いた
  `print-object` は `print-object` という名前で登録されるので、この探し方では見つからない。

## 3. 言語の規則

### 3.1 書けるもの、断るもの

- 総称型の具体型（型引数がすべて具体的な型）ごとに、固有 impl とトレイト impl を書ける。
- 同じメソッド名を、総称 impl（`Box<T>`）と具体型の impl（`Box<int>`）の両方に書くのはエラー。
  Rust と同じ（重複する定義、E0592）。どちらを先に書いても断る。
- 同じトレイトを、総称 impl と具体型の impl の両方に書くのもエラー（重複する実装、E0119）。
- 同じ具体型に同じメソッドをもう一度書くのは、今と同じく再定義（警告して置き換える）。
- 型変数と具体型を混ぜた対象（`Pair<T,int>`）は、今と同じく断る。

### 3.2 どの実装を呼ぶか

- インスタンスメソッド: 受け手の型（`Box<int>`）で選ぶ。受け手の具体型に実装が無く、他の具体型に
  あるときは、今と同じ型エラー（"`twice` is a method of `box<int>`, and the receiver is a
  `box<string>`"）。
- 静的メソッド（`self` を取らない）: 期待型から具体型を決める。期待型が無いときは、その名前の
  静的メソッドを持つ具体型が 1 つだけならそれを選び、2 つ以上あればエラー（どの具体型か
  決められない）。`Box<int>::make` という書き方は今の言語に無く、この計画でも足さない。
- 関数値として取り出すとき: 期待する関数型の第 1 引数（静的メソッドなら戻り値の型）で選ぶ。
- 名前解決（第 1 引数の型のメソッド）、`~/name/`、`where` の境界、`:dyn` は、すべて上の
  インスタンスメソッドの選び方を通る。
- `print-object` は具体型ごとに効く。
- 関連型（`(type Item int)`）は、具体型ごとの impl がそれぞれ束縛する。
- スーパートレイト: `(impl Ord Box<int> ...)` には、`Box<int>` を覆う `Eq` の impl
  （`Box<int>` の具体型 impl か、`Box<T>` の総称 impl）が要る。
- ブランケット impl を具体型に実体化するとき（`Box<int>`）は、具体型の impl として入る。
  今は `Box<int>` と `Box<string>` への実体化が互いを置き換えるが、それも解消する。

## 4. 内部の名前

総称型 `G` のメソッド `m` の、型引数 `A...` での実体の名前は、総称 impl の特殊化と同じ
`m <A...>` にする（`ConcreteNames::specialization`）。メソッド自身の型引数 `U...` があれば
`m <A...,U...>`。リンク名は `G''m <A...>`。綴りはすべて `typelisp_mem::ConcreteNames` が作る。

この名前がぶつからない理由:

- 型は `''` の前の型パスで分かれる。型パスはモジュールまで含む。
- 具体型は `<...>` の中で分かれる。中身は型キーの綴り（`type_key_of_type`）で、型を
  モジュールまで含めて一意に表す。型キーの中の `,` は `<>` の内側にしか無いので、つないだ綴りから
  元の並びが一意に決まる。
- 空白と `'` を含むので、ソースに書ける名前（関数名、メソッド名、モジュールパス）とは重ならない。
- 総称 impl の特殊化と同じ綴りになるのは、同じ (型, 具体型, メソッド) を指すときだけである。
  §3.1 で両方を書くことを断るので、1 つの名前につく定義は常に 1 つ。

別の綴り（例えば `m for <int>`）にしない理由: 実行時の `print-object` は値の型キーから名前を
組み立てる。綴りを分けると、実行時に 2 通りの名前を探すことになる。同じ綴りなら、実行時、
AOT、ダンプの側は今のままで済む。

## 5. 登録表

`AdtDef` に、具体型ごとの表を足す。

```rust
/// What `impl`s written for one instantiation of a generic type add to it.
pub struct InstImpls {
    pub args: Vec<Type>,
    pub assoc: BTreeMap<String, AssocFn>,
    pub impls: Vec<Path>,
    pub trait_assoc: BTreeMap<Path, BTreeMap<String, Type>>,
}
// AdtDef
pub insts: BTreeMap<String, InstImpls>,   // キーは型引数の型キーを `,` でつないだもの
```

- `AdtDef::assoc`/`impls`/`trait_assoc` は、すべての具体型を覆う定義（総称 impl、型引数の無い型）
  だけを持つ。具体型の impl は `insts` に入る。
- `Type` は `Ord` を持たないので、キーは型キーの綴りにする（`ConcreteNames::type_args`、名前の
  `<...>` の中身と同じ）。
- 引く入口は `AdtDef::method_at(name, args)`（その具体型の定義か、すべての具体型に共通の定義）、
  `AdtDef::method_defs(name)`（すべての定義）、`AdtDef::implements_at(trait, args)`。呼び出しが
  どの定義を指すかは `Checker::select_method` が決める（受け手の型、静的メソッドなら期待される戻り値の型）。
- 自前の型引数を持つ具体型 impl のメソッド（`(impl Box<int> (m<U> ...))`）の雛形は、
  `generic_method_templates` に `(G, "m <A...>")` のキーで入る。特殊化の要求
  （`SpecRequest::Method`）は雛形のキーの名前（`template`）を運ぶ。
- `def_locs.methods` と `docs.methods`（キーは (型, メソッド名)）も、具体型の impl では名前を
  `m <A...>` にして入れる。
- `AdtDef` はダンプに入るので、`dump::FORMAT_VERSION` を上げる。

## 6. 段階

実装では Phase 1 と 2 を一度に入れた（トレイト impl のメソッドも同じ登録の関数を通るので、
登録先を変えると両方が同時に変わる）。Phase 3 のうち、名前で定義を指す道具（`documentation`・`ed`・
`compile`・`trace`・`disassemble`）は `G<A...>::m` を受け付ける形で入れた。LSP の定義へ移動と hover は、
呼び出しのノードが持つ名前（`m <A...>`）で引くので、登録の鍵を同じ名前にしただけで合う。

### Phase 1: 固有 impl

- `InstImpls` と `AdtDef::insts`、メソッドを引く入口。
- `check_defmethod_in_form`: 具体型の impl のメソッドを `insts` に、名前 `m <A...>` で登録し、
  その名前で定義の形を作る。§3.1 の重複の検査。自前の型引数を持つ場合の雛形。
- `check_assoc_call`、`method_value`、名前解決、静的メソッドの選び方（§3.2）。
- `AdtDef::assoc` を直接引いている所（約 25 箇所）を入口に置き換えるか、総称の定義だけを見て
  よい所かを 1 つずつ決める。
- テスト: 2 つの具体型の固有 impl が両方生きる、インタプリタとコンパイル済みの両方、静的メソッド、
  関数値、重複の検査（両方の順序）、自前の型引数、`~/name/`。

### Phase 2: トレイト impl

- `check_impl_form`: 具体型の impl の `impls`/`trait_assoc` を `insts` に入れる。重複の検査。
- `type_implements`、`explicit_impl_covers`（`insts` を直接見る形に置き換える）、
  `dyn_vtable_slots`、関連型の解決、スーパートレイトの検査、トレイトとの整合の検査。
- ブランケット impl の実体化。
- `print-object`: 具体型の impl に書いたものが、インタプリタと AOT の両方で効く。
- テスト: `:dyn`、`where`、関連型、スーパートレイト、ブランケット、`print-object`。

### Phase 3: 周辺

- LSP（hover、定義へ移動、補完）、`documentation`/`trace`/`compile` に名前で渡すとき、
  エラーメッセージで具体型の実体を `box<int>::m` と書く（`CompileTarget::written_name`）。
- 利用者向け文書（日本語原本、20 言語の訳）。
