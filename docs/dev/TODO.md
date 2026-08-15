# typelisp 開発 TODO

最終更新: 2026-08-15 / ブランチ: `feature/compiled-unwind`

このドキュメントは**現在残っている作業のみ**を記録する。

## 残っている作業

### `catch`/`throw`/`unwind-protect` の compiled 側（作業中）

interpreted 層は入っている（checker・core form・`EvalError::Throw`・評価）。
関数を跨ぐ throw、`unwind-protect` の cleanup、捕まらない throw のエラー、
シンボルごとの型検査まで動く。

**残りは compiled 側。** 今 `compile` すると
`compile: the free-variable walk does not know the tag \`catch\`` で止まる
（明確なエラーであって壊れてはいない）。方式は LLVM EH 一択——`catch`/`throw` は
関数を跨ぐので `break`/`return` の関数内戻り値方式では書けない。実現可能性は
`tests/compiled_unwind_test.rs` で実証済み（`invoke` + cleanup 専用 `landingpad` +
`resume` が JIT で動き、cleanup が走ったうえで panic は先の catch まで届く）。

着手順:

1. `src/check/registry.rs` + `Interp` の `rt_llvm_call` に
   `build-invoke`/`build-landing-pad`/`build-resume`/`set-personality-function`
   を新設。島は registry が公開した `llvm-builder` メソッドしか呼べず、
   現状 `build-*` は 27 個で EH 命令が無い。
   **op-id は名前の FNV ハッシュ（`compile::symbols::llvm_op_id`）で連番ではないので、
   追加しても既存 op の id は動かず島の成果物は無効化されない** — ここは島と独立に
   入れて単体検証できる
2. `src/compile/core_freevars.rs`（今の停止点）と `src/compile/core_bridge.rs`。
   **タグの `Repr` をノードに焼き込む**こと: 投げた値は境界を跨ぐので、`apply` が
   arg/ret repr を運ぶのと同じ理由が要る
3. `crates/typelisp-rt` に実行中 throw のスロットと
   `rt_throw`/`rt_throw_matches`/`rt_throw_take_value`
4. 島 `src/compiler.rs` の `compile-catch`/`compile-throw`/`compile-unwind-protect`。
   **personality は `rust_eh_personality`**。`__gxx_personality_v0` は JIT では
   動くが AOT の実リンク行（`cc obj libtypelisp_rt.a`）で undefined になる（実測）
5. GC ルート: unwind は compiled フレームの root pop を全部飛ばすので、
   catch の landing pad で `rt_truncate_sexpr_roots` により catch 入口の深さへ
   巻き戻す（`loop` の `loop-root-base` と同じ手）
6. interpreted の `EvalError::Throw` と rt 側の実行中 throw の相互変換
7. `scripts/regen-compiler-island.sh` / `scripts/regen-prelude-bitcode.sh` で
   成果物を再生成し、`compile_test`/`compile_file_test` を含めてフルスイート

設計判断（`Result` とコンディションの二重化を避け、コンディションは入れず
`catch`/`throw`/`unwind-protect` に一本化する）は
[language-design.md](language-design.md) §7 の改定として書くこと——まだ未着手。

### `fatal()` の到達可能な呼び出し元がまだ abort する

`rt_panic` は `extern "C-unwind"` になり、compiled な `(panic ...)` は catchable な
`EvalError::Panic` として返るようになった（JIT は `compile::catch_compiled_panic`、
AOT は `typelisp_rt::rt_run_entry` が受ける）。REPL も compiled な panic で落ちなくなった。

残っているのは **`typelisp_rt::fatal()` を経由する経路**。`fatal()` の doc は「ここに来るのは
`compiler.rs` の契約違反だけ」と書いているが、実際にはユーザ入力で到達する:

| 式 | 現状 | 本来 |
|---|---|---|
| `(random 0)` / `(random -5)` | abort | `panic: random: bound must be positive` |
| `(rem 5 0)`（i32/i64/bignum/ratio） | abort | `panic: divide by zero` |
| `(mod 1/2 0)` | abort | 同上 |
| `(floor-div 5 0)`/`(truncate-div 5 0)`/`(ceiling-div 5 0)`/`(round-div 5 0)` | abort | 同上 |
| compiled なユーザコードの `(/ a b)` / `(mod a b)` / 範囲外 `substring` | abort | 同上 |
| 範囲外の `vector-ref`（`rt_struct_field_get`/`_set`） | abort | 同上 |

`fatal()` の呼び出し元 100 箇所超のうち大半は本当に内部不変条件（arity 違反、タグ違い）なので、
一括で `rt_panic` の機構に載せ替えるか、到達可能なものだけ選ぶかを決めること。
載せ替える関数は `extern "C-unwind"` にする必要がある。

検査を interpreted prelude に逃がしているテストが `tests/numeric_test.rs` の
`run_interpreted` に 1 つ残っている（`random`）。この項目が終わったら消せる。

### prelude ビットコードの起動時コスト（+198〜258 ms）

417 KB のビットコード全体を MCJIT が起動時に解決する分（冗長パースの除去で削れる分は既に
削ってあり、残りは解決そのもの）。実行速度とのトレードオフとして現状は常時有効（ユーザー判断）。

2026-08-14 の「コンパイル経路の穴」を塞ぐ作業で成果物は 244 KB → 417 KB になり、コストもほぼ
比例して増えた（`scripts/bench-prelude.sh`、N=3）:

| | 成果物 | インストール | 空ファイルに対する `typl` 全体（release, N=5） |
|---|---|---|---|
| 穴を塞ぐ前 | 244 KB | +105〜109 ms | 0.84 s |
| 現在 | 417 KB | +198〜258 ms | 0.98 s |

遅延インストール——呼ばれた関数だけ解決する——で削るなら、まず `scripts/bench-prelude.sh` で
測り直してから。

作業を始めるときはここに項目を足し、終わったら（経緯・設計判断を
[implementation-log.md](implementation-log.md) へ書いたうえで）ここから消す。

次に何かを実装するなら、着手候補の地図は
[cl-missing-classes-and-methods.md](cl-missing-classes-and-methods.md) §3（横断的な欠落）。
ただしあれは TODO ではなく残差の測定であり、優先度も筆者の見立てであって確定した方針ではない。

## 関連ドキュメント

| 知りたいこと | 参照先 |
|---|---|
| 完了した実装の経緯・設計判断 | [implementation-log.md](implementation-log.md) |
| 言語仕様の確定事項・非採用と決めた機能 | [language-design.md](language-design.md)（非採用リストは §9） |
| Common Lisp と比べてまだ無いクラス・メソッド | [cl-missing-classes-and-methods.md](cl-missing-classes-and-methods.md) |
| ビルド・テストの実行方法 | [development.md](development.md) |
| ユーザ向けの関数・構文リファレンス | [functions.md](../functions.md) / [syntax.md](../syntax.md) |
