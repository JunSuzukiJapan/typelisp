# typelisp — 次回やること / 選択肢（ハンドオフ）

最終更新: 2026-06-10 / ブランチ: `feature/typed-lisp-impl`（`main` の 9 コミット先行）

このドキュメントは、実装を中断した時点の状態と、次回着手する作業・選択肢をまとめたもの。
文法仕様は [grammar.md](grammar.md)。

---

## 1. 現在の状態（完了済み）

`read → 型検査 → JIT 実行` のパイプラインが通っており、**85 テスト green**。

| 区分 | 内容 |
|---|---|
| M0 | ツールチェーン移行（inkwell 0.5 / llvm@18）。仮 builtin ABI 廃止 |
| M1 | リーダー（float/char/基数/サフィックス/`;`・`#\| \|#`コメント/エスケープ/nil） |
| M2 | `src/types/`：`Type` enum ＋ `parse_type` |
| M3 | 型付き AST（`Form`/`TypedExpr`/`ExprKind`）＋ `make_ast` head-symbol ディスパッチ |
| M4 | 旧ツリーウォーク評価器を削除 |
| M5 | `src/type_inference/`：双方向型検査、`CheckedProgram` |
| M6–M8 | ネイティブコンパイラ（defun/再帰/let/if/cond/when/unless/progn/while/dotimes/loop/setf/incf/decf/比較/算術） |
| M9 | `src/gc/`：精密 mark-sweep GC（header/TypeInfo/heap/shadow stack）。Rust 単体 7 テスト |
| M10 | 構造体のコード生成統合（gc_alloc・フィールド・shadow stack。GC 圧迫テスト合格） |
| M11 | クロージャ（自由変数解析・GC env・lifted 関数・間接呼び出し） |
| M13 | defmethod コード生成（インスタンス/スタティック/プリミティブ、静的単一ディスパッチ） |
| M14 | match コード生成（int/bool/char：リテラル/`\|`/range/wildcard/bind） |

**動作例**: `(fact 10)=3628800`、相互再帰、構造体＋メソッド、match、クロージャ（キャプチャ`=105`・高階`=12`）、30 万件 garbage 確保下で rooted 構造体が生存。

### ビルド/テスト
```sh
# .cargo/config.toml が LLVM_SYS_18x_PREFIX=/usr/local/opt/llvm@18 を設定済み
cargo build
cargo test                 # 全テスト
cargo test --test gc_test  # GC 単体
cargo run                  # (fact 10) => 3628800
```
> 注意: inkwell 0.5 は llvm18-0 まで対応。LLVM 12/13 は環境に無く、llvm@18 を使用。
> 別バージョンに変えるなら `Cargo.toml` の `features` と `.cargo/config.toml` の prefix を合わせる。

---

## 2. 残作業: M12 — String / Vec ランタイム（**文法拡張が前提**）

GC コア（M9）は String/Vec のオブジェクト形状とトレースを実装・テスト済み
（`Vec<String>` の `vec→buffer→要素` 推移マークまで確認済み）。
しかし **`grammar.md` は String/Vec を「型」として定義するだけで、生成・操作の構文が無い**。
また JIT ハーネス（`__main__`）は `i64` を返すため、String 値を観測する手段も無い。

### 着手手順（推奨順）
1. **文法拡張**（[grammar.md](grammar.md) に追記）。組込み関数の構文案:
   - 文字列: `(string-length s)→usize`, `(string-append a b)→String`, `(string-eq a b)→bool`,
     `(char-at s i)→char`
   - ベクタ: `(vec elem...)→Vec<T>`（型は要素から推論/注釈）, `(vec-new)→Vec<T>`,
     `(vec-push v x)→()`, `(vec-len v)→usize`, `(aref v i)→T`, `(setf (aref v i) x)`
   - これらを「組込み関数」として checker のシグネチャ表に登録（ジェネリックは
     既存の best-effort 単段推論を流用、または専用処理）。
2. **ランタイム関数**（`src/gc/` に `objects/{string,vec}.rs` を追加し `api.rs` で `extern "C"` 公開）:
   - `string_from_bytes(ptr,len)->*mut u8`, `string_concat(a,b)`, `string_len`, `string_bytes_ptr`
   - `vec_new(elem_size,elem_is_ptr,cap)->*mut u8`, `vec_push(v,&elem)->v`, `vec_get(v,i)->*elem`,
     `vec_set`, `vec_len`
   - Vec は **ctrl ブロック `{data_ptr,cap}` ＋ RawBuffer** の二段（push 成長でも Vec ポインタ不変）。
     `data_ptr` を GC 子としてトレース、buffer は `ElemInfo.elem_is_ptr` で要素走査要否を判定。
3. **コード生成**:
   - `ExprKind::Str` を `string_from_bytes`（バイト列は LLVM グローバル）で GC String 化。
   - String/Vec を GC 型として `is_gc_type`/`basic_type`(ptr) に追加 → shadow stack ルート対象に。
   - 上記組込みを `gc::*` への `add_global_mapping` 呼び出しに lower。
   - フィールド/`aref` の `setf` place を拡張（`Place::Index` の codegen は現状 stub）。
4. **観測手段**: `(string-length s)` 等が `usize`(=i64) を返すので、それを `__main__` の戻り値にして検証。
   `tests/compile_test.rs` に文字列連結＋長さ、Vec 構築＋総和、GC 圧迫下の生存テストを追加。

---

## 3. その他の選択肢

- **ブランチをマージ**: `feature/typed-lisp-impl` → `main`（PR 作成でも可）。
- **AOT コンパイル**: `TargetMachine::write_to_file` でオブジェクト出力＋ランタイムを staticlib 化。
  `add_global_mapping` をやめ外部シンボル参照に。`TypeInfo` は現在ホストポインタを IR に焼いている
  （`build_int_to_ptr`）ので、**AOT 化時は `TypeInfo` を LLVM グローバル定数として emit する変更が必須**。
- **整数オーバーフロー方針**: 現状ラップ（`(fact 21)` は i64 で静かにオーバーフロー）。
  `llvm.sadd.with.overflow.*` ＋ trap ヘルパに差し替え可能（算術 lower の局所変更）。
- **float コード生成**: 現状コンパイラは int/bool のみ。`f32/f64` の lower（`build_float_*`）を追加。
- **GC ハードニング**: サイズクラス別フリーリスト（現状 malloc-backed）、Miri/ASan 検証
  （ただし inkwell が LLVM をリンクするため Miri は GC を切り出さないと不可）。

---

## 4. 既知の制限（次に直す候補）

- **shadow stack の一時値**: ルート登録は「名前付きローカル（構造体/クロージャの params・let・self）」のみ。
  入れ子確保をまたぐ**無名一時値（例: `(Outer (Inner))` の Inner）は未ルート**。
  1MiB 閾値未満なら collection が起きず安全だが、厳密には一時値もスロットに退避すべき。
  （クロージャの env→closure の 2 段確保も同様。）
- **ジェネリック推論**: 構造体の型変数は単段マッチのみ（未拘束は `CannotInferGeneric`）。
- **match codegen**: struct/tuple/string/float パターンは未対応（int/bool/char のみ）。網羅性検査なし。
- **path 呼び出し** `(std::x::y ..)`: checker は型検査せず Unit、codegen は未対応。
- **`compile_and_run`(単一式)** は型検査を通さないため `e.ty` が None（既定 i64）。複数フォームは
  `run_program`（型検査済み）を使うこと。
- リーダーのジェネリック型はトークン内に空白不可（`Vec<T>` は可、`Vec< T >` は不可）— 仕様どおり。

---

## 5. 再開チェックリスト

1. `git checkout feature/typed-lisp-impl && cargo test`（85 green を確認）
2. 計画の全体像: `~/.claude/plans/lisp-common-bubbly-key.md`（マイルストーン進捗ログ）
3. M12 をやるなら **§2 の手順 1（文法拡張）から**。組込み構文を grammar.md に確定 → checker → runtime → codegen → tests。
4. 主要ファイル:
   - `src/read/reader.rs`（Cursor ベース）, `src/read/object.rs`
   - `src/types/{ty.rs,parse.rs}`
   - `src/compile/ast/{expr.rs,make_ast.rs}`
   - `src/type_inference/checker.rs`（シグネチャ表・双方向検査）
   - `src/compile/compiler.rs`（lower・構造体・メソッド・クロージャ・shadow frame）
   - `src/gc/{types,heap,shadow,api}.rs`
