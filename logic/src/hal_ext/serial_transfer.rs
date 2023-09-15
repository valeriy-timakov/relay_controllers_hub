#![deny(unsafe_code)]

use embedded_dma::{ReadBuffer, WriteBuffer};
use crate::errors::{DMAError, Errors};
use core::marker::PhantomData;
use crate::utils::dma_read_buffer::BufferWriter;

pub trait Decomposable<T>
{
    type Container<Y>;
    fn decompose(self) -> (Self::Container<()>, T);
}

pub trait ReadableBuffer {
    fn slice_to(&self, to: usize) -> &[u8];
}

pub trait RxTransferProxy<BUF>
where BUF: WriteBuffer,
{
    fn get_fifo_error_flag(&self) -> bool;
    fn get_transfer_complete_flag(&self) -> bool;
    fn clear_dma_interrupts(&mut self);
    fn get_read_bytes_count(&self) -> usize;
    fn next_transfer(&mut self, new_buf: BUF) -> Result<BUF, DMAError<BUF>>;
    fn is_idle(&self) -> bool;
    fn is_rx_not_empty(&self) -> bool;
    fn clear_idle_interrupt(&self);
}

pub trait TxTransferProxy<BUF>
where
    BUF: ReadBuffer,
{
    fn get_fifo_error_flag(&self) -> bool;
    fn get_transfer_complete_flag(&self) -> bool;
    fn clear_dma_interrupts(&mut self);
    fn next_transfer(&mut self, new_buf: BUF) -> Result<BUF, DMAError<BUF>>;
}

pub struct SerialTransfer<T, R, TxBuff, RxBuff>
where
    T: TxTransferProxy<TxBuff>,
    R: RxTransferProxy<RxBuff>,
    TxBuff: ReadBuffer + BufferWriter,
    RxBuff: WriteBuffer + ReadableBuffer,
{
    tx: TxTransfer<T, TxBuff>,
    rx: RxTransfer<R, RxBuff>,
}

impl <T, R, TxBuff, RxBuff> SerialTransfer<T, R, TxBuff, RxBuff>
    where
        T: TxTransferProxy<TxBuff>,
        R: RxTransferProxy<RxBuff>,
        TxBuff: ReadBuffer + BufferWriter,
        RxBuff: WriteBuffer + ReadableBuffer,
{

    pub fn new(tx_transfer: T, tx_back_buffer: TxBuff, rx_transfer: R, rx_back_buffer: RxBuff) -> Self
    {
        Self {
            tx: TxTransfer::new(tx_transfer, tx_back_buffer),
            rx: RxTransfer::new(rx_transfer, rx_back_buffer),
        }
    }

    pub fn rx(&mut self) -> &mut RxTransfer<R, RxBuff> {
        &mut self.rx
    }

    pub fn tx(&mut self) -> &mut TxTransfer<T, TxBuff> {
        &mut self.tx
    }

    pub fn split(&mut self) -> (&mut TxTransfer<T, TxBuff>,
                                &mut RxTransfer<R, RxBuff>) {
        (&mut self.tx, &mut self.rx)
    }

    pub fn into(self) -> (TxTransfer<T, TxBuff>,
                          RxTransfer<R, RxBuff>) {
        (self.tx, self.rx)
    }
}


pub struct RxTransfer<R, BUF>
where
    R: RxTransferProxy<BUF>,
    BUF: WriteBuffer + ReadableBuffer,
{
    rx_transfer: R,
    back_buffer: Option<BUF>,
    fifo_error: bool,
    buffer_overflow: bool,
}

impl<R, BUF> RxTransfer<R, BUF>
    where
        R: RxTransferProxy<BUF>,
        BUF: WriteBuffer + ReadableBuffer,
{
    pub fn new(rx_transfer: R, back_buffer: BUF) -> Self {
        Self {
            rx_transfer,
            back_buffer: Some(back_buffer),
            fifo_error: false,
            buffer_overflow: false,
        }
    }

    pub fn return_buffer(&mut self, buffer: BUF) {
        self.back_buffer = Some(buffer);
        self.fifo_error = false;
        self.buffer_overflow = false;
    }

    pub fn on_rx_transfer_interrupt<F> (&mut self, receiver: F) -> Result<(), Errors>
        where
            F: FnOnce(&[u8]) -> Result<(), Errors>
    {
        if self.rx_transfer.is_idle() {
            self.rx_transfer.clear_idle_interrupt();
            let bytes_count = self.rx_transfer.get_read_bytes_count();
            let new_buffer = self.back_buffer.take().unwrap();
            let res: Result<(), Errors> = match self.rx_transfer.next_transfer(new_buffer) {
                Ok(buffer) => {
                    let result = receiver(buffer.slice_to(bytes_count));
                    self.return_buffer(buffer);
                    result
                },
                Err(err) => {
                    let (err, buffer) = err.decompose();
                    self.return_buffer(buffer);
                    Err(Errors::DmaError(err))
                }
            };
            return res;
        }
        Err(Errors::TransferInProgress)
    }

    pub fn on_dma_interrupts(&mut self) {
        self.rx_transfer.clear_dma_interrupts();
        if self.rx_transfer.get_fifo_error_flag() {
            self.fifo_error = true;
        }
        if self.rx_transfer.get_transfer_complete_flag() {
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

pub struct TxTransfer<T, BUF>
where
    T: TxTransferProxy<BUF>,
    BUF: ReadBuffer,
{
    tx_transfer: T,
    back_buffer: Option<BUF>,
    fifo_error: bool,
    last_transfer_ended: bool,
}
/*

*/
impl<T, BUF> TxTransfer<T, BUF>
    where
        T: TxTransferProxy<BUF>,
        BUF: ReadBuffer + BufferWriter,
{
    pub fn new(tx_transfer: T, back_buffer: BUF) -> Self {
        Self {
            tx_transfer,
            back_buffer: Some(back_buffer),
            fifo_error: false,
            last_transfer_ended: true,
        }
    }

    /**
    Takes writter function to generate send data and sens them to UART though DMA. Should always return Ok if
    is called from one thread only at the same time.
    */
    pub fn start_transfer<F: FnOnce(&mut BUF)->Result<(), Errors>>(&mut self, writter: F) -> Result<(), Errors>
        where
            F: FnOnce(&mut BUF) -> Result<(), Errors>
    {
        if !self.last_transfer_ended {
            return Err(Errors::TransferInProgress);
        }
        let mut new_buffer = match self.back_buffer.take() {
            Some(buffer) => Ok(buffer),
            None => Err(Errors::NoBufferAvailable),
        }?;
        new_buffer.clear();
        writter(&mut new_buffer)?;


        match self.tx_transfer.next_transfer( new_buffer) {
            Ok(buffer) => {
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
        self.tx_transfer.clear_dma_interrupts();
        if  self.tx_transfer.get_fifo_error_flag() {
            self.fifo_error = true;
        }
        if  self.tx_transfer.get_transfer_complete_flag() {
            self.last_transfer_ended = true;
        }
    }
}