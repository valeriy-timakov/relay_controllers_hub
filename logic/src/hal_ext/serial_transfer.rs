#![allow(unsafe_code)]

use embedded_dma::{ReadBuffer, WriteBuffer};
use crate::errors::{DMAError, Errors};
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
    fn clear_idle_interrupt(&mut self);
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

pub trait Receiver {
    fn on_rx_transfer_interrupt<F: FnOnce(&[u8]) -> Result<(), Errors>> (&mut self, receiver: F) -> Result<(), Errors>;
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

    fn return_buffer(&mut self, buffer: BUF) {
        self.back_buffer = Some(buffer);
        self.fifo_error = false;
        self.buffer_overflow = false;
    }
}

impl <R, BUF> Receiver for RxTransfer<R, BUF>
    where
        R: RxTransferProxy<BUF>,
        BUF: WriteBuffer + ReadableBuffer,
{

    fn on_rx_transfer_interrupt<F> (&mut self, receiver: F) -> Result<(), Errors>
        where
            F: FnOnce(&[u8]) -> Result<(), Errors>
    {
        if self.rx_transfer.is_idle() {
            self.rx_transfer.clear_idle_interrupt();
            let bytes_count = self.rx_transfer.get_read_bytes_count();
            let new_buffer = self.back_buffer.take().unwrap();
            match self.rx_transfer.next_transfer(new_buffer) {
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
            }
        } else {
            Err(Errors::TransferInProgress)
        }
    }

}

pub trait Sender<BUF: ReadBuffer + BufferWriter> {
    fn start_transfer<F: FnOnce(&mut BUF)->Result<(), Errors>>(&mut self, writter: F) -> Result<(), Errors>;
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

    pub fn on_dma_interrupts(&mut self) {
        self.tx_transfer.clear_dma_interrupts();
        if  self.tx_transfer.get_fifo_error_flag() {
            self.fifo_error = true;
        }
        if  self.tx_transfer.get_transfer_complete_flag() {
            self.last_transfer_ended = true;
        }
    }

    pub fn fifo_error(&self) -> bool {
        self.fifo_error
    }
    pub fn last_transfer_ended(&self) -> bool {
        self.last_transfer_ended
    }
}

impl<T, BUF> Sender<BUF> for TxTransfer<T, BUF>
    where
        T: TxTransferProxy<BUF>,
        BUF: ReadBuffer + BufferWriter,
{

    /**
    Takes writter function to generate send data and sens them to UART though DMA. Should always return Ok if
    is called from one thread only at the same time.
     */
    fn start_transfer<F: FnOnce(&mut BUF)->Result<(), Errors>>(&mut self, writter: F) -> Result<(), Errors> {
        if !self.last_transfer_ended && !self.fifo_error {
            return Err(Errors::TransferInProgress);
        }
        let mut new_buffer = match self.back_buffer.take() {
            Some(buffer) => Ok(buffer),
            None => Err(Errors::NoBufferAvailable),
        }?;
        new_buffer.clear();
        writter(&mut new_buffer)?;

        self.fifo_error = false;
        self.last_transfer_ended = false;


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

}




#[cfg(test)]
mod tests {
    use core::cell::RefCell;
    use core::cmp::min;
    use core::ops::DerefMut;
    use super::*;
    use std::rc::Rc;
    use quickcheck_macros::quickcheck;
    use rand::prelude::*;

    const BUFFER_SIZE: usize = 20;

    struct MockRxBuffer {
        number: u8,
        buffer: [u8; BUFFER_SIZE],
    }

    unsafe impl WriteBuffer for MockRxBuffer {
        type Word = u8;

        unsafe fn write_buffer(&mut self) -> (*mut Self::Word, usize) {
            let ptr = self.buffer.as_mut_ptr();
            (ptr, self.buffer.len())
        }
    }

    impl ReadableBuffer for MockRxBuffer {
        fn slice_to(&self, to: usize) -> &[u8] {
            &self.buffer[..to]
        }
    }

    impl MockRxBuffer {
        fn new(number: u8) -> Self {
            Self {
                number,
                buffer: [0_u8; BUFFER_SIZE],
            }
        }

        fn set(&mut self, value: &[u8]) {
            let len = min(value.len(), self.buffer.len());
            for i in 0..len {
                self.buffer[i] = value[i];
            }
        }
    }

    struct MockRxTransfer {
        clear_dma_interrupts_calls: usize,
        clear_idle_interrupt_calls: usize,
        fifo_error: bool,
        transfer_complete: bool,
        idle: bool,
        read_bytes_count: usize,
        rx_not_empty: bool,
        return_error_on_next: bool,
        curr_buf: Option<MockRxBuffer>,
    }

    impl  MockRxTransfer {
        fn new(fifo_error: bool, transfer_complete: bool, idle: bool, rx_not_empty: bool,
               return_error_on_next: bool, read_bytes_count: usize, curr_buf: MockRxBuffer) -> Self {
            MockRxTransfer {
                clear_dma_interrupts_calls: 0,
                clear_idle_interrupt_calls: 0,
                fifo_error,
                transfer_complete,
                idle,
                read_bytes_count,
                rx_not_empty,
                return_error_on_next,
                curr_buf: Some(curr_buf),
            }
        }
    }

    fn create_transfer_error_err<T>(value: T) -> DMAError<T> {
        DMAError::NotReady(value)
    }

    impl RxTransferProxy<MockRxBuffer> for MockRxTransfer {

        fn get_fifo_error_flag(&self) -> bool {
            self.fifo_error
        }

        fn get_transfer_complete_flag(&self) -> bool {
            self.transfer_complete
        }

        fn get_read_bytes_count(&self) -> usize {
            self.read_bytes_count
        }

        fn is_idle(&self) -> bool {
            self.idle
        }

        fn is_rx_not_empty(&self) -> bool {
            self.rx_not_empty
        }

        fn clear_dma_interrupts(&mut self) {
            self.clear_dma_interrupts_calls += 1;
        }

        fn clear_idle_interrupt(&mut self) {
            self.clear_idle_interrupt_calls += 1;
        }

        fn next_transfer(&mut self, new_buf: MockRxBuffer) -> Result<MockRxBuffer, DMAError<MockRxBuffer>> {
            let last_buf = self.curr_buf.take().unwrap();
            self.curr_buf = Some(new_buf);
            if self.return_error_on_next {
                self.return_error_on_next = false;
                Err(create_transfer_error_err(last_buf))
            } else {
                Ok(last_buf)
            }
        }
    }

    impl RxTransferProxy<MockRxBuffer> for Rc<RefCell<MockRxTransfer>> {

            fn get_fifo_error_flag(&self) -> bool {
                self.borrow().get_fifo_error_flag()
            }

            fn get_transfer_complete_flag(&self) -> bool {
                self.borrow().get_transfer_complete_flag()
            }

            fn get_read_bytes_count(&self) -> usize {
                self.borrow().get_read_bytes_count()
            }

            fn is_idle(&self) -> bool {
                self.borrow().is_idle()
            }

            fn is_rx_not_empty(&self) -> bool {
                self.borrow().is_rx_not_empty()
            }

            fn clear_dma_interrupts(&mut self) {
                self.borrow_mut().clear_dma_interrupts()
            }

            fn clear_idle_interrupt(&mut self) {
                self.borrow_mut().clear_idle_interrupt()
            }

            fn next_transfer(&mut self, new_buf: MockRxBuffer) -> Result<MockRxBuffer, DMAError<MockRxBuffer>> {
                self.borrow_mut().next_transfer(new_buf)
            }
    }

    struct MockTxBuffer {
        number: u8,
        buffer: [u8; BUFFER_SIZE],
        cleared: bool,
    }

    unsafe impl ReadBuffer for MockTxBuffer {
        type Word = u8;

        unsafe fn read_buffer(&self) -> (*const Self::Word, usize) {
            let ptr = self.buffer.as_ptr();
            (ptr, self.buffer.len())
        }
    }

    impl BufferWriter for MockTxBuffer {


        fn add_str(&mut self, string: &str) -> Result<(), Errors> {
            Ok(())
        }

        fn add(&mut self, data: &[u8]) -> Result<(), Errors> {
            Ok(())
        }

        fn add_u8(&mut self, byte: u8) -> Result<(), Errors> {
            Ok(())
        }

        fn add_u16(&mut self, value: u16) -> Result<(), Errors> {
            Ok(())
        }

        fn add_u32(&mut self, value: u32) -> Result<(), Errors> {
            Ok(())
        }

        fn add_u64(&mut self, value: u64) -> Result<(), Errors> {
            Ok(())
        }

        fn clear(&mut self) {
            self.cleared = true;
        }

    }

    impl MockTxBuffer {
        fn new(number: u8) -> Self {
            Self {
                number,
                buffer: [0_u8; BUFFER_SIZE],
                cleared: false,
            }
        }
    }

    struct MockTxTransfer {
        clear_dma_interrupts_calls: usize,
        clear_idle_interrupt_calls: usize,
        fifo_error: bool,
        transfer_complete: bool,
        idle: bool,
        read_bytes_count: usize,
        rx_not_empty: bool,
        return_error_on_next: bool,
        curr_buf: Option<MockTxBuffer>,
    }

    impl  MockTxTransfer {
        fn new(fifo_error: bool, transfer_complete: bool, idle: bool, rx_not_empty: bool,
               return_error_on_next: bool, read_bytes_count: usize, curr_buf: MockTxBuffer) -> Self {
            MockTxTransfer {
                clear_dma_interrupts_calls: 0,
                clear_idle_interrupt_calls: 0,
                fifo_error,
                transfer_complete,
                idle,
                read_bytes_count,
                rx_not_empty,
                return_error_on_next,
                curr_buf: Some(curr_buf),
            }
        }
    }

    impl TxTransferProxy<MockTxBuffer> for MockTxTransfer {

        fn get_fifo_error_flag(&self) -> bool {
            self.fifo_error
        }

        fn get_transfer_complete_flag(&self) -> bool {
            self.transfer_complete
        }

        fn clear_dma_interrupts(&mut self) {
            self.clear_dma_interrupts_calls += 1;
        }


        fn next_transfer(&mut self, new_buf: MockTxBuffer) -> Result<MockTxBuffer, DMAError<MockTxBuffer>> {
            let last_buf = self.curr_buf.take().unwrap();
            self.curr_buf = Some(new_buf);
            if self.return_error_on_next {
                self.return_error_on_next = false;
                Err(create_transfer_error_err(last_buf))
            } else {
                Ok(last_buf)
            }
        }

    }

    impl TxTransferProxy<MockTxBuffer> for Rc<RefCell<MockTxTransfer>> {

        fn get_fifo_error_flag(&self) -> bool {
            self.borrow().get_fifo_error_flag()
        }

        fn get_transfer_complete_flag(&self) -> bool {
            self.borrow().get_transfer_complete_flag()
        }

        fn clear_dma_interrupts(&mut self) {
            self.borrow_mut().clear_dma_interrupts()
        }

        fn next_transfer(&mut self, new_buf: MockTxBuffer) -> Result<MockTxBuffer, DMAError<MockTxBuffer>> {
            self.borrow_mut().next_transfer(new_buf)
        }
    }


    #[quickcheck]
    fn test_rx_on_dma_interrupts(fifo_error: bool, transfer_complete: bool) {
        let (mut rx_transfer, mock) = create_testable_rx_transfer();
        mock.borrow_mut().fifo_error = fifo_error;
        mock.borrow_mut().transfer_complete = transfer_complete;
        rx_transfer.on_dma_interrupts();
        assert_eq!(mock.borrow().clear_dma_interrupts_calls, 1);
        assert_eq!(rx_transfer.fifo_error(), fifo_error);
        assert_eq!(rx_transfer.buffer_overflow(), transfer_complete);
    }

    #[test]
    fn test_on_rx_transfer_interrupt_on_no_idle() {
        let (mut rx_transfer, mock) = create_testable_rx_transfer();
        let mut callback = false;
        let curr_buff_num = mock.borrow().curr_buf.as_ref().unwrap().number;
        let next_buff_num = rx_transfer.back_buffer.as_ref().unwrap().number;

        //
        // assert_eq!(next_buff_num, mock.borrow().curr_buf.as_ref().unwrap().number);

        let res = rx_transfer.on_rx_transfer_interrupt(|_| {
            callback = true;
            Ok(())
        } );

        assert_eq!(Err(Errors::TransferInProgress), res);
        assert_eq!(0, mock.borrow().clear_idle_interrupt_calls);
        assert_eq!(false, callback);
        assert_eq!(curr_buff_num, mock.borrow().curr_buf.as_ref().unwrap().number);
        assert_eq!(next_buff_num, rx_transfer.back_buffer.as_ref().unwrap().number);
    }

    #[test]
    fn test_on_rx_transfer_interrupt_on_idle_error() -> Result<(), ()> {
        let (mut rx_transfer, mock) = create_testable_rx_transfer();
        mock.borrow_mut().idle = true;
        mock.borrow_mut().return_error_on_next = true;
        let mut callback = false;
        let next_buff_num = rx_transfer.back_buffer.as_ref().unwrap().number;
        let curr_buff_num = mock.borrow().curr_buf.as_ref().unwrap().number;

        let res = rx_transfer.on_rx_transfer_interrupt(|_| {
            callback = true;
            Ok(())
        } );

        assert_eq!(mock.borrow().clear_idle_interrupt_calls, 1);
        assert_eq!(callback, false);
        assert_eq!(next_buff_num, mock.borrow().curr_buf.as_ref().unwrap().number);
        assert_eq!(curr_buff_num, rx_transfer.back_buffer.as_ref().unwrap().number);
        match res {
            Err(Errors::DmaError(err)) => {
                if err == create_transfer_error_err(()) {
                    Ok(())
                } else {
                    Err(())
                }
            }
            _ => { Err(()) }
        }
    }

    #[test]
    fn test_on_rx_transfer_interrupt_on_idle() {
        let (mut rx_transfer, mock) = create_testable_rx_transfer();
        mock.borrow_mut().idle = true;
        let mut callback = false;
        let next_buff_num = rx_transfer.back_buffer.as_ref().unwrap().number;
        let curr_buff_num = mock.borrow().curr_buf.as_ref().unwrap().number;

        let res = rx_transfer.on_rx_transfer_interrupt(|_| {
            callback = true;
            Ok(())
        } );

        assert_eq!(res, Ok(()));
        assert_eq!(mock.borrow().clear_idle_interrupt_calls, 1);
        assert_eq!(callback, true);
        assert_eq!(next_buff_num, mock.borrow().curr_buf.as_ref().unwrap().number);
        assert_eq!(curr_buff_num, rx_transfer.back_buffer.as_ref().unwrap().number);
    }

    #[test]
    fn test_on_rx_transfer_interrupt_on_idle_return_all_bytes_count() {
        let mut rng = rand::thread_rng();
        for returned_bytes_count in 0..BUFFER_SIZE {
            let mut test_data = [1_u8; BUFFER_SIZE];
            for i in 0..returned_bytes_count {
                test_data[i] = rng.gen_range(1..100);
            }
            let (mut rx_transfer, mock) = create_testable_rx_transfer();
            mock.borrow_mut().idle = true;
            mock.borrow_mut().read_bytes_count = returned_bytes_count;
            mock.borrow_mut().curr_buf.as_mut().unwrap().set(&test_data);
            let mut returned_bytes_count_res = BUFFER_SIZE;
            let mut read_data = [0_u8; BUFFER_SIZE];

            let res = rx_transfer.on_rx_transfer_interrupt(|slice| {
                returned_bytes_count_res = slice.len();
                for i in 0..returned_bytes_count_res {
                    read_data[i] = slice[i];
                }
                &read_data[..returned_bytes_count_res].copy_from_slice(slice);
                Ok(())
            } );

            assert_eq!(returned_bytes_count, returned_bytes_count_res);
            assert_eq!(&test_data[..returned_bytes_count_res], &read_data[..returned_bytes_count_res]);

        }

    }

    #[test]
    fn test_on_rx_transfer_interrupt_on_idle_should_proxy_callback_error() {
        let errors = [Errors::DataCorrupted, Errors::DmaBufferOverflow, Errors::DmaError(DMAError::NotReady(()))];

        for error in errors {
            let (mut rx_transfer, mock) = create_testable_rx_transfer();
            mock.borrow_mut().idle = true;

            let res = rx_transfer.on_rx_transfer_interrupt(|_| {
                Err(error)
            });

            assert_eq!(res, Err(error));
        }
    }

    fn create_testable_rx_transfer() ->
                                     (RxTransfer<Rc<RefCell<MockRxTransfer>>, MockRxBuffer>, Rc<RefCell<MockRxTransfer>>)
    {
        let mut buf1 = MockRxBuffer::new(1);
        let mut buf2 = MockRxBuffer::new(2);

        let transfer1 = MockRxTransfer::new(
            false, false, false, false,
            false, 0, buf1);
        let mock = Rc::new(RefCell::new(transfer1));

        let transfer = mock.clone().borrow_mut().deref_mut();
        let mut rx_transfer = RxTransfer::new(mock.clone(), buf2);

        (rx_transfer, mock)

    }


    #[quickcheck]
    fn test_tx_on_dma_interrupts(fifo_error: bool, transfer_complete: bool) {
        let (mut tx_transfer, mock) = create_testable_tx_transfer();
        mock.borrow_mut().fifo_error = fifo_error;
        mock.borrow_mut().transfer_complete = transfer_complete;
        //move to transfer state
        tx_transfer.start_transfer(|_| { Ok(()) }).unwrap();
        tx_transfer.on_dma_interrupts();
        assert_eq!(mock.borrow().clear_dma_interrupts_calls, 1);
        assert_eq!(tx_transfer.fifo_error(), fifo_error);
        assert_eq!(tx_transfer.last_transfer_ended(), transfer_complete);
    }

    #[test]
    fn test_start_transfer_should_return_error_if_transfer_not_ended()  {
        let (mut tx_transfer, mock) = create_testable_tx_transfer();

        assert_eq!(Ok(()), tx_transfer.start_transfer(|_| { Ok(()) }));

        assert_eq!(Err(Errors::TransferInProgress), tx_transfer.start_transfer(|_| { Ok(()) }));

    }

    #[test]
    fn test_start_transfer_should_proxy_error_from_next_transfer() -> Result<(), ()> {
        let (mut tx_transfer, mock) = create_testable_tx_transfer();
        mock.borrow_mut().return_error_on_next = true;
        let mut callback = false;
        let next_buff_num = tx_transfer.back_buffer.as_ref().unwrap().number;
        let curr_buff_num = mock.borrow().curr_buf.as_ref().unwrap().number;

        let res = tx_transfer.start_transfer(|_| {
            callback = true;
            Ok(())
        });

        assert_eq!(true, callback);
        assert_eq!(next_buff_num, mock.borrow().curr_buf.as_ref().unwrap().number);
        assert_eq!(curr_buff_num, tx_transfer.back_buffer.as_ref().unwrap().number);
        assert_eq!(true, mock.borrow().curr_buf.as_ref().unwrap().cleared);

        match res {
            Err(Errors::DmaError(err)) => {
                if err == create_transfer_error_err(()) {
                    Ok(())
                } else {
                    Err(())
                }
            }
            _ => { Err(()) }
        }
    }

    #[test]
    fn test_start_transfer_works()  {
        let (mut tx_transfer, mock) = create_testable_tx_transfer();
        let mut callback = false;
        let curr_buff_num = mock.borrow().curr_buf.as_ref().unwrap().number;
        let next_buff_num = tx_transfer.back_buffer.as_ref().unwrap().number;
        let mut buff_num_to_write = 0_u8;

        let res = tx_transfer.start_transfer(|buff| {
            callback = true;
            buff_num_to_write = buff.number;
            Ok(())
        });

        assert_eq!(Ok(()), res);
        assert_eq!(true, callback);
        assert_eq!(false, tx_transfer.last_transfer_ended());
        assert_eq!(next_buff_num, buff_num_to_write);
        assert_eq!(curr_buff_num, tx_transfer.back_buffer.as_ref().unwrap().number);
        assert_eq!(next_buff_num, mock.borrow().curr_buf.as_ref().unwrap().number);
        assert_eq!(true, mock.borrow().curr_buf.as_ref().unwrap().cleared);

    }

    #[test]
    fn test_start_transfer_clear_flags()  {
        let (mut tx_transfer, mock) = create_testable_tx_transfer();

        mock.borrow_mut().fifo_error = true;
        mock.borrow_mut().transfer_complete = true;
        tx_transfer.on_dma_interrupts();
        assert_eq!(true, tx_transfer.last_transfer_ended());
        assert_eq!(true, tx_transfer.fifo_error());

        let res = tx_transfer.start_transfer(|buff| { Ok(()) });

        assert_eq!(false, tx_transfer.last_transfer_ended());
        assert_eq!(false, tx_transfer.fifo_error());

    }

    fn create_testable_tx_transfer() ->
                                     (TxTransfer<Rc<RefCell<MockTxTransfer>>, MockTxBuffer>, Rc<RefCell<MockTxTransfer>>)
    {
        let buf1 = MockTxBuffer::new(1);
        let buf2 = MockTxBuffer::new(2);

        let transfer1 = MockTxTransfer::new(
            false, false, false, false,
            false, 0, buf1);
        let mock = Rc::new(RefCell::new(transfer1));

        let transfer = mock.clone().borrow_mut().deref_mut();
        let mut tx_transfer = TxTransfer::new(mock.clone(), buf2);

        (tx_transfer, mock)

    }


}