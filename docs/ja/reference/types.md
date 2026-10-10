# 型の一覧

typelisp にある型と、各型が実装している標準トレイトの一覧。型の書き方は
[構文リファレンス 2 章](syntax.md#2-型の書き方)、各型の関数・メソッドは [組み込み関数](functions/README.md)。

## 1. プリミティブ型

| 型 | 内容 | 詳細 |
|---|---|---|
| `int` | 任意精度の整数。63bit に入るあいだは即値、超えると自動で多倍長になる。未注釈の整数リテラルの既定の型 | [数値 3 章](functions/numbers.md#3-任意精度整数-int) |
| `i8` `i16` `i32` | 符号付き固定幅整数 | [数値 1 章](functions/numbers.md#1-固定幅整数) |
| `u8` `u16` `u32` | 符号なし固定幅整数 | 同上 |
| `f32` `f64` | IEEE-754 浮動小数点数。小数リテラルの既定は `f64` | [数値 4 章](functions/numbers.md#4-浮動小数点数f64--f32) |
| `ratio` | 既約な有理数 | [数値 5 章](functions/numbers.md#5-有理数-ratio) |
| `bool` | `true` / `false` | [数値 7 章](functions/numbers.md#7-真偽値) |
| `char` | Unicode スカラ値 | [文字](functions/collections.md#2-文字-char) |
| `string` | 不変の文字列 | [文字列](functions/collections.md#1-文字列-string) |
| `symbol` | シンボル。キーワード（`:name`）もこの型 | [シンボル](functions/sequences.md#3-シンボル) |
| `()` | Unit 型。値も `()` | |
| `!` | Never 型。`panic` など、戻らない式の型。どの型の位置にも置ける | |
| `ptr` `c-long` `c-ulong` | C との受け渡し専用の語。`unsafe` の中でだけ値にでき、置ける場所も限られる | [数値 2 章](functions/numbers.md#2-c-境界の生の語ptr--c-long--c-ulong) |
| `random-state` | 乱数生成器の状態 | [数値 12 章](functions/numbers.md#12-乱数) |

64bit 幅の整数型は無い。幅を気にしない整数は `int` を使う。

## 2. 組み込みのジェネリック型

| 型 | 内容 | 詳細 |
|---|---|---|
| `Option<T>` | 値があるか無いか。`some` / `none` | [Option と Result](functions/option-result.md) |
| `Result<T,E>` | 成功か失敗か。`ok` / `err` | 同上 |
| `Vector<T>` | 可変長配列 | [Vector](functions/collections.md#3-vectort) |
| `HashTable<K,V>` | ハッシュ表。キーの型は `Hash` を実装していること | [HashTable](functions/collections.md#4-hashtablekv) |
| `#{T0 T1 ...}` | タプル（要素 1〜12 個）。要素は `t::0` で読む | [構文 2 章](syntax.md#2-型の書き方) |
| `Task<T>` | タスクのハンドル | [タスク](functions/concurrency.md#1-taskt--タスクのハンドル) |
| `Thread<T>` | 専用 OS スレッドで走るタスクのハンドル | [Thread](functions/concurrency.md#7-threadt--専用の-os-スレッド) |
| `Chan<T>` | チャネル | [チャネル](functions/concurrency.md#2-chant--チャネル) |

関数型は `(fn (引数型...) 戻り値型)`、トレイトオブジェクトは `:dyn Trait` と書く
（[構文リファレンス 2 章](syntax.md#2-型の書き方)）。

## 3. S 式データ

| 型 | 内容 | 詳細 |
|---|---|---|
| `Sexpr` | 空でない S 式。`int`・`i8`〜`u32`・`f32`・`f64`・`char`・`bool`・`sym`・`str`・`cons`・`ratio`・`path`・`vector`・`array`・`tuple` の 19 変種 | [S 式データ](functions/sequences.md#2-s-式データ-sexpr) |
| `Option<Sexpr>` | S 式データ一般。空リスト `()` は `none` | 同上 |

## 4. 標準ライブラリの型

標準ライブラリ（prelude）が `defstruct` / `defenum` で定義している型。自分で書いた型と同じ扱いで、
`defstruct` にできることは全部できる。

| 型 | 内容 | 詳細 |
|---|---|---|
| `cons-cell<A,B>` | ペア。`cons`/`car`/`cdr` | [ペア](functions/sequences.md#1-ペア-cons-cellab) |
| `complex` | 複素数（成分は `f64`） | [数値 6 章](functions/numbers.md#6-複素数-complex) |
| `Array<T>` | 多次元配列 | [Array](functions/collections.md#5-arrayt多次元配列) |
| `BitVector` | 固定長のビット列 | [BitVector](functions/collections.md#6-bitvectorビットベクタ) |
| `vector-iter<T>` `hashtable-iter<K,V>` `array-iter<T>` | 各コレクションの `iter` が返すイテレータ | [Iter](functions/traits.md#1-iter-トレイトと反復) |
| `WaitGroup` | N 個の完了待ち | [WaitGroup](functions/concurrency.md#4-waitgroup--n-個の完了待ち) |
| `Mutex<T>` | 共有データの排他 | [Mutex](functions/concurrency.md#6-mutext--共有データの排他) |
| `pathname` | 分解したファイル名 | [パス名](functions/streams-files.md#9-パス名-pathname) |
| `file-stream` `binary-file-stream` `string-input-stream` `string-output-stream` `standard-stream` | ストリーム | [ストリーム](functions/streams-files.md#3-具象ストリーム型) |
| `broadcast-stream` `two-way-stream` `echo-stream` `concatenated-stream` `peek-stream` | 合成ストリーム | [合成ストリーム](functions/streams-files.md#4-合成ストリーム) |
| `socket-stream` `socket-byte-stream` `socket-listener` `udp-socket` `datagram` | ネットワーク | [ネットワーク](functions/network.md#1-型) |
| `ReadOutcome` | `read-sexpr` の結果。`datum` / `eof` | [ストリーム](functions/streams-files.md#6-ジェネリック関数とファイル操作) |
| `universal-time` `internal-time` `decoded-time` | 時刻 | [時間](functions/system.md#1-時間) |
| `heap-info` | ヒープの現況 | [処理系の道具](functions/system.md#51-heap-info-の欄) |

## 5. エラー型

`Error` は型ではなくトレイトで、次の型がそれを実装している。種類を問わず扱うときは `:dyn Error` と書く。

| 型 | 生成元 |
|---|---|
| `ParseIntError` | `parse-int` |
| `ParseFloatError` | `parse-float` |
| `ReadError` | `read` / `read-from-string` / `read-sexpr` |
| `EvalError` | `eval` / `macroexpand` |
| `FileError` | ファイル・ストリーム操作 |
| `NetError` | ネットワーク操作 |
| `SimpleError` | `simple-error` |
| `WrappedError` | `wrap-error` |

詳細は [エラー型と Error トレイト](functions/option-result.md#3-エラー型と-error-トレイト)。

## 6. 標準トレイトの実装

どの型がどのトレイトを実装しているか。各トレイトのメソッドは [標準トレイト](functions/traits.md) と、
表の右端に挙げた章にある。

### 6.1 比較・ハッシュ・印字

| トレイト | 実装している型 | 詳細 |
|---|---|---|
| `Eq` | `int` `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `ratio` `complex` `bool` `char` `string` `symbol` `Sexpr` `cons-cell<A,B>` | [Eq / Ord](functions/traits.md#2-eq--ord比較) |
| `Ord` | `int` `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `ratio` `char` `string` `cons-cell<A,B>` | 同上 |
| `Hash` | `int` `i8` `i16` `i32` `u8` `u16` `u32` `bool` `char` `string` `symbol` | [HashTable](functions/collections.md#4-hashtablekv) |
| `print-object` | `int` `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `ratio` `complex` `bool` `char` `string` `symbol` `Array<T>` `pathname` `universal-time` `internal-time` 組み込みのエラー型すべて | [print-object](functions/printing.md#5-print-object型ごとの印字表現) |

`cons-cell<A,B>` の `Eq`/`Ord` は、要素の型が `Eq`/`Ord` を実装しているときに使える。

### 6.2 算術

| トレイト | 実装している型 |
|---|---|
| `Add` `Sub` `Mul` `Div` `Rem` `Number` | `int` `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `ratio` |
| `Bits` | `int` `i8` `i16` `i32` `u8` `u16` `u32` |

詳細は [算術トレイト](functions/traits.md#3-算術トレイトadd--sub--mul--div--rem--bits--number)。

### 6.3 反復

| トレイト | 実装している型 |
|---|---|
| `Iter` | `vector-iter<T>` `hashtable-iter<K,V>` `array-iter<T>` `Chan<T>` |

### 6.4 ストリーム

| 型 | 実装しているトレイト |
|---|---|
| `file-stream` | `CharInput` `PeekInput` `CharOutput` |
| `binary-file-stream` | `ByteInput` `ByteOutput` |
| `string-input-stream` | `CharInput` `PeekInput` |
| `string-output-stream` | `CharOutput` |
| `standard-stream` | `CharInput` `PeekInput` `CharOutput` |
| `socket-stream` | `CharInput` `PeekInput` `CharOutput` |
| `socket-byte-stream` | `ByteInput` `ByteOutput` |
| `broadcast-stream` | `CharOutput` |
| `two-way-stream` | `CharInput` `CharOutput` |
| `echo-stream` | `CharInput` |
| `concatenated-stream` | `CharInput` |
| `peek-stream` | `CharInput` `PeekInput` |

どのストリームも `Stream` を実装し、入力側は `InputStream`、出力側は `OutputStream` も実装する。
`socket-listener` と `udp-socket` は `Stream`（`close` / `open-stream-p`）だけを実装する。
詳細は [ストリーム](functions/streams-files.md#1-トレイト階層)。

### 6.5 その他

| トレイト | 実装している型 | 詳細 |
|---|---|---|
| `Error` | 5 章のエラー型すべて | [エラー型](functions/option-result.md#3-エラー型と-error-トレイト) |
| `Pathish` | `string` `pathname` | [パス名](functions/streams-files.md#91-パス名指定子トレイト-pathish) |
