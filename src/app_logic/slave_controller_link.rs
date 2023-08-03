pub mod domain;

use alloc::boxed::Box;
use core::mem::size_of;
use cortex_m_semihosting::hprintln;
use stm32f4xx_hal::dma::{ChannelX, MemoryToPeripheral, PeripheralToMemory};
use stm32f4xx_hal::serial::{Rx, Tx, Instance, RxISR, TxISR, RxListen};
use stm32f4xx_hal::dma::traits::{Channel, DMASet, PeriAddress, Stream};
use crate::app_logic::slave_controller_link::domain::{*};
use crate::errors::Errors;
use crate::hal_ext::rtc_wrapper::{RelativeMillis, RelativeSeconds };
use crate::hal_ext::serial_transfer::{RxTransfer, SerialTransfer, TxTransfer};




const fn max_of(size1: usize, size2: usize, size3: usize, size4: usize, size5: usize, ) -> usize {
    let mut max = size1;
    if size2 > max { max = size2; }
    if size3 > max { max = size3; }
    if size4 > max { max = size4; }
    if size5 > max { max = size5; }
    max
}

const RESPONSE_BUFFER_SIZE: usize = max_of(size_of::<FixDataContainer>(), size_of::<RelaysSettings>(),
    size_of::<ContactsWaitData>(), size_of::<StateSwitchDatas>(), size_of::<AllData>());

type ResponseBuffer = [u8; RESPONSE_BUFFER_SIZE];

pub struct SignalData {
    instruction: Signals,
    relative_timestamp: RelativeSeconds,
    relay_idx: u8,
    is_on: bool,
    is_called_internally: Option<bool>,
}

const MAX_INSTANCES_COUNT: usize = 3;
static mut STATIC_BUFFERS: [ResponseBuffer; MAX_INSTANCES_COUNT] = [ [0; RESPONSE_BUFFER_SIZE], [0; RESPONSE_BUFFER_SIZE], [0; RESPONSE_BUFFER_SIZE] ];
static mut INSTANCES_COUNT: usize = 0;

pub trait SignalsReceiver {
    fn on_signal(&mut self, signal_data: SignalData);
    fn on_signal_error(&mut self, instruction: Option<Signals>, error_code: ErrorCode);
    fn on_request_success(&mut self, request: &SentRequest);
    fn on_request_error(&mut self, request: &SentRequest, error_code: ErrorCode);
    fn on_request_parse_error(&mut self, request: &SentRequest, error: Errors, data: &[u8]);
    fn on_request_response(&mut self, request: &SentRequest, response: DataInstructions);
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

pub struct SentRequest {
    operation: Operation,
    instruction: DataInstructions,
    rel_timestamp: RelativeMillis
}

impl SentRequest {
    fn new(operation: Operation, instruction: DataInstructions, rel_timestamp: RelativeMillis) -> Self {
        Self {
            operation,
            instruction,
            rel_timestamp
        }
    }
}

const MAX_REQUESTS_COUNT: usize = 4;

struct TransmitterToSlaveController<U, TxStream, const TX_CHANNEL: u8>
    where
        U: Instance,
        Tx<U>: PeriAddress<MemSize=u8> + DMASet<TxStream, TX_CHANNEL, MemoryToPeripheral> + TxISR,
        TxStream: Stream,
        ChannelX<TX_CHANNEL>: Channel,
{
    tx: TxTransfer<U, TxStream, TX_CHANNEL>,
    sent_requests: [Option<SentRequest>; MAX_REQUESTS_COUNT],
    requests_count: usize,
    request_needs_cache_send: bool,
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
            request_needs_cache_send: false,
        }
    }

    pub fn send_request(&mut self, operation: Operation, instruction: DataInstructions, timestamp: RelativeMillis) -> Result<(), Errors> {
        if self.requests_count == MAX_REQUESTS_COUNT {
            return Err(Errors::RequestsLimitReached);
        }
        let is_request_needs_cache = request_needs_cache(instruction.code());
        if is_request_needs_cache && self.request_needs_cache_send {
            return Err(Errors::RequestsNeedsCacheAlreadySent);
        }

        let result = self.tx.start_transfer(|buffer| {
            buffer.clear();
            buffer.add_u8(Operation::None as u8)?;
            buffer.add_u8(operation as u8)?;
            buffer.add_u8(instruction.discriminant())?;
            instruction.serialize(buffer)
        });
        self.sent_requests[self.requests_count] = Some(SentRequest::new(operation, instruction, timestamp));
        self.requests_count += 1;
        if is_request_needs_cache {
            self.request_needs_cache_send = true;
        }

        result
    }

    pub fn send_error(&mut self, instruction_code: u8, error_code: ErrorCode) -> Result<(), Errors> {
        self.tx.start_transfer(|buffer| {
            buffer.add_u8(Operation::None as u8)?;
            buffer.add_u8(Operation::Error as u8)?;
            buffer.add_u8(instruction_code)?;
            buffer.add_u8(error_code.discriminant())
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
            hprintln!("rx got");
            if data.len() > 3 && data[0] == Operation::None as u8 {
                let operation_code = data[1];
                let instruction_code = data[2];
                if operation_code == Operation::Signal as u8 {
                    if instruction_code == Signals::GetTimeStamp as u8 {
                        let timestamp = time_src();
                        tx.send_request(Operation::Set,
                            DataInstructions::RemoteTimestamp(Conversation::Data(timestamp.seconds())), timestamp)
                    } else {
                        let instruction = if instruction_code == Signals::MonitoringStateChanged as u8 {
                            Some(Signals::MonitoringStateChanged)
                        } else  if instruction_code == Signals::StateFixTry as u8 {
                            Some(Signals::StateFixTry)
                        } else  if instruction_code == Signals::ControlStateChanged as u8 {
                            Some(Signals::ControlStateChanged)
                        } else  if instruction_code == Signals::RelayStateChanged as u8 {
                            Some(Signals::RelayStateChanged)
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
                                        tx.send_error(instruction as u8, error).unwrap();
                                        Err(Errors::DataCorrupted)
                                    }
                                }
                            }
                            None => {
                                signal_receiver.on_signal_error(None, ErrorCode::EInstructionUnrecognized);
                                tx.send_error(Signals::Unknown as u8, ErrorCode::ERequestDataNoValue).unwrap();
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
                                if request.instruction.discriminant() == instruction_code && request.operation == search_operation {
                                    if operation_code == Operation::Success as u8 {
                                        signal_receiver.on_request_success(request);
                                    } else if operation_code == Operation::Error as u8 {
                                        signal_receiver.on_request_error(request, ErrorCode::for_code(instruction_code));
                                    } else {
                                        match parse_response(instruction_code, &data[3..], *static_buffers_idx) {
                                            Ok(response) => {
                                                signal_receiver.on_request_response(request, response);
                                            }
                                            Err(error) => {
                                                signal_receiver.on_request_parse_error(request, error, &data[3..]);
                                            }
                                        }
                                    }
                                    if operation_code == Operation::Response as u8 && request_needs_cache(request.instruction.code()) {
                                        tx.request_needs_cache_send = false;
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

pub fn init_slave_controllers() {
    init_cache_getters();
}

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

static mut df: [Option<Box<dyn CashedInstructionGetter>>; INSTRUCTIONS_COUNT] = [None, None, None, None, None, None, None,
    None, None, None, None, None, None, None, None, None, None, None, None, None, None, None, None, None, None];

fn init_cache_getters() {
    unsafe {
        df[DataInstructionCodes::Settings as usize] = Some(Box::new(RelasySettingsCashedInstructionGetter));
        df[DataInstructionCodes::ContactWaitData as usize] = Some(Box::new(ContactsWaitDataCashedInstructionGetter));
        df[DataInstructionCodes::SwitchData as usize] = Some(Box::new(StateSwitchDataCashedInstructionGetter));
        df[DataInstructionCodes::FixData as usize] = Some(Box::new(FixDataContainerCashedInstructionGetter));
        df[DataInstructionCodes::All as usize] = Some(Box::new(AllDataCashedInstructionGetter));
    }
}

fn request_needs_cache(instruction: DataInstructionCodes) -> bool {
    match cache_getter(instruction) {
        Some(_) => { true },
        None => { false }
    }
}

fn cache_getter(code: DataInstructionCodes) -> Option< &'static Box<dyn CashedInstructionGetter>> {
    unsafe {
        df[code as usize].as_ref()
    }
}

fn parse_response(instruction_code: u8, data: &[u8], static_buffers_idx: usize) -> Result<DataInstructions, Errors> {
    let instruction = DataInstructionCodes::get(instruction_code)?;
    match cache_getter(instruction) {
        Some(getter) => {
            let mut cached_instruction = getter.get(static_buffers_idx);
            cached_instruction.parse_from(data)?;
            Ok(cached_instruction)
        },
        None => Ok(DataInstructions::parse(instruction, data)?)
    }
}

fn read_signal_data(instruction: Signals, data: &[u8]) -> Result<SignalData, ErrorCode> {
    if data.len() < 5 {
        return Err(ErrorCode::ERequestDataNoValue);
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

    Ok(SignalData {
        instruction,
        relative_timestamp: RelativeSeconds::new(relative_seconds),
        relay_idx,
        is_on,
        is_called_internally,
    })
}