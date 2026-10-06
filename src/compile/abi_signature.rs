//! What compiled code assumes about the archive it links, written out as text.
//!
//! An AOT executable is this compiler's output linked against an archive built
//! from the runtime crates (`aot::link_archive`). The two meet at more than
//! the `rt_*` symbols: the numbers in the ABI and value-representation crates,
//! the well-known symbol vocabulary, the built-in types' variant order, the
//! dump format the `eval` environment is written in. [`describe`] lists every
//! one of those it can read off a table, and the ABI version
//! ([`typelisp_abi::ABI_VERSION`]) is "which description this is":
//! `docs/dev/api_version/` keeps each one, `scripts/regen-abi-version.sh`
//! writes a new one when this text changes, and `tests/abi_version_test.rs`
//! fails until it has.
//!
//! What a table cannot say is not here: what a shim does with its arguments,
//! the island's lowering choices, the layout of a heap object compiled code
//! reads in place. A change to one of those that the runtime has to agree
//! with changes nothing below, so it needs one of the listed things changed
//! with it, or `scripts/regen-abi-version.sh --bump`.

use std::collections::BTreeMap;
use std::fmt::Write;

use typelisp_front::check::registry::{AdtDef, AdtKind, FnSig, Namespace};
use typelisp_front::check::repr::Repr;
use typelisp_front::check::Registry;
use typelisp_front::types::Path;
use typelisp_front::Type;

/// `docs/dev/api_version/`, relative to the repository root.
pub const DOC_DIR: &str = "docs/dev/api_version";
/// The newest version's document, in [`DOC_DIR`].
pub const LATEST_FILE: &str = "latest_api_signature.md";

/// An ABI version: `MAJOR.MINOR.PATCH`, as [`typelisp_abi::ABI_VERSION`]
/// writes it. MAJOR.MINOR is the typelisp version's; PATCH counts the
/// descriptions written under that MAJOR.MINOR.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct AbiVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl AbiVersion {
    /// `None` for text other than three dot-separated numbers.
    pub fn parse(text: &str) -> Option<AbiVersion> {
        let mut parts = text.split('.').map(|part| part.parse::<u32>().ok());
        let version = AbiVersion { major: parts.next()??, minor: parts.next()??, patch: parts.next()?? };
        parts.next().is_none().then_some(version)
    }

    /// The version `typelisp-abi` names.
    pub fn current() -> AbiVersion {
        AbiVersion::parse(typelisp_abi::ABI_VERSION)
            .unwrap_or_else(|| panic!("typelisp-abi names `{}`, not MAJOR.MINOR.PATCH", typelisp_abi::ABI_VERSION))
    }

    /// Whether this version belongs to the typelisp version being built:
    /// whether the two have the same MAJOR.MINOR.
    pub fn is_of_this_package(self) -> bool {
        let (major, minor) = package_major_minor();
        self.major == major && self.minor == minor
    }

    /// The version a new description written after this one takes: the next
    /// PATCH under the same MAJOR.MINOR, or PATCH 0 under the typelisp
    /// version's MAJOR.MINOR once that has moved on.
    pub fn next(self) -> AbiVersion {
        let next = if self.is_of_this_package() {
            AbiVersion { patch: self.patch + 1, ..self }
        } else {
            let (major, minor) = package_major_minor();
            AbiVersion { major, minor, patch: 0 }
        };
        assert!(next > self, "the typelisp version {} is older than ABI version {}", env!("CARGO_PKG_VERSION"), self);
        next
    }
}

impl std::fmt::Display for AbiVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// MAJOR.MINOR of the typelisp version being built.
fn package_major_minor() -> (u32, u32) {
    let mut parts = env!("CARGO_PKG_VERSION").split('.').map(|part| part.parse::<u32>().ok());
    match (parts.next().flatten(), parts.next().flatten()) {
        (Some(major), Some(minor)) => (major, minor),
        _ => panic!("the typelisp version `{}` does not start with MAJOR.MINOR", env!("CARGO_PKG_VERSION")),
    }
}

/// Version `version`'s document, relative to [`DOC_DIR`].
pub fn history_file(version: AbiVersion) -> String {
    format!("history/api_{}.md", version)
}

/// The version a file in `history/` is the document of, from its name.
pub fn history_file_version(name: &str) -> Option<AbiVersion> {
    AbiVersion::parse(name.strip_prefix("api_")?.strip_suffix(".md")?)
}

/// The document for `version` whose description is `description`: a header
/// naming the version and the description's hash, then the description.
pub fn document(version: AbiVersion, description: &str) -> String {
    format!(
        "# ABI バージョン {}\n\n\
         `scripts/regen-abi-version.sh` が生成した文書。手で編集しない。\n\n\
         コンパイラが生成するコードが、ABI バージョン {} のランタイムライブラリについて仮定していることの一覧。\n\
         ここに書かれていない前提(シムが引数をどう扱うか、生成コードがヒープを直接読む箇所など)は含まない。\n\n\
         内容ハッシュ (FNV-1a 64): `{:016x}`\n\n{}",
        version,
        version,
        fnv1a(description),
        description
    )
}

/// What [`document`] was given: the version, the hash its header records,
/// and the description. `None` for text not shaped like one.
pub fn parse_document(text: &str) -> Option<(AbiVersion, u64, &str)> {
    let version = AbiVersion::parse(text.strip_prefix("# ABI バージョン ")?.split('\n').next()?)?;
    let hash_at = text.find("内容ハッシュ (FNV-1a 64): `")? + "内容ハッシュ (FNV-1a 64): `".len();
    let hash = u64::from_str_radix(text.get(hash_at..hash_at + 16)?, 16).ok()?;
    let body = &text[text.find("\n## ")? + 1..];
    Some((version, hash, body))
}

/// FNV-1a, 64 bits: fixed by its definition, unlike `std`'s hasher, so a
/// hash written into a document today is the hash computed for it later.
pub fn fnv1a(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| (h ^ b as u64).wrapping_mul(0x0000_0100_0000_01b3))
}

/// The description [`typelisp_abi::ABI_VERSION`] names — see the module doc
/// comment. Deterministic: every table is walked in sorted or declared order.
pub fn describe() -> String {
    let mut out = String::new();
    constants(&mut out);
    vocabulary(&mut out);
    let reg = Registry::with_builtins();
    let mut defs = Defs::default();
    collect(&reg.root, &mut Vec::new(), &mut defs);
    reprs(&mut out, &defs);
    shims(&mut out);
    types(&mut out, &defs);
    functions(&mut out, &defs);
    out
}

/// One `| name | value |` row per constant, under a heading naming where
/// they live. The names are written once: the macro spells each as both the
/// path it reads and the text it prints.
macro_rules! const_table {
    ($out:expr, $title:literal, $module:path, [$($name:ident),+ $(,)?]) => {{
        use $module as m;
        writeln!($out, "## {}\n\n| 名前 | 値 |\n|---|---|", $title).unwrap();
        $(writeln!($out, "| {} | {:?} |", stringify!($name), m::$name).unwrap();)+
        $out.push('\n');
    }};
}

fn constants(out: &mut String) {
    const_table!(out, "呼び出し規約 (typelisp_abi)", typelisp_abi, [
        STATUS_RETURN, STATUS_CALL, STATUS_SUSPEND, STATUS_UNWIND,
        BODY_ABI_CLASSIC, BODY_ABI_COROUTINE,
        FRAME_VALUE_SLOT, FRAME_HANDLER_SLOT, FRAME_RESERVED_SLOTS,
    ]);
    const_table!(out, "中断の種類 (typelisp_abi::call_state)", typelisp_abi::call_state, [
        SUSPEND_YIELD, SUSPEND_SLEEP, SUSPEND_WAIT, SUSPEND_SAFEPOINT,
        SUSPEND_CHAN_NEW, SUSPEND_CHAN_LEN, SUSPEND_CHAN_CAP, SUSPEND_CHAN_CLOSE,
        SUSPEND_CHAN_SEND, SUSPEND_CHAN_RECV, SUSPEND_CHAN_SELECT,
        SUSPEND_IO, SUSPEND_IO_FOR, SUSPEND_TASK, SUSPEND_THREAD, SUSPEND_MAIN,
    ]);
    const_table!(out, "生成コードの本体の ABI (typelisp::compile)", crate::compile, [EMITTED_BODY_ABI]);
    const_table!(out, "値の表現 (typelisp_mem::tagged)", typelisp_mem::tagged, [
        LAYOUT, LAYOUT_THREE_BIT, LAYOUT_FIXNUM_ONE_BIT, LAYOUT_OPTION_NICHE,
        FIXNUM_SHIFT, FIXNUM_MIN, FIXNUM_MAX, FIXNUM_MASK, FIXNUM_TAG,
        LOW_MASK, TAG_CONS, TAG_SYMBOL, TAG_BOXED, TAG_SMALL,
        SMALL_MASK, SMALL_SHIFT, TAG_IMMEDIATE, TAG_CHAR, TAG_STR, TAG_PATH,
        BOXED_SHIFT, IMMEDIATE_NIL, IMMEDIATE_FALSE, IMMEDIATE_TRUE, NIL_WORD,
    ]);
    const_table!(out, "C メモリ (typelisp_rt::c_mem)", typelisp_rt::c_mem, [
        KIND_I8, KIND_I16, KIND_I32, KIND_U8, KIND_U16, KIND_U32, KIND_C_LONG, KIND_C_ULONG,
        KIND_F32, KIND_F64, KIND_BOOL, KIND_PTR, DESC_FIELD, DESC_PAIR,
    ]);
    const_table!(out, "終了コード (typelisp_rt)", typelisp_rt, [EXIT_CODE_PANIC, EXIT_CODE_SUCCESS]);
    const_table!(out, "ダンプ形式 (typelisp_front::dump)", typelisp_front::dump, [FORMAT_VERSION, CONTAINER_VERSION]);
}

/// Tables of names the two sides agree on by position or by spelling.
fn vocabulary(out: &mut String) {
    out.push_str("## well-known シンボル (typelisp_mem::BUILTIN_SYMBOLS)\n\n| 番号 | 名前 |\n|---|---|\n");
    for (i, name) in typelisp_mem::BUILTIN_SYMBOLS.iter().enumerate() {
        writeln!(out, "| {} | `{}` |", i, name).unwrap();
    }
    out.push_str("\n## 組み込み型キー (typelisp_mem::BUILTIN_TYPE_KEYS)\n\n| 番号 | 型キー |\n|---|---|\n");
    for (i, key) in typelisp_mem::BUILTIN_TYPE_KEYS.iter().enumerate() {
        writeln!(out, "| {} | `{}` |", i, key).unwrap();
    }
    out.push_str("\n## 実行時が組み立てる値の型キー (typelisp_rt)\n\n| 表 | 関数 | 型キー |\n|---|---|---|\n");
    let tables: [(&str, &[(&str, &str)]); 5] = [
        ("sys_builtin::RESULT_KEYS", typelisp_rt::sys_builtin::RESULT_KEYS),
        ("stream_builtin::RESULT_KEYS", typelisp_rt::stream_builtin::RESULT_KEYS),
        ("stream_builtin::INNER_KEYS", typelisp_rt::stream_builtin::INNER_KEYS),
        ("net_builtin::RESULT_KEYS", typelisp_rt::net_builtin::RESULT_KEYS),
        ("net_builtin::INNER_KEYS", typelisp_rt::net_builtin::INNER_KEYS),
    ];
    for (table, rows) in tables {
        for (name, key) in rows {
            writeln!(out, "| {} | `{}` | `{}` |", table, name, key).unwrap();
        }
    }
    writeln!(out, "| readtable::READER_MACRO_OPTION_KEY | | `{}` |", typelisp_rt::readtable::READER_MACRO_OPTION_KEY).unwrap();
    out.push('\n');
}

/// The built-in definitions, gathered from every namespace of the registry
/// with their full paths, so each table below can be printed sorted.
#[derive(Default)]
struct Defs {
    types: BTreeMap<String, AdtDef>,
    fns: BTreeMap<String, (Path, FnSig)>,
}

fn collect(ns: &Namespace, at: &mut Vec<String>, defs: &mut Defs) {
    for def in ns.types.values().filter(|d| d.builtin) {
        defs.types.insert(def.name.to_string(), def.clone());
    }
    for (name, sig) in ns.fns.iter().filter(|(_, s)| s.builtin) {
        let mut segs = at.clone();
        segs.push(name.clone());
        let path = Path::from_segments(segs);
        defs.fns.insert(path.to_string(), (path, sig.clone()));
    }
    for (name, child) in &ns.modules {
        at.push(name.clone());
        collect(child, at, defs);
        at.pop();
    }
}

/// How each type a built-in signature mentions crosses into compiled code:
/// which `Repr` it gets, and the two numbers the island and the runtime both
/// read off it.
fn reprs(out: &mut String, defs: &Defs) {
    let kinds: BTreeMap<String, AdtKind> = defs.types.iter().map(|(k, d)| (k.clone(), d.kind)).collect();
    let kind_of = |p: &Path| kinds.get(&p.to_string()).copied();
    let mut seen: BTreeMap<String, Type> = BTreeMap::new();
    let mut note = |t: &Type| {
        seen.entry(t.to_string()).or_insert_with(|| t.clone());
    };
    for (_, sig) in defs.fns.values() {
        sig_types(sig, &mut note);
    }
    for def in defs.types.values() {
        for v in &def.variants {
            v.fields.iter().for_each(&mut note);
        }
        for f in def.assoc.values() {
            sig_types(&f.sig, &mut note);
        }
    }
    out.push_str("## 型ごとの表現 (Repr)\n\n| 型 | 型キー | Repr | field_kind | binding_kind |\n|---|---|---|---|---|\n");
    for (name, ty) in &seen {
        let repr = Repr::of_by(ty, &kind_of);
        writeln!(
            out,
            "| `{}` | `{}` | {} | {} | {} |",
            name,
            typelisp_front::type_key::type_key_of_type(ty),
            repr.tag(),
            repr.field_kind(),
            repr.binding_kind()
        )
        .unwrap();
    }
    out.push('\n');
}

fn sig_types(sig: &FnSig, note: &mut impl FnMut(&Type)) {
    sig.params.iter().for_each(&mut *note);
    note(&sig.ret);
    if let Some(rest) = &sig.rest {
        note(rest);
    }
    for p in sig.optionals.iter().chain(&sig.keys) {
        note(&p.decl_ty);
    }
}

/// Every symbol compiled code calls in the archive, with the LLVM type it
/// declares it at.
fn shims(out: &mut String) {
    const CLASSIC: &str = "i64 (ptr, i32)";
    let mut rows: BTreeMap<&str, &str> = super::externs::rt_extern_functions().iter().map(|(n, _)| (*n, CLASSIC)).collect();
    // Declared by the AOT entry sequence rather than the shim table
    // (`aot::build_main_wrapper`, `aot::coroutine_door`).
    for (name, ty) in [("rt_heap_init", CLASSIC), ("rt_run_entry", "i64 (i64)"), ("rt_drive_body", CLASSIC)] {
        rows.insert(name, ty);
    }
    out.push_str("## ランタイム関数\n\n| シンボル | LLVM の型 |\n|---|---|\n");
    for (name, ty) in rows {
        writeln!(out, "| `{}` | `{}` |", name, ty).unwrap();
    }
    out.push('\n');
}

fn types(out: &mut String, defs: &Defs) {
    out.push_str("## 組み込み型\n\n");
    for (name, def) in &defs.types {
        let kind = match def.kind {
            AdtKind::Sum => "sum",
            AdtKind::Struct => "struct",
        };
        writeln!(out, "### `{}` ({}, 型引数 [{}])\n", name, kind, def.params.join(", ")).unwrap();
        for (i, v) in def.variants.iter().enumerate() {
            let fields: Vec<String> = v.fields.iter().map(|t| format!("`{}`", t)).collect();
            writeln!(out, "- 変種 {} `{}`: {}", i, v.name, fields.join(", ")).unwrap();
        }
        if !def.field_names.is_empty() {
            writeln!(out, "- フィールド名: {}", def.field_names.iter().map(|f| format!("`{}`", f)).collect::<Vec<_>>().join(", ")).unwrap();
        }
        for (method, f) in def.assoc.iter().filter(|(_, f)| f.builtin) {
            let receiver = if f.instance { "インスタンス" } else { "関連関数" };
            writeln!(out, "- {} `{}`: {}", receiver, method, sig_text(&f.sig)).unwrap();
        }
        out.push('\n');
    }
}

fn functions(out: &mut String, defs: &Defs) {
    out.push_str("## 組み込み関数\n\n| 関数 | シム | シグネチャ |\n|---|---|---|\n");
    for (name, (path, sig)) in &defs.fns {
        let shim = super::externs::builtin_shim(path).map(|s| format!("`{}`", s)).unwrap_or_default();
        writeln!(out, "| `{}` | {} | {} |", name, shim, sig_text(sig).replace('|', "\\|")).unwrap();
    }
    out.push('\n');
}

/// `<T, U> (a, b, &optional c, &key d, &rest e) -> r`, with the `where`
/// bounds after it.
fn sig_text(sig: &FnSig) -> String {
    let mut params: Vec<String> = sig.params.iter().map(|t| t.to_string()).collect();
    if !sig.optionals.is_empty() {
        params.push("&optional".to_string());
        params.extend(sig.optionals.iter().map(|p| format!("{}: {}", p.name, p.decl_ty)));
    }
    if !sig.keys.is_empty() {
        params.push("&key".to_string());
        params.extend(sig.keys.iter().map(|p| format!("{}: {}", p.name, p.decl_ty)));
    }
    if let Some(rest) = &sig.rest {
        params.push(format!("&rest {}", rest));
    }
    let generics = if sig.type_params.is_empty() { String::new() } else { format!("<{}> ", sig.type_params.join(", ")) };
    let mut text = format!("`{}({}) -> {}`", generics, params.join(", "), sig.ret);
    for (param, bounds) in &sig.bounds {
        for b in bounds {
            write!(text, " where `{}: {}`", param, b.trait_path).unwrap();
            for (assoc, ty) in &b.assoc {
                write!(text, " (`{}` = `{}`)", assoc, ty).unwrap();
            }
        }
    }
    text
}
