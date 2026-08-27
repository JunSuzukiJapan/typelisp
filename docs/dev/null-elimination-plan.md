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
3. `Option<Sexpr>` の niche 表現は**必須**で、**`nil` 削除と同じ段階で入れる**（§3.2）。
   niche なら実行時のビット表現が今日と同一なので、リーダ・印字・等価・GC には触らずに済む。
   IR に要る追加はパターンノード 1 種類だけで、`Repr` にも構築側にも新しいものは要らない（§3.2.1）。

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
区別が付かないからである。したがって「niche だけ先に入れて green にする」ことはできない。

> **2026-08-27 訂正（段階 1 実装中の調査による）。**
> 当初ここには「先に `Option<Sexpr>` を**箱のまま**にした遅い中間状態を通し、
> あとから niche を最適化として被せる」案を書き、そちらを推していた。**これは逆である。**
> 箱のままの中間状態のほうが**大きく、かつ捨てる作業になる**:
>
> - `Option<Sexpr>` が箱だと、cons セルの cdr が `Value::Boxed` になる。すると
>   リーダ（`typelisp-read/src/reader.rs`）・印字（`typelisp-print`）・`equal`/`equalp`・
>   cons を歩く全経路（`Value::Empty` 162 箇所）が、箱を被せる/剥がす形に**全部**書き換わる。
> - そして niche を被せる段階で、その書き換えを**全部元に戻す**ことになる。
> - niche なら実行時のビット表現が今日と同一なので、**リーダ・印字・等価・GC には一切触らない**。
>   変わるのは静的型の層と、`Option` の構築/`match` の lowering だけである。
>
> よって **niche と `nil` 削除は同じ段階で入れる**。§6 の段階表はこれに合わせて改めた。

### 3.2.1 niche に要る IR の変更は、パターンノード 1 種類だけ

> **2026-08-27 再訂正（段階 1 完了後の調査による）。**
> ここには当初「`pat-ctor` / `construct` に被検査値の repr を足す必要がある」と書いた。
> 実際にコードを読んだところ、**もっと小さく済む**ことが分かった。以下が調査の結果である。

**(1) `Repr` に新しい variant は要らない。`Repr::of(Option<Sexpr>)` を `Repr::Sexpr` にする。**

niche は「`Option<Sexpr>` と `Sexpr` の実行時表現が同一」という主張そのものなので、
`Repr`（＝表現の分類）でも同じものにするのが正しい。そうすると:

- `field_kind` は 6、`binding_kind` は 2。**どちらも `Repr::Sexpr` と同じ数**なので、
  島の `compile-sexpr-field` / `compile-tag-struct-field` / 束縛境界は**一切変わらない**。
- 境界の decode（`interp.rs:997`）は `Repr::Sexpr` と `Repr::Enum` を**同じ腕**で扱っている
  ので、ここも変わらない。
- 置く場所だけ注意: `Repr::of_by` の `is_enum_ty_by` の腕より**前**に置く（`repr.rs:206` は
  `Option` を enum と判定するので、後ろに置くと届かない）。

**(2) 構築側は新しいノードが要らない。**

- `(Option::some x)` は、`x` を lower したものそのもの。ノードを作らない。
- `(Option::none)` は空リスト定数、すなわち `(construct sexpr 0 false ())`。
  §4 の通り `nil` の index 0 を IR に残す限り、島の `compile-construct-sexpr` の
  variant-0 の腕が**そのまま使える**。「ソース面から `nil` を消すが IR の 0 番は残す」
  という §4 の判断が、ここで効いてくる。

**(3) パターン側だけ、新しいノードが 1 種類要る。**

既存のパターンノードでは表せない:

- `pat-lit`（`core_eval.rs:1428`）は `int`/`bool`/`char` しか受け付けない。空リストは通らない。
- `pat-guard`（`core_eval.rs:1457`）は**束縛を捨てる**（`Ok(Some(Vec::new()))`）ので、
  `(some p)` の `p` を束縛できない。

したがって次の 1 種類を足す（タグ 2 つでも、極性フラグ付き 1 つでもよい）:

| ノード | 意味 | 対応する書き方 |
|---|---|---|
| `(pat-empty)` | 値が空リストなら成立 | `((none) ...)` |
| `(pat-nonempty P)` | 値が空リストでなければ、`P` を同じ値に当てる | `((some P) ...)` |

島側は `compile-value` の `icase` ディスパッチ（`compiler.rs:1642` 付近）と同型の
パターンコンパイラに腕を足すだけで、既存ノードのフィールド位置は動かない。

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
| 2 | パターンノード `pat-empty` / `pat-nonempty` を新設（§3.2.1(3)）。インタプリタと島の両方 | ○ | 要 |
| 3 | `Option<Sexpr>` の niche 表現（§3.1）と `Sexpr` からの `nil` 削除を**同時に**。変種番号は 0 を空きに（§4）。cons variant を `cons-cell<Option<Sexpr>, Option<Sexpr>>` 化、`sexpr-car`/`sexpr-cdr` の戻りを `Option<Sexpr>` に | ○ | 要 |
| 4 | prelude・`core_macros` の `sexpr-*` 利用を `match`/`if-let` へ | ○ | 不要 |
| 5 | 島 `src/compiler.rs` の 522 箇所を移行 → 再生成 | ○ | 要 ×2 |
| 6 | examples / tests / docs、印字仕様の明記（§3.3） | ○ | 不要 |

段階 5 が支配的リスク。`symbol-sexpr-redesign.md` の Phase 2（島の全面移行）が
「週単位」と見積もられたのと同じ規模で、あれと違い今回は**型が変わる**ので
コンパイラが移行漏れを全部教えてくれる分だけ有利。

島の再生成は**不動点なので 2 回**回す（成果物は 4n+1 バイト）。
段階 2 と 3 は IR ノードの形と `Repr` の内容が変わるので再生成が要る。

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

- 段階 3 は `nil` 削除と niche が同時に落ちるので、途中の状態でテストが緑にならない
  区間がある。段階 2（IR ノードに repr を足す）を先に単独で緑にしておくことで、
  その区間を「型の層だけ」に絞れる見込みだが、**まだ実際にやっていない**。
- `sexpr-car` の新しい型。`Sexpr -> Option<Sexpr>`（非 cons でも `none` を返す）か、
  `cons-cell<...> -> Option<Sexpr>`（cons であることを型で要求）か。
  後者のほうが強いが、`sexpr-consp` で分岐してから cons セルを取り出す手段
  （`match` で cons variant を剥がす）が要る。段階 2 の設計時に決める。

---

## 9. 実装ログ

### 段階 1 — `read-sexpr` の EOF と `()` を分離【完了、2026-08-27】

`ReadOutcome`（`(eof)` / `(datum Sexpr)`）を prelude に新設し、
`read-sexpr` / `read-sexpr-preserving-whitespace` / `reader-read-one-until` の戻りを
`Result<Option<Sexpr>, ReadError>` から `Result<ReadOutcome, ReadError>` へ変えた。
内部の呼び出し元は `read-delimited-list` の 1 箇所だけ。

`datum` のフィールドは今は `Sexpr`。段階 3 で `Option<Sexpr>` になるが、
そのとき**このシグネチャは変わらない**——EOF が別バリアントに出ているので、
`datum` の中身が `Option` になっても衝突しない。それがこの段階の目的である。

分かったこと: prelude にはこれまで `defenum` が 1 つも無かった（`Option`/`Result` は
組み込み、`registry.rs` 側の定義）。prelude 内の `defenum` が型検査もダンプ生成も
通ることは、この段階で初めて確認された。

副作用: `ReadOutcome` は公開型なので `editor/emacs/typelisp-mode.el` と
`editor/vscode/syntaxes/typelisp.tmLanguage.json` の型リストにも追加した
（`editor_keyword_sync_test` が番人）。

### 段階 2 — 空リストのパターンを `pat-empty` として切り出す【完了、2026-08-27】

`(none)` を照合するパターンノードを 4 層（チェッカーの `pattern_form` /
`eval/interp/core_eval.rs` / `src/compile/core_bridge.rs` の `translate_pattern` /
島の `compile-pattern-test`）＋番人（`tests/core_vocabulary_test.rs`）に追加した。
段階 3 で `(some P)` 用の `pat-nonempty` が同じ 4 層に加わっている。

### 段階 3 — `nil` 削除と niche を同時に【完了、2026-08-27】

§6 の表では段階 3〜6 に分けていたが、**分けられなかった**。型が変わる以上、
prelude も島も docs も同じコミットで緑にするしかない。以下は実際の作業。

**見積もりとの差**。段階 5（島の 522 箇所）を「支配的リスク・週単位」と見ていたが、
実際は**機械変換 258 箇所・手直し 0 箇所**で済んだ。prelude は機械変換 23・手直し 3、
`core_macros` は機械変換 7・チェッカー側の追随 7。

この軽さは利点ではなく**方針の裏面**である。`sexpr-*` が `Option<Sexpr>` を
受けるようにした結果、**呼び出し側は 1 箇所も型エラーにならない**。移行が楽なのと
静的検査が効かないのは同じ事実の two sides で、島に対して「空リストの取り違えを
コンパイラに指摘させる」という当初の目的は達成していない。得られた静的検査は
`Option<Sexpr>` → `Sexpr` の**縮小**を明示させる境界だけに集中している。

**テストを回して初めて出た 2 件**（型が付くこととは別の話）:

1. `Sexpr` → `Option<Sexpr>` の暗黙拡大が無かった。`(sexpr-cons (Int 7) ...)` が
   型エラーになる。niche のおかげでこの拡大は実行時ゼロ命令なので、`check_atom` の
   既存の Sexpr 拡大群に透過的な retype として加えた。逆向きは型エラーのまま。
2. `match` 糖衣の網羅性検査が `option` の 2 変種で数えていた。10 個の `Sexpr` 腕を
   並べても「1/2」になり、**全域 match が黙って部分 match になる**。被覆の宇宙を
   「`Sexpr` の 10 形 ＋ `none`」に変更。番号が衝突しないのは `none` が空リストの
   明け渡した variant 0 にちょうど収まるからで、§4「変種番号を詰めない」がここで効いた。

**`car`/`cdr` の CL 互換は両ティアに入れる必要があった**（`Interp::call_builtin` と
`typelisp-rt` の `rt_car`/`rt_cdr`）。片方だけだと、同じリスト走査が
「呼び出し元がコンパイル済みかどうか」で違う終わり方をする。非 cons の**原子**は
引き続きエラー——`(car 5)` は型混同であって列の終わりではない（CL も同じ）。

**チェッカーを変えたらダンプを手で再生成する**。ダンプの番人はソースのハッシュしか
見ていないので、チェッカーの変更ではハッシュが変わらないまま中身が古くなる。

### 段階 3 補遺 — 静的検査が実は効いていなかった【2026-08-28】

上の段階 3 の記録に**誤りがあった**ので訂正する。「逆向き（`Option<Sexpr>` →
`Sexpr` の縮小）は型エラーのまま」と書いたが、**型エラーになっていなかった**。

```lisp
(defun takes-sexpr ((s Sexpr)) i64 1)
(takes-sexpr (the Option<Sexpr> ()))   ; 通ってしまっていた
```

原因は既存の widening である。`Option<Sexpr>` は `AdtKind::Sum` なので
`is_heap_repr` が真を返し、「ユーザ ADT を `Sexpr` データに入れる」ための retype が
空リストにも適用されていた。widening の衣を着た narrowing である。

**型安全性が破れていた。** 空リストが裸の `Sexpr` に届いた結果:

```lisp
(defun tag ((s Sexpr)) i64
  (match s ((int _) 1) ... ((path _) 10)))   ; 10 変種、網羅的、catchall 無し
(tag (the Option<Sexpr> ()))
;; => internal error: eval: no matching match arm
```

**チェッカーが網羅的だと証明した `match` が実行時に腕を使い果たした。** null が
「来ないはず」の場所に届くという、この移行が消すはずだったもの、そのものである。

`is_heap_repr` の retype から `Option<Sexpr>` を除外して修正。番人は
`tests/check_test.rs` の `an_option_sexpr_does_not_narrow_to_a_bare_sexpr`
（拡大が通ること・縮小が落ちること・ユーザ ADT の拡大が壊れていないこと の 3 つ）。

**教訓**: 段階 3 の移行が「機械変換 258・手直し 0」で済んだのは、方針の裏面である
以上に**この穴のせい**でもあった。移行が楽すぎるときは、型が仕事をしていない可能性を
先に疑うべきだった。「呼び出し側が 1 箇所も型エラーにならない」を設計の帰結として
説明し、それ以上調べなかったのが誤り。

**同時に見つかった移行漏れ 3 件**（縮小を塞いだ結果ではなく、裸の `Sexpr` を要求する
表面を数え上げて見つけた）:

| 箇所 | 症状 |
|---|---|
| `lambda` の `&rest` | 裸の `Sexpr` に束縛。`defun` の `&rest` は `Option<Sexpr>` で、コメントは「同一の扱い」と主張していた |
| `apply` の末尾リスト引数 | 同上 |
| `pprint-logical-block` の `obj` | **壊れていた**。docs が「何も反復しないブロックには `()`」と言う、その `()` が型エラー |

数え上げの結果（裸の `Sexpr` が残る箇所）:

- `registry.rs` の組み込みシグネチャ: **0**（`sexpr()` 36 箇所すべて `option_of` 包み）
- prelude / `core_macros` / 島 `compiler.rs` の typelisp コード: **0**（すべてコメント内）
- 唯一の例外は prelude の `(impl Eq sexpr ...)`——トレイト実装の対象型なので `Self` が
  `Sexpr` になる。避けられない
