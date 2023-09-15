#![deny(unsafe_code)]


use crate::hal_ext::serial_transfer::Decomposable;

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
    RequestsLimitReached,
    RequestsNeedsCacheAlreadySent,
    NoRequestsFound,
    RelayIndexOutOfRange,
    RelayCountOverflow,
    SlaveControllersInstancesMaxCountReached,
    FromAfterTo,
    OutOfRange,
    SwitchesDataCountOverflow,
    InvalidDataSize,
    InstructionNotSerializable,
}

#[derive(Debug)]
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