# 評価器の CPS 化（設計）

最終更新: 2026-09-08 / ブランチ: `feature/cps-evaluator` / **移行は完了している**

この文書は、ツリーウォーク評価器を **Rust の再帰から継続スタックへ**移した作業の設計を記録する。
実行の意味は変えていない。変えたのは「計算の残り」がどこにあるか、それだけ。

評価のループは `crates/typelisp-front/src/eval/interp/core_cps.rs` にあり、`core_eval.rs` には
タグの語彙（`Op`）・葉の評価（`eval_leaf`）・ノードの読み取り・トップレベルが残っている。
再帰版（`eval_core_recursive` と `step_core` の非葉アーム、855 行）は削除した。

作業の経緯は [implementation-log.md](implementation-log.md) に書く。

---

## 1. なぜ

**Lisp の再帰の深さが、そのまま Rust のスタックの深さになっていた。**

旧 `Interp::eval_core` は `step_core` を呼ぶループだったが、`step_core` の中から
`self.eval_core(...)` を呼び返している箇所が **36 箇所**あった。末尾でない部分式 — `if` の
条件、引数、`let` の初期化式、`match` の scrutinee — はすべてそこを通っていた。

その帰結が、`scripts/test-serial.sh` の `RUST_MIN_STACK=32MB` と、
`src/bin/bootstrap_island.rs` が 64MB スタックのスレッドを明示的に立てていること。

得られるものは 3 つ:

1. **深い再帰が書ける**。Lisp の再帰深度が GC ヒープの容量だけで決まるようになる
2. **実行状態がデータになる** — 保存・中断・再開・移動ができる。軽量スレッド（goroutine 相当）の
   前提条件で、これが本来の目的
3. **非局所脱出が自前の操作になる**。いま Rust の `Result::Err` と `panic` に相乗りしている
   `break`/`return`/`return-from`/`throw` が、継続スタックを畳む操作として書き直される

---

## 2. 出発点 — トランポリンは半分できていた

旧評価器には既にトランポリンがあった（以下はいずれも削除済み）:

```rust
enum Step {
    Done(Value),
    Tail(Value, Value),
}
```

`eval_core` のループは `Step::Tail` を受けたら再帰せずにループ先頭へ戻っていたので、
**末尾位置は既にスタックを食わなかった**。各 `Op` の実装が「最後の本体式は `Step::Tail` で
返し、それ以外は `eval_core` を再帰呼び出しする」という形で一貫していた:

```rust
for e in rest {
    self.eval_core(heap, *e, env)?;     // ← 再帰
}
Ok(Step::Tail(*last, env))              // ← 末尾はジャンプ
```

CPS 化とは、**この `for` ループを継続フレームに置き換える**ことに尽きた。
`Step::Tail` の役目は `sequence_state`（残り 1 個ならフレームを積まずに `Eval` へ）が継いでいる。

---

## 3. 3 つの状態

```
State::Eval(form, env)   この形を評価する
State::Apply(value)      値ができた。継続スタックの先頭に渡す
State::Unwind(exit)      脱出中。巻き戻しながら受け手を探す
```

ループは 1 つ。Rust の再帰も `?` による脱出も使わない。

```
loop {
    state = match state {
        Eval(form, env)  => step_eval(form, env),   // 分解して1歩進む
        Apply(v)         => match stack.pop() {
                                None       => return Ok(v),   // 完了
                                Some(f)    => resume(f, v),
                            },
        Unwind(exit)     => match stack.pop() {
                                None       => return Err(exit),
                                Some(f)    => unwind_through(f, exit),
                            },
    };
}
```

- `step_eval` は**部分式を持つなら継続フレームを 1 つ積んで `Eval(部分式)` へ落ちる**。
  葉なら `Apply(値)` へ
- `resume` は「その値で何をするか」を実行する。次の部分式があればフレームを積み直す
- `unwind_through` は `Unwind`/`Catch`/`Loop`/`Block` フレームだけに反応し、他は捨てる

**この 3 状態は「継続を脱関数化した CPS」**そのもの。継続をクロージャで表すと毎ステップ
ヒープ確保が要り、この処理系の GC には重すぎるので、種類が有限であることを使ってタグ付きの
フレームにする。結果として実装は「明示的なスタックを持つ状態機械」になる — CPS 変換の理論的な
帰結と、書きたいものが一致する地点がここ。

---

## 4. 継続フレーム — 27 種

45 個の `Op`（`core_eval.rs` の `enum Op`）に対して、フレームは 27 種。`step_cps` の `match` に
catch-all は無いので、**`Op` を足してフレームを与えないとビルドが落ちる**。

### 4.1 フレーム不要（葉）— 19 個

`Int` `Float` `Bignum` `Ratio` `Char` `Bool` `Str` `Sym` `Unit` `Var` `Quote` `Lambda`
`FnRef` `MethodRef` `Global` `CompileFn` `Trace` `Untrace` `DisassembleFn`

これだけが `core_eval.rs` に `Interp::eval_leaf` として残っている。`Break` も部分式を持たないが、
値ではなく `State::Unwind` へ行く。

### 4.2 部分式が 1 つ

| Op | フレーム | 値を受け取って何をするか |
|---|---|---|
| `If` | `If { form, env }` | `Bool` で分岐先を選び `Eval(分岐, env)`（末尾ジャンプ） |
| `Panic` | `Panic` | 文字列を取り出して `Unwind(Panic)` |
| `Set` | `Set { sym, env }` | **セルでなく名前を持つ** — 引くのは値が揃ってから |
| `SetGlobal` | `SetGlobal { form }` | グローバルへ書く |
| `FieldGet` | `FieldGet { idx }` | フィールドを読む |
| `DynValue` | `DynValue` | 箱から中身を取る |
| `Match` | `MatchArms { form, env }` | 腕を順に試し、当たった腕で環境を伸ばして `Seq` |
| `Throw` | `Throw { tag }` | `Unwind(Throw)` |
| `Return` / `ReturnFrom` | `Return` / `ReturnFrom { name }` | `Unwind` |
| `DynNew` / `DynUpcast` | `DynNew { form }` / `DynUpcast { form }` | 箱に入れる／vtable を差し替える |

### 4.3 部分式が複数

- **`Seq { rest, env }`** — 本体の残り。`sequence_state` が作る。
  **残り 1 個なら積まずに `Eval` へ**（`Step::Tail` の役目を継いだのはここ）
- **`Args { form, done, env, kind }`** — 引数を 1 つずつ `done` に積む。`ArgsKind` が
  `Call`/`Construct`/`Assoc`/`DynCall` を分けるが、違いは**ノードの何番目から引数が始まるか**
  （4/5/7/5）だけ。揃ったら `finish_args` が種別ごとの処理へ振る
- **`ApplyCallee { form, env }` → `ApplyArgs { form, callee, done, env }`** — `apply` は
  呼ぶものが先に来るので 2 段
- **`LetInit { .. }`** — 初期化式を 1 つずつ。揃ったら `extend_env` して `Seq`
- **`FieldSetObj { form, env }` → `FieldSetVal { obj, idx }`** — レシーバ → 値の 2 段

### 4.4 制御

| Op | フレーム | 巻き戻しでの振る舞い |
|---|---|---|
| `Loop` | `Loop { body, rest, env }` | `Break` を吸って `Apply(())`、`Return(v)` を吸って `Apply(v)` |
| `Block` | `Block { name }` | 同名の `ReturnFrom(v)` を吸って `Apply(v)` |
| `Catch` | `Catch { tag }` | 同タグの `Throw(v)` を吸って `Apply(v)` |
| `UnwindProtect` | `Protect { cleanup, env }` → `CleanupValue { value }` / `CleanupUnwind { pending }` | **どの脱出でも** cleanup を評価し、終わったら保留したものを再開する |
| `Step` | `Step { was_stepping, was_quiet }` | ステッパの状態を戻す |
| 監視された呼び出し | `TracedCall { name, depth }` | 戻り値を印字する |

`UnwindProtect` が 3 フレームに割れているのは、cleanup が**何を中断して走っているか**で戻り先が
違うから — 値を持って抜けようとしていたのか（`CleanupValue`）、脱出中だったのか
（`CleanupUnwind`）。cleanup 自身の脱出が飛行中のものに勝つ規則は、`CleanupUnwind` を resume
せずに新しい `Unwind` がそのまま上へ行くことで自然に出る。

**`Call` フレームは無い。** 関数に入るのは `enter_fn` で、本体はそのまま `Seq` になる — だから
呼び出しの末尾位置が末尾ジャンプになる。例外は 2 つ:

- **トレース／ステップ中の呼び出し**は `TracedCall` を積むので末尾位置を失う。戻り値を印字する
  には戻ってくる場所が要るため
- **`Interp::apply`**（コンパイル済みコードからの再入口）は本体の最後の式を `eval_core` で
  評価するので、そこはネイティブフレームが待ったままになる

## 5. GC — 継続フレームが持つ値のルート

### 既存の `roots` にそのまま乗った

`crates/typelisp-mem/src/heap.rs` の `roots` は**厳密な LIFO** で、`push_root`/`pop_root`/
`truncate_roots` で操作する。**継続スタックも LIFO なので、そのまま乗る**:

```text
roots: [ ..., state-env, state-form, frame0 の値.., frame1 の値.., ... ]
              ^ sbase                ^ frame0 の base
```

- 状態が運ぶもの（環境と形、または `Apply` の値）は**底の 2 スロット**に置き、`set_root` で
  上書きする。押し込まないので、末尾ループがルートスタックを伸ばさない
- フレームを積むときにその値を `push_root` し、積んだ時点の `root_count()` をフレームと一緒に
  覚える。畳むときに `truncate_roots(base)` — 既存の `RootScope` と同じ規律

**新しいルート源は要らなかった。** `heap.rs` は Phase A では 1 行も変えていない。

### 実装で確定した 2 つの規律

1. **`truncate_roots` → `push` → `set_state` の窓では確保しない。** フレームを畳んで次の状態を
   root するまでの間、生きている値がどのルートからも見えない瞬間がある。ここで `cons` すると
   GC が走って回収されうる
2. **`resume` はフレームを積まない。** 積むべきフレームを**返し**、呼び出し側が
   truncate → push → `set_state` の順に実行する。resume の中で push すると、直後の
   `truncate_roots` が今積んだばかりのルートを巻き添えにする（LIFO なので位置で切るため）

### 消えたもの

- **インタプリタが `in_flight_throw`（`heap.rs` の 1 スロット）を使わなくなった。** 飛行中の値は
  `State::Unwind` が持ち、状態スロットから root される。`Loop` の `Break` の値が Rust の
  `Box` で運ばれ「コレクタから見えないが、巻き戻しが allocate しないから安全」という綱渡りも
  同じ理由で消えた。

  ただし**スロット自体は残る**。コンパイル済みコードの `throw` は Rust の panic で運ばれ、
  その payload はコレクタから見えないので、`typelisp_rt::park_throw` が今もそこへ値を置く。
  **「同時に飛ぶ throw は高々 1 つ」という前提は compiled 経路では生きたまま**で、
  Phase B で 2 つのタスクが同時に巻き戻せるようになったときに見る必要がある——
  Phase A で先に解消できたのはインタプリタ側だけ
- **引数を集める間だけの `RootScope`**。集めかけの引数は `Args` フレームが持ち続けるので、
  フレームの寿命がそのまま root の寿命になる

### Phase B — タスクごとに分ける

`gc()` は現在 5 つの源を歩く: `roots` / `permanent_roots` / `session_roots` /
`in_flight_throw` / `cell_registry`。**タスクを入れると `roots` の単一 LIFO が壊れる**
（複数の実行文脈が同時にフレームを積むため）ので、そこで初めて「タスクごとのルートスタック」
という源に作り変える。

## 6. compiled コードとの境界

**CPS 化されるのはインタプリタだけ。** LLVM が生成したネイティブコードは普通の Rust
スタックを使うので、そこには継続スタックが無い。

境界は 2 方向あり、どちらも既存の仕組みがそのまま残る:

- **interpreted → compiled**: `Interp::enter_compiled`（`eval/interp.rs:1173`）。
  継続スタック上の**1 フレーム**として扱う。呼び出しが返るまでインタプリタは何もしない
- **compiled → interpreted**: `rt_apply_any` → `call_interpreted_closure`（`core_eval.rs`）。
  **新しい継続スタックを立てて完了まで回す**。ネイティブフレームが待っているので、
  末尾位置を諦めてその場で評価しきるしかない
  —— **Phase C5 でこの向きは変わった**（`rt_apply_any` は削除。呼び出し地点は呼び先を
  名指ししてドライバへ戻り、インタプリタの呼び先はタスク自身の継続スタックに積まれる）。
  `docs/dev/compiled-cps-design.md` の C5 節を参照

帰結: **タスクの切り替え点は interpreted 経路にしか置けない。** compiled 関数の実行中は
切り替えられない。並行機構ではこれを「compiled 呼び出しは切り替えの単位として atomic、
その間はその OS スレッドを占有する」という制限として受け入れる。

---

## 7. 移行で分かったこと

方法は当初のとおりに進めた。新しいファイル `core_cps.rs` に書き、**未実装の `Op` は
`unimplemented!()` にして旧経路へフォールバックしない**（フォールバックは黙って動いてしまい、
移行の穴を隠す）。全段階を GC ストレス（`heap.set_gc_stress(true)`）下でテストした。
切り替え後は既存の 117 ファイルがそのまま回帰試験になった — CPS 化は意味を変えない書き換えなので、
1 本でも落ちれば移行のバグ。

以下は、設計の段階では見えていなかったもの。

### 7.1 `Err` が継続スタックを素通りしていた（設計の見落とし）

`State::Unwind` は「インタプリタ自身が起こした脱出」のために作ったが、**エラーはそこからだけ
来るのではない**。コンパイル済みコードからの `throw` は Rust の `Err` として `step_cps` の
戻り値に現れるので、素直に `?` で返すと `Frame::Catch` を飛び越えてしまう。同じ理由で
`panic` が `unwind-protect` の cleanup を走らせなかった。

**すべての `Err` を `State::Unwind` に載せ直す**のが答え。継続スタックが空になって初めて
`Err` として関数から出る。`step_cps`・`resume`・`unwind_through` の 3 箇所すべてで、
エラーは「返す」のではなく「状態にする」。

### 7.2 `enter_fn` のルート漏れ（本物のバグ）

`extend_env` の結果を root せずに `core::list`（確保する）を呼んでいた。GC ストレス下の
`bignum_ratio_gc_test` が `gc-root-audit: set_root given freed cell` で捕まえた。監視付きの
呼び出しの経路にも同じ穴があった。

[[typelisp-checker-syntax-rebuild-gc-leak]] と同じ形 — **Rust のローカルに置いた `Value` は
コレクタから見えない**。フレームを設計しても、フレームを作るまでの数行は従来どおりの規律が要る。

### 7.3 エラーのソース位置はフレームにも要る

旧評価器はトランポリンの各周回で `EvalError::at` を呼んでいた（`form` が手元にあるので位置が
読める）。CPS では `State::Eval` で `heap.cons_loc(form)` を読むだけでは足りない —
**resume 中に起きたエラーは、フレームを積んだ form に属する**。だからフレーム自身にも
`Option<Loc>` を持たせている。

### 7.4 監視された呼び出しは本体を 1 フォームに包む

`TracedCall` フレームを積むと、本体が複数フォームのときに「本体を回すフレーム」と
「戻り値を印字するフレーム」の 2 つが要る。`resume` は 1 つしか返せない。
**本体を `(let () E...)` で包んで 1 フォームにする**ことで解いた。

### 完了の判定

- `scripts/test-serial.sh` が全 117 ファイル green（129 の test result、失敗ゼロ）
- **深さ 20,000 の非末尾再帰が通る**。同じ形を旧評価器に通すと、既定のテストスタックでも
  `RUST_MIN_STACK=32MB` でも `fatal runtime error: stack overflow` で abort した
  （削除する前に実測し、`core_cps.rs` のテストにコメントとして残した）

## 8. 失うもの

- **Rust のスタックトレースに Lisp の呼び出し階層が出なくなる。**
  継続スタックを印字するデバッグ機能が代わりに要る（`Op::Step` のデバッガと同じ場所）
- 性能は測らない（`docs/dev/` の方針）。フレームの `Vec` 操作が増える一方、Rust の
  関数呼び出しは減る。速度を目標にも根拠にもしない
