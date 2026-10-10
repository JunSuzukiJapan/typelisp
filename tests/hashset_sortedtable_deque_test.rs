//! `HashSet<T>`, `SortedTable<K,V>` and `Deque<T>`: created with `make`,
//! named after `HashTable`'s methods, iterable with `iter`, printable with
//! `print-object`. Every case runs interpreted and compiled.

mod common;
use common::{check_err, eval_string, eval_string_compiled};

/// `body` (a `string`-valued expression), interpreted and inside a compiled
/// function, both expected to give `expected`.
fn both(body: &str, expected: &str) {
    let interpreted = eval_string(body);
    assert_eq!(interpreted, expected, "interpreted: {}", body);
    let compiled = eval_string_compiled(&format!("(defun go () string {})\n(compile go)\n(go)", body));
    assert_eq!(compiled, expected, "compiled: {}", body);
}

#[test]
fn a_hashset_holds_each_element_once() {
    both(
        r##"(let ((s (the HashSet<int> (HashSet::make))))
             (let ((added (list (insert s 3) (insert s 1) (insert s 3))))
               (format false "~s ~a ~a ~a ~a ~a"
                 added (contains s 1) (contains s 2) (remove s 1) (remove s 1) (count s))))"##,
        "(true true false) true false true false 1",
    );
}

/// Elements are compared by their own `Hash`/`Eq`, as `HashTable` keys are.
#[test]
fn a_hashset_of_strings_compares_by_content() {
    both(
        r##"(let ((s (the HashSet<string> (HashSet::make))))
             (insert s "a")
             (insert s (append "a" ""))
             (clear s)
             (insert s "b")
             (format false "~a ~s" (count s) (collect (iter s))))"##,
        r##"1 #("b")"##,
    );
}

#[test]
fn a_hashset_element_type_must_implement_hash() {
    let msg = check_err("(defun f () bool (let ((s (the HashSet<f64> (HashSet::make)))) (insert s 1.0)))");
    assert!(msg.contains("hash"), "{}", msg);
}

#[test]
fn a_sortedtable_keeps_its_keys_in_order() {
    both(
        r##"(let ((t (the SortedTable<string,int> (SortedTable::make))))
             (set t "b" 2) (set t "c" 3) (set t "a" 1) (set t "b" 20)
             (format false "~s ~s ~s ~s ~s"
               (keys t) (values t) (get t "b") (get t "zz") (collect (iter t))))"##,
        r##"#("a" "b" "c") #(1 20 3) (some 20) none #(#{"a" 1} #{"b" 20} #{"c" 3})"##,
    );
}

#[test]
fn a_sortedtable_remove_and_clear() {
    both(
        r##"(let ((t (the SortedTable<int,string> (SortedTable::make))))
             (dotimes (i 5) (set t (- 10 i) (format false "~a" i)))
             (let ((r1 (remove t 8)) (r2 (remove t 8)) (n (count t)) (ks (keys t)))
               (clear t)
               (format false "~s ~s ~a ~s ~a" r1 r2 n ks (count t))))"##,
        r##"(some "2") none 4 #(6 7 9 10) 0"##,
    );
}

#[test]
fn a_sortedtable_key_type_must_implement_ord() {
    let msg = check_err(
        "(defstruct p (x int))\n(defun f () () (let ((t (the SortedTable<p,int> (SortedTable::make)))) (set t (p::new 1) 1)))",
    );
    assert!(msg.contains("ord"), "{}", msg);
}

#[test]
fn a_deque_takes_and_gives_at_both_ends() {
    both(
        r##"(let ((d (the Deque<int> (Deque::make))))
             (push-back d 1) (push-back d 2) (push-front d 0) (push-front d -1)
             (let ((all (collect (iter d))) (f (front d)) (b (back d)) (g (get d 2)) (out (get d 4)))
               (set d 0 -9)
               (format false "~s ~s ~s ~s ~s ~s ~s ~s"
                 all f b g out (pop-front d) (pop-back d) (count d))))"##,
        "#(-1 0 1 2) (some -1) (some 2) (some 1) none (some -9) (some 2) 2",
    );
}

/// Pushing at one end and popping at the other moves elements across
/// between the deque's two halves.
#[test]
fn a_deque_pops_what_was_pushed_at_the_other_end() {
    both(
        r##"(let ((d (the Deque<int> (Deque::make))) (a "") (b ""))
             (dotimes (i 6) (push-front d i))
             (dotimes (i 3) (setf a (format false "~a~a" a (unwrap (pop-back d)))))
             (dotimes (i 3) (push-back d (+ 10 i)))
             (while-let ((some x) (pop-front d)) (setf b (format false "~a ~a" b x)))
             (format false "~a |~a| ~s ~s" a b (pop-back d) (count d)))"##,
        "012 | 5 4 3 10 11 12| none 0",
    );
}

#[test]
fn setting_past_the_end_of_a_deque_panics() {
    let msg = common::eval_err("(let ((d (the Deque<int> (Deque::make)))) (push-back d 1) (set d 3 0))");
    assert!(msg.contains("Deque::set: index 3 is out of range for count 1"), "{}", msg);
}

#[test]
fn the_collections_print_their_elements() {
    both(
        r##"(let ((s (the HashSet<string> (HashSet::make)))
                  (t (the SortedTable<int,string> (SortedTable::make)))
                  (d (the Deque<char> (Deque::make))))
             (insert s "x")
             (set t 2 "b") (set t 1 "a")
             (push-back d #\q) (push-front d #\p)
             (format false "~s ~s ~s ~a" s t d t))"##,
        r##"#<hashset "x"> #<sortedtable 1 "a" 2 "b"> #<deque #\p #\q> #<sortedtable 1 a 2 b>"##,
    );
}
