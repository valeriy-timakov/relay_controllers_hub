#![deny(unsafe_code)]

use stm32f4xx_hal::rtc::{Error, Rtc};
use time::PrimitiveDateTime;
use time_core::convert::{ Millisecond, Second, Nanosecond};

#[derive(Copy, Clone)]
pub struct RelativeMillis(u32);

#[derive(Copy, Clone)]
pub struct RelativeSeconds(u32);

impl RelativeMillis {

    pub fn new(value: u32) -> Self {
        Self(value)
    }

    #[inline(always)]
    pub fn value(&self) -> u32 {
        self.0
    }

    #[inline(always)]
    pub fn seconds(&self) -> RelativeSeconds {
        RelativeSeconds(self.0 / Millisecond.per(Second) as u32)
    }
}

impl RelativeSeconds {

    #[inline(always)]
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    #[inline(always)]
    pub fn value(&self) -> u32 {
        self.0
    }
}

pub struct RtcWrapper {
    rtc: Rtc,
    base_date_time: Option<PrimitiveDateTime>,
}

impl RtcWrapper {
    pub fn new(rtc: Rtc) -> Self {
        Self { rtc, base_date_time: None }
    }

    pub fn get_datetime(&mut self) -> PrimitiveDateTime {
        self.rtc.get_datetime()
    }

    pub fn set_datetime(&mut self, date: PrimitiveDateTime) -> Result<Option<PrimitiveDateTime>, Error> {
        self.rtc.set_datetime(&date).map(|()| {
            let old_base_date_time = self.base_date_time;
            self.base_date_time = Some(date);
            old_base_date_time
        })
    }

    pub fn get_relative_timestamp(&mut self) -> RelativeMillis {
        match self.base_date_time {
            Some(base_date_time) => {
                let current_date_time = self.get_datetime();
                let duration = current_date_time - base_date_time;

                RelativeMillis( duration.whole_seconds() as u32 * Millisecond.per(Second) as u32
                    + duration.subsec_nanoseconds() as u32 / Nanosecond.per(Millisecond) )
            },
            None => {
                self.base_date_time = Some(self.get_datetime());
                RelativeMillis(0)
            },
        }
    }

    pub fn relative_timestamp_to_date_time(&self, millis: RelativeMillis) -> Option<PrimitiveDateTime> {
        self.base_date_time.map(|base_date_time| {
            let duration = time::Duration::new(
                (millis.0 / Millisecond.per(Second) as u32) as i64,
                ((millis.0 % Millisecond.per(Second) as u32) * Nanosecond.per(Millisecond)) as i32);
            base_date_time + duration
        })
    }

    pub fn get_relative_seconds(&mut self) -> RelativeSeconds {
        match self.base_date_time {
            Some(base_date_time) => {
                let current_date_time = self.get_datetime();
                let duration = current_date_time - base_date_time;
                RelativeSeconds( duration.whole_seconds() as u32  )
            },
            None => {
                self.base_date_time = Some(self.get_datetime());
                RelativeSeconds(0)
            },
        }
    }

    pub fn relative_seconds_to_date_time(&self, millis: RelativeMillis) -> Option<PrimitiveDateTime> {
        self.base_date_time.map(|base_date_time| {
            let duration = time::Duration::new(
                (millis.0 / Millisecond.per(Second) as u32) as i64,
                ((millis.0 % Millisecond.per(Second) as u32) * Nanosecond.per(Millisecond)) as i32);
            base_date_time + duration
        })
    }

}