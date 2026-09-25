//! The CL character and string catalog added by
//! [cl-parity-plan.md](../docs/dev/cl-parity-plan.md) Phase 2a/2b — everything
//! `registry::char_assoc`/`string_assoc` do *not* provide, written as ordinary
//! prelude definitions on top of the ones they do.
//!
//! Every definition under test lives in `prelude.rs`'s `SOURCE`, so these all
//! need the prelude loaded. They also all deliberately avoid `upcase`/
//! `downcase`/`alphap`/`digitp`/`int->char`, which the island has no lowering
//! for — the prelude generator's `PRELUDE_COMPILE_UNSUPPORTED` reconcile is
//! the test that keeps that true, not anything here.

extern crate typelisp;
use typelisp::{load_prelude, Checker, Heap, Interp, Reader, Value};

/// Runs `src` with the prelude loaded and returns the last value, together
/// with the heap it lives in — a `string` result is a `Value::Str` index into
/// that heap, so the two cannot be separated.
fn eval_with_prelude(src: &str) -> (Heap, Value) {
    let mut h = Heap::with_capacity(1 << 18);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut h, &mut chk, &mut interp);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).expect("read failed");
    let mut last = Value::Empty;
    for v in vs {
        let tl = chk.check_form(&mut h, &interp, v).expect("check failed");
        if let Some(val) = interp.exec(&mut h, tl).expect("eval failed") {
            last = val;
        }
    }
    (h, last)
}

fn eval_ok(src: &str) -> Value {
    eval_with_prelude(src).1
}

fn eval_string(src: &str) -> String {
    let (h, v) = eval_with_prelude(src);
    match v {
        Value::Str(id) => h.string(id).to_string(),
        other => panic!("expected a string, got {:?}", other),
    }
}

fn eval_bool(src: &str) -> bool {
    match eval_ok(src) {
        Value::Bool(b) => b,
        other => panic!("expected a bool, got {:?}", other),
    }
}

// ----------------------------------------------------------------------
// Phase 2a — characters
// ----------------------------------------------------------------------

#[test]
fn char_inequality_is_the_negation_of_equal() {
    assert!(eval_bool("(/= #\\a #\\b)"));
    assert!(!eval_bool("(/= #\\a #\\a)"));
}

/// The checker's variadic `/=` sugar folds onto the binary method, so adding
/// the method is what makes the n-ary form work on characters at all.
///
/// The n-ary form compares *adjacent* pairs (`functions.md` §4.1: `(cmp a b
/// c)` becomes `(and (cmp a b) (cmp b c))`), which is where this language
/// parts company with CL — CL's `char/=` asks whether all the arguments are
/// pairwise distinct, so `(char/= #\a #\b #\a)` is false there and true
/// here. That is a pre-existing property of the sugar shared with `/=` on
/// numbers, not something the character method introduces.
#[test]
fn char_inequality_works_through_the_variadic_sugar() {
    assert!(eval_bool("(/= #\\a #\\b #\\c)"));
    assert!(!eval_bool("(/= #\\a #\\a #\\b)"));
    // Adjacent-pairs, not all-pairs: the repeat is not adjacent, so this holds.
    assert!(eval_bool("(/= #\\a #\\b #\\a)"));
}

#[test]
fn case_insensitive_order_comparisons_fold_both_operands() {
    assert!(eval_bool("(lessp #\\A #\\b)"));
    assert!(!eval_bool("(lessp #\\b #\\A)"));
    assert!(eval_bool("(greaterp #\\B #\\a)"));
    // Equal after folding: not less, not greater, but both of the "not"
    // forms hold — the boundary CL's `char-not-lessp` is defined at.
    assert!(!eval_bool("(lessp #\\A #\\a)"));
    assert!(eval_bool("(not-lessp #\\A #\\a)"));
    assert!(eval_bool("(not-greaterp #\\A #\\a)"));
}

#[test]
fn case_predicates_classify_ascii_letters_only() {
    assert!(eval_bool("(upper-casep #\\A)"));
    assert!(!eval_bool("(upper-casep #\\a)"));
    assert!(!eval_bool("(upper-casep #\\1)"));
    assert!(eval_bool("(lower-casep #\\a)"));
    assert!(!eval_bool("(lower-casep #\\A)"));
    // `both-case-p`: has both cases at all, so true for either letter case
    // and false for everything that has no case.
    assert!(eval_bool("(both-casep #\\a)"));
    assert!(eval_bool("(both-casep #\\Z)"));
    assert!(!eval_bool("(both-casep #\\1)"));
}

#[test]
fn alphanumericp_covers_letters_and_digits() {
    assert!(eval_bool("(alphanumericp #\\7)"));
    assert!(eval_bool("(alphanumericp #\\q)"));
    assert!(!eval_bool("(alphanumericp #\\?)"));
    assert!(!eval_bool("(alphanumericp #\\space)"));
}

/// CL's `graphic-char-p` includes space and excludes every control character;
/// `standard-char-p` is the graphic set plus newline and nothing else.
#[test]
fn graphic_and_standard_differ_exactly_at_newline() {
    assert!(eval_bool("(graphicp #\\space)"));
    assert!(eval_bool("(graphicp #\\A)"));
    assert!(!eval_bool("(graphicp #\\newline)"));
    assert!(!eval_bool("(graphicp #\\tab)"));
    assert!(eval_bool("(standardp #\\newline)"));
    assert!(eval_bool("(standardp #\\space)"));
    assert!(!eval_bool("(standardp #\\tab)"));
}

/// CL's `digit-char-p` — the digit's *weight*, not a boolean. The pre-existing
/// `digitp` keeps its boolean meaning (the prelude's own reader calls it as a
/// predicate), which is why this is a separate name.
#[test]
fn digit_weight_returns_the_weight_in_the_given_radix() {
    assert_eq!(eval_string("(format false \"~a\" (digit-weight #\\7))"), "(some 7)");
    assert_eq!(eval_string("(format false \"~a\" (digit-weight #\\f 16))"), "(some 15)");
    assert_eq!(eval_string("(format false \"~a\" (digit-weight #\\F 16))"), "(some 15)");
    // Out of range for the radix, and not a digit character at all.
    assert_eq!(eval_string("(format false \"~a\" (digit-weight #\\9 8))"), "none");
    assert_eq!(eval_string("(format false \"~a\" (digit-weight #\\?))"), "none");
}

#[test]
fn digit_to_char_is_the_inverse_of_digit_weight() {
    assert_eq!(eval_string("(format false \"~a\" (digit->char 7))"), "(some 7)");
    assert_eq!(eval_string("(format false \"~a\" (digit->char 15 16))"), "(some F)");
    assert_eq!(eval_string("(format false \"~a\" (digit->char 9 8))"), "none");
    assert_eq!(eval_string("(format false \"~a\" (digit->char -1))"), "none");
}

#[test]
fn char_names_round_trip_through_the_readers_own_table() {
    assert_eq!(eval_string("(format false \"~a\" (char->name #\\newline))"), "(some newline)");
    assert_eq!(eval_string("(format false \"~a\" (char->name #\\a))"), "none");
    // `name->char` is case-insensitive and takes the reader's aliases, since
    // the reader's own lookup does both.
    assert!(eval_bool("(equal (unwrap (name->char \"TAB\")) #\\tab)"));
    assert!(eval_bool("(equal (unwrap (name->char \"linefeed\")) #\\newline)"));
    assert_eq!(eval_string("(format false \"~a\" (name->char \"nope\"))"), "none");
}

// ----------------------------------------------------------------------
// Phase 2b — strings
// ----------------------------------------------------------------------

#[test]
fn string_inequality_is_the_negation_of_equal() {
    assert!(eval_bool("(/= \"a\" \"b\")"));
    assert!(!eval_bool("(/= \"a\" \"a\")"));
}

#[test]
fn case_insensitive_string_order_folds_and_breaks_ties_by_length() {
    assert!(eval_bool("(lessp \"ABC\" \"abd\")"));
    assert!(!eval_bool("(lessp \"abd\" \"ABC\")"));
    // A common prefix: the shorter string is the lesser one.
    assert!(eval_bool("(lessp \"ab\" \"abc\")"));
    assert!(eval_bool("(greaterp \"abc\" \"AB\")"));
    // Equal after folding.
    assert!(eval_bool("(not-lessp \"ABC\" \"abc\")"));
    assert!(eval_bool("(not-greaterp \"ABC\" \"abc\")"));
    assert!(!eval_bool("(lessp \"ABC\" \"abc\")"));
}

#[test]
fn string_filled_builds_n_copies_of_a_character() {
    assert_eq!(eval_string("(string::filled 5 #\\x)"), "xxxxx");
    assert_eq!(eval_string("(string::filled 0 #\\x)"), "");
}

#[test]
fn search_finds_the_first_occurrence() {
    assert_eq!(eval_string("(format false \"~a\" (search \"hello world\" \"o w\"))"), "(some 4)");
    assert_eq!(eval_string("(format false \"~a\" (search \"hello\" \"zz\"))"), "none");
    // CL: the empty sequence occurs at index 0.
    assert_eq!(eval_string("(format false \"~a\" (search \"hello\" \"\"))"), "(some 0)");
    // The needle longer than the haystack must not read past the end.
    assert_eq!(eval_string("(format false \"~a\" (search \"ab\" \"abcd\"))"), "none");
}

#[test]
fn mismatch_reports_the_first_differing_index() {
    assert_eq!(eval_string("(format false \"~a\" (mismatch \"abc\" \"abd\"))"), "(some 2)");
    assert_eq!(eval_string("(format false \"~a\" (mismatch \"abc\" \"abc\"))"), "none");
    // One a prefix of the other: the mismatch is at the shorter one's end.
    assert_eq!(eval_string("(format false \"~a\" (mismatch \"ab\" \"abc\"))"), "(some 2)");
    assert_eq!(eval_string("(format false \"~a\" (mismatch \"abc\" \"ab\"))"), "(some 2)");
}

#[test]
fn the_trim_family_strips_only_characters_in_the_bag() {
    assert_eq!(eval_string("(trim \"  hi  \")"), "hi");
    assert_eq!(eval_string("(left-trim \"xxhixx\" \"x\")"), "hixx");
    assert_eq!(eval_string("(right-trim \"xxhixx\" \"x\")"), "xxhi");
    assert_eq!(eval_string("(trim \"xyhixy\" \"xy\")"), "hi");
    // Nothing to strip, and everything stripped.
    assert_eq!(eval_string("(trim \"hi\")"), "hi");
    assert_eq!(eval_string("(trim \"   \")"), "");
}

/// CL's `string-capitalize`: a word is a maximal run of alphanumerics, its
/// first character upcased and the rest downcased. `3rd` is one word starting
/// with a digit, so `r`/`d` are downcased and nothing is upcased in it.
#[test]
fn capitalize_upcases_each_words_first_alphanumeric() {
    assert_eq!(eval_string("(capitalize \"hello wORLD 3rd-time\")"), "Hello World 3rd-Time");
    assert_eq!(eval_string("(capitalize \"\")"), "");
    assert_eq!(eval_string("(capitalize \"a\")"), "A");
}

#[test]
fn split_keeps_empty_pieces_between_adjacent_separators() {
    assert_eq!(eval_string("(format false \"~a\" (split \"a,b,,c\" \",\"))"), "#<vector<string> a b  c>");
    // A multi-character separator, and one that never occurs.
    assert_eq!(eval_string("(format false \"~a\" (split \"a, b\" \", \"))"), "#<vector<string> a b>");
    assert_eq!(eval_string("(format false \"~a\" (split \"abc\" \",\"))"), "#<vector<string> abc>");
}

#[test]
fn to_string_renders_each_scalar_the_way_tilde_a_does() {
    assert_eq!(eval_string("(to-string 42)"), "42");
    assert_eq!(eval_string("(to-string 1.5)"), "1.5");
    assert_eq!(eval_string("(to-string true)"), "true");
    assert_eq!(eval_string("(to-string #\\z)"), "z");
    assert_eq!(eval_string("(to-string \"already\")"), "already");
}

/// `parse-int`'s answer as text: the integer, or `err:` and the message, so
/// one assertion covers both arms of the `Result`.
fn parsed(call: &str) -> String {
    eval_string(&format!(
        "(match {} ((ok n) (to-string n)) ((err e) (append \"err:\" (message e))))",
        call
    ))
}

/// CL's `parse-integer`: surrounding whitespace is skipped, one sign is read,
/// and anything else in the string is an error.
#[test]
fn parse_int_skips_surrounding_whitespace_and_reads_a_sign() {
    assert_eq!(parsed("(parse-int \"42\")"), "42");
    assert_eq!(parsed("(parse-int \"  -17 \")"), "-17");
    assert_eq!(parsed("(parse-int \"+8\")"), "8");
    assert_eq!(parsed("(parse-int \"\\t\\n5\\r\")"), "5");
    assert_eq!(parsed("(parse-int \"12x\")"), "err:parse-int: invalid integer literal: \"12x\"");
    assert_eq!(parsed("(parse-int \"1 2\")"), "err:parse-int: invalid integer literal: \"1 2\"");
    assert_eq!(parsed("(parse-int \"-\")"), "err:parse-int: invalid integer literal: \"-\"");
    assert_eq!(parsed("(parse-int \"   \")"), "err:parse-int: invalid integer literal: \"   \"");
    assert_eq!(parsed("(parse-int \"\")"), "err:parse-int: invalid integer literal: \"\"");
}

/// The answer is an `int`, so a number too wide for a fixnum comes back as
/// one anyway rather than wrapping.
#[test]
fn parse_int_answers_numbers_wider_than_a_fixnum() {
    assert_eq!(parsed("(parse-int \"123456789012345678901234567890\")"), "123456789012345678901234567890");
    assert_eq!(parsed("(parse-int \"-123456789012345678901234567890\")"), "-123456789012345678901234567890");
}

/// `:radix` picks the digits (either case above 9); a digit too large for the
/// radix is junk like any other character.
#[test]
fn parse_int_reads_the_digits_of_its_radix() {
    assert_eq!(parsed("(parse-int \"ff\" :radix 16)"), "255");
    assert_eq!(parsed("(parse-int \"-FF\" :radix 16)"), "-255");
    assert_eq!(parsed("(parse-int \"101\" :radix 2)"), "5");
    assert_eq!(parsed("(parse-int \"zz\" :radix 36)"), "1295");
    assert_eq!(parsed("(parse-int \"12\" :radix 2)"), "err:parse-int: invalid integer literal: \"12\"");
}

/// `:junk-allowed` stops at the first non-digit instead of rejecting it, but
/// a string with no digits at all is still an `Err` — CL's `nil`.
#[test]
fn parse_int_with_junk_allowed_stops_at_the_first_non_digit() {
    assert_eq!(parsed("(parse-int \" 123abc\" :junk-allowed true)"), "123");
    assert_eq!(parsed("(parse-int \"-4 5\" :junk-allowed true)"), "-4");
    assert_eq!(parsed("(parse-int \"12\" :radix 2 :junk-allowed true)"), "1");
    assert_eq!(parsed("(parse-int \"abc\" :junk-allowed true)"), "err:parse-int: invalid integer literal: \"abc\"");
}

/// A radix outside 2..36 is the caller's mistake, not the text's: a panic,
/// not an `Err`.
#[test]
fn parse_int_panics_on_a_radix_outside_2_to_36() {
    let mut h = Heap::with_capacity(1 << 18);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut h, &mut chk, &mut interp);
    for bad in ["1", "37"] {
        let src = format!("(parse-int \"1\" :radix {})", bad);
        let v = Reader::new().read_all(&mut h, &src).expect("read failed").remove(0);
        let tl = chk.check_form(&mut h, &interp, v).expect("check failed");
        let err = interp.exec(&mut h, tl).expect_err("a bad radix must panic");
        assert!(format!("{:?}", err).contains(&format!("radix {} is not between 2 and 36", bad)), "{:?}", err);
    }
}
