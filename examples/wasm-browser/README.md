# showcase: typelisp in a browser

`showcase.typl` runs one section per feature: numbers (bignums, ratios,
floats), strings, lists, vectors and hash tables, structs and enums with
`match`, traits and `:dyn`, macros, `Result`, `catch`/`throw`,
`unwind-protect` and `block`, tasks, channels, `select` and `sleep`, `eval`,
and the host's clock, standard error and exit code. It prints the same
natively, under Node and in a browser (only the time differs).

`showcase.typl` は機能ごとに 1 節ずつ実行します。数（多倍長整数・分数・浮動小数点数）、
文字列、リスト・ベクタ・ハッシュテーブル、構造体と列挙型と `match`、トレイトと `:dyn`、
マクロ、`Result`、`catch`/`throw`・`unwind-protect`・`block`、タスク・チャネル・`select`・
`sleep`、`eval`、ホストの時計・標準エラー・終了コードです。ネイティブ、Node、ブラウザで
同じ出力になります（時刻だけが違います）。

## Build / ビルド

In this folder, with a `typl` that can compile to WebAssembly:

WebAssembly 向けにコンパイルできる `typl` で、このフォルダで次を実行します。

```sh
typl -c showcase.typl --target wasm32-unknown-unknown   # showcase.wasm and showcase.mjs
```

## Run in a browser / ブラウザで実行する

```sh
node serve.mjs          # http://127.0.0.1:8000/
```

Open http://127.0.0.1:8000/. The page runs the module in a Worker
(`worker.mjs`), because `sleep` waits with `Atomics.wait`, which a browser
allows only off its main thread and only on a cross-origin isolated page;
`serve.mjs` sends the two headers that make the page so. Any other server
works if it sends them too:

http://127.0.0.1:8000/ を開きます。`sleep` は `Atomics.wait` で待ち、ブラウザはこれを
クロスオリジン分離されたページのメインスレッド以外でしか許さないので、ページはモジュールを
Worker（`worker.mjs`）で実行します。`serve.mjs` はページをクロスオリジン分離にする次の
2 つのヘッダを付けます。ほかのサーバでも、このヘッダを付ければ動きます。

```
Cross-Origin-Opener-Policy: same-origin
Cross-Origin-Embedder-Policy: require-corp
```

## Run under Node / Node で実行する

```sh
node showcase.mjs; echo "exit code $?"   # exit code 3
```
