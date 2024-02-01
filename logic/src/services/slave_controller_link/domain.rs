#![allow(unsafe_code)]

use crate::errors::Errors;
use crate::hal_ext::rtc_wrapper::{RelativeSeconds};
use crate::utils::{BitsU64, BitsU8};
use crate::utils::dma_read_buffer::{BufferWriter};


pub const MAX_RELAYS_COUNT: u8 = 16;
pub const SWITCHES_DATA_BUFFER_SIZE: u8 = 50;


#[repr(u8)]
#[derive(Copy, Clone, PartialEq, Debug)]
pub enum OperationCodes {
    None = 0x00,
    Read = 0x01,
    Set = 0x02,
    Success = 0x03,
    Error = 0x04,
    Signal = 0x05,
    Response = 0x06,
    Command = 0x07,
    SuccessV2 = 0x08,
    ErrorV2 = 0x09,
    ResponseV2 = 0x0a,
    Unknown = 0x0f
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum Operation {
    None,
    Read,
    Set,
    Success,
    Error,
    Signal,
    Response,
    Command,
}

impl Operation {
    pub fn is_response(self) -> bool {
        self == Operation::Set || self == Operation::Read || self == Operation::Error
    }
    pub fn is_signal(self) -> bool {
        self == Operation::Signal 
    }
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum Version {
    V1,
    V2,
}

pub enum Commands {
    ClearSwitchCount = 0x08,
}

#[repr(u8)]
#[derive(Copy, Clone, PartialEq, Debug)]
pub enum DataInstructionCodes {
    None = 0x00,
    Settings = 0x01,
    State = 0x02,
    Id = 0x03,
    InterruptPin = 0x04,
    RemoteTimestamp = 0x05,
    StateFixSettings = 0x06,
    RelayState = 0x09,
    Version = 0x0f,
    CurrentTime = 0x10,
    ContactWaitData = 0x11,
    FixData = 0x12,
    SwitchData = 0x13,
    CyclesStatistics = 0x18,
    //v2 instructions
    SwitchCountingSettings = 0x07,
    RelayDisabledTemp = 0x0a,
    RelaySwitchedOn = 0x0b,
    RelayMonitorOn = 0x0c,
    RelayControlOn = 0x0d,
    All = 0x0e,
    Last = 0x19,
    Unknown = 0xff,
}

impl DataInstructionCodes {
    pub fn get(code_value: u8) -> Result<Self, Errors> {
        if code_value == Self::RemoteTimestamp as u8 {
            Ok(Self::RemoteTimestamp)
        } else if code_value == Self::CurrentTime as u8 {
            Ok(Self::CurrentTime)
        } else if code_value == Self::Id as u8 {
            Ok(Self::Id)
        } else if code_value == Self::Version as u8 {
            Ok(Self::Version)
        } else if code_value == Self::StateFixSettings as u8 {
            Ok(Self::StateFixSettings)
        } else if code_value == Self::RelayState as u8 {
            Ok(Self::RelayState)
        } else if code_value == Self::State as u8 {
            Ok(Self::State)
        } else if code_value == Self::CyclesStatistics as u8 {
            Ok(Self::CyclesStatistics)
        } else if code_value == Self::FixData as u8 {
            Ok(Self::FixData)
        } else if code_value == Self::Settings as u8 {
            Ok(Self::Settings)
            //v2 instructions
        } else if code_value == Self::ContactWaitData as u8 {
            Ok(Self::ContactWaitData)
        } else if code_value == Self::SwitchData as u8 {
            Ok(Self::SwitchData)
        } else if code_value == Self::InterruptPin as u8 {
            Ok(Self::InterruptPin)
        } else if code_value == Self::SwitchCountingSettings as u8 {
            Ok(Self::SwitchCountingSettings)
        } else if code_value == Self::RelayDisabledTemp as u8 {
            Ok(Self::RelayDisabledTemp)
        } else if code_value == Self::RelaySwitchedOn as u8 {
            Ok(Self::RelaySwitchedOn)
        } else if code_value == Self::RelayMonitorOn as u8 {
            Ok(Self::RelayMonitorOn)
        } else if code_value == Self::RelayControlOn as u8 {
            Ok(Self::RelayControlOn)
        } else if code_value == Self::All as u8 {
            Ok(Self::All)
        } else {
            Err(Errors::InstructionNotRecognized(code_value))
        }
    }
}

pub trait DataInstruction {
    fn code(&self) -> DataInstructionCodes;
    fn serialize<B: BufferWriter>(&self, buffer: &mut B) -> Result<(), Errors>;
}

#[repr(u8)]
#[derive(PartialEq, Debug)]
pub enum DataInstructions {
    Settings(Conversation<EmptyRequest, RelaysSettings>) = DataInstructionCodes::Settings as u8,
    State(Conversation<EmptyRequest, State>) = DataInstructionCodes::State as u8,
    Id(Conversation<EmptyRequest, u32>) = DataInstructionCodes::Id as u8,
    InterruptPin(Conversation<EmptyRequest, u8>) = DataInstructionCodes::InterruptPin as u8,
    RemoteTimestamp(Conversation<EmptyRequest, RelativeSeconds>) = DataInstructionCodes::RemoteTimestamp as u8,
    StateFixSettings(Conversation<EmptyRequest, StateFixSettings>) = DataInstructionCodes::StateFixSettings as u8,
    RelayState(Conversation<RelayIndexRequest, RelayState>) = DataInstructionCodes::RelayState as u8,
    Version(Conversation<EmptyRequest, u8>) = DataInstructionCodes::Version as u8,
    CurrentTime(Conversation<EmptyRequest, RelativeSeconds>) = DataInstructionCodes::CurrentTime as u8,
    ContactWaitData(Conversation<EmptyRequest, ContactsWaitData>) = DataInstructionCodes::ContactWaitData as u8,
    FixData(Conversation<EmptyRequest, FixDataContainer>) = DataInstructionCodes::FixData as u8,
    SwitchData(Conversation<EmptyRequest, StateSwitchDatas>) = DataInstructionCodes::SwitchData as u8,
    CyclesStatistics(Conversation<EmptyRequest, CyclesStatistics>) = DataInstructionCodes::CyclesStatistics as u8,
    //v2 instructions
    SwitchCountingSettings(Conversation<EmptyRequest, SwitchCountingSettings>) = DataInstructionCodes::SwitchCountingSettings as u8,
    RelayDisabledTemp(Conversation<EmptyRequest, RelaySingleState>) = DataInstructionCodes::RelayDisabledTemp as u8,
    RelaySwitchedOn(Conversation<EmptyRequest, RelaySingleState>) = DataInstructionCodes::RelaySwitchedOn as u8,
    RelayMonitorOn(Conversation<EmptyRequest, RelaySingleState>) = DataInstructionCodes::RelayMonitorOn as u8,
    RelayControlOn(Conversation<EmptyRequest, RelaySingleState>) = DataInstructionCodes::RelayControlOn as u8,
    All(Conversation<EmptyRequest, AllData>) = DataInstructionCodes::All as u8,
}

impl DataInstructions {

    pub fn code(&self) -> DataInstructionCodes {
        match self {
            DataInstructions::RemoteTimestamp(_) => {
                DataInstructionCodes::RemoteTimestamp
            }
            DataInstructions::CurrentTime(_) => {
                DataInstructionCodes::CurrentTime
            }
            DataInstructions::Id(_) => {
                DataInstructionCodes::Id
            }
            DataInstructions::Version(_) => {
                DataInstructionCodes::Version
            }
            DataInstructions::StateFixSettings(_) => {
                DataInstructionCodes::StateFixSettings
            }
            DataInstructions::RelayState(_) => {
                DataInstructionCodes::RelayState
            }
            DataInstructions::State(_) => {
                DataInstructionCodes::State
            }
            DataInstructions::CyclesStatistics(_) => {
                DataInstructionCodes::CyclesStatistics
            }
            DataInstructions::FixData(_) => {
                DataInstructionCodes::FixData
            }
            DataInstructions::Settings(_) => {
                DataInstructionCodes::Settings
            }
            //v2 instructions
            DataInstructions::ContactWaitData(_) => {
                DataInstructionCodes::ContactWaitData
            }
            DataInstructions::SwitchData(_) => {
                DataInstructionCodes::SwitchData
            }
            DataInstructions::InterruptPin(_) => {
                DataInstructionCodes::InterruptPin
            }
            DataInstructions::SwitchCountingSettings(_) => {
                DataInstructionCodes::SwitchCountingSettings
            }
            DataInstructions::RelayDisabledTemp(_) => {
                DataInstructionCodes::RelayDisabledTemp
            }
            DataInstructions::RelaySwitchedOn(_) => {
                DataInstructionCodes::RelaySwitchedOn
            }
            DataInstructions::RelayMonitorOn(_) => {
                DataInstructionCodes::RelayMonitorOn
            }
            DataInstructions::RelayControlOn(_) => {
                DataInstructionCodes::RelayControlOn
            }
            DataInstructions::All(_) => {
                DataInstructionCodes::All
            }
        }
    }

    pub fn parse_from(&mut self, data: &[u8]) -> Result<(), Errors> {
        match self {
            DataInstructions::RemoteTimestamp(Conversation::Data(ref_data)) => {
                ref_data.parse_from(data)
            }
            DataInstructions::CurrentTime(Conversation::Data(ref_data)) => {
                ref_data.parse_from(data)
            }
            DataInstructions::Id(Conversation::Data(ref_data)) => {
                ref_data.parse_from(data)
            }
            DataInstructions::Version(Conversation::Data(ref_data)) => {
                ref_data.parse_from(data)
            }
            DataInstructions::StateFixSettings(Conversation::Data(ref_data)) => {
                ref_data.parse_from(data)
            }
            DataInstructions::RelayState(Conversation::Data(ref_data)) => {
                ref_data.parse_from(data)
            }
            DataInstructions::State(Conversation::Data(ref_data)) => {
                ref_data.parse_from(data)
            }
            DataInstructions::CyclesStatistics(Conversation::Data(ref_data)) => {
                ref_data.parse_from(data)
            }
            DataInstructions::FixData(Conversation::Data(ref_data)) => {
                ref_data.parse_from(data)
            }
            DataInstructions::Settings(Conversation::Data(ref_data)) => {
                ref_data.parse_from(data)
            }
            //v2 instructions
            DataInstructions::ContactWaitData(Conversation::Data(ref_data)) => {
                ref_data.parse_from(data)
            }
            DataInstructions::SwitchData(Conversation::Data(ref_data)) => {
                ref_data.parse_from(data)
            }
            DataInstructions::InterruptPin(Conversation::Data(ref_data)) => {
                ref_data.parse_from(data)
            }
            DataInstructions::SwitchCountingSettings(Conversation::Data(ref_data)) => {
                ref_data.parse_from(data)
            }
            DataInstructions::RelayDisabledTemp(Conversation::Data(ref_data)) => {
                ref_data.parse_from(data)
            }
            DataInstructions::RelaySwitchedOn(Conversation::Data(ref_data)) => {
                ref_data.parse_from(data)
            }
            DataInstructions::RelayMonitorOn(Conversation::Data(ref_data)) => {
                ref_data.parse_from(data)
            }
            DataInstructions::RelayControlOn(Conversation::Data(ref_data)) => {
                ref_data.parse_from(data)
            }
            DataInstructions::All(Conversation::Data(ref_data)) => {
                ref_data.parse_from(data)
            }
            _ => {
                Err(Errors::InstructionNotSerializable)
            }
        }
    }

    pub fn parse(instruction: DataInstructionCodes, data: &[u8]) -> Result<Self, Errors> {
        match instruction {
            DataInstructionCodes::Settings => {
                Ok(DataInstructions::Settings(Conversation::Data(RelaysSettings::parse(data)?)))
            }
            DataInstructionCodes::State => {
                Ok(DataInstructions::State(Conversation::Data(State::parse(data)?)))
            }
            DataInstructionCodes::Id => {
                Ok(DataInstructions::Id(Conversation::Data(u32::parse(data)?)))
            }
            DataInstructionCodes::InterruptPin => {
                Ok(DataInstructions::InterruptPin(Conversation::Data(u8::parse(data)?)))
            }
            DataInstructionCodes::RemoteTimestamp => {
                Ok(DataInstructions::RemoteTimestamp(Conversation::Data(RelativeSeconds::parse(data)?)))
            }
            DataInstructionCodes::StateFixSettings => {
                Ok(DataInstructions::StateFixSettings(Conversation::Data(StateFixSettings::parse(data)?)))
            }
            DataInstructionCodes::RelayState => {
                Ok(DataInstructions::RelayState(Conversation::Data(RelayState::parse(data)?)))
            }
            DataInstructionCodes::Version => {
                Ok(DataInstructions::Version(Conversation::Data(u8::parse(data)?)))
            }
            DataInstructionCodes::CurrentTime => {
                Ok(DataInstructions::CurrentTime(Conversation::Data(RelativeSeconds::parse(data)?)))
            }
            DataInstructionCodes::ContactWaitData => {
                Ok(DataInstructions::ContactWaitData(Conversation::Data(ContactsWaitData::parse(data)?)))
            }
            DataInstructionCodes::FixData => {
                Ok(DataInstructions::FixData(Conversation::Data(FixDataContainer::parse(data)?)))
            }
            DataInstructionCodes::SwitchData => {
                Ok(DataInstructions::SwitchData(Conversation::Data(StateSwitchDatas::parse(data)?)))
            }
            DataInstructionCodes::CyclesStatistics => {
                Ok(DataInstructions::CyclesStatistics(Conversation::Data(CyclesStatistics::parse(data)?)))
            }
            //v2 instructions
            DataInstructionCodes::SwitchCountingSettings => {
                Ok(DataInstructions::SwitchCountingSettings(Conversation::Data(SwitchCountingSettings::parse(data)?)))
            }
            DataInstructionCodes::RelayDisabledTemp => {
                Ok(DataInstructions::RelayDisabledTemp(Conversation::Data(RelaySingleState::parse(data)?)))
            }
            DataInstructionCodes::RelaySwitchedOn => {
                Ok(DataInstructions::RelaySwitchedOn(Conversation::Data(RelaySingleState::parse(data)?)))
            }
            DataInstructionCodes::RelayMonitorOn => {
                Ok(DataInstructions::RelayMonitorOn(Conversation::Data(RelaySingleState::parse(data)?)))
            }
            DataInstructionCodes::RelayControlOn => {
                Ok(DataInstructions::RelayControlOn(Conversation::Data(RelaySingleState::parse(data)?)))
            }
            DataInstructionCodes::All => {
                Ok(DataInstructions::All(Conversation::Data(AllData::parse(data)?)))
            }
            _ => { Err(Errors::InstructionNotRecognized(instruction as u8)) }
        }
    }

    pub fn serialize<B: BufferWriter>(&self, buffer: &mut B) -> Result<(), Errors> {
        match self {
            DataInstructions::RemoteTimestamp(Conversation::Data(value)) => {
                value.serialize(buffer)
            }
            DataInstructions::CurrentTime(Conversation::Data(value)) => {
                value.serialize(buffer)
            }
            DataInstructions::Id(Conversation::Data(value)) => {
                value.serialize(buffer)
            }
            DataInstructions::Version(Conversation::Data(value)) => {
                value.serialize(buffer)
            }
            DataInstructions::StateFixSettings(Conversation::Data(value)) => {
                value.serialize(buffer)
            }
            DataInstructions::RelayState(Conversation::Data(value)) => {
                value.serialize(buffer)
            }
            DataInstructions::State(Conversation::Data(value)) => {
                value.serialize(buffer)
            }
            DataInstructions::CyclesStatistics(Conversation::Data(value)) => {
                value.serialize(buffer)
            }
            DataInstructions::FixData(Conversation::Data(value)) => {
                value.serialize(buffer)
            }
            DataInstructions::Settings(Conversation::Data(value)) => {
                value.serialize(buffer)
            }
            //v2 instructions
            DataInstructions::ContactWaitData(Conversation::Data(value)) => {
                value.serialize(buffer)
            }
            DataInstructions::SwitchData(Conversation::Data(value)) => {
                value.serialize(buffer)
            }
            DataInstructions::InterruptPin(Conversation::Data(value)) => {
                value.serialize(buffer)
            }
            DataInstructions::SwitchCountingSettings(Conversation::Data(value)) => {
                value.serialize(buffer)
            }
            DataInstructions::RelayDisabledTemp(Conversation::Data(value)) => {
                value.serialize(buffer)
            }
            DataInstructions::RelaySwitchedOn(Conversation::Data(value)) => {
                value.serialize(buffer)
            }
            DataInstructions::RelayMonitorOn(Conversation::Data(value)) => {
                value.serialize(buffer)
            }
            DataInstructions::RelayControlOn(Conversation::Data(value)) => {
                value.serialize(buffer)
            }
            DataInstructions::All(Conversation::Data(value)) => {
                value.serialize(buffer)
            }
            _ => {
                Err(Errors::InstructionNotSerializable)
            }
        }
    }

}

impl DataInstruction for DataInstructions {

    #[inline(always)]
    fn code(&self) -> DataInstructionCodes {
        self.code()
    }

    #[inline(always)]
    fn serialize<B: BufferWriter>(&self, buffer: &mut B) -> Result<(), Errors> {
        self.serialize(buffer)
    }

}

#[repr(u8)]
#[derive(Copy, Clone, PartialEq, Debug)]
pub enum ErrorCode {
    OK = 0x00,
    ERequestDataNoValue = 0x01,
    EInstructionUnrecognized = 0x02,
    ECommandEmpty = 0x03,
    ECommandSizeOverflow = 0x04,
    EInstructionWrongStart = 0x05,
    EWriteMaxAttemptsExceeded = 0x06,
    EUndefinedOperation = 0x07,
    ERelayCountOverflow = 0x08,
    ERelayCountAndDataMismatch = 0x09,
    ERelayIndexOutOfRange = 0x0a,
    ESwitchCountMaxValueOverflow = 0x0b,
    EControlInterruptedPinNotAllowedValue = 0x0c,
    EInternalError = 0x0d,
    ERelayNotAllowedPinUsed = 0b00100000,
    EUndefinedCode(u8) = 128
}

impl ErrorCode {
    pub fn discriminant(&self) -> u8 {
        unsafe { *(self as *const Self as *const u8) }
    }
    pub fn for_code(code: u8) -> ErrorCode {
        if code == ErrorCode::OK.discriminant() {
            ErrorCode::OK
        } else if code == ErrorCode::ERequestDataNoValue.discriminant() {
            ErrorCode::ERequestDataNoValue
        } else if code == ErrorCode::EInstructionUnrecognized.discriminant() {
            ErrorCode::EInstructionUnrecognized
        } else if code == ErrorCode::ECommandEmpty.discriminant() {
            ErrorCode::ECommandEmpty
        } else if code == ErrorCode::ECommandSizeOverflow.discriminant() {
            ErrorCode::ECommandSizeOverflow
        } else if code == ErrorCode::EInstructionWrongStart.discriminant() {
            ErrorCode::EInstructionWrongStart
        } else if code == ErrorCode::EWriteMaxAttemptsExceeded.discriminant() {
            ErrorCode::EWriteMaxAttemptsExceeded
        } else if code == ErrorCode::EUndefinedOperation.discriminant() {
            ErrorCode::EUndefinedOperation
        } else if code == ErrorCode::ERelayCountOverflow.discriminant() {
            ErrorCode::ERelayCountOverflow
        } else if code == ErrorCode::ERelayCountAndDataMismatch.discriminant() {
            ErrorCode::ERelayCountAndDataMismatch
        } else if code == ErrorCode::ERelayIndexOutOfRange.discriminant() {
            ErrorCode::ERelayIndexOutOfRange
        } else if code == ErrorCode::ESwitchCountMaxValueOverflow.discriminant() {
            ErrorCode::ESwitchCountMaxValueOverflow
        } else if code == ErrorCode::EControlInterruptedPinNotAllowedValue.discriminant() {
            ErrorCode::EControlInterruptedPinNotAllowedValue
        } else if code == ErrorCode::ERelayNotAllowedPinUsed.discriminant() {
            ErrorCode::ERelayNotAllowedPinUsed
        } else if code == ErrorCode::EInternalError.discriminant() {
            ErrorCode::EInternalError
        } else {
            ErrorCode::EUndefinedCode(code)
        }
    }

    pub fn for_error(error: Errors) -> Self {
        match error {
            Errors::InstructionNotRecognized(code) => { ErrorCode::EUndefinedCode(code) }
            Errors::InvalidDataSize => { ErrorCode::ERequestDataNoValue }
            _ => { ErrorCode::EInternalError }
        }
    }
}

#[derive(PartialEq, Debug)]
pub enum Conversation<RQ: Request, D: Data + 'static> {
    Request(RQ),
    Data(D),
    DataCashed(&'static mut D),
    Response(Response),
}

pub trait Request {  }

pub trait AutoCreator {
    fn default() -> Self where Self: Sized;
}

pub trait Parser: AutoCreator {
    fn parse(data: &[u8]) -> Result<Self, Errors> where Self: Sized {
        let mut result = Self::default();
        result.parse_from(data)?;
        Ok(result)
    }

    fn parse_from(&mut self, data: &[u8]) -> Result<(), Errors>;
}

pub trait Serializable {
    fn serialize<B: BufferWriter>(&self, buffer: &mut B)->Result<(), Errors>;
}

pub trait Data : Parser + Serializable {}

#[derive(PartialEq, Debug)]
pub enum Response {
    Success,
    Error(ErrorCode),
}

#[derive(PartialEq, Debug)]
pub struct EmptyRequest;

impl EmptyRequest {
    pub fn new() -> Self {
        Self {}
    }
}

impl Request for EmptyRequest {}

#[derive(PartialEq, Debug)]
pub struct RelayIndexRequest {
    pub index: u8,
}

impl Request for RelayIndexRequest {}

impl Parser for RelativeSeconds {
    fn parse(data: &[u8]) -> Result<Self, Errors> where Self: Sized {
        Ok(RelativeSeconds::new(u32::parse(data)?))
    }

    fn parse_from(&mut self, data: &[u8]) -> Result<(), Errors> {
        *self = RelativeSeconds::new(u32::parse(data)?);
        Ok(())
    }
}

impl Serializable for RelativeSeconds {

    #[inline(always)]
    fn serialize<B: BufferWriter>(&self, buffer: &mut B) -> Result<(), Errors> {
        buffer.add_u32(self.value())
    }

}

impl AutoCreator for RelativeSeconds {
    fn default() -> Self where Self: Sized {
        RelativeSeconds::new(0)
    }
}

impl Data for RelativeSeconds {}

pub trait Extractor {
    fn extract(data: &[u8]) -> Self;
}

impl Extractor for u8 {
    fn extract(data: &[u8]) -> u8 {
        data[0]
    }
}

impl Extractor for u16 {
    #[inline(always)]
    fn extract(data: &[u8]) -> u16 {
        (data[0] as u16) << 8 | data[1] as u16
    }
}

impl Extractor for u32 {
    #[inline(always)]
    fn extract(data: &[u8]) -> u32 {
        (data[0] as u32) << 24 | (data[1] as u32) << 16 | (data[2] as u32) << 8 | data[3] as u32
    }
}

impl Extractor for u64 {
    #[inline(always)]
    fn extract(data: &[u8]) -> u64 {
        (data[0] as u64) << 56 | (data[1] as u64) << 48 | (data[2] as u64) << 40 | (data[3] as u64) << 32 |
            (data[4] as u64) << 24 | (data[5] as u64) << 16 | (data[6] as u64) << 8 | data[7] as u64
    }
}


#[derive(PartialEq, Debug)]
pub struct RelativeMillis16(u16);

#[derive(PartialEq, Debug)]
pub struct RelativeSeconds8(u8);

#[derive(PartialEq, Debug)]
pub struct RelativeSeconds16(u16);

impl Extractor for RelativeMillis16 {
    #[inline(always)]
    fn extract(data: &[u8]) -> RelativeMillis16 {
        RelativeMillis16(u16::extract(data))
    }
}

impl Extractor for RelativeSeconds8 {
    #[inline(always)]
    fn extract(data: &[u8]) -> RelativeSeconds8 {
        RelativeSeconds8(u8::extract(data))
    }
}

impl Extractor for RelativeSeconds16 {
    #[inline(always)]
    fn extract(data: &[u8]) -> RelativeSeconds16 {
        RelativeSeconds16(u16::extract(data))
    }
}

impl Extractor for BitsU64 {
    #[inline(always)]
    fn extract(data: &[u8]) -> Self {
        BitsU64::new(u64::extract(data))
    }
}

impl Parser for u8 {

    fn parse(data: &[u8]) -> Result<Self, Errors> where Self: Sized {
        if data.len() != 1 {
            return Err(Errors::InvalidDataSize);
        } else {
            Ok(data[0])
        }
    }

    fn parse_from(&mut self, data: &[u8]) -> Result<(), Errors> {
        *self = Self::parse(data)?;
        Ok(())
    }
}

impl Serializable for u8 {

    #[inline(always)]
    fn serialize<B: BufferWriter>(&self, buffer: &mut B) -> Result<(), Errors> {
        buffer.add_u8(*self)
    }

}

impl AutoCreator for u8 {
    fn default() -> Self where Self: Sized {
        0
    }
}

impl Data for u8 {}

impl Parser for u16 {

        fn parse(data: &[u8]) -> Result<Self, Errors> where Self: Sized {
            if data.len() == 2 {
                Ok(u16::extract(&data[0..2]))
            } else {
                Err(Errors::InvalidDataSize)
            }
        }

        fn parse_from(&mut self, data: &[u8]) -> Result<(), Errors> {
            *self = Self::parse(data)?;
            Ok(())
        }
}

impl Serializable for u16 {
        #[inline(always)]
        fn serialize<B: BufferWriter>(&self, buffer: &mut B) -> Result<(), Errors> {
            buffer.add_u16(*self)
        }
}

impl AutoCreator for u16 {
    fn default() -> Self where Self: Sized {
        0
    }
}

impl Data for u16 {}

impl Parser for u32 {

    fn parse(data: &[u8]) -> Result<Self, Errors> where Self: Sized {
        if data.len() == 4 {
            Ok(u32::extract(&data[0..4]))
        } else {
            Err(Errors::InvalidDataSize)
        }
    }

    fn parse_from(&mut self, data: &[u8]) -> Result<(), Errors> {
        *self = Self::parse(data)?;
        Ok(())
    }
}

impl Serializable for u32 {
    #[inline(always)]
    fn serialize<B: BufferWriter>(&self, buffer: &mut B) -> Result<(), Errors> {
        buffer.add_u32(*self)
    }

}

impl AutoCreator for u32 {
    fn default() -> Self where Self: Sized {
        0
    }
}

impl Data for u32 {}

#[derive(PartialEq, Debug)]
pub struct AllData {
    pub id: u32,
    pub interrupt_pin: u8,
    pub relays_count: u8,
    pub relays_settings: [RelaySettings; MAX_RELAYS_COUNT as usize],
    pub state_data: BitsU64,
}

impl AllData {
    pub fn new(id: u32, interrupt_pin: u8) -> Self {
        Self {
            id,
            interrupt_pin,
            relays_count : 0,
            relays_settings: [RelaySettings::new(); MAX_RELAYS_COUNT as usize],
            state_data: BitsU64::new(0),
        }
    }


    pub(crate) fn add(&mut self, set_pin: u8, monitor_pin: u8, control_pin: u8, state: u8) -> Result<(), Errors> {
        if self.relays_count < MAX_RELAYS_COUNT {
            self.relays_settings[self.relays_count as usize] = RelaySettings::create(set_pin, monitor_pin, control_pin);
            let from = self.relays_count * 4;
            self.state_data.set_byte(from, from + 3, state)?;
            self.relays_count += 1;
            Ok(())
        } else {
            Err(Errors::RelayCountOverflow)
        }
    }
}

impl Parser for AllData {

    fn parse_from(&mut self, data: &[u8]) -> Result<(), Errors> {

        if data.len() < 6 {
            return Err(Errors::NotEnoughDataGot);
        }
        self.id = u32::parse(&data[0..4])?;
        self.interrupt_pin = data[4];
        self.relays_count = data[5];
        let data = &data[6..];

        let data = RelaysSettings::parse_items(data, self.relays_count, &mut self.relays_settings)?;

        let pairs_count = (self.relays_count + 1) / 2;
        if data.len() < pairs_count as usize {
            return Err(Errors::NotEnoughDataGot);
        }
        self.state_data = State::parse_state_data_force(pairs_count, data)?;

        Ok(())
    }
}

impl Serializable for AllData {

    fn serialize<B: BufferWriter>(&self, buffer: &mut B) -> Result<(), Errors> {
        self.id.serialize(buffer)?;
        buffer.add_u8(self.interrupt_pin)?;
        buffer.add_u8(self.relays_count)?;
        for i in 0..self.relays_count as usize {
            let setting = &self.relays_settings[i];
            setting.serialize(buffer)?;
        }
        let pairs_count = (self.relays_count + 1) / 2;
        for i in 0..pairs_count {
            let from = i * 8;
            let state = self.state_data.bits_u8(from, from + 7)?;
            buffer.add_u8(state)?;
        }

        Ok(())
    }

}

impl AutoCreator for AllData {
    fn default() -> Self where Self: Sized {
        Self {
            id: 0,
            interrupt_pin: 0,
            relays_count: 0,
            relays_settings: [RelaySettings::new(); MAX_RELAYS_COUNT as usize],
            state_data: BitsU64::new(0),
        }
    }
}

impl Data for AllData {}

#[derive(PartialEq, Debug)]
pub struct SwitchCountingSettings {
    pub switch_limit_interval: RelativeSeconds16,
    pub max_switch_count: u8,
}

impl SwitchCountingSettings {
    pub const fn new(switch_limit_interval_seconds: u16, max_switch_count: u8) -> Self {
        Self {
            switch_limit_interval: RelativeSeconds16(switch_limit_interval_seconds),
            max_switch_count,
        }
    }

    pub fn create(switch_limit_interval: u16, max_switch_count: u8) -> Self {
        Self {
            switch_limit_interval: RelativeSeconds16(switch_limit_interval),
            max_switch_count: max_switch_count,
        }
    }

}

impl Parser for SwitchCountingSettings {

    fn parse_from(&mut self, data: &[u8]) -> Result<(), Errors> {
        if data.len() != 3 {
            return Err(Errors::InvalidDataSize)
        } else {
            self.switch_limit_interval = RelativeSeconds16::extract(&data[0..2]);
            self.max_switch_count = data[2];
            Ok(())
        }
    }
}

impl Serializable for SwitchCountingSettings {

    fn serialize<B: BufferWriter>(&self, buffer: &mut B) -> Result<(), Errors> {
        buffer.add_u16(self.switch_limit_interval.0)?;
        buffer.add_u8(self.max_switch_count)
    }

}

impl AutoCreator for SwitchCountingSettings {
    fn default() -> Self where Self: Sized {
        Self {
            switch_limit_interval: RelativeSeconds16(0),
            max_switch_count: 0,
        }
    }
}

impl Data for SwitchCountingSettings {}

#[derive(PartialEq, Debug)]
pub struct StateSwitchDatas {
    pub data: [StateSwitchData; SWITCHES_DATA_BUFFER_SIZE as usize],
    pub count: u8,
}

impl StateSwitchDatas {

    pub const fn new() -> Self {
        Self {
            data: [StateSwitchData::new(0, 0); SWITCHES_DATA_BUFFER_SIZE as usize],
            count: 0,
        }
    }

    pub fn add_switch_data(&mut self, fix_try_count: u8, fix_last_try_time_seconds: u32) -> Result<(), Errors> {
        if self.count < MAX_RELAYS_COUNT {
            let data = StateSwitchData::new(fix_try_count, fix_last_try_time_seconds);
            self.data[self.count as usize] = data;
            self.count += 1;
            Ok(())
        } else {
            Err(Errors::RelayCountOverflow)
        }
    }

    fn set_count(&mut self, new_count: u8) -> Result<(), Errors> {
        if new_count > SWITCHES_DATA_BUFFER_SIZE {
            Err(Errors::SwitchesDataCountOverflow)
        } else {
            self.count = new_count;
            Ok(())
        }
    }

    fn set_data(&mut self, index: u8, new_data: StateSwitchData) -> Result<(), Errors> {
        if index >= SWITCHES_DATA_BUFFER_SIZE {
            Err(Errors::SwitchesDataCountOverflow)
        } else {
            self.data[index as usize] = new_data;
            Ok(())
        }
    }

}

impl Parser for StateSwitchDatas {

    fn parse_from(&mut self, data: &[u8]) -> Result<(), Errors> {
        if data.len() < 1 {
            return Err(Errors::NotEnoughDataGot);
        }
        let relays_count = data[0];
        if (data.len() as u8) != relays_count * 5 + 1 {
            return Err(Errors::InvalidDataSize);
        }
        self.set_count(relays_count)?;
        for i in 0..relays_count  {
            let pos = 1 + i as usize * 5;
            let switch_count_data = data[pos];
            let timestamp = u32::extract(&data[pos + 1..pos + 5]);
            self.set_data(i, StateSwitchData::create(switch_count_data, timestamp))?;
        }
        Ok(())
    }
}

impl Serializable for StateSwitchDatas {

    fn serialize<B: BufferWriter>(&self, buffer: &mut B) -> Result<(), Errors> {
        buffer.add_u8(self.count)?;
        for i in 0..self.count {
            let data = &self.data[i as usize];
            buffer.add_u8(data.state.bits)?;
            data.time_stamp.serialize(buffer)?;
        }
        Ok(())
    }

}

impl AutoCreator for StateSwitchDatas {
    fn default() -> Self where Self: Sized {
        Self::new()
    }
}

impl Data for StateSwitchDatas {}

#[derive(Copy, Clone, PartialEq, Debug)]
pub struct StateSwitchData {
    state: BitsU8,
    time_stamp: RelativeSeconds
}

impl StateSwitchData {

    pub const fn new(state_byte: u8, time_stamp: u32) -> Self {
        Self {
            state: BitsU8::new(state_byte),
            time_stamp: RelativeSeconds::new(time_stamp),
        }
    }

    pub fn create(data: u8, timestamp: u32) -> Self {
        Self {
            state: BitsU8::new(data),
            time_stamp: RelativeSeconds::new(timestamp),
        }
    }

}

#[derive(PartialEq, Debug)]
pub struct ContactsWaitData {
    relays_count: u8,
    contacts_wait_start_timestamps: [RelativeSeconds; MAX_RELAYS_COUNT as usize],
}

impl ContactsWaitData {

    pub const fn new() -> Self {
        Self {
            relays_count: 0,
            contacts_wait_start_timestamps: [RelativeSeconds::new(0); MAX_RELAYS_COUNT as usize],
        }
    }

    pub fn add(&mut self, timestamp: u32) -> Result<(), Errors> {
        if self.relays_count < MAX_RELAYS_COUNT {
            self.contacts_wait_start_timestamps[self.relays_count as usize] = RelativeSeconds::new(timestamp);
            self.relays_count += 1;
            Ok(())
        } else {
            Err(Errors::RelayCountOverflow)
        }
    }

    fn update_count(&mut self, new_count: u8) -> Result<(), Errors> {
        if new_count > MAX_RELAYS_COUNT {
            Err(Errors::RelayCountOverflow)
        } else {
            self.relays_count = new_count;
            Ok(())
        }
    }

    fn update_timestamp(&mut self, relay_idx: u8, timestamp: u32) -> Result<(), Errors> {
        if relay_idx > self.relays_count {
            Err(Errors::RelayIndexOutOfRange)
        } else {
            self.contacts_wait_start_timestamps[relay_idx as usize] = RelativeSeconds::new(timestamp);
            Ok(())
        }
    }
}

impl Parser for ContactsWaitData {

    fn parse_from(&mut self, data: &[u8]) -> Result<(), Errors> {
        if data.len() < 1 {
            return Err(Errors::NotEnoughDataGot);
        }
        let relays_count = data[0];
        if (data.len() as u8) != relays_count * 4 + 1 {
            return Err(Errors::InvalidDataSize);
        }
        self.update_count(relays_count)?;
        for i in 0..relays_count {
            let pos = 1 + i as usize * 4;
            let timestamp = u32::extract(&data[pos..pos + 4]);
            self.update_timestamp(i, timestamp)?;
        }
        Ok(())
    }
}

impl Serializable for ContactsWaitData {

    fn serialize<B: BufferWriter>(&self, buffer: &mut B) -> Result<(), Errors> {
        buffer.add_u8(self.relays_count)?;
        for i in 0..self.relays_count {
            self.contacts_wait_start_timestamps[i as usize].serialize(buffer)?;
        }
        Ok(())
    }

}

impl AutoCreator for ContactsWaitData {
    fn default() -> Self where Self: Sized {
        Self {
            relays_count: 0,
            contacts_wait_start_timestamps: [RelativeSeconds::new(0); MAX_RELAYS_COUNT as usize],
        }
    }
}

impl Data for ContactsWaitData {}

#[derive(PartialEq, Debug)]
pub struct FixDataContainer {
    fix_data: [FixData; MAX_RELAYS_COUNT as usize],
    fix_data_count: u8,
}

impl FixDataContainer {

    fn update_count(&mut self, new_count: u8) -> Result<(), Errors> {
        if new_count > MAX_RELAYS_COUNT {
            Err(Errors::RelayCountOverflow)
        } else {
            self.fix_data_count = new_count;
            Ok(())
        }
    }

    fn update_data(&mut self, relay_idx: u8, fix_try_count: u8, fix_last_try_time: u32) -> Result<(), Errors> {
        if relay_idx > self.fix_data_count {
            Err(Errors::RelayIndexOutOfRange)
        } else {
            self.fix_data[relay_idx as usize] = FixData::create(fix_try_count, fix_last_try_time);
            Ok(())
        }
    }

    pub const fn new() -> Self {
        Self {
            fix_data: [FixData::new(0, 0); MAX_RELAYS_COUNT as usize],
            fix_data_count: 0,
        }
    }

    pub fn add_fix_data(&mut self, fix_try_count: u8, fix_last_try_time_seconds: u32) -> Result<(), Errors> {
        if self.fix_data_count < MAX_RELAYS_COUNT {
            let fix_data = FixData::new(fix_try_count, fix_last_try_time_seconds);
            self.fix_data[self.fix_data_count as usize] = fix_data;
            self.fix_data_count += 1;
            Ok(())
        } else {
            Err(Errors::RelayCountOverflow)
        }
    }

    pub fn get_fix_data(&self, index: u8) -> Option<&FixData> {
        if index < self.fix_data_count {
            Some(&self.fix_data[index as usize])
        } else {
            None
        }
    }

    pub fn get_fix_data_count(&self) -> u8 {
        self.fix_data_count
    }

}

impl Parser for FixDataContainer {

    fn parse_from(&mut self, data: &[u8]) -> Result<(), Errors> {
        if data.len() < 1 {
            return Err(Errors::NotEnoughDataGot);
        }
        let fix_data_count = data[0];
        if (data.len() as u8) < fix_data_count * 5 + 1 {
            return Err(Errors::NotEnoughDataGot);
        }
        self.update_count(fix_data_count)?;
        for i in 0..fix_data_count {
            let pos = 1 + i as usize * 5;
            let try_count = data[pos];
            let try_time = u32::extract(&data[pos + 1..pos + 5]);
            self.update_data(i, try_count, try_time)?;
        }
        Ok(())
    }
}

impl Serializable for FixDataContainer {

    fn serialize<B: BufferWriter>(&self, buffer: &mut B) -> Result<(), Errors> {
        buffer.add_u8(self.fix_data_count as u8)?;
        for i in 0..self.fix_data_count {
            let fix_data = &self.fix_data[i as usize];
            buffer.add_u8(fix_data.fix_try_count)?;
            fix_data.fix_last_try_time.serialize(buffer)?;
        }
        Ok(())
    }

}

impl AutoCreator for FixDataContainer {
    fn default() -> Self where Self: Sized {
        Self {
            fix_data: [FixData::new(0, 0); MAX_RELAYS_COUNT as usize],
            fix_data_count: 0,
        }
    }
}

impl Data for FixDataContainer {}

#[derive(Copy, Clone, PartialEq, Debug)]
pub struct FixData {
    fix_try_count: u8,
    fix_last_try_time: RelativeSeconds
}

impl FixData {

    pub const fn new(fix_try_count: u8, fix_last_try_time_seconds: u32) -> Self {
        Self {
            fix_try_count,
            fix_last_try_time: RelativeSeconds::new(fix_last_try_time_seconds),
        }
    }

    pub fn create(fix_try_count: u8, fix_last_try_time: u32) -> Self {
        Self {
            fix_try_count,
            fix_last_try_time: RelativeSeconds::new(fix_last_try_time),
        }
    }

}

#[derive(PartialEq, Debug)]
pub struct CyclesStatistics {
    min_cycle_duration: RelativeMillis16,
    max_cycle_duration: RelativeMillis16,
    avg_cycle_duration: RelativeMillis16,
    cycles_count: u64,
}

impl CyclesStatistics {

    pub fn default() -> Self {
        Self {
            min_cycle_duration: RelativeMillis16(0),
            max_cycle_duration: RelativeMillis16(0),
            avg_cycle_duration: RelativeMillis16(0),
            cycles_count: 0,
        }
    }

    pub fn new(min_cycle_duration: u16, max_cycle_duration: u16, avg_cycle_duration: u16, cyles_count: u64) -> Self {
        Self {
            min_cycle_duration: RelativeMillis16(min_cycle_duration),
            max_cycle_duration: RelativeMillis16(max_cycle_duration),
            avg_cycle_duration: RelativeMillis16(avg_cycle_duration),
            cycles_count: cyles_count,
        }
    }

    pub fn create(min_cycle_duration: u16, max_cycle_duration: u16, avg_cycle_duration: u16, cyles_count: u64) -> Self {
        Self {
            min_cycle_duration: RelativeMillis16(min_cycle_duration),
            max_cycle_duration: RelativeMillis16(max_cycle_duration),
            avg_cycle_duration: RelativeMillis16(avg_cycle_duration),
            cycles_count: cyles_count,
        }
    }

}

impl Parser for CyclesStatistics {

    fn parse_from(&mut self, data: &[u8]) -> Result<(), Errors> {
        if data.len() != 14 {
            Err(Errors::InvalidDataSize)
        } else {
            self.min_cycle_duration = RelativeMillis16::extract( &data[0..2]);
            self.max_cycle_duration = RelativeMillis16::extract( &data[2..4]);
            self.avg_cycle_duration = RelativeMillis16::extract( &data[4..6]);
            self.cycles_count = u64::extract(&data[6..14]);
            Ok(())
        }
    }
}

impl Serializable for CyclesStatistics {

    fn serialize<B: BufferWriter>(&self, buffer: &mut B) -> Result<(), Errors> {
        buffer.add_u16(self.min_cycle_duration.0)?;
        buffer.add_u16(self.max_cycle_duration.0)?;
        buffer.add_u16(self.avg_cycle_duration.0)?;
        buffer.add_u64(self.cycles_count)?;
        Ok(())
    }

}

impl AutoCreator for CyclesStatistics {
    fn default() -> Self where Self: Sized {
        Self::default()
    }
}

impl Data for CyclesStatistics {}

#[derive(PartialEq, Debug)]
pub struct StateFixSettings {
    switch_try_duration: RelativeMillis16,
    switch_try_count: u8,
    wait_delay: RelativeSeconds8,
    contact_ready_wait_delay: RelativeMillis16,
}

impl StateFixSettings {

    pub fn default() -> Self {
        Self {
            switch_try_duration: RelativeMillis16(0),
            switch_try_count: 0,
            wait_delay: RelativeSeconds8(0),
            contact_ready_wait_delay: RelativeMillis16(0),
        }
    }

    pub fn new(switch_try_duration: u16, switch_try_count: u8, wait_delay: u8, contact_ready_wait_delay: u16) -> Self {
        Self {
            switch_try_duration: RelativeMillis16(switch_try_duration),
            switch_try_count,
            wait_delay: RelativeSeconds8(wait_delay),
            contact_ready_wait_delay: RelativeMillis16(contact_ready_wait_delay),
        }
    }

}

impl Parser for StateFixSettings {

    fn parse_from(&mut self, data: &[u8]) -> Result<(), Errors> {
        if data.len() != 6 {
            Err(Errors::InvalidDataSize)
        } else {
            self.switch_try_duration = RelativeMillis16::extract(&data[0..2]);
            self.switch_try_count = data[2];
            self.wait_delay = RelativeSeconds8(data[3]);
            self.contact_ready_wait_delay = RelativeMillis16::extract(&data[4..6]);
            Ok(())
        }
    }
}

impl Serializable for StateFixSettings {

    fn serialize<B: BufferWriter>(&self, buffer: &mut B) -> Result<(), Errors> {
        buffer.add_u16(self.switch_try_duration.0)?;
        buffer.add_u8(self.switch_try_count)?;
        buffer.add_u8(self.wait_delay.0)?;
        buffer.add_u16(self.contact_ready_wait_delay.0)
    }

}

impl AutoCreator for StateFixSettings {
    fn default() -> Self where Self: Sized {
        Self {
            switch_try_duration: RelativeMillis16(0),
            switch_try_count: 0,
            wait_delay: RelativeSeconds8(0),
            contact_ready_wait_delay: RelativeMillis16(0),
        }
    }
}

impl Data for StateFixSettings {}

#[derive(PartialEq, Debug)]
pub struct State {
    pub data: BitsU64,
    pub count: u8,
}

impl State {
    pub fn new () -> Self {
        Self { data: BitsU64::new(0), count: 0 }
    }

    pub fn create(count: u8, raw_data: u64) -> Result<Self, Errors> {
        if count > MAX_RELAYS_COUNT {
            return Err(Errors::RelayCountOverflow);
        }
        let raw_data = raw_data & ((1 << (count * 2)) - 1);
        Ok( Self { count, data: BitsU64::new(raw_data) } )
    }

    fn parse_state_data_force(bytes_count: u8, data: &[u8]) -> Result<BitsU64, Errors> {
        let mut state_data = BitsU64::new(0);
        for i in 0..bytes_count {
            let from = i * 8;
            state_data.set_byte(from, from  + 7, data[i as usize])?;
        }
        Ok(state_data)
    }

}

impl Parser for State {

    fn parse_from(&mut self, data: &[u8]) -> Result<(), Errors> {
        if data.len() < 1 {
            return Err(Errors::NotEnoughDataGot);
        }
        self.count = data[0];
        if self.count > MAX_RELAYS_COUNT {
            return Err(Errors::RelayCountOverflow);
        }
        let pairs_count = (self.count + 1) / 2;
        if data.len() != 1 + pairs_count as usize {
            return Err(Errors::InvalidDataSize);
        }
        self.data = Self::parse_state_data_force(pairs_count, &data[1..])?;
        Ok(())
    }
}

impl Serializable for State {

    fn serialize<B: BufferWriter>(&self, buffer: &mut B) -> Result<(), Errors> {
        buffer.add_u8(self.count)?;
        let pairs_count = (self.count + 1) / 2;
        for i in 0..pairs_count {
            buffer.add_u8( self.data.bits_u8(i * 8, (i + 1) * 8 - 1)? )?;
        }
        Ok(())
    }

}

impl AutoCreator for State {
    fn default() -> Self where Self: Sized {
        Self { data: BitsU64::new(0), count: 0 }
    }
}

impl Data for State {}

#[derive(PartialEq, Debug)]
pub struct RelaySingleState {
    data: BitsU8,
}

impl RelaySingleState {

    pub fn new (relay_idx: u8, is_on: bool) -> Self {
        let mut result = Self { data: BitsU8::new(relay_idx) };
        if is_on {
            result.data.set(4);
        }
        result
    }

    pub fn relay_index(&self) -> u8 {
        self.data.bits(0, 3).unwrap()
    }

    pub fn is_set(&self) -> bool {
        self.data.get(4)
    }

}

impl Parser for RelaySingleState {

    fn parse_from(&mut self, data: &[u8]) -> Result<(), Errors> {
        if data.len() == 1 {
            self.data = BitsU8::new(data[0]);
            Ok(())
        } else {
            Err(Errors::InvalidDataSize)
        }
    }
}

impl Serializable for RelaySingleState {

    fn serialize<B: BufferWriter>(&self, buffer: &mut B) -> Result<(), Errors> {
        buffer.add_u8(self.data.bits)
    }

}

impl AutoCreator for RelaySingleState {
    fn default() -> Self where Self: Sized {
        Self { data: BitsU8::new(0) }
    }
}

impl Data for RelaySingleState {}

#[derive(PartialEq, Debug)]
pub struct RelayState {
    data: BitsU8,
}

impl RelayState {
    pub fn create(relay_index: u8, on: bool, disabled: bool) -> Result<Self, Errors> {
        if relay_index > MAX_RELAYS_COUNT {
            return Err(Errors::RelayCountOverflow);
        }
        let mut data = BitsU8::new(relay_index & 0x0f);
        data.set_value(5, on);
        data.set_value(6, disabled);
        Ok(RelayState { data })
    }

    #[inline(always)]
    pub fn relay_index(&self) -> u8 {
        self.data.bits & 0x0f
    }

    #[inline(always)]
    pub fn is_on(&self) -> bool {
        self.data.get(5)
    }

    #[inline(always)]
    pub fn is_disabled(&self) -> bool {
        self.data.get(6)
    }

    #[inline(always)]
    pub fn is_monitoring_on(&self) -> bool {
        self.data.get(0)
    }

    #[inline(always)]
    pub fn is_control_on(&self) -> bool {
        self.data.get(7)
    }

    #[inline(always)]
    pub fn set_on(&mut self, on: bool) {
        self.data.set_value(5, on);
    }

    #[inline(always)]
    pub fn set_disables(&mut self, on: bool) {
        self.data.set_value(6, on);
    }
}

impl Parser for RelayState {

    fn parse_from(&mut self, data: &[u8]) -> Result<(), Errors> {
        if data.len() == 1 {
            self.data = BitsU8::new(data[0]);
            Ok(())
        } else {
            Err(Errors::InvalidDataSize)
        }
    }
}

impl Serializable for RelayState {

    fn serialize<B: BufferWriter>(&self, buffer: &mut B) -> Result<(), Errors> {
        buffer.add_u8(self.data.bits)
    }

}

impl AutoCreator for RelayState {
    fn default() -> Self where Self: Sized {
        RelayState { data: BitsU8::new(0) }
    }
}

impl Data for RelayState {}

#[derive(Copy, Clone, PartialEq, Debug)]
pub struct PinData {
    data: u8,
}

impl PinData {
    const fn new() -> Self {
        Self { data: 0 }
    }
    fn create(data: u8) -> Self {
        Self { data }
    }

    pub fn data(&self) -> u8 {
        self.data
    }
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub struct RelaySettings {
    set_pin: PinData,
    monitor_pin: PinData,
    control_pin: PinData,
}

impl RelaySettings {

    pub const fn new() -> Self {
        Self {
            set_pin: PinData::new(),
            monitor_pin: PinData::new(),
            control_pin: PinData::new(),
        }
    }

    pub fn create(set_pin: u8, monitor_pin: u8, control_pin: u8) -> Self {
        Self {
            set_pin: PinData::create(set_pin),
            monitor_pin: PinData::create(monitor_pin),
            control_pin: PinData::create(control_pin),
        }
    }

    fn serialize<B: BufferWriter>(&self, buffer: &mut B) -> Result<(), Errors> {
        buffer.add_u8(self.set_pin.data)?;
        buffer.add_u8(self.monitor_pin.data)?;
        buffer.add_u8(self.control_pin.data)
    }

    pub fn set_pin(&self) -> PinData {
        self.set_pin
    }

    pub fn monitor_pin(&self) -> PinData {
        self.monitor_pin
    }

    pub fn control_pin(&self) -> PinData {
        self.control_pin
    }

}

#[derive(PartialEq, Debug)]
pub struct RelaysSettings {
    pub relays: [RelaySettings; MAX_RELAYS_COUNT as usize],
    pub relays_count: u8,
}

impl RelaysSettings {
    pub(crate) fn add(&mut self, set_pin: u8, monitor_pin: u8, control_pin: u8) -> Result<(), Errors> {
        if self.relays_count < MAX_RELAYS_COUNT {
            self.relays[self.relays_count as usize] = RelaySettings::create(set_pin, monitor_pin, control_pin);
            self.relays_count += 1;
            Ok(())
        } else {
            Err(Errors::RelayCountOverflow)
        }
    }

    pub fn get_relays(&self) -> &[RelaySettings] {
        &self.relays[..self.relays_count as usize]
    }

    pub const fn new() -> Self {
        Self {
            relays: [RelaySettings::new(); MAX_RELAYS_COUNT as usize],
            relays_count: 0,
        }
    }

    fn set_relay_count(&mut self, relays_count: u8) -> Result<(), Errors> {
        if relays_count > MAX_RELAYS_COUNT {
            return Err(Errors::RelayCountOverflow);
        }
        self.relays_count = relays_count;
        Ok(())
    }

    // fn set_relay_settings(&mut self, relay_index: u8, relay_settings: RelaySettings) -> Result<(), Errors> {
    //     if relay_index >= self.relays_count {
    //         return Err(Errors::RelayIndexOutOfRange);
    //     }
    //     self.relays[relay_index as usize] = relay_settings;
    //     Ok(())
    // }

    fn parse_items<'a>(data: &'a[u8], relays_count: u8, relays_settings_buffer: &mut [RelaySettings]) -> Result<&'a[u8], Errors> {
        if (data.len() as u8) < relays_count * 3 {
            return Err(Errors::NotEnoughDataGot);
        }
        for i in 0..relays_count as usize {
            let pos = i * 3;
            let set_pin = data[pos];
            let monitor_pin = data[pos + 1];
            let control_pin = data[pos + 2];
            relays_settings_buffer[i] = RelaySettings::create(set_pin, monitor_pin, control_pin);
        }
        let  shift = relays_count as usize * 3;
        Ok(if data.len() > shift { &data[shift..] } else { &data[0..0] })
    }

}

impl Parser for RelaysSettings {

    fn parse_from(&mut self, data: &[u8]) -> Result<(), Errors> {
        if data.len() < 1 {
            return Err(Errors::NotEnoughDataGot);
        }
        let relays_count = data[0];
        if (data.len() as u8) < relays_count * 3 + 1 {
            return Err(Errors::NotEnoughDataGot);
        }
        self.set_relay_count(relays_count)?;
        _ = Self::parse_items(&data[1..], relays_count, &mut self.relays)?;
        Ok(())
    }
}

impl Serializable for RelaysSettings {
    fn serialize<B: BufferWriter>(&self, buffer: &mut B) -> Result<(), Errors> {
        buffer.add_u8(self.relays_count)?;
        for i in 0..self.relays_count as usize {
            let setting = &self.relays[i];
            setting.serialize(buffer)?;
        }
        Ok(())
    }
}

impl AutoCreator for RelaysSettings {
    fn default() -> Self where Self: Sized {
        Self::new()
    }
}

impl Data for RelaysSettings {}


#[repr(u8)]
#[derive(Copy, Clone, PartialEq, Debug)]
pub enum Signals {
    None = 0x00,
    GetTimeStamp = 0x14,
    RelayStateChanged = 0x15,
    MonitoringStateChanged = 0x16,
    ControlStateChanged = 0x17,
    StateFixTry = 0x19,
    Unknown = 0xff,
}

#[repr(u8)]
#[derive(Copy, Clone, PartialEq, Debug)]
pub enum SignalData {
    GetTimeStamp = Signals::GetTimeStamp as u8,
    RelayStateChanged(RelaySignalDataExt) = Signals::RelayStateChanged as u8,
    MonitoringStateChanged(RelaySignalData) = Signals::MonitoringStateChanged as u8,
    ControlStateChanged(RelaySignalData) = Signals::ControlStateChanged as u8,
    StateFixTry (RelaySignalData)= Signals::StateFixTry as u8,
}

impl SignalData {
    pub fn code(&self) -> Signals {
        match self {
            SignalData::GetTimeStamp => Signals::GetTimeStamp,
            SignalData::RelayStateChanged(_) => Signals::RelayStateChanged,
            SignalData::MonitoringStateChanged(_) => Signals::MonitoringStateChanged,
            SignalData::ControlStateChanged(_) => Signals::ControlStateChanged,
            SignalData::StateFixTry(_) => Signals::StateFixTry,
        }
    }

    pub fn create(signal: Signals) -> Result<Self, Errors> {
        match signal {
            Signals::GetTimeStamp => Ok(SignalData::GetTimeStamp),
            Signals::RelayStateChanged => Ok(SignalData::RelayStateChanged(RelaySignalDataExt::default())),
            Signals::MonitoringStateChanged => Ok(SignalData::MonitoringStateChanged(RelaySignalData::default())),
            Signals::ControlStateChanged => Ok(SignalData::ControlStateChanged(RelaySignalData::default())),
            Signals::StateFixTry => Ok(SignalData::StateFixTry(RelaySignalData::default())),
            _ => Err(Errors::UndefinedOperation),
        }
    }

    pub fn parse(signal: Signals, data: &[u8]) -> Result<Self, Errors> {
        match signal {
            Signals::GetTimeStamp => Ok(SignalData::GetTimeStamp),
            Signals::RelayStateChanged => Ok(SignalData::RelayStateChanged(RelaySignalDataExt::parse(data)?)),
            Signals::MonitoringStateChanged => Ok(SignalData::MonitoringStateChanged(RelaySignalData::parse(data)?)),
            Signals::ControlStateChanged => Ok(SignalData::ControlStateChanged(RelaySignalData::parse(data)?)),
            Signals::StateFixTry => Ok(SignalData::StateFixTry(RelaySignalData::parse(data)?)),
            _ => Err(Errors::UndefinedOperation),
        }
    }
}

impl Signals {
    pub fn get(signal_code: u8) -> Result<Signals, Errors> {
        if signal_code == Signals::MonitoringStateChanged as u8 {
            Ok(Signals::MonitoringStateChanged)
        } else if signal_code == Signals::StateFixTry as u8 {
            Ok(Signals::StateFixTry)
        } else if signal_code == Signals::ControlStateChanged as u8 {
            Ok(Signals::ControlStateChanged)
        } else if signal_code == Signals::RelayStateChanged as u8 {
            Ok(Signals::RelayStateChanged)
        } else if signal_code == Signals::GetTimeStamp as u8 {
            Ok(Signals::GetTimeStamp)
        } else {
            Err(Errors::InstructionNotRecognized(signal_code))
        }
    }

}


pub trait RelaySignalDataGetter {
    fn get_relative_timestamp(&self) -> RelativeSeconds;
    fn get_relay_idx(&self) -> u8;
    fn is_on(&self) -> bool;
}

trait RelaySignalDataSetter : AutoCreator {
    fn set_relative_timestamp(&mut self, relative_timestamp: RelativeSeconds);
    fn set_relay_idx(&mut self, relay_idx: u8);
    fn set_is_on(&mut self, is_on: bool);
    fn parse_from(&mut self, data: &[u8]) -> Result<(), Errors> {
        if data.len() != 5 {
            return Err(Errors::InvalidDataSize);
        }
        let state = u8::parse(&data[0..1])?;
        self.set_relay_idx( state & 0x0f_u8 );
        self.set_is_on( state & 0x10 > 0 );
        self.set_relative_timestamp( RelativeSeconds::parse(&data[1..])? );

        Ok(())
    }
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct RelaySignalData {
    relative_timestamp: RelativeSeconds,
    relay_idx: u8,
    is_on: bool,
}

impl RelaySignalData {
    pub fn new(relative_timestamp: RelativeSeconds, relay_idx: u8, is_on: bool) -> Self {
        Self {
            relative_timestamp,
            relay_idx,
            is_on,
        }
    }
}

impl RelaySignalDataGetter for RelaySignalData {
    fn get_relative_timestamp(&self) -> RelativeSeconds {
        self.relative_timestamp
    }

    fn get_relay_idx(&self) -> u8 {
        self.relay_idx
    }

    fn is_on(&self) -> bool {
        self.is_on
    }
}

impl RelaySignalDataSetter for RelaySignalData {
    fn set_relative_timestamp(&mut self, relative_timestamp: RelativeSeconds) {
        self.relative_timestamp = relative_timestamp;
    }

    fn set_relay_idx(&mut self, relay_idx: u8) {
        self.relay_idx = relay_idx;
    }

    fn set_is_on(&mut self, is_on: bool) {
        self.is_on = is_on;
    }
}

impl Parser for RelaySignalData {
    fn parse_from(&mut self, data: &[u8]) -> Result<(), Errors> {
        RelaySignalDataSetter::parse_from(self, data)
    }
}

impl Serializable for RelaySignalData {
    fn serialize<B: BufferWriter>(&self, buffer: &mut B) -> Result<(), Errors> {
        let mut state = 0_u8;
        state |= self.get_relay_idx() & 0x0f;
        if self.is_on() {
            state |= 0x10;
        }
        state.serialize(buffer)?;
        self.get_relative_timestamp().serialize(buffer)?;
        Ok(())
    }
}

impl AutoCreator for RelaySignalData {
    fn default() -> Self where Self: Sized {
        Self::new(RelativeSeconds::new(0), 0, false)
    }
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct RelaySignalDataExt {
    relative_timestamp: RelativeSeconds,
    relay_idx: u8,
    is_on: bool,
    is_called_internally: bool,
}

impl RelaySignalDataExt {
    pub fn new(relative_timestamp: RelativeSeconds, relay_idx: u8, is_on: bool, is_called_internally: bool) -> Self {
        Self {
            relative_timestamp,
            relay_idx,
            is_on,
            is_called_internally,
        }
    }
}

impl RelaySignalDataGetter for RelaySignalDataExt {
    fn get_relative_timestamp(&self) -> RelativeSeconds {
        self.relative_timestamp
    }

    fn get_relay_idx(&self) -> u8 {
        self.relay_idx
    }

    fn is_on(&self) -> bool {
        self.is_on
    }
}

impl RelaySignalDataSetter for RelaySignalDataExt {
    fn set_relative_timestamp(&mut self, relative_timestamp: RelativeSeconds) {
        self.relative_timestamp = relative_timestamp;
    }

    fn set_relay_idx(&mut self, relay_idx: u8) {
        self.relay_idx = relay_idx;
    }

    fn set_is_on(&mut self, is_on: bool) {
        self.is_on = is_on;
    }
}

impl AutoCreator for RelaySignalDataExt {
    fn default() -> Self where Self: Sized {
        Self::new(RelativeSeconds::new(0), 0, false, false)
    }
}

impl Parser for RelaySignalDataExt {
    fn parse_from(&mut self, data: &[u8]) -> Result<(), Errors> {
        RelaySignalDataSetter::parse_from(self, data)?;
        self.is_called_internally = data[0] & 0x20 > 0;
        Ok(())
    }
}

impl Serializable for RelaySignalDataExt {
    fn serialize<B: BufferWriter>(&self, buffer: &mut B) -> Result<(), Errors> {
        let mut state = 0_u8;
        state |= self.get_relay_idx() & 0x0f;
        if self.is_on() {
            state |= 0x10;
        }
        if self.is_called_internally {
            state |= 0x20;
        }
        state.serialize(buffer)?;
        self.get_relative_timestamp().serialize(buffer)?;
        Ok(())
    }
}