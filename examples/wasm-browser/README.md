# showcase: typelisp in a browser

`showcase.typl` runs one section per feature: numbers (bignums, ratios,
floats), strings, lists, vectors and hash tables, structs and enums with
`match`, traits and `:dyn`, macros, `Result`, `catch`/`throw`,
`unwind-protect` and `block`, tasks, channels, `select` and `sleep`, `eval`,
and the host's clock and time zone, standard error and exit code. It prints
the same natively, under Node and in a browser (only the time differs).

`showcase.typl` は機能ごとに 1 節ずつ実行します。数（多倍長整数・分数・浮動小数点数）、
文字列、リスト・ベクタ・ハッシュテーブル、構造体と列挙型と `match`、トレイトと `:dyn`、
マクロ、`Result`、`catch`/`throw`・`unwind-protect`・`block`、タスク・チャネル・`select`・
`sleep`、`eval`、ホストの時計と時間帯・標準エラー・終了コードです。ネイティブ、Node、
ブラウザで同じ出力になります（時刻だけが違います）。

## Build / ビルド

This needs a `typl` that can compile to WebAssembly. To install one from the
repository, run these at the repository's root (rustup is needed, for the
WebAssembly targets and, when compiling, for their linker):

WebAssembly 向けにコンパイルできる `typl` が要ります。リポジトリから入れるには、
リポジトリの一番上で次を実行します（WebAssembly の target と、コンパイルするときの
リンカのために rustup が要ります）。

```sh
rustup target add wasm32-wasip1 wasm32-unknown-unknown
cargo install --locked --path . --features wasm
```

Then, in this folder, the following writes `showcase.wasm` and `showcase.mjs`:

そのあと、このフォルダで次を実行すると `showcase.wasm` と `showcase.mjs` ができます。

```sh
typl -c showcase.typl --target wasm32-unknown-unknown
```

## Run in a browser / ブラウザで実行する

```sh
node serve.mjs
```

Open http://127.0.0.1:8000/ (`node serve.mjs 8080` serves another port). The page runs the module in a Worker
(`worker.mjs`), because `sleep` waits with `Atomics.wait`, which a browser
allows only off its main thread and only on a cross-origin isolated page;
`serve.mjs` sends the two headers that make the page so. Any other server
works if it sends them too:

http://127.0.0.1:8000/ を開きます（`node serve.mjs 8080` なら別のポートで配信します）。`sleep` は `Atomics.wait` で待ち、ブラウザはこれを
クロスオリジン分離されたページのメインスレッド以外でしか許さないので、ページはモジュールを
Worker（`worker.mjs`）で実行します。`serve.mjs` はページをクロスオリジン分離にする次の
2 つのヘッダを付けます。ほかのサーバでも、このヘッダを付ければ動きます。

```
Cross-Origin-Opener-Policy: same-origin
Cross-Origin-Embedder-Policy: require-corp
```

## Run under Node / Node で実行する

It ends with exit code 3. / 終了コード 3 で終わります。

```sh
node showcase.mjs
echo "exit code $?"
```
