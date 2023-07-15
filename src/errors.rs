#![deny(unsafe_code)]
#![deny(warnings)]

use stm32f4xx_hal::dma::DMAError;

#[derive(Debug)]
pub enum Errors {
    NoBufferAvailable,
    TransferInProgress,
    DmaBufferOverflow,
    CommandDataCorrupted,
    NotEnoughDataGot,
    OperationNotRecognized(u8),
    InstructionNotRecognized(u8),
    DataCorrupted,
    DmaError(DMAError<()>),
    RequestslLimitReached,
    NoRequestsFound,
    RelayIndexOutOfRange,
    RelayCountOverflow,
    SlaveControllersInstancesMaxCountReached,
    FromAfterTo,
    OutOfRange,
    SwitchesDataCountOverflow,
}