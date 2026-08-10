// SPDX-License-Identifier: MPL-2.0

use core::fmt;

use ostd::{
    arch::cpu::context::{CpuException, CpuExceptionInfo, UserContext},
    cpu::PinCurrentCpu,
    task::DisabledPreemptGuard,
    user::UserContextApi,
    Pod,
};

use crate::{cpu::LinuxAbi, thread::exception::PageFaultInfo, vm::perms::VmPerms};

/// AArch64 Linux ABI: syscall number is in x8, arguments in x0..x5, return value in x0.
/// Reference: <https://man7.org/linux/man-pages/man2/syscall.2.html> (AArch64 row)
impl LinuxAbi for UserContext {
    fn syscall_num(&self) -> usize {
        self.x8()
    }

    fn set_syscall_num(&mut self, num: usize) {
        self.set_x8(num);
    }

    fn syscall_ret(&self) -> usize {
        self.x0()
    }

    fn set_syscall_ret(&mut self, ret: usize) {
        self.set_x0(ret);
    }

    fn syscall_args(&self) -> [usize; 6] {
        [
            self.x0(),
            self.x1(),
            self.x2(),
            self.x3(),
            self.x4(),
            self.x5(),
        ]
    }
}

/// Represents the context of a signal handler on AArch64.
///
/// Saved before invoking a signal handler; restored by `sys_rt_sigreturn`.
/// Reference: <https://elixir.bootlin.com/linux/v6.15.7/source/arch/arm64/include/uapi/asm/sigcontext.h>
#[repr(C)]
#[repr(align(16))]
#[derive(Clone, Copy, Debug, Default, Pod)]
pub struct SigContext {
    /// Saved program counter (ELR_EL1).
    pub pc: usize,
    /// Saved processor state (SPSR_EL1).
    pub pstate: usize,
    /// Saved stack pointer.
    pub sp: usize,
    /// General-purpose registers x0..x29 (30 registers).
    pub regs: [usize; 30],
    /// Link register (x30 / LR).
    pub regs_lr: usize,
}

impl SigContext {
    /// Restore user registers from this context.
    pub fn copy_user_regs_to(&self, dst: &mut UserContext) {
        let gp = dst.general_regs_mut();
        gp.x0 = self.regs[0];
        gp.x1 = self.regs[1];
        gp.x2 = self.regs[2];
        gp.x3 = self.regs[3];
        gp.x4 = self.regs[4];
        gp.x5 = self.regs[5];
        gp.x6 = self.regs[6];
        gp.x7 = self.regs[7];
        gp.x8 = self.regs[8];
        gp.x9 = self.regs[9];
        gp.x10 = self.regs[10];
        gp.x11 = self.regs[11];
        gp.x12 = self.regs[12];
        gp.x13 = self.regs[13];
        gp.x14 = self.regs[14];
        gp.x15 = self.regs[15];
        gp.x16 = self.regs[16];
        gp.x17 = self.regs[17];
        gp.x18 = self.regs[18];
        gp.x19 = self.regs[19];
        gp.x20 = self.regs[20];
        gp.x21 = self.regs[21];
        gp.x22 = self.regs[22];
        gp.x23 = self.regs[23];
        gp.x24 = self.regs[24];
        gp.x25 = self.regs[25];
        gp.x26 = self.regs[26];
        gp.x27 = self.regs[27];
        gp.x28 = self.regs[28];
        gp.x29 = self.regs[29];
        dst.set_instruction_pointer(self.pc);
        dst.set_stack_pointer(self.sp);
        dst.set_lr(self.regs_lr);
    }

    /// Save user registers into this context.
    pub fn copy_user_regs_from(&mut self, src: &UserContext) {
        let gp = src.general_regs();
        self.regs[0] = gp.x0;
        self.regs[1] = gp.x1;
        self.regs[2] = gp.x2;
        self.regs[3] = gp.x3;
        self.regs[4] = gp.x4;
        self.regs[5] = gp.x5;
        self.regs[6] = gp.x6;
        self.regs[7] = gp.x7;
        self.regs[8] = gp.x8;
        self.regs[9] = gp.x9;
        self.regs[10] = gp.x10;
        self.regs[11] = gp.x11;
        self.regs[12] = gp.x12;
        self.regs[13] = gp.x13;
        self.regs[14] = gp.x14;
        self.regs[15] = gp.x15;
        self.regs[16] = gp.x16;
        self.regs[17] = gp.x17;
        self.regs[18] = gp.x18;
        self.regs[19] = gp.x19;
        self.regs[20] = gp.x20;
        self.regs[21] = gp.x21;
        self.regs[22] = gp.x22;
        self.regs[23] = gp.x23;
        self.regs[24] = gp.x24;
        self.regs[25] = gp.x25;
        self.regs[26] = gp.x26;
        self.regs[27] = gp.x27;
        self.regs[28] = gp.x28;
        self.regs[29] = gp.x29;
        self.pc = src.instruction_pointer();
        self.sp = src.stack_pointer();
        self.regs_lr = src.lr();
    }
}

impl TryFrom<&CpuExceptionInfo> for PageFaultInfo {
    type Error = ();

    fn try_from(value: &CpuExceptionInfo) -> Result<Self, ()> {
        // EC field is in code (CpuException enum).
        // Instruction Abort: EC 0x20/0x21 -> EXEC required
        // Data Abort: EC 0x24/0x25 -> ISS[6] = WnR (write=1, read=0)
        let required_perms = match value.code {
            CpuException::InstructionAbortLowerEL | CpuException::InstructionAbortCurrentEL => {
                VmPerms::EXEC
            }
            CpuException::DataAbortLowerEL | CpuException::DataAbortCurrentEL => {
                // error_code is raw ESR_EL1; ISS[6] = WnR
                let wnr = (value.error_code >> 6) & 1;
                if wnr == 1 {
                    VmPerms::WRITE
                } else {
                    VmPerms::READ
                }
            }
            _ => return Err(()),
        };

        Ok(PageFaultInfo {
            address: value.page_fault_addr,
            required_perms,
        })
    }
}

/// CPU information for `/proc/cpuinfo`.
pub struct CpuInformation {
    processor: u32,
}

impl CpuInformation {
    /// Constructs the information for the current CPU.
    pub fn new(guard: &DisabledPreemptGuard) -> Self {
        Self {
            processor: guard.current_cpu().as_usize() as u32,
        }
    }
}

impl fmt::Display for CpuInformation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "processor\t: {}", self.processor)
    }
}
