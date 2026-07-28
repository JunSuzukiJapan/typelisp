"""Drive the real `typl-lsp` over stdio and print the semantic tokens it returns.

A companion to `lsp-recover-smoke.py`: `tests/lsp_semantic_test.rs` covers the
name resolution and the encoding at the library level, but only an actual
process exercises the capability handshake and the JSON-RPC wire format.

The document it opens (`todo-cli`'s `store.typl`) defines no types of its own --
`todo-item` reaches it through `(use model::todo-item)` -- so a non-empty result
is exactly the cross-file case an editor's own grammar cannot resolve.

    python3 scripts/lsp-semantic-smoke.py

Expects `target/debug/typl-lsp` to be built.
"""
import json, subprocess, sys, os

# The checkout this script lives in, so it runs from anywhere and on any
# machine (`scripts/` sits directly under the repository root).
ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
FILE = f"{ROOT}/examples/projects/todo-cli/src/store.typl"
text = open(FILE, encoding="utf-8").read()

p = subprocess.Popen([f"{ROOT}/target/debug/typl-lsp"],
                     stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)

def send(msg):
    b = json.dumps(msg).encode()
    p.stdin.write(f"Content-Length: {len(b)}\r\n\r\n".encode() + b)
    p.stdin.flush()

def read():
    n = None
    while True:
        line = p.stdout.readline()
        if not line: return None
        line = line.strip()
        if not line: break
        if line.lower().startswith(b"content-length:"):
            n = int(line.split(b":")[1])
    return json.loads(p.stdout.read(n))

send({"jsonrpc":"2.0","id":1,"method":"initialize",
      "params":{"processId":os.getpid(),"rootUri":f"file://{ROOT}","capabilities":{}}})
init = read()
caps = init["result"]["capabilities"]
st = caps.get("semanticTokensProvider")
print("semanticTokensProvider:", json.dumps(st))

send({"jsonrpc":"2.0","method":"initialized","params":{}})
send({"jsonrpc":"2.0","method":"textDocument/didOpen",
      "params":{"textDocument":{"uri":f"file://{FILE}","languageId":"typelisp","version":1,"text":text}}})

send({"jsonrpc":"2.0","id":2,"method":"textDocument/semanticTokens/full",
      "params":{"textDocument":{"uri":f"file://{FILE}"}}})

# skip diagnostics notifications until our response arrives
while True:
    m = read()
    if m is None:
        print("no response"); sys.exit(1)
    if m.get("id") == 2:
        break

data = m["result"]["data"]
print(f"tokens: {len(data)//5}")
lines = text.split("\n")
legend = st["legend"]["tokenTypes"]
line = col = 0
for i in range(0, len(data), 5):
    dl, dc, ln, tt, _ = data[i:i+5]
    line += dl
    col = col + dc if dl == 0 else dc
    print(f"  L{line+1} c{col} {lines[line][col:col+ln]!r} -> {legend[tt]}")
send({"jsonrpc":"2.0","id":99,"method":"shutdown","params":None}); read()
send({"jsonrpc":"2.0","method":"exit","params":None})
p.wait(timeout=5)
