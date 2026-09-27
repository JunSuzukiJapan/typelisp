# 組み込み関数

組み込み関数・メソッド・標準ライブラリの一覧。構文（特殊形・定義方法）は
[構文リファレンス](../syntax.md)、型の一覧は [型の一覧](../types.md) を参照。

## 呼び出し形式

呼び出し形式は3種類ある。

- 自由関数: `(name args...)`
- インスタンスメソッド: `(name receiver args...)`（第一引数の静的型から解決される）
- 静的メソッド（関連関数）: `(Type::name args...)`

同じ名前のメソッドが型ごとにあってよい。`(+ a b)` は `a` の型の `+` を呼ぶ。

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
| [sequences.md](sequences.md) | ペア `cons-cell`、S 式データ `Sexpr`、シンボル、シーケンス関数、高階関数 |
| [collections.md](collections.md) | 文字列、文字、`Vector`、`HashTable`、`Array`、`BitVector` |
| [option-result.md](option-result.md) | `Option`、`Result`、エラー型と `Error` トレイト |
| [traits.md](traits.md) | `Iter`、`Eq`/`Ord`、算術トレイト |
| [printing.md](printing.md) | `print`/`println`/`format`、pretty printer、`print-object`、印字の制御変数 |
| [format.md](format.md) | 書式ディレクティブ |
| [streams-files.md](streams-files.md) | ストリーム、ファイル操作、パス名、readtable |
| [concurrency.md](concurrency.md) | タスク、チャネル、`WaitGroup`、`Mutex`、`Thread` |
| [network.md](network.md) | TCP、TLS、Unix ドメインソケット、UDP |
| [system.md](system.md) | 時間、実行環境、処理系の道具、`read`/`eval`、docstring、マクロまわり |
