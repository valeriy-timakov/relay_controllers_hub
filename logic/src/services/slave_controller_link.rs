#![allow(unsafe_code)]

pub mod domain;
pub mod parsers;

use embedded_dma::{ReadBuffer, WriteBuffer};
use crate::services::slave_controller_link::domain::{*};
use crate::errors::Errors;
use crate::hal_ext::rtc_wrapper::{RelativeMillis, RelativeSeconds };
use crate::hal_ext::serial_transfer::{ ReadableBuffer, Receiver, RxTransfer, RxTransferProxy, Sender, SerialTransfer, TxTransfer, TxTransferProxy};
use crate::services::slave_controller_link::parsers::{ResponsesParser, RequestsParserImpl, SignalsParser, SignalsParserImpl};
use crate::utils::dma_read_buffer::BufferWriter;

#[derive(Copy, Clone, PartialEq, Debug)]
pub struct SignalData {
    instruction: Signals,
    relay_signal_data: Option<RelaySignalData>,
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct RelaySignalData {
    relative_timestamp: RelativeSeconds,
    relay_idx: u8,
    is_on: bool,
    is_called_internally: Option<bool>,
}

const MAX_REQUESTS_COUNT: usize = 4;


#[derive(PartialEq, Debug, Copy, Clone)]
pub struct SentRequest {
    id: u32,
    operation: OperationCodes,
    instruction: DataInstructionCodes,
    rel_timestamp: RelativeMillis,
}

impl SentRequest {
    fn new(operation: OperationCodes, instruction: DataInstructionCodes, rel_timestamp: RelativeMillis) -> Self {
        Self {
            id: 0,
            operation,
            instruction,
            rel_timestamp
        }
    }
}

pub trait SignalsHandler {
    fn on_signal(&mut self, signal_data: SignalData);
    fn on_signal_error(&mut self, instruction: Signals, error: Errors, sent_to_slave_success: bool);
}

pub trait ResponseHandler {
    fn on_request_success(&mut self, request: SentRequest);
    fn on_request_error(&mut self, request: SentRequest, error_code: ErrorCode);
    fn on_request_process_error(&mut self, request: Option<SentRequest>, error: Errors, data: &[u8]);
    fn on_request_response(&mut self, request: SentRequest, response: DataInstructions);
}

trait RequestsControllerTx {
    fn add_sent_request(&mut self, request: SentRequest) -> u32;
    fn check_request(&self, instruction: DataInstructionCodes) -> Result<(), Errors>;
}

trait RequestsControllerRx {
    fn process_response(&mut self, operation_code: u8, data: &[u8]);
    fn is_response(&self, operation_code: u8) -> bool {
        operation_code == OperationCodes::Success as u8 || operation_code == OperationCodes::Response as u8 || operation_code == OperationCodes::Error as u8 ||
            operation_code == OperationCodes::SuccessV2 as u8 || operation_code == OperationCodes::ResponseV2 as u8 || operation_code == OperationCodes::ErrorV2 as u8
    }
}

struct RequestsController<RH: ResponseHandler, RP: ResponsesParser> {
    sent_requests: [Option<SentRequest>; MAX_REQUESTS_COUNT],
    requests_count: usize,
    request_needs_cache_send: bool,
    response_handler: RH,
    requests_parser: RP,
    last_request_id: u32,
}

impl <RH: ResponseHandler, RP: ResponsesParser> RequestsController<RH, RP> {
    fn new(response_handler: RH, requests_parser: RP) -> Self {
        Self {
            sent_requests: [None, None, None, None],
            requests_count: 0,
            request_needs_cache_send: false,
            response_handler,
            requests_parser,
            last_request_id: 0,
        }
    }
}

impl <RH: ResponseHandler, RP: ResponsesParser> RequestsControllerTx for RequestsController<RH, RP> {

    #[inline(always)]
    fn check_request(&self, instruction_code: DataInstructionCodes) -> Result<(), Errors> {
        if self.requests_count == MAX_REQUESTS_COUNT {
            return Err(Errors::RequestsLimitReached);
        }
        if self.requests_parser.request_needs_cache(instruction_code) && self.request_needs_cache_send {
            return Err(Errors::RequestsNeedsCacheAlreadySent);
        }
        Ok(())
    }

    #[inline(always)]
    fn add_sent_request(&mut self, request: SentRequest) -> u32 {
        if self.requests_parser.request_needs_cache(request.instruction) {
            self.request_needs_cache_send = true;
        }
        self.sent_requests[self.requests_count] = Some(request);
        self.requests_count += 1;
        self.last_request_id += 1;
        self.last_request_id
    }

}

impl <RH: ResponseHandler, RP: ResponsesParser> RequestsControllerRx for RequestsController<RH, RP> {

    fn process_response(&mut self, operation_code: u8, data: &[u8]) {
        let instruction_code = data[0];
        let data = &data[1..];
        if self.requests_count > 0 {

            let (search_operation, version) =
                if operation_code == OperationCodes::Success as u8 {
                    (OperationCodes::Set, 1)
                } else if operation_code == OperationCodes::Response as u8 {
                    (OperationCodes::Read, 1)
                } else if operation_code == OperationCodes::Error as u8 {
                    (OperationCodes::Error, 1)
                } else if operation_code == OperationCodes::SuccessV2 as u8 {
                    (OperationCodes::Set, 2)
                } else if operation_code == OperationCodes::ResponseV2 as u8 {
                    (OperationCodes::Read, 2)
                } else if operation_code == OperationCodes::ErrorV2 as u8 {
                    (OperationCodes::Error, 2)
                } else {
                    (OperationCodes::None, 0)
                };

            let (id, data) = if version == 2 {
                if data.len() > 4 {
                    let id = self.requests_parser.parse_u32(data);
                    (Some(id), &data[4..])
                } else {
                    (None, data)
                }
            } else {
                (None, data)
            };

            for i in (0..self.requests_count).rev() {
                if let Some(request) = self.sent_requests[i].as_ref() {
                    if request.instruction as u8 == instruction_code && request.operation == search_operation {
                        if operation_code == OperationCodes::Success as u8 {
                            self.response_handler.on_request_success(self.sent_requests[i].unwrap());
                        } else if operation_code == OperationCodes::Error as u8 {
                            self.response_handler.on_request_error(self.sent_requests[i].unwrap(), ErrorCode::for_code(instruction_code));
                        } else {
                            match self.requests_parser.parse_response(instruction_code, data) {
                                Ok(response) => {
                                    self.response_handler.on_request_response(self.sent_requests[i].unwrap(), response);
                                }
                                Err(error) => {
                                    self.response_handler.on_request_process_error(self.sent_requests[i], error, data);
                                }
                            }
                        }
                        if operation_code == OperationCodes::Response as u8 && self.requests_parser.request_needs_cache(request.instruction) {
                            self.request_needs_cache_send = false;
                        }
                        let mut next_pos = i + 1;
                        while next_pos < self.requests_count {
                            self.sent_requests.swap(next_pos - 1, next_pos);
                            next_pos += 1;
                        }
                        self.sent_requests[next_pos - 1] = None;
                        self.requests_count -= 1;
                    }
                }
            }
        }
        self.response_handler.on_request_process_error(None, Errors::NoRequestsFound, data);
    }

}

trait ControlledRequestSender {
    fn send(&mut self, operation: OperationCodes, instruction: DataInstructions, timestamp: RelativeMillis) -> Result<(), Errors>;
}

struct SignalsHandlerProxy<'a, SH, TS, S>
    where
        SH: SignalsHandler,
        TS: Fn() -> RelativeMillis,
        S: ControlledRequestSender + ErrorsSender,
{
    handler: SH,
    time_source: TS,
    tx:  &'a mut S,
}

impl <'a, SH, TS, S> SignalsHandlerProxy<'a, SH, TS, S>
    where
        SH: SignalsHandler,
        TS: Fn() -> RelativeMillis,
        S: ControlledRequestSender + ErrorsSender,
{
    fn new(handler: SH, time_source: TS, tx:  &'a mut S) -> Self {
        Self {
            handler, time_source, tx
        }
    }
}

impl  <'a, SH, TS, S> SignalsHandler for SignalsHandlerProxy<'a, SH, TS, S>
    where
        SH: SignalsHandler,
        TS: Fn() -> RelativeMillis,
        S: ControlledRequestSender + ErrorsSender,
{

    fn on_signal(&mut self, signal_data: SignalData) {
        if signal_data.instruction == Signals::GetTimeStamp {
            let timestamp = (self.time_source)();
            let res = self.tx.send(
                OperationCodes::Set,
                DataInstructions::RemoteTimestamp(Conversation::Data(timestamp.seconds())),
                timestamp);
            if res.is_err() {
                self.handler.on_signal_error(signal_data.instruction, res.err().unwrap(), false);
            }
        } else {
            self.handler.on_signal(signal_data);
        }
    }

    fn on_signal_error(&mut self, instruction: Signals, error: Errors, _: bool) {
        let error_code = ErrorCode::for_error(error);
        let sent_to_slave_success = self.tx.send_error(instruction as u8, error_code).is_ok();
        self.handler.on_signal_error(instruction, error, sent_to_slave_success);
    }

}

pub struct SlaveControllerLink<T, R, TxBuff, RxBuff, SH, RH, EH>
    where
        TxBuff: ReadBuffer + BufferWriter,
        RxBuff: WriteBuffer + ReadableBuffer,
        T: TxTransferProxy<TxBuff>,
        R: RxTransferProxy<RxBuff>,
        SH: SignalsHandler,
        RH: ResponseHandler,
        EH: Fn(Errors),
{
    tx: TransmitterToSlaveController<TxBuff, TxTransfer<T, TxBuff>>,
    rx: ReceiverFromSlaveController<RxTransfer<R, RxBuff>, SignalControllerImpl<SH, SignalsParserImpl>, RequestsController<RH, RequestsParserImpl>, EH>,
}


impl <T, R, TxBuff, RxBuff, SH, RH, EH> SlaveControllerLink<T, R,TxBuff, RxBuff, SH, RH, EH>
    where
        TxBuff: ReadBuffer + BufferWriter,
        RxBuff: WriteBuffer + ReadableBuffer,
        T: TxTransferProxy<TxBuff>,
        R: RxTransferProxy<RxBuff>,
        SH: SignalsHandler,
        RH: ResponseHandler,
        EH: Fn(Errors),
{
    pub fn create(serial_transfer: SerialTransfer<T, R, TxBuff, RxBuff>, signals_handler: SH,
                  responses_handler: RH, receive_error_handler: EH) -> Result<Self, Errors>
    {
        let (tx, rx) = serial_transfer.into();
        let requests_parser = RequestsParserImpl::create()?;
        let requests_controller = RequestsController::new(responses_handler, requests_parser);
        let signals_parser = SignalsParserImpl::new();
        let signals_controller = SignalControllerImpl::new(signals_handler, signals_parser);

        Ok(Self {
            tx: TransmitterToSlaveController::new(tx),
            rx: ReceiverFromSlaveController::new(rx, signals_controller, requests_controller, receive_error_handler),
        })
    }

    #[inline(always)]
    pub fn on_get_command<E, TS:  FnOnce() -> RelativeMillis>( &mut self) {
        self.rx.on_get_command();
    }

    #[inline(always)]
    pub fn on_rx_dma_interrupts(&mut self) {
        self.rx.rx.on_dma_interrupts();
    }

    #[inline(always)]
    pub fn on_tx_dma_interrupts(&mut self) {
        self.tx.tx.on_dma_interrupts();
    }
}

trait RequestsSender<RCT, I>
    where
        RCT: RequestsControllerTx,
        I: DataInstruction,
{
    fn send_request(&mut self, operation: OperationCodes, instruction: I, timestamp: RelativeMillis, request_controller: &mut RCT) -> Result<u32, Errors>;
}

trait ErrorsSender {
    fn send_error(&mut self, instruction_code: u8, error_code: ErrorCode) -> Result<(), Errors>;
}

struct TransmitterToSlaveController<TxBuff, S>
    where
        TxBuff: ReadBuffer + BufferWriter,
        S: Sender<TxBuff>,
{
    tx: S,
    _phantom: core::marker::PhantomData<TxBuff>,
}

impl <TxBuff, S> TransmitterToSlaveController<TxBuff, S>
    where
        TxBuff: ReadBuffer + BufferWriter,
        S: Sender<TxBuff>,
{
    pub fn new (tx: S) -> Self {
        Self {
            tx,
            _phantom: core::marker::PhantomData
        }
    }

}

impl <TxBuff, S> Sender<TxBuff> for TransmitterToSlaveController<TxBuff, S>
    where
        TxBuff: ReadBuffer + BufferWriter,
        S: Sender<TxBuff>,
{
    #[inline(always)]
    fn start_transfer<F: FnOnce(&mut TxBuff) -> Result<(), Errors>>(&mut self, writter: F) -> Result<(), Errors> {
        self.tx.start_transfer(writter)
    }
}

impl <TxBuff, S> ErrorsSender for TransmitterToSlaveController<TxBuff, S>
    where
        TxBuff: ReadBuffer + BufferWriter,
        S: Sender<TxBuff>,
{
    fn send_error(&mut self, instruction_code: u8, error_code: ErrorCode) -> Result<(), Errors> {
        self.start_transfer(|buffer| {
            buffer.clear();
            buffer.add_u8(OperationCodes::None as u8)?;
            buffer.add_u8(OperationCodes::Error as u8)?;
            buffer.add_u8(instruction_code)?;
            buffer.add_u8(error_code.discriminant())
        })
    }
}

impl <TxBuff, RCT, S, I> RequestsSender<RCT, I> for TransmitterToSlaveController<TxBuff, S>
    where
        TxBuff: ReadBuffer + BufferWriter,
        RCT: RequestsControllerTx,
        S: Sender<TxBuff>,
        I: DataInstruction,
{
    fn send_request(&mut self, operation: OperationCodes, instruction: I, timestamp: RelativeMillis, request_controller: &mut RCT) -> Result<u32, Errors> {

        request_controller.check_request(instruction.code())?;

        self.start_transfer(|buffer| {
            buffer.clear();
            buffer.add_u8(OperationCodes::None as u8)?;
            buffer.add_u8(operation as u8)?;
            buffer.add_u8(instruction.code() as u8)?;
            instruction.serialize(buffer)
        })?;

        Ok(request_controller.add_sent_request(SentRequest::new(operation, instruction.code(), timestamp)))
    }
}

trait SignalController {
    fn process_signal(&mut self, data: &[u8]);

}

trait ReceiverFromSlaveControllerAbstract<Rc, SC, RCR, EH>
    where
        Rc: Receiver,
        SC: SignalController,
        RCR: RequestsControllerRx,
        EH: Fn(Errors),
{

    fn slice(&mut self) -> (&mut Rc, &mut SC, &mut RCR);
    fn error_handler(&mut self) -> &mut EH;

    fn on_get_command(&mut self) {
        let (rx, signal_controller, request_controller) = self.slice();
        let res = rx.on_rx_transfer_interrupt(|data| {
            if data.len() >= 2 {
                if data[0] == OperationCodes::None as u8 {
                    let operation_code = data[1];
                    let data = &data[2..];
                    if operation_code == OperationCodes::Signal as u8 {
                        signal_controller.process_signal(data);
                        Ok(())
                    } else if request_controller.is_response(operation_code) {
                        request_controller.process_response(operation_code, data);
                        Ok(())
                    } else {
                        Err(Errors::OperationNotRecognized(operation_code))
                    }
                } else {
                    Err(Errors::CommandDataCorrupted)
                }
            } else {
                Err(Errors::NotEnoughDataGot)
            }
        });
        if res.is_err() {
            (self.error_handler())(res.err().unwrap());
        }
    }
}

struct SignalControllerImpl<SH, SP>
    where
        SH: SignalsHandler,
        SP: SignalsParser,
{
    signal_handler: SH,
    signal_parser: SP,
}

impl <SH, SP> SignalControllerImpl<SH, SP>
    where
        SH: SignalsHandler,
        SP: SignalsParser,
{
    pub fn new(signal_handler: SH, signal_parser: SP) -> Self {
        Self { signal_handler, signal_parser }
    }
}

impl <SH, SP> SignalController for SignalControllerImpl<SH, SP>
    where
        SH: SignalsHandler,
        SP: SignalsParser,
{
    fn process_signal(&mut self, data: &[u8]) {

        if data.len() < 1 {
            self.signal_handler.on_signal_error(Signals::Unknown, Errors::InvalidDataSize, false);
            return;
        }

        let instruction_code = data[0];
        let data = &data[1..];

        match self.signal_parser.parse_instruction(instruction_code) {
            Some(instruction) => {
                match self.signal_parser.parse(instruction, data) {
                    Ok(signal_data) => {
                        self.signal_handler.on_signal(signal_data);
                    }
                    Err(error) => {
                        self.signal_handler.on_signal_error(instruction, error, false);
                    }
                }
            }
            None => {
                self.signal_handler.on_signal_error(Signals::Unknown, Errors::InstructionNotRecognized(instruction_code), false);
            }
        }
    }
}

struct ReceiverFromSlaveController<Rc, SC, RCR, EH>
    where
        Rc: Receiver,
        SC: SignalController,
        RCR: RequestsControllerRx,
        EH: Fn(Errors),
{
    rx: Rc,
    signal_controller: SC,
    requests_controller_rx: RCR,
    error_handler: EH,
}

impl <Rc, SC, RCR, EH> ReceiverFromSlaveController<Rc, SC, RCR, EH>
    where
        Rc: Receiver,
        SC: SignalController,
        RCR: RequestsControllerRx,
        EH: Fn(Errors),
{
    pub fn new(rx: Rc, signal_controller: SC, requests_controller_rx: RCR, error_handler: EH) -> Self {
        Self {
            rx,
            signal_controller,
            requests_controller_rx,
            error_handler,
        }
    }
}

impl <Rc, RCR, SC, EH> ReceiverFromSlaveControllerAbstract<Rc, SC, RCR, EH> for ReceiverFromSlaveController<Rc, SC, RCR, EH>
    where
        Rc: Receiver,
        SC: SignalController,
        RCR: RequestsControllerRx,
        EH: Fn(Errors),
{
    #[inline(always)]
    fn slice(&mut self) ->( &mut Rc, &mut SC, &mut RCR) {
        (&mut self.rx, &mut self.signal_controller, &mut self.requests_controller_rx)
    }

    fn error_handler(&mut self) -> &mut EH {
        &mut self.error_handler
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

    #[test]
    fn test_send_error() {
        let start_transfer_result = Ok(());
        let sending_error: ErrorCode = ErrorCode::EInstructionUnrecognized;
        let instruction_code = Signals::MonitoringStateChanged as u8;

        let mock = Rc::new(RefCell::new(MockSender::new(true, start_transfer_result)));

        let mut tested = TransmitterToSlaveController::new(mock.clone());

        let result = tested.send_error(instruction_code, sending_error);

        assert_eq!(true, mock.borrow().start_transfer_called);
        assert_eq!(start_transfer_result, result);
        assert_eq!(true, mock.borrow().buffer.cleared);
        assert_eq!(4, mock.borrow().buffer.add_ua_arguments.len());
        assert_eq!(OperationCodes::None as u8, mock.borrow().buffer.add_ua_arguments[0]);
        assert_eq!(OperationCodes::Error as u8, mock.borrow().buffer.add_ua_arguments[1]);
        assert_eq!(instruction_code, mock.borrow().buffer.add_ua_arguments[2]);
        assert_eq!(sending_error.discriminant(), mock.borrow().buffer.add_ua_arguments[3]);
    }

    #[test]
    fn test_send_error_code_proxy() {
        let start_transfer_result = Ok(());

        let sending_errors = [
            ErrorCode::ERequestDataNoValue,
            ErrorCode::EInstructionUnrecognized,
            ErrorCode::ECommandEmpty,
            ErrorCode::ECommandSizeOverflow,
            ErrorCode::EInstructionWrongStart,
            ErrorCode::EWriteMaxAttemptsExceeded,
            ErrorCode::EUndefinedOperation,
            ErrorCode::ERelayCountOverflow,
            ErrorCode::ERelayCountAndDataMismatch,
            ErrorCode::ERelayIndexOutOfRange,
            ErrorCode::ESwitchCountMaxValueOverflow,
            ErrorCode::EControlInterruptedPinNotAllowedValue,
            ErrorCode::ERelayNotAllowedPinUsed,
            ErrorCode::EUndefinedCode(0),
            ErrorCode::EUndefinedCode(1),
            ErrorCode::EUndefinedCode(2),
            ErrorCode::EUndefinedCode(3),
            ErrorCode::EUndefinedCode(127),
            ErrorCode::EUndefinedCode(255),
        ];
        let mock = Rc::new(RefCell::new(MockSender::new(true, start_transfer_result)));

        let mut tested = TransmitterToSlaveController::new(mock.clone());

        for instruction_code in 0 .. 100 {
            for sending_error in sending_errors {
                tested.send_error(instruction_code, sending_error).unwrap();
                assert_eq!(4, mock.borrow().buffer.add_ua_arguments.len());
                assert_eq!(instruction_code, mock.borrow().buffer.add_ua_arguments[2]);
                assert_eq!(sending_error.discriminant(), mock.borrow().buffer.add_ua_arguments[3]);
            }
        }
    }

    #[test]
    fn test_send_request() {
        let start_transfer_result = Ok(());
        let operation = OperationCodes::Set;
        let instruction_id = 12385249;
        let instruction = Rc::new(MockIntruction::new(instruction_id, DataInstructionCodes::Id, Ok(())));
        let instruction_code = instruction.code();
        let mut rng = rand::thread_rng();
        let timestamp = RelativeMillis::new(rng.gen_range(1..u32::MAX));

        let mock = Rc::new(RefCell::new(MockSender::new(true, start_transfer_result)));
        let mut tested = TransmitterToSlaveController::new(mock.clone());

        let add_sent_request_result = rng.gen_range(1..u32::MAX);
        let mut mock_request_controller = MockRequestsControllerTx::new(Ok(()), add_sent_request_result);

        let result = tested.send_request(operation, instruction.clone(), timestamp, &mut mock_request_controller);

        assert_eq!(true, *mock_request_controller.check_request_result_called.borrow());
        assert_eq!(Ok(add_sent_request_result), result);
        //check add request operation
        assert_eq!(true, mock_request_controller.add_sent_request_called);
        assert!(mock_request_controller.add_sent_request_parameter.is_some());
        assert_eq!(operation, mock_request_controller.add_sent_request_parameter.as_ref().unwrap().operation);
        assert_eq!(timestamp, mock_request_controller.add_sent_request_parameter.as_ref().unwrap().rel_timestamp);
        assert_eq!(instruction_code, mock_request_controller.add_sent_request_parameter.as_ref().unwrap().instruction);
        //check buffer operations
        assert_eq!(true, mock.borrow().buffer.cleared);
        assert!(3 <= mock.borrow().buffer.add_ua_arguments.len());
        assert_eq!(OperationCodes::None as u8, mock.borrow().buffer.add_ua_arguments[0]);
        assert_eq!(operation as u8, mock.borrow().buffer.add_ua_arguments[1]);
        assert_eq!(instruction_code as u8, mock.borrow().buffer.add_ua_arguments[2]);
        assert_eq!(true, *instruction.serialize_called.borrow());
    }

    #[test]
    fn test_send_request_returns_all_check_request_result_errors() {
        let start_transfer_result = Ok(());
        let operation = OperationCodes::Set;
        let mut rng = rand::thread_rng();
        let timestamp = RelativeMillis::new(rng.gen_range(1..u32::MAX));

        let mock = Rc::new(RefCell::new(MockSender::new(true, start_transfer_result)));
        let mut tested = TransmitterToSlaveController::new(mock.clone());

        let errors = [
            Errors::RequestsLimitReached,
            Errors::RequestsNeedsCacheAlreadySent,
        ];

        let mut mock_request_controller = MockRequestsControllerTx::new(Ok(()), 0);

        for error in errors {
            let instruction = DataInstructions::Id(Conversation::Data(rng.gen_range(1..u32::MAX)));
            *mock_request_controller.check_request_result_called.borrow_mut() = false;
            mock_request_controller.check_request_result = Err(error);
            let result = tested.send_request(operation, instruction, timestamp, &mut mock_request_controller);
            assert_eq!(true, *mock_request_controller.check_request_result_called.borrow());
            assert_eq!(false, mock_request_controller.add_sent_request_called);
            assert_eq!(Err(error), result);
        }
    }

    #[test]
    fn test_send_request_returns_all_start_transfer_errors() {
        let start_transfer_result = Ok(());
        let operation = OperationCodes::Set;
        let mut rng = rand::thread_rng();
        let timestamp = RelativeMillis::new(rng.gen_range(1..u32::MAX));

        let mock = Rc::new(RefCell::new(MockSender::new(true, start_transfer_result)));
        let mut tested = TransmitterToSlaveController::new(mock.clone());

        let errors = [
            Errors::TransferInProgress,
            Errors::NoBufferAvailable,
            Errors::DmaError(DMAError::Overrun(())),
            Errors::DmaError(DMAError::NotReady(())),
            Errors::DmaError(DMAError::SmallBuffer(())),
        ];

        let mut mock_request_controller = MockRequestsControllerTx::new(
            Ok(()), 0);
        for error in errors {
            let instruction = DataInstructions::Id(Conversation::Data(rng.gen_range(1..u32::MAX)));
            mock.borrow_mut().start_transfer_result = Err(error);
            let result =
                tested.send_request(operation, instruction, timestamp, &mut mock_request_controller);
            assert_eq!(false, mock_request_controller.add_sent_request_called);
            assert_eq!(Err(error), result);
        }
    }

    #[test]
    fn test_signal_controller_should_report_error_on_empty_data() {
        let mut signal_controller =
            SignalControllerImpl::new(MockSignalsHandler::new(),
                MockSignalsParser::new(Err(Errors::CommandDataCorrupted),
                                       None));
        let data = [];

        signal_controller.process_signal(&data);

        assert_eq!(Some((Signals::Unknown, Errors::InvalidDataSize, false)),
                   signal_controller.signal_handler.on_signal_error__params);
        //should not call other methods
        assert_eq!(None, signal_controller.signal_handler.on_signal__signal_data);
        assert_eq!(None, signal_controller.signal_parser.test_data.borrow().parse_instruction_params);
        assert_eq!(None, signal_controller.signal_parser.test_data.borrow().parse_params);
    }

    #[test]
    fn test_signal_controller_should_try_to_parse_signal_code_and_report_error() {
        let mock_signal_parser = MockSignalsParser::new(
            Err(Errors::CommandDataCorrupted), None);
        let mut signal_controller =
            SignalControllerImpl::new( MockSignalsHandler::new(), mock_signal_parser);
        let signal_code = Signals::MonitoringStateChanged as u8;
        let data = [signal_code, 1, 2, 3, 4, 5, 6, 7, 8, 9];

        signal_controller.process_signal(&data);

        assert_eq!(Some((Signals::Unknown, Errors::InstructionNotRecognized(signal_code), false)),
                   signal_controller.signal_handler.on_signal_error__params);
        //should call parse signal code
        assert_eq!(Some(signal_code as u8), signal_controller.signal_parser.test_data.borrow().parse_instruction_params);
        //should not call other methods
        assert_eq!(None, signal_controller.signal_parser.test_data.borrow().parse_params);
        assert_eq!(None, signal_controller.signal_handler.on_signal__signal_data);
    }

    #[test]
    fn test_signal_controller_should_try_parse_signal_body_and_report_error_if_any() {
        let signal = Signals::MonitoringStateChanged;
        let signal_parse_error = Errors::CommandDataCorrupted;
        let mock_signal_parser = MockSignalsParser::new(
            Err(signal_parse_error), Some(signal));
        let mut signal_controller =
            SignalControllerImpl::new( MockSignalsHandler::new(), mock_signal_parser);
        let signal_code = Signals::MonitoringStateChanged as u8;
        let data = [signal as u8, 1, 2, 3, 4, 5, 6, 7, 8, 9];

        signal_controller.process_signal(&data);

        assert_eq!(Some((signal, signal_parse_error, false)),
                   signal_controller.signal_handler.on_signal_error__params);
        //should call parse signal code
        assert_eq!(Some(signal as u8), signal_controller.signal_parser.test_data.borrow().parse_instruction_params);
        //should call parse signal body
        assert_eq!(Some((signal, (&data[1..]).to_vec())), signal_controller.signal_parser.test_data.borrow().parse_params);
        //should not call other methods
        assert_eq!(None, signal_controller.signal_handler.on_signal__signal_data);

    }

    #[test]
    fn test_signal_controller_should_proxy_parsed_signal_on_success() {
        let signal = Signals::MonitoringStateChanged;
        let signal_parse_error = Errors::CommandDataCorrupted;
        let mut rng = rand::thread_rng();
        let parsed_signal_data = SignalData {
            instruction: signal,
            relay_signal_data: Some(RelaySignalData {
                relative_timestamp: RelativeSeconds::new(rng.gen_range(1..u32::MAX)),
                relay_idx: rng.gen_range(0..15),
                is_on: rng.gen_range(0..1) == 1,
                is_called_internally: Some(rng.gen_range(0..1) == 1),
            }),
        };
        let mock_signal_parser = MockSignalsParser::new(Ok(parsed_signal_data), Some(signal));
        let mut signal_controller =
            SignalControllerImpl::new( MockSignalsHandler::new(), mock_signal_parser);
        let signal_code = Signals::MonitoringStateChanged as u8;
        let data = [signal as u8, 1, 2, 3, 4, 5, 6, 7, 8, 9];

        signal_controller.process_signal(&data);

        assert_eq!(Some(parsed_signal_data), signal_controller.signal_handler.on_signal__signal_data);
        //should call parse signal code
        assert_eq!(Some(signal as u8), signal_controller.signal_parser.test_data.borrow().parse_instruction_params);
        //should call parse signal body
        assert_eq!(Some((signal, (&data[1..]).to_vec())), signal_controller.signal_parser.test_data.borrow().parse_params);
        //should not call other methods
        assert_eq!(None, signal_controller.signal_handler.on_signal_error__params);

    }

    #[test]
    fn test_on_get_command_should_report_error_not_enough_data_error_on_low_bytes_message() {

        let datas = Vec::from([[].to_vec(), [1].to_vec()]);

        for data in datas {
            let mock_receiver = MockReceiver::new(data);
            let mut handled_error: RefCell<Option<Errors>> = RefCell::new(None);
            let mock_error_handler = |error: Errors| {
                *handled_error.borrow_mut() = Some(error);
            };
            let mut controller = ReceiverFromSlaveController::new(
                mock_receiver, MockSignalController::new(),
                MockRequestsControllerRx::new(false), &mock_error_handler);


            controller.on_get_command();

            assert_eq!(true, controller.rx.on_rx_transfer_interrupt_called);
            assert_eq!(Err(Errors::NotEnoughDataGot), controller.rx.receiver_result.unwrap());
            assert_eq!(Some(Errors::NotEnoughDataGot), *handled_error.borrow());
            //nothing other should be called
            assert_eq!(None, controller.signal_controller.process_signal_params);
            assert_eq!(None, controller.requests_controller_rx.process_response_params);
        }
    }

    #[test]
    fn test_on_get_command_should_return_corrupted_data_error_on_starting_not_0() {
        let mock_receiver = MockReceiver::new( [1, 2, 3].to_vec() );
        let mut handled_error: RefCell<Option<Errors>> = RefCell::new(None);
        let mock_error_handler = |error: Errors| {
            *handled_error.borrow_mut() = Some(error);
        };
        let mut controller = ReceiverFromSlaveController::new(
            mock_receiver, MockSignalController::new(),
            MockRequestsControllerRx::new(false), &mock_error_handler);

        controller.on_get_command();

        assert_eq!(true, controller.rx.on_rx_transfer_interrupt_called);
        assert_eq!(Err(Errors::CommandDataCorrupted), controller.rx.receiver_result.unwrap());
        assert_eq!(Some(Errors::CommandDataCorrupted), *handled_error.borrow());
        //nothing other should be called
        assert_eq!(None, controller.signal_controller.process_signal_params);
        assert_eq!(None, controller.requests_controller_rx.process_response_params);
    }

    #[test]
    fn test_on_get_command_should_renurn_not_recognized_on_unknown() {
        let not_request_not_signal_operations = [OperationCodes::Unknown as u8, OperationCodes::None as u8,
            OperationCodes::Set as u8, OperationCodes::Read as u8, OperationCodes::Command as u8, 12, 56];

        for operation_code in not_request_not_signal_operations {
            let mock_receiver = MockReceiver::new( [OperationCodes::None as u8, operation_code, 0].to_vec() );
            let mut handled_error: RefCell<Option<Errors>> = RefCell::new(None);
            let mock_error_handler = |error: Errors| {
                *handled_error.borrow_mut() = Some(error);
            };
            let mut controller = ReceiverFromSlaveController::new(
                mock_receiver, MockSignalController::new(),
                MockRequestsControllerRx::new(false), &mock_error_handler);

            controller.on_get_command();

            assert_eq!(true, controller.rx.on_rx_transfer_interrupt_called);
            assert_eq!(Some(operation_code), *controller.requests_controller_rx.is_response_param.borrow());
            assert_eq!(Err(Errors::OperationNotRecognized(operation_code)), controller.rx.receiver_result.unwrap());
            assert_eq!(Some(Errors::OperationNotRecognized(operation_code)), *handled_error.borrow());
            //nothing other should be called
            assert_eq!(None, controller.signal_controller.process_signal_params);
            assert_eq!(None, controller.requests_controller_rx.process_response_params);
        }
    }

    #[test]
    fn test_on_get_command_should_call_request_controller_on_response_operations() {
        let response_operations = [OperationCodes::Response as u8, OperationCodes::Success as u8, OperationCodes::Error as u8];

        for operation_code in response_operations {
            for instruction_code in 0..100 {

                let mock_receiver = MockReceiver::new(
                    [OperationCodes::None as u8, operation_code, instruction_code, 1, 2, 3].to_vec() );
                let mut handled_error: RefCell<Option<Errors>> = RefCell::new(None);
                let mock_error_handler = |error: Errors| {
                    *handled_error.borrow_mut() = Some(error);
                };
                let mut controller = ReceiverFromSlaveController::new(
                    mock_receiver, MockSignalController::new(),
                    MockRequestsControllerRx::new(true), &mock_error_handler);

                controller.on_get_command();

                assert_eq!(true, controller.rx.on_rx_transfer_interrupt_called);
                assert_eq!(Some(operation_code), *controller.requests_controller_rx.is_response_param.borrow());
                assert_eq!(Some((operation_code, [instruction_code, 1, 2, 3].to_vec())),
                           controller.requests_controller_rx.process_response_params);
                assert_eq!(Ok(()), controller.rx.receiver_result.unwrap());
                //nothing other should be called
                assert_eq!(None, controller.signal_controller.process_signal_params);
                assert_eq!(None, *handled_error.borrow());
            }
        }
    }

    #[test]
    fn test_on_get_command_should_proxy_signals() {
        let operation_code = OperationCodes::Signal as u8;

        for instruction_code in 0..50 {
            let data = [OperationCodes::None as u8, operation_code, instruction_code, 1, 2, 3, 4];
            let mock_receiver = MockReceiver::new(data.to_vec() );
            let mut handled_error: RefCell<Option<Errors>> = RefCell::new(None);
            let mock_error_handler = |error: Errors| {
                *handled_error.borrow_mut() = Some(error);
            };
            let mut controller = ReceiverFromSlaveController::new(
                mock_receiver, MockSignalController::new(),
                MockRequestsControllerRx::new(true), &mock_error_handler);

            controller.on_get_command();

            assert_eq!(true, controller.rx.on_rx_transfer_interrupt_called);
            assert_eq!(Ok(()), controller.rx.receiver_result.unwrap());
            assert_eq!(Some((&data[2..]).to_vec().to_vec()), controller.signal_controller.process_signal_params);
            //nothing other should be called
            assert_eq!(None, *handled_error.borrow());
            assert_eq!(None, controller.requests_controller_rx.process_response_params);
        }
    }

    #[test]
    fn test_signals_proxy_sends_timestamp() {

        let mock_signals_handler = Rc::new(RefCell::new(MockSignalsHandler::new()));
        let time_source_called = RefCell::new(false);
        let mut rng = rand::thread_rng();
        let timestamp = RelativeMillis::new(rng.gen_range(1..u32::MAX));
        let mock_time_source = || {
            *time_source_called.borrow_mut() = true;
            timestamp
        };
        let mut mock_tx = MockControlledSender::new(Ok(()), Ok(()));

        let mut proxy = SignalsHandlerProxy::new(
            mock_signals_handler.clone(),
            mock_time_source,
            &mut mock_tx
        );

        let data = SignalData {
            instruction: Signals::GetTimeStamp,
            relay_signal_data: None,
        };

        proxy.on_signal(data);

        assert_eq!(true, *time_source_called.borrow());
        assert_eq!(true, mock_tx.send_called);
        assert_eq!(
            Some((
                OperationCodes::Set,
                DataInstructions::RemoteTimestamp(Conversation::Data(timestamp.seconds())),
                timestamp)),
            mock_tx.send_params);
        assert_eq!(true, *time_source_called.borrow());
        assert_eq!(None, mock_signals_handler.borrow().on_signal__signal_data);
        assert_eq!(None, mock_signals_handler.borrow().on_signal_error__params);

    }

    #[test]
    fn test_signals_proxy_proxies_send_timestamp_errors_to_handler() {

        let mock_signals_handler = Rc::new(RefCell::new(MockSignalsHandler::new()));
        let time_source_called = RefCell::new(false);
        let mut rng = rand::thread_rng();
        let timestamp = RelativeMillis::new(rng.gen_range(1..u32::MAX));
        let mock_time_source = || {
            *time_source_called.borrow_mut() = true;
            timestamp
        };

        let data = SignalData {
            instruction: Signals::GetTimeStamp,
            relay_signal_data: None,
        };

        let errors = [Errors::OutOfRange, Errors::NoBufferAvailable, Errors::DmaError(DMAError::Overrun(())),
            Errors::DmaError(DMAError::NotReady(())), Errors::DmaError(DMAError::SmallBuffer(())),
            Errors::NotEnoughDataGot, Errors::InvalidDataSize, Errors::DmaBufferOverflow];

        for error in errors {
            let mut mock_tx = MockControlledSender::new(Err(error), Ok(()));

            let mut proxy = SignalsHandlerProxy::new(
                mock_signals_handler.clone(),
                mock_time_source,
                &mut mock_tx
            );

            proxy.on_signal(data);

            assert_eq!(true, *time_source_called.borrow());
            assert_eq!(true, mock_tx.send_called);
            assert_eq!(
                Some((
                    OperationCodes::Set,
                    DataInstructions::RemoteTimestamp(Conversation::Data(timestamp.seconds())),
                    timestamp)),
                mock_tx.send_params);
            assert_eq!(true, *time_source_called.borrow());
            assert_eq!(None, mock_signals_handler.borrow().on_signal__signal_data);
            assert_eq!(Some((Signals::GetTimeStamp, error, false)), mock_signals_handler.borrow().on_signal_error__params);
        }

    }

    #[test]
    fn test_signals_proxy_proxies_other_signals_to_handler() {

        let mock_signals_handler = Rc::new(RefCell::new(MockSignalsHandler::new()));
        let time_source_called = RefCell::new(false);
        let mut rng = rand::thread_rng();
        let timestamp = RelativeMillis::new(rng.gen_range(1..u32::MAX));
        let mock_time_source = || {
            *time_source_called.borrow_mut() = true;
            timestamp
        };
        let mut mock_tx = MockControlledSender::new(Ok(()), Ok(()));


        let datas = [
            SignalData {
                instruction: Signals::ControlStateChanged,
                relay_signal_data: None,
            },
            SignalData {
                instruction: Signals::MonitoringStateChanged,
                relay_signal_data: Some(RelaySignalData{
                    relative_timestamp: RelativeSeconds::new(123485_u32),
                    relay_idx: 10,
                    is_on: false,
                    is_called_internally: Some(true),
                }),
            }
        ];

        for data in datas {

            let mut proxy = SignalsHandlerProxy::new(
                mock_signals_handler.clone(),
                mock_time_source,
                &mut mock_tx
            );
            proxy.on_signal(data);

            assert_eq!(false, *time_source_called.borrow());
            assert_eq!(false, mock_tx.send_called);
            assert_eq!(None, mock_tx.send_params);
            assert_eq!(false, *time_source_called.borrow());
            assert_eq!(Some(data), mock_signals_handler.borrow().on_signal__signal_data);
            assert_eq!(None, mock_signals_handler.borrow().on_signal_error__params);
        }
    }

    #[test]
    fn test_requests_controller_check_request_should_return_error_on_cache_overflow() {
        let mock_response_handler = MockResponsesHandler::new();
        let mock_responses_parser = default();

        let mut tested =
            RequestsController::new(mock_response_handler, mock_responses_parser);
        tested.requests_count = MAX_REQUESTS_COUNT;

        for data_instruction_code in ADD_DATA_INSTRUCTION_CODES {
            let result = tested.check_request(data_instruction_code);

            assert_eq!(Err(Errors::RequestsLimitReached), result);
        }
    }

    #[test]
    fn test_requests_controller_check_request_should_return_error_on_needed_cache_request_send_duplication() {
        let mock_response_handler = MockResponsesHandler::new();
        let mock_responses_parser = new_check_needs_cache( |_| true );

        let mut tested =
            RequestsController::new(mock_response_handler, mock_responses_parser);
        tested.request_needs_cache_send = true;

        for data_instruction_code in ADD_DATA_INSTRUCTION_CODES {
            let result = tested.check_request(data_instruction_code);

            assert_eq!(Err(Errors::RequestsNeedsCacheAlreadySent), result);
        }
    }

    #[test]
    fn test_requests_controller_check_request_success() {
        let mock_response_handler = MockResponsesHandler::new();
        let mock_responses_parser = new_check_needs_cache( |_| false );

        let mut tested =
            RequestsController::new(mock_response_handler, mock_responses_parser);

        for data_instruction_code in ADD_DATA_INSTRUCTION_CODES {
            tested.request_needs_cache_send = false;
            let result = tested.check_request(data_instruction_code);
            assert_eq!(Ok(()), result);

            tested.request_needs_cache_send = true;
            let result = tested.check_request(data_instruction_code);
            assert_eq!(Ok(()), result);
        }
    }

    #[test]
    fn test_requests_controller_add_sent_request() {
        let mock_response_handler = MockResponsesHandler::new();
        let needs_cache_result = Rc::new(RefCell::new(false));
        let needs_cache_result_clone = needs_cache_result.clone();
        let mock_responses_parser = new_check_needs_cache(move |_| *needs_cache_result_clone.borrow() );

        let mut tested =
            RequestsController::new(mock_response_handler, mock_responses_parser);

        let mut rng = rand::thread_rng();
        let requests = [
            SentRequest::new (OperationCodes::None, DataInstructionCodes::None,
                              RelativeMillis::new(rng.gen_range(1..u32::MAX))
            ),
            SentRequest::new (OperationCodes::None, DataInstructionCodes::None,
                              RelativeMillis::new(rng.gen_range(1..u32::MAX))
            ),
            SentRequest::new (OperationCodes::None, DataInstructionCodes::None,
                              RelativeMillis::new(rng.gen_range(1..u32::MAX))
            ),
            SentRequest::new (OperationCodes::None, DataInstructionCodes::None,
                              RelativeMillis::new(rng.gen_range(1..u32::MAX))
            ),
        ];

        let mut count = 0;
        let mut id = 0;
        tested.request_needs_cache_send = false;

        *needs_cache_result.borrow_mut() = false;
        let res = tested.add_sent_request(requests[0]);

        count += 1;
        id += 1;
        assert_eq!(id, res);
        assert_eq!(count, tested.requests_count);
        assert_eq!(false, tested.request_needs_cache_send);
        assert_eq!(Some(requests[0]), tested.sent_requests[tested.requests_count - 1]);


        tested.request_needs_cache_send = false;

        *needs_cache_result.borrow_mut() = true;
        let res = tested.add_sent_request(requests[1]);

        count += 1;
        id += 1;
        assert_eq!(id, res);
        assert_eq!(count, tested.requests_count);
        assert_eq!(true, tested.request_needs_cache_send);
        assert_eq!(Some(requests[1]), tested.sent_requests[tested.requests_count - 1]);

        *needs_cache_result.borrow_mut() = false;
        let res = tested.add_sent_request(requests[2]);

        count += 1;
        id += 1;
        assert_eq!(id, res);
        assert_eq!(count, tested.requests_count);
        assert_eq!(true, tested.request_needs_cache_send);
        assert_eq!(Some(requests[2]), tested.sent_requests[tested.requests_count - 1]);


        tested.add_sent_request(requests[3]);
    }

    #[test]
    fn test_requests_controller_is_request() {

        let controller = RequestsController::new(MockResponsesHandler::new(), default());

        let responses = [OperationCodes::Response as u8, OperationCodes::Success as u8, OperationCodes::Error as u8,
            OperationCodes::SuccessV2 as u8, OperationCodes::ResponseV2 as u8, OperationCodes::ErrorV2 as u8];
        let not_responses = [OperationCodes::None as u8, OperationCodes::Set as u8, OperationCodes::Read as u8,
            OperationCodes::Command as u8, OperationCodes::Signal as u8, 11, 12, 13, 14, 56, 128, 255];

        for response in responses {
            assert_eq!(true, controller.is_response(response));
        }
        for response in not_responses {
            assert_eq!(false, controller.is_response(response));
        }

    }

    const ADD_DATA_INSTRUCTION_CODES: [DataInstructionCodes; 22] = [
        DataInstructionCodes::None,
        DataInstructionCodes::Settings,
        DataInstructionCodes::State,
        DataInstructionCodes::Id,
        DataInstructionCodes::InterruptPin,
        DataInstructionCodes::RemoteTimestamp,
        DataInstructionCodes::StateFixSettings,
        DataInstructionCodes::RelayState,
        DataInstructionCodes::Version,
        DataInstructionCodes::CurrentTime,
        DataInstructionCodes::ContactWaitData,
        DataInstructionCodes::FixData,
        DataInstructionCodes::SwitchData,
        DataInstructionCodes::CyclesStatistics,
        DataInstructionCodes::SwitchCountingSettings,
        DataInstructionCodes::RelayDisabledTemp,
        DataInstructionCodes::RelaySwitchedOn,
        DataInstructionCodes::RelayMonitorOn,
        DataInstructionCodes::RelayControlOn,
        DataInstructionCodes::All,
        DataInstructionCodes::Last,
        DataInstructionCodes::Unknown, ];

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

    struct MockSender {
        start_transfer_called: bool,
        call_writer: bool,
        start_transfer_result: Result<(), Errors>,
        buffer: MockTxBuffer,
    }

    impl MockSender {
        pub fn new (call_writer: bool, start_transfer_result: Result<(), Errors>) -> Self {
            let buffer = MockTxBuffer::new();
            Self {
                start_transfer_called: false,
                call_writer,
                start_transfer_result,
                buffer,
            }
        }

    }

    impl Sender<MockTxBuffer> for MockSender {
        fn start_transfer<F: FnOnce(&mut MockTxBuffer) -> Result<(), Errors>>(&mut self, writter: F) -> Result<(), Errors> {
            self.start_transfer_called = true;
            if self.call_writer {
                writter(&mut self.buffer)?;
            }
            self.start_transfer_result
        }
    }

    impl Sender<MockTxBuffer> for Rc<RefCell<MockSender>> {
        fn start_transfer<F: FnOnce(&mut MockTxBuffer) -> Result<(), Errors>>(&mut self, writter: F) -> Result<(), Errors> {
            self.borrow_mut().start_transfer(writter)
        }
    }

    #[derive(PartialEq, Debug)]
    struct MockRequestsControllerTx {
        check_request_result: Result<(), Errors>,
        check_request_result_called: RefCell<bool>,
        add_sent_request_called: bool,
        add_sent_request_parameter: Option<SentRequest>,
        add_sent_request_result: u32,
    }

    impl MockRequestsControllerTx {
        pub fn new (check_request_result: Result<(), Errors>, add_sent_request_result: u32) -> Self {
            Self {
                check_request_result,
                check_request_result_called: RefCell::new(false),
                add_sent_request_called: false,
                add_sent_request_parameter: None,
                add_sent_request_result,
            }
        }
    }

    impl RequestsControllerTx for MockRequestsControllerTx {

        fn check_request(&self, _: DataInstructionCodes) -> Result<(), Errors> {
            *self.check_request_result_called.borrow_mut() = true;
            self.check_request_result
        }

        fn add_sent_request(&mut self, request: SentRequest) -> u32 {
            self.add_sent_request_called = true;
            self.add_sent_request_parameter = Some(request);
            self.add_sent_request_result
        }
    }


    struct MockRequestsControllerRx {
        process_response_params: Option<(u8, Vec<u8>)>,
        is_response_result: bool,
        is_response_param: RefCell<Option<u8>>,
    }

    impl MockRequestsControllerRx {
        pub fn new (is_response_result: bool) -> Self {
            Self {
                process_response_params: None,
                is_response_result,
                is_response_param: RefCell::new(None),
            }
        }
    }

    impl RequestsControllerRx for MockRequestsControllerRx {
        fn process_response(&mut self, operation_code: u8, data: &[u8]) {
            self.process_response_params = Some((operation_code, data.to_vec()));
        }
        fn is_response(&self, operation_code: u8) -> bool {
            *self.is_response_param.borrow_mut() = Some(operation_code);
            self.is_response_result
        }
    }

    const BUFFER_SIZE: usize = 20;

    struct MockTxBuffer {
        buffer: [u8; BUFFER_SIZE],
        add_ua_arguments: Vec<u8>,
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
            self.add_ua_arguments.push(byte);
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
            self.add_ua_arguments.clear();
            self.cleared = true;
        }

    }

    impl MockTxBuffer {
        fn new() -> Self {
            Self {
                buffer: [0_u8; BUFFER_SIZE],
                add_ua_arguments: Vec::new(),
                cleared: false,
            }
        }
    }

    struct MockIntruction {
        id: u32,
        code: DataInstructionCodes,
        serialize_called: RefCell<bool>,
        serialize_result: Result<(), Errors>,
    }

    impl MockIntruction {
        fn new(id: u32, code: DataInstructionCodes, serialize_result: Result<(), Errors>) -> Self {
            Self {
                id,
                code,
                serialize_called: RefCell::new(false),
                serialize_result,
            }
        }
    }

    impl DataInstruction for MockIntruction {
        fn code(&self) -> DataInstructionCodes {
            self.code
        }

        fn serialize<B: BufferWriter>(&self, buffer: &mut B) -> Result<(), Errors> {
            *self.serialize_called.borrow_mut() = true;
            self.serialize_result
        }
    }

    impl DataInstruction for Rc<MockIntruction> {
        fn code(&self) -> DataInstructionCodes {
            self.code
        }

        fn serialize<B: BufferWriter>(&self, buffer: &mut B) -> Result<(), Errors> {
            *self.serialize_called.borrow_mut() = true;
            self.serialize_result
        }

    }

    struct MockReceiver {
        data: Vec<u8>,
        receiver_result: Option<Result<(), Errors>>,
        on_rx_transfer_interrupt_called: bool,
    }

    impl MockReceiver {
        pub fn new(data: Vec<u8>) -> Self {
            Self {
                data,
                receiver_result: None,
                on_rx_transfer_interrupt_called: false,
            }
        }
    }

    impl Receiver for MockReceiver {
        fn on_rx_transfer_interrupt<F: FnOnce(&[u8]) -> Result<(), Errors>> (&mut self, receiver: F) -> Result<(), Errors> {
            let res = receiver(&self.data.as_slice());
            self.receiver_result = Some(res);
            self.on_rx_transfer_interrupt_called = true;
            res
        }
    }

    struct MockSignalsHandler {
        on_signal__signal_data: Option<SignalData>,
        on_signal_error__params: Option<(Signals, Errors, bool)>,
    }

    impl MockSignalsHandler {
        fn new() -> Self {
            Self {
                on_signal__signal_data: None,
                on_signal_error__params: None,
            }
        }
    }

    impl SignalsHandler for MockSignalsHandler {
        fn on_signal(&mut self, signal_data: SignalData) {
            self.on_signal__signal_data = Some(signal_data);
        }
        fn on_signal_error(&mut self, instruction: Signals, error: Errors, sent: bool) {
            self.on_signal_error__params = Some((instruction, error, sent));
        }
    }

    impl SignalsHandler for Rc<RefCell<MockSignalsHandler>> {
        fn on_signal(&mut self, signal_data: SignalData) {
            self.borrow_mut().on_signal(signal_data);
        }
        fn on_signal_error(&mut self, instruction: Signals, error: Errors, sent: bool) {
            self.borrow_mut().on_signal_error(instruction, error, sent);
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

/*
    struct MockReceiverFromSlaveController {
        rx: MockReceiver,
        signal_controller: MockSignalController,
        error_handler:  fn(Errors),
    }


    impl MockReceiverFromSlaveController {
        pub fn new(rx: MockReceiver, signal_controller: MockSignalController, error_handler:  fn(Errors)) -> Self {
            Self {
                rx,
                signal_controller,
                error_handler,
            }
        }

    }

    impl ReceiverFromSlaveControllerAbstract<MockSignalsHandler, MockReceiver, MockRequestsControllerRx, MockSignalsParser, MockSignalController>
    for MockReceiverFromSlaveController
    {
        #[inline(always)]
        fn slice(&mut self) -> (&mut MockReceiver, &mut MockSignalController) {
            let MockReceiverFromSlaveController { rx, signal_controller, .. } = self;
            (rx, signal_controller)
        }

        fn error_handler(&mut self) -> &mut fn(Errors) {
            &mut self.error_handler
        }
    }
    */

    struct MockSignalsParserTestData {
        parse_params: Option<(Signals, Vec<u8>)>,
        parse_instruction_params: Option<u8>,
    }

    impl MockSignalsParserTestData {
        fn new() -> Self {
            Self {
                parse_params : None,
                parse_instruction_params: None,
            }
        }
    }

    struct MockSignalsParser {
        parse_result: Result<SignalData, Errors>,
        parse_instruction_result: Option<Signals>,
        test_data: RefCell<MockSignalsParserTestData>,
    }

    impl MockSignalsParser {
        pub fn new(parse_result: Result<SignalData, Errors>, parse_instruction_result: Option<Signals>) -> Self {
            let test_data = RefCell::new(MockSignalsParserTestData::new());
            Self {
                parse_result,
                parse_instruction_result,
                test_data,
            }
        }
    }

    impl SignalsParser for MockSignalsParser {

        fn parse(&self, instruction: Signals, data: &[u8]) -> Result<SignalData, Errors> {
            self.test_data.borrow_mut().parse_params = Some((instruction, data.to_vec()));
            self.parse_result
        }


        fn parse_instruction(&self, instruction_code: u8) -> Option<Signals> {
            self.test_data.borrow_mut().parse_instruction_params = Some(instruction_code);
            self.parse_instruction_result
        }
    }

    struct MockSignalController {
        process_signal_params: Option<Vec<u8>>,
    }

    impl MockSignalController {
        pub fn new() -> Self {
            Self {
                process_signal_params: None,
            }
        }
    }

    impl SignalController for MockSignalController {
        fn process_signal(&mut self, data: &[u8]) {
            self.process_signal_params = Some(data.to_vec());
        }
    }

    struct MockControlledSender {
        send_called: bool,
        send_params: Option<(OperationCodes, DataInstructions, RelativeMillis)>,
        send_result: Result<(), Errors>,
        send_error_called: bool,
        send_error_params: Option<(u8, ErrorCode)>,
        send_error_result: Result<(), Errors>,
    }

    impl MockControlledSender {
        pub fn new(send_result: Result<(), Errors>, send_error_result: Result<(), Errors>) -> Self {
            Self {
                send_called: false,
                send_params: None,
                send_result,
                send_error_called: false,
                send_error_params: None,
                send_error_result,
            }
        }
    }

    impl ErrorsSender for MockControlledSender {
        fn send_error(&mut self, instruction_code: u8, error_code: ErrorCode) -> Result<(), Errors> {
            self.send_error_called = true;
            self.send_error_params = Some((instruction_code, error_code));
            self.send_error_result
        }
    }

    impl ControlledRequestSender for MockControlledSender {
        fn send(&mut self, operation: OperationCodes, instruction: DataInstructions, timestamp: RelativeMillis) -> Result<(), Errors> {
            self.send_called = true;
            self.send_params = Some((operation, instruction, timestamp));
            self.send_result
        }
    }

    struct MockResponsesHandler {

    }

    impl MockResponsesHandler {
        pub fn new() -> Self {
            Self {

            }
        }
    }

    impl ResponseHandler for MockResponsesHandler {
        fn on_request_success(&mut self, request: SentRequest) {

        }

        fn on_request_error(&mut self, request: SentRequest, error_code: ErrorCode) {

        }

        fn on_request_process_error(&mut self, request: Option<SentRequest>, error: Errors, data: &[u8]) {

        }

        fn on_request_response(&mut self, request: SentRequest, response: DataInstructions) {

        }
    }

    struct MockResponsesParser<F1, F2>
        where
            F1: Fn(u8, &[u8]) -> Result<DataInstructions, Errors>,
            F2: Fn(DataInstructionCodes) -> bool
    {
        parse_response_cb: F1,
        request_needs_cache_cb: F2,
    }

    fn new_parse_response<F>(parse_response_cb: F) -> MockResponsesParser<F, fn(DataInstructionCodes) -> bool>
        where
            F: Fn(u8, &[u8]) -> Result<DataInstructions, Errors>,
    {
        MockResponsesParser::new(parse_response_cb, |_| unimplemented!())
    }

    fn new_check_needs_cache<F>(request_needs_cache_cb: F) -> MockResponsesParser<fn(u8, &[u8]) -> Result<DataInstructions, Errors>, F>
        where
            F: Fn(DataInstructionCodes) -> bool,
    {
        MockResponsesParser::new(|_, _| unimplemented!(), request_needs_cache_cb)
    }

    fn default() -> MockResponsesParser<fn(u8, &[u8]) -> Result<DataInstructions, Errors>, fn(DataInstructionCodes) -> bool> {
        MockResponsesParser::new(|_, _| unimplemented!(), |_| unimplemented!())
    }

    impl <F1, F2>  MockResponsesParser<F1, F2>
        where
            F1: Fn(u8, &[u8]) -> Result<DataInstructions, Errors> ,
            F2: Fn(DataInstructionCodes) -> bool
    {
        pub fn new<>(parse_response_cb: F1, request_needs_cache_cb: F2) -> Self {
            Self {
                parse_response_cb,
                request_needs_cache_cb,
            }
        }
    }

    impl <F1, F2> ResponsesParser for MockResponsesParser<F1, F2>
        where
            F1: Fn(u8, &[u8]) -> Result<DataInstructions, Errors> ,
            F2: Fn(DataInstructionCodes) -> bool
    {
        fn parse_response(&self, instruction_code: u8, data: &[u8]) -> Result<DataInstructions, Errors> {
            (self.parse_response_cb)(instruction_code, data)
        }
        fn request_needs_cache(&self, instruction: DataInstructionCodes) -> bool {
            (self.request_needs_cache_cb)(instruction)
        }
    }


}