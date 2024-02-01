#![deny(unsafe_code)]

use embedded_dma::ReadBuffer;
use crate::errors::Errors;
use crate::hal_ext::rtc_wrapper::RelativeMillis;
use crate::hal_ext::serial_transfer::Sender;
use crate::services::slave_controller_link::domain::{DataInstruction, ErrorCode, Operation, OperationCodes};
use crate::services::slave_controller_link::requests_controller::{RequestsControllerTx, SentRequest};
use crate::utils::dma_read_buffer::BufferWriter;

pub trait RequestsSender<RCT>
    where
        RCT: RequestsControllerTx,
{
    fn send_request<I: DataInstruction>(&mut self, operation: Operation, instruction: I, timestamp: RelativeMillis, request_controller: &mut RCT) -> Result<Option<u32>, Errors>;
}

pub trait ErrorsSender {
    fn send_error(&mut self, instruction_code: u8, error_code: ErrorCode) -> Result<(), Errors>;
}

pub struct TransmitterToSlaveController<TxBuff, S>
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

    #[inline(always)]
    pub fn inner_tx(&mut self) -> &mut S {
        &mut self.tx
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

impl <TxBuff, RCT, S> RequestsSender<RCT> for TransmitterToSlaveController<TxBuff, S>
    where
        TxBuff: ReadBuffer + BufferWriter,
        RCT: RequestsControllerTx,
        S: Sender<TxBuff>,
{
    fn send_request<I: DataInstruction>(&mut self, operation: Operation, instruction: I, timestamp: RelativeMillis, request_controller: &mut RCT) -> Result<Option<u32>, Errors> {

        let id = request_controller.check_request(instruction.code())?;

        self.start_transfer(|buffer| {
            buffer.clear();
            buffer.add_u8(OperationCodes::None as u8)?;
            buffer.add_u8(operation as u8)?;
            buffer.add_u8(instruction.code() as u8)?;
            if let Some(id) = id {
                buffer.add_u32(id)?;
            }
            instruction.serialize(buffer)
        })?;

        request_controller.add_sent_request(SentRequest::new(
            id, operation, instruction.code(), timestamp));
        Ok(id)
    }
}


#[cfg(test)]
mod tests {
    #![allow(unsafe_code)]

    use alloc::rc::Rc;
    use alloc::vec::Vec;
    use core::cell::RefCell;
    use super::*;
    use rand::prelude::*;
    use crate::errors::DMAError;
    use crate::services::slave_controller_link::domain::{Conversation, DataInstructionCodes, DataInstructions, Signals};

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
        assert_eq!(4, mock.borrow().buffer.add_u8_arguments.len());
        assert_eq!(OperationCodes::None as u8, mock.borrow().buffer.add_u8_arguments[0]);
        assert_eq!(OperationCodes::Error as u8, mock.borrow().buffer.add_u8_arguments[1]);
        assert_eq!(instruction_code, mock.borrow().buffer.add_u8_arguments[2]);
        assert_eq!(sending_error.discriminant(), mock.borrow().buffer.add_u8_arguments[3]);
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
                assert_eq!(4, mock.borrow().buffer.add_u8_arguments.len());
                assert_eq!(instruction_code, mock.borrow().buffer.add_u8_arguments[2]);
                assert_eq!(sending_error.discriminant(), mock.borrow().buffer.add_u8_arguments[3]);
            }
        }
    }

    #[test]
    fn test_send_request_v1() {
        let start_transfer_result = Ok(());
        let operation = Operation::Set;
        let instruction = Rc::new(MockIntruction::new(DataInstructionCodes::Id, Ok(())));
        let instruction_code = instruction.code();
        let mut rng = rand::thread_rng();
        let timestamp = RelativeMillis::new(rng.gen_range(1..u32::MAX));

        let mock = Rc::new(RefCell::new(MockSender::new(true, start_transfer_result)));
        let mut tested = TransmitterToSlaveController::new(mock.clone());

        let mut mock_request_controller = MockRequestsControllerTx::new(Ok(None));

        let result = tested.send_request(operation, instruction.clone(), timestamp, &mut mock_request_controller);

        assert_eq!(Ok(None), result);
        //check controller operations
        assert_eq!(Some(instruction.code), *mock_request_controller.check_request_parameter.borrow());
        assert_eq!(Some(SentRequest::new(None, operation, instruction.code(), timestamp)),
                   mock_request_controller.add_sent_request_parameter);
        //check buffer operations
        assert_eq!(true, mock.borrow().buffer.cleared);
        assert_eq!(3, mock.borrow().buffer.add_u8_arguments.len());
        assert_eq!(OperationCodes::None as u8, mock.borrow().buffer.add_u8_arguments[0]);
        assert_eq!(operation as u8, mock.borrow().buffer.add_u8_arguments[1]);
        assert_eq!(instruction_code as u8, mock.borrow().buffer.add_u8_arguments[2]);
        assert_eq!(true, *instruction.serialize_called.borrow());
    }

    #[test]
    fn test_send_request_v2() {
        let start_transfer_result = Ok(());
        let operation = Operation::Set;
        let instruction = Rc::new(MockIntruction::new(DataInstructionCodes::Id, Ok(())));
        let instruction_code = instruction.code();
        let mut rng = rand::thread_rng();
        let timestamp = RelativeMillis::new(rng.gen_range(1..u32::MAX));

        let mock = Rc::new(RefCell::new(MockSender::new(true, start_transfer_result)));
        let mut tested = TransmitterToSlaveController::new(mock.clone());

        let add_sent_request_result = rng.gen_range(1..u32::MAX);
        let mut mock_request_controller = MockRequestsControllerTx::new(Ok(Some(add_sent_request_result)));

        let result = tested.send_request(operation, instruction.clone(), timestamp, &mut mock_request_controller);

        assert_eq!(Ok(Some(add_sent_request_result)), result);
        //check controller operations
        assert_eq!(Some(instruction.code), *mock_request_controller.check_request_parameter.borrow());
        assert_eq!(Some(SentRequest::new(Some(add_sent_request_result), operation, instruction.code(), timestamp)),
                   mock_request_controller.add_sent_request_parameter);
        //check buffer operations
        assert_eq!(true, mock.borrow().buffer.cleared);
        assert_eq!(3, mock.borrow().buffer.add_u8_arguments.len());
        assert_eq!(OperationCodes::None as u8, mock.borrow().buffer.add_u8_arguments[0]);
        assert_eq!(operation as u8, mock.borrow().buffer.add_u8_arguments[1]);
        assert_eq!(instruction_code as u8, mock.borrow().buffer.add_u8_arguments[2]);
        assert_eq!(1, mock.borrow().buffer.add_u32_arguments.len());
        assert_eq!(add_sent_request_result, mock.borrow().buffer.add_u32_arguments[0]);
        assert_eq!(true, *instruction.serialize_called.borrow());
    }

    #[test]
    fn test_send_request_returns_all_check_request_result_errors() {
        let start_transfer_result = Ok(());
        let operation = Operation::Set;
        let mut rng = rand::thread_rng();
        let timestamp = RelativeMillis::new(rng.gen_range(1..u32::MAX));

        let mock = Rc::new(RefCell::new(MockSender::new(true, start_transfer_result)));
        let mut tested = TransmitterToSlaveController::new(mock.clone());

        let errors = [
            Errors::RequestsLimitReached,
            Errors::RequestsNeedsCacheAlreadySent,
        ];

        let mut mock_request_controller = MockRequestsControllerTx::new(
            Ok(None));

        for error in errors {
            let instruction = DataInstructions::Id(Conversation::Data(rng.gen_range(1..u32::MAX)));
            let instruction_code = instruction.code();
            *mock_request_controller.check_request_parameter.borrow_mut() = None;
            mock_request_controller.check_request_result = Err(error);
            let result =
                tested.send_request(operation, instruction, timestamp, &mut mock_request_controller);

            assert_eq!(Some(instruction_code), *mock_request_controller.check_request_parameter.borrow());
            assert_eq!(None, mock_request_controller.add_sent_request_parameter);
            assert_eq!(Err(error), result);
        }
    }

    #[test]
    fn test_send_request_returns_all_start_transfer_errors() {
        let start_transfer_result = Ok(());
        let operation = Operation::Set;
        let mut rng = rand::thread_rng();
        let timestamp = RelativeMillis::new(rng.gen_range(1..u32::MAX));

        let mock = Rc::new(RefCell::new(MockSender::new(true,
                                                        start_transfer_result)));
        let mut tested =
            TransmitterToSlaveController::new(mock.clone());

        let errors = [
            Errors::TransferInProgress,
            Errors::NoBufferAvailable,
            Errors::DmaError(DMAError::Overrun(())),
            Errors::DmaError(DMAError::NotReady(())),
            Errors::DmaError(DMAError::SmallBuffer(())),
        ];

        let mut mock_request_controller = MockRequestsControllerTx::new(
            Ok(None));
        for error in errors {
            let instruction = DataInstructions::Id(Conversation::Data(rng.gen_range(1..u32::MAX)));
            let instruction_code = instruction.code();
            mock.borrow_mut().start_transfer_result = Err(error);
            let result =
                tested.send_request(operation, instruction, timestamp, &mut mock_request_controller);

            assert_eq!(Some(instruction_code), *mock_request_controller.check_request_parameter.borrow());
            assert_eq!(None, mock_request_controller.add_sent_request_parameter);
            assert_eq!(Err(error), result);
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
        check_request_result: Result<Option<u32>, Errors>,
        check_request_parameter: RefCell<Option<DataInstructionCodes>>,
        add_sent_request_parameter: Option<SentRequest>,
    }

    impl MockRequestsControllerTx {
        pub fn new (check_request_result: Result<Option<u32>, Errors>) -> Self {
            Self {
                check_request_result,
                check_request_parameter: RefCell::new(None),
                add_sent_request_parameter: None,
            }
        }
    }

    impl RequestsControllerTx for MockRequestsControllerTx {

        fn check_request(&mut self, value: DataInstructionCodes) -> Result<Option<u32>, Errors> {
            *self.check_request_parameter.borrow_mut() = Some(value);
            self.check_request_result
        }

        fn add_sent_request(&mut self, request: SentRequest) {
            self.add_sent_request_parameter = Some(request);
        }
    }

    struct MockIntruction {
        code: DataInstructionCodes,
        serialize_called: RefCell<bool>,
        serialize_result: Result<(), Errors>,
    }

    impl MockIntruction {
        fn new(code: DataInstructionCodes, serialize_result: Result<(), Errors>) -> Self {
            Self {
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

        fn serialize<B: BufferWriter>(&self, _: &mut B) -> Result<(), Errors> {
            *self.serialize_called.borrow_mut() = true;
            self.serialize_result
        }
    }

    impl DataInstruction for Rc<MockIntruction> {
        fn code(&self) -> DataInstructionCodes {
            self.code
        }

        fn serialize<B: BufferWriter>(&self, _: &mut B) -> Result<(), Errors> {
            *self.serialize_called.borrow_mut() = true;
            self.serialize_result
        }

    }
    const BUFFER_SIZE: usize = 20;

    struct MockTxBuffer {
        buffer: [u8; BUFFER_SIZE],
        add_u8_arguments: Vec<u8>,
        add_u32_arguments: Vec<u32>,
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

        fn add_str(&mut self, _: &str) -> Result<(), Errors> {
            Ok(())
        }

        fn add(&mut self, _: &[u8]) -> Result<(), Errors> {
            Ok(())
        }

        fn add_u8(&mut self, byte: u8) -> Result<(), Errors> {
            self.add_u8_arguments.push(byte);
            Ok(())
        }

        fn add_u16(&mut self, _: u16) -> Result<(), Errors> {
            Ok(())
        }

        fn add_u32(&mut self, value: u32) -> Result<(), Errors> {
            self.add_u32_arguments.push(value);
            Ok(())
        }

        fn add_u64(&mut self, _: u64) -> Result<(), Errors> {
            Ok(())
        }

        fn clear(&mut self) {
            self.add_u8_arguments.clear();
            self.add_u32_arguments.clear();
            self.cleared = true;
        }

    }

    impl MockTxBuffer {
        fn new() -> Self {
            Self {
                buffer: [0_u8; BUFFER_SIZE],
                add_u8_arguments: Vec::new(),
                add_u32_arguments: Vec::new(),
                cleared: false,
            }
        }
    }
}