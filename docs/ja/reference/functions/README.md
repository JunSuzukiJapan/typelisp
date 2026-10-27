# 組み込み関数

組み込み関数・メソッド・標準ライブラリの一覧。構文（特殊形・定義方法）は
[構文リファレンス](../syntax.md)、型の一覧は [型の一覧](../types.md) を参照。

## 呼び出し形式

呼び出し形式は3種類ある。

- 自由関数: `(name args...)`
- インスタンスメソッド: `(name receiver args...)`（第一引数の静的型から解決される）
- 静的メソッド（関連関数）: `(Type::name args...)`

同じ名前のメソッドが型ごとにあってよい。`(+ a b)` は `a` の型の `+` を呼ぶ。

## `use` が要る名前

標準ライブラリの名前のうち、何も書かずにどのモジュールからでも使えるのは次のもの
（[構文リファレンス 3.10](../syntax.md#310-module--use--名前空間)）。

- 基本型、`Option`・`Result`・`Vector`、反復、S 式とリスト、列の関数、制御のマクロ、比較、算術のトレイト
- Common Lisp の `COMMON-LISP` パッケージにある名前と同じ名前のもの（`length`・`format`・`open` など）
- `Error`・`assert`・`print-object`、`HashTable`・`Array`・`BitVector`、ストリームのトレイト
  `Stream`・`InputStream`・`OutputStream`、`HashTable` のキーが実装する `Hash`
- これらの関数・変数の型に現れる型

それ以外の名前は、`(use std::net::tcp-connect)` のように `use` で取り込むか、`std::net::tcp-connect`
とパスで書く。次の表がその名前と置き場所。

| モジュール | 名前 |
|---|---|
| `std::array` | `array-iter` |
| `std::char` | `digit->char`、`digit-weight`、`name->char` |
| `std::collections` | `Deque`、`HashSet`、`Scope`、`SortedTable`、`hashtable-iter` |
| `std::env` | `command-line-args`、`getenv`、`home-directory` |
| `std::error` | `WrappedError`、`as-dyn-error`、`assert-eq`、`describe-error`、`wrap-error` |
| `std::fmt` | `pprint-list-exhausted` |
| `std::fs` | `direction-append`、`direction-input`、`direction-output`、`directory-p`、`open-binary`、`open-binary-input`、`open-binary-output`、`open-file`、`open-input`、`open-output`、`read-file-lines`、`read-file-string`、`write-file-string` |
| `std::hash` | `*sxhash-mask*`、`sxhash-string` |
| `std::io` | `ByteInput`、`ByteOutput`、`PeekInput`、`binary-file-stream`、`copy-stream`、`make-peek-stream`、`peek-stream`、`read-lines`、`write-lines` |
| `std::net` | `NetError`、`byte-stream-of`、`char-stream-of`、`datagram`、`socket-byte-stream`、`socket-listener`、`socket-stream`、`tcp-connect`、`tcp-listen`、`tls-connect`、`tls-listen`、`udp-bind`、`udp-socket`、`unix-connect`、`unix-listen`、`with-connection` |
| `std::num` | `ParseFloatError`、`ParseIntError`、`atan2`、`parse-float`、`parse-int` |
| `std::path` | `Pathish`、`pathname-absolute-p` |
| `std::process` | `exit` |
| `std::random` | `seed-random-state` |
| `std::reader` | `ReadOutcome`、`read-from-string-preserving-whitespace`、`read-sexpr`、`read-sexpr-preserving-whitespace` |
| `std::repl` | `heap-info` |
| `std::string` | `left-trim`、`right-trim`、`string->utf8`、`trim`、`utf8->string` |
| `std::symbol` | `symbol->string` |
| `std::sync` | `Chan`、`Context`、`Mutex`、`WaitGroup`、`after`、`with-lock` |
| `std::thread` | `Task`、`Thread`、`yield` |
| `std::time` | `internal-time-seconds`、`timezone-daylight-p`、`timezone-offset-seconds` |

## 表の読み方

各章の表は「名前・形式・型・説明」の列を持つ。型の列は `(引数の型,...)→戻り値の型` の形で書く。

- `T` `A` `B` などの 1 文字の大文字は型変数。
- `where Eq A` のような注記は、その型変数が満たすべきトレイト境界。
- `Iter<A>` は「`Item` が `A` の任意の `Iter` 実装型」。
- `&optional` / `&key` の付いた引数は省略できる。

## 章

| ファイル | 内容 |
|---|---|
| [numbers.md](numbers.md) | 整数・浮動小数点数・有理数・複素数・真偽値、ビット演算、乱数 |
| [sequences.md](sequences.md) | ペア `cons-cell`、S 式データ `Sexpr`、シンボル、シーケンス関数、遅延イテレータ `lazy`、高階関数 |
| [collections.md](collections.md) | 文字列、文字、`Vector`、`HashTable`、`Array`、`BitVector`、`HashSet`、`SortedTable`、`Deque` |
| [option-result.md](option-result.md) | `Option`、`Result`、エラー型と `Error` トレイト |
| [traits.md](traits.md) | `Iter`、`Eq`/`Ord`、算術トレイト |
| [printing.md](printing.md) | `print`/`println`/`format`、pretty printer、`print-object`、印字の制御変数 |
| [format.md](format.md) | 書式ディレクティブ |
| [streams-files.md](streams-files.md) | ストリーム、ファイル操作、パス名、readtable |
| [concurrency.md](concurrency.md) | タスク、チャネル、`WaitGroup`、`Mutex`、`Thread`、`Context` |
| [network.md](network.md) | TCP、TLS、Unix ドメインソケット、UDP |
| [system.md](system.md) | 時間、実行環境、処理系の道具、`read`/`eval`、docstring、マクロまわり |
