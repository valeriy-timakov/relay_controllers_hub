#![no_std]


use logic::services::slave_controller_link::{SentRequest, SignalData, SignalsHandler};
use logic::services::slave_controller_link::domain::{DataInstructions, ErrorCode, Signals};
use logic::errors::Errors;


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
use stm32f4xx_hal::dma::{DMAError, Stream1, Stream5, Stream6, Stream7};
use stm32f4xx_hal::gpio::{Output, Pin, PushPull};
use stm32f4xx_hal::pac::{DMA1, USART2, USART6};
use time::{Date, PrimitiveDateTime, Time};
use time::Month;
use logic::services::adc_transfer::{ ADCTransfer};
use logic::hal_ext::rtc_wrapper::{ RtcWrapper};
use logic::services::led::Led;
use logic::services::slave_controller_link::{init_slave_controllers, SlaveControllerLink};
use logic::hal_ext::serial_transfer::{RxTransfer, RxTransferProxy, SerialTransfer, TxTransfer, TxTransferProxy};
use logic::utils::write_to;


type Serial1Transfer = SerialTransfer<USART1, Stream7<DMA2>, 4, Stream2<DMA2>, 4>;
type Rx1Transfer = RxTransfer<USART1, Stream2<DMA2>, 4>;
type Tx1Transfer = TxTransfer<USART1, Stream7<DMA2>, 4>;
type Serial2Transfer = SerialTransfer<USART2, Stream6<DMA1>, 4, Stream5<DMA1>, 4>;
type Rx2Transfer = RxTransfer<USART2, Stream5<DMA1>, 4>;
type Tx2Transfer = TxTransfer<USART2, Stream6<DMA1>, 4>;
pub type ControllerLinkSlave6 = SlaveControllerLink<USART6, Stream6<DMA2>, 5, Stream1<DMA2>, 5, SignalReceiverImp>;



pub struct SignalReceiverImp();

impl SignalsHandler for SignalReceiverImp {
    fn on_signal(&mut self, _: SignalData) {
        todo!()
    }

    fn on_signal_error(&mut self, _: Signals, _: ErrorCode) {
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


pub struct Board {
    pub controller_link_slave6: ControllerLinkSlave6,
    pub in_work: InWork
}

impl Board {

    pub fn init(mut dp: Peripherals, monoHz: u32) -> Board {

        init_slave_controllers();

        let rcc = dp.RCC.constrain();
        let clocks = rcc.cfgr
            .use_hse(25.MHz())
            .require_pll48clk()
            .sysclk(monoHz.Hz())
            .hclk(monoHz.Hz())
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

        let in_work = InWork {
            serial_transfer_1,
            serial_transfer_2,
            led,
            adc_transfer,
            rtc,
            button,
            counter,
            counter2
        };

        Self {
            controller_link_slave6,
            in_work
        }
    }



}

pub struct InWork {
    serial_transfer_1: Serial1Transfer,
    serial_transfer_2: Serial2Transfer,
    led: Led<Pin<'C', 13, Output<PushPull>>>,
    adc_transfer: ADCTransfer,
    pub rtc: RtcWrapper<Rtc>,
    button: gpio::PA0<Input>,
    counter: timer::CounterMs<TIM3>,
    counter2: timer::CounterMs<TIM2>,
}

impl InWork {

    pub fn on_button_pressed(&mut self) {
        self.button.clear_interrupt_pending_bit();
        self.led.update_periods(|prev_on_cylcles_count: u16, prev_off_cycles_count|{
            let mut off_cylcles_count = prev_off_cycles_count - 1;
            if off_cylcles_count == 0 {
                off_cylcles_count = 8;
            }
            hprintln!("off_cylcles_count: {}", off_cylcles_count);
            (prev_on_cylcles_count, off_cylcles_count)
        });
    }


    pub fn on_tim2(&mut self) {
        self.counter2.clear_interrupt(timer::Event::Update);
        self.counter2.now().ticks();
        let time: PrimitiveDateTime = self.rtc.get_datetime();
        let mut buf = [0u8; 64];
        let _s: &str = write_to::show(
            &mut buf,
            format_args!("time: {}.{}.{} {}:{}:{}\r\n", time.day(), time.month(),
                         time.year(), time.hour(), time.minute(),
                         time.second())
        ).unwrap();
        let tx: &mut Tx1Transfer = self.serial_transfer_1.tx();
        tx.start_transfer(|buf| {
            buf.add_str(_s)
        }).unwrap();

    }

    pub fn on_tim3(&mut self) {
        self.counter.clear_interrupt(timer::Event::Update);
        self.led.update();
    }

    pub fn on_usart1(&mut self) {
        let (tx, rx): ( &mut Tx1Transfer, &mut Rx1Transfer) = self.serial_transfer_1.split();
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


    pub fn on_usart2(&mut self) {
        let (tx, rx): ( &mut Tx2Transfer, &mut Rx2Transfer) = self.serial_transfer_2.split();
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
    }




    pub fn on_dma2_stream2(&mut self) {
        self.serial_transfer_1.rx().on_dma_interrupts();
    }


    pub fn on_dma2_stream7(&mut self) {
        self.serial_transfer_1.tx().on_dma_interrupts();
    }


    pub fn on_dma1_stream5(&mut self) {
        self.serial_transfer_2.rx().on_dma_interrupts();
    }


    pub fn on_dma1_stream6(&mut self) {
        self.serial_transfer_2.tx().on_dma_interrupts();
    }


    pub fn on_polling(&mut self) {
        self.adc_transfer.start_measurement();

    }

    pub fn on_dma(&mut self) {
        match self.adc_transfer.get_results() {
            Some(_data) => {
                let buff = _data.1;
                let (temperature, voltage) = ADCTransfer::get_last_data(_data.0, buff);
                self.adc_transfer.return_buffer(buff);
                hprintln!("temperature: {}, voltage: {}", temperature, voltage);
            }
            None => {}
        }
    }
}




























//
//
// use stm32f4xx_hal::dma::{ChannelX, MemoryToPeripheral, PeripheralToMemory, Transfer};
// use stm32f4xx_hal::serial::{Rx, Tx, Serial, Instance, RxISR, TxISR, RxListen};
// use stm32f4xx_hal::dma::config::DmaConfig;
// use stm32f4xx_hal::dma::traits::{Channel, DMASet, PeriAddress, Stream};
// use logic::utils::dma_read_buffer::Buffer;
//
// const BUFFER_SIZE: usize = 256;
// pub type TxBuffer = Buffer<BUFFER_SIZE>;
// pub type RxBuffer = &'static mut [u8; BUFFER_SIZE];
//
// pub struct SerialTransfer<U, TxStream, const TX_CHANNEL: u8, RxStream, const RX_CHANNEL: u8>
//     where
//         U: Instance,
//         Tx<U>: PeriAddress<MemSize=u8> + DMASet<TxStream, TX_CHANNEL, MemoryToPeripheral> + TxISR,
//         Rx<U, u8>: PeriAddress<MemSize=u8> + DMASet<RxStream, RX_CHANNEL, PeripheralToMemory> + RxISR + RxListen,
//         TxStream: Stream,
//         ChannelX<TX_CHANNEL>: Channel,
//         RxStream: Stream,
//         ChannelX<RX_CHANNEL>: Channel,
// {
//     tx: TxTransfer<U, TxStream, TX_CHANNEL>,
//     rx: RxTransfer<U, RxStream, RX_CHANNEL>,
// }
//
// impl <U, TxStream, const TX_CHANNEL: u8, RxStream, const RX_CHANNEL: u8> SerialTransfer<U, TxStream, TX_CHANNEL, RxStream, RX_CHANNEL>
//     where
//         U: Instance,
//         Tx<U>: PeriAddress<MemSize=u8> + DMASet<TxStream, TX_CHANNEL, MemoryToPeripheral> + TxISR,
//         Rx<U, u8>: PeriAddress<MemSize=u8> + DMASet<RxStream, RX_CHANNEL, PeripheralToMemory> + RxISR + RxListen,
//         TxStream: Stream,
//         ChannelX<TX_CHANNEL>: Channel,
//         RxStream: Stream,
//         ChannelX<RX_CHANNEL>: Channel,
// {
//
//     pub fn new(
//         serial: Serial<U>,
//         tx_dma_stream: TxStream,
//         rx_dma_stream: RxStream,
//     ) -> Self {
//         let (tx, rx) = serial.split();
//
//         Self {
//             tx: TxTransfer::new(tx, tx_dma_stream),
//             rx: RxTransfer::new(rx, rx_dma_stream),
//         }
//     }
//
//     pub fn rx(&mut self) -> &mut RxTransfer<U, RxStream, RX_CHANNEL> {
//         &mut self.rx
//     }
//
//     pub fn tx(&mut self) -> &mut TxTransfer<U, TxStream, TX_CHANNEL> {
//         &mut self.tx
//     }
//
//     pub fn split(&mut self) -> (&mut TxTransfer<U, TxStream, TX_CHANNEL>, &mut RxTransfer<U, RxStream, RX_CHANNEL>) {
//         (&mut self.tx, &mut self.rx)
//     }
//
//     pub fn into(self) -> (TxTransfer<U, TxStream, TX_CHANNEL>, RxTransfer<U, RxStream, RX_CHANNEL>) {
//         (self.tx, self.rx)
//     }
// }
//
//
// pub trait RxDmaISR: RxISR {
//     fn get_fifo_error_flag() -> bool;
//     fn get_transfer_complete_flag() -> bool;
//     fn clear_dma_interrupts(&mut self);
// }
//
// pub struct RxTransferWrapper<U, STREAM, const CHANNEL: u8>
//     where
//         U: Instance,
//         Rx<U, u8>: PeriAddress<MemSize=u8> + DMASet<STREAM, CHANNEL, PeripheralToMemory> + RxISR + RxListen,
//         STREAM: Stream,
//         ChannelX<CHANNEL>: Channel,
// {
//     rx_transfer: Transfer<STREAM, CHANNEL, Rx<U>, PeripheralToMemory, RxBuffer>,
// }
//
// impl<U, STREAM, const CHANNEL: u8> RxTransferWrapper<U, STREAM, CHANNEL>
//     where
//         U: Instance,
//         Rx<U>: PeriAddress<MemSize=u8> + DMASet<STREAM, CHANNEL, PeripheralToMemory> + RxISR + RxListen,
//         STREAM: Stream,
//         ChannelX<CHANNEL>: Channel,
// {
//     pub fn new(
//         mut rx: Rx<U>,
//         dma_stream: STREAM,
//         rx_buffer1: RxBuffer,
//     ) -> Self {
//
//         rx.listen_idle();
//
//         let mut rx_transfer: Transfer<STREAM, CHANNEL, Rx<U, u8>, PeripheralToMemory, RxBuffer> = Transfer::init_peripheral_to_memory(
//             dma_stream,
//             rx,
//             rx_buffer1,
//             None,
//             DmaConfig::default()
//                 .memory_increment(true)
//                 .fifo_enable(true)
//                 .fifo_error_interrupt(true)
//                 .transfer_complete_interrupt(true),
//         );
//
//         rx_transfer.start(|_stream| {});
//
//         Self {
//             rx_transfer,
//         }
//     }
//
// }
//
// impl<U, STREAM, const CHANNEL: u8> RxTransferProxy<RxBuffer, u8, DMAError<RxBuffer>, DMAError<()>> for RxTransferWrapper<U, STREAM, CHANNEL>
//     where
//         U: Instance,
//         Rx<U, u8>: PeriAddress<MemSize=u8> + DMASet<STREAM, CHANNEL, PeripheralToMemory> + RxISR,
//         STREAM: Stream,
//         ChannelX<CHANNEL>: Channel,
// {
//
//     fn get_fifo_error_flag(&self) -> bool {
//         STREAM::get_fifo_error_flag()
//     }
//     fn get_transfer_complete_flag(&self) -> bool {
//         STREAM::get_transfer_complete_flag()
//     }
//     fn clear_dma_interrupts(&mut self) {
//         self.rx_transfer.clear_interrupts();
//     }
//     fn get_number_of_transfers() -> u16 {
//         STREAM::get_number_of_transfers()
//     }
//     fn next_transfer(&mut self, new_buf: RxBuffer) -> Result<RxBuffer, DMAError<RxBuffer>> {
//         self.rx_transfer.next_transfer(new_buf)
//             .map(|(buffer, _)| { buffer } )
//     }
// }
//
//
// pub struct TxTransferWrapper<U, STREAM, const CHANNEL: u8>
//     where
//         U: Instance,
//         Tx<U, u8>: PeriAddress<MemSize=u8> + DMASet<STREAM, CHANNEL, MemoryToPeripheral> + TxISR,
//         STREAM: Stream,
//         ChannelX<CHANNEL>: Channel,
// {
//     tx_transfer: Transfer<STREAM, CHANNEL, Tx<U>, MemoryToPeripheral, TxBuffer>
// }
//
// impl<U, STREAM, const CHANNEL: u8> TxTransferWrapper<U, STREAM, CHANNEL>
//     where
//         U: Instance,
//         Tx<U, u8>: PeriAddress<MemSize=u8> + DMASet<STREAM, CHANNEL, MemoryToPeripheral> + TxISR,
//         STREAM: Stream,
//         ChannelX<CHANNEL>: Channel,
// {
//     pub fn new(
//         tx: Tx<U, u8>,
//         dma_stream: STREAM,
//         tx_buffer1: TxBuffer,
//     ) -> Self {
//
//         let tx_transfer: Transfer<STREAM, CHANNEL, Tx<U, u8>, MemoryToPeripheral, TxBuffer> = Transfer::init_memory_to_peripheral(
//             dma_stream,
//             tx,
//             tx_buffer1,
//             None,
//             DmaConfig::default()
//                 .memory_increment(true)
//                 .fifo_enable(true)
//                 .fifo_error_interrupt(true)
//                 .transfer_complete_interrupt(true),
//         );
//
//         Self {
//             tx_transfer
//         }
//     }
//
//
// }
//
// impl<U, STREAM, const CHANNEL: u8> TxTransferProxy<TxBuffer, u8, DMAError<TxBuffer>, DMAError<()>> for TxTransferWrapper<U, STREAM, CHANNEL>
//     where
//         U: Instance,
//         Tx<U, u8>: PeriAddress<MemSize=u8> + DMASet<STREAM, CHANNEL, MemoryToPeripheral> + TxISR,
//         STREAM: Stream,
//         ChannelX<CHANNEL>: Channel,
// {
//     fn get_fifo_error_flag(&self) -> bool {
//         STREAM::get_fifo_error_flag()
//     }
//
//     fn get_transfer_complete_flag(&self) -> bool {
//         STREAM::get_transfer_complete_flag()
//     }
//
//     fn clear_dma_interrupts(&mut self) {
//         self.tx_transfer.clear_interrupts();
//     }
//
//     fn next_transfer(&mut self, buffer: TxBuffer) -> Result<TxBuffer, DMAError<TxBuffer>> {
//         self.tx_transfer.next_transfer(buffer)
//             .map(|(buffer, _)| { buffer } )
//     }
//
// }