//! The types an executable's startup makes, written as words.
//!
//! An id is a number this process gave out, so an executable cannot carry one
//! over from the compiler. It carries the types' shapes instead, and its
//! startup interns them ([`define`]) into the table of ids the generated code
//! reads. A shape names the types it is made of by their positions earlier in
//! the same list, and a definition by its module and its symbol: the module as
//! its segments from the root, the symbol as its home module and its name.
//! A definition alone is written the same way ([`definition_words`]), for
//! the tables an executable registers definitions in.
//!
//! ```text
//! type   := NAMED ns sym N type-pos^N | DYN ns sym N type-pos^N
//!         | FN N type-pos^N rest-pos ret-pos     rest-pos is -1 for no `&rest`
//!         | PTR type-pos | CSTRUCT ns sym | UNIT | NEVER | PARAM N
//! ns     := N string^N                           the segments below the root
//! sym    := ns string                            the home module and the name
//! string := address length                       UTF-8 bytes
//! ```

use std::collections::HashMap;

use crate::symbols::{child_ns, intern_exact, ns_name, ns_parent, NsId, SymRef};
use crate::{TypeId, TypeShape};

const NAMED: i64 = 0;
const DYN: i64 = 1;
const FN: i64 = 2;
const PTR: i64 = 3;
const CSTRUCT: i64 = 4;
const UNIT: i64 = 5;
const NEVER: i64 = 6;
const PARAM: i64 = 7;

/// One word of the list, before it is placed in an executable: a number, or
/// a string, which the executable writes as its address and its length.
#[derive(Clone, Debug, PartialEq)]
pub enum Word {
    Int(i64),
    Str(&'static str),
}

/// The list being written: every type added so far, each after the types it
/// is made of.
#[derive(Default)]
pub struct TypeWords {
    words: Vec<Word>,
    positions: HashMap<TypeId, usize>,
}

impl TypeWords {
    /// The position of `ty` in the list [`define`] returns, writing it — and
    /// first the types it is made of — if it is not written yet.
    pub fn add(&mut self, ty: TypeId) -> usize {
        if let Some(&pos) = self.positions.get(&ty) {
            return pos;
        }
        let shape = crate::type_shape(ty);
        let mut words = Vec::new();
        let each = |this: &mut Self, words: &mut Vec<Word>, ids: &[TypeId]| {
            words.push(Word::Int(ids.len() as i64));
            for id in ids {
                let pos = this.add(*id);
                words.push(Word::Int(pos as i64));
            }
        };
        match &shape {
            TypeShape::Named(ns, sym, args) | TypeShape::Dyn(ns, sym, args) => {
                words.push(Word::Int(if matches!(shape, TypeShape::Named(..)) { NAMED } else { DYN }));
                write_ns(&mut words, *ns);
                write_sym(&mut words, *sym);
                each(self, &mut words, args);
            }
            TypeShape::Fn(params, rest, ret) => {
                words.push(Word::Int(FN));
                each(self, &mut words, params);
                let rest = rest.map_or(-1, |r| self.add(r) as i64);
                let ret = self.add(*ret);
                words.push(Word::Int(rest));
                words.push(Word::Int(ret as i64));
            }
            TypeShape::Ptr(pointee) => {
                let pointee = self.add(*pointee);
                words.push(Word::Int(PTR));
                words.push(Word::Int(pointee as i64));
            }
            TypeShape::CStruct(ns, sym) => {
                words.push(Word::Int(CSTRUCT));
                write_ns(&mut words, *ns);
                write_sym(&mut words, *sym);
            }
            TypeShape::Unit => words.push(Word::Int(UNIT)),
            TypeShape::Never => words.push(Word::Int(NEVER)),
            TypeShape::Param(n) => {
                words.push(Word::Int(PARAM));
                words.push(Word::Int(i64::from(*n)));
            }
        }
        let pos = self.positions.len();
        self.positions.insert(ty, pos);
        self.words.extend(words);
        pos
    }

    /// How many types the list holds — the length of what [`define`] returns.
    pub fn len(&self) -> usize {
        self.positions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.positions.is_empty()
    }

    /// The list, for the executable to embed.
    pub fn words(&self) -> &[Word] {
        &self.words
    }
}

fn write_ns(words: &mut Vec<Word>, ns: NsId) {
    let mut segments = Vec::new();
    let mut at = ns;
    while let Some(parent) = ns_parent(at) {
        segments.push(ns_name(at).expect("a module with a parent has a name").name());
        at = parent;
    }
    words.push(Word::Int(segments.len() as i64));
    words.extend(segments.into_iter().rev().map(Word::Str));
}

fn write_sym(words: &mut Vec<Word>, sym: SymRef) {
    write_ns(words, sym.home());
    words.push(Word::Str(sym.name()));
}

/// `ns sym`: the definition `sym` names in the module `ns`, as words.
pub fn definition_words(ns: NsId, sym: SymRef) -> Vec<Word> {
    let mut words = Vec::new();
    write_ns(&mut words, ns);
    write_sym(&mut words, sym);
    words
}

/// The definition [`definition_words`] wrote.
///
/// # Safety
///
/// As [`define`].
pub unsafe fn read_definition(words: &[i64]) -> (NsId, SymRef) {
    let mut reader = Reader { words, at: 0 };
    let ns = reader.ns();
    (ns, reader.sym())
}

/// Interns the types `words` writes, in order, and returns their ids.
///
/// # Safety
///
/// Each string's two words must be the address of `length` bytes of valid
/// UTF-8 that stay alive while this runs — what an executable's constant
/// strings are.
pub unsafe fn define(words: &[i64]) -> Vec<TypeId> {
    let mut reader = Reader { words, at: 0 };
    let mut ids: Vec<TypeId> = Vec::new();
    while reader.at < words.len() {
        let shape = match reader.next() {
            tag @ (NAMED | DYN) => {
                let ns = reader.ns();
                let sym = reader.sym();
                let args = reader.ids(&ids);
                if tag == NAMED {
                    TypeShape::Named(ns, sym, args)
                } else {
                    TypeShape::Dyn(ns, sym, args)
                }
            }
            FN => {
                let params = reader.ids(&ids);
                let rest = match reader.next() {
                    -1 => None,
                    pos => Some(ids[pos as usize]),
                };
                let ret = ids[reader.next() as usize];
                TypeShape::Fn(params, rest, ret)
            }
            PTR => TypeShape::Ptr(ids[reader.next() as usize]),
            CSTRUCT => {
                let ns = reader.ns();
                TypeShape::CStruct(ns, reader.sym())
            }
            UNIT => TypeShape::Unit,
            NEVER => TypeShape::Never,
            PARAM => TypeShape::Param(reader.next() as u32),
            tag => panic!("the type list has an unknown tag {} at word {}", tag, reader.at - 1),
        };
        ids.push(crate::intern_type(shape));
    }
    ids
}

struct Reader<'w> {
    words: &'w [i64],
    at: usize,
}

impl Reader<'_> {
    fn next(&mut self) -> i64 {
        let w = self.words[self.at];
        self.at += 1;
        w
    }

    /// # Safety
    ///
    /// See [`define`].
    unsafe fn string(&mut self) -> &str {
        let address = self.next() as usize as *const u8;
        let length = self.next() as usize;
        std::str::from_utf8(std::slice::from_raw_parts(address, length)).expect("a type list's strings are UTF-8")
    }

    unsafe fn ns(&mut self) -> NsId {
        let n = self.next();
        let mut ns = NsId::ROOT;
        for _ in 0..n {
            ns = child_ns(ns, self.string());
        }
        ns
    }

    unsafe fn sym(&mut self) -> SymRef {
        let home = self.ns();
        intern_exact(home, self.string())
    }

    fn ids(&mut self, defined: &[TypeId]) -> Vec<TypeId> {
        let n = self.next();
        (0..n).map(|_| defined[self.next() as usize]).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What an executable does with the list: each string becomes its
    /// address and length.
    fn placed(words: &[Word]) -> Vec<i64> {
        words
            .iter()
            .flat_map(|w| match w {
                Word::Int(n) => vec![*n],
                Word::Str(s) => vec![s.as_ptr() as i64, s.len() as i64],
            })
            .collect()
    }

    #[test]
    fn a_list_defines_the_types_it_was_written_from() {
        let m = child_ns(NsId::STD, "type-words-test");
        let int = crate::intern_type(TypeShape::named(NsId::ROOT, "int", &[]));
        let gen = crate::intern_type(TypeShape::named(m, "gen", &[int, TypeId::OPTION_SEXPR]));
        let template = crate::intern_type(TypeShape::named(NsId::ROOT, "option", &[crate::intern_type(TypeShape::Param(1))]));
        let f = crate::intern_type(TypeShape::Fn(vec![gen], Some(int), crate::intern_type(TypeShape::Unit)));
        let ptr = crate::intern_type(TypeShape::Ptr(gen));
        let mut list = TypeWords::default();
        let wanted = [f, template, ptr, gen, int];
        let positions: Vec<usize> = wanted.iter().map(|t| list.add(*t)).collect();
        let ids = unsafe { define(&placed(list.words())) };
        assert_eq!(ids.len(), list.len());
        for (ty, pos) in wanted.iter().zip(positions) {
            assert_eq!(ids[pos], *ty, "{}", &*crate::type_name(*ty));
        }
    }

    #[test]
    fn a_definition_reads_back() {
        let m = child_ns(child_ns(NsId::ROOT, "type-words-test"), "inner");
        let sym = intern_exact(m, "callback");
        assert_eq!(unsafe { read_definition(&placed(&definition_words(m, sym))) }, (m, sym));
    }
}
