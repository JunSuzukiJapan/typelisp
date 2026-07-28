// The TextMate grammar is run through the same engine VS Code uses
// (`vscode-textmate` + `vscode-oniguruma`), so these assertions are about what
// a user actually sees, not about the regexes in the abstract.
//
// The cases mirror the ones the Emacs mode is checked against, because the two
// are meant to agree: same keyword tables, same decisions about what counts as
// a type, a builtin, or a literal.

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import test, { before } from "node:test";

import * as oniguruma from "vscode-oniguruma";
import * as textmate from "vscode-textmate";

const EXT_ROOT = join(__dirname, "..", "..");
const GRAMMAR = join(EXT_ROOT, "syntaxes", "typelisp.tmLanguage.json");
const SCOPE = "source.typelisp";

let grammar: textmate.IGrammar;

before(async () => {
  const wasm = readFileSync(
    join(EXT_ROOT, "node_modules", "vscode-oniguruma", "release", "onig.wasm"),
  );
  await oniguruma.loadWASM(wasm.buffer as ArrayBuffer);
  const registry = new textmate.Registry({
    onigLib: Promise.resolve({
      createOnigScanner: (sources) => new oniguruma.OnigScanner(sources),
      createOnigString: (str) => new oniguruma.OnigString(str),
    }),
    loadGrammar: async (scope) =>
      scope === SCOPE
        ? (textmate.parseRawGrammar(readFileSync(GRAMMAR, "utf8"), GRAMMAR) as textmate.IRawGrammar)
        : null,
  });
  const loaded = await registry.loadGrammar(SCOPE);
  assert.ok(loaded, "grammar failed to load");
  grammar = loaded;
});

/** Tokenize `source` and return `[text, innermost-scope]` for every non-blank token. */
function tokenize(source: string): Array<[string, string]> {
  const out: Array<[string, string]> = [];
  let state = textmate.INITIAL;
  for (const line of source.split("\n")) {
    const result = grammar.tokenizeLine(line, state);
    for (const token of result.tokens) {
      const text = line.slice(token.startIndex, token.endIndex);
      if (text.trim() === "") {
        continue;
      }
      out.push([text, token.scopes[token.scopes.length - 1]]);
    }
    state = result.ruleStack;
  }
  return out;
}

/** The innermost scope the grammar gives the first token exactly equal to `text`. */
function scopeOf(source: string, text: string): string | undefined {
  return tokenize(source).find(([t]) => t === text)?.[1];
}

test("definition forms name a function, a type, or a variable", () => {
  const src = [
    "(defun factorial ((n i32)) bignum 1)",
    "(pub defun g ((x i32)) i32 x)",
    "(defstruct point<T> (x T))",
    "(deftrait Shape (label ((self Self)) string))",
    "(pub defvar (*flag* bool) false)",
  ].join("\n");
  assert.equal(scopeOf(src, "defun"), "storage.type.function.typelisp");
  assert.equal(scopeOf(src, "factorial"), "entity.name.function.typelisp");
  // The `pub` marker must not stop the name from being recognized.
  assert.equal(scopeOf(src, "g"), "entity.name.function.typelisp");
  assert.equal(scopeOf(src, "pub"), "storage.modifier.typelisp");
  // A type-defining form names a type, not a function -- and the generic header
  // is one token.
  assert.equal(scopeOf(src, "point<T>"), "entity.name.type.typelisp");
  assert.equal(scopeOf(src, "Shape"), "entity.name.type.typelisp");
  assert.equal(scopeOf(src, "*flag*"), "variable.other.definition.typelisp");
});

test("impl names both the trait and the type, even lowercase ones", () => {
  // `print-object` is a prelude trait whose name is not capitalized, so the
  // Capitalized-name rule cannot reach it.
  const src = "(impl print-object point\n  (print-object ((self Self) (escape bool)) string \"p\"))";
  const tokens = tokenize(src);
  assert.equal(tokens[1][0], "impl");
  assert.equal(tokens[1][1], "keyword.control.impl.typelisp");
  assert.equal(tokens[2][1], "entity.name.type.typelisp", "trait name");
  assert.equal(tokens[3][1], "entity.name.type.typelisp", "receiver type");
  // The method of the same name, inside the block, reads as the builtin it is.
  const method = tokens.filter(([t]) => t === "print-object")[1];
  assert.equal(method[1], "support.function.typelisp");
});

test("primitive and builtin types, including bignum/ratio and the error types", () => {
  const src = "(defun f ((a bignum) (b ratio)) Result<i32,ParseIntError> (todo))";
  assert.equal(scopeOf(src, "bignum"), "support.type.typelisp");
  assert.equal(scopeOf(src, "ratio"), "support.type.typelisp");
  assert.equal(scopeOf(src, "Result"), "support.type.typelisp");
  assert.equal(scopeOf(src, "i32"), "support.type.typelisp");
  assert.equal(scopeOf(src, "ParseIntError"), "support.type.typelisp");
});

test("a type name inside a longer function name is not highlighted as a type", () => {
  // `int->bignum` is one builtin; the `bignum` inside it must not read as the
  // type. This is what the type rule's leading-`>` exclusion is for.
  const tokens = tokenize("(int->bignum 1)");
  const whole = tokens.find(([t]) => t === "int->bignum");
  assert.ok(whole, `expected one token, got ${JSON.stringify(tokens)}`);
  assert.equal(whole[1], "support.function.typelisp");
});

test(":dyn is a keyword in every type position, including inside generics", () => {
  const src = "(defvar (shapes Vector<:dyn Drawable>) (Vector::new))\n(defun a ((d :dyn Error)) string (message d))";
  const dyns = tokenize(src).filter(([t]) => t === ":dyn");
  assert.equal(dyns.length, 2);
  for (const [, scope] of dyns) {
    assert.equal(scope, "keyword.other.dyn.typelisp");
  }
  assert.equal(scopeOf(src, "Drawable"), "entity.name.type.typelisp");
  assert.equal(scopeOf(src, "Error"), "entity.name.type.typelisp");
});

test("special forms, declarations, clause keywords and lambda-list markers", () => {
  const src = [
    "(module m)",
    "(use tree)",
    "(load \"x\")",
    "(defmacro k (a &optional b &rest c) `(cond ((< a 0) -1) (else (as i64 a))))",
    "(println \"~a\" x)",
    "(pprint-logical-block (o :prefix \"[\") (pprint-newline :fill))",
  ].join("\n");
  assert.equal(scopeOf(src, "module"), "keyword.control.declaration.typelisp");
  assert.equal(scopeOf(src, "use"), "keyword.control.declaration.typelisp");
  assert.equal(scopeOf(src, "load"), "keyword.control.declaration.typelisp");
  assert.equal(scopeOf(src, "&optional"), "storage.modifier.lambda-list.typelisp");
  assert.equal(scopeOf(src, "&rest"), "storage.modifier.lambda-list.typelisp");
  assert.equal(scopeOf(src, "cond"), "keyword.control.typelisp");
  assert.equal(scopeOf(src, "else"), "keyword.control.conditional.typelisp");
  assert.equal(scopeOf(src, "as"), "keyword.control.typelisp");
  assert.equal(scopeOf(src, "println"), "keyword.control.typelisp");
  assert.equal(scopeOf(src, "pprint-logical-block"), "keyword.control.typelisp");
  assert.equal(scopeOf(src, "pprint-newline"), "support.function.typelisp");
  assert.equal(scopeOf(src, ":prefix"), "constant.other.symbol.typelisp");
});

test("numeric literals in every form the reader accepts", () => {
  const src = "(list 42 0xff 1.5 3.0e10 1/3 -7)";
  for (const lit of ["42", "0xff", "1.5", "3.0e10", "1/3", "-7"]) {
    assert.equal(scopeOf(src, lit), "constant.numeric.typelisp", lit);
  }
  // A digit run inside a type name is not a number.
  assert.equal(scopeOf("(the f64 x)", "f64"), "support.type.typelisp");
  // A bare `-` is the operator, not a sign with nothing after it.
  assert.notEqual(scopeOf("(- 1 2)", "-"), "constant.numeric.typelisp");
});

test("format directives are highlighted inside the control string", () => {
  const tokens = tokenize('(println "~5,\'0d ~{~a~^, ~} ~s ~~ ~%" xs)');
  const directives = tokens
    .filter(([, s]) => s === "constant.other.placeholder.typelisp")
    .map(([t]) => t);
  assert.deepEqual(directives, ["~5,'0d", "~{", "~a", "~^", "~}", "~s", "~~", "~%"]);
  // Ordinary string content around them stays string-scoped.
  assert.ok(tokens.some(([, s]) => s === "string.quoted.double.typelisp"));
});

test("comments, nested block comments and character literals", () => {
  const src = [
    "; a line comment with an unbalanced ( paren",
    "#| a block #| nested |# comment |#",
    '(println "~a" #\\Space #\\( #\\; )',
  ].join("\n");
  const tokens = tokenize(src);
  assert.ok(
    tokens.some(([t, s]) => t.includes("unbalanced") && s.startsWith("comment.line")),
    "line comment",
  );
  // The nesting must close on the *outer* `|#`, not the inner one: if it closed
  // early, `comment |#` would tokenize as code.
  assert.ok(
    tokens.some(([t, s]) => t.includes("comment") && s.startsWith("comment.block")),
    "block comment survives nesting",
  );
  const chars = tokens.filter(([, s]) => s === "constant.character.typelisp").map(([t]) => t);
  assert.deepEqual(chars, ["#\\Space", "#\\(", "#\\;"]);
});

test("earmuffed globals and the Never type", () => {
  assert.equal(
    scopeOf("(setf *print-pretty* true)", "*print-pretty*"),
    "variable.other.global.typelisp",
  );
  assert.equal(scopeOf("(setf *print-pretty* true)", "true"), "constant.language.typelisp");
  assert.equal(scopeOf("(defun bye () ! (exit 0))", "!"), "support.type.never.typelisp");
});

test("every example file tokenizes without falling back to plain source", () => {
  // A guard against a grammar that silently matches nothing: each example must
  // produce a healthy share of scoped tokens.
  const examples = join(EXT_ROOT, "..", "..", "examples");
  const files = ["bst.typl", "primes.typl", "game_of_life.typl"].map((f) => join(examples, f));
  for (const file of files) {
    const tokens = tokenize(readFileSync(file, "utf8"));
    const scoped = tokens.filter(([, s]) => s !== SCOPE).length;
    assert.ok(
      scoped > tokens.length / 3,
      `${file}: only ${scoped}/${tokens.length} tokens were scoped`,
    );
  }
});
