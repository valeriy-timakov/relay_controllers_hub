#![allow(unsafe_code)]

pub mod domain;
mod parsers;
mod requests_controller;
mod signals_controller;
mod transmitter_to_slave;
mod receiver_from_slave;
mod signals_handler_proxy;

use embedded_dma::{ReadBuffer, WriteBuffer};
use crate::services::slave_controller_link::domain::{*};
use crate::errors::Errors;
use crate::hal_ext::rtc_wrapper::{RelativeMillis, RelativeSeconds };
use crate::hal_ext::serial_transfer::{ ReadableBuffer, Receiver, RxTransfer, RxTransferProxy, Sender, SerialTransfer, TxTransfer, TxTransferProxy};
use crate::services::slave_controller_link::parsers::{PayloadParserImpl, ResponseBodyParserImpl, ResponseParser, ResponsePayload, ResponsePayloadParsed, SignalPayload};
use crate::services::slave_controller_link::receiver_from_slave::ReceiverFromSlaveController;
use crate::utils::dma_read_buffer::BufferWriter;
use crate::services::slave_controller_link::requests_controller::{RequestsController, RequestsControllerRx, RequestsControllerTx, ResponseHandler, SentRequest};
use crate::services::slave_controller_link::signals_controller::{SignalControllerImpl, SignalsHandler, SignalController};
use crate::services::slave_controller_link::transmitter_to_slave::TransmitterToSlaveController;
use crate::services::slave_controller_link::receiver_from_slave::ReceiverFromSlaveControllerAbstract;



pub struct SlaveControllerLink<'a, T, R, TxBuff, RxBuff, SH, RH, EH>
    where
        TxBuff: ReadBuffer + BufferWriter,
        RxBuff: WriteBuffer + ReadableBuffer,
        T: TxTransferProxy<TxBuff>,
        R: RxTransferProxy<RxBuff>,
        SH: SignalsHandler,
        RH: ResponseHandler<ResponsePayloadParsed<'a>>,
        EH: Fn(Errors),
{
    tx: TransmitterToSlaveController<TxBuff, TxTransfer<T, TxBuff>>,
    rx: ReceiverFromSlaveController<'a,
        RxTransfer<R, RxBuff>, SignalControllerImpl<SH>, RequestsController<RH, ResponseBodyParserImpl, ResponsePayloadParsed<'a>>,
        EH, PayloadParserImpl, SignalPayload<'a>, ResponsePayload<'a>, ResponseBodyParserImpl, ResponsePayloadParsed<'a>
    >,
}


impl <'a, T, R, TxBuff, RxBuff, SH, RH, EH> SlaveControllerLink<'a, T, R,TxBuff, RxBuff, SH, RH, EH>
    where
        TxBuff: ReadBuffer + BufferWriter,
        RxBuff: WriteBuffer + ReadableBuffer,
        T: TxTransferProxy<TxBuff>,
        R: RxTransferProxy<RxBuff>,
        SH: SignalsHandler,
        RH: ResponseHandler<ResponsePayloadParsed<'a>>,
        EH: Fn(Errors),
{
    pub fn create(serial_transfer: SerialTransfer<T, R, TxBuff, RxBuff>, signals_handler: SH,
                  responses_handler: RH, receive_error_handler: EH, api_version: Version) -> Result<Self, Errors>
    {
        let (tx, rx) = serial_transfer.into();
        let response_body_parser = ResponseBodyParserImpl::create(api_version)?;
        let requests_controller = RequestsController::new(responses_handler, response_body_parser);
        let signals_controller = SignalControllerImpl::new(signals_handler);
        let payload_parser = PayloadParserImpl::new();

        let rx = ReceiverFromSlaveController::new(rx, signals_controller,
                                                          requests_controller, receive_error_handler, payload_parser);
        Ok(Self {
            tx: TransmitterToSlaveController::new(tx),
            rx,
        })
    }

    #[inline(always)]
    pub fn on_get_command<E, TS:  FnOnce() -> RelativeMillis>( &mut self) {
        self.rx.on_get_command();
    }

    #[inline(always)]
    pub fn on_rx_dma_interrupts(&mut self) {
        self.rx.inner_rx().on_dma_interrupts();
    }

    #[inline(always)]
    pub fn on_tx_dma_interrupts(&mut self) {
        self.tx.inner_tx().on_dma_interrupts();
    }
}



#[cfg(test)]
mod tests {
    use alloc::boxed::Box;
    use alloc::rc::Rc;
    use alloc::vec::Vec;
    use core::cell::{Ref, RefCell};
    use core::ops::Deref;
    use super::*;
    use quickcheck_macros::quickcheck;
    use rand::distributions::uniform::SampleBorrow;
    use rand::prelude::*;
    use crate::errors::DMAError;



/*
    struct MockRequestSender<I, F>
        where
            I: DataInstruction,
            F: FnMut(OperationCodes, I, RelativeMillis, &mut MockRequestsControllerTx) -> Result<u32, Errors>,
    {
        on_send_request: F,
        send_error_called: bool,
        senderror_params: Option<(u8, ErrorCode)>,
        send_error_result: Result<(), Errors>,
        _phantom: core::marker::PhantomData<I>,
    }

    impl <I, F> MockRequestSender<I, F>
        where
            I: DataInstruction,
            F: FnMut(OperationCodes, I, RelativeMillis, &mut MockRequestsControllerTx) -> Result<u32, Errors>,
    {
        pub fn new (on_send_request: F, send_error_result: Result<(), Errors>) -> Self {
            Self {
                on_send_request,
                send_error_called: false,
                senderror_params: None,
                send_error_result,
                _phantom: core::marker::PhantomData,
            }
        }
    }

    impl <I, F> Sender<MockTxBuffer> for MockRequestSender<I, F>
        where
            I: DataInstruction,
            F: FnMut(OperationCodes, I, RelativeMillis, &mut MockRequestsControllerTx) -> Result<u32, Errors>,
    {
        fn start_transfer<W: FnOnce(&mut MockTxBuffer) -> Result<(), Errors>>(&mut self, writter: W) -> Result<(), Errors> {
            //should never be called
            Err(Errors::OutOfRange)
        }
    }

    impl <I, F> Sender<MockTxBuffer> for Rc<MockRequestSender<I, F>>
        where
            I: DataInstruction,
            F: FnMut(OperationCodes, I, RelativeMillis, &mut MockRequestsControllerTx) -> Result<u32, Errors>,
    {
        fn start_transfer<W: FnOnce(&mut MockTxBuffer) -> Result<(), Errors>>(&mut self, writter: W) -> Result<(), Errors> {
            //should never be called
            Err(Errors::OutOfRange)
        }
    }

    impl ErrorsSender for MockRequestSender<MockIntruction, fn(OperationCodes, MockIntruction, RelativeMillis, &mut MockRequestsControllerTx) -> Result<u32, Errors>> {
        fn send_error(&mut self, instruction_code: u8, error_code: ErrorCode) -> Result<(), Errors> {
            self.send_error_called = true;
            self.senderror_params = Some((instruction_code, error_code));
            self.send_error_result
        }
    }

    impl <I, F> RequestsSender<MockRequestsControllerTx, I> for MockRequestSender<I, F>
        where
            I: DataInstruction,
            F: FnMut(OperationCodes, I, RelativeMillis, &mut MockRequestsControllerTx) -> Result<u32, Errors>,
    {

        fn send_request(&mut self, operation: OperationCodes, instruction: I, timestamp: RelativeMillis, request_controller: &mut MockRequestsControllerTx) -> Result<u32, Errors> {
            (self.on_send_request)(operation, instruction, timestamp, request_controller)
        }
    }


    struct MockRequestHandler {
        on_request_success__params__checker: Box<dyn FnMut(SentRequest) -> ()>,
        on_request_error__params__checker: Box<dyn FnMut(SentRequest, ErrorCode) -> ()>,
        on_request_parse_error__params__checker: Box<dyn FnMut(Option<SentRequest>, Errors, &[u8]) -> ()>,
        on_request_response__params__checker: Box<dyn FnMut(SentRequest, DataInstructions) -> ()>,
    }

    impl MockRequestHandler {
        fn new() -> Self {
            Self {
                on_request_success__params__checker: Box::new(|_| {}),
                on_request_error__params__checker: Box::new(|_, _| {}),
                on_request_parse_error__params__checker: Box::new(|_, _, _| {}),
                on_request_response__params__checker: Box::new(|_, _| {}),
            }
        }
    }

    impl ResponseHandler for MockRequestHandler {
        fn on_request_success(&mut self, request: SentRequest) {
            (self.on_request_success__params__checker)(request);
        }
        fn on_request_error(&mut self, request: SentRequest, error_code: ErrorCode) {
            (self.on_request_error__params__checker)(request, error_code);
        }
        fn on_request_process_error(&mut self, request: Option<SentRequest>, error: Errors, data: &[u8]) {
            (self.on_request_parse_error__params__checker)(request, error, data);
        }
        fn on_request_response(&mut self, request: SentRequest, response: DataInstructions) {
            (self.on_request_response__params__checker)(request, response);
        }

    }


    */















}