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

残る穴が 1 つ: **呼び出しの無いタイトループには driver 往復が来ない**ので safepoint が
無い。`compile-loop` の後退辺に 1 箇所ポーリングを入れれば埋まる（C7、Phase C の完了
条件ではない）。

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
