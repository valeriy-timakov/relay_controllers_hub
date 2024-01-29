#![allow(unsafe_code)]

use alloc::boxed::Box;
use core::mem::size_of;
use crate::errors::Errors;
use crate::services::slave_controller_link::domain::{AllData, ContactsWaitData, Conversation, Data, DataInstructionCodes, DataInstructions, ErrorCode, Extractor, FixDataContainer, Operation, OperationCodes, RelaysSettings, Request, SignalData, Signals, StateSwitchDatas, Version};



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

pub fn init_cache_getters() {
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
    fn parse(&self, instruction: DataInstructionCodes, data: &[u8]) -> Result<DataInstructions, Errors>;
}

pub struct ResponseBodyParserImpl {
    static_buffers_idx: usize,
}

impl ResponseBodyParserImpl {
    pub fn create() -> Result<Self, Errors> {
        let static_buffers_idx = get_next_static_buffer_index()?;
        Ok(Self {
            static_buffers_idx,
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

}

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum PayloadParserResult<SP, RP>
    where 
        SP: SignalParser,
        RP: ResponseParser,
{
    ResponsePayload(RP),
    SignalPayload(SP),
}

#[derive(Debug, PartialEq)]
pub struct ResponseParserImpl {
    operation: Operation,
}

impl <'a> ResponseParserImpl {
    fn new(operation: Operation) -> Self {
        Self {
            operation,
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct SignalParserImpl();

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct ResponseData {
    operation: Operation,
    instruction: DataInstructionCodes,
    request_id: Option<u32>,
    error_code: ErrorCode,
}

impl <'a> ResponseData {
    pub fn new(
        operation: Operation,
        instruction: DataInstructionCodes,
        request_id: Option<u32>,
        error_code: ErrorCode,
    ) -> Self {
        Self {
            operation,
            instruction,
            request_id,
            error_code,
        }
    }

    pub fn operation(&self) -> Operation {
        self.operation
    }

    pub fn instruction(&self) -> DataInstructionCodes {
        self.instruction
    }

    pub fn request_id(&self) -> Option<u32> {
        self.request_id
    }

    pub fn error_code(&self) -> ErrorCode {
        self.error_code
    }
}

pub trait PayloadParser<SP, RP>
    where
        SP: SignalParser,
        RP: ResponseParser,
{
    fn parse<'a>(&self, data: &'a[u8]) -> Result<(PayloadParserResult<SP, RP>, &'a[u8]), Errors>;
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
        } else if operation_code == OperationCodes::Signal as u8 {
            Operation::Signal
        } else {
            Operation::None
        };
        let operation_result =
            if operation != Operation::None { Ok(operation) }
            else { Err(Errors::OperationNotRecognized(operation_code)) };
        operation_result.map(|operation: Operation| { (operation, &data[1..]) })
    }
}


impl PayloadParser<SignalParserImpl, ResponseParserImpl> for PayloadParserImpl {
    fn parse<'a>(&self, data: &'a[u8]) -> Result<(PayloadParserResult<SignalParserImpl, ResponseParserImpl>, &'a[u8]), Errors> {
        if data.len() < 2 {
            Err(Errors::NotEnoughDataGot)
        } else if data[0] != OperationCodes::None as u8 {
            Err(Errors::CommandDataCorrupted)
        } else {
            let (operation, data) = Self::parse_operation(&data[1..])?;
            if operation.is_response() {
                Ok((PayloadParserResult::ResponsePayload(ResponseParserImpl::new(operation)), data))
            } else if operation.is_signal() {
                Ok((PayloadParserResult::SignalPayload(SignalParserImpl()), data))
            } else {
                Err(Errors::WrongIncomingOperation(operation))
            }
        }
    }
}

pub trait ResponseParser {
    fn parse<'a>(&self, data: &'a[u8], slave_controller_version: Version) -> Result<(ResponseData, &'a[u8]), Errors>;
}

impl ResponseParser for ResponseParserImpl {
    fn parse<'a>(&self, data: &'a[u8], slave_controller_version: Version) -> Result<(ResponseData, &'a[u8]), Errors> {
        if data.len() < 1 {
            return Err(Errors::NotEnoughDataGot);
        }
        let (instruction_code, error_code, next_position) =
            if self.operation == Operation::Error {
                if data.len() < 2 {
                    return Err(Errors::SlaveError(ErrorCode::for_code(data[0])));
                }
                (data[1], data[0], 2_usize)
            } else {
                (data[0], 0_u8, 1_usize)
            };
        let error_code = ErrorCode::for_code(error_code);
        let instruction = DataInstructionCodes::get(instruction_code)?;
        let data =  if data.len() > next_position { &data[next_position..] } else { &data[0..0] };
        let (request_id, data) = match slave_controller_version {
                Version::V1 => {
                    Ok( (None, data) )
                },
                Version::V2 => {
                    if data.len() >= 4 {
                        let data_res = if data.len() > 4 { &data[4..] } else { &data[0..0] };
                        Ok( (Some(u32::extract(&(data)[0..4])), data_res) )
                    } else {
                        Err(Errors::NotEnoughDataGot)
                    }
                },
            }?;


        Ok((ResponseData {
            operation: self.operation,
            instruction,
            request_id,
            error_code,
        }, data))
    }
}


pub trait SignalParser {
    fn parse(&self, data: &[u8]) -> Result<SignalData, Errors>;
}


impl SignalParser for SignalParserImpl {
    fn parse(&self, data: &[u8]) -> Result<SignalData, Errors> {
        if data.len() < 1 {
            Err(Errors::InvalidDataSize)
        } else {
            let signal = Signals::get(data[0])?;
            let data = if data.len() > 1 { &data[1..] } else { &data[0..0] };
            SignalData::parse(signal, data)
        }
    }
}



#[cfg(test)]
mod tests {
    #![allow(unsafe_code)]

    use alloc::vec::Vec;
    use core::cell::RefCell;
    use embedded_dma::ReadBuffer;
    use super::*;
    use rand::prelude::*;
    use crate::hal_ext::rtc_wrapper::RelativeSeconds;
    use crate::services::slave_controller_link::domain::{Serializable, DataInstructions, RelaySignalData, RelaySignalDataExt, Signals, MAX_RELAYS_COUNT, State, StateFixSettings, RelayState, CyclesStatistics, SwitchCountingSettings, RelaySingleState, Parser, RelaySettings};
    use crate::utils::BitsU64;
    use crate::utils::dma_read_buffer::{Buffer, BufferWriter};

    #[test]
    fn test_signal_parser_parse_should_return_error_on_empty_data() {
        let parser = SignalParserImpl {};
        let data = [];

        let result = parser.parse(&data);

        assert_eq!(Err(Errors::InvalidDataSize), result);
    }

    #[test]
    fn test_signal_parser_parse_should_return_error_on_wrong_signal_code() {
        let all_signal_codes = ALL_SIGNALS.map(|s| s as u8);
        let parser = SignalParserImpl {};
        let datas = Vec::from([[1].to_vec(), [1, 2, 3, 4, 5].to_vec()]);

        for code in 0_u8..u8::MAX {
            if !all_signal_codes.contains(&code) {
                let data = [code];
                let result = parser.parse(&data);

                assert_eq!(Err(Errors::InstructionNotRecognized(code)), result);
            }
        }
    }

    #[test]
    fn test_signal_parser_parse_get_timestamp() {
        let parser = SignalParserImpl {};
        let data = [Signals::GetTimeStamp as u8];

        let result = parser.parse(&data);

        assert_eq!(Ok(SignalData::GetTimeStamp), result);
    }

    #[test]
    fn test_signal_parser_parse_other_signals_should_return_error_on_not_enough_data() {
        let parser = SignalParserImpl {};
        let signal_codes = [Signals::MonitoringStateChanged, Signals::StateFixTry,
            Signals::ControlStateChanged, Signals::RelayStateChanged];
        for signal_code in signal_codes {
            let data = [signal_code as u8, 1, 2, 3, 4];
            let result = parser.parse(&data);
            assert_eq!(Err(Errors::InvalidDataSize), result);
        }
    }

    #[test]
    fn test_signal_parser_parse_extended_relay_signals_should_parse_relay_signal_data() {
        for _ in 0..1 {
            let mut rng = rand::thread_rng();
            let relay_data = RelaySignalDataExt::new(
                RelativeSeconds::new(rng.gen_range(1..u32::MAX)),
                rng.gen_range(0..15_u8), rng.gen_range(0..2) == 1, rng.gen_range(0..2) == 1);

            let relay_data = RelaySignalDataExt::new(
                RelativeSeconds::new(1), 12, false, true);

            static mut BUFFER: [u8; 16] = [0; 16];
            let data = unsafe {
                &mut BUFFER
            }       ;
            let mut buffer = unsafe { Buffer::new(&mut BUFFER) };
            let _ = buffer.add_u8(0);
            let _ = relay_data.serialize(&mut buffer);

            let parser = SignalParserImpl {};
            let signal_codes = [Signals::RelayStateChanged];
            for signal_code in signal_codes {
                data[0] = signal_code as u8;
                let data = &data[0..buffer.bytes().len()];
                let result = parser.parse(data);

                let expected = match signal_code {
                    Signals::RelayStateChanged => SignalData::RelayStateChanged(relay_data),
                    _ => panic!("unexpected signal code")
                };
                assert_eq!(Ok(expected), result);
            }
        }
    }

    #[test]
    fn test_signal_parser_parse_other_signals_should_parse_relay_signal_data() {
        for _ in 0..5 {
            let mut rng = rand::thread_rng();
            let relay_data = RelaySignalData::new(
                RelativeSeconds::new(rng.gen_range(1..u32::MAX)),
                rng.gen_range(0..15_u8), rng.gen_range(0..2) == 1);

            static mut BUFFER: [u8; 16] = [0; 16];
            let data = unsafe {
                &mut BUFFER
            }       ;
            let mut buffer = unsafe { Buffer::new(&mut BUFFER) };
            let _ = buffer.add_u8(0);
            let _ = relay_data.serialize(&mut buffer);

            let parser = SignalParserImpl {};
            let signal_codes = [Signals::MonitoringStateChanged, Signals::StateFixTry,
                Signals::ControlStateChanged];
            for signal_code in signal_codes {
                data[0] = signal_code as u8;
                let data = &data[0..buffer.bytes().len()];
                let result = parser.parse(data);

                let expected = match signal_code {
                    Signals::MonitoringStateChanged => SignalData::MonitoringStateChanged(relay_data),
                    Signals::StateFixTry => SignalData::StateFixTry(relay_data),
                    Signals::ControlStateChanged => SignalData::ControlStateChanged(relay_data),
                    _ => panic!("unexpected signal code")
                };
                assert_eq!(Ok(expected), result);
            }
        }
    }

    #[test]
    fn test_response_parser_parse_should_return_error_on_empty_data() {
        let operations = [Operation::Success, Operation::Response, Operation::Error,
            Operation::Read, Operation::Command, Operation::Set];
        let data = [];
        for operation in operations {
            let parser = ResponseParserImpl::new(operation);
            let result = parser.parse(&data, Version::V1);
            assert_eq!(Err(Errors::NotEnoughDataGot), result);
        }
    }

    #[test]
    fn test_response_parser_parse_should_proxy_parse_id_error() {
        let mut rng = rand::thread_rng();
        let operations = [Operation::Success, Operation::Response, Operation::Read,
            Operation::Command, Operation::Set];
        let data = [DataInstructionCodes::Id as u8, rng.gen_range(1..u8::MAX), rng.gen_range(1..u8::MAX),
            rng.gen_range(1..u8::MAX)];
        for operation in operations {
            let parser = ResponseParserImpl::new(operation);
            let result = parser.parse(&data, Version::V2);
            assert_eq!(Err(Errors::NotEnoughDataGot), result);
        }
    }

    #[test]
    fn test_response_parser_parse_should_proxy_parse_instruction_error() {
        let mut rng = rand::thread_rng();
        let operations = [Operation::Success, Operation::Response, Operation::Read,
            Operation::Command, Operation::Set];
        let all_instruction_codes = ALL_INSTRUCTIONS.map(|i| i as u8);
        for operation in operations {
            for instruction_code in 0..u8::MAX {
                if all_instruction_codes.contains(&instruction_code) {
                    continue;
                }
                let data = [instruction_code, rng.gen_range(1..u8::MAX), rng.gen_range(1..u8::MAX),
                    rng.gen_range(1..u8::MAX)];
                let parser = ResponseParserImpl::new(operation);
                let result = parser.parse(&data, Version::V1);
                assert_eq!(Err(Errors::InstructionNotRecognized(instruction_code)), result);
            }
        }
    }

    #[test]
    fn test_response_parser_parse_on_error_response_not_enough_data() {
        for version in [Version::V1, Version::V2] {
            let mut rng = rand::thread_rng();
            for error_code in ALL_ERROR_CODES {
                let data = [error_code.discriminant()];
                let parser = ResponseParserImpl::new(Operation::Error);
                let result = parser.parse(&data, version);
                assert_eq!(Err(Errors::SlaveError(error_code)), result);
            }
            for error_code in [21_u8, 23, 45, 112] {
                let data = [error_code];
                let parser = ResponseParserImpl::new(Operation::Error);
                let result = parser.parse(&data, version);
                assert_eq!(Err(Errors::SlaveError(ErrorCode::EUndefinedCode(error_code))), result);
            }
        }
    }

    #[test]
    fn test_response_parser_parse_on_error_response_v1() {
        let mut rng = rand::thread_rng();
        for instruction in ALL_INSTRUCTIONS {
            for error_code in ALL_ERROR_CODES {
                let data = [error_code.discriminant(), instruction as u8];
                let parser = ResponseParserImpl::new(Operation::Error);
                let result = parser.parse(&data, Version::V1);
                assert_eq!(Ok((ResponseData {
                    operation: Operation::Error,
                    instruction,
                    request_id: None,
                    error_code,
                }, &data[0..0])), result);
            }
        }
    }

    #[test]
    fn test_response_parser_parse_on_error_response_v2() {
        let mut rng = rand::thread_rng();
        for instruction in ALL_INSTRUCTIONS {
            for error_code in ALL_ERROR_CODES {
                let id = rng.gen_range(1..u32::MAX);
                let id_bytes = id.to_be_bytes();
                let data = [error_code.discriminant(), instruction as u8, id_bytes[0], id_bytes[1], id_bytes[2], id_bytes[3]];
                let parser = ResponseParserImpl::new(Operation::Error);
                let result = parser.parse(&data, Version::V2);
                assert_eq!(Ok((ResponseData {
                    operation: Operation::Error,
                    instruction,
                    request_id: Some(id),
                    error_code,
                }, &data[0..0])), result);
            }
        }
    }

    #[test]
    fn test_response_parser_parse_on_success_response_v1 () {
        let mut rng = rand::thread_rng();
        for operation in [Operation::Success, Operation::Response, Operation::Read, Operation::Command, Operation::Set] {
            for instruction in ALL_INSTRUCTIONS {
                let data = [instruction as u8, rng.gen_range(1..u8::MAX), rng.gen_range(1..u8::MAX),
                    rng.gen_range(1..u8::MAX)];
                let parser = ResponseParserImpl::new(operation);
                let result = parser.parse(&data, Version::V1);
                let data = if operation == Operation::Error { &data[2..] } else { &data[1..] };
                assert_eq!(Ok((ResponseData {
                    operation,
                    instruction,
                    request_id: None,
                    error_code: ErrorCode::OK,
                }, data)), result);
            }
        }
    }

    #[test]
    fn test_response_parser_parse_on_success_response_v2 () {
        let mut rng = rand::thread_rng();
        for operation in [Operation::Success, Operation::Response, Operation::Read, Operation::Command, Operation::Set] {
            for instruction in ALL_INSTRUCTIONS {
                let id = rng.gen_range(1..u32::MAX);
                let id_bytes = id.to_be_bytes();
                let data = [instruction as u8, id_bytes[0], id_bytes[1], id_bytes[2], id_bytes[3]];
                let parser = ResponseParserImpl::new(operation);
                let result = parser.parse(&data, Version::V2);
                let data = &data[0..0];
                assert_eq!(Ok((ResponseData {
                    operation,
                    instruction,
                    request_id: Some(id),
                    error_code: ErrorCode::OK,
                }, data)), result);
            }
        }
    }

    #[test]
    fn test_payload_parser_parse_should_return_error_on_not_enough_data() {
        let mut rng = rand::thread_rng();
        let parser = PayloadParserImpl::new();
        let data = [rng.gen_range(1..u8::MAX)];

        let result = parser.parse(&data);

        assert!(result.is_err());
        assert_eq!(Errors::NotEnoughDataGot, result.unwrap_err());
    }

    #[test]
    fn test_payload_parser_parse_should_return_error_on_not_none_second_code() {
        for operation_code in ALL_POSSIBLE_OPERATION_CODES {
            let mut rng = rand::thread_rng();
            let parser = PayloadParserImpl::new();
            let data = [operation_code as u8, rng.gen_range(1..u8::MAX)];

            let result = parser.parse(&data);

            assert!(result.is_err());
            assert_eq!(Errors::CommandDataCorrupted, result.unwrap_err());
        }
    }

    #[test]
    fn test_payload_parser_parse_responses() {
        let operation_codes = [OperationCodes::Success, OperationCodes::Response, OperationCodes::Error,
            OperationCodes::SuccessV2, OperationCodes::ResponseV2, OperationCodes::ErrorV2];
        let operations = [Operation::Set, Operation::Read, Operation::Error, Operation::Set, Operation::Read, Operation::Error];
        for idx in 0..operation_codes.len() {
            let mut rng = rand::thread_rng();
            let parser = PayloadParserImpl::new();
            let data = [OperationCodes::None as u8, operation_codes[idx] as u8, rng.gen_range(1..u8::MAX)];

            let result = parser.parse(&data);
            let data = &data[2..];

            assert_eq!(
                Ok((PayloadParserResult::ResponsePayload(ResponseParserImpl::new(operations[idx])), data)),
                result
            );
        }
    }

    #[test]
    fn test_payload_parser_parse_signal() {
        let mut rng = rand::thread_rng();
        let parser = PayloadParserImpl::new();
        let data = [OperationCodes::None as u8, OperationCodes::Signal as u8, rng.gen_range(1..u8::MAX)];

        let result = parser.parse(&data);
        let data = &data[2..];

        assert_eq!(
            Ok((PayloadParserResult::SignalPayload(SignalParserImpl()), data)),
            result
        );
    }

    #[test]
    fn test_payload_parser_parse_should_return_error_on_unforseen_operation() {
        let operation_codes = [OperationCodes::None, OperationCodes::Read, OperationCodes::Set, OperationCodes::Command];
        for idx in 0..operation_codes.len() {
            let mut rng = rand::thread_rng();
            let parser = PayloadParserImpl::new();
            let data = [OperationCodes::None as u8, operation_codes[idx] as u8, rng.gen_range(1..u8::MAX)];

            let result = parser.parse(&data);

            assert_eq!( Err(Errors::OperationNotRecognized(operation_codes[idx] as u8)), result );
        }
    }

    static mut BUFFER_ARR: [u8; 256] = [0_u8; 256];

    #[test]
    fn test_response_body_parser_parse_on_relay_settings() {
        let mut rng = thread_rng();
        let mut data_object = RelaysSettings::new();
        let count = rng.gen_range(0..MAX_RELAYS_COUNT);
        for _ in 0..count {
            data_object.add(rng.gen_range(0..u8::MAX), rng.gen_range(0..u8::MAX), rng.gen_range(0..u8::MAX));
        }

        let mut buffer = unsafe { Buffer::new(&mut BUFFER_ARR ) };

        data_object.serialize(&mut buffer);

        clear_static_buffer_index();
        let parser = ResponseBodyParserImpl::create().unwrap();

        let result = parser.parse(DataInstructionCodes::Settings, buffer.bytes());

        assert_eq!(Ok(DataInstructions::Settings(Conversation::Data(data_object))), result);
    }

    #[test]
    fn test_response_body_parser_parse_on_state() {
        let mut rng = thread_rng();
        let data_object = State::create(rng.gen_range(0..15), rng.next_u64()).unwrap();
        let mut buffer = unsafe { Buffer::new(&mut BUFFER_ARR) };
        data_object.serialize(&mut buffer);
        clear_static_buffer_index();
        let parser = ResponseBodyParserImpl::create().unwrap();

        let result = parser.parse(DataInstructionCodes::State, buffer.bytes());

        assert_eq!(Ok(DataInstructions::State(Conversation::Data(data_object))), result);
    }

    #[test]
    fn test_response_body_parser_parse_on_id() {
        let mut rng = thread_rng();
        let data_object: u32 = rng.next_u32();
        let mut buffer = unsafe { Buffer::new(&mut BUFFER_ARR) };
        data_object.serialize(&mut buffer);
        clear_static_buffer_index();
        let parser = ResponseBodyParserImpl::create().unwrap();

        let result = parser.parse(DataInstructionCodes::Id, buffer.bytes());

        assert_eq!(Ok(DataInstructions::Id(Conversation::Data(data_object))), result);
    }

    #[test]
    fn test_response_body_parser_parse_on_interrupt_pin() {
        let mut rng = thread_rng();
        let data_object: u8 = rng.gen_range(0..u8::MAX);

        let mut buffer = unsafe { Buffer::new(&mut BUFFER_ARR ) };

        data_object.serialize(&mut buffer);

        clear_static_buffer_index();
        let parser = ResponseBodyParserImpl::create().unwrap();

        let result = parser.parse(DataInstructionCodes::InterruptPin, buffer.bytes());

        assert_eq!(Ok(DataInstructions::InterruptPin(Conversation::Data(data_object))), result);
    }

    #[test]
    fn test_response_body_parser_parse_on_remote_timestamp() {
        let mut rng = thread_rng();
        let data_object = RelativeSeconds::new(rng.gen_range(0..u32::MAX));

        let mut buffer = unsafe { Buffer::new(&mut BUFFER_ARR ) };

        data_object.serialize(&mut buffer);

        clear_static_buffer_index();
        let parser = ResponseBodyParserImpl::create().unwrap();

        let result = parser.parse(DataInstructionCodes::RemoteTimestamp, buffer.bytes());

        assert_eq!(Ok(DataInstructions::RemoteTimestamp(Conversation::Data(data_object))), result);
    }

    #[test]
    fn test_response_body_parser_parse_on_state_fix_settings() {
        let mut rng = thread_rng();
        let data_object = StateFixSettings::new(
            rng.gen_range(0..u16::MAX),
            rng.gen_range(0..u8::MAX),
            rng.gen_range(0..u8::MAX),
            rng.gen_range(0..u16::MAX),
        );

        let mut buffer = unsafe { Buffer::new(&mut BUFFER_ARR ) };

        data_object.serialize(&mut buffer);

        clear_static_buffer_index();
        let parser = ResponseBodyParserImpl::create().unwrap();

        let result = parser.parse(DataInstructionCodes::StateFixSettings, buffer.bytes());

        assert_eq!(Ok(DataInstructions::StateFixSettings(Conversation::Data(data_object))), result);
    }

    #[test]
    fn test_response_body_parser_parse_on_relay_state() {
        let mut rng = thread_rng();
        let data_object = RelayState::create(
            rng.gen_range(0..15),
            rng.gen_range(0..2) > 0,
            rng.gen_range(0..2) > 0,
        ).unwrap();

        let mut buffer = unsafe { Buffer::new(&mut BUFFER_ARR ) };

        data_object.serialize(&mut buffer);

        clear_static_buffer_index();
        let parser = ResponseBodyParserImpl::create().unwrap();

        let result = parser.parse(DataInstructionCodes::RelayState, buffer.bytes());

        assert_eq!(Ok(DataInstructions::RelayState(Conversation::Data(data_object))), result);
    }

    #[test]
    fn test_response_body_parser_parse_on_version() {
        let mut rng = thread_rng();
        let data_object = rng.gen_range(0..u8::MAX);

        let mut buffer = unsafe { Buffer::new(&mut BUFFER_ARR ) };

        data_object.serialize(&mut buffer);

        clear_static_buffer_index();
        let parser = ResponseBodyParserImpl::create().unwrap();

        let result = parser.parse(DataInstructionCodes::Version, buffer.bytes());

        assert_eq!(Ok(DataInstructions::Version(Conversation::Data(data_object))), result);
    }

    #[test]
    fn test_response_body_parser_parse_on_current_time() {
        let mut rng = thread_rng();
        let data_object = RelativeSeconds::new(rng.next_u32());

        let mut buffer = unsafe { Buffer::new(&mut BUFFER_ARR ) };

        data_object.serialize(&mut buffer);

        clear_static_buffer_index();
        let parser = ResponseBodyParserImpl::create().unwrap();

        let result = parser.parse(DataInstructionCodes::CurrentTime, buffer.bytes());

        assert_eq!(Ok(DataInstructions::CurrentTime(Conversation::Data(data_object))), result);
    }

    #[test]
    fn test_response_body_parser_parse_on_contact_wait_data() {
        let mut rng = thread_rng();
        let mut data_object = ContactsWaitData::new();
        let count = rng.gen_range(0..15);
        for _ in 0..count {
            data_object.add(rng.next_u32()).unwrap();
        }

        let mut buffer = unsafe { Buffer::new(&mut BUFFER_ARR ) };

        data_object.serialize(&mut buffer);

        clear_static_buffer_index();
        let parser = ResponseBodyParserImpl::create().unwrap();

        let result = parser.parse(DataInstructionCodes::ContactWaitData, buffer.bytes());

        assert_eq!(Ok(DataInstructions::ContactWaitData(Conversation::Data(data_object))), result);
    }

    #[test]
    fn test_response_body_parser_parse_on_fix_data() {
        let mut rng = thread_rng();
        let mut data_object = FixDataContainer::new();
        data_object.add_fix_data(rng.gen_range(0..u8::MAX), rng.next_u32());
        data_object.add_fix_data(rng.gen_range(0..u8::MAX), rng.next_u32());
        data_object.add_fix_data(rng.gen_range(0..u8::MAX), rng.next_u32());
        data_object.add_fix_data(rng.gen_range(0..u8::MAX), rng.next_u32());
        data_object.add_fix_data(rng.gen_range(0..u8::MAX), rng.next_u32());

        let mut buffer = unsafe { Buffer::new(&mut BUFFER_ARR ) };

        data_object.serialize(&mut buffer);

        clear_static_buffer_index();
        let parser = ResponseBodyParserImpl::create().unwrap();

        let result = parser.parse(DataInstructionCodes::FixData, buffer.bytes());

        assert_eq!(Ok(DataInstructions::FixData(Conversation::Data(data_object))), result);
    }

    #[test]
    fn test_response_body_parser_parse_on_switch_data() {
        let mut rng = thread_rng();
        let mut data_object = StateSwitchDatas::new();
        data_object.add_switch_data(rng.gen_range(0..u8::MAX), rng.next_u32());
        data_object.add_switch_data(rng.gen_range(0..u8::MAX), rng.next_u32());
        data_object.add_switch_data(rng.gen_range(0..u8::MAX), rng.next_u32());
        data_object.add_switch_data(rng.gen_range(0..u8::MAX), rng.next_u32());

        let mut buffer = unsafe { Buffer::new(&mut BUFFER_ARR ) };

        data_object.serialize(&mut buffer);

        clear_static_buffer_index();
        let parser = ResponseBodyParserImpl::create().unwrap();

        let result = parser.parse(DataInstructionCodes::SwitchData, buffer.bytes());

        assert_eq!(Ok(DataInstructions::SwitchData(Conversation::Data(data_object))), result);
    }

    #[test]
    fn test_response_body_parser_parse_on_cycle_statistics() {
        let mut rng = thread_rng();
        let mut data_object = CyclesStatistics::new(rng.gen_range(0..u16::MAX), rng.gen_range(0..u16::MAX),
                                                    rng.gen_range(0..u16::MAX), rng.next_u64());

        let mut buffer = unsafe { Buffer::new(&mut BUFFER_ARR) };

        data_object.serialize(&mut buffer);

        clear_static_buffer_index();
        let parser = ResponseBodyParserImpl::create().unwrap();

        let result = parser.parse(DataInstructionCodes::CyclesStatistics, buffer.bytes());

        assert_eq!(Ok(DataInstructions::CyclesStatistics(Conversation::Data(data_object))), result);
    }

    #[test]
    fn test_response_body_parser_parse_on_switch_counting_settings() {
        let mut rng = thread_rng();
        let mut data_object = SwitchCountingSettings::new(rng.gen_range(0..u16::MAX), rng.gen_range(0..u8::MAX),);

        let mut buffer = unsafe { Buffer::new(&mut BUFFER_ARR ) };

        data_object.serialize(&mut buffer);

        clear_static_buffer_index();
        let parser = ResponseBodyParserImpl::create().unwrap();

        let result = parser.parse(DataInstructionCodes::SwitchCountingSettings, buffer.bytes());

        assert_eq!(Ok(DataInstructions::SwitchCountingSettings(Conversation::Data(data_object))), result);
    }

    #[test]
    fn test_response_body_parser_parse_on_relay_disabled_temp() {
        let mut rng = thread_rng();
        let mut data_object = RelaySingleState::new(rng.gen_range(0..u8::MAX),
                                                    rng.gen_range(0..1) > 0);

        let mut buffer = unsafe { Buffer::new(&mut BUFFER_ARR ) };

        data_object.serialize(&mut buffer);

        clear_static_buffer_index();
        let parser = ResponseBodyParserImpl::create().unwrap();

        let result = parser.parse(DataInstructionCodes::RelayDisabledTemp, buffer.bytes());

        assert_eq!(Ok(DataInstructions::RelayDisabledTemp(Conversation::Data(data_object))), result);
    }

    #[test]
    fn test_response_body_parser_parse_on_relay_switched_on() {
        let mut rng = thread_rng();
        let mut data_object = RelaySingleState::new(rng.gen_range(0..u8::MAX),
                                                    rng.gen_range(0..1) > 0);

        let mut buffer = unsafe { Buffer::new(&mut BUFFER_ARR ) };

        data_object.serialize(&mut buffer);

        clear_static_buffer_index();
        let parser = ResponseBodyParserImpl::create().unwrap();

        let result = parser.parse(DataInstructionCodes::RelaySwitchedOn, buffer.bytes());

        assert_eq!(Ok(DataInstructions::RelaySwitchedOn(Conversation::Data(data_object))), result);
    }

    #[test]
    fn test_response_body_parser_parse_on_relay_monitor_on() {
        let mut rng = thread_rng();
        let mut data_object = RelaySingleState::new(rng.gen_range(0..u8::MAX),
                                                    rng.gen_range(0..1) > 0);

        let mut buffer = unsafe { Buffer::new(&mut BUFFER_ARR ) };

        data_object.serialize(&mut buffer);

        clear_static_buffer_index();
        let parser = ResponseBodyParserImpl::create().unwrap();

        let result = parser.parse(DataInstructionCodes::RelayMonitorOn, buffer.bytes());

        assert_eq!(Ok(DataInstructions::RelayMonitorOn(Conversation::Data(data_object))), result);
    }

    #[test]
    fn test_response_body_parser_parse_on_relay_ctrl_on() {
        let mut rng = thread_rng();
        let mut data_object = RelaySingleState::new(rng.gen_range(0..u8::MAX),
                                                    rng.gen_range(0..1) > 0);

        let mut buffer = unsafe { Buffer::new(&mut BUFFER_ARR ) };

        data_object.serialize(&mut buffer);

        clear_static_buffer_index();
        let parser = ResponseBodyParserImpl::create().unwrap();

        let result = parser.parse(DataInstructionCodes::RelayControlOn, buffer.bytes());

        assert_eq!(Ok(DataInstructions::RelayControlOn(Conversation::Data(data_object))), result);
    }




    #[test]
    fn test_response_body_parser_parse_on_all_data() {
        let mut rng = rand::thread_rng();
        for count in 0 .. MAX_RELAYS_COUNT {
            if count == 0 {
                continue;
            }
            let mut data_object = AllData::new(rng.next_u32(), rng.gen_range(0..count));
            for _ in 0..count {
                data_object.add(rng.gen_range(0..u8::MAX), rng.gen_range(0..u8::MAX), rng.gen_range(0..u8::MAX), rng.gen_range(0..16));
            }
            let mut buffer = unsafe { Buffer::new(&mut BUFFER_ARR) };
            AllData::serialize(&data_object, &mut buffer);

            clear_static_buffer_index();
            let parser = ResponseBodyParserImpl::create().unwrap();
            let result = parser.parse(DataInstructionCodes::All, buffer.bytes());

            assert_eq!(Ok(DataInstructions::All(Conversation::Data(data_object))), result);
        }
    }



    fn clear_static_buffer_index() {
        unsafe {
            INSTANCES_COUNT = 0;
        }
    }

    const ALL_INSTRUCTIONS: [DataInstructionCodes; 19] = [
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
    ];

    const ALL_ERROR_CODES: [ErrorCode; 15] = [
        ErrorCode::OK,
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
        ErrorCode::EInternalError,
        ErrorCode::ERelayNotAllowedPinUsed,
    ];

    const ALL_OPERATION_CODES: [OperationCodes; 12] = [
        OperationCodes::None,
        OperationCodes::Set,
        OperationCodes::Read,
        OperationCodes::Command,
        OperationCodes::Response,
        OperationCodes::Error,
        OperationCodes::Success,
        OperationCodes::Signal,
        OperationCodes::SuccessV2,
        OperationCodes::ErrorV2,
        OperationCodes::ResponseV2,
        OperationCodes::Unknown,
    ];

    const ALL_POSSIBLE_OPERATION_CODES: [OperationCodes; 10] = [
        OperationCodes::Set,
        OperationCodes::Read,
        OperationCodes::Command,
        OperationCodes::Response,
        OperationCodes::Error,
        OperationCodes::Success,
        OperationCodes::Signal,
        OperationCodes::SuccessV2,
        OperationCodes::ErrorV2,
        OperationCodes::ResponseV2,
    ];


    const ALL_SIGNALS: [Signals; 5] = [Signals::GetTimeStamp, Signals::MonitoringStateChanged,
        Signals::StateFixTry, Signals::ControlStateChanged, Signals::RelayStateChanged];

    struct MockResponseBodyParser {
        parse_id_result: Result<Option<u32>, Errors>,
        parse_id_params: RefCell<Option<Vec<u8>>>,
        parse_result: Result<DataInstructions, Errors>,
        parse_params: RefCell<Option<Vec<u8>>>,
        request_needs_cache_result: bool,
        request_needs_cache_params: RefCell<Option<DataInstructionCodes>>,
        slave_controller_version_called: RefCell<bool>,
    }

    impl MockResponseBodyParser {
        fn new_for_body_parse(result: Result<DataInstructions, Errors>) -> Self {
            Self {
                parse_id_result: Err(Errors::DataCorrupted),
                parse_id_params: RefCell::new(None),
                parse_result: result,
                parse_params: RefCell::new(None),
                request_needs_cache_result: false,
                request_needs_cache_params: RefCell::new(None),
                slave_controller_version_called: RefCell::new(false),
            }
        }

        fn new_for_id_parse(parse_id_result: Result<Option<u32>, Errors>) -> Self {
            Self {
                parse_id_result,
                parse_id_params: RefCell::new(None),
                parse_result: Err(Errors::DataCorrupted),
                parse_params: RefCell::new(None),
                request_needs_cache_result: false,
                request_needs_cache_params: RefCell::new(None),
                slave_controller_version_called: RefCell::new(false),
            }
        }
    }

    impl ResponseBodyParser for MockResponseBodyParser {
        fn request_needs_cache(&self, instruction: DataInstructionCodes) -> bool {
            *self.request_needs_cache_params.borrow_mut() = Some(instruction);
            self.request_needs_cache_result
        }

        fn parse(&self, instruction: DataInstructionCodes, data: &[u8]) -> Result<DataInstructions, Errors> {
            *self.parse_params.borrow_mut() = Some(data.to_vec());
            Err(Errors::DataCorrupted)
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


}

