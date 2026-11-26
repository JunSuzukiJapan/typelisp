//! The REPL's own commands: a `:name` at the start of a line, before any
//! form has begun.
//!
//! [`COMMANDS`] is the one list of them. `:help` prints it and [`parse`]
//! dispatches by it, so a command cannot be reachable without being listed,
//! or listed without being reachable. A `:name` the list does not have is
//! not a command: the line is read as forms, where `:name` is a keyword.

/// What a command does; [`super::Session`] runs it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandKind {
    Help,
    Quit,
    Type,
    Cd,
    Pwd,
    Backtrace,
    Locals,
}

/// One REPL command: what [`COMMANDS`] lists.
pub struct Command {
    /// The name `:help` shows, with its colon.
    pub name: &'static str,
    /// Other names for the same command.
    pub aliases: &'static [&'static str],
    /// What follows the name, as `:help` shows it; empty when nothing does.
    pub args: &'static str,
    pub help: &'static str,
    pub kind: CommandKind,
}

/// Every REPL command, in the order `:help` lists them.
pub const COMMANDS: &[Command] = &[
    Command { name: ":help", aliases: &[":h", ":?"], args: "", help: "list the REPL commands", kind: CommandKind::Help },
    Command {
        name: ":type",
        aliases: &[":t"],
        args: "FORM",
        help: "print the type of FORM without running it",
        kind: CommandKind::Type,
    },
    Command {
        name: ":backtrace",
        aliases: &[":bt"],
        args: "",
        help: "list the calls that were open when the last line failed, innermost first",
        kind: CommandKind::Backtrace,
    },
    Command {
        name: ":locals",
        aliases: &[],
        args: "[N]",
        help: "print the arguments of call N of :backtrace (0 without N)",
        kind: CommandKind::Locals,
    },
    Command {
        name: ":cd",
        aliases: &[],
        args: "[DIR]",
        help: "change the current folder (to the home folder without DIR)",
        kind: CommandKind::Cd,
    },
    Command { name: ":pwd", aliases: &[], args: "", help: "print the current folder", kind: CommandKind::Pwd },
    Command { name: ":quit", aliases: &[":exit", ":q"], args: "", help: "leave the REPL", kind: CommandKind::Quit },
];

/// The command `line` starts with, and the text after its name.
pub fn parse(line: &str) -> Option<(CommandKind, &str)> {
    let line = line.trim_start();
    let end = line.find(char::is_whitespace).unwrap_or(line.len());
    let (word, rest) = line.split_at(end);
    COMMANDS
        .iter()
        .find(|c| c.name == word || c.aliases.contains(&word))
        .map(|c| (c.kind, rest.trim()))
}

/// What `:help` prints.
pub fn help_text() -> String {
    let usage = |c: &Command| if c.args.is_empty() { c.name.to_string() } else { format!("{} {}", c.name, c.args) };
    let width = COMMANDS.iter().map(|c| usage(c).len()).max().unwrap_or(0);
    let mut out = String::new();
    for c in COMMANDS {
        out.push_str(&format!("  {:<width$}  {}", usage(c), c.help, width = width));
        if !c.aliases.is_empty() {
            out.push_str(&format!(" (also {})", c.aliases.join(", ")));
        }
        out.push('\n');
    }
    out.push_str("Anything else is read as forms and evaluated.\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_distinct() {
        let mut seen = std::collections::HashSet::new();
        for c in COMMANDS {
            for name in std::iter::once(&c.name).chain(c.aliases) {
                assert!(name.starts_with(':'), "{} does not start with a colon", name);
                assert!(seen.insert(*name), "{} names two commands", name);
            }
        }
    }

    #[test]
    fn every_name_dispatches_to_its_command() {
        for c in COMMANDS {
            for name in std::iter::once(&c.name).chain(c.aliases) {
                assert_eq!(parse(&format!("{} x", name)), Some((c.kind, "x")));
                assert_eq!(parse(name), Some((c.kind, "")));
            }
        }
    }

    #[test]
    fn a_keyword_that_is_no_command_is_not_parsed() {
        assert_eq!(parse(":foo"), None);
        assert_eq!(parse(":types"), None);
        assert_eq!(parse("(:type 1)"), None);
    }
}
