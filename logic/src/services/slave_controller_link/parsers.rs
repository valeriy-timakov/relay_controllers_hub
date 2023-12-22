use alloc::boxed::Box;
use core::mem::size_of;
use crate::errors::Errors;
use crate::hal_ext::rtc_wrapper::RelativeSeconds;
use crate::services::slave_controller_link::domain::{AllData, ContactsWaitData, Conversation, Data, DataInstructionCodes, DataInstructions, ErrorCode, Extractor, FixDataContainer, Operation, OperationCodes, RelaysSettings, Request, Signals, StateSwitchDatas, Version};



pub fn init_slave_controllers() {
    init_cache_getters();
}



const fn max_of(size1: usize, size2: usize, size3: usize, size4: usize, size5: usize) -> usize {
    let mut max = size1;
    if size2 > max { max = size2; }
    if size3 > max { max = size3; }
    if size4 > max { max = size4; }
    if size5 > max { max = size5; }
    max
}

fn get_next_static_buffer_index() -> Result<usize, Errors> {
    let static_buffers_idx = unsafe {
        if INSTANCES_COUNT >= MAX_INSTANCES_COUNT {
            return Err(Errors::SlaveControllersInstancesMaxCountReached);
        }
        let instances_count = INSTANCES_COUNT;
        INSTANCES_COUNT += 1;
        instances_count as usize
    };
    Ok(static_buffers_idx)
}

const RESPONSE_BUFFER_SIZE: usize = max_of(size_of::<FixDataContainer>(), size_of::<RelaysSettings>(),
                                           size_of::<ContactsWaitData>(), size_of::<StateSwitchDatas>(), size_of::<AllData>());

type ResponseBuffer = [u8; RESPONSE_BUFFER_SIZE];

const MAX_INSTANCES_COUNT: usize = 3;
static mut STATIC_BUFFERS: [ResponseBuffer; MAX_INSTANCES_COUNT] = [ [0; RESPONSE_BUFFER_SIZE], [0; RESPONSE_BUFFER_SIZE], [0; RESPONSE_BUFFER_SIZE] ];
static mut INSTANCES_COUNT: usize = 0;

unsafe fn get_data_container_cashed<RQ: Request, D: Data + 'static>(static_buffers_idx: usize) -> Conversation<RQ, D> {
    let buf = &mut STATIC_BUFFERS[static_buffers_idx];
    let raw_ptr  = buf.as_mut_ptr() as *mut D;
    Conversation::DataCashed(&mut *raw_ptr)
}

trait CashedInstructionGetter {
    fn get(&self, static_buffers_idx: usize) -> DataInstructions;
}

#[derive(Copy, Clone)]
struct RelasySettingsCashedInstructionGetter;
impl CashedInstructionGetter for RelasySettingsCashedInstructionGetter {
    fn get(&self, static_buffers_idx: usize) -> DataInstructions {
        DataInstructions::Settings(unsafe {get_data_container_cashed(static_buffers_idx)})
    }
}

#[derive(Copy, Clone)]
struct ContactsWaitDataCashedInstructionGetter;
impl CashedInstructionGetter for ContactsWaitDataCashedInstructionGetter {
    fn get(&self, static_buffers_idx: usize) -> DataInstructions {
        DataInstructions::ContactWaitData(unsafe {get_data_container_cashed(static_buffers_idx)})
    }
}

#[derive(Copy, Clone)]
struct StateSwitchDataCashedInstructionGetter;
impl CashedInstructionGetter for StateSwitchDataCashedInstructionGetter {
    fn get(&self, static_buffers_idx: usize) -> DataInstructions {
        DataInstructions::SwitchData(unsafe {get_data_container_cashed(static_buffers_idx)})
    }
}

#[derive(Copy, Clone)]
struct FixDataContainerCashedInstructionGetter;
impl CashedInstructionGetter for FixDataContainerCashedInstructionGetter {
    fn get(&self, static_buffers_idx: usize) -> DataInstructions {
        DataInstructions::FixData(unsafe {get_data_container_cashed(static_buffers_idx)})
    }
}

#[derive(Copy, Clone)]
struct AllDataCashedInstructionGetter;
impl CashedInstructionGetter for AllDataCashedInstructionGetter {
    fn get(&self, static_buffers_idx: usize) -> DataInstructions {
        DataInstructions::All(unsafe {get_data_container_cashed(static_buffers_idx)})
    }
}


const INSTRUCTIONS_COUNT: usize = DataInstructionCodes::Last as usize;

static mut DF: [Option<Box<dyn CashedInstructionGetter>>; INSTRUCTIONS_COUNT] = [None, None, None, None, None, None, None,
    None, None, None, None, None, None, None, None, None, None, None, None, None, None, None, None, None, None];

fn init_cache_getters() {
    unsafe {
        DF[DataInstructionCodes::Settings as usize] = Some(Box::new(RelasySettingsCashedInstructionGetter));
        DF[DataInstructionCodes::ContactWaitData as usize] = Some(Box::new(ContactsWaitDataCashedInstructionGetter));
        DF[DataInstructionCodes::SwitchData as usize] = Some(Box::new(StateSwitchDataCashedInstructionGetter));
        DF[DataInstructionCodes::FixData as usize] = Some(Box::new(FixDataContainerCashedInstructionGetter));
        DF[DataInstructionCodes::All as usize] = Some(Box::new(AllDataCashedInstructionGetter));
    }
}

fn cache_getter(code: DataInstructionCodes) -> Option< &'static Box<dyn CashedInstructionGetter>> {
    unsafe {
        DF[code as usize].as_ref()
    }
}

pub trait ResponseBodyParser {
    fn request_needs_cache(&self, instruction: DataInstructionCodes) -> bool;
    fn parse_id<'a>(&self, data: &'a[u8]) -> Result<(Option<u32>, &'a[u8]), Errors>;
    fn parse(&self, instruction: DataInstructionCodes, data: &[u8]) -> Result<DataInstructions, Errors>;
    fn slave_controller_version(&self) -> Version;
}

pub struct ResponseBodyParserImpl {
    static_buffers_idx: usize,
    slave_controller_version: Version,
}

impl ResponseBodyParserImpl {
    pub fn create(slave_controller_version: Version) -> Result<Self, Errors> {
        let static_buffers_idx = get_next_static_buffer_index()?;
        Ok(Self {
            static_buffers_idx,
            slave_controller_version,
        })
    }
}

impl ResponseBodyParser for ResponseBodyParserImpl {
    fn parse(&self, instruction: DataInstructionCodes, data: &[u8]) -> Result<DataInstructions, Errors> {
        match cache_getter(instruction) {
            Some(getter) => {
                let mut cached_instruction = getter.get(self.static_buffers_idx);
                cached_instruction.parse_from(data)?;
                Ok(cached_instruction)
            },
            None => Ok(DataInstructions::parse(instruction, data)?)
        }
    }

    fn request_needs_cache(&self, instruction: DataInstructionCodes) -> bool {
        match cache_getter(instruction) {
            Some(_) => { true },
            None => { false }
        }
    }

    fn parse_id<'a>(&self, data: &'a[u8]) -> Result<(Option<u32>, &'a[u8]), Errors> {
        match self.slave_controller_version {
            Version::V1 => {
                Ok( (None, data) )
            },
            Version::V2 => {
                if data.len() >= 4 {
                    let data = if data.len() > 4 { &data[4..] } else { &data[0..0] };
                    Ok( (Some(u32::extract(&(data)[0..4])), data) )
                } else {
                    Err(Errors::NotEnoughDataGot)
                }
            },
        }
    }


    fn slave_controller_version(&self) -> Version {
        self.slave_controller_version
    }

}

#[derive(Copy, Clone, Debug)]
pub enum PayloadParserResult<SP, RP, RBP, RPP>
    where 
        SP: SignalParser,
        RP: ResponseParser<RBP, RPP>,
        RBP: ResponseBodyParser,
        RPP: ResponsePostParser,
{
    ResponsePayload(RP),
    SignalPayload(SP),
    _fake((RBP, RPP)),
}

pub struct ResponsePayload {
    operation: Operation,
}

impl <'a> ResponsePayload {
    fn new(operation: Operation) -> Self {
        Self {
            operation,
        }
    }
}

pub struct SignalPayload ();

pub struct ResponsePayloadParsed {
    operation: Operation,
    instruction: DataInstructionCodes,
    request_id: Option<u32>,
    needs_cache: bool,
    error_code: ErrorCode,
}

impl <'a> ResponsePayloadParsed {
    pub fn new(
        operation: Operation,
        instruction: DataInstructionCodes,
        request_id: Option<u32>,
        needs_cache: bool,
        error_code: ErrorCode,
    ) -> Self {
        Self {
            operation,
            instruction,
            request_id,
            needs_cache,
            error_code,
        }
    }
}

impl <'a> ResponsePostParser for ResponsePayloadParsed {
    fn operation(&self) -> Operation {
        self.operation
    }

    fn instruction(&self) -> DataInstructionCodes {
        self.instruction
    }

    fn request_id(&self) -> Option<u32> {
        self.request_id
    }

    fn needs_cache(&self) -> bool {
        self.needs_cache
    }

    fn error_code(&self) -> ErrorCode {
        self.error_code
    }
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub struct SignalParseResult {
    signal: Signals,
    relay_signal_data: Option<RelaySignalData>,
}

impl SignalParseResult {
    pub fn signal(&self) -> Signals {
        self.signal
    }

    pub fn relay_signal_data(&self) -> Option<RelaySignalData> {
        self.relay_signal_data
    }
}

impl SignalParseResult {
    pub fn new(signal: Signals, relay_signal_data: Option<RelaySignalData>) -> Self {
        Self {
            signal,
            relay_signal_data,
        }
    }
}

pub trait PayloadParser<SP, RP, RBP, RPP>
    where
        SP: SignalParser,
        RP: ResponseParser<RBP, RPP>,
        RBP: ResponseBodyParser,
        RPP: ResponsePostParser,
{
    fn parse<'a>(&self, data: &'a[u8]) -> Result<(PayloadParserResult<SP, RP, RBP, RPP>, &'a[u8]), Errors>;
}

pub struct PayloadParserImpl ();

impl PayloadParserImpl {
    pub fn new() -> Self {
        Self {}
    }
    
    fn parse_operation(data: &[u8]) -> Result<(Operation, &[u8]), Errors> {
        let operation_code = data[0];
        let operation = if operation_code == OperationCodes::Success as u8 {
            Operation::Set
        } else if operation_code == OperationCodes::Response as u8 {
            Operation::Read
        } else if operation_code == OperationCodes::Error as u8 {
            Operation::Error
        } else if operation_code == OperationCodes::SuccessV2 as u8 {
            Operation::Set
        } else if operation_code == OperationCodes::ResponseV2 as u8 {
            Operation::Read
        } else if operation_code == OperationCodes::ErrorV2 as u8 {
            Operation::Error
        } else {
            Operation::None
        };
        let operation_result =
            if operation != Operation::None { Ok(operation) }
            else { Err(Errors::OperationNotRecognized(operation_code)) };
        operation_result.map(|operation: Operation| { (operation, &data[1..]) })
    }
}


impl <RBP: ResponseBodyParser> PayloadParser<SignalPayload, ResponsePayload, RBP, ResponsePayloadParsed> for PayloadParserImpl {
    fn parse<'a>(&self, data: &'a[u8]) -> Result<(PayloadParserResult<SignalPayload, ResponsePayload, RBP, ResponsePayloadParsed>, &'a[u8]), Errors> {
        if data.len() < 2 {
            Err(Errors::NotEnoughDataGot)
        } else if data[0] != OperationCodes::None as u8 {
            Err(Errors::CommandDataCorrupted)
        } else {
            let (operation, data) = Self::parse_operation(&data[1..])?;
            if operation.is_response() {
                Ok((PayloadParserResult::ResponsePayload(ResponsePayload::new(operation)), data))
            } else if operation.is_signal() {
                Ok((PayloadParserResult::SignalPayload(SignalPayload()), data))
            } else {
                Err(Errors::WrongIncomingOperation(operation))
            }
        }
    }
}

pub trait ResponsePostParser {
    fn operation(&self) -> Operation;
    fn instruction(&self) -> DataInstructionCodes;
    fn request_id(&self) -> Option<u32>;
    fn needs_cache(&self) -> bool;
    fn error_code(&self) -> ErrorCode;
}

pub trait ResponseParser <RBP: ResponseBodyParser, RPP: ResponsePostParser> {
    fn parse<'a>(&self, body_parser: &RBP, data: &'a[u8]) -> Result<(RPP, &'a[u8]), Errors>;
}

impl <RBP: ResponseBodyParser> ResponseParser<RBP, ResponsePayloadParsed> for ResponsePayload {
    fn parse<'a>(&self, body_parser: &RBP, data: &'a[u8]) -> Result<(ResponsePayloadParsed, &'a[u8]), Errors> {
        if data.len() < 1 {
            return Err(Errors::NotEnoughDataGot);
        }
        let (instruction_code, error_code, next_position) =
            if self.operation == Operation::Error {
                if data.len() < 2 {
                    return Err(Errors::NotEnoughDataGot);
                }
                (data[1], data[0], 2_usize)
            } else {
                (data[0], 0_u8, 1_usize)
            };
        let error_code = ErrorCode::for_code(error_code);
        let instruction = DataInstructionCodes::get(instruction_code)?;
        let data =  if data.len() > next_position { &data[next_position..] } else { &data[0..0] };
        let (request_id, data) = body_parser.parse_id(data)?;
        let needs_cache = body_parser.request_needs_cache(instruction);
        Ok((ResponsePayloadParsed {
            operation: self.operation,
            instruction,
            request_id,
            needs_cache,
            error_code,
        }, data))
    }
}

pub trait SignalParser {
    fn parse(&self, data: &[u8]) -> Result<SignalParseResult, Errors>;
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct RelaySignalData {
    relative_timestamp: RelativeSeconds,
    relay_idx: u8,
    is_on: bool,
    is_called_internally: Option<bool>,
}

impl RelaySignalData {
    pub fn new(relative_timestamp: RelativeSeconds, relay_idx: u8, is_on: bool, is_called_internally: Option<bool>) -> Self {
        Self {
            relative_timestamp,
            relay_idx,
            is_on,
            is_called_internally,
        }
    }
}

impl SignalParser for SignalPayload {
    fn parse(&self, data: &[u8]) -> Result<SignalParseResult, Errors> {
        if data.len() < 1 {
            Err(Errors::InvalidDataSize)
        } else {
            let signal = Signals::get(data[0])?;

            let relay_signal_data =
                if signal == Signals::GetTimeStamp {
                    None
                } else {
                    if data.len() < 6 {
                        return Err(Errors::InvalidDataSize);
                    }
                    let data =  &data[1..];
                    let relay_idx = data[0] & 0x0f_u8;
                    let is_on = data[0] & 0x10 > 0;
                    let mut relative_seconds = 0_u32;
                    for i in 0..4 {
                        relative_seconds |= (data[1 + i] as u32) << (8 * (3 - i));
                    }
                    let is_called_internally = if signal == Signals::RelayStateChanged {
                        Some(data[0] & 0x20 > 0)
                    } else {
                        None
                    };
                    Some(RelaySignalData{
                        relay_idx,
                        is_on,
                        relative_timestamp: RelativeSeconds::new(relative_seconds),
                        is_called_internally,
                    })
                };

            Ok(SignalParseResult {
                signal,
                relay_signal_data,
            })
        }
    }
}


/*


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


    #[test]
    fn test_on_get_command_should_return_error_on_parse_error_for_correct_signals() {
        let operation_code = OperationCodes::Signal as u8;
        let correct_signals = [Signals::GetTimeStamp, Signals::MonitoringStateChanged,
            Signals::StateFixTry, Signals::ControlStateChanged, Signals::RelayStateChanged];

        let parse_errors = [Errors::InstructionNotRecognized(19), Errors::DataCorrupted,
            Errors::InvalidDataSize, Errors::NoRequestsFound, Errors::NotEnoughDataGot, Errors::OutOfRange,
            Errors::OperationNotRecognized(0), Errors::OperationNotRecognized(1)];

        for instruction_code in correct_signals {
            let mut controller = MockReceiverFromSlaveController::create([
                OperationCodes::None as u8, operation_code, instruction_code as u8].to_vec());

            let mut request_controller = MockRequestsControllerRx::new(Ok(()));

            for parse_error in parse_errors {
                controller.signals_parser.parse_result = Err(parse_error);

                let result = controller.on_get_command(&mut request_controller);

                assert_eq!(true, controller.rx.on_rx_transfer_interrupt_called);
                assert_eq!(false, request_controller.process_response_called);
                assert_eq!(Ok(()), controller.rx.receiver_result.unwrap());
                assert_eq!(Ok(()), result);
                assert_eq!(None, controller.signal_receiver.on_signal__signal_data);
                assert_eq!(Some((instruction_code, parse_error, false)), controller.signal_receiver.on_signal_error__params);
            }
        }
    }
    
    
    #[test]
    fn test_on_get_command_should_report_error_not_enough_data_error_on_low_bytes_message() {

        let datas = Vec::from([[].to_vec(), [1].to_vec()]);

        for data in datas {
            let mock_receiver = MockReceiver::new(data);
            let handled_error: RefCell<Option<Errors>> = RefCell::new(None);
            let mock_error_handler = |error: Errors| {
                *handled_error.borrow_mut() = Some(error);
            };
            let mut controller = ReceiverFromSlaveController::new(
                mock_receiver, MockSignalController::new(),
                MockRequestsControllerRx::new(), &mock_error_handler);


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
        let handled_error: RefCell<Option<Errors>> = RefCell::new(None);
        let mock_error_handler = |error: Errors| {
            *handled_error.borrow_mut() = Some(error);
        };
        let mut controller = ReceiverFromSlaveController::new(
            mock_receiver, MockSignalController::new(),
            MockRequestsControllerRx::new(), &mock_error_handler);

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
            let handled_error: RefCell<Option<Errors>> = RefCell::new(None);
            let mock_error_handler = |error: Errors| {
                *handled_error.borrow_mut() = Some(error);
            };
            let mut controller = ReceiverFromSlaveController::new(
                mock_receiver, MockSignalController::new(),
                MockRequestsControllerRx::new(), &mock_error_handler);

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
    fn test_on_get_command_should_report_error_on_parse_operation_error() {
        let mut rng = rand::thread_rng();
        let data = [rng.gen_range(1..u8::MAX), rng.gen_range(1..u8::MAX),
            rng.gen_range(1..u8::MAX)].to_vec();
        let mock_receiver = MockReceiver::new(data);
        let handled_error: RefCell<Option<Errors>> = RefCell::new(None);
        let mock_error_handler = |error: Errors| {
            *handled_error.borrow_mut() = Some(error);
        };

        let error = Errors::CommandDataCorrupted;
        let mock_parser = MockPayloadParser::default(Err(error));
        let payload_parser_producer_param: Option<Vec<u8>> = None;
        let mock_parse_payload_producer = |data: &[u8]| {
            payload_parser_producer_param.replace(data.to_vec());
            Ok(mock_parser)
        };

        let mut controller = ReceiverFromSlaveControllerTestable::new(
            mock_receiver, MockSignalController::new(),
            MockRequestsControllerRx::new(), &mock_error_handler,
            mock_parse_payload_producer);

        controller.on_get_command();

        assert_eq!(true, controller.rx.on_rx_transfer_interrupt_called);
        assert_eq!(Some(data), payload_parser_producer_param.borrow());
        assert_eq!(true, mock_parser.parse_operation_called);
        assert_eq!(Err(error), controller.rx.receiver_result.unwrap());
        assert_eq!(Some(error), *handled_error.borrow());
        //nothing other should be called
        assert_eq!(None, controller.signal_controller.process_signal_params);
        assert_eq!(None, controller.requests_controller_rx.process_response_params);
    }

    #[test]
    fn test_on_get_command_should_renurn_not_recognized_on_unknown() {
        let not_request_not_signal_operations = [Operation::None, Operation::Success,
            Operation::Error, Operation::Response, Operation::Command];
        let mut rng = rand::thread_rng();

        for operation in not_request_not_signal_operations {
            let data = [rng.gen_range(1..u8::MAX), rng.gen_range(1..u8::MAX),
                rng.gen_range(1..u8::MAX)].to_vec();
            let mock_receiver = MockReceiver::new(data);
            let handled_error: RefCell<Option<Errors>> = RefCell::new(None);
            let mock_error_handler = |error: Errors| {
                *handled_error.borrow_mut() = Some(error);
            };

            let mock_parser = MockPayloadParser::default(Ok(operation));
            let payload_parser_producer_param: Option<Vec<u8>> = None;
            let mock_parse_payload_producer = |data: &[u8]| {
                payload_parser_producer_param.replace(data.to_vec());
                Ok(mock_parser)
            };

            let mut controller = ReceiverFromSlaveControllerTestable::new(
                mock_receiver, MockSignalController::new(),
                MockRequestsControllerRx::new(), &mock_error_handler,
                mock_parse_payload_producer);

            controller.on_get_command();

            assert_eq!(true, controller.rx.on_rx_transfer_interrupt_called);
            assert_eq!(Some(data), payload_parser_producer_param.borrow());
            assert_eq!(true, mock_parser.parse_operation_called);
            assert_eq!(Err(Errors::WrongIncomingOperation(operation)), controller.rx.receiver_result.unwrap());
            assert_eq!(Some(Errors::WrongIncomingOperation(operation)), *handled_error.borrow());
            //nothing other should be called
            assert_eq!(None, controller.signal_controller.process_signal_params);
            assert_eq!(None, controller.requests_controller_rx.process_response_params);
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
                   signal_controller.signal_handler.on_signal_error_params);
        //should not call other methods
        assert_eq!(None, signal_controller.signal_handler.on_signal_signal_data);
        assert_eq!(None, signal_controller.signal_parser.test_data.borrow().parse_instruction_params);
        assert_eq!(None, signal_controller.signal_parser.test_data.borrow().parse_params);
    }
 */

