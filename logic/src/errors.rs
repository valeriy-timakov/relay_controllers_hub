#![deny(unsafe_code)]

use crate::services::slave_controller_link::domain::Operation;


#[derive(Debug, PartialEq, Copy, Clone)]
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