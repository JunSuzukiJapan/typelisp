#!/usr/bin/env python3
"""typl-lsp のエラー回復モードを stdio 越しに実測するスモークテスト。

チェッカーのエラー回復モード（docs/dev/implementation-log.md「チェッカーのエラー回復モード」）
が、実際の LSP トランスポート越しに次の3点を満たすことを確認する:

  1. 型エラーが複数あるファイルで、全エラーが同時に publishDiagnostics される。
  2. catchall でない `match` アーム本体内で補完すると、ローカル束縛
     （パターン束縛・パラメータ）が候補に出る（本改修の動機バグ）。
  3. 型エラーのあるファイルでも hover / goto-definition が機能する（部分木から解決）。

前提: `cargo build --bin typl-lsp` 済み。既定でその debug バイナリを起動する。
使い方:
    python3 scripts/lsp-recover-smoke.py
    python3 scripts/lsp-recover-smoke.py --bin target/release/typl-lsp

終了コード 0 = 全チェック PASS。"""

import argparse
import json
import subprocess
import sys

# --- LSP over stdio: Content-Length 枠つき JSON-RPC の最小実装 --------------


def _write(proc, msg):
    body = json.dumps(msg).encode("utf-8")
    header = f"Content-Length: {len(body)}\r\n\r\n".encode("ascii")
    proc.stdin.write(header + body)
    proc.stdin.flush()


def _read(proc):
    """1メッセージ読む。ヘッダを解釈して本文を返す。EOF なら None。"""
    headers = {}
    while True:
        line = proc.stdout.readline()
        if not line:
            return None
        line = line.decode("ascii").strip()
        if line == "":
            break
        k, _, v = line.partition(":")
        headers[k.strip().lower()] = v.strip()
    n = int(headers["content-length"])
    return json.loads(proc.stdout.read(n).decode("utf-8"))


def _request(proc, next_id, method, params):
    _write(proc, {"jsonrpc": "2.0", "id": next_id, "method": method, "params": params})


def _notify(proc, method, params):
    _write(proc, {"jsonrpc": "2.0", "method": method, "params": params})


def _await_response(proc, want_id):
    """指定 id のレスポンスが来るまで読み進め、その間の通知を貯めて一緒に返す。"""
    notifications = []
    while True:
        msg = _read(proc)
        if msg is None:
            raise RuntimeError("server closed the connection unexpectedly")
        if msg.get("id") == want_id and ("result" in msg or "error" in msg):
            return msg, notifications
        if "method" in msg and "id" not in msg:
            notifications.append(msg)


def _drain_notifications(proc, method, timeout_msgs=10):
    """method 一致の通知が来るまで最大 timeout_msgs 件読む。"""
    for _ in range(timeout_msgs):
        msg = _read(proc)
        if msg is None:
            break
        if msg.get("method") == method:
            return msg
    return None


# --- シナリオ本体 ----------------------------------------------------------

URI = "file:///tmp/typl-lsp-recover-smoke/scenario.typl"

# 1行に収めて列位置を予測可能にする。非網羅 match（Some のみ、catchall なし）で、
# かつ未定義変数 `undefined_global` も置いて「複数エラー」を作る。
DOC = '(defvar (bad i32) true)\n(defun f ((o Option<i32>)) i32 (match o ((Some v) v)))\n'

results = []


def check(name, ok, detail=""):
    results.append((name, ok, detail))
    mark = "PASS" if ok else "FAIL"
    print(f"[{mark}] {name}" + (f" — {detail}" if detail else ""))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--bin", default="target/debug/typl-lsp")
    args = ap.parse_args()

    proc = subprocess.Popen(
        [args.bin], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL
    )
    try:
        # initialize / initialized
        _request(proc, 1, "initialize", {"processId": None, "rootUri": None, "capabilities": {}})
        _await_response(proc, 1)
        _notify(proc, "initialized", {})

        # didOpen → publishDiagnostics を受け取る
        _notify(
            proc,
            "textDocument/didOpen",
            {"textDocument": {"uri": URI, "languageId": "typelisp", "version": 1, "text": DOC}},
        )
        diag = _drain_notifications(proc, "textDocument/publishDiagnostics")
        diags = diag["params"]["diagnostics"] if diag else []

        # (1) 複数診断: 非網羅 match と defvar 型不一致の両方が出ているはず。
        check(
            "multiple diagnostics reported at once",
            len(diags) >= 2,
            f"{len(diags)} diagnostic(s): " + "; ".join(d["message"][:50] for d in diags),
        )

        # (2) 非 catchall match アーム内の補完でローカルが出る。
        # 行1（0-based）の `((Some v) v` の body `v` の直前（空白の直後）に
        # カーソルを置く = 識別子をまだ打ち始めていない状態。こうすると
        # handle_completion の prefix フィルタが空になり、`v` 始まり以外の
        # ローカル（`o`）も候補に残る。handle_completion は truncate +
        # placeholder `(panic "")` を挿入して recover モードで再チェックする。
        line1 = DOC.split("\n")[1]
        body_v = line1.rindex("v")  # `(match o ((Some v) v)))` の末尾側（本体）の v
        completion_char = body_v  # v の直前にカーソル → prefix は空
        _request(
            proc,
            2,
            "textDocument/completion",
            {"textDocument": {"uri": URI}, "position": {"line": 1, "character": completion_char}},
        )
        resp, _ = _await_response(proc, 2)
        result = resp.get("result") or []
        items = result["items"] if isinstance(result, dict) else result
        labels = {it["label"] for it in items}
        check(
            "completion inside non-catchall match arm offers locals `o` and `v`",
            {"o", "v"}.issubset(labels),
            "locals present" if {"o", "v"}.issubset(labels) else f"got labels sample: {sorted(labels)[:15]}",
        )

        # (3) 型エラーのあるファイルでも hover が効く（部分木から解決）。
        # 行1 の `o` パラメータ参照（`(match o` の o）位置。
        o_char = line1.index("(match o ") + len("(match o ") - 2  # `o` の桁
        _request(
            proc,
            3,
            "textDocument/hover",
            {"textDocument": {"uri": URI}, "position": {"line": 1, "character": o_char}},
        )
        resp, _ = _await_response(proc, 3)
        hover = resp.get("result")
        check(
            "hover works despite type errors in the file",
            hover is not None,
            "hover returned a type" if hover else "hover was null",
        )

        # shutdown / exit
        _request(proc, 99, "shutdown", None)
        _await_response(proc, 99)
        _notify(proc, "exit", None)
    finally:
        proc.stdin.close()
        proc.wait(timeout=5)

    failed = [n for n, ok, _ in results if not ok]
    print()
    if failed:
        print(f"RESULT: {len(failed)} check(s) FAILED: {failed}")
        sys.exit(1)
    print(f"RESULT: all {len(results)} checks PASSED")


if __name__ == "__main__":
    main()
