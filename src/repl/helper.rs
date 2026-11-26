//! What the line editor asks of the REPL while a line is typed: completion
//! (Tab), the signature of the function being called (shown after the
//! cursor), and the parenthesis matching the one at the cursor.
//!
//! The names come from the checker, borrowed only while the editor waits
//! for a key, when nothing is being checked or run. Whether a form is
//! finished is not decided here: that is the reader's to say, and the read
//! loop asks it once the line is entered.

use std::borrow::Cow;
use std::cell::RefCell;
use std::rc::Rc;

use rustyline::completion::{Completer, FilenameCompleter, Pair};
use rustyline::highlight::{CmdKind, Highlighter, MatchingBracketHighlighter};
use rustyline::hint::Hinter;
use rustyline::validate::Validator;
use rustyline::{Context, Editor, Helper};

use crate::check::CompletionKind;
use crate::Checker;

use super::commands::COMMANDS;

/// The REPL's line editor.
pub(super) type ReplEditor = Editor<ReplHelper, rustyline::history::DefaultHistory>;

pub(super) struct ReplHelper {
    checker: Rc<RefCell<Checker>>,
    files: FilenameCompleter,
    brackets: MatchingBracketHighlighter,
}

impl ReplHelper {
    pub(super) fn new(checker: Rc<RefCell<Checker>>) -> ReplHelper {
        ReplHelper { checker, files: FilenameCompleter::new(), brackets: MatchingBracketHighlighter::new() }
    }
}

impl Helper for ReplHelper {}

impl Validator for ReplHelper {}

impl Completer for ReplHelper {
    type Candidate = Pair;

    fn complete(&self, line: &str, pos: usize, ctx: &Context<'_>) -> rustyline::Result<(usize, Vec<Pair>)> {
        if scan(&line[..pos]).in_string {
            // Inside a string the word is a file's name: what `load` and
            // `use` are given.
            return self.files.complete(line, pos, ctx);
        }
        let (start, names) = completions(&self.checker.borrow(), line, pos);
        Ok((start, names.into_iter().map(|n| Pair { display: n.clone(), replacement: n }).collect()))
    }
}

impl Hinter for ReplHelper {
    type Hint = String;

    fn hint(&self, line: &str, pos: usize, _ctx: &Context<'_>) -> Option<String> {
        if pos < line.len() {
            return None;
        }
        signature_hint(&self.checker.borrow(), line)
    }
}

impl Highlighter for ReplHelper {
    fn highlight<'l>(&self, line: &'l str, pos: usize) -> Cow<'l, str> {
        self.brackets.highlight(line, pos)
    }

    fn highlight_char(&self, line: &str, pos: usize, kind: CmdKind) -> bool {
        self.brackets.highlight_char(line, pos, kind)
    }

    fn highlight_hint<'h>(&self, hint: &'h str) -> Cow<'h, str> {
        // Dim, so that it does not read as typed text.
        Cow::Owned(format!("\x1b[2m{}\x1b[0m", hint))
    }
}

/// Where the word ending at `pos` in `line` starts, and the names it can be
/// completed to, in order.
pub(super) fn completions(checker: &Checker, line: &str, pos: usize) -> (usize, Vec<String>) {
    let start = line[..pos].char_indices().rev().find(|(_, c)| !is_constituent(*c)).map_or(0, |(i, c)| i + c.len_utf8());
    let word = &line[start..pos];
    let mut names: Vec<String> = if let Some(command) = word.strip_prefix(':').filter(|_| line[..start].trim().is_empty()) {
        // A command is only a command at the start of a line.
        COMMANDS
            .iter()
            .flat_map(|c| std::iter::once(c.name).chain(c.aliases.iter().copied()))
            .filter(|name| name[1..].starts_with(command))
            .map(str::to_string)
            .collect()
    } else if word.starts_with(':') {
        Vec::new()
    } else if word.contains("::") {
        qualified(checker, word)
    } else {
        let here = typelisp_mem::symbols::ns_path(checker.current_module());
        checker
            .completion_candidates(&here)
            .into_iter()
            .map(|c| c.name)
            .chain(Checker::form_names().into_iter().map(str::to_string))
            .filter(|name| name.starts_with(word))
            .collect()
    };
    names.sort();
    names.dedup();
    (start, names)
}

/// The paths a word with `::` in it can be completed to: every definition,
/// and every type's method, whose path has the word's segments at its end,
/// kept as far as the word wrote it.
fn qualified(checker: &Checker, word: &str) -> Vec<String> {
    let mut paths = Vec::new();
    for d in checker.definitions() {
        let path = d.path.to_string();
        paths.extend(d.methods.iter().map(|m| format!("{}::{}", path, m.name())));
        paths.push(path);
    }
    paths
        .into_iter()
        .filter_map(|p| {
            // The word may start at any segment of the path.
            let at = std::iter::once(0).chain(p.match_indices("::").map(|(i, _)| i + 2)).find(|&i| p[i..].starts_with(word))?;
            Some(p[at..].to_string())
        })
        .collect()
}

/// The type of the function named first in the innermost list open at the
/// end of `line`, once a space has followed its name.
pub(super) fn signature_hint(checker: &Checker, line: &str) -> Option<String> {
    let s = scan(line);
    let open = *s.open.last()?;
    if s.in_string {
        return None;
    }
    let after = &line[open + 1..];
    let end = after.find(|c: char| !is_constituent(c))?;
    let head = &after[..end];
    if head.is_empty() {
        return None;
    }
    let here = typelisp_mem::symbols::ns_path(checker.current_module());
    let found = checker.completion_candidates(&here).into_iter().find(|c| c.name == head && c.kind == CompletionKind::Function)?;
    Some(format!("  {}", found.detail))
}

/// How many lists are open at the end of `text`: how far the next line is
/// indented.
pub(super) fn open_lists(text: &str) -> usize {
    scan(text).open.len()
}

/// What a left-to-right reading of `text` has open at its end.
struct Scan {
    /// Where each list still open begins, outermost first.
    open: Vec<usize>,
    /// Whether the end is inside a string.
    in_string: bool,
}

/// Reads `text` far enough to know which lists and strings are open at its
/// end: strings, `\` escapes in them, character literals (`#\(`) and `;`
/// comments are skipped. For the editor's display only — whether a form is
/// finished is the reader's to say.
fn scan(text: &str) -> Scan {
    let mut open = Vec::new();
    let mut in_string = false;
    let mut chars = text.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        if in_string {
            match c {
                '\\' => {
                    chars.next();
                }
                '"' => in_string = false,
                _ => {}
            }
            continue;
        }
        match c {
            '"' => in_string = true,
            ';' => {
                while chars.peek().is_some_and(|(_, c)| *c != '\n') {
                    chars.next();
                }
            }
            '#' if chars.peek().is_some_and(|(_, c)| *c == '\\') => {
                chars.next();
                chars.next();
            }
            '(' => open.push(i),
            ')' => {
                open.pop();
            }
            _ => {}
        }
    }
    Scan { open, in_string }
}

/// Whether `c` can be part of a name.
fn is_constituent(c: char) -> bool {
    !c.is_whitespace() && !"()'`,\";".contains(c)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{load_prelude, Heap, Interp, Reader};

    /// A checker with the prelude loaded, in `user`, after `src` is run.
    fn session(src: &str) -> Checker {
        let mut heap = Heap::with_capacity(1 << 16);
        let mut checker = Checker::new();
        let mut interp = Interp::new();
        load_prelude(&mut heap, &mut checker, &mut interp);
        checker.enter_file_module(&[crate::types::USER_MODULE.to_string()]).unwrap();
        let reader = Reader::new();
        let mut forms = reader.forms_within("<test>", src, checker.current_module());
        while let Some((v, loc)) = forms.next_form_with(&mut heap, Some(&interp)).unwrap() {
            let tl = checker.check_form_at(&mut heap, &interp, v, Some(loc)).unwrap();
            interp.exec(&mut heap, tl).unwrap();
        }
        checker
    }

    #[test]
    fn completes_names_forms_commands_and_paths() {
        let checker = session("(defun inner-fn ((x int) (s string)) int 1)");
        assert_eq!(completions(&checker, "(inner-f", 8), (1, vec!["inner-fn".to_string()]));
        // A special form and a top-level form are offered beside definitions.
        assert!(completions(&checker, "(unwind-p", 9).1.contains(&"unwind-protect".to_string()));
        assert!(completions(&checker, "(defstr", 7).1.contains(&"defstruct".to_string()));
        assert_eq!(completions(&checker, ":desc", 5), (0, vec![":describe".to_string()]));
        // Not at the start of a line, a colon begins a keyword.
        assert_eq!(completions(&checker, "(f :desc", 8).1, Vec::<String>::new());
        assert_eq!(completions(&checker, "(seq::rever", 11).1, vec!["seq::reverse".to_string()]);
        assert_eq!(completions(&checker, "(std::seq::rever", 16).1, vec!["std::seq::reverse".to_string()]);
    }

    #[test]
    fn hints_the_type_of_the_function_being_called() {
        let checker = session("(defun inner-fn ((x int) (s string)) int 1)");
        assert_eq!(signature_hint(&checker, "(+ 1 (inner-fn "), Some("  (fn (int string) int)".to_string()));
        assert_eq!(signature_hint(&checker, "(inner-fn"), None);
        assert_eq!(signature_hint(&checker, "(inner-fn 1 \"a"), None);
        assert_eq!(signature_hint(&checker, "(no-such-thing "), None);
    }

    #[test]
    fn scan_skips_strings_comments_and_characters() {
        assert_eq!(open_lists("(defun f ()"), 1);
        assert_eq!(open_lists("(f \"(\" #\\( ; (\n"), 1);
        assert_eq!(open_lists("(a (b) (c"), 2);
        assert!(scan("(load \"fi").in_string);
        assert!(!scan("(load \"a\\\"b\" ").in_string);
    }
}
