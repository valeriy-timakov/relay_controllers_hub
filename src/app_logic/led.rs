#![deny(unsafe_code)]
#![deny(warnings)]

use stm32f4xx_hal::{
    gpio::{ Output, PushPull, Pin },
};

pub struct Led<const P: char, const N: u8> {
    pin: Pin<P, N, Output<PushPull>>,
    on_cycles_count: u16,
    off_cycles_count: u16,
    on_when_low: bool,
    cycles_count: u16,
    is_on: bool,
}

impl<const P: char, const N: u8> Led<P, N> {
    pub fn new(on_cycles_count: u16, off_cycles_count: u16, on_when_low: bool, pin: Pin<P, N, Output<PushPull>>) -> Self {
        Self {
            pin,
            on_cycles_count,
            off_cycles_count,
            on_when_low,
            cycles_count: 0,
            is_on: false,
        }
    }

    pub fn init(&mut self, on: bool) {
        self.is_on = on;
        self.cycles_count = 0;
        if self.is_on ^ self.on_when_low {
            self.pin.set_high();
        } else {
            self.pin.set_low();
        }
    }

    pub fn update(&mut self) {
        self.cycles_count += 1;
        if self.is_on && self.cycles_count >= self.on_cycles_count {
            self.init(false);
        } else if !self.is_on && self.cycles_count >= self.off_cycles_count {
            self.init(true);
        }
    }

    pub fn update_periods<F>(&mut self, updater: F) where F: FnOnce(u16, u16) -> (u16, u16) {
        (self.on_cycles_count, self.off_cycles_count) = updater(self.on_cycles_count, self.off_cycles_count);
    }
}