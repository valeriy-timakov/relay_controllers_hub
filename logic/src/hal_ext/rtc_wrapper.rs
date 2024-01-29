#![deny(unsafe_code)]

use time::PrimitiveDateTime;
use time_core::convert::{ Millisecond, Second, Nanosecond};

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct RelativeMillis(u32);

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
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
        RelativeSeconds(self.0 / Millisecond::per(Second) as u32)
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

pub trait Rtc {
    type Error;
    fn get_datetime(&mut self) -> PrimitiveDateTime;
    fn set_datetime(&mut self, date: &PrimitiveDateTime) -> Result<(), Self::Error>;
}

pub trait RelativeTimestampSource {
    fn get(&mut self) -> RelativeMillis;
}

pub struct DateTimeSource<RTC: Rtc> {
    rtc: RTC,
    base_date_time: Option<PrimitiveDateTime>,
}

impl <RTC: Rtc> RelativeTimestampSource for DateTimeSource<RTC> {
    #[inline(always)]
    fn get(&mut self) -> RelativeMillis {
        self.get_relative_timestamp()
    }
}

impl<RTC: Rtc> DateTimeSource<RTC> {
    pub fn new(rtc: RTC) -> Self {
        Self { rtc, base_date_time: None }
    }

    pub fn get_datetime(&mut self) -> PrimitiveDateTime {
        self.rtc.get_datetime()
    }

    pub fn set_datetime(&mut self, date: PrimitiveDateTime) -> Result<Option<PrimitiveDateTime>, RTC::Error> {
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

                RelativeMillis( duration.whole_seconds() as u32 * Millisecond::per(Second) as u32
                    + duration.subsec_nanoseconds() as u32 / Nanosecond::per(Millisecond) )
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
                (millis.0 / Millisecond::per(Second) as u32) as i64,
                ((millis.0 % Millisecond::per(Second) as u32) * Nanosecond::per(Millisecond)) as i32);
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

    pub fn relative_seconds_to_date_time(&self, seconds: RelativeSeconds) -> Option<PrimitiveDateTime> {
        self.base_date_time.map(|base_date_time| {
            let duration = time::Duration::new(seconds.0 as i64,0);
            base_date_time + duration
        })
    }

}


#[cfg(test)]
mod tests {
    use super::*;
    use std::rc::Rc;
    use std::cell::RefCell;
    use time::{Month, PrimitiveDateTime};
    use quickcheck_macros::quickcheck;

    #[test]
    fn test_relative_millis() {
        let millis = RelativeMillis::new(1000);
        assert_eq!(millis.value(), 1000);
        assert_eq!(millis.seconds().value(), 1);
    }

    #[test]
    fn test_relative_seconds() {
        let millis = RelativeSeconds::new(1000);
        assert_eq!(millis.value(), 1000);
    }



    struct TestRtc {
        date_time: PrimitiveDateTime,
        get_calls: u32,
        set_calls: u32,
    }

    impl TestRtc {
        pub fn new(date_time: PrimitiveDateTime) -> Self {
            Self {
                date_time,
                get_calls: 0,
                set_calls: 0,
            }
        }

        pub fn get_calls(&self) -> u32 {
            self.get_calls
        }

        pub fn set_calls(&self) -> u32 {
            self.set_calls
        }

        pub fn clear_calls(&mut self) {
            self.get_calls = 0;
            self.set_calls = 0;
        }

        pub fn set_time_passed(&mut self, millis: RelativeMillis) {
            self.date_time = self.date_time + time::Duration::new(
                (millis.0 / 1000) as i64, (millis.0 % 1000) as i32 * 1000000);
        }
    }

    #[derive(Debug, PartialEq)]
    enum TestRtcError {
        InvalidDate,
    }

    impl Rtc for TestRtc {
        type Error = TestRtcError;

        fn get_datetime(&mut self) -> PrimitiveDateTime {
            self.get_calls += 1;
            self.date_time
        }

        fn set_datetime(&mut self, date: &PrimitiveDateTime) -> Result<(), Self::Error> {
            self.set_calls += 1;
            self.date_time = *date;
            Ok(())
        }
    }

    impl Rtc for Rc<RefCell<TestRtc>> {
        type Error = TestRtcError;

        fn get_datetime(&mut self) -> PrimitiveDateTime {
            self.borrow_mut().get_datetime()
        }

        fn set_datetime(&mut self, date: &PrimitiveDateTime) -> Result<(), Self::Error> {
            self.borrow_mut().set_datetime(date);
            Ok(())
        }
    }

    #[test]
    fn test_rtc_wrapper_get_date_time() {
        let start_date_time = PrimitiveDateTime::new(
            time::Date::from_calendar_date(2020, Month::January, 1).unwrap(),
            time::Time::from_hms(0, 0, 0).unwrap());
        let mock = Rc::new(RefCell::new(TestRtc::new(start_date_time)));
        let mut rtc_wrapper = DateTimeSource::new(mock.clone());
        //should proxy to wrapped
        assert_eq!(rtc_wrapper.get_datetime(), start_date_time);
        assert_eq!(mock.borrow().get_calls(), 1);
    }

    #[test]
    fn test_rtc_wrapper_set_date() {
        let start_date_time = PrimitiveDateTime::new(
            time::Date::from_calendar_date(2020, Month::January, 1).unwrap(),
            time::Time::from_hms(0, 0, 0).unwrap());
        let mock = Rc::new(RefCell::new(TestRtc::new(start_date_time)));
        let mut rtc_wrapper = DateTimeSource::new(mock.clone());
        let date_time_2 = PrimitiveDateTime::new(
            time::Date::from_calendar_date(2021, Month::February, 2).unwrap(),
            time::Time::from_hms(1, 1, 10).unwrap());
        //check initial state
        assert_eq!(rtc_wrapper.get_datetime(), start_date_time);
        //first set should return Ok(None)
        assert_eq!(rtc_wrapper.set_datetime(date_time_2), Ok(None));
        //... and set the date
        assert_eq!(rtc_wrapper.get_datetime(), date_time_2);
        //should proxy to wrapped
        assert_eq!(mock.borrow().set_calls(), 1);
        //previous set should set base date time
        assert_eq!(rtc_wrapper.set_datetime(date_time_2), Ok(Some(date_time_2)));
    }

    #[quickcheck]
    fn test_rtc_wrapper_get_relative_timestamp(shift1: u32, shift2: u32) {
        let start_date_time = PrimitiveDateTime::new(
            time::Date::from_calendar_date(2020, Month::January, 1).unwrap(),
            time::Time::from_hms(0, 0, 0).unwrap());
        let mock = Rc::new(RefCell::new(TestRtc::new(start_date_time)));
        let mut rtc_wrapper = DateTimeSource::new(mock.clone());
        //first call should return 0
        assert_eq!(rtc_wrapper.get_relative_timestamp(), RelativeMillis(0));
        //should proxy to wrapped get_datetime
        assert_eq!(mock.borrow().get_calls(), 1);
        //next calls should based on value set by first call
        let shift = RelativeMillis(shift1);
        mock.borrow_mut().set_time_passed(shift);
        assert_eq!(rtc_wrapper.get_relative_timestamp(), shift);
        //if set_datetime is called, next calls should be based on new value
        let date_time_2 = PrimitiveDateTime::new(
            time::Date::from_calendar_date(2021, Month::February, 2).unwrap(),
            time::Time::from_hms(1, 1, 10).unwrap());
        rtc_wrapper.set_datetime(date_time_2).unwrap();
        let shift = RelativeMillis(shift2);
        mock.borrow_mut().set_time_passed(shift);
        assert_eq!(rtc_wrapper.get_relative_timestamp(), shift);
    }

    #[quickcheck]
    fn test_rtc_wrapper_relative_timestamp_to_date_time(shift_u32: u32) {
        let start_date_time = PrimitiveDateTime::new(
            time::Date::from_calendar_date(2020, Month::January, 1).unwrap(),
            time::Time::from_hms(0, 0, 0).unwrap());
        let mock = Rc::new(RefCell::new(TestRtc::new(start_date_time)));
        let mut rtc_wrapper = DateTimeSource::new(mock.clone());
        rtc_wrapper.get_relative_timestamp();
        //next calls should based on value set by first call
        let shift = RelativeMillis(shift_u32);
        mock.borrow_mut().set_time_passed(shift);
        let relative_date_time = rtc_wrapper.relative_timestamp_to_date_time(shift);
        assert_eq!(relative_date_time.unwrap(), rtc_wrapper.get_datetime());
    }

    #[quickcheck]
    fn test_rtc_wrapper_get_relative_seconds(shift1: u32, shift2: u32) {
        let shift1 = shift1 / 1000;
        let shift2 = shift2 / 1000;
        let start_date_time = PrimitiveDateTime::new(
            time::Date::from_calendar_date(2020, Month::January, 1).unwrap(),
            time::Time::from_hms(0, 0, 0).unwrap());
        let mock = Rc::new(RefCell::new(TestRtc::new(start_date_time)));
        let mut rtc_wrapper = DateTimeSource::new(mock.clone());
        //first call should return 0
        assert_eq!(rtc_wrapper.get_relative_seconds(), RelativeSeconds(0));
        //should proxy to wrapped get_datetime
        assert_eq!(mock.borrow().get_calls(), 1);
        //next calls should based on value set by first call
        let shift = RelativeSeconds(shift1);
        mock.borrow_mut().set_time_passed(RelativeMillis(shift.value() as u32 * 1000));
        assert_eq!(rtc_wrapper.get_relative_seconds(), shift);
        //if set_datetime is called, next calls should be based on new value
        let date_time_2 = PrimitiveDateTime::new(
            time::Date::from_calendar_date(2021, Month::February, 2).unwrap(),
            time::Time::from_hms(1, 1, 10).unwrap());
        rtc_wrapper.set_datetime(date_time_2).unwrap();
        let shift = RelativeSeconds(shift2);
        mock.borrow_mut().set_time_passed(RelativeMillis(shift.value() as u32 * 1000));
        assert_eq!(rtc_wrapper.get_relative_seconds(), shift);
    }

    #[quickcheck]
    fn test_rtc_wrapper_relative_seconds_to_date_time(shift_u32: u32) {
        let shift_u32 = shift_u32 / 1000;
        let start_date_time = PrimitiveDateTime::new(
            time::Date::from_calendar_date(2020, Month::January, 1).unwrap(),
            time::Time::from_hms(0, 0, 0).unwrap());
        let mock = Rc::new(RefCell::new(TestRtc::new(start_date_time)));
        let mut rtc_wrapper = DateTimeSource::new(mock.clone());
        rtc_wrapper.get_relative_seconds();
        //next calls should based on value set by first call
        let shift = RelativeSeconds(shift_u32);
        mock.borrow_mut().set_time_passed(RelativeMillis(shift.value() as u32 * 1000));
        let relative_date_time = rtc_wrapper.relative_seconds_to_date_time(shift);
        assert_eq!(relative_date_time.unwrap(), rtc_wrapper.get_datetime());
    }
}




