//! `:inspect FORM`: runs FORM and opens its value by its type, as SBCL's
//! and CCL's inspectors do. A struct opens into its fields, an enum into its
//! variant's fields, a `Vector` into its elements and a `HashTable` into its
//! entries; a number gives the part to go into, `u` goes back up, `q` leaves.
//!
//! The value stays rooted while it is open; what is opened from it is
//! reachable from it.

use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;

use crate::check::repr::Repr;
use crate::*;
use typelisp_front::Parts;

use super::{is_incomplete, Session};

/// One part of a value: what it is called in its whole, the value and its
/// type.
struct Part {
    label: String,
    value: Value,
    ty: Type,
}

impl Session<'_> {
    /// `:inspect FORM`.
    pub(super) fn inspect(&mut self, rl: &mut DefaultEditor, arg: &str) {
        let mark = self.heap.root_count();
        match self.value_of(arg) {
            Ok(top) => self.browse(rl, top),
            Err(e) => eprintln!("error: {}", e),
        }
        self.heap.truncate_roots(mark);
    }

    /// Runs the one form `text`, leaving its value rooted.
    fn value_of(&mut self, text: &str) -> Result<Part, String> {
        let mut forms = self.reader.forms_within("<stdin>", text, self.checker.borrow().current_module());
        let (v, loc) = match forms.next_form_with(self.heap, Some(&*self.interp)) {
            Ok(Some(pair)) => pair,
            Ok(None) => return Err(":inspect: give a form, as in `:inspect (list 1 2)`".to_string()),
            Err(e) if is_incomplete(&e) => return Err(":inspect: the form is not finished on this line".to_string()),
            Err(e) => return Err(e.to_string()),
        };
        if !matches!(forms.next_form_with(self.heap, Some(&*self.interp)), Ok(None)) {
            return Err(":inspect: give one form".to_string());
        }
        let checked = self.checker.borrow_mut().check_form_at(self.heap, &*self.interp, v, Some(loc));
        for w in self.checker.borrow_mut().take_warnings() {
            eprintln!("{}", w);
        }
        let tl = checked.map_err(|e| e.to_string())?;
        let ty = self.checker.borrow().expr_type().cloned();
        match (self.interp.exec(self.heap, tl).map_err(|e| e.to_string())?, ty) {
            (Some(value), Some(ty)) => {
                self.heap.push_root(value);
                Ok(Part { label: text.trim().to_string(), value, ty })
            }
            _ => Err(":inspect: the form has no value to open".to_string()),
        }
    }

    /// The inspector's own loop, from `top` down.
    fn browse(&mut self, rl: &mut DefaultEditor, top: Part) {
        let mut path = vec![top];
        loop {
            let here = path.last().expect("the inspector always has the value it is at");
            let parts = match self.parts(here) {
                Ok(parts) => parts,
                Err(e) => {
                    eprintln!("error: {}", e);
                    return;
                }
            };
            print!("{}", self.listing(here, &parts));
            let line = match self.heap.native(|| rl.readline("inspect> ")) {
                Ok(line) => line,
                Err(ReadlineError::Interrupted | ReadlineError::Eof) => return,
                Err(e) => {
                    eprintln!("readline error: {}", e);
                    return;
                }
            };
            match line.trim() {
                "q" => return,
                "u" if path.len() > 1 => {
                    path.pop();
                }
                "u" => println!("already at the top"),
                n => match n.parse::<usize>() {
                    Ok(i) if i < parts.len() => path.push(parts.into_iter().nth(i).expect("checked against the length")),
                    Ok(_) | Err(_) => println!("give a part's number, u to go up or q to leave"),
                },
            }
        }
    }

    /// What the inspector shows at `here`: the value and its type, then its
    /// numbered parts.
    fn listing(&mut self, here: &Part, parts: &[Part]) -> String {
        let mut out = format!("{} = {}\n  type: {}\n", here.label, self.render(here.value, &here.ty), here.ty);
        for (i, part) in parts.iter().enumerate() {
            out.push_str(&format!("  {}. {} = {}\n", i, part.label, self.render(part.value, &part.ty)));
        }
        out
    }

    /// `v` as `~s` prints it, by its type.
    fn render(&mut self, v: Value, ty: &Type) -> String {
        let repr = {
            let chk = self.checker.borrow();
            Repr::of_by(ty, &|p| chk.type_kind(p))
        };
        self.interp.render_readably(self.heap, v, Some(&repr)).unwrap_or_else(|e| format!("#<unprintable: {}>", e))
    }

    /// The parts `here` opens into.
    fn parts(&mut self, here: &Part) -> Result<Vec<Part>, String> {
        let (parts, repr) = {
            let chk = self.checker.borrow();
            (chk.parts_of(&here.ty), Repr::of_by(&here.ty, &|p| chk.type_kind(p)))
        };
        let heap = &*self.heap;
        let boxed = match here.value {
            Value::Boxed(id) => Some(id),
            _ => None,
        };
        let unexpected = || format!("internal error: a value of type {} is not laid out as its type says", here.ty);
        let part = |label: String, value: Value, ty: &Type| Part { label, value, ty: ty.clone() };
        Ok(match parts {
            Parts::Whole => Vec::new(),
            Parts::Fields(fields) => {
                let id = boxed.filter(|id| heap.is_struct(*id)).ok_or_else(unexpected)?;
                fields.iter().enumerate().map(|(i, (name, ty))| part(name.clone(), heap.struct_field(id, i), ty)).collect()
            }
            Parts::Elements(ty) => {
                let id = boxed.filter(|id| heap.is_struct(*id)).ok_or_else(unexpected)?;
                (0..heap.struct_field_count(id)).map(|i| part(format!("[{}]", i), heap.struct_field(id, i), &ty)).collect()
            }
            Parts::Entries(key_ty, value_ty) => {
                let id = boxed.filter(|id| heap.is_hashtable(*id)).ok_or_else(unexpected)?;
                let mut out = Vec::new();
                for (i, (k, v)) in heap.hashtable_pairs(id).into_iter().enumerate() {
                    out.push(part(format!("key {}", i), k, &key_ty));
                    out.push(part(format!("value {}", i), v, &value_ty));
                }
                out
            }
            // An `Option` of a type with a spare value is that value or
            // nothing, not a box: `None` is the empty word.
            Parts::Variants(variants) if matches!(repr, Repr::Niche(_)) => match here.value {
                Value::Empty => Vec::new(),
                v => {
                    let (name, fields) = variants.iter().find(|(_, fs)| fs.len() == 1).ok_or_else(unexpected)?;
                    vec![part(format!("{} 0", name), v, &fields[0])]
                }
            },
            Parts::Variants(variants) => {
                let id = boxed.filter(|id| heap.is_enum(*id)).ok_or_else(unexpected)?;
                let (name, fields) = variants.get(heap.enum_variant(id)).ok_or_else(unexpected)?;
                fields.iter().enumerate().map(|(i, ty)| part(format!("{} {}", name, i), heap.enum_field(id, i), ty)).collect()
            }
        })
    }
}
