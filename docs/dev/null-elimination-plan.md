# null 排除計画 — `Sexpr` の `nil` を `Option` へ

作成: 2026-08-27。状態: **未着手（計画のみ）**。

方針（ユーザー指示）:

- null を言語から完全に排除する。
- cons セルの型を `cons<T, Option<U>>` にする。
- `Sexpr` から null を削除。
- null を使わないと書けない箇所は、`Option` を使う。

以下は、その指示を実装可能な形に落とすための調査結果と段階分けである。
**先に結論を 3 つ**書いておく。いずれも着手前に合意しておく必要がある。

1. 空リストという値は消えない。`Option<Sexpr>` の `none` に**改名**され、
   型検査の対象になる。得られるのは「`sexpr-car`/`sexpr-cdr` の実行時パニックが
   型エラーになる」ことであって、値が 1 つ減ることではない。
2. cdr だけでなく **car も `Option<Sexpr>`** にせざるを得ない。結果として
   「S 式データの型」は `Sexpr` ではなく `Option<Sexpr>` になる。
3. `Option<Sexpr>` の niche 表現は**必須**だが、**`nil` 削除より先には入れられない**
   （§3.2）。この 2 つは同じ段階で入れるか、間に「箱のままの遅い中間状態」を
   1 段挟むかのどちらかになる。

---

## 1. 現状

`nil` は言語レベルではすでに存在しない。`typelisp-mem/src/value.rs:13` が
「removed language-level `nil`」と書いている通りで、残っているのは
**`Sexpr` の `nil` variant**＝空リストというデータである
（`check/registry.rs:1198`）。

`cons<T,U>` / `cons-cell<A,B>` は**すでにある**（`prelude.rs:1215`）。
generic な 2 フィールド積で、`car`/`cdr` は `defstruct` が生成する全域の
フィールドアクセサ。`HashTable<K,V>::entries` と `floor-div` 系の 2 値返しが
これを使っている。したがって指示の第 2 項は「新設」ではなく
**「`Sexpr` のリストをこの型の上に載せ直す」**作業である。

`Sexpr` 側のリスト走査は `sexpr-*` 島レイヤ（`registry.rs:765` 前後）が担う。
`sexpr-car`/`sexpr-cdr` は現在どちらも `Sexpr -> Sexpr` で、非 cons に当てると
実行時パニックする。これが今回なくしたい穴である。

| 呼び出し元 | `sexpr-car` | `sexpr-cdr` | `sexpr-consp` |
|---|---|---|---|
| `src/compiler.rs`（島） | 175 | 324 | 23 |
| `prelude.rs` | 51 | — | — |
| `core_macros.rs` | 14 | — | — |

---

## 2. 設計上の帰結

### 2.1 car も `Option` にせざるを得ない

`cons<T, Option<U>>` の素直な読みは「cdr だけ `Option`」だが、それだと
`'(a () b)` が書けない。要素位置に空リストが置けないからである。
島の IR には `(defun f () ...)` の空引数リストのように**要素としての空リスト**が
確実に含まれるので、この読みは成立しない。

したがって cons セルは `cons-cell<Option<Sexpr>, Option<Sexpr>>` になる。
dotted pair は保たれる（cdr が `some` の非 cons）。

### 2.2 「S 式の型」は `Option<Sexpr>` になる

2.1 の帰結として、`Sexpr` は「空リストでない S 式」を意味する型に格下げされ、
**データとして受け渡される型は `Option<Sexpr>`** になる。`read` の戻りも
`Result<Option<Sexpr>, ReadError>` になる。

これは「null を消した」というより「null に `none` という名前を与え、
型に現れるようにした」である。利得は 1 つだけ、ただしそれは大きい:
島の 522 箇所の暗黙の実行時前提が、すべて型検査の対象になる。

### 2.3 `unwrap` は逃げ道であって既定ではない

`unwrap` はすでにある（`prelude.rs:319`、`Option<T>` の generic method）。
`Option<Sexpr>` にもそのまま効くので、この件での追加実装は不要。

**ただし既定にしてはいけない。** 島の 522 箇所を機械的に
`(unwrap (sexpr-cdr x))` へ置換すると、実行時挙動は今日と同一で、
空リストへの car も型エラーにならない。書き換えコストだけ払って防げるバグは
0 件になる。`unwrap` に意味があるのは、それが「ここは絶対 `some` だ」という
**明示的で grep 可能な主張**である場合に限られる。移行の既定は
`match` / `if-let` とし、`unwrap` は主張として書く場所にだけ置く。

なお Rust の `unwrap` は呼び出し元に伝播せず panic する。伝播は `?` の役目であり、
この言語は `?`/try を意図的に持たない（`implementation-log.md:106`）。
`?` 相当を後から入れる場合、**関数レベルの早期 return が無い**ことが前提の障害になる
（`return` は loop 専用: `checker.rs:11403`「return: not inside a loop」）。
また `catch` は panic を claim しない設計である（`implementation-log.md:6914`）ので、
「unwrap 失敗を catch で受ける」も現状は成立しない。今回は**この 3 つには手を付けない**。

---

## 3. 実行時表現 — niche が必須

### 3.1 niche の内容

現在 `Option<T>` は一律 `Repr::Enum`（`check/repr.rs:53`）で、実体は
`BoxedObj::Enum { type_name: String, variant, fields: Vec<Value> }`
（`typelisp-mem/src/value.rs:251`）。**空リスト 1 個ごとに box スロット＋`String` 確保**に
なるので、prelude＋島で 117,729 セルという規模に対してそのままでは成立しない。

`Option<Sexpr>` にだけ niche 表現を与える:

| 値 | 表現 |
|---|---|
| `(Option::none)` | `Value::Empty` / `TAG_IMMEDIATE \| IMMEDIATE_NIL`（`typelisp-abi/src/lib.rs:105`） |
| `(Option::some v)` | `v` そのもの |

`Sexpr` から `nil` を抜いた後はタグが衝突しないので成立し、**実行時のビット表現は
今日と完全に同一**になる。FASL/ダンプの値表現も変わらない。

必要な変更:

1. `Repr` に 1 variant 追加（例: `Repr::OptionSexpr`）。`class()` は
   `Tagged { collectable: true }`＝`Repr::Sexpr` と同じなので、
   **`binding_kind`（2）も `field_kind`（6）も既存の番号のまま**。島向けの
   番号空間は動かない（§4 と併せて、ここが今回いちばん幸運な点）。
2. 構築: `(Option::some x)` が `rt_data_new` でなく `x` を素通しし、
   `(Option::none)` が `Value::Empty` を出す。インタプリタ（`core_eval.rs` の
   construct）と島（`compile-construct`）の両方。
3. `match`: `((some x) ...)` は `v != Empty` のテストと `x = v` の束縛、
   `((none) ...)` は `v == Empty` のテスト。同じく両方。

### 3.2 順序の制約 — niche は `nil` 削除より先に入れられない

**`Sexpr` に `nil` が残っている間は niche が不健全になる。**
`(Option::some ())` と `(Option::none)` がどちらも `Value::Empty` になり、
区別が付かないからである。したがって「段階 0 として niche だけ先に入れて green にする」
ことはできない。取れる形は 2 つ:

- **(a) 同時に入れる。** `nil` 削除と niche を 1 段階で。中間状態が無い代わりに、
  その 1 段階が大きい。
- **(b) 間に遅い中間状態を挟む。** 先に「`Sexpr` から `nil` を削除し、
  `Option<Sexpr>` は箱のまま」を green にし、そのあと niche を**純粋な最適化として**
  被せる。空リストごとに box＋`String` を確保する重い状態を一時的に通るが、
  規模は 10 万オーダの確保なのでビルドもテストも通るはずで、
  正しさと表現を分離できる。

**(b) を推す。** 島の 522 箇所の書き換えと、表現の切り替えを、別々にデバッグできる。

### 3.3 印字は値駆動である（仕様変更を伴う）

印字は静的型でなく値を見る。`PrintHooks::enum_variant_name` が
`(type_key, variant)` を box から読む形（`typelisp-print/src/runtime.rs:40`）なので、
niche を入れると `Option<Sexpr>` の値は「`(some ...)`」でなく**中身の S 式**として、
`none` は `()` として印字される。

`Option<Sexpr>` が「S 式の型」になった後はこれが**正しい**挙動である
（`none` は空リストなのだから `()` と出るべき）。ただし
`(println (Option::some 42))` が `(some 42)` でなく `42` を出すようになるのは
`Sexpr` インスタンス化に限った可視の仕様変更なので、テストと docs に明記する。

---

## 4. 変種番号は詰めない

`Sexpr` の変種番号は 3 箇所に焼かれている:

- `eval/interp.rs:2419` の `SEXPR_NIL`..`SEXPR_PATH`（0..10）
- `src/compile/core_bridge.rs:1219` の同名定数
- `check/repr.rs:433` の `field_kind`（1=int 2=float 3=char 4=bool 6=str …）
  ——これは「`Sexpr` 自身の変種番号を借りている」と doc comment が明言しており、
  島の `compile-sexpr-field` / `compile-construct-sexpr` が同じ空間を読む

`bignum`/`ratio` が `cons` の後ろに**追記**された（`registry.rs:1206` のコメント:
「so the existing `SEXPR_*` variant-index constants stay valid」）ことからも分かる通り、
この番号空間は**詰めない**のが確立した規律である。

したがって `nil` を削除する際、**index 0 は空き番として残し、他を繰り上げない**。
`int` は 1 のまま。これで上記 3 箇所と島の成果物に手を入れずに済む。

---

## 5. 塞ぐ穴 — `read-sexpr` の EOF

`read-sexpr` は `Result<Option<Sexpr>, ReadError>` を返し、**`none` が EOF** を意味する
（`prelude.rs:4125`、ほか `:4103` `:4115`）。`()` が `none` になると EOF と区別が付かない。

該当は 3 箇所だけなので、EOF を専用の変種に分けて塞ぐ（`Result<ReadOutcome, ReadError>`、
`ReadOutcome = eof | datum(Option<Sexpr>)`）。**段階 2 より前に単独で入れられる**
数少ない作業で、`Sexpr` 本体に触らない。

---

## 6. 段階

| 段階 | 内容 | 単独 green | 島再生成 |
|---|---|---|---|
| 1 | `read-sexpr` の EOF と `()` を分離（§5） | ○ | 不要 |
| 2 | `Sexpr` から `nil` 削除（番号は 0 を空きに）、cons variant を `cons-cell<Option<Sexpr>, Option<Sexpr>>` 化、`sexpr-cdr` の戻りを `Option<Sexpr>` に。**`Option<Sexpr>` は箱のまま** | ○（遅い） | 要 |
| 3 | prelude・`core_macros` の `sexpr-*` 利用を `match`/`if-let` へ | ○ | 不要 |
| 4 | 島 `src/compiler.rs` の 522 箇所を移行 → 再生成 | ○ | 要 ×2 |
| 5 | `Option<Sexpr>` の niche 表現を最適化として被せる（§3.1） | ○ | 要 |
| 6 | examples / tests / docs、印字仕様の明記（§3.3） | ○ | 不要 |

段階 4 が支配的リスク。`symbol-sexpr-redesign.md` の Phase 2（島の全面移行）が
「週単位」と見積もられたのと同じ規模で、あれと違い今回は**型が変わる**ので
コンパイラが移行漏れを全部教えてくれる分だけ有利。

島の再生成は**不動点なので 2 回**回す（成果物は 4n+1 バイト）。
段階 2 と 5 は `Repr` の内容が変わるので再生成が要る。

---

## 7. 規模

| 対象 | 量 |
|---|---|
| Rust の `Value::Empty` 参照 | 162 |
| 島の `sexpr-car`/`cdr`/`consp` | 522 |
| prelude の `sexpr-*` / `(Nil)` | 約 75 |
| `Option<Sexpr>` を既に使う箇所（EOF と衝突） | 3 |
| ダンプ `FORMAT_VERSION` | 4 → 5（段階 2 で bump） |

---

## 8. 着手前に決まっていないこと

- 段階 2 の中間状態（`Option<Sexpr>` が箱）で、prelude＋島のロードが
  実用的な時間に収まるか。**未計測**。段階 2 に入る前に、
  `Option<i64>` を 10 万個作る程度のマイクロベンチで当たりを付ける価値がある。
  収まらないなら §3.2 (a)（同時投入）に切り替える。
- `sexpr-car` の新しい型。`Sexpr -> Option<Sexpr>`（非 cons でも `none` を返す）か、
  `cons-cell<...> -> Option<Sexpr>`（cons であることを型で要求）か。
  後者のほうが強いが、`sexpr-consp` で分岐してから cons セルを取り出す手段
  （`match` で cons variant を剥がす）が要る。段階 2 の設計時に決める。
