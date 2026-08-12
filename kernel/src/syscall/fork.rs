// SPDX-License-Identifier: MPL-2.0

use ostd::arch::cpu::context::UserContext;

use super::SyscallReturn;
use crate::{
    prelude::*,
    process::{clone_child, CloneArgs},
};

pub fn sys_fork(ctx: &Context, parent_context: &UserContext) -> Result<SyscallReturn> {
    warn!("+F");
    let clone_args = CloneArgs::for_fork();
    let child_pid = clone_child(ctx, parent_context, clone_args).unwrap();
    warn!("-F");
    Ok(SyscallReturn::Return(child_pid as _))
}

pub fn sys_vfork(ctx: &Context, parent_context: &UserContext) -> Result<SyscallReturn> {
    info!("sys_vfork: called");
    let clone_args = CloneArgs::for_vfork();
    let child_pid = clone_child(ctx, parent_context, clone_args).unwrap();
    info!("sys_vfork: child pid = {}", child_pid);
    Ok(SyscallReturn::Return(child_pid as _))
}
