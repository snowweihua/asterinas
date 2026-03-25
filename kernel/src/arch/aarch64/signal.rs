// SPDX-License-Identifier: MPL-2.0

use ostd::arch::cpu::context::{CpuException, CpuExceptionInfo, UserContext};

use crate::process::signal::{
    constants::{SEGV_ACCERR, SEGV_MAPERR, SIGBUS, SIGILL, SIGSEGV},
    sig_num::SigNum,
    signals::fault::FaultSignal,
    SignalContext,
};

impl SignalContext for UserContext {
    fn set_arguments(&mut self, sig_num: SigNum, siginfo_addr: usize, ucontext_addr: usize) {
        // AArch64 Linux calling convention: x0=arg0, x1=arg1, x2=arg2
        self.set_x0(sig_num.as_u8() as usize);
        self.set_x1(siginfo_addr);
        self.set_x2(ucontext_addr);
    }
}

impl From<&CpuExceptionInfo> for FaultSignal {
    fn from(trap_info: &CpuExceptionInfo) -> Self {
        let addr = Some(trap_info.page_fault_addr as u64);
        let (num, code) = match trap_info.code {
            CpuException::InstructionAbortLowerEL | CpuException::DataAbortLowerEL => {
                // ISS DFSC/IFSC bits [5:0]: 0b0001xx = translation fault (unmapped), 0b0010xx = access flag
                // bit 6 in ISS is WnR for data aborts; DFSC[5:0] = fault status code
                let fsc = trap_info.error_code & 0x3f;
                let is_perm_fault = fsc >= 0x0c && fsc <= 0x0f; // 0x0c..0x0f = permission fault LSB
                let code = if is_perm_fault { SEGV_ACCERR } else { SEGV_MAPERR };
                (SIGSEGV, code)
            }
            CpuException::SPAlignmentFault | CpuException::PCAlignmentFault => (SIGBUS, 1), // BUS_ADRALN
            CpuException::TrappedSimdFpSve => (SIGILL, 1),                                  // ILL_ILLOPC
            _ => (SIGSEGV, SEGV_MAPERR),
        };
        FaultSignal::new(num, code, addr)
    }
}
