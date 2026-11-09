//! `module`: `(module path body...)`, the body's forms checked in the module
//! `path` names under the current one.
//!
//! Each form of the body is a top-level form, checked as one. A form that
//! has to take effect before the next is checked (a `defmacro`, a readtable
//! registration) is run as soon as it is checked, as the driver runs it
//! between two top-level forms.

use crate::types::Path;
use crate::{Error, Heap, Loc, Value};
use typelisp_mem::symbols::{child_ns, KEYWORD_MODULE};
use typelisp_mem::NsId;

use super::use_form::Site;

/// The site `(module path ...)` written in `site` checks its body in, and
/// the module's path.
pub(crate) fn module_site(heap: &Heap, site: Site, parts: &[(Value, Option<Loc>)]) -> Result<(Site, Path), Error> {
    let Some((path, _)) = parts.first() else {
        return Err(Error::TypeError("module: (module path body...)".into()));
    };
    let segs: Vec<String> = match *path {
        Value::Symbol(id) => vec![heap.symbol_spelling(id).to_string()],
        Value::Path(id) => heap.path_segments(id).iter().map(|s| heap.symbol_spelling(*s).to_string()).collect(),
        _ => return Err(Error::TypeError("expected a name or `::` path".into())),
    };
    if site.ns == NsId::ROOT && segs.first().is_some_and(|s| s.eq_ignore_ascii_case(KEYWORD_MODULE)) {
        return Err(Error::TypeError(format!(
            "module `{}` is the keyword namespace, which holds the keywords and nothing else — choose another name",
            KEYWORD_MODULE
        )));
    }
    let ns = segs.iter().fold(site.ns, |ns, s| child_ns(ns, s));
    Ok((Site { ns, ..site }, Path::of_module(ns)))
}
