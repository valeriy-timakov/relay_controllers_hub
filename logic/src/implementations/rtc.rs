use time::PrimitiveDateTime;
use crate::hal_ext::rtc_wrapper::Rtc;

impl Rtc for stm32f4xx_hal::rtc::Rtc {
    type Error = stm32f4xx_hal::rtc::Error;

    #[inline(always)]
    fn get_datetime(&mut self) -> PrimitiveDateTime {
        self.get_datetime()
    }

    #[inline(always)]
    fn set_datetime(&mut self, date: &PrimitiveDateTime) -> Result<(), Self::Error> {
        self.set_datetime(date)
    }
}