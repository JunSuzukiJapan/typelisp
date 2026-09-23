//! Several OS threads, one heap: each thread attaches its own view onto the
//! same `HeapShared` and allocates, and collections stop every thread
//! (`docs/dev/os-threads-design.md` §4).
//!
//! A thread that holds a view but waits — here, the main thread in
//! `join`, or a thread blocked on a channel — does so inside
//! `Heap::native`. Outside one, the other threads' collections would wait
//! for it forever; that is the rule these tests are also checking.

use std::sync::{mpsc, Arc, Barrier};

use typelisp_mem::{Heap, Value};

/// `(n-1 n-2 ... 0)` built one cons at a time, the list rooted while it
/// grows — every `cons` here is a point where some thread's collection may
/// run.
fn build(h: &mut Heap, n: i64) -> Value {
    let base = h.root_count();
    h.push_root(Value::Empty);
    for i in 0..n {
        let tail = h.root(base);
        let l = h.cons(Value::Int(i), tail).unwrap();
        h.set_root(base, l);
    }
    let l = h.root(base);
    h.truncate_roots(base);
    l
}

fn ints(h: &Heap, mut l: Value) -> Vec<i64> {
    let mut out = Vec::new();
    while let Value::Cons(_) = l {
        match h.car(l).unwrap() {
            Value::Int(i) => out.push(i),
            other => panic!("not an int: {other:?}"),
        }
        l = h.cdr(l).unwrap();
    }
    out
}

/// Each worker keeps one rooted list per round and throws the rest away, in
/// an arena far smaller than what all of them allocate together — so
/// collections keep happening, started from whichever thread ran out, while
/// the others are in the middle of building. At the end every kept list must
/// still read back exactly as built.
fn many_threads_allocating(threads: usize, rounds: usize, len: i64) {
    let mut main = Heap::with_capacity(256);
    let shared = main.shared_handle();
    let start = Arc::new(Barrier::new(threads));
    let handles: Vec<_> = (0..threads)
        .map(|t| {
            let shared = Arc::clone(&shared);
            let start = Arc::clone(&start);
            std::thread::Builder::new()
                .name(format!("alloc-{t}"))
                .spawn(move || {
                    let mut h = Heap::attach(&shared);
                    h.native(|| start.wait());
                    let kept_base = h.root_count();
                    for r in 0..rounds {
                        let kept = build(&mut h, len + r as i64);
                        h.push_root(kept);
                        // Garbage, unrooted: the next collection takes it.
                        let _ = build(&mut h, len);
                    }
                    for r in 0..rounds {
                        let l = h.root(kept_base + r);
                        let want: Vec<i64> = (0..len + r as i64).rev().collect();
                        assert_eq!(ints(&h, l), want, "thread {t}, round {r}");
                    }
                    h.truncate_roots(kept_base);
                })
                .unwrap()
        })
        .collect();
    main.native(|| {
        for h in handles {
            h.join().unwrap();
        }
    });
    assert!(main.gc_count() > 0, "the arena was meant to be too small to get by without a collection");
}

#[test]
fn two_threads_share_one_heap() {
    many_threads_allocating(2, 20, 40);
}

#[test]
fn eight_threads_share_one_heap() {
    many_threads_allocating(8, 12, 30);
}

/// The same under `gc_stress`: every `cons` on every thread is a full
/// stop-the-world collection, so a value any thread fails to root is freed
/// at the very next allocation anywhere.
#[test]
fn threads_share_one_heap_under_gc_stress() {
    let mut main = Heap::with_capacity(256);
    main.set_gc_stress(true);
    let shared = main.shared_handle();
    let handles: Vec<_> = (0..4)
        .map(|t| {
            let shared = Arc::clone(&shared);
            std::thread::spawn(move || {
                let mut h = Heap::attach(&shared);
                let base = h.root_count();
                for r in 0..3 {
                    let l = build(&mut h, 8 + r);
                    h.push_root(l);
                }
                for r in 0..3 {
                    let want: Vec<i64> = (0..8 + r).rev().collect();
                    assert_eq!(ints(&h, h.root(base + r as usize)), want, "thread {t}");
                }
                h.truncate_roots(base);
            })
        })
        .collect();
    main.native(|| {
        for h in handles {
            h.join().unwrap();
        }
    });
}

/// A thread that is native — here blocked on a channel that only the
/// collecting thread will ever send on — does not hold the collection up.
/// Without `native`, `gc()` would wait for the blocked thread and the
/// blocked thread for the `send` that comes after `gc()`.
#[test]
fn a_native_thread_does_not_block_another_threads_collection() {
    let mut main = Heap::with_capacity(64);
    let kept = build(&mut main, 5);
    main.push_root(kept);
    let shared = main.shared_handle();
    let (tx, rx) = mpsc::channel::<()>();
    let collector = std::thread::spawn(move || {
        let mut h = Heap::attach(&shared);
        let _ = build(&mut h, 30);
        h.gc();
        tx.send(()).unwrap();
    });
    main.native(|| rx.recv().unwrap());
    main.native(|| collector.join().unwrap());
    assert_eq!(ints(&main, kept), vec![4, 3, 2, 1, 0], "the native thread's roots were marked");
}

/// A thread that is running and never allocates still stops, at an explicit
/// `safepoint` — the one a compiled loop's back edge polls.
#[test]
fn a_thread_stops_at_an_explicit_safepoint() {
    let main = Heap::with_capacity(64);
    let shared = main.shared_handle();
    let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let spinner = {
        let (shared, stop) = (Arc::clone(&shared), Arc::clone(&stop));
        std::thread::spawn(move || {
            let mut h = Heap::attach(&shared);
            while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                h.safepoint();
            }
        })
    };
    let collector = std::thread::spawn(move || {
        let mut h = Heap::attach(&shared);
        for _ in 0..5 {
            h.gc();
        }
        stop.store(true, std::sync::atomic::Ordering::Relaxed);
    });
    let mut main = main;
    main.native(|| {
        collector.join().unwrap();
        spinner.join().unwrap();
    });
    assert_eq!(main.gc_count(), 5);
}

/// Two threads asking for a collection at the same moment: one collects, the
/// other waits it out and then runs its own — `gc()` returns only after a
/// collection that started after the call.
#[test]
fn simultaneous_collections_both_complete() {
    let mut main = Heap::with_capacity(64);
    let shared = main.shared_handle();
    let start = Arc::new(Barrier::new(4));
    let handles: Vec<_> = (0..4)
        .map(|_| {
            let (shared, start) = (Arc::clone(&shared), Arc::clone(&start));
            std::thread::spawn(move || {
                let mut h = Heap::attach(&shared);
                h.native(|| start.wait());
                for _ in 0..10 {
                    h.gc();
                }
            })
        })
        .collect();
    main.native(|| {
        for h in handles {
            h.join().unwrap();
        }
    });
    assert_eq!(main.gc_count(), 40);
}

/// Touching the heap inside a native section is the bug that would let a
/// collection run under a thread's feet; debug builds catch it at once.
#[test]
#[cfg(debug_assertions)]
#[should_panic(expected = "inside a native section")]
fn touching_the_heap_while_native_panics() {
    let mut h = Heap::with_capacity(8);
    h.enter_native();
    let _ = h.cons(Value::Int(1), Value::Empty);
}

/// A dropped view leaves the registry: a later collection does not wait for
/// a thread that is gone.
#[test]
fn a_dropped_view_is_not_waited_for() {
    let mut main = Heap::with_capacity(64);
    let shared = main.shared_handle();
    std::thread::spawn(move || {
        let mut h = Heap::attach(&shared);
        let _ = build(&mut h, 10);
    })
    .join()
    .unwrap();
    main.gc();
    assert_eq!(main.live_count(), 0);
}
