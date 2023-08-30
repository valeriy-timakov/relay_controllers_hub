#![allow(unsafe_code)]
#![deny(warnings)]


use core::panic::PanicInfo;
use core::sync::atomic;
use core::sync::atomic::Ordering;
use cortex_m_rt::{exception, ExceptionFrame};
use cortex_m_semihosting::hprintln;

#[exception]
unsafe  fn DefaultHandler(irqn: i16) {
    hprintln!("irqn={}", irqn);
    loop {}
}

#[exception]
unsafe  fn HardFault(ef: &ExceptionFrame) -> ! {
    /*if let Ok(mut hstdout) = hio::hstdout() {
        writeln!(hstdout, "{:#?}", ef).ok();
    }*/
    hprintln!("ef={:#?}", ef);

    loop {}
}


#[inline(never)]
#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {
        atomic::compiler_fence(Ordering::SeqCst);
    }
}

