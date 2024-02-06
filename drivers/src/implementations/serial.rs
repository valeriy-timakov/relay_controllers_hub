#![allow(unsafe_code)]

use stm32f4xx_hal::dma::{ChannelX, DMAError, MemoryToPeripheral, PeripheralToMemory};
use stm32f4xx_hal::dma::traits::{Channel, DMASet, PeriAddress, Stream, StreamISR};
use stm32f4xx_hal::serial::{Instance, Rx, RxISR, RxListen, Serial, Tx, TxISR};
use stm32f4xx_hal::dma::config::DmaConfig;
use logic::hal_ext::serial_transfer::{ReadableBuffer, RxTransferProxy, SerialTransfer, TransferProxy, TxTransferProxy};
use logic::utils::dma_read_buffer::Buffer;
use core::marker::PhantomData;
use core::ops::{Deref, DerefMut};
use embedded_dma::WriteBuffer;
use stm32f4xx_hal::ClearFlags;
use logic::errors;

pub struct RxBuffer (&'static mut [u8]);

impl RxBuffer {
    pub fn len(&self) -> usize {
        self.0.len()
    }
}

impl ReadableBuffer for RxBuffer {
    fn slice_to(&self, to: usize) -> &[u8] {
        &self.0[..to]
    }
}

unsafe  impl WriteBuffer for RxBuffer {
    type Word = u8;

    unsafe fn write_buffer(&mut self) -> (*mut Self::Word, usize) {
        self.0.write_buffer()
    }
}

impl Deref for RxBuffer {
    type Target = &'static mut [u8];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for RxBuffer {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

pub struct Transfer<STREAM: Stream, const CHANNEL: u8, PERIPHERAL: PeriAddress, DIRECTION, BUF> {
    inner: stm32f4xx_hal::dma::Transfer<STREAM, CHANNEL, PERIPHERAL, DIRECTION, BUF>,
    capacity: usize,
    _stream: PhantomData<STREAM>,
    _peripheral: PhantomData<PERIPHERAL>,
    _direction: PhantomData<DIRECTION>,
    _buf: PhantomData<BUF>,
}

impl<STREAM, PERIPHERAL, DIRECTION, BUF, const CHANNEL: u8> TransferProxy<BUF> for
Transfer<STREAM, CHANNEL, PERIPHERAL, DIRECTION, BUF>
    where
        DIRECTION: stm32f4xx_hal::dma::traits::Direction,
        STREAM: Stream,
        ChannelX<CHANNEL>: Channel,
        PERIPHERAL: PeriAddress +  DMASet<STREAM, CHANNEL, DIRECTION>,
{
    #[inline(always)]
    fn is_fifo_error(&self) -> bool {
        self.inner.is_fifo_error()
    }

    #[inline(always)]
    fn is_transfer_complete(&self) -> bool {
        self.inner.is_transfer_complete()
    }
    #[inline(always)]
    fn is_direct_mode_error(&self) -> bool {
        self.inner.is_direct_mode_error()
    }
    #[inline(always)]
    fn is_half_transfer(&self) -> bool {
        self.inner.is_half_transfer()
    }
    #[inline(always)]
    fn is_transfer_error(&self) -> bool {
        self.inner.is_transfer_error()
    }

    #[inline(always)]
    fn clear_dma_interrupts(&mut self) {
        self.inner.clear_all_flags();
    }
    #[inline(always)]
    fn clear_direct_mode_error(&mut self) {
        self.inner.clear_direct_mode_error();
    }
    #[inline(always)]
    fn clear_fifo_error(&mut self) {
        self.inner.clear_fifo_error();
    }
    #[inline(always)]
    fn clear_half_transfer(&mut self) {
        self.inner.clear_half_transfer();
    }
    #[inline(always)]
    fn clear_transfer_complete(&mut self) {
        self.inner.clear_transfer_complete();
    }
    #[inline(always)]
    fn clear_transfer_error(&mut self) {
        self.inner.clear_transfer_error();
    }

}


impl<U, STREAM, const BUFFER_SIZE_2: usize, const CHANNEL: u8> TxTransferProxy<Buffer<BUFFER_SIZE_2>> for
Transfer<STREAM, CHANNEL, Tx<U>, MemoryToPeripheral, Buffer<BUFFER_SIZE_2>>
    where
        U: Instance,
        Tx<U, u8>: PeriAddress<MemSize=u8> + DMASet<STREAM, CHANNEL, MemoryToPeripheral> + TxISR,
        STREAM: Stream,
        ChannelX<CHANNEL>: Channel,
{

    #[inline(always)]
    fn next_transfer(&mut self, buffer: Buffer<BUFFER_SIZE_2>) -> Result<Buffer<BUFFER_SIZE_2>, errors::DMAError<Buffer<BUFFER_SIZE_2>>> {
        self.inner.next_transfer(buffer)
            .map(|(buffer, _)| { buffer } )
            .map_err(convert_dma_error )
    }

}


impl<U, STREAM, const CHANNEL: u8> RxTransferProxy<RxBuffer> for
Transfer<STREAM, CHANNEL, Rx<U>, PeripheralToMemory, RxBuffer>
    where
        U: Instance,
        Rx<U, u8>: PeriAddress<MemSize=u8> + DMASet<STREAM, CHANNEL, PeripheralToMemory> + RxISR,
        STREAM: Stream,
        ChannelX<CHANNEL>: Channel,
{

    #[inline(always)]
    fn next_transfer(&mut self, new_buf: RxBuffer) -> Result<RxBuffer, errors::DMAError<RxBuffer>> {
        self.inner.next_transfer(new_buf)
            .map(|(buffer, _)| { buffer } )
            .map_err(convert_dma_error )
    }

    #[inline(always)]
    fn get_read_bytes_count(&self) -> usize {
        self.capacity - self.inner.number_of_transfers() as usize
    }

    #[inline(always)]
    fn is_idle(&self) -> bool {
        (&self.inner as &dyn RxISR).is_idle()
    }

    #[inline(always)]
    fn is_rx_not_empty(&self) -> bool {
        (&self.inner as &dyn RxISR).is_rx_not_empty()
    }

    #[inline(always)]
    fn clear_idle_interrupt(&mut self) {
        (&self.inner as &dyn RxISR).clear_idle_interrupt()
    }

}

fn convert_dma_error<T>(e: DMAError<T>) -> errors::DMAError<T> {
    match e {
        DMAError::NotReady(t) => errors::DMAError::NotReady(t),
        DMAError::SmallBuffer(t) => errors::DMAError::SmallBuffer(t),
        DMAError::Overrun(t) => errors::DMAError::Overrun(t),
    }
}


pub struct Buffers<const SIZE: usize> {
    tx_buffer_1: &'static mut [u8; SIZE],
    tx_buffer_2: &'static mut [u8; SIZE],
    rx_buffer_1: &'static mut [u8; SIZE],
    rx_buffer_2: &'static mut [u8; SIZE],
}

impl <const SIZE: usize> Buffers<SIZE> {

    pub fn new (
        tx_buffer_1: &'static mut [u8; SIZE],
        tx_buffer_2: &'static mut [u8; SIZE],
        rx_buffer_1: &'static mut [u8; SIZE],
        rx_buffer_2: &'static mut [u8; SIZE]
    ) -> Self {
        Self {
            tx_buffer_1,
            tx_buffer_2,
            rx_buffer_1,
            rx_buffer_2
        }
    }

}

pub struct SerialTransferBuilderSTMF401x<U, TxStream, const TX_CHANNEL: u8, RxStream, const BUFFER_SIZE_2: usize, const RX_CHANNEL: u8>
    where
        U: Instance,
        Tx<U, u8>: PeriAddress<MemSize=u8> + DMASet<TxStream, TX_CHANNEL, MemoryToPeripheral> + TxISR,
        TxStream: Stream,
        ChannelX<TX_CHANNEL>: Channel,
        Rx<U>: PeriAddress<MemSize=u8> + DMASet<RxStream, RX_CHANNEL, PeripheralToMemory> + RxISR + RxListen,
        RxStream: Stream,
        ChannelX<RX_CHANNEL>: Channel
{
    _peripheral: PhantomData<U>,
    _tx_stream: PhantomData<TxStream>,
    _rx_stream: PhantomData<RxStream>,
    _tx_buff: PhantomData<Buffer<BUFFER_SIZE_2>>
}

impl<U, TxStream, const BUFFER_SIZE_2: usize, const TX_CHANNEL: u8, RxStream, const RX_CHANNEL: u8>
        SerialTransferBuilderSTMF401x<U, TxStream, TX_CHANNEL, RxStream, BUFFER_SIZE_2, RX_CHANNEL>
    where
        U: Instance,
        Tx<U, u8>: PeriAddress<MemSize=u8> + DMASet<TxStream, TX_CHANNEL, MemoryToPeripheral> + TxISR,
        TxStream: Stream,
        ChannelX<TX_CHANNEL>: Channel,
        Rx<U>: PeriAddress<MemSize=u8> + DMASet<RxStream, RX_CHANNEL, PeripheralToMemory> + RxISR + RxListen,
        RxStream: Stream,
        ChannelX<RX_CHANNEL>: Channel,
{

    pub fn create_serial_transfer(
        serial: Serial<U>,
        dma_tx_stream: TxStream,
        dma_rx_stream: RxStream,
        buffers: Buffers<BUFFER_SIZE_2>
    ) -> SerialTransfer<
        Transfer<TxStream, TX_CHANNEL, Tx<U, u8>, MemoryToPeripheral, Buffer<BUFFER_SIZE_2>>,
        Transfer<RxStream, RX_CHANNEL, Rx<U, u8>, PeripheralToMemory, RxBuffer>,
        Buffer<BUFFER_SIZE_2>, RxBuffer
    > {

        let (tx, rx) = serial.split();

        SerialTransfer::new(
            Self::create_tx(tx, dma_tx_stream, Buffer::new(buffers.tx_buffer_1)), Buffer::new(buffers.tx_buffer_2),
            Self::create_rx(rx, dma_rx_stream, RxBuffer(buffers.rx_buffer_1)), RxBuffer(buffers.rx_buffer_2)
        )
    }

    fn create_tx(
        tx: Tx<U, u8>,
        dma_stream: TxStream,
        tx_buffer: Buffer<BUFFER_SIZE_2>,
    ) -> Transfer<TxStream, TX_CHANNEL, Tx<U, u8>, MemoryToPeripheral, Buffer<BUFFER_SIZE_2>> {

        let tx_transfer: stm32f4xx_hal::dma::Transfer<TxStream, TX_CHANNEL, Tx<U, u8>, MemoryToPeripheral, Buffer<BUFFER_SIZE_2>> =
            stm32f4xx_hal::dma::Transfer::init_memory_to_peripheral(
                dma_stream,
                tx,
                tx_buffer,
                None,
                DmaConfig::default()
                    .memory_increment(true)
                    .fifo_enable(true)
                    .fifo_error_interrupt(true)
                    .transfer_complete_interrupt(true),
            );

        Transfer {
            inner: tx_transfer,
            capacity: BUFFER_SIZE_2,
            _stream: PhantomData,
            _peripheral: PhantomData,
            _direction: PhantomData,
            _buf: PhantomData
        }
    }


    fn create_rx(
        mut rx: Rx<U>,
        dma_stream: RxStream,
        rx_buffer1: RxBuffer,
    ) -> Transfer<RxStream, RX_CHANNEL, Rx<U, u8>, PeripheralToMemory, RxBuffer> {

        rx.listen_idle();

        let mut rx_transfer: stm32f4xx_hal::dma::Transfer<RxStream, RX_CHANNEL, Rx<U, u8>, PeripheralToMemory, RxBuffer> =
            stm32f4xx_hal::dma::Transfer::init_peripheral_to_memory(
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

        Transfer {
            inner: rx_transfer,
            capacity: BUFFER_SIZE_2,
            _stream: PhantomData,
            _peripheral: PhantomData,
            _direction: PhantomData,
            _buf: PhantomData
        }
    }


}

