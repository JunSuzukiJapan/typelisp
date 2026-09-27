//! A C callback's identity: which typelisp function C is handed, under which C
//! signature.
//!
//! A `defffi` parameter declared `(fn (T...) R)` takes a function C will call
//! back. C holds nothing but a code address, so each (function, signature)
//! pair needs an entry of its own that converts C's arguments, runs the
//! function and converts its answer — made by the JIT when a session first
//! asks for it, or emitted into an executable by `compile-file`. This key is
//! what every one of those places agrees on: the checker writes it into the
//! call site, the interpreter and the runtime table look it up, and the
//! backend reads the signature back out of it.
//!
//! The function is always a top-level one. A capture-free `lambda` or local
//! function is lifted to one by the checker before a key is made for it, so
//! the entry never needs an environment — C would have nowhere to pass one.

use crate::types::{Path, Type};

/// The internal builtin a callback argument lowers to: `(" ffi-callback-address" KEY)`
/// answers the entry's address as a raw word.
///
/// The leading space is deliberate. The reader never produces a symbol that
/// starts with one, so no program can call this by name or define a function
/// that a compiled call site would mistake for it.
pub const ADDRESS_BUILTIN: &str = " ffi-callback-address";

/// Separates the key's fields. A unit separator occurs in no path segment and
/// in no type key.
const SEP: char = '\u{1f}';

/// One callback entry: the function's full path and the C types it is called
/// with, spelled as `type_key::type_key_of_type` spells them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CallbackSig {
    pub path: String,
    pub params: Vec<String>,
    pub ret: String,
}

impl CallbackSig {
    /// The signature a `(fn (T...) R)` parameter type declares, for `path`.
    /// `None` when `ty` is not a fixed-arity function type.
    pub fn from_fn_type(path: &Path, ty: &Type) -> Option<CallbackSig> {
        match ty {
            Type::Fn(params, None, ret) => Some(CallbackSig {
                path: path.to_string(),
                params: params.iter().map(crate::type_key::type_key_of_type).collect(),
                ret: crate::type_key::type_key_of_type(ret),
            }),
            _ => None,
        }
    }

    /// The key: path, return type, then each parameter type.
    pub fn key(&self) -> String {
        let mut k = String::new();
        k.push_str(&self.path);
        k.push(SEP);
        k.push_str(&self.ret);
        for p in &self.params {
            k.push(SEP);
            k.push_str(p);
        }
        k
    }

    /// [`Self::key`] read back.
    pub fn parse(key: &str) -> Result<CallbackSig, String> {
        let mut parts = key.split(SEP);
        let path = parts.next().filter(|p| !p.is_empty());
        let ret = parts.next();
        match (path, ret) {
            (Some(path), Some(ret)) => Ok(CallbackSig {
                path: path.to_string(),
                ret: ret.to_string(),
                params: parts.map(str::to_string).collect(),
            }),
            _ => Err(format!("ffi: `{}` is not a callback key", key.replace(SEP, "|"))),
        }
    }
}

/// The parameter and return keys of a function type's key,
/// `(fn (K,K...) K)` — what `type_key_of_type` writes for `Type::Fn` — or
/// `None` if `key` is not one. Split at top-level commas only, so a
/// parameter key that itself contains parentheses stays whole.
pub fn split_fn_key(key: &str) -> Option<(Vec<String>, String)> {
    let rest = key.strip_prefix("(fn (")?;
    let mut depth = 0usize;
    let mut params = Vec::new();
    let mut cur = String::new();
    let mut chars = rest.char_indices();
    let close = loop {
        let (i, c) = chars.next()?;
        match c {
            '(' => {
                depth += 1;
                cur.push(c);
            }
            ')' if depth == 0 => break i,
            ')' => {
                depth -= 1;
                cur.push(c);
            }
            ',' if depth == 0 => params.push(std::mem::take(&mut cur)),
            _ => cur.push(c),
        }
    };
    if !cur.is_empty() {
        params.push(cur);
    }
    let ret = rest[close + 1..].strip_prefix(' ')?.strip_suffix(')')?;
    // `&rest` is written into the parameter list by `type_key_of_type`; a
    // callback type never has one, and a key with one is not a callback's.
    if params.iter().any(|p| p.starts_with("&rest")) {
        return None;
    }
    Some((params, ret.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_key_reads_back() {
        let sig = CallbackSig { path: "m::cmp".into(), params: vec!["ptr".into(), "ptr".into()], ret: "i32".into() };
        assert_eq!(CallbackSig::parse(&sig.key()).unwrap(), sig);
        let none = CallbackSig { path: "go".into(), params: vec![], ret: "()".into() };
        assert_eq!(CallbackSig::parse(&none.key()).unwrap(), none);
    }

    #[test]
    fn a_function_type_key_splits_at_top_level_commas() {
        assert_eq!(split_fn_key("(fn (ptr,ptr) i32)"), Some((vec!["ptr".into(), "ptr".into()], "i32".into())));
        assert_eq!(split_fn_key("(fn () ())"), Some((vec![], "()".into())));
        assert_eq!(
            split_fn_key("(fn ((ptr point),i32) bool)"),
            Some((vec!["(ptr point)".into(), "i32".into()], "bool".into()))
        );
        assert_eq!(split_fn_key("ptr"), None);
        assert_eq!(split_fn_key("(fn (i32,&rest i32) i32)"), None);
    }
}
