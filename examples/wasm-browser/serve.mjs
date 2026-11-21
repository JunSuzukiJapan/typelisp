// Serves this folder at http://127.0.0.1:8000/ (or the port given), with the
// two headers that make the page cross-origin isolated, which a page needs
// before its Worker can use `SharedArrayBuffer` and `Atomics.wait`.
// このフォルダを http://127.0.0.1:8000/（または指定したポート）で配信する。
// ページをクロスオリジン分離にする 2 つのヘッダを付ける。これがないと、Worker で
// `SharedArrayBuffer` と `Atomics.wait` が使えない。
//
//   node serve.mjs [PORT]
import { createServer } from 'node:http';
import { readFile } from 'node:fs/promises';
import { extname, join, normalize, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import { argv } from 'node:process';

const root = fileURLToPath(new URL('.', import.meta.url));
const port = Number(argv[2] ?? 8000);
const types = {
  '.html': 'text/html; charset=utf-8',
  '.mjs': 'text/javascript; charset=utf-8',
  '.wasm': 'application/wasm',
};

createServer(async (req, res) => {
  const path = decodeURIComponent(new URL(req.url, 'http://localhost').pathname);
  const file = normalize(join(root, path === '/' ? 'index.html' : path));
  const type = types[extname(file)];
  if (!file.startsWith(root.endsWith(sep) ? root : root + sep) || type === undefined) {
    res.writeHead(404).end();
    return;
  }
  try {
    const body = await readFile(file);
    res.writeHead(200, {
      'Content-Type': type,
      'Cross-Origin-Opener-Policy': 'same-origin',
      'Cross-Origin-Embedder-Policy': 'require-corp',
    });
    res.end(body);
  } catch {
    res.writeHead(404).end();
  }
}).listen(port, '127.0.0.1', () => {
  console.log(`http://127.0.0.1:${port}/`);
});
