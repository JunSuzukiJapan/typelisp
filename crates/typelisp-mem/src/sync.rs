//! Locks, one-time initialisation and per-thread state: the one door the
//! runtime's crates go through for each, so that the same code builds with
//! std and without it (a microcontroller, `no_std` + `alloc`).
//!
//! The locks are `lock_api`'s, whose API is parking_lot's (parking_lot's
//! types are `lock_api`'s over its own raw locks): `read_recursive`,
//! `RwLockReadGuard::map` and the rest are the same in both builds. With std
//! the raw locks are parking_lot's. Without it there is one core and no
//! thread to wait for, so a lock that is held when it is asked for has been
//! re-entered, which is a bug, and is said so rather than spun on forever.
//!
//! Per-thread state is declared with [`crate::thread_local!`]: std's own
//! with std, and one value for the whole program without it.

#[cfg(feature = "std")]
pub use parking_lot::{RawMutex, RawRwLock};

#[cfg(not(feature = "std"))]
pub use single::{RawMutex, RawRwLock};

pub type Mutex<T> = lock_api::Mutex<RawMutex, T>;
pub type MutexGuard<'a, T> = lock_api::MutexGuard<'a, RawMutex, T>;
pub type RwLock<T> = lock_api::RwLock<RawRwLock, T>;
pub type RwLockReadGuard<'a, T> = lock_api::RwLockReadGuard<'a, RawRwLock, T>;
pub type RwLockWriteGuard<'a, T> = lock_api::RwLockWriteGuard<'a, RawRwLock, T>;
pub type MappedRwLockReadGuard<'a, T> = lock_api::MappedRwLockReadGuard<'a, RawRwLock, T>;

/// A counter shared between threads: 64 bits where the target has 64-bit
/// atomics, 32 where it has not (Cortex-M).
#[cfg(target_has_atomic = "64")]
pub type AtomicCount = core::sync::atomic::AtomicU64;

#[cfg(not(target_has_atomic = "64"))]
pub type AtomicCount = core::sync::atomic::AtomicU32;

#[cfg(feature = "std")]
pub use parking_lot::Condvar;

#[cfg(feature = "std")]
pub use std::sync::OnceLock;

#[cfg(not(feature = "std"))]
pub use single::{Condvar, OnceLock};

/// Who the calling thread is: an id to compare, and a name for messages.
#[cfg(feature = "std")]
pub fn current_thread() -> (ThreadKey, String) {
    let current = std::thread::current();
    let name = current.name().map(str::to_string).unwrap_or_else(|| format!("{:?}", current.id()));
    (current.id(), name)
}

#[cfg(feature = "std")]
pub type ThreadKey = std::thread::ThreadId;

/// Without std there is one thread.
#[cfg(not(feature = "std"))]
pub fn current_thread() -> (ThreadKey, alloc::string::String) {
    (ThreadKey, alloc::string::String::from("main"))
}

#[cfg(not(feature = "std"))]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ThreadKey;

/// `std::thread_local!`, or without std one value for the whole program
/// ([`Local`]). Only `with` is offered, which is all the runtime uses.
#[cfg(feature = "std")]
#[macro_export]
macro_rules! thread_local {
    ($($t:tt)*) => { ::std::thread_local!($($t)*); };
}

#[cfg(not(feature = "std"))]
#[macro_export]
macro_rules! thread_local {
    () => {};
    ($(#[$attr:meta])* $vis:vis static $name:ident: $ty:ty = const { $init:expr }; $($rest:tt)*) => {
        $(#[$attr])* $vis static $name: $crate::sync::Local<$ty> = $crate::sync::Local::new_const($init);
        $crate::thread_local!($($rest)*);
    };
    ($(#[$attr:meta])* $vis:vis static $name:ident: $ty:ty = const $init:block; $($rest:tt)*) => {
        $(#[$attr])* $vis static $name: $crate::sync::Local<$ty> = $crate::sync::Local::new_const($init);
        $crate::thread_local!($($rest)*);
    };
    ($(#[$attr:meta])* $vis:vis static $name:ident: $ty:ty = $init:expr; $($rest:tt)*) => {
        $(#[$attr])* $vis static $name: $crate::sync::Local<$ty> = $crate::sync::Local::new_lazy(|| $init);
        $crate::thread_local!($($rest)*);
    };
    ($(#[$attr:meta])* $vis:vis static $name:ident: $ty:ty = $init:expr) => {
        $crate::thread_local!($(#[$attr])* $vis static $name: $ty = $init;);
    };
}

#[cfg(not(feature = "std"))]
pub use single::Local;

#[cfg(not(feature = "std"))]
mod single {
    use core::cell::UnsafeCell;
    use core::sync::atomic::{AtomicU8, AtomicUsize, Ordering};
    use lock_api::RawRwLock as _;

    /// A mutex for one core: taking it while it is held is a re-entry.
    pub struct RawMutex(AtomicU8);

    unsafe impl lock_api::RawMutex for RawMutex {
        #[allow(clippy::declare_interior_mutable_const)]
        const INIT: RawMutex = RawMutex(AtomicU8::new(0));
        type GuardMarker = lock_api::GuardSend;

        fn lock(&self) {
            if !self.try_lock() {
                panic!("internal error: a lock was taken again while held, and there is no other thread to release it");
            }
        }

        fn try_lock(&self) -> bool {
            self.0.compare_exchange(0, 1, Ordering::Acquire, Ordering::Relaxed).is_ok()
        }

        unsafe fn unlock(&self) {
            self.0.store(0, Ordering::Release);
        }
    }

    /// A reader-writer lock for one core: the count of readers, or
    /// [`WRITER`] while written. Taking it in a way the state does not allow
    /// is a re-entry.
    pub struct RawRwLock(AtomicUsize);

    const WRITER: usize = usize::MAX;

    unsafe impl lock_api::RawRwLock for RawRwLock {
        #[allow(clippy::declare_interior_mutable_const)]
        const INIT: RawRwLock = RawRwLock(AtomicUsize::new(0));
        type GuardMarker = lock_api::GuardSend;

        fn lock_shared(&self) {
            if !self.try_lock_shared() {
                panic!("internal error: a lock was read while written, and there is no other thread to release it");
            }
        }

        fn try_lock_shared(&self) -> bool {
            let n = self.0.load(Ordering::Relaxed);
            n != WRITER && self.0.compare_exchange(n, n + 1, Ordering::Acquire, Ordering::Relaxed).is_ok()
        }

        unsafe fn unlock_shared(&self) {
            self.0.fetch_sub(1, Ordering::Release);
        }

        fn lock_exclusive(&self) {
            if !self.try_lock_exclusive() {
                panic!("internal error: a lock was written while held, and there is no other thread to release it");
            }
        }

        fn try_lock_exclusive(&self) -> bool {
            self.0.compare_exchange(0, WRITER, Ordering::Acquire, Ordering::Relaxed).is_ok()
        }

        unsafe fn unlock_exclusive(&self) {
            self.0.store(0, Ordering::Release);
        }
    }

    unsafe impl lock_api::RawRwLockRecursive for RawRwLock {
        fn lock_shared_recursive(&self) {
            self.lock_shared();
        }

        fn try_lock_shared_recursive(&self) -> bool {
            self.try_lock_shared()
        }
    }

    /// With one thread nothing waits for a condition another thread sets.
    pub struct Condvar;

    impl Condvar {
        pub const fn new() -> Condvar {
            Condvar
        }

        pub fn wait<T>(&self, _guard: &mut lock_api::MutexGuard<'_, RawMutex, T>) {
            unreachable!("a condition waited for with one thread, which nothing else can set");
        }

        pub fn wait_for<T>(&self, _guard: &mut lock_api::MutexGuard<'_, RawMutex, T>, _timeout: core::time::Duration) -> WaitTimeoutResult {
            unreachable!("a condition waited for with one thread, which nothing else can set");
        }

        pub fn notify_all(&self) -> usize {
            0
        }
    }

    /// What [`Condvar::wait_for`] would answer; it never returns.
    pub struct WaitTimeoutResult;

    impl WaitTimeoutResult {
        pub fn timed_out(&self) -> bool {
            unreachable!("no wait returns with one thread")
        }
    }

    impl Default for Condvar {
        fn default() -> Condvar {
            Condvar
        }
    }

    const EMPTY: u8 = 0;
    const BUSY: u8 = 1;
    const READY: u8 = 2;

    /// `std::sync::OnceLock`, for one core: initialising it again from
    /// inside its own initialiser is a re-entry.
    pub struct OnceLock<T> {
        state: AtomicU8,
        value: UnsafeCell<Option<T>>,
    }

    // SAFETY: the value is written once, before `READY` is published, and
    // only read after; with one core there is nothing to race the write.
    unsafe impl<T> Sync for OnceLock<T> {}

    impl<T> OnceLock<T> {
        pub const fn new() -> OnceLock<T> {
            OnceLock { state: AtomicU8::new(EMPTY), value: UnsafeCell::new(None) }
        }

        pub fn get(&self) -> Option<&T> {
            if self.state.load(Ordering::Acquire) == READY {
                // SAFETY: `READY` says the value is written and never again.
                unsafe { (*self.value.get()).as_ref() }
            } else {
                None
            }
        }

        pub fn get_or_init(&self, init: impl FnOnce() -> T) -> &T {
            if let Some(v) = self.get() {
                return v;
            }
            if self.state.compare_exchange(EMPTY, BUSY, Ordering::Acquire, Ordering::Relaxed).is_err() {
                panic!("internal error: a value was asked for again while it was being made");
            }
            let v = init();
            // SAFETY: `BUSY` is held by this call alone.
            unsafe { *self.value.get() = Some(v) };
            self.state.store(READY, Ordering::Release);
            self.get().expect("the value was just set")
        }

        pub fn set(&self, v: T) -> Result<(), T> {
            if self.state.compare_exchange(EMPTY, BUSY, Ordering::Acquire, Ordering::Relaxed).is_err() {
                return Err(v);
            }
            // SAFETY: as in `get_or_init`.
            unsafe { *self.value.get() = Some(v) };
            self.state.store(READY, Ordering::Release);
            Ok(())
        }
    }

    impl<T> Default for OnceLock<T> {
        fn default() -> OnceLock<T> {
            OnceLock::new()
        }
    }

    /// One value of a [`crate::thread_local!`] for the whole program. With
    /// one core and no interrupt handler touching it, the one thread there
    /// is is the only one that reaches it.
    pub enum Local<T: 'static> {
        Const(T),
        Lazy(OnceLock<T>, fn() -> T),
    }

    // SAFETY: see the type's doc comment.
    unsafe impl<T> Sync for Local<T> {}

    impl<T> Local<T> {
        pub const fn new_const(value: T) -> Local<T> {
            Local::Const(value)
        }

        pub const fn new_lazy(init: fn() -> T) -> Local<T> {
            Local::Lazy(OnceLock::new(), init)
        }

        pub fn with<R>(&'static self, f: impl FnOnce(&T) -> R) -> R {
            match self {
                Local::Const(value) => f(value),
                Local::Lazy(value, init) => f(value.get_or_init(*init)),
            }
        }
    }
}
