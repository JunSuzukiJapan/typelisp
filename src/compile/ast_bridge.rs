//! Bridges the checker's typed AST ([`Typed`]/[`Expr`]) into a plain `Sexpr`
//! the (typelisp-hosted) compiler body can pattern-match on directly — the
//! same "give the compiler ordinary data, not a Rust API" approach
//! `prelude.rs` already uses for the standard library.
//!
//! Each node becomes a tagged list `(tag . fields...)`, e.g. `Expr::Int(42)`
//! -> `(int 42)`. Only the shapes a current compiler phase actually consumes
//! are translated for real; anything else becomes `(unsupported "<Variant>")`
//! so the compiler body can `panic` with a clear message instead of the
//! bridge silently doing the wrong thing — add a real translation here only
//! once a phase needs to compile that node.

use crate::{Error, Expr, Heap, Typed, Value};

/// Conses `tag` onto a list built from `items` (in order), rooting as it
/// goes — the same push/pop discipline `crate::eval::interp::alloc_quoted`
/// uses for `Expr::Quote`, just generalized to N items instead of a fixed
/// two (`car`/`cdr`).
fn tagged(heap: &mut Heap, tag: &str, items: &[Value]) -> Result<Value, Error> {
    let mut acc = Value::Empty;
    for item in items.iter().rev() {
        heap.push_root(*item);
        heap.push_root(acc);
        let next = heap.cons(*item, acc);
        heap.pop_root();
        heap.pop_root();
        acc = next?;
    }
    let tag_sym = heap.intern_symbol(tag);
    heap.push_root(tag_sym);
    heap.push_root(acc);
    let result = heap.cons(tag_sym, acc);
    heap.pop_root();
    heap.pop_root();
    result
}

/// `(unsupported "<Variant>")` — see this module's doc comment.
fn unsupported(heap: &mut Heap, variant: &str) -> Result<Value, Error> {
    let name = heap.alloc_string(variant.to_string());
    tagged(heap, "unsupported", &[name])
}

/// Translates one typed AST node. See the module doc comment for the tagged
/// shape and which variants are real vs. `unsupported` placeholders today.
pub fn ast_to_sexpr(heap: &mut Heap, typed: &Typed) -> Result<Value, Error> {
    match &typed.expr {
        Expr::Int(n) => tagged(heap, "int", &[Value::Int(*n)]),
        Expr::Float(f) => tagged(heap, "float", &[Value::Float(*f)]),
        Expr::Bool(b) => tagged(heap, "bool", &[Value::Bool(*b)]),
        Expr::Char(c) => tagged(heap, "char", &[Value::Char(*c)]),
        Expr::Str(s) => {
            let v = heap.alloc_string(s.clone());
            tagged(heap, "str", &[v])
        }
        Expr::Unit => tagged(heap, "unit", &[]),
        Expr::Var(name) => {
            let v = heap.alloc_string(name.clone());
            tagged(heap, "var", &[v])
        }
        // Everything below needs either multi-child rooting (a child's
        // translated `Value` must stay rooted while its siblings are
        // translated — `Expr::If`'s three children, `Expr::Let`'s bindings
        // plus body, ...) or a node shape this phase doesn't compile yet
        // (`Construct`/`FieldGet`/`FieldSet`/`Match`/`Loop`/...). Both are
        // deliberately deferred to the phase that first needs them, rather
        // than building out unrooted multi-child plumbing nothing exercises
        // yet — see the module doc comment.
        Expr::Global(_) => unsupported(heap, "Global"),
        Expr::FnRef(_) => unsupported(heap, "FnRef"),
        Expr::MethodRef { .. } => unsupported(heap, "MethodRef"),
        Expr::If(..) => unsupported(heap, "If"),
        Expr::Let(..) => unsupported(heap, "Let"),
        Expr::Call(..) => unsupported(heap, "Call"),
        Expr::Lambda { .. } => unsupported(heap, "Lambda"),
        Expr::Labels { .. } => unsupported(heap, "Labels"),
        Expr::Apply(..) => unsupported(heap, "Apply"),
        Expr::Assoc { .. } => unsupported(heap, "Assoc"),
        Expr::Construct { .. } => unsupported(heap, "Construct"),
        Expr::FieldGet(..) => unsupported(heap, "FieldGet"),
        Expr::FieldSet(..) => unsupported(heap, "FieldSet"),
        Expr::Match(..) => unsupported(heap, "Match"),
        Expr::Set(..) => unsupported(heap, "Set"),
        Expr::SetGlobal(..) => unsupported(heap, "SetGlobal"),
        Expr::Loop(_) => unsupported(heap, "Loop"),
        Expr::Break => unsupported(heap, "Break"),
        Expr::Return(_) => unsupported(heap, "Return"),
        Expr::Panic(_) => unsupported(heap, "Panic"),
        Expr::Quote(_) => unsupported(heap, "Quote"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Type;

    fn typed(expr: Expr, ty: Type) -> Typed {
        Typed { expr, ty }
    }

    /// Unpacks a tagged-list `Value` into (tag name, field values), asserting
    /// it's a proper list (every `cdr` until the final `Empty` is itself a
    /// `Cons`, matching `tagged`'s own construction).
    fn untag(heap: &Heap, v: Value) -> (String, Vec<Value>) {
        let sym = heap.car(v).expect("tagged list has a car");
        let tag = match sym {
            Value::Symbol(id) => heap.symbol_name(id).to_string(),
            other => panic!("expected a tag symbol, got {:?}", other),
        };
        let mut fields = Vec::new();
        let mut rest = heap.cdr(v).expect("tagged list has a cdr");
        while !rest.is_empty() {
            fields.push(heap.car(rest).expect("list field has a car"));
            rest = heap.cdr(rest).expect("list field has a cdr");
        }
        (tag, fields)
    }

    #[test]
    fn translates_an_int_literal() {
        let mut heap = Heap::with_capacity(1 << 10);
        let v = ast_to_sexpr(&mut heap, &typed(Expr::Int(42), Type::I64)).unwrap();
        let (tag, fields) = untag(&heap, v);
        assert_eq!(tag, "int");
        assert_eq!(fields, vec![Value::Int(42)]);
    }

    #[test]
    fn translates_a_bool_literal() {
        let mut heap = Heap::with_capacity(1 << 10);
        let v = ast_to_sexpr(&mut heap, &typed(Expr::Bool(true), Type::Bool)).unwrap();
        let (tag, fields) = untag(&heap, v);
        assert_eq!(tag, "bool");
        assert_eq!(fields, vec![Value::Bool(true)]);
    }

    #[test]
    fn translates_a_str_literal() {
        let mut heap = Heap::with_capacity(1 << 10);
        let v = ast_to_sexpr(&mut heap, &typed(Expr::Str("hi".to_string()), Type::Str)).unwrap();
        let (tag, fields) = untag(&heap, v);
        assert_eq!(tag, "str");
        match fields[0] {
            Value::Str(id) => assert_eq!(heap.string(id), "hi"),
            other => panic!("expected a Str, got {:?}", other),
        }
    }

    #[test]
    fn an_unimplemented_node_becomes_an_explicit_unsupported_tag() {
        let mut heap = Heap::with_capacity(1 << 10);
        let cond = Box::new(typed(Expr::Bool(true), Type::Bool));
        let then = Box::new(typed(Expr::Int(1), Type::I64));
        let els = Box::new(typed(Expr::Int(2), Type::I64));
        let v = ast_to_sexpr(&mut heap, &typed(Expr::If(cond, then, els), Type::I64)).unwrap();
        let (tag, fields) = untag(&heap, v);
        assert_eq!(tag, "unsupported");
        match fields[0] {
            Value::Str(id) => assert_eq!(heap.string(id), "If"),
            other => panic!("expected a Str, got {:?}", other),
        }
    }
}
