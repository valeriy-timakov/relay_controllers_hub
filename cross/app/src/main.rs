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
    use board::Board;


    #[global_allocator]
    static HEAP: Heap = Heap::empty();

    const MONO_HZ: u32 = 84_000_000;

    #[shared]
    struct Shared {
        board: Board,
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

        let board = Board::init(ctx.device, MONO_HZ);

        let mono = DwtSystick::new(&mut ctx.core.DCB, ctx.core.DWT,
                                   ctx.core.SYST, MONO_HZ);


        polling::spawn_after(1.secs()).ok();

        (
            Shared { board },
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

    #[task(binds = EXTI0, local = [], shared=[board])]
    fn button_pressed(mut ctx: button_pressed::Context) {
        ctx.shared.board.lock(|board: &mut Board| {
            board.on_button_pressed();
        });
    }

    #[task(binds = TIM2, local = [], shared=[board])]
    fn tim2(mut ctx: tim2::Context) {
        ctx.shared.board.lock(|board: &mut Board| {
            board.on_tim2();
        });
    }

    #[task(binds = TIM3, local = [], shared=[board])]
    fn tim3(mut ctx: tim3::Context) {
        ctx.shared.board.lock(|board: &mut Board| {
            board.on_tim3();
        });
    }

    // Important! USART1 and DMA2_STREAM2 should the same interrupt priority!
    #[task(binds = USART1, priority=1, local = [],shared = [board])]
    fn usart1(mut ctx: usart1::Context) {
        ctx.shared.board.lock(|board: &mut Board| {
            board.on_usart1();
        });
    }

    #[task(binds = USART2, priority=1, local = [], shared = [board])]
    fn usart2(mut ctx: usart2::Context) {
        ctx.shared.board.lock(|board: &mut Board| {
            board.on_usart2();
        });
    }

    #[task(binds = USART6, priority=1, local = [],shared = [board])]
    fn usart6(mut ctx: usart6::Context) {
        ctx.shared.board.lock(|board: &mut Board| {
            board.on_usart6();
        });
    }

    #[task(binds = DMA2_STREAM2, priority=1, shared = [board])]
    fn dma2_stream2(mut ctx: dma2_stream2::Context) {
        ctx.shared.board.lock(|board: &mut Board| {
            board.on_dma2_stream2();
        });
    }

    #[task(binds = DMA2_STREAM7, priority=1, shared = [board])]
    fn dma2_stream7(mut ctx: dma2_stream7::Context) {
        ctx.shared.board.lock(|board: &mut Board| {
            board.on_dma2_stream7();
        });
    }

    #[task(binds = DMA2_STREAM1, priority=1, shared = [board])]
    fn dma2_stream1(mut ctx: dma2_stream1::Context) {
        ctx.shared.board.lock(|board: &mut Board| {
            board.on_dma2_stream1();
        });
    }

    #[task(binds = DMA2_STREAM6, priority=1,shared = [board])]
    fn dma2_stream6(mut ctx: dma2_stream6::Context) {
        ctx.shared.board.lock(|board: &mut Board| {
            board.on_dma2_stream6();
        });
    }

    #[task(binds = DMA1_STREAM5, priority=1, shared = [board])]
    fn dma1_stream5(mut ctx: dma1_stream5::Context) {
        ctx.shared.board.lock(|board: &mut Board| {
            board.on_dma1_stream5();
        });
    }

    #[task(binds = DMA1_STREAM6, priority=1, shared = [board])]
    fn dma1_stream6(mut ctx: dma1_stream6::Context) {
        ctx.shared.board.lock(|board: &mut Board| {
            board.on_dma1_stream6();
        });
    }


    #[task(priority=1, shared = [board])]
    fn polling(mut ctx: polling::Context) {
        ctx.shared.board.lock(|board: &mut Board| {
            board.on_polling();
        });
        polling::spawn_after(1.secs()).ok();
    }

    #[task(binds = DMA2_STREAM0, priority=1, shared = [board], local = [])]
    fn dma(mut ctx: dma::Context) {
        ctx.shared.board.lock(|board: &mut Board| {
            board.on_dma();
        });
    }

}

