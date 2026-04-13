#![no_std]
#![no_main]

use core::arch::asm;

#[no_mangle]
pub extern "C" fn _start() {
    unsafe {
        asm!(
            "mov x8, 93",
            "mov x0, 42",
            "svc 0",
            options(noreturn)
        );
    }
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}
