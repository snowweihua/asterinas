// SPDX-License-Identifier: MPL-2.0

use ostd::arch::cpu::context::{CpuExceptionInfo, UserContext};

use crate::process::signal::{sig_num::SigNum, signals::fault::FaultSignal, SignalContext};

impl SignalContext for UserContext {
    fn set_arguments(&mut self, sig_num: SigNum, siginfo_addr: usize, ucontext_addr: usize) {
        // AArch64 Linux calling convention: x0=arg0, x1=arg1, x2=arg2
        self.set_x0(sig_num.as_u8() as usize);
        self.set_x1(siginfo_addr);
        self.set_x2(ucontext_addr);
    }
}

impl From<&CpuExceptionInfo> for FaultSignal {
    fn from(_trap_info: &CpuExceptionInfo) -> Self {
        unimplemented!()
    }
}
