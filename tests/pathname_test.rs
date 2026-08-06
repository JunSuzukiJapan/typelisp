//! Tests for the pathname layer (CLHS 19) and the `Pathish` designator.
//!
//! Two claims are under test. First, that taking a file name apart and
//! putting it back together follows CL's component model (`a.tar.gz` is
//! `a.tar` of type `gz`; a trailing `/` means no name at all; `merge-pathnames`
//! fills in what is missing). Second — the reason the layer is worth having —
//! that a `pathname` is accepted wherever a `string` is, because every file
//! operation is generic over `Pathish` rather than taking one concrete type.

extern crate typelisp;
use typelisp::{load_prelude, Checker, Heap, Interp, Reader, RtValue};

/// The text of a `string` result.
///
/// A `string` is a heap `Value::Str` since the scalar unification, so reading
/// one needs the heap it lives in — and `run` above drops its heap on return.
/// Hence this parallel runner, which reads the text out first.
fn eval_string(src: &str) -> String {
    let mut h = Heap::with_capacity(1 << 16);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut h, &mut chk, &mut interp);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).expect("read failed");
    let mut last = RtValue::Unit;
    for v in vs {
        let tl = chk.check_form(&mut h, &interp, v).expect("check failed");
        if let Some(val) = interp.exec(&mut h, tl).expect("eval failed") {
            last = val;
        }
    }
    match last {
        RtValue::Sexpr(typelisp::Value::Str(id)) => h.string(id).to_string(),
        other => panic!("expected a string, got {:?}", other),
    }
}

/// A directory that removes itself — same shape as `stream_test.rs`'s, so a
/// failing test cannot leave files behind.
struct TmpDir(std::path::PathBuf);

impl TmpDir {
    fn new(tag: &str) -> TmpDir {
        let mut p = std::env::temp_dir();
        p.push(format!("typelisp-pathname-{}-{}", tag, std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).expect("create temp dir");
        TmpDir(p)
    }
    /// The directory itself, with the trailing `/` that makes it a pathname
    /// with no name.
    fn dir(&self) -> String {
        format!("{}/", self.0.to_string_lossy().replace('\\', "/"))
    }
}

impl Drop for TmpDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

// ---- parsing ------------------------------------------------------------

#[test]
fn an_absolute_name_splits_into_directory_name_and_type() {
    let v = eval_string(
        r#"(let ((p (parse-namestring "/usr/local/lib/thing.tar.gz")))
             (format false "~a|~a|~a|~a"
               (directory-namestring p) (pathname-name p) (pathname-type p) (pathname-absolute-p p)))"#,
    );
    // The last dot separates, so the name keeps the inner one.
    assert_eq!(v, "/usr/local/lib/|(some thing.tar)|(some gz)|true");
}

#[test]
fn a_relative_name_stays_relative() {
    let v = eval_string(
        r#"(let ((p (parse-namestring "a/b/c.txt")))
             (format false "~a|~a" (namestring p) (pathname-absolute-p p)))"#,
    );
    assert_eq!(v, "a/b/c.txt|false");
}

#[test]
fn a_trailing_slash_means_no_name() {
    let v = eval_string(
        r#"(format false "~a|~a|~a"
             (namestring (parse-namestring "a/b/"))
             (pathname-name "a/b/")
             (file-namestring "a/b/"))"#,
    );
    assert_eq!(v, "a/b/|none|");
}

#[test]
fn a_leading_dot_is_all_name_as_in_cl() {
    let v = eval_string(r#"(format false "~a|~a" (pathname-name ".gitignore") (pathname-type ".gitignore"))"#);
    assert_eq!(v, "(some .gitignore)|none");
}

#[test]
fn the_directory_components_come_out_outermost_first() {
    let v = eval_string(
        r#"(let ((out ""))
             (doiter (d (iter (pathname-directory "/usr/local/bin/x")))
               (setf out (append (append out d) ".")))
             out)"#,
    );
    assert_eq!(v, "usr.local.bin.");
}

#[test]
fn parsing_and_rendering_round_trip() {
    let v = eval_string(
        r#"(format false "~a|~a|~a"
             (namestring (parse-namestring "/a/b.c"))
             (namestring (parse-namestring "b.c"))
             (namestring (parse-namestring "/")))"#,
    );
    assert_eq!(v, "/a/b.c|b.c|/");
}

// ---- building and merging ----------------------------------------------

#[test]
fn make_pathname_takes_the_components_you_have() {
    let v = eval_string(r#"(namestring (make-pathname :name "notes" :type "md"))"#);
    assert_eq!(v, "notes.md");
}

#[test]
fn merge_pathnames_fills_in_what_is_missing() {
    let v = eval_string(
        r#"(format false "~a|~a|~a"
             (namestring (merge-pathnames "b.txt" "/var/log/a.md"))
             (namestring (merge-pathnames "sub/b" "/var/log/"))
             (namestring (merge-pathnames (make-pathname :name "c") "/var/log/a.md")))"#,
    );
    // The third takes `default`'s type as well as its directory.
    assert_eq!(v, "/var/log/b.txt|/var/log/sub/b|/var/log/c.md");
}

#[test]
fn an_absolute_pathname_keeps_its_own_directory_when_merged() {
    let v = eval_string(r#"(namestring (merge-pathnames "/etc/x.conf" "/var/log/a.md"))"#);
    assert_eq!(v, "/etc/x.conf");
}

#[test]
fn enough_namestring_strips_the_prefix_it_finds_and_nothing_else() {
    let v = eval_string(
        r#"(format false "~a|~a"
             (enough-namestring "/var/log/a.md" "/var/log/")
             (enough-namestring "/etc/x" "/var/log/"))"#,
    );
    assert_eq!(v, "a.md|/etc/x");
}

// ---- the designator -----------------------------------------------------

#[test]
fn a_string_is_its_own_namestring() {
    // The `Pathish` impl for `string` is the identity, so passing a string to
    // a file operation costs no parse.
    let v = eval_string(r#"(namestring "a/b.txt")"#);
    assert_eq!(v, "a/b.txt");
}

#[test]
fn a_pathname_can_be_used_wherever_a_string_can() {
    let d = TmpDir::new("designator");
    let dir = d.dir();
    let v = eval_string(&format!(
        r#"(let ((p (merge-pathnames (make-pathname :name "note" :type "txt") "{dir}")))
             (match (write-file-string p "hello")
               ((ok _)
                (match (read-file-string p)
                  ((ok s) (format false "~a|~a" s (probe-file p)))
                  ((err e) (message e))))
               ((err e) (message e))))"#
    ));
    assert_eq!(v, "hello|true");
}

#[test]
fn a_pathname_opens_a_stream_like_a_string_does() {
    let d = TmpDir::new("openstream");
    let dir = d.dir();
    let v = eval_string(&format!(
        r#"(let ((p (merge-pathnames "out.log" "{dir}")))
             (match (open-output p)
               ((ok f) (progn (write-line f "line") (close f)
                              (match (read-file-lines p)
                                ((ok ls) (get ls 0))
                                ((err e) (message e)))))
               ((err e) (message e))))"#
    ));
    assert_eq!(v, "line");
}

#[test]
fn with_open_file_accepts_a_pathname_too() {
    let d = TmpDir::new("withopen");
    let dir = d.dir();
    let v = eval_string(&format!(
        r#"(let ((p (merge-pathnames "w.txt" "{dir}")))
             (with-open-file (f p direction-output) (write-string f "written"))
             (match (read-file-string p) ((ok s) s) ((err e) (message e))))"#
    ));
    assert_eq!(v, "written");
}

#[test]
fn rename_can_mix_a_pathname_and_a_string() {
    let d = TmpDir::new("mixrename");
    let dir = d.dir();
    let v = eval_string(&format!(
        r#"(let ((from (merge-pathnames "a.txt" "{dir}")))
             (match (write-file-string from "x")
               ((ok _)
                (match (rename-file from "{dir}b.txt")
                  ((ok _) (format false "~a|~a" (probe-file from) (probe-file "{dir}b.txt")))
                  ((err e) (message e))))
               ((err e) (message e))))"#
    ));
    assert_eq!(v, "false|true");
}
