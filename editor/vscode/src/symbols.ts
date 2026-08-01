// Document symbols (the Outline view, breadcrumbs, and Ctrl+Shift+O).
//
// `typl-lsp` implements diagnostics, hover, go-to-definition and completion,
// but not `textDocument/documentSymbol`, so without this the Outline view for a
// `.typl` file would simply be empty. Extracting definitions is cheap and needs
// no type information, so the extension does it locally -- the same division of
// labour as the Emacs mode, where `imenu` reads the buffer directly rather than
// asking the server.
//
// The patterns mirror `typelisp-imenu-generic-expression`. As there, matching
// the head is enough because every definition form names its subject in a fixed
// position right after the head, modulo the optional `pub` marker.
//
// Nothing here imports `vscode`, so it is unit-testable under `node --test`;
// `extension.ts` maps the results onto `vscode.DocumentSymbol`.

/** Every character that can appear inside a typelisp symbol. */
const SYMBOL = "[A-Za-z0-9_?!*<>=/+\\-.%&^~:]+";

/**
 * What kind of thing a definition introduces. Deliberately close to the groups
 * the Emacs mode's imenu index uses, and each maps onto a `vscode.SymbolKind`.
 */
export type DefinitionKind =
  | "function"
  | "method"
  | "macro"
  | "struct"
  | "enum"
  | "trait"
  | "impl"
  | "variable"
  | "constant"
  | "module";

export interface Definition {
  kind: DefinitionKind;
  /** The defined name as written, e.g. `point<T>` or `Shape rect` for an impl. */
  name: string;
  /** Whether the definition carries the `pub` marker. */
  isPublic: boolean;
  /** Offset of the opening paren of the whole form. */
  start: number;
  /** Offset one past the form's closing paren (or end of text if unbalanced). */
  end: number;
  /** Offset of the name, for the "selection range" an editor highlights. */
  nameStart: number;
  nameEnd: number;
}

interface Pattern {
  kind: DefinitionKind;
  regex: RegExp;
}

/**
 * `(` then an optional `pub` -- visibility is spelled flat, `(pub defun f ...)`
 * rather than `(pub (defun f ...))` -- then the form's own head.
 */
function head(word: string): string {
  return `\\(\\s*(?:(pub)\\s+)?(${word})(?![A-Za-z0-9_?!*<>=/+\\-.%&^~:])`;
}

/** A defined name, optionally wrapped in parens as `defvar`'s `(NAME Type)` is. */
const NAMED = `\\s*\\(?\\s*(${SYMBOL})`;

const PATTERNS: Pattern[] = [
  { kind: "function", regex: new RegExp(head("defun") + NAMED, "gi") },
  { kind: "method", regex: new RegExp(head("defmethod") + NAMED, "gi") },
  { kind: "macro", regex: new RegExp(head("defmacro") + NAMED, "gi") },
  { kind: "struct", regex: new RegExp(head("defstruct") + NAMED, "gi") },
  { kind: "enum", regex: new RegExp(head("defenum") + NAMED, "gi") },
  { kind: "trait", regex: new RegExp(head("deftrait") + NAMED, "gi") },
  { kind: "variable", regex: new RegExp(head("defvar") + NAMED, "gi") },
  { kind: "constant", regex: new RegExp(head("defconstant") + NAMED, "gi") },
  { kind: "module", regex: new RegExp(head("module") + NAMED, "gi") },
  // `(impl Trait Type ...)` -- both names identify the block, so both are shown.
  {
    kind: "impl",
    regex: new RegExp(head("impl(?:<[^>]*>)?") + `\\s*(${SYMBOL}\\s+${SYMBOL})`, "gi"),
  },
];

/**
 * Whether an offset is inside a string, a line comment or a block comment, so a
 * `defun` mentioned in prose is not indexed as a definition and the word
 * "rect" in a remark is not painted as the type.
 *
 * Returns a predicate rather than a set so the whole document is scanned once.
 */
function makeSkipPredicate(text: string): (offset: number) => boolean {
  const ranges = collectSkipRanges(text);
  // Binary search rather than a linear scan: `findTypeReferences` queries this
  // once per candidate occurrence, and a file with many types would otherwise
  // be quadratic.
  return (offset: number) => {
    let lo = 0;
    let hi = ranges.length - 1;
    while (lo <= hi) {
      const mid = (lo + hi) >> 1;
      const [from, to] = ranges[mid];
      if (offset < from) {
        hi = mid - 1;
      } else if (offset >= to) {
        lo = mid + 1;
      } else {
        return true;
      }
    }
    return false;
  };
}

/** The string, line-comment and block-comment spans of `text`, in order. */
function collectSkipRanges(text: string): Array<[number, number]> {
  const skipped: Array<[number, number]> = [];
  let i = 0;
  while (i < text.length) {
    const c = text[i];
    if (c === ";") {
      const start = i;
      while (i < text.length && text[i] !== "\n") {
        i++;
      }
      skipped.push([start, i]);
    } else if (c === "#" && text[i + 1] === "|") {
      const start = i;
      let depth = 1;
      i += 2;
      while (i < text.length && depth > 0) {
        if (text[i] === "#" && text[i + 1] === "|") {
          depth++;
          i += 2;
        } else if (text[i] === "|" && text[i + 1] === "#") {
          depth--;
          i += 2;
        } else {
          i++;
        }
      }
      skipped.push([start, i]);
    } else if (c === "#" && text[i + 1] === "\\") {
      // A character literal, so `#\"` and `#\;` cannot open a string or comment.
      i += 2;
      if (i < text.length && /[A-Za-z]/.test(text[i])) {
        while (i < text.length && /[A-Za-z0-9]/.test(text[i])) {
          i++;
        }
      } else {
        i++;
      }
    } else if (c === '"') {
      const start = i;
      i++;
      while (i < text.length && text[i] !== '"') {
        if (text[i] === "\\") {
          i++;
        }
        i++;
      }
      i = Math.min(i + 1, text.length);
      skipped.push([start, i]);
    } else {
      i++;
    }
  }
  return skipped;
}

/**
 * The offset one past the `)` matching the `(` at `open`, or the end of `text`
 * when the form is unbalanced -- so a definition still gets a sensible range in
 * a file that is mid-edit.
 */
function endOfForm(text: string, open: number): number {
  let depth = 0;
  let i = open;
  while (i < text.length) {
    const c = text[i];
    if (c === ";") {
      while (i < text.length && text[i] !== "\n") {
        i++;
      }
      continue;
    }
    if (c === "#" && text[i + 1] === "|") {
      let d = 1;
      i += 2;
      while (i < text.length && d > 0) {
        if (text[i] === "#" && text[i + 1] === "|") {
          d++;
          i += 2;
        } else if (text[i] === "|" && text[i + 1] === "#") {
          d--;
          i += 2;
        } else {
          i++;
        }
      }
      continue;
    }
    if (c === "#" && text[i + 1] === "\\") {
      i += 2;
      if (i < text.length && /[A-Za-z]/.test(text[i])) {
        while (i < text.length && /[A-Za-z0-9]/.test(text[i])) {
          i++;
        }
      } else {
        i++;
      }
      continue;
    }
    if (c === '"') {
      i++;
      while (i < text.length && text[i] !== '"') {
        if (text[i] === "\\") {
          i++;
        }
        i++;
      }
      i++;
      continue;
    }
    if (c === "(") {
      depth++;
    } else if (c === ")") {
      depth--;
      if (depth === 0) {
        return i + 1;
      }
    }
    i++;
  }
  return text.length;
}

/**
 * Every top-level-visible definition in `text`, in source order.
 *
 * Nesting is not reconstructed: a `module`'s definitions are returned alongside
 * it rather than under it, which is what `imenu` does too. The Outline view
 * shows a flat, source-ordered list, and for a Lisp file that is the useful
 * shape -- `(module ...)` blocks are rare and shallow.
 */
export function findDefinitions(text: string): Definition[] {
  const isSkipped = makeSkipPredicate(text);
  const found: Definition[] = [];
  for (const { kind, regex } of PATTERNS) {
    regex.lastIndex = 0;
    let m: RegExpExecArray | null;
    while ((m = regex.exec(text)) !== null) {
      const start = m.index;
      if (isSkipped(start)) {
        continue;
      }
      const name = m[3];
      if (name === undefined) {
        continue;
      }
      const nameStart = m.index + m[0].lastIndexOf(name);
      found.push({
        kind,
        name: name.replace(/\s+/g, " "),
        isPublic: m[1] !== undefined,
        start,
        end: endOfForm(text, start),
        nameStart,
        nameEnd: nameStart + name.length,
      });
    }
  }
  return found.sort((a, b) => a.start - b.start);
}

// ---------------------------------------------------------------- type uses

/** The definition forms that introduce a type name. */
const TYPE_KINDS = new Set<DefinitionKind>(["struct", "enum", "trait"]);

export interface TypeReference {
  name: string;
  kind: "struct" | "enum" | "trait";
  start: number;
  end: number;
}

/**
 * Every *use* of a type this document defines.
 *
 * A `defstruct`/`defenum`/`deftrait` name is usually lowercase (`rect`,
 * `todo-item`, `board`), so the grammar's Capitalized-name rule cannot reach it
 * -- and a TextMate grammar could not do this anyway, being line-local with no
 * knowledge of what the file declares elsewhere. `extension.ts` turns these
 * into semantic tokens, which is the mechanism that *can* see the whole
 * document.
 *
 * Document-local by design: resolving a type imported through `use` would mean
 * reimplementing the project's `typelisp.toml` module resolution here, which is
 * the language server's job. A cross-file type therefore stays uncoloured
 * rather than being guessed at. The Emacs mode draws the line in the same place
 * (`typelisp--match-local-type`).
 */
export function findTypeReferences(text: string): TypeReference[] {
  const kinds = new Map<string, "struct" | "enum" | "trait">();
  for (const def of findDefinitions(text)) {
    if (!TYPE_KINDS.has(def.kind)) {
      continue;
    }
    // A generic header is one token, so `point<T>` declares the type `point`.
    const name = def.name.split("<")[0];
    if (name && !kinds.has(name)) {
      kinds.set(name, def.kind as "struct" | "enum" | "trait");
    }
  }
  if (kinds.size === 0) {
    return [];
  }

  // Longest-first so a name that prefixes another cannot shadow it.
  const alternation = [...kinds.keys()]
    .sort((a, b) => b.length - a.length || a.localeCompare(b))
    .map((n) => n.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"))
    .join("|");
  // The neighbour rules match the Emacs mode's `typelisp--type-adjacent`:
  //   before -- `<` and `,` are allowed, because a type appears right after
  //     them as a generic argument (`Vector<lexpr>`, `HashTable<i32,token>`);
  //     `>` is forbidden, which is what stops `bignum` in `int->bignum`.
  //   after  -- `<` (its own generic arguments), `:` (`rect::new`) and `>`
  //     (closing an enclosing generic) are allowed; anything else that could
  //     continue a symbol is not, so `rect` does not match in `rectangle`.
  const regex = new RegExp(
    `(?<![-A-Za-z0-9_?!*>=/+.%&^~:])(${alternation})(?![-A-Za-z0-9_?!*=/+.%&^~])`,
    "g",
  );

  const isSkipped = makeSkipPredicate(text);
  const out: TypeReference[] = [];
  let m: RegExpExecArray | null;
  while ((m = regex.exec(text)) !== null) {
    if (isSkipped(m.index)) {
      continue;
    }
    const name = m[1];
    out.push({ name, kind: kinds.get(name)!, start: m.index, end: m.index + name.length });
  }
  return out;
}
