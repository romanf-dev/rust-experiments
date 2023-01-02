#![no_std]
#![no_main]

use core::panic::PanicInfo;

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}

#[no_mangle]
pub fn _exception() -> ! {
    loop {}
}

#[no_mangle]
pub fn _interrupt() -> ! {
    loop {}
}

#[no_mangle]
pub fn _start() -> ! {
    loop {}
}

