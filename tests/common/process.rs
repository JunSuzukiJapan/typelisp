//! Running things outside the test's own process: the `typl` binary, and an
//! executable `compile-file` writes.

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

/// Exit code, stdout, stderr.
pub fn output(cmd: &mut Command) -> (i32, String, String) {
    let out = cmd.output().expect("running a command");
    (
        out.status.code().expect("did not exit normally"),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// Feed `input` to the `typl` REPL over stdin and return its stdout.
pub fn repl_stdout(input: &str) -> String {
    let mut child = Command::new(env!("CARGO_BIN_EXE_typl"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("failed to start the typl binary");
    child.stdin.take().unwrap().write_all(input.as_bytes()).expect("failed to write stdin");
    let out = child.wait_with_output().expect("failed to wait on typl");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// The scratch directory the AOT tests write sources and executables into.
pub fn tmp_dir() -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target").join("aot-test-tmp");
    std::fs::create_dir_all(&dir).expect("failed to create the AOT test scratch dir");
    dir
}

/// Writes and compiles `source` under `name`, returning the executable's path.
pub fn compile_only(name: &str, source: &str) -> PathBuf {
    let dir = tmp_dir();
    let src_path = dir.join(format!("{}.typl", name));
    let out_path = dir.join(name);
    std::fs::write(&src_path, source).expect("failed to write test source file");
    typelisp::compile::aot::compile_file(src_path.to_str().unwrap(), out_path.to_str().unwrap())
        .expect("compile_file failed");
    out_path
}
