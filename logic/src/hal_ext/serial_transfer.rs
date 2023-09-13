#![deny(unsafe_code)]

use embedded_dma::{ReadBuffer, WriteBuffer};
use crate::errors::Errors;
use core::marker::PhantomData;

pub trait Decomposable<T>
{
    type Container<Y>;
    fn decompose(self) -> (Self::Container<()>, T);
}

pub trait RxTransferProxy<BUF, DmaError>
where BUF: WriteBuffer,
      DmaError: Decomposable<BUF>,
{
    fn get_fifo_error_flag(&self) -> bool;
    fn get_transfer_complete_flag(&self) -> bool;
    fn clear_dma_interrupts(&mut self);
    fn get_read_butes_count() -> usize;
    fn next_transfer(&mut self, new_buf: BUF) -> Result<BUF, DmaError>;
    fn is_idle(&self) -> bool;
    fn is_rx_not_empty(&self) -> bool;
    fn clear_idle_interrupt(&self);
}

pub trait TxTransferProxy<BUF, DmaError>
where
    BUF: ReadBuffer,
    DmaError: Decomposable<BUF>,
{
    fn get_fifo_error_flag(&self) -> bool;
    fn get_transfer_complete_flag(&self) -> bool;
    fn clear_dma_interrupts(&mut self);
    fn next_transfer(&mut self, new_buf: BUF) -> Result<BUF, DmaError>;
}

pub struct SerialTransfer<T, R, TxBuff, RxBuff, DmaErrorT, DmaErrorR>
where
    T: TxTransferProxy<TxBuff, DmaErrorT>,
    R: RxTransferProxy<RxBuff, DmaErrorR>,
    TxBuff: ReadBuffer,
    RxBuff: WriteBuffer,
    DmaErrorR: Decomposable<RxBuff>,
    DmaErrorT: Decomposable<TxBuff>,
{
    tx: TxTransfer<T, TxBuff, DmaErrorT>,
    rx: RxTransfer<R, RxBuff, DmaErrorR>,
}

impl <T, R, TxBuff, RxBuff, DmaErrorT, DmaErrorR> SerialTransfer<T, R, TxBuff, RxBuff, DmaErrorT, DmaErrorR>
    where
        T: TxTransferProxy<TxBuff, DmaErrorT>,
        R: RxTransferProxy<RxBuff, DmaErrorR>,
        TxBuff: ReadBuffer,
        RxBuff: WriteBuffer,
        DmaErrorR: Decomposable<RxBuff>,
        DmaErrorT: Decomposable<TxBuff>,
{

    pub fn new(tx_transfer: T, tx_back_buffer: TxBuff, rx_transfer: R, rx_back_buffer: RxBuff) -> Self
    {
        Self {
            tx: TxTransfer::new(tx_transfer, tx_back_buffer),
            rx: RxTransfer::new(rx_transfer, rx_back_buffer),
        }
    }

    pub fn rx(&mut self) -> &mut RxTransfer<R, RxBuff, DmaErrorR> {
        &mut self.rx
    }

    pub fn tx(&mut self) -> &mut TxTransfer<T, TxBuff, DmaErrorT> {
        &mut self.tx
    }

    pub fn split(&mut self) -> (&mut TxTransfer<T, TxBuff, DmaErrorT>,
                                &mut RxTransfer<R, RxBuff, DmaErrorR>) {
        (&mut self.tx, &mut self.rx)
    }

    pub fn into(self) -> (TxTransfer<T, TxBuff, DmaErrorT>,
                          RxTransfer<R, RxBuff, DmaErrorR>) {
        (self.tx, self.rx)
    }
}


pub struct RxTransfer<R, BUF, DmaError>
where
    R: RxTransferProxy<BUF, DmaError>,
    BUF: WriteBuffer,
    DmaError: Decomposable<BUF>,
{
    rx_transfer: R,
    back_buffer: Option<BUF>,
    fifo_error: bool,
    buffer_overflow: bool,
    _error_buffer_container: PhantomData<DmaError>,
}

impl<R, BUF, DmaError> RxTransfer<R, BUF, DmaError>
    where
        R: RxTransferProxy<BUF, DmaError>,
        BUF: WriteBuffer,
        DmaError: Decomposable<BUF>,
{
    pub fn new(rx_transfer: R, back_buffer: BUF) -> Self {

        rx_transfer.start();

        Self {
            rx_transfer,
            back_buffer: Some(back_buffer),
            fifo_error: false,
            buffer_overflow: false,
            _error_buffer_container: PhantomData,
        }
    }

    pub fn get_transferred_buffer(&mut self) -> Result<(BUF, usize), Errors> {
        if self.rx_transfer.is_idle() {
            self.rx_transfer.clear_idle_interrupt();
            let bytes_count = self.rx_transfer.get_read_butes_count() as usize;
            let new_buffer = self.back_buffer.take().unwrap();
            let (buffer, _) = self.rx_transfer.next_transfer(new_buffer).unwrap();
            return Ok((buffer, bytes_count));
        }
        Err(Errors::TransferInProgress)
    }

    pub fn return_buffer(&mut self, buffer: BUF) {
        self.back_buffer = Some(buffer);
        self.fifo_error = false;
        self.buffer_overflow = false;
    }

    pub fn on_rx_transfer_interrupt<F: FnOnce(&[u8]) -> Result<(), Errors>>(&mut self, receiver: F) -> Result<(), Errors> {
        if self.rx_transfer.is_idle() {
            self.rx_transfer.clear_idle_interrupt();
            let bytes_count = self.rx_transfer.get_read_butes_count();
            let new_buffer = self.back_buffer.take().unwrap();
            let (buffer, _) = self.rx_transfer.next_transfer(new_buffer).unwrap();
            let result = receiver(&buffer[..bytes_count]);
            self.return_buffer(buffer);
            return result;
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

pub struct TxTransfer<T, BUF, DmaError>
where
    T: TxTransferProxy<BUF, DmaError>,
    BUF: ReadBuffer,
    DmaError: Decomposable<BUF>,
{
    tx_transfer: T,
    back_buffer: Option<BUF>,
    fifo_error: bool,
    last_transfer_ended: bool,
    _error_buffer_container: PhantomData<DmaError>,
}
/*

*/
impl<T, BUF, DmaError> TxTransfer<T, BUF, DmaError>
    where
        T: TxTransferProxy<BUF, DmaError>,
        BUF: ReadBuffer,
        DmaError: Decomposable<BUF>,
{
    pub fn new(tx_transfer: T, back_buffer: BUF) -> Self {
        Self {
            tx_transfer,
            back_buffer: Some(back_buffer),
            fifo_error: false,
            last_transfer_ended: true,
            _error_buffer_container: PhantomData,
        }
    }

    /**
    Takes writter function to generate send data and sens them to UART though DMA. Should always return Ok if
    is called from one thread only at the same time.
    */
    pub fn start_transfer<F: FnOnce(&mut BUF)->Result<(), Errors>>(&mut self, writter: F) -> Result<(), Errors> {
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
        if  self.tx_transfer.get_fifo_error_flag() {
            self.fifo_error = true;
        }
        if  self.tx_transfer.get_transfer_complete_flag() {
            self.last_transfer_ended = true;
        }
    }
}