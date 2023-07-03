use cortex_m_semihosting::hprintln;
use stm32f4xx_hal::dma::{ChannelX, MemoryToPeripheral, PeripheralToMemory};
use stm32f4xx_hal::serial::{Rx, Tx, Instance, RxISR, TxISR, RxListen};
use stm32f4xx_hal::dma::traits::{Channel, DMASet, PeriAddress, Stream};
use crate::errors::Errors;
use crate::hal_ext::rtc_wrapper::{RelativeMillis, RelativeSeconds};
use crate::hal_ext::serial_transfer::{RxTransfer, SerialTransfer, TxBuffer, TxTransfer};

#[repr(u8)]
#[derive(Copy, Clone)]
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
pub enum Instruction<T> {
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
    GetCyclesStatistics = 0x18,
    StateFixTry = 0x19,
    Unknown = 0xff
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
    EUndefinedCode = 128
}

pub trait SignalsReceiver {
    fn on_signal(&mut self, signal_data: SignalData);
    fn on_signal_error(&mut self, instruction: Option<Instruction<()>>, error_code: ErrorCode);
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
    pub fn new (serial_transfer: SerialTransfer<U, TxStream, TX_CHANNEL, RxStream, RX_CHANNEL>, signal_receiver: SR) -> Self {
        let (tx, rx) = serial_transfer.into();
        Self {
            tx: TransmitterToSlaveController::new(tx),
            rx: ReceiverFromSlaveController::new(rx, signal_receiver)
        }
    }

    pub fn on_get_command<TS:  FnOnce() -> RelativeSeconds>( &mut self, time_src: TS) -> Result<(), Errors> {
        self.rx.on_get_command(&mut self.tx, time_src)
    }

    pub fn on_rx_dma_interrupts(&mut self) {
        self.rx.on_dma_interrupts();
    }

    pub fn on_tx_dma_interrupts(&mut self) {
        self.tx.on_dma_interrupts();
    }
}

struct TransmitterToSlaveController<U, TxStream, const TX_CHANNEL: u8>
    where
        U: Instance,
        Tx<U>: PeriAddress<MemSize=u8> + DMASet<TxStream, TX_CHANNEL, MemoryToPeripheral> + TxISR,
        TxStream: Stream,
        ChannelX<TX_CHANNEL>: Channel,
{
    tx: TxTransfer<U, TxStream, TX_CHANNEL>,
}

struct Request<T> {
    operation: Operation,
    instruction: Instruction<T>,
    rel_timestamp: RelativeMillis
}

impl <U, TxStream, const TX_CHANNEL: u8> TransmitterToSlaveController<U, TxStream, TX_CHANNEL>
    where
        U: Instance,
        Tx<U>: PeriAddress<MemSize=u8> + DMASet<TxStream, TX_CHANNEL, MemoryToPeripheral> + TxISR,
        TxStream: Stream,
        ChannelX<TX_CHANNEL>: Channel,
{
    pub fn new (tx: TxTransfer<U, TxStream, TX_CHANNEL>) -> Self {
        Self { tx }
    }

    fn send_set_request<T, F: FnOnce(&mut TxBuffer)->()>(&mut self, instruction: Instruction<T>, writter: F) -> Result<(), Errors> {
        self.tx.start_transfer(|buffer| {
            buffer.add_byte(Operation::None as u8).unwrap();
            buffer.add_byte(Operation::Set as u8).unwrap();
            buffer.add_byte(instruction as u8).unwrap();
            writter(buffer);
        })
    }

    pub fn send_relative_timestamp(&mut self, value: RelativeSeconds) -> Result<(), Errors> {
        let seconds: u32 = value.value();
        self.tx.start_transfer(|buffer| {
            buffer.add_byte(Operation::None as u8).unwrap();
            buffer.add_byte(Operation::Set as u8).unwrap();
            buffer.add_byte(Instruction::RemoteTimestamp as u8).unwrap();
            for i in 0..4 {
                buffer.add_byte(((seconds >> ((3 - i) * 8)) & 0xff) as u8).unwrap();
            }
        })
    }



    pub fn send_error<T>(&mut self, instruction: Instruction<T>, error_code: ErrorCode) -> Result<(), Errors> {
        self.tx.start_transfer(|buffer| {
            buffer.add_byte(Operation::None as u8).unwrap();
            buffer.add_byte(Operation::Error as u8).unwrap();
            buffer.add_byte(instruction as u8).unwrap();
            buffer.add_byte(error_code as u8).unwrap();
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
}

pub struct SignalData {
    instruction: Instruction<()>,
    relative_timestamp: RelativeSeconds,
    relay_idx: u8,
    is_on: bool,
    is_called_internally: Option<bool>,
}

impl <U, RxStream, const RX_CHANNEL: u8, SR> ReceiverFromSlaveController<U, RxStream, RX_CHANNEL, SR>
    where
        U: Instance,
        Rx<U, u8>: PeriAddress<MemSize=u8> + DMASet<RxStream, RX_CHANNEL, PeripheralToMemory> + RxISR + RxListen,
        RxStream: Stream,
        ChannelX<RX_CHANNEL>: Channel,
        SR: SignalsReceiver,
{
    pub fn new (rx: RxTransfer<U, RxStream, RX_CHANNEL>, signal_receiver: SR) -> Self {
        Self { rx, signal_receiver: signal_receiver }
    }

    pub fn on_get_command<TS, TxStream, const TX_CHANNEL: u8>(
            &mut self,
            tx:  &mut TransmitterToSlaveController<U, TxStream, TX_CHANNEL>,
            time_src: TS) -> Result<(), Errors>
        where
            Tx<U>: PeriAddress<MemSize=u8> + DMASet<TxStream, TX_CHANNEL, MemoryToPeripheral> + TxISR,
            TxStream: Stream,
            ChannelX<TX_CHANNEL>: Channel,
            TS: FnOnce() -> RelativeSeconds,
    {
        let ReceiverFromSlaveController { rx, signal_receiver } = self;
        rx.on_rx_transfer_interrupt(|data| {
            hprintln!("rx6 got");
            if data.len() > 3 && data[0] == Operation::None as u8 {
                let operation = data[0];
                if operation == Operation::Signal as u8 {
                    let instruction_code = data[2];
                    if instruction_code == Instruction::GetTimeStamp as u8 {
                        let seconds: RelativeSeconds = time_src();
                        tx.send_relative_timestamp(seconds)
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
                                match Self::read_signal_data(instruction, &data[3..]) {
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
                } else if operation == Operation::Error as u8 {
                    Ok(())
                } else if operation == Operation::Response as u8 {
                    Ok(())
                } else if operation == Operation::Success as u8 {
                    Ok(())
                } else {
                    Err(Errors::OperationNotRecognized(operation))
                }
            } else {
                Err(Errors::NotEnoughDataGot)
            }
        })
    }

    fn read_signal_data<T>(instruction: Instruction<T>, data: &[u8]) -> Result<SignalData, ErrorCode> {
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

    #[inline(always)]
    pub fn on_dma_interrupts(&mut self) {
        self.rx.on_dma_interrupts();
    }
}
