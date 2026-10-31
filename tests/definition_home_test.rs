//! A definition is named by a symbol of the module that defines it.
//!
//! The symbol alone then says which definition it names — its home is the
//! module — which is what lets the core IR carry a definition as its symbol.
//! A name every module reads as one symbol made elsewhere (a reserved word, a
//! vocabulary member made in another module) would name a definition of a
//! module it does not belong to.

mod common;
use common::{Load, Session};

/// Every definition the library, the compiler island and a program's modules
/// make is keyed by a symbol whose home is the defining module.
#[test]
fn every_definition_is_named_by_a_symbol_of_its_module() {
    let mut s = Session::new(Load::Compiler);
    s.eval(
        "(module m
           (pub defstruct Point (x int))
           (pub defenum Shape (circle int) (square int))
           (pub deftrait Area () (area (self) int))
           (pub defvar (tally int) 0)
           (pub defmacro twice (x) `(progn ,x ,x))
           (pub defun next-board ((b int)) int b))",
    )
    .expect("the program runs");
    let mut wrong = Vec::new();
    for m in s.checker.registry().namespaces() {
        let tables = [
            ("fn", m.fns.symbols().collect::<Vec<_>>()),
            ("var", m.vars.symbols().collect()),
            ("macro", m.macros.symbols().collect()),
            ("type", m.types.symbols().collect()),
            ("trait", m.traits.symbols().collect()),
            ("type alias", m.type_aliases.symbols().collect()),
            ("C struct", m.c_structs.symbols().collect()),
            ("constructor", m.ctors.symbols().collect()),
        ];
        for (what, syms) in tables {
            for sym in syms {
                if sym.home() != m.ns {
                    wrong.push(format!(
                        "{} `{}` in `{}` is named by a symbol of `{}`",
                        what,
                        sym.name(),
                        typelisp::mem::symbols::ns_path(m.ns).join("::"),
                        typelisp::mem::symbols::ns_path(sym.home()).join("::")
                    ));
                }
            }
        }
    }
    wrong.sort();
    assert!(wrong.is_empty(), "{} definitions:\n{}", wrong.len(), wrong.join("\n"));
}
