use time::PrimitiveDateTime;
use logic::hal_ext::rtc_wrapper::Rtc;

pub  struct RtcWrapper {
    rtc: stm32f4xx_hal::rtc::Rtc
}

impl RtcWrapper {
    pub fn new(rtc: stm32f4xx_hal::rtc::Rtc) -> Self {
        Self {
            rtc
        }
    }
}

impl Rtc for RtcWrapper {
    type Error = stm32f4xx_hal::rtc::Error;

    #[inline(always)]
    fn get_datetime(&mut self) -> PrimitiveDateTime {
        self.rtc.get_datetime()
    }

    #[inline(always)]
    fn set_datetime(&mut self, date: &PrimitiveDateTime) -> Result<(), Self::Error> {
        self.rtc.set_datetime(date)
    }
}