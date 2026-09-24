// SPDX-License-Identifier: MPL-2.0

use crate::{prelude::*, syscall::SyscallReturn};

/// expand the user heap to new heap end, returns the new heap end if expansion succeeds.
pub(super) fn sys_brk(heap_end: u64, ctx: &Context) -> Result<SyscallReturn> {
    #[cfg(target_arch = "aarch64")]
    {
        ostd::arch::serial::marker(b'B'); // brk entry
        for shift in [28usize, 24, 20, 16, 12, 8, 4, 0] {
            let n = ((heap_end >> shift) & 0xf) as u8;
            ostd::arch::serial::marker(if n < 10 { b'0' + n } else { b'a' + n - 10 });
        }
        ostd::arch::serial::marker(b'v'); // brk: hex done, body entered
    }
    let new_heap_end = if heap_end == 0 {
        None
    } else {
        Some(heap_end as usize)
    };
    debug!("new heap end = {:x?}", heap_end);

    let user_space = ctx.user_space();
    let user_heap = user_space.vmar().process_vm().heap();
    #[cfg(target_arch = "aarch64")]
    ostd::arch::serial::marker(b'w'); // brk: accessors done

    let current_heap_end = match new_heap_end {
        Some(addr) => user_heap
            .modify_heap_end(addr, ctx)
            .unwrap_or_else(|cur_heap_end| cur_heap_end),
        None => {
            #[cfg(target_arch = "aarch64")]
            ostd::arch::serial::marker(b'y'); // brk(0): pre heap lock
            let end = {
                let guard = user_heap.lock();
                #[cfg(target_arch = "aarch64")]
                ostd::arch::serial::marker(b'z'); // brk(0): heap locked
                guard.heap_range().end
            };
            end
        }
    };
    #[cfg(target_arch = "aarch64")]
    ostd::arch::serial::marker(b'C'); // brk: returning
    Ok(SyscallReturn::Return(current_heap_end as _))
}
