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
        if from >= 8 || to >= 8 {
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

    /**
    Value of bits from `from` to `to` (inclusive).
     */
    pub fn bits_u8(&self, from: u8, to: u8) -> Result<u8, Errors> {
        if from > to {
            return Err(Errors::FromAfterTo);
        }
        let count = to - from + 1;
        if count > 8 {
            return Err(Errors::IndexOverflow);
        }
        let res = (self.bits >> from) as u8;
        let mask = ((1_u16 << count) - 1)  as u8;
        Ok(res & mask)
    }

    /**
    Sets value of bits from `from` to `to` (inclusive) in u8 range.
     */
    pub fn set_byte(&mut self, from: u8, to: u8, value: u8) -> Result<(), Errors> {
        if from > to {
            return Err(Errors::FromAfterTo);
        }
        if from >= 64 || to >= 64 {
            return Err(Errors::OutOfRange);
        }
        let count = to - from + 1;
        if count > 8 {
            return Err(Errors::IndexOverflow);
        }
        if count < 8 && value >> count > 0 {
            return Err(Errors::DataOverflow);
        }
        let value = (value as u64) << from;
        let mask = ((1_u64 << count) - 1) << from;
        self.bits = (self.bits & !mask) | (value & mask);
        Ok(())
    }

    /**
    Value of bits from `from` to `to` (inclusive) in u32 range.
     */
    pub fn set_bits_u32(&mut self, from: u8, to: u8, value: u32) -> Result<(), Errors> {
        if from > to {
            return Err(Errors::FromAfterTo);
        }
        if from >= 64 || to >= 64 {
            return Err(Errors::OutOfRange);
        }
        let count = to - from + 1;
        if count > 32 {
            return Err(Errors::IndexOverflow);
        }
        if count < 8 && value >> count > 0 {
            return Err(Errors::DataOverflow);
        }
        let value = (value as u64) << from;
        let mask = ((1_u64 << count) - 1) << from;
        self.bits = (self.bits & !mask) | (value & mask);
        Ok(())
    }

}


#[cfg(test)]
mod tests {
    use crate::errors::Errors;
    use crate::utils::{BitsU64, BitsU8};

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
    fn test_bits() {
        let d = BitsU8::new(15);
        assert_eq!(d.bits(0, 2).unwrap(), 7);
        assert_eq!(d.bits(7, 7).unwrap(), 0);
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

    fn bits_u8(item: &BitsU64, from: u8, to: u8) -> Result<u8, Errors> {
    if from > to {
        return Err(Errors::FromAfterTo);
    }
    if from >= 64 || to >= 64 {
        return Err(Errors::OutOfRange);
    }
    if to - from > 8 {
        return Err(Errors::DataOverflow);
    }
    let res = (item.bits >> from) as u8;
    let mask = ((1_u16 << (to - from + 1)) - 1)  as u8;
    Ok(res & mask)
}

    #[test]
    fn test_bits_u64() {
        let d = BitsU64::new(15);
        assert_eq!(bits_u8(&d, 0, 0).unwrap(), 1);
        assert_eq!(bits_u8(&d, 1, 1).unwrap(), 1);
        assert_eq!(bits_u8(&d, 2, 2).unwrap(), 1);
        assert_eq!(bits_u8(&d, 3, 3).unwrap(), 1);
        assert_eq!(bits_u8(&d, 4, 4).unwrap(), 0);
        assert_eq!(bits_u8(&d, 5, 5).unwrap(), 0);
        assert_eq!(bits_u8(&d, 6, 6).unwrap(), 0);
        assert_eq!(bits_u8(&d, 7, 7).unwrap(), 0);
        assert_eq!(bits_u8(&d, 18, 18).unwrap(), 0);


        assert_eq!(bits_u8(&d, 0, 1).unwrap(), 3);
        assert_eq!(bits_u8(&d, 1, 2).unwrap(), 3);
        assert_eq!(bits_u8(&d, 2, 3).unwrap(), 3);
        assert_eq!(bits_u8(&d, 3, 4).unwrap(), 1);
        assert_eq!(bits_u8(&d, 4, 5).unwrap(), 0);
        assert_eq!(bits_u8(&d, 5, 6).unwrap(), 0);
        assert_eq!(bits_u8(&d, 6, 7).unwrap(), 0);
        assert_eq!(bits_u8(&d, 7, 8).unwrap(), 0);
        assert_eq!(bits_u8(&d, 21, 22).unwrap(), 0);


        assert_eq!(bits_u8(&d, 0, 2).unwrap(), 7);
        assert_eq!(bits_u8(&d, 1, 3).unwrap(), 7);
        assert_eq!(bits_u8(&d, 2, 4).unwrap(), 3);
        assert_eq!(bits_u8(&d, 3, 5).unwrap(), 1);
        assert_eq!(bits_u8(&d, 4, 6).unwrap(), 0);
        assert_eq!(bits_u8(&d, 5, 7).unwrap(), 0);
        assert_eq!(bits_u8(&d, 6, 8).unwrap(), 0);
        assert_eq!(bits_u8(&d, 7, 9).unwrap(), 0);
        assert_eq!(bits_u8(&d, 33, 35).unwrap(), 0);


        assert_eq!(bits_u8(&d, 0, 3).unwrap(), 15);
        assert_eq!(bits_u8(&d, 1, 4).unwrap(), 7);
        assert_eq!(bits_u8(&d, 2, 5).unwrap(), 3);
        assert_eq!(bits_u8(&d, 3, 6).unwrap(), 1);
        assert_eq!(bits_u8(&d, 4, 7).unwrap(), 0);
        assert_eq!(bits_u8(&d, 5, 8).unwrap(), 0);
        assert_eq!(bits_u8(&d, 6, 9).unwrap(), 0);
        assert_eq!(bits_u8(&d, 7, 10).unwrap(), 0);
        assert_eq!(bits_u8(&d, 36, 39).unwrap(), 0);


        assert_eq!(bits_u8(&d, 0, 4).unwrap(), 15);
        assert_eq!(bits_u8(&d, 1, 5).unwrap(), 7);
        assert_eq!(bits_u8(&d, 2, 6).unwrap(), 3);
        assert_eq!(bits_u8(&d, 3, 7).unwrap(), 1);
        assert_eq!(bits_u8(&d, 4, 8).unwrap(), 0);
        assert_eq!(bits_u8(&d, 5, 9).unwrap(), 0);
        assert_eq!(bits_u8(&d, 6, 10).unwrap(), 0);
        assert_eq!(bits_u8(&d, 7, 11).unwrap(), 0);
        assert_eq!(bits_u8(&d, 41, 45).unwrap(), 0);
    }

    #[test]
    fn test_set_byte() {
        let mut value = BitsU64::new(0b0000_0000);

        value.set_byte(0, 0, 0b0000_0001).unwrap();
        assert_eq!(value.bits, 1);
        value.set_byte(1, 1, 0b0000_0001).unwrap();
        assert_eq!(value.bits, 3);
        value.set_byte(2, 2, 0b0000_0001).unwrap();
        assert_eq!(value.bits, 7);
        value.set_byte(3, 3, 0b0000_0001).unwrap();
        assert_eq!(value.bits, 15);
        value.set_byte(4, 4, 0b0000_0001).unwrap();
        assert_eq!(value.bits, 31);

        let mut value = BitsU64::new(0b0000_0000);

        value.set_byte(0, 1, 0b0000_0011).unwrap();
        assert_eq!(value.bits, 0b0000_0011);
        value.set_byte(2, 3, 0b0000_0011).unwrap();
        assert_eq!(value.bits, 0b0000_1111);
        value.set_byte(4, 5, 0b0000_0011).unwrap();
        assert_eq!(value.bits, 0b0011_1111);
        value.set_byte(6, 7, 0b0000_0011).unwrap();
        assert_eq!(value.bits, 0b1111_1111);
        value.set_byte(8, 9, 0b0000_0011).unwrap();
        assert_eq!(value.bits, 0b0011_1111_1111);
        value.set_byte(9, 10, 0b0000_0011).unwrap();
        assert_eq!(value.bits, 0b0111_1111_1111);

        let mut value = BitsU64::new(0b0000_0000);

        value.set_byte(0, 2, 0b0000_0111).unwrap();
        assert_eq!(value.bits, 0b0000_0111);
        value.set_byte(3, 5, 0b0000_0111).unwrap();
        assert_eq!(value.bits, 0b0011_1111);
        value.set_byte(6, 8, 0b0000_0111).unwrap();
        assert_eq!(value.bits, 0b0001_1111_1111);
        value.set_byte(9, 11, 0b0000_0111).unwrap();
        assert_eq!(value.bits, 0b1111_1111_1111);
        value.set_byte(10, 12, 0b0000_0111).unwrap();
        assert_eq!(value.bits, 0b0001_1111_1111_1111);

        let mut value = BitsU64::new(0b0000_0000);

        value.set_byte(0, 7, 0b1111_1111).unwrap();
        assert_eq!(value.bits, 0b1111_1111);
        value.set_byte(8, 15, 0b1111_1111).unwrap();
        assert_eq!(value.bits, 0b1111_1111_1111_1111);
        value.set_byte(16, 23, 0b1111_1111).unwrap();
        assert_eq!(value.bits, 0b1111_1111_1111_1111_1111_1111);
        value.set_byte(24, 31, 0b1111_1111).unwrap();
        assert_eq!(value.bits, 0b1111_1111_1111_1111_1111_1111_1111_1111);
        value.set_byte(32, 39, 0b1111_1111).unwrap();
        assert_eq!(value.bits, 0b1111_1111_1111_1111_1111_1111_1111_1111_1111_1111);
        value.set_byte(40, 47, 0b1111_1111).unwrap();
        assert_eq!(value.bits, 0b1111_1111_1111_1111_1111_1111_1111_1111_1111_1111_1111_1111);
        value.set_byte(48, 55, 0b1111_1111).unwrap();
        assert_eq!(value.bits, 0b1111_1111_1111_1111_1111_1111_1111_1111_1111_1111_1111_1111_1111_1111);
        value.set_byte(56, 63, 0b1111_1111).unwrap();
        assert_eq!(value.bits, 0b1111_1111_1111_1111_1111_1111_1111_1111_1111_1111_1111_1111_1111_1111_1111_1111);

        value.set_byte(40, 45, 0b0000_0000).unwrap();
        assert_eq!(value.bits, 0b1111_1111_1111_1111_1100_0000_1111_1111_1111_1111_1111_1111_1111_1111_1111_1111);

        value.set_byte(55, 61, 0b0000_0000).unwrap();
        assert_eq!(value.bits, 0b1100_0000_0111_1111_1100_0000_1111_1111_1111_1111_1111_1111_1111_1111_1111_1111);

        value.set_byte(1, 5, 0b0000_1010).unwrap();
        assert_eq!(value.bits, 0b1100_0000_0111_1111_1100_0000_1111_1111_1111_1111_1111_1111_1111_1111_1101_0101);


        assert_eq!(Err(Errors::DataOverflow),  value.set_byte(1, 2, 0b0000_1111));
        assert_eq!(Err(Errors::IndexOverflow),  value.set_byte(1, 17, 0b0000_1111));
    }


}