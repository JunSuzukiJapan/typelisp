//! The CL list/sequence catalog added by
//! [cl-parity-plan.md](../docs/dev/cl-parity-plan.md) Phase 3a/3b/3c/3d.
//!
//! Everything here is a prelude definition over the `Iter` trait (or, for the
//! destructive half, a `defmethod` on `Vector<T>`), so these need the prelude
//! loaded. The results are compared through their `~a` rendering, which is how
//! a `Vector` is legible without reaching into the heap element by element.

extern crate typelisp;
use typelisp::{load_prelude, Checker, Heap, Interp, Reader, Value};

/// Runs `src` with the prelude loaded and renders the last value with `~a`.
///
/// The `~a` wrapping happens *inside* the run, so the result is a `Value::Str`
/// in the run's own heap rather than a structure that would have to outlive
/// it.
fn show(expr: &str) -> String {
    let src = format!("(format false \"~a\" {})", expr);
    let mut h = Heap::with_capacity(1 << 18);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut h, &mut chk, &mut interp);
    let r = Reader::new();
    let vs = r.read_all(&mut h, &src).expect("read failed");
    let mut last = Value::Empty;
    for v in vs {
        let tl = chk.check_form(&mut h, &interp, v).expect("check failed");
        if let Some(val) = interp.exec(&mut h, tl).expect("eval failed") {
            last = val;
        }
    }
    match last {
        Value::Str(id) => h.string(id).to_string(),
        other => panic!("expected a string, got {:?}", other),
    }
}

/// `expr` evaluated with `a` = `#<vector 1 2 2 3>` and `b` = `#<vector 2 3 4>`
/// in scope, plus `al` = an alist of `1->"one"`, `2->"two"`.
fn with_fixtures(expr: &str) -> String {
    show(&format!(
        "(let ((a (the Vector<i32> (Vector::new)))
               (b (the Vector<i32> (Vector::new)))
               (al (the Vector<cons-cell<i32,string>> (Vector::new))))
           (progn
             (push a 1) (push a 2) (push a 2) (push a 3)
             (push b 2) (push b 3) (push b 4)
             (push al (cons 1 \"one\")) (push al (cons 2 \"two\"))
             {}))",
        expr
    ))
}

// ---------------------------------------------------------------- Phase 3a

#[test]
fn the_named_positional_accessors_delegate_to_nth() {
    assert_eq!(with_fixtures("(first (iter a))"), "(some 1)");
    assert_eq!(with_fixtures("(second (iter a))"), "(some 2)");
    assert_eq!(with_fixtures("(fourth (iter a))"), "(some 3)");
    // Past the end is `none`, matching `nth`, not CL's `nil`.
    assert_eq!(with_fixtures("(tenth (iter a))"), "none");
}

#[test]
fn rest_and_copy_seq_materialize_fresh_vectors() {
    assert_eq!(with_fixtures("(rest (iter a))"), "#<vector 2 2 3>");
    assert_eq!(with_fixtures("(copy-seq (iter a))"), "#<vector 1 2 2 3>");
    // `rest` of a one-element sequence, and of an empty one.
    assert_eq!(show("(rest (iter (the Vector<i32> (Vector::filled 1 7))))"), "#<vector>");
}

#[test]
fn revappend_reverses_the_first_argument_only() {
    assert_eq!(with_fixtures("(revappend (iter b) (iter b))"), "#<vector 4 3 2 2 3 4>");
}

#[test]
fn vector_filled_builds_n_copies() {
    assert_eq!(show("(the Vector<i32> (Vector::filled 3 7))"), "#<vector 7 7 7>");
    assert_eq!(show("(the Vector<i32> (Vector::filled 0 7))"), "#<vector>");
}

/// The `c*r` family over nested pairs. `cadr` wants `cons-cell<A,cons-cell<B,C>>`,
/// which is what `(cons 1 (cons 2 3))` builds — these are pair accessors, not
/// list accessors, because `cons-cell` has no empty-list constructor.
#[test]
fn the_cxr_family_walks_nested_pairs() {
    assert_eq!(show("(caar (cons (cons 1 2) 3))"), "1");
    assert_eq!(show("(cdar (cons (cons 1 2) 3))"), "2");
    assert_eq!(show("(cadr (cons 1 (cons 2 3)))"), "2");
    assert_eq!(show("(cddr (cons 1 (cons 2 3)))"), "3");
    assert_eq!(show("(caddr (cons 1 (cons 2 (cons 3 4))))"), "3");
    assert_eq!(show("(cddddr (cons 0 (cons 1 (cons 2 (cons 3 9)))))"), "9");
}

// ---------------------------------------------------------------- Phase 3b

#[test]
fn the_negated_search_variants_mirror_their_positives() {
    assert_eq!(with_fixtures("(notany (iter a) (lambda ((x i32)) bool (> x 9)))"), "true");
    assert_eq!(with_fixtures("(notevery (iter a) (lambda ((x i32)) bool (> x 1)))"), "true");
    assert_eq!(with_fixtures("(find-if-not (iter a) (lambda ((x i32)) bool (< x 2)))"), "(some 2)");
    assert_eq!(with_fixtures("(count-if-not (iter a) (lambda ((x i32)) bool (< x 2)))"), "3");
    assert_eq!(
        with_fixtures("(remove-if-not (iter a) (lambda ((x i32)) bool (> x 1)))"),
        "#<vector 2 2 3>"
    );
    // `member-if` reports `bool`, the same departure `member` already makes.
    assert_eq!(with_fixtures("(member-if (iter a) (lambda ((x i32)) bool (> x 2)))"), "true");
    assert_eq!(with_fixtures("(member-if-not (iter a) (lambda ((x i32)) bool (> x 0)))"), "false");
}

#[test]
fn remove_and_remove_duplicates_keep_first_appearance_order() {
    assert_eq!(with_fixtures("(remove 2 (iter a))"), "#<vector 1 3>");
    assert_eq!(with_fixtures("(remove-duplicates (iter a))"), "#<vector 1 2 3>");
}

#[test]
fn substitute_replaces_by_value_and_by_predicate() {
    assert_eq!(with_fixtures("(substitute 9 2 (iter a))"), "#<vector 1 9 9 3>");
    assert_eq!(
        with_fixtures("(substitute-if 0 (lambda ((x i32)) bool (> x 2)) (iter a))"),
        "#<vector 1 2 2 0>"
    );
}

#[test]
fn the_alist_catalog_searches_by_key_value_and_predicate() {
    assert_eq!(
        with_fixtures("(assoc-if (iter al) (lambda ((k i32)) bool (= k 2)))"),
        "(some #<cons-cell 2 two>)"
    );
    assert_eq!(with_fixtures("(rassoc \"two\" (iter al))"), "(some #<cons-cell 2 two>)");
    assert_eq!(
        with_fixtures("(rassoc-if (iter al) (lambda ((s string)) bool (equal s \"one\")))"),
        "(some #<cons-cell 1 one>)"
    );
    // `acons` puts the new pair in front, as CL's cons does.
    assert_eq!(
        with_fixtures("(acons 3 \"three\" (iter al))"),
        "#<vector #<cons-cell 3 three> #<cons-cell 1 one> #<cons-cell 2 two>>"
    );
    // `pairlis` stops at the shorter of the two.
    assert_eq!(
        with_fixtures("(pairlis (iter a) (iter b))"),
        "#<vector #<cons-cell 1 2> #<cons-cell 2 3> #<cons-cell 2 4>>"
    );
}

#[test]
fn the_mapping_variants_cover_two_sequences_effects_and_tails() {
    assert_eq!(
        with_fixtures("(map2 (iter a) (iter b) (lambda ((x i32) (y i32)) i32 (+ x y)))"),
        "#<vector 3 5 6>"
    );
    assert_eq!(
        with_fixtures("(mapcan (iter b) (lambda ((x i32)) Vector<i32> (Vector::filled 2 x)))"),
        "#<vector 2 2 3 3 4 4>"
    );
    // `maplist` walks successive tails, so the lengths count down.
    assert_eq!(
        with_fixtures("(maplist (iter b) (lambda ((t Vector<i32>)) i32 (len t)))"),
        "#<vector 3 2 1>"
    );
}

#[test]
fn merge_orders_the_concatenation() {
    assert_eq!(
        with_fixtures("(merge (iter b) (iter a) (lambda ((x i32) (y i32)) bool (< x y)))"),
        "#<vector 1 2 2 2 3 3 4>"
    );
}

// ---------------------------------------------------------------- Phase 3c

#[test]
fn the_set_operations_answer_in_first_appearance_order() {
    // `adjoin` puts the new element in front and is a no-op when present.
    assert_eq!(with_fixtures("(adjoin 9 (iter a))"), "#<vector 9 1 2 2 3>");
    assert_eq!(with_fixtures("(adjoin 2 (iter a))"), "#<vector 1 2 2 3>");
    assert_eq!(with_fixtures("(union (iter a) (iter b))"), "#<vector 1 2 3 4>");
    assert_eq!(with_fixtures("(intersection (iter a) (iter b))"), "#<vector 2 3>");
    assert_eq!(with_fixtures("(set-difference (iter a) (iter b))"), "#<vector 1>");
    assert_eq!(with_fixtures("(set-exclusive-or (iter a) (iter b))"), "#<vector 1 4>");
    assert_eq!(with_fixtures("(subsetp (iter a) (iter b))"), "false");
    assert_eq!(with_fixtures("(subsetp (iter b) (iter b))"), "true");
}

/// `tailp`/`ldiff` ask about suffixes by *value*: CL asks whether `tail` is one
/// of `whole`'s own conses, and there is no shared structure here to ask about.
#[test]
fn tailp_and_ldiff_work_on_value_suffixes() {
    assert_eq!(with_fixtures("(tailp (iter (subseq (iter a) 2 4)) (iter a))"), "true");
    assert_eq!(with_fixtures("(tailp (iter b) (iter a))"), "false");
    assert_eq!(with_fixtures("(ldiff (iter a) (iter (subseq (iter a) 2 4)))"), "#<vector 1 2>");
    // Not a suffix: CL returns a copy of the whole list.
    assert_eq!(with_fixtures("(ldiff (iter a) (iter b))"), "#<vector 1 2 2 3>");
}

// ---------------------------------------------------------------- Phase 3d

/// The destructive half returns the receiver *and* mutates it, so the same
/// binding shows the change afterwards — the property that makes them worth
/// having over their functional twins.
#[test]
fn nreverse_mutates_the_receiver_in_place() {
    assert_eq!(
        show(
            "(let ((v (the Vector<i32> (Vector::new))))
               (progn (push v 1) (push v 2) (push v 3) (nreverse v) v))"
        ),
        "#<vector 3 2 1>"
    );
}

#[test]
fn the_delete_family_shortens_the_receiver_in_place() {
    let mk = |call: &str| {
        show(&format!(
            "(let ((v (the Vector<i32> (Vector::new))))
               (progn (push v 1) (push v 2) (push v 2) (push v 3) {} v))",
            call
        ))
    };
    assert_eq!(mk("(delete v 2)"), "#<vector 1 3>");
    assert_eq!(mk("(delete-if v (lambda ((x i32)) bool (> x 1)))"), "#<vector 1>");
    assert_eq!(mk("(delete-if-not v (lambda ((x i32)) bool (> x 1)))"), "#<vector 2 2 3>");
    assert_eq!(mk("(delete-duplicates v)"), "#<vector 1 2 3>");
    assert_eq!(mk("(nsubstitute v 9 2)"), "#<vector 1 9 9 3>");
    assert_eq!(mk("(nsubstitute-if v 0 (lambda ((x i32)) bool (evenp x)))"), "#<vector 1 0 0 3>");
    assert_eq!(mk("(nbutlast v)"), "#<vector 1 2 2>");
}

/// `fill`/`replace`/`map-into` write into the receiver without changing its
/// length, copying `(min (len self) (len src))` elements.
#[test]
fn the_in_place_writers_leave_the_length_alone() {
    let mk = |call: &str| {
        show(&format!(
            "(let ((v (the Vector<i32> (Vector::new))) (w (the Vector<i32> (Vector::new))))
               (progn (push v 1) (push v 2) (push v 3) (push w 7) (push w 8) {} v))",
            call
        ))
    };
    assert_eq!(mk("(fill v 5)"), "#<vector 5 5 5>");
    // `w` is shorter, so only the first two positions are written.
    assert_eq!(mk("(replace v w)"), "#<vector 7 8 3>");
    assert_eq!(mk("(map-into v w (lambda ((x i32)) i32 (* x 10)))"), "#<vector 70 80 3>");
}

#[test]
fn nconc_appends_into_the_receiver_and_nreconc_reverses_first() {
    assert_eq!(
        show(
            "(let ((v (the Vector<i32> (Vector::new))) (w (the Vector<i32> (Vector::new))))
               (progn (push v 1) (push v 2) (push w 3) (nconc v w) v))"
        ),
        "#<vector 1 2 3>"
    );
    assert_eq!(
        show(
            "(let ((v (the Vector<i32> (Vector::new))) (w (the Vector<i32> (Vector::new))))
               (progn (push v 1) (push v 2) (push w 3) (nreconc v w)))"
        ),
        "#<vector 2 1 3>"
    );
}

/// `rplaca`/`rplacd` return the cell, so they compose the way CL's do —
/// `cons-cell`'s own `set-car`/`set-cdr` return `()`.
#[test]
fn rplaca_and_rplacd_return_the_mutated_cell() {
    assert_eq!(show("(rplaca (cons 1 2) 9)"), "#<cons-cell 9 2>");
    assert_eq!(show("(rplacd (rplaca (cons 1 2) 9) 8)"), "#<cons-cell 9 8>");
}
