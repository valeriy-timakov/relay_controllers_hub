#![no_std]


mod custom_interrupt_class;

use core::fmt::Display;
use logic::services::slave_controller_link::parsers::ResponseData;
use logic::services::slave_controller_link::domain::{DataInstructions, ErrorCode, SignalData, Version};
use logic::errors::Errors;


use cortex_m_semihosting::hprintln;
use drivers::implementations::rtc::RtcWrapper;
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
use stm32f4xx_hal::dma::{MemoryToPeripheral, PeripheralToMemory, Stream1, Stream5, Stream6, Stream7};
use stm32f4xx_hal::gpio::{Output, Pin, PushPull};
use stm32f4xx_hal::pac::{DMA1, USART2, USART6};
use time::{Date, PrimitiveDateTime, Time};
use time::Month;
use drivers::services::adc_transfer::{ ADCTransfer};
use logic::hal_ext::rtc_wrapper::{DateTimeSource};
use logic::services::led::Led;
use logic::services::slave_controller_link::{init_slave_controllers, SlaveControllerLink};
use logic::hal_ext::serial_transfer::{Receiver, RxTransfer, Sender, SerialTransfer, TxTransfer};
use logic::utils::write_to;
use drivers::implementations::serial::{Buffers, RxBuffer, SerialTransferBuilderSTMF401x, Transfer};
use logic::services::slave_controller_link::receiver_from_slave::ErrorHandler;
use logic::services::slave_controller_link::requests_controller::{ResponseHandler, SentRequest};
use logic::services::slave_controller_link::signals_controller::SignalsHandler;
use logic::utils::dma_read_buffer::{Buffer, BufferWriter};
use stm32f4xx_hal::serial::{Rx, Tx};
use stm32f4xx_hal::dma::traits::StreamISR;
use stm32f4xx_hal::pac::{interrupt, Interrupt};
use stm32f4xx_hal::{pac, prelude::*};
use stm32f4xx_hal::otg_fs::{UsbBus, USB, UsbBusType};
use usb_device::class_prelude::*;
use usb_device::prelude::*;
use usbd_serial::SerialPort;
use crate::custom_interrupt_class::CustomInterruptClass;


const BUFFER_SIZE: usize = 256;

type TxBuffer = Buffer<BUFFER_SIZE>;

type Rx1Transfer_ = Transfer<Stream2<DMA2>, 4, Rx<USART1>, PeripheralToMemory, RxBuffer>;
type Tx1Transfer_ = Transfer<Stream7<DMA2>, 4, Tx<USART1>, MemoryToPeripheral, TxBuffer>;
type Serial1Transfer = SerialTransfer<Tx1Transfer_, Rx1Transfer_, TxBuffer, RxBuffer>;
type Rx1Transfer = RxTransfer<Rx1Transfer_, RxBuffer>;
type Tx1Transfer = TxTransfer<Tx1Transfer_, TxBuffer>;
type Rx2Transfer_ = Transfer<Stream5<DMA1>, 4, Rx<USART2>, PeripheralToMemory, RxBuffer>;
type Tx2Transfer_ = Transfer<Stream6<DMA1>, 4, Tx<USART2>, MemoryToPeripheral, TxBuffer>;
type Serial2Transfer = SerialTransfer<Tx2Transfer_, Rx2Transfer_, TxBuffer, RxBuffer>;
type Rx2Transfer = RxTransfer<Rx2Transfer_, RxBuffer>;
type Tx2Transfer = TxTransfer<Tx2Transfer_, TxBuffer>;
type Rx6Transfer_ = Transfer<Stream1<DMA2>, 5, Rx<USART6>, PeripheralToMemory, RxBuffer>;
type Tx6Transfer_ = Transfer<Stream6<DMA2>, 5, Tx<USART6>, MemoryToPeripheral, TxBuffer>;
type Serial6Transfer = SerialTransfer<crate::Tx6Transfer_, crate::Rx6Transfer_, TxBuffer, RxBuffer>;
type Rx6Transfer = RxTransfer<crate::Rx6Transfer_, RxBuffer>;
type Tx6Transfer = TxTransfer<crate::Tx6Transfer_, TxBuffer>;
pub type ControllerLinkSlave1 = SlaveControllerLink<Tx1Transfer_, Rx1Transfer_, TxBuffer, RxBuffer, SignalHandlerImp, ResponseHandlerImp, ErrorHandlerImp>;

pub struct SignalHandlerImp();

impl SignalsHandler for SignalHandlerImp {
    fn on_signal(&mut self, signal_data: SignalData, processed_successfully: bool) {
        todo!()
    }

    fn on_signal_parse_error(&mut self, error: Errors, sent_to_slave_success: bool, data: &[u8]) {
        todo!()
    }

    fn on_signal_process_error(&mut self, error: Errors, sent_to_slave_success: bool, data: SignalData) {
        todo!()
    }
}

pub struct ResponseHandlerImp();

impl ResponseHandler for ResponseHandlerImp {
    fn on_request_success(&mut self, request: SentRequest) {
        todo!()
    }

    fn on_request_response(&mut self, request: SentRequest, response: DataInstructions) {
        todo!()
    }

    fn on_request_error(&mut self, request: SentRequest, error_code: ErrorCode) {
        todo!()
    }

    fn on_request_parse_error(&mut self, request: Option<SentRequest>, error: Errors, data: &[u8]) {
        todo!()
    }

    fn on_request_search_error(&mut self, payload: ResponseData, error: Errors) {
        todo!()
    }
}

pub struct ErrorHandlerImp();

impl ErrorHandler for ErrorHandlerImp {
    fn on_error(&mut self, error: Errors) {
        todo!()
    }
}



pub struct Board {
    pub controller_link_slave1: ControllerLinkSlave1,
    pub in_work: InWork
}



impl Board {

    pub fn init(mut dp: Peripherals, monoHz: u32) -> Board {
        static mut EP_MEMORY: [u32; 1024] = [0; 1024];
        static mut USB_BUS: Option<UsbBusAllocator<stm32f4xx_hal::otg_fs::UsbBusType>> = None;

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
        let mut rtc = DateTimeSource::new( RtcWrapper::new( Rtc::new(dp.RTC, &mut dp.PWR) ) );

        if rtc_not_initialized {
            let date = Date::from_calendar_date(2023, Month::June, 13).unwrap();
            let time = Time::from_hms(8, 26, 30).unwrap();
            let date_time = PrimitiveDateTime::new(date, time);
            rtc.set_datetime(date_time).unwrap();
        }

        let gpioa = dp.GPIOA.split();
        let gpiob = dp.GPIOB.split();
        let gpioc = dp.GPIOC.split();

        let usb = USB::new(
            (dp.OTG_FS_GLOBAL, dp.OTG_FS_DEVICE, dp.OTG_FS_PWRCLK),
            (gpioa.pa11, gpioa.pa12),
            &clocks,
        );

        unsafe {
            USB_BUS.replace(UsbBus::new(usb, &mut EP_MEMORY));
        }
        let usb_bus = unsafe { USB_BUS.as_ref().unwrap() };
        let usb_serial: SerialPort<UsbBusType> = usbd_serial::SerialPort::new(usb_bus);
        let usb_interrupt_device = CustomInterruptClass::new(usb_bus);
        let usb_dev: UsbDevice<UsbBusType> = UsbDeviceBuilder::new(usb_bus, UsbVidPid(0x16c0, 0x27dd))
            .device_class(usbd_serial::USB_CLASS_CDC)
            .strings(&[StringDescriptors::default()
                .manufacturer("Fake Company")
                .product("Product")
                .serial_number("TEST")])
            .unwrap()
            .build();
        

        let dma2 = StreamsTuple::new(dp.DMA2);
        let dma1 = StreamsTuple::new(dp.DMA1);

        let serial1 = dp.USART1.serial(
            (gpioa.pa9.into_alternate(), gpioa.pa10),
            Config::default()
                .baudrate(9600.bps())
                .dma(config::DmaConfig::TxRx),
            &clocks,
        ).unwrap();

        let serial2 = dp.USART2.serial(
            (gpioa.pa2.into_alternate(), gpioa.pa3),
            Config::default()
                .baudrate(9600.bps())
                .dma(config::DmaConfig::TxRx),
            &clocks,
        ).unwrap();

        let buffers1 = Buffers::new(
            cortex_m::singleton!(: [u8; BUFFER_SIZE] = [0; BUFFER_SIZE]).unwrap(),
            cortex_m::singleton!(: [u8; BUFFER_SIZE] = [0; BUFFER_SIZE]).unwrap(),
            cortex_m::singleton!(: [u8; BUFFER_SIZE] = [0; BUFFER_SIZE]).unwrap(),
            cortex_m::singleton!(: [u8; BUFFER_SIZE] = [0; BUFFER_SIZE]).unwrap()
        );

        let buffers2 = Buffers::new(
            cortex_m::singleton!(: [u8; BUFFER_SIZE] = [0; BUFFER_SIZE]).unwrap(),
            cortex_m::singleton!(: [u8; BUFFER_SIZE] = [0; BUFFER_SIZE]).unwrap(),
            cortex_m::singleton!(: [u8; BUFFER_SIZE] = [0; BUFFER_SIZE]).unwrap(),
            cortex_m::singleton!(: [u8; BUFFER_SIZE] = [0; BUFFER_SIZE]).unwrap()
        );

        let serial_transfer_1 = SerialTransferBuilderSTMF401x::create_serial_transfer(serial1, dma2.7, dma2.2, buffers1);
        let serial_transfer_2 = SerialTransferBuilderSTMF401x::create_serial_transfer(serial2, dma1.6, dma1.5, buffers2);
        // let serial_transfer_6 = SerialTransferBuilderSTMF401x::create_serial_transfer(serial6, dma2.6, dma2.1, buffers6);

        let signal_handler = SignalHandlerImp();

        let controller_link_slave1 =
            SlaveControllerLink::create(serial_transfer_1, signal_handler, ResponseHandlerImp(),
                 ErrorHandlerImp(), Version::V1).unwrap();

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

        let last_sent = rtc.get_relative_timestamp().value();

        let in_work = InWork {
            serial_transfer_2,
            // serial_transfer_6,
            led,
            adc_transfer,
            rtc,
            button,
            counter,
            counter2, 
            usb_serial,
            usb_dev,
            usb_interrupt_device,
            measure_data: [0; 3],
            last_sent,
            send_interrupts: false,
        };

        Self {
            controller_link_slave1,
            in_work
        }
    }
}


pub struct InWork {
    serial_transfer_2: Serial2Transfer,
    led: Led<Pin<'C', 13, Output<PushPull>>>,
    adc_transfer: ADCTransfer,
    pub rtc: DateTimeSource<RtcWrapper>,
    button: gpio::PA0<Input>,
    counter: timer::CounterMs<TIM3>,
    counter2: timer::CounterMs<TIM2>,
    usb_serial: SerialPort<'static, UsbBusType>, 
    usb_dev: UsbDevice<'static, UsbBusType>,
    usb_interrupt_device: CustomInterruptClass<'static, UsbBusType>,
    measure_data: [u8; 3], 
    last_sent: u32,
    send_interrupts: bool,
}

impl InWork {

    pub fn on_button_pressed(&mut self) {
        self.button.clear_interrupt_pending_bit();

        self.last_sent = self.rtc.get_relative_timestamp().value();
        match self.usb_interrupt_device.write(&mut self.measure_data) {
            Ok(_) => hprintln!("Measured data sent to USB!"),
            Err(err) => hprintln!("Error measured data to USB! {}", UsbErrorWrapper::from(err)),
        }
        
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
        self.counter2.clear_all_flags();
        self.counter2.now().ticks();
        let time: PrimitiveDateTime = self.rtc.get_datetime();
        let mut buf = [0u8; 64];
        let _s: &str = write_to::show(
            &mut buf,
            format_args!("time: {}.{}.{} {}:{}:{}\r\n", time.day(), time.month(),
                         time.year(), time.hour(), time.minute(),
                         time.second())
        ).unwrap();
        let tx: &mut Tx2Transfer = self.serial_transfer_2.tx();

        match self.usb_interrupt_device.write(_s.as_bytes()) {
            Ok(_) => { hprintln!("usb interrupt sent!"); }
            Err(err) => {
                hprintln!("Error sending interrupu on usb! {}", UsbErrorWrapper::from(err));
            }
        }
        if self.send_interrupts {
            match tx.start_transfer(|buf| {
                buf.add_str(_s)
            }) {
                Ok(_) => { hprintln!("tx interrupt handled!"); }
                Err(err) => { hprintln!("Error sending on tim 2! {}", err); }
            };
        }

    }

    pub fn on_tim3(&mut self) {
        self.counter.clear_all_flags();
        self.led.update().unwrap();
        if self.send_interrupts && self.rtc.get_relative_timestamp().value() - self.last_sent > 5000 {
            self.last_sent = self.rtc.get_relative_timestamp().value();
            match self.usb_serial.write(&mut self.measure_data) {
                Ok(_) => hprintln!("Measured data sent to USB!"),
                Err(err) => hprintln!("Error measured data to USB! {}", UsbErrorWrapper::from(err)),
            }
        }
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
            Err(err) => { hprintln!("Wrong UART2 on idle interrupt: {}!", err); }
        };
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

    pub fn on_dma2_stream0(&mut self) {
        let data  = self.adc_transfer.get_results();
        let (temperature, voltage) = ADCTransfer::get_last_data(data.0, data.1);
        self.adc_transfer.return_buffer(data.1);
        self.measure_data[0] = temperature as u8;
        self.measure_data[1] = (voltage & 0xFF) as u8;
        self.measure_data[2] = (voltage >> 8) as u8;
        hprintln!("temperature: {}, voltage: {}", temperature, voltage);
    }
    
    pub fn on_usb_otg_fs(&mut self) {
        let serial: &mut SerialPort<UsbBusType> = &mut self.usb_serial;
            if self.usb_dev.poll(&mut [serial]) {
                let mut buf = [0u8; 64];

                hprintln!("reading USB");
                match serial.read(&mut buf) {
                    Ok(count) if count > 0 => {
                        hprintln!("read succes: {} bytes", count);
                        for i in 0..count {
                            buf[i] = buf[i] * 2 + 7;
                        }
                        self.send_interrupts = true;
                        let mut write_offset = 0;
                        while write_offset < count {
                            match serial.write(&mut buf[write_offset..count]) {
                                Ok(len) if len > 0 => {
                                    write_offset += len;
                                }
                                _ => {}
                            }
                        }
                    }, 
                    Ok(i) => {
                        hprintln!("read succes: {} bytes", i);
                    }
                    Err(e) => {
                        hprintln!("read error: {:?}", e);
                    }
                }
            }
    }
}


struct UsbErrorWrapper {
    error: UsbError
}

impl From<UsbError> for UsbErrorWrapper {
    fn from(error: UsbError) -> Self {
        UsbErrorWrapper { error }
    }
}

impl Display for UsbErrorWrapper {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.error  { 
            UsbError::BufferOverflow => write!(f, "BufferOverflow"),
            UsbError::EndpointMemoryOverflow => write!(f, "EndpointMemoryOverflow"),
            UsbError::InvalidEndpoint => write!(f, "InvalidEndpoint"),
            UsbError::InvalidState => write!(f, "InvalidState"),
            UsbError::WouldBlock => write!(f, "WouldBlock"),
            UsbError::ParseError => write!(f, "ParseError"),
            UsbError::EndpointOverflow => write!(f, "EndpointOverflow"),
            UsbError::Unsupported => write!(f, "Unsupported"),
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