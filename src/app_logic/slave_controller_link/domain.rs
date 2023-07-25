

use crate::errors::Errors;
use crate::hal_ext::rtc_wrapper::{ RelativeSeconds };
use crate::hal_ext::serial_transfer::{ TxBuffer };
use crate::utils::{BitsU64, BitsU8};


pub const MAX_RELAYS_COUNT: usize = 16;
pub const SWITCHES_DATA_BUFFER_SIZE: usize = 50;


#[repr(u8)]
#[derive(Copy, Clone, PartialEq)]
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
    Unknown = 0xff,
}

#[repr(u8)]
pub enum DataInstructions {
    Settings(Option<Conversation<EmptyRequest, RelaysSettings>>) = DataInstructionCodes::Settings as u8,
    State(Option<Conversation<EmptyRequest, State>>) = DataInstructionCodes::State as u8,
    Id(Option<Conversation<EmptyRequest, u32>>) = DataInstructionCodes::Id as u8,
    InterruptPin(Option<Conversation<EmptyRequest, u8>>) = DataInstructionCodes::InterruptPin as u8,
    RemoteTimestamp(Option<Conversation<EmptyRequest, RelativeSeconds>>) = DataInstructionCodes::RemoteTimestamp as u8,
    StateFixSettings(Option<Conversation<EmptyRequest, StateFixSettings>>) = DataInstructionCodes::StateFixSettings as u8,
    RelayState(Option<Conversation<RelayIndexRequest, RelayState>>) = DataInstructionCodes::RelayState as u8,
    Version(Option<Conversation<EmptyRequest, u8>>) = DataInstructionCodes::Version as u8,
    CurrentTime(Option<Conversation<EmptyRequest, RelativeSeconds>>) = DataInstructionCodes::CurrentTime as u8,
    ContactWaitData(Option<Conversation<EmptyRequest, ContactsWaitData>>) = DataInstructionCodes::ContactWaitData as u8,
    FixData(Option<Conversation<EmptyRequest, FixDataContainer>>) = DataInstructionCodes::FixData as u8,
    SwitchData(Option<Conversation<EmptyRequest, StateSwitchDatas>>) = DataInstructionCodes::SwitchData as u8,
    CyclesStatistics(Option<Conversation<EmptyRequest, CyclesStatistics>>) = DataInstructionCodes::CyclesStatistics as u8,
    //v2 instructions
    SwitchCountingSettings(Option<Conversation<EmptyRequest, SwitchCountingSettings>>) = DataInstructionCodes::SwitchCountingSettings as u8,
    RelayDisabledTemp(Option<Conversation<EmptyRequest, RelaySingleState>>) = DataInstructionCodes::RelayDisabledTemp as u8,
    RelaySwitchedOn(Option<Conversation<EmptyRequest, RelaySingleState>>) = DataInstructionCodes::RelaySwitchedOn as u8,
    RelayMonitorOn(Option<Conversation<EmptyRequest, RelaySingleState>>) = DataInstructionCodes::RelayMonitorOn as u8,
    RelayControlOn(Option<Conversation<EmptyRequest, RelaySingleState>>) = DataInstructionCodes::RelayControlOn as u8,
    All(Option<Conversation<EmptyRequest, AllData>>) = DataInstructionCodes::All as u8,
}



impl DataInstructions {
    pub fn discriminant(&self) -> u8 {
        unsafe { *(self as *const Self as *const u8) }
    }

    pub fn parse_data(&self, data: &[u8]) -> Result<Conversation<EmptyRequest, State>, Errors> {
        self::Data::parse(data).map(|data| Conversation::Data(data))
    }

    pub fn parse_from(&mut self, data: &[u8]) -> Result<(), Errors> {
        match self {
            DataInstructions::Settings(Some(Conversation::DataCashed(ref_data))) => {
                ref_data.parse_from(data)
            }
            _ => { Err(Errors::InstructionNotSerializable) }
        }
    }

    pub fn parse(instruction_code: u8, data: &[u8]) -> Result<Self, Errors> {
        if instruction_code == DataInstructionCodes::RemoteTimestamp as u8 {
            Ok(DataInstructions::RemoteTimestamp(Some(Conversation::Data(RelativeSeconds::parse(data)?))))
        } else if instruction_code == DataInstructionCodes::CurrentTime as u8 {
            Ok(DataInstructions::CurrentTime(Some(Conversation::Data(RelativeSeconds::parse(data)?))))
        } else if instruction_code == DataInstructionCodes::Id as u8 {
            Ok(DataInstructions::Id(Some(Conversation::Data(u32::parse(data)?))))
        } else if instruction_code == DataInstructionCodes::Version as u8 {
            Ok(DataInstructions::Version(Some(Conversation::Data(u8::parse(data)?))))
        } else if instruction_code == DataInstructionCodes::StateFixSettings as u8 {
            Ok(DataInstructions::StateFixSettings(Some(Conversation::Data(StateFixSettings::parse(data)?))))
        } else if instruction_code == DataInstructionCodes::RelayState as u8 {
            Ok(DataInstructions::RelayState(Some(Conversation::Data(RelayState::parse(data)?))))
        } else if instruction_code == DataInstructionCodes::State as u8 {
            Ok(DataInstructions::State(Some(Conversation::Data(State::parse(data)?))))
        } else if instruction_code == DataInstructionCodes::CyclesStatistics as u8 {
            Ok(DataInstructions::CyclesStatistics(Some(Conversation::Data(CyclesStatistics::parse(data)?))))
        } else if instruction_code == DataInstructionCodes::FixData as u8 {
            Ok(DataInstructions::FixData(Some(Conversation::Data(FixDataContainer::parse(data)?))))
        } else if instruction_code == DataInstructionCodes::Settings as u8 {
            Ok(DataInstructions::Settings(Some(Conversation::Data(RelaysSettings::parse(data)?))))
            //v2 instructions
        } else if instruction_code == DataInstructionCodes::ContactWaitData as u8 {
            Ok(DataInstructions::ContactWaitData(Some(Conversation::Data(ContactsWaitData::parse(data)?))))
        } else if instruction_code == DataInstructionCodes::SwitchData as u8 {
            Ok(DataInstructions::SwitchData(Some(Conversation::Data(StateSwitchDatas::parse(data)?))))
        } else if instruction_code == DataInstructionCodes::InterruptPin as u8 {
            Ok(DataInstructions::InterruptPin(Some(Conversation::Data(u8::parse(data)?))))
        } else if instruction_code == DataInstructionCodes::SwitchCountingSettings as u8 {
            Ok(DataInstructions::SwitchCountingSettings(Some(Conversation::Data(SwitchCountingSettings::parse(data)?))))
        } else if instruction_code == DataInstructionCodes::RelayDisabledTemp as u8 {
            Ok(DataInstructions::RelayDisabledTemp(Some(Conversation::Data(RelaySingleState::parse(data)?))))
        } else if instruction_code == DataInstructionCodes::RelaySwitchedOn as u8 {
            Ok(DataInstructions::RelaySwitchedOn(Some(Conversation::Data(RelaySingleState::parse(data)?))))
        } else if instruction_code == DataInstructionCodes::RelayMonitorOn as u8 {
            Ok(DataInstructions::RelayMonitorOn(Some(Conversation::Data(RelaySingleState::parse(data)?))))
        } else if instruction_code == DataInstructionCodes::RelayControlOn as u8 {
            Ok(DataInstructions::RelayControlOn(Some(Conversation::Data(RelaySingleState::parse(data)?))))
        } else if instruction_code == DataInstructionCodes::All as u8 {
            Ok(DataInstructions::All(Some(Conversation::Data(AllData::parse(data)?))))
        } else {
            Err(Errors::InstructionNotRecognized(instruction_code))
        }

    }

    pub fn serialize(&self, buffer: &mut TxBuffer) -> Result<(), Errors> {
        match self {
            DataInstructions::RemoteTimestamp(Some(Conversation::Data(value))) => {
                value.serialize(buffer)
            }
            DataInstructions::CurrentTime(Some(Conversation::Data(value))) => {
                value.serialize(buffer)
            }
            DataInstructions::Id(Some(Conversation::Data(value))) => {
                value.serialize(buffer)
            }
            DataInstructions::Version(Some(Conversation::Data(value))) => {
                value.serialize(buffer)
            }
            DataInstructions::StateFixSettings(Some(Conversation::Data(value))) => {
                value.serialize(buffer)
            }
            DataInstructions::RelayState(Some(Conversation::Data(value))) => {
                value.serialize(buffer)
            }
            DataInstructions::State(Some(Conversation::Data(value))) => {
                value.serialize(buffer)
            }
            DataInstructions::CyclesStatistics(Some(Conversation::Data(value))) => {
                value.serialize(buffer)
            }
            DataInstructions::FixData(Some(Conversation::Data(value))) => {
                value.serialize(buffer)
            }
            DataInstructions::Settings(Some(Conversation::Data(value))) => {
                value.serialize(buffer)
            }
            //v2 instructions
            DataInstructions::ContactWaitData(Some(Conversation::Data(value))) => {
                value.serialize(buffer)
            }
            DataInstructions::SwitchData(Some(Conversation::Data(value))) => {
                value.serialize(buffer)
            }
            DataInstructions::InterruptPin(Some(Conversation::Data(value))) => {
                value.serialize(buffer)
            }
            DataInstructions::SwitchCountingSettings(Some(Conversation::Data(value))) => {
                value.serialize(buffer)
            }
            DataInstructions::RelayDisabledTemp(Some(Conversation::Data(value))) => {
                value.serialize(buffer)
            }
            DataInstructions::RelaySwitchedOn(Some(Conversation::Data(value))) => {
                value.serialize(buffer)
            }
            DataInstructions::RelayMonitorOn(Some(Conversation::Data(value))) => {
                value.serialize(buffer)
            }
            DataInstructions::RelayControlOn(Some(Conversation::Data(value))) => {
                value.serialize(buffer)
            }
            DataInstructions::All(Some(Conversation::Data(value))) => {
                value.serialize(buffer)
            }
            _ => {
                Err(Errors::InstructionNotSerializable)
            }
        }
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

pub struct RelativeMillis16(u16);
pub struct RelativeSeconds8(u8);
pub struct RelativeSeconds16(u16);


pub enum Conversation<RQ: Request, D: Data + 'static> {
    Request(RQ),
    Data(D),
    DataCashed(&'static mut D),
    Response(Response),
}

pub trait Request {  }

pub trait Data {
    fn parse(data: &[u8]) -> Result<Self, Errors> where Self: Sized;

    fn serialize(&self, buffer: &mut TxBuffer)->Result<(), Errors>;

    fn parse_from(&mut self, data: &[u8]) -> Result<(), Errors>;
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
        Ok(RelativeSeconds::new(get_u32(data)?))
    }

    fn parse_from(&mut self, data: &[u8]) -> Result<(), Errors> {
        *self = RelativeSeconds::new(get_u32(data)?);
        Ok(())
    }

    fn serialize(&self, buffer: &mut TxBuffer) -> Result<(), Errors> {
        let seconds: u32 = self.value();
        for i in 0..4 {
            buffer.add_byte(((seconds >> ((3 - i) * 8)) & 0xff) as u8)?;
        }
        Ok(())
    }
}

impl Data for u32 {
    fn parse(data: &[u8]) -> Result<Self, Errors> where Self: Sized {
        if data.len() == 4 {
            Ok(get_u32_force(&data[0..4]))
        } else {
            Err(Errors::InvalidDataSize)
        }
    }

    fn parse_from(&mut self, data: &[u8]) -> Result<(), Errors> {
        if data.len() != 4 {
            Err(Errors::NotEnoughDataGot)
        } else {
            *self = get_u32_force(&data[0..4]);
            Ok(())
        }
    }

    fn serialize(&self, buffer: &mut TxBuffer) -> Result<(), Errors> {
        todo!()
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
        if data.len() != 1 {
            Err(Errors::NotEnoughDataGot)
        } else {
            *self = data[0];
            Ok(())
        }
    }

    fn serialize(&self, buffer: &mut TxBuffer) -> Result<(), Errors> {
        todo!()
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
    fn parse(data: &[u8]) -> Result<Self, Errors> where Self: Sized {
        panic!("Not implemented");
    }

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

    fn serialize(&self, buffer: &mut TxBuffer) -> Result<(), Errors> {
        todo!()
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

    fn parse(data: &[u8]) -> Result<Self, Errors> {
        if data.len() < 3 {
            return Err(Errors::NotEnoughDataGot)
        } else {
            let switch_limit_interval = data[0] as u16 + ((data[1] as u16) << 8);
            let max_switch_count = data[2];
            Ok(Self::create(switch_limit_interval, max_switch_count))
        }
    }
}

impl Data for SwitchCountingSettings {
    fn parse(data: &[u8]) -> Result<Self, Errors> {
        if data.len() != 3 {
            return Err(Errors::DataCorrupted)
        } else {
            let switch_limit_interval = data[0] as u16 + ((data[1] as u16) << 8);
            let max_switch_count = data[2];
            Ok(Self::create(switch_limit_interval, max_switch_count))
        }
    }

    fn parse_from(&mut self, data: &[u8]) -> Result<(), Errors> {
        if data.len() != 3 {
            return Err(Errors::DataCorrupted)
        } else {
            let switch_limit_interval = data[0] as u16 + ((data[1] as u16) << 8);
            let max_switch_count = data[2];
            Ok(Self::create(switch_limit_interval, max_switch_count))
        }
    }

    fn serialize(&self, buffer: &mut TxBuffer) -> Result<(), Errors> {
        todo!()
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
    fn parse(data: &[u8]) -> Result<Self, Errors> where Self: Sized {
        panic!("Not implemented");
    }

    fn serialize(&self, buffer: &mut TxBuffer) -> Result<(), Errors> {
        todo!()
    }

    fn parse_from(&mut self, data: &[u8]) -> Result<(), Errors> {
        if data.len() < 1 {
            return Err(Errors::NotEnoughDataGot);
        }
        let relays_count = data[0];
        if (data.len() as u8) != relays_count * 4 + 1 {
            return Err(Errors::DataCorrupted);
        }
        self.set_count(relays_count as usize)?;
        for i in 0..relays_count as usize {
            let pos = 1 + i * 5;
            let switch_count_data = data[pos];
            let timestamp = get_u32_force(&data[pos + 1..pos + 5]);
            self.set_data(i, StateSwitchData::create(switch_count_data, timestamp))?;
        }
        Ok(())
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
    fn parse(data: &[u8]) -> Result<Self, Errors> where Self: Sized {
        panic!("Not implemented");
    }

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
            let timestamp = get_u32_force(&data[pos..pos + 4]);
            self.update_timestamp(i, timestamp)?;
        }
        Ok(())
    }

    fn serialize(&self, buffer: &mut TxBuffer) -> Result<(), Errors> {
        todo!()
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
    fn parse(data: &[u8]) -> Result<Self, Errors> where Self: Sized {
        panic!("Not implemented");
    }

    fn parse_from(&mut self, data: &[u8]) -> Result<(), Errors> {
        if data.len() < 1 {
            return Err(Errors::NotEnoughDataGot);
        }
        let relays_count = data[0];
        if (data.len() as u8) < relays_count * 5 + 1 {
            return Err(Errors::NotEnoughDataGot);
        }
        self.update_count(relays_count as usize)?;
        for i in 0..relays_count as usize {
            let pos = 1 + i * 5;
            let try_count = data[pos];
            let try_time = (data[pos + 1] as u32) << 8 | data[pos + 2] as u32;
            self.update_data(i, try_count, try_time)?;
        }
        Ok(())
    }

    fn serialize(&self, buffer: &mut TxBuffer) -> Result<(), Errors> {
        todo!()
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

    #[inline(always)]
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
    cycles_count: u32,
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
    pub fn create(min_cycle_duration: u16, max_cycle_duration: u16, avg_cycle_duration: u16, cyles_count: u32) -> Self {
        Self {
            min_cycle_duration: RelativeMillis16(min_cycle_duration),
            max_cycle_duration: RelativeMillis16(max_cycle_duration),
            avg_cycle_duration: RelativeMillis16(avg_cycle_duration),
            cycles_count: cyles_count,
        }
    }
}

impl Data for CyclesStatistics {
    fn parse(data: &[u8]) -> Result<Self, Errors> {
        if data.len() < 10 {
            Err(Errors::NotEnoughDataGot)
        } else {
            let min_cycle_duration = (data[0] as u16) << 8 | data[1] as u16;
            let max_cycle_duration = (data[2] as u16) << 8 | data[3] as u16;
            let avg_cycle_duration = (data[4] as u16) << 8 | data[5] as u16;
            let mut cycles_count = get_u32_force(data);
            if data.len() >= 14 {
                if data[10] > 0 || data[11] > 0 || data[12] > 0 || data[13] > 0 {
                    cycles_count |= 0x80000000;
                }
            }
            Ok(CyclesStatistics::create(min_cycle_duration, max_cycle_duration, avg_cycle_duration, cycles_count))
        }
    }

    fn parse_from(&mut self, data: &[u8]) -> Result<(), Errors> {
        todo!()
    }

    fn serialize(&self, buffer: &mut TxBuffer) -> Result<(), Errors> {
        todo!()
    }
}

pub struct StateFixSettings {
    switch_try_duration: RelativeMillis16,
    switch_try_count: u8,
    wait_delay: RelativeSeconds8,
    contact_ready_wait_delay: RelativeMillis16,
}

impl Data for StateFixSettings {
    fn parse(data: &[u8]) -> Result<Self, Errors> {
        if data.len() < 6 {
            Err(Errors::NotEnoughDataGot)
        } else {
            Ok(Self {
                switch_try_duration: RelativeMillis16(((data[0] as u16) << 8) | data[1] as u16),
                switch_try_count: data[2],
                wait_delay: RelativeSeconds8(data[3]),
                contact_ready_wait_delay: RelativeMillis16(((data[4] as u16) << 8) | data[5] as u16),
            })
        }
    }

    fn parse_from(&mut self, data: &[u8]) -> Result<(), Errors> {
        todo!()
    }

    fn serialize(&self, buffer: &mut TxBuffer) -> Result<(), Errors> {
        todo!()
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
    fn parse(data: &[u8]) -> Result<Self, Errors> {
        if data.len() < 1 {
            return Err(Errors::NotEnoughDataGot);
        }
        let count = data[0];
        if count > 16 {
            return Err(Errors::RelayIndexOutOfRange);
        }
        let pairs_count = count as usize / 2;
        if data.len() != 1 + pairs_count {
            return Err(Errors::InvalidDataSize);
        }
        let state_data = Self::parse_state_data_force(pairs_count, &data[1..]);
        Ok(Self{ count, data: BitsU64::new(state_data) })
    }

    fn parse_from(&mut self, data: &[u8]) -> Result<(), Errors> {
        todo!()
    }

    fn serialize(&self, buffer: &mut TxBuffer) -> Result<(), Errors> {
        todo!()
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
    fn parse(data: &[u8]) -> Result<Self, Errors> {
        if data.len() == 1 {
            Ok(Self::new(data[0]))
        } else {
            Err(Errors::InvalidDataSize)
        }
    }

    fn parse_from(&mut self, data: &[u8]) -> Result<(), Errors> {
        todo!()
    }

    fn serialize(&self, buffer: &mut TxBuffer) -> Result<(), Errors> {
        todo!()
    }
}

pub struct RelayState {
    data: BitsU8,
}

impl RelayState {

    pub fn new() -> Self {
        RelayState { data: BitsU8::new(0) }
    }

    pub fn from_u8(raw_data: u8) -> Self {
        RelayState { data: BitsU8::new(raw_data) }
    }

    pub fn parse(data: &[u8]) -> Result<Self, Errors> {
        if data.len() < 1 {
            return Err(Errors::NotEnoughDataGot);
        } else {
            Ok(Self::from_u8(data[0]))
        }
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
    fn parse(data: &[u8]) -> Result<Self, Errors> {
        if data.len() == 1 {
            Ok(Self::from_u8(data[0]))
        } else {
            Err(Errors::InvalidDataSize)
        }
    }

    fn parse_from(&mut self, data: &[u8]) -> Result<(), Errors> {
        todo!()
    }

    fn serialize(&self, buffer: &mut TxBuffer) -> Result<(), Errors> {
        todo!()
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

    #[inline(always)]
    fn set_relay_count(&mut self, relays_count: usize) -> Result<(), Errors> {
        if relays_count > MAX_RELAYS_COUNT {
            return Err(Errors::RelayIndexOutOfRange);
        }
        self.relays_count = relays_count;
        Ok(())
    }

    #[inline(always)]
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
    fn parse(data: &[u8]) -> Result<Self, Errors> {
        panic!("Not implemented");
    }

    fn serialize(&self, buffer: &mut TxBuffer) -> Result<(), Errors> {
        buffer.add_byte(self.relays.len() as u8)?;
        for setting in self.relays {
            buffer.add_byte(setting.set_pin.data)?;
            buffer.add_byte(setting.monitor_pin.data)?;
            buffer.add_byte(setting.control_pin.data)?;
        }
        Ok(())
    }

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
}

fn get_u32(data: &[u8]) -> Result<u32, Errors> {
    if data.len() < 4 {
        Err(Errors::NotEnoughDataGot)
    } else {
        Ok(get_u32_force(&data[0..4]))
    }
}

fn get_u32_force(data: &[u8]) -> u32 {
    (data[0] as u32) << 24 | (data[1] as u32) << 16 | (data[2] as u32) << 8 | data[3] as u32
}