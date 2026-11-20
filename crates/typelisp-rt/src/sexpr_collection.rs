//! Building `Sexpr` vectors, tuples and arrays from lists.

use typelisp_abi::{active_heap, decode, encode, fatal, raise, Raised};
use typelisp_mem::{Heap, TypeId, Value};

/// The elements of a proper `Sexpr` list, in order. `who` names the caller
/// in the fatal message an improper list gets.
fn proper_list_elements(heap: &Heap, list: Value, who: &str) -> Vec<Value> {
    let mut out = Vec::new();
    let mut cur = list;
    loop {
        match cur {
            Value::Empty => return out,
            Value::Cons(_) => {
                match heap.car(cur) {
                    Ok(v) => out.push(v),
                    Err(_) => fatal(&format!("{}: argument is not a proper list", who)),
                }
                cur = match heap.cdr(cur) {
                    Ok(v) => v,
                    Err(_) => fatal(&format!("{}: argument is not a proper list", who)),
                };
            }
            _ => fatal(&format!("{}: argument is not a proper list", who)),
        }
    }
}

/// A quoted `#(..)` in compiled code: `args[0]` is a `Sexpr` list of the
/// elements (`core_bridge::quoted_form` takes the vector apart into one,
/// since compiled code cannot refer to the compiling heap's box), and the
/// result is the `Sexpr` `vector` variant's box holding them — what the
/// reader builds from the same text.
///
/// # Safety
///
/// `argc` must be `>= 1` and `args` must point to a valid tagged word; a
/// `Heap` must already be registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_list_to_sexpr_vector(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_list_to_sexpr_vector: expected 1 argument");
    }
    let heap = active_heap();
    let list = decode(*args);
    let elems = proper_list_elements(heap, list, "rt_list_to_sexpr_vector");
    // The elements are reachable only through `list`, which nothing else
    // roots while the box is allocated.
    heap.push_root(list);
    let v = heap.alloc_struct(TypeId::SEXPR_VECTOR, elems);
    heap.pop_root();
    encode(v)
}

/// A tuple box of `Option<Sexpr>` elements holding `elems`, or the catchable
/// error the interpreter raises when there are too few or too many of them.
fn sexpr_tuple_of(heap: &mut Heap, elems: Vec<Value>) -> Result<Value, Raised> {
    if !(1..=typelisp_mem::TUPLE_MAX_ARITY).contains(&elems.len()) {
        return Err(raise(format!("a tuple has 1 to {} elements, not {}", typelisp_mem::TUPLE_MAX_ARITY, elems.len())));
    }
    let key = typelisp_mem::sexpr_tuple_type(elems.len());
    Ok(heap.alloc_struct(key, elems))
}

/// A quoted `#{..}` in compiled code: `args[0]` is a `Sexpr` list of the
/// elements, as for [`rt_list_to_sexpr_vector`], and the result is the tuple
/// box the reader builds from the same text.
///
/// # Safety
///
/// `argc` must be `>= 1` and `args` must point to a valid tagged word; a
/// `Heap` must already be registered on this thread.
#[no_mangle]
pub unsafe extern "C-unwind" fn rt_list_to_sexpr_tuple(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_list_to_sexpr_tuple: expected 1 argument");
    }
    let heap = active_heap();
    let list = decode(*args);
    let elems = proper_list_elements(heap, list, "rt_list_to_sexpr_tuple");
    heap.push_root(list);
    let t = sexpr_tuple_of(heap, elems);
    heap.pop_root();
    match t {
        Ok(t) => encode(t),
        Err(raised) => raised.into_word(),
    }
}

/// `(construct sexpr tuple V)` in compiled code: the tuple of the elements of
/// the `Vector<Option<Sexpr>>` `args[0]`.
///
/// # Safety
///
/// `argc` must be `>= 1` and `args` must point to a tagged vector box; a
/// `Heap` must already be registered on this thread.
#[no_mangle]
pub unsafe extern "C-unwind" fn rt_vector_to_sexpr_tuple(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_vector_to_sexpr_tuple: expected 1 argument");
    }
    let heap = active_heap();
    let v = decode(*args);
    let Value::Boxed(id) = v else { fatal("rt_vector_to_sexpr_tuple: argument is not a vector") };
    let elems: Vec<Value> = (0..heap.struct_field_count(id)).map(|i| heap.struct_field(id, i)).collect();
    heap.push_root(v);
    let t = sexpr_tuple_of(heap, elems);
    heap.pop_root();
    match t {
        Ok(t) => encode(t),
        Err(raised) => raised.into_word(),
    }
}

/// The payload a `(Tuple v)` pattern binds: a fresh `Vector<Option<Sexpr>>`
/// of the elements of the tuple box `args[0]` — the interpreter's
/// `match_sexpr_core` `tuple` arm, for compiled code.
///
/// # Safety
///
/// `argc` must be `>= 1` and `args` must point to a tagged tuple box; a
/// `Heap` must already be registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_sexpr_tuple_items(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_sexpr_tuple_items: expected 1 argument");
    }
    let heap = active_heap();
    let t = decode(*args);
    let Value::Boxed(id) = t else { fatal("rt_sexpr_tuple_items: argument is not a tuple") };
    let elems: Vec<Value> = (0..heap.struct_field_count(id)).map(|i| heap.struct_field(id, i)).collect();
    heap.push_root(t);
    let v = heap.alloc_struct(TypeId::SEXPR_VECTOR, elems);
    heap.pop_root();
    encode(v)
}

/// A quoted `#nA(..)` in compiled code: `args[0]` is a list of the
/// dimensions as `int`s, `args[1]` a list of the elements in row-major order
/// (`core_bridge::quoted_form` again). The result is the `Sexpr` `array`
/// variant's box, built by the reader's own `alloc_sexpr_array`.
///
/// # Safety
///
/// `argc` must be `>= 2` and `args` must point to two valid tagged words; a
/// `Heap` must already be registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_lists_to_sexpr_array(args: *const i64, argc: u32) -> i64 {
    if argc < 2 {
        fatal("rt_lists_to_sexpr_array: expected 2 arguments");
    }
    let heap = active_heap();
    let dims_list = decode(*args);
    let data_list = decode(*args.add(1));
    let dims = proper_list_elements(heap, dims_list, "rt_lists_to_sexpr_array");
    let data = proper_list_elements(heap, data_list, "rt_lists_to_sexpr_array");
    // The elements are reachable only through `data_list`.
    heap.push_root(data_list);
    let array = typelisp_read::alloc_sexpr_array(heap, dims, data);
    heap.pop_root();
    encode(array)
}
