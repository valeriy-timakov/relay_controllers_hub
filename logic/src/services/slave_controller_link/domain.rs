#![allow(unsafe_code)]

use crate::errors::Errors;
use crate::hal_ext::rtc_wrapper::{ RelativeSeconds };
use crate::utils::{BitsU64, BitsU8};
use crate::utils::dma_read_buffer::{Buffer, BufferWriter};


pub const MAX_RELAYS_COUNT: usize = 16;
pub const SWITCHES_DATA_BUFFER_SIZE: usize = 50;


#[repr(u8)]
#[derive(Copy, Clone, PartialEq, Debug)]
pub enum Operation {
    None = 0x00,
    Read = 0x01,
    Set = 0x02,
    Success = 0x03,
    Error = 0x04,
    Signal = 0x05,
    Response = 0x06,
    Command = 0x07,
    Unknown = 0x0f
}

#[repr(u8)]
#[derive(Copy, Clone, PartialEq)]
pub enum Signals {
    None = 0x00,
    GetTimeStamp = 0x14,
    RelayStateChanged = 0x15,
    MonitoringStateChanged = 0x16,
    ControlStateChanged = 0x17,
    StateFixTry = 0x19,
    Unknown = 0xff,
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
    fn discriminant(&self) -> u8 {
        unsafe { *(self as *const Self as *const u8) }
    }

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

    pub fn parse_data(&self, data: &[u8]) -> Result<Conversation<EmptyRequest, State>, Errors> {
        self::Data::parse(data).map(|data| Conversation::Data(data))
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
#[derive(Copy, Clone)]
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
        } else {
            ErrorCode::EUndefinedCode(code)
        }
    }
}

pub enum Conversation<RQ: Request, D: Data + 'static> {
    Request(RQ),
    Data(D),
    DataCashed(&'static mut D),
    Response(Response),
}

pub trait Request {  }

pub trait Data {

    fn parse(data: &[u8]) -> Result<Self, Errors> where Self: Sized {
        let mut result = Self::default();
        result.parse_from(data)?;
        Ok(result)
    }

    fn parse_from(&mut self, data: &[u8]) -> Result<(), Errors>;

    fn serialize<B: BufferWriter>(&self, buffer: &mut B)->Result<(), Errors>;

    fn default() -> Self where Self: Sized;

}

pub enum Response {
    Success,
    Error(ErrorCode),
}

pub struct EmptyRequest;

impl Request for EmptyRequest {}

pub struct RelayIndexRequest {
    pub index: u8,
}

impl Request for RelayIndexRequest {}

impl Data for RelativeSeconds {

    fn parse(data: &[u8]) -> Result<Self, Errors> where Self: Sized {
        Ok(RelativeSeconds::new(u32::parse(data)?))
    }

    fn parse_from(&mut self, data: &[u8]) -> Result<(), Errors> {
        *self = RelativeSeconds::new(u32::parse(data)?);
        Ok(())
    }


    #[inline(always)]
    fn serialize<B: BufferWriter>(&self, buffer: &mut B) -> Result<(), Errors> {
        buffer.add_u32(self.value())
    }

    fn default() -> Self where Self: Sized {
        RelativeSeconds::new(0)
    }

}

trait Extractor {
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


pub struct RelativeMillis16(u16);
pub struct RelativeSeconds8(u8);
pub struct RelativeSeconds16(u16);

impl Extractor for RelativeMillis16 {

    #[inline(always)]
    fn extract(data: &[u8]) -> RelativeMillis16 {
        RelativeMillis16(u16::extract(data))
    }

    // #[inline(always)]
    // fn serialize<B: BufferWriter>(&self, buffer: &mut B) -> Result<(), Errors> {
    //     buffer.add_u16(self.0)
    // }

}

impl Extractor for RelativeSeconds8 {

    #[inline(always)]
    fn extract(data: &[u8]) -> RelativeSeconds8 {
        RelativeSeconds8(u8::extract(data))
    }

    // #[inline(always)]
    // fn serialize<B: BufferWriter>(&self, buffer: &mut B) -> Result<(), Errors> {
    //     buffer.add_u8(self.0)
    // }

}

impl Extractor for RelativeSeconds16 {

    #[inline(always)]
    fn extract(data: &[u8]) -> RelativeSeconds16 {
        RelativeSeconds16(u16::extract(data))
    }

    // #[inline(always)]
    // fn serialize<B: BufferWriter>(&self, buffer: &mut B) -> Result<(), Errors> {
    //     buffer.add_u16(self.0)
    // }

}

impl Extractor for BitsU64 {
    #[inline(always)]
    fn extract(data: &[u8]) -> Self {
        BitsU64::new(u64::extract(data))
    }
}

impl Data for u8 {

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

    #[inline(always)]
    fn serialize<B: BufferWriter>(&self, buffer: &mut B) -> Result<(), Errors> {
        buffer.add_u8(*self)
    }

    fn default() -> Self where Self: Sized {
        0
    }

}

impl Data for u16 {

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

        #[inline(always)]
        fn serialize<B: BufferWriter>(&self, buffer: &mut B) -> Result<(), Errors> {
            buffer.add_u16(*self)
        }

        fn default() -> Self where Self: Sized {
            0
        }

}

impl Data for u32 {

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

    #[inline(always)]
    fn serialize<B: BufferWriter>(&self, buffer: &mut B) -> Result<(), Errors> {
        buffer.add_u32(*self)
    }

    fn default() -> Self where Self: Sized {
        0
    }

}

pub struct AllData {
    pub id: u32,
    pub interrupt_pin: u8,
    pub relays_count: u8,
    pub relays_settings: [RelaySettings; MAX_RELAYS_COUNT],
    pub state_data: BitsU64,
}

impl AllData {
    pub const fn new() -> Self {
        Self {
            id: 0,
            interrupt_pin: 0,
            relays_count: 0,
            relays_settings: [RelaySettings::new(); MAX_RELAYS_COUNT],
            state_data: BitsU64::new(0),
        }
    }
}

impl Data for AllData {

    fn parse_from(&mut self, data: &[u8]) -> Result<(), Errors> {

        if data.len() < 6 {
            return Err(Errors::NotEnoughDataGot);
        }
        self.id = u32::parse(&data[0..4])?;
        self.interrupt_pin = data[4];
        self.relays_count = data[5];

        let pairs_count = self.relays_count as usize / 2;
        let data = &data[6..];
        if data.len() < pairs_count {
            return Err(Errors::NotEnoughDataGot);
        }
        let state_data = State::parse_state_data_force(pairs_count, data);
        self.state_data = BitsU64::new(state_data);

        let data = &data[pairs_count..];
        if data.len() != self.relays_count as usize * 3 {
            return Err(Errors::InvalidDataSize);
        }
        RelaysSettings::parse_items(&data[1..], self.relays_count, &mut self.relays_settings)?;
        Ok(())
    }

    fn serialize<B: BufferWriter>(&self, buffer: &mut B) -> Result<(), Errors> {
        self.id.serialize(buffer)?;
        buffer.add_u8(self.interrupt_pin)?;
        buffer.add_u8(self.relays_count)?;
        buffer.add_u64(self.state_data.bits)?;
        for setting in self.relays_settings {
            setting.serialize(buffer)?;
        }
        Ok(())
    }

    fn default() -> Self where Self: Sized {
        Self::new()
    }

}

pub struct SwitchCountingSettings {
    pub switch_limit_interval: RelativeSeconds16,
    pub max_switch_count: u8,
}

impl SwitchCountingSettings {
    pub const fn new() -> Self {
        Self {
            switch_limit_interval: RelativeSeconds16(0),
            max_switch_count: 0,
        }
    }

    pub fn create(switch_limit_interval: u16, max_switch_count: u8) -> Self {
        Self {
            switch_limit_interval: RelativeSeconds16(switch_limit_interval),
            max_switch_count: max_switch_count,
        }
    }

}

impl Data for SwitchCountingSettings {

    fn parse_from(&mut self, data: &[u8]) -> Result<(), Errors> {
        if data.len() != 3 {
            return Err(Errors::InvalidDataSize)
        } else {
            self.switch_limit_interval = RelativeSeconds16::extract(&data[0..2]);
            self.max_switch_count = data[2];
            Ok(())
        }
    }

    fn serialize<B: BufferWriter>(&self, buffer: &mut B) -> Result<(), Errors> {
        buffer.add_u16(self.switch_limit_interval.0)?;
        buffer.add_u8(self.max_switch_count)
    }

    fn default() -> Self where Self: Sized {
        Self::new()
    }

}

pub struct StateSwitchDatas {
    pub data: [StateSwitchData; SWITCHES_DATA_BUFFER_SIZE],
    pub count: usize,
}

impl StateSwitchDatas {

    pub const fn new() -> Self {
        Self {
            data: [StateSwitchData::new(); SWITCHES_DATA_BUFFER_SIZE],
            count: 0,
        }
    }

    fn set_count(&mut self, new_count: usize) -> Result<(), Errors> {
        if new_count > SWITCHES_DATA_BUFFER_SIZE {
            Err(Errors::SwitchesDataCountOverflow)
        } else {
            self.count = new_count;
            Ok(())
        }
    }

    fn set_data(&mut self, index: usize, new_data: StateSwitchData) -> Result<(), Errors> {
        if index >= SWITCHES_DATA_BUFFER_SIZE {
            Err(Errors::SwitchesDataCountOverflow)
        } else {
            self.data[index] = new_data;
            Ok(())
        }
    }

}

impl Data for StateSwitchDatas {

    fn parse_from(&mut self, data: &[u8]) -> Result<(), Errors> {
        if data.len() < 1 {
            return Err(Errors::NotEnoughDataGot);
        }
        let relays_count = data[0];
        if (data.len() as u8) != relays_count * 4 + 1 {
            return Err(Errors::InvalidDataSize);
        }
        self.set_count(relays_count as usize)?;
        for i in 0..relays_count as usize {
            let pos = 1 + i * 5;
            let switch_count_data = data[pos];
            let timestamp = u32::extract(&data[pos + 1..pos + 5]);
            self.set_data(i, StateSwitchData::create(switch_count_data, timestamp))?;
        }
        Ok(())
    }

    fn serialize<B: BufferWriter>(&self, buffer: &mut B) -> Result<(), Errors> {
        buffer.add_u8(self.count as u8)?;
        for i in 0..self.count {
            let data = &self.data[i];
            buffer.add_u8(data.state.bits)?;
            data.time_stamp.serialize(buffer)?;
        }
        Ok(())
    }

    fn default() -> Self where Self: Sized {
        Self::new()
    }

}

#[derive(Copy, Clone)]
pub struct StateSwitchData {
    state: BitsU8,
    time_stamp: RelativeSeconds
}

impl StateSwitchData {

    pub const fn new() -> Self {
        Self {
            state: BitsU8::new(0),
            time_stamp: RelativeSeconds::new(0),
        }
    }

    pub fn create(data: u8, timestamp: u32) -> Self {
        Self {
            state: BitsU8::new(data),
            time_stamp: RelativeSeconds::new(timestamp),
        }
    }

}

pub struct ContactsWaitData {
    relays_count: usize,
    contacts_wait_start_timestamps: [RelativeSeconds; MAX_RELAYS_COUNT],
}

impl ContactsWaitData {

    pub const fn new() -> Self {
        Self {
            relays_count: 0,
            contacts_wait_start_timestamps: [RelativeSeconds::new(0); MAX_RELAYS_COUNT],
        }
    }

    fn update_count(&mut self, new_count: usize) -> Result<(), Errors> {
        if new_count > MAX_RELAYS_COUNT {
            Err(Errors::RelayCountOverflow)
        } else {
            self.relays_count = new_count;
            Ok(())
        }
    }

    fn update_timestamp(&mut self, relay_idx: usize, timestamp: u32) -> Result<(), Errors> {
        if relay_idx > self.relays_count {
            Err(Errors::RelayIndexOutOfRange)
        } else {
            self.contacts_wait_start_timestamps[relay_idx] = RelativeSeconds::new(timestamp);
            Ok(())
        }
    }
}

impl Data for ContactsWaitData {

    fn parse_from(&mut self, data: &[u8]) -> Result<(), Errors> {
        if data.len() < 1 {
            return Err(Errors::NotEnoughDataGot);
        }
        let relays_count = data[0];
        if (data.len() as u8) < relays_count * 4 + 1 {
            return Err(Errors::NotEnoughDataGot);
        }
        self.update_count(relays_count as usize)?;
        for i in 0..relays_count as usize {
            let pos = 1 + i * 4;
            let timestamp = u32::extract(&data[pos..pos + 4]);
            self.update_timestamp(i, timestamp)?;
        }
        Ok(())
    }

    fn serialize<B: BufferWriter>(&self, buffer: &mut B) -> Result<(), Errors> {
        buffer.add_u8(self.relays_count as u8)?;
        for i in 0..self.relays_count {
            self.contacts_wait_start_timestamps[i].serialize(buffer)?;
        }
        Ok(())
    }

    fn default() -> Self where Self: Sized {
        Self::new()
    }

}

pub struct FixDataContainer {
    fix_data: [FixData; MAX_RELAYS_COUNT],
    fix_data_count: usize,
}

impl FixDataContainer {

    fn update_count(&mut self, new_count: usize) -> Result<(), Errors> {
        if new_count > MAX_RELAYS_COUNT {
            Err(Errors::RelayCountOverflow)
        } else {
            self.fix_data_count = new_count;
            Ok(())
        }
    }

    fn update_data(&mut self, relay_idx: usize, fix_try_count: u8, fix_last_try_time: u32) -> Result<(), Errors> {
        if relay_idx > self.fix_data_count {
            Err(Errors::RelayIndexOutOfRange)
        } else {
            self.fix_data[relay_idx as usize] = FixData::create(fix_try_count, fix_last_try_time);
            Ok(())
        }
    }

    pub const fn new() -> Self {
        Self {
            fix_data: [FixData::new(); MAX_RELAYS_COUNT as usize],
            fix_data_count: 0,
        }
    }

    pub fn add_fix_data(&mut self, fix_data: FixData) -> Result<(), Errors> {
        if self.fix_data_count < MAX_RELAYS_COUNT {
            self.fix_data[self.fix_data_count as usize] = fix_data;
            self.fix_data_count += 1;
            Ok(())
        } else {
            Err(Errors::RelayIndexOutOfRange)
        }
    }

    pub fn get_fix_data(&self, index: usize) -> Option<&FixData> {
        if index < self.fix_data_count {
            Some(&self.fix_data[index])
        } else {
            None
        }
    }

    pub fn get_fix_data_count(&self) -> usize {
        self.fix_data_count
    }

}

impl Data for FixDataContainer {

    fn parse_from(&mut self, data: &[u8]) -> Result<(), Errors> {
        if data.len() < 1 {
            return Err(Errors::NotEnoughDataGot);
        }
        let fix_data_count = data[0];
        if (data.len() as u8) < fix_data_count * 5 + 1 {
            return Err(Errors::NotEnoughDataGot);
        }
        self.update_count(fix_data_count as usize)?;
        for i in 0..fix_data_count as usize {
            let pos = 1 + i * 5;
            let try_count = data[pos];
            let try_time = u32::extract(&data[pos + 1..pos + 5]);
            self.update_data(i, try_count, try_time)?;
        }
        Ok(())
    }

    fn serialize<B: BufferWriter>(&self, buffer: &mut B) -> Result<(), Errors> {
        buffer.add_u8(self.fix_data_count as u8)?;
        for i in 0..self.fix_data_count {
            let fix_data = &self.fix_data[i];
            buffer.add_u8(fix_data.fix_try_count)?;
            fix_data.fix_last_try_time.serialize(buffer)?;
        }
        Ok(())
    }

    fn default() -> Self where Self: Sized {
        Self::new()
    }

}

#[derive(Copy, Clone)]
pub struct FixData {
    fix_try_count: u8,
    fix_last_try_time: RelativeSeconds
}

impl FixData {

    pub const fn new() -> Self {
        Self {
            fix_try_count: 0,
            fix_last_try_time: RelativeSeconds::new(0),
        }
    }

    pub fn create(fix_try_count: u8, fix_last_try_time: u32) -> Self {
        Self {
            fix_try_count,
            fix_last_try_time: RelativeSeconds::new(fix_last_try_time),
        }
    }

}

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

    pub fn create(min_cycle_duration: u16, max_cycle_duration: u16, avg_cycle_duration: u16, cyles_count: u64) -> Self {
        Self {
            min_cycle_duration: RelativeMillis16(min_cycle_duration),
            max_cycle_duration: RelativeMillis16(max_cycle_duration),
            avg_cycle_duration: RelativeMillis16(avg_cycle_duration),
            cycles_count: cyles_count,
        }
    }

}

impl Data for CyclesStatistics {

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

    fn serialize<B: BufferWriter>(&self, buffer: &mut B) -> Result<(), Errors> {
        buffer.add_u16(self.min_cycle_duration.0)?;
        buffer.add_u16(self.max_cycle_duration.0)?;
        buffer.add_u16(self.avg_cycle_duration.0)?;
        buffer.add_u64(self.cycles_count)?;
        Ok(())
    }

    fn default() -> Self where Self: Sized {
        Self::default()
    }

}

pub struct StateFixSettings {
    switch_try_duration: RelativeMillis16,
    switch_try_count: u8,
    wait_delay: RelativeSeconds8,
    contact_ready_wait_delay: RelativeMillis16,
}

impl Data for StateFixSettings {

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

    fn serialize<B: BufferWriter>(&self, buffer: &mut B) -> Result<(), Errors> {
        buffer.add_u16(self.switch_try_duration.0)?;
        buffer.add_u8(self.switch_try_count)?;
        buffer.add_u8(self.wait_delay.0)?;
        buffer.add_u16(self.contact_ready_wait_delay.0)
    }

    fn default() -> Self where Self: Sized {
        Self {
            switch_try_duration: RelativeMillis16(0),
            switch_try_count: 0,
            wait_delay: RelativeSeconds8(0),
            contact_ready_wait_delay: RelativeMillis16(0),
        }
    }

}

pub struct State {
    data: BitsU64,
    count: u8,
}

impl State {

    pub fn new () -> Self {
        Self { data: BitsU64::new(0), count: 0 }
    }

    pub fn create(count: u8, raw_data: u64) -> Self {
        Self { count, data: BitsU64::new(raw_data) }
    }

    fn parse_state_data_force(bytes_count: usize, data: &[u8]) -> u64 {
        let mut state_data = 0_u64;
        for i in 0..bytes_count {
            state_data |= (data[i] as u64) << (i * 8);
        }
        state_data
    }

}

impl Data for State {

    fn parse_from(&mut self, data: &[u8]) -> Result<(), Errors> {
        if data.len() < 1 {
            return Err(Errors::NotEnoughDataGot);
        }
        self.count = data[0];
        if self.count > 16 {
            return Err(Errors::RelayIndexOutOfRange);
        }
        let pairs_count = self.count as usize / 2;
        if data.len() != 1 + pairs_count {
            return Err(Errors::InvalidDataSize);
        }
        let state_data = Self::parse_state_data_force(pairs_count, &data[1..]);
        self.count;
        self.data = BitsU64::new(state_data);
        Ok(())
    }

    fn serialize<B: BufferWriter>(&self, buffer: &mut B) -> Result<(), Errors> {
        buffer.add_u64(self.data.bits)
    }

    fn default() -> Self where Self: Sized {
        Self::new()
    }

}

pub struct RelaySingleState {
    data: BitsU8,
}

impl RelaySingleState {

    pub fn new (value: u8) -> Self {
        Self { data: BitsU8::new(value) }
    }

    pub fn relay_index(&self) -> u8 {
        self.data.bits(0, 3).unwrap()
    }

    pub fn is_set(&self) -> bool {
        self.data.bits(4, 7).unwrap() > 0
    }

}

impl Data for RelaySingleState {

    fn parse_from(&mut self, data: &[u8]) -> Result<(), Errors> {
        if data.len() == 1 {
            self.data = BitsU8::new(data[0]);
            Ok(())
        } else {
            Err(Errors::InvalidDataSize)
        }
    }

    fn serialize<B: BufferWriter>(&self, buffer: &mut B) -> Result<(), Errors> {
        buffer.add_u8(self.data.bits)
    }

    fn default() -> Self where Self: Sized {
        Self::new(0)
    }

}

pub struct RelayState {
    data: BitsU8,
}

impl RelayState {

    pub fn new() -> Self {
        RelayState { data: BitsU8::new(0) }
    }

    pub fn create(relay_index: u8, on: bool, disabled: bool) -> Result<Self, Errors> {
        if relay_index > 0x0f {
            return Err(Errors::RelayIndexOutOfRange);
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

impl Data for RelayState {

    fn parse_from(&mut self, data: &[u8]) -> Result<(), Errors> {
        if data.len() == 1 {
            self.data = BitsU8::new(data[0]);
            Ok(())
        } else {
            Err(Errors::InvalidDataSize)
        }
    }

    fn serialize<B: BufferWriter>(&self, buffer: &mut B) -> Result<(), Errors> {
        buffer.add_u8(self.data.bits)
    }

    fn default() -> Self where Self: Sized {
        Self::new()
    }

}

#[derive(Copy, Clone)]
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
}

#[derive(Copy, Clone)]
pub struct RelaySettings {
    control_pin: PinData,
    monitor_pin: PinData,
    set_pin: PinData,
}

impl RelaySettings {

    pub const fn new() -> Self {
        Self {
            control_pin: PinData::new(),
            monitor_pin: PinData::new(),
            set_pin: PinData::new(),
        }
    }

    pub fn create(control_pin: u8, monitor_pin: u8, set_pin: u8) -> Self {
        Self {
            control_pin: PinData::create(control_pin),
            monitor_pin: PinData::create(monitor_pin),
            set_pin: PinData::create(set_pin),
        }
    }

    fn serialize<B: BufferWriter>(&self, buffer: &mut B) -> Result<(), Errors> {
        buffer.add_u8(self.set_pin.data)?;
        buffer.add_u8(self.monitor_pin.data)?;
        buffer.add_u8(self.control_pin.data)
    }

}

pub struct RelaysSettings {
    relays: [RelaySettings; MAX_RELAYS_COUNT],
    relays_count: usize,
}

impl RelaysSettings {

    pub fn get_relays(&self) -> &[RelaySettings] {
        &self.relays[..self.relays_count]
    }

    pub const fn new() -> Self {
        Self {
            relays: [RelaySettings::new(); MAX_RELAYS_COUNT],
            relays_count: 0,
        }
    }

    fn set_relay_count(&mut self, relays_count: usize) -> Result<(), Errors> {
        if relays_count > MAX_RELAYS_COUNT {
            return Err(Errors::RelayIndexOutOfRange);
        }
        self.relays_count = relays_count;
        Ok(())
    }

    fn set_relay_settings(&mut self, relay_index: usize, relay_settings: RelaySettings) -> Result<(), Errors> {
        if relay_index >= self.relays_count {
            return Err(Errors::RelayIndexOutOfRange);
        }
        self.relays[relay_index] = relay_settings;
        Ok(())
    }

    fn parse_items(data: &[u8], relays_count: u8, relays_settings_buffer: &mut [RelaySettings]) -> Result<(), Errors> {
        for i in 0..relays_count as usize {
            let pos = i * 3;
            let set_pin = data[pos];
            let monitor_pin = data[pos + 1];
            let control_pin = data[pos + 2];
            relays_settings_buffer[i] = RelaySettings::create(control_pin, monitor_pin, set_pin);
        }
        Ok(())
    }

}

impl Data for RelaysSettings {

    fn parse_from(&mut self, data: &[u8]) -> Result<(), Errors> {
        if data.len() < 1 {
            return Err(Errors::NotEnoughDataGot);
        }
        let relays_count = data[0];
        if relays_count > MAX_RELAYS_COUNT as u8 {
            return Err(Errors::RelayIndexOutOfRange);
        }
        if (data.len() as u8) < relays_count * 3 + 1 {
            return Err(Errors::NotEnoughDataGot);
        }
        self.set_relay_count(relays_count as usize)?;
        Self::parse_items(&data[1..], relays_count, &mut self.relays)?;
        Ok(())
    }

    fn serialize<B: BufferWriter>(&self, buffer: &mut B) -> Result<(), Errors> {
        buffer.add_u8(self.relays.len() as u8)?;
        for setting in self.relays {
            setting.serialize(buffer)?;
        }
        Ok(())
    }

    fn default() -> Self where Self: Sized {
        Self::new()
    }

}
