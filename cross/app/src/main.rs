//#![deny(unsafe_code)]
//#![deny(warnings)]
#![no_main]
#![no_std]

//use panic_halt as _;

mod handlers;

#[rtic::app(device = stm32f4xx_hal::pac, peripherals = true, dispatchers = [EXTI3])]
mod app {
    use stm32f4xx_hal::{
        prelude::*,
    };
    use dwt_systick_monotonic::DwtSystick;
    use embedded_alloc::Heap;
    use board::{ Board, ControllerLinkSlave6, InWork };


    #[global_allocator]
    static HEAP: Heap = Heap::empty();

    const MONO_HZ: u32 = 84_000_000;

    #[shared]
    struct Shared {
        in_work: InWork,
        #[lock_free]
        controller_link_slave6: ControllerLinkSlave6,
    }

    #[local]
    struct Local {
    }

    #[monotonic(binds = SysTick, default = true)]
    type MyMono = DwtSystick<MONO_HZ>;

    #[init]
    fn init(mut ctx: init::Context) -> (Shared, Local, init::Monotonics) {

        {
            use core::mem::MaybeUninit;
            const HEAP_SIZE: usize = 1024;
            static mut HEAP_MEM: [MaybeUninit<u8>; HEAP_SIZE] = [MaybeUninit::uninit(); HEAP_SIZE];
            unsafe { HEAP.init(HEAP_MEM.as_ptr() as usize, HEAP_SIZE) }
        }

        let Board { controller_link_slave6, in_work } = Board::init(ctx.device, MONO_HZ);

        let mono = DwtSystick::new(&mut ctx.core.DCB, ctx.core.DWT, ctx.core.SYST, MONO_HZ);

        polling::spawn_after(1.secs()).ok();

        (
            Shared { controller_link_slave6, in_work },
            Local {  },
            init::Monotonics(mono),
        )
    }

    // Background task, runs whenever no other tasks are running
    #[idle(local = [], shared = [])]
    fn idle(_: idle::Context) -> ! {
        loop {
            cortex_m::asm::wfi();
        }
    }

    #[task(binds = EXTI0, local = [], shared=[in_work])]
    fn button_pressed(mut ctx: button_pressed::Context) {
        ctx.shared.in_work.lock(|in_work: &mut InWork| {
            in_work.on_button_pressed();
        });
    }

    #[task(binds = TIM2, local = [], shared=[in_work])]
    fn tim2(mut ctx: tim2::Context) {
        ctx.shared.in_work.lock(|in_work: &mut InWork| {
            in_work.on_tim2();
        });
    }

    #[task(binds = TIM3, local = [], shared=[in_work])]
    fn tim3(mut ctx: tim3::Context) {
        ctx.shared.in_work.lock(|in_work: &mut InWork| {
            in_work.on_tim3();
        });
    }

    // Important! USART1 and DMA2_STREAM2 should the same interrupt priority!
    #[task(binds = USART1, priority=1, local = [],shared = [in_work])]
    fn usart1(mut ctx: usart1::Context) {
        ctx.shared.in_work.lock(|in_work: &mut InWork| {
            in_work.on_usart1();
        });
    }

    #[task(binds = USART2, priority=1, local = [], shared = [in_work])]
    fn usart2(mut ctx: usart2::Context) {
        ctx.shared.in_work.lock(|in_work: &mut InWork| {
            in_work.on_usart2();
        });
    }

    #[task(binds = USART6, priority=1, local = [], shared = [controller_link_slave6, in_work])]
    fn usart6(mut ctx: usart6::Context) {
        let usart6::SharedResources { mut controller_link_slave6, mut in_work } = ctx.shared;
        in_work.lock(|in_work: &mut InWork| {
            controller_link_slave6.on_get_command(&mut in_work.rtc)
        });
    }

    #[task(binds = DMA2_STREAM2, priority=1, shared = [in_work])]
    fn dma2_stream2(mut ctx: dma2_stream2::Context) {
        ctx.shared.in_work.lock(|in_work: &mut InWork| {
            in_work.on_dma2_stream2();
        });
    }

    #[task(binds = DMA2_STREAM7, priority=1, shared = [in_work])]
    fn dma2_stream7(mut ctx: dma2_stream7::Context) {
        ctx.shared.in_work.lock(|in_work: &mut InWork| {
            in_work.on_dma2_stream7();
        });
    }

    #[task(binds = DMA2_STREAM1, priority=1, shared = [controller_link_slave6])]
    fn dma2_stream1(mut ctx: dma2_stream1::Context) {
        ctx.shared.controller_link_slave6.on_rx_dma_interrupts();
    }

    #[task(binds = DMA2_STREAM6, priority=1,shared = [controller_link_slave6])]
    fn dma2_stream6(mut ctx: dma2_stream6::Context) {
        ctx.shared.controller_link_slave6.on_tx_dma_interrupts();
    }

    #[task(binds = DMA1_STREAM5, priority=1, shared = [in_work])]
    fn dma1_stream5(mut ctx: dma1_stream5::Context) {
        ctx.shared.in_work.lock(|in_work: &mut InWork| {
            in_work.on_dma1_stream5();
        });
    }

    #[task(binds = DMA1_STREAM6, priority=1, shared = [in_work])]
    fn dma1_stream6(mut ctx: dma1_stream6::Context) {
        ctx.shared.in_work.lock(|in_work: &mut InWork| {
            in_work.on_dma1_stream6();
        });
    }

    #[task(binds = DMA2_STREAM0, priority=1, shared = [in_work])]
    fn dma2_stream0(mut ctx: dma2_stream0::Context) {
        ctx.shared.in_work.lock(|in_work: &mut InWork| {
            in_work.on_dma2_stream0();
        });
    }

    #[task(priority=1, shared = [in_work])]
    fn polling(mut ctx: polling::Context) {
        ctx.shared.in_work.lock(|in_work: &mut InWork| {
            in_work.on_polling();
        });
        polling::spawn_after(1.secs()).ok();
    }

}

