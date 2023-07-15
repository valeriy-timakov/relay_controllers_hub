use cortex_m_semihosting::hprintln;
use stm32f4xx_hal::dma::{ChannelX, MemoryToPeripheral, PeripheralToMemory};
use stm32f4xx_hal::serial::{Rx, Tx, Instance, RxISR, TxISR, RxListen};
use stm32f4xx_hal::dma::traits::{Channel, DMASet, PeriAddress, Stream};
use crate::errors::Errors;
use crate::hal_ext::rtc_wrapper::{RelativeMillis, RelativeSeconds };
use crate::hal_ext::serial_transfer::{RxTransfer, SerialTransfer, TxBuffer, TxTransfer};
use crate::utils::{BitsU64, BitsU8};

#[repr(u8)]
#[derive(Copy, Clone, PartialEq)]
enum Operation {
    None = 0x00,
    Read = 0x01,
    Set = 0x02,
    Success = 0x03,
    Error = 0x04,
    Signal = 0x05,
    Response = 0x06,
    Unknown = 0x0f
}

#[repr(u8)]
#[derive(Copy, Clone, PartialEq)]
pub enum Instruction {
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
    GetTimeStamp = 0x14,
    RelayStateChanged = 0x15,
    MonitoringStateChanged = 0x16,
    ControlStateChanged = 0x17,
    CyclesStatistics = 0x18,
    StateFixTry = 0x19,
    Unknown = 0xff,
    //v2 instructions
    SwitchCountingSettings = 0x07,
    ClearSwitchCount = 0x08,
    RelayDisabledTemp = 0x0a,
    RelaySwitchedOn = 0x0b,
    RelayMonitorOn = 0x0c,
    RelayControlOn = 0x0d,
    All = 0x0e,
}

pub enum Response {
    None,
    Settings(&'static RelaysSettings),
    State(State),
    Id(u32),
    InterruptPin(u8),
    RemoteTimestamp(RelativeSeconds),
    StateFixSettings(StateFixSettings),
    RelayState(RelayState),
    Version(u8),
    CurrentTime(RelativeSeconds),
    ContactWaitData(&'static ContactsWaitData),
    FixData(&'static FixDataContainer),
    SwitchData(&'static StateSwitchDatas),
    CyclesStatistics(CyclesStatiscics),
    SwitchCountingSettings(SwitchCountingSettings),
    RelayDisabledTemp(RelaySingleState),
    RelaySwitchedOn(RelaySingleState),
    RelayMonitorOn(RelaySingleState),
    RelayControlOn(RelaySingleState),
    All(AllData),
    NotParsed
}
impl Response {
    fn discriminant(&self) -> u8 {
        unsafe { *(self as *const Self as *const u8) }
    }
    pub fn is_same(&self, other: &Response) -> bool {
        self.discriminant() == other.discriminant()
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
    fn discriminant(&self) -> u8 {
        unsafe { *(self as *const Self as *const u8) }
    }
    fn for_code(code: u8) -> ErrorCode {
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

const MAX_RELAYS_COUNT: usize = 16;
const SWITCHES_DATA_BUFFER_SIZE: usize = 50;

pub struct StaticBuffers {
    pub fix_data_buffer: FixDataContainer,
    pub relays_settings_buffer: RelaysSettings,
    pub contacts_wait_data_buffer: ContactsWaitData,
    pub state_switch_data_buffer: StateSwitchDatas,
}

pub struct AllData {
    pub id: u32,
    pub interrupt_pin: u8,
    pub relays_count: u8,
    pub relays_settings: &'static [RelaySettings; MAX_RELAYS_COUNT],
    pub state_data: BitsU64,
}

impl AllData {
    pub fn create(id: u32, interrupt_pin: u8, relays_count: u8, state_data: BitsU64,
                   relays_settings: &'static [RelaySettings; MAX_RELAYS_COUNT]) -> Self {
        Self {
            id,
            interrupt_pin,
            relays_count,
            relays_settings,
            state_data,
        }
    }
}

impl StaticBuffers {
    pub const fn new() -> Self {
        Self {
            fix_data_buffer: FixDataContainer::new(),
            relays_settings_buffer: RelaysSettings::new(),
            contacts_wait_data_buffer: ContactsWaitData::new(),
            state_switch_data_buffer: StateSwitchDatas::new(),
        }
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

    const fn new() -> Self {
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
}

impl FixDataContainer {
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

pub struct CyclesStatiscics {
    min_cycle_duration: RelativeMillis16,
    max_cycle_duration: RelativeMillis16,
    avg_cycle_duration: RelativeMillis16,
    cycles_count: u32,
}

impl CyclesStatiscics {
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

pub struct StateFixSettings {
    switch_try_duration: RelativeMillis16,
    switch_try_count: u8,
    wait_delay: RelativeSeconds8,
    contact_ready_wait_delay: RelativeMillis16,
}

pub struct State {
    data: BitsU64,
    count: u8,
}

impl State {

    pub fn new () -> Self {
        Self { data: BitsU64::new(0), count: 0 }
    }

    pub fn from_u64(count: u8, raw_data: u64) -> Self {
        Self { count, data: BitsU64::new(raw_data) }
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

}

pub struct SignalData {
    instruction: Instruction,
    relative_timestamp: RelativeSeconds,
    relay_idx: u8,
    is_on: bool,
    is_called_internally: Option<bool>,
}

const MAX_INSTANCES_COUNT: usize = 3;
static mut STATIC_BUFFERS: [StaticBuffers; MAX_INSTANCES_COUNT] = [ StaticBuffers::new(), StaticBuffers::new(), StaticBuffers::new() ];
static mut INSTANCES_COUNT: usize = 0;

pub trait SignalsReceiver {
    fn on_signal(&mut self, signal_data: SignalData);
    fn on_signal_error(&mut self, instruction: Option<Instruction>, error_code: ErrorCode);
    fn on_request_success(&mut self, request: &Request);
    fn on_request_error(&mut self, request: &Request, error_code: ErrorCode);
    fn on_request_parse_error(&mut self, request: &Request, data: &[u8]);
    fn on_request_response(&mut self, request: &Request, response: Response);
}

pub struct SlaveControllerLink<U, TxStream, const TX_CHANNEL: u8, RxStream, const RX_CHANNEL: u8, SR>
    where
        U: Instance,
        Tx<U>: PeriAddress<MemSize=u8> + DMASet<TxStream, TX_CHANNEL, MemoryToPeripheral> + TxISR,
        Rx<U, u8>: PeriAddress<MemSize=u8> + DMASet<RxStream, RX_CHANNEL, PeripheralToMemory> + RxISR + RxListen,
        TxStream: Stream,
        ChannelX<TX_CHANNEL>: Channel,
        RxStream: Stream,
        ChannelX<RX_CHANNEL>: Channel,
        SR: SignalsReceiver,
{
    tx: TransmitterToSlaveController<U, TxStream, TX_CHANNEL>,
    rx: ReceiverFromSlaveController<U, RxStream, RX_CHANNEL, SR>,
}

impl <U, TxStream, const TX_CHANNEL: u8, RxStream, const RX_CHANNEL: u8, SR> SlaveControllerLink<U, TxStream, TX_CHANNEL, RxStream, RX_CHANNEL, SR>
    where
        U: Instance,
        Tx<U>: PeriAddress<MemSize=u8> + DMASet<TxStream, TX_CHANNEL, MemoryToPeripheral> + TxISR,
        Rx<U, u8>: PeriAddress<MemSize=u8> + DMASet<RxStream, RX_CHANNEL, PeripheralToMemory> + RxISR + RxListen,
        TxStream: Stream,
        ChannelX<TX_CHANNEL>: Channel,
        RxStream: Stream,
        ChannelX<RX_CHANNEL>: Channel,
        SR: SignalsReceiver,
{
    pub fn create (serial_transfer: SerialTransfer<U, TxStream, TX_CHANNEL, RxStream, RX_CHANNEL>, signal_receiver: SR) -> Result<Self, Errors> {
        let (tx, rx) = serial_transfer.into();
        Ok(Self {
            tx: TransmitterToSlaveController::new(tx),
            rx: ReceiverFromSlaveController::create(rx, signal_receiver)?,
        })
    }

    pub fn on_get_command<TS:  FnOnce() -> RelativeMillis>( &mut self, time_src: TS) -> Result<(), Errors> {
        self.rx.on_get_command(&mut self.tx, time_src)
    }

    pub fn on_rx_dma_interrupts(&mut self) {
        self.rx.on_dma_interrupts();
    }

    pub fn on_tx_dma_interrupts(&mut self) {
        self.tx.on_dma_interrupts();
    }
}

pub struct Request {
    operation: Operation,
    instruction: Instruction,
    rel_timestamp: RelativeMillis
}

impl Request {
    fn new(operation: Operation, instruction: Instruction, rel_timestamp: RelativeMillis) -> Self {
        Self {
            operation,
            instruction,
            rel_timestamp
        }
    }
}

const REQUESTS_COUNT: usize = 4;

struct TransmitterToSlaveController<U, TxStream, const TX_CHANNEL: u8>
    where
        U: Instance,
        Tx<U>: PeriAddress<MemSize=u8> + DMASet<TxStream, TX_CHANNEL, MemoryToPeripheral> + TxISR,
        TxStream: Stream,
        ChannelX<TX_CHANNEL>: Channel,
{
    tx: TxTransfer<U, TxStream, TX_CHANNEL>,
    sent_requests: [Option<Request>; REQUESTS_COUNT],
    requests_count: usize,
}

impl <U, TxStream, const TX_CHANNEL: u8> TransmitterToSlaveController<U, TxStream, TX_CHANNEL>
    where
        U: Instance,
        Tx<U>: PeriAddress<MemSize=u8> + DMASet<TxStream, TX_CHANNEL, MemoryToPeripheral> + TxISR,
        TxStream: Stream,
        ChannelX<TX_CHANNEL>: Channel,
{
    pub fn new (tx: TxTransfer<U, TxStream, TX_CHANNEL>) -> Self {
        Self {
            tx,
            sent_requests: [None, None, None, None],
            requests_count: 0,
        }
    }

    pub fn send_settings(&mut self, settings: &[RelaySettings], timestamp: RelativeMillis) -> Result<(), Errors> {
        self.send_set_request(Operation::Set, Instruction::Settings, timestamp, |buffer| {
            buffer.add_byte(settings.len() as u8)?;
            for setting in settings {
                buffer.add_byte(setting.set_pin.data)?;
                buffer.add_byte(setting.monitor_pin.data)?;
                buffer.add_byte(setting.control_pin.data)?;
            }
            Ok(())
        })
    }

    pub fn send_relative_timestamp(&mut self, timestamp: RelativeMillis) -> Result<(), Errors> {
        let seconds: u32 = timestamp.seconds().value();
        self.send_set_request(Operation::Set, Instruction::RemoteTimestamp, timestamp, |buffer| {
            for i in 0..4 {
                buffer.add_byte(((seconds >> ((3 - i) * 8)) & 0xff) as u8)?;
            }
            Ok(())
        })
    }

    fn send_set_request<F>(&mut self, operation: Operation, instruction: Instruction, timestamp: RelativeMillis, writter: F) -> Result<(), Errors>
            where F: FnOnce(&mut TxBuffer)->Result<(), Errors> {
        if self.requests_count == REQUESTS_COUNT {
            return Err(Errors::RequestslLimitReached);
        }
        let result = self.tx.start_transfer(|buffer| {
            buffer.clear();
            buffer.add_byte(Operation::None as u8)?;
            buffer.add_byte(operation as u8)?;
            buffer.add_byte(instruction as u8)?;
            writter(buffer)
        });
        self.sent_requests[self.requests_count] = Some(Request::new(operation, instruction, timestamp));
        self.requests_count += 1;
        result
    }

    pub fn send_error(&mut self, instruction: Instruction, error_code: ErrorCode) -> Result<(), Errors> {
        self.tx.start_transfer(|buffer| {
            buffer.add_byte(Operation::None as u8)?;
            buffer.add_byte(Operation::Error as u8)?;
            buffer.add_byte(instruction as u8)?;
            buffer.add_byte(error_code.discriminant())
        })
    }

    #[inline(always)]
    pub fn on_dma_interrupts(&mut self) {
        self.tx.on_dma_interrupts();
    }
}

struct ReceiverFromSlaveController<U, RxStream, const RX_CHANNEL: u8, SR>
    where
        U: Instance,
        Rx<U, u8>: PeriAddress<MemSize=u8> + DMASet<RxStream, RX_CHANNEL, PeripheralToMemory> + RxISR + RxListen,
        RxStream: Stream,
        ChannelX<RX_CHANNEL>: Channel,
        SR: SignalsReceiver,
{
    rx: RxTransfer<U, RxStream, RX_CHANNEL>,
    signal_receiver: SR,
    static_buffers_idx: usize
}

impl <U, RxStream, const RX_CHANNEL: u8, SR> ReceiverFromSlaveController<U, RxStream, RX_CHANNEL, SR>
    where
        U: Instance,
        Rx<U, u8>: PeriAddress<MemSize=u8> + DMASet<RxStream, RX_CHANNEL, PeripheralToMemory> + RxISR + RxListen,
        RxStream: Stream,
        ChannelX<RX_CHANNEL>: Channel,
        SR: SignalsReceiver,
{
    pub fn create (rx: RxTransfer<U, RxStream, RX_CHANNEL>, signal_receiver: SR) -> Result<Self, Errors> {
        let static_buffers_idx = unsafe {
            if INSTANCES_COUNT >= MAX_INSTANCES_COUNT {
                return Err(Errors::SlaveControllersInstancesMaxCountReached);
            }
            let instances_count = INSTANCES_COUNT;
            INSTANCES_COUNT += 1;
            instances_count as usize
        };
        Ok(Self { rx, signal_receiver: signal_receiver, static_buffers_idx })
    }

    pub fn on_get_command<TS, TxStream, const TX_CHANNEL: u8>(
            &mut self,
            tx:  &mut TransmitterToSlaveController<U, TxStream, TX_CHANNEL>,
            time_src: TS) -> Result<(), Errors>
        where
            Tx<U>: PeriAddress<MemSize=u8> + DMASet<TxStream, TX_CHANNEL, MemoryToPeripheral> + TxISR,
            TxStream: Stream,
            ChannelX<TX_CHANNEL>: Channel,
            TS: FnOnce() -> RelativeMillis,
    {
        let ReceiverFromSlaveController { rx, signal_receiver, static_buffers_idx } = self;
        rx.on_rx_transfer_interrupt(|data| {
            hprintln!("rx6 got");
            if data.len() > 3 && data[0] == Operation::None as u8 {
                let operation_code = data[1];
                let instruction_code = data[2];
                if operation_code == Operation::Signal as u8 {
                    if instruction_code == Instruction::GetTimeStamp as u8 {
                        tx.send_relative_timestamp(time_src())
                    } else {
                        let instruction = if instruction_code == Instruction::MonitoringStateChanged as u8 {
                            Some(Instruction::MonitoringStateChanged)
                        } else  if instruction_code == Instruction::StateFixTry as u8 {
                            Some(Instruction::StateFixTry)
                        } else  if instruction_code == Instruction::ControlStateChanged as u8 {
                            Some(Instruction::ControlStateChanged)
                        } else  if instruction_code == Instruction::RelayStateChanged as u8 {
                            Some(Instruction::RelayStateChanged)
                        } else {
                            None
                        };
                        match instruction {
                            Some(instruction) => {
                                match read_signal_data(instruction, &data[3..]) {
                                    Ok(signal_data) => {
                                        signal_receiver.on_signal(signal_data);
                                        Ok(())
                                    }
                                    Err(error) => {
                                        signal_receiver.on_signal_error(Some(instruction), error);
                                        tx.send_error(instruction, error).unwrap();
                                        Err(Errors::DataCorrupted)
                                    }
                                }
                            }
                            None => {
                                signal_receiver.on_signal_error(None, ErrorCode::EInstructionUnrecognized);
                                tx.send_error(Instruction::Unknown, ErrorCode::ERequestDataNoValue).unwrap();
                                Err(Errors::InstructionNotRecognized(instruction_code))
                            }
                        }
                    }
                } else if operation_code == Operation::Success as u8 || operation_code == Operation::Response as u8 || operation_code == Operation::Error as u8 {
                    if tx.requests_count > 0 {
                        let search_operation = if operation_code == Operation::Success as u8 {
                            Operation::Set
                        } else if operation_code == Operation::Response as u8 {
                            Operation::Read
                        } else {
                            Operation::Error
                        };
                        for i in (0..tx.requests_count).rev() {
                            if let Some(request) = tx.sent_requests[i].as_ref() {
                                if request.instruction as u8 == instruction_code && request.operation == search_operation {
                                    if operation_code == Operation::Success as u8 {
                                        signal_receiver.on_request_success(request);
                                    } else if operation_code == Operation::Error as u8 {
                                        signal_receiver.on_request_error(request, ErrorCode::for_code(instruction_code));
                                    } else {
                                        let response = parse_response(instruction_code, &data[3..], *static_buffers_idx);
                                        if (Response::NotParsed.is_same(&response)) {
                                            signal_receiver.on_request_parse_error(request, &data[3..]);
                                        } else {
                                            signal_receiver.on_request_response(request, response);
                                        }
                                    }
                                    let mut next_pos = i + 1;
                                    while next_pos < tx.requests_count {
                                        tx.sent_requests.swap(next_pos - 1, next_pos);
                                        next_pos += 1;
                                    }
                                    tx.sent_requests[next_pos - 1] = None;
                                    tx.requests_count -= 1;
                                    return Ok(());
                                }
                            }
                        }
                    }
                    Err(Errors::NoRequestsFound)
                } else if operation_code == Operation::Response as u8 {
                    Ok(())
                } else {
                    Err(Errors::OperationNotRecognized(operation_code))
                }
            } else {
                Err(Errors::NotEnoughDataGot)
            }
        })
    }

    #[inline(always)]
    pub fn on_dma_interrupts(&mut self) {
        self.rx.on_dma_interrupts();
    }
}




fn parse_response(instruction_code: u8, data: &[u8], static_buffers_idx: usize) -> Response {
    if instruction_code == Instruction::RemoteTimestamp as u8 {
        parse_remote_timestamp(data)
    } else if instruction_code == Instruction::CurrentTime as u8 {
        parse_current_time(data)
    } else if instruction_code == Instruction::Id as u8 {
        parse_id(data)
    } else if instruction_code == Instruction::Version as u8 {
        Response::Version(data[0])
    } else if instruction_code == Instruction::StateFixSettings as u8 {
        parse_state_fix_settings(data)
    } else if instruction_code == Instruction::RelayState as u8 {
        parse_relay_state(data)
    } else if instruction_code == Instruction::State as u8 {
        parse_state(data)
    } else if instruction_code == Instruction::CyclesStatistics as u8 {
        parse_cycles_statistics(data)
    } else if instruction_code == Instruction::FixData as u8 {
        parse_fix_data(data, static_buffers_idx)
    } else if instruction_code == Instruction::Settings as u8 {
        parse_relay_settings(data, static_buffers_idx)
    //v2 instructions
    } else if instruction_code == Instruction::ContactWaitData as u8 {
        parse_contact_wait_data(data, static_buffers_idx)
    } else if instruction_code == Instruction::SwitchData as u8 {
        parse_switch_data(data, static_buffers_idx)
    } else if instruction_code == Instruction::InterruptPin as u8 {
        parse_interrupt_pin(data)
    } else if instruction_code == Instruction::SwitchCountingSettings as u8 {
        parse_switch_counting_settings(data)
    } else if instruction_code == Instruction::RelayDisabledTemp as u8 {
        match parse_relay_single_state(data) {
            Some(state) => Response::RelayDisabledTemp(state),
            None => Response::NotParsed
        }
    } else if instruction_code == Instruction::RelaySwitchedOn as u8 {
        match parse_relay_single_state(data) {
            Some(state) => Response::RelaySwitchedOn(state),
            None => Response::NotParsed
        }
    } else if instruction_code == Instruction::RelayMonitorOn as u8 {
        match parse_relay_single_state(data) {
            Some(state) => Response::RelayMonitorOn(state),
            None => Response::NotParsed
        }
    } else if instruction_code == Instruction::RelayControlOn as u8 {
        match parse_relay_single_state(data) {
            Some(state) => Response::RelayControlOn(state),
            None => Response::NotParsed
        }
    } else if instruction_code == Instruction::All as u8 {
        parse_all(data, static_buffers_idx)
    } else {
        Response::NotParsed
    }
}

fn parse_all(data: &[u8], static_buffers_idx: usize) -> Response {
    if data.len() < 6 {
        return Response::NotParsed;
    }
    let id = get_u32_force(&data[0..4]);
    let interrupt_pin = data[4];
    let relays_count = data[5];

    let pairs_count = relays_count as usize / 2;
    let data = &data[6..];
    if data.len() < pairs_count {
        return Response::NotParsed;
    }
    let state_data = parse_state_data_force(pairs_count, data);

    match parse_settings(&data[pairs_count..], relays_count, static_buffers_idx) {
        Ok(relays_settings) => Response::All(AllData::create(id, interrupt_pin, relays_count,
         BitsU64::new(state_data), relays_settings)),
        Err(_) => Response::NotParsed
    }
}

fn parse_settings(data: &[u8], relays_count: u8, static_buffers_idx: usize) -> Result<&'static [RelaySettings; MAX_RELAYS_COUNT], Errors> {
    if data.len() < relays_count as usize * 3 {
        return Err(Errors::NotEnoughDataGot);
    }
    let relays_settings_buffer: &'static mut RelaysSettings =
        unsafe{ &mut STATIC_BUFFERS[static_buffers_idx].relays_settings_buffer };
    relays_settings_buffer.set_relay_count(relays_count as usize)?;
    let relays_settings = parse_relay_settings_items(&data[1..], relays_count, relays_settings_buffer)?;
    Ok(&relays_settings.relays)
}

fn parse_switch_counting_settings(data: &[u8]) -> Response {
    if data.len() < 3 {
        return Response::NotParsed;
    }
    let switch_limit_interval = data[0] as u16 + ((data[1] as u16) << 8);
    let max_switch_count = data[2];
    Response::SwitchCountingSettings(SwitchCountingSettings::create(switch_limit_interval, max_switch_count))
}

fn parse_interrupt_pin(data: &[u8]) -> Response {
    if data.len() < 1 {
        return Response::NotParsed;
    }
    Response::InterruptPin(data[0])
}

fn parse_switch_data(data: &[u8], static_buffers_idx: usize) -> Response {
    if data.len() < 1 {
        return Response::NotParsed;
    }
    let state_switch_data_buffer: &'static mut StateSwitchDatas = unsafe{ &mut STATIC_BUFFERS[static_buffers_idx].state_switch_data_buffer };
    match parse_switch_data_struct(data, state_switch_data_buffer) {
        Ok(state_switch_data) => Response::SwitchData(state_switch_data),
        Err(_) => Response::NotParsed
    }
}

fn parse_switch_data_struct<'a>(data: &[u8], state_switch_data: &'a mut StateSwitchDatas) -> Result<&'a StateSwitchDatas, Errors> {
    let relays_count = data[0];
    if (data.len() as u8) < relays_count * 4 + 1 {
        return Err(Errors::NotEnoughDataGot);
    }
    state_switch_data.set_count(relays_count as usize)?;
    for i in 0..relays_count as usize {
        let pos = 1 + i * 5;
        let switch_count_data = data[pos];
        let timestamp = get_u32_force(&data[pos + 1..pos + 5]);
        state_switch_data.set_data(i, StateSwitchData::create(switch_count_data, timestamp))?;
    }
    Ok(state_switch_data)
}


fn parse_contact_wait_data(data: &[u8], static_buffers_idx: usize) -> Response {
    if data.len() < 1 {
        return Response::NotParsed;
    }
    let contacts_wait_data_buffer: &'static mut ContactsWaitData = unsafe{ &mut STATIC_BUFFERS[static_buffers_idx].contacts_wait_data_buffer };
    match parse_contact_wait_data_struct(data, contacts_wait_data_buffer) {
        Ok(contacts_wait_data) => Response::ContactWaitData(contacts_wait_data),
        Err(_) => Response::NotParsed
    }
}

fn parse_contact_wait_data_struct<'a>(data: &[u8], contacts_wait_data: &'a mut ContactsWaitData) -> Result<&'a ContactsWaitData, Errors> {
    let relays_count = data[0];
    if (data.len() as u8) < relays_count * 4 + 1 {
        return Err(Errors::NotEnoughDataGot);
    }
    contacts_wait_data.update_count(relays_count as usize)?;
    for i in 0..relays_count as usize {
        let pos = 1 + i * 4;
        let timestamp = get_u32_force(&data[pos..pos + 4]);
        contacts_wait_data.update_timestamp(i, timestamp)?;
    }
    Ok(contacts_wait_data)
}

fn parse_relay_settings(data: &[u8], static_buffers_idx: usize) -> Response {
    if data.len() < 1 {
        return Response::NotParsed;
    }
    let relays_settings_buffer: &'static mut RelaysSettings = unsafe{ &mut STATIC_BUFFERS[static_buffers_idx].relays_settings_buffer };
    match parse_relay_settings_struct(data, relays_settings_buffer) {
        Ok(relays_settings_buffer) => Response::Settings(relays_settings_buffer),
        Err(_) => Response::NotParsed
    }
}

fn parse_relay_settings_struct<'a>(data: &[u8], relays_settings_buffer: &'a mut RelaysSettings) -> Result<&'a RelaysSettings, Errors> {
    let relays_count = data[0];
    if (data.len() as u8) < relays_count * 3 + 1 {
        return Err(Errors::NotEnoughDataGot);
    }
    relays_settings_buffer.set_relay_count(relays_count as usize)?;
    Ok(parse_relay_settings_items(&data[1..], relays_count, relays_settings_buffer)?)
}

fn parse_relay_settings_items<'a>(data: &[u8], relays_count: u8, relays_settings_buffer: &'a mut RelaysSettings) -> Result<&'a RelaysSettings, Errors> {
    for i in 0..relays_count as usize {
        let pos = i * 3;
        let set_pin = data[pos];
        let monitor_pin = data[pos + 1];
        let control_pin = data[pos + 2];
        relays_settings_buffer.set_relay_settings(i, RelaySettings::create(control_pin, monitor_pin, set_pin))?;
    }
    Ok(relays_settings_buffer)
}

fn parse_relay_single_state(data: &[u8]) -> Option<RelaySingleState> {
    if data.len() < 1 {
        return None
    } else {
        Some(RelaySingleState::new(data[0]))
    }
}

fn parse_cycles_statistics(data: &[u8]) -> Response {
    if data.len() < 10 {
        return Response::NotParsed;
    }
    let min_cycle_duration = (data[0] as u16) << 8 | data[1] as u16;
    let max_cycle_duration = (data[2] as u16) << 8 | data[3] as u16;
    let avg_cycle_duration = (data[4] as u16) << 8 | data[5] as u16;
    let mut cycles_count = get_u32_force(data);
    if data.len() >= 14 {
        if data[10] > 0 || data[11] > 0 || data[12] > 0 || data[13] > 0 {
            cycles_count |= 0x80000000;
        }
    }
    Response::CyclesStatistics( CyclesStatiscics::create(min_cycle_duration, max_cycle_duration, avg_cycle_duration, cycles_count) )
}

fn parse_fix_data(data: &[u8], static_buffers_idx: usize) -> Response {
    if data.len() < 1 {
        return Response::NotParsed;
    }
    let fix_data_buffer: &'static mut FixDataContainer = unsafe{ &mut STATIC_BUFFERS[static_buffers_idx].fix_data_buffer };
    match parse_fix_data_struct(data, fix_data_buffer) {
        Ok(fix_data_buffer) => Response::FixData(fix_data_buffer),
        Err(_) => Response::NotParsed
    }
}

fn parse_fix_data_struct<'a>(data: &[u8], fix_data_buffer: &'a mut FixDataContainer) -> Result<&'a FixDataContainer, Errors> {
    let relays_count = data[0];
    if (data.len() as u8) < relays_count * 5 + 1 {
        return Err(Errors::NotEnoughDataGot);
    }
    fix_data_buffer.update_count(relays_count as usize)?;
    for i in 0..relays_count as usize {
        let pos = 1 + i * 5;
        let try_count = data[pos];
        let try_time = (data[pos + 1] as u32) << 8 | data[pos + 2] as u32;
        fix_data_buffer.update_data(i, try_count, try_time)?;
    }
    Ok(fix_data_buffer)
}

fn parse_state(data: &[u8]) -> Response {
    if data.len() < 1 {
        return Response::NotParsed;
    }
    let count = data[0];
    if data.len() < 1 + count as usize || count > 16 {
        return Response::NotParsed;
    }
    let pairs_count = count as usize / 2;
    if data.len() < 1 + pairs_count {
        return Response::NotParsed;
    }
    let state_data = parse_state_data_force(pairs_count, &data[1..]);
    Response::State(State::from_u64(count, state_data))
}

fn parse_state_data_force(bytes_count: usize, data: &[u8]) -> u64 {
    let mut state_data = 0_u64;
    for i in 0..bytes_count {
        state_data |= (data[i] as u64) << (i * 8);
    }
    state_data
}

fn parse_relay_state(data: &[u8]) -> Response {
    if data.len() < 1 {
        return Response::NotParsed;
    }
    let data = RelayState::from_u8(data[0]);
    Response::RelayState(data)
}

fn parse_state_fix_settings(data: &[u8]) -> Response {
    if data.len() < 4 {
        return Response::NotParsed;
    }
    let mut state_fix_settings = StateFixSettings {
        switch_try_duration: RelativeMillis16(((data[0] as u16) << 8) | data[1] as u16),
        switch_try_count: data[2],
        wait_delay: RelativeSeconds8(data[3]),
        contact_ready_wait_delay: RelativeMillis16(((data[4] as u16) << 8) | data[5] as u16),
    };
    Response::StateFixSettings(state_fix_settings)
}

fn parse_id(data: &[u8]) -> Response {
    match get_u32(data) {
        Some(remote_timestamp) => Response::Id(remote_timestamp),
        None => Response::NotParsed,
    }
}

fn parse_current_time(data: &[u8]) -> Response {
    match get_u32(data) {
        Some(remote_timestamp) => Response::CurrentTime(RelativeSeconds::new(remote_timestamp)),
        None => Response::NotParsed,
    }
}

fn parse_remote_timestamp(data: &[u8]) -> Response {
    match get_u32(data) {
        Some(remote_timestamp) => Response::RemoteTimestamp(RelativeSeconds::new(remote_timestamp)),
        None => Response::NotParsed,
    }
}

fn get_u32(data: &[u8]) -> Option<u32> {
    if data.len() < 4 {
        None
    } else {
        Some(get_u32_force(&data[0..4]))
    }
}

fn get_u32_force(data: &[u8]) -> u32 {
    (data[0] as u32) << 24 | (data[1] as u32) << 16 | (data[2] as u32) << 8 | data[3] as u32
}

fn read_signal_data(instruction: Instruction, data: &[u8]) -> Result<SignalData, ErrorCode> {
    if data.len() < 5 {
        return Err(ErrorCode::ERequestDataNoValue);
    }
    let relay_idx = data[0] & 0x0f_u8;
    let is_on = data[0] & 0x10 > 0;
    let mut relative_seconds = 0_u32;
    for i in 0..4 {
        relative_seconds |= (data[1 + i] as u32) << (8 * (3 - i));
    }
    let is_called_internally = if instruction == Instruction::RelayStateChanged {
        Some(data[0] & 0x20 > 0)
    } else {
        None
    };

    Ok(SignalData {
        instruction,
        relative_timestamp: RelativeSeconds::new(relative_seconds),
        relay_idx,
        is_on,
        is_called_internally,
    })
}