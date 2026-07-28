// Lisp indentation for typelisp.
//
// VS Code has no built-in Lisp indenter and its regexp-based `indentationRules`
// cannot express one, because Lisp indentation depends on the *head* of the
// enclosing form. So this is a port of the algorithm the Emacs mode uses
// (`editor/emacs/typelisp-mode.el`): the same `INDENT_SPECS` table, and the
// same three-way resolution Emacs's `calculate-lisp-indent`,
// `lisp-indent-defform` and `lisp-indent-specform` perform between them.
//
// Keeping it a faithful port -- rather than a fresh design -- is the point: a
// file must indent identically whichever editor touched it last. That is
// enforced by `src/test/indent.test.ts`, which re-indents all of `examples/`
// and requires the result to be byte-identical to what is committed (the same
// corpus, and the same standard, the Emacs mode is held to).
//
// Nothing here imports `vscode`, so it is directly unit-testable under `node
// --test`.

/** Columns a body line is indented past the head of its enclosing form. */
export const BODY_INDENT = 2;

/** Emacs's default `tab-width`, used when measuring a column. */
const TAB_WIDTH = 8;

/**
 * How a form's arguments are indented. `"defun"` means "everything through the
 * end of the first line is header; indent the body by `BODY_INDENT`". A number
 * N means "N distinguished arguments, then body".
 */
export type IndentSpec = "defun" | number;

/**
 * Indent specs keyed by the form's head as written.
 *
 * A direct transcription of `typelisp-indent-specs` in the Emacs mode; see the
 * comments there for why the definition forms all take `"defun"` (a typelisp
 * header is longer than the Emacs Lisp shape the integers were tuned for) and
 * why `if` takes 3 (its `else` branch is mandatory, so both branches line up).
 */
export const INDENT_SPECS: ReadonlyMap<string, IndentSpec> = new Map<string, IndentSpec>([
  // Definition forms.
  ["defun", "defun"],
  ["defmethod", "defun"],
  ["defmacro", "defun"],
  ["defstruct", "defun"],
  ["defenum", "defun"],
  ["deftrait", "defun"],
  ["defvar", "defun"],
  ["defconstant", "defun"],
  ["module", "defun"],
  ["impl", "defun"],
  // `pub` prefixes a whole definition flatly -- `(pub defun f ...)`.
  ["pub", "defun"],
  // `lambda` also carries a return type: `(lambda (PARAMS) RETTYPE ...)`.
  ["lambda", "defun"],
  // Binding and conditionals.
  ["let", 1],
  ["let*", 1],
  ["labels", 1],
  ["if", 3],
  ["if-let", 3],
  ["when", 1],
  ["unless", 1],
  ["cond", 0],
  ["case", 1],
  ["and", 0],
  ["or", 0],
  ["progn", 0],
  ["the", 1],
  ["match", 1],
  ["while-let", 1],
  // Iteration.
  ["loop", 0],
  ["while", 1],
  ["until", 1],
  ["dotimes", 1],
  ["dolist", 1],
  ["doiter", 1],
  ["do", 2],
  // Pretty printer.
  ["pprint-logical-block", 1],
]);

/** Forms whose members are themselves `(NAME (PARAMS) RETTYPE BODY...)`. */
const MEMBER_DEFORM_HEADS = new Set(["impl", "deftrait"]);

/**
 * Every character that can appear inside a typelisp symbol, matching
 * `typelisp-mode-syntax-table`: alphanumerics plus the operator characters.
 */
function isSymbolChar(ch: string | undefined): boolean {
  return ch !== undefined && /[A-Za-z0-9_?!*<>=/+\-.%&^~:]/.test(ch);
}

function isWhitespace(ch: string): boolean {
  return ch === " " || ch === "\t" || ch === "\n" || ch === "\r" || ch === "\f";
}

/** Reader prefixes that attach to the sexp that follows them. */
function isPrefixChar(ch: string): boolean {
  return ch === "'" || ch === "`" || ch === "," || ch === "#";
}

interface Sexp {
  /** Offset of the sexp's first character, reader prefixes included. */
  start: number;
  /** Offset one past the sexp's last character. */
  end: number;
}

interface Form {
  /** Offset of the `(`. */
  open: number;
  /** Offset of the matching `)`, or -1 when the form is still open at EOF. */
  close: number;
  /** Offset the form's own sexp starts at (before any reader prefix). */
  start: number;
  /** Index into `Scan.forms` of the enclosing form, or -1 at top level. */
  parent: number;
  /** The form's direct children, in source order. */
  children: Sexp[];
}

export interface Scan {
  text: string;
  /** Offset each line begins at. */
  lineStarts: number[];
  forms: Form[];
  /** Start offset of every sexp at any depth, ascending. */
  sexpStarts: number[];
  /** Innermost still-open form at each line's first character, -1 if none. */
  containingAtLineStart: number[];
  /** Whether each line's first character sits inside a string literal. */
  insideStringAtLineStart: boolean[];
}

/**
 * Parse `text` into the paren structure the indenter needs: for every form, its
 * open/close offsets and its direct children's ranges; plus, per line, the
 * innermost enclosing form and whether the line opens inside a string.
 *
 * Comments (`;` to end of line, and nestable `#| ... |#`) and string contents
 * are skipped so their parens never affect structure, and `#\(` is read as the
 * character literal it is.
 */
export function scan(text: string): Scan {
  const lineStarts = [0];
  for (let i = 0; i < text.length; i++) {
    if (text[i] === "\n") {
      lineStarts.push(i + 1);
    }
  }

  const forms: Form[] = [];
  const sexpStarts: number[] = [];
  const containingAtLineStart: number[] = new Array(lineStarts.length).fill(-1);
  const insideStringAtLineStart: boolean[] = new Array(lineStarts.length).fill(false);
  const stack: number[] = [];
  // Offset of a pending reader prefix (`'`, `` ` ``, `,`, `#`), or -1. Reset by
  // whitespace, since Emacs's `backward-prefix-chars` only walks back over
  // prefixes immediately adjacent to the sexp.
  let pendingPrefix = -1;
  // Next line whose starting state has yet to be recorded.
  let nextLine = 1;

  // `pos` is the offset just past the character consumed, so that the newline
  // at offset i records the line beginning at i+1.
  const recordLinesUpTo = (pos: number, inString: boolean): void => {
    while (nextLine < lineStarts.length && lineStarts[nextLine] <= pos) {
      containingAtLineStart[nextLine] = stack.length ? stack[stack.length - 1] : -1;
      insideStringAtLineStart[nextLine] = inString;
      nextLine++;
    }
  };

  const beginSexp = (natural: number): number => (pendingPrefix >= 0 ? pendingPrefix : natural);

  const addSexp = (start: number, end: number): void => {
    sexpStarts.push(start);
    if (stack.length) {
      forms[stack[stack.length - 1]].children.push({ start, end });
    }
    pendingPrefix = -1;
  };

  let i = 0;
  while (i < text.length) {
    const c = text[i];

    // Line comment: runs to (but not past) the newline.
    if (c === ";") {
      while (i < text.length && text[i] !== "\n") {
        i++;
      }
      pendingPrefix = -1;
      continue;
    }

    // Nestable block comment.
    if (c === "#" && text[i + 1] === "|") {
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
          if (text[i] === "\n") {
            recordLinesUpTo(i + 1, false);
          }
          i++;
        }
      }
      pendingPrefix = -1;
      continue;
    }

    // Character literal: `#\a`, `#\Space`. Consumed whole so that `#\(`,
    // `#\)`, `#\;` and `#\"` cannot be mistaken for structure.
    if (c === "#" && text[i + 1] === "\\") {
      const start = beginSexp(i);
      let j = i + 2;
      if (j < text.length && /[A-Za-z]/.test(text[j])) {
        while (j < text.length && /[A-Za-z0-9]/.test(text[j])) {
          j++;
        }
      } else {
        j = Math.min(j + 1, text.length);
      }
      addSexp(start, j);
      i = j;
      continue;
    }

    if (isPrefixChar(c)) {
      if (pendingPrefix < 0) {
        pendingPrefix = i;
      }
      i++;
      continue;
    }

    if (c === '"') {
      const start = beginSexp(i);
      let j = i + 1;
      while (j < text.length && text[j] !== '"') {
        if (text[j] === "\\") {
          j++;
        }
        if (j < text.length && text[j] === "\n") {
          recordLinesUpTo(j + 1, true);
        }
        j++;
      }
      j = Math.min(j + 1, text.length);
      addSexp(start, j);
      i = j;
      continue;
    }

    if (c === "(") {
      const start = beginSexp(i);
      const idx = forms.length;
      forms.push({
        open: i,
        close: -1,
        start,
        parent: stack.length ? stack[stack.length - 1] : -1,
        children: [],
      });
      sexpStarts.push(start);
      pendingPrefix = -1;
      stack.push(idx);
      i++;
      continue;
    }

    if (c === ")") {
      const idx = stack.pop();
      if (idx !== undefined) {
        forms[idx].close = i;
        const parent = forms[idx].parent;
        if (parent >= 0) {
          forms[parent].children.push({ start: forms[idx].start, end: i + 1 });
        }
      }
      pendingPrefix = -1;
      i++;
      continue;
    }

    if (isWhitespace(c)) {
      if (c === "\n") {
        recordLinesUpTo(i + 1, false);
      }
      pendingPrefix = -1;
      i++;
      continue;
    }

    // An atom.
    const start = beginSexp(i);
    let j = i;
    while (j < text.length && isSymbolChar(text[j])) {
      j++;
    }
    if (j === i) {
      j++;
    }
    addSexp(start, j);
    i = j;
  }
  recordLinesUpTo(text.length, false);

  return { text, lineStarts, forms, sexpStarts, containingAtLineStart, insideStringAtLineStart };
}

function lineOf(s: Scan, pos: number): number {
  let lo = 0;
  let hi = s.lineStarts.length - 1;
  while (lo < hi) {
    const mid = (lo + hi + 1) >> 1;
    if (s.lineStarts[mid] <= pos) {
      lo = mid;
    } else {
      hi = mid - 1;
    }
  }
  return lo;
}

/** Emacs's `current-column`: tab stops every `TAB_WIDTH` columns. */
function columnOf(s: Scan, pos: number): number {
  let col = 0;
  for (let i = s.lineStarts[lineOf(s, pos)]; i < pos; i++) {
    col = s.text[i] === "\t" ? col + TAB_WIDTH - (col % TAB_WIDTH) : col + 1;
  }
  return col;
}

/** Offset the line after the one containing `pos` begins at. */
function startOfNextLine(s: Scan, pos: number): number {
  const next = lineOf(s, pos) + 1;
  return next < s.lineStarts.length ? s.lineStarts[next] : s.text.length;
}

/** The first sexp starting at or after `pos`, at any depth. */
function firstSexpStartFrom(s: Scan, pos: number): number {
  let lo = 0;
  let hi = s.sexpStarts.length;
  while (lo < hi) {
    const mid = (lo + hi) >> 1;
    if (s.sexpStarts[mid] < pos) {
      lo = mid + 1;
    } else {
      hi = mid;
    }
  }
  return lo < s.sexpStarts.length ? s.sexpStarts[lo] : pos;
}

/**
 * The head symbol of `form` as written, downcased, or null when the form's
 * first element is not a symbol (a binding list, whose first element is itself
 * a list). Mirrors `typelisp--head-at`, including skipping whitespace after the
 * open paren.
 */
function headAt(s: Scan, form: Form): string | null {
  let i = form.open + 1;
  while (i < s.text.length && isWhitespace(s.text[i])) {
    i++;
  }
  let j = i;
  while (j < s.text.length && isSymbolChar(s.text[j])) {
    j++;
  }
  return j > i ? s.text.slice(i, j).toLowerCase() : null;
}

/**
 * Whether `form` is a *local* definition, whose body should indent like any
 * other definition body even though its head is a user-chosen name with no
 * entry in `INDENT_SPECS`.
 *
 * Two forms nest a `(NAME (PARAMS) RETTYPE BODY...)` definition: an `impl` /
 * `deftrait` member, and a `labels` local function -- which sits one level
 * deeper, inside the binding list. The `labels` case is matched through the
 * *grandparent* because a `let` binding has the very same shape one level down,
 * and its value expression must keep aligning under the bound name. Mirrors
 * `typelisp--local-defform-body-p`.
 */
function isLocalDefformBody(s: Scan, form: Form): boolean {
  const parent = form.parent >= 0 ? s.forms[form.parent] : undefined;
  if (!parent) {
    return false;
  }
  const parentHead = headAt(s, parent);
  if (parentHead !== null) {
    return MEMBER_DEFORM_HEADS.has(parentHead);
  }
  const grandparent = parent.parent >= 0 ? s.forms[parent.parent] : undefined;
  return grandparent !== undefined && headAt(s, grandparent) === "labels";
}

/**
 * The column Emacs's `calculate-lisp-indent` leaves point at before consulting
 * the indent spec -- "align with the argument above" -- reproducing its
 * three-way `cond` verbatim.
 */
function normalIndentPos(s: Scan, form: Form, lastSexp: Sexp): number {
  const first = form.children[0];
  const whitespaceAfterOpen = isWhitespace(s.text[form.open + 1] ?? "");

  // The containing form's first element is itself a list: indent under it.
  if (s.text[first.start] === "(") {
    return first.start;
  }
  // The last complete sexp is on the same line as the first element, so this is
  // the first line to start within the containing form -- almost certainly a
  // function call.
  if (startOfNextLine(s, first.start) > lastSexp.start) {
    if (first.start === lastSexp.start || whitespaceAfterOpen) {
      // Nothing before this line but the first element (or the first element is
      // preceded by whitespace): indent under that element.
      return first.start;
    }
    // Otherwise skip the head and indent under the first argument.
    return firstSexpStartFrom(s, first.end);
  }
  // Indent beneath the first sexp on the same line as the last complete sexp.
  return firstSexpStartFrom(s, s.lineStarts[lineOf(s, lastSexp.start)]);
}

/** Emacs's `lisp-indent-defform`. Null means "no special indentation". */
function indentDefform(s: Scan, form: Form, lastSexp: Sexp): number | null {
  return startOfNextLine(s, form.open) > lastSexp.start
    ? columnOf(s, form.open) + BODY_INDENT
    : null;
}

/** Emacs's `lisp-indent-specform`. */
function indentSpecform(
  s: Scan,
  count: number,
  form: Form,
  indentPoint: number,
  normalIndent: number,
): number {
  const requested = count;
  const containingCol = columnOf(s, form.open);
  const bodyIndent = containingCol + BODY_INDENT;

  // Walk the arguments after the head, counting one off per argument, until
  // reaching the line being indented.
  let remaining = count;
  let idx = 1;
  let pos =
    form.children.length > 1 && form.children[1].start < indentPoint
      ? form.children[1].start
      : indentPoint;
  while (pos < indentPoint) {
    remaining--;
    idx++;
    pos =
      idx < form.children.length && form.children[idx].start < indentPoint
        ? form.children[idx].start
        : indentPoint;
  }

  if (remaining > 0) {
    // A distinguished argument. The first or second one gets double the body
    // indent; any later one aligns with the argument above.
    return requested - remaining <= 1 ? containingCol + 2 * BODY_INDENT : normalIndent;
  }
  // A body argument: use the body indent for the first one, otherwise align.
  if ((requested === 0 && remaining === 0) || (remaining === 0 && bodyIndent <= normalIndent)) {
    return bodyIndent;
  }
  return normalIndent;
}

/**
 * The column line `lineIndex` should be indented to, or null to leave the line
 * exactly as it is (it starts inside a string, or is a `;;;` comment -- both of
 * which `lisp-indent-line` declines to touch).
 */
export function computeIndent(s: Scan, lineIndex: number): number | null {
  const indentPoint = s.lineStarts[lineIndex];
  if (s.insideStringAtLineStart[lineIndex]) {
    return null;
  }
  let firstNonWs = indentPoint;
  while (
    firstNonWs < s.text.length &&
    s.text[firstNonWs] !== "\n" &&
    isWhitespace(s.text[firstNonWs])
  ) {
    firstNonWs++;
  }
  if (s.text.startsWith(";;;", firstNonWs)) {
    return null;
  }

  const containingIdx = s.containingAtLineStart[lineIndex];
  if (containingIdx < 0) {
    return 0;
  }
  const form = s.forms[containingIdx];

  // The last complete sexp terminated at or before the line being indented.
  let lastSexp: Sexp | undefined;
  for (const child of form.children) {
    if (child.end <= indentPoint) {
      lastSexp = child;
    } else {
      break;
    }
  }
  // The line begins immediately after the open paren; there is nothing to align
  // with, so sit just past it.
  if (!lastSexp) {
    return columnOf(s, form.open) + 1;
  }

  const normalIndent = columnOf(s, normalIndentPos(s, form, lastSexp));

  // A form whose head is not a symbol gets no spec lookup: align with the
  // argument above, re-anchoring to the last sexp's line when it is elsewhere.
  if (!isSymbolChar(s.text[form.open + 1])) {
    const pos = normalIndentPos(s, form, lastSexp);
    if (startOfNextLine(s, pos) > lastSexp.start) {
      return columnOf(s, pos);
    }
    return columnOf(s, firstSexpStartFrom(s, s.lineStarts[lineOf(s, lastSexp.start)]));
  }

  const head = s.text.slice(form.children[0].start, form.children[0].end).toLowerCase();
  let spec = INDENT_SPECS.get(head);
  // An unlisted `def...` head is treated as a definition form, so a user's own
  // `(defmacro defthing ...)` indents like the built-in ones -- the same
  // heuristic `lisp-indent-function` applies. Local definitions inside `impl` /
  // `labels` get the same treatment.
  if (spec === undefined && ((head.length > 3 && head.startsWith("def")) || isLocalDefformBody(s, form))) {
    spec = "defun";
  }

  if (spec === "defun") {
    const indent = indentDefform(s, form, lastSexp);
    return indent !== null ? indent : normalIndent;
  }
  if (typeof spec === "number") {
    return indentSpecform(s, spec, form, indentPoint, normalIndent);
  }
  return normalIndent;
}

/** The leading-whitespace run of the line starting at `lineStart`. */
function leadingWhitespaceLength(text: string, lineStart: number): number {
  let i = lineStart;
  while (i < text.length && (text[i] === " " || text[i] === "\t")) {
    i++;
  }
  return i - lineStart;
}

/**
 * Re-indent every line of `text`, returning the result.
 *
 * Lines are processed top-down and the text is re-scanned whenever one actually
 * moves, because a later line's alignment can depend on the column an earlier
 * line ended up at -- the same order and the same dependency Emacs's
 * `indent-region` has. Empty lines are skipped, which is what `indent-region`
 * does with them (`(and (bolp) (eolp))`).
 */
export function indentText(text: string): string {
  let current = text;
  let s = scan(current);
  for (let line = 0; line < s.lineStarts.length; line++) {
    const lineStart = s.lineStarts[line];
    const lineEnd = line + 1 < s.lineStarts.length ? s.lineStarts[line + 1] - 1 : current.length;
    if (lineEnd === lineStart) {
      continue;
    }
    const want = computeIndent(s, line);
    if (want === null) {
      continue;
    }
    const have = leadingWhitespaceLength(current, lineStart);
    const replacement = " ".repeat(want);
    if (current.slice(lineStart, lineStart + have) === replacement) {
      continue;
    }
    current = current.slice(0, lineStart) + replacement + current.slice(lineStart + have);
    s = scan(current);
  }
  return current;
}

/**
 * The indentation line `lineIndex` of `text` should have, and the length of the
 * whitespace currently there -- what an editor needs to build a single edit.
 * Null when the line should be left alone.
 */
export function indentForLine(
  text: string,
  lineIndex: number,
): { column: number; existing: number } | null {
  const s = scan(text);
  if (lineIndex < 0 || lineIndex >= s.lineStarts.length) {
    return null;
  }
  const column = computeIndent(s, lineIndex);
  if (column === null) {
    return null;
  }
  return { column, existing: leadingWhitespaceLength(text, s.lineStarts[lineIndex]) };
}
