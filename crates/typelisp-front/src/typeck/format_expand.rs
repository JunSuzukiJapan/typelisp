//! `format`, `print` and `println` expanded into calls of the printer's
//! pieces (`typelisp_print::format::ops`).
//!
//! The control string is a literal, so what it says is decided here, once:
//! the expansion is a sequence of calls, each writing one directive's output
//! into a buffer, and nothing reads the control string when the program runs.
//! A program therefore links the renderers its control strings use and no
//! others.
//!
//! The arguments are evaluated first, each into a local of its own type.
//! Then the expansion takes one of two shapes:
//!
//! - **Fixed positions.** Where every directive's argument is known here —
//!   which it is unless the cursor's path depends on a value — the expansion
//!   names each argument directly, and a `~^` either fires or does not, here.
//!   This is the shape almost every control string has.
//! - **A list of arguments.** Otherwise (a `~[` whose clauses consume
//!   different numbers of arguments, a `~v*`, a `~@{` over the arguments
//!   themselves, ...) the arguments are also made into one `Sexpr` list and
//!   walked with a cursor at run time, as the elements of a `~{…~}` always
//!   are.
//!
//! Which renderer writes a value is chosen by its type when the expansion is
//! lowered ([`lowered_piece`]): a generic body is lowered once per
//! specialization, and only there is every type known.

use std::collections::HashMap;

use typelisp_mem::symbols::well_known_symbol;
use typelisp_mem::SymRef;
use typelisp_print::format::ops::{PARAM_CHAR, PARAM_INT, PARAM_NON_NEG, PARAM_POS, PARAM_RADIX};
use typelisp_print::format::{self as engine, Head, Node as Directive, Param, ParamKind};

use crate::types::{Path, Type};
use crate::{wk, Error, Heap, Loc, Value};

use super::expr::{check_expr, Cx};
use super::locals::Locals;
use super::sexpr_value::{option_of_sexpr, sexpr_ty};
use super::tree::{Node, Typed};

/// Where the output goes.
pub(crate) enum Dest {
    /// `format`'s: the text is the value, and is also written to stdout when
    /// `write` (the destination) is true.
    Text { write: Typed },
    /// `print`.
    Print,
    /// `println`.
    Println,
}

/// The pieces an expansion calls when the program runs, each a builtin with
/// a shim of its own (`typelisp_print::shim`). `F_FMT_A` and `F_FMT_D` are
/// not among them: lowering replaces each with one of these
/// ([`lowered_piece`]).
pub const PIECES: &[u32] = &[
    wk::F_FMT_OPEN, wk::F_FMT_TEXT, wk::F_FMT_FRESH_LINE, wk::F_FMT_STRING, wk::F_FMT_NUMBER, wk::F_FMT_VALUE,
    wk::F_FMT_INTEGER, wk::F_FMT_INTEGER_ANY, wk::F_FMT_WORDS, wk::F_FMT_PLURAL, wk::F_FMT_CHAR, wk::F_FMT_FIXED,
    wk::F_FMT_EXPONENT, wk::F_FMT_GENERAL, wk::F_FMT_DOLLARS, wk::F_FMT_TAB, wk::F_FMT_NEWLINE, wk::F_FMT_INDENT,
    wk::F_FMT_CASE_OPEN, wk::F_FMT_CASE_CLOSE, wk::F_FMT_JUST_OPEN, wk::F_FMT_JUST_SEGMENT, wk::F_FMT_JUST_CLOSE,
    wk::F_FMT_BLOCK_OPEN, wk::F_FMT_BLOCK_CLOSE, wk::F_FMT_RESULT, wk::F_FMT_EMIT, wk::F_FMT_FIRST, wk::F_FMT_REST,
    wk::F_FMT_EMPTY, wk::F_FMT_LENGTH, wk::F_FMT_GOTO, wk::F_FMT_AS_LIST, wk::F_FMT_PARAM, wk::F_FMT_SELECT,
    wk::F_FMT_BOOL,
];

/// The type of the buffer an expansion writes into.
pub(crate) fn format_out_ty() -> Type {
    Type::Named(Path::builtin(wk::FORMAT_OUT), Vec::new())
}

/// `control` with the checked arguments `items`, expanded. `calls` holds the
/// checked call of each `~/name/` directive, by where its name starts in the
/// control string and the argument it is handed ([`super::format`]).
pub(crate) fn expand(
    cx: &Cx,
    heap: &mut Heap,
    locals: &Locals,
    dest: Dest,
    control: &str,
    items: Vec<Typed>,
    calls: HashMap<(usize, usize), Typed>,
    loc: Option<Loc>,
) -> Result<Typed, Error> {
    let directives = engine::parse(control).map_err(Error::TypeError)?;
    let mut args = Vec::with_capacity(items.len());
    let mut arg_binds = Vec::with_capacity(items.len());
    for item in items {
        let name = cx.temp();
        args.push((name, item.ty.clone()));
        arg_binds.push((name, item));
    }
    let fixed = fixed_positions(&directives, 0, args.len()).is_some();
    let mut g = Gen { cx, heap, locals, o: cx.temp(), args, calls, temps: Vec::new(), labels: 0 };
    let mut body = Vec::new();
    if fixed {
        g.seq(&directives, &mut Frame::Fixed { pos: 0 }, &mut body)?;
    } else {
        let mut frame = g.list_frame()?;
        let Frame::List { whole, rest, .. } = frame.clone() else { unreachable!("list_frame makes a list frame") };
        let items: Vec<Typed> = g.args.iter().map(|(name, ty)| local(*name, ty.clone())).collect();
        body.push(set(whole, super::format::sexpr_list(items)));
        body.push(set(rest, local(whole, option_of_sexpr())));
        let label = g.label();
        let mut inner = Vec::new();
        let mut esc = Escape { label: label.clone(), closes: Vec::new() };
        g.seq_list(&directives, &mut frame, &mut esc, &mut inner)?;
        body.push(block(label, inner, &mut g)?);
    }
    let o = local(g.o, format_out_ty());
    let ty = match &dest {
        Dest::Text { .. } => Type::Str,
        Dest::Print | Dest::Println => Type::Unit,
    };
    body.push(match dest {
        Dest::Text { write } => call(wk::F_FMT_RESULT, vec![o, write], Type::Str),
        Dest::Print => call(wk::F_FMT_EMIT, vec![o, g.flag(false)?], Type::Unit),
        Dest::Println => call(wk::F_FMT_EMIT, vec![o, g.flag(true)?], Type::Unit),
    });
    let mut binds = vec![(g.o, call(wk::F_FMT_OPEN, Vec::new(), format_out_ty()))];
    binds.append(&mut g.temps);
    let inner = Typed { node: Node::Let { binds, body }, ty: ty.clone(), loc: None };
    Ok(Typed { node: Node::Let { binds: arg_binds, body: vec![inner] }, ty, loc })
}

/// The piece a call the expansion made of `~a`/`~s` (`F_FMT_A`) or `~d` and
/// its kin (`F_FMT_D`) lowers to, and the arguments to hand it, chosen by the
/// settled type of the value it writes (`settle` gives a type as the
/// lowering sees it). `None` for any other call.
///
/// The value arrives held as a `Sexpr` ([`Node::AsSexpr`]) when the
/// expansion named it; one read off a list of arguments is a `Sexpr`
/// already, and is written by the pieces that take any value.
pub(crate) fn lowered_piece<'t>(path: &Path, args: &'t [Typed], settle: impl Fn(&Type) -> Type) -> Option<(Path, Vec<&'t Typed>)> {
    let held = match &args.get(1)?.node {
        Node::AsSexpr { value } => Some((&**value, settle(&value.ty))),
        _ => None,
    };
    let number = |t: &Type| t.is_int_family() || matches!(t, Type::Ratio | Type::Rational | Type::F32 | Type::F64);
    let all: Vec<&Typed> = args.iter().collect();
    let (piece, args) = if *path == Path::internal_builtin(wk::F_FMT_A) {
        // The buffer, the value, `escape`, then the padding.
        match held {
            Some((value, Type::Str)) => {
                let mut args = all;
                args[1] = value;
                (wk::F_FMT_STRING, args)
            }
            // A number prints the same escaped or not.
            Some((_, t)) if number(&t) => (wk::F_FMT_NUMBER, all.into_iter().enumerate().filter(|(i, _)| *i != 2).map(|(_, a)| a).collect()),
            _ => (wk::F_FMT_VALUE, all),
        }
    } else if *path == Path::internal_builtin(wk::F_FMT_D) {
        match held {
            Some((_, t)) if t.is_int_family() => (wk::F_FMT_INTEGER, all),
            _ => (wk::F_FMT_INTEGER_ANY, all),
        }
    } else {
        return None;
    };
    Some((Path::internal_builtin(piece), args))
}

// ---- deciding the shape ----------------------------------------------------

/// Where the cursor is after `nodes`, run from `pos` with `n` arguments,
/// and whether a `~^` stopped them — when that is known here, so that the
/// expansion can name each argument. `None` when it depends on a value.
fn fixed_positions(nodes: &[Directive], mut pos: usize, n: usize) -> Option<(usize, bool)> {
    for node in nodes {
        match node {
            Directive::Text(_) => {}
            Directive::Escape { head } => {
                let (after, present) = fixed_params(head, pos, n, true)?;
                pos = after;
                if escape_fires(&present, n.saturating_sub(pos)) {
                    return Some((pos, true));
                }
            }
            Directive::Case { body, .. } => {
                let (after, escaped) = fixed_positions(body, pos, n)?;
                pos = after;
                if escaped {
                    return Some((pos, true));
                }
            }
            Directive::Block { head, segments, .. } => {
                if head.at {
                    if let Some(body) = block_body(segments) {
                        pos = fixed_positions(body, pos, n)?.0;
                    }
                } else {
                    pos += 1;
                }
            }
            Directive::Cond { head, clauses, default } => {
                if head.colon || head.params.is_empty() {
                    pos += 1;
                } else {
                    pos = fixed_params(head, pos, n, false)?.0;
                }
                let mut outcomes = Vec::new();
                for clause in clauses {
                    outcomes.push(fixed_positions(clause, pos, n)?);
                }
                // Choosing no clause leaves the cursor where it is.
                let can_choose_none = if head.colon { clauses.len() < 2 } else { default.is_none() };
                if can_choose_none {
                    outcomes.push((pos, false));
                }
                let first = *outcomes.first()?;
                if outcomes.iter().any(|o| *o != first) {
                    return None;
                }
                pos = first.0;
                if first.1 {
                    return Some((pos, true));
                }
            }
            Directive::Iter { head, .. } => {
                if head.at {
                    return None;
                }
                pos = fixed_params(head, pos, n, false)?.0 + 1;
            }
            Directive::Just { head, segments } => {
                pos = fixed_params(head, pos, n, false)?.0;
                for segment in segments {
                    let (after, escaped) = fixed_positions(segment, pos, n)?;
                    pos = after;
                    if escaped {
                        break;
                    }
                }
            }
            Directive::Call { .. } => pos += 1,
            Directive::Dir { head, ch } => {
                let (after, present) = fixed_params(head, pos, n, *ch == '*')?;
                pos = after;
                match ch {
                    'a' | 's' | 'w' | 'd' | 'b' | 'o' | 'x' | 'r' | 'c' | 'f' | 'e' | 'g' | '$' => pos += 1,
                    'p' => {
                        if head.colon {
                            pos = pos.checked_sub(1)?;
                        }
                        pos += 1;
                    }
                    '*' => {
                        let k = match present.first() {
                            Some(k) => *k,
                            None if head.at => 0,
                            None => 1,
                        };
                        let target = if head.at {
                            k
                        } else if head.colon {
                            pos as i64 - k
                        } else {
                            pos as i64 + k
                        };
                        if target < 0 || target as usize > n {
                            return None;
                        }
                        pos = target as usize;
                    }
                    _ => {}
                }
            }
        }
    }
    Some((pos, false))
}

/// The values of `head`'s parameters that are given, and the cursor after
/// the `v` ones read theirs. When the values decide where the cursor goes
/// (`moves`, for `~*` and `~^`), `None` if one is a `v`: only the run knows
/// its value.
fn fixed_params(head: &Head, mut pos: usize, n: usize, moves: bool) -> Option<(usize, Vec<i64>)> {
    let mut present = Vec::new();
    let mut known = true;
    for p in &head.params {
        match p {
            Param::Int(v) => present.push(*v),
            Param::Char(c) => present.push(*c as i64),
            Param::Count => present.push(n.saturating_sub(pos) as i64),
            Param::Arg => {
                pos += 1;
                known = false;
            }
            Param::Default => {}
        }
    }
    if !known && moves {
        return None;
    }
    Some((pos, present))
}

/// Whether a `~^` whose given parameters are `present` fires with `remaining`
/// arguments left: with none, when none remain; `~n^` when `n` is zero;
/// `~n,m^` when they are equal; `~n,m,o^` when `n <= m <= o`.
fn escape_fires(present: &[i64], remaining: usize) -> bool {
    match present {
        [] => remaining == 0,
        [a] => *a == 0,
        [a, b] => a == b,
        [a, b, c, ..] => a <= b && b <= c,
    }
}

/// The body of a logical block (`~<…~:>`), its first segment being its
/// prefix and its last its suffix when it has more than one.
fn block_body(segments: &[Vec<Directive>]) -> Option<&Vec<Directive>> {
    match segments.len() {
        0 | 1 => segments.first(),
        _ => segments.get(1),
    }
}

// ---- building the expansion --------------------------------------------------

/// Where the arguments a stretch of directives consumes come from.
#[derive(Clone)]
enum Frame {
    /// The call's own arguments, at a position known here.
    Fixed { pos: usize },
    /// A list walked at run time: the whole list, what is left of it, and how
    /// many of its arguments are behind the cursor.
    List { whole: SymRef, rest: SymRef, idx: SymRef },
}

/// Where a `~^` that fires at run time goes: out of the block `label`, after
/// closing the `~(` buffers opened inside it (innermost last).
struct Escape {
    label: Box<str>,
    closes: Vec<(bool, bool)>,
}

struct Gen<'x, 'c, 'l> {
    cx: &'x Cx<'c>,
    heap: &'x mut Heap,
    locals: &'x Locals<'l>,
    /// The buffer.
    o: SymRef,
    /// The arguments' locals and types, in order.
    args: Vec<(SymRef, Type)>,
    calls: HashMap<(usize, usize), Typed>,
    /// The expansion's own locals, bound with their first values around all
    /// of it.
    temps: Vec<(SymRef, Typed)>,
    labels: usize,
}

fn local(name: SymRef, ty: Type) -> Typed {
    Typed { node: Node::Local { name }, ty, loc: None }
}

fn call(piece: u32, args: Vec<Typed>, ty: Type) -> Typed {
    Typed { node: Node::Call { path: Path::internal_builtin(piece), targs: Vec::new(), args }, ty, loc: None }
}

fn set(name: SymRef, value: Typed) -> Typed {
    Typed { node: Node::Set { name, value: Box::new(value) }, ty: Type::Unit, loc: None }
}

fn as_sexpr(value: Typed) -> Typed {
    Typed { node: Node::AsSexpr { value: Box::new(value) }, ty: sexpr_ty(), loc: None }
}

/// A method of `i32` on two `i32`s.
fn int_op(op: u32, a: Typed, b: Typed, ty: Type) -> Typed {
    let node = Node::MethodCall { owner: Path::primitive("i32"), method: well_known_symbol(op), instance: true, targs: Vec::new(), args: vec![a, b] };
    Typed { node, ty, loc: None }
}

fn block(label: Box<str>, body: Vec<Typed>, g: &mut Gen) -> Result<Typed, Error> {
    let body = g.seq_of(body)?;
    Ok(Typed { node: Node::Block { label, body: Box::new(body) }, ty: Type::Unit, loc: None })
}

fn return_from(label: &str) -> Typed {
    Typed { node: Node::ReturnFrom { label: label.into(), value: None }, ty: Type::Never, loc: None }
}

fn looping(body: Vec<Typed>) -> Typed {
    Typed { node: Node::Loop { body }, ty: Type::Unit, loc: None }
}

impl Gen<'_, '_, '_> {
    fn checked(&mut self, v: Value, ty: Type) -> Result<Typed, Error> {
        check_expr(self.cx, self.heap, self.locals, v, Some(&ty), None)
    }

    fn int(&mut self, n: i64) -> Result<Typed, Error> {
        self.checked(Value::Int(n), Type::I32)
    }

    fn flag(&mut self, b: bool) -> Result<Typed, Error> {
        self.checked(Value::Bool(b), Type::Bool)
    }

    /// A string literal, rooted until the top-level form is checked.
    fn string(&mut self, s: &str) -> Result<Typed, Error> {
        let v = self.heap.alloc_string(s.to_string());
        self.heap.push_root(v);
        self.checked(v, Type::Str)
    }

    fn unit(&mut self) -> Result<Typed, Error> {
        self.checked(Value::Empty, Type::Unit)
    }

    fn out(&self) -> Typed {
        local(self.o, format_out_ty())
    }

    fn label(&mut self) -> Box<str> {
        self.labels += 1;
        format!("format''{}", self.labels).into()
    }

    /// A fresh local, bound to `init` around the whole expansion.
    fn temp(&mut self, init: Typed) -> SymRef {
        let name = self.cx.temp();
        self.temps.push((name, init));
        name
    }

    fn int_temp(&mut self) -> Result<SymRef, Error> {
        let zero = self.int(0)?;
        Ok(self.temp(zero))
    }

    fn list_temp(&mut self) -> SymRef {
        self.temp(Typed::none(sexpr_ty()))
    }

    fn list_frame(&mut self) -> Result<Frame, Error> {
        let whole = self.list_temp();
        let rest = self.list_temp();
        let idx = self.int_temp()?;
        Ok(Frame::List { whole, rest, idx })
    }

    /// `body` as one expression, `()` when it is empty.
    fn seq_of(&mut self, mut body: Vec<Typed>) -> Result<Typed, Error> {
        if body.is_empty() {
            body.push(self.unit()?);
        }
        let ty = body.last().map(|t| t.ty.clone()).expect("not empty");
        Ok(Typed { node: Node::Let { binds: Vec::new(), body }, ty, loc: None })
    }

    fn when(&mut self, cond: Typed, then: Vec<Typed>) -> Result<Typed, Error> {
        let then = self.seq_of(then)?;
        let els = self.unit()?;
        Ok(Typed { node: Node::If { cond: Box::new(cond), then: Box::new(then), els: Box::new(els) }, ty: Type::Unit, loc: None })
    }

    fn if_else(&mut self, cond: Typed, then: Vec<Typed>, els: Vec<Typed>) -> Result<Typed, Error> {
        let then = self.seq_of(then)?;
        let els = self.seq_of(els)?;
        Ok(Typed { node: Node::If { cond: Box::new(cond), then: Box::new(then), els: Box::new(els) }, ty: Type::Unit, loc: None })
    }

    fn and(&mut self, a: Typed, b: Typed) -> Result<Typed, Error> {
        let f = self.flag(false)?;
        Ok(Typed { node: Node::If { cond: Box::new(a), then: Box::new(b), els: Box::new(f) }, ty: Type::Bool, loc: None })
    }

    fn panic(&mut self, message: &str) -> Result<Typed, Error> {
        let message = self.string(message)?;
        Ok(Typed { node: Node::Panic { message: Box::new(message) }, ty: Type::Never, loc: None })
    }

    fn text(&mut self, s: &str, out: &mut Vec<Typed>) -> Result<(), Error> {
        if s.is_empty() {
            return Ok(());
        }
        let s = self.string(s)?;
        out.push(call(wk::F_FMT_TEXT, vec![self.out(), s], Type::Unit));
        Ok(())
    }

    // ---- consuming arguments ----

    /// The next argument as a `Sexpr`, and the statements that move the
    /// cursor past it, to run after the value is used.
    fn next(&mut self, frame: &mut Frame) -> Result<(Typed, Vec<Typed>), Error> {
        match frame {
            Frame::Fixed { pos } => {
                let (name, ty) = self.args[*pos].clone();
                *pos += 1;
                Ok((as_sexpr(local(name, ty)), Vec::new()))
            }
            Frame::List { rest, idx, .. } => {
                let (rest, idx) = (*rest, *idx);
                let value = call(wk::F_FMT_FIRST, vec![local(rest, option_of_sexpr())], sexpr_ty());
                Ok((value, self.advance(rest, idx)?))
            }
        }
    }

    fn advance(&mut self, rest: SymRef, idx: SymRef) -> Result<Vec<Typed>, Error> {
        let one = self.int(1)?;
        Ok(vec![
            set(rest, call(wk::F_FMT_REST, vec![local(rest, option_of_sexpr())], option_of_sexpr())),
            set(idx, int_op(wk::M_OP_ADD, local(idx, Type::I32), one, Type::I32)),
        ])
    }

    /// How many arguments are left.
    fn remaining(&mut self, frame: &Frame) -> Result<Typed, Error> {
        match frame {
            Frame::Fixed { pos } => self.int((self.args.len() - *pos) as i64),
            Frame::List { rest, .. } => {
                let what = self.string("the argument list")?;
                Ok(call(wk::F_FMT_LENGTH, vec![local(*rest, option_of_sexpr()), what], Type::I32))
            }
        }
    }

    /// `head`'s parameters as `i32` expressions, `defaults` filling the
    /// omitted ones (`None` stays omitted). The `v` and `#` ones are read
    /// into locals by the statements pushed onto `out`, in order.
    fn params(&mut self, frame: &mut Frame, head: &Head, defaults: &[Option<i64>], out: &mut Vec<Typed>) -> Result<Vec<Option<Typed>>, Error> {
        let mut vals = Vec::new();
        for (i, (p, kind)) in head.params.iter().zip(head.kinds).enumerate() {
            let code = match kind {
                ParamKind::NonNeg => PARAM_NON_NEG,
                ParamKind::Pos => PARAM_POS,
                ParamKind::Char => PARAM_CHAR,
                ParamKind::Radix => PARAM_RADIX,
                ParamKind::Int | ParamKind::Unsupported(_) => PARAM_INT,
            };
            vals.push(match p {
                Param::Int(n) => Some(self.int(*n)?),
                Param::Char(c) => Some(self.int(*c as i64)?),
                Param::Default => match defaults.get(i).copied().flatten() {
                    Some(n) => Some(self.int(n)?),
                    None => None,
                },
                Param::Count if matches!(frame, Frame::Fixed { .. }) => Some(self.remaining(frame)?),
                Param::Count | Param::Arg => {
                    let read = if matches!(p, Param::Count) {
                        (as_sexpr(self.remaining(frame)?), Vec::new())
                    } else {
                        self.next(frame)?
                    };
                    let (kind_code, index) = (self.int(code)?, self.int(i as i64 + 1)?);
                    let name = self.int_temp()?;
                    out.push(set(name, call(wk::F_FMT_PARAM, vec![read.0, kind_code, index], Type::I32)));
                    out.extend(read.1);
                    Some(local(name, Type::I32))
                }
            });
        }
        for d in defaults.iter().skip(head.params.len()) {
            vals.push(match d {
                Some(n) => Some(self.int(*n)?),
                None => None,
            });
        }
        Ok(vals)
    }

    /// Parameter `i`, or `fallback` when it is omitted.
    fn param_or(&mut self, vals: &mut [Option<Typed>], i: usize, fallback: i64) -> Result<Typed, Error> {
        match vals.get_mut(i).and_then(Option::take) {
            Some(t) => Ok(t),
            None => self.int(fallback),
        }
    }

    // ---- the fixed shape ----

    /// `nodes` in a frame at fixed positions. True when a `~^` stopped them.
    fn seq(&mut self, nodes: &[Directive], frame: &mut Frame, out: &mut Vec<Typed>) -> Result<bool, Error> {
        let Frame::Fixed { .. } = frame else { unreachable!("seq runs in a fixed frame") };
        for node in nodes {
            match node {
                Directive::Escape { head } => {
                    // Known here: a `~v^` takes the list shape.
                    let Frame::Fixed { pos } = frame else { unreachable!() };
                    let present = fixed_params(head, *pos, self.args.len(), true).expect("the shape was decided as fixed").1;
                    if escape_fires(&present, self.args.len() - *pos) {
                        return Ok(true);
                    }
                }
                Directive::Case { head, body } => {
                    out.push(call(wk::F_FMT_CASE_OPEN, vec![self.out()], Type::Unit));
                    let escaped = self.seq(body, frame, out)?;
                    let (c, a) = (self.flag(head.colon)?, self.flag(head.at)?);
                    out.push(call(wk::F_FMT_CASE_CLOSE, vec![self.out(), c, a], Type::Unit));
                    if escaped {
                        return Ok(true);
                    }
                }
                Directive::Cond { head, clauses, default } => {
                    let (selector, after) = self.selector(frame, head, out)?;
                    let start = frame.clone();
                    let mut branches = Vec::new();
                    let mut escaped = false;
                    for clause in clauses {
                        let mut f = start.clone();
                        let mut body = Vec::new();
                        escaped = self.seq(clause, &mut f, &mut body)?;
                        *frame = f;
                        branches.push(body);
                    }
                    if clauses.is_empty() {
                        *frame = start;
                    }
                    let chosen = self.choose(head, selector, after, branches, *default)?;
                    out.extend(chosen);
                    if escaped {
                        return Ok(true);
                    }
                }
                _ => self.common(node, frame, out)?,
            }
        }
        Ok(false)
    }

    // ---- the list shape ----

    /// `nodes` in a frame walked at run time, a `~^` leaving by `esc`.
    fn seq_list(&mut self, nodes: &[Directive], frame: &mut Frame, esc: &mut Escape, out: &mut Vec<Typed>) -> Result<(), Error> {
        for node in nodes {
            match node {
                Directive::Escape { head } => {
                    let mut vals = {
                        let mut stmts = Vec::new();
                        let vals = self.params(frame, head, &[], &mut stmts)?;
                        out.extend(stmts);
                        vals
                    };
                    let present: Vec<Typed> = vals.iter_mut().filter_map(Option::take).collect();
                    let fires = match present.len() {
                        0 => {
                            let Frame::List { rest, .. } = frame else { unreachable!("seq_list runs in a list frame") };
                            call(wk::F_FMT_EMPTY, vec![local(*rest, option_of_sexpr())], Type::Bool)
                        }
                        1 => {
                            let zero = self.int(0)?;
                            int_op(wk::RETIRED_EQUALS, present.into_iter().next().expect("one"), zero, Type::Bool)
                        }
                        2 => {
                            let mut it = present.into_iter();
                            let (a, b) = (it.next().expect("two"), it.next().expect("two"));
                            int_op(wk::RETIRED_EQUALS, a, b, Type::Bool)
                        }
                        _ => {
                            // Each read once: the middle one is compared twice.
                            let mut it = present.into_iter();
                            let (a, b, c) = (it.next().expect("three"), it.next().expect("three"), it.next().expect("three"));
                            let m = self.int_temp()?;
                            out.push(set(m, b));
                            let lo = int_op(wk::M_OP_LE, a, local(m, Type::I32), Type::Bool);
                            let hi = int_op(wk::M_OP_LE, local(m, Type::I32), c, Type::Bool);
                            self.and(lo, hi)?
                        }
                    };
                    let leave = self.leave(esc)?;
                    out.push(self.when(fires, leave)?);
                }
                Directive::Case { head, body } => {
                    out.push(call(wk::F_FMT_CASE_OPEN, vec![self.out()], Type::Unit));
                    esc.closes.push((head.colon, head.at));
                    self.seq_list(body, frame, esc, out)?;
                    esc.closes.pop();
                    let (c, a) = (self.flag(head.colon)?, self.flag(head.at)?);
                    out.push(call(wk::F_FMT_CASE_CLOSE, vec![self.out(), c, a], Type::Unit));
                }
                Directive::Cond { head, clauses, default } => {
                    let (selector, after) = self.selector(frame, head, out)?;
                    let mut branches = Vec::new();
                    for clause in clauses {
                        let mut body = Vec::new();
                        self.seq_list(clause, frame, esc, &mut body)?;
                        branches.push(body);
                    }
                    let chosen = self.choose(head, selector, after, branches, *default)?;
                    out.extend(chosen);
                }
                _ => self.common(node, frame, out)?,
            }
        }
        Ok(())
    }

    /// The statements that leave by `esc`: the `~(` buffers opened inside it
    /// closed, innermost first, then out of its block.
    fn leave(&mut self, esc: &Escape) -> Result<Vec<Typed>, Error> {
        let mut stmts = Vec::new();
        for (colon, at) in esc.closes.iter().rev() {
            let (c, a) = (self.flag(*colon)?, self.flag(*at)?);
            stmts.push(call(wk::F_FMT_CASE_CLOSE, vec![self.out(), c, a], Type::Unit));
        }
        stmts.push(return_from(&esc.label));
        Ok(stmts)
    }

    // ---- `~[` ----

    /// What selects a `~[`'s clause, read into a local, and the statements
    /// that move the cursor past what it read.
    fn selector(&mut self, frame: &mut Frame, head: &Head, out: &mut Vec<Typed>) -> Result<(Typed, Vec<Typed>), Error> {
        if head.colon {
            let (v, after) = self.next(frame)?;
            let init = self.flag(false)?;
            let b = self.temp(init);
            out.push(set(b, call(wk::F_FMT_BOOL, vec![v], Type::Bool)));
            out.extend(after);
            return Ok((local(b, Type::Bool), Vec::new()));
        }
        let n = self.int_temp()?;
        if head.params.is_empty() {
            let (v, after) = self.next(frame)?;
            out.push(set(n, call(wk::F_FMT_SELECT, vec![v], Type::I32)));
            out.extend(after);
        } else {
            let mut vals = self.params(frame, head, &[], out)?;
            let v = self.param_or(&mut vals, 0, -1)?;
            out.push(set(n, v));
        }
        Ok((local(n, Type::I32), Vec::new()))
    }

    /// The chosen clause of a `~[`, `selector` being what [`Self::selector`]
    /// read.
    fn choose(&mut self, head: &Head, selector: Typed, after: Vec<Typed>, mut branches: Vec<Vec<Typed>>, default: Option<usize>) -> Result<Vec<Typed>, Error> {
        let mut out = after;
        if head.colon {
            let mut take = |i: usize| if i < branches.len() { std::mem::take(&mut branches[i]) } else { Vec::new() };
            let (t, f) = (take(1), take(0));
            out.push(self.if_else(selector, t, f)?);
            return Ok(out);
        }
        let Node::Local { name } = selector.node else { unreachable!("a selector is a local") };
        let otherwise = match default {
            Some(d) => std::mem::take(&mut branches[d]),
            None => Vec::new(),
        };
        let mut chain = otherwise;
        for i in (0..branches.len()).rev() {
            if Some(i) == default {
                continue;
            }
            let k = self.int(i as i64)?;
            let test = int_op(wk::RETIRED_EQUALS, local(name, Type::I32), k, Type::Bool);
            let then = std::mem::take(&mut branches[i]);
            chain = vec![self.if_else(test, then, chain)?];
        }
        out.extend(chain);
        Ok(out)
    }

    // ---- the directives both shapes write the same way ----

    fn common(&mut self, node: &Directive, frame: &mut Frame, out: &mut Vec<Typed>) -> Result<(), Error> {
        match node {
            Directive::Text(t) => self.text(t, out),
            Directive::Dir { head, ch } => self.directive(head, *ch, frame, out),
            Directive::Call { head, span, .. } => self.call_directive(head, *span, frame, out),
            Directive::Iter { head, body, close_colon } => self.iteration(head, body, *close_colon, frame, out),
            Directive::Just { head, segments } => self.justification(head, segments, frame, out),
            Directive::Block { head, segments, sep_at } => self.logical_block(head, segments, sep_at, frame, out),
            Directive::Escape { .. } | Directive::Case { .. } | Directive::Cond { .. } => {
                unreachable!("each shape writes these itself")
            }
        }
    }

    /// `nodes` in `frame`, whichever shape it is, a `~^` that fires at run
    /// time leaving by `esc`. True when a `~^` stopped them here.
    fn run(&mut self, nodes: &[Directive], frame: &mut Frame, esc: &mut Escape, out: &mut Vec<Typed>) -> Result<bool, Error> {
        match frame {
            Frame::Fixed { .. } => self.seq(nodes, frame, out),
            Frame::List { .. } => self.seq_list(nodes, frame, esc, out).map(|()| false),
        }
    }

    fn directive(&mut self, head: &Head, ch: char, frame: &mut Frame, out: &mut Vec<Typed>) -> Result<(), Error> {
        match ch {
            'a' | 's' | 'w' => {
                let defaults = if ch == 'w' { vec![] } else { vec![Some(0), Some(1), Some(0), Some(' ' as i64)] };
                let mut vals = self.params(frame, head, &defaults, out)?;
                let (v, after) = self.next(frame)?;
                let escape = self.flag(ch != 'a')?;
                let padded = self.flag(!head.params.is_empty())?;
                let mut args = vec![self.out(), v, escape, padded];
                for (i, d) in [(0, 0), (1, 1), (2, 0), (3, ' ' as i64)] {
                    args.push(self.param_or(&mut vals, i, d)?);
                }
                args.push(self.flag(head.at)?);
                out.push(call(wk::F_FMT_A, args, Type::Unit));
                out.extend(after);
            }
            'd' | 'b' | 'o' | 'x' => {
                let radix = match ch {
                    'b' => 2,
                    'o' => 8,
                    'x' => 16,
                    _ => 10,
                };
                let mut vals = self.params(frame, head, &[Some(0), None, None, Some(3)], out)?;
                let radix = self.int(radix)?;
                self.integer(radix, &mut vals, 0, head, frame, out)?;
            }
            'r' if !head.params.is_empty() => {
                let mut vals = self.params(frame, head, &[None, Some(0), None, None, Some(3)], out)?;
                let radix = self.param_or(&mut vals, 0, 10)?;
                self.integer(radix, &mut vals, 1, head, frame, out)?;
            }
            'r' => {
                let (v, after) = self.next(frame)?;
                let (c, a) = (self.flag(head.colon)?, self.flag(head.at)?);
                out.push(call(wk::F_FMT_WORDS, vec![self.out(), v, c, a], Type::Unit));
                out.extend(after);
            }
            'c' => {
                let (v, after) = self.next(frame)?;
                let (c, a) = (self.flag(head.colon)?, self.flag(head.at)?);
                out.push(call(wk::F_FMT_CHAR, vec![self.out(), v, c, a], Type::Unit));
                out.extend(after);
            }
            'p' => {
                if head.colon {
                    self.back_up(frame, out)?;
                }
                let (v, after) = self.next(frame)?;
                let a = self.flag(head.at)?;
                out.push(call(wk::F_FMT_PLURAL, vec![self.out(), v, a], Type::Unit));
                out.extend(after);
            }
            'f' => {
                let mut vals = self.params(frame, head, &[None, None, Some(0), None, Some(' ' as i64)], out)?;
                let (v, after) = self.next(frame)?;
                let mut args = vec![self.out(), v];
                for (i, d) in [(0, -1), (1, -1), (2, 0), (3, -1), (4, ' ' as i64)] {
                    args.push(self.param_or(&mut vals, i, d)?);
                }
                args.push(self.flag(head.at)?);
                out.push(call(wk::F_FMT_FIXED, args, Type::Unit));
                out.extend(after);
            }
            'e' => {
                let mut vals = self.params(frame, head, &[None, None, None, Some(1), None, Some(' ' as i64), None], out)?;
                let (v, after) = self.next(frame)?;
                let mut args = vec![self.out(), v];
                for (i, d) in [(0, -1), (1, 6), (6, 'e' as i64), (5, ' ' as i64)] {
                    args.push(self.param_or(&mut vals, i, d)?);
                }
                args.push(self.flag(head.at)?);
                out.push(call(wk::F_FMT_EXPONENT, args, Type::Unit));
                out.extend(after);
            }
            'g' => {
                let (v, after) = self.next(frame)?;
                let a = self.flag(head.at)?;
                out.push(call(wk::F_FMT_GENERAL, vec![self.out(), v, a], Type::Unit));
                out.extend(after);
            }
            '$' => {
                let mut vals = self.params(frame, head, &[Some(2), Some(1), Some(0), Some(' ' as i64)], out)?;
                let (v, after) = self.next(frame)?;
                let mut args = vec![self.out(), v];
                for (i, d) in [(0, 2), (1, 1), (2, 0), (3, ' ' as i64)] {
                    args.push(self.param_or(&mut vals, i, d)?);
                }
                let (c, a) = (self.flag(head.colon)?, self.flag(head.at)?);
                args.push(c);
                args.push(a);
                out.push(call(wk::F_FMT_DOLLARS, args, Type::Unit));
                out.extend(after);
            }
            '%' | '|' | '~' => {
                let unit = match ch {
                    '%' => "\n",
                    '|' => "\u{000c}",
                    _ => "~",
                };
                let mut vals = self.params(frame, head, &[Some(1)], out)?;
                let count = vals[0].take().expect("defaulted");
                match count.node {
                    Node::Literal { value: Some(Value::Int(n)), .. } => self.text(&unit.repeat(usize::try_from(n).unwrap_or(0)), out)?,
                    _ => self.repeat_text(count, unit, out)?,
                }
            }
            '&' => {
                let mut vals = self.params(frame, head, &[Some(1)], out)?;
                let n = vals[0].take().expect("defaulted");
                out.push(call(wk::F_FMT_FRESH_LINE, vec![self.out(), n], Type::Unit));
            }
            't' => {
                let mut vals = self.params(frame, head, &[Some(1), Some(1)], out)?;
                let (col, inc) = (self.param_or(&mut vals, 0, 1)?, self.param_or(&mut vals, 1, 1)?);
                let (c, a) = (self.flag(head.colon)?, self.flag(head.at)?);
                out.push(call(wk::F_FMT_TAB, vec![self.out(), col, inc, c, a], Type::Unit));
            }
            '_' => {
                let kind = match (head.colon, head.at) {
                    (false, false) => 0,
                    (true, false) => 1,
                    (false, true) => 2,
                    (true, true) => 3,
                };
                let kind = self.int(kind)?;
                out.push(call(wk::F_FMT_NEWLINE, vec![self.out(), kind], Type::Unit));
            }
            'i' => {
                let mut vals = self.params(frame, head, &[Some(0)], out)?;
                let n = self.param_or(&mut vals, 0, 0)?;
                let current = self.flag(head.colon)?;
                out.push(call(wk::F_FMT_INDENT, vec![self.out(), current, n], Type::Unit));
            }
            '*' => self.goto(head, frame, out)?,
            other => return Err(Error::TypeError(format!("format: unknown directive ~{}", other))),
        }
        Ok(())
    }

    /// `~D` and its kin once their parameters are read: `vals` from `at` on
    /// are the mincol, padchar, commachar and interval.
    fn integer(&mut self, radix: Typed, vals: &mut [Option<Typed>], at: usize, head: &Head, frame: &mut Frame, out: &mut Vec<Typed>) -> Result<(), Error> {
        let (v, after) = self.next(frame)?;
        let mut args = vec![self.out(), v, radix];
        for (i, d) in [(at, 0), (at + 1, ' ' as i64), (at + 2, ',' as i64), (at + 3, 3)] {
            args.push(self.param_or(vals, i, d)?);
        }
        let (c, a) = (self.flag(head.colon)?, self.flag(head.at)?);
        args.push(c);
        args.push(a);
        out.push(call(wk::F_FMT_D, args, Type::Unit));
        out.extend(after);
        Ok(())
    }

    /// `text` written `count` times, `count` known only when the program runs.
    fn repeat_text(&mut self, count: Typed, text: &str, out: &mut Vec<Typed>) -> Result<(), Error> {
        let n = self.int_temp()?;
        out.push(set(n, count));
        let label = self.label();
        let zero = self.int(0)?;
        let done = int_op(wk::M_OP_LE, local(n, Type::I32), zero, Type::Bool);
        let leave = self.when(done, vec![return_from(&label)])?;
        let mut body = vec![leave];
        self.text(text, &mut body)?;
        let one = self.int(1)?;
        body.push(set(n, int_op(wk::M_OP_SUB, local(n, Type::I32), one, Type::I32)));
        let lp = looping(body);
        out.push(block(label, vec![lp], self)?);
        Ok(())
    }

    /// `~:P`'s step back to the argument before.
    fn back_up(&mut self, frame: &mut Frame, out: &mut Vec<Typed>) -> Result<(), Error> {
        match frame {
            Frame::Fixed { pos } => {
                *pos -= 1;
                Ok(())
            }
            Frame::List { whole, rest, idx } => {
                let (whole, rest, idx) = (*whole, *rest, *idx);
                let one = self.int(1)?;
                out.push(set(idx, int_op(wk::M_OP_SUB, local(idx, Type::I32), one, Type::I32)));
                let backup = self.flag(true)?;
                out.push(set(rest, call(wk::F_FMT_GOTO, vec![local(whole, option_of_sexpr()), local(idx, Type::I32), backup], option_of_sexpr())));
                Ok(())
            }
        }
    }

    /// `~*`: skip forward (`~n*`), back (`~n:*`) or to an argument (`~n@*`).
    fn goto(&mut self, head: &Head, frame: &mut Frame, out: &mut Vec<Typed>) -> Result<(), Error> {
        let mut vals = self.params(frame, head, &[], out)?;
        match frame {
            Frame::Fixed { pos } => {
                let k = match head.params.first() {
                    Some(Param::Int(k)) => *k,
                    Some(Param::Count) => (self.args.len() - *pos) as i64,
                    _ if head.at => 0,
                    _ => 1,
                };
                let target = if head.at {
                    k
                } else if head.colon {
                    *pos as i64 - k
                } else {
                    *pos as i64 + k
                };
                *pos = usize::try_from(target).expect("the shape was decided as fixed");
            }
            Frame::List { whole, rest, idx } => {
                let (whole, rest, idx) = (*whole, *rest, *idx);
                let k = self.param_or(&mut vals, 0, if head.at { 0 } else { 1 })?;
                let target = if head.at {
                    k
                } else if head.colon {
                    int_op(wk::M_OP_SUB, local(idx, Type::I32), k, Type::I32)
                } else {
                    int_op(wk::M_OP_ADD, local(idx, Type::I32), k, Type::I32)
                };
                out.push(set(idx, target));
                let backup = self.flag(false)?;
                out.push(set(rest, call(wk::F_FMT_GOTO, vec![local(whole, option_of_sexpr()), local(idx, Type::I32), backup], option_of_sexpr())));
            }
        }
        Ok(())
    }

    /// `~/name/`: the call the checker checked for the argument it is handed.
    fn call_directive(&mut self, head: &Head, span: (usize, usize), frame: &mut Frame, out: &mut Vec<Typed>) -> Result<(), Error> {
        let _ = head;
        match frame {
            Frame::Fixed { pos } => {
                let k = *pos;
                *pos += 1;
                let checked = self.call_for(span, k)?;
                out.push(call(wk::F_FMT_TEXT, vec![self.out(), checked], Type::Unit));
            }
            Frame::List { rest, idx, .. } => {
                let (rest, idx) = (*rest, *idx);
                let mut indices: Vec<usize> = self.calls.keys().filter(|(s, _)| *s == span.0).map(|(_, i)| *i).collect();
                indices.sort_unstable();
                let Some(last) = indices.pop() else {
                    return Err(Error::TypeError("format: internal error: a ~/name/ directive no argument reaches".to_string()));
                };
                let call_last = self.call_for(span, last)?;
                let mut chain = vec![call(wk::F_FMT_TEXT, vec![self.out(), call_last], Type::Unit)];
                for k in indices.into_iter().rev() {
                    let checked = self.call_for(span, k)?;
                    let write = call(wk::F_FMT_TEXT, vec![self.out(), checked], Type::Unit);
                    let kk = self.int(k as i64)?;
                    let test = int_op(wk::RETIRED_EQUALS, local(idx, Type::I32), kk, Type::Bool);
                    chain = vec![self.if_else(test, vec![write], chain)?];
                }
                out.extend(chain);
                out.extend(self.advance(rest, idx)?);
            }
        }
        Ok(())
    }

    /// The checked call of the `~/name/` at `span` for argument `k`, handed
    /// that argument.
    fn call_for(&mut self, span: (usize, usize), k: usize) -> Result<Typed, Error> {
        let checked = self
            .calls
            .remove(&(span.0, k))
            .ok_or_else(|| Error::TypeError(format!("format: internal error: no checked call for ~/name/ with argument {}", k + 1)))?;
        let (name, ty) = self.args[k].clone();
        let binds = vec![(super::format::call_arg(), local(name, ty))];
        Ok(Typed { node: Node::Let { binds, body: vec![checked] }, ty: Type::Str, loc: None })
    }

    // ---- `~{…~}` ----

    fn iteration(&mut self, head: &Head, body: &[Directive], close_colon: bool, frame: &mut Frame, out: &mut Vec<Typed>) -> Result<(), Error> {
        let mut vals = self.params(frame, head, &[], out)?;
        let max = vals.first_mut().and_then(Option::take);
        let max = match max {
            Some(m) => {
                let name = self.int_temp()?;
                out.push(set(name, m));
                Some(name)
            }
            None => None,
        };
        let label = self.label();
        let mut esc = Escape { label: label.clone(), closes: Vec::new() };
        let mut inner = Vec::new();
        if head.colon {
            // Each iteration binds one sublist: of a list argument (`~:{`),
            // or of the remaining arguments (`~:@{`).
            let lists = self.list_temp();
            let what = if head.at { "an argument of ~:@{" } else { "an element of ~:{'s argument" };
            if head.at {
                let Frame::List { rest, idx, .. } = frame else { unreachable!("~:@{{ over fixed positions takes the list shape") };
                let (rest, idx) = (*rest, *idx);
                inner.push(set(lists, local(rest, option_of_sexpr())));
                let n = self.remaining(frame)?;
                inner.push(set(idx, int_op(wk::M_OP_ADD, local(idx, Type::I32), n, Type::I32)));
                inner.push(set(rest, Typed::none(sexpr_ty())));
            } else {
                let v = self.list_argument(frame, "~:{'s argument", out)?;
                inner.push(set(lists, v));
            }
            let sub = self.list_frame()?;
            let Frame::List { whole, rest, idx } = sub.clone() else { unreachable!() };
            let count = self.int_temp()?;
            if close_colon {
                let empty = call(wk::F_FMT_EMPTY, vec![local(lists, option_of_sexpr())], Type::Bool);
                let once = self.body_once(body, &label)?;
                inner.push(self.when(empty, once)?);
            }
            let mut round = Vec::new();
            let empty = call(wk::F_FMT_EMPTY, vec![local(lists, option_of_sexpr())], Type::Bool);
            round.push(self.when(empty, vec![return_from(&label)])?);
            self.max_reached(max, count, &label, &mut round)?;
            let what = self.string(what)?;
            let first = call(wk::F_FMT_FIRST, vec![local(lists, option_of_sexpr())], sexpr_ty());
            round.push(set(whole, call(wk::F_FMT_AS_LIST, vec![first, what], option_of_sexpr())));
            round.push(set(rest, local(whole, option_of_sexpr())));
            let zero = self.int(0)?;
            round.push(set(idx, zero));
            round.push(set(lists, call(wk::F_FMT_REST, vec![local(lists, option_of_sexpr())], option_of_sexpr())));
            let mut sub = sub;
            self.seq_list(body, &mut sub, &mut esc, &mut round)?;
            let one = self.int(1)?;
            round.push(set(count, int_op(wk::M_OP_ADD, local(count, Type::I32), one, Type::I32)));
            inner.push(looping(round));
        } else {
            // One flat list: a list argument (`~{`), or the remaining
            // arguments in place (`~@{`).
            let sub = self.list_frame()?;
            let Frame::List { whole, rest, idx } = sub.clone() else { unreachable!() };
            if head.at {
                let Frame::List { rest: outer_rest, .. } = frame else { unreachable!("~@{{ over fixed positions takes the list shape") };
                inner.push(set(whole, local(*outer_rest, option_of_sexpr())));
            } else {
                let v = self.list_argument(frame, "~{'s argument", out)?;
                inner.push(set(whole, v));
            }
            inner.push(set(rest, local(whole, option_of_sexpr())));
            let zero = self.int(0)?;
            inner.push(set(idx, zero));
            let count = self.int_temp()?;
            if close_colon {
                let empty = call(wk::F_FMT_EMPTY, vec![local(whole, option_of_sexpr())], Type::Bool);
                let once = self.body_once(body, &label)?;
                inner.push(self.when(empty, once)?);
            }
            let mut round = Vec::new();
            let empty = call(wk::F_FMT_EMPTY, vec![local(rest, option_of_sexpr())], Type::Bool);
            round.push(self.when(empty, vec![return_from(&label)])?);
            self.max_reached(max, count, &label, &mut round)?;
            let before = self.int_temp()?;
            round.push(set(before, local(idx, Type::I32)));
            let mut sub = sub;
            self.seq_list(body, &mut sub, &mut esc, &mut round)?;
            let one = self.int(1)?;
            round.push(set(count, int_op(wk::M_OP_ADD, local(count, Type::I32), one, Type::I32)));
            if max.is_none() {
                let stuck = int_op(wk::RETIRED_EQUALS, local(idx, Type::I32), local(before, Type::I32), Type::Bool);
                let fail = self.panic("format: an iteration of ~{ consumed no argument, so it would never end")?;
                round.push(self.when(stuck, vec![fail])?);
            }
            inner.push(looping(round));
            let lp = block(label.clone(), inner, self)?;
            out.push(lp);
            if head.at {
                // The arguments the iteration consumed are behind the outer
                // cursor now.
                let Frame::List { rest: outer_rest, idx: outer_idx, .. } = frame else { unreachable!() };
                let (outer_rest, outer_idx) = (*outer_rest, *outer_idx);
                out.push(set(outer_rest, local(rest, option_of_sexpr())));
                out.push(set(outer_idx, int_op(wk::M_OP_ADD, local(outer_idx, Type::I32), local(idx, Type::I32), Type::I32)));
            }
            return Ok(());
        }
        out.push(block(label, inner, self)?);
        Ok(())
    }

    /// `~:}`'s pass over an empty list: the body once, with no arguments,
    /// then out of the iteration.
    fn body_once(&mut self, body: &[Directive], label: &str) -> Result<Vec<Typed>, Error> {
        let mut once_frame = self.list_frame()?;
        let once_label = self.label();
        let mut once_esc = Escape { label: once_label.clone(), closes: Vec::new() };
        let mut once = Vec::new();
        self.seq_list(body, &mut once_frame, &mut once_esc, &mut once)?;
        Ok(vec![block(once_label, once, self)?, return_from(label)])
    }

    fn max_reached(&mut self, max: Option<SymRef>, count: SymRef, label: &str, out: &mut Vec<Typed>) -> Result<(), Error> {
        if let Some(max) = max {
            let reached = int_op(wk::M_OP_GE, local(count, Type::I32), local(max, Type::I32), Type::Bool);
            out.push(self.when(reached, vec![return_from(label)])?);
        }
        Ok(())
    }

    /// The next argument as a list, `what` naming it when it is not one.
    fn list_argument(&mut self, frame: &mut Frame, what: &str, out: &mut Vec<Typed>) -> Result<Typed, Error> {
        let (v, after) = self.next(frame)?;
        let what = self.string(what)?;
        let name = self.list_temp();
        out.push(set(name, call(wk::F_FMT_AS_LIST, vec![v, what], option_of_sexpr())));
        out.extend(after);
        Ok(local(name, option_of_sexpr()))
    }

    // ---- `~<…~>` ----

    fn justification(&mut self, head: &Head, segments: &[Vec<Directive>], frame: &mut Frame, out: &mut Vec<Typed>) -> Result<(), Error> {
        let mut vals = self.params(frame, head, &[Some(0), Some(1), Some(0), Some(' ' as i64)], out)?;
        out.push(call(wk::F_FMT_JUST_OPEN, vec![self.out()], Type::Unit));
        let label = self.label();
        let mut seg_esc = Escape { label: label.clone(), closes: Vec::new() };
        let mut inner = Vec::new();
        for segment in segments {
            inner.push(call(wk::F_FMT_JUST_SEGMENT, vec![self.out()], Type::Unit));
            if self.run(segment, frame, &mut seg_esc, &mut inner)? {
                break;
            }
        }
        out.push(block(label, inner, self)?);
        let mut args = vec![self.out()];
        for (i, d) in [(0, 0), (1, 1), (2, 0), (3, ' ' as i64)] {
            args.push(self.param_or(&mut vals, i, d)?);
        }
        let (c, a) = (self.flag(head.colon)?, self.flag(head.at)?);
        args.push(c);
        args.push(a);
        out.push(call(wk::F_FMT_JUST_CLOSE, args, Type::Unit));
        Ok(())
    }

    // ---- `~<…~:>` ----

    fn logical_block(&mut self, head: &Head, segments: &[Vec<Directive>], sep_at: &[bool], frame: &mut Frame, out: &mut Vec<Typed>) -> Result<(), Error> {
        let (default_prefix, default_suffix) = if head.colon { ("(", ")") } else { ("", "") };
        let literal = |nodes: &[Directive]| engine::literal_segment(nodes).map_err(Error::TypeError);
        let (prefix, suffix, per_line) = match segments.len() {
            0 | 1 => (default_prefix.to_string(), default_suffix.to_string(), false),
            2 => (literal(&segments[0])?, default_suffix.to_string(), sep_at.first() == Some(&true)),
            _ => (literal(&segments[0])?, literal(&segments[segments.len() - 1])?, sep_at.first() == Some(&true)),
        };
        // `~:<` consumes its list before the block opens, as the engine did.
        let list = if head.at { None } else { Some(self.list_argument(frame, "a logical block's argument", out)?) };
        let (p, pl, s) = (self.string(&prefix)?, self.flag(per_line)?, self.string(&suffix)?);
        out.push(call(wk::F_FMT_BLOCK_OPEN, vec![self.out(), p, pl, s], Type::Unit));
        if let Some(body) = block_body(segments) {
            let label = self.label();
            let mut esc = Escape { label: label.clone(), closes: Vec::new() };
            let mut inner = Vec::new();
            match list {
                // In place: the body consumes this frame's arguments.
                None => {
                    self.run(body, frame, &mut esc, &mut inner)?;
                }
                Some(list) => {
                    let mut sub = self.list_frame()?;
                    let Frame::List { whole, rest, idx } = sub.clone() else { unreachable!() };
                    inner.push(set(whole, list));
                    inner.push(set(rest, local(whole, option_of_sexpr())));
                    let zero = self.int(0)?;
                    inner.push(set(idx, zero));
                    self.seq_list(body, &mut sub, &mut esc, &mut inner)?;
                }
            }
            out.push(block(label, inner, self)?);
        }
        let s = self.string(&suffix)?;
        out.push(call(wk::F_FMT_BLOCK_CLOSE, vec![self.out(), s], Type::Unit));
        Ok(())
    }
}
