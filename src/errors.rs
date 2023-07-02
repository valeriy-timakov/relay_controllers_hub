#![deny(unsafe_code)]
#![deny(warnings)]

use stm32f4xx_hal::dma::DMAError;

#[derive(Debug)]
pub enum Errors {
    NoBufferAvailable,
    TransferInProgress,
    DmaBufferOverflow,
    CommandDataCorrupted,
    DmaError(DMAError<()>),
}