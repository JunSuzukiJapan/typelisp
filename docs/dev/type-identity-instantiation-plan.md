# 型 identity に実体化を載せる計画

作成: 2026-08-31。状態: **未着手**（Stage 1 から）。

## 0. なぜ

`print-object` がジェネリック型に対して一度も発火しない、という症状から入った。
調べた結果、おかしいのは `print-object` ではなく**値が自分の実体化を持っていないこと**だった。

同じ言語の中に、両方の答えが並んでいる:

```rust
// checker.rs の `:dyn` 箱詰め — 型引数ごと
let concrete_key = mangle_type(&value.ty);      // "gen<wi>"

// type_key.rs の struct/enum 構築 — 型引数を捨てる
heap.intern_type_key(&type_key_of(p))           // "gen"
```

どちらも「具象型が手元にあるチェッカーの地点」なのに、片方だけ捨てている。
その結果:

- **`:dyn` 越しのメソッドは実体化ごとに正しい特殊化へ飛ぶ**（実測済み。
  `(say (gen::new (wi::new 1)))` → `"gen-of-INT"`、`ws` なら `"gen-of-STR"`)。
- **プリンタからは選べない**。値の鍵は `gen`、登録名は `print-object <i32>`。
- **downcast が不健全**（この計画で見つけた別の穴）:

  ```lisp
  (defstruct gen<T> (v T))
  (defun probe ((s Sexpr)) i32 (match s ((the gen<i32> g) (v g)) (_ 0)))
  (probe (the Sexpr (gen::new "abc")))   ; => internal error: sexpr: expected an i64 field
  ```

  `gen<string>` の値が `(the gen<i32> ...)` に通り、フィールドを `i32` として読む。
  今は下層のガードが「internal error」で止めているが、これは型エラーであるべきもの。

## 1. 決定事項（2026-08-31、ユーザ判断）

- **範囲は「全部」**。ユーザ定義 ADT だけでなく、組み込みジェネリック
  （`Vector<T>` / `HashTable<K,V>` / `cons-cell<K,V>` / `Option<T>` / `Result<T,E>`）も
  実体化を鍵にする。半分だけ直すと「型の identity は実体化まで含む——ただし組み込みは別」
  という分裂した規則になり、`type_key.rs` の冒頭が戒めている 2026-08-04 の事故
  （同じ概念を 2 通りに綴った）と同じ形になる。
- 鍵の綴りは **`mangle_type` の綴り**。非ジェネリック型では今日の `type_key_of` と
  文字単位で同じ（`Type::Named(p, [])` → `p.to_string()`）なので、既存の鍵は変わらない。

## 2. 影響の棚卸し（実測）

### 2.1 箱を作っている場所

| 場所 | 件数 | 実体化を知っているか |
|---|---|---|
| core IR の `construct`（ユーザ ADT・`Option`/`Result`・`defenum`） | 1 ノード | **知っている**（チェッカーが作る） |
| `interp.rs`（`Vector::new` ほか） | 3 | 呼び出し地点の静的型が要る |
| `typelisp-rt/lib.rs`（compiled 側の `rt_struct_new`/`rt_vector_*`/`cons-cell`） | 6 | 島から文字列で受け取る形が既にある |
| `typelisp-rt/stream_builtin.rs` | 5 | **署名で固定**（`Result<Option<string>, FileError>` 等） |
| `typelisp-rt/sys_builtin.rs` | 5 | 同上 |
| `typelisp-read/shim.rs` | 7 | 同上 |
| `typelisp-print/runtime.rs` | 3 | テスト用 |
| `src/compile/llvm_builtins.rs` | 2 | `Option<llvm-value>` で固定 |

**署名で固定されている 20 箇所は、その場で綴りを書けば終わる。** 配管が要るのは
「呼び出し地点の型引数が要る」ものだけ——`Vector::new`/`Vector::filled`/
`HashTable::new`/`cons-cell` の構築と、その compiled 版。

### 2.2 鍵を読んでいる場所

`heap_type_path` 6 / `heap_type_is` 3 / `type_key_of` 12 / `type_key_id` 7。
`type_key.rs` が全ての入口なので、変えるのはその中と呼び出し 30 箇所ほど。

## 3. 段階

### Stage 1 — 鍵の生産者を 1 つにする

`mangle_type` を `checker.rs` から `type_key.rs` へ移し、`type_key_of_type(ty: &Type)`
にする。`checker.rs` の `mangle_type` はそれを呼ぶ薄い別名にして、関数の特殊化名
（`mangled_method_name`）と `:dyn` の `concrete_key` が**同じ綴りを共有していること**を
仕組みで保つ。この段階では鍵の中身は変わらない（誰も `Type` を渡していない）。

**完了判定**: 全テスト green、成果物は不変（md5 同一）。

### Stage 2 — `construct` ノードに鍵を載せる

`(construct PATH VARIANT MUTABLE (REPR...) ARGS...)` に鍵を挿し
`(construct PATH KEY VARIANT MUTABLE (REPR...) ARGS...)` にする。`PATH` は残す——
`is_struct` の判定と LSP の goto-definition が使っている。

読み手: `checker.rs::construct_form` / `core_eval.rs::construct_core`（添字がずれる）/
`core_freevars.rs` の `from(4)` → `from(5)` / `core_bridge.rs::translate_construct` /
`locate.rs`（フィールド 0 のみ＝無変更）。

`Option`/`Result`/ユーザ ADT はこれで実体化を持つ。**ここで `print-object` の
ジェネリック穴が塞がる**（プリンタ側は Stage 4）。

### Stage 3 — 組み込みビルダに実体化を渡す

1. 署名で固定の 20 箇所: その場で綴る（`option<string>` 等）。
2. `Vector::new` / `Vector::filled` / `HashTable::new` / `cons-cell`:
   - 解釈実行: `assoc` の lowering が結果の鍵を運ぶ。
   - compiled: `vector-op` / `hashtable-op` の島ノードに型名を足し、`rt_vector_new` 等が
     受け取る。**島の SOURCE 変更＝再生成 2 回（自己ホストの不動点）**。

### Stage 4 — 読み手

- `heap_type_path` は**基底**（`<` の手前）を返す。型引数が要る呼び出しには
  `heap_type_args` を足す。
- `print-object` / `~/name/` のディスパッチ: 基底で型を引き、
  `mangled_method_name` の綴りで特殊化を引く。
- 印字: `#<gen<i32> 1>`。
- `enum_variant_name`: 基底で引く。
- downcast (`pat-typetest` / downcast `pat-ctor`): 実体化した鍵で比較する
  ＝ §0 の不健全が閉じる。ノードに鍵を足す（Stage 2 と同じ形）。

### Stage 5 — 版と成果物

ダンプ／FASL に鍵が載るので版を上げる。島と prelude を再生成。
`tests/type_identity_guard_test.rs` の番人を「実体化まで含む」に更新。

### Stage 6 — 締め

`scripts/test-serial.sh`（直列の全実行）。docs（`functions.md` の
`print-object` の「ジェネリック型には効かない（既知の穴）」を消す、
`syntax.md` の downcast の項に「実体化まで見る」を書く）。

## 4. リスク

- **`TypeKeyId::VECTOR` 等のコンパイル時定数**（27 箇所）が「素の鍵」を前提にしている。
  Stage 3 で全部潰すまで、混在した状態を作らないこと——混在は
  「値が作られた経路によって同じ型が別 identity になる」という最悪の形になる。
  Stage 2 と Stage 3 は**同じコミットで入れる**。
- 島の再生成は 2 回要る（Stage 3 で SOURCE を変えるため）。
- `Option<Sexpr>` は niche で箱を作らないので、この計画の外（鍵を持たない）。
