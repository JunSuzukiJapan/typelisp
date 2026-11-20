// Runs a WASI (preview 1) command module under Node's `node:wasi`:
//
//   node --no-warnings scripts/run-wasi.mjs MODULE.wasm [ARGS...]
//
// What the wasm32-wasip1 tests run what `typl -c --target wasm32-wasip1`
// builds with. The module sees ARGS after its own name, the environment, and
// the file system through one preopened directory, `/`, so a path it is given
// has to be absolute. (Preopening `.` as well makes WASI's C library resolve
// absolute paths against it too, and they are not found.) Exits with the
// module's exit code.
import { readFile } from 'node:fs/promises';
import { WASI } from 'node:wasi';
import { argv, env, exit } from 'node:process';

if (argv.length < 3) {
  console.error('usage: node scripts/run-wasi.mjs MODULE.wasm [ARGS...]');
  exit(2);
}
const wasi = new WASI({
  version: 'preview1',
  args: argv.slice(2),
  env,
  preopens: { '/': '/' },
  returnOnExit: true,
});
const module = await WebAssembly.compile(await readFile(argv[2]));
const instance = await WebAssembly.instantiate(module, wasi.getImportObject());
exit(wasi.start(instance));
