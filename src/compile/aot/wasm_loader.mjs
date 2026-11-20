// Runs the WebAssembly module typl compiled beside this file
// (`typl -c --target wasm32-unknown-unknown`), giving it what it imports from
// `typelisp`: standard output and error, exit, the time and sleeping.
//
// From the command line, under Node:
//
//   node prog.mjs
//
// exits with the program's exit code. From other JavaScript, in Node or a
// browser:
//
//   import { run } from './prog.mjs';
//   const code = await run({ write: (fd, text) => ... });
//
// `write` receives each piece of output as text, `fd` 1 for standard output
// and 2 for standard error; without it, Node writes to its own and a browser
// to the console, a line at a time. `sleep` waits with `Atomics.wait` on a
// `SharedArrayBuffer`, which a browser allows only off its main thread and on a
// cross-origin isolated page: a program that sleeps runs in a Worker there.

const MODULE = '__TYPELISP_MODULE__';

const isNode = typeof process !== 'undefined' && process.versions != null && process.versions.node != null;

class Exit {
  constructor(code) {
    this.code = code;
  }
}

async function moduleBytes() {
  const url = new URL(MODULE, import.meta.url);
  if (isNode) {
    const { readFile } = await import('node:fs/promises');
    return readFile(url);
  }
  const response = await fetch(url);
  if (!response.ok) {
    throw new Error(`${url}: ${response.status} ${response.statusText}`);
  }
  return response.arrayBuffer();
}

async function defaultWrite() {
  if (isNode) {
    const { writeSync } = await import('node:fs');
    return (fd, _text, bytes) => {
      let off = 0;
      while (off < bytes.length) {
        off += writeSync(fd, bytes, off);
      }
    };
  }
  const pending = { 1: '', 2: '' };
  const show = (fd, line) => (fd === 2 ? console.error : console.log)(line);
  const write = (fd, text) => {
    const lines = (pending[fd] + text).split('\n');
    pending[fd] = lines.pop();
    for (const line of lines) {
      show(fd, line);
    }
  };
  // What is left without a newline when the program ends.
  write.end = () => {
    for (const fd of [1, 2]) {
      if (pending[fd] !== '') {
        show(fd, pending[fd]);
        pending[fd] = '';
      }
    }
  };
  return write;
}

export async function run(options = {}) {
  const write = options.write ?? (await defaultWrite());
  const decoder = new TextDecoder();
  let sleeper = null;
  let memory = null;
  const imports = {
    typelisp: {
      write(fd, ptr, len) {
        const bytes = new Uint8Array(memory.buffer, ptr, len).slice();
        write(fd, decoder.decode(bytes), bytes);
      },
      exit(code) {
        throw new Exit(code);
      },
      now() {
        return performance.now();
      },
      unix_time() {
        return Date.now();
      },
      sleep(ms) {
        sleeper ??= new Int32Array(new SharedArrayBuffer(4));
        Atomics.wait(sleeper, 0, 0, ms);
      },
    },
  };
  const { instance } = await WebAssembly.instantiate(await moduleBytes(), imports);
  memory = instance.exports.memory;
  try {
    return instance.exports.typelisp_main();
  } catch (e) {
    if (e instanceof Exit) {
      return e.code;
    }
    throw e;
  } finally {
    write.end?.();
  }
}

if (isNode) {
  const { fileURLToPath } = await import('node:url');
  const { realpathSync } = await import('node:fs');
  const self = realpathSync(fileURLToPath(import.meta.url));
  if (process.argv[1] != null && realpathSync(process.argv[1]) === self) {
    process.exitCode = await run();
  }
}
