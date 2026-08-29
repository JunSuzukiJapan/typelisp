//! CL's extended `loop` — [cl-parity-plan.md](../../../../docs/dev/cl-parity-plan.md)
//! Stage 4b.
//!
//! `loop` stays what it was, an unconditional loop, whenever its first
//! argument is not a keyword; that is CL's own rule (its "simple loop" form),
//! so nothing that used to read as a body starts reading as a clause. When the
//! first argument *is* a keyword, `Checker::check_loop` routes here.
//!
//! This module does two separable jobs and nothing else:
//!
//! 1. [`parse`] reads the clause sequence out of the heap into [`Plan`].
//!    It resolves no names and knows no types.
//! 2. [`build`] turns a [`Plan`] back into ordinary source — `let*`, `loop`,
//!    `if`, `setf`, `push`, `return` — which the checker then checks like any
//!    other form.
//!
//! The one thing `build` cannot work out for itself is the *type* of an
//! accumulator, which is why it takes `acc_types`. The plan assumed inference
//! would supply it — "`collect` は `(let ((acc (Vector::new))) … (push acc e) …
//! acc)` へ展開すれば要素型が推論で決まる" — and that is not so:
//!
//! ```text
//! (let ((acc (Vector::new))) (progn (push acc 5) (len acc)))
//! => type error: cannot infer type argument `t` for `vector::new`
//! ```
//!
//! `Vector::new`'s type argument comes from the *expected* type, forwards; a
//! later `push` cannot reach back for it. So `Checker::check_loop_dsl` checks
//! each accumulated expression first, renders the type it found, and hands the
//! written form down here — the checker knowing a type and writing it into the
//! tree it builds, which is what it does everywhere else it has to
//! ([[typelisp-static-types-are-always-known]]).
//!
//! **Keywords, not bare symbols.** CL writes `(loop for i from 1 to 3 collect
//! i)`; here every clause word is a keyword, `(loop :for i :from 1 :to 3
//! :collect i)`. A bare `for` would be an ordinary variable reference, and the
//! keyword is also what tells `check_loop` a DSL body from a simple one.

use crate::{Error, Heap, Loc, SymId, Value};

use super::forms;

/// Where a `:for … :from` range stops.
#[derive(Debug)]
pub(super) enum RangeEnd {
    /// `:to` — inclusive, counting up. `:downto` is the same bound counting
    /// down, which `Range::down` records.
    Inclusive(Value),
    /// `:below` (up) / `:above` (down) — exclusive.
    Exclusive(Value),
    /// No bound: the range runs until something else stops the loop.
    Open,
}

/// How one `:for` variable advances.
#[derive(Debug)]
pub(super) enum ForIter {
    /// `:in` / `:across` — the elements of any `Iter`. CL distinguishes the
    /// two (list vs vector); this language has one sequence protocol, so they
    /// are the same clause under two spellings.
    Seq(Value),
    /// `:on` — the successive suffixes, each a fresh `Vector<A>`. CL yields
    /// the shared tail conses; an `Iter` has no tail to share.
    Suffixes(Value),
    /// `:from a :to/:below/:downto/:above b :by s`.
    Range { from: Value, end: RangeEnd, by: Option<Value>, down: bool },
    /// `:= init :then next`. Without `:then`, `init` is re-evaluated every
    /// iteration, as in CL.
    Assign { init: Value, next: Option<Value> },
}

/// What an accumulation clause accumulates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Acc {
    Collect,
    Append,
    Sum,
    Count,
    Maximize,
    Minimize,
}

/// A clause that runs once per iteration, in written order.
#[derive(Debug)]
pub(super) enum Step {
    /// `:do form...`
    Do(Vec<Value>),
    /// `:collect`/`:append`/`:sum`/`:count`/`:maximize`/`:minimize`, with the
    /// accumulator name `:into` gave it (or the anonymous one).
    Accumulate { acc: Acc, expr: Value, into: String },
    /// `:always` / `:never` — leave with `false` the moment the test fails.
    /// `never` is `always` over the negation, so only the polarity differs.
    Assert { expr: Value, want: bool },
    /// `:thereis` — leave with `(some expr)` the moment it holds.
    Thereis { expr: Value },
    /// `:while` / `:until` — stop here (normally, so `:finally` still runs).
    Stop { expr: Value, when: bool },
    /// `:return form`
    Return(Value),
    /// `:when`/`:unless`/`:if` … [`:else` …]. CL lets one clause follow;
    /// `:and` chains are not accepted (see the module comment).
    Cond { test: Value, want: bool, then: Box<Step>, els: Option<Box<Step>> },
}

/// One `:with` or `:for` variable, in written order.
#[derive(Debug)]
pub(super) struct Var {
    pub name: String,
    pub loc: Option<Loc>,
    pub kind: VarKind,
}

#[derive(Debug)]
pub(super) enum VarKind {
    /// `:with v = e` — bound once, never stepped.
    With(Value),
    For(ForIter),
}

/// Everything [`build`] needs, with nothing resolved.
#[derive(Debug)]
pub(super) struct Plan {
    pub vars: Vec<Var>,
    /// `:repeat n` — an iteration count, independent of any variable.
    pub repeat: Option<Value>,
    pub initially: Vec<Value>,
    pub steps: Vec<Step>,
    pub finally: Vec<Value>,
    /// Accumulator names in first-written order. The loop's own value is the
    /// first one — CL's rule for an unnamed accumulation, and the only sane
    /// choice for a named one too (`:into` names exist to be read by
    /// `:finally`, which can `return` whichever it likes).
    pub accs: Vec<AccVar>,
    /// Whether any `:always`/`:never`/`:thereis` clause was written; those
    /// decide the value the loop leaves with when nothing stopped it early.
    pub verdict: Option<Verdict>,
}

#[derive(Debug)]
pub(super) struct AccVar {
    pub name: String,
    pub acc: Acc,
    /// The expression whose type decides the accumulator's — kept so
    /// `Checker::check_loop_dsl` can check it before `build` runs.
    pub sample: Value,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Verdict {
    /// `:always`/`:never` — `true` if the loop ran to exhaustion.
    Boolean,
    /// `:thereis` — `(none)` if the loop ran to exhaustion. The `Option`'s
    /// element type comes from the `:thereis` expression, which
    /// `Checker::check_loop_dsl` finds by walking [`Plan::steps`].
    Optional,
}

/// The name an accumulation clause with no `:into` accumulates into. The
/// space makes it unwritable in source, the same trick `mangled_method_name`
/// uses — two anonymous accumulations in one loop are CL-illegal anyway, and
/// this way the name can never collide with a `:with` variable.
const ANON_ACC: &str = "loop acc";

/// `v` as a clause keyword, or `None` when `v` is not a keyword.
///
/// Returns the symbol itself, not its text: which clause word this is gets
/// decided by comparing that symbol against a [`SymId`] constant, the way one
/// symbol is ever compared to another. The name is still *read* here, but only
/// to answer "is this a keyword at all?" — an open set (any symbol whose name
/// starts with one colon), which no fixed set of ids can express.
fn keyword(heap: &Heap, v: Value) -> Option<SymId> {
    match v {
        Value::Symbol(id) => {
            let name = heap.symbol_name(id);
            (name.starts_with(':') && !name[1..].starts_with(':')).then_some(id)
        }
        _ => None,
    }
}

/// Whether `loop`'s argument list is a DSL body rather than a simple loop's
/// forms — CL's own rule, read off the first argument.
pub(super) fn is_dsl(heap: &Heap, args: &[Value]) -> bool {
    args.first().and_then(|v| keyword(heap, *v)).is_some()
}

/// The symbol name of `v`, for a clause position that must name a variable.
fn var_name(heap: &Heap, v: Value) -> Option<String> {
    match v {
        Value::Symbol(id) if !heap.symbol_name(id).starts_with(':') => {
            Some(heap.symbol_name(id).to_string())
        }
        _ => None,
    }
}

fn err(msg: impl Into<String>) -> Error {
    Error::TypeError(format!("loop: {}", msg.into()))
}

/// A cursor over the clause sequence.
struct Cursor<'a> {
    args: &'a [Value],
    locs: &'a [Option<Loc>],
    i: usize,
}

impl<'a> Cursor<'a> {
    fn peek_keyword(&self, heap: &Heap) -> Option<SymId> {
        self.args.get(self.i).and_then(|v| keyword(heap, *v))
    }

    fn next_value(&mut self, heap: &Heap, what: &str) -> Result<(Value, Option<Loc>), Error> {
        let Some(v) = self.args.get(self.i).copied() else {
            return Err(err(format!("{} needs a form after it", what)));
        };
        if keyword(heap, v).is_some() {
            return Err(err(format!("{} needs a form after it, not another clause word", what)));
        }
        let loc = self.locs.get(self.i).cloned().flatten();
        self.i += 1;
        Ok((v, loc))
    }

    /// Every form up to the next clause keyword (or the end).
    fn next_body(&mut self, heap: &Heap, what: &str) -> Result<Vec<Value>, Error> {
        let mut out = Vec::new();
        while let Some(v) = self.args.get(self.i).copied() {
            if keyword(heap, v).is_some() {
                break;
            }
            out.push(v);
            self.i += 1;
        }
        if out.is_empty() {
            return Err(err(format!("{} needs at least one form after it", what)));
        }
        Ok(out)
    }

    /// Consume the `=` that separates a variable from its value, written
    /// either bare (`:with k = 10`, CL's own spelling) or as the keyword
    /// `:=`. A bare `=` is unambiguous here — the position holds a clause
    /// word, and no clause word is a variable reference — so both read.
    fn eat_eq(&mut self) -> bool {
        match self.args.get(self.i).copied() {
            Some(Value::Symbol(id)) if id == SymId::EQUALS || id == SymId::COLON_EQUALS => {
                self.i += 1;
                true
            }
            _ => false,
        }
    }

    /// Consume `word` if it is the next clause keyword.
    fn eat(&mut self, heap: &Heap, word: SymId) -> bool {
        if self.peek_keyword(heap) == Some(word) {
            self.i += 1;
            true
        } else {
            false
        }
    }
}

/// Read `args` as a clause sequence.
pub(super) fn parse(heap: &Heap, args: &[Value], locs: &[Option<Loc>]) -> Result<Plan, Error> {
    let mut c = Cursor { args, locs, i: 0 };
    let mut plan = Plan {
        vars: Vec::new(),
        repeat: None,
        initially: Vec::new(),
        steps: Vec::new(),
        finally: Vec::new(),
        accs: Vec::new(),
        verdict: None,
    };

    // CL's own order: every variable clause first, then the main clauses.
    // Enforced rather than tolerated, because a `:for` written after a `:do`
    // would read as if it iterated only the rest of the body, and it does not.
    while let Some(word) = c.peek_keyword(heap) {
        match word {
            SymId::KW_WITH | SymId::KW_FOR | SymId::KW_AS => {
                c.i += 1;
                let (name_v, loc) = c.next_value(heap, heap.symbol_name(word))?;
                let Some(name) = var_name(heap, name_v) else {
                    return Err(err(format!("{} needs a variable name", heap.symbol_name(word))));
                };
                let kind = if word == SymId::KW_WITH {
                    if !c.eat_eq() {
                        return Err(err(":with needs `= form` after the variable"));
                    }
                    VarKind::With(c.next_value(heap, ":with =")?.0)
                } else {
                    VarKind::For(parse_for_iter(heap, &mut c)?)
                };
                plan.vars.push(Var { name, loc, kind });
            }
            SymId::KW_REPEAT => {
                c.i += 1;
                if plan.repeat.is_some() {
                    return Err(err(":repeat may appear only once"));
                }
                plan.repeat = Some(c.next_value(heap, ":repeat")?.0);
            }
            _ => break,
        }
    }

    while let Some(word) = c.peek_keyword(heap) {
        match word {
            SymId::KW_WITH | SymId::KW_FOR | SymId::KW_AS => {
                return Err(err(format!(
                    "{} must come before the body clauses, as in CL — it steps the whole loop, \
                     not the part written after it",
                    heap.symbol_name(word)
                )))
            }
            SymId::KW_INITIALLY => {
                c.i += 1;
                plan.initially.extend(c.next_body(heap, ":initially")?);
            }
            SymId::KW_FINALLY => {
                c.i += 1;
                plan.finally.extend(c.next_body(heap, ":finally")?);
            }
            _ => {
                let step = parse_step(heap, &mut c, &mut plan)?;
                plan.steps.push(step);
            }
        }
    }

    if c.i < args.len() {
        return Err(err("a clause word (`:for`, `:collect`, …) was expected here"));
    }
    // A `:return` gives the loop a value of its own, so the loop needs one on
    // every way out — including running to exhaustion. CL answers nil there;
    // there is no nil here, so the loop has to say. Caught now rather than
    // left to surface as `expected Unit, found …` on the whole form.
    if plan.accs.is_empty()
        && plan.verdict.is_none()
        && plan.finally.is_empty()
        && plan.steps.iter().any(has_return)
    {
        return Err(err(
            "`:return` gives this loop a value, but nothing says what it is when the loop runs \
             to exhaustion — accumulate (`:collect`/`:sum`/…) or end with `:finally (return …)`",
        ));
    }
    Ok(plan)
}

fn has_return(step: &Step) -> bool {
    match step {
        Step::Return(_) => true,
        Step::Cond { then, els, .. } => {
            has_return(then) || els.as_deref().is_some_and(has_return)
        }
        _ => false,
    }
}

fn parse_for_iter(heap: &Heap, c: &mut Cursor) -> Result<ForIter, Error> {
    if c.eat_eq() {
        let init = c.next_value(heap, "=")?.0;
        let next = if c.eat(heap, SymId::KW_THEN) { Some(c.next_value(heap, ":then")?.0) } else { None };
        return Ok(ForIter::Assign { init, next });
    }
    let Some(word) = c.peek_keyword(heap) else {
        return Err(err(":for needs `:in`, `:across`, `:on`, `:from` or `=` after the variable"));
    };
    c.i += 1;
    match word {
        SymId::KW_IN | SymId::KW_ACROSS => Ok(ForIter::Seq(c.next_value(heap, heap.symbol_name(word))?.0)),
        SymId::KW_ON => Ok(ForIter::Suffixes(c.next_value(heap, ":on")?.0)),
        SymId::KW_FROM | SymId::KW_DOWNFROM | SymId::KW_UPFROM => {
            let from = c.next_value(heap, heap.symbol_name(word))?.0;
            let mut down = word == SymId::KW_DOWNFROM;
            let end = match c.peek_keyword(heap) {
                Some(SymId::KW_TO) => {
                    c.i += 1;
                    RangeEnd::Inclusive(c.next_value(heap, ":to")?.0)
                }
                Some(SymId::KW_BELOW) => {
                    c.i += 1;
                    RangeEnd::Exclusive(c.next_value(heap, ":below")?.0)
                }
                Some(SymId::KW_DOWNTO) => {
                    c.i += 1;
                    down = true;
                    RangeEnd::Inclusive(c.next_value(heap, ":downto")?.0)
                }
                Some(SymId::KW_ABOVE) => {
                    c.i += 1;
                    down = true;
                    RangeEnd::Exclusive(c.next_value(heap, ":above")?.0)
                }
                _ => RangeEnd::Open,
            };
            let by = if c.eat(heap, SymId::KW_BY) { Some(c.next_value(heap, ":by")?.0) } else { None };
            Ok(ForIter::Range { from, end, by, down })
        }
        other => Err(err(format!(
            ":for {}: expected `:in`, `:across`, `:on`, `:from`/`:downfrom` or `=`",
            heap.symbol_name(other)
        ))),
    }
}

fn parse_step(heap: &Heap, c: &mut Cursor, plan: &mut Plan) -> Result<Step, Error> {
    let Some(word) = c.peek_keyword(heap) else {
        return Err(err("a clause word was expected here"));
    };
    c.i += 1;
    match word {
        SymId::KW_DO | SymId::KW_DOING => Ok(Step::Do(c.next_body(heap, ":do")?)),
        SymId::KW_COLLECT | SymId::KW_COLLECTING => accumulate(heap, c, plan, Acc::Collect, word),
        SymId::KW_APPEND | SymId::KW_APPENDING => accumulate(heap, c, plan, Acc::Append, word),
        SymId::KW_SUM | SymId::KW_SUMMING => accumulate(heap, c, plan, Acc::Sum, word),
        SymId::KW_COUNT | SymId::KW_COUNTING => accumulate(heap, c, plan, Acc::Count, word),
        SymId::KW_MAXIMIZE | SymId::KW_MAXIMIZING => accumulate(heap, c, plan, Acc::Maximize, word),
        SymId::KW_MINIMIZE | SymId::KW_MINIMIZING => accumulate(heap, c, plan, Acc::Minimize, word),
        SymId::KW_ALWAYS => verdict_step(heap, c, plan, true),
        SymId::KW_NEVER => verdict_step(heap, c, plan, false),
        SymId::KW_THEREIS => {
            let expr = c.next_value(heap, ":thereis")?.0;
            if plan.verdict.is_some() {
                return Err(err("only one of `:always`/`:never`/`:thereis` may appear"));
            }
            plan.verdict = Some(Verdict::Optional);
            Ok(Step::Thereis { expr })
        }
        SymId::KW_WHILE => Ok(Step::Stop { expr: c.next_value(heap, ":while")?.0, when: false }),
        SymId::KW_UNTIL => Ok(Step::Stop { expr: c.next_value(heap, ":until")?.0, when: true }),
        SymId::KW_RETURN => Ok(Step::Return(c.next_value(heap, ":return")?.0)),
        SymId::KW_WHEN | SymId::KW_IF | SymId::KW_UNLESS => {
            let want = word != SymId::KW_UNLESS;
            let test = c.next_value(heap, heap.symbol_name(word))?.0;
            let then = Box::new(parse_step(heap, c, plan)?);
            let els =
                if c.eat(heap, SymId::KW_ELSE) { Some(Box::new(parse_step(heap, c, plan)?)) } else { None };
            Ok(Step::Cond { test, want, then, els })
        }
        other => Err(err(format!("`{}` is not a clause this `loop` knows", heap.symbol_name(other)))),
    }
}

fn verdict_step(heap: &Heap, c: &mut Cursor, plan: &mut Plan, want: bool) -> Result<Step, Error> {
    let expr = c.next_value(heap, if want { ":always" } else { ":never" })?.0;
    if plan.verdict.is_some() {
        return Err(err("only one of `:always`/`:never`/`:thereis` may appear"));
    }
    plan.verdict = Some(Verdict::Boolean);
    Ok(Step::Assert { expr, want })
}

fn accumulate(
    heap: &Heap,
    c: &mut Cursor,
    plan: &mut Plan,
    acc: Acc,
    word: SymId,
) -> Result<Step, Error> {
    let expr = c.next_value(heap, heap.symbol_name(word))?.0;
    let into = if c.eat(heap, SymId::KW_INTO) {
        let (v, _) = c.next_value(heap, ":into")?;
        var_name(heap, v).ok_or_else(|| err(":into needs a variable name"))?
    } else {
        ANON_ACC.to_string()
    };
    match plan.accs.iter().find(|a| a.name == into) {
        Some(prev) if prev.acc != acc => {
            return Err(err(format!(
                "`{}` accumulates two different ways in one loop",
                if into == ANON_ACC { "the loop's own result" } else { &into }
            )))
        }
        Some(_) => {}
        None => plan.accs.push(AccVar { name: into.clone(), acc, sample: expr }),
    }
    Ok(Step::Accumulate { acc, expr, into })
}

// ---------------------------------------------------------------- building

/// The flag that suppresses the stepping pass on the first time round.
/// It contains a space, so no source can write it and no user variable can
/// collide with it — the same trick `mangled_method_name` uses. So do the
/// per-clause state names [`state_name`] makes.
const FIRST_FLAG: &str = "loop first";

fn state_name(prefix: &str, n: usize) -> String {
    format!("loop {} {}", prefix, n)
}

/// Everything the checker had to work out before source could be written:
/// one initializer form per accumulator (in `Plan::accs` order), and the
/// `(the Option<T> (Option::none))` a `:thereis` leaves with when nothing
/// matched.
pub(super) struct Resolved {
    pub acc_inits: Vec<(String, Value)>,
    pub thereis_none: Option<Value>,
}

struct Build<'a, 'h> {
    s: &'a mut crate::RootScope<'h>,
    /// Serial number for the temporaries [`build_step`] binds, so two steps
    /// never pick the same name even when one nests inside another.
    tmp: usize,
}

impl Build<'_, '_> {
    fn sym(&mut self, name: &str) -> Value {
        self.s.intern_symbol(name)
    }

    /// A list, rooted before it is returned — every caller holds it across a
    /// later allocation, which is where this file's kind of GC leak lives
    /// (`forms::rooted`'s doc comment).
    fn list(&mut self, items: &[Value]) -> Result<Value, Error> {
        let pairs: Vec<(Value, Option<Loc>)> = items.iter().map(|v| (*v, None)).collect();
        let out = forms::list_from_vec_locs(self.s, &pairs)?;
        self.s.push_root(out);
        Ok(out)
    }

    /// A `::` name. The reader splits `a::b` into a `Value::Path` at read
    /// time, so a symbol spelled that way would resolve to nothing.
    fn path(&mut self, name: &str) -> Value {
        let p = crate::Path::from_segments(name.split("::").map(str::to_string).collect());
        forms::path_form(self.s, &p)
    }

    fn call(&mut self, head: &str, args: &[Value]) -> Result<Value, Error> {
        let h = self.sym(head);
        let mut items = Vec::with_capacity(args.len() + 1);
        items.push(h);
        items.extend_from_slice(args);
        self.list(&items)
    }

    /// [`Self::call`] with a `::` head.
    fn call_path(&mut self, head: &str, args: &[Value]) -> Result<Value, Error> {
        let h = self.path(head);
        let mut items = Vec::with_capacity(args.len() + 1);
        items.push(h);
        items.extend_from_slice(args);
        self.list(&items)
    }

    /// `(progn forms...)` — the value is the last form's. For a *statement*
    /// sequence use [`Self::statements`] instead.
    fn progn(&mut self, forms: &[Value]) -> Result<Value, Error> {
        let h = self.sym("progn");
        let mut items = Vec::with_capacity(forms.len() + 1);
        items.push(h);
        items.extend_from_slice(forms);
        self.list(&items)
    }

    /// `(progn forms... ())` — the trailing unit keeps every generated
    /// statement position `Unit`-typed, which is what `if`'s two branches and
    /// a loop body have to agree on (`setf` evaluates to what it assigned, so
    /// a bare `setf` in statement position would not).
    fn statements(&mut self, forms: &[Value]) -> Result<Value, Error> {
        let h = self.sym("progn");
        let mut items = Vec::with_capacity(forms.len() + 2);
        items.push(h);
        items.extend_from_slice(forms);
        items.push(Value::Empty);
        self.list(&items)
    }

    fn if_(&mut self, test: Value, then: Value, els: Value) -> Result<Value, Error> {
        self.call("if", &[test, then, els])
    }

    /// `(if test then ())` — a conditional statement.
    fn when(&mut self, test: Value, then: Value) -> Result<Value, Error> {
        self.if_(test, then, Value::Empty)
    }

    fn setf(&mut self, name: &str, value: Value) -> Result<Value, Error> {
        let n = self.sym(name);
        self.call("setf", &[n, value])
    }

    /// `(let* ((n v)...) body)`, or `body` when there is nothing to bind.
    fn let_star(&mut self, binds: &[(String, Value)], body: Value) -> Result<Value, Error> {
        if binds.is_empty() {
            return Ok(body);
        }
        let mut pairs = Vec::with_capacity(binds.len());
        for (name, val) in binds {
            let n = self.sym(name);
            let pair = self.list(&[n, *val])?;
            pairs.push(pair);
        }
        let list = self.list(&pairs)?;
        let head = self.sym("let*");
        self.list(&[head, list, body])
    }

    fn next_tmp(&mut self, prefix: &str) -> String {
        self.tmp += 1;
        state_name(prefix, self.tmp)
    }
}

/// Turn `plan` back into ordinary source for the checker to check.
pub(super) fn build(heap: &mut Heap, plan: &Plan, resolved: &Resolved) -> Result<Value, Error> {
    let mut scope = crate::RootScope::new(heap);
    let mut b = Build { s: &mut scope, tmp: 0 };

    // What each variable clause contributes: a binding that outlives the
    // loop, a binding refreshed every pass, a way for the loop to be over,
    // and a way to advance.
    let mut outer: Vec<(String, Value)> = Vec::new();
    let mut per_pass: Vec<(String, Value)> = Vec::new();
    let mut exhausted: Vec<Value> = Vec::new();
    let mut steppers: Vec<Value> = Vec::new();

    for (n, var) in plan.vars.iter().enumerate() {
        match &var.kind {
            VarKind::With(init) => outer.push((var.name.clone(), *init)),
            VarKind::For(ForIter::Seq(seq)) | VarKind::For(ForIter::Suffixes(seq)) => {
                let suffixes = matches!(var.kind, VarKind::For(ForIter::Suffixes(_)));
                let buf = state_name("buf", n);
                let idx = state_name("idx", n);
                let materialized = b.call("copy-seq", &[*seq])?;
                outer.push((buf.clone(), materialized));
                outer.push((idx.clone(), Value::Int(0)));

                let buf_sym = b.sym(&buf);
                let idx_sym = b.sym(&idx);
                let len = b.call("len", &[buf_sym])?;
                let test = b.call(">=", &[idx_sym, len])?;
                exhausted.push(test);

                let element = if suffixes {
                    // `:on` — the suffix from here, as a fresh `Vector`. CL
                    // hands out the shared tail conses; an `Iter` has none.
                    let it = b.call("iter", &[buf_sym])?;
                    let end = b.call("len", &[buf_sym])?;
                    b.call("subseq", &[it, idx_sym, end])?
                } else {
                    b.call("get", &[buf_sym, idx_sym])?
                };
                per_pass.push((var.name.clone(), element));

                let next = b.call("+", &[idx_sym, Value::Int(1)])?;
                let step = b.setf(&idx, next)?;
                steppers.push(step);
            }
            VarKind::For(ForIter::Range { from, end, by, down }) => {
                outer.push((var.name.clone(), *from));
                let v = b.sym(&var.name);
                match end {
                    RangeEnd::Inclusive(limit) => {
                        let op = if *down { "<" } else { ">" };
                        let test = b.call(op, &[v, *limit])?;
                        exhausted.push(test);
                    }
                    RangeEnd::Exclusive(limit) => {
                        let op = if *down { "<=" } else { ">=" };
                        let test = b.call(op, &[v, *limit])?;
                        exhausted.push(test);
                    }
                    RangeEnd::Open => {}
                }
                let stride = by.unwrap_or(Value::Int(1));
                let op = if *down { "-" } else { "+" };
                let next = b.call(op, &[v, stride])?;
                let step = b.setf(&var.name, next)?;
                steppers.push(step);
            }
            VarKind::For(ForIter::Assign { init, next }) => {
                outer.push((var.name.clone(), *init));
                // Without `:then`, CL re-evaluates the same form each pass.
                let step = b.setf(&var.name, next.unwrap_or(*init))?;
                steppers.push(step);
            }
        }
    }

    if let Some(count) = &plan.repeat {
        let n = state_name("rep", plan.vars.len());
        let limit = state_name("rep-limit", plan.vars.len());
        outer.push((limit.clone(), *count));
        outer.push((n.clone(), Value::Int(0)));
        let n_sym = b.sym(&n);
        let limit_sym = b.sym(&limit);
        let test = b.call(">=", &[n_sym, limit_sym])?;
        exhausted.push(test);
        let next = b.call("+", &[n_sym, Value::Int(1)])?;
        let step = b.setf(&n, next)?;
        steppers.push(step);
    }

    for (name, init) in &resolved.acc_inits {
        outer.push((name.clone(), *init));
    }
    outer.push((FIRST_FLAG.to_string(), Value::Bool(true)));

    // The loop body, in CL's order: initialize (done by the `let*` above),
    // then test, then work, then step. Written here as step-then-test-then-
    // work with the first pass's step suppressed, which is the same sequence
    // and puts the exit in one place.
    let mut body: Vec<Value> = Vec::new();

    let cleared = b.setf(FIRST_FLAG, Value::Bool(false))?;
    let cleared = b.statements(&[cleared])?;
    let stepped = b.statements(&steppers)?;
    let flag = b.sym(FIRST_FLAG);
    let stepping = b.if_(flag, cleared, stepped)?;
    body.push(stepping);

    if let Some(test) = fold_or(&mut b, &exhausted)? {
        let done = leave_normally(&mut b, plan, resolved)?;
        let branch = b.when(test, done)?;
        body.push(branch);
    }

    let mut work: Vec<Value> = Vec::with_capacity(plan.steps.len());
    for step in &plan.steps {
        let form = build_step(&mut b, step, plan, resolved)?;
        work.push(form);
    }
    let work = b.statements(&work)?;
    let work = b.let_star(&per_pass, work)?;
    body.push(work);

    let head = b.sym("loop");
    let mut loop_items = Vec::with_capacity(body.len() + 1);
    loop_items.push(head);
    loop_items.extend_from_slice(&body);
    let looping = b.list(&loop_items)?;

    let out = if plan.initially.is_empty() {
        looping
    } else {
        // `progn`, not `statements`: the loop is the last form here, and its
        // value is the whole construct's.
        let mut items = plan.initially.clone();
        items.push(looping);
        b.progn(&items)?
    };
    let out = b.let_star(&outer, out)?;
    // The scope drops here, unrooting `out` — the caller re-roots before it
    // allocates again (`forms::rooted`).
    Ok(out)
}

/// `(if t0 true (if t1 true …))`, or `None` when nothing can exhaust — a
/// `loop` with no bounded `:for`, no `:repeat` and no `:while` runs until
/// something inside it leaves, exactly as the simple `loop` does.
fn fold_or(b: &mut Build, tests: &[Value]) -> Result<Option<Value>, Error> {
    let mut it = tests.iter().rev();
    let Some(last) = it.next() else { return Ok(None) };
    let mut acc = *last;
    for t in it {
        acc = b.if_(*t, Value::Bool(true), acc)?;
    }
    Ok(Some(acc))
}

/// `(progn finally… (return RESULT))` — how the loop ends when it was not
/// cut short: `:finally` runs, then the accumulation (or the verdict, or
/// unit) is the value. An explicit `:return` clause skips this, as in CL.
///
/// The trailing `return` is left off when the last `:finally` form is
/// already one — CL's `finally (return x)` idiom, and the only way a loop
/// with nothing to accumulate can name its own answer. Emitting both would
/// have the loop leaving with two different types (the user's, and the unit
/// this would otherwise return).
fn leave_normally(b: &mut Build, plan: &Plan, resolved: &Resolved) -> Result<Value, Error> {
    if finally_returns(b, plan) {
        return b.statements(&plan.finally);
    }
    let value = match plan.accs.first() {
        Some(first) => b.sym(&first.name),
        None => match plan.verdict {
            Some(Verdict::Boolean) => Value::Bool(true),
            Some(Verdict::Optional) => resolved
                .thereis_none
                .ok_or_else(|| err("internal: `:thereis` left no `none` form"))?,
            None => Value::Empty,
        },
    };
    let leave = b.call("return", &[value])?;
    let mut forms = plan.finally.clone();
    forms.push(leave);
    b.statements(&forms)
}

/// Whether the last `:finally` form is a `(return …)` written by the user.
fn finally_returns(b: &Build, plan: &Plan) -> bool {
    let Some(last) = plan.finally.last() else { return false };
    let Ok(items) = b.s.list_to_vec(*last) else { return false };
    matches!(items.first(), Some(Value::Symbol(id)) if *id == SymId::RETURN)
}

fn build_step(b: &mut Build, step: &Step, plan: &Plan, resolved: &Resolved) -> Result<Value, Error> {
    match step {
        Step::Do(forms) => b.statements(forms),
        Step::Return(form) => {
            let leave = b.call("return", &[*form])?;
            b.statements(&[leave])
        }
        Step::Stop { expr, when } => {
            // `:while`/`:until` end the loop *normally*, so `:finally` still
            // runs and the accumulation is still the answer — which is why
            // this is the same ending as exhaustion rather than a `break`.
            let test = if *when { *expr } else { b.call("not", &[*expr])? };
            let done = leave_normally(b, plan, resolved)?;
            b.when(test, done)
        }
        Step::Assert { expr, want } => {
            let test = if *want { *expr } else { b.call("not", &[*expr])? };
            let leave = b.call("return", &[Value::Bool(false)])?;
            let leave = b.statements(&[leave])?;
            b.if_(test, Value::Empty, leave)
        }
        Step::Thereis { expr } => {
            let tmp = b.next_tmp("thereis");
            let tmp_sym = b.sym(&tmp);
            let leave = b.call("return", &[tmp_sym])?;
            let leave = b.statements(&[leave])?;
            let is_some = b.call("is-some", &[tmp_sym])?;
            let branch = b.when(is_some, leave)?;
            b.let_star(&[(tmp, *expr)], branch)
        }
        Step::Accumulate { acc, expr, into } => build_accumulate(b, *acc, *expr, into),
        Step::Cond { test, want, then, els } => {
            let then_form = build_step(b, then, plan, resolved)?;
            let else_form = match els {
                Some(e) => build_step(b, e, plan, resolved)?,
                None => Value::Empty,
            };
            let (then_form, else_form) =
                if *want { (then_form, else_form) } else { (else_form, then_form) };
            b.if_(*test, then_form, else_form)
        }
    }
}

fn build_accumulate(b: &mut Build, acc: Acc, expr: Value, into: &str) -> Result<Value, Error> {
    match acc {
        Acc::Collect => {
            let target = b.sym(into);
            let push = b.call("push", &[target, expr])?;
            b.statements(&[push])
        }
        Acc::Append => {
            // `(setf acc (append (iter acc) e))` rather than pushing element
            // by element: `append` is the one operation that takes any `Iter`
            // on the right, and the loop DSL should not need a nested loop of
            // its own to say "and these too".
            let target = b.sym(into);
            let it = b.call("iter", &[target])?;
            let joined = b.call("append", &[it, expr])?;
            let set = b.setf(into, joined)?;
            b.statements(&[set])
        }
        Acc::Sum => {
            let target = b.sym(into);
            let sum = b.call("+", &[target, expr])?;
            let set = b.setf(into, sum)?;
            b.statements(&[set])
        }
        Acc::Count => {
            let target = b.sym(into);
            let inc = b.call("+", &[target, Value::Int(1)])?;
            let set = b.setf(into, inc)?;
            let set = b.statements(&[set])?;
            b.when(expr, set)
        }
        Acc::Maximize | Acc::Minimize => {
            // (let* ((tmp EXPR))
            //   (setf acc (match acc
            //               ((some best) (Option::some (if (OP tmp best) tmp best)))
            //               ((none) (Option::some tmp)))))
            //
            // `Option<T>`, not a running `T`: CL's `maximize` over an empty
            // sequence answers nil, and there is no least element of an
            // arbitrary `Ord` type to start from.
            let tmp = b.next_tmp("extremum");
            let best = b.next_tmp("best");
            let tmp_sym = b.sym(&tmp);
            let best_sym = b.sym(&best);
            let target = b.sym(into);
            let op = if acc == Acc::Maximize { ">" } else { "<" };

            let cmp = b.call(op, &[tmp_sym, best_sym])?;
            let pick = b.if_(cmp, tmp_sym, best_sym)?;
            let some_pick = b.call_path("Option::some", &[pick])?;
            let some_head = b.sym("some");
            let some_pat = b.list(&[some_head, best_sym])?;
            let some_arm = b.list(&[some_pat, some_pick])?;

            let some_tmp = b.call_path("Option::some", &[tmp_sym])?;
            let none_head = b.sym("none");
            let none_pat = b.list(&[none_head])?;
            let none_arm = b.list(&[none_pat, some_tmp])?;

            let matched = b.call("match", &[target, some_arm, none_arm])?;
            let set = b.setf(into, matched)?;
            let set = b.statements(&[set])?;
            b.let_star(&[(tmp, expr)], set)
        }
    }
}

// ------------------------------------------------------------- type probes

/// A form whose type is the type `var` will have — checked, never evaluated,
/// by `Checker::check_loop_dsl`'s first pass.
pub(super) fn var_probe(heap: &mut Heap, var: &Var) -> Result<Value, Error> {
    let mut scope = crate::RootScope::new(heap);
    let mut b = Build { s: &mut scope, tmp: 0 };
    match &var.kind {
        VarKind::With(init) => Ok(*init),
        VarKind::For(ForIter::Range { from, .. }) => Ok(*from),
        VarKind::For(ForIter::Assign { init, .. }) => Ok(*init),
        VarKind::For(ForIter::Seq(seq)) => {
            let buf = b.call("copy-seq", &[*seq])?;
            b.call("get", &[buf, Value::Int(0)])
        }
        VarKind::For(ForIter::Suffixes(seq)) => b.call("copy-seq", &[*seq]),
    }
}

/// A form whose type is the type `acc` accumulates.
pub(super) fn acc_probe(heap: &mut Heap, acc: &AccVar) -> Result<Value, Error> {
    match acc.acc {
        // `:append` splices a sequence in, so what it accumulates is that
        // sequence's element, not the sequence.
        Acc::Append => {
            let mut scope = crate::RootScope::new(heap);
            let mut b = Build { s: &mut scope, tmp: 0 };
            let buf = b.call("copy-seq", &[acc.sample])?;
            b.call("get", &[buf, Value::Int(0)])
        }
        _ => Ok(acc.sample),
    }
}

/// The `:thereis` expression, if the loop has one.
pub(super) fn thereis_expr(plan: &Plan) -> Option<Value> {
    fn walk(step: &Step) -> Option<Value> {
        match step {
            Step::Thereis { expr } => Some(*expr),
            Step::Cond { then, els, .. } => {
                walk(then).or_else(|| els.as_deref().and_then(walk))
            }
            _ => None,
        }
    }
    plan.steps.iter().find_map(walk)
}
