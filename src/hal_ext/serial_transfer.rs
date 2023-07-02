#![deny(unsafe_code)]
#![deny(warnings)]

use stm32f4xx_hal::dma::{ChannelX, MemoryToPeripheral, PeripheralToMemory, Transfer};
use stm32f4xx_hal::serial::{Rx, Tx, Serial, Instance, RxISR, TxISR, RxListen};
use stm32f4xx_hal::dma::config::DmaConfig;
use stm32f4xx_hal::dma::traits::{Channel, DMASet, PeriAddress, Stream};
use crate::errors::Errors;

const BUFFER_SIZE: usize = 256;
pub type TxBuffer = Buffer<BUFFER_SIZE>;

pub struct SerialTransfer<U, TxStream, const TX_CHANNEL: u8, RxStream, const RX_CHANNEL: u8>
    where
        U: Instance,
        Tx<U>: PeriAddress<MemSize=u8> + DMASet<TxStream, TX_CHANNEL, MemoryToPeripheral> + TxISR,
        Rx<U, u8>: PeriAddress<MemSize=u8> + DMASet<RxStream, RX_CHANNEL, PeripheralToMemory> + RxISR + RxListen,
        TxStream: Stream,
        ChannelX<TX_CHANNEL>: Channel,
        RxStream: Stream,
        ChannelX<RX_CHANNEL>: Channel,
{
    tx: TxTransfer<U, TxStream, TX_CHANNEL>,
    rx: RxTransfer<U, RxStream, RX_CHANNEL>,
}

impl <U, TxStream, const TX_CHANNEL: u8, RxStream, const RX_CHANNEL: u8> SerialTransfer<U, TxStream, TX_CHANNEL, RxStream, RX_CHANNEL>
    where
        U: Instance,
        Tx<U>: PeriAddress<MemSize=u8> + DMASet<TxStream, TX_CHANNEL, MemoryToPeripheral> + TxISR,
        Rx<U, u8>: PeriAddress<MemSize=u8> + DMASet<RxStream, RX_CHANNEL, PeripheralToMemory> + RxISR + RxListen,
        TxStream: Stream,
        ChannelX<TX_CHANNEL>: Channel,
        RxStream: Stream,
        ChannelX<RX_CHANNEL>: Channel,
{

    pub fn new(
        serial: Serial<U>,
        tx_dma_stream: TxStream,
        rx_dma_stream: RxStream,
    ) -> Self {
        let (tx, rx) = serial.split();

        Self {
            tx: TxTransfer::new(tx, tx_dma_stream),
            rx: RxTransfer::new(rx, rx_dma_stream),
        }
    }

    pub fn rx(&mut self) -> &mut RxTransfer<U, RxStream, RX_CHANNEL> {
        &mut self.rx
    }

    pub fn tx(&mut self) -> &mut TxTransfer<U, TxStream, TX_CHANNEL> {
        &mut self.tx
    }

    pub fn split(&mut self) -> (&mut TxTransfer<U, TxStream, TX_CHANNEL>, &mut RxTransfer<U, RxStream, RX_CHANNEL>) {
        (&mut self.tx, &mut self.rx)
    }

    pub fn into(self) -> (TxTransfer<U, TxStream, TX_CHANNEL>, RxTransfer<U, RxStream, RX_CHANNEL>) {
        (self.tx, self.rx)
    }
}

pub type RxBuffer = &'static mut [u8; BUFFER_SIZE];

pub struct RxTransfer<U, STREAM, const CHANNEL: u8>
    where
        U: Instance,
        Rx<U, u8>: PeriAddress<MemSize=u8> + DMASet<STREAM, CHANNEL, PeripheralToMemory> + RxISR + RxListen,
        STREAM: Stream,
        ChannelX<CHANNEL>: Channel,
{
    rx_transfer: Transfer<STREAM, CHANNEL, Rx<U>, PeripheralToMemory, RxBuffer>,
    back_buffer: Option<RxBuffer>,
    fifo_error: bool,
    buffer_overflow: bool,
}

impl<U, STREAM, const CHANNEL: u8> RxTransfer<U, STREAM, CHANNEL>
    where
        U: Instance,
        Rx<U, u8>: PeriAddress<MemSize=u8> + DMASet<STREAM, CHANNEL, PeripheralToMemory> + RxISR + RxListen,
        STREAM: Stream,
        ChannelX<CHANNEL>: Channel,
{
    pub fn new(
        mut rx: Rx<U>,
        dma_stream: STREAM,
    ) -> Self {

        rx.listen_idle();

        let rx_buffer1 = cortex_m::singleton!(: [u8; BUFFER_SIZE] = [0; BUFFER_SIZE]).unwrap();
        let rx_buffer2 = cortex_m::singleton!(: [u8; BUFFER_SIZE] = [0; BUFFER_SIZE]).unwrap();

        let mut rx_transfer: Transfer<STREAM, CHANNEL, Rx<U, u8>, PeripheralToMemory, RxBuffer> = Transfer::init_peripheral_to_memory(
            dma_stream,
            rx,
            rx_buffer1,
            None,
            DmaConfig::default()
                .memory_increment(true)
                .fifo_enable(true)
                .fifo_error_interrupt(true)
                .transfer_complete_interrupt(true),
        );

        rx_transfer.start(|_stream| {});

        Self {
            rx_transfer,
            back_buffer: Some(rx_buffer2),
            fifo_error: false,
            buffer_overflow: false,
        }
    }

    pub fn get_transferred_buffer(&mut self) -> Result<(RxBuffer, usize), Errors> {
        if self.rx_transfer.is_idle() {
            self.rx_transfer.clear_idle_interrupt();
            let bytes_count = BUFFER_SIZE - STREAM::get_number_of_transfers() as usize;
            let new_buffer = self.back_buffer.take().unwrap();
            let (buffer, _) = self.rx_transfer.next_transfer(new_buffer).unwrap();
            return Ok((buffer, bytes_count));
        }
        Err(Errors::TransferInProgress)
    }

    pub fn return_buffer(&mut self, buffer: RxBuffer) {
        self.back_buffer = Some(buffer);
        self.fifo_error = false;
        self.buffer_overflow = false;
    }

    pub fn on_rx_transfer_interrupt<F: FnOnce(&[u8])->()>(&mut self, receiver: F) -> Result<(), Errors> {
        if self.rx_transfer.is_idle() {
            self.rx_transfer.clear_idle_interrupt();
            let bytes_count = BUFFER_SIZE - STREAM::get_number_of_transfers() as usize;
            let new_buffer = self.back_buffer.take().unwrap();
            let (buffer, _) = self.rx_transfer.next_transfer(new_buffer).unwrap();
            receiver(&buffer[..bytes_count]);
            self.return_buffer(buffer);
            return Ok(());
        }
        Err(Errors::TransferInProgress)
    }

    pub fn on_dma_interrupts(&mut self) {
        self.rx_transfer.clear_interrupts();
        if STREAM::get_fifo_error_flag() {
            self.fifo_error = true;
        }
        if STREAM::get_transfer_complete_flag() {
            self.buffer_overflow = true;
        }
    }
    pub fn fifo_error(&self) -> bool {
        self.fifo_error
    }
    pub fn buffer_overflow(&self) -> bool {
        self.buffer_overflow
    }
}

use crate::hal_ext::dma_read_buffer::Buffer;

pub struct TxTransfer<U, STREAM, const CHANNEL: u8>
    where
        U: Instance,
        Tx<U, u8>: PeriAddress<MemSize=u8> + DMASet<STREAM, CHANNEL, MemoryToPeripheral> + TxISR,
        STREAM: Stream,
        ChannelX<CHANNEL>: Channel,
{
    tx_transfer: Transfer<STREAM, CHANNEL, Tx<U>, MemoryToPeripheral, TxBuffer>,
    back_buffer: Option<TxBuffer>,
    fifo_error: bool,
    last_transfer_ended: bool,
}

impl<U, STREAM, const CHANNEL: u8> TxTransfer<U, STREAM, CHANNEL>
    where
        U: Instance,
        Tx<U, u8>: PeriAddress<MemSize=u8> + DMASet<STREAM, CHANNEL, MemoryToPeripheral> + TxISR,
        STREAM: Stream,
        ChannelX<CHANNEL>: Channel,
{
    pub fn new(
        tx: Tx<U, u8>,
        dma_stream: STREAM,
    ) -> Self {

        let tx_buffer2 = Buffer::new(cortex_m::singleton!(: [u8; BUFFER_SIZE] = [0; BUFFER_SIZE]).unwrap());

        let tx_buffer1 = Buffer::new(cortex_m::singleton!(: [u8; BUFFER_SIZE] = [0; BUFFER_SIZE]).unwrap());

        let tx_transfer: Transfer<STREAM, CHANNEL, Tx<U, u8>, MemoryToPeripheral, TxBuffer> = Transfer::init_memory_to_peripheral(
            dma_stream,
            tx,
            tx_buffer1,
            None,
            DmaConfig::default()
                .memory_increment(true)
                .fifo_enable(true)
                .fifo_error_interrupt(true)
                .transfer_complete_interrupt(true),
        );

        Self {
            tx_transfer,
            back_buffer: Some(tx_buffer2),
            fifo_error: false,
            last_transfer_ended: true,
        }
    }

    /**
    Takes writter function to generate send data and sens them to UART though DMA. Should always return Ok if
    is called from one thread only at the same time.
    */
    pub fn start_transfer<F: FnOnce(&mut TxBuffer)->()>(&mut self, writter: F) -> Result<(), Errors> {
        if !self.last_transfer_ended {
            return Err(Errors::TransferInProgress);
        }
        let mut new_buffer = match self.back_buffer.take() {
            Some(buffer) => Ok(buffer),
            None => Err(Errors::NoBufferAvailable),
        }?;
        new_buffer.clear();
        writter(&mut new_buffer);


        match self.tx_transfer.next_transfer( new_buffer) {
            Ok((buffer, _)) => {
                self.back_buffer = Some(buffer);
                Ok(())
            },
            Err(err) => {
                let (err, buffer) = err.decompose();
                self.back_buffer = Some(buffer);
                Err(Errors::DmaError(err))
            }
        }
    }

    pub fn on_dma_interrupts(&mut self) {
        self.tx_transfer.clear_interrupts();
        if STREAM::get_fifo_error_flag() {
            self.fifo_error = true;
        }
        if STREAM::get_transfer_complete_flag() {
            self.last_transfer_ended = true;
        }
    }
}