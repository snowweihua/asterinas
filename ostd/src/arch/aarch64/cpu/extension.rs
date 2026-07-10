// SPDX-License-Identifier: MPL-2.0

//! AArch64 ISA extensions.

use bitflags::bitflags;

pub struct SimpleOnce<T> {
    value: core::cell::UnsafeCell<Option<T>>,
    initialized: core::cell::UnsafeCell<u8>,
}

impl<T> SimpleOnce<T> {
    pub const fn new() -> Self {
        Self {
            value: core::cell::UnsafeCell::new(None),
            initialized: core::cell::UnsafeCell::new(0),
        }
    }

    pub fn get(&self) -> Option<&T> {
        if unsafe { *self.initialized.get() == 1 } {
            Some(unsafe { (*self.value.get()).as_ref().unwrap_unchecked() })
        } else {
            None
        }
    }

    pub fn call_once<F: FnOnce() -> T>(&self, f: F) -> &T {
        if unsafe { *self.initialized.get() == 0 } {
            unsafe {
                *self.value.get() = Some(f());
                *self.initialized.get() = 1;
            }
        }
        unsafe { (*self.value.get()).as_ref().unwrap_unchecked() }
    }
}

unsafe impl<T> Sync for SimpleOnce<T> where T: Send {}

/// Detects available AArch64 ISA extensions.
pub(in crate::arch) fn init() {
    let global_isa_extensions = IsaExtensions::empty();

    log::info!("Detected ISA extensions: {:?}", global_isa_extensions);

    GLOBAL_ISA_EXTENSIONS.call_once(|| global_isa_extensions);
}

/// Checks if the specified set of ISA extensions are available.
pub fn has_extensions(required: IsaExtensions) -> bool {
    GLOBAL_ISA_EXTENSIONS.get().unwrap().contains(required)
}

static GLOBAL_ISA_EXTENSIONS: SimpleOnce<IsaExtensions> = SimpleOnce::new();

macro_rules! define_isa_extensions {
    (
        $(
            $name:ident, $str:expr, $doc:expr;
        )*
    ) => {
        bitflags! {
            /// RISC-V ISA extensions.
            pub struct IsaExtensions: u128 {
                $(
                    #[doc = $doc]
                    const $name = 1u128 << ${index()};
                )*
            }
        }
    };
}

define_isa_extensions! {
    // AA64ISAR0_EL1: AArch64 Instruction Set Attribute Register 0
    AES,       "aes",        "Advanced Encryption Standard";
    SHA1,       "sha1",       "Secure Hash Algorithm 1";
    SHA2,       "sha2",       "Secure Hash Algorithm 2";
    CRC32,      "crc32",      "Cyclic Redundancy Check 32";
    ATOMIC,   "atomic",    "Atomic memory operations";
    SSTC,     "sstc",      "Supervisor-level timer compare";
    // AA64PFR0_EL1: AArch64 Performance Feature Register 0
    SVE,        "sve",        "Scalable Vector Extension";
    ASIMD,    "asimd",     "Advanced SIMD";
}
