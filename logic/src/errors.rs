#![deny(unsafe_code)]

use core::fmt::Display;
use crate::services::slave_controller_link::domain::{ErrorCode, Operation};


#[derive(Debug, PartialEq, Copy, Clone)]
pub enum Errors {
    NoBufferAvailable,
    TransferInProgress,
    DmaBufferOverflow,
    CommandDataCorrupted,
    NotEnoughDataGot,
    OperationNotRecognized(u8),
    InstructionNotRecognized(u8),
    SlaveError(ErrorCode),
    DataCorrupted,
    DmaError(DMAError<()>),
    RequestsLimitReached,
    RequestsNeedsCacheAlreadySent,
    NoRequestsFound,
    UndefinedOperation,
    SentRequestsQueueIsEmpty,
    RelayIndexOutOfRange,
    RelayCountOverflow,
    SlaveControllersInstancesMaxCountReached,
    FromAfterTo,
    OutOfRange,
    SwitchesDataCountOverflow,
    InvalidDataSize,
    InstructionNotSerializable,
    WrongStateNotParsed,
    WrongStateIncompatibleOperation(Operation),
    WrongIncomingOperation(Operation),
    DataOverflow,
    IndexOverflow,
}

impl Display for Errors {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Errors::NoBufferAvailable => write!(f, "No buffer available"),
            Errors::TransferInProgress => write!(f, "Transfer in progress"),
            Errors::DmaBufferOverflow => write!(f, "DMA buffer overflow"),
            Errors::CommandDataCorrupted => write!(f, "Command data corrupted"),
            Errors::NotEnoughDataGot => write!(f, "Not enough data got"),
            Errors::OperationNotRecognized(op) => write!(f, "Operation not recognized: {}", op),
            Errors::InstructionNotRecognized(inst) => write!(f, "Instruction not recognized: {}", inst),
            Errors::SlaveError(err) => write!(f, "Slave error: {:?}", err),
            Errors::DataCorrupted => write!(f, "Data corrupted"),
            Errors::DmaError(err) => write!(f, "DMA error: {:?}", err),
            Errors::RequestsLimitReached => write!(f, "Requests limit reached"),
            Errors::RequestsNeedsCacheAlreadySent => write!(f, "Requests needs cache already sent"),
            Errors::NoRequestsFound => write!(f, "No requests found"),
            Errors::UndefinedOperation => write!(f, "Undefined operation"),
            Errors::SentRequestsQueueIsEmpty => write!(f, "Sent requests queue is empty"),
            Errors::RelayIndexOutOfRange => write!(f, "Relay index out of range"),
            Errors::RelayCountOverflow => write!(f, "Relay count overflow"),
            Errors::SlaveControllersInstancesMaxCountReached => write!(f, "Slave controllers instances max count reached"),
            Errors::FromAfterTo => write!(f, "From after to"),
            Errors::OutOfRange => write!(f, "Out of range"),
            Errors::SwitchesDataCountOverflow => write!(f, "Switches data count overflow"),
            Errors::InvalidDataSize => write!(f, "Invalid data size"),
            Errors::InstructionNotSerializable => write!(f, "Instruction not serializable"),
            Errors::WrongStateNotParsed => write!(f, "Wrong state not parsed"),
            Errors::WrongStateIncompatibleOperation(op) => write!(f, "Wrong state incompatible operation: {:?}", op),
            Errors::WrongIncomingOperation(op) => write!(f, "Wrong incoming operation: {:?}", op),
            Errors::DataOverflow => write!(f, "Data overflow"),
            Errors::IndexOverflow => write!(f, "Index overflow"),
        }
    }
}

#[derive(Debug, PartialEq, Copy, Clone)]
pub enum DMAError<T> {
    /// DMA not ready to change buffers.
    NotReady(T),
    /// The user provided a buffer that is not big enough while double buffering.
    SmallBuffer(T),
    /// Overrun during a double buffering or circular transfer.
    Overrun(T),
}

impl <T> DMAError<T> {
    #[inline(always)]
    pub fn decompose(self) -> (DMAError<()>, T) {
        match self {
            DMAError::NotReady(t) => (DMAError::NotReady(()), t),
            DMAError::SmallBuffer(t) => (DMAError::SmallBuffer(()), t),
            DMAError::Overrun(t) => (DMAError::Overrun(()), t),
        }
    }
}


/*
impl <T> Decomposable<T> for DMAError<T> {
    type Container<Y> = DMAError<Y>;

    #[inline(always)]
    fn decompose(self) -> (Self::Container<()>, T) {
        self.decompose()
    }

}*/