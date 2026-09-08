// SPDX-License-Identifier: MPL-2.0

use alloc::{boxed::Box, sync::{Arc, Weak}};
use core::{
    alloc::Layout,
    intrinsics::abort,
    mem::{self, MaybeUninit},
    ptr::NonNull,
    sync::atomic::{AtomicUsize, Ordering},
};

const MAX_REFCOUNT: usize = isize::MAX as usize;

#[repr(C)]
struct ArcInnerRepr {
    strong: AtomicUsize,
    weak: AtomicUsize,
}

#[repr(C)]
struct ArcInnerUninit<T> {
    strong: AtomicUsize,
    weak: AtomicUsize,
    data: MaybeUninit<T>,
}

fn data_offset_for_align(align: usize) -> usize {
    let layout = Layout::new::<ArcInnerRepr>();
    layout.size() + (layout.size().wrapping_neg() & (align - 1))
}

unsafe fn arc_inner_from_data<T: ?Sized>(data: *const T) -> *mut ArcInnerRepr {
    let align = mem::align_of_val_raw(data);
    let offset = data_offset_for_align(align);
    (data as *mut u8).wrapping_sub(offset) as *mut ArcInnerRepr
}

fn is_rpi3_single_core() -> bool {
    #[cfg(target_arch = "aarch64")]
    {
        crate::arch::is_rpi3()
    }
    #[cfg(not(target_arch = "aarch64"))]
    {
        false
    }
}

fn inc_count(counter: &AtomicUsize) {
    // Must be a single atomic RMW: a load/store pair loses updates when a
    // concurrent clone (or preemption) interleaves, undercounting the
    // refcount and freeing the object while still referenced.
    // This matches std `Arc::clone`, which uses `fetch_add(Relaxed)`.
    let old = counter.fetch_add(1, Ordering::Relaxed);
    if old > MAX_REFCOUNT {
        abort();
    }
}

#[inline(never)]
#[unsafe(link_section = ".rpi3_arc_clone")]
pub fn arc_clone<T: ?Sized>(arc: &Arc<T>) -> Arc<T> {
    if !is_rpi3_single_core() {
        return Arc::clone(arc);
    }

    let raw = Arc::as_ptr(arc);
    unsafe {
        let inner = arc_inner_from_data(raw);
        inc_count(&(*inner).strong);
        Arc::from_raw(raw)
    }
}

pub fn weak_clone<T: ?Sized>(weak: &Weak<T>) -> Weak<T> {
    if !is_rpi3_single_core() {
        return Weak::clone(weak);
    }

    let raw = Weak::as_ptr(weak);
    if (raw as *const ()).addr() == usize::MAX {
        return Weak::clone(weak);
    }

    unsafe {
        let inner = arc_inner_from_data(raw);
        inc_count(&(*inner).weak);
        Weak::from_raw(raw)
    }
}

pub fn arc_new_cyclic<T, F>(data_fn: F) -> Arc<T>
where
    F: FnOnce(&Weak<T>) -> T,
{
    if !is_rpi3_single_core() {
        return Arc::new_cyclic(data_fn);
    }

    unsafe {
        let boxed = Box::new(ArcInnerUninit {
            strong: AtomicUsize::new(0),
            weak: AtomicUsize::new(1),
            data: MaybeUninit::<T>::uninit(),
        });
        let uninit_ptr = NonNull::new_unchecked(Box::into_raw(boxed));
        let base = uninit_ptr.as_ptr() as *mut u8;
        let data_ptr =
            base.add(data_offset_for_align(mem::align_of::<T>())) as *mut MaybeUninit<T>;

        let weak = Weak::from_raw(data_ptr as *const T);
        let data = data_fn(&weak);
        data_ptr.write(MaybeUninit::new(data));

        (*uninit_ptr.as_ptr()).strong.store(1, Ordering::Release);
        let _ = weak.into_raw();
        Arc::from_raw(data_ptr as *const T)
    }
}
