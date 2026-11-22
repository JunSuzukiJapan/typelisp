//! The pieces an expanded `format`/`print`/`println` is made of.
//!
//! A control string is a literal, so the checker expands each call into a
//! sequence of calls of these (`typeck::format_expand`): nothing reads a
//! control string at run time. Each directive is one function here, called
//! only by the code a directive of its kind expanded to, so a program links
//! the renderers its control strings use and no others.
//!
//! The output goes to a [`FormatBuf`], held in a native box of the type
//! `format-out` ([`TypeId::FORMAT_OUT`]). Being a heap value is what lets a
//! `throw` out of the middle of a `format` leave its buffer to the collector.
//!
//! The functions that walk a list of arguments (`~{…~}`, and a control
//! string whose cursor is decided at run time) take no buffer: the list and
//! the cursor are the expansion's own locals, so the collector sees them.

use typelisp_mem::{Heap, TypeId, Value};

use super::{
    apply_case, current_column, decorate_integer, english_cardinal, english_ordinal, float_of, format_char, int_of,
    int_to_radix, justify, left_pad, pad, render_number, render_value, roman, trim_f64, PrintVars, RenderCtx,
};
use crate::pprint::{self, IndentKind, NewlineKind, Op, Opts, Out, Style, TabKind};
use crate::runtime::{current_opts, current_print_vars, emit, RtPrintEnv};

/// What one `format` writes into.
///
/// `stack` holds the buffer being written (the last) above the ones it will
/// be merged into: `~(…~)` writes into a buffer of its own and merges it
/// converted, and `~<…~>` collects one buffer per segment above a
/// [`Entry::Just`] mark. The printer control variables are read once, when
/// the buffer is opened, as the engine read them once per operation.
pub(crate) struct FormatBuf {
    stack: Vec<Entry>,
    opts: Opts,
    vars: PrintVars,
}

enum Entry {
    Buf(Out),
    /// Where a justification's segments start.
    Just,
}

impl FormatBuf {
    fn top(&mut self) -> Result<&mut Out, String> {
        match self.stack.last_mut() {
            Some(Entry::Buf(out)) => Ok(out),
            _ => Err(internal("no buffer is open to write into")),
        }
    }

    fn pop(&mut self) -> Result<Out, String> {
        match self.stack.pop() {
            Some(Entry::Buf(out)) => Ok(out),
            _ => Err(internal("closed a buffer that was not open")),
        }
    }
}

fn internal(what: &str) -> String {
    format!("format: internal error: {}", what)
}

/// Runs `f` on the buffer `o`.
fn with<R>(heap: &Heap, o: Value, f: impl FnOnce(&mut FormatBuf) -> Result<R, String>) -> Result<R, String> {
    let Value::Boxed(id) = o else { return Err(internal("not a format buffer")) };
    heap.native_with::<FormatBuf, _>(id, f).ok_or_else(|| internal("not a format buffer"))?
}

fn settings(heap: &Heap, o: Value) -> Result<(Opts, PrintVars), String> {
    with(heap, o, |b| Ok((b.opts, b.vars)))
}

fn push_str(heap: &Heap, o: Value, s: &str) -> Result<(), String> {
    with(heap, o, |b| {
        b.top()?.push_str(s);
        Ok(())
    })
}

fn append(heap: &Heap, o: Value, piece: Out) -> Result<(), String> {
    with(heap, o, |b| {
        b.top()?.append(piece);
        Ok(())
    })
}

fn op(heap: &Heap, o: Value, pretty_op: impl FnOnce() -> Op) -> Result<(), String> {
    with(heap, o, |b| {
        if b.opts.pretty {
            let op = pretty_op();
            b.top()?.op(op);
        }
        Ok(())
    })
}

fn padchar(code: i64) -> Result<char, String> {
    u32::try_from(code).ok().and_then(char::from_u32).ok_or_else(|| internal("a character parameter is not a character"))
}

fn usize_of(n: i64) -> usize {
    usize::try_from(n).unwrap_or(0)
}

/// The padding `~mincol,colinc,minpad,padcharA` asks for, applied when the
/// directive was written with parameters.
pub struct Padding {
    pub padded: bool,
    pub mincol: i64,
    pub colinc: i64,
    pub minpad: i64,
    pub padchar: i64,
    pub at: bool,
}

impl Padding {
    fn apply(&self, text: String) -> Result<String, String> {
        if !self.padded {
            return Ok(text);
        }
        Ok(pad(&text, usize_of(self.mincol), usize_of(self.colinc), usize_of(self.minpad), padchar(self.padchar)?, self.at))
    }
}

/// Opens a buffer, reading the printer control variables as they are now.
pub fn open(heap: &mut Heap) -> Result<Value, String> {
    let opts = current_opts(heap)?;
    let vars = current_print_vars(heap)?;
    Ok(heap.alloc_native(TypeId::FORMAT_OUT, FormatBuf { stack: vec![Entry::Buf(Out::new())], opts, vars }))
}

/// Literal text, and what `~%`/`~|`/`~~` expanded to.
pub fn text(heap: &Heap, o: Value, s: &str) -> Result<(), String> {
    push_str(heap, o, s)
}

/// `~n&`: a newline unless the buffer is at the start of a line, then `n - 1`
/// more.
pub fn fresh_line(heap: &Heap, o: Value, n: i64) -> Result<(), String> {
    with(heap, o, |b| {
        let out = b.top()?;
        if n > 0 {
            if !out.is_empty() && !out.ends_with('\n') {
                out.push('\n');
            }
            for _ in 1..n {
                out.push('\n');
            }
        }
        Ok(())
    })
}

/// `~a`/`~s` of a `string`.
pub fn string(heap: &Heap, o: Value, s: &str, escape: bool, padding: &Padding) -> Result<(), String> {
    let (_, vars) = settings(heap, o)?;
    let text = if escape || vars.readably { format!("{:?}", s) } else { s.to_string() };
    push_str(heap, o, &padding.apply(text)?)
}

/// `~a`/`~s` of a number of a number type.
pub fn number(heap: &Heap, o: Value, v: Value, padding: &Padding) -> Result<(), String> {
    let (_, vars) = settings(heap, o)?;
    let env = RtPrintEnv::current();
    let mut s = String::new();
    if !render_number(heap, RenderCtx { env: &env, print_vars: vars }, v, &mut s)? {
        return Err(internal("a number directive was handed something else"));
    }
    push_str(heap, o, &padding.apply(s)?)
}

/// `~a`/`~s`/`~w` of any other value. Pretty printed when `*print-pretty*`
/// is on and the directive has no parameters: padding fixes a width the
/// layout is free to change.
pub fn value(heap: &mut Heap, o: Value, v: Value, escape: bool, padding: &Padding) -> Result<(), String> {
    let (opts, vars) = settings(heap, o)?;
    let env = RtPrintEnv::current();
    let ctx = RenderCtx { env: &env, print_vars: vars };
    if opts.pretty && !padding.padded {
        let mut piece = Out::new();
        pprint::render(heap, ctx, v, escape, Style::Default, &mut piece)?;
        return append(heap, o, piece);
    }
    let mut s = String::new();
    render_value(heap, ctx, v, escape, &mut s)?;
    push_str(heap, o, &padding.apply(s)?)
}

/// The parameters of `~D`/`~B`/`~O`/`~X` and of `~R` with a radix.
pub struct IntegerParams {
    pub radix: i64,
    pub mincol: i64,
    pub padchar: i64,
    pub commachar: i64,
    pub interval: i64,
    pub colon: bool,
    pub at: bool,
}

/// `v`'s digits in `radix`, with a `-` for a negative one. `None` when `v` is
/// not an integer.
fn integer_digits(heap: &Heap, v: Value, radix: u32) -> Option<String> {
    match v {
        _ if int_of(heap, v).is_some() => Some(int_to_radix(int_of(heap, v).expect("just tested"), radix)),
        Value::Boxed(id) if heap.is_bignum(id) => Some(heap.bignum_value(id).to_str_radix(radix)),
        _ => None,
    }
}

fn write_integer(heap: &Heap, o: Value, digits: &str, p: &IntegerParams) -> Result<(), String> {
    let body = decorate_integer(digits, p.colon, p.at, padchar(p.commachar)?, usize_of(p.interval));
    push_str(heap, o, &pad(&body, usize_of(p.mincol), 1, 0, padchar(p.padchar)?, true))
}

/// `~D` and its kin of an integer of an integer type.
pub fn integer(heap: &Heap, o: Value, v: Value, p: &IntegerParams) -> Result<(), String> {
    let digits = integer_digits(heap, v, p.radix as u32).ok_or_else(|| internal("an integer directive was handed something else"))?;
    write_integer(heap, o, &digits, p)
}

/// `~D` and its kin of a `Sexpr`, which may hold anything. A non-integer is
/// printed as `~a` would print it, in decimal, padded to `mincol` (CL).
pub fn integer_any(heap: &mut Heap, o: Value, v: Value, p: &IntegerParams) -> Result<(), String> {
    if let Some(digits) = integer_digits(heap, v, p.radix as u32) {
        return write_integer(heap, o, &digits, p);
    }
    let (_, mut vars) = settings(heap, o)?;
    vars.base = 10;
    vars.radix = false;
    let env = RtPrintEnv::current();
    let mut s = String::new();
    render_value(heap, RenderCtx { env: &env, print_vars: vars }, v, false, &mut s)?;
    push_str(heap, o, &pad(&s, usize_of(p.mincol), 1, 0, padchar(p.padchar)?, true))
}

/// `~R` with no radix: English cardinal (`~R`) or ordinal (`~:R`), Roman
/// (`~@R`) or old Roman (`~:@R`).
pub fn words(heap: &Heap, o: Value, v: Value, colon: bool, at: bool) -> Result<(), String> {
    let n = match int_of(heap, v) {
        Some(n) => n,
        None if matches!(v, Value::Boxed(id) if heap.is_bignum(id)) => {
            return Err("format: ~r cannot spell an integer this large in words".to_string())
        }
        None => return Err("format: ~r requires an integer argument".to_string()),
    };
    let text = match (colon, at) {
        (false, false) => english_cardinal(n),
        (true, false) => english_ordinal(n),
        (false, true) => roman(n, false)?,
        (true, true) => roman(n, true)?,
    };
    push_str(heap, o, &text)
}

/// `~P`/`~@P`: the plural suffix for the integer `v`.
pub fn plural(heap: &Heap, o: Value, v: Value, at: bool) -> Result<(), String> {
    let n = match v {
        _ if int_of(heap, v).is_some() => int_of(heap, v).expect("just tested"),
        // Every bignum is far from 1.
        Value::Boxed(id) if heap.is_bignum(id) => 2,
        _ => return Err("format: ~p requires an integer argument".to_string()),
    };
    let suffix = match (at, n == 1) {
        (true, true) => "y",
        (true, false) => "ies",
        (false, true) => "",
        (false, false) => "s",
    };
    push_str(heap, o, suffix)
}

/// `~C`.
pub fn character(heap: &Heap, o: Value, v: Value, colon: bool, at: bool) -> Result<(), String> {
    let Value::Char(c) = v else {
        return Err("format: ~c requires a character argument".to_string());
    };
    push_str(heap, o, &format_char(c, colon, at))
}

fn number_of(heap: &Heap, v: Value) -> Result<f64, String> {
    float_of(heap, v).ok_or_else(|| "format: this directive requires a numeric argument".to_string())
}

/// The parameters of `~F`. A width or digit count of -1 is one not given;
/// `overflow` is -1 when not given.
pub struct FixedParams {
    pub w: i64,
    pub d: i64,
    pub k: i64,
    pub overflow: i64,
    pub padchar: i64,
    pub at: bool,
}

fn fixed_text(f: f64, p: &FixedParams) -> Result<String, String> {
    let scaled = f * 10f64.powi(p.k as i32);
    let mut s = if p.d >= 0 { format!("{:.*}", p.d as usize, scaled) } else { trim_f64(scaled) };
    if p.at && !s.starts_with('-') {
        s.insert(0, '+');
    }
    if p.w >= 0 {
        let w = p.w as usize;
        if s.chars().count() > w && p.overflow >= 0 {
            s = std::iter::repeat_n(padchar(p.overflow)?, w).collect();
        }
        s = left_pad(&s, w, padchar(p.padchar)?);
    }
    Ok(s)
}

/// `~F`.
pub fn fixed(heap: &Heap, o: Value, v: Value, p: &FixedParams) -> Result<(), String> {
    let f = number_of(heap, v)?;
    push_str(heap, o, &fixed_text(f, p)?)
}

/// The parameters of `~E`. A width of -1 is one not given.
pub struct ExponentParams {
    pub w: i64,
    pub d: i64,
    pub exptchar: i64,
    pub padchar: i64,
    pub at: bool,
}

fn exponent_text(f: f64, p: &ExponentParams) -> Result<String, String> {
    let mut s = format!("{:.*e}", usize_of(p.d), f);
    // Rust prints `1.5e2`; CL uses an explicit sign on the exponent.
    if let Some(idx) = s.find('e') {
        let (mant, exp) = s.split_at(idx);
        let exp = &exp[1..];
        let (sign, mag) = if let Some(m) = exp.strip_prefix('-') { ("-", m) } else { ("+", exp) };
        s = format!("{}{}{}{}", mant, padchar(p.exptchar)?, sign, mag);
    }
    if p.at && !s.starts_with('-') {
        s.insert(0, '+');
    }
    if p.w >= 0 {
        s = left_pad(&s, p.w as usize, padchar(p.padchar)?);
    }
    Ok(s)
}

/// `~E`.
pub fn exponent(heap: &Heap, o: Value, v: Value, p: &ExponentParams) -> Result<(), String> {
    let f = number_of(heap, v)?;
    push_str(heap, o, &exponent_text(f, p)?)
}

/// `~G`: `~E` for a magnitude outside `[1e-3, 1e7)`, `~F` otherwise, each with
/// its own defaults (`~G` takes no parameters).
pub fn general(heap: &Heap, o: Value, v: Value, at: bool) -> Result<(), String> {
    let f = number_of(heap, v)?;
    let a = f.abs();
    let text = if a != 0.0 && !(1e-3..1e7).contains(&a) {
        exponent_text(f, &ExponentParams { w: -1, d: 6, exptchar: 'e' as i64, padchar: ' ' as i64, at })?
    } else {
        fixed_text(f, &FixedParams { w: -1, d: -1, k: 0, overflow: -1, padchar: ' ' as i64, at })?
    };
    push_str(heap, o, &text)
}

/// The parameters of `~$`.
pub struct DollarParams {
    pub d: i64,
    pub n: i64,
    pub w: i64,
    pub padchar: i64,
    pub colon: bool,
    pub at: bool,
}

/// `~$`.
pub fn dollars(heap: &Heap, o: Value, v: Value, p: &DollarParams) -> Result<(), String> {
    let f = number_of(heap, v)?;
    let (d, n, w, padchar) = (usize_of(p.d), usize_of(p.n), usize_of(p.w), padchar(p.padchar)?);
    let neg = f < 0.0;
    let body = format!("{:.*}", d, f.abs());
    // At least `n` digits before the decimal point.
    let (int_part, frac_part) = match body.split_once('.') {
        Some((i, fr)) => (i.to_string(), format!(".{}", fr)),
        None => (body.clone(), String::new()),
    };
    let int_part = if int_part.len() < n { left_pad(&int_part, n, '0') } else { int_part };
    let s = format!("{}{}", int_part, frac_part);
    let sign = if neg { "-" } else if p.at { "+" } else { "" };
    let text = if p.colon {
        // The sign before the padding.
        format!("{}{}", sign, left_pad(&s, w.saturating_sub(sign.len()), padchar))
    } else {
        left_pad(&format!("{}{}", sign, s), w, padchar)
    };
    push_str(heap, o, &text)
}

/// `~T`. Under the pretty printer the column a tab lands on is not known
/// until the layout pass, so the tab is recorded. `~:T` (`pprint-tab
/// :section`) does nothing when `*print-pretty*` is false.
pub fn tab(heap: &Heap, o: Value, colnum: i64, colinc: i64, colon: bool, at: bool) -> Result<(), String> {
    with(heap, o, |b| {
        let pretty = b.opts.pretty;
        let out = b.top()?;
        if pretty {
            let kind = match (colon, at) {
                (false, false) => TabKind::Line,
                (false, true) => TabKind::LineRelative,
                (true, false) => TabKind::Section,
                (true, true) => TabKind::SectionRelative,
            };
            out.op(Op::Tab { kind, colnum, colinc });
            return Ok(());
        }
        if colon {
            return Ok(());
        }
        let cur = current_column(&out.text);
        let target = pprint::tab_target(cur, 0, colnum, colinc, at);
        for _ in cur..target {
            out.push(' ');
        }
        Ok(())
    })
}

/// `~_`: a conditional newline, `kind` 0 to 3 for `~_`, `~:_`, `~@_` and
/// `~:@_`. Nothing when `*print-pretty*` is false.
pub fn newline(heap: &Heap, o: Value, kind: i64) -> Result<(), String> {
    let kind = match kind {
        0 => NewlineKind::Linear,
        1 => NewlineKind::Fill,
        2 => NewlineKind::Miser,
        _ => NewlineKind::Mandatory,
    };
    op(heap, o, || Op::Newline(kind))
}

/// `~nI` (`current` false) and `~n:I`. Nothing when `*print-pretty*` is
/// false.
pub fn indent(heap: &Heap, o: Value, current: bool, n: i64) -> Result<(), String> {
    op(heap, o, || Op::Indent(if current { IndentKind::Current } else { IndentKind::Block }, n))
}

/// `~(`: what follows is written into a buffer of its own.
pub fn case_open(heap: &Heap, o: Value) -> Result<(), String> {
    with(heap, o, |b| {
        b.stack.push(Entry::Buf(Out::new()));
        Ok(())
    })
}

/// `~)`: the buffer `~(` opened, case-converted into the one below it.
pub fn case_close(heap: &Heap, o: Value, colon: bool, at: bool) -> Result<(), String> {
    with(heap, o, |b| {
        let mut inner = b.pop()?;
        apply_case(&mut inner, colon, at);
        b.top()?.append(inner);
        Ok(())
    })
}

/// `~<` of a justification: its segments are collected from here.
pub fn just_open(heap: &Heap, o: Value) -> Result<(), String> {
    with(heap, o, |b| {
        b.stack.push(Entry::Just);
        Ok(())
    })
}

/// The start of one segment of a justification.
pub fn just_segment(heap: &Heap, o: Value) -> Result<(), String> {
    with(heap, o, |b| {
        b.stack.push(Entry::Buf(Out::new()));
        Ok(())
    })
}

/// The parameters of a justification `~mincol,colinc,minpad,padchar<`.
pub struct JustParams {
    pub mincol: i64,
    pub colinc: i64,
    pub minpad: i64,
    pub padchar: i64,
    pub colon: bool,
    pub at: bool,
}

/// `~>`: the segments written since [`just_open`], justified into the buffer
/// below them. A `~^` that stopped the segments early leaves fewer.
pub fn just_close(heap: &Heap, o: Value, p: &JustParams) -> Result<(), String> {
    let padchar = padchar(p.padchar)?;
    with(heap, o, |b| {
        let mut pieces = Vec::new();
        loop {
            match b.stack.pop() {
                Some(Entry::Buf(out)) => pieces.push(out),
                Some(Entry::Just) => break,
                None => return Err(internal("a justification closed that was not open")),
            }
        }
        pieces.reverse();
        let justified = justify(pieces, usize_of(p.mincol), usize_of(p.colinc), usize_of(p.minpad), padchar, p.colon, p.at);
        b.top()?.append(justified);
        Ok(())
    })
}

/// `~<…~:>`'s opening: a logical block when `*print-pretty*` is on, its
/// prefix as text otherwise.
pub fn block_open(heap: &Heap, o: Value, prefix: &str, per_line: bool, suffix: &str) -> Result<(), String> {
    with(heap, o, |b| {
        let pretty = b.opts.pretty;
        let out = b.top()?;
        if pretty {
            out.op(Op::BlockStart { prefix: prefix.to_string(), per_line, suffix: suffix.to_string() });
        } else {
            out.push_str(prefix);
        }
        Ok(())
    })
}

/// `~<…~:>`'s closing: the block's end, or its suffix as text.
pub fn block_close(heap: &Heap, o: Value, suffix: &str) -> Result<(), String> {
    with(heap, o, |b| {
        let pretty = b.opts.pretty;
        let out = b.top()?;
        if pretty {
            out.op(Op::BlockEnd);
        } else {
            out.push_str(suffix);
        }
        Ok(())
    })
}

/// The finished output, taken out of the buffer.
fn take(heap: &Heap, o: Value) -> Result<Out, String> {
    with(heap, o, |b| {
        let out = b.pop()?;
        if !b.stack.is_empty() {
            return Err(internal("a buffer was left open"));
        }
        Ok(out)
    })
}

/// `format`'s value: the laid-out text. When `write` (the destination was
/// `true`) it is also written, through [`emit`], which merges it into an
/// open logical block rather than jumping the queue past that block's
/// buffered output.
pub fn result(heap: &mut Heap, o: Value, write: bool) -> Result<String, String> {
    let out = take(heap, o)?;
    let opts = current_opts(heap)?;
    let text = super::finish(out.clone(), &opts);
    if write {
        emit(heap, out, false, &opts)?;
    }
    Ok(text)
}

/// `print` (`newline` false) and `println`.
pub fn emit_out(heap: &mut Heap, o: Value, newline: bool) -> Result<(), String> {
    let out = take(heap, o)?;
    let opts = current_opts(heap)?;
    emit(heap, out, newline, &opts)
}

/// `v` as `~s` (`escape`) or `~a` writes it, laid out: what an expanded
/// `(format false "~s" v)` returns, for Rust code that prints a value.
pub fn render(heap: &mut Heap, v: Value, escape: bool) -> Result<String, String> {
    heap.push_root(v);
    let o = open(heap);
    let r = o.and_then(|o| {
        heap.push_root(o);
        let plain = Padding { padded: false, mincol: 0, colinc: 1, minpad: 0, padchar: ' ' as i64, at: false };
        let r = value(heap, o, v, escape, &plain).and_then(|()| result(heap, o, false));
        heap.pop_root();
        r
    });
    heap.pop_root();
    r
}

// ---- walking a list of arguments ------------------------------------------

/// The next argument of a list being walked.
pub fn first(heap: &Heap, rest: Value) -> Result<Value, String> {
    match rest {
        Value::Cons(_) => heap.car(rest).map_err(|e| internal(&e.to_string())),
        _ => Err("format: ran out of arguments for the control string".to_string()),
    }
}

/// The list after its first argument.
pub fn rest(heap: &Heap, rest: Value) -> Result<Value, String> {
    heap.cdr(rest).map_err(|e| internal(&e.to_string()))
}

/// Whether no argument is left.
pub fn empty(rest: Value) -> bool {
    !matches!(rest, Value::Cons(_))
}

/// How many arguments the list holds. `what` names the list for the error
/// when it is not a proper one.
pub fn length(heap: &Heap, list: Value, what: &str) -> Result<i64, String> {
    heap.list_to_vec(list).map(|v| v.len() as i64).map_err(|_| format!("format: {} is not a proper list", what))
}

/// The arguments of `whole` from the `n`th on: where `~*`/`~:*`/`~@*` and
/// `~:P` move the cursor to. `backup` is true for `~:P`, whose error says
/// what it was doing.
pub fn goto(heap: &Heap, whole: Value, n: i64, backup: bool) -> Result<Value, String> {
    let len = length(heap, whole, "the argument list")?;
    if backup && n < 0 {
        return Err("format: ~:p reuses the previous argument, and there is none".to_string());
    }
    if n < 0 || n > len {
        return Err(format!("format: ~* moves to argument {}, outside the {} there are", n, len));
    }
    let mut at = whole;
    for _ in 0..n {
        at = rest(heap, at)?;
    }
    Ok(at)
}

/// `v` as a list: an element of `~:{`'s argument, or one of `~:@{`'s
/// arguments, each of which must be one.
pub fn as_list(heap: &Heap, v: Value, what: &str) -> Result<Value, String> {
    match v {
        Value::Empty | Value::Cons(_) => {
            length(heap, v, what)?;
            Ok(v)
        }
        _ => Err(format!("format: {} is not a proper list", what)),
    }
}

/// What a prefix parameter slot accepts, as the expansion passes it to
/// [`param`].
pub const PARAM_NON_NEG: i64 = 0;
pub const PARAM_POS: i64 = 1;
pub const PARAM_INT: i64 = 2;
pub const PARAM_CHAR: i64 = 3;
pub const PARAM_RADIX: i64 = 4;

/// The value of a `v` or `#` parameter of a directive, `v` being what it read
/// (a `#` count arrives as an integer), checked against the slot `kind` (one
/// of the `PARAM_*` codes). `index` is the slot's number from 1, for the
/// error. A character is its code point.
pub fn param(heap: &Heap, v: Value, kind: i64, index: i64) -> Result<i64, String> {
    let bad = |why: String| format!("format: parameter {}: {}", index, why);
    if kind == PARAM_CHAR {
        return match v {
            Value::Char(c) => Ok(c as i64),
            _ => Err(bad("its `v` argument must be a character".to_string())),
        };
    }
    let n = int_of(heap, v).ok_or_else(|| bad("its `v` argument must be an integer".to_string()))?;
    let kind = match kind {
        PARAM_NON_NEG => super::ParamKind::NonNeg,
        PARAM_POS => super::ParamKind::Pos,
        PARAM_RADIX => super::ParamKind::Radix,
        _ => super::ParamKind::Int,
    };
    kind.check_int(n).map_err(bad)?;
    Ok(n)
}

/// `~[`'s selector read from an argument: the integer, or -1 (which selects
/// no numbered clause) for one too large for a word.
pub fn select(heap: &Heap, v: Value) -> Result<i64, String> {
    match int_of(heap, v) {
        Some(n) => Ok(n),
        None if matches!(v, Value::Boxed(id) if heap.is_bignum(id)) => Ok(-1),
        None => Err("format: ~[ requires an integer argument".to_string()),
    }
}

/// `~:[`'s selector read from a `Sexpr` argument.
pub fn boolean(v: Value) -> Result<bool, String> {
    match v {
        Value::Bool(b) => Ok(b),
        _ => Err("format: ~:[ requires a bool argument".to_string()),
    }
}

// ---- the interpreter's door -------------------------------------------------

/// Runs the piece `name` (one of the `wk::F_FMT_*`) on `args`, in the order
/// and representations the expansion passes them: the buffer, strings and
/// `Sexpr`s as themselves, `i32`s as `Value::Int`, flags as `Value::Bool`.
/// `None` for any other name. The interpreter's way in; compiled code calls
/// each piece's own shim (`crate::shim`), so that it links only those.
pub fn interpret(heap: &mut Heap, name: u32, args: &[Value]) -> Option<Result<Value, String>> {
    use typelisp_mem::wk;
    let int = |i: usize| match args.get(i) {
        Some(Value::Int(n)) => Ok(*n),
        _ => Err(internal("expected an integer argument")),
    };
    let flag = |i: usize| match args.get(i) {
        Some(Value::Bool(b)) => Ok(*b),
        _ => Err(internal("expected a bool argument")),
    };
    let arg = |i: usize| args.get(i).copied().ok_or_else(|| internal("too few arguments"));
    let text = |heap: &Heap, i: usize| match args.get(i) {
        Some(Value::Str(id)) => Ok(heap.string(*id).to_string()),
        _ => Err(internal("expected a string argument")),
    };
    let padding = |from: usize| -> Result<Padding, String> {
        Ok(Padding { padded: flag(from)?, mincol: int(from + 1)?, colinc: int(from + 2)?, minpad: int(from + 3)?, padchar: int(from + 4)?, at: flag(from + 5)? })
    };
    let integer_params = || -> Result<IntegerParams, String> {
        Ok(IntegerParams { radix: int(2)?, mincol: int(3)?, padchar: int(4)?, commachar: int(5)?, interval: int(6)?, colon: flag(7)?, at: flag(8)? })
    };
    let unit = |r: Result<(), String>| r.map(|()| Value::Empty);
    let r = match name {
        wk::F_FMT_OPEN => open(heap),
        wk::F_FMT_TEXT => (|| unit(text(heap, 1).and_then(|s| self::text(heap, arg(0)?, &s))))(),
        wk::F_FMT_FRESH_LINE => (|| unit(fresh_line(heap, arg(0)?, int(1)?)))(),
        wk::F_FMT_STRING => (|| unit(string(heap, arg(0)?, &text(heap, 1)?, flag(2)?, &padding(3)?)))(),
        wk::F_FMT_NUMBER => (|| unit(number(heap, arg(0)?, arg(1)?, &padding(2)?)))(),
        wk::F_FMT_VALUE => (|| unit(value(heap, arg(0)?, arg(1)?, flag(2)?, &padding(3)?)))(),
        wk::F_FMT_INTEGER => (|| unit(integer(heap, arg(0)?, arg(1)?, &integer_params()?)))(),
        wk::F_FMT_INTEGER_ANY => (|| unit(integer_any(heap, arg(0)?, arg(1)?, &integer_params()?)))(),
        wk::F_FMT_WORDS => (|| unit(words(heap, arg(0)?, arg(1)?, flag(2)?, flag(3)?)))(),
        wk::F_FMT_PLURAL => (|| unit(plural(heap, arg(0)?, arg(1)?, flag(2)?)))(),
        wk::F_FMT_CHAR => (|| unit(character(heap, arg(0)?, arg(1)?, flag(2)?, flag(3)?)))(),
        wk::F_FMT_FIXED => (|| {
            let p = FixedParams { w: int(2)?, d: int(3)?, k: int(4)?, overflow: int(5)?, padchar: int(6)?, at: flag(7)? };
            unit(fixed(heap, arg(0)?, arg(1)?, &p))
        })(),
        wk::F_FMT_EXPONENT => (|| {
            let p = ExponentParams { w: int(2)?, d: int(3)?, exptchar: int(4)?, padchar: int(5)?, at: flag(6)? };
            unit(exponent(heap, arg(0)?, arg(1)?, &p))
        })(),
        wk::F_FMT_GENERAL => (|| unit(general(heap, arg(0)?, arg(1)?, flag(2)?)))(),
        wk::F_FMT_DOLLARS => (|| {
            let p = DollarParams { d: int(2)?, n: int(3)?, w: int(4)?, padchar: int(5)?, colon: flag(6)?, at: flag(7)? };
            unit(dollars(heap, arg(0)?, arg(1)?, &p))
        })(),
        wk::F_FMT_TAB => (|| unit(tab(heap, arg(0)?, int(1)?, int(2)?, flag(3)?, flag(4)?)))(),
        wk::F_FMT_NEWLINE => (|| unit(newline(heap, arg(0)?, int(1)?)))(),
        wk::F_FMT_INDENT => (|| unit(indent(heap, arg(0)?, flag(1)?, int(2)?)))(),
        wk::F_FMT_CASE_OPEN => (|| unit(case_open(heap, arg(0)?)))(),
        wk::F_FMT_CASE_CLOSE => (|| unit(case_close(heap, arg(0)?, flag(1)?, flag(2)?)))(),
        wk::F_FMT_JUST_OPEN => (|| unit(just_open(heap, arg(0)?)))(),
        wk::F_FMT_JUST_SEGMENT => (|| unit(just_segment(heap, arg(0)?)))(),
        wk::F_FMT_JUST_CLOSE => (|| {
            let p = JustParams { mincol: int(1)?, colinc: int(2)?, minpad: int(3)?, padchar: int(4)?, colon: flag(5)?, at: flag(6)? };
            unit(just_close(heap, arg(0)?, &p))
        })(),
        wk::F_FMT_BLOCK_OPEN => (|| unit(block_open(heap, arg(0)?, &text(heap, 1)?, flag(2)?, &text(heap, 3)?)))(),
        wk::F_FMT_BLOCK_CLOSE => (|| unit(block_close(heap, arg(0)?, &text(heap, 1)?)))(),
        wk::F_FMT_RESULT => (|| result(heap, arg(0)?, flag(1)?).map(|s| heap.alloc_string(s)))(),
        wk::F_FMT_EMIT => (|| unit(emit_out(heap, arg(0)?, flag(1)?)))(),
        wk::F_FMT_FIRST => (|| first(heap, arg(0)?))(),
        wk::F_FMT_REST => (|| rest(heap, arg(0)?))(),
        wk::F_FMT_EMPTY => (|| Ok(Value::Bool(empty(arg(0)?))))(),
        wk::F_FMT_LENGTH => (|| length(heap, arg(0)?, &text(heap, 1)?).map(Value::Int))(),
        wk::F_FMT_GOTO => (|| goto(heap, arg(0)?, int(1)?, flag(2)?))(),
        wk::F_FMT_AS_LIST => (|| as_list(heap, arg(0)?, &text(heap, 1)?))(),
        wk::F_FMT_PARAM => (|| param(heap, arg(0)?, int(1)?, int(2)?).map(Value::Int))(),
        wk::F_FMT_SELECT => (|| select(heap, arg(0)?).map(Value::Int))(),
        wk::F_FMT_BOOL => (|| boolean(arg(0)?).map(Value::Bool))(),
        _ => return None,
    };
    Some(r)
}
