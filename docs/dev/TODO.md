# typelisp 開発 TODO

最終更新: 2026-08-15 / ブランチ: `feature/compile-strict-names-and-prelude-bitcode`

このドキュメントは**現在残っている作業のみ**を記録する。

## 残っている作業

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
