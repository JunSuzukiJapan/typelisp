# ランタイムを 3 つのクレートに分ける計画

2026-11-23 ユーザー決定。ブランチ `feature/cross-compile-embedded`。[cross-compile-embedded-plan.md](cross-compile-embedded-plan.md) の段階 1 の後に行い、手順 5（組み込みのランタイムのクレートと拒否の表）はこの分割の上に作る。

## 目的

段階 1 の手順 4 では、`typelisp-rt` の std が要る部分を `#[cfg(feature = "std")]` の下に置いた（約 190 か所）。この形には次の問題がある。

- 境界をコンパイラが見張らない。std の無いターゲットで使えないものを `cfg` を付けずに書いても、std の既定の構成では通る。
- 組み込みと wasm で使えない shim の表を手で書き、別のテストで実際の記号と突き合わせる必要がある。

そこで、ランタイムを機能の有無で 3 つのクレートに分ける。

| クレート | 中身 | 使うターゲット |
|---|---|---|
| `typelisp-rt`（B） | 全ターゲットにあるもの | 全て |
| `typelisp-rt-sys`（C1） | WASI にもあるもの: ファイル、ファイルの操作、環境変数、コマンド行引数、ホームディレクトリ、dribble | ネイティブ、wasm32-wasip1 |
| `typelisp-rt-posix`（C2） | WASI に無いもの: ソケット、TLS、名前解決、OS スレッド、descriptor の見張り、ホスト名、OS の版、CPU 時間、ファイルの持ち主、エディタの起動、共有ライブラリの読み込み | ネイティブ |

C1 と C2 は B に依存し、B は C1 と C2 を知らない。C2 は C1 に依存しない。

## 規則

- **B から OS に尋ねて、無いときに決まった答えを返すもの**（標準入力の読み取り、端末かどうか、ローカルのタイムゾーン、CPU の名前、並列度）は `typelisp_mem::host` に置く。ターゲットごとの実装の違いは host の中の `cfg` で書く（標準出力と同じ）。
- **ターゲットによっては無い機能**は C1 か C2 に置く。B には、C1 と C2 が中身を入れる口だけを作る。
- B の `cfg(feature = "std")` は、依存先の `std` を有効にすることと、大域アロケータの選択だけになる。

## B に作る口

### ストリーム

`Backend` の変種は、標準入力、標準出力、標準エラー、文字列の入出力、閉じたもの、と `Ext(Box<dyn ExtBackend>)` だけにする。`ExtBackend` は B のトレイトで、文字・バイトの読み書き、`finish-output`、`close`、印字用の説明、`listen`、見張れる descriptor を持つ。ファイルは C1、ソケット・リスナー・UDP・名前解決は C2 が実装する。C2 は自分の型へ `Any` で戻して触る（TLS の接続を箱に入れたのと同じ形）。

### ソケットの待ち

`Waiting::Io` は B に残し、descriptor を見張る手続き（`Poller`）への参照を持たせる。参照は descriptor を持つバックエンドが渡す（`ExtBackend` が descriptor と一緒に返す）。見張っている間のスレッドを起こす口（今の `SelfPipe`）も `Poller` が作る。C2 だけが `Poller` を実装し、C2 のバックエンドを作らないプログラムでは `Waiting::Io` は生まれない。

### OS スレッド

スケジューラの仕組み（Crew、ワーカーの開始と終了、`thread` の固定されたタスク）は B に残す。C2 は OS スレッドを始めて待つ部品だけを渡す。

- B: `SchedShared` に、スレッドを始める手続きと Crew を入れる口を作る。終わりを待つ相手は `Box<dyn Joiner>`。口が空なら、待ちは今の wasm と同じく 1 本のスレッドの待ち（期限があれば眠る、無ければ fatal）になる。
- C2: `ThreadKit`、`start_workers`、`install_crew`、`thread_count`（`TYPELISP_THREADS`）。
- 実行ファイルの入口: ネイティブでは C2 の `rt_run_program_posix`（と `_int`）が、ワーカーを始める手続きを B の `run_program` に渡す。どちらを呼ぶかは `aot.rs` がターゲットで決める。

## 組み込み関数の表

`src/compile/externs.rs` の `rt_extern_functions` を、クレートごとの表に分ける。各クレートが自分の shim の名前と番地と「何の機能か」を返す関数を持つ（`extern_functions()`）。どの shim がどのクレートのものかは、その表から読む。

断るものはクレートから決まる。

| ターゲット | 断る shim |
|---|---|
| wasm32-wasip1 | C2 |
| wasm32-unknown-unknown | C1、C2 |
| 組み込み | C1、C2、`eval` の shim（`typelisp-front`） |

今の `WASM_UNAVAILABLE_SHIMS`、`WASM_UNAVAILABLE_PREFIX`、`NO_FILE_SYSTEM_SHIMS`、`NO_FILE_SYSTEM_PREFIX` は無くなる。wasm32-unknown-unknown では、今は断っていない環境変数・コマンド行引数・dribble の shim も断るようになる。

## 進め方

各段で、std の既定の構成の全体テスト、警告 0、組み込みと wasm 2 種の `cargo check` が通ること。

1. host への移動: 標準入力の読み取り、端末かどうか、ローカルのタイムゾーン、CPU の名前、並列度。
2. ストリームの `ExtBackend`。ファイルとソケットを箱に入れる（まだ `typelisp-rt` の中）。
3. スケジューラの口: `Poller` と、スレッドの部品の口。
4. `typelisp-rt-sys` を作り、C1 の中身を移す。
5. `typelisp-rt-posix` を作り、C2 の中身を移す。テスト（`crates/typelisp-rt/tests` のスレッドとソケットのもの）も移す。
6. 組み込み関数の表をクレートごとにし、拒否をクレートから決める。
7. 組み込みのランタイムのクレート（cross-compile-embedded-plan.md の段階 1 の手順 5）。

## 結果

手順 1〜7 を終えた。

- `typelisp-rt` に残る std の cfg は、大域アロケータの選択と `no_std` の宣言だけ。libc・rustls・webpki-roots に依存しない。
- C1 は `typelisp-rt-sys`、C2 は `typelisp-rt-posix`。wasm の `typelisp-front` は C2 に依存せず、`eval` からの呼び出しは Panic で断る。
- shim の表は `src/compile/externs.rs` で、全ターゲットが持つもの（`rt_shims`）と C1・C2 のもの（`RuntimeCrate::shims`、届いた経路の説明つき）に分かれる。ターゲットが持たないクレートは `Target::missing_runtimes` が答え、そのクレートの shim に届くプログラムはコンパイル時に断る。
- 表を分けると `rt_*` の宣言の順が変わるので、島と prelude を再生成した。
- 組み込みのランタイムは `crates/typelisp-rt-embedded`（`staticlib`、公開しない）。`target_os = "none"` のときだけ中身があり、B の 5 つのクレートを束ねて、パニックハンドラと、使われない巻き戻しの入口（`rust_eh_personality`、`__aeabi_unwind_cpp_pr0`）を持つ。未定義で残るのはボードの関数（`typelisp_host_*`）だけ。`scripts/build-embedded-runtime.sh` がビルドし、全体テストのスクリプトと CI が毎回ビルドまたは検査する。
- 組み込みで断る shim（C1、C2、`eval`）の表は、`Target` に組み込みのターゲットが加わる段階 2 で作る。
