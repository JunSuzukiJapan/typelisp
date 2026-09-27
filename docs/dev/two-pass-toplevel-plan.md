# トップレベル2パス化計画 — 前方参照と compile-function 分割

作成: 2026-07-31。`compile-value` の 40 段 if 連鎖をどう平坦化するかの調査中に
判明した3つの問題をまとめた計画。当初は3つとも同根(check の2パス化で一括解決)と
見たが、調査の結果 (2) は別解(島自前のマクロ `icond`)で独立に解けることが判明し、
**実施済み**。残るのは (1)(3) で、こちらは依然 2パス化が必要。

状態: **全て完了**(2026-07-31 〜 08-01)
- 問題 (2) 島の分岐の可読性 → 完了(icond、Stage 3.4)
- 問題 (1) トップレベル前方参照 → 完了(Phase 1、`Checker::predeclare_program`)
- 問題 (3) compile-function の分割 → 完了(Phase 3、61ヘルパをトップレベル defun 化)

> **2026-08-23 追記: Phase 1 の暗黙の先読みは撤去された。**
> `predeclare_program` は「最初のフォームを検査する前に**全フォームを読む**」ことを要求し、
> それはリーダマクロ(読み込み中にユーザコードを走らせる、cl-parity-plan.md Phase 8c)と
> 正面から衝突する。前方参照は明示的な宣言 **`defsignature`** に置き換えた
> (旧 `docs/syntax.md`（現 [syntax.md](../ja/reference/syntax.md)） の該当節)。問題 (3) の成果 — 島がトップレベル defun の集まりであること —
> はそのまま残っており、リングは SOURCE 冒頭の `defsignature` ブロック 65 件で宣言している。
> 以下の Phase 1 の記述は、その時点の設計として残す。

最終的な指標(SOURCE):

| | 着手前 | icond 後 | Phase 3 後 |
|---|---|---|---|
| 最大インデント | 132 | 93 | **72** |
| トップレベル defun | 44 | 44 | **105** |
| 最大の単一関数 | ~2700行 | ~2700行 | **エントリ点のみ** |
| コメント行 | 2088 | 2112 | 2112(全数保存) |

## 背景: 3つの問題は同根

1. **トップレベル `defun` の前方参照/相互再帰が不可**
   `check_defun_fixed` はフォーム単体では「自己再帰のためにシグネチャを本体より先に登録」
   している(checker.rs:1986-2017 → 本体チェックは :2047)が、フォーム*間*では
   ドライバ(`prelude::load`/`project.rs`/`main.rs` 等)が1フォームずつ
   check→exec するため、後方の defun への参照は `no such function` になる。
   実証: `(defun even2? ...)` が後続の `odd2?` を呼ぶと 2:20 でエラー。

2. **compiler.rs の SOURCE 内でマクロ(`cond`/`case` 等)が使えない**

   理由は**2つ**あり、両方を解かないとマクロは使えない。

   ### 2a. 階層: 島は prelude に依存しない(実測で判明、Phase 2 の主要制約)
   SOURCE が使う `equal`/`append`/`not` 等は全て **registry.rs の組み込み**
   (registry.rs:600 等)であって prelude の定義ではない。ゆえに
   `load_compiler` は prelude 無しで単独ロードでき、テストはこれに**明示的に
   依存**している: `run_with_compiler`(prelude 無し)と
   `run_with_compiler_and_prelude` の2ヘルパが doc comment 付きで併存
   (tests/compile_test.rs:37-58)。
   実測: SOURCE の `compile-value` を prelude マクロ `cond` で書き換えると
   `--lib` / compile_file_test(28件全滅) / compile_test / sexpr_user_adt_test で
   40件超が `value is not callable: Bool` で失敗する。
   これは単なるテスト都合ではなく、prelude 自体が「島がコンパイルする
   typelisp コード」である以上、島→prelude 依存は層の逆転になる。
   → 当初はここで層の方針決定が必要と考えたが、`icond` で回避した(Phase 2 参照)。

   ### 2b. ブートストラップ循環(当初の発見)
   `case` の展開器は `sexpr-map` に `lambda` を渡す。interp クロージャ削除
   Stage 8c 以降、クロージャ値は必ず JIT され(interp.rs:892 で decline は
   hard panic)、JIT は島 = `get_fn("compile-function")` の `compiled` スロット
   (interp.rs:2546-2558)を要求する。しかし `load_aot`(compiler.rs:4058-4075)は
   SOURCE を check→exec し終えた**後**に `install_island_bitcode` するので、
   check 中のマクロ展開時点では島が無い
   (`definition-time JIT failed: compiler island not loaded`)。
   実証: SOURCE に `case` を1箇所入れると regen が上記 panic で落ちる。

3. **`compile-function` が2700行の巨大 `labels` 一個**
   59個のヘルパ(`compile-value`/`compile-assoc`/...)が相互再帰するため、
   前方参照不可の言語ではトップレベルに分割できず、`labels` に押し込むしか
   なかった。ヘルパのインデントは23段超。

(1) を解けば (3) が解ける。(2) は当初 (1) の機構に相乗りする計画だったが、
島が自前でマクロを持てば独立に解けたため先行して完了した(Phase 2 参照)。

## 検証済みの事実(実装前提)

- SOURCE は**トップレベル定義のみ**(調査時点で defun 43個。その後 `icond`/
  `icond-build` を追加したので defun 44 + defmacro 1)。defvar/module/式は無い。
  → SOURCE に限り「check 全部 → exec 全部」の分離が安全(マクロ定義の
  逐次 exec を要求するフォームが無い)。
- prelude は `case` を自身の check 対象位置で**使っていない**(3件は全てコメント)。
  → コールドキャッシュで prelude ロードが通るのはこのため。prelude 自身は
  島より先に check されるので「lambda を作るマクロを prelude 内で展開しない」
  制約は本計画後も残る(文書化のみ)。
- 起動順: `load_prelude` → `load_compiler_aot`(= `load_aot`) → ユーザコード
  (main.rs:169-173)。ユーザファイルの check 時点で島は常在
  → Phase 1 単独に島の問題は無い。
- `install_island_bitcode`(interp.rs:1047)は「登録済み FnDef の `compiled`
  スロットにネイティブ本体を後付け」する。FnDef が先に要る
  (`expect("island defun already registered by exec'ing SOURCE")`)。
  bootstrap 用に「古い .bc に無い名前は読み飛ばす」フィルタが既にある
  (`check_hash=false` 経路)。
- `Interp::exec` の Defun アームは FnDef を `compiled: RefCell::new(None)` で
  **毎回作り直す**(interp.rs:1111)→ 先行インストール後に exec すると
  ネイティブ本体が捨てられる(Phase 2 の主要な罠)。
- 島の呼び出し経路はエントリ1点: `run_compile_function` が
  `Path::root("compile-function")` の FnDef.compiled を呼ぶ(interp.rs:2539-2558)。
  他の島 defun 同士の呼び合いはモジュール内で完結。
- `labels` ヘルパ群が外側から捕捉しているのは実質 `m`(llvm-module、
  `get-function m` 66箇所+`add-function m` 1箇所)。`f`/`param-names` の使用は
  `compile-function` 冒頭のみ(要 Stage 3.1 で最終監査)。
- 島全体の AOT 再生成は 1.7s(cargo run 込み 2.7s)。性能は動機ではない。

## Phase 1: Checker 2パス化(言語機能: トップレベル前方参照) — **完了**

### Stage 1.1 — `Checker::predeclare_form`
新メソッド。フォーム先頭が `defun` / `(pub defun)` のとき:
- `parse_defun_name` + `parse_defun_sig`(interp 不要 = マクロ展開されないことを
  シグネチャ側の不変条件として維持)で FnSig を構築し
  `reg.root.module_mut(&self.ns).fns` に登録。
- ジェネリックなら `FnTemplate` 登録+parts の permanent-root
  (checker.rs:1996-2004 と同じ処理)。specialization 要求はテンプレートを
  先に引くので、前方のジェネリック呼び出しはこれだけで成立する。
- fq 名を新設の `predeclared: RefCell<HashSet<String>>` に記録。
  既に集合にあれば真の重複 → 既存の `check_redef` を発火。
- `&optional`/`&key` ヘッダ(`params_declare_opt_key` 分岐)も同様に対応。
- def_locs/docs は登録しない(パス2 = 従来の check_defun に任せる。
  ここは最小登録に徹する)。
- 対象外: defmacro(define-before-use 不変条件を意図的に維持)、
  defstruct/defenum/deftrait/impl(型の前方参照は本計画のスコープ外。
  「defun のシグネチャが後方の型を参照する」ケースは従来どおりエラーのまま
  → 制限として文書化)。

### Stage 1.2 — redef 耐性
`check_defun_fixed`/`check_defun_opt_key` の `check_redef` 呼び出しの直前で
`predeclared.remove(&fq)` が true なら redef チェックをスキップ
(sig の再 insert は同一内容の上書きなので可)。ジェネリックのテンプレート
再登録は「既に存在すれば skip」ガード(parts の permanent-root 二重 push 回避)。
RedefPolicy との相互作用をテストで固定(真の重複は従来どおりポリシー適用)。

### Stage 1.3 — ドライバ更新
共有ヘルパ(例 `Checker::precheck_program(heap, forms)` — フォーム列を先頭走査して
`predeclare_form` を呼ぶだけ)を作り、各ドライバのループ前に挿す。
**check→exec の逐次交互は従来のまま**(prelude はマクロを定義するので
check-all/exec-all 分離は不可。分離は Phase 2 の SOURCE 専用)。
更新対象(check_form 呼び出し箇所の全数、interp.rs:3922 の eval builtin と
main.rs:452 の REPL は単発フォームなので対象外・現状維持を文書化):
- `prelude::load` / `source_load_capturing`(prelude.rs:1398, 1448)
- `project.rs` のファイルローダ2箇所(:416, :817 — recover モードでは
  predeclare のヘッダ解析エラーも recoverable として `errors` に積むだけにする)
- `main.rs` run_file(:185)
- `compile/aot.rs` compile-module(:164)
- `compile/bootstrap.rs`(:66)と `compiler::load_aot` は Phase 2 で別形に

### Stage 1.4 — 拡張: defmethod と module 入れ子
- defmethod ヘッダの先行登録((type, method) キー)。
- `(module m ...)` 本体への再帰(check 側の ns 切替と同じ規則で predeclare も
  潜る)。モジュール越し前方参照はファイル↔モジュール機構(走査先行
  オンデマンドロード)と役割が重なるので、単一ファイル内の入れ子のみ対象。

### Stage 1.5 — テスト
- even2?/odd2? 相互再帰(interp / `(compile ...)` JIT / AOT compile-module の3経路)
- 前方ジェネリック呼び出し(specialization が pass 2 前に要求されるケース)
- &optional/&key 付き前方参照、defmethod 前方参照、lambda 本体からの前方参照
- 真の重複 defun が従来どおりエラー
- マクロの define-before-use が従来どおりエラー
- エッジ(文書化+挙動固定): defmacro 本体が「テキスト上は後方の defun」を
  呼ぶ場合 — sig は見えるので check は通るが、展開時に FnDef 未 exec で
  実行時エラーになる。エラーメッセージが原因を示すことを確認
  (必要なら expand_macro 側でメッセージ改善)。
- prelude fasl 等価性テスト(既存 tests/fasl_test.rs)が「2パス化しても
  結果レジストリが単一パスと同一」を保証することを確認。fasl フォーマット
  変更は無し(predeclared は一時状態でシリアライズ対象外)。

### Stage 1.6 — docs
旧 `docs/syntax.md`（現 [syntax.md](../ja/reference/syntax.md)）(前方参照可に)、language-design.md、既知の制限一覧
(型の前方参照・prelude 内 lambda マクロ制約・マクロ本体の展開時制約)。

## Phase 2 — **解決済み・不要**(2026-07-31)

当初は「SOURCE 内でマクロを使うには島の先行インストールが要る」と考え、
そのために check の2パス化を Phase 1 から流用する計画だった。**その必要は
無かった。**

`cond` が使えなかったのは「マクロだから」ではなく「**prelude の**マクロ
だから」(2a)。島が自分の SOURCE 内でマクロを定義すれば prelude 依存は
発生せず、2b(ブートストラップ循環)も回避できる。実装済みの `icond` が
それで、以下の2つの制約を満たすように書かれている:

1. **クロージャを作らない展開器**。`lambda` を含む展開器は JIT を要求し、
   JIT は島を要求する(= 2b)。`cond` 系の変換は `sexpr-cons`/`list` で
   足りるのでこれは満たせる。
2. **`,@`(unquote-splicing)を使わない**。`,@` は `sexpr-append` を呼ぶが、
   これは prelude の defun(prelude.rs:172)であって組み込みではない。
   使うと 2a に逆戻りする(エラーが明示的に教えてくれる:
   "unquote-splicing (,@) requires the prelude's `sexpr-append` to be loaded")。
   代わりに `sexpr-cons`+`list`(いずれも registry.rs の組み込み)で木を組む。

さらに実測で判明した第3の制約:

3. **展開器を再帰で書いてはいけない**。展開器はインタプリタで動き、debug
   ビルドの `Interp::eval` フレームは太い。`compile-value` の 37 アームで
   1アーム1再帰にすると**スタック要求が 2MB → 8MB に4倍化**し、素の
   `cargo test` が落ちた(`scripts/test-serial.sh` の `RUST_MIN_STACK=32MB`
   では隠れてしまう)。`icond-build` を `loop`/`setf` の反復に書き換えて
   基準線の 2MB に戻した。

結果として Stage 2.0 の案 A(層の逆転)も案 B(prelude より下の層を新設)も
不要になり、**案 C のまま可読性の問題も解けた**。Phase 2 の旧 Stage 2.1〜2.4
(`Interp::predeclare_fn` / `load_aot` 再構成 / bootstrap 同型化)は破棄。

## Phase 3: `compile-function` の分割 — **完了**

### Stage 3.1 — 捕捉監査
59ヘルパが `m` 以外に外側の束縛(`f`/`param-names`/`env`/`fn-env` 初期値)を
参照していないことを機械的に確認(現時点の目視では `m` のみ)。

### Stage 3.2 — トップレベル defun 化
- 各ヘルパを `(defun compile-xxx ((m llvm-module) ...既存パラメータ) ...)` に
  昇格。呼び出し側は `m` を追加で渡す(相互再帰は Phase 1 で合法)。
- `compile-function` 本体はエントリブロック構築+`compile-value` 呼び出しの
  薄い関数になる。
- 変換は機械的だが巨大(59関数+数百呼び出し)。1コミットで一括変換し、
  regen + 全テストで検証(段階分割は「m を捕捉する残党」と「引数で受ける
  新参」が混在して却って危険)。インデントは23段→2段に落ちる。
- 島再生成の安全性: 新トップレベル名は old .bc に無い → bootstrap の
  名前フィルタで素通り、新 `compile-function` 系は old の native
  `compile-value` 一式で普通にコンパイルされる(名前 `compile-function` は
  不変なのでエントリ解決も不変)。

### Stage 3.3 — regen + 全テスト + `island_artifacts_are_fresh`

### Stage 3.4 — **実施済み**(2026-07-31、Phase 1/3 に先行)
分岐の `icond` 化。Phase 2 が不要と判明した時点で Phase 1 を待たずに実施:
`compile-value` のタグ分岐(37アーム)、`compile-assoc` の外側の型分岐
(7アーム)と内側のメソッド分岐(12チェーン)、`*-native-method?` 述語5件。
展開は `if` 連鎖で同一なので IR 不変。指標: SOURCE の最大インデント
132 → 93、120桁超の行 342 → 290。コメントは全数保存(変換スクリプトに
コメント行数が変わったら中断するガードを入れた)。

### Stage 3.5 — 文書更新
compiler.rs モジュール doc(labels 前提の記述全面)、freevars.rs の
「compile-value chain」言及、docs/dev/implementation-log.md、メモリ。

## 実施順序と依存

```
P3.4 ── 実施済み(2026-07-31、他に依存しない)

P1.1 → P1.2 → P1.3 → P1.5(基本) → P1.4 → P1.5(拡張) → P1.6
                                    └→ P3.1 → P3.2 → P3.3 → P3.5

Phase 2 は不要(上記参照)。
```
P1 は単独で言語価値がある(先行リリース可)。P2 は P1 の predeclare を
前提。P3.2 は P1 のみに依存、P3.4 は P2 にも依存。

## リスク一覧

| # | リスク | 対処 |
|---|---|---|
| R1 | defun シグネチャが後方定義の型を参照 → predeclare で解決不能 | スコープ外として現状どおりエラー。文書化(1.6) |
| R2 | exec の FnDef 作り直しが先行インストールを潰す | check-all/exec-all 分離+最終再インストール(2.2)。SOURCE=全defun assert |
| R3 | 二重 LLVM JIT の起動コスト | 計測(2.4)。超過時は island 専用 exec 経路で compiled 引き継ぎ |
| R4 | マクロ本体が後方 defun を展開時に呼ぶ | check は通り実行時エラー。メッセージ改善+文書化(1.5) |
| R5 | recover モード(LSP)で predeclare エラーがロードを落とす | recoverable 扱いで errors に積む(1.3) |
| R6 | prelude fasl 等価性が崩れる | 既存等価性テストで検証。predeclared は非シリアライズ(1.5) |
| R7 | P3 一括変換の regression | regen+全テスト+`island_artifacts_are_fresh` を各段で。捕捉監査(3.1)を先行 |
| R8 | 前世代島に無い機能をマクロ展開が要求(bootstrap) | 連鎖制約として文書化(2.3)。rt_gensym 注記と同類 |
| R9 | 島→prelude の層逆転(2a) | **解消**: `icond`(島自前のマクロ)で prelude 依存を作らずに済んだ。Phase 2 ごと不要 |
| R10 | 島のマクロ展開器がスタックを食う | 展開器は反復で書く(`icond-build`)。再帰版は 37 アームで 2MB→8MB |

## 付録: 本計画策定時に試して取り消した変更

`compile-value` の 40 段 if 連鎖を prelude マクロ `cond` で平坦化する変更を
実際に作り、島 regen まで通した。平坦化自体は正しく動作した(生成 IR も同サイズ)。
しかし 2a の prelude 非依存性を破り 40 件超のテストが落ちたため取り消した。
`case` ではなく `cond` を使ったのは 2b(`case` の展開器は `sexpr-map` に
`lambda` を渡すため展開時に JIT が必要)による。
この経緯自体が「表面的な可読性修正では届かない」ことの証拠であり、
本計画(特に Phase 3)が必要な理由。

## 実施結果と、計画から変えた点

### Phase 1(commit 06a6eb4)
`Checker::predeclare_program` を新設し、6ドライバのチェックループ前に挿した。
計画から変えたのは2点、いずれも実装してみて判明した:

- **defmethod の先行登録は削除**。実装したが**一度も発火しない死んだコード**だった:
  組み込み型のメソッドは `type_def.assoc` ではなく registry の別テーブルにあり、
  ユーザ型は先行走査の時点で未登録(型は先行登録しない方針のため)。型の「殻」を
  先行登録すればよいが、フィールドを持たない殻が本体チェックから見えるので見送り。
- **`&optional`/`&key` も対象外**。その `FnSig` は各デフォルト値の*検査済み*
  `Typed` を含み(引数省略時に呼び出し側へ splice される)、シグネチャだけの
  登録では不完全なものが前方の呼び出し側に見えてしまう。

Stage 1.4(defmethod/module 入れ子)は module 入れ子のみ実施。

### Phase 3
61ヘルパを機械的にトップレベル化。捕捉していたのは `m` と `name` の2つだけで、
`fn-name` として明示引数に(局所 `(let ((name ...)))` によるシャドウを避けるため
`name` という綴りは使えない)。

計画に無かった作業が1つ: **`crate::compile::bootstrap` が全島関数を先に
前方宣言する必要がある**。島の呼び出しグラフが宣言順の DAG でなくなり、
`compile-call` の `(get-function m "tl_<callee>")` が下方の関数で失敗するため。
`Interp::compile_scc` が JIT の循環に対して既に使っている形と同じ。

### 変換スクリプトで踏んだ罠(再実行するなら必読)
- ヘルパの doc コメントは s 式の**外**にある。素直に切り出すと**1261行**消える。
  コメント行数が変わったら中断するガードを入れること。
- 文字列リテラル保護の正規表現は、Rust の raw string 開始 `r#"` の `"` を
  文字列開始と誤認し、SOURCE 全体を隠す。**書き換えが静かに0件になる**。
  必ず SOURCE 領域内だけを対象にし、実際の置換件数を数えて検証すること。
