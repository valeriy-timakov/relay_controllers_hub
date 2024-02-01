#![deny(unsafe_code)]

pub mod domain;
pub mod parsers;
pub mod requests_controller;
pub mod signals_controller;
mod transmitter_to_slave;
pub mod receiver_from_slave;

use embedded_dma::{ReadBuffer, WriteBuffer};
use domain::{*};
use crate::errors::Errors;
use crate::hal_ext::rtc_wrapper::{RelativeMillis, RelativeTimestampSource};
use crate::hal_ext::serial_transfer::{ ReadableBuffer, RxTransfer, RxTransferProxy, SerialTransfer, TxTransfer, TxTransferProxy};
use crate::services::slave_controller_link::parsers::{init_cache_getters, PayloadParserImpl, ResponseBodyParserImpl, ResponseParser, ResponseParserImpl, SignalParserImpl};
use crate::services::slave_controller_link::receiver_from_slave::{ErrorHandler, ReceiverFromSlaveController, RequestsControllerSource};
use crate::utils::dma_read_buffer::BufferWriter;
use crate::services::slave_controller_link::requests_controller::{RequestsController, ResponseHandler};
use crate::services::slave_controller_link::signals_controller::{ControlledRequestSender, SignalControllerImpl, SignalsHandler};
use crate::services::slave_controller_link::transmitter_to_slave::{ErrorsSender, RequestsSender, TransmitterToSlaveController};
use crate::services::slave_controller_link::receiver_from_slave::ReceiverFromSlaveControllerAbstract;


pub fn init_slave_controllers() {
    init_cache_getters();
}



pub struct SlaveControllerLink<T, R, TxBuff, RxBuff, SH, RH, EH>
    where
        TxBuff: ReadBuffer + BufferWriter,
        RxBuff: WriteBuffer + ReadableBuffer,
        T: TxTransferProxy<TxBuff>,
        R: RxTransferProxy<RxBuff>,
        SH: SignalsHandler,
        RH: ResponseHandler,
        EH: ErrorHandler,
{
    tx: TransmitterToSlaveController<TxBuff, TxTransfer<T, TxBuff>>,
    rx: ReceiverFromSlaveController<RxTransfer<R, RxBuff>, EH, PayloadParserImpl, SignalParserImpl, ResponseParserImpl>,
    signal_controller: SignalControllerImpl<SH>,
    requests_controller: RequestsController<RH, ResponseBodyParserImpl>,
}


impl <T, R, TxBuff, RxBuff, SH, RH, EH> SlaveControllerLink<T, R,TxBuff, RxBuff, SH, RH, EH>
    where
        TxBuff: ReadBuffer + BufferWriter,
        RxBuff: WriteBuffer + ReadableBuffer,
        T: TxTransferProxy<TxBuff>,
        R: RxTransferProxy<RxBuff>,
        SH: SignalsHandler,
        RH: ResponseHandler,
        EH: ErrorHandler,
{
    pub fn create(serial_transfer: SerialTransfer<T, R, TxBuff, RxBuff>, signals_handler: SH,
                  responses_handler: RH, receive_error_handler: EH, api_version: Version) -> Result<Self, Errors>
    {
        let (tx, rx) = serial_transfer.into();
        let tx = TransmitterToSlaveController::new(tx);
        let response_body_parser = ResponseBodyParserImpl::create()?;
        let requests_controller = RequestsController::new(responses_handler,
                                                          response_body_parser, api_version);
         // let signals_handler = SignalsHandlerProxy::new(signals_handler,
         //                                                || {rtc.get_relative_timestamp()},
         //                                                &mut tx);

        let signal_controller = SignalControllerImpl::new(signals_handler);
        let payload_parser = PayloadParserImpl::new();

        let rx = ReceiverFromSlaveController::new(rx, receive_error_handler, payload_parser);
        Ok(Self {
            tx,
            rx,
            signal_controller,
            requests_controller
        })
    }

    #[inline(always)]
    pub fn on_get_command<TS: RelativeTimestampSource>( &mut self, time_source: &mut TS) {
        let Self{ rx, tx,
            signal_controller, requests_controller} = { &mut *self };
        let mut sender = SenderImp::new(tx, requests_controller);
        rx.on_get_command(signal_controller,  &mut sender, time_source);
    }

    #[inline(always)]
    pub fn on_rx_dma_interrupts(&mut self) {
        self.rx.inner_rx().on_dma_interrupts();
    }

    #[inline(always)]
    pub fn on_tx_dma_interrupts(&mut self) {
        self.tx.inner_tx().on_dma_interrupts();
    }

    #[inline(always)]
    pub fn send_request<I: DataInstruction>(&mut self, operation: Operation, instruction: I, timestamp: RelativeMillis) -> Result<Option<u32>, Errors> {
         self.tx.send_request(operation, instruction, timestamp, &mut self.requests_controller)
    }
}

impl <T, R, TxBuff, RxBuff, SH, RH, EH> ControlledRequestSender for SlaveControllerLink<T, R,TxBuff, RxBuff, SH, RH, EH>
    where
        TxBuff: ReadBuffer + BufferWriter,
        RxBuff: WriteBuffer + ReadableBuffer,
        T: TxTransferProxy<TxBuff>,
        R: RxTransferProxy<RxBuff>,
        SH: SignalsHandler,
        RH: ResponseHandler,
        EH: ErrorHandler,
{
    fn send(&mut self, operation: Operation, instruction: DataInstructions, timestamp: RelativeMillis) -> Result<Option<u32>, Errors> {
        self.send_request(operation, instruction, timestamp)
    }
}

impl <T, R, TxBuff, RxBuff, SH, RH, EH> ErrorsSender for SlaveControllerLink<T, R,TxBuff, RxBuff, SH, RH, EH>
    where
        TxBuff: ReadBuffer + BufferWriter,
        RxBuff: WriteBuffer + ReadableBuffer,
        T: TxTransferProxy<TxBuff>,
        R: RxTransferProxy<RxBuff>,
        SH: SignalsHandler,
        RH: ResponseHandler,
        EH: ErrorHandler, {

    #[inline(always)]
    fn send_error(&mut self, instruction_code: u8, error_code: ErrorCode) -> Result<(), Errors> {
        self.tx.send_error(instruction_code, error_code)
    }
}

struct SenderImp<'a, T, TxBuff, RH>
    where
        TxBuff: ReadBuffer + BufferWriter,
        T: TxTransferProxy<TxBuff>,
        RH: ResponseHandler,
{
    tx: &'a mut TransmitterToSlaveController<TxBuff, TxTransfer<T, TxBuff>>,
    requests_controller: &'a mut RequestsController<RH, ResponseBodyParserImpl>,
}

impl <'a, T, TxBuff, RH>SenderImp<'a, T, TxBuff, RH>
    where
        TxBuff: ReadBuffer + BufferWriter,
        T: TxTransferProxy<TxBuff>,
        RH: ResponseHandler,
{
    fn new(tx: &'a mut TransmitterToSlaveController<TxBuff, TxTransfer<T, TxBuff>>,
           requests_controller: &'a mut RequestsController<RH, ResponseBodyParserImpl>) -> Self {
        Self {
            tx,
            requests_controller
        }
    }
}

impl <'a, T, TxBuff, RH> ControlledRequestSender for SenderImp<'a, T, TxBuff, RH>
    where
        TxBuff: ReadBuffer + BufferWriter,
        T: TxTransferProxy<TxBuff>,
        RH: ResponseHandler,
{
    fn send(&mut self, operation: Operation, instruction: DataInstructions, timestamp: RelativeMillis) -> Result<Option<u32>, Errors> {
        self.tx.send_request(operation, instruction, timestamp, self.requests_controller)
    }
}

impl <'a, T, TxBuff, RH> ErrorsSender for SenderImp<'a, T, TxBuff, RH>
    where
        TxBuff: ReadBuffer + BufferWriter,
        T: TxTransferProxy<TxBuff>,
        RH: ResponseHandler,
{
    #[inline(always)]
    fn send_error(&mut self, instruction_code: u8, error_code: ErrorCode) -> Result<(), Errors> {
        self.tx.send_error(instruction_code, error_code)
    }
}

impl <'a, T, TxBuff, RH, RP> RequestsControllerSource<RequestsController<RH, ResponseBodyParserImpl>, RP> for SenderImp<'a, T, TxBuff, RH>
    where
        TxBuff: ReadBuffer + BufferWriter,
        T: TxTransferProxy<TxBuff>,
        RH: ResponseHandler,
        RP: ResponseParser,
{
    #[inline(always)]

    fn requests_controller(&mut self) -> &mut RequestsController<RH, ResponseBodyParserImpl> {
        &mut self.requests_controller
    }
}


#[cfg(test)]
mod tests {

}
