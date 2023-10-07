#![allow(unsafe_code)]

pub mod domain;
pub mod parsers;

use embedded_dma::{ReadBuffer, WriteBuffer};
use crate::services::slave_controller_link::domain::{*};
use crate::errors::Errors;
use crate::hal_ext::rtc_wrapper::{RelativeMillis, RelativeSeconds };
use crate::hal_ext::serial_transfer::{ ReadableBuffer, Receiver, RxTransfer, RxTransferProxy, Sender, SerialTransfer, TxTransfer, TxTransferProxy};
use crate::services::slave_controller_link::parsers::{RequestsParser, RequestsParserImpl, SignalsParser, SignalsParserImpl};
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


pub struct SentRequest {
    operation: Operation,
    instruction: DataInstructionCodes,
    rel_timestamp: RelativeMillis
}

impl SentRequest {
    fn new(operation: Operation, instruction: DataInstructionCodes, rel_timestamp: RelativeMillis) -> Self {
        Self {
            operation,
            instruction,
            rel_timestamp
        }
    }
}

pub trait SignalsReceiver {
    fn on_signal(&mut self, signal_data: SignalData);
    fn on_signal_error(&mut self, instruction_code: u8, error_code: ErrorCode, sent_to_slave_success: bool);
}

pub trait ResponseHandler {
    fn on_request_success(&mut self, request: &SentRequest);
    fn on_request_error(&mut self, request: &SentRequest, error_code: ErrorCode);
    fn on_request_parse_error(&mut self, request: &SentRequest, error: Errors, data: &[u8]);
    fn on_request_response(&mut self, request: &SentRequest, response: DataInstructions);
}

trait RequestsControllerTx {
    fn add_sent_request(&mut self, request: SentRequest);
    fn check_request(&self, instruction: DataInstructionCodes) -> Result<(), Errors>;
}

trait RequestsControllerRx {
    fn process_response(&mut self, operation_code: u8, instruction_code: u8, data: &[u8]) -> Result<(), Errors>;
}

struct RequestsController<RH: ResponseHandler, RP: RequestsParser> {
    sent_requests: [Option<SentRequest>; MAX_REQUESTS_COUNT],
    requests_count: usize,
    request_needs_cache_send: bool,
    response_handler: RH,
    requests_parser: RP,
}

impl <RH: ResponseHandler, RP: RequestsParser> RequestsController<RH, RP> {
    fn new(response_handler: RH, requests_parser: RP) -> Self {
        Self {
            sent_requests: [None, None, None, None],
            requests_count: 0,
            request_needs_cache_send: false,
            response_handler,
            requests_parser,
        }
    }
}

impl <RH: ResponseHandler, RP: RequestsParser> RequestsControllerTx for RequestsController<RH, RP> {

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
    fn add_sent_request(&mut self, request: SentRequest) {
        self.requests_count += 1;
        if self.requests_parser.request_needs_cache(request.instruction) {
            self.request_needs_cache_send = true;
        }
        self.sent_requests[self.requests_count] = Some(request);
    }

}

impl <RH: ResponseHandler, RP: RequestsParser> RequestsControllerRx for RequestsController<RH, RP> {

    fn process_response(&mut self, operation_code: u8, instruction_code: u8, data: &[u8]) -> Result<(), Errors> {
        if self.requests_count > 0 {
            let search_operation = if operation_code == Operation::Success as u8 {
                Operation::Set
            } else if operation_code == Operation::Response as u8 {
                Operation::Read
            } else {
                Operation::Error
            };
            for i in (0..self.requests_count).rev() {
                if let Some(request) = self.sent_requests[i].as_ref() {
                    if request.instruction as u8 == instruction_code && request.operation == search_operation {
                        if operation_code == Operation::Success as u8 {
                            self.response_handler.on_request_success(request);
                        } else if operation_code == Operation::Error as u8 {
                            self.response_handler.on_request_error(request, ErrorCode::for_code(instruction_code));
                        } else {
                            match self.requests_parser.parse_response(instruction_code, &data[3..]) {
                                Ok(response) => {
                                    self.response_handler.on_request_response(request, response);
                                }
                                Err(error) => {
                                    self.response_handler.on_request_parse_error(request, error, &data[3..]);
                                }
                            }
                        }
                        if operation_code == Operation::Response as u8 && self.requests_parser.request_needs_cache(request.instruction) {
                            self.request_needs_cache_send = false;
                        }
                        let mut next_pos = i + 1;
                        while next_pos < self.requests_count {
                            self.sent_requests.swap(next_pos - 1, next_pos);
                            next_pos += 1;
                        }
                        self.sent_requests[next_pos - 1] = None;
                        self.requests_count -= 1;
                        return Ok(());
                    }
                }
            }
        }
        Err(Errors::NoRequestsFound)
    }

}

struct SignalsHandlerProxy<'a, SH, TS, S, TxBuff, RC>
    where
        SH: SignalsReceiver,
        TS: Fn() -> RelativeMillis,
        TxBuff: ReadBuffer + BufferWriter,
        S: TransmitterToSlaveControllerAbstract<TxBuff, RC>,
        RC: RequestsControllerTx + RequestsControllerRx,
{
    proxy: SH,
    time_source: TS,
    tx:  &'a mut S,
    request_controller: &'a mut RC,
    phantom: core::marker::PhantomData<TxBuff>,
}

impl <'a, SH, TS, S, TxBuff, RC> SignalsHandlerProxy<'a, SH, TS, S, TxBuff, RC>
    where
        SH: SignalsReceiver,
        TS: Fn() -> RelativeMillis,
        TxBuff: ReadBuffer + BufferWriter,
        S: TransmitterToSlaveControllerAbstract<TxBuff, RC>,
        RC: RequestsControllerTx + RequestsControllerRx,
{
    fn new(proxy: SH, time_source: TS, tx:  &'a mut S, request_controller: &'a mut RC) -> Self {
        Self {
            proxy, time_source, tx, request_controller, phantom: core::marker::PhantomData
        }
    }
}

impl  <'a, SH, TS, S, TxBuff, RC> SignalsReceiver for SignalsHandlerProxy<'a, SH, TS, S, TxBuff, RC>
    where
        SH: SignalsReceiver,
        TS: Fn() -> RelativeMillis,
        TxBuff: ReadBuffer + BufferWriter,
        S: TransmitterToSlaveControllerAbstract<TxBuff, RC>,
        RC: RequestsControllerTx + RequestsControllerRx,
{

    fn on_signal(&mut self, signal_data: SignalData) {
        if signal_data.instruction == Signals::GetTimeStamp {
            let timestamp = (self.time_source)();
            let _ = self.tx.send_request(Operation::Set,
                            DataInstructions::RemoteTimestamp(Conversation::Data(timestamp.seconds())),
                            timestamp,
                                 self.request_controller);
        } else {
            self.proxy.on_signal(signal_data);
        }
    }

    fn on_signal_error(&mut self, instruction_code: u8, error_code: ErrorCode, _: bool) {
        let sent_to_slave_success = self.tx.send_error(instruction_code, error_code).is_ok();
        self.proxy.on_signal_error(instruction_code, error_code, sent_to_slave_success);
    }

}

pub struct SlaveControllerLink<T, R, TxBuff, RxBuff, SH, RH>
    where
        TxBuff: ReadBuffer + BufferWriter,
        RxBuff: WriteBuffer + ReadableBuffer,
        T: TxTransferProxy<TxBuff>,
        R: RxTransferProxy<RxBuff>,
        SH: SignalsReceiver,
        RH: ResponseHandler,
{
    tx: TransmitterToSlaveController<T, TxBuff>,
    rx: ReceiverFromSlaveController<R, RxBuff, SH, SignalsParserImpl>,
    requests_controller: RequestsController<RH, RequestsParserImpl>,
}


impl <T, R, TxBuff, RxBuff, SH, RH> SlaveControllerLink<T, R,TxBuff, RxBuff, SH, RH>
    where
        TxBuff: ReadBuffer + BufferWriter,
        RxBuff: WriteBuffer + ReadableBuffer,
        T: TxTransferProxy<TxBuff>,
        R: RxTransferProxy<RxBuff>,
        SH: SignalsReceiver,
        RH: ResponseHandler,
{
    pub fn create(serial_transfer: SerialTransfer<T, R, TxBuff, RxBuff>, signal_receiver: SH, response_handler: RH) -> Result<Self, Errors> {
        let (tx, rx) = serial_transfer.into();
        let requests_parser = RequestsParserImpl::create()?;
        let requests_controller = RequestsController::new(response_handler, requests_parser);
        let signals_parser = SignalsParserImpl::new();

        Ok(Self {
            tx: TransmitterToSlaveController::new(tx),
            rx: ReceiverFromSlaveController::new(rx, signal_receiver, signals_parser),
            requests_controller
        })
    }

    #[inline(always)]
    pub fn on_get_command<E, TS:  FnOnce() -> RelativeMillis>( &mut self) -> Result<(), Errors> {
        let SlaveControllerLink { rx, requests_controller, .. } = self;
        rx.on_get_command(requests_controller)
    }

    #[inline(always)]
    pub fn on_rx_dma_interrupts(&mut self) {
        self.rx.on_dma_interrupts();
    }

    #[inline(always)]
    pub fn on_tx_dma_interrupts(&mut self) {
        self.tx.on_dma_interrupts();
    }
}

trait TransmitterToSlaveControllerAbstract<TxBuff, RCT> : Sender<TxBuff>
    where
        TxBuff: ReadBuffer + BufferWriter,
        RCT: RequestsControllerTx,
{

    fn send_request<I: DataInstruction>(&mut self, operation: Operation, instruction: I, timestamp: RelativeMillis, request_controller: &mut RCT) -> Result<(), Errors> {

        request_controller.check_request(instruction.code())?;

        let result = self.start_transfer(|buffer| {
            buffer.clear();
            buffer.add_u8(Operation::None as u8)?;
            buffer.add_u8(operation as u8)?;
            buffer.add_u8(instruction.code() as u8)?;
            instruction.serialize(buffer)
        });

        if result.is_ok() {
            request_controller.add_sent_request(SentRequest::new(operation, instruction.code(), timestamp));
        }

        result
    }

    fn send_error(&mut self, instruction_code: u8, error_code: ErrorCode) -> Result<(), Errors> {
        self.start_transfer(|buffer| {
            buffer.clear();
            buffer.add_u8(Operation::None as u8)?;
            buffer.add_u8(Operation::Error as u8)?;
            buffer.add_u8(instruction_code)?;
            buffer.add_u8(error_code.discriminant())
        })
    }

}

struct TransmitterToSlaveController<T, TxBuff>
    where
        TxBuff: ReadBuffer + BufferWriter,
        T: TxTransferProxy<TxBuff>,
{
    tx: TxTransfer<T, TxBuff>
}

impl <T, TxBuff> TransmitterToSlaveController<T, TxBuff>
    where
        TxBuff: ReadBuffer + BufferWriter,
        T: TxTransferProxy<TxBuff>,
{
    pub fn new (tx: TxTransfer<T, TxBuff>) -> Self {
        Self {
            tx,
        }
    }

    #[inline(always)]
    fn on_dma_interrupts(&mut self) {
        self.tx.on_dma_interrupts();
    }

}

impl <T, TxBuff> Sender<TxBuff> for TransmitterToSlaveController<T, TxBuff>
    where
        TxBuff: ReadBuffer + BufferWriter,
        T: TxTransferProxy<TxBuff>,
{
    fn start_transfer<F: FnOnce(&mut TxBuff) -> Result<(), Errors>>(&mut self, writter: F) -> Result<(), Errors> {
        self.tx.start_transfer(writter)
    }
}

impl <T, TxBuff, RCT> TransmitterToSlaveControllerAbstract<TxBuff, RCT> for TransmitterToSlaveController<T, TxBuff>
    where
        TxBuff: ReadBuffer + BufferWriter,
        T: TxTransferProxy<TxBuff>,
        RCT: RequestsControllerTx,
{}

trait ReceiverFromSlaveControllerAbstract<SR, Rc, RCR, SP>
    where
        SR: SignalsReceiver,
        Rc: Receiver,
        RCR: RequestsControllerRx,
        SP: SignalsParser,
{

    fn slice(&mut self) -> (&mut Rc, &mut SR, &SP);

    fn on_get_command(&mut self, request_controller: &mut RCR) -> Result<(), Errors> {
        let (rx, sr, signal_pargser) = self.slice();
        rx.on_rx_transfer_interrupt(|data| {
            if data.len() >= 3 {
                if data[0] == Operation::None as u8 {
                    let operation_code = data[1];
                    let instruction_code = data[2];
                    let data = &data[3..];
                    if operation_code == Operation::Signal as u8 {
                        let instruction = if instruction_code == Signals::MonitoringStateChanged as u8 {
                            Some(Signals::MonitoringStateChanged)
                        } else if instruction_code == Signals::StateFixTry as u8 {
                            Some(Signals::StateFixTry)
                        } else if instruction_code == Signals::ControlStateChanged as u8 {
                            Some(Signals::ControlStateChanged)
                        } else if instruction_code == Signals::RelayStateChanged as u8 {
                            Some(Signals::RelayStateChanged)
                        } else if instruction_code == Signals::GetTimeStamp as u8 {
                            Some(Signals::GetTimeStamp)
                        } else {
                            None
                        };
                        match instruction {
                            Some(instruction) => {
                                match signal_pargser.parse(instruction, data) {
                                    Ok(signal_data) => {
                                        sr.on_signal(signal_data);
                                        Ok(())
                                    }
                                    Err(error) => {
                                        sr.on_signal_error(instruction_code, error, false);
                                        Ok(())
                                    }
                                }
                            }
                            None => {
                                sr.on_signal_error(instruction_code, ErrorCode::EInstructionUnrecognized, false);
                                Ok(())
                            }
                        }
                    } else if operation_code == Operation::Success as u8 || operation_code == Operation::Response as u8 || operation_code == Operation::Error as u8 {
                        request_controller.process_response(operation_code, instruction_code, data)
                    } else {
                        Err(Errors::OperationNotRecognized(operation_code))
                    }
                } else {
                    Err(Errors::CommandDataCorrupted)
                }
            } else {
                Err(Errors::NotEnoughDataGot)
            }
        })
    }
}

struct ReceiverFromSlaveController<R, RxBuff, SR, SP>
    where
        RxBuff: WriteBuffer + ReadableBuffer,
        R: RxTransferProxy<RxBuff>,
        SR: SignalsReceiver,
        SP: SignalsParser,
{
    rx: RxTransfer<R, RxBuff>,
    signal_receiver: SR,
    signal_parser: SP,
}

impl <R, RxBuff, SR, SP> ReceiverFromSlaveController<R, RxBuff, SR, SP>
    where
        RxBuff: WriteBuffer + ReadableBuffer,
        R: RxTransferProxy<RxBuff>,
        SR: SignalsReceiver,
        SP: SignalsParser,
{
    pub fn new(rx: RxTransfer<R, RxBuff>, signal_receiver: SR, signal_parser: SP) -> Self {
        Self { rx, signal_receiver, signal_parser }
    }

    #[inline(always)]
    pub fn on_dma_interrupts(&mut self) {
        self.rx.on_dma_interrupts();
    }
}

impl <R, RxBuff, SR, RCR, SP> ReceiverFromSlaveControllerAbstract<SR, RxTransfer<R, RxBuff>, RCR, SP> for ReceiverFromSlaveController<R, RxBuff, SR, SP>
    where
        RxBuff: WriteBuffer + ReadableBuffer,
        R: RxTransferProxy<RxBuff>,
        SR: SignalsReceiver,
        RCR: RequestsControllerRx,
        SP: SignalsParser,
{
    #[inline(always)]
    fn slice(&mut self) ->( &mut RxTransfer<R, RxBuff>, &mut SR, &SP) {
        (&mut self.rx, &mut self.signal_receiver, &self.signal_parser)
    }
}



#[cfg(test)]
mod tests {
    use alloc::boxed::Box;
    use alloc::vec::Vec;
    use core::cell::RefCell;
    use super::*;
    use std::rc::Rc;
    use quickcheck_macros::quickcheck;
    use rand::distributions::uniform::SampleBorrow;
    use rand::prelude::*;
    use crate::errors::DMAError;

    #[test]
    fn test_send_error() {
        let start_transfer_result = Ok(());
        let sending_error: ErrorCode = ErrorCode::EInstructionUnrecognized;
        let instruction_code = Signals::MonitoringStateChanged as u8;

        let mut mock =
            MockTransmitterToSlaveController::new(true, start_transfer_result);

        let result = mock.send_error(instruction_code, sending_error);

        assert_eq!(true, mock.start_transfer_called);
        assert_eq!(start_transfer_result, result);
        assert_eq!(true, mock.buffer.cleared);
        assert_eq!(4, mock.buffer.add_ua_arguments.len());
        assert_eq!(Operation::None as u8, mock.buffer.add_ua_arguments[0]);
        assert_eq!(Operation::Error as u8, mock.buffer.add_ua_arguments[1]);
        assert_eq!(instruction_code, mock.buffer.add_ua_arguments[2]);
        assert_eq!(sending_error.discriminant(), mock.buffer.add_ua_arguments[3]);
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
        let mut mock =
            MockTransmitterToSlaveController::new(true, start_transfer_result);

        for instruction_code in 0 .. 100 {
            for sending_error in sending_errors {
                mock.send_error(instruction_code, sending_error).unwrap();
                assert_eq!(4, mock.buffer.add_ua_arguments.len());
                assert_eq!(instruction_code, mock.buffer.add_ua_arguments[2]);
                assert_eq!(sending_error.discriminant(), mock.buffer.add_ua_arguments[3]);
            }
        }
    }

    #[test]
    fn test_send_request() {
        let start_transfer_result = Ok(());
        let operation = Operation::Set;
        let instruction_id = 12385249;
        let instruction = Rc::new(MockIntruction::new(instruction_id, DataInstructionCodes::Id, Ok(())));
        let instruction_code = instruction.code();
        let timestamp = RelativeMillis::new(0x12345678_u32);

        let mut mock =
            MockTransmitterToSlaveController::new(true, start_transfer_result);

        let mut mock_request_controller = MockRequestsControllerTx::new(Ok(()));

        let result = mock.send_request(operation, instruction.clone(), timestamp, &mut mock_request_controller);

        assert_eq!(true, *mock_request_controller.check_request_result_called.borrow());
        assert_eq!(start_transfer_result, result);
        //check add request operation
        assert_eq!(true, mock_request_controller.add_sent_request_called);
        assert!(mock_request_controller.add_sent_request_parameter.is_some());
        assert_eq!(operation, mock_request_controller.add_sent_request_parameter.as_ref().unwrap().operation);
        assert_eq!(timestamp, mock_request_controller.add_sent_request_parameter.as_ref().unwrap().rel_timestamp);
        assert_eq!(instruction_code, mock_request_controller.add_sent_request_parameter.as_ref().unwrap().instruction);
        //check buffer operations
        assert_eq!(true, mock.buffer.cleared);
        assert!(3 <= mock.buffer.add_ua_arguments.len());
        assert_eq!(Operation::None as u8, mock.buffer.add_ua_arguments[0]);
        assert_eq!(operation as u8, mock.buffer.add_ua_arguments[1]);
        assert_eq!(instruction_code as u8, mock.buffer.add_ua_arguments[2]);
        assert_eq!(true, *instruction.serialize_called.borrow());
    }

    #[test]
    fn test_send_request_returns_all_check_request_result_errors() {
        let start_transfer_result = Ok(());
        let operation = Operation::Set;
        let timestamp = RelativeMillis::new(0x12345678_u32);

        let mut mock =
            MockTransmitterToSlaveController::new(true, start_transfer_result);

        let errors = [
            Errors::RequestsLimitReached,
            Errors::RequestsNeedsCacheAlreadySent,
        ];

        let mut mock_request_controller = MockRequestsControllerTx::new(Ok(()));

        for error in errors {
            let instruction = DataInstructions::Id(Conversation::Data(0x12345678_u32));
            *mock_request_controller.check_request_result_called.borrow_mut() = false;
            mock_request_controller.check_request_result = Err(error);
            let result = mock.send_request(operation, instruction, timestamp, &mut mock_request_controller);
            assert_eq!(true, *mock_request_controller.check_request_result_called.borrow());
            assert_eq!(false, mock_request_controller.add_sent_request_called);
            assert_eq!(Err(error), result);
        }
    }

    #[test]
    fn test_send_request_returns_all_start_transfer_errors() {
        let start_transfer_result = Ok(());
        let operation = Operation::Set;
        let timestamp = RelativeMillis::new(0x12345678_u32);

        let mut mock =
            MockTransmitterToSlaveController::new(true, start_transfer_result);

        let errors = [
            Errors::TransferInProgress,
            Errors::NoBufferAvailable,
            Errors::DmaError(DMAError::Overrun(())),
            Errors::DmaError(DMAError::NotReady(())),
            Errors::DmaError(DMAError::SmallBuffer(())),
        ];

        let mut mock_request_controller = MockRequestsControllerTx::new(Ok(()));
        for error in errors {
            let instruction = DataInstructions::Id(Conversation::Data(0x12345678_u32));
            mock.start_transfer_result = Err(error);
            let result = mock.send_request(operation, instruction, timestamp, &mut mock_request_controller);
            assert_eq!(false, mock_request_controller.add_sent_request_called);
            assert_eq!(Err(error), result);
        }
    }

    #[test]
    fn test_on_get_command_should_return_not_enough_data_error_on_low_bytes_message() {

        let datas = Vec::from([[].to_vec(), [1].to_vec(), [1, 2].to_vec()]);

        for data in datas {
            let mut mock = MockReceiverFromSlaveController::create(data);

            let mut request_controller = MockRequestsControllerRx::new(Ok(()));

            let result = mock.on_get_command(&mut request_controller);

            assert_eq!(true, mock.rx.on_rx_transfer_interrupt_called);
            assert_eq!(Err(Errors::NotEnoughDataGot), result);
            assert_eq!(Err(Errors::NotEnoughDataGot), mock.rx.receiver_result.unwrap());
            assert_eq!(false, request_controller.process_response_called);
            assert_eq!(None, mock.signal_receiver.on_signal__signal_data);
            assert_eq!(None, mock.signal_receiver.on_signal_error__params);
        }
    }

    #[test]
    fn test_on_get_command_should_return_corrupted_data_error_on_starting_not_0() {
        let mut mock = MockReceiverFromSlaveController::create([1, 2, 3].to_vec());

        let mut request_controller = MockRequestsControllerRx::new(Ok(()));

        let result = mock.on_get_command(&mut request_controller);

        assert_eq!(true, mock.rx.on_rx_transfer_interrupt_called);
        assert_eq!(Err(Errors::CommandDataCorrupted), result);
        assert_eq!(Err(Errors::CommandDataCorrupted), mock.rx.receiver_result.unwrap());
        assert_eq!(false, request_controller.process_response_called);
        assert_eq!(None, mock.signal_receiver.on_signal__signal_data);
        assert_eq!(None, mock.signal_receiver.on_signal_error__params);
    }

    #[test]
    fn test_on_get_command_should_renurn_not_recognized_on_unknown() {
        let unknown_operations = [Operation::Unknown as u8, Operation::None as u8, Operation::Set as u8, Operation::Read as u8, Operation::Command as u8, 8, 9, 11, 56];

        for operation_code in unknown_operations {
            let mut mock = MockReceiverFromSlaveController::create(
                [Operation::None as u8, operation_code, 0].to_vec());

            let mut request_controller = MockRequestsControllerRx::new(Ok(()));

            let result = mock.on_get_command(&mut request_controller);

            assert_eq!(true, mock.rx.on_rx_transfer_interrupt_called);
            assert_eq!(Err(Errors::OperationNotRecognized(operation_code)), result);
            assert_eq!(Err(Errors::OperationNotRecognized(operation_code)), mock.rx.receiver_result.unwrap());
            assert_eq!(false, request_controller.process_response_called);
            assert_eq!(None, mock.signal_receiver.on_signal__signal_data);
            assert_eq!(None, mock.signal_receiver.on_signal_error__params);
        }
    }

    #[test]
    fn test_on_get_command_should_call_request_controller_on_response_operations() {
        let response_operations = [Operation::Response as u8, Operation::Success as u8, Operation::Error as u8];

        for operation_code in response_operations {
            for instruction_code in 0..100 {
                let mut mock = MockReceiverFromSlaveController::create(
                    [Operation::None as u8, operation_code, instruction_code].to_vec());

                let mut request_controller = MockRequestsControllerRx::new(Ok(()));

                let result = mock.on_get_command(&mut request_controller);

                assert_eq!(true, mock.rx.on_rx_transfer_interrupt_called);
                assert_eq!(true, request_controller.process_response_called);
                assert_eq!(request_controller.process_response_result, mock.rx.receiver_result.unwrap());
                assert_eq!(request_controller.process_response_result, result);
                assert_eq!(None, mock.signal_receiver.on_signal__signal_data);
                assert_eq!(None, mock.signal_receiver.on_signal_error__params);

                let request_controller_result_errors = [Errors::NoRequestsFound, Errors::DataCorrupted,
                    Errors::InstructionNotRecognized(0), Errors::OperationNotRecognized(0)];

                for request_controller_result_error in request_controller_result_errors {
                    let mut request_controller =
                        MockRequestsControllerRx::new(Err(request_controller_result_error));

                    let result = mock.on_get_command(&mut request_controller);

                    assert_eq!(true, mock.rx.on_rx_transfer_interrupt_called);
                    assert_eq!(true, request_controller.process_response_called);
                    assert_eq!(request_controller.process_response_result, mock.rx.receiver_result.unwrap());
                    assert_eq!(request_controller.process_response_result, result);
                    assert_eq!(None, mock.signal_receiver.on_signal__signal_data);
                    assert_eq!(None, mock.signal_receiver.on_signal_error__params);
                }
            }
        }
    }

    #[test]
    fn test_on_get_command_should_return_error_on_wrong_signal() {
        let operation_code = Operation::Signal as u8;
        let correct_signals = [Signals::GetTimeStamp as u8, Signals::MonitoringStateChanged as u8,
            Signals::StateFixTry as u8, Signals::ControlStateChanged as u8, Signals::RelayStateChanged as u8];

        for instruction_code in 0..50 {
            if !correct_signals.contains(&instruction_code) {

                let mut mock = MockReceiverFromSlaveController::create([
                    Operation::None as u8, operation_code, instruction_code].to_vec());

                let mut request_controller = MockRequestsControllerRx::new(Ok(()));

                let result = mock.on_get_command(&mut request_controller);

                assert_eq!(true, mock.rx.on_rx_transfer_interrupt_called);
                assert_eq!(false, request_controller.process_response_called);
                assert_eq!(Ok(()), mock.rx.receiver_result.unwrap());
                assert_eq!(Ok(()), result);
                assert_eq!(None, mock.signal_receiver.on_signal__signal_data);
                assert_eq!(Some((instruction_code, ErrorCode::EInstructionUnrecognized, false)), mock.signal_receiver.on_signal_error__params);
            }
        }
    }

    #[test]
    fn test_on_get_command_should_return_error_on_parse_error_for_correct_signals() {
        let operation_code = Operation::Signal as u8;
        let correct_signals = [Signals::GetTimeStamp, Signals::MonitoringStateChanged,
            Signals::StateFixTry, Signals::ControlStateChanged, Signals::RelayStateChanged];

        let parse_error_codes = [ErrorCode::ERequestDataNoValue, ErrorCode::EInstructionUnrecognized, ErrorCode::ECommandEmpty,
            ErrorCode::ECommandSizeOverflow, ErrorCode::EInstructionWrongStart,
            ErrorCode::EWriteMaxAttemptsExceeded, ErrorCode::EUndefinedOperation,
            ErrorCode::ERelayCountOverflow, ErrorCode::ERelayCountAndDataMismatch,
            ErrorCode::ERelayIndexOutOfRange, ErrorCode::ESwitchCountMaxValueOverflow,
            ErrorCode::EControlInterruptedPinNotAllowedValue, ErrorCode::ERelayNotAllowedPinUsed,
            ErrorCode::EUndefinedCode(0), ErrorCode::EUndefinedCode(1), ErrorCode::EUndefinedCode(2),
            ErrorCode::EUndefinedCode(3), ErrorCode::EUndefinedCode(127), ErrorCode::EUndefinedCode(255)];

        for instruction_code in correct_signals {
            let mut mock = MockReceiverFromSlaveController::create([
                Operation::None as u8, operation_code, instruction_code as u8].to_vec());

            let mut request_controller = MockRequestsControllerRx::new(Ok(()));

            for parse_error_code in parse_error_codes {
                mock.signals_parser.parse_result = Err(parse_error_code);

                let result = mock.on_get_command(&mut request_controller);

                assert_eq!(true, mock.rx.on_rx_transfer_interrupt_called);
                assert_eq!(false, request_controller.process_response_called);
                assert_eq!(Ok(()), mock.rx.receiver_result.unwrap());
                assert_eq!(Ok(()), result);
                assert_eq!(None, mock.signal_receiver.on_signal__signal_data);
                assert_eq!(Some((instruction_code as u8, parse_error_code, false)), mock.signal_receiver.on_signal_error__params);
            }
        }
    }

    #[test]
    fn test_on_get_command_should_proxy_parses_correct_signals() {
        let operation_code = Operation::Signal as u8;
        let correct_signals = [Signals::GetTimeStamp, Signals::MonitoringStateChanged,
            Signals::StateFixTry, Signals::ControlStateChanged, Signals::RelayStateChanged];

        let parsed_datas = [None, Some(RelaySignalData{
            relative_timestamp: RelativeSeconds::new(0x12345678_u32),
            relay_idx: 1,
            is_on: false,
            is_called_internally: Some(false),
        }),
        Some(RelaySignalData{
            relative_timestamp: RelativeSeconds::new(2547852),
            relay_idx: 12,
            is_on: true,
            is_called_internally: None,
        }), ];

        for instruction_code in correct_signals {
            let mut mock = MockReceiverFromSlaveController::create([
                Operation::None as u8, operation_code, instruction_code as u8].to_vec());

            let mut request_controller = MockRequestsControllerRx::new(Ok(()));

            for relay_signal_data in parsed_datas {
                mock.signals_parser.parse_result = Ok(SignalData{
                    instruction: instruction_code,
                    relay_signal_data: None});

                let result = mock.on_get_command(&mut request_controller);

                assert_eq!(true, mock.rx.on_rx_transfer_interrupt_called);
                assert_eq!(false, request_controller.process_response_called);
                assert_eq!(Ok(()), mock.rx.receiver_result.unwrap());
                assert_eq!(Ok(()), result);
                assert_eq!(Some(mock.signals_parser.parse_result.unwrap()), mock.signal_receiver.on_signal__signal_data);
                assert_eq!(None, mock.signal_receiver.on_signal_error__params);
            }
        }
    }

    struct MockTransmitterToSlaveController {
        start_transfer_called: bool,
        call_writer: bool,
        start_transfer_result: Result<(), Errors>,
        buffer: MockTxBuffer,
    }

    impl MockTransmitterToSlaveController {
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

    impl Sender<MockTxBuffer> for MockTransmitterToSlaveController {
        fn start_transfer<F: FnOnce(&mut MockTxBuffer) -> Result<(), Errors>>(&mut self, writter: F) -> Result<(), Errors> {
            self.start_transfer_called = true;
            if self.call_writer {
                writter(&mut self.buffer)?;
            }
            self.start_transfer_result
        }
    }

    struct MockRequestsControllerTx {
        check_request_result: Result<(), Errors>,
        check_request_result_called: RefCell<bool>,
        add_sent_request_called: bool,
        add_sent_request_parameter: Option<SentRequest>,
    }

    impl MockRequestsControllerTx {
        pub fn new (check_request_result: Result<(), Errors>) -> Self {
            Self {
                check_request_result,
                check_request_result_called: RefCell::new(false),
                add_sent_request_called: false,
                add_sent_request_parameter: None,
            }
        }
    }

    impl RequestsControllerTx for MockRequestsControllerTx {

        fn check_request(&self, instruction_code: DataInstructionCodes) -> Result<(), Errors> {
            *self.check_request_result_called.borrow_mut() = true;
            self.check_request_result
        }

        fn add_sent_request(&mut self, request: SentRequest) {
            self.add_sent_request_called = true;
            self.add_sent_request_parameter = Some(request);
        }
    }

    struct MockRequestsControllerRxCallData {
        operation_code: u8,
        instruction_code: u8,
        data: Vec<u8>,
    }

    impl MockRequestsControllerRxCallData {
        pub fn new(operation_code: u8, instruction_code: u8, data: Vec<u8>) -> Self {
            Self {
                operation_code,
                instruction_code,
                data,
            }
        }
    }

    struct MockRequestsControllerRx {
        process_response_called: bool,
        process_response_result: Result<(), Errors>,
        test_data: Option<MockRequestsControllerRxCallData>,
    }

    impl MockRequestsControllerRx {
        pub fn new (process_response_result: Result<(), Errors>) -> Self {
            Self {
                process_response_called: false,
                process_response_result,
                test_data: None,
            }
        }
    }

    impl RequestsControllerRx for MockRequestsControllerRx {
        fn process_response(&mut self, operation_code: u8, instruction_code: u8, data: &[u8]) -> Result<(), Errors> {
            self.process_response_called = true;
            self.test_data = Some(MockRequestsControllerRxCallData::new(operation_code, instruction_code, data.to_vec()));
            self.process_response_result
        }
    }

    impl TransmitterToSlaveControllerAbstract<MockTxBuffer, MockRequestsControllerTx> for MockTransmitterToSlaveController {}

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
/*
    impl DataInstruction for MockIntruction {
        fn code(&self) -> DataInstructionCodes {
            self.code
        }

        fn serialize<B: BufferWriter>(&self, buffer: &mut B) -> Result<(), Errors> {
            *self.serialize_called.borrow_mut() = true;
            self.serialize_result
        }
    }
*/
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
        on_rx_transfer_interrupt_called: bool,
        receiver_result: Option<Result<(), Errors>>,
    }

    impl MockReceiver {
        pub fn new(data: Vec<u8>) -> Self {
            Self {
                data,
                on_rx_transfer_interrupt_called: false,
                receiver_result: None,
            }
        }
    }

    impl Receiver for MockReceiver {
        fn on_rx_transfer_interrupt<F: FnOnce(&[u8]) -> Result<(), Errors>> (&mut self, receiver: F) -> Result<(), Errors> {
            self.on_rx_transfer_interrupt_called = true;
            let res = receiver(&self.data.as_slice());
            self.receiver_result = Some(res);
            res
        }
    }

    struct MockSignalReceiver {
        on_signal__signal_data: Option<SignalData>,
        on_signal_error__params: Option<(u8, ErrorCode, bool)>,
    }

    impl SignalsReceiver for MockSignalReceiver {
        fn on_signal(&mut self, signal_data: SignalData) {
            self.on_signal__signal_data = Some(signal_data);
        }
        fn on_signal_error(&mut self, instruction_code: u8, error_code: ErrorCode, sent: bool) {
            self.on_signal_error__params = Some((instruction_code, error_code, sent));
        }
    }

    struct MockRequestHandler {
        on_request_success__params__checker: Box<dyn FnMut(&SentRequest) -> ()>,
        on_request_error__params__checker: Box<dyn FnMut(&SentRequest, ErrorCode) -> ()>,
        on_request_parse_error__params__checker: Box<dyn FnMut(&SentRequest, Errors, &[u8]) -> ()>,
        on_request_response__params__checker: Box<dyn FnMut(&SentRequest, DataInstructions) -> ()>,
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
        fn on_request_success(&mut self, request: &SentRequest) {
            (self.on_request_success__params__checker)(request);
        }
        fn on_request_error(&mut self, request: &SentRequest, error_code: ErrorCode) {
            (self.on_request_error__params__checker)(request, error_code);
        }
        fn on_request_parse_error(&mut self, request: &SentRequest, error: Errors, data: &[u8]) {
            (self.on_request_parse_error__params__checker)(request, error, data);
        }
        fn on_request_response(&mut self, request: &SentRequest, response: DataInstructions) {
            (self.on_request_response__params__checker)(request, response);
        }

    }


    struct MockReceiverFromSlaveController {
        rx: MockReceiver,
        signal_receiver: MockSignalReceiver,
        signals_parser: MockSignalsParser,
    }


    impl MockReceiverFromSlaveController {
        pub fn create(data: Vec<u8>) -> Self {
            let signal_receiver = MockSignalReceiver {
                on_signal__signal_data: None,
                on_signal_error__params: None,
            };
            let rx = MockReceiver::new(data);

            let signals_data = SignalData {
                instruction: Signals::GetTimeStamp,
                relay_signal_data: None,
            };
            let signals_parser = MockSignalsParser::new(Ok(signals_data));

            Self {
                rx,
                signal_receiver: signal_receiver,
                signals_parser,
            }
        }

    }

    struct MockSignalsParserTestData {
        parse_called: bool,
        parse_instruction: Option<Signals>,
        parse_data: Vec<u8>,
    }

    impl MockSignalsParserTestData {
        fn new() -> Self {
            Self {
                parse_called: false,
                parse_instruction : None,
                parse_data: Vec::new(),
            }
        }
    }

    struct MockSignalsParser {
        parse_result: Result<SignalData, ErrorCode>,
        test_data: RefCell<MockSignalsParserTestData>,
    }

    impl MockSignalsParser {
        pub fn new(parse_result: Result<SignalData, ErrorCode>) -> Self {
            let test_data = RefCell::new(MockSignalsParserTestData::new());
            Self {
                parse_result,
                test_data,
            }
        }
    }

    impl SignalsParser for MockSignalsParser {
        fn parse(&self, instruction: Signals, data: &[u8]) -> Result<SignalData, ErrorCode> {
            let mut test_data = self.test_data.borrow_mut();
            test_data.parse_called = true;
            test_data.parse_instruction = Some(instruction);
            test_data.parse_data.copy_from_slice(data);
            self.parse_result
        }
    }


    impl ReceiverFromSlaveControllerAbstract<MockSignalReceiver, MockReceiver, MockRequestsControllerRx, MockSignalsParser>
    for MockReceiverFromSlaveController
    {
        #[inline(always)]
        fn slice(&mut self) -> (&mut MockReceiver, &mut MockSignalReceiver, &MockSignalsParser) {
            let MockReceiverFromSlaveController { rx, signal_receiver, signals_parser } = self;
            (rx, signal_receiver, signals_parser)
        }
    }





}