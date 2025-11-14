// SPDX-License-Identifier: MPL-2.0

//! AArch64 ISA extensions.

use bitflags::bitflags;
use spin::Once;



use aarch64_cpu::registers::*;
use aarch64_cpu::registers::Readable;

// Import the 'Field' type from the tock-registers crate directly
use tock_registers::fields::Field; 

// Define the missing constants locally.
// These match the bit ranges from the ARM Architecture Reference Manual.
const AES_F: Field = Field::new(4, 4);   // Bits [7:4]
const SHA1_F: Field = Field::new(8, 4);  // Bits [11:8]
const SHA2_F: Field = Field::new(12, 4); // Bits [15:12]
const CRC32_F: Field = Field::new(20, 4); // Bits [23:20]
const SVE_F: Field = Field::new(24, 4);  // Bits [27:24]
const ASIMD_F: Field = Field::new(28, 4); // Bits [31:28]

/// Detects available AArch64 ISA extensions.
pub(in crate::arch) fn init() {
    let mut global_isa_extensions = IsaExtensions::all();

    if ID_AA64ISAR0_EL1.read(AES_F) {
        global_isa_extensions |= IsaExtensions::AES;
    }

    if ID_AA64ISAR0_EL1.read(SHA1_F) {
        global_isa_extensions |= IsaExtensions::SHA1;
    }

    if ID_AA64ISAR0_EL1.read(SHA2_F) {
        global_isa_extensions |= IsaExtensions::SHA2;
    }

    if ID_AA64ISAR0_EL1.read(CRC32_F) {
        global_isa_extensions |= IsaExtensions::CRC32;
    }

    if ID_AA64PFR0_EL1.read(SVE_F) > 0 {
        global_isa_extensions |= IsaExtensions::SVE;
    }

    if ID_AA64PFR0_EL1.read(ASIMD_F) > 0 {
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
    // AA64ISAR0_EL1: AArch64 Instruction Set Attribute Register 0
    AES,       "aes",        "Advanced Encryption Standard";
    SHA1,       "sha1",       "Secure Hash Algorithm 1";
    SHA2,       "sha2",       "Secure Hash Algorithm 2";
    CRC32,      "crc32",      "Cyclic Redundancy Check 32";
    ATOMIC,   "atomic",    "Atomic memory operations";
    // AA64PFR0_EL1: AArch64 Performance Feature Register 0
    SVE,        "sve",        "Scalable Vector Extension";
    ASIMD,    "asimd",     "Advanced SIMD";
}

