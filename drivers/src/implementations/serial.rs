

use stm32f4xx_hal::dma::{ChannelX, DMAError, MemoryToPeripheral, PeripheralToMemory, Transfer};
use stm32f4xx_hal::dma::traits::{Channel, DMASet, PeriAddress, Stream};
use stm32f4xx_hal::serial::{Instance, Rx, RxISR, RxListen, Tx, TxISR};
use stm32f4xx_hal::dma::config::DmaConfig;
use logic::hal_ext::serial_transfer::{Decomposable, ReadableBuffer, RxTransferProxy, SerialTransfer, TxTransferProxy};
use logic::utils::dma_read_buffer::Buffer;
use core::marker::PhantomData;
use logic::errors;


const BUFFER_SIZE: usize = 256;
pub type TxBuffer = Buffer<BUFFER_SIZE>;
pub type RxBuffer = &'static mut [u8; BUFFER_SIZE];

impl ReadableBuffer for RxBuffer {
    fn slice_to(&self, to: usize) -> &[u8] {
        &self[..to]
    }
}

impl<U, STREAM, const CHANNEL: u8> TxTransferProxy<TxBuffer> for
Transfer<STREAM, CHANNEL, Tx<U>, MemoryToPeripheral, TxBuffer>
    where
        U: Instance,
        Tx<U, u8>: PeriAddress<MemSize=u8> + DMASet<STREAM, CHANNEL, MemoryToPeripheral> + TxISR,
        STREAM: Stream,
        ChannelX<CHANNEL>: Channel,
{
    #[inline(always)]
    fn get_fifo_error_flag(&self) -> bool {
        STREAM::get_fifo_error_flag()
    }

    #[inline(always)]
    fn get_transfer_complete_flag(&self) -> bool {
        STREAM::get_transfer_complete_flag()
    }

    #[inline(always)]
    fn clear_dma_interrupts(&mut self) {
        self.clear_interrupts();
    }

    #[inline(always)]
    fn next_transfer(&mut self, buffer: TxBuffer) -> Result<TxBuffer, errors::DMAError<TxBuffer>> {
        self.next_transfer(buffer)
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
    fn get_fifo_error_flag(&self) -> bool {
        STREAM::get_fifo_error_flag()
    }

    #[inline(always)]
    fn get_transfer_complete_flag(&self) -> bool {
        STREAM::get_transfer_complete_flag()
    }

    #[inline(always)]
    fn clear_dma_interrupts(&mut self) {
        self.clear_interrupts();
    }

    #[inline(always)]
    fn get_read_bytes_count(&self) -> usize {
        BUFFER_SIZE - STREAM::get_number_of_transfers() as usize
    }

    #[inline(always)]
    fn next_transfer(&mut self, new_buf: RxBuffer) -> Result<RxBuffer, errors::DMAError<RxBuffer>> {
        self.next_transfer(new_buf)
            .map(|(buffer, _)| { buffer } )
            .map_err(convert_dma_error )
    }

    #[inline(always)]
    fn is_idle(&self) -> bool {
        (self as &dyn RxISR).is_idle()
    }

    #[inline(always)]
    fn is_rx_not_empty(&self) -> bool {
        (self as &dyn RxISR).is_rx_not_empty()
    }

    #[inline(always)]
    fn clear_idle_interrupt(&mut self) {
        (self as &dyn RxISR).clear_idle_interrupt()
    }

}

fn convert_dma_error<T>(e: DMAError<T>) -> errors::DMAError<T> {
    match e {
        DMAError::NotReady(t) => errors::DMAError::NotReady(t),
        DMAError::SmallBuffer(t) => errors::DMAError::SmallBuffer(t),
        DMAError::Overrun(t) => errors::DMAError::Overrun(t),
    }
}


pub struct SerialTransferBuilderSTMF401x<U, TxStream, const TX_CHANNEL: u8, RxStream, const RX_CHANNEL: u8>
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
    _rx_stream: PhantomData<RxStream>
}

impl<U, TxStream, const TX_CHANNEL: u8, RxStream, const RX_CHANNEL: u8> SerialTransferBuilderSTMF401x<U, TxStream, TX_CHANNEL, RxStream, RX_CHANNEL>
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
        tx: Tx<U, u8>,
        dma_tx_stream: TxStream,
        rx: Rx<U>,
        dma_rx_stream: RxStream,
    ) -> SerialTransfer<
        Transfer<TxStream, TX_CHANNEL, Tx<U, u8>, MemoryToPeripheral, TxBuffer>,
        Transfer<RxStream, RX_CHANNEL, Rx<U, u8>, PeripheralToMemory, RxBuffer>,
        TxBuffer, RxBuffer
    > {

        let tx_buffer1 = Buffer::new(cortex_m::singleton!(: [u8; BUFFER_SIZE] = [0; BUFFER_SIZE]).unwrap());
        let tx_buffer2 = Buffer::new(cortex_m::singleton!(: [u8; BUFFER_SIZE] = [0; BUFFER_SIZE]).unwrap());
        let rx_buffer1 = cortex_m::singleton!(: [u8; BUFFER_SIZE] = [0; BUFFER_SIZE]).unwrap();
        let rx_buffer2 = cortex_m::singleton!(: [u8; BUFFER_SIZE] = [0; BUFFER_SIZE]).unwrap();

        SerialTransfer::new(
            Self::create_tx(tx, dma_tx_stream, tx_buffer1), tx_buffer2,
            Self::create_rx(rx, dma_rx_stream, rx_buffer1), rx_buffer2
        )
    }

    fn create_tx(
        tx: Tx<U, u8>,
        dma_stream: TxStream,
        tx_buffer1: TxBuffer,
    ) -> Transfer<TxStream, TX_CHANNEL, Tx<U, u8>, MemoryToPeripheral, TxBuffer> {

        let tx_transfer: Transfer<TxStream, TX_CHANNEL, Tx<U, u8>, MemoryToPeripheral, TxBuffer> = Transfer::init_memory_to_peripheral(
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

        tx_transfer
    }


    fn create_rx(
        mut rx: Rx<U>,
        dma_stream: RxStream,
        rx_buffer1: RxBuffer,
    ) -> Transfer<RxStream, RX_CHANNEL, Rx<U, u8>, PeripheralToMemory, RxBuffer> {

        rx.listen_idle();

        let mut rx_transfer: Transfer<RxStream, RX_CHANNEL, Rx<U, u8>, PeripheralToMemory, RxBuffer> = Transfer::init_peripheral_to_memory(
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

        rx_transfer
    }


}

