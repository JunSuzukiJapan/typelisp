//! Parsing of `Object` syntax into [`Type`].
//!
//! Two surface shapes (grammar §3):
//!  * symbol tokens: primitives (`i32`), generics (`Vec<T>`, glued into one
//!    token by the reader because they contain no spaces), and paths
//!    (`std::string::String`).
//!  * list tokens: `(fn (A B) R)` and `(tuple A B)`.

use crate::{Object, Type, Error};

/// Parse an `Object` as a type.
pub fn parse_type(obj: &Object) -> Result<Type, Error> {
    match obj {
        // `()` is read as Null and denotes the unit type.
        Object::Null => Ok(Type::Unit),
        Object::Symbol(s) => parse_type_symbol(s),
        Object::List(_) => parse_type_list(obj),
        other => Err(Error::InvalidTypeSyntax(format!("{:?}", other))),
    }
}

/// Parse a symbol token as a type.
fn parse_type_symbol(s: &str) -> Result<Type, Error> {
    if s.is_empty() {
        return Err(Error::InvalidTypeSyntax("empty type".to_string()));
    }

    // Generic form: `Head<A, B<C>>` — split the angle section out of the token.
    if let Some(open) = s.find('<') {
        if !s.ends_with('>') {
            return Err(Error::InvalidTypeSyntax(format!(
                "malformed generic type (missing '>'): {}",
                s
            )));
        }
        let head = &s[..open];
        let inner = &s[open + 1..s.len() - 1];
        let args = split_top_level_commas(inner)?
            .into_iter()
            .map(|p| parse_type_symbol(&p))
            .collect::<Result<Vec<_>, _>>()?;
        if head.is_empty() {
            return Err(Error::InvalidTypeSyntax(format!("missing generic head: {}", s)));
        }
        return Ok(Type::Named(head.to_string(), args));
    }

    // Path form: `a::b::c`
    if s.contains("::") {
        let segments: Vec<String> = s.split("::").map(|x| x.to_string()).collect();
        if segments.iter().any(|x| x.is_empty()) {
            return Err(Error::InvalidTypeSyntax(format!("malformed path type: {}", s)));
        }
        return Ok(Type::Path(segments));
    }

    // Primitive or bare name (type variable / nullary struct).
    if let Some(prim) = Type::from_prim_name(s) {
        Ok(prim)
    } else {
        Ok(Type::Named(s.to_string(), Vec::new()))
    }
}

/// Split `s` on top-level commas, respecting nested `<...>`.
fn split_top_level_commas(s: &str) -> Result<Vec<String>, Error> {
    let mut parts = Vec::new();
    let mut depth = 0i32;
    let mut cur = String::new();
    for c in s.chars() {
        match c {
            '<' => {
                depth += 1;
                cur.push(c);
            }
            '>' => {
                depth -= 1;
                if depth < 0 {
                    return Err(Error::InvalidTypeSyntax(format!("unbalanced '>' in {}", s)));
                }
                cur.push(c);
            }
            ',' if depth == 0 => {
                parts.push(cur.trim().to_string());
                cur = String::new();
            }
            _ => cur.push(c),
        }
    }
    if depth != 0 {
        return Err(Error::InvalidTypeSyntax(format!("unbalanced '<' in {}", s)));
    }
    let last = cur.trim().to_string();
    if !last.is_empty() || !parts.is_empty() {
        parts.push(last);
    }
    Ok(parts)
}

/// Parse a list-form type: `(fn (A B) R)` or `(tuple A B)`.
fn parse_type_list(obj: &Object) -> Result<Type, Error> {
    let elems = list_elements(obj);
    let head = match elems.first() {
        Some(Object::Symbol(s)) => s.as_str(),
        _ => return Err(Error::InvalidTypeSyntax("type list must start with a symbol".to_string())),
    };

    match head {
        "fn" => {
            // (fn (param-types...) ret-type)
            if elems.len() != 3 {
                return Err(Error::InvalidTypeSyntax(
                    "fn type must be (fn (param-types) ret-type)".to_string(),
                ));
            }
            let params = parse_type_seq(elems[1])?;
            let ret = parse_type(elems[2])?;
            Ok(Type::Fn(params, Box::new(ret)))
        }
        "tuple" => {
            let mut items = Vec::new();
            for e in &elems[1..] {
                items.push(parse_type(e)?);
            }
            Ok(Type::Tuple(items))
        }
        other => Err(Error::InvalidTypeSyntax(format!("unknown type constructor: {}", other))),
    }
}

/// Parse a parenthesised sequence of types, e.g. the `(A B)` in an `fn` type.
/// An empty sequence is the reader's `Object::Null` (`()`).
fn parse_type_seq(obj: &Object) -> Result<Vec<Type>, Error> {
    match obj {
        Object::Null => Ok(Vec::new()),
        Object::List(_) => list_elements(obj).iter().map(|o| parse_type(o)).collect(),
        other => Err(Error::InvalidTypeSyntax(format!(
            "expected a parenthesised type list, found {:?}",
            other
        ))),
    }
}

/// Collect the elements of a proper list `Object` into a vector of references.
fn list_elements(obj: &Object) -> Vec<&Object> {
    match obj {
        Object::List(cons) => cons.iter().collect(),
        _ => Vec::new(),
    }
}
