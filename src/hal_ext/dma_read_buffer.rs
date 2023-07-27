#![allow(unsafe_code)]
#![deny(warnings)]

//use core::ops::Deref;
use embedded_dma::{ReadBuffer};
use crate::errors::Errors;

pub struct Buffer<const BUFFER_SIZE: usize> {
    buffer: &'static mut [u8; BUFFER_SIZE],
    size: usize,
}

impl <const BUFFER_SIZE: usize> Buffer<BUFFER_SIZE> {

    pub fn new(buffer: &'static mut [u8; BUFFER_SIZE]) -> Self {
        Self { buffer, size: 0 }
    }

    #[inline(always)]
    pub fn add_str(&mut self, string: &str) -> Result<(), Errors> {
        self.add(string.as_bytes())
    }

    pub fn add(&mut self, data: &[u8]) -> Result<(), Errors> {
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
    pub fn add_u8(&mut self, byte: u8) -> Result<(), Errors> {
        if 1 > BUFFER_SIZE - self.size {
            return Err(Errors::DmaBufferOverflow);
        }
        self.buffer[self.size] = byte;
        self.size += 1;
        Ok(())
    }

    #[inline]
    pub fn add_u16(&mut self, value: u16) -> Result<(), Errors> {
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
    pub fn add_u32(&mut self, value: u32) -> Result<(), Errors> {
        if 4 > BUFFER_SIZE - self.size {
            return Err(Errors::DmaBufferOverflow);
        }
        self.buffer[self.size] = ((value >> 24) & 0xff) as u8;
        self.size += 1;
        self.buffer[self.size] = ((value >> 16) & 0xff) as u8;
        self.size += 1;
        self.buffer[self.size] = ((value >> 8) & 0xff) as u8;
        self.size += 1;
        self.buffer[self.size] = (value & 0xff) as u8;
        self.size += 1;
        Ok(())
    }

    #[inline]
    pub fn add_u64(&mut self, value: u64) -> Result<(), Errors> {
        if 8 > BUFFER_SIZE - self.size {
            return Err(Errors::DmaBufferOverflow);
        }
        self.buffer[self.size] = ((value >> 56) & 0xff) as u8;
        self.size += 1;
        self.buffer[self.size] = ((value >> 48) & 0xff) as u8;
        self.size += 1;
        self.buffer[self.size] = ((value >> 40) & 0xff) as u8;
        self.size += 1;
        self.buffer[self.size] = ((value >> 32) & 0xff) as u8;
        self.size += 1;
        self.buffer[self.size] = ((value >> 24) & 0xff) as u8;
        self.size += 1;
        self.buffer[self.size] = ((value >> 16) & 0xff) as u8;
        self.size += 1;
        self.buffer[self.size] = ((value >> 8) & 0xff) as u8;
        self.size += 1;
        self.buffer[self.size] = (value & 0xff) as u8;
        self.size += 1;
        Ok(())
    }

    pub fn clear(&mut self) {
        self.size = 0;
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

unsafe impl <const BUFFER_SIZE: usize> ReadBuffer for Buffer<BUFFER_SIZE> {
    type Word = u8;

    unsafe fn read_buffer(&self) -> (*const Self::Word, usize) {
        let ptr = self.buffer.as_ptr();
        (ptr, self.size)
    }
}
/*
impl <const BUFFER_SIZE: usize> Deref for Buffer<BUFFER_SIZE> {
    type Target = [u8; BUFFER_SIZE];

    fn deref(&self) -> &Self::Target {
        &self.buffer
    }
}
*/