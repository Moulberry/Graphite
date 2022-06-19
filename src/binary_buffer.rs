use thiserror::Error;

use bytes::Buf;

use crate::varint;

#[derive(Error, Debug)]
pub enum BinaryReadError {
    #[error("buffer does not contain enough bytes to perform read")]
    NotEnoughRemainingBytes,
    #[error("string byte count ({0}) exceeds maximum ({1})")]
    StringBytesExceedMaxSize(usize, usize),
    #[error("string character count ({0}) exceeds maximum ({1})")]
    StringCharsExceedMaxSize(usize, usize),
    #[error("didn't fully consume packet buffer, {0} byte(s) remained")]
    DidntFullyConsume(isize)
}

pub struct BinaryBuf<'a> {
    slice: &'a [u8],
    reader_index: usize
}

impl <'a> BinaryBuf<'a> {
    const STRING_MAX_SIZE: usize = 32767;

    pub fn new(slice: &[u8]) -> BinaryBuf {
        return BinaryBuf {slice, reader_index: 0};
    }

    pub fn check_finished(&self) -> anyhow::Result<()> {
        if self.reader_index == self.slice.len() {
            Ok(())
        } else {
            Err(BinaryReadError::DidntFullyConsume(self.slice.len() as isize - self.reader_index as isize).into())
        }
    }

    pub fn get_all_bytes(&mut self) -> anyhow::Result<&[u8]> {
        if self.reader_index >= self.slice.len() {
            return Err(BinaryReadError::NotEnoughRemainingBytes.into());
        }

        let ret = &self.slice[self.reader_index..];
        self.reader_index = self.slice.len();
        Ok(ret)
    }

    pub fn get_varint(&mut self) -> anyhow::Result<i32> {
        if self.reader_index >= self.slice.len() {
            return Err(BinaryReadError::NotEnoughRemainingBytes.into());
        }

        let (num, size) = varint::decode_varint(&self.slice[self.reader_index..])?;
        self.reader_index += size;
        Ok(num)
    }

    pub fn get_string(&mut self) -> anyhow::Result<&str> {
        self.get_string_with_max_size(BinaryBuf::STRING_MAX_SIZE)
    }

    pub fn get_string_with_max_size(&mut self, max_size: usize) -> anyhow::Result<&str> {
        if self.reader_index >= self.slice.len() {
            return Err(BinaryReadError::NotEnoughRemainingBytes.into());
        }

        // Get string length
        let (num, size) = varint::decode_varint3(&self.slice[self.reader_index..])?;
        self.reader_index += size;

        if num as usize > max_size * 4 {
            return Err(BinaryReadError::StringBytesExceedMaxSize(num as usize, max_size * 4).into());
        }
        if self.reader_index + num as usize > self.slice.len() {
            return Err(BinaryReadError::NotEnoughRemainingBytes.into());
        }

        // Read string, validate utf-8
        let start = self.reader_index;
        self.reader_index += num as usize;
        let string = std::str::from_utf8(&self.slice[start..self.reader_index])?;

        // Check character count, if necessary
        if num as usize > max_size {
            let character_count = string.chars().count();
            if character_count > max_size {
                return Err(BinaryReadError::StringCharsExceedMaxSize(character_count, max_size).into());
            }
        }

        Ok(string)
    }
}

impl <'a> Buf for BinaryBuf<'a> {
    fn remaining(&self) -> usize {
        self.slice.len() - self.reader_index
    }

    fn chunk(&self) -> &[u8] {
        &self.slice[self.reader_index..]
    }

    fn advance(&mut self, cnt: usize) {
        self.reader_index += cnt;
    }
}