// SPDX-License-Identifier: MPL-2.0

//! Restartable sequences (rseq) syscall stub.

use crate::{context::Context, prelude::*};

/// Handle rseq syscall.
///
/// This is a minimal stub implementation that returns success (0).
/// A full implementation would register a restartable sequences region
/// for the calling thread.
///
/// # Arguments
/// * `rseq_ptr` - pointer to rseq structure (ignored in stub)
/// * `rseq_len` - length of rseq structure (must be >= sizeof(struct rseq))
/// * `flags` - rseq flags (ignored in stub)
/// * `sig` - restart signal (ignored in stub)
///
/// # Returns
/// Always returns 0 (success).
pub fn sys_rseq(
    _rseq_ptr: usize,
    _rseq_len: u32,
    _flags: u32,
    _sig: u32,
    _ctx: &Context,
) -> Result<super::SyscallReturn> {
    // Minimal stub: just return success
    // A full implementation would register the rseq region
    Ok(super::SyscallReturn::Return(0))
}