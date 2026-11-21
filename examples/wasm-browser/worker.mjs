// Runs showcase.wasm in a Worker: `sleep` waits with `Atomics.wait`, which a
// browser allows off its main thread only. Each piece of output, and then the
// exit code, goes to the page as a message.
// showcase.wasm を Worker で実行する。`sleep` は `Atomics.wait` で待つが、
// ブラウザはこれをメインスレッドの外でしか許さない。出力の断片と、最後に
// 終了コードを、メッセージでページへ送る。
import { run } from './showcase.mjs';

try {
  const code = await run({ write: (fd, text) => postMessage({ fd, text }) });
  postMessage({ exit: code });
} catch (e) {
  postMessage({ error: String(e) });
}
