#![allow(unsafe_code)]
/*
use stm32f4xx_hal::dma::{CurrentBuffer, FifoLevel, StreamX};
use stm32f4xx_hal::dma::traits::{Instance, Stream, StreamISR };
use stm32f4xx_hal::pac;

pub trait StreamExt: Stream {

    /// Get the number of transfers (ndt) for the DMA stream.
    fn get_number_of_transfers(&self) -> u16;

    /// Convenience method to get the value of the 4 common interrupts for the DMA stream.
    /// The order of the returns are: `transfer_complete`, `half_transfer`, `transfer_error` and
    /// `direct_mode_error`.
    fn get_interrupts_enable(&self) -> (bool, bool, bool, bool);

    /// Get the current fifo level (fs) of the DMA stream.
    fn fifo_level(&self) -> FifoLevel;

    /// Get which buffer is currently in use by the DMA.
    fn current_buffer(&self) -> CurrentBuffer;

    //private - for internal use only
    unsafe  fn st() -> &'static pac::dma2::ST;
}


impl<I: Instance, const S: u8> StreamExt for StreamX<I, S>
    where
        Self: StreamISR,
{
    #[cfg(not(any(feature = "gpio-f411", feature = "gpio-f413", feature = "gpio-f410")))]
    #[inline(always)]
    unsafe fn st() -> &'static pac::dma2::ST {
        &(*I::ptr()).st[S as usize]
    }
    #[cfg(any(feature = "gpio-f411", feature = "gpio-f413", feature = "gpio-f410"))]
    #[inline(always)]
    unsafe fn st() -> &'static pac::dma1::ST {
        &(*DMA::ptr()).st[S as usize]
    }

    #[inline(always)]
    fn get_number_of_transfers(&self) -> u16 {
        unsafe { Self::st() }.ndtr.read().ndt().bits()
    }

    #[inline(always)]
    fn get_interrupts_enable(&self) -> (bool, bool, bool, bool) {
        let cr = unsafe { Self::st() }.cr.read();
        (
            cr.tcie().bit_is_set(),
            cr.htie().bit_is_set(),
            cr.teie().bit_is_set(),
            cr.dmeie().bit_is_set(),
        )
    }

    #[inline(always)]
    fn fifo_level(&self) -> FifoLevel {
        unsafe { Self::st() }.fcr.read().fs().bits().into()
    }

    fn current_buffer(&self) -> CurrentBuffer {
        if unsafe { Self::st() }.cr.read().ct().bit_is_set() {
            CurrentBuffer::DoubleBuffer
        } else {
            CurrentBuffer::FirstBuffer
        }
    }
}


/// Trait for DMA stream interrupt handling.
pub trait StreamExtISR: StreamISR {
    /// Get transfer complete flag.
    fn get_transfer_complete_flag() -> bool;

    /// Get half transfer flag.
    fn get_half_transfer_flag() -> bool;

    /// Get transfer error flag
    fn get_transfer_error_flag() -> bool;

    /// Get fifo error flag
    fn get_fifo_error_flag() -> bool;

    /// Get direct mode error flag
    fn get_direct_mode_error_flag() -> bool;
}

*/