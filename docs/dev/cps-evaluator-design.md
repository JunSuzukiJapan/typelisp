# 評価器の CPS 化（設計）

最終更新: 2026-09-08 / ブランチ: `feature/cps-evaluator`

この文書は、ツリーウォーク評価器を **Rust の再帰から継続スタックへ**移す作業の設計を記録する。
実行の意味は変えない。変えるのは「計算の残り」がどこにあるか、それだけ。

作業計画（段階と完了条件）は別途プランに、完了後の経緯は
[implementation-log.md](implementation-log.md) に書く。

---

## 1. なぜ

**Lisp の再帰の深さが、そのまま Rust のスタックの深さになっている。**

`Interp::eval_core`（`crates/typelisp-front/src/eval/interp/core_eval.rs:204`）は
`step_core` を呼ぶループだが、`step_core` の中から `self.eval_core(...)` を呼び返している
箇所が **36 箇所**ある。末尾でない部分式 — `if` の条件、引数、`let` の初期化式、`match` の
scrutinee — はすべてここを通る。

その帰結が、`scripts/test-serial.sh:52` の `RUST_MIN_STACK=32MB` と、
`src/bin/bootstrap_island.rs:26` が 64MB スタックのスレッドを明示的に立てていること。

得られるものは 3 つ:

1. **深い再帰が書ける**。Lisp の再帰深度が GC ヒープの容量だけで決まるようになる
2. **実行状態がデータになる** — 保存・中断・再開・移動ができる。軽量スレッド（goroutine 相当）の
   前提条件で、これが本来の目的
3. **非局所脱出が自前の操作になる**。いま Rust の `Result::Err` と `panic` に相乗りしている
   `break`/`return`/`return-from`/`throw` が、継続スタックを畳む操作として書き直される

---

## 2. 現状 — トランポリンは半分できている

```rust
// core_eval.rs:191
enum Step {
    Done(Value),
    Tail(Value, Value),
}
```

`eval_core` のループは `Step::Tail` を受けたら再帰せずにループ先頭へ戻る（`:210-224`）。
**末尾位置は既にスタックを食わない。** 各 `Op` の実装が「最後の本体式は `Step::Tail` で返し、
それ以外は `eval_core` を再帰呼び出しする」という形で一貫している:

```rust
// core_eval.rs:334-337 (Op::Let)
for e in rest {
    self.eval_core(heap, *e, env)?;     // ← 再帰
}
Ok(Step::Tail(*last, env))              // ← 末尾はジャンプ
```

CPS 化とは、**この `for` ループを継続フレームに置き換える**ことに尽きる。
`Step::Tail` の考え方はそのまま活きる。

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

## 4. 継続フレーム

45 個の `Op`（`core_eval.rs:66-111`）を、必要なフレームで分類する。

### 4.1 フレーム不要（葉）— 19 個

`Int` `Float` `Bignum` `Ratio` `Char` `Bool` `Str` `Sym` `Unit` `Var` `Quote` `Lambda`
`FnRef` `MethodRef` `Global` `CompileFn` `Trace` `Untrace` `DisassembleFn`

即座に `Apply(値)` へ。`Break` も葉だが `Unwind` へ行く。

### 4.2 部分式が 1 つ

| Op | フレーム | 値を受け取って何をするか |
|---|---|---|
| `If` | `If { then, else, env }` | `Bool` で分岐先を選び `Eval(分岐, env)`（末尾ジャンプ） |
| `Panic` | `Panic` | 文字列を取り出して `Unwind(Panic)` |
| `Set` | `Set { cell }` | セルへ書く |
| `SetGlobal` | `SetGlobal { path }` | グローバルへ書く |
| `FieldGet` | `FieldGet { field }` | フィールドを読む |
| `DynValue` | `DynValue` | 箱から中身を取る |
| `Match` | `MatchArms { form, env }` | 腕を順に試し、当たった腕で `extend_env` して `Seq` |
| `Throw` | `Throw { tag }` | `Unwind(Throw)` |
| `Return` / `ReturnFrom` | `Return` / `ReturnFrom { name }` | `Unwind` |

### 4.3 部分式が複数

- **`Seq { rest, env }`** — 本体の残り。`resume` で先頭を取り出し、
  **残り 1 個なら積み直さずに `Eval` へ**（これが `Step::Tail` の役目を引き継ぐ）
- **`Args { callee, done, rest, env }`** — 引数を 1 つずつ。`done` に積み上げる。
  `Call` `Assoc` `DynCall` `Apply` `Construct` `DynNew` `DynUpcast` が共有する
- **`LetInit { done, rest, body, env }`** — `let` の初期化式を 1 つずつ。揃ったら
  `extend_env` して `Seq` を積む（現 `extend_let`、`:1085-1111`）
- **`FieldSet { obj, field }`** — レシーバ → 値の 2 段

### 4.4 制御

| Op | フレーム | 巻き戻しでの振る舞い |
|---|---|---|
| `Loop` | `Loop { body, pos, env }` | `Break` を吸って `Apply(())`、`Return(v)` を吸って `Apply(v)` |
| `Block` | `Block { name }` | 同名の `ReturnFrom(v)` を吸って `Apply(v)` |
| `Catch` | `Catch { tag }` | 同タグの `Throw(v)` を吸って `Apply(v)` |
| `UnwindProtect` | `Unwind { cleanup, env }` | **どの脱出でも** cleanup を評価してから巻き戻しを再開 |
| `Call` | `Call { .. }` | 関数フレーム。トレース（`Interp::enter`）と暗黙 block の境界 |

`UnwindProtect` の「cleanup 自身の脱出が飛行中のものに勝つ」規則は、cleanup の評価中に
新しい `Unwind` が起きたら古いほうを捨てる、というスタック操作として自然に出る。

---

## 5. GC — 継続フレームが持つ値のルート

### Phase A — 既存の `roots` をそのまま使う

`crates/typelisp-mem/src/heap.rs:83` の `roots` は**厳密な LIFO** で、`push_root`/`pop_root`/
`truncate_roots` で操作する。**継続スタックも LIFO なので、そのまま乗る**:

- フレームを積むとき、そのフレームが持つ `Value`（`env`、集めかけの引数、`Loop` の本体…）を
  `push_root` する
- フレームごとに積んだ時点の `root_count()` を覚えておき、フレームを畳むときに
  `truncate_roots(base)` する — 既存の `RootScope`（`heap.rs:2174`）とまったく同じ規律
- `Args` フレームのように値が増えていくものは、1 つ増えるたびに `push_root` すればよい

**新しいルート源は要らない。** `heap.rs` は Phase A では変更しない。

### 消えるもの

- **`RootScope` による一時的な rooting の大半**。いまは「引数を集める間だけ `push_root`」
  （`core_eval.rs:906-911`）という形が随所にあるが、集めかけの引数は `Args` フレームが
  持ち続けるので、フレームの寿命がそのまま root の寿命になる
- **`in_flight_throw`（`heap.rs:88` の 1 スロット）**。飛行中の値は `State::Unwind` が持ち、
  巻き戻し中も root されたままにできる。`Loop` の `Break` の値がいま Rust の `Box` で運ばれ
  「コレクタから見えないが、巻き戻しが allocate しないから安全」という綱渡り
  （`core_eval.rs:619-624` のコメント）も同じ理由で消える

### Phase B — タスクごとに分ける

`gc()`（`heap.rs:2019`）は現在 5 つの源を歩く: `roots` / `permanent_roots` / `session_roots` /
`in_flight_throw` / `cell_registry`。**タスクを入れると `roots` の単一 LIFO が壊れる**
（複数の実行文脈が同時にフレームを積むため）ので、そこで初めて
「タスクごとのルートスタック」という 6 つ目の源に作り変える。

`in_flight_throw` が 1 スロットしかない前提も、そこでどのみち壊れる。だから Phase A の
うちに `State::Unwind` へ移して**先に解消しておく** — Phase B で 2 つの前提を同時に
壊さずに済む。

## 6. compiled コードとの境界

**CPS 化されるのはインタプリタだけ。** LLVM が生成したネイティブコードは普通の Rust
スタックを使うので、そこには継続スタックが無い。

境界は 2 方向あり、どちらも既存の仕組みがそのまま残る:

- **interpreted → compiled**: `Interp::enter_compiled`（`eval/interp.rs:1173`）。
  継続スタック上の**1 フレーム**として扱う。呼び出しが返るまでインタプリタは何もしない
- **compiled → interpreted**: `rt_apply_any`（`crates/typelisp-rt/src/lib.rs:1882`）→
  `call_interpreted_closure`（`core_eval.rs:1005`）。**新しい継続スタックを立てて完了まで回す**。
  現在この関数が「native フレームが待っているので `Step::Tail` を返さず自分で評価しきる」と
  書いているのと同じ理由

帰結: **タスクの切り替え点は interpreted 経路にしか置けない。** compiled 関数の実行中は
切り替えられない。並行機構ではこれを「compiled 呼び出しは切り替えの単位として atomic、
その間はその OS スレッドを占有する」という制限として受け入れる。

---

## 7. 移行の方法

- **新しいファイル `crates/typelisp-front/src/eval/interp/core_cps.rs` に書く。**
  旧 `core_eval.rs` は切り替えまで手を触れない
- **未実装の `Op` は `unimplemented!()`。旧経路へフォールバックしない** —
  フォールバックは黙って動いてしまい、移行の穴を隠す
- 段階ごとに新経路を直接叩くテストを `tests/cps_eval_test.rs` に足す。
  既存の 117 本は最後の切り替えで初めて新経路を通る
- **全段階で GC ストレス（`heap.set_gc_stress(true)`）を回す**。継続フレームが持つ値の
  ルート漏れが最も起きやすい失敗（`check/forms.rs:176` が 5 例目のルートリークを記録している）

### 完了の判定

`scripts/test-serial.sh` が**移行前と同じ結果**になること。CPS 化は意味を変えない書き換えなので、
1 本でも落ちれば移行のバグ。加えて:

- 現在スタックオーバーフローする深さの再帰が通ること（CPS 化の目に見える成果）
- `RUST_MIN_STACK=32MB` と bootstrap の 64MB スタックを下げられるかを測る

---

## 8. 失うもの

- **Rust のスタックトレースに Lisp の呼び出し階層が出なくなる。**
  継続スタックを印字するデバッグ機能が代わりに要る（`Op::Step` のデバッガと同じ場所）
- 性能は測らない（`docs/dev/` の方針）。フレームの `Vec` 操作が増える一方、Rust の
  関数呼び出しは減る。速度を目標にも根拠にもしない
