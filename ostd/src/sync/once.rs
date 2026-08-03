// SPDX-License-Identifier: MPL-2.0

use crate::boot::SimpleOnce;

pub struct Once<T> {
    simple: SimpleOnce<T>,
    concurrent: spin::Once<T>,
}

impl<T> Once<T> {
    pub const fn new() -> Self {
        Self {
            simple: SimpleOnce::new(),
            concurrent: spin::Once::new(),
        }
    }

    pub fn get(&self) -> Option<&T> {
        #[cfg(target_arch = "aarch64")]
        if crate::arch::is_rpi3() {
            return self.simple.get();
        }

        self.concurrent.get()
    }

    pub fn call_once<F: FnOnce() -> T>(&self, f: F) -> &T {
        #[cfg(target_arch = "aarch64")]
        if crate::arch::is_rpi3() {
            return self.simple.call_once(f);
        }

        self.concurrent.call_once(f)
    }

    pub fn is_completed(&self) -> bool {
        #[cfg(target_arch = "aarch64")]
        if crate::arch::is_rpi3() {
            return self.simple.is_completed();
        }

        self.concurrent.is_completed()
    }
}

unsafe impl<T: Send> Sync for Once<T> {}
