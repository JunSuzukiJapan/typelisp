# コンパイル出力の一様コルーチン化（設計）

Phase C。プランは `~/.claude/plans/go-gorutine-adaptive-raccoon.md` の Phase C 節。
Phase A（評価器の CPS 化、`cps-evaluator-design.md`）の続きで、**同じことをコンパイラの
出力にもやる**。

## 1. なぜ

Phase A でインタプリタは CPS 化したが、コンパイラの出力は素の直接形式のままだった。
`compile-call` が出すのは LLVM の `call` 命令 1 つで、継続もコルーチンも状態機械も無い。
結果として同じ Lisp コードが経路によって別の機械で走る:

| | インタプリタ | コンパイル済み |
|---|---|---|
| 実行状態 | `Vec<Frame>` + `State`（ヒープ側） | マシンスタックとレジスタ |
| 中断 | 「`step_task` を呼ばない」だけ | **不可能** |
| 再帰の深さ | ヒープが尽きるまで | マシンスタック |
| GC ルート | フレームを歩くだけ | **明示的に push/pop する規律** |

「中断できない」が並行機構の B6 制限そのもので、`wait`/`yield`/`sleep` を含む関数は
compile を断っている。「GC ルートを明示的に持つ」のほうは、目に見えにくいが実は大きい:
島には約 85 のルート操作があり、`compile-value` の 16 引数のうち 8 つが脱出とルートの
ためだけに 56〜64 個の署名を貫いている。

**選択的なコルーチン化（中断しうる関数だけ変換する）は採らない。** 半分がマシンスタックの
ままならルート管理の機構は丸ごと残るので、一番大きい利得が手に入らない。そのうえ ABI が
2 種類になり、**関数値と `:dyn` が色を消す**ので型で防げない実行時エラーが出る。

## 2. フレーム

```rust
BoxedObj::Frame { words: Vec<i64>, mask: Vec<u64>, pc: u32 }
```

`BoxedObj::CompiledClosure { fn_ptr, env: Vec<Value>, sexpr_mask }` と同じ形。
**違いは 1 つだけで、それが他の全部を決める**: `words` は `Vec<i64>` であって
`Vec<Value>` ではない。コンパイル済みコードはローカルを生ポインタ経由の `load`/`store`
で触る——機械が持った時点でローカルとはそういうものだ——ので、格納は機械語でなければ
ならない。クロージャの env が `Vec<Value>` でいられるのは、apply の時に一度だけ変換される
から。

### マスクは 2 つのうちの 1 つ目

コレクタは 2 つの篩を通す。

1. **`mask`** — そのスロットがそもそも**タグ付き語か**。マスクされていないスロットは
   生の `i32`/`f64` のビット列で、下位 3 ビットはタグではない。タグとして読むと
   **算術からヒープ参照を捏造する**
2. **タグ** — そのタグ付き語が何かを指しているか。`Sexpr` 表現のローカルは fixnum や
   即値を正当に持つ（`tagged::references_heap`）

`tests/mem_test.rs` の `an_unmasked_frame_slot_is_never_read_as_a_reference` が
1 つ目を、`a_masked_frame_slot_holding_a_fixnum_traces_nothing` が 2 つ目を押さえている。

### タグの表が `typelisp-mem` へ降りた

`encode`/`decode` と 8 つのタグは `typelisp-abi` にあった。**コレクタが読めない場所**
だった——`typelisp-abi` は `typelisp-mem` に依存しているので、mark フェーズからは呼べない。
フレームの語を辿るには decode が要るので、`typelisp-mem/src/tagged.rs` へ移し、
`typelisp-abi` は再エクスポートにした。呼び手は 1 つも変わらない。

同じ表を mem 側に書き直す案は採らない——8 つのタグについて合意し続けねばならない表が
2 つになる。

### 64 スロット上限は初日から成り立たなかった

当初は `sexpr_mask` と同じ `u64` 1 語にして、「超える関数が出たらマスクを配列にする」と
書いていた。**測ったら島自身が超えていた。**

| | 束縛サイト |
|---|---|
| `compile-assoc` | **69** |
| `compile-vector-op` | 51 |
| `compile-value` | 50 |
| 64 を超える defun | **1 / 148** |

スロットは束縛サイトごとに 1 つ配る。これは今の
「束縛ごとに `alloca-args` を 1 つ」の忠実な翻訳で、`match` の互いに素な腕どうしも
それぞれ自分のスタックスロットを持っている（同じ 1 つのスタックフレームに全部載る）
のと同じ勘定になる。だから 69 は「同時に生きている数」ではなく「配るスロットの数」で、
そのまま上限に当たる。

**スロットをスコープの深さで使い回す案は採らない。** 使い回すと、同じスロットが
ある時は収集対象を、別の時は生の語を持つ。静的なマスクはどちらの向きにも安全でなく
（マスクし忘れれば早すぎる回収、マスクしすぎれば算術からの捏造）、実行時に立てて
**消す**必要が出る。消す側は「スコープを抜けた」を捉えねばならず、それはいま消そうと
している `pop-sexpr-root` の LIFO 規律そのものだ。1 サイト 1 スロットなら、
スロットの種別は活性化の間ずっと変わらない。だから `Heap::set_frame_mask_bit` に
対応する「下ろす」操作は**わざと無い**。

### マスクは前もって渡さない

`rt_frame_new` は**スロット数だけ**を取る。マスクは各束縛の場所で
`rt_frame_mask_bit` が 1 ビットずつ立てる。

これは `Vec` にした都合ではなく、**フレームを確保するのに束縛の種別を知らなくてよく
なる**ためのもの。島は本体を出す前にフレームを確保するので、マスクを引数にすると
「本体を歩いて種別を数える前段」がもう 1 つ要る。数えるのはサイト数だけでよくなった。

呼び出しの回数も増えない——ビットを立てる呼び出しは、その束縛の
`rt_push_sexpr_root` が立っていた場所にそのまま立つ。

### ポインタの安定性

`frame_data_ptr` が配る番地は、**フレームが生きている限り**有効。箱の表が伸びると
`BoxedObj` は動くが `Vec` のバッファは動かず、スイープはスロットに `None` を書くだけで
詰め直さない。だから無効化する方法は「フレームを回収させる」しかない——それは呼び手が
到達可能に保つ責任（C2 以降はタスクのスタックが持つ）。

## 3. ローカルがフレームに載る仕組み

**島は既にローカルを SSA レジスタでなくスロットとして扱っている。** `setf` とループ
再入のために、`env` は名前を 1 要素の `alloca-args` スロットに写している
（`compiler.rs` のモジュールコメントが理由を書いている）。読み書きは
`load-raw`/`store-arg` という**ポインタ + 添字**の組。

だから必要な新しい原始関数は 1 つだけだった:

```
(build-slot-ptr builder base-ptr index)   ; load-raw の GEP から load を抜いたもの
```

`resolve-value` / `compile-set` / `retain-bindings` は既にポインタで喋っているので、
**スロットをマシンスタックの alloca から切り出すかフレームから切り出すかの違いだけ**に
なる。これが C1 の変更を小さくしている。

## 4. ドライバの規約（C2 以降）

関数の形は `i64 f(i64 frame)`、返すのは状態語:

| | 意味 | 付随する場所 |
|---|---|---|
| `Return` | 値を返して終わり | フレームの結果スロット |
| `Call` | 呼び先のフレームを積んだ | 新しいフレーム |
| `Suspend` | 中断する | `Waiting` |
| `Unwind` | 脱出中 | 運ぶ値 |

**ドライバは `Interp::step_task` そのもの。** `CpsStack` がインタプリタの `Frame` と
コンパイル済みフレームの両方を持ち、`step_task` がどちらかで分岐する。インタプリタと
コンパイラが 1 つの実行モデルに統合される。

`frame->pc` に応じて再開点へ分岐する前段が要る。`build-switch` は無いので
`build-icmp-eq` + `build-cond-br` の連鎖から始める。

## 5. マルチコアとの関係

共有ヒープでマルチコアにするには「GC のために全スレッドを安全な点で止める」が要る。
**一様変換なら driver ループがそのまま safepoint** で、ポーリング挿入も
`gc.statepoint` も要らない。ファイバ（タスクごとにマシンスタック）だと compiled コードに
safepoint がどこにも無く、結局 codegen にポーリングを差し込むことになる——この Phase の
作業の一部を、利得なしでやることになる。

加えて、一様変換ではタスクが**ヒープ上のデータ**になるので OS スレッド間を移動できる
（M:N の前提）。

**ただし本当の壁は別にある**: `Heap` は `!Send` で `ACTIVE_HEAP` は 1 つの thread_local
でなければならない（`typelisp-abi` が理由を書いている）。ヒープを共有可能にする作業は
Phase C より先に来るし、たぶん一番大きい。Phase C はその**障害物を減らす**だけ。

2026-09-17（AOT スケジューラ）で減った障害物: スケジューラは `typelisp-rt::sched` の
`Scheduler<B: TaskBody>` になり、タスク本体は「インタプリタの継続スタック」（front の
`Task`、`Cx = Interp`）と「compiled 鎖だけ」（`CompiledTask`、`Cx = ()`）の 2 種類。
M:N でスレッド間を動かすのは後者で、前者は main スレッドに pin する、という線引きが
型で言えるようになった。`CompiledTask` が `!Send` なのは `Value`（ヒープへの生ポインタ）
だけが理由で、`assert_send::<CompiledTask>()` はちょうど `*mut Cell` で落ちる——壁が
ヒープにあってスケジューラに無いことの実証。スケジューラは Heap の持ち主が所有する値
（`Interp` のフィールド／`rt_run_entry_driven` のフレーム）で、global にも thread_local
にも置かない。`go` が hook（`SPAWN_TASK`）でなく suspend 種別になったので thread_local
は 1 本減った。OS を待つ場所は `wake_io` の 1 つのまま。

残る穴が 1 つ: **呼び出しの無いタイトループには driver 往復が来ない**ので safepoint が
無い。`compile-loop` の後退辺に 1 箇所ポーリングを入れれば埋まる（C7、Phase C の完了
条件ではない）。

**2026-09-24 に壁は外れた**（[os-threads-design.md](os-threads-design.md)、経緯は
implementation-log.md の同日の節）。上の見立てのうち当たったものと外れたもの:

- **当たった**: codegen には何も足していない。STW GC の safepoint は `cons`・
  `rt_loop_safepoint`（C7 の後退辺。256 回ごとの yield とは別に毎回 GC 要求を見る）・
  drive の step 境界・`leave_native` の 4 つで、compiled コードが自分から止まる点は
  C7 のポーリングだけで足りた。タスクがヒープ上のデータなので、`CompiledTask` は
  `Value: Send` だけで OS スレッド間を移動できた（`assert_send` の番人は
  `crates/typelisp-rt/tests/sched_send_test.rs`）。
- **「driver ループがそのまま safepoint」は半分だけ**: driver 往復は GC 点にしてよい
  場所だが、「全確保が GC 点」にはできなかった。`alloc_string`/`alloc_boxed` は一度も
  GC しなかったので呼び手が未ルート値を抱えたまま呼んでいる——safepoint は「そこで
  GC が起きてよいと呼び手が既に約束している点」だけ。
- **壁の大きさ**: 予想どおり並行機構本体より大きかった。`Heap` を「スレッドごとの
  ビュー + `Arc<HeapShared>`」に分け（API は無変更）、thread_local の表を「ランタイム
  実体ごとの `Arc`」にし、飛行中の unwind 状態をタスクへ移した。
- **インタプリタは動かない側に残った**: スレッド間を動くのは compiled 鎖だけ、という
  上の線引きがそのまま `typl` の規則になった。ワーカー上の compiled タスクが
  インタプリタを要すると main へ移送され、以後戻らない（`Progress::NeedsMain`）。

## 6. 唯一残る非コルーチン境界 — C FFI

C は「呼んだら結果が返る」しか知らないので、`defffi` のコールバック thunk
（`src/compile/ffi.rs`）はコルーチンにできない。完了まで回すシムにし、その中で中断したら
エラーにする。**B6 の制限は消えるのではなく「C のコールバックの中では中断できない」まで
縮む。**

---

# C1 — ローカルをフレームへ（実施記録）

## 何が消えたか

| | |
|---|---|
| `retain-bindings` / `release-bindings` | 削除 |
| `unroot-let-sexpr-values` | 削除 |
| `compile-set` の `rt_set_sexpr_root` 更新 | 削除 |
| スロットの 2 語目（ルート添字） | 消滅 |
| `bind-params` / `bind-let-values` の一時 push/pop | 消滅 |

SOURCE は −213 / +84 行、島の成果物は 3489586 → 3303170 バイト。

**いちばん効いたのは `compile-set`。** `setf` は「スロットに書く」だけでは済まず、
ルートスタック上の**複製**を書き換える必要があった（`rt_set_sexpr_root`、スロットの
2 語目がその添字）。フレームスロットは複製ではなく**コレクタが読む当のもの**なので、
書けば新しい値が root される。`typelisp-rt` の
`a_setf_reassigned_sexpr_value_is_corrupted_by_a_gc_triggered_by_other_allocations_without_rt_set_sexpr_root`
が名指ししているバグの種類が、構造ごと無くなった。

## カウンタは島に置けない

スロット添字を配るには可変な整数状態が要るが、島には置けない。実験した:
`Scope<i32>` はインタプリタでは動くが **compile できない**——`llvm_op_key` は
`Repr::Handle` の V しか `native-scope` に回さないので、`scope::get` が
「モジュールに `tl_scope::get` が無い」で落ちる。島は関数型で `compile-value` の戻りは
`llvm-value` 1 つ、タプルは無く、島は `Sexpr` を作らない。

そこで**ビルダを鍵にして Rust 側に置いた**。関数を組み立てる 3 箇所
（`compile-function` / `compile-lambda` / `compile-labels-bodies`）はそれぞれ自前の
`llvm-builder::create` を持つので、ビルダは構築中の関数と 1 対 1。おかげで
**島の 56 個の署名は 1 つも引数が増えていない**——`compile-value` は既に 16 引数で、
この段はその一覧を短くするためにある。

`alloca-args` が既に引いている線と同じ分け方でもある。島が「どの束縛に記憶域が要るか、
いつか」を決め、Rust が「記憶域がどこか」を知る。

## サイズは後から埋める

`frame-begin` は `rt_frame_new(0)` を出すしかない——数は本体が決めるものだから。
`frame-end` が `InstructionValue::set_operand` でその 0 を置き換えるので、出力される数は
**構成上「実際に配られたスロット数」**になる。本体を歩いて数え直す第 2 のパスが無いので、
第 1 のパスとずれようがない。

`frame-begin` / `frame-end` はフレームの寿命を丸ごと持つ組にした（確保・自身の GC ルート・
最後にスロット数）。島の 3 箇所が両端 1 行ずつで済むのはそのため。ルートの push/pop を
島でなくここで出すのも同じ理由——呼び手が片方を忘れられる形にすると、
**ルートスタックの不均衡というこの段が消しに来たバグ**をそのまま作ることになる。

## 3 つ踏んだ

**1. `llvm-*` 組み込みの署名を変えると、それを呼ぶ島が壊れる。** `frame-end` を
1 引数から 2 引数にしたら、前回の再生成で作った島が古い arity で呼び続けて
`store-arg` に `Int(0)` が渡った。島は自分自身をコンパイルするので、**署名を変える前の
成果物に `git checkout` してから**回す。[[typelisp-island-regen-fixpoint]] の
「2 回では足りない」とは別の、もう一段手前の話。

**2. ハンドメイドのモジュールには `rt_*` の宣言が無い。** 旧 `retain-bindings` は
`kind = 2` のときしか呼び出しを出さなかったので露見していなかった。`frame-begin` は
無条件なので、**前段が自分の依存を自分で宣言する**ようにした。`declare_external_function`
は素の `add_function` なので、既にある名前にそのまま呼ぶと `rt_frame_new.1` に化ける
——get-or-create が要る（`llvm_module_add_function` が同じ理由で既にそうなっている）。

**3. コンパイル済み関数は生きたヒープを要求するようになった。** 活性化記録がヒープに
載る以上これは**新しい契約**であって、テストの都合ではない。JIT して呼ぶテスト 11 本に
ヒープ登録を足した。うち 1 本は `drop(h)` の後に呼んでいて、宙に浮いた `ACTIVE_HEAP` で
SIGSEGV——[[typelisp-dangling-active-heap]] の形そのもの。潜在的な UB を 11 箇所塞いだ
ことになる。

## プランとの差

**`loop-root-base` は抜けなかった**（239 箇所そのまま）。無名の一時値——呼び出し引数、
cons の被演算子——はまだルートスタックに載るので、`break`/`return` の巻き戻しが要る。
プランが C1 に置いていた「ルート操作 ~85 → 0」は、**名前付き束縛の分（C1）と一時値の分**
に割れる。残るのは `push-sexpr-root` 22 / `pop-sexpr-root` 32 / `pop-sexpr-roots` 14 で、
すべて一時値かフレーム自身の root。

一時値をフレームに移すのは C2 の前提でもある（中断点を跨いで生きる値はフレームに無いと
いけない）。そのとき `loop-root-base` と `rt_truncate_sexpr_roots` が一緒に抜ける。

---

# C2a — ドライバの機構（実施記録）

島に手を入れる前に、機構だけを作って手組みのモジュールで証明した（C0 と同じ形）。

## 証明した 2 つ

**1. コンパイル済み関数が本体の途中で止まり、ローカルを保ったまま再開する。**
手組みの関数が 2 つに割れて走る。初回は自分のフレームを作り、ローカルに 40 を
書き、再開点を記録して `STATUS_SUSPEND` を返す。ドライバが同じフレームで呼び
直すと `pc` で分岐し、40 を読み戻して 42 を返す。**40 が要点**——中断の前に書いて
後で読むので、間でマシンスタックが完全に壊れて作り直される。フレームに居る以外に
生き残る道が無い。

これは **C2 以前には述べることすらできなかった主張**。旧 ABI では関数が止まる
手段が「返る」しか無く、残りの仕事はマシンスタックの上にあって誰も拾えなかった。
機能が無かったのではなく**形が無かった**。

**2. ドライバが 2 段の呼び出し連鎖を回す。** `outer` がドライバに `inner` の
呼び出しを頼み、返ってきた値を受け取って続きを走る。**どちらの呼び出しも LLVM の
`call` 命令ではない**——`outer` は `STATUS_CALL` を*返して*あとで入り直す。だから
再帰の深さの限界が OS スタックでなくヒープになる（Phase A がインタプリタに
やったのと同じ「スタックがデータになる」）。

## 決めたこと

**ABI は引数 1 つ**（`i64 f(i64 frame)`）でなければならない。**入るのと再開するのが
同じ呼び出しでないと**、クロージャや `:dyn` メソッドのような間接呼び出しが「今
どちらをしているか」を知らねばならなくなる。だから引数・現在フレーム・呼び出し
要求は `typelisp-abi::call_state` に置いた——`ACTIVE_HEAP` と同じ性格のもので、
境界を跨ぐ必要があるのに乗る場所が無いから。それぞれ隣り合う 2 文の間だけ生きて
いて、**その窓では何も確保してはいけない**（生の語が Rust の `Vec` に居て
コレクタから見えない）。

**フレームは呼び先が自分で作る。** スロット数を知っているのは呼び先だけで、相互
再帰では呼び元のほうが先にコンパイルされることもある。だから初回は frame の
代わりに `0` を渡す。

**ドライバは値スロットの中身を見ない。** 生の語をそのまま通す——タグ付きか生かは
**宣言型にしか無い**（[[typelisp-crossing-must-be-type-driven]]）ので、ドライバが
decode すると同じ間違いを 4 度目にやることになる。

**スロット 0 は予約**（`FRAME_VALUE_SLOT`）。関数の結果が出ていき、待っていた
呼び出しの結果が入ってくる——再開する側から見れば同じもの、待っていた答え。

## 状態語

| | |
|---|---|
| `STATUS_RETURN` | 終わり。値はスロット 0 |
| `STATUS_CALL` | 呼びたい。呼び先と引数は `call_state` |
| `STATUS_SUSPEND` | 中断。次に走るのはスケジューラが決める |
| `STATUS_UNWIND` | 脱出中（C4 でペイロードを付ける。今は誰も出さない） |

## 残る宿題

**値スロットのマスク。** 今は非マスク（コレクタが読まない）。ドライバの窓では何も
確保しないので安全だが、**再開した関数がスロット 0 の値を消費する前に確保すると
危ない**。島を変換するとき、再開直後に正しくマスクされたスロットへ移すことを
規則にする。あるいは戻り表現をフレームに持たせる——`compile-function` は今のところ
戻り表現を受け取っていない。

**フレーム自身の GC ルート。** `frame-begin`/`frame-end` が `rt_push_sexpr_root` /
`rt_pop_sexpr_root` の対を出すが、ドライバ経由になると `FrameStack::roots()` が
フレームを持つので不要になる。中断を跨ぐと対が崩れる（押して、戻って、あとで
降ろす）ので、C2 本体で外す。

## C2c. コルーチンの前段（機構だけ、島は触らない）

C0・C2a と同じ形で、島に手を入れる前に機構だけを作り、手組みのモジュールで
証明した。実装は `src/compile/llvm_builtins.rs` の `coroutine-begin` /
`coroutine-call` / `coroutine-end`。

### 名前を変えずに新しく足した理由

`frame-begin` を広げるのでなく別の名前にした。**次の島をコンパイルするのは
今コミットされている島**で、それは自分がビルドされた時のアリティで
`rt_llvm_call` を呼ぶ。既存の署名を変えると、まさに直せない瞬間に
ブートストラップが壊れる（C1b で 1 度踏んでいる）。

### 前段が直線でない理由

```text
entry:      %cell = alloca i64;  br (%param == 0), fresh, resumed
fresh:      %f = rt_frame_new(0);  store %f -> %cell
            rt_push_sexpr_root(%f);  rt_frame_entered(%f);  br prologue
resumed:    store %param -> %cell;  br prologue
prologue:   %frame = load %cell;  %data = rt_frame_data(%frame)
            %pc = rt_frame_pc(%frame);  br dispatch
dispatch:   (空 —— `coroutine-end` が連鎖を書く)
body:       ...
```

フレームとその data ポインタは、**入口の経路が一度も通らない再開ブロック**が
使う。SSA は定義が使用を支配することを要求するので、入口の経路では定義できない。
`prologue` は新規入場と再開の両方が落ちてくる唯一のブロックで、それが
「すべてを支配する」の中身。

フレームが phi でなく **セル（entry ブロックの alloca）** で合流するのは、
島に phi を作る手段が無いからでもあるが、それ以前に必要が無い——1 つのアドレスが
1 回の活性化を通じて同じ場所を指すなら、「作ったフレーム」と「渡されたフレーム」は
そこへ store するだけで合流する。

### 中断点を跨ぐ SSA 値は作れない

これがこの ABI の中心的な制約で、**LLVM の検証器がそのまま言葉にする**——
最初に書いたモジュールは `Instruction does not dominate all uses` で弾かれた。
呼び出し地点のブロックは再開ブロックを支配しない（再開ブロックへは dispatch
からしか来ない）ので、**呼び出しを跨いで生きる値はフレームスロットに無ければ
ならない**。

その帰結が `frame-slot` にも及んだ。スロットの**アドレス**（data からの GEP）は
島が要求した場所でなく `prologue` に置く。マスクビットのほうは `coro.fresh` に
置く——スロットを marked にするのはフレームについての事実で、フレームは 1 度しか
作られない。**書く前に marked にしても安全**なのは、触っていないスロットが生の
`0` すなわち `TAG_FIXNUM` で、コレクタが何も辿らないから。

### 引数はどこで守られているか

`rt_frame_call` は引数を `call_state` の Rust の `Vec` に写す。コレクタから
見えないその窓の中で、呼び先の `rt_frame_new` が確保する。それでも生き残るのは、
**呼び元が既に収集対象の引数を自分のフレームの marked スロットに持っている**
から（`root-temporary`）——旧 ABI で呼び先の確保を生き延びていたのと同じ理由で、
新しい穴ではない。

### 証明したこと

`tests/compile_test.rs` の
`a_compiled_recursion_runs_two_hundred_thousand_frames_deep`：
`sum_to(n) = n + sum_to(n-1)` を深さ 200,000 で。再帰の 1 段ごとが
`rt_frame_call` + `ret STATUS_CALL` なので、降りていくのはドライバであって
LLVM の `call` ではない。マシンスタックは `FrameStack::run` の深さのまま。

`n` が二つ目の要点で、入場時に 1 度だけ pending 引数から読み、**再帰呼び出しの
後で**もう一度使う。間でマシンスタックは崩れて作り直されるので、フレームに
居る以外に生き残る道が無い。

### C2d（島の切り替え）に持ち越した宿題

**島の 137 個の `alloca-args` のうち、呼び出しを跨いで生きるものは全部フレーム
スロットにしなければならない。** 検証器が漏れなく見つけるので、作業自体は機械的。
分かっているものだけで:

| 場所 | 何が跨ぐか |
|---|---|
| `compile-call-args` の `args-ptr` | 先に評価した引数。後の引数の評価が中断しうる |
| `compile-if` の `slot` | 分岐前に alloca し、腕の中で store、merge で load |
| `compile-let-values` の `acc` | `Scope<llvm-value>` に **SSA 値**を溜める。後の初期化式が中断しうる |
| `compile-loop` / `compile-block` の slot | 同型 |

`args-ptr` には新しい組み込みが要らない見込み：`frame-slot` は連番のスロットを
配るので、引数の数だけ連続して呼べば先頭ポインタから `store-arg` の GEP が
そのまま効く。

## C2d. 島がコルーチン ABI を出す

### 到達した点

**自己ホストの不動点に達した。** ブートストラップは 3 ラウンドかかり、
`ISLAND_DUMP_BODY_ABI` と `ISLAND_DUMP_EMITS_ABI` を間で進める:

| ラウンド | サイズ | 本体 / 出力 |
|---|---|---|
| 0（元） | 3,200,164 | classic / classic |
| 1 | 3,183,629 | classic / **coroutine** |
| 2 | 3,845,765 | **coroutine** / coroutine |
| 3 | 3,845,765 | 同上（**バイト同一 = 不動点**） |

真ん中の世代が本当に存在することがこの表に出ている。C2b の 2 つの定数は
ラウンド 1 の 1 行のためだけにある。

**反復のたびにラウンド 0 から作り直す必要がある。** SOURCE を直しても、
コードを出している島は 1 世代前なので検証器の文句は変わらない。
`scratchpad/threeRounds.sh` が手順を持っている（島を HEAD へ戻し、定数を
両方 CLASSIC にしてから走らせる）。

### 検証器が作業リストを出した

`Instruction does not dominate all uses` が 30 件。中身は 2 種類で、
**2 番目は検証器が黙る**ぶん危ない:

1. **レジスタに残したまま跨いだ値**。`compile-let-values` が全部の値を
   計算してから `bind-let-values` がスロットへ置いていた——支配関係の違反
   であると同時に**潜在的な GC 穴**でもあった（後の初期化式が確保すると
   先の値が根なしで浮く）。スロットを先に配って計算した端から入れる形に
   したので両方消えた
2. **中断を跨いで埋まる配列**。`alloca` を entry ブロックへ持ち上げたので
   支配関係は満たされるが、**マシンフレームは中断で消えるので中身が失われる**。
   `compile-call-args`・`construct-*`・ratio リテラルの配列をフレームスロットへ

`compile-assoc` は 19 箇所を個別に直す代わりに**被演算子の評価を dispatch の
前に括り出した**。`rest` の長さがどの腕でもメソッドのアリティで、短絡する
メソッドが 1 つも無いので、評価するものも順序も変わらない。

**マスクは一律にできない。** `compile-construct-box` の配列はスロット 1 だけが
生のバリアント番号で、marked にすると算術からヒープ参照を捏造する。1 本の
run でなく 3 回に分けて配る。

### 踏んだ罠

- **`compile-call` は `rt_*` 組み込みも名前で呼ぶ。** 両方をドライバに渡すと
  ドライバが `rt_consp` を「入場」させ（`f(0)`）、引数ポインタが null で落ちる。
  `callee_symbol_name` が既にどちらかを決めていて、`rt_` 接頭辞がその答え
- **Lisp 名の先行宣言が旧 ABI だった**（`declare_external_function`、
  `bootstrap.rs`/`prelude_bootstrap.rs` の宣言ループ）。宣言と定義は名前で
  結ばれるので、これは 2 つ目の宣言でなく**その宣言に本体を付けようとして**
  型が食い違う
- **一括置換が「使用箇所」と「引き継ぎ」を区別しない。** `compile-match-arms`
  の `scrut-v` を全部 `(load-raw ...)` にしたら再帰呼び出しの引数まで置き換わり、
  スロットを渡すべきところに読み出した値が入った

### パターン束縛が `alloca` に載っていた（解決）

**`labels` を含む関数のコンパイルが落ちた。**

```
llvm-builder::coroutine-begin: dangling llvm handle 140442234737264
  (registry holds 18, args so far [17, 0, 140442234737264])
```

`sib-fn`（`(get inner-fn-env nm)` の結果）が**ハンドル番号でなくポインタ**。
ハンドルは全部小さいレジストリ添字なので、値そのものが別のものだった。

診断は**島の IR を読んで**終わった。ダンプはビットコード区画を持つので
（`dump.rs` の `parse`）、切り出して `llvm-dis` にかけると
`tl_compile-labels-bodies` の中身が見える:

```llvm
pat-ok:
  %call_result191 = call i64 @rt_data_field(...)   ; Some の中身
  %ashr = ashr i64 %call_result191, 3
  store i64 %ashr, ptr %call_args192               ; ← alloca [1 x i64]
...
  %frame_call = call i64 @rt_frame_call(... tl_new-env ...)
  ret i64 1                                        ; 中断
coro.resume1:
  %load_raw_val257 = load i64, ptr %call_args192   ; ← 消えたフレームから読む
```

`compile-pattern-test` の `pat-bind`/`pat-guard` が `alloca-args` で束縛して
いた。**「検証器が黙る」種類の 2 つ目**（上の分類の 2）そのもので、しかも
今度は配列でなく名前付きの束縛だった。

症状の形が診断を早めた: `(append-block sib-fn "entry")` は通り、その後の
`(coroutine-begin sib-builder m sib-fn)` で落ちる。**同じ値の最初の使用は
通って後の使用が壊れる**なら、跨いだのは呼び出しである。

**なぜ島の再生成は成功していたのか。** ドライバは戻ってすぐ同じ関数へ
再入するので、マシンスタックのその領域がまだ書き換わっていないことが多い。
運がよければ読めてしまう——だから不動点にも達したし、テストでは落ちた。

#### 直し方: 数字はブリッジが持っている

島で「タグ付きか生か」を分類し直すことはしない。`pattern_bindings` が
**既に束縛ごとの `Repr` を歩いている**（本体に何が見えるかを決めるため）ので、
同じ歩きを 1 歩延ばして `Repr::binding_kind` をノードに載せた:

```
(pat-bind "NAME")       -> (pat-bind "NAME" KIND)
(pat-guard "NAME" TEST) -> (pat-guard "NAME" TEST KIND)
```

島は `(binding-slot builder m KIND)` を呼ぶだけになる——`bind-params` や
`bind-let-values` が自分のノードから kind を取るのと同じ形。セルには
決してならない（`10 +` が来ない）: 捕捉される腕の束縛は、その腕の本体を
包む `let` が別名で束ね直すので、パターン束縛は素のスロットのままでよい。

副産物: `translate_ctor_pattern` の長さ検査を**ループの前**へ、しかも
`sexpr` を含む全 kind へ移した。表現を伴わない部分パターンは、そのまま
zip すると報告されずに**翻訳結果から落ちる**。

#### 同じ形がほかに無いことを機械で確かめた

検証器が黙る以上、目で探しても意味が無い。島の IR を全部走査して、
「**どの store よりも後の活性化で load される alloca**」を探した
（`coro.resume*` ラベルを跨ぐたびに活性化を 1 つ進める）。
`scratchpad/scan_alloca.py`。

島 4,000 関数超のうち該当は **4 つだけ**で、4 つとも同じ `pat-bind` だった
（`compile-labels-bodies` / `resolve-value` / `bind-let-values` /
`compile-apply`）。`if`/`match`/`loop`/`block` の合流スロットは store と load
が同じ活性化にあるので該当しない。

### 入場の公表は 1 スロットでは足りない

`labels` が通ると次が出た:

```
a coroutine function did not publish its frame on entry
```

**呼ばれた関数は公表していた。** ドライバは `f(0)` が**戻ってから**
`take_current_frame` を呼ぶが、その間に呼び先が自分のドライバを始めうる——
`rt_apply_any` と `rt_protected_drive` はマシンスタックの上で入れ子の
`FrameStack` を回す（C4/C5 が畳む境界）。内側のドライバの take が外側の分を
食う。`CURRENT_FRAME` を `Vec` にして LIFO にした: 入れ子の公表と take は
必ず外側の呼び先の活性化の中で釣り合うので、これで各ドライバが自分の入場で
できたフレームを受け取る。

**エラーが濡れ衣を着せていた**のが厄介だった——公表しなかったのは呼び先だと
名指しするが、公表は済んでいて、後で消されていた。

### ABI は関数のもので、プロセスのものではない

`build-make-closure` が `rt_closure_new` と `rt_coroutine_closure_new` を
`EMITTED_BODY_ABI` で選んでいた。**コメント自身が「プロセス全体の設定では
答えられない」と書いてあるのに、プロセス全体の設定を読んでいた。**

正しい出どころは箱に入れる関数そのもので、**その LLVM 型が既に ABI**
（`coroutine_fn_type` は `i64(i64)`、classic は `i64(ptr, i32)`）。
別に記録するものが無いのでずれようがない。手で組んだテストのモジュール
（`add-function` で本体を作る＝classic）が、島の出す ABI に引きずられて
壊れたのがこれを暴いた。

**最終的に 10 箇所**。`build-make-closure`、vtable のスロット、prelude
ダンプの `body_abi`、宣言ループ 4 つ（bootstrap / prelude_bootstrap / aot /
dump）、FFI thunk、そして `build_main_wrapper` の **2 つ**——
`$global_init$N` をドライバに渡すかと、`tl_main` の入口 shim を
`rt_run_entry_driven` にするか。後者 2 つは**全体スイートの `--lib` だけが
見つけた**: `aot::tests` が手組みの classic な `tl_main` を建てるので、
プロセス定数を読むとコルーチンのドライバが classic な本体に当たり、
`a coroutine function did not publish its frame on entry` になる。
個別に再実行した 8 ファイルにはこのユニットテストが入っていなかった。

**正しい用法は 1 つだけ**——「これから自分が出す本体を*宣言する*」。
`grep -rn EMITTED_BODY_ABI src/` して、**既にある関数について訊いている
ものは全部バグ**。

### 検証器は「そのプログラムが通った経路」しか見ない

島が自分をコンパイルして verify が通っても、**島が自分では通らない経路**の
crossing は残る。prelude を作り直そうとして 5 件出た（`bitvector` の 4 関数）。

犯人は `compile-vector-op` の `set`: **添字を先に計算してから値の部分式を
コンパイル**していた。値が呼び出しを含むと添字が中断を跨ぐ。同じ形が
`compile-hashtable-op` の `bucket-*` 4 腕（`h`/`i`/`k`/`v`）と、
`compile-catch`・`compile-unwind-protect` の `root-base`、`compile-catch` の
`tag` にもあった。

**バケットのキーと値は「タグ付けしてから」退避する。** そうすればスロットの
マスクに kind ごとの判断が要らない——`compile-tag-struct-field` の結果は常に
タグ付き `Sexpr` で、スカラのタグは `TAG_FIXNUM`（コレクタは辿らない）。
副産物で GC の穴も 1 つ塞がった: 旧コードは `k` をタグ付けせずに `v` の
評価を跨いでいた。

**この形も機械で探せる**（`scratchpad/scan_ssa.py`）。`let*` の束縛のうち
**LLVM 値を作るもの**が、後続の `compile-value` より後で読まれていたら候補。
落とし穴 2 つ:

- **翻訳時の Lisp 値を除く**。`arg-forms` や `kind` は島自身のフレームに
  載っているので関係ない。除かないと 75 件出て使いものにならない
- **束縛式の「頭」で判定する**。中に `alloca-args` が出てくるだけの
  `(build-call ... (alloca-args ...) ...)` を除外してしまい、`root-base` を
  2 件取り逃していた

修正前の版に当てると 9 件（実際に直した 9 箇所）、修正後は 0 件。

### 「関数を名指ししない検証エラー」を直した

`Module::verify` のメッセージは検証器の文句だけで、**どの関数か**を言わない。
何百もある中で "Instruction does not dominate all uses" が 5 件と言われても
読む先が分からない。1 関数ずつ聞いてから名前を添えるようにした
（`compile::verify_module_naming_functions`）。

`disassemble ... true` も、**検証に落ちても IR を印字する**ようにした。IR を
求めるのは IR がおかしいからで、そこで見せないのは逆立ちしている。

### prelude が自分の ABI を名乗っていなかった

`(sxhash 1)` が `rt_frame_data: Int(17564761476804) is not a frame` で落ちた。
数を 8 倍すると `0x7FCE...`——**ポインタ**である。インタプリタが
`f(args, argc)` で呼び、引数ポインタがフレームの位置に届いていた。

C2b でダンプに「本体がどの ABI に答えるか」を持たせたが、**入れたのは島の
ダンプだけ**で、prelude は既定の `capture_types`（classic 固定）のままだった。
島が classic なうちは正しかった——prelude の本体を出すのは島だから、両方
classic で一致していた。島が翻った瞬間に、コルーチンの本体に classic の札が
付く。

prelude は自分ではコードを出さないので `emits_abi` は無意味、`body_abi` だけ
が `EMITTED_BODY_ABI`。

**成果物の既定値は、その既定値が正しかった理由が消えても黙っている。**

### vtable のスロットも ABI を運ぶ

`rt_dyn_call` は vtable のアドレスを classic に transmute していた。
`build-make-closure` と同じ話が 2 度目に出た形で、しかも今度は**関数の LLVM
型を見る手が使えない**——実行時の表にあるのはアドレスだけである。

だからスロットを `(アドレス, ABI)` の組にした。書き手は 2 つあって、どちらも
答えを持っている:

- JIT（`Interp::publish_vtable`）は `CompiledBody::body_abi()` を持っている
- AOT（`aot.rs` の `rt_vtable_set`）は**その関数の LLVM 型**を見られる
  ——`build-make-closure` と同じ問い

呼び出し側は `rt_apply_any` と同じ境界に立つ: コルーチンなら自前のドライバで
完走させる。マシンスタックの上で待っている呼び出し元に値を返さなければ
ならないため。C5 がこの 2 つをまとめて畳む。

### 全体スイートが見つけた 6 つ

`compile_test` 262 本が全部通っても、残りの 115 ファイルには別の経路がある。

1. **`pat-ctor` のスクルーチニーがフィールドごとに再利用される。**
   `((new "x" "y") 1)` のように部分パターンが値パターン（`pat-guard`）だと、
   その test は呼び出しなので、次のフィールドを読むときには前の活性化が
   終わっている。`compile-ctor-pattern` で kind 2 に退避——`pat-ctor` は
   `Sexpr`/sum-ADT 箱/箱付き構造体のいずれかを分解するので、3 つとも
   ヒープ値で判断の余地がない
2. **被演算子が脱出すると、続く store がターミネータの後に積まれる。**
   `(block outer (+ (block inner (return-from outer 7)) 10))`。被演算子を
   dispatch の前へ括り出した帰結で、**括り出す前は「両方定数なので命令が
   1 つも出ていなかった」から出なかった**。`compile-call-args` と
   `compile-assoc` に `block-terminated?` の門を置いた
3. **AOT の Lisp 名の先行宣言が旧 ABI だった**（`aot.rs`）。C2d で
   `bootstrap.rs`/`prelude_bootstrap.rs` は直したが、ここが残っていた。
   同じ罠の 3 度目
4. **`protected()` が入場フレームのスタックを巻き戻していなかった。**
   GC ルートは巻き戻していた（`truncate_roots(base)`）が、`CURRENT_FRAME`
   を LIFO にしたので同じ規律が要る。unwind は publish と take の間を通る
   ので、その窓の分は誰も take しない。結果は「値スロットに 0」——
   ドライバが**別の呼び出しのフレーム**を受け取っていた
5. **FFI の thunk は Lisp から呼ばれる入口。** プランの「C FFI の thunk は
   非コルーチンで残る」は C→Lisp のコールバック方向の話で、Lisp→C の入口は
   コルーチン ABI に答えなければならない。中身（マーシャリング）は
   `$ffi` 接尾の名前へ移し、Lisp 名は最小の入場だけを持つ薄い包みにした
   ——pc 分岐も再開ブロックも無い（再開するものが無い）
6. **`ISLAND_DEFUNS` が 8 追加 1 削除ぶん古かった。**

### 合流スロットが根でない（**解決** →「合流スロットは3つ残っていた」）

`catch_throw_test` の 2 本（どちらも gc_stress）:

```
(loop (unwind-protect (return (append "sur" "vives")) (cons 3 (cons 4 ()))))
```

`compile-return` が値をループの結果スロットに置き、**その後 cleanup が走って
確保する**。スロットは `alloca-args` なのでコレクタに見えず、回収される。
`compile-block` と `compile-unwind-protect` の合流スロットも同じ形。
`compile-if` の合流は store と load の間に確保が無いので安全。

**C1 が push/pop を消したときの取りこぼし**で、今日の作業が壊したものでは
ない（テストのコメント自身が「a plain `alloca`, not a GC root」と、捕まえたい
バグを書いている）。全体スイートが C2c 以降走っていなかったので出なかった。

**島だけでは直せない。** マスクを立てるには「その値が回収対象か」が要るが、
`loop`/`block`/`unwind-protect` のノードは kind を運んでいない。一律に
masked にするのは不可——生の i64 の下位 3 bit が箱のタグに見えたら、
コレクタが算術からヒープ参照を捏造する（`compile-construct-box` と同じ話）。
チェッカーが `loop`/`block`/`unwind-protect` に結果の `Repr` を載せる作業が
先に来る（`core_vocabulary_test` と `core_cps` の読み手も動く）。

**やった。** 下の「合流スロットは3つ残っていた」を参照——この段落の見積もり
（動く読み手が 2 つ）は足りず、実際には 6 つ（`core_freevars` の子の列挙、
橋、島、評価器、語彙、手書きコア IR を持つテスト 3 ファイル）だった。

### 「本体を運ぶダンプは自分の ABI を名乗る」は 5 箇所あった

prelude で 1 度直したあと、同じ穴が続けて出た:

| どこ | 症状 |
|---|---|
| `prelude_bootstrap` | `(sxhash 1)` が `rt_frame_data: ... is not a frame` |
| `dump.rs` のセッション画像 | `typl --image` が abort |
| `aot.rs` の Lisp 名の先行宣言 | `coroutine-begin` が `ptr %0` を受ける |
| `dump.rs` の Lisp 名の先行宣言 | 同じ |
| FFI thunk の `CompiledBody::body_abi` | `rt_pending_arg: argument 0 was not passed` |

**どれも「間違った規約」で、エラーではない。** 宣言と定義は名前で結ばれ、
ABI の札は「誰かがその本体を呼ぶとき」にしか読まれないので、書き間違いは
その場では何も言わない。既定値が classic なのが効いていたのは島が classic
だった間だけで、**その正しさの理由が消えても既定値は黙っていた**。

`capture_types` の既定値（classic 固定）が正しいのは**ビットコードを持たない
ユニット**だけ。本体を運ぶなら必ず `capture_types_with_abi`。

### AOT の入口はドライバを通す

`main` が状態語のループを回すことはできない（定数の store が数個の関数
だから）。どちらを呼ぶかは生成時に決まる:

- `$global_init$N` → `rt_drive_entry`（panic はそのまま通す。置き換えた
  classic 呼びもそうだった）
- `tl_main` → `rt_run_entry_driven`（`rt_run_entry` と同じ panic 処理の前に
  `FrameStack` を置いただけ）

**2026-09-17 の続き: `rt_run_entry_driven` は `FrameStack` でなくスケジューラを
置く。** `FrameStack::run` 1 回では最初の中断で abort だった——`sleep` も
`net-wait` も、そして C7 の safepoint も（`run_to_end` の腕は `rt_drive_entry`
にしか無く、10 万回のループが abort した）。スケジューラの核は
`typelisp-rt::sched`（`Scheduler<B: TaskBody>`）へ下ろし、AOT の `main` は
`Scheduler<CompiledTask>` の main タスク。`rt_drive_entry`（`defvar` 初期化子）と
`rt_drive_body`（printer の door）は `run_to_end` のまま。

**2026-09-18 の続き: `$global_init$N` も `main` と同じスケジューラの下へ。**
`rt_drive_entry`/`rt_run_entry_driven[_int]` を消して `rt_run_program[_int]`
1 本に統一——`args = [inits, n, entry]`（初期化子のアドレス配列・個数・
エントリ）を渡し、初期化子 1 つにつき `admit`→`drive`、最後に `main` を
`admit`→`drive`（REPL がフォームごとに drive するのと同じ形。前の drive が
残したタスクは次の drive で走る）。これで `defvar` 初期化子がチャネルを作る・
`go`/`wait` するといった「ふつうのプログラム」になり、`rt_drive_body`
（printer の door）だけが `run_to_end` に残る。`eval` を呼ぶ実行ファイルは
`rt_run_program_interp[_int]`（`typelisp_front::shim`）——front の `Task` で
`main` と初期化子を回すので、`Interp::scheduler` が1つだけになり、eval の中の
`(go ...)` が次の `rt_eval` を待たず動く。生成時（`eval_env.is_some()`）に
どちらの shim を呼ぶか決める——実行時の hook では分岐しない
（[[typelisp-c6-abi-never-implicit]]と同じ規律）。

**`rt_*` を足したら staticlib を作り直す。** AOT テストは `cargo test` が
作らない `typelisp-front` の staticlib にリンクしているので、新しい shim は
`cargo build -p typelisp-front` まで存在しない——症状は
`linker failed with status exit status: 1` だけで、名前を言わない。

### lambda の LLVM 名が prelude と衝突していた

`(apply-fn (adder 5) 10)` が AOT で 15 でなく 10 を返した。捕捉が 0 に
読めている。

`core_bridge` は escaping lambda を**プロセス大域のカウンタ**で
`lambda$N` と名づける。1 プロセスの中では一意だが、**AOT のモジュールは
2 プロセスぶんの出力を持つ**——prelude のビットコードは、それを作った
実行の `lambda$0`..`lambda$20` を凍らせて運んでいる。
`add-coroutine-function` は名前で get-or-create なので、素の `lambda$0` は
**prelude の関数を返し**、クロージャはそれを呼んでいた。

**先在のバグで、C2d が露出させただけ。** このプロセスが自前の lambda を
21 個以上（単型化された prelude のジェネリックから）先に橋渡ししている限り、
カウンタが衝突を通り越していた。C2d でその数か順序が変わって当たった。
——**「穴が無い」と「誰も踏んでいない」は見分けがつかない**（
[[typelisp-remove-i64]] と同じ形）。

直し方は `declare-labels-siblings` と同じ: 囲む関数の名前で mangle する。
LLVM 名は誰も外から参照しないので、ただで済む。

診断には AOT のモジュール全体の IR が要った（`disassemble` は 1 関数、
ダンプは成果物、機械語になってからでは遅い）——`TYPELISP_AOT_IR=<path>` を
`write_executable` に付けた。

### 印字側は driver を持てない

`~/name/` と `print-object` の登録先アドレスを持っているのは
`typelisp-print` で、**この crate は `typelisp-rt` に依存しない**
（`Cargo.toml` が理由を書いている——リンカはアーカイブのメンバ単位で
引くので、印字系を別 crate にしておけば「使わないプログラムは払わない」）。
つまり `FrameStack` に手が届かない。

だから包みを**生成側**に出させた（`aot.rs` の `classic_door`）: 呼び先の
LLVM 型がコルーチンなら、4 命令の classic な関数を作って
`rt_drive_body` に渡す。呼び先の ABI を知っている唯一の場所は、その呼び先を
出したコード生成器である。

型を見て分岐するので、classic のままなら包みは出ない——`build-make-closure`
と同じ判定を同じ理由で使っている。



### 合流スロットは3つ残っていた（loop / block / unwind-protect）

C1 が名前付き束縛と一時値をフレームへ移したとき、**制御形の合流スロットは
`alloca` のまま残った**。`compile-if` のそれは安全（store と load の間に
確保が無い）だが、3 つは違う:

```lisp
(loop (unwind-protect (return (append "sur" "vives")) (cons 3 (cons 4 ()))))
```

`return` は値をループの合流スロットに置き、**それから** cleanup へ飛ぶ。
cleanup は確保する。`gc-stress` の下で、スロットは `alloca` なのでコレクタ
から見えず、返る値が回収される。`catch_throw_test` の 2 本が
（自分のコメントで「a plain `alloca`, not a GC root」と、捕まえたいものを
名指しして）これを見張っていた。

**島だけでは直せなかった。** マスクを立てるには値の kind が要り、
`loop`/`block`/`unwind-protect` のノードは運んでいない。一律に masked に
するのは不可——生の `i32` の下位 3 bit が箱のタグに見えたら、コレクタが
算術からヒープ参照を捏造する。

だからチェッカーに運ばせた。`catch`/`throw` が投げる値の repr を運ぶのと
同じ形で、この 3 つは**自分の合流スロットの** repr を運ぶ:

| ノード | 新しい形 | repr の意味 |
|---|---|---|
| `loop` | `(loop REPR BODY...)` | `break`/`return` が持ち出す型（腕の join） |
| `block` | `(block NAME BODY REPR)` | `return-from` が持ち出す型 |
| `unwind-protect` | `(unwind-protect P C REPR)` | 保護域の型。cleanup の分は要らない（捨てられる） |

橋は `field_kind` でなく **`binding_kind`**（`repr_binding_kind`）を渡す:
島の `binding-slot` が訊いているのは「この語を辿ってよいか」だけで、
`catch` の kind のような「どう包み直すか」ではない。

#### `loop` の repr だけが先頭にある

他はすべて末尾なのに `loop` は先頭。本体が可変長なので、末尾の 1 つは
「もう 1 つの文」と見分けがつかない。

**この非対称が自己ホストの巡回を 1 回で済ませた。** 再生成の 1 巡目は
*古い*島が新 SOURCE をコンパイルする。末尾に足したフィールドは、読まない
古い読み手には見えない（C2d の `pat-bind` の kind がそうだった）——が、
`loop` は全フィールドを本体として消費するので、**どの位置でも古い島には
壊れて見える**。

助かったのは別の理由だった: **島の SOURCE には `loop`/`block`/
`unwind-protect`/`break`/`return` のフォームが 1 つも無い**（`(loop ...)`
に見える 5 箇所は `compile-value` の `case` のキー）。島は再帰と `if` だけで
書かれている。だから古い島はこれらのノードを一度も見ない。

確かめずに「先頭に足す」と決めていたら、C2d と同じ定数 2 つ＋3 巡の
段取りが必要だと思い込んでいた。**成果物の入力が何かは、数えれば分かる。**

#### 直った跡

`catch_throw_test` 36/36（gc_stress の 2 本を含む）。島の再生成は
2 巡で不動点、prelude も再生成（prelude は `unwind-protect` を実際に使う
——`with-open-file`）。

`compile-catch` の合流スロットは `alloca` のまま**正しい**: 通常路も pad 路も
「store → br merge → load」で、間に確保も呼び出しも無い。`compile-if` と
`build-try-option` と bignum/vector の `try` 系も同じ理由で安全。
機械的に数えた: store と load の両方に現れる `alloca-args builder 1` は
8 つあり、3 つが上の表（直したもの）、残り 5 つが `compile-if`・
`compile-catch`・`build-try-option`・bignum の `try-int`・vector の `pop`
——どれも「store → br merge → load」で、間に何も起きない。

#### 先頭の repr は省略できない（9 時間の教訓）

`core_cps.rs` / `core_eval.rs` のテストは**手書きのコア IR** を書く。そこでは
**末尾の repr は省いてよい**——評価器はフィールドを添字で読むので、
`catch` のテストは実際に `(catch (quote done) (int-any-width 3))` と 2
フィールドで書いている。`block` と `unwind-protect` も同じ理由で無変更。

**`loop` だけは違う。** 先頭を省くと、最初の*文*が repr の位置に繰り上がって
**黙って落ちる**:

```
(loop (if .. (break) ..) (set ..))   ; 抜ける if が消えて、出口の無いループに
```

症状は「100% CPU で 9 時間、何も印字されない」だった。悪化させた要因が 2 つ:

- `scripts/test-serial.sh` に**ターゲットごとのタイムアウトが無い**
- 出力を `grep` に通していたので**ブロック単位でバッファされ**、
  どのファイルまで進んだかも見えなかった（`--line-buffered` か
  `stdbuf -oL` が要る）

だから `Op::Loop` は**先頭フィールドが repr として読めることを検査する**。
この評価器が無視している他の repr フィールドと違う扱いにするのは、
**先頭にあるものだけが「無いこと」を別の意味に化けさせる**から。
ループの入場ごとに 1 回（`Frame::Loop` は本体から再開するのでノードを
読み直さない）。


## C3. 中断

コンパイル済みのフレームが `STATUS_SUSPEND` を言えるようになった。
`concurrency_test` の

```lisp
(defun tick ((name string) (n i32)) ()
  (dotimes (i n) (setf trail (append trail name)) (yield)))
(compile tick)
(let ((a (go (tick "a" 3))) (b (go (tick "b" 3)))) (progn (wait a) (wait b)))
trail                                  ; => "ababab"
```

が緑になった。`ababab` は **compiled な本体が途中で止まって 3 回ずつ再入する**
以外の出方がない——何もタスクをプリエンプトしないので、止まらなければ
`aaabbb`（`without_yield_each_task_runs_to_its_end` がそれを固定している）。

### 呼び出しが既にこの形だったので、中断は 40 行だった

`coroutine-suspend` は `coroutine-call` から**呼び先を抜いただけ**:

| | `coroutine-call` | `coroutine-suspend` |
|---|---|---|
| 前に | `rt_frame_call(callee, args...)` | 中断する組み込みの `rt_suspend_*` |
| pc | 立てる | 立てる |
| `ret` | `STATUS_CALL` | `STATUS_SUSPEND` |
| 再開ブロック | 値スロットを読む | 値スロットを読む |

呼び出しが既に「活性を終えて pc で戻ってくる」形だったので、中断は同じ形の
status 語が違うだけになる。C2 の投資がここで返った。

島側の判定は **`rt_suspend_` という、より長い接頭辞**。`rt_` が「これは Rust
だから*呼ぶ*」の構造的な合図であるのと同じで、`externs.rs` の表だけが
「どの組み込みが中断するか」を決める——名前の第 2 のリストを作らない。

「何を待つか」は `typelisp_abi::call_state` の**生の 2 語**（kind と payload）
で渡す。`Waiting` は `TaskId` と `Instant` を名指すスケジューラの型で、
公表する側の crate はその下にいる——`PENDING_CALL` がアドレスと
`CoroutineFn` を分けているのと同じ分業。

### タスクがフレーム鎖を持つ

`Task::compiled` は `FrameStack` で、**タスクは一度に 1 本しか駆動しない**:
鎖から出る道は「戻る」と「中断する」だけで、鎖が立っている間タスクは
それを駆動しているか、それに対してブロックしているかのどちらか——`step_cps`
は走れないので 2 本目を始められない。compiled が*インタプリタへ*戻る呼び出し
（`rt_apply_any`）はマシンスタック上の入れ子ドライバで走るので、**その境界だけ
は今も中断できない**。

**スケジューラは 1 行も変えていない。** 起床は `State::Apply(v)` を置くだけ
なので、それを受け取る CPS フレーム（`Frame::DriveCompiled`）を 1 つ足して
compiled 側へ橋渡しした。`Blocked` を太らせるより、受け手を 1 つ足すほうが
触る面が小さい。

### 詰まった 1 点: crossing root が truncate に消されていた

`enter_fn` で marshal した root を、直後の `heap.truncate_roots(fbase)` が
消していた。同期版の `call_compiled` はこれに当たらない——push と pop を
**1 回の Rust 呼び出しの中で完結**させ、truncate はその後だったから。

だから引数は marshal を跨いで生き残らなければならない。`State::CompiledEnter`
は引数を**ヒープリスト**で運び（状態スロットが根にする。`State::Enter` が
`go` の引数にしているのと同じ理由——`Vec<Value>` はコレクタから見えない）、
encode は切り替えの**後**で行う。`DriveStart` と `DriveCtx` が分かれているのは
この一点のため。

推測でなく計測で当てた: `set_state` ごとの root 数と rt 側の push/pop を出し、
`xroots=1 on_entry=4` なのに `count=2` という 1 行で確定した。

### 予告していたコメントが 2 つ外れた

`coroutine-begin` のコメントは「C3 でこの push/pop の対は成り立たなくなり、
frame の root は `FrameStack::roots()` へ移る」と書いていた。**移らなかった。
理由はルートスタックがタスクごとだから**——タスクが待っている間、その stack
には誰も push しない。鎖に入る他の道（別のタスク、compiled からの `go`、
`rt_apply_any` のコールバック）はどれも自分の stack を持つか、1 活性の中で
均衡する。だから LIFO は今も成り立ち、`FrameStack::roots()` は**答えの出て
いる問いへの 2 つ目の答え**になった（C5 の、後ろにタスクのいないドライバの
ためには要る）。

予告を残すのは実装より長生きするコメントの典型なので、両方「なぜ成り立った
か」に書き換えた。

### C3c: `sleep` と `task::wait`、そして名前の接頭辞では足りなかった

`yield` だけなら島が**名前の接頭辞**（`rt_suspend_`）を読めば済んだ。
`wait` で足りなくなった——**答えが型付き**（`Task<T>` → `T`）なのに、
`call` ノードには引数の repr を置く場所しかなく、結果の repr は無い。

だから中断を 1 つのノードにした:

```
(suspend NAME KIND (kind . arg)...)
```

`NAME` は先に呼ぶ shim（何を待つかを `call_state` に書く）、`KIND` は
再開時に値スロットから答えをどう読むか。`0` は「読むものが無い」
（`yield`/`sleep` は unit）——`compile-catch` が、何も投げないタグに使っている
のと同じ約束。`wait` の答えは **`T` が何であっても tagged `Sexpr`** で渡り、
再開ブロックが `compile-sexpr-field` で戻す。投げられた値とまったく同じ分業で、
理由も同じ: 境界は 1 語しか渡さず、それが何かは表現にしか書いていない。

`externs.rs` が「どの組み込みが中断するか」を決める側であり続ける。自由関数は
`rt_suspend_` 接頭辞（`rt_builtin_symbol`）、メソッドは
`rt_suspend_method_symbol`（1 行）。島は自分のリストを持たない。

**`rt_sleep` は残さずに消した。** compiled にはずっとスレッドを止める
`rt_sleep` があり、それがまさに `sleep` をコンパイルできなかった理由だった
——残しておけば、同じソースが interp では 1 つのタスクを、compiled では
プログラム全体を、黙って止め続ける。

**拒否は 2 段あった。** `SUSPENDING_CALLS` を消しても `wait` は
`Uncompilable { target: "task::wait" }` で落ちた——グラフ構築側の「実装の無い
メソッド target」チェック。中断するメソッドは*呼び出しではない*ので、native
lowering される primitive メソッドと同じく target から外す。

### C3 で消えなかったもの

プランは `run_to_completion` の「compiled から来たので中断できない」も消すと
書いていたが、**消せない**。これは compiled がインタプリタへ戻った先
（`rt_apply_any`）で、その下にはマシンスタックのフレームが積まれている。
中断すればそれを置き去りにする。プランが C5（境界の整理）に割り当てている
`rt_apply_any` / `rt_dyn_call` の作業がここに来る。

## C4. 巻き戻しをドライバの状態に

`throw`/`panic` が Rust の panic であることは変えていない。**変えたのは誰が
それを受け止めるか**——呼び出し地点の 9 つのトランポリンではなく、
**ドライバがアクティベーションの境界で 1 箇所**。

C2 が既に条件を作っていた: Lisp の呼び出しはドライバ往復であってマシンの
`call` ではない。だから compiled フレームで上がった panic が travel できる
マシンフレームは**そのフレーム 1 活性化分だけ**で、出口はドライバの
`f(frame)` しかない。受け止める場所が 1 つに決まる。

### 領域はフレームのスロットになった

`FRAME_HANDLER_SLOT`（スロット 1）。値は「いま入っている
`catch`/`unwind-protect` 領域の dispatch ブロックの `pc`」で、`0` は「この
フレームは何も受けない」。`rt_frame_new` がゼロ埋めするので、領域を持たない
関数はこのスロットに一度も触らない。

**呼び出しは何も出さない。** `check-unwind` が消え、`rt_protected_*` が消え、
`emit-direct-call` / `emit-lisp-call` / `emit-lisp-call-with-env` /
`emit-closure-apply` / `emit-rt-call` の 5 つは「守られた枝」が無くなった時点で
`build-call` / `coroutine-call` の別名になったので、呼び出し地点へ展開して
消した（島の SOURCE が 110 行短くなった）。領域の中の呼び出しは、領域の外の
呼び出しと 1 命令も違わない。

静的な表（呼び出しの `pc` → ハンドラの `pc`）にしなかったのは、答えが
**呼び出しについての事実ではない**から。それは「その呼び出しがどの領域に
書かれているか」という事実で、島は自分が emit している地点でそれを常に
知っている。スロットなら、島は知っていることを書くだけでよい。

ハンドラに再開するのは、呼び出しの継続に再開するのと**同じ機構**——同じ
`pc`、同じ dispatch チェーン。`frame-set-handler` が pad に resume id を
配り、`coroutine-end` が普通のアームとして並べる。

### 書く場所は 4 種類、規則は 1 つ

**いま emit している地点の領域を書く。**

| 場所 | 書く値 |
|---|---|
領域に入るところ | その pad |
pad の先頭 | **外側**の領域 |
領域を普通に出るところ | 外側の領域 |
静的な脱出が着地するブロックの先頭 | そのブロックが書かれている領域 |

pad が外側を書くのは、cleanup が自分の `unwind-protect` に守られない（CLHS）
から——そして tag が合わなかった `catch` も、その時点で自分の領域からは
出ている。だから `emit-unwind-onward` は何も復元しない。

**4 番目だけが、コンパイル時だけの `protect` には無かった義務。**
`(loop (catch 'a (break)))` の `break` はチェッカーが決めた `br` で、
放っておけばスロットは「制御がもう出た pad」を指したままになる。後から
throw が来ると、終わったはずの `catch` に着地して merge スロットに書き、
その後ろをもう一度走る。だから `compile-loop` / `compile-block` の exit
ブロックと、`emit-block-cleanups` の各コピーの先頭で書き直す。

### 合流スロットが 4 つ目あった

`compile-catch` の結果スロットは `alloca` のままだった。今まで無事だったのは
**pad が同じ活性化からの `br` で到達されていた**から。C4 で pad は dispatch の
宛先になる——prologue から、別の活性化で入る——ので `alloca` は pad を支配
しないし、そのマシン記憶はもう存在しない。

`(catch TAG KIND BODY BKIND)` の `BKIND` がそれ。**1 つの repr に 2 つの質問を
しているだけ**で、チェッカーのノード（`(catch TAG BODY REPR)`）は変えていない
——`KIND` は「投げられた語をどう untag するか」、`BKIND` は「そのスロットを
コレクタが辿ってよいか」。`loop` / `block` / `unwind-protect` の 3 つに続く
4 つ目で、見つけ方も同じだった（検証器が支配関係で言葉にする）。

### 関数ポインタ型が嘘をついていた

`CoroutineFn` は C2c から `extern "C"` だった。`rt_panic` は unwind するので、
compiled な `(panic ...)` は**ずっとこのポインタを通って unwind していた**。
`"C"` は「呼び先は unwind しない」という呼び出し側への約束で、LLVM は
呼び出し地点を `nounwind` と扱ってよい。通っていたのは運で、そこに
`catch_unwind` を置くなら運では済まない。`"C-unwind"` に直した——リポジトリの
他の compiled 本体ポインタ型（`CompiledSignature` / `ApplyInterpretedFn` /
`DynSlotClosureFn`）は最初から全部これで、**コルーチン ABI だけが外れていた**。

### ルートの修復が呼び出しごとからフレームごとになった

`protected` は呼び出しの前の深さを覚えて、caught のときにそこへ戻していた。
いまは `FrameStack` の各エントリが**入場時のルート深さ**を持ち、unwind で
フレームを pop するたびにそこへ切る。pad 自身の
`rt_truncate_sexpr_roots(root-base)` は残る——そちらは「領域の入口の深さ」で、
フレームの入場より深い。2 つは入れ子で、どちらも縮める方向にしか動かない。

### 消えたもの

ランタイム側 12: `rt_protected_call` / `_call_env` / `_drive` / `_drive_env` /
`_apply_any` / `_dyn_call` / `_panic` / `_throw` / `_go` の 9 本と、その共通の
本体 `protected`、`rt_unwind_pending` と `UNWIND_PENDING`、`rt_resume_unwind`。
extern の表は 266 → 255。

島側 6: `check-unwind` と、守られた枝を失って別名になった 5 つの emitter。

残したもの: `drive_to_completion` と `collect_words`（`rt_drive_body` /
`rt_drive_entry` / `rt_dyn_call` がまだ使う）、`CAUGHT_UNWIND`（トランポリンの
受け皿だったものが、そのままドライバの受け皿になった）、`IN_FLIGHT_TAG` と
`rt_throw` / `rt_throw_matches` / `rt_throw_take_value`（投げる側と、pad が
「これは自分のか」と訊く側は変わっていない）。

`tests/protected_call_test.rs` は `tests/driver_unwind_test.rs` に置き換えた。
旧版は**トランポリンの形**を LLVM IR で組み立てていた——その機構がもう無い。
新版はコルーチン ABI を手書きの Rust の本体で守る（入場でフレームを作り、
公表し、`pc` で分岐する）。IR でなく Rust なのは、ここで固定したいのが
**規約**だから: ドライバがどのフレームに訊き、答えをどう扱い、何を残すか。
IR のレベルは島が emit するようになった時点で `catch_throw_test` の 36 本が
端から端まで通している。

### 6 箇所のコメントがトランポリンを説明したままだった

`grep -rn rt_protected_` が作業リストを出した。うち 1 つは**島の SOURCE の中**
（`raising-binop-call` の「だから `emit-direct-call` を通す」）で、これは
コメント 1 文字でも成果物が無効になるので regen も付いてくる。ほかは
`build-fn-address` の宣言とドキュメント 2 箇所（「protected 形式が唯一の
呼び出し元」——いまは*すべての* Lisp 呼び出しが呼び出し元）、`call_state` の
「入れ子のドライバの例」、`crossing.rs` の「compiled 側の catcher」。

**消した機構の名前で grep するのが、この種の作業の最後の一歩。**
[[feedback-comments-outliving-implementations]] の「消したコードの帰結だけ残る」
がそのまま出る場所で、型検査もテストも何も言わない。

### C4 で消えなかったもの

- **`crossing.rs` は大半が残る。** プランは「139 行の大半が消える」と書いて
  いたが、そこは compiled↔interpreted の境界で、C4 が動かしたのは
  compiled の*内側*。`catch_compiled_panic` はインタプリタが compiled を
  呼ぶ入口として要り、`park_interpreted_error` / `unwind_interpreted_failure`
  は compiled が interpreted を呼ぶ出口として要る。C4 が足したのは
  `carried_unwind_error`（ドライバが運んできた payload を `EvalError` に
  する口）で、payload → `EvalError` の変換は `error_from_payload` として
  2 つの入口が共有する
- **「同時に飛ぶ throw は高々 1 つ」はまだ死んでいない。** `IN_FLIGHT_TAG` と
  `Heap::set_in_flight_throw` は 1 スロットのまま。ドライバは panic を
  捕まえてから次の frame を探す間ずっとそれを持っている。タスクごとに
  分けるのは B の側の作業

## C5. 境界の整理

### C5a: `apply` の答えを誰が出すか

compiled な `apply` は `rt_apply_any` への**素の呼び出し**だった。`rt_apply_any`
は C の関数なので**答えを持って返らなければならない**——その下にあるものすべてが
マシンフレームの上に立ち、置けなくなっていた。

いまは呼び出し地点が**呼び先を名指しして戻る**（`rt_frame_apply` +
`STATUS_CALL`）。`coroutine-apply` は `coroutine-call` と 1 命令も違わない。
違いは全部ドライバ側で、値を見て 3 通りに割れる:

| 呼び先 | ドライバの答え |
|---|---|
コルーチン本体 | 呼び出し側と同じ鎖にフレームを積む |
classic 本体 | 完走するので呼んで、答えを待っているフレームに渡す |
**インタプリタのクロージャ** | `Paused::Applying`——継続スタックを持つ者に渡す |

**答えが「呼び出しについての事実ではない」**という C4 と同じ形。直接呼び出しは
アドレスを知っている（チェッカーが名前を解決した）が、`apply` は*値*しか
知らない。値が何かを見て誰が走らせるか決められるのは、フレームを積める唯一の
当事者——ドライバ——だけ。

### 受け皿は C3 が作ってあった

`Paused::Applying` を受けたタスクドライバは、**そのタスク自身の継続スタック**に
適用を積む（`begin_applying`）。答えを捕まえるのは C3 が中断のために作った
`Frame::DriveCompiled` そのままで、`wake` は呼び先の戻り repr。

**鎖の側から見れば、スケジューラからの答えとインタプリタの呼び先からの答えは
同じもの**——どちらも「待っていた 1 語が値スロットに来る」。だから新しい Frame
変種は要らなかった。

### 脱出の経路が 1 本増えた

適用したクロージャが `throw` したら、もうマシンフレームは間に無いので Rust の
panic では運べない。継続スタックを普通に上がってきて `Frame::DriveCompiled` に
着き、そこから**状態として鎖に手渡す**（`FrameStack::raise` = `drive` を
`STATUS_UNWIND` で始めるだけ）。鎖の pad が見つけるものは、巻き戻していた
呼び出しが残していたものと同じ——だから `park_for_compiled` は 2 つのチャネル
（タグと値、payload の種類）の両方を書く。書かないと、鎖が declined して
`Paused::Unwinding` で出てきたときに `carried_unwind_error` が空を見る。

この `Frame::DriveCompiled` の巻き戻しアームは **C3 の時点で「構造上到達不能」と
コメントしてあった場所**。C5 で到達可能になった。中断では依然到達しない
（`Blocked` から出る道はスケジューラが値を置く 1 本だけ）ので、この frame が
スタックに在って巻き戻っているなら、それは必ず適用である。

### マシンフレームの上のドライバには別の答えを

`rt_drive_body` / `rt_dyn_call` / C FFI の thunk は、呼び出し側がマシンフレーム
なので `Paused::Applying` を継続スタックに渡せない。そこは
`FrameStack::run_to_end` が**その場で**インタプリタを回す——今日と同じ振る舞いで、
同じ制限付き。プランが B6 で名指ししている制限がここまで縮んだ。

**`pause_on_a_machine_frame` の `Applying` アームは `fatal`。** `run_to_end` を
使わずに `run`/`resume` を直接書いたドライバがあれば、それは規約違反であって
制限ではない。

**2026-09-18 の続き: `Paused::Suspended` もここで解決するようになった
（`typelisp-machine-frame-answer-now` 計画）。** それまでは `Suspended` も
`fatal`——`print-object` メソッドの中の `(recv ch)`、AOT の `defvar` 初期化子の
中の `(Chan::new ...)` が、**待たずに答えが出る操作ですら** abort していた
（インタプリタの `run_to_completion` は `try_now` で答えていたのに、compiled 側の
`run_to_end` は safepoint しか飲まなかったという非対称）。直し方は `sched::drive`
が自分の extent の間だけ `&RefCell<Scheduler<B>>` を thread-local
（`sched::DRIVING`、借用のみ・所有は今までどおり）に publish し、`run_to_end` が
`Paused::Suspended` を `sched::pending_wait` で `Waiting` にした上で
`sched::answer_now` に尋ねる——`(yield)`/safepoint は無条件で続行、答えが出る操作は
その語で `resume`、**本当に待つ操作は `sched::cannot_block_message` を
`CompiledPanic` として鎖の中へ `raise`**（インタプリタの拒否と同じ文言、
catchable）。これで `pause_on_a_machine_frame` の `Suspended` アームも `Applying`
と同じ「規約違反」の `fatal` になった——両方とも `run_to_end` が解決してしまうので、
ここに来ること自体が壊れている。

### C5b: `:dyn` も同じ形

`compile-dyn-call` は受け手の vtable id を読み、概念的な受け手を取り出し、
**スロットと引数をドライバに名指しする**（`coroutine-dyn-call`）。ドライバの
分岐は `apply` と同じ 3 通りで、1 つ増える: **スロットが空**のとき——具体型が
ユーザ自身の構造体でそのメソッドを誰もコンパイルしていない場合——インタプリタに
そのスロットが立つクロージャを reify してもらい、そこからは普通の apply。

これが `rt_dyn_call` が「呼び出し地点でなくランタイムに」存在した理由そのもの
で、同じ理由で判断がドライバに移った。

### タスクは複数の鎖の区間を持てる

`stream_test` の 1 本が `debug_assert_eq!(task.compiled.depth(), 0,
"a task drives one compiled chain at a time")` で落ちた。**C5 が壊した不変条件を
その assertion が名指しした。**

C5 より前、compiled から届いたインタプリタの呼び先は*マシン*フレームの上で
走っていた。C5 がそれを継続スタックに載せたので、その呼び先が compiled 関数を
呼ぶと、**1 つの `FrameStack` の中に compiled フレームの連なりが 2 区間でき、
間にインタプリタのフレームが挟まる**。各区間は自分の base まで駆動する
（`DriveCtx::base`）。

`resume` が base 0 を固定していたのは「入れ子のドライバは中断できないので、
他人のスタックの途中を再開するということ自体が無い」というコメント付きだった
——C5 がその予告を偽にした。[[feedback-comments-outliving-implementations]] の
3 度目。**assertion は同じ失敗の裏返しで、こちらは自分が壊れたことを言う。**

### C5c: 3 つが死んだ

`rt_apply_any`、`rt_dyn_call`、`build-closure-apply`。どれも「呼び出し地点から
呼ばれる C の関数で、答えを持って返らなければならない」形をしていて、それが
まさに下にマシンフレームを敷いていたもの。extern の表は 257 → 255。

残ったもの: `resolve_closure`（ドライバが値を見るときに使う。`rt_apply_any` から
切り出した本体そのまま）、`reify_dyn_slot` と `DYN_SLOT_CLOSURE`（スロットが
空のときインタプリタに訊く口）、`apply_on_this_frame`（マシンフレームの上の
ドライバが使う最後の手段）、`drive_to_completion`（`rt_drive_body` /
`rt_drive_entry` / C FFI）。

**SOURCE を 1 文字も変えていないのに成果物が変わった。** extern の表が
[[typelisp-island-hash-reads-forms]] の言う「ハッシュが見ていない 2 つ目の入力」
で、これはその実証。`git diff src/compiler.rs` が空のまま
`island_artifacts_are_fresh` が落ちる。

### 消した名前で grep すると 45 箇所出た

うち present tense で嘘になっていたものを直した（`fatal` の文面 2 つ——
使用者に見える——、ドキュメントリンク、`rt_dyn_call` が「スロットが空の場合を
扱う」と書いてあった箇所など）。残りは「かつては」「retired した」の歴史的
記述で、そのままが正しい。

**C4 と同じ手順が 2 度目に効いた。** 消した機構の名前で grep するのが最後の
一歩で、型検査もテストも何も言わない。

### grep は足りていなかった —— 組み込みの名前は文字列

全体スイートが `compile_test` で 1 本見つけた。
`a_closure_made_from_a_capturing_function_can_be_called_indirectly` が
`build-closure-apply` と `rt_apply_any` を **Lisp のソース文字列の中から**
呼んでいた。組み込みは `llvm_builtins.rs` の文字列 match でディスパッチされる
ので、**消した builder を呼ぶコードは `cargo check --workspace --all-targets`
の警告 0 をそのまま通る**。45 箇所の grep で名前は出ていたのに、コメントだと
思って読み飛ばしたのが取りこぼしの原因。

書き直し方は「呼び出し側をコルーチンにし、callee は classic のまま
`coroutine-apply` に載せる」。callee を classic に残したのは意図的で、
`resolve_closure` の `Callee::Classic`（ドライバが自分で呼び、unwind を捕まえ、
答えをフレームの値スロットに書いて渡す腕）には C5a 以降テストが 1 本も
無かった。

同じ grep で**宣言だけの死んだ記号**が 3 行出た。`rt_apply_any` を明示宣言して
いた `compile-function` テスト 2 本と、そのうち 1 本の `rt_push_sexpr_root`
（C1 以降 `bind-captures` は `binding-slot` を配るだけで root しない）。
使われない `add-function` 宣言は LLVM が解決しようとしないので、**消えた記号を
宣言していても誰も何も言わない**。3 行消して 3 本とも緑。

### `run_to_completion` の拒否は残るが、理由が入れ替わった

プランは C5 で消えると書いていたが、消えない——**指している相手が変わった**。
「compiled から来たので中断できない」は C5 で偽になり、いま Rust フレームを
本当に握っているのは `Interp::apply` と公開 API、`eval` 組み込み、
`print-object` メソッド、リーダマクロ、そしてそれらから入ったドライバ。
文面をそう直した。

**2026-09-18: `(yield)` は例外から外れた。** 譲る相手を探しているだけの
`(yield)` を「待つことになった」と一緒に拒否するのは、譲る相手が無い場所で
「譲らない」以上の意味を持たない拒否だった。ここでは無条件で続行、
compiled 側の safepoint と同じ扱いにした。拒否の文言自体は
`sched::cannot_block_message` に括り出して `FrameStack::run_to_end` と共有——
2 つの前端が同じ理由で同じことを拒否している以上、言葉も 1 つでよい。

## C6. ABI を暗黙にしない

プランは C6 を「ブートストラップと再生成」と書き、`FORMAT_VERSION` を上げて
ロード側が本体の ABI を見て呼び分ける段を作れ、不動点に達したら旧経路を消せと
していた。**調べたらその大半は既に済んでいた**:

- ダンプは `body_abi` と `emits_abi` をユニットごとに記録し、`CompiledLibrary`
  がそれを呼び出しまで運んでいる（C2d で入った）
- 不動点も成立している。`ISLAND_DUMP_BODY_ABI` と `ISLAND_DUMP_EMITS_ABI` は
  どちらも `BODY_ABI_COROUTINE`
- classic ABI の本体を今も作るのは C FFI の thunk の**内側**だけ

残っていたのは削除ではなく、**答えを言わずに済む場所**だった。

### 既定値が 4 つあり、正しかったのは 1 つだけ

| 場所 | 状態 |
|---|---|
`aot.rs` | `target.get_type() == coroutine_fn_type()` で導出。**唯一の正直な場所** |
`CompiledBody::body_abi` | 既定値 classic。**両方の実装が上書きしているので誰も使っていない** |
`CompiledFn::new` | classic 決め打ち。`new_multi` は引数で受けるのに |
`capture_types` | `body_abi`/`emits_abi` とも classic。本体を持つユニットが使うと黙って嘘になる |

`CompiledBody::body_abi` の doc は理由まで書いてあった——「**島自身が
切り替わるまでは全ての生産者がこう言う**」。島は C2d で切り替わっている。
[[typelisp-c2d-island-coroutine-abi]] の「成果物の既定値は理由が消えても黙る」
がそのまま当たっていた。

`EMITTED_BODY_ABI` の doc も「JIT コンパイルされた**すべての**関数に記録される
ABI」と主張していたが、`CompiledFn::new` はそれを無視して classic と書いて
いた。書いた値が読まれていなかったから誰も気づかない。

### 直し方は「省略で答えられなくする」

- `CompiledBody::body_abi` を**必須メソッド**に（既定値を削除）
- `CompiledFn::new` が `body_abi` を受ける。`new_multi` と対称になり、4 つの
  呼び手が自分で答える
- `capture_types` から `items` を外した。**本体を持つユニットは ABI を言わない
  構築子を構造的に使えない。** この関数のコメント 2 箇所（session ダンプと
  prelude）が「同じ間違いを 2 度した」ことを既に記録していて、どちらも
  「呼ばれたときに初めて誤った規約になる」と書いてある——それを型で防いだ

C FFI の thunk は 1 つ得をした。`CompiledFn::new` に classic と書かれた値は
**読まれない場所に置かれていた**（`_code` フィールド）一方、`FfiThunk` は
`body_abi()` を COROUTINE で上書きしていた——Lisp 名の下にあるのはコルーチン
入口だから。いまは `CompiledFn` に COROUTINE と言わせ、`FfiThunk` はそれを
読み返す。答えは記号を引いた場所に 1 つ。

### `emits_abi` の「何も言わない」を実在の値で綴っていた

`UnitState` の doc は「島でないものについては*意味が無く、`body_abi` と等しい*」
と定義している。ところが session ダンプと prelude の呼び出し地点は
`BODY_ABI_CLASSIC` を渡していた——**実在の ABI 値を「答えなし」として**。
次の切り替えで `emits_abi` を読む者は、その 0 が「classic を出す」なのか
「何も出さない」なのか区別できない。doc の規約どおり `body_abi` と同じ値に直した。

### SOURCE を触らずに prelude が変わり、ハッシュは黙っていた

`emits_abi` を 1 バイト変えただけで prelude の記録が変わる。結果:

- `prelude_artifacts_are_fresh`（**ハッシュ判定**）は **ok**
- `the_committed_prelude_matches_a_fresh_build`（**バイト比較**）は **FAILED**

[[typelisp-island-hash-reads-forms]] の「入力は 3 つだがハッシュは 1 つしか
見ていない」の 3 度目。**2 つの粒度で番人を置いてあるのはこのため**で、
`island_artifacts_test` のモジュール doc がまさにそう書いている。島は
再生成不要だった（`bootstrap.rs` は最初から実値を渡していた）。

### 番人を 1 本足した

`the_committed_prelude_records_the_abi_its_bodies_answer_to`。prelude の
`body_abi` が `EMITTED_BODY_ABI` と一致し、`emits_abi` がそれを繰り返している
ことを検査する。**修正前の成果物に当てて、`emits_abi` の側で鳴ることを確かめた**
——バイト比較は「違う」しか言わないが、これはどのフィールドがなぜ違うかを言う。

コメントが記録していたバグ（「coroutine なのに classic と書かれた prelude を
インタプリタが `f(args, argc)` で呼び、フレームの位置に引数ポインタが届いた」）
を、いまは誰も検査していなかった。

## C7. ループの safepoint

`compile-loop` の後退辺が `rt_loop_safepoint` を呼び、非ゼロなら中断する。
プランは任意扱い（「マルチコアに進むときに必要になるもの」）だったが、
入れてみると**前段が 1 つ必須**で、そちらのほうが中身がある。

### プランの前提が 1 つ間違っていた

プランはこう書いている——「呼び出しの無いタイトループには driver 往復が
来ないので safepoint が無い」。**driver 往復はそもそもスケジューリング点では
ない。** `FrameStack::drive` は状態語でループしていて、`STATUS_CALL` を受けたら
呼び先に入ってそのまま回り続け、外に戻るのは `STATUS_SUSPEND` のときだけ。

つまり:

| 観点 | 穴だったのは |
|---|---|
**飢餓**（他のタスクが走れない） | **あらゆるループ**。呼び出しの有無は関係ない |
**コレクタ**（全スレッドを止める） | 呼び出しの無いループだけ。往復は「止められる機会」なので |

プランの言い方が正しいのは下の行で、C7 の動機として挙げているのもそちら。
上の行はプランが v1 の制限として別に挙げていた「タイトループが他のタスクを
飢えさせる」で、**同じ 1 箇所が両方を埋める**。

### 前段: machine frame のドライバが safepoint を飲めること

`Interp::call_coroutine` や AOT の入口、C FFI の thunk は Rust
フレームの上に立っているので、`Paused::Suspended` を `raise` する。
**そこへ後退辺の poll を足すと、compiled クロージャ値経由のループが軒並み
壊れる**——しかも失敗の文面は「中断」と言い、ループとは言わない。

`run_to_end` に腕を 1 つ足して解決した: 中断の種別が safepoint なら、
値スロットに 0 を書いて即再開する。**申し出を断ることが、それを honour する
ことの全部**——何も待っていないのだから。実在の待機（`sleep`/`wait`）は
今まで通り断り、種別を戻してから返すので、エラーの文面も変わらない。

**種別を `SUSPEND_YIELD` と分けたのはこのため。** 一緒にしていたら、
`print-object` メソッドやリーダマクロの中の明示的な `(yield)` が黙って
何もしなくなる——`(yield)` が約束しているものとは別の約束になる。

### 決めるのは Rust、分岐するのは島

`rt_loop_safepoint` が可否を決め、島は答えに分岐するだけ。`rt_suspend_*` が
「何を待つか」を決め `coroutine-suspend` が活性を終えるだけ、というのと同じ
分割。周期（256）は**任意の数で、任意でよい**——この プロジェクトは実行時間を
計測しないので選ぶ根拠が無く、safepoint に要るのは有限であることだけ。

### 不動点は 1 ラウンド

**島に実ループが 1 つも無い**（`(loop ` の出現は `icase` のラベルと
コメントだけ）ので、島自身の本体は safepoint を持たない。ラウンド 1 と 2 の
md5 が一致。prelude は +22,200 バイト——そちらのループは全部 poll を持つ。

### テストが本物かを確かめる

2 本とも、**修正前に当てて鳴ることを確認した**:

- 周期を 1,000,000 にする（発火しない）と、`ba` を期待する interleave の
  テストが `ab` で落ちる——ループが本当にスレッドを握っていた証拠
- `run_to_end` の腕を外すと、machine frame のテストが
  「a compiled closure suspended underneath an interpreted caller」で落ちる
  ——コメントが予告したとおりの文面

### 途中で見つけたもの: `Interp::apply` は `compiled` を見ない

最初のテストは `print-object` のメソッドを compile して印字経路から呼んで
いた。`(compile spinner::print-object)` は `true` を返し、`disassemble` すると
`callq _rt_loop_safepoint` も出ている。なのに `rt_loop_safepoint` を
`panic!` にしても鳴らなかった。

`Interp::apply` が `FnDef::body` を `eval_core` で走らせるだけで
**`FnDef::compiled` を一切見ない**ため。同じ経路に乗っているのは
`print-object` のディスパッチ、`format` の `~/.../`、リーダマクロの 3 つ。
どれも `(compile ...)` の効果を受けない。

**直した（同日）。** `apply` を変えるのではなく、3 経路を `Interp::enter` へ
合流させた——`enter` が「compiled があればそれを、無ければ tree-walk」を決める
唯一の場所で、`apply` はその**インタプリタ側の半分**。マクロ展開は既に同じ
理由で `enter` へ移されていて、その呼び出し地点のコメントが議論ごと残っていた。

副産物として `trace` が 3 経路に届くようになった。`trace_test.rs` の
モジュール doc は「名前付き関数へのあらゆる呼び出しが `enter` を通る」と
書いていたが、**この 3 つについては偽だった**。

テストは `trace` の出力で取る（`(compile ...)` の有無で結果の値は変わらない
——同じソースなのだから——のに対し、`enter` を通ったかどうかは観測できる）。
**最初の版は偽陽性だった**: `(trace f)` は compiled な本体を持つ定義に対して
名前入りの注記を書くので、`contains("pt::print-object")` は修正を戻しても
通ってしまう。呼び出し行 `(pt::print-object` を見るように締めた。

### 手順の誤り 1 件

C6 の検証バッチを流している最中に C7 の編集を始めてしまい、島の成果物が
SOURCE に対して古くなって、`compile_file_test` 以降の 4 ターゲットが
全滅した（58 + 3 + 19 + 17 件）。**cargo を同時に走らせないだけでは足りない
——編集も同じく無効化する。** C6 と C7 が偶然まったく別のファイルを触って
いたので、`git add` を明示して分けられた。
