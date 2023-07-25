#![deny(unsafe_code)]
//#![deny(warnings)]
#![no_main]
#![no_std]

use stm32_test::app_logic::slave_controller_link::{ SentRequest, SignalData, SignalsReceiver};
use stm32_test::app_logic::slave_controller_link::domain::{DataInstructions, ErrorCode, Signals};
use stm32_test::errors::Errors;
//use panic_halt as _;


pub struct SignalReceiverImp();

impl SignalsReceiver for SignalReceiverImp {
    fn on_signal(&mut self, _: SignalData) {
        todo!()
    }

    fn on_signal_error(&mut self, _: Option<Signals>, _: ErrorCode) {
        todo!()
    }

    fn on_request_success(&mut self, _: &SentRequest) {
        todo!()
    }

    fn on_request_error(&mut self, request: &SentRequest, error_code: ErrorCode) {
        todo!()
    }

    fn on_request_parse_error(&mut self, request: &SentRequest, error: Errors, data: &[u8]) {
        todo!()
    }

    fn on_request_response(&mut self, request: &SentRequest, response: DataInstructions) {
        todo!()
    }
}

#[rtic::app(device = stm32f4xx_hal::pac, peripherals = true, dispatchers = [EXTI3])]
mod app {
    use cortex_m_semihosting::hprintln;
    use stm32f4xx_hal::{
        gpio::{ self, Edge, Input },
        pac::{ TIM2, TIM3, Peripherals, DMA2, USART1 },
        prelude::*,
        timer,
        dma::{
            Stream2, StreamsTuple,
        },
        serial::{ Config, config },
        rtc::{ Rtc },
    };
    use dwt_systick_monotonic::DwtSystick;
    use stm32f4xx_hal::dma::{Stream1, Stream5, Stream6, Stream7};
    use stm32f4xx_hal::pac::{DMA1, USART2, USART6};
    use time::{Date, PrimitiveDateTime, Time};
    use time::Month;
    use stm32_test::app_logic::adc_transfer::{ ADCTransfer};
    use stm32_test::hal_ext::rtc_wrapper::{ RtcWrapper};
    use stm32_test::app_logic::led::Led;
    use stm32_test::app_logic::slave_controller_link::SlaveControllerLink;
    use stm32_test::hal_ext::serial_transfer::{RxTransfer, SerialTransfer, TxTransfer};
    use stm32_test::utils::write_to;
    use crate::{ SignalReceiverImp };

    const MONO_HZ: u32 = 84_000_000;
    type Serial1Transfer = SerialTransfer<USART1, Stream7<DMA2>, 4, Stream2<DMA2>, 4>;
    type Rx1Transfer = RxTransfer<USART1, Stream2<DMA2>, 4>;
    type Tx1Transfer = TxTransfer<USART1, Stream7<DMA2>, 4>;
    type Serial2Transfer = SerialTransfer<USART2, Stream6<DMA1>, 4, Stream5<DMA1>, 4>;
    type Rx2Transfer = RxTransfer<USART2, Stream5<DMA1>, 4>;
    type Tx2Transfer = TxTransfer<USART2, Stream6<DMA1>, 4>;
    type ControllerLinkSlave6 = SlaveControllerLink<USART6, Stream6<DMA2>, 5, Stream1<DMA2>, 5, SignalReceiverImp>;

    // Resources shared between tasks
    #[shared]
    struct Shared {
        #[lock_free]
        serial_transfer_1: Serial1Transfer,
        serial_transfer_2: Serial2Transfer,
        controller_link_slave6: ControllerLinkSlave6,
        led: Led<'C', 13>,
        adc_transfer: ADCTransfer,
        rtc: RtcWrapper,
    }

    // Local resources to specific tasks (cannot be shared)
    #[local]
    struct Local {
        button: gpio::PA0<Input>,
        counter: timer::CounterMs<TIM3>,
        counter2: timer::CounterMs<TIM2>,
    }

    #[monotonic(binds = SysTick, default = true)]
    type MyMono = DwtSystick<MONO_HZ>;

    #[init]
    fn init(mut ctx: init::Context) -> (Shared, Local, init::Monotonics) {
        let mut dp: Peripherals = ctx.device;

        let rcc = dp.RCC.constrain();
        let clocks = rcc.cfgr
            .use_hse(25.MHz())
            .require_pll48clk()
            .sysclk(MONO_HZ.Hz())
            .hclk(MONO_HZ.Hz())
            .pclk1(21.MHz())
            .pclk2(42.MHz())
            .freeze();

        let rtc_not_initialized = dp.RTC.isr.read().inits().is_not_initalized();
        let mut rtc = RtcWrapper::new( Rtc::new(dp.RTC, &mut dp.PWR) );

        if rtc_not_initialized {
            let date = Date::from_calendar_date(2023, Month::June, 13).unwrap();
            let time = Time::from_hms(8, 26, 30).unwrap();
            let date_time = PrimitiveDateTime::new(date, time);
            rtc.set_datetime(date_time).unwrap();
        }

        let mono = DwtSystick::new(&mut ctx.core.DCB, ctx.core.DWT,
                                   ctx.core.SYST, MONO_HZ);

        let gpioa = dp.GPIOA.split();
        let gpiob = dp.GPIOB.split();
        let gpioc = dp.GPIOC.split();

        let dma2 = StreamsTuple::new(dp.DMA2);
        let dma1 = StreamsTuple::new(dp.DMA1);

        let serial1 = dp.USART1.serial(
            (gpioa.pa9.into_alternate(), gpioa.pa10),
            Config::default()
                .baudrate(115_200.bps())
                .dma(config::DmaConfig::TxRx),
            &clocks,
        ).unwrap();

        let serial2 = dp.USART2.serial(
            (gpioa.pa2.into_alternate(), gpioa.pa3),
            Config::default()
                .baudrate(18_200.bps())
                .dma(config::DmaConfig::TxRx),
            &clocks,
        ).unwrap();

        let serial6 = dp.USART6.serial(
            (gpioa.pa11.into_alternate(), gpioa.pa12),
            Config::default()
                .baudrate(18_200.bps())
                .dma(config::DmaConfig::TxRx),
            &clocks,
        ).unwrap();

        let serial_transfer_1 = SerialTransfer::new(serial1, dma2.7, dma2.2);
        let serial_transfer_2 = SerialTransfer::new(serial2, dma1.6, dma1.5);
        let serial_transfer_6 = SerialTransfer::new(serial6, dma2.6, dma2.1);

        let signal_receiver = SignalReceiverImp();
        let controller_link_slave6 = SlaveControllerLink::create(serial_transfer_6, signal_receiver).unwrap();

        let led = Led::new(4, 2, true, gpioc.pc13.into_push_pull_output());
        let mut button = gpioa.pa0.into_pull_up_input();

        let mut syscfg = dp.SYSCFG.constrain();
        button.make_interrupt_source(&mut syscfg);
        button.trigger_on_edge(&mut dp.EXTI, Edge::Rising);
        button.enable_interrupt(&mut dp.EXTI);

        let mut counter = dp.TIM3.counter_ms(&clocks);
        counter.start(500_u32.millis()).unwrap();
        counter.listen(timer::Event::Update);

        let mut counter2 = dp.TIM2.counter_ms(&clocks);
        counter2.start(5000_u32.millis()).unwrap();
        counter2.listen(timer::Event::Update);



        let adc_transfer =
            ADCTransfer::new(dma2.0, dp.ADC1, gpiob.pb1.into_analog());

        polling::spawn_after(1.secs()).ok();

        (
            Shared { led, serial_transfer_1, serial_transfer_2, controller_link_slave6, adc_transfer, rtc },
            Local { button, counter, counter2 },
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

    #[task(binds = EXTI0, local = [button], shared=[led])]
    fn button_pressed(mut ctx: button_pressed::Context) {
        ctx.local.button.clear_interrupt_pending_bit();
        ctx.shared.led.lock(|led: &mut Led<'C', 13>| {
            led.update_periods(|prev_on_cylcles_count: u16, prev_off_cycles_count|{
                let mut off_cylcles_count = prev_off_cycles_count - 1;
                if off_cylcles_count == 0 {
                    off_cylcles_count = 8;
                }
                hprintln!("off_cylcles_count: {}", off_cylcles_count);
                (prev_on_cylcles_count, off_cylcles_count)
            });
        });
    }

    #[task(binds = TIM2, local = [counter2], shared=[serial_transfer_1, rtc])]
    fn tim2(mut ctx: tim2::Context) {
        let counter: &mut timer::CounterMs<TIM2> = ctx.local.counter2;
        counter.clear_interrupt(timer::Event::Update);
        counter.now().ticks();
        let time: PrimitiveDateTime = ctx.shared.rtc.lock(|rtc: &mut RtcWrapper| {rtc.get_datetime()});
        let mut buf = [0u8; 64];
        let _s: &str = write_to::show(
            &mut buf,
            format_args!("time: {}.{}.{} {}:{}:{}\r\n", time.day(), time.month(),
                         time.year(), time.hour(), time.minute(),
                         time.second())
        ).unwrap();
        let serial_transfer_1: &mut Serial1Transfer = ctx.shared.serial_transfer_1;
        let tx: &mut Tx1Transfer = serial_transfer_1.tx();
        tx.start_transfer(|buf| {
            buf.add_str(_s)
        }).unwrap();

    }

    #[task(binds = TIM3, local = [counter], shared=[led])]
    fn tim3(mut ctx: tim3::Context) {
        let counter: &mut timer::CounterMs<TIM3> = ctx.local.counter;
        counter.clear_interrupt(timer::Event::Update);
        ctx.shared.led.lock(|led: &mut Led<'C', 13>| {
            led.update();
        });
    }

    // Important! USART1 and DMA2_STREAM2 should the same interrupt priority!
    #[task(binds = USART1, priority=1, local = [],shared = [serial_transfer_1])]
    fn usart1(ctx: usart1::Context) {
        let serial_transfer_1: &mut Serial1Transfer = ctx.shared.serial_transfer_1;

        let (tx, rx): ( &mut Tx1Transfer, &mut Rx1Transfer) = serial_transfer_1.split();
        match rx.on_rx_transfer_interrupt(|data| {
            hprintln!("rx got");
            tx.start_transfer(|buffer| {
                hprintln!("writng answer...");
                buffer.add("bytes_: ".as_bytes())?;
                buffer.add(data)?;
                hprintln!("answer wroten!");
                Ok(())
            })
        }) {
            Ok(_) => { hprintln!("rx interrupt handled!"); }
            Err(_) => { hprintln!("Wrong UART1 on idle interrupt: no buffer!"); }
        };

    }

    #[task(binds = USART2, priority=1, local = [], shared = [serial_transfer_2])]
    fn usart2(mut ctx: usart2::Context) {
        ctx.shared.serial_transfer_2.lock(|serial_transfer: &mut Serial2Transfer| {
            let (tx, rx): ( &mut Tx2Transfer, &mut Rx2Transfer) = serial_transfer.split();
            match rx.on_rx_transfer_interrupt(|data| {
                hprintln!("rx got");
                tx.start_transfer(|buffer| {
                    hprintln!("writng answer...");
                    buffer.add("bytes_: ".as_bytes()).unwrap();
                    buffer.add(data).unwrap();
                    hprintln!("answer wroten!");
                    Ok(())
                })
            }) {
                Ok(_) => { hprintln!("rx interrupt handled!"); }
                Err(_) => { hprintln!("Wrong UART1 on idle interrupt: no buffer!"); }
            };
        });
    }

    #[task(binds = USART6, priority=1, local = [],shared = [controller_link_slave6, rtc])]
    fn usart6(ctx: usart6::Context) {
        let usart6::SharedResources { mut controller_link_slave6, mut rtc } = ctx.shared;
        controller_link_slave6.lock(|controller_link_slave: &mut ControllerLinkSlave6| {
            match controller_link_slave.on_get_command(|| {
                rtc.lock(|rtc: &mut RtcWrapper| { rtc.get_relative_timestamp() })
            }) {
                Ok(_) => { hprintln!("rx interrupt handled!"); }
                Err(_) => { hprintln!("Wrong UART1 on idle interrupt: no buffer!"); }
            };
        });
    }

    #[task(binds = DMA2_STREAM2, priority=1, shared = [serial_transfer_1])]
    fn dma2_stream2(ctx: dma2_stream2::Context) {
        let serial_transfer: &mut Serial1Transfer = ctx.shared.serial_transfer_1;
        serial_transfer.rx().on_dma_interrupts();
    }

    #[task(binds = DMA2_STREAM7, priority=1, shared = [serial_transfer_1])]
    fn dma2_stream7(ctx: dma2_stream7::Context) {
        let serial_transfer: &mut Serial1Transfer = ctx.shared.serial_transfer_1;
        serial_transfer.tx().on_dma_interrupts();
    }

    #[task(binds = DMA2_STREAM1, priority=1, shared = [controller_link_slave6])]
    fn dma2_stream1(mut ctx: dma2_stream1::Context) {
        ctx.shared.controller_link_slave6.lock(|controller_link: &mut ControllerLinkSlave6| {
            controller_link.on_rx_dma_interrupts();
        });
    }

    #[task(binds = DMA2_STREAM6, priority=1,shared = [controller_link_slave6])]
    fn dma2_stream6(mut ctx: dma2_stream6::Context) {
        ctx.shared.controller_link_slave6.lock(|controller_link: &mut ControllerLinkSlave6| {
            controller_link.on_tx_dma_interrupts();
        });
    }

    #[task(binds = DMA1_STREAM5, priority=1, shared = [serial_transfer_2])]
    fn dma1_stream5(mut ctx: dma1_stream5::Context) {
        ctx.shared.serial_transfer_2.lock(|serial_transfer| {
            serial_transfer.rx().on_dma_interrupts();
        });
    }

    #[task(binds = DMA1_STREAM6, priority=1, shared = [serial_transfer_2])]
    fn dma1_stream6(mut ctx: dma1_stream6::Context) {
        ctx.shared.serial_transfer_2.lock(|serial_transfer| {
            serial_transfer.tx().on_dma_interrupts();
        });
    }


    #[task(priority=1, shared = [adc_transfer])]
    fn polling(mut ctx: polling::Context) {
        ctx.shared.adc_transfer.lock(|transfer: &mut ADCTransfer| {
            transfer.start_measurement();
        });

        polling::spawn_after(1.secs()).ok();
    }

    #[task(binds = DMA2_STREAM0, priority=1, shared = [adc_transfer], local = [])]
    fn dma(mut ctx: dma::Context) {
        let data = ctx.shared.adc_transfer.lock(|adc_transfer: &mut ADCTransfer| {
            adc_transfer.get_results()
        });

        match data {
            Some(_data) => {
                let buff = _data.1;
                let (temperature, voltage) = ADCTransfer::get_last_data(_data.0, buff);
                ctx.shared.adc_transfer.lock(move |adc_transfer: &mut ADCTransfer| {
                    adc_transfer.return_buffer(buff);
                });
                hprintln!("temperature: {}, voltage: {}", temperature, voltage);
            }
            None => {}
        }

    }

}

