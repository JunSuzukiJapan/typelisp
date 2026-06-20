#![cfg(feature = "compile")]

#[test]
fn llvm_toolchain_links_and_runs() {
    assert_eq!(typelisp::llvm_smoke_test(), "typelisp_smoke");
}
