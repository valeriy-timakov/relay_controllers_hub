use crate::errors::Errors;

pub mod write_to;

pub struct Empty;

pub const EMPTY: Empty = Empty;

#[derive(Copy, Clone)]
pub struct BitsU8 {
    pub bits: u8,
}

impl BitsU8 {

    #[inline(always)]
    pub const fn new(bits: u8) -> Self {
        Self { bits }
    }

    #[inline(always)]
    pub fn set(&mut self, bit: u8) {
        self.bits |= 1 << bit;
    }

    #[inline(always)]
    pub fn clear(&mut self, bit: u8) {
        self.bits &= !(1 << bit);
    }

    #[inline(always)]
    pub fn get(&self, bit: u8) -> bool {
        self.bits & (1 << bit) != 0
    }

    #[inline(always)]
    pub fn toggle(&mut self, bit: u8) {
        self.bits ^= 1 << bit;
    }

    #[inline(always)]
    pub fn set_value(&mut self, bit: u8, value: bool) {
        if value {
            self.set(bit);
        } else {
            self.clear(bit);
        }
    }

    /**
    Value of bits from `from` to `to` (inclusive).
    */
    pub fn bits(&self, from: u8, to: u8) -> Result<u8, Errors> {
        if from > to {
            return Err(Errors::FromAfterTo);
        }
        if from > 7 || to > 7 {
            return Err(Errors::OutOfRange);
        }
        let mask = (1 << (to - from + 1)) - 1;
        Ok((self.bits >> from) & mask)
    }

}

pub struct BitsU64 {
    pub bits: u64,
}

impl BitsU64 {

    pub const fn new(bits: u64) -> Self {
        Self { bits }
    }

    #[inline(always)]
    pub fn set(&mut self, bit: u8) {
        self.bits |= 1 << bit;
    }

    #[inline(always)]
    pub fn clear(&mut self, bit: u8) {
        self.bits &= !(1 << bit);
    }

    #[inline(always)]
    pub fn get(&self, bit: u8) -> bool {
        self.bits & (1 << bit) != 0
    }

    #[inline(always)]
    pub fn toggle(&mut self, bit: u8) {
        self.bits ^= 1 << bit;
    }

    #[inline(always)]
    pub fn set_value(&mut self, bit: u8, value: bool) {
        if value {
            self.set(bit);
        } else {
            self.clear(bit);
        }
    }

}

