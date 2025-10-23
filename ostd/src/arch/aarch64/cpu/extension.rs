// SPDX-License-Identifier: MPL-2.0

//! AArch64 ISA extensions.

use bitflags::bitflags;
use spin::Once;

use crate::arch::boot::DEVICE_TREE;

use aarch64_cpu::registers::*;
use aarch64_cpu::registers::Readable;

/// Detects available AArch64 ISA extensions.
pub(in crate::arch) fn init() {
    let mut global_isa_extensions = IsaExtensions::all();

    if ID_AA64ISAR0_EL1.is_set(ID_AA64ISAR0_EL1::AES) {
        global_isa_extensions |= IsaExtensions::AES;
    }

    if ID_AA64ISAR0_EL1.is_set(ID_AA64ISAR0_EL1::SHA1) {
        global_isa_extensions |= IsaExtensions::SHA1;
    }

    if ID_AA64ISAR0_EL1.is_set(ID_AA64ISAR0_EL1::SHA2) {
        global_isa_extensions |= IsaExtensions::SHA2;
    }

    if ID_AA64ISAR0_EL1.is_set(ID_AA64ISAR0_EL1::CRC32) {
        global_isa_extensions |= IsaExtensions::CRC32;
    }

    if ID_AA64PFR0_EL1.read(ID_AA64PFR0_EL1::SVE) > 0 {
        global_isa_extensions |= IsaExtensions::SVE;
    }

    if ID_AA64PFR0_EL1.read(ID_AA64PFR0_EL1::ASIMD) > 0 {
        global_isa_extensions |= IsaExtensions::ASIMD;
    }

    log::info!("Detected ISA extensions: {:?}", global_isa_extensions);

    GLOBAL_ISA_EXTENSIONS.call_once(|| global_isa_extensions);
}

/// Checks if the specified set of ISA extensions are available.
pub fn has_extensions(required: IsaExtensions) -> bool {
    GLOBAL_ISA_EXTENSIONS.get().unwrap().contains(required)
}

static GLOBAL_ISA_EXTENSIONS: Once<IsaExtensions> = Once::new();

/// A macro for RISC-V ISA extension definition and lookup table generation.
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

        const EXTENSION_TABLE: &[ExtensionData] = &[
            $(
                ExtensionData {
                    name: $str,
                    flag: IsaExtensions::$name
                },
            )*
        ];
    };
}

struct ExtensionData {
    name: &'static str,
    flag: IsaExtensions,
}

define_isa_extensions! {
    /// AA64ISAR0_EL1: AArch64 Instruction Set Attribute Register 0
    AES,       "aes",        "Advanced Encryption Standard";
    SHA1,       "sha1",       "Secure Hash Algorithm 1";
    SHA2,       "sha2",       "Secure Hash Algorithm 2";
    CRC32,      "crc32",      "Cyclic Redundancy Check 32";
    ATOMIC,   "atomic",    "Atomic memory operations";
    /// AA64PFR0_EL1: AArch64 Performance Feature Register 0
    SVE,        "sve",        "Scalable Vector Extension";
    ASIMD,    "asimd",     "Advanced SIMD";
}

