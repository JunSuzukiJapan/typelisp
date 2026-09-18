//! `CompiledTask: Send` is the one fact the OS-threads scheduler design
//! (`docs/dev/os-threads-design.md`) depends on: a worker moving a task
//! between OS threads has to be able to move its `CompiledTask` there.
//! `Value: Send` (`typelisp-mem/src/value.rs`) is what makes this an `unsafe
//! impl`-free auto trait rather than a lie — see `sched::CompiledTask`'s doc
//! comment for the history (it used to fail on `*mut Cell`).

fn assert_send<T: Send>() {}

#[test]
fn compiled_task_is_send() {
    assert_send::<typelisp_rt::sched::CompiledTask>();
}
