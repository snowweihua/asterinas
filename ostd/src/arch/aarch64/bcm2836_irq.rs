// SPDX-License-Identifier: MPL-2.0

//! BCM2836 Local Interrupt Controller for Raspberry Pi 3B/3B+.
//!
//! The BCM2836/BCM2837 (used on Raspberry Pi 3) does not have a GIC. Instead,
//! it uses the ARM local interrupt controller for per-CPU interrupts.
//!
//! Register base: PA 0x4000_0000 (ARM Local registers)
//!
//! ARM local interrupt controller register offsets (per core):
//!   0x34: Core0 Timer Interrupts Control
//!   0x38: Core0 Mailboxes Interrupts Control
//!   0x3C: Core0 Doorbells Interrupts Control
//!   0x40: Core0 IRQ Source (read-only)
//!   0x44: Core0 FIQ Source (read-only)
//!
//! Timer interrupt bits in control/source registers:
//!   bit 0: CNTPNSIRQ (physical timer, non-secure)
//!   bit 1: CNTPSIRQ  (physical timer, secure)
//!   bit 2: CNTVIRQ   (virtual timer)
//!   bit 3: CNTHPIRQ  (hypervisor timer)


/// BCM2836 ARM Local Interrupt Controller (one set of regs per core).
const LOCAL_IC_BASE_PA: usize = 0x4000_0000;

const CORE0_TIMER_INT_CONTROL: usize = 0x40;
const CORE0_IRQ_SOURCE: usize = 0x60;

/// Spin-table mailbox / cpu-release-addr registers (per core).
/// Standard RPi3 DTB values:
///   cpu@1: cpu-release-addr = <0x0 0x000000e8>
///   cpu@2: cpu-release-addr = <0x0 0x000000f0>
///   cpu@3: cpu-release-addr = <0x0 0x000000f8>
const CORE1_BOOT_CONTROL: usize = 0xE8;
const CORE2_BOOT_CONTROL: usize = 0xF0;
const CORE3_BOOT_CONTROL: usize = 0xF8;

/// Per-core register block stride for ARM local registers.
const CORE_REG_STRIDE: usize = 0x400;

const CNTVIRQ_BIT: u32 = 1 << 3;

/// BCM2835 Peripheral Interrupt Controller (shared across RPi3 peripherals).
///
/// U-Boot enables the USB/Ethernet interrupt while doing TFTP downloads.
/// If those interrupts are not disabled before the kernel enables IRQs, every
/// pending peripheral interrupt re-enters `irq_current`, which returns without
/// dismissing the source, causing an IRQ storm → stack overflow → random crash.
const BCM2835_IC_BASE_PA: usize = 0x3F00_B200;
const BCM2835_IRQS_PENDING_2: usize = 0x08; // pending GPU IRQs 32-63
const BCM2835_ENABLE_IRQS_2: usize = 0x14; // enable GPU IRQs 32-63
const BCM2835_DISABLE_IRQS_1: usize = 0x1C; // disable IRQs 0-31
const BCM2835_DISABLE_IRQS_2: usize = 0x20; // disable IRQs 32-63
const BCM2835_DISABLE_BASIC: usize = 0x24; // disable basic IRQs
/// FIQ control register.  Bit 7 = FIQ_ENABLE, bits[6:0] = source select.
/// U-Boot enables the USB DWC interrupt as FIQ for TFTP; we must clear
/// this register before unmasking the CPU F-bit, otherwise the unacknowledged
/// FIQ fires in a storm and starves the main kthread.
const BCM2835_FIQ_CONTROL: usize = 0x0C;

/// CORE0_IRQ_SOURCE bit 8 = GPU IRQ (a BCM2835 peripheral IRQ is pending).
const GPU_IRQ_BIT: u32 = 1 << 8;

/// PL011 UART is GPU IRQ 57 → bit 25 of IRQS_PENDING_2 / ENABLE_IRQS_2.
const UART_PERI_IRQ_BIT: u32 = 1 << 25;

/// Abstract IRQ number returned by `acknowledge_interrupt()` for the PL011 UART.
/// Must match the number allocated via `IrqLine::alloc_specific` in the kernel driver.
pub const UART_IRQ_NUM: usize = 57;

fn local_ic_base_va() -> usize {
    // Use the kernel linear mapping: the ARM-local registers stay accessible
    // after TTBR0 is switched to a user page table.
    crate::mm::kspace::paddr_to_vaddr(LOCAL_IC_BASE_PA)
}

/// Read MPIDR_EL1 and return the affinity level 0 (core ID within cluster).
fn core_id() -> usize {
    let mpidr: u64;
    unsafe {
        core::arch::asm!("mrs {}, mpidr_el1", out(reg) mpidr, options(nomem, nostack));
    }
    (mpidr & 0x3) as usize
}

unsafe fn read_reg(offset: usize) -> u32 {
    unsafe { core::ptr::read_volatile((local_ic_base_va() + offset) as *const u32) }
}

unsafe fn write_reg(offset: usize, value: u32) {
    unsafe {
        core::ptr::write_volatile((local_ic_base_va() + offset) as *mut u32, value);
    }
}

pub unsafe fn init_on_bsp() {
    // Disable ARM-local timer IRQ on Core 0 (re-enabled later by enable_timer_irq).
    unsafe { write_reg(CORE0_TIMER_INT_CONTROL, 0) };

    // Disable ALL BCM2835 peripheral interrupts AND FIQ.
    // Use the kernel linear mapping so the MMIO window stays accessible after
    // TTBR0 is switched to a user page table.
    let ic_va = crate::mm::kspace::paddr_to_vaddr(BCM2835_IC_BASE_PA);
    unsafe {
        core::ptr::write_volatile((ic_va + BCM2835_DISABLE_IRQS_1) as *mut u32, 0xFFFF_FFFF);
        core::ptr::write_volatile((ic_va + BCM2835_DISABLE_IRQS_2) as *mut u32, 0xFFFF_FFFF);
        core::ptr::write_volatile((ic_va + BCM2835_DISABLE_BASIC) as *mut u32, 0xFFFF_FFFF);
        // Clear FIQ enable bit.  U-Boot may have left the USB DWC or another
        // peripheral routed as FIQ.  Once the CPU F-bit is unmasked by
        // enable_local(), an unacknowledged FIQ fires in a tight storm and
        // starves all kernel threads.
        core::ptr::write_volatile((ic_va + BCM2835_FIQ_CONTROL) as *mut u32, 0);
    }
}

pub fn enable_timer_irq() {
    let core = core_id();
    let offset = CORE0_TIMER_INT_CONTROL + (core * CORE_REG_STRIDE);
    unsafe { write_reg(offset, CNTVIRQ_BIT) };
}

pub unsafe fn init_on_ap() {
    let core = core_id();
    let offset = CORE0_TIMER_INT_CONTROL + (core * CORE_REG_STRIDE);
    unsafe { write_reg(offset, CNTVIRQ_BIT) };
}

/// Enable the BCM2835 PL011 UART IRQ (GPU IRQ 57).
///
/// Writes bit 25 to ENABLE_IRQS_2 so that UART RX interrupts are routed
/// from the peripheral controller to CPU0.  No-op on non-RPi3 boards.
pub fn enable_uart_irq() {
    // BCM2835 peripheral controller only exists on RPi3 (cached board type 2).
    // Accessing these addresses on QEMU virt would cause a fault.
    if crate::arch::board::BoardType::cached() != 2 {
        return;
    }
    let ic_va = crate::mm::kspace::paddr_to_vaddr(BCM2835_IC_BASE_PA);
    unsafe {
        core::ptr::write_volatile(
            (ic_va + BCM2835_ENABLE_IRQS_2) as *mut u32,
            UART_PERI_IRQ_BIT,
        );
    }
}

pub fn read_irq_source() -> u32 {
    unsafe { read_reg(CORE0_IRQ_SOURCE) }
}

pub fn acknowledge_interrupt() -> usize {
    let core = core_id();
    let offset = CORE0_IRQ_SOURCE + (core * CORE_REG_STRIDE);
    let pending = unsafe { read_reg(offset) };

    if pending & CNTVIRQ_BIT != 0 {
        // Timer IRQ: return the virtual timer PPI number used by AArch64 timer::init().
        return 27;
    }

    if pending & GPU_IRQ_BIT != 0 {
        // A BCM2835 peripheral IRQ is pending.  Check which one.
        let ic_va = crate::mm::kspace::paddr_to_vaddr(BCM2835_IC_BASE_PA);
        let irq2 =
            unsafe { core::ptr::read_volatile((ic_va + BCM2835_IRQS_PENDING_2) as *const u32) };
        if irq2 & UART_PERI_IRQ_BIT != 0 {
            return UART_IRQ_NUM; // PL011 UART RX
        }
        // Unknown peripheral IRQ — silently ignore to avoid IRQ storms.
        return 0;
    }

    if pending != 0 {
        // Unexpected ARM-local interrupt (PMU, mailbox, etc.).
        crate::console::early_print(format_args!("[irq] unexpected CORE0_IRQ_SOURCE\n"));
        return 0;
    }

    // No interrupt pending (spurious wakeup).
    0
}

pub fn end_interrupt(_irq: usize) {}

/// Mailbox IRQ set register offsets (per core).
/// Writing to COREn_MAILBOX0_SET generates a mailbox IRQ to core n.
const CORE0_MAILBOX0_SET: usize = 0x80;
const CORE1_MAILBOX0_SET: usize = 0x84;
const CORE2_MAILBOX0_SET: usize = 0x88;
const CORE3_MAILBOX0_SET: usize = 0x8C;

/// Trigger a mailbox IRQ to wake up the given core from WFE.
pub unsafe fn trigger_mailbox_irq(core_id: u32) {
    let offset = match core_id {
        1 => CORE1_MAILBOX0_SET,
        2 => CORE2_MAILBOX0_SET,
        3 => CORE3_MAILBOX0_SET,
        _ => return,
    };
    let base_va = local_ic_base_va();
    let reg_addr = base_va + offset;
    // SAFETY: early_puts writes directly to UART, safe during early boot
    unsafe {
        crate::arch::boot::pl011_puts(b"[a2-smp] rpi3: mailbox base=");
        let mut buf = [0u8; 20];
        let hex = b"0123456789abcdef";
        buf[0] = b'0'; buf[1] = b'x';
        for i in 0..16 {
            let nibble = (base_va >> (60 - i * 4)) & 0xf;
            buf[2 + i] = hex[nibble as usize];
        }
        buf[18] = b'\n'; buf[19] = 0;
        crate::arch::boot::pl011_puts(&buf[..19]);
    }
    // Read and print core IRQ source to see what's pending
    unsafe {
        let irq_src = core::ptr::read_volatile((base_va + 0x60) as *const u32);
        crate::arch::boot::pl011_puts(b"[a2-smp] rpi3: core-irq-source=");
        let mut buf = [0u8; 20];
        let hex = b"0123456789abcdef";
        buf[0] = b'0'; buf[1] = b'x';
        for i in 0..8 {
            let nibble = (irq_src >> (28 - i * 4)) & 0xf;
            buf[2 + i] = hex[nibble as usize];
        }
        buf[10] = b'\n'; buf[11] = 0;
        crate::arch::boot::pl011_puts(&buf[..11]);
    }
    unsafe {
        core::ptr::write_volatile((reg_addr) as *mut u32, 1);
        core::arch::asm!("dsb ish", "sev", "isb", options(nostack, preserves_flags));
    }
}

/// Write the spin-table cpu-release-addr for the given core.
///
/// `core_id` is 1-3 (BSP is 0).  `entry_pa` is the physical address
/// where the AP should start executing (MMU off).
pub unsafe fn write_spin_table_mailbox(core_id: u32, entry_pa: u64) {
    let offset = match core_id {
        1 => CORE1_BOOT_CONTROL,
        2 => CORE2_BOOT_CONTROL,
        3 => CORE3_BOOT_CONTROL,
        _ => return,
    };
    unsafe {
        core::ptr::write_volatile((local_ic_base_va() + offset) as *mut u64, entry_pa);
    }
}
