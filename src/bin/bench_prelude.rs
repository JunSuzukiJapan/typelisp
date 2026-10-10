//! What the precompiled prelude is worth: the same workload, run against a
//! prelude with its native bodies installed and against a purely interpreted
//! one, in one process.
//!
//! One process on purpose. The two runs then share a build, a machine, a CPU
//! governor state and a heap size, so the ratio between them is about the
//! prelude and nothing else — which timing two `typl` invocations from a shell
//! is not. Startup is reported separately, and there the two numbers really
//! are two different things: installing the artifact is work the interpreted
//! load doesn't do.
//!
//! Reports; asserts nothing. A timing threshold in the test suite fails on a
//! busy machine and teaches people to ignore it.
//!
//!   scripts/bench-prelude.sh

use std::time::{Duration, Instant};

use typelisp::{Checker, Heap, Interp, Reader};

/// Workloads that actually reach compiled prelude bodies. Each is a `defun`
/// plus a call, so the loop lives in prelude code rather than in the
/// interpreter's own top-level dispatch.
const WORKLOADS: &[(&str, &str)] = &[
    (
        "i64 gcd",
        r#"
        (defun bench ((n i32)) i64
          (let ((acc (as i64 0)))
            (dotimes (i n)
              (setf acc (+ acc (gcd (+ (as i64 i) 4620) 1071))))
            acc))
        (bench 20000)
        "#,
    ),
    (
        "i64 abs/signum/rem",
        r#"
        (defun bench ((n i32)) i64
          (let ((acc (as i64 0)))
            (dotimes (i n)
              (setf acc (+ acc (abs (- (rem (as i64 i) 7) 3)) (signum (- (as i64 i) 100)))))
            acc))
        (bench 50000)
        "#,
    ),
    (
        "int abs/+",
        r#"
        (defun bench ((n int)) int
          (let ((acc 0))
            (dotimes (i n)
              (setf acc (+ acc (abs (- i 500)))))
            acc))
        (bench 20000)
        "#,
    ),
    (
        "f64 abs/signum",
        r#"
        (defun bench ((n i32)) f64
          (let ((acc 0.0))
            (dotimes (i n)
              (setf acc (+ acc (abs (- (as f64 (as i64 i)) 250.0)) (signum (- (as f64 (as i64 i)) 250.0)))))
            acc))
        (bench 50000)
        "#,
    ),
];

/// Loads a prelude (compiled or interpreted) plus the compiler island, timing
/// the load.
fn load(compiled: bool) -> (Heap, Checker, Interp, Duration) {
    let mut heap = Heap::with_capacity(1 << 18);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    let t0 = Instant::now();
    if compiled {
        typelisp::load_prelude(&mut heap, &mut chk, &mut interp);
    } else {
        typelisp::prelude::load_interpreted(&mut heap, &mut chk, &mut interp);
    }
    let elapsed = t0.elapsed();
    // Both sides get the island: it is loaded unconditionally by `typl`, and
    // leaving it out of one side would measure it instead of the prelude.
    typelisp::load_compiler_aot(&mut heap, &mut chk, &mut interp);
    (heap, chk, interp, elapsed)
}

fn run(heap: &mut Heap, chk: &mut Checker, interp: &Interp, src: &str) -> Duration {
    let r = Reader::new();
    let forms = r.read_all(heap, src).expect("read failed");
    let checked: Vec<_> =
        forms.into_iter().map(|v| chk.check_form(heap, interp, v).expect("check failed")).collect();
    let t0 = Instant::now();
    for tl in checked {
        interp.exec(heap, tl).expect("eval failed");
    }
    t0.elapsed()
}

/// Where the startup second actually goes.
///
/// `load_prelude`/`load_compiler_aot` each do read -> predeclare -> check ->
/// exec and then install a committed `.bc` over the result, and the whole of
/// that is paid at every `typl` startup. This replicates the two loaders step
/// by step against fresh environments so each step can be timed on its own;
/// the totals of the real loaders are reported alongside, since the manual
/// replication cannot reach the crate-private collect/promote steps between
/// exec and install.
fn startup_breakdown() {
    use typelisp::compile::bootstrap::island_source_hash;
    use typelisp::Value;

    println!("startup breakdown (one process, fresh 1<<18-cell heap per measurement):");

    for (label, source) in [("prelude", typelisp::prelude::SOURCE.as_str()), ("island", &*typelisp::compiler::SOURCE)] {
        let mut heap = Heap::with_capacity(1 << 18);
        let mut chk = Checker::new();
        let mut interp = Interp::new();
        // The island's SOURCE references prelude names, so it needs the
        // prelude in scope before it can be checked at all. Interpreted, and
        // not timed: this is the other row of this same table.
        if label == "island" {
            typelisp::prelude::load_interpreted(&mut heap, &mut chk, &mut interp);
            typelisp::compile::install_llvm_backend();
        }

        let r = Reader::new();
        let t0 = Instant::now();
        let forms = r.read_all(&mut heap, source).expect("read failed");
        let read = t0.elapsed();

        let t0 = Instant::now();
        let predeclare = t0.elapsed();

        let mut checked: Vec<Value> = Vec::with_capacity(forms.len());
        let mut check = Duration::ZERO;
        let mut exec = Duration::ZERO;
        for v in forms {
            let t0 = Instant::now();
            let tl = chk.check_form(&mut heap, &interp, v).expect("check failed");
            check += t0.elapsed();
            let _ = chk.take_warnings();
            heap.push_root(tl);
            checked.push(tl);
            let t0 = Instant::now();
            interp.exec(&mut heap, tl).expect("exec failed");
            exec += t0.elapsed();
        }

        // The island hashes its SOURCE a second time at every load, into a
        // second 1<<18-cell heap, to check the committed `.bc` for staleness
        // (`compiler::load_aot`). The prelude does not: it hashes the forms it
        // just read.
        let rehash = if label == "island" {
            let t0 = Instant::now();
            island_source_hash(source).expect("hash failed");
            t0.elapsed()
        } else {
            Duration::ZERO
        };

        let ms = |d: Duration| d.as_secs_f64() * 1000.0;
        println!(
            "  {:<8} read {:>7.1} ms | predeclare {:>6.1} ms | check {:>7.1} ms | exec {:>7.1} ms | re-hash {:>6.1} ms",
            label, ms(read), ms(predeclare), ms(check), ms(exec), ms(rehash)
        );
    }

    let mut heap = Heap::with_capacity(1 << 18);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    let t0 = Instant::now();
    typelisp::load_prelude(&mut heap, &mut chk, &mut interp);
    let prelude_total = t0.elapsed();
    let t0 = Instant::now();
    typelisp::load_compiler_aot(&mut heap, &mut chk, &mut interp);
    let island_total = t0.elapsed();
    println!(
        "  real loaders: load_prelude {:.1} ms, load_compiler_aot {:.1} ms (the excess over the rows \
         above is the bitcode install)",
        prelude_total.as_secs_f64() * 1000.0,
        island_total.as_secs_f64() * 1000.0
    );

    // What `(dump ...)` costs a session that never calls it: the baseline
    // signature has to be taken while the environment is still only the loaded
    // units, so it is paid at startup or not at all.
    let t0 = Instant::now();
    let sig = chk.signature(&heap).expect("signature");
    println!(
        "  baseline signature for (dump ...): {:.1} ms over {} entries",
        t0.elapsed().as_secs_f64() * 1000.0,
        sig.len()
    );
    println!();
}

fn main() {
    startup_breakdown();
    let (_h1, _c1, _i1, compiled_load) = load(true);
    let (_h2, _c2, _i2, interpreted_load) = load(false);
    println!("prelude load (read+check+exec, plus the bitcode install for compiled):");
    println!("  compiled     {:>8.1} ms", compiled_load.as_secs_f64() * 1000.0);
    println!("  interpreted  {:>8.1} ms", interpreted_load.as_secs_f64() * 1000.0);
    println!(
        "  cost of installing the artifact: {:+.1} ms",
        (compiled_load.as_secs_f64() - interpreted_load.as_secs_f64()) * 1000.0
    );
    println!();

    println!("{:<24} {:>12} {:>12} {:>10}", "workload", "compiled", "interpreted", "speedup");
    for (name, src) in WORKLOADS {
        // A fresh environment per workload per side: a `defun bench` redefined
        // across workloads would trip the redefinition policy, and reusing a
        // heap would let one workload's garbage bias the next.
        let (mut ch, mut cc, ci, _) = load(true);
        let compiled = run(&mut ch, &mut cc, &ci, src);
        let (mut ih, mut ic, ii, _) = load(false);
        let interpreted = run(&mut ih, &mut ic, &ii, src);
        println!(
            "{:<24} {:>9.1} ms {:>9.1} ms {:>9.2}x",
            name,
            compiled.as_secs_f64() * 1000.0,
            interpreted.as_secs_f64() * 1000.0,
            interpreted.as_secs_f64() / compiled.as_secs_f64()
        );
    }
}
