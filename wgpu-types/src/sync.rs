//! Provides [`Mutex`] and [`RwLock`] types with an appropriate implementation.

/// A [`Mutex`](lock_api::Mutex) using [`RawMutex`] for its backing implementation.
pub type Mutex<T> = lock_api::Mutex<RawMutex, T>;

/// A [`MutexGuard`](lock_api::MutexGuard) using [`RawMutex`] for its backing implementation.
pub type MutexGuard<'a, T> = lock_api::MutexGuard<'a, RawMutex, T>;

/// A [`MappedMutexGuard`](lock_api::MappedMutexGuard) using [`RawMutex`] for its backing implementation.
pub type MappedMutexGuard<'a, T> = lock_api::MappedMutexGuard<'a, RawMutex, T>;

/// A [`RwLock`](lock_api::RwLock) using [`RawRwLock`] for its backing implementation.
pub type RwLock<T> = lock_api::RwLock<RawRwLock, T>;

/// A [`RwLockReadGuard`](lock_api::RwLockReadGuard) using [`RawRwLock`] for its backing implementation.
pub type RwLockReadGuard<'a, T> = lock_api::RwLockReadGuard<'a, RawRwLock, T>;

/// A [`RwLockWriteGuard`](lock_api::RwLockWriteGuard) using [`RawRwLock`] for its backing implementation.
pub type RwLockWriteGuard<'a, T> = lock_api::RwLockWriteGuard<'a, RawRwLock, T>;

/// A [`RwLockUpgradableReadGuard`](lock_api::RwLockUpgradableReadGuard) using [`RawRwLock`] for its backing implementation.
pub type RwLockUpgradableReadGuard<'a, T> = lock_api::RwLockUpgradableReadGuard<'a, RawRwLock, T>;

// FIXME:
// * `Condvar` is only available through `parking_lot` and not through `lock_api`.
// * `Condvar` only works with the speific `RawMutex` implementation from `parking_lot`.
#[cfg(feature = "std")]
pub use parking_lot::{Condvar, Mutex as CondvarMutex};

// Note that both `spin` and `parking_lot` provide types which already implement
// the parts of the `lock_api` we're going to implement below.
// We explicitly wrap those implementations to ensure we have the intersection of
// their available APIs.
//
// For example, `spin` implements `RawMutex` with `GuardSend`, while `parking_lot`
// implements it with `GuardNoSend`.
// Further, `parking_lot` implements `RawRwLockUpgrade`, while `spin` does not.

cfg_if::cfg_if! {
    if #[cfg(feature = "std")] {
        type RawMutexInner = parking_lot::RawMutex;
        type RawRwLockInner = parking_lot::RawRwLock;
    } else if #[cfg(feature = "spin")] {
        type RawMutexInner = spin::Mutex<()>;
        type RawRwLockInner = spin::RwLock<()>;
    } else {
        #[repr(usize)]
        enum RawRwLockState {
            Exclusive = 0,
            Unlocked = 1,
        }

        cfg_if::cfg_if! {
            if #[cfg(all(target_has_atomic = "8", target_has_atomic = "ptr"))] {
                use core::sync::atomic::Ordering;
                type RawMutexInner = core::sync::atomic::AtomicBool;
                type RawRwLockInner = core::sync::atomic::AtomicUsize;
            } else {
                // RefCell is used instead of an atomic for broader platform compatibility
                // at the expense of performance and `Sync`.
                type RawMutexInner = core::cell::RefCell<bool>;
                type RawRwLockInner = core::cell::RefCell<usize>;
            }
        }
    }
}

/// Raw implementation for a [`Mutex`].
///
/// This will delegate to (in order):
///
/// * [`parking_lot`] if the `std` feature is enabled (default)
/// * [`spin`] if the `spin` feature is enabled
/// * [`atomic`] if the target supports atomic operations
/// * [`RefCell`] otherwise
///
/// [`parking_lot`]: https://docs.rs/parking_lot/
/// [`spin`]: https://docs.rs/spin/
/// [`atomic`]: core::sync::atomic
/// [`RefCell`]: core::cell::RefCell
pub struct RawMutex(RawMutexInner);

impl RawMutex {
    /// Constructs a new [`RawMutex`].
    pub const fn new() -> Self {
        Self({
            cfg_if::cfg_if! {
                if #[cfg(any(feature = "std", feature = "spin"))] {
                    lock_api::RawMutex::INIT
                } else {
                    RawMutexInner::new(false)
                }
            }
        })
    }
}

impl Default for RawMutex {
    fn default() -> Self {
        Self::new()
    }
}

impl core::fmt::Debug for RawMutex {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_tuple("RawMutex").finish()
    }
}

// SAFETY:
//
// # With `std` or `spin`
//
// This implementation directly delegates to an existing implementation of
// `RawMutex`, and is therefore safe.
//
// # Without `std` or `spin`
//
// From the `lock_api` documentation for `RawMutex`:
//
// > Implementations of this trait must ensure that the mutex is actually
// > exclusive: a lock can’t be acquired while the mutex is already locked.
//
// 1. A lock can only be acquired after `try_lock` returns `true` _or_ `lock` returns.
// 2. `try_lock` can only return `true` _if_ it is able to write a value of `true`
//    into its internal state _and_ the internal state is currently false.
// 3. `lock` can only return when `try_lock` returns `true`.
// 4. Therefore, a lock can only be acquired when the internal state is `false`
// 5. Internal state can only be `false` if the lock has been released by a call
//    to `unlock`, or it is in its initial state.
//
// Therefore, this implementation of `RawMutex` is safe.
unsafe impl lock_api::RawMutex for RawMutex {
    type GuardMarker = lock_api::GuardNoSend;

    const INIT: RawMutex = RawMutex::new();

    #[inline]
    fn lock(&self) {
        cfg_if::cfg_if! {
            if #[cfg(any(feature = "std", feature = "spin"))] {
                lock_api::RawMutex::lock(&self.0)
            } else {
                while !self.try_lock() {
                    core::hint::spin_loop()
                }
            }
        }
    }

    #[inline]
    fn try_lock(&self) -> bool {
        cfg_if::cfg_if! {
            if #[cfg(any(feature = "std", feature = "spin"))] {
                lock_api::RawMutex::try_lock(&self.0)
            } else if #[cfg(all(target_has_atomic = "8", target_has_atomic = "ptr"))] {
                self.0
                    .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
                    .is_ok()
            } else {
                match self.0.try_borrow_mut() {
                    Ok(mut guard) => {
                        if *guard {
                            false
                        } else {
                            *guard = true;
                            true
                        }
                    }
                    Err(_) => false,
                }
            }
        }
    }

    #[inline]
    unsafe fn unlock(&self) {
        cfg_if::cfg_if! {
            if #[cfg(any(feature = "std", feature = "spin"))] {
                // SAFETY: directly delegating to an accepted implementation
                unsafe { lock_api::RawMutex::unlock(&self.0) }
            } else if #[cfg(all(target_has_atomic = "8", target_has_atomic = "ptr"))] {
                self.0.store(false, Ordering::Release);
            } else {
                loop {
                    match self.0.try_borrow_mut() {
                        Ok(mut guard) => {
                            *guard = false;
                            break;
                        }
                        Err(_) => core::hint::spin_loop(),
                    }
                }
            }
        }
    }

    #[inline]
    fn is_locked(&self) -> bool {
        cfg_if::cfg_if! {
            if #[cfg(any(feature = "std", feature = "spin"))] {
                lock_api::RawMutex::is_locked(&self.0)
            } else if #[cfg(all(target_has_atomic = "8", target_has_atomic = "ptr"))] {
                self.0.load(Ordering::Acquire)
            } else {
                loop {
                    match self.0.try_borrow_mut() {
                        Ok(guard) => break *guard,
                        Err(_) => core::hint::spin_loop(),
                    }
                }
            }
        }
    }
}

// SAFETY:
//
// # With `std`
//
// This implementation directly delegates to an existing implementation of
// `RawMutexTimed`, and is therefore safe.
//
// # Without `std`
//
// There is no specific safety conditions for this trait.
// However, the implementation without `std` only uses safe APIs and pessimistically
// fails since, without `std`, `wgpu-types` currently has no mechanism for timekeeping.
unsafe impl lock_api::RawMutexTimed for RawMutex {
    cfg_if::cfg_if! {
        if #[cfg(feature = "std")] {
            type Duration = <RawMutexInner as lock_api::RawMutexTimed>::Duration;
            type Instant = <RawMutexInner as lock_api::RawMutexTimed>::Instant;
        } else {
            type Duration = core::time::Duration;
            type Instant = ();
        }
    }

    fn try_lock_for(&self, _timeout: Self::Duration) -> bool {
        cfg_if::cfg_if! {
            if #[cfg(feature = "std")] {
                lock_api::RawMutexTimed::try_lock_for(&self.0, _timeout)
            } else {
                lock_api::RawMutex::try_lock(self)
            }
        }
    }

    fn try_lock_until(&self, _timeout: Self::Instant) -> bool {
        cfg_if::cfg_if! {
            if #[cfg(feature = "std")] {
                lock_api::RawMutexTimed::try_lock_until(&self.0, _timeout)
            } else {
                lock_api::RawMutex::try_lock(self)
            }
        }
    }
}

/// Raw implementation for a [`RwLock`].
///
/// This will delegate to (in order):
///
/// * [`parking_lot`] if the `std` feature is enabled (default)
/// * [`spin`] if the `spin` feature is enabled
/// * [`atomic`] if the target supports atomic operations
/// * [`RefCell`] otherwise
///
/// [`parking_lot`]: https://docs.rs/parking_lot/
/// [`spin`]: https://docs.rs/spin/
/// [`atomic`]: core::sync::atomic
/// [`RefCell`]: core::cell::RefCell
pub struct RawRwLock(RawRwLockInner);

impl RawRwLock {
    /// Constructs a new [`RawRwLock`].
    pub const fn new() -> Self {
        Self({
            cfg_if::cfg_if! {
                if #[cfg(any(feature = "std", feature = "spin"))] {
                    lock_api::RawRwLock::INIT
                } else {
                    RawRwLockInner::new(RawRwLockState::Unlocked as _)
                }
            }
        })
    }
}

impl Default for RawRwLock {
    fn default() -> Self {
        Self::new()
    }
}

impl core::fmt::Debug for RawRwLock {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_tuple("RawRwLock").finish()
    }
}

// SAFETY:
// With `std` or `spin` enabled, this implementation directly delegates to an
// existing implementation of `RawRwLock`, and is therefore safe.
//
// From the `lock_api` documentation for RawRwLock:
//
// > Implementations of this trait must ensure that the RwLock is actually exclusive:
// > an exclusive lock can’t be acquired while an exclusive or shared lock exists,
// > and a shared lock can’t be acquire while an exclusive lock exists.
//
//  1. An exclusive lock can only be acquired when `try_lock_exclusive` returns `true`
//     _or_ `lock_exclusive` returns.
//  2. `try_lock_exclusive` can only return `true` when `count` is `0`.
//  3. `lock_exclusive` can only return when `try_lock_exclusive` returns `true`.
//  4. Therefore, an exclusive lock can only be acquired when the `count` is `0`.
//  5. When `try_lock_exclusive` returns `true`, it sets `count` to `1` and
//     `is_exclusive` to `true`.
//  6. A shared lock can only be acquired when `try_lock_shared` returns `true,
//     _or_ `lock_shared` returns, _or_ `downgrade` returns.
//  7. `try_lock_shared` can only return true when `is_exclusive` is `false` _or_
//     `count` is `0`.
//  8. `lock_shared` can only return when `try_lock_shared` returns `true`.
//  9. `downgrade` can only be called when an exclusive lock is already acquired,
//     _and_ the exclusive lock is being exchanged for a shared lock.
// 10. Therefore, a shared lock can only be acquired when `is_exclusive` is `false`
//     _or_ `count` is `0`.
// 11. `is_exclusive` is set to `true` whenever an exclusive lock is acquired.
// 12. `is_exclusive` is set to `false` whenever a shared lock is acquired.
// 13. `count` is incremented whenever any lock is acquired.
// 14. `count` is decremented whenever any lock is released.
// 15. Therefore, an exclusive lock can only be acquired when all other locks are released.
// 16. Therefore, a shared lock can only be acquired when any exclusive lock is released.
//
// Therefore, this implementation of `RawRwLock` is safe.
unsafe impl lock_api::RawRwLock for RawRwLock {
    type GuardMarker = lock_api::GuardNoSend;

    const INIT: RawRwLock = RawRwLock::new();

    #[inline]
    fn lock_exclusive(&self) {
        cfg_if::cfg_if! {
            if #[cfg(any(feature = "std", feature = "spin"))] {
                lock_api::RawRwLock::lock_exclusive(&self.0)
            } else {
                while !self.try_lock_exclusive() {
                    core::hint::spin_loop()
                }
            }
        }
    }

    #[inline]
    fn try_lock_exclusive(&self) -> bool {
        cfg_if::cfg_if! {
            if #[cfg(any(feature = "std", feature = "spin"))] {
                lock_api::RawRwLock::try_lock_exclusive(&self.0)
            } else if #[cfg(all(target_has_atomic = "8", target_has_atomic = "ptr"))] {
                self.0
                    .compare_exchange(RawRwLockState::Unlocked as _, RawRwLockState::Exclusive as _, Ordering::Acquire, Ordering::Relaxed)
                    .is_ok()
            } else {
                match self.0.try_borrow_mut() {
                    Ok(mut state) => {
                        if state.count == 0 {
                            state.count += 1;
                            state.is_exclusive = true;
                            true
                        } else {
                            false
                        }
                    }
                    Err(_) => false,
                }
            }
        }
    }

    #[inline]
    unsafe fn unlock_exclusive(&self) {
        cfg_if::cfg_if! {
            if #[cfg(any(feature = "std", feature = "spin"))] {
                // SAFETY: directly delegating to an accepted implementation
                unsafe { lock_api::RawRwLock::unlock_exclusive(&self.0) }
            } else if #[cfg(all(target_has_atomic = "8", target_has_atomic = "ptr"))] {
                self.0.store(RawRwLockState::Unlocked as _, Ordering::Release);
            } else {
                loop {
                    match self.0.try_borrow_mut() {
                        Ok(mut state) => {
                            state.count -= 1;
                            break;
                        }
                        Err(_) => core::hint::spin_loop(),
                    }
                }
            }
        }
    }

    #[inline]
    fn lock_shared(&self) {
        cfg_if::cfg_if! {
            if #[cfg(any(feature = "std", feature = "spin"))] {
                lock_api::RawRwLock::lock_shared(&self.0)
            } else {
                while !self.try_lock_shared() {
                    core::hint::spin_loop()
                }
            }
        }
    }

    #[inline]
    fn try_lock_shared(&self) -> bool {
        cfg_if::cfg_if! {
            if #[cfg(any(feature = "std", feature = "spin"))] {
                lock_api::RawRwLock::try_lock_shared(&self.0)
            } else if #[cfg(all(target_has_atomic = "8", target_has_atomic = "ptr"))] {
                self.0
                    .fetch_update(Ordering::Acquire, Ordering::Relaxed, |state| {
                        if state == RawRwLockState::Exclusive as _ {
                            None
                        } else {
                            Some(state + 1)
                        }
                    })
                    .is_ok()
            } else {
                match self.0.try_borrow_mut() {
                    Ok(mut state) => {
                        if state.count == 0 || !state.is_exclusive {
                            state.count += 1;
                            state.is_exclusive = false;
                            true
                        } else {
                            false
                        }
                    }
                    Err(_) => false,
                }
            }
        }
    }

    #[inline]
    unsafe fn unlock_shared(&self) {
        cfg_if::cfg_if! {
            if #[cfg(any(feature = "std", feature = "spin"))] {
                // SAFETY: directly delegating to an accepted implementation
                unsafe { lock_api::RawRwLock::unlock_shared(&self.0) }
            } else if #[cfg(all(target_has_atomic = "8", target_has_atomic = "ptr"))] {
                self.0.fetch_sub(1, Ordering::Release);
            } else {
                loop {
                    match self.0.try_borrow_mut() {
                        Ok(mut state) => {
                            state.count -= 1;
                            break;
                        }
                        Err(_) => core::hint::spin_loop(),
                    }
                }
            }
        }
    }

    #[inline]
    fn is_locked(&self) -> bool {
        cfg_if::cfg_if! {
            if #[cfg(any(feature = "std", feature = "spin"))] {
                lock_api::RawRwLock::is_locked(&self.0)
            } else if #[cfg(all(target_has_atomic = "8", target_has_atomic = "ptr"))] {
                self.0.load(Ordering::Acquire) != RawRwLockState::Unlocked as _
            } else {
                loop {
                    match self.0.try_borrow_mut() {
                        Ok(state) => break state.count > 0,
                        Err(_) => core::hint::spin_loop(),
                    }
                }
            }
        }
    }

    #[inline]
    fn is_locked_exclusive(&self) -> bool {
        cfg_if::cfg_if! {
            if #[cfg(any(feature = "std", feature = "spin"))] {
                lock_api::RawRwLock::is_locked_exclusive(&self.0)
            } else if #[cfg(all(target_has_atomic = "8", target_has_atomic = "ptr"))] {
                self.0.load(Ordering::Acquire) == RawRwLockState::Exclusive as _
            } else {
                loop {
                    match self.0.try_borrow_mut() {
                        Ok(state) => break state.count > 0 && state.is_exclusive,
                        Err(_) => core::hint::spin_loop(),
                    }
                }
            }
        }
    }
}

// SAFETY:
//
// # With `std` or `spin`
//
// This implementation directly delegates to an existing implementation of
// `RawRwLockDowngrade`, and is therefore safe.
//
// # Without `std` or `spin`
//
// There is no specific safety conditions for this trait.
// However, from the `lock_api` documentation for `RawRwLockDowngrade`:
//
// > Additional methods for RwLocks which support atomically downgrading an
// > exclusive lock to a shared lock.
//
// `RawRwLock` supports an atomic downgrade by directly flipping `is_exclusive`
// to `false`.
// This is exactly equivalent to releasing an exclusive lock (decrements `count`),
// and acquiring a shared lock (increments `count` and sets `is_exclusive` to `false`).
//
// Therefore, this implementation of `RawRwLockDowngrade` is safe.
unsafe impl lock_api::RawRwLockDowngrade for RawRwLock {
    unsafe fn downgrade(&self) {
        cfg_if::cfg_if! {
            if #[cfg(any(feature = "std", feature = "spin"))] {
                // SAFETY: directly delegating to an accepted implementation
                unsafe { lock_api::RawRwLockDowngrade::downgrade(&self.0) }
            } else if #[cfg(all(target_has_atomic = "8", target_has_atomic = "ptr"))] {
                self.0.store(1, Ordering::Release);
            } else {
                loop {
                    match self.0.try_borrow_mut() {
                        Ok(mut state) => {
                            state.is_exclusive = false;
                            break;
                        }
                        Err(_) => core::hint::spin_loop(),
                    }
                }
            }
        }
    }
}
