# チェッカーのエラー回復モード 検証手順

対象: `Checker::set_recover` によるエラー回復モードと、それを使う LSP の
`diagnostics_for` / `candidates_for`（設計は
[implementation-log.md](implementation-log.md)「チェッカーのエラー回復モード」節）。

検証する挙動は3点:

1. **複数診断** — 型エラーが複数あるファイルで、全エラーが同時に `publishDiagnostics` される
   （従来は最初の1件で打ち切り）。
2. **非 catchall `match` アーム内の補完**（本改修の動機バグ）— catchall でないアーム本体で
   補完すると、後続アームが truncate で消えて非網羅になっても、ローカル束縛
   （パターン束縛・パラメータ）が候補に出る。
3. **エラーがあっても hover / goto-definition が効く** — 部分的な `Typed` 木から解決される。

---

## 1. 自動テスト（コアロジック）

厳格モード（既存 600+ 件）が無変更であることと、回復モードの単体挙動を確認する。

```sh
source scripts/setup-cargo-env.sh   # 素の cargo を使えるようにする（初回のみ）

# 回復モードの単体テスト（lib 側、Checker::check_form を直接駆動）
cargo test --test lsp_completion_test

# 補完前処理層＋candidates_for の E2E（bin 側ユニットテスト）
cargo test --bin typl-lsp

# 全体（厳格経路の無変更確認を含む）
cargo test
```

期待: すべて green。とくに `tests/lsp_completion_test.rs` の
`recover_mode_*` 7 件と、`src/bin/lsp.rs` の
`candidates_for_offers_locals_inside_a_truncated_non_catchall_match_arm` が
回復モードの中核を固定している。

主なテストと確認内容:

| テスト | 確認する境界 |
|---|---|
| `recover_mode_records_multiple_form_errors_and_keeps_checking_the_rest` | B5（最上位フォーム）— 複数エラー蓄積＋間の `defun` 生存 |
| `recover_mode_offers_locals_inside_a_non_catchall_match_arm` | B1（網羅性）— 動機バグ |
| `recover_mode_skips_a_bad_arm_but_keeps_the_good_arms_bindings` | B2（アーム単位）|
| `recover_mode_holes_one_bad_body_form_and_keeps_the_siblings` | B3（`check_seq` 要素）|
| `recover_mode_keeps_a_let_binding_whose_init_failed` | B4（`let` init）|
| `recover_mode_records_each_independent_error_with_its_own_location` | 各エラーが個別位置を持つ |

---

## 2. LSP スモークテスト（stdio 実測、自動）

実際の LSP トランスポート越しに上記3点を確認する。付属スクリプトが
`initialize` → `didOpen` → `completion` / `hover` を送り、応答を検証する。

```sh
cargo build --bin typl-lsp
python3 scripts/lsp-recover-smoke.py
```

期待出力:

```
[PASS] multiple diagnostics reported at once — 2 diagnostic(s): ...
[PASS] completion inside non-catchall match arm offers locals `o` and `v` — locals present
[PASS] hover works despite type errors in the file — hover returned a type

RESULT: all 3 checks PASSED
```

終了コード 0 = 全 PASS。release バイナリを使う場合は
`--bin target/release/typl-lsp` を渡す。

スクリプトが使う検証用ドキュメント（1行に収めて列位置を予測可能にしている）:

```lisp
(defvar (bad i32) true)                               ; 型不一致: i32 に bool
(defun f ((o Option<i32>)) i32 (match o ((Some v) v))) ; 非網羅: Some のみ
```

- **複数診断**: `didOpen` で `type mismatch` と `non-exhaustive match` の2件が返る。
- **補完**: 2行目のアーム本体 `v` の直前（空白の直後 = 識別子未入力）にカーソルを置いて
  補完要求。`handle_completion` が識別子以降を truncate し `(panic "")` を挿入、回復モードで
  再チェックして、ローカル `o`（パラメータ）と `v`（`Some` の束縛）を候補に含める。
  ※カーソルを `v` の直後に置くと入力中識別子の prefix が `"v"` になり、`o` は
  prefix フィルタで除外される（正しい LSP 挙動）。空 prefix の位置で確認すること。
- **hover**: 同じ（型エラーを含む）ファイルで `(match o` の `o` にホバーし、型が返る。

---

## 3. 手動確認（エディタ）

エディタの LSP クライアントに `typl-lsp` を接続し、`.typl` ファイルで:

1. わざと型エラーを2箇所以上入れ、両方に赤波線（診断）が同時に出ることを確認。
2. `Option<i32>` を受け取る関数内で `(match o ((Some v) …))` と catchall なしのアームを書き、
   アーム本体で補完を呼び、`o` と `v` が候補に出ることを確認。
3. ファイルに型エラーがある状態で、束縛変数にホバー／goto-definition が効くことを確認。

---

## 注意

- 回復モードは LSP（`diagnostics_for` / `candidates_for`）専用。CLI（`typl <file>`）・REPL・
  prelude ロードは厳格モード（`recover=false`）のままで、最初のエラーで停止する挙動は変えていない。
  この分離が壊れていないことは「1. 自動テスト」の全 green で担保される。
- スモークスクリプトは依存を持たない単一ファイルを対象にしている。マルチファイル
  （`use` 依存）の回復挙動は現状スクリプト化していない（依存側は従来どおり file:line:col 付きで
  文書先頭に集約表示される）。
