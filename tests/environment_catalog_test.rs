//! The environment layer added by
//! [cl-parity-plan.md](../docs/dev/cl-parity-plan.md) Phase 9c: the command
//! line, environment variables, the filesystem queries that need no open
//! stream, universal-time decoding, and the implementation-identity strings.
//!
//! The calendar arithmetic is where the risk is, so it gets most of the
//! tests: `decode-universal-time` and `encode-universal-time` are exact
//! integer formulas with no table to check them against, and a wrong constant
//! would be off by a day only in some years.
//!
//! Not tested here: `y-or-n-p`/`yes-or-no-p`, which read `*standard-input*`.
//! A test would have to feed the *process's* stdin, which under `cargo test`
//! is whatever the harness inherited — a terminal, if someone runs the suite
//! by hand, and then the test blocks forever waiting for a person to type.
//! They were checked through a pipe instead:
//!
//! ```text
//! $ printf 'maybe\nY\nno\n' | typl yn.typl
//! delete everything? (y/n) delete everything? (y/n) true
//! really? (yes/no) false
//! ```
//!
//! — which shows all three behaviours that matter: an unrecognized answer
//! re-asks, the match is case-insensitive, and `no` is `false`.

extern crate typelisp;
use typelisp::{load_prelude, Checker, Heap, Interp, Reader, Value};

/// Runs `src` with the prelude loaded and renders the last form with `~a`.
fn show(expr: &str) -> String {
    run(&format!("(format false \"~a\" {})", expr))
}

/// One component of a decoded time. A struct field is read as `var::field`,
/// so the value has to be bound before anything can be projected out of it.
fn part(time_expr: &str, field: &str) -> String {
    run(&format!("(let ((d {})) (format false \"~a\" d::{}))", time_expr, field))
}

/// One component of `(decode-universal-time <ut> [zone])`, where `ut_expr`
/// produces the `universal-time` to decode.
///
/// A universal time is a struct (`day`/`second`) rather than one integer,
/// because there is no 64-bit-wide integer type to hold the count. So it
/// cannot be printed and spliced back into a later program's source the way
/// these tests used to do with the integer — the value has to stay inside the
/// program that produced it, which is what this composes.
fn decoded(ut_expr: &str, field: &str) -> String {
    part(&format!("(decode-universal-time {})", ut_expr), field)
}

/// Runs a whole program (definitions and all), rendering the last form.
fn run(src: &str) -> String {
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
    match last {
        Value::Str(id) => h.string(id).to_string(),
        other => panic!("expected a string, got {:?}", other),
    }
}

// ---- Universal time ----------------------------------------------------

/// Universal time 0 is 1900-01-01 00:00:00 UTC, and that day was a Monday —
/// which is what makes `day-of-week` (CL's 0 = Monday) a plain remainder.
#[test]
fn universal_time_zero_is_the_start_of_1900() {
    let t = "(universal-time::new 0 0)";
    assert_eq!(decoded(t, "second"), "0");
    assert_eq!(decoded(t, "minute"), "0");
    assert_eq!(decoded(t, "hour"), "0");
    assert_eq!(decoded(t, "date"), "1");
    assert_eq!(decoded(t, "month"), "1");
    assert_eq!(decoded(t, "year"), "1900");
    assert_eq!(decoded(t, "day-of-week"), "0");
}

/// A date past several century boundaries, where a wrong leap-year rule shows
/// up: 2000 was a leap year (divisible by 400) and 1900 was not.
#[test]
fn decoding_gets_the_century_leap_year_rule_right() {
    // 2000-02-29 12:34:56 UTC. 2000 is a leap year, so this date exists.
    let t = "(encode-universal-time 56 34 12 29 2 2000)";
    assert_eq!(decoded(t, "year"), "2000");
    assert_eq!(decoded(t, "month"), "2");
    assert_eq!(decoded(t, "date"), "29");
    assert_eq!(decoded(t, "hour"), "12");
    assert_eq!(decoded(t, "minute"), "34");
    assert_eq!(decoded(t, "second"), "56");
    // 1900 was *not* a leap year, so 1900-03-01 is day 60 of that year, not
    // 61: one day after 1900-02-28. Differenced inside the language, since
    // the two instants are structs.
    assert_eq!(
        show(
            "(let ((a (encode-universal-time 0 0 0 28 2 1900))
                   (b (encode-universal-time 0 0 0 1 3 1900)))
               (+ (* (- b::day a::day) 86400) (- b::second a::second)))"
        ),
        "86400",
        "1900 must not have a February 29th"
    );
}

/// `encode` and `decode` are inverses across a spread of dates, which is the
/// property that catches an off-by-one in either direction.
#[test]
fn encode_and_decode_round_trip() {
    for (s, mi, h, d, mo, y) in [
        (0, 0, 0, 1, 1, 1900),
        (59, 59, 23, 31, 12, 1999),
        (30, 15, 6, 29, 2, 2024),
        (0, 0, 12, 4, 7, 1976),
        (1, 2, 3, 15, 8, 2100),
    ] {
        let t = format!("(encode-universal-time {} {} {} {} {} {})", s, mi, h, d, mo, y);
        assert_eq!(decoded(&t, "year"), y.to_string(), "year, ut={}", t);
        assert_eq!(decoded(&t, "month"), mo.to_string(), "month, ut={}", t);
        assert_eq!(decoded(&t, "date"), d.to_string(), "date, ut={}", t);
        assert_eq!(decoded(&t, "hour"), h.to_string(), "hour, ut={}", t);
        assert_eq!(decoded(&t, "minute"), mi.to_string(), "minute, ut={}", t);
        assert_eq!(decoded(&t, "second"), s.to_string(), "second, ut={}", t);
    }
}

/// `zone` is CL's: hours *west* of Greenwich, so a positive zone reads the
/// same instant as an earlier wall-clock time. Encoding with the same zone
/// undoes it.
#[test]
fn the_zone_argument_shifts_west_of_greenwich() {
    // 5 hours west: 1970-01-01 00:00 UTC decodes as 1969-12-31 19:00.
    let t = "(encode-universal-time 0 0 0 1 1 1970) 5";
    assert_eq!(decoded(t, "year"), "1969");
    assert_eq!(decoded(t, "month"), "12");
    assert_eq!(decoded(t, "date"), "31");
    assert_eq!(decoded(t, "hour"), "19");
    // And encoding that local time back with the same zone returns the
    // instant we started from — compared field by field, since a
    // `universal-time` is a struct.
    assert_eq!(
        show(
            "(let ((a (encode-universal-time 0 0 0 1 1 1970))
                   (b (encode-universal-time 0 0 19 31 12 1969 5)))
               (and (= a::day b::day) (= a::second b::second)))"
        ),
        "true"
    );
}

/// A negative universal time has no meaning — CL defines the scale as
/// non-negative — so it panics rather than decoding to something plausible.
#[test]
fn a_negative_universal_time_is_rejected() {
    let mut h = Heap::with_capacity(1 << 18);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut h, &mut chk, &mut interp);
    let r = Reader::new();
    let vs = r
        .read_all(&mut h, "(decode-universal-time (universal-time::new -1 0))")
        .expect("read failed");
    let tl = chk.check_form(&mut h, &interp, vs[0]).expect("check failed");
    match interp.exec(&mut h, tl) {
        Err(e) => assert!(
            format!("{:?}", e).contains("never negative"),
            "expected the universal-time panic, got {:?}",
            e
        ),
        Ok(v) => panic!("expected a panic, got {:?}", v),
    }
}

/// `get-decoded-time` is now, so only its self-consistency can be asserted:
/// it agrees with decoding `get-universal-time`, and it lands in a plausible
/// range.
#[test]
fn get_decoded_time_agrees_with_the_clock() {
    let year: i32 = part("(get-decoded-time)", "year").parse().expect("a year");
    assert!((2020..2200).contains(&year), "the clock says the year is {}", year);
    let month: i32 = part("(get-decoded-time)", "month").parse().expect("a month");
    assert!((1..=12).contains(&month), "month out of range: {}", month);
}

// ---- Environment and implementation identity ---------------------------

/// `getenv` answers `some` for a variable that is set and `none` for one that
/// is not. `PATH` is set in every environment a test runs in.
#[test]
fn getenv_distinguishes_set_from_unset() {
    assert_eq!(show("(is-some (getenv \"PATH\"))"), "true");
    assert_eq!(
        show("(is-none (getenv \"TYPELISP_A_VARIABLE_NOBODY_SETS\"))"),
        "true"
    );
}

/// The command line always has at least the program in it — element 0 names
/// the program in both the interpreted and the AOT world, which is the whole
/// contract `sys_builtin::COMMAND_LINE_ARGS` exists to keep.
#[test]
fn the_command_line_always_names_the_program() {
    assert_eq!(show("(> (len (command-line-args)) 0)"), "true");
}

/// The implementation-identity strings are constants, so the test is that
/// they are the *right* constants and non-empty.
#[test]
fn the_implementation_identifies_itself() {
    assert_eq!(show("(lisp-implementation-type)"), "typelisp");
    assert_eq!(show("(> (length (lisp-implementation-version)) 0)"), "true");
    assert_eq!(show("(> (length (machine-type)) 0)"), "true");
    assert_eq!(show("(> (length (software-type)) 0)"), "true");
}

// ---- Filesystem queries ------------------------------------------------

/// `ensure-directories-exist` creates a whole missing chain and succeeds
/// again on the second call; `directory-p` then agrees, and `directory`
/// lists what was put inside.
#[test]
fn directories_are_created_listed_and_recognized() {
    let base = std::env::temp_dir().join(format!("typelisp-9c-{}", std::process::id()));
    let nested = base.join("a").join("b");
    let _ = std::fs::remove_dir_all(&base);
    let nested_s = nested.to_string_lossy().replace('\\', "/");

    assert_eq!(
        show(&format!("(is-ok (ensure-directories-exist \"{}\"))", nested_s)),
        "true"
    );
    // Idempotent: "ensure" means the second call is a success, not an error.
    assert_eq!(
        show(&format!("(is-ok (ensure-directories-exist \"{}\"))", nested_s)),
        "true"
    );
    assert_eq!(show(&format!("(directory-p \"{}\")", nested_s)), "true");

    std::fs::write(nested.join("f.txt"), "hello").expect("write the probe file");
    let file_s = nested.join("f.txt").to_string_lossy().replace('\\', "/");
    // A plain file is not a directory, and is not confused with a missing one.
    assert_eq!(show(&format!("(directory-p \"{}\")", file_s)), "false");
    assert_eq!(show(&format!("(probe-file \"{}\")", file_s)), "true");

    // The listing holds exactly the one entry, as a full path.
    assert_eq!(
        run(&format!(
            "(let ((es (unwrap (directory \"{}\")))) (format false \"~a\" (len es)))",
            nested_s
        )),
        "1"
    );
    assert_eq!(
        run(&format!(
            "(let ((es (unwrap (directory \"{}\")))) \
               (let ((e (get es 0))) (format false \"~a\" (is-some (search e \"f.txt\")))))",
            nested_s
        )),
        "true"
    );

    // `truename` resolves to an absolute path that still names the file.
    assert_eq!(
        run(&format!(
            "(let ((t (unwrap (truename \"{}\")))) (format false \"~a\" (is-some (search t \"f.txt\"))))",
            file_s
        )),
        "true"
    );

    // `file-write-date` is a universal time, so it decodes to a year in the
    // range a file written just now can have.
    let year: i32 = part(
        &format!("(decode-universal-time (unwrap (file-write-date \"{}\")))", file_s),
        "year",
    )
    .parse()
    .expect("a year");
    assert!((2020..2200).contains(&year), "the file's mtime decodes to {}", year);

    let _ = std::fs::remove_dir_all(&base);
}

/// Every filesystem query fails with `FileError` rather than panicking, so a
/// caller can `match` one arm over all of them.
#[test]
fn a_missing_path_is_an_err_not_a_panic() {
    let missing = "/typelisp-no-such-path-9c/nothing";
    assert_eq!(show(&format!("(is-err (truename \"{}\"))", missing)), "true");
    assert_eq!(show(&format!("(is-err (directory \"{}\"))", missing)), "true");
    assert_eq!(show(&format!("(is-err (file-write-date \"{}\"))", missing)), "true");
    assert_eq!(show(&format!("(directory-p \"{}\")", missing)), "false");
}

/// `Pathish` in front of each means a `pathname` is as ordinary an argument
/// as a string — the property the wrappers exist for.
#[test]
fn the_queries_take_a_pathname_too() {
    let dir = std::env::temp_dir().join(format!("typelisp-9c-path-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create the probe directory");
    let dir_s = dir.to_string_lossy().replace('\\', "/");
    assert_eq!(show(&format!("(directory-p (to-pathname \"{}\"))", dir_s)), "true");
    assert_eq!(show(&format!("(is-ok (truename (to-pathname \"{}\")))", dir_s)), "true");
    let _ = std::fs::remove_dir_all(&dir);
}
