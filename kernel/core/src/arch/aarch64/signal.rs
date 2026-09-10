// SPDX-License-Identifier: MPL-2.0

use ostd::{
    arch::cpu::context::{CpuException, UserContext},
    user::UserContextApi,
};

use crate::{
    process::signal::{
        SignalContext,
        constants::{BUS_ADRALN, ILL_ILLOPC, SEGV_MAPERR, SIGBUS, SIGILL, SIGSEGV},
        sig_num::SigNum,
        signals::fault::FaultSignal,
    },
    thread::exception::ToFaultSignal,
};

impl SignalContext for UserContext {
    fn set_arguments(&mut self, sig_num: SigNum, siginfo_addr: usize, ucontext_addr: usize) {
        // AArch64 Linux calling convention: x0=arg0, x1=arg1, x2=arg2
        self.set_x0(sig_num.as_u8() as usize);
        self.set_x1(siginfo_addr);
        self.set_x2(ucontext_addr);
    }
}

impl ToFaultSignal for CpuException {
    fn to_fault_signal(&self, user_ctx: &UserContext) -> Option<FaultSignal> {
        use CpuException::*;

        let sepc = user_ctx.instruction_pointer() as u64;

        let (num, code, addr) = match self {
            InstructionAbortLowerEL(addr) | DataAbortLowerEL(addr, _) => {
                (SIGSEGV, SEGV_MAPERR, *addr as u64)
            }
            SPAlignmentFault | PCAlignmentFault => (SIGBUS, BUS_ADRALN, sepc),
            TrappedSimdFpSve => (SIGILL, ILL_ILLOPC, sepc),
            _ => (SIGSEGV, SEGV_MAPERR, sepc),
        };

        Some(FaultSignal::new(num, code, Some(addr)))
    }
}
