//! `typl -c SOURCE [-o OUTPUT]` and `typl --lib-dir DIR`: the command-line
//! doors to `compile::aot::compile_file`, driven through the `typl` binary
//! itself since what is under test is the argument handling.
//!
//! Scratch files go under `target/aot-test-tmp/`, one name per test, for the
//! reason `tests/compile_file_test.rs`'s module doc comment gives.

use std::path::PathBuf;
use std::process::{Command, Output};

const TYPL: &str = env!("CARGO_BIN_EXE_typl");

const HELLO: &str = "(defun main () () (println \"hello\"))\n(main)\n";

fn tmp_dir(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target").join("aot-test-tmp").join(name);
    std::fs::create_dir_all(&dir).expect("failed to create the test scratch dir");
    dir
}

fn typl(args: &[&str]) -> Output {
    Command::new(TYPL).args(args).output().expect("failed to start the typl binary")
}

#[test]
fn dash_c_names_the_executable_after_the_source() {
    let dir = tmp_dir("typl-c-default-output");
    let src = dir.join("hello.typl");
    let exe = dir.join("hello");
    std::fs::write(&src, HELLO).unwrap();
    let _ = std::fs::remove_file(&exe);

    let out = typl(&["-c", src.to_str().unwrap()]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let run = Command::new(&exe).output().expect("failed to run the compiled executable");
    assert_eq!(String::from_utf8_lossy(&run.stdout), "hello\n");
}

#[test]
fn dash_o_names_the_executable() {
    let dir = tmp_dir("typl-c-dash-o");
    let src = dir.join("hello.typl");
    let exe = dir.join("renamed");
    std::fs::write(&src, HELLO).unwrap();
    let _ = std::fs::remove_file(&exe);

    let out = typl(&["-c", src.to_str().unwrap(), "-o", exe.to_str().unwrap()]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let run = Command::new(&exe).output().expect("failed to run the compiled executable");
    assert_eq!(String::from_utf8_lossy(&run.stdout), "hello\n");
}

/// A copy of the archive in a folder of its own links like the original.
#[test]
fn lib_dir_links_a_copied_archive() {
    let dir = tmp_dir("typl-lib-dir-copy");
    let lib = dir.join("lib");
    std::fs::create_dir_all(&lib).unwrap();
    let profile = if cfg!(debug_assertions) { "debug" } else { "release" };
    let original = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target").join(profile).join("libtypelisp_front.a");
    std::fs::copy(&original, lib.join("libtypelisp_front.a")).expect("the static library is not built");
    let src = dir.join("hello.typl");
    let exe = dir.join("hello");
    std::fs::write(&src, HELLO).unwrap();
    let _ = std::fs::remove_file(&exe);

    let out = typl(&["--lib-dir", lib.to_str().unwrap(), "-c", src.to_str().unwrap()]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let run = Command::new(&exe).output().expect("failed to run the compiled executable");
    assert_eq!(String::from_utf8_lossy(&run.stdout), "hello\n");
}

/// The named folder's archive is the one linked, not the build tree's: an
/// empty one there fails the link. Through `compile-file` in a script, so
/// the flag is seen to reach the builtin as well as `-c`.
#[test]
fn lib_dir_is_what_compile_file_links() {
    let dir = tmp_dir("typl-lib-dir-empty-archive");
    let lib = dir.join("lib");
    std::fs::create_dir_all(&lib).unwrap();
    std::fs::write(lib.join("libtypelisp_front.a"), b"").unwrap();
    let src = dir.join("hello.typl");
    let exe = dir.join("hello");
    std::fs::write(&src, HELLO).unwrap();
    let build = dir.join("build.typl");
    std::fs::write(&build, format!("(compile-file {:?} {:?})\n", src.to_str().unwrap(), exe.to_str().unwrap())).unwrap();

    let out = typl(&[&format!("--lib-dir={}", lib.to_str().unwrap()), build.to_str().unwrap()]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("linker failed"), "{}", String::from_utf8_lossy(&out.stderr));
}

#[test]
fn lib_dir_without_the_archive_is_refused() {
    let dir = tmp_dir("typl-lib-dir-missing");
    let out = typl(&["--lib-dir", dir.to_str().unwrap(), "-c", "unused.typl"]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("has no libtypelisp_front.a"));
}

#[test]
fn dash_c_refuses_what_it_cannot_use() {
    for args in [
        &["-c"][..],
        &["-c", "noext"][..],
        &["-c", "a.typl", "b.typl"][..],
        &["-c", "a.typl", "-o"][..],
        &["--heap-cells", "100", "-c", "a.typl"][..],
        &["--feature", "x", "-c", "a.typl"][..],
        &["--image", "x.typld", "-c", "a.typl"][..],
    ] {
        let out = typl(args);
        assert_eq!(out.status.code(), Some(1), "{:?}", args);
        assert!(!out.stderr.is_empty(), "{:?}", args);
    }
}

#[test]
fn dash_dash_compile_is_dash_c() {
    let dir = tmp_dir("typl-dash-dash-compile");
    let src = dir.join("hello.typl");
    let exe = dir.join("hello");
    std::fs::write(&src, HELLO).unwrap();
    let _ = std::fs::remove_file(&exe);

    let out = typl(&["--compile", src.to_str().unwrap()]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let run = Command::new(&exe).output().expect("failed to run the compiled executable");
    assert_eq!(String::from_utf8_lossy(&run.stdout), "hello\n");

    let out = typl(&["--image", "x.typld", "--compile", "a.typl"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).starts_with("--compile:"));
}
