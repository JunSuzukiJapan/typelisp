//! LLVM ベースのコンパイラ基盤（feature = "compile"）。
//! 設計・段階的ロードマップは docs/TODO.md「ステップ5: compile」を参照。
//!
//! コンパイラ本体（`compile`）は **typelisp で書く**（[`compiler_source`]）。
//! Rust が提供するのは LLVM バインディング（`check::registry`に登録された
//! `LlvmModule`/`LlvmBuilder`等の組み込み型、実装は`eval::interp`）と、
//! 型付きASTをtypelisp側に橋渡しする [`ast_bridge`] のみ。

pub mod ast_bridge;
pub mod compiler_source;

use inkwell::context::Context;

/// inkwell/llvm-sys が正しくリンクされ、Context/Module の生成・verify が
/// 実際に動くことを確認する smoke test（tests/compile_test.rs から実行）。
pub fn llvm_smoke_test() -> String {
    let context = Context::create();
    let module = context.create_module("typelisp_smoke");
    module.verify().expect("empty module should verify");
    module.get_name().to_str().unwrap().to_string()
}
