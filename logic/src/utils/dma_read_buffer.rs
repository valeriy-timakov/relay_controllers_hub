#![allow(unsafe_code)]

use embedded_dma::{ReadBuffer, WriteBuffer};
use crate::errors::Errors;

pub struct Buffer<const BUFFER_SIZE: usize> {
    buffer: &'static mut [u8; BUFFER_SIZE],
    size: usize,
}

pub  trait BufferWriter {
    fn add_str(&mut self, string: &str) -> Result<(), Errors>;
    fn add(&mut self, data: &[u8]) -> Result<(), Errors>;
    fn add_u8(&mut self, byte: u8) -> Result<(), Errors>;
    fn add_u16(&mut self, value: u16) -> Result<(), Errors>;
    fn add_u32(&mut self, value: u32) -> Result<(), Errors>;
    fn add_u64(&mut self, value: u64) -> Result<(), Errors>;
    fn clear(&mut self);
}

impl <const BUFFER_SIZE: usize> Buffer<BUFFER_SIZE> {

    pub fn new(buffer: &'static mut [u8; BUFFER_SIZE]) -> Self {
        Self { buffer, size: 0 }
    }

    pub fn bytes(&self) -> &[u8] {
        self.buffer[0..self.size].as_ref()
    }
    /*
    pub fn as_read_buffer(&self) -> (*const u8, usize) {
        unsafe { self.read_buffer() }
    }
    */
}

impl <const BUFFER_SIZE: usize> BufferWriter for Buffer<BUFFER_SIZE> {


    #[inline(always)]
    fn add_str(&mut self, string: &str) -> Result<(), Errors> {
        self.add(string.as_bytes())
    }

    fn add(&mut self, data: &[u8]) -> Result<(), Errors> {
        if data.len() > BUFFER_SIZE - self.size {
            return Err(Errors::DmaBufferOverflow);
        }
        data.iter().for_each(|byte| {
            self.buffer[self.size] = *byte;
            self.size += 1;
        });
        Ok(())
    }

    #[inline]
    fn add_u8(&mut self, byte: u8) -> Result<(), Errors> {
        if 1 > BUFFER_SIZE - self.size {
            return Err(Errors::DmaBufferOverflow);
        }
        self.buffer[self.size] = byte;
        self.size += 1;
        Ok(())
    }

    #[inline]
    fn add_u16(&mut self, value: u16) -> Result<(), Errors> {
        if 2 > BUFFER_SIZE - self.size {
            return Err(Errors::DmaBufferOverflow);
        }
        self.buffer[self.size] = ((value >> 8) & 0xff) as u8;
        self.size += 1;
        self.buffer[self.size] = (value & 0xff) as u8;
        self.size += 1;
        Ok(())
    }

    #[inline]
    fn add_u32(&mut self, value: u32) -> Result<(), Errors> {
        if 4 > BUFFER_SIZE - self.size {
            return Err(Errors::DmaBufferOverflow);
        }
        for i in 0..4 {
            self.buffer[self.size] = ((value >> (24 - i * 8)) & 0xff) as u8;
            self.size += 1;
        }
        Ok(())
    }

    #[inline]
    fn add_u64(&mut self, value: u64) -> Result<(), Errors> {
        if 8 > BUFFER_SIZE - self.size {
            return Err(Errors::DmaBufferOverflow);
        }
        for i in 0..8 {
            self.buffer[self.size] = ((value >> (56 - i * 8)) & 0xff) as u8;
            self.size += 1;
        }
        Ok(())
    }

    fn clear(&mut self) {
        self.size = 0;
    }
}

unsafe impl <const BUFFER_SIZE: usize> ReadBuffer for Buffer<BUFFER_SIZE> {
    type Word = u8;

    unsafe fn read_buffer(&self) -> (*const Self::Word, usize) {
        let ptr = self.buffer.as_ptr();
        (ptr, self.size)
    }
}

unsafe impl <const BUFFER_SIZE: usize> WriteBuffer for Buffer<BUFFER_SIZE> {
    type Word = u8;

    unsafe fn write_buffer(&mut self) -> (*mut Self::Word, usize) {
        let ptr = self.buffer.as_mut_ptr();
        (ptr, self.size)
    }
}



#[cfg(test)]
mod tests {
    use embedded_dma::ReadBuffer;
    use crate::errors::Errors;
    use crate::utils::dma_read_buffer::Buffer;
    use crate::utils::dma_read_buffer::BufferWriter;

    #[test]
    fn test_ptr() {
        static mut BUFFER: [u8; 1] = [0; 1];
        let buffer = unsafe { Buffer::new(&mut BUFFER) };
        unsafe { assert_eq!(buffer.read_buffer().0, buffer.bytes().as_ptr()); }
    }

    #[test]
    fn test_str() {
        static mut BUFFER: [u8; 10] = [0; 10];
        let mut buffer = unsafe { Buffer::new(&mut BUFFER) };
        let str = "12345678";
        buffer.add_str(str).unwrap();
        for i in 0..str.len() {
            assert_eq!(buffer.bytes()[i], (i + 49) as u8);
        }
    }

    #[test]
    fn test_arr() {
        static mut BUFFER: [u8; 10] = [0; 10];
        let mut buffer = unsafe { Buffer::new(&mut BUFFER) };
        let arr = [1u8, 2u8, 3u8, 4u8, 5u8, 6u8, 10u8, 11u8, 85u8, 52u8];
        buffer.add(&arr).unwrap();
        for i in 0..arr.len() {
            assert_eq!(buffer.bytes()[i], arr[i]);
        }
    }

    #[test]
    fn test_u64() {
        static mut BUFFER: [u8; 32] = [0; 32];
        let mut buffer = unsafe { Buffer::new(&mut BUFFER) };
        let arr = [1u64, 11u64, 85u64, 52u64];
        for i in 0..arr.len() {
            buffer.add_u64(arr[i]).unwrap();
        }
        for i in 0..arr.len() {
            let mut value = 0u64;
            for j in 0..8 {
                let pos = if is_little_endian() { 7 - j } else { j };
                value |= (buffer.bytes()[i * 8 + j] as u64) << (pos * 8);
            }
            assert_eq!(value, arr[i]);
        }
    }

    #[test]
    fn test_u32() {
        static mut BUFFER: [u8; 16] = [0; 16];
        let mut buffer = unsafe { Buffer::new(&mut BUFFER) };
        let arr = [1u32, 11u32, 85u32, 52u32];
        for i in 0..arr.len() {
            buffer.add_u32(arr[i]).unwrap();
        }
        for i in 0..arr.len() {
            let mut value = 0u32;
            for j in 0..4 {
                let pos = if is_little_endian() { 3 - j } else { j };
                value |= (buffer.bytes()[i * 4 + j] as u32) << (pos * 4);
            }
            assert_eq!(value, arr[i]);
        }
    }

    #[test]
    fn test_u16() {
        static mut BUFFER: [u8; 8] = [0; 8];
        let mut buffer = unsafe { Buffer::new(&mut BUFFER) };
        let arr = [1u16, 11u16, 85u16, 52u16];
        for i in 0..arr.len() {
            buffer.add_u16(arr[i]).unwrap();
        }
        for i in 0..arr.len() {
            let j = i * 2;
            let b0 = buffer.bytes()[j] as u16;
            let b1 = buffer.bytes()[j + 1] as u16;
            let value =  if is_little_endian() { (b0 << 8) | b1 } else { b0 | (b1 << 8) };
            assert_eq!(value, arr[i]);
        }
    }

    #[test]
    fn test_u8() {
        static mut BUFFER: [u8; 4] = [0; 4];
        let mut buffer = unsafe { Buffer::new(&mut BUFFER) };
        let arr = [1u8, 11u8, 85u8, 52u8];
        for i in 0..arr.len() {
            buffer.add_u8(arr[i]).unwrap();
        }
        for i in 0..arr.len() {
            assert_eq!(buffer.bytes()[i], arr[i]);
        }
    }

    fn is_little_endian() -> bool {
        let num: u16 = 0x0102;
        let ptr = &num as *const u16 as *const u8;
        unsafe { *ptr == 0x02 }
    }

    #[test]
    fn test_str_overflow() {
        static mut BUFFER: [u8; 6] = [0; 6];
        let mut buffer = unsafe { Buffer::new(&mut BUFFER) };
        let res = buffer.add_str("12345678");
        assert!(matches!(res, Result::Err(Errors::DmaBufferOverflow)));
    }

    #[test]
    fn test_arr_overflow() {
        static mut BUFFER: [u8; 5] = [0; 5];
        let mut buffer = unsafe { Buffer::new(&mut BUFFER) };
        let arr = [1u8, 2u8, 3u8, 4u8, 5u8, 6u8];
        let res = buffer.add(&arr);
        assert!(matches!(res, Result::Err(Errors::DmaBufferOverflow)));
    }

    #[test]
    fn test_u64_overflow() {
        static mut BUFFER: [u8; 6] = [0; 6];
        let mut buffer = unsafe { Buffer::new(&mut BUFFER) };
        let res = buffer.add_u64(12u64);
        assert!(matches!(res, Result::Err(Errors::DmaBufferOverflow)));
    }

    #[test]
    fn test_u32_overflow() {
        static mut BUFFER: [u8; 3] = [0; 3];
        let mut buffer = unsafe { Buffer::new(&mut BUFFER) };
        let res = buffer.add_u32(12u32);
        assert!(matches!(res, Result::Err(Errors::DmaBufferOverflow)));
    }

    #[test]
    fn test_u16_overflow() {
        static mut BUFFER: [u8; 1] = [0; 1];
        let mut buffer = unsafe { Buffer::new(&mut BUFFER) };
        let res = buffer.add_u16(12u16);
        assert!(matches!(res, Result::Err(Errors::DmaBufferOverflow)));
    }

    #[test]
    fn test_u8_overflow() {
        static mut BUFFER: [u8; 1] = [0; 1];
        let mut buffer = unsafe { Buffer::new(&mut BUFFER) };
        buffer.add_u8(12u8).unwrap();
        let res = buffer.add_u8(12u8);
        assert!(matches!(res, Result::Err(Errors::DmaBufferOverflow)));
    }

}