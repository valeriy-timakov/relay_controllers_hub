use alloc::boxed::Box;
use core::mem::size_of;
use crate::errors::Errors;
use crate::hal_ext::rtc_wrapper::RelativeSeconds;
use crate::services::slave_controller_link::domain::{AllData, ContactsWaitData, Conversation, Data, DataInstructionCodes, DataInstructions, ErrorCode, FixDataContainer, RelaysSettings, Request, Signals, StateSwitchDatas};
use crate::services::slave_controller_link::{RelaySignalData, SignalData};

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

pub trait ResponsesParser {
    fn parse_response(&self, instruction_code: u8, data: &[u8]) -> Result<DataInstructions, Errors>;
    fn request_needs_cache(&self, instruction: DataInstructionCodes) -> bool;
    fn parse_u32(&self, data: &[u8]) -> Result<u32, Errors> {
        u32::parse(data)
    }
}

pub struct RequestsParserImpl {
    static_buffers_idx: usize,
}

impl RequestsParserImpl {
    pub fn create() -> Result<Self, Errors> {
        Ok(Self {
            static_buffers_idx: get_next_static_buffer_index()?,
        })
    }
}

impl ResponsesParser for RequestsParserImpl {
    
    fn parse_response(&self, instruction_code: u8, data: &[u8]) -> Result<DataInstructions, Errors> {
        let instruction = DataInstructionCodes::get(instruction_code)?;
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

pub trait SignalsParser {
    fn parse(&self, instruction: Signals, data: &[u8]) -> Result<SignalData, Errors>;
    fn parse_instruction(&self, instruction_code: u8) -> Option<Signals>;
}

pub struct SignalsParserImpl;

impl SignalsParserImpl {
    pub fn new() -> Self {
        Self {}
    }
}

impl SignalsParser for SignalsParserImpl {

    fn parse_instruction(&self, instruction_code: u8) -> Option<Signals> {
        if instruction_code == Signals::MonitoringStateChanged as u8 {
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
        }
    }

    fn parse(&self, instruction: Signals, data: &[u8]) -> Result<SignalData, Errors> {

        let relay_signal_data =
            if instruction == Signals::GetTimeStamp {
                None
            } else {
                if data.len() < 5 {
                    return Err(Errors::InvalidDataSize);
                }
                let relay_idx = data[0] & 0x0f_u8;
                let is_on = data[0] & 0x10 > 0;
                let mut relative_seconds = 0_u32;
                for i in 0..4 {
                    relative_seconds |= (data[1 + i] as u32) << (8 * (3 - i));
                }
                let is_called_internally = if instruction == Signals::RelayStateChanged {
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

        Ok(SignalData {
            instruction,
            relay_signal_data,
        })
    }
}


/*


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
 */

