/* SPDX-License-Identifier: MPL-2.0 */

.text
.global context_switch
.global first_context_switch
.global kernel_task_entry_wrapper

context_switch: # (nxt: *const TaskContext, cur: *mut TaskContext)
  # Save cur's register
    stp     x19, x20, [x1, #16*0]
    stp     x21, x22, [x1, #16*1]
    stp     x23, x24, [x1, #16*2]
    stp     x25, x26, [x1, #16*3]
    stp     x27, x28, [x1, #16*4]
    stp     x29, x30, [x1, #16*5]
    mrs     x19, tpidr_el0
    mov     x20, sp
    stp     x19, x20, [x1, #16*6]
  # Fallthrough

first_context_switch: # (nxt: *const TaskContext)
  # Restore nxt's registers
    ldp     x23, x24, [x0, #16*6]
    msr     tpidr_el0, x23
    mov     sp, x24
    ldp     x19, x20, [x0, #16*0]
    ldp     x21, x22, [x0, #16*1]
    ldp     x23, x24, [x0, #16*2]
    ldp     x25, x26, [x0, #16*3]
    ldp     x27, x28, [x0, #16*4]
    ldp     x29, x30, [x0, #16*5]
  ret

kernel_task_entry_wrapper:
  .cfi_startproc
  .cfi_undefined ra # mark return address as undefined to indicate end of call stack
  call kernel_task_entry
  .cfi_endproc
