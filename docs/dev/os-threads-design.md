# goroutine を OS スレッドで走らせる + `Thread<T>`（設計）

プランは `~/.claude/plans/goroutine-os-goroutine-os-twinkly-thacker.md`。実装が進むにつれて
この文書は「今どうなっているか」を、プランは「フェーズの進み具合」を追う——両方が要る。

## 1. なぜ

軽量スレッド（`go`/`Task<T>`/`Chan<T>`/`select`/`WaitGroup`/`Mutex<T>`）と AOT スケジューラは
完了しているが、**全タスクが 1 つの OS スレッドで協調的に動く**（`crates/typelisp-rt/src/sched.rs:42-45`）。
2026-09-08 の B6'（`~/.claude/plans/go-gorutine-adaptive-raccoon.md`）で「A: GIL / B: 独立ヒープ /
C: シングルスレッド」を比べて C を選び、「マルチコア並列が無い」を v1 の制限として受け入れた
（`docs/dev/TODO.md`）。この設計はその制限を外す。

壁はスケジューラでなくヒープにある。`Scheduler<B>` 自体は「1 つのスケジューラをロックで共有
するか、各スレッドが自分のスケジューラを持つか」どちらでも所有権の形が同じになるよう
最初から書かれている（`sched.rs:22-30`）。動けないのは `Heap` が `!Send` で `ACTIVE_HEAP` が
1 つの thread-local でなければならないから（`crates/typelisp-abi/src/lib.rs:46-80`）。

| 箇所 | 現状 | 複数 OS スレッドで壊れる理由 |
|---|---|---|
| `typelisp-abi/src/lib.rs:46-80` | `ACTIVE_HEAP` は thread_local、`active_heap()` は `&'static mut Heap` を返す | 1 スレッド = 1 ヒープの前提。約 190 の `rt_*` シムが毎回 `&mut Heap` を取る |
| `typelisp-mem/src/heap.rs:72-174` | `Heap` は cons アリーナ・`str_slots`/`box_slots`（`Vec<Option<_>>`）・4 系統のルート・interning 表を 1 構造体に持つ | `Vec` の伸長で再配置される、free list が単一、`current_stack` が 1 本、`cell_registry` が `Rc` |
| `heap.rs:2330-2458` `gc()` | 単一スレッドの mark-sweep | 他スレッドを止める機構が無い |
| `typelisp-mem/src/value.rs:98` | `ConsRef(*mut Cell)` → `Value: !Send` | `CompiledTask` が `!Send` な唯一の理由（`sched.rs:1428`） |
| `typelisp-rt/src/sched.rs:481-524, 1114-1188` | `Scheduler<B>` は `RefCell` で所有、`drive` が 1 スレッドで全タスクを `step` | ロック無し、待機は `std::thread::sleep`/`poll`（`sched.rs:1149`, `1024`） |
| `typelisp-rt/src/lib.rs:2846-2856` | `IN_FLIGHT_TAG`/`CAUGHT_UNWIND` は thread_local。`CompiledTask::deliver(Err)`（`sched.rs:1622-1631`）は**起こした側のスレッド**で `park_activation_unwind` を呼ぶ | payload が別スレッドの TLS に残り、被起床タスクの `raise` が `resume_activation_unwind` の fatal（`lib.rs:3074`）に落ちる |
| thread_local 群（§5） | `GLOBAL_INDEX`・`VTABLES`・`STREAMS` などは thread_local | ワーカーからグローバル変数・ストリームが見えない。thread_local なのは `cargo test` が別スレッドで別 `Heap`/`Interp` を並走させるため（`stream.rs:40-67`）で、素朴なプロセス共有にすると隔離が壊れる |
| `typelisp-front/src/eval/interp.rs:4637-4653` | typl の print hooks は全項目が `with_active_interp`（thread_local の `*const Interp`） | ワーカーでは `None` → enum が `<unknown-variant>`。`rt_print` はシムなので中断で救えない |
| `src/compile/mod.rs:222-225` | `destroy_retired_llvm` は次の JIT 構築の先頭で旧オブジェクトを**全破棄** | ワーカーが走らせている旧 JIT 本体が REPL の再定義で解放される |
| `typelisp-front/src/prelude.rs:6361-6373, 6415-6419` | `WaitGroup::add/done` は `(setf self::count (+ self::count n))`、`Mutex::unlock` は `(len gate)`→`send` | 協調的なら atomic だった RMW が真の並列では競合する |

front の `Task`（`typelisp-front/src/eval/interp/core_cps.rs:520-538`）は `Rc<dyn CompiledBody>`
（`core_cps.rs:184`）・`Rc<FnDef>` を含み、`Interp` 自体も `Rc`/`RefCell` なので **main スレッド
固定**。`CompiledTask`（`sched.rs:1435-1447`）は `Value` さえ `Send` になれば移動できる。

## 2. 方針（ユーザ決定、2026-09-18）

- **GIL 段階を置かず最初から真の並列**: stop-the-world GC を実装する。
- 直接 OS スレッドを扱う機能は **Rust 風の spawn / join**（`thread`/`Thread<T>`/`join`）:
  専用 OS スレッドで走り、多重化されず、中でブロックしても goroutine は止まらない。
  `Chan<T>`/`Mutex<T>`/`go` はそのまま使える。
- 対象は **AOT + `typl` の両方**。typl では compiled な `go` もワーカーへ出し、interpreted
  に触れた瞬間に `NeedsMain` で main へ移送する（戻さない）。

## 3. `Heap` を「スレッドごとのビュー」と「プロセス共有部」に分ける

`Heap` という名前と `&mut Heap` の API はそのまま残す（シム約 190 箇所・front・テストを
触らないため）。中身を 2 層にする:

```rust
pub struct Heap {                    // スレッドごとのビュー。active_heap() が返すもの。!Send
    shared: Arc<HeapShared>,
    cache: AllocCache,               // cons の free セル・str/box の空きスロット index を数十個
    current_stack: *mut RootStack,   // 今このスレッドが走らせているタスクのルートスタック
    home_stack: RootStackId,         // attach 時に必ず作る（drive の `home` `sched.rs:1121` が前提にする）
    session_roots: Vec<Value>,       // main の compile セッション用
    cell_registry: Vec<sync::Weak<BoxId>>,   // Rc → Arc（GC が他スレッドから読むため）
    in_native: bool,                 // debug: native 区間で heap を触ったら即 panic
}

pub struct HeapShared {              // Send + Sync（unsafe impl、不変条件は §4）
    arena: Mutex<Arena>,             // chunks / global free list / cap / growth_limit
    strs: Slab<String>,              // chunk 式。chunk 表は固定長 `[AtomicPtr<_>; 64]`（倍々 chunk）
    boxes: Slab<BoxedObj>,           //   外側の Vec も再配置されない形にする
    permanent_roots: Slab<Value>,    // 同上。defvar グローバルは index でロック無しに読み書き
    root_stacks: Mutex<Vec<Option<Box<RootStack>>>>,   // RootStackId は index のまま
    interning: RwLock<{type_keys, paths, locs, macro_chars, dispatch_chars}>,
    threads: ThreadRegistry,         // §4
    gc_count: AtomicU64, gc_stress: AtomicBool,
}
```

- `Heap::with_capacity(n)` は shared を作って最初のビューを返す（既存呼び出し無変更）。
  `Heap::attach(&Arc<HeapShared>)` が 2 本目以降のビュー（ワーカー・専用スレッド）で、
  home ルートスタックを作り `ThreadRegistry` に登録する。
- `Slab<T>`: `Vec<Option<T>>` の置き換え。chunk を足すだけで既存要素を動かさない
  （cons アリーナと同じ規律、`heap.rs` 冒頭）。chunk サイズは 2 の冪でシフト/マスク。
- `cons()`（`heap.rs:2047`）: cache から取る → 空なら arena ロックで N 個補充 → global も空なら
  grow / GC（§4）。`free_count` は近似（Atomic）。
- mark 配列（`str_marks`/`box_marks`）は GC 専用なので `HeapShared` に置き、STW 中だけ触る。
- `new_root_stack`/`switch_to_root_stack`/`drop_root_stack`（`heap.rs:483-529`）は registry を
  ロックして `Box<RootStack>` のポインタをビューに入れる。`push_root`/`pop_root`/`set_root`/
  `RootScope`（`heap.rs:2461-2524`、`current` 経由なので無変更）はロック無し。
  `chan_roots`（`sched.rs:792-825`）はスケジューラロック下で per-thread の `current` を
  一時切替するだけなので競合しない。
- `Value`/`ConsRef` に `unsafe impl Send`（ヒープがプロセス共有になった、が根拠）。
  `assert_send::<CompiledTask>()` をテストに昇格（`tests/sched_send_test.rs`）。
- **`in_flight_throw`（`heap.rs:101`）はビューにもシェアにも置かない**——§5 でタスクへ移す。

## 4. stop-the-world GC

`ThreadRegistry`: 登録スレッドごとの状態 `Running | AtSafepoint | Native` と、
`gc_requested: AtomicBool`、`Mutex + Condvar`。

- **safepoint**: (a) 全確保の slow path（cache 空）、(b) `rt_loop_safepoint`
  （`coroutine.rs:677-689`。256 回に 1 回の yield は残し、毎回 `gc_requested` を relaxed load）、
  (c) `drive` の step 境界、(d) `leave_native`。呼び出しはフレームを確保する
  （`alloc_frame`）ので (a) がほぼ全域を覆う。
- **native 区間**（`enter_native`/`leave_native`）: `std::thread::sleep`、`poll_ready`
  （`os.rs`）、スケジューラの Mutex/Condvar 待ち、FFI 呼び出し（`src/compile/ffi.rs`、
  コールバック thunk は leave→enter で挟む）、stdin/パイプの読み、`Command::wait`、
  **REPL の行読み**（`src/main.rs` の rustyline。main がプロンプトで止まっている間に他
  スレッドが GC できなければ全員止まる）。
  **規則: 他の typelisp スレッドの進行に依存しうる待ちは必ず native**。native 区間で
  ヒープを触ったら debug で即 panic（`in_native`）。
- **`answer_now`**（`sched.rs:433-439` ← `run_to_end` `coroutine.rs:276`）の順序:
  `enter_native → lock → leave_native → try_now → unlock`。ロックを持ったまま native の
  ままだと `alloc_option`（`sched.rs:881, 926`）が `in_native` panic を踏む。
- **GC 手順**（要求したスレッドが collector。同時に 2 本が要求したら 2 本目は waiter に
  降格）: フラグを立てる → 他の全スレッドが AtSafepoint か Native になるまで待つ
  （**arena ロックはこの確認の後に取る**。arena 待ちは native ではない）→ mark（registry の
  全ルートスタック・permanent・各スレッドの session/cell_registry・各タスクの in-flight
  状態スロット・macro 表）→ sweep（アリーナ・Slab）→ **全スレッドの cache を空にする**
  （cache のセルは未確保なので sweep が回収済み。二重払い出し防止）→ フラグを下ろして notify。
- safepoint はヒープ API の境界にしか無いので、STW 時に Rust の `Vec`/`HashMap` が操作途中で
  止まることは無い（mark が壊れた構造を読まない根拠）。
- ロック順序: arena ロックを持ったまま GC しない。registry ロックを持ったままヒープを
  触らない。スケジューラロック待ちは native なので、ロック保持中の GC 要求（`recv` が
  `some` 箱を確保する `sched.rs:881`）はデッドロックしない。
- debug 用 watchdog: フラグを立てて N 秒 parked にならないスレッドの名前と状態を stderr に
  出す（native 忘れの発見器）。

### 実装（Phase 2）と上の案からの差分

`crates/typelisp-mem/src/heap.rs` の `ThreadRegistry`/`ViewSlot`、`Heap::gc`/`safepoint`/
`enter_native`/`leave_native`/`native`。テストは `crates/typelisp-mem/tests/shared_heap_threads.rs`。

- **safepoint は `cons` だけで、`alloc_string`/`alloc_boxed` は safepoint にしない。** 上の (a)
  「全確保の slow path」は成り立たなかった: 文字列・箱の確保は今まで一度も GC しなかったので、
  呼び出し側は未ルートの値を抱えたまま呼んでいる（`alloc_struct` に渡す `fields` がその典型）。
  そこで他スレッドの GC を走らせるとそれらが掃かれる。safepoint は「そこで GC が起きても
  よいと呼び出し側がすでに約束している点」だけ——`cons`・`rt_loop_safepoint`（毎回。
  256 回ごとの yield とは別）・`drive` の step 境界・`leave_native`。
- **collector は自スレッドのビューを待たない**（`ViewSlot.thread`）。同じ OS スレッドに
  2 本目のビューを作る既存テスト（`tests/mem_test.rs` の attach 系）がそうで、自スレッドが
  collector である以上、他のビューは何かの途中ではありえない。
- **2 本目の collector は降格したあと自分でも GC を走らせる。** `gc()` の呼び手には
  「呼んだ後に始まった GC」を約束する（`cons` の再試行判定がそれに依る）。
- **`cons` の「空か確認」と「取り出し」は 1 つの critical section**（`pop_free`）。
  別々だと他スレッドが最後のセルを取った後に null を pop する（新テストで実際に踏んだ）。
  `HeapExhausted` は「自分の GC が 0 個しか回収せず、それでも空で、伸長もできない」。
- **`AllocCache` は入れていない。** 正しさには要らない最適化で、入れると「GC 時に全ビューの
  cache を空にする」手順も要る。`cons` は毎回 arena の `Mutex` を取る。
- collector は registry の `Mutex` を mark〜sweep の間ずっと持つ。その間 attach/detach/
  起床はできない（`register` は GC 中なら終わるまで待ってから `RUNNING` で入る）。
- ビューの `Drop` は home ルートスタックを解放してから registry から抜ける（抜けないと次の
  GC が永遠に待つ）。
- `session_roots` は `ViewSlot` へ（他スレッドの collector が mark する）。`gc_count`/
  `gc_stress` は `HeapShared` の atomic（どのビューからの GC もヒープ全体の GC）。
- `HeapShared` は `unsafe impl Send + Sync`（根拠はコメント: 全フィールドがロックか atomic の
  裏、ロックが守らないセルは STW と §6 の規則が守る）。
- watchdog は 5 秒、debug ビルドだけ。待ち続ける（タイムアウトではない）。
- native に指定した箇所: `drive` の `sleep`、`wake_io` の `poll`（timeout 0 以外）、
  `sys_builtin::sleep`（interpreted な `sleep`）、`ed-open` の子プロセス待ち、
  `stream-read-char`/`stream-read-byte`（ストリーム表のロック取得ごと）、`step` のプロンプト読み、
  REPL の `rl.readline`、FFI の C 呼び出し（thunk が `rt_ffi_enter_native`/
  `rt_ffi_leave_native` で挟む。C 側に渡るのはスカラと C 文字列だけなので C はヒープ値を
  持たない）。

**ストリーム表のロック（Phase 2 の後に修正）**: `stream::with_streams(heap, f)` は
**ロック待ちも `f` も丸ごと native**。`f` はブロックしうる（stdin・パイプ）し、ロックは
そういう読みをしているスレッドが持っているかもしれない。`StreamTable` は Rust の値しか
持たず、`heap` を引数で借りているので `f` はヒープを捕まえられない（借用検査が保証）。
同じ理由で stdout への書き込み（print クレートと front の `write_stdout`、`emit`/
`block_end`/`flush` は `&mut Heap` を取るようになった）も native。

**規則: ロックは native 区間の内側で取って内側で離す。** 外で取ったロックを持ったまま
`leave_native` すると、GC 中なら park する——そのロックを native の外で待つスレッドが
いれば collector はそれを待ち、ロックは返らない。遅延でなくデッドロックになる
（`crates/typelisp-rt/tests/stream_lock_native_test.rs` を旧形に戻すとハングする）。

native を増やすと GC 点が増える: native に入る前に抱えている値はルートが要る。
`exec` の `(expr ..)` は評価結果を `flush_pretty` の間ルートし、`trace` の入口/出口は
引数と戻り値を自分でルートするようにした。ワーカーには `shared::set_rt_shared` で main の
`RtShared` を渡す（ストリーム表を共有する）。

**Phase 3 に残したもの**: スケジューラロック待ちの native 化と `answer_now` の順序
（上の規則そのもの。ロックがまだ無い）。

## 5. 飛行中の状態はタスクが持つ（`IN_FLIGHT_TAG` / `CAUGHT_UNWIND` / `in_flight_throw`）
（Phase 1e で実装）

3 つとも「スレッドの」状態として TLS/`Heap` にあるが、`compile-unwind-protect` の cleanup
pad（`src/compiler.rs:5614-5628`）は任意の式なので throw 飛行中に `(sleep)`/`(recv)` で
中断でき、**既に単一スレッドでもタスク切替を跨ぐ**（別タスクの throw に上書きされうる
潜在バグ）。真の並列ではさらに `deliver(Err)` がロック下で起こした側のスレッドの TLS に書く。

実装は2本立て——TLS の2つ（スカラ）と値（GC ルートが要る）で置き場所を分けた:

- **`IN_FLIGHT_TAG`/`CAUGHT_UNWIND`（タグと捕捉済み unwind payload、どちらもスカラ）**:
  `typelisp_rt::ParkedUnwind`（`tag: Option<String>` + `caught: Option<Box<dyn Any+Send>>`）
  にまとめ、`take_parked_unwind`/`restore_parked_unwind` で save/restore する。
  `CompiledTask`/front の `Task` それぞれに `parked_unwind: ParkedUnwind` フィールドを足し、
  `step`（`CompiledTask::step` / `Interp::step_task_isolated`）が呼ばれるたびに
  「自分の保存分を thread-local へ復元 → 本体を実行 → thread-local から取り出して自分に
  保存」を行う。ネストする呼び出し（`run_to_completion` が別タスクを直接 step する経路）も
  Rust の呼び出しスタックがそのまま save/restore のスタックになるので正しく動く。
  `deliver(Err)`（`CompiledTask::deliver`、旧 `sched.rs:1622-1631`）はもう TLS に触らず
  `self.parked_unwind = ParkedUnwind::panic(message)` を直接代入するだけ——`deliver` は
  「起こした側」と「起こされる側」が別タスクになりうる場所（`deliver_to` は root stack だけ
  切り替える）なので、TLS 越しに渡すと次に `step` した別タスクに渡ってしまう。
- **`in_flight_throw`（値、GC ルートが要る）**: `Heap` 単体のフィールドではなく
  **`RootStack` 1本ごとのフィールド**にした（`sbase+2` という追加スロットではなく、
  `RootStack { roots, in_flight_throw }`）。`Heap::in_flight_throw`/`set_in_flight_throw`
  は `unsafe { &*self.current }.in_flight_throw` への薄い窓口——「今 current な
  RootStack」は必ず「今 step している task 自身の RootStack」なので、タスクごとに
  別オブジェクトである時点で自動的に隔離される（`CompiledTask`/front `Task` 側に
  3本目の状態スロットを足す必要がなかった）。GC の mark はタスクの `roots` を歩く
  既存ループにこの1フィールドを混ぜるだけ（`heap.rs`の`gc()`、`root_stacks`ループ）。
  `deliver`の Err 枝は `deliver_to` が root stack を切り替え済みなので
  `heap.set_in_flight_throw(None)` を直接呼んでよい（このタスク自身のスロットだけ触る）。
- `sbase+2`案を取らなかった理由: `tests/driver_unwind_test.rs` はスケジューラ/タスクを
  一切経由せず `FrameStack::run` を裸で叩いて unwind プロトコルだけを検証しており、
  状態スロットが3本存在する保証がない（`heap.set_root`は範囲外 index で panic する）。
  `RootStack` 埋め込みならどんな `Heap`/`RootStack` でも常に有効なので、このテストを
  タスクの体裁に合わせて書き換える必要がなかった。
- 退行テスト: `tests/concurrency_test.rs`の
  `a_task_switch_during_a_cleanup_does_not_corrupt_the_parked_throw` —
  タスク`a`が`unwind-protect`のcleanupで`sleep`し、その間にタスク`b`が別のタグで
  catch/throwを完走しても、`a`が再開したときの throw は`b`に汚染されず自分のタグ/値の
  ままであることを検証する（この節が説明している潜在バグの再現）。

## 6. データ競合の意味論（Go の立場を採る）

`Value` は 16 バイトの enum で、2 スレッドが同じフィールド/グローバルを競合して書くと
torn write が起きうる。Rust の `Vec`（`Vector<T>` の push）/`HashMap` を競合して変更すると
UB。**Go と同じく「データ競合のあるプログラムは未定義」と文書に明記し、共有可変状態は
`Mutex<T>`/`Chan<T>` を通す**。ランタイム内部の共有表だけがロックで守られる。

prelude 自身がこの規則を破ってはいけない:
- `WaitGroup`（`prelude.rs:6355-6373`）: `count` を `Chan<int>`（容量 1）のトークンにし、
  `add`/`done` は `recv → send (± n)` で RMW を直列化。0 になったら waiter 用の `Chan<()>` を
  close して全員起こす（今の `gate` の使い方を維持）。
- `Mutex::unlock`（`prelude.rs:6415-6419`）: `(len gate)` の TOCTOU を `select` の即時分岐
  （`send` できなければ panic）に置き換える。

## 7. ランタイム実体ごとの共有表（thread_local の分類）

`cargo test` は 1 プロセス内で別スレッドに別 `Heap`/`Interp` を立てるので、プロセス大域
（`OnceLock<RwLock<_>>`）にはしない。**「ランタイム実体ごとの `Arc` の表 + thread_local は
そのハンドル」**にし、`Heap::attach`/`Workers::start` が main の `Arc` を複製する。

| thread_local | 今の場所 | 扱い |
|---|---|---|
| `GLOBAL_INDEX` | `rt/src/lib.rs:4324` | `RtShared.globals: RwLock<Vec<usize>>`（値本体は `permanent_roots` Slab の index） |
| `VTABLES` | `rt/src/lib.rs:4464` | `RtShared.vtables: RwLock<_>`（登録は起動時） |
| `STREAMS` | `rt/src/stream.rs:65` | `RtShared.streams: Mutex<StreamTable>`（ハンドル id は不変） |
| `ENUM_NAMES` | `print/src/aot.rs:39` | `PrintShared.enum_names: RwLock<_>` |
| print `HOOKS` / read `HOOKS` | `print/runtime.rs:76`, `read/runtime.rs:33` | fn ポインタ表。ワーカー起動時に main の値を複製（AOT）。typl は §8 の detached hooks |
| `APPLY_INTERPRETED` / `DYN_SLOT_CLOSURE` | `rt/lib.rs:1516, 4598` | **ワーカーには複製しない**（`Interp` 直呼び）。未設定なら §8 の規則 |
| `CAUGHT_UNWIND` / `IN_FLIGHT_TAG` | `rt/lib.rs:2846-2856` | §5 でタスク側へ |
| `ACTIVE_HEAP`, `call_state::*`, `SAFEPOINT_COUNTDOWN`, `DRIVING`, `SESSION`, `DRIBBLE` | 各所 | per-thread のまま（実行状態） |
| `ACTIVE_INTERP`, `BACKEND`, `INTERPRETED_ERROR`, `EVAL_DUMP`, `LLVM_HANDLES`, `FRAME_CTXS` | front / src/compile | main 専用のまま |

`RtShared` は `typelisp-rt`、`PrintShared` は `typelisp-print` に置く。thread_local は
`RefCell<Option<Arc<_>>>` で、`rt_heap_init`/`Interp::new` が作り、`Heap::attach` の隣で複製。

## 8. スケジューラの多スレッド化

```rust
struct SchedShared<B> { sched: Mutex<Scheduler<B>>, wake: Condvar, io_wake: SelfPipe,
                        failure: Mutex<Option<B::Error>> }
enum Affinity { Any, Main, Dedicated(ThreadKey) }   // Slot ごと
```

- ワーカー N 本（`TYPELISP_THREADS`、既定 `available_parallelism()`）が
  `drive_worker(shared, heap_view)` を回す: ロック → `next_ready(Affinity::Any)` → 解錠 →
  `step`（ロック無し）→ ロック → `put_back`/`block`/`finish`。
- チャネル操作・`select`・`wake_due`・`answer_now` はロック下。`DRIVING` thread_local の
  借用（`sched.rs:393-425`）は per-thread のまま。
- ready は affinity 別: `ready_any`、`ready_main`、`ready_dedicated: HashMap<ThreadKey, TaskId>`。
  main は `Any | Main` を取る。専用スレッド（§9）は自分のキーだけ取る。
- **起こす側**: `wake`（`sched.rs:869-876`。rendezvous・close・`wake_due`・`finish` 全部）で
  `wake.notify_all()`。`admit`/`block(Io)`/`block(Until)`（より近い deadline）/`finish` は
  self-pipe に 1 バイト書いて poller を起こす（書き過ぎは無害）。
- 空のとき: 1 本が poller。`wake_io`（`sched.rs:1024-1054`）を「`(slot, fd, interest)` の
  snapshot → native で poll → 反映」に 3 分割し、反映時に**その slot がまだ同じ
  `Waiting::Io{fd}` か**を確かめる（ロックを外している間に `wake_due`・スロット再利用が
  起きる）。他は Condvar 待ち（native）。
- 「every task is blocked」判定（`sched.rs:1138-1143`）は他スレッドの `Running` スロット数
  も数える。
- ワーカー上の `Progress::Done(Err)`（`sched.rs:1183`）は `failure` に置き、`drive_main` が
  次のロック取得時に見て返す。
- `switch_every_step`（テスト用）は main だけの単一スレッド動作を保つ。

**AOT**（`rt_run_entry_driven` / `crates/typelisp-rt/src/lib.rs` の `rt_heap_init` 周辺）:
`Scheduler<CompiledTask>` を `Arc<SchedShared>` に、ワーカーを起動してから main タスクを
`drive_main`。main が終わればプロセス終了（Go の規則、既存どおり）。

### typl（AOT が完成してから）

`Interp` は `Rc`/`RefCell`、front の `Task` は `Rc<dyn CompiledBody>` を含む。方針は
「main 以外のスレッドは `Interp` に触らない。触る必要が出た瞬間にタスクを main へ移送する」。
**compiled な `go` もワーカーへ出し、`NeedsMain` 移送を作る**（ユーザ決定）。

- スケジューラの本体を `enum TyplBody { Compiled(CompiledTask), Interp(MainOnly<Task>) }` に。
  `MainOnly<T>` は `unsafe impl Send`、取り出しは main だけ（affinity `Main` を admit 時に付け、
  `next_ready` が唯一の配り手。**根拠は `next_ready` の 1 点だけ**——`Main` affinity のスロット
  を main 以外に渡さないという assert）。front の `Rc` を `Arc` に総取り替えしない。
  `TaskBody::step` の `cx` は `Option<&Interp>`（`Compiled` は無視、`Interp` に `None` が
  来ることは affinity が禁じる）。
- **admit で閉包の種類を見る**: compiled closure（`heap.is_compiled_closure`）→
  `Compiled(CompiledTask)`、affinity `Any`（ワーカー）。それ以外 → `Interp(Task)`、`Main`。
  typl もワーカーを起動する（`TYPELISP_THREADS`）。
- **`NeedsMain` 移送**: ワーカー上の `CompiledTask` が `Interp` の要る瞬間——interpreted
  closure の apply（C5 の `Paused::Applying`、`APPLY_INTERPRETED` はワーカーに無い）、`:dyn`
  の interpreted impl（`DYN_SLOT_CLOSURE`）、`compile`/`eval`——に `Progress::NeedsMain` を
  返す。スケジューラはロック下で `Interp(MainOnly(Task::adopt_chain(compiled_task)))` に
  組み替え、affinity を `Main` にして `ready_main` へ。**一度移送したら main に固定のまま**
  （戻さない）。移送後の長い処理の途中で 1 度 interpreted に触ると以後 main で走ることを文書に
  書く。`adopt_chain` はロック下で行うので、ヒープ確保をしない（`DriveCtx` の合成は Rust
  値のみ）。`Task`/`CompiledTask` とも状態スロットは `sbase`/`sbase+1`（+ §5 の `sbase+2`）で
  互換（`core_cps.rs:544-563`, `sched.rs:1566-1577`）、`Paused::Applying{closure,args}` は
  i64 で Send、`begin_applying`（`core_cps.rs:1230`）には
  `DriveCtx{start: {Closure,[Fn],Sexpr,None}, roots_on_entry: sbase+2, crossing_roots: 0, base: 0}`
  を合成して載せる。
- **JIT 本体の寿命**: `CompiledTask` は `DriveCallee::Body(Rc<dyn CompiledBody>)` を持てない
  ので `Address` だけ持つ。`destroy_retired_llvm`（`src/compile/mod.rs:222-225`）は
  スケジューラに `Compiled` の非 Done スロットが 1 つでもある間は**延期**（`COMPILE_LOCK` 下で
  スケジューラをロックして数える。延期分は次の機会に破棄）。
- **detached print hooks**（typl の非 main 用）: `INTERP_PRINT_HOOKS`（`interp.rs:4637-4653`）
  の 6 項目を `PrintShared` から答える。`enum_variant_name`/`field_is_niched_option`
  （`defenum` 時に更新）、`opts`/`print_vars`（`setf` 時に更新）は素直に写せる。
  `print_object`/`format_call` は **type_key → compiled 本体アドレスの表**（`Interp` が impl
  をコンパイルしたときに登録。printer の door `run_to_end` から呼ぶのは AOT と同じ経路）で
  答え、表に無い型（impl が interpreted）は `rt_print` がシムで中断できないので
  **catchable panic**「print-object of X is not compiled; call it from main or
  `(compile ...)`」。AOT の hooks（`typelisp-print/src/aot.rs`）がこの表をどう持っているかを
  先に読んで同じ形に揃える。
- `thread` の本体は **推移的自動コンパイル**（`compile_function_rec`、既存。print-object
  impl も型が到達可能なら対象に含める）。不能なら「`thread` は compiled にできる関数にしか
  使えない」の言語エラー。
- REPL の行読みを native に（§4）。`step_task`（`core_cps.rs:1112` の
  `scheduler.borrow_mut()`）を lock（native）に。

## 9. `Thread<T>` — 直接 OS スレッド

`go` ↔ `Task<T>`/`wait` と対にする:

| 名前 | 使い方 | 型 | 意味 |
|---|---|---|---|
| `thread` | `(thread (f args...))` | 特殊形 → `Thread<T>` | 専用 OS スレッドを 1 本作り、その上で呼び出しを走らせる |
| `join` | `(join th)` | `(Thread<T>)→T` | 完了を待つ（呼んだ**タスク**が止まる。何度でも可、値はキャッシュ） |
| `Thread::available-parallelism` | `(Thread::available-parallelism)` | `()→int` | `std::thread::available_parallelism` |
| `Thread::current-id` | `(Thread::current-id)` | `()→int` | 走っている OS スレッドの id |

- 構文は `go` と同じ「呼び出し形」（`check_go` `checker.rs:11349` の SHAPE をそのまま）。
  `(fn () T)` から `T` が推論できれば prelude に
  `(pub defmethod spawn (Thread<T> (f (fn () T))) Thread<T> (thread (funcall f)))` を足して
  `(Thread::spawn (lambda () ...))` も書けるようにする（着手時に推論可否を先に確かめる）。
- 実装は「専用 OS スレッドに pin されたタスク」: `check_go` に `kind: Go|Thread` を足し、
  `forms::go_form`（`check/forms.rs:224` 付近）と `core_bridge::translate_go`
  （`src/compile/core_bridge.rs:938-1018`）の thunk/tag 生成は共用、suspend だけ
  `rt_suspend_thread`（`coroutine.rs:621` の `rt_suspend_go` の隣）→
  `Waiting::SpawnThread(closure)` → `Scheduler::admit_dedicated`: `std::thread::spawn` で
  スレッドを起こし、`Heap::attach` + `RtShared`/hooks の複製 → そのスレッドは
  `drive_dedicated(key)` で自分のタスクだけを回し、ブロックしたら Condvar で待つ（native）。
  タスクが `Done` になったらスロットに値を残してスレッドは終わる。
- `join` は `wait`（`Waiting::Task`、`sched.rs:638`）そのもの。ランタイム表現も
  `Task<T>` と同じ「スケジューラ id を持つ箱」（`sched.rs:283-310`）で、型だけ別。
- 専用スレッドの中の `go` は共有キューへ（typl では系譜規則で `Any`）。専用スレッド上のタスク
  panic の規則は goroutine と同じ（`failure_left_task`）。main 終了でプロセス終了（他スレッドも）。

## 10. 既知のリスク

1. **`Slab` 化のコスト**: `box_slots[id]` の index 計算が (chunk, offset) になる。
   chunk サイズを 2 の冪にしてシフト/マスクで済ませる。外側の chunk 表は固定長配列。
2. **`switch_to_root_stack` の規則**（`heap.rs:503-506`「Rust フレームが root index を
   持っているときは呼べない」）はそのまま。ビューが生ポインタを持っても契約は変わらない。
3. **native 忘れ**: 1 箇所でも漏れると GC 要求で全員止まる。watchdog（§4）と `in_native`
   の debug panic を最初に入れてから native 区間を指定していく。
4. **poller と self-pipe**: `wake_io` の反映で slot の再検証を忘れると、再利用された slot
   の別タスクを起こす。
5. **`gc_stress` × 複数スレッド**: 毎 cons で STW になるので極端に遅いが、ルート漏れ検出器
   としては最強。テストの規模を小さく保つ。
6. **既存の `switch_every_step` テスト**: main 専用の単一スレッド経路で意味を保つ。
7. **島の再生成**: `TaskBody` は Rust 側なので島は無関係だが、extern の追加で
   `externs.rs` の表と島のハッシュが変わる → regen 2 回（`typelisp-island-regen-fixpoint`）。
   SOURCE 無変更でも成果物は変わる。

## 11. 検証

- 各フェーズ末: `scripts/test-serial.sh`（zsh の変数分割に注意）。`cargo check` の警告 0 を
  「完了」の根拠にしない（dead code 警告の数を数える）。
- STW 導入後: 新テストは `TYPELISP_THREADS=1` と `=4` の両方で走らせ、答えが同じことを見る。
- スケジューラ多スレッド化後: `examples/`・`projects/` の並行サンプル（`go` を使うもの）を
  AOT で、typl 対応後は typl でも実行し、出力が単一スレッド時と一致。
- 時間は計測しない（`feedback-do-not-measure-runtime`）。並列性は thread id の集合で観測する。
