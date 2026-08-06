// SPDX-License-Identifier: MPL-2.0

//! Useful synchronization primitives.

mod arc_single;
mod guard;
mod mutex;
mod once;
mod rcu;
mod rwarc;
mod rwlock;
mod rwmutex;
mod spin;
mod wait;

pub(crate) use self::rcu::finish_grace_period;
pub use self::{
    arc_single::{arc_clone, arc_new_cyclic, weak_clone},
    guard::{GuardTransfer, LocalIrqDisabled, PreemptDisabled, SpinGuardian, WriteIrqDisabled},
    mutex::{ArcMutexGuard, Mutex, MutexGuard},
    once::Once,
    rcu::{non_null, Rcu, RcuDrop, RcuOption, RcuOptionReadGuard, RcuReadGuard},
    rwarc::{RoArc, RwArc},
    rwlock::{
        ArcRwLockReadGuard, ArcRwLockUpgradeableGuard, ArcRwLockWriteGuard, RwLock,
        RwLockReadGuard, RwLockUpgradeableGuard, RwLockWriteGuard,
    },
    rwmutex::{
        ArcRwMutexReadGuard, ArcRwMutexUpgradeableGuard, ArcRwMutexWriteGuard, RwMutex,
        RwMutexReadGuard, RwMutexUpgradeableGuard, RwMutexWriteGuard,
    },
    spin::{ArcSpinLockGuard, SpinLock, SpinLockGuard},
    wait::{WaitQueue, Waiter, Waker},
};

pub(crate) fn init() {
    #[cfg(target_arch = "aarch64")]
    unsafe { crate::arch::boot::pl011_puts(b"[sync.init.0] start\n"); }
    // TEMPORARILY SKIPPED: rcu::init();
    #[cfg(target_arch = "aarch64")]
    unsafe { crate::arch::boot::pl011_puts(b"[sync.init.1] after rcu::init\n"); }
}
