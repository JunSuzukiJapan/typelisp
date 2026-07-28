// The manifest is the one part of an extension the compiler cannot check: a
// command declared in `package.json` but never registered shows up in the
// palette and does nothing, a setting read but never declared silently reads as
// undefined, and a contributed path that does not exist fails at load time with
// a message the user sees but the developer usually does not.
//
// These are cheap string comparisons, but they are the failures that survive a
// green `tsc` and a green test run.

import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import test from "node:test";

const EXT_ROOT = join(__dirname, "..", "..");

interface Manifest {
  main: string;
  contributes: {
    languages: Array<{ id: string; extensions: string[]; configuration: string }>;
    grammars: Array<{ language: string; scopeName: string; path: string }>;
    commands: Array<{ command: string; title: string }>;
    keybindings: Array<{ command: string; key: string; when?: string }>;
    configuration: { properties: Record<string, unknown> };
    problemMatchers: Array<{ name: string; pattern: { regexp: string } }>;
  };
}

const manifest: Manifest = JSON.parse(readFileSync(join(EXT_ROOT, "package.json"), "utf8"));
const extensionSource = readFileSync(join(EXT_ROOT, "src", "extension.ts"), "utf8");

function contributedPath(p: string): string {
  return join(EXT_ROOT, p.replace(/^\.\//, ""));
}

test("every contributed file exists", () => {
  for (const lang of manifest.contributes.languages) {
    assert.ok(existsSync(contributedPath(lang.configuration)), lang.configuration);
  }
  for (const grammar of manifest.contributes.grammars) {
    assert.ok(existsSync(contributedPath(grammar.path)), grammar.path);
  }
});

test("the grammar declares the scope the manifest points at, and covers .typl", () => {
  const grammar = JSON.parse(readFileSync(contributedPath("syntaxes/typelisp.tmLanguage.json"), "utf8"));
  assert.equal(grammar.scopeName, manifest.contributes.grammars[0].scopeName);
  assert.equal(manifest.contributes.grammars[0].language, manifest.contributes.languages[0].id);
  assert.deepEqual(manifest.contributes.languages[0].extensions, [".typl"]);
});

test("declared commands and registered commands are the same set", () => {
  const declared = new Set(manifest.contributes.commands.map((c) => c.command));
  const registered = new Set(
    [...extensionSource.matchAll(/registerCommand\("([^"]+)"/g)].map((m) => m[1]),
  );
  assert.deepEqual(
    [...declared].sort(),
    [...registered].sort(),
    "a command in the palette that does nothing, or a handler nobody can invoke",
  );
});

test("every keybinding names a declared command", () => {
  const declared = new Set(manifest.contributes.commands.map((c) => c.command));
  for (const kb of manifest.contributes.keybindings) {
    assert.ok(declared.has(kb.command), kb.command);
    assert.equal(kb.when, "editorLangId == typelisp", `${kb.command} should be scoped to typelisp`);
  }
});

test("every setting the code reads is declared", () => {
  const declared = new Set(Object.keys(manifest.contributes.configuration.properties));
  const read = [...extensionSource.matchAll(/\.get<[^>]+>\("([^"]+)"/g)].map(
    (m) => `typelisp.${m[1]}`,
  );
  for (const key of read) {
    assert.ok(declared.has(key), `read but not declared: ${key}`);
  }
});

test("the problem matcher parses the diagnostics typl actually prints", () => {
  const { regexp } = manifest.contributes.problemMatchers[0].pattern;
  const re = new RegExp(regexp);
  // Both a type error and a run-time panic carry a location; a redefinition
  // warning has none and must not match, since there is nothing to jump to.
  const withLocation = "error: /tmp/err.typl:2:8: type error: type mismatch: expected I32, found Str";
  const m = re.exec(withLocation);
  assert.ok(m, "a located diagnostic must match");
  assert.equal(m[2], "/tmp/err.typl");
  assert.equal(m[3], "2");
  assert.equal(m[4], "8");
  assert.ok(re.exec("error: examples/factorial.typl:1:17: panic: boom"), "panics carry a location");
  const warning = re.exec("warning: src/model.typl:9:3: something");
  assert.ok(warning && warning[1] === "warning", "the warning group drives severity");
  assert.equal(re.exec("warning: redefining function `f'"), null, "an unlocated warning must not match");
});

test("the compiled entry point the manifest names is what tsc emits", () => {
  assert.equal(manifest.main, "./out/extension.js");
  assert.ok(existsSync(contributedPath(manifest.main)), "run `npm run compile` first");
});
