//! The stream table's lock, held for as long as a read blocks, must not hold
//! up another thread's collection (`docs/dev/os-threads-design.md` §4).
//!
//! Three threads share one heap and one stream table. `holder` takes the
//! table's lock and blocks inside it (standing in for a read from stdin
//! nobody is typing into); `waiter` then waits for the same lock;
//! `collector` runs a collection. The collection has to finish while both
//! of the others are still stuck — so both the wait *inside* the lock and
//! the wait *for* it must count as native.
//!
//! Waiting for the lock outside a native section does not merely delay the
//! collection: when `holder` lets go of its blocking wait it parks for the
//! collection still holding the lock, and `waiter` never gets it. That form
//! of the bug shows up here as a hang, not as the assertion below.

use std::sync::mpsc;
use std::time::Duration;

use typelisp_mem::Heap;
use typelisp_rt::shared::{rt_shared, set_rt_shared};
use typelisp_rt::stream::with_streams;

#[test]
fn a_collection_runs_while_one_thread_holds_the_stream_lock_and_another_waits_for_it() {
    let mut main = Heap::with_capacity(64);
    let heap = main.shared_handle();
    let rt = rt_shared();

    let (locked_tx, locked_rx) = mpsc::channel::<()>();
    let (release_tx, release_rx) = mpsc::channel::<()>();
    let holder = {
        let (heap, rt) = (heap.clone(), rt.clone());
        std::thread::spawn(move || {
            set_rt_shared(rt);
            let mut h = Heap::attach(&heap);
            with_streams(&mut h, |_| {
                locked_tx.send(()).unwrap();
                release_rx.recv().unwrap();
            });
        })
    };
    main.native(|| locked_rx.recv().unwrap());

    let (waiting_tx, waiting_rx) = mpsc::channel::<()>();
    let waiter = {
        let (heap, rt) = (heap.clone(), rt.clone());
        std::thread::spawn(move || {
            set_rt_shared(rt);
            let mut h = Heap::attach(&heap);
            waiting_tx.send(()).unwrap();
            with_streams(&mut h, |_| ());
        })
    };
    main.native(|| waiting_rx.recv().unwrap());
    // Give `waiter` time to reach the lock. Not needed for the test to be
    // right — only for it to be testing something: if `waiter` has not got
    // there yet, the collection below passes whether or not the wait is
    // native.
    main.native(|| std::thread::sleep(Duration::from_millis(100)));

    let (done_tx, done_rx) = mpsc::channel::<()>();
    let collector = std::thread::spawn(move || {
        let mut h = Heap::attach(&heap);
        h.gc();
        done_tx.send(()).unwrap();
    });
    let finished = main.native(|| done_rx.recv_timeout(Duration::from_secs(10)));
    // Let go either way, so a failure reports instead of hanging.
    release_tx.send(()).unwrap();
    main.native(|| {
        holder.join().unwrap();
        waiter.join().unwrap();
        collector.join().unwrap();
    });
    assert!(finished.is_ok(), "the collection waited for a thread blocked on the stream table's lock");
}
