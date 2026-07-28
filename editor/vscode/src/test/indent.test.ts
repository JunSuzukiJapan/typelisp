// The indenter is held to the same standard as the Emacs mode's: re-indenting
// every committed `.typl` file under `examples/` must reproduce it byte for
// byte. That corpus is the project's own idea of correct typelisp layout, and
// it exercises the cases the spec table exists for -- three-element `defun`
// headers, `if` with its mandatory `else`, `impl` members, `labels` local
// functions, `match` clauses, `cond`/`case`, nested `let`.

import assert from "node:assert/strict";
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join, relative } from "node:path";
import test from "node:test";

import { BODY_INDENT, INDENT_SPECS, computeIndent, indentText, scan } from "../indent";

const EXT_ROOT = join(__dirname, "..", "..");
const REPO_ROOT = join(EXT_ROOT, "..", "..");
const EXAMPLES = join(REPO_ROOT, "examples");

function typlFiles(dir: string): string[] {
  const out: string[] = [];
  for (const entry of readdirSync(dir)) {
    const full = join(dir, entry);
    if (statSync(full).isDirectory()) {
      out.push(...typlFiles(full));
    } else if (entry.endsWith(".typl")) {
      out.push(full);
    }
  }
  return out.sort();
}

test("every committed example re-indents to itself", () => {
  const files = typlFiles(EXAMPLES);
  assert.ok(files.length >= 20, `expected the example corpus, found ${files.length} files`);

  const changed: string[] = [];
  for (const file of files) {
    const original = readFileSync(file, "utf8");
    const reindented = indentText(original);
    if (reindented !== original) {
      changed.push(firstDifference(relative(REPO_ROOT, file), original, reindented));
    }
  }
  assert.deepEqual(changed, [], `re-indentation changed:\n${changed.join("\n")}`);
});

/** A one-line report of where two versions of a file first diverge. */
function firstDifference(name: string, a: string, b: string): string {
  const as = a.split("\n");
  const bs = b.split("\n");
  for (let i = 0; i < Math.max(as.length, bs.length); i++) {
    if (as[i] !== bs[i]) {
      return `  ${name}:${i + 1}\n    committed: ${JSON.stringify(as[i])}\n    indented:  ${JSON.stringify(bs[i])}`;
    }
  }
  return `  ${name}: differs outside line content`;
}

test("every example is restored after its indentation is flattened", () => {
  // The round-trip above could in principle pass by doing nothing at all, so
  // this is the load-bearing test: throw away every line's indentation and
  // require the indenter to reconstruct the committed file exactly. The Emacs
  // mode passes the same test on the same 22 files, which is what makes "the
  // two editors agree" a checked claim rather than an intention.
  //
  // Flattening is safe on this corpus specifically: it contains no multi-line
  // strings and no `;;;` comments, the two things whose indentation is content
  // rather than layout.
  const changed: string[] = [];
  for (const file of typlFiles(EXAMPLES)) {
    const original = readFileSync(file, "utf8");
    const restored = indentText(original.replace(/^[ \t]+/gm, ""));
    if (restored !== original) {
      changed.push(firstDifference(relative(REPO_ROOT, file), original, restored));
    }
  }
  assert.deepEqual(changed, [], `flattened files were not restored:\n${changed.join("\n")}`);
});

test("an already-correct file is not rewritten by a second pass", () => {
  for (const file of typlFiles(EXAMPLES)) {
    const once = indentText(readFileSync(file, "utf8"));
    assert.equal(indentText(once), once, `${relative(REPO_ROOT, file)} is not idempotent`);
  }
});

test("mangled indentation is restored", () => {
  const want = [
    "(defun f ((n i32)) i32",
    "  (let ((acc 0))",
    "    (dotimes (i n)",
    "      (setf acc (+ acc i)))",
    "    acc))",
  ].join("\n");
  // Every body line flattened to column 0, and to a wrong column.
  const flattened = want.replace(/^ +/gm, "");
  const skewed = want.replace(/^ +/gm, "         ");
  assert.equal(indentText(flattened), want);
  assert.equal(indentText(skewed), want);
});

test("if lines up both branches, unlike Emacs Lisp's spec of 2", () => {
  const src = ["(defun g ((x i32)) i32", "  (if (< x 0)", "      (- 0 x)", "      x))"].join("\n");
  assert.equal(indentText(src.replace(/^ +/gm, "")), src);
});

test("impl members and labels local functions indent as definitions", () => {
  const impl = [
    "(impl Shape rect",
    "  (label ((self Self)) string",
    '    (append "rect"',
    '            "!")))',
  ].join("\n");
  assert.equal(indentText(impl), impl);

  const labels = [
    "(defun h () i32",
    "  (labels",
    "      ((go ((n i32)) i32",
    "         (if (= n 0)",
    "             0",
    "             (go (- n 1)))))",
    "    (go 3)))",
  ].join("\n");
  assert.equal(indentText(labels), labels);
});

test("a let binding's value gets ordinary call alignment, not a body indent", () => {
  // The local-definition rule must not leak into `let`, whose bindings have the
  // very same `(name ...)` shape one level down as a `labels` local function.
  // The continuation of `(+ 1` therefore aligns under `+`'s first argument the
  // way any call does -- it does not become a definition body at column 2.
  // Verified against Emacs (`indent-region` in `typelisp-mode`).
  const src = ["(defun k () i32", "  (let ((a (+ 1", "              2)))", "    a))"].join("\n");
  assert.equal(indentText(src), src);
});

test("string and ;;; lines are left alone", () => {
  const s = scan('(defun f () string\n  "a\nb")\n');
  // Line 2 opens inside the string literal started on line 1.
  assert.equal(computeIndent(s, 2), null);

  const withHeader = ";;; a top-level remark\n(defun f () i32 1)\n";
  assert.equal(indentText(withHeader), withHeader);
  // Even indented, a `;;;` line keeps whatever column it is at.
  const indented = "      ;;; kept\n";
  assert.equal(indentText(indented), indented);
});

test("comments and char literals do not disturb structure", () => {
  const src = [
    "(defun f () ()",
    "  ;; a remark with an unbalanced ( paren",
    '  (println "~a" #\\()',
    "  #| a block #| nested |# comment with ) |#",
    "  (println \"~a\" #\\;))",
  ].join("\n");
  assert.equal(indentText(src), src);
});

test("lines inside a block comment are re-indented, as Emacs does", () => {
  // Not obviously desirable in itself -- a formatter reflowing prose is a bit
  // rude -- but the two editors must not disagree, and Emacs's `indent-region`
  // indents them because `#| ... |#` is comment syntax rather than a string, so
  // the enclosing form is still what determines the column. Verified against
  // Emacs directly.
  const src = ["(defun f () i32", "  #| a block", "  second line", "  third |#", "  1)"].join("\n");
  assert.equal(indentText(src), src);
  assert.equal(indentText(src.replace(/^ +/gm, "")), src);
});

test("quoted forms carry their prefix into the alignment column", () => {
  const src = ["(defun f () Sexpr", "  (append '(1 2)", "          '(3 4)))"].join("\n");
  assert.equal(indentText(src), src);
});

test("every Emacs reference case is reproduced byte for byte", () => {
  // The strongest form of "faithful port" available without running Emacs at
  // test time: the expected side of each case was produced by `indent-region`
  // in a real `typelisp-mode` buffer, so a divergence here means the two
  // editors would actually lay the same file out differently.
  // Read from `src/`, not next to the compiled test: `tsc` copies only
  // TypeScript, so pointing at `out/` would need a build step that is easy to
  // forget and whose absence looks like a missing fixture.
  const fixture = readFileSync(
    join(EXT_ROOT, "src", "test", "fixtures", "emacs-indent-reference.txt"),
    "utf8",
  );
  const cases = parseFixture(fixture);
  assert.ok(cases.length >= 15, `expected the reference cases, found ${cases.length}`);
  for (const { name, input, expected } of cases) {
    assert.equal(indentText(input), expected, name);
    // And Emacs's own output must be a fixed point, as it is for the corpus.
    assert.equal(indentText(expected), expected, `${name} (idempotent)`);
  }
});

/** Parse the `=== name` / `--- input` / `--- expected` fixture format. */
function parseFixture(text: string): Array<{ name: string; input: string; expected: string }> {
  const out: Array<{ name: string; input: string; expected: string }> = [];
  // Drop the leading `#` header, then split on the case marker.
  for (const block of text.split(/^=== /m).slice(1)) {
    const lines = block.split("\n");
    const name = lines[0].trim();
    const inputAt = lines.indexOf("--- input");
    const expectedAt = lines.indexOf("--- expected");
    assert.ok(inputAt >= 0 && expectedAt > inputAt, `malformed fixture case: ${name}`);
    // Each section runs to the next marker; a trailing blank line separates
    // cases and is not part of the text.
    const input = lines.slice(inputAt + 1, expectedAt).join("\n");
    const rest = lines.slice(expectedAt + 1);
    while (rest.length > 0 && rest[rest.length - 1] === "") {
      rest.pop();
    }
    out.push({ name, input, expected: rest.join("\n") });
  }
  return out;
}

test("the spec table matches the Emacs mode's", () => {
  // A guard on the port itself: these are the values `typelisp-indent-specs`
  // holds, and the two must not drift apart.
  assert.equal(BODY_INDENT, 2);
  assert.equal(INDENT_SPECS.get("defun"), "defun");
  assert.equal(INDENT_SPECS.get("pub"), "defun");
  assert.equal(INDENT_SPECS.get("lambda"), "defun");
  assert.equal(INDENT_SPECS.get("impl"), "defun");
  assert.equal(INDENT_SPECS.get("if"), 3);
  assert.equal(INDENT_SPECS.get("if-let"), 3);
  assert.equal(INDENT_SPECS.get("cond"), 0);
  assert.equal(INDENT_SPECS.get("loop"), 0);
  assert.equal(INDENT_SPECS.get("match"), 1);
  assert.equal(INDENT_SPECS.get("do"), 2);
  assert.equal(INDENT_SPECS.get("pprint-logical-block"), 1);
  // Not a special form: an ordinary call aligns under its first argument.
  assert.equal(INDENT_SPECS.get("println"), undefined);
});
