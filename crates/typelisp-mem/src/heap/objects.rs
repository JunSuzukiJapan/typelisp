//! Symbols, paths, strings and the boxed objects: allocation and access.

use crate::prelude::*;
use crate::sync::atomic::Ordering;
use crate::symbols::{self, SymRef};
use crate::value::{BoxId, BoxedObj, FloatBox, NarrowInt, PathId, StrId, StructPayload, TypeId, Value};
use super::{Heap, NAMES_HELD, NamesGuard};

impl Heap {
    // ---- symbols ----------------------------------------------------------

    /// Intern a symbol by name, returning its `Value::Symbol`.
    ///
    /// typelisp symbols are **case-insensitive**, so names are folded to a
    /// canonical lowercase form: `Foo`, `FOO`, `foo` all intern to the same
    /// [`SymRef`], and [`symbol_spelling`](Self::symbol_spelling) returns the canonical
    /// (lowercase) spelling.
    pub fn intern_symbol(&mut self, name: &str) -> Value {
        Value::Symbol(symbols::intern(name))
    }

    /// What names mean in this session: its modules, its types and their
    /// members, what its `use`s brought in. Shared by every view of the heap.
    ///
    /// Panics when this thread already holds the guard: the mutex is not
    /// reentrant, so a second lock would wait forever instead of failing.
    pub fn names(&self) -> NamesGuard<'_> {
        let at = &self.shared.names as *const _ as usize;
        NAMES_HELD.with(|held| {
            let mut held = held.borrow_mut();
            assert!(!held.contains(&at), "this thread already holds the session's names; a second lock would wait forever");
            held.push(at);
        });
        NamesGuard { guard: self.shared.names.lock(), at }
    }

    /// Intern a symbol as written inside module `ns` — the reader's entry
    /// point, and the only one that can place a symbol anywhere but the root
    /// (see [`symbols::intern_in`] for the inheritance rule).
    pub fn intern_symbol_in(&mut self, ns: symbols::NsId, name: &str) -> Value {
        Value::Symbol(symbols::intern_in(ns, name))
    }

    /// The keyword named `name` (no colon) — [`symbols::intern_keyword`].
    pub fn intern_keyword(&mut self, name: &str) -> Value {
        Value::Symbol(symbols::intern_keyword(name))
    }

    /// How an interned symbol is written — [`SymRef::spelling`]: the name,
    /// with a keyword's colon in front. The compiler keys symbols by this
    /// when it needs a string, so a keyword is never mistaken for the plain
    /// symbol of the same name. CL's `symbol-name` is [`SymRef::name`].
    ///
    /// Takes `&self` only for call-site compatibility: a symbol carries its
    /// own spelling, so this needs no heap at all.
    pub fn symbol_spelling(&self, id: SymRef) -> &'static str {
        id.spelling()
    }


    // ---- paths ------------------------------------------------------------

    /// Intern a `::` path from its symbol segments, returning a `Value::Path`.
    /// Paths are permanent (they reference only permanent symbols), and equal
    /// segment sequences share one [`PathId`].
    pub fn intern_path(&mut self, segs: &[SymRef]) -> Value {
        let mut interning = self.shared.interning.write();
        if let Some(&id) = interning.path_ids.get(segs) {
            return Value::Path(PathId(id));
        }
        let id = interning.paths.len() as u32;
        interning.paths.push(segs.to_vec());
        interning.path_ids.insert(segs.to_vec(), id);
        Value::Path(PathId(id))
    }

    /// The symbol segments of an interned path.
    ///
    /// Returns a mapped read guard rather than `&[SymRef]`: the table lives
    /// behind `shared.interning`'s lock, and `crate::sync::RwLockReadGuard`
    /// is what lets this narrow "the whole table, read-locked" down to "this
    /// one entry" while keeping the lock held for exactly the guard's
    /// lifetime (see `HeapShared`'s doc comment).
    pub fn path_segments(&self, id: PathId) -> crate::sync::MappedRwLockReadGuard<'_, [SymRef]> {
        crate::sync::RwLockReadGuard::map(self.shared.interning.read_recursive(), |i| i.paths[id.0 as usize].as_slice())
    }

    // ---- strings ----------------------------------------------------------

    /// Store a string, returning its `Value::Str`. Strings are GC-collected.
    pub fn alloc_string(&mut self, s: String) -> Value {
        self.debug_assert_not_native("alloc_string");
        let mut strings = self.shared.strings.write();
        let id = strings.store.alloc(s);
        self.shared.spare_strings.store(strings.store.spare_bytes(), Ordering::Relaxed);
        Value::Str(StrId(id))
    }

    /// The contents of a stored string. See `path_segments`'s doc comment
    /// for why this returns a mapped guard rather than `&str`.
    pub fn string(&self, id: StrId) -> crate::sync::MappedRwLockReadGuard<'_, str> {
        crate::sync::RwLockReadGuard::map(self.shared.strings.read_recursive(), |s| {
            s.store.slots[id.0 as usize].as_deref().expect("dangling StrId")
        })
    }

    // ---- boxed objects ------------------------------------------------------

    /// Store a [`BoxedObj`], returning its `Value::Boxed`. Boxed objects are
    /// GC-collected — the same [`SlotStore`] as
    /// [`alloc_string`](Self::alloc_string), and `gc`'s mark phase traces into
    /// whatever `Value`s the payload itself holds (see [`gc`](Self::gc)).
    ///
    /// Not a safepoint, and neither is [`alloc_string`](Self::alloc_string):
    /// callers hold unrooted values across them (a struct's fields, on their
    /// way into `obj`), which a collection here would sweep. What they
    /// allocate is collected at the next [`safepoint`](Self::safepoint).
    pub(super) fn alloc_boxed(&mut self, obj: BoxedObj) -> Value {
        self.debug_assert_not_native("alloc_boxed");
        let mut boxes = self.shared.boxes.write();
        let id = boxes.alloc(obj);
        self.shared.spare_boxes.store(boxes.spare_bytes(), Ordering::Relaxed);
        Value::Boxed(BoxId(id))
    }

    /// Store an `f64`, returning its `Value::Boxed` — `Sexpr`'s `f64`
    /// variant's runtime representation (see [`BoxedObj`]'s doc comment for
    /// why a float can't be an immediate `Value` variant the way `Int`/
    /// `Char`/`Bool` are).
    pub fn alloc_f64(&mut self, f: f64) -> Value {
        self.alloc_boxed(BoxedObj::Float64(f))
    }

    /// Store an `f32`. A separate box from [`alloc_f64`](Self::alloc_f64),
    /// because `f32` and `f64` are separate types and the box has to say
    /// which one it is — see [`BoxedObj`]'s doc comment.
    pub fn alloc_f32(&mut self, f: f32) -> Value {
        self.alloc_boxed(BoxedObj::Float32(f))
    }

    /// The `f64` behind a boxed `f64`. Panics if `id` doesn't hold a
    /// `BoxedObj::Float64` — an internal-invariant trap, not a user-facing
    /// error, the same convention [`string`](Self::string)'s dangling-`StrId`
    /// panic already uses: a correctly type-checked program never passes a
    /// mismatched `BoxId` here.
    ///
    /// Deliberately *not* widening an `f32` box: a caller that would accept
    /// either width has to say so, with [`float_box`](Self::float_box).
    pub fn f64_value(&self, id: BoxId) -> f64 {
        match &self.shared.boxes.read_recursive().slots[id.0 as usize] {
            Some(BoxedObj::Float64(f)) => *f,
            _ => panic!("BoxId does not hold an f64"),
        }
    }

    /// The `f32` behind a boxed `f32`. Panics like
    /// [`f64_value`](Self::f64_value), and for the same reason.
    pub fn f32_value(&self, id: BoxId) -> f32 {
        match &self.shared.boxes.read_recursive().slots[id.0 as usize] {
            Some(BoxedObj::Float32(f)) => *f,
            _ => panic!("BoxId does not hold an f32"),
        }
    }

    /// A boxed float of either width, as the width it actually is — `None`
    /// when `id` is not a float box at all.
    ///
    /// The one reader for code that genuinely handles both (printing,
    /// equality, `Sexpr` dispatch). It hands back the width rather than
    /// hiding it, so those call sites still have to decide what each width
    /// means instead of silently treating everything as `f64`.
    pub fn float_box(&self, id: BoxId) -> Option<FloatBox> {
        match &self.shared.boxes.read_recursive().slots[id.0 as usize] {
            Some(BoxedObj::Float32(f)) => Some(FloatBox::F32(*f)),
            Some(BoxedObj::Float64(f)) => Some(FloatBox::F64(*f)),
            _ => None,
        }
    }

    /// Store an `i8`/`i16`/`i32`/`u8`/`u16`/`u32`, returning its
    /// `Value::Boxed` — the runtime representation of those six `Sexpr`
    /// variants. `i32` boxes like the rest since `int` took the bare fixnum
    /// word: inside a `Sexpr` the word alone no longer says which of the two
    /// it is.
    pub fn alloc_narrow(&mut self, n: NarrowInt) -> Value {
        self.alloc_boxed(BoxedObj::Narrow(n))
    }

    /// A boxed narrow integer, as the type it actually is — `None` when `id`
    /// is not one.
    ///
    /// The integer twin of [`float_box`](Self::float_box), and the only
    /// reader: there is deliberately no `narrow_value` that hands back the
    /// word without its width, because that word alone is what the five
    /// types were being confused through in the first place.
    pub fn narrow_box(&self, id: BoxId) -> Option<NarrowInt> {
        match &self.shared.boxes.read_recursive().slots[id.0 as usize] {
            Some(BoxedObj::Narrow(n)) => Some(*n),
            _ => None,
        }
    }

    /// True if `id` holds a boxed narrow integer of exactly this width and
    /// signedness — the positive per-type test, like
    /// [`is_f32`](Self::is_f32)/[`is_f64`](Self::is_f64). There is no
    /// width-agnostic `is_narrow` for the same reason there is no
    /// `is_float`.
    pub fn is_narrow(&self, id: BoxId, ty: crate::fixed::FixedType) -> bool {
        match &self.shared.boxes.read_recursive().slots[id.0 as usize] {
            Some(BoxedObj::Narrow(n)) => n.ty() == ty,
            _ => false,
        }
    }

    /// The primitive type a numeric box *is*, spelled the way the checker
    /// spells it (`typelisp_mem::ConcreteNames::type_key`) — `None` for
    /// every other box (a struct, an enum, a closure, a cell, ...), which
    /// carries an interned type key instead.
    ///
    /// The boxes below are the values whose type is written down nowhere but
    /// in the box itself: a struct says what it is through its stored key, an
    /// immediate `Value::Char` *is* a `char`, but a box holding the number
    /// `200` is a `u8` or a `u16` only because it says so. Dispatchers that
    /// need the receiver's type name (`format`'s `~/name/`) ask here.
    pub fn primitive_box_type_name(&self, id: BoxId) -> Option<&'static str> {
        match &self.shared.boxes.read_recursive().slots[id.0 as usize] {
            Some(BoxedObj::Float32(_)) => Some("f32"),
            Some(BoxedObj::Float64(_)) => Some("f64"),
            Some(BoxedObj::Narrow(n)) => Some(match n {
                NarrowInt::I8(_) => "i8",
                NarrowInt::I16(_) => "i16",
                NarrowInt::I32(_) => "i32",
                NarrowInt::U8(_) => "u8",
                NarrowInt::U16(_) => "u16",
                NarrowInt::U32(_) => "u32",
            }),
            // A bignum box is an `int`: the `bignum` type was folded into it.
            Some(BoxedObj::Bignum(_)) => Some("int"),
            Some(BoxedObj::Ratio(_)) => Some("ratio"),
            Some(BoxedObj::RandomState(_)) => Some("random-state"),
            _ => None,
        }
    }

    /// Store a bignum box **whatever its value** — the raw allocation
    /// [`int_from_bigint`](Self::int_from_bigint) performs once it has
    /// decided the value does not fit a fixnum. Not a producer of `int`s:
    /// an `int` is canonical (a box exists only past the fixnum range), and
    /// every producer goes through `canonical_int`/`int_from_bigint`. This
    /// exists for the tests of that invariant, which need the one shape
    /// nothing else may make.
    pub fn alloc_bignum_unchecked(&mut self, n: num_bigint::BigInt) -> Value {
        self.box_bignum(n)
    }

    /// Stores `n` in a bignum box: every bignum is made here, and the first
    /// one made installs how a bignum is written in a message
    /// ([`bignum_text`](Self::bignum_text)).
    fn box_bignum(&mut self, n: num_bigint::BigInt) -> Value {
        TYPELISP_BIGNUM_TEXT.store(decimal as fn(&num_bigint::BigInt) -> String as *mut (), crate::sync::atomic::Ordering::Relaxed);
        self.alloc_boxed(BoxedObj::Bignum(n))
    }

    /// The bignum in `id` in decimal, for a message. Through what the
    /// bignum's maker installed: writing a `BigInt` is linked into an
    /// executable only when the program can make one.
    pub fn bignum_text(&self, id: BoxId) -> String {
        let text = TYPELISP_BIGNUM_TEXT.load(crate::sync::atomic::Ordering::Relaxed);
        if text.is_null() {
            panic!("internal error: a bignum box exists and nothing made one");
        }
        // SAFETY: only ever `decimal`'s address, stored by `box_bignum`.
        let text: fn(&num_bigint::BigInt) -> String = unsafe { core::mem::transmute(text) };
        text(&self.bignum_value(id))
    }

    /// An `int` from a machine-wide result: a fixnum when the value fits
    /// 63 bits, a bignum box otherwise. **The one place that decides**, so
    /// that "a box exists only when the value does not fit a fixnum" is an
    /// invariant every consumer can lean on (`eq`, `eql`, hashing, the
    /// compiled fast paths' one-bit test) rather than a convention each
    /// producer may or may not have followed.
    ///
    /// `i128` because that is what a 63-bit `+`/`-`/`*` can produce before
    /// anyone has decided whether it fits.
    pub fn canonical_int(&mut self, n: i128) -> Value {
        if n >= i128::from(crate::tagged::FIXNUM_MIN) && n <= i128::from(crate::tagged::FIXNUM_MAX) {
            Value::Int(n as i64)
        } else {
            self.box_bignum(num_bigint::BigInt::from(n))
        }
    }

    /// [`canonical_int`](Self::canonical_int) for a result that is already a
    /// `BigInt` — the arbitrary-precision arithmetic's own output, demoted
    /// to a fixnum whenever it fits.
    pub fn int_from_bigint(&mut self, n: num_bigint::BigInt) -> Value {
        match i64::try_from(&n) {
            Ok(v) if crate::tagged::fixnum_fits(v) => Value::Int(v),
            _ => self.box_bignum(n),
        }
    }

    /// Whether the bignum in `id` holds a value a fixnum could — which an
    /// `int` never does (`canonical_int`), and which the boundaries check
    /// so that a non-canonical box is reported where it was made rather
    /// than where `eq` first disagrees.
    pub fn bignum_fits_fixnum(&self, id: BoxId) -> bool {
        matches!(i64::try_from(&*self.bignum_value(id)), Ok(v) if crate::tagged::fixnum_fits(v))
    }

    /// The `BigInt` behind a boxed bignum. Panics if `id` doesn't hold a
    /// `BoxedObj::Bignum` — same internal-invariant-trap convention as
    /// [`f64_value`](Self::f64_value).
    /// See `path_segments`'s doc comment for why this returns a mapped guard
    /// rather than `&num_bigint::BigInt`.
    pub fn bignum_value(&self, id: BoxId) -> crate::sync::MappedRwLockReadGuard<'_, num_bigint::BigInt> {
        crate::sync::RwLockReadGuard::map(self.shared.boxes.read_recursive(), |b| match &b.slots[id.0 as usize] {
            Some(BoxedObj::Bignum(n)) => n,
            _ => panic!("BoxId does not hold a Bignum"),
        })
    }

    /// True if `id` holds a `BoxedObj::Bignum` — the peer of
    /// [`is_f64`](Self::is_f64) for callers decoding an unknown
    /// `Value::Boxed`.
    pub fn is_bignum(&self, id: BoxId) -> bool {
        matches!(self.shared.boxes.read_recursive().slots[id.0 as usize], Some(BoxedObj::Bignum(_)))
    }

    /// Store a `ratio` (exact rational), returning its `Value::Boxed` — the
    /// boxed counterpart of an `int`'s bignum box. The
    /// caller passes any `BigRational`; the crate keeps it reduced with a
    /// positive denominator.
    pub fn alloc_ratio(&mut self, r: num_rational::BigRational) -> Value {
        self.alloc_boxed(BoxedObj::Ratio(r))
    }

    /// The `BigRational` behind a boxed ratio. Panics if `id` doesn't hold a
    /// `BoxedObj::Ratio` — same convention as [`bignum_value`](Self::bignum_value).
    /// See `path_segments`'s doc comment for why this returns a mapped guard
    /// rather than `&num_rational::BigRational`.
    pub fn ratio_value(&self, id: BoxId) -> crate::sync::MappedRwLockReadGuard<'_, num_rational::BigRational> {
        crate::sync::RwLockReadGuard::map(self.shared.boxes.read_recursive(), |b| match &b.slots[id.0 as usize] {
            Some(BoxedObj::Ratio(r)) => r,
            _ => panic!("BoxId does not hold a Ratio"),
        })
    }

    /// True if `id` holds a `BoxedObj::Ratio`.
    pub fn is_ratio(&self, id: BoxId) -> bool {
        matches!(self.shared.boxes.read_recursive().slots[id.0 as usize], Some(BoxedObj::Ratio(_)))
    }

    /// Store a struct-shaped [`BoxedObj`] with fixed- or variable-length
    /// `fields`, returning its `Value::Boxed` — the runtime representation a
    /// `defstruct` instance, `Vector<T>`, and `cons-cell<K,V>` all share
    /// (see `BoxedObj`'s doc comment). `type_key` is what a later builtin
    /// method dispatch (`eval_builtin_method`) uses to decide what the
    /// fields mean; this layer itself doesn't interpret it.
    ///
    /// Takes an interned [`TypeId`] rather than a name, so a struct cannot
    /// be built without its type identity already existing as a comparable id
    /// — see [`crate::intern_type`].
    pub fn alloc_struct(&mut self, type_key: TypeId, fields: Vec<Value>) -> Value {
        self.alloc_boxed(BoxedObj::Struct { type_key, payload: StructPayload::Fields(fields) })
    }

    /// The type identity of a boxed struct — what a "is this a `point`?" test
    /// compares. Panics if `id` doesn't hold a `BoxedObj::Struct` — same
    /// internal-invariant-trap convention as [`f64_value`](Self::f64_value).
    pub fn struct_type_key(&self, id: BoxId) -> TypeId {
        match &self.shared.boxes.read_recursive().slots[id.0 as usize] {
            Some(BoxedObj::Struct { type_key, .. }) => *type_key,
            _ => panic!("BoxId does not hold a Struct"),
        }
    }


    /// The number of fields a boxed struct holds — e.g. `Vector<T>::length`
    /// reads this directly, unlike a `defstruct` instance's fixed arity
    /// (known statically, so callers with a static field index rarely need
    /// this). Panics if `id` doesn't hold a `BoxedObj::Struct`.
    pub fn struct_field_count(&self, id: BoxId) -> usize {
        match &self.shared.boxes.read_recursive().slots[id.0 as usize] {
            Some(BoxedObj::Struct { payload: StructPayload::Fields(fields), .. }) => fields.len(),
            _ => panic!("BoxId does not hold a Struct"),
        }
    }

    /// The `idx`-th field of a boxed struct. Panics if `id` doesn't hold a
    /// `BoxedObj::Struct`, or if `idx` is out of range — the checker (for a
    /// `defstruct` field access) or the builtin method itself (for
    /// `Vector<T>`'s own bounds check) is responsible for guaranteeing that
    /// never happens, the same convention [`car`](Self::car)/[`cdr`](Self::cdr)
    /// use for a non-cons argument.
    pub fn struct_field(&self, id: BoxId, idx: usize) -> Value {
        match &self.shared.boxes.read_recursive().slots[id.0 as usize] {
            Some(BoxedObj::Struct { payload: StructPayload::Fields(fields), .. }) => {
                *fields.get(idx).unwrap_or_else(|| panic!("struct field index {} out of range", idx))
            }
            _ => panic!("BoxId does not hold a Struct"),
        }
    }

    /// Overwrites the `idx`-th field of a boxed struct in place — the
    /// mutable-reference-semantics counterpart to [`struct_field`](Self::struct_field),
    /// mirroring [`set_car`](Self::set_car)/[`set_cdr`](Self::set_cdr)'s
    /// in-place mutation of a cons cell. Panics under the same conditions as
    /// [`struct_field`](Self::struct_field).
    pub fn struct_set_field(&mut self, id: BoxId, idx: usize, val: Value) {
        match self.shared.boxes.write().slots[id.0 as usize].as_mut() {
            Some(BoxedObj::Struct { payload: StructPayload::Fields(fields), .. }) => {
                if idx >= fields.len() {
                    panic!("struct field index {} out of range", idx);
                }
                fields[idx] = val;
            }
            _ => panic!("BoxId does not hold a Struct"),
        }
    }

    /// Appends a new field, growing a boxed struct's field count by one —
    /// `Vector<T>::push`'s primitive (a `defstruct` instance's field count is
    /// otherwise fixed for its lifetime once [`alloc_struct`](Self::alloc_struct)
    /// returns, since only `Vector<T>`'s builtin methods ever call this).
    /// Panics if `id` doesn't hold a `BoxedObj::Struct`.
    pub fn struct_push_field(&mut self, id: BoxId, val: Value) {
        match self.shared.boxes.write().slots[id.0 as usize].as_mut() {
            Some(BoxedObj::Struct { payload: StructPayload::Fields(fields), .. }) => fields.push(val),
            _ => panic!("BoxId does not hold a Struct"),
        }
    }

    /// Removes and returns the last field of a boxed struct, shrinking its
    /// field count by one — `Vector<T>::pop`'s primitive, the inverse of
    /// [`struct_push_field`](Self::struct_push_field). Unlike
    /// [`struct_field`](Self::struct_field)'s out-of-range panic, an empty
    /// vector is a legitimate outcome here (`None`), not an invariant
    /// violation — `pop` returns `Option<T>` (matching `HashTable<K,V>`'s
    /// `remove`, and `Vec::pop`'s own idiomatic Rust signature), unlike
    /// `get`/`set`'s "out of range panics" convention. Panics only if `id`
    /// doesn't hold a `BoxedObj::Struct` at all.
    pub fn struct_pop_field(&mut self, id: BoxId) -> Option<Value> {
        match self.shared.boxes.write().slots[id.0 as usize].as_mut() {
            Some(BoxedObj::Struct { payload: StructPayload::Fields(fields), .. }) => fields.pop(),
            _ => panic!("BoxId does not hold a Struct"),
        }
    }

    /// The arity of the `#{..}` datum `id` is, or `None` when it is no such
    /// box (see [`crate::tuple`]).
    pub fn sexpr_tuple_arity(&self, id: BoxId) -> Option<usize> {
        if !self.is_struct(id) {
            return None;
        }
        crate::tuple::sexpr_tuple_arity(self.struct_type_key(id))
    }

    /// True if `id` holds a `BoxedObj::Struct` with a `StructPayload::Fields`
    /// payload (a `defstruct`/`Vector<T>`/`cons-cell<K,V>` instance) —
    /// deliberately narrower than "any `Struct`" now that [`StructPayload`]
    /// has more than one shape (see [`is_hashtable`](Self::is_hashtable)),
    /// so a caller holding only a `Value::Boxed` (e.g. decoding a struct
    /// field or a generic `match` scrutinee back into an interpreter-level
    /// value) can tell a nested struct apart from a boxed float *or* a
    /// `HashTable` without risking [`f64_value`](Self::f64_value)'s or
    /// [`struct_field`](Self::struct_field)'s "wrong kind" panic.
    pub fn is_struct(&self, id: BoxId) -> bool {
        matches!(self.shared.boxes.read_recursive().slots[id.0 as usize], Some(BoxedObj::Struct { payload: StructPayload::Fields(_), .. }))
    }

    /// True if `id` holds a `BoxedObj::Struct` with a `StructPayload::Map`
    /// payload (a `HashTable<K,V>` instance) — see
    /// [`is_struct`](Self::is_struct)'s doc comment for why this needs to be
    /// a separate predicate rather than folded into it.
    pub fn is_hashtable(&self, id: BoxId) -> bool {
        matches!(self.shared.boxes.read_recursive().slots[id.0 as usize], Some(BoxedObj::Struct { payload: StructPayload::Map(..), .. }))
    }

    // ---- enums ----------------------------------------------------------------

    /// Store an enum (sum-ADT) value — variant index `variant` of `type_key`
    /// with that variant's `fields` — returning its `Value::Boxed`; the
    /// runtime representation `Option<T>`/`Result<T,E>`/user `defenum`
    /// instances all share (see [`BoxedObj::Enum`]'s doc comment for why
    /// this is a variant of its own, not a `Struct`). The enum counterpart
    /// of [`alloc_struct`](Self::alloc_struct); like `type_key` there,
    /// `variant` is stored uninterpreted — which variant means what is the
    /// checker's business, this layer only carries the index.
    pub fn alloc_enum(&mut self, type_key: TypeId, variant: usize, fields: Vec<Value>) -> Value {
        self.alloc_boxed(BoxedObj::Enum { type_key, variant, fields })
    }

    /// True if `id` holds a `BoxedObj::Enum` — the enum peer of
    /// [`is_struct`](Self::is_struct), for callers decoding an unknown
    /// `Value::Boxed`.
    pub fn is_enum(&self, id: BoxId) -> bool {
        matches!(self.shared.boxes.read_recursive().slots[id.0 as usize], Some(BoxedObj::Enum { .. }))
    }

    /// The type identity of a boxed enum value — the enum peer of
    /// [`struct_type_key`](Self::struct_type_key). Panics if `id` doesn't hold
    /// a `BoxedObj::Enum`.
    pub fn enum_type_key(&self, id: BoxId) -> TypeId {
        match &self.shared.boxes.read_recursive().slots[id.0 as usize] {
            Some(BoxedObj::Enum { type_key, .. }) => *type_key,
            _ => panic!("BoxId does not hold an Enum"),
        }
    }


    /// The variant index of a boxed enum value — what a `match` arm's tag
    /// test compares against. Panics if `id` doesn't hold a `BoxedObj::Enum`.
    pub fn enum_variant(&self, id: BoxId) -> usize {
        match &self.shared.boxes.read_recursive().slots[id.0 as usize] {
            Some(BoxedObj::Enum { variant, .. }) => *variant,
            _ => panic!("BoxId does not hold an Enum"),
        }
    }

    /// The number of fields the boxed enum value's variant carries — a
    /// `match` pattern's arity check reads this alongside
    /// [`enum_variant`](Self::enum_variant). Panics if `id` doesn't hold a
    /// `BoxedObj::Enum`.
    pub fn enum_field_count(&self, id: BoxId) -> usize {
        match &self.shared.boxes.read_recursive().slots[id.0 as usize] {
            Some(BoxedObj::Enum { fields, .. }) => fields.len(),
            _ => panic!("BoxId does not hold an Enum"),
        }
    }

    /// The `idx`-th field of a boxed enum value. Panics if `id` doesn't hold
    /// a `BoxedObj::Enum` or if `idx` is out of range — the checker's
    /// pattern-arity guarantee makes that an internal invariant, the same
    /// convention as [`struct_field`](Self::struct_field). No `set`
    /// counterpart exists: enum values are immutable.
    pub fn enum_field(&self, id: BoxId, idx: usize) -> Value {
        match &self.shared.boxes.read_recursive().slots[id.0 as usize] {
            Some(BoxedObj::Enum { fields, .. }) => {
                *fields.get(idx).unwrap_or_else(|| panic!("enum field index {} out of range", idx))
            }
            _ => panic!("BoxId does not hold an Enum"),
        }
    }

    // ---- trait objects --------------------------------------------------------

    /// Box `value` as a trait object dispatching through vtable `vtable_id` —
    /// see [`BoxedObj::Dyn`]. Like `alloc_enum`, `vtable_id` is stored
    /// uninterpreted: which table it names is the interpreter's/runtime's
    /// business, this layer only carries the number.
    pub fn alloc_dyn(&mut self, vtable_id: u32, value: Value) -> Value {
        self.alloc_boxed(BoxedObj::Dyn { vtable_id, value })
    }

    /// True if `id` holds a `BoxedObj::Dyn`, for a caller decoding an unknown
    /// `Value::Boxed` — note this is `false` for the *concrete* value inside,
    /// so `is_struct`/`is_enum` stay unaffected by boxing.
    pub fn is_dyn(&self, id: BoxId) -> bool {
        matches!(self.shared.boxes.read_recursive().slots[id.0 as usize], Some(BoxedObj::Dyn { .. }))
    }

    /// The vtable identifier of a trait object. Panics if `id` doesn't hold a
    /// `BoxedObj::Dyn` — same internal-invariant-trap convention as
    /// [`enum_variant`](Self::enum_variant).
    pub fn dyn_vtable_id(&self, id: BoxId) -> u32 {
        match &self.shared.boxes.read_recursive().slots[id.0 as usize] {
            Some(BoxedObj::Dyn { vtable_id, .. }) => *vtable_id,
            _ => panic!("BoxId does not hold a Dyn"),
        }
    }

    /// The concrete value inside a trait object. Panics if `id` doesn't hold
    /// a `BoxedObj::Dyn`.
    pub fn dyn_value(&self, id: BoxId) -> Value {
        match &self.shared.boxes.read_recursive().slots[id.0 as usize] {
            Some(BoxedObj::Dyn { value, .. }) => *value,
            _ => panic!("BoxId does not hold a Dyn"),
        }
    }

    // ---- cells ----------------------------------------------------------------

    /// Store a mutable variable slot holding `v`, returning an owning
    /// `Arc<BoxId>` handle — see [`BoxedObj::Cell`]. The cell (and thus its
    /// current contents) stays live for exactly as long as any clone of the
    /// handle does: `gc()` treats every still-referenced cell as a root by
    /// consulting the weak registry this populates (`shared.cell_registry`),
    /// so no caller-side root bookkeeping exists for cells at all — a
    /// collection triggered from *anywhere* (interpreter or compiled code)
    /// sees them. Unlike [`cons`](Self::cons), this can never itself trigger
    /// a collection (the box store grows on demand, it is not a fixed
    /// arena), so `v` may be un-rooted at the moment of the call.
    ///
    /// `Arc`, not `Rc`: the handle must stay a valid liveness proof no
    /// matter which thread ends up holding it or running `gc()` — see
    /// `shared.cell_registry`'s doc comment.
    pub fn alloc_cell(&mut self, v: Value) -> crate::sync::Arc<BoxId> {
        let id = match self.alloc_boxed(BoxedObj::Cell(v)) {
            Value::Boxed(id) => id,
            _ => unreachable!("alloc_boxed always returns Value::Boxed"),
        };
        let rc = crate::sync::Arc::new(id);
        self.shared.cell_registry.lock().push(crate::sync::Arc::downgrade(&rc));
        rc
    }

    /// The current contents of a cell. Panics if `id` doesn't hold a
    /// `BoxedObj::Cell` — same internal-invariant-trap convention as
    /// [`f64_value`](Self::f64_value).
    pub fn cell_get(&self, id: BoxId) -> Value {
        match &self.shared.boxes.read_recursive().slots[id.0 as usize] {
            Some(BoxedObj::Cell(v)) => *v,
            _ => panic!("BoxId does not hold a Cell"),
        }
    }

    /// Overwrites a cell's contents in place — `setf`'s primitive for a
    /// heap-cell-backed binding. Panics under the same conditions as
    /// [`cell_get`](Self::cell_get).
    pub fn cell_set(&mut self, id: BoxId, v: Value) {
        match self.shared.boxes.write().slots[id.0 as usize].as_mut() {
            Some(BoxedObj::Cell(slot)) => *slot = v,
            _ => panic!("BoxId does not hold a Cell"),
        }
    }

    /// [`cell_get`](Self::cell_get), or `None` when `id` does not hold a cell
    /// — the check and the read under one lock.
    pub fn try_cell_get(&self, id: BoxId) -> Option<Value> {
        match &self.shared.boxes.read_recursive().slots[id.0 as usize] {
            Some(BoxedObj::Cell(v)) => Some(*v),
            _ => None,
        }
    }

    /// True if `id` holds a `BoxedObj::Cell` — the peer of
    /// [`is_struct`](Self::is_struct)/[`is_hashtable`](Self::is_hashtable)
    /// for callers that decode a `Value::Boxed` without knowing its kind.
    pub fn is_cell(&self, id: BoxId) -> bool {
        matches!(self.shared.boxes.read_recursive().slots[id.0 as usize], Some(BoxedObj::Cell(_)))
    }

    /// [`alloc_cell`](Self::alloc_cell) without the owning-`Rc` liveness
    /// registration — for *compiled* code (`rt_cell_new`), which has no Rust
    /// `Rc` to hold: the cell's lifetime is governed purely by ordinary
    /// reachability (a GC root, a compiled closure's env slot, another
    /// cell, ...), exactly like any other boxed object. The caller must
    /// therefore make the returned value reachable before anything can
    /// trigger a collection; like `alloc_cell`, the allocation itself never
    /// triggers one.
    pub fn alloc_cell_unregistered(&mut self, v: Value) -> Value {
        self.alloc_boxed(BoxedObj::Cell(v))
    }

    // ---- closures --------------------------------------------------------------

    /// True if `id` holds a `BoxedObj::Float64` — the *positive* `f64` test
    /// callers decoding an unknown `Value::Boxed` must use now that "not a
    /// struct and not a hashtable" no longer implies float (cells and
    /// closures are boxed too).
    ///
    /// There is deliberately no width-agnostic `is_float`: a caller that
    /// accepts either width has to name both, or read the width out with
    /// [`float_box`](Self::float_box).
    pub fn is_f64(&self, id: BoxId) -> bool {
        matches!(self.shared.boxes.read_recursive().slots[id.0 as usize], Some(BoxedObj::Float64(_)))
    }

    /// True if `id` holds a `BoxedObj::Float32`. See [`is_f64`](Self::is_f64).
    pub fn is_f32(&self, id: BoxId) -> bool {
        matches!(self.shared.boxes.read_recursive().slots[id.0 as usize], Some(BoxedObj::Float32(_)))
    }

    // ---- compiled closures ------------------------------------------------------

    /// Store a compiled function value — native entry point `fn_ptr` plus
    /// its captured environment — returning its `Value::Boxed`; see
    /// [`BoxedObj::CompiledClosure`] for the `env`/`sexpr_mask` slot
    /// convention. Like [`alloc_cell`](Self::alloc_cell), never itself
    /// triggers a collection, so the env values may be un-rooted at the
    /// moment of the call.
    pub fn alloc_compiled_closure(
        &mut self,
        fn_ptr: usize,
        env: Vec<Value>,
        sexpr_mask: u64,
        body_abi: u8,
    ) -> Value {
        self.alloc_boxed(BoxedObj::CompiledClosure { fn_ptr, env, sexpr_mask, body_abi })
    }

    /// Which calling convention `fn_ptr` was compiled under — the classic
    /// `f(args, argc, env, envlen)` or the coroutine `f(frame)` (Phase C2).
    ///
    /// The closure has to carry this rather than the process deciding it once,
    /// because a single process really can hold both: the island loaded from
    /// its committed dump makes closures under whatever ABI *it* was compiled
    /// under, while code JIT-compiled in the same process makes them under the
    /// ABI this binary emits, and across an ABI change those are two different
    /// answers.
    ///
    /// Panics like [`compiled_closure_fnptr`](Self::compiled_closure_fnptr).
    pub fn compiled_closure_body_abi(&self, id: BoxId) -> u8 {
        match &self.shared.boxes.read_recursive().slots[id.0 as usize] {
            Some(BoxedObj::CompiledClosure { body_abi, .. }) => *body_abi,
            _ => panic!("BoxId does not hold a CompiledClosure"),
        }
    }

    /// True if `id` holds a `BoxedObj::CompiledClosure`. A function value can
    /// equally be an *interpreted* closure ([`is_closure`](Self::is_closure))
    /// or a built-in ([`is_builtin_fn`](Self::is_builtin_fn)); the three
    /// together are every `Type::Fn` value.
    pub fn is_compiled_closure(&self, id: BoxId) -> bool {
        matches!(self.shared.boxes.read_recursive().slots[id.0 as usize], Some(BoxedObj::CompiledClosure { .. }))
    }
}

impl Heap {
    // ---- random states ------------------------------------------------------

    /// Store a `random-state` at `seed`, returning its `Value::Boxed` — see
    /// [`BoxedObj::RandomState`] for why a stream is a box rather than an
    /// immediate. Allocates one box slot and never collects.
    pub fn alloc_random_state(&mut self, seed: u64) -> Value {
        self.alloc_boxed(BoxedObj::RandomState(seed))
    }

    /// True if `id` holds a `BoxedObj::RandomState`.
    pub fn is_random_state(&self, id: BoxId) -> bool {
        matches!(self.shared.boxes.read_recursive().slots[id.0 as usize], Some(BoxedObj::RandomState(_)))
    }

    /// A `random-state`'s current seed. Panics if `id` doesn't hold a
    /// `BoxedObj::RandomState` — the same internal-invariant-trap convention as
    /// [`f64_value`](Self::f64_value).
    pub fn random_state_seed(&self, id: BoxId) -> u64 {
        match &self.shared.boxes.read_recursive().slots[id.0 as usize] {
            Some(BoxedObj::RandomState(seed)) => *seed,
            _ => panic!("BoxId does not hold a RandomState"),
        }
    }

    /// Advance a `random-state` to `seed`, in place — every holder of this box
    /// observes the change, which is what makes it one shared stream. Panics
    /// like [`random_state_seed`](Self::random_state_seed).
    pub fn set_random_state_seed(&mut self, id: BoxId, seed: u64) {
        match &mut self.shared.boxes.write().slots[id.0 as usize] {
            Some(BoxedObj::RandomState(s)) => *s = seed,
            _ => panic!("BoxId does not hold a RandomState"),
        }
    }

    /// A compiled closure's native entry point. Panics if `id` doesn't hold
    /// a `BoxedObj::CompiledClosure` — the same internal-invariant-trap
    /// convention as [`f64_value`](Self::f64_value).
    pub fn compiled_closure_fnptr(&self, id: BoxId) -> usize {
        match &self.shared.boxes.read_recursive().slots[id.0 as usize] {
            Some(BoxedObj::CompiledClosure { fn_ptr, .. }) => *fn_ptr,
            _ => panic!("BoxId does not hold a CompiledClosure"),
        }
    }

    /// A compiled closure's function, its body ABI, its capture mask and its
    /// captures (appended to `env`) — or `None` when `id` does not hold one.
    /// What applying it needs, under one lock rather than one per part.
    pub fn compiled_closure_parts(&self, id: BoxId, env: &mut Vec<Value>) -> Option<(usize, u8, u64)> {
        match &self.shared.boxes.read_recursive().slots[id.0 as usize] {
            Some(BoxedObj::CompiledClosure { fn_ptr, env: captured, sexpr_mask, body_abi }) => {
                env.extend_from_slice(captured);
                Some((*fn_ptr, *body_abi, *sexpr_mask))
            }
            _ => None,
        }
    }

    /// A compiled closure's capture mask — bit `i` set means env slot `i`
    /// holds a real tagged value (see [`BoxedObj::CompiledClosure`]).
    /// Panics like [`compiled_closure_fnptr`](Self::compiled_closure_fnptr).
    pub fn compiled_closure_mask(&self, id: BoxId) -> u64 {
        match &self.shared.boxes.read_recursive().slots[id.0 as usize] {
            Some(BoxedObj::CompiledClosure { sexpr_mask, .. }) => *sexpr_mask,
            _ => panic!("BoxId does not hold a CompiledClosure"),
        }
    }

    /// The number of captured slots a compiled closure carries. Panics like
    /// [`compiled_closure_fnptr`](Self::compiled_closure_fnptr).
    pub fn compiled_closure_env_len(&self, id: BoxId) -> usize {
        match &self.shared.boxes.read_recursive().slots[id.0 as usize] {
            Some(BoxedObj::CompiledClosure { env, .. }) => env.len(),
            _ => panic!("BoxId does not hold a CompiledClosure"),
        }
    }

    /// The `idx`-th captured slot of a compiled closure. Panics if `id`
    /// doesn't hold a `BoxedObj::CompiledClosure` or if `idx` is out of
    /// range — the compiler's capture-layout guarantee makes that an
    /// internal invariant, same convention as [`enum_field`](Self::enum_field).
    pub fn compiled_closure_env_get(&self, id: BoxId, idx: usize) -> Value {
        match &self.shared.boxes.read_recursive().slots[id.0 as usize] {
            Some(BoxedObj::CompiledClosure { env, .. }) => {
                *env.get(idx).unwrap_or_else(|| panic!("compiled closure env index {} out of range", idx))
            }
            _ => panic!("BoxId does not hold a CompiledClosure"),
        }
    }
}

/// How [`Heap::bignum_text`] writes a bignum, installed by
/// [`Heap::box_bignum`]. An exported symbol, so that the optimizer does not
/// take the one value ever stored for the only one it can hold and call
/// `decimal` directly.
#[no_mangle]
static TYPELISP_BIGNUM_TEXT: crate::sync::atomic::AtomicPtr<()> = crate::sync::atomic::AtomicPtr::new(core::ptr::null_mut());

fn decimal(n: &num_bigint::BigInt) -> String {
    n.to_string()
}
