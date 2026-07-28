// The Outline view's contents. `typl-lsp` does not implement
// `textDocument/documentSymbol`, so this scanner is the only thing standing
// between a `.typl` file and an empty Outline -- and it is checked against the
// real example corpus rather than only hand-written snippets.

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import test from "node:test";

import { Definition, findDefinitions, findTypeReferences } from "../symbols";

const REPO_ROOT = join(__dirname, "..", "..", "..", "..");

function names(defs: Definition[]): string[] {
  return defs.map((d) => `${d.kind}:${d.name}${d.isPublic ? " (pub)" : ""}`);
}

test("each definition form is found, with or without pub", () => {
  const src = [
    "(defun f ((x i32)) i32 x)",
    "(pub defun g () i32 1)",
    "(defmethod area ((self Self)) i32 1)",
    "(defmacro twice (e) `(progn ,e ,e))",
    "(defstruct point<T> (x T) (y T))",
    "(pub defenum color (red) (blue))",
    "(deftrait Shape (label ((self Self)) string))",
    "(impl Shape point (label ((self Self)) string \"p\"))",
    "(defvar (*flag* bool) false)",
    "(pub defconstant (limit i32) 10)",
    "(module geo)",
  ].join("\n");
  assert.deepEqual(names(findDefinitions(src)), [
    "function:f",
    "function:g (pub)",
    "method:area",
    "macro:twice",
    "struct:point<T>",
    "enum:color (pub)",
    "trait:Shape",
    "impl:Shape point",
    "variable:*flag*",
    "constant:limit (pub)",
    "module:geo",
  ]);
});

test("definitions are returned in source order", () => {
  const src = "(defstruct a (x i32))\n(defun b () i32 1)\n(deftrait C (m ((self Self)) i32))";
  const defs = findDefinitions(src);
  assert.deepEqual(defs.map((d) => d.name), ["a", "b", "C"]);
  for (let i = 1; i < defs.length; i++) {
    assert.ok(defs[i].start > defs[i - 1].start, "starts must increase");
  }
});

test("a definition's range spans the whole form and its name range is the name", () => {
  const src = "(defun f ((x i32)) i32\n  (+ x 1))\n";
  const [def] = findDefinitions(src);
  assert.equal(src.slice(def.start, def.end), "(defun f ((x i32)) i32\n  (+ x 1))");
  assert.equal(src.slice(def.nameStart, def.nameEnd), "f");
});

test("an unbalanced form still gets a range, so a mid-edit file has an outline", () => {
  const src = "(defun f ((x i32)) i32\n  (+ x 1";
  const [def] = findDefinitions(src);
  assert.equal(def.name, "f");
  assert.equal(def.end, src.length);
});

test("definitions mentioned in comments and strings are not indexed", () => {
  const src = [
    ";; (defun ghost-line () i32 1)",
    "#| (defun ghost-block () i32 1) |#",
    '(defvar (note string) "(defun ghost-string () i32 1)")',
    "(defun real () i32 1)",
  ].join("\n");
  assert.deepEqual(names(findDefinitions(src)), ["variable:note", "function:real"]);
});

test("a char literal cannot open a phantom string or comment", () => {
  // `#\"` and `#\;` must not be read as a quote or a comment start, or the
  // definition after them would vanish from the outline.
  const src = '(defun q () char #\\")\n(defun s () char #\\;)\n(defun after () i32 1)';
  assert.deepEqual(names(findDefinitions(src)), ["function:q", "function:s", "function:after"]);
});

test("the example corpus produces the definitions its source actually contains", () => {
  const file = join(REPO_ROOT, "examples", "projects", "shape-canvas", "src", "shapes.typl");
  const defs = findDefinitions(readFileSync(file, "utf8"));
  const kinds = new Set(defs.map((d) => d.kind));
  assert.ok(kinds.has("struct"), "expected defstruct definitions");
  assert.ok(kinds.has("trait"), "expected a deftrait");
  assert.ok(kinds.has("impl"), "expected impl blocks");
  assert.ok(kinds.has("function"), "expected defuns");
  // Every name must actually appear at its recorded offset. `impl` records both
  // of its names, so compare against the whitespace-normalized slice.
  const text = readFileSync(file, "utf8");
  for (const def of defs) {
    assert.equal(text.slice(def.nameStart, def.nameEnd).replace(/\s+/g, " "), def.name, def.name);
  }
});

test("the scanner finds exactly the definitions a line-oriented count does", () => {
  // A cross-check by an independent method. The corpus writes every top-level
  // definition at column 0, so counting heads at line starts is an honest
  // second opinion on the regex scanner -- and requiring exact equality catches
  // both a missed definition and a phantom one.
  const heads =
    /^\((?:pub )?(?:defun|defmethod|defmacro|defstruct|defenum|deftrait|impl|defvar|defconstant|module)\b/gm;
  for (const rel of [
    "examples/bst.typl",
    "examples/game_of_life.typl",
    "examples/projects/shape-canvas/src/shapes.typl",
    "examples/projects/expr-eval/src/parser.typl",
  ]) {
    const text = readFileSync(join(REPO_ROOT, rel), "utf8");
    const expected = text.match(heads)?.length ?? 0;
    assert.equal(findDefinitions(text).length, expected, rel);
  }
});

// ---------------------------------------------------------------- type uses

test("uses of a lowercase user type are found, not just its definition", () => {
  const src = [
    "(defstruct rect (x i32))",
    "(defun area ((r rect)) i32 r::x)",
    "(defun mk () rect (rect::new 1))",
  ].join("\n");
  const refs = findTypeReferences(src);
  assert.deepEqual(refs.map((r) => r.name), ["rect", "rect", "rect", "rect"]);
  for (const r of refs) {
    assert.equal(r.kind, "struct");
    assert.equal(src.slice(r.start, r.end), "rect");
  }
});

test("a type used as a generic argument is found", () => {
  const src = [
    "(pub defenum token (num i32))",
    "(defvar (ts Vector<token>) (Vector::new))",
    "(defvar (m HashTable<i32,token>) (HashTable::new))",
  ].join("\n");
  // Once at the definition, then inside each generic argument list -- the `<`
  // and `,` before them must not block the match.
  assert.equal(findTypeReferences(src).length, 3);
  assert.ok(findTypeReferences(src).every((r) => r.kind === "enum"));
});

test("a name that merely contains a type name is not a use", () => {
  const src = [
    "(defstruct rect (x i32))",
    ";; rect mentioned in a comment",
    '(println "rect in a string")',
    "(defun rectangle () i32 1)",
    "(defun my-rect () i32 2)",
    "(defun int->rect () i32 3)",
  ].join("\n");
  // Only the definition itself.
  const refs = findTypeReferences(src);
  assert.equal(refs.length, 1);
  assert.equal(refs[0].start, src.indexOf("rect"));
});

test("traits and enums get their own kinds", () => {
  const src = [
    "(deftrait shape (label ((self Self)) string))",
    "(defenum color (red))",
    "(defstruct box (w i32))",
    "(defun f ((s :dyn shape) (c color) (b box)) () ())",
  ].join("\n");
  const byName = new Map(findTypeReferences(src).map((r) => [r.name, r.kind]));
  assert.equal(byName.get("shape"), "trait");
  assert.equal(byName.get("color"), "enum");
  assert.equal(byName.get("box"), "struct");
});

test("a generic header declares the bare name", () => {
  const src = "(defstruct pair<A,B> (fst A) (snd B))\n(defun f ((p pair<i32,i32>)) i32 1)";
  const refs = findTypeReferences(src);
  assert.deepEqual(refs.map((r) => r.name), ["pair", "pair"]);
});

test("a file that defines no types yields nothing", () => {
  assert.deepEqual(findTypeReferences("(defun f () i32 1)"), []);
});

test("the real corpus resolves its own type annotations", () => {
  const file = join(REPO_ROOT, "examples", "projects", "shape-canvas", "src", "shapes.typl");
  const text = readFileSync(file, "utf8");
  const refs = findTypeReferences(text);
  const names = new Set(refs.map((r) => r.name));
  // The three shapes are defined here and used as `the`-pattern heads and in
  // `impl` blocks; before semantic tokens none of those uses were coloured.
  for (const expected of ["rect", "circle", "hline"]) {
    assert.ok(names.has(expected), `${expected} not found`);
    assert.ok(
      refs.filter((r) => r.name === expected).length > 1,
      `${expected} found only at its definition`,
    );
  }
  // Every reported span must really be that name.
  for (const r of refs) {
    assert.equal(text.slice(r.start, r.end), r.name);
  }
});
