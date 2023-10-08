#![deny(unsafe_code)]
#![deny(warnings)]
use crate::errors::Errors;

pub mod dma_read_buffer;
pub mod write_to;


pub struct Empty;

pub const EMPTY: Empty = Empty;

#[derive(Copy, Clone, PartialEq, Debug)]
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

#[derive(PartialEq, Debug)]
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


#[cfg(test)]
mod tests {
    use crate::utils::BitsU8;

    #[test]
    fn test_set0() {
        let mut value = BitsU8::new(0b0000_0000);
        value.set(0);
        assert_eq!(value.bits, 1);
    }

    #[test]
    fn test_set1() {
        let mut value = BitsU8::new(0b0000_0000);
        value.set(1);
        assert_eq!(value.bits, 2);
    }

    #[test]
    fn test_set2() {
        let mut value = BitsU8::new(0b0000_0000);
        value.set(2);
        assert_eq!(value.bits, 4);
    }

    #[test]
    fn test_set3() {
        let mut value = BitsU8::new(0b0000_0000);
        value.set(3);
        assert_eq!(value.bits, 8);
    }

    #[test]
    fn test_set4() {
        let mut value = BitsU8::new(0b0000_0000);
        value.set(4);
        assert_eq!(value.bits, 16);
    }

    #[test]
    fn test_set5() {
        let mut value = BitsU8::new(0b0000_0000);
        value.set(5);
        assert_eq!(value.bits, 32);
    }

    #[test]
    fn test_se6() {
        let mut value = BitsU8::new(0b0000_0000);
        value.set(6);
        assert_eq!(value.bits, 64);
    }

    #[test]
    fn test_set7() {
        let mut value = BitsU8::new(0b0000_0000);
        value.set(7);
        assert_eq!(value.bits, 128);
    }

    #[test]
    #[should_panic]
    fn test_set8() {
        let mut value = BitsU8::new(0b0000_0000);
        value.set(8);
    }

    #[test]
    fn test_setn() {
        for i in 0..8 {
            let mut value = BitsU8::new(0b0000_0000);
            value.set(i);
            assert_eq!(value.bits, 1 << i);
        }
    }

    #[test]
    fn test_set_n2() {
        for i in 0..8 {
            for j in 0..8 {
                if i == j {
                    let mut value = BitsU8::new(0b0000_0000);
                    value.set(i);
                    value.set(j);
                    assert_eq!(value.bits, 1 << i);
                }  else {
                    let mut value = BitsU8::new(0b0000_0000);
                    value.set(i);
                    value.set(j);
                    assert_eq!(value.bits, (1 << i) | (1 << j));
                }
            }
        }
    }

    #[test]
    fn test_clear() {
        for i in 0..8 {
            let mut value = BitsU8::new(0b1111_1111);
            value.clear(i);
            assert_eq!(value.bits, 0b1111_1111 & !(1 << i));
        }
    }

    #[test]
    fn test_get() {
        for i in 0..8 {
            let mut value = BitsU8::new(0b0000_0000);
            assert_eq!(value.get(i), false);
            value.set(i);
            assert_eq!(value.get(i), true);
        }
    }

    #[test]
    fn test_toggle() {
        for i in 0..8 {
            let mut value = BitsU8::new(0b0000_0000);
            value.toggle(i);
            assert_eq!(value.get(i), true);
            value.toggle(i);
            assert_eq!(value.get(i), false);
        }
    }

    #[test]
    fn test_set_value() {
        for i in 0..8 {
            let mut value = BitsU8::new(0b0000_0000);
            value.set_value(i, true);
            assert_eq!(value.get(i), true);
            value.set_value(i, false);
            assert_eq!(value.get(i), false);
        }
    }

    #[test]
    fn test_set_u64() {
        for i in 0..64 {
            let mut value = crate::utils::BitsU64::new(0b0000_0000);
            value.set(i);
            assert_eq!(value.bits, 1 << i);
        }
    }

    #[test]
    fn test_clear_u64() {
        for i in 0..64 {
            let mut value = crate::utils::BitsU64::new(0b1111_1111);
            value.clear(i);
            assert_eq!(value.bits, 0b1111_1111 & !(1 << i));
        }
    }

    #[test]
    fn test_get_u64() {
        for i in 0..64 {
            let mut value = crate::utils::BitsU64::new(0b0000_0000);
            assert_eq!(value.get(i), false);
            value.set(i);
            assert_eq!(value.get(i), true);
        }
    }

    #[test]
    fn test_toggle_u64() {
        for i in 0..64 {
            let mut value = crate::utils::BitsU64::new(0b0000_0000);
            value.toggle(i);
            assert_eq!(value.get(i), true);
            value.toggle(i);
            assert_eq!(value.get(i), false);
        }
    }

    #[test]
    fn test_set_value_u64() {
        for i in 0..64 {
            let mut value = crate::utils::BitsU64::new(0b0000_0000);
            value.set_value(i, true);
            assert_eq!(value.get(i), true);
            value.set_value(i, false);
            assert_eq!(value.get(i), false);
        }
    }


}